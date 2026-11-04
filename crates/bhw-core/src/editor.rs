//! The world editor (Phase 2): editing sessions on Worlds-library worlds through [`bhw_editor`].
//!
//! Opening a world loads a working copy into memory; nothing is written until the user saves, either back into the
//! same library file ("Save to Worlds") or as a new world ("Save as new world").

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use bhw_content::players::Stats;
use bhw_content::tile::Face;
use bhw_content::WorldContent;
use bhw_editor::atlas::{Atlas, Catalog};
use bhw_editor::{Brush, Label, Match, OpKind, Outcome, Rect, Schematic, Session};
use bhw_lmdb::Layout;
use bhw_save::save::{FileMap, WORLD_DB};
use bhw_save::world::{from_file_map, to_file_map};
use bhw_save::AnySave;
use serde::{Deserialize, Serialize};

use crate::error::{AppError, ErrorCode, Result};
use crate::types::World;
use crate::util::{lock, now_ms};
use crate::Core;

impl From<bhw_editor::Error> for AppError {
    fn from(e: bhw_editor::Error) -> AppError {
        match e {
            bhw_editor::Error::Content(bhw_content::Error::Format(m)) => AppError::with(ErrorCode::WorldDamaged, m),
            bhw_editor::Error::Content(bhw_content::Error::Plist(m)) => AppError::with(ErrorCode::WorldDamaged, m),
            e => AppError::with(ErrorCode::NotAllowed, e),
        }
    }
}

impl From<bhw_content::Error> for AppError {
    fn from(e: bhw_content::Error) -> AppError {
        bhw_editor::Error::from(e).into()
    }
}

/// One open editing session.
pub struct Open {
    /// Library file name of the world.
    pub lib_id: String,
    session: Session,
    files: FileMap,
    layout: Layout,
    clipboard: Option<Schematic>,
}

/// Open sessions by session id.
pub type Sessions = Mutex<HashMap<String, Arc<Mutex<Open>>>>;

/// A blockhead as the editor shows it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EditorBlockhead {
    pub player: String,
    pub id: u64,
    pub name: String,
    pub x: i64,
    pub y: i64,
    pub stats: Option<[f32; 4]>,
    /// Inventory slots: per slot `[item, count]` (`[0, 0]` = empty).
    pub inventory: Option<Vec<[u32; 2]>>,
}

/// What the editor needs when a world opens.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EditorWorld {
    pub session: String,
    pub lib_id: String,
    pub world_id: String,
    pub name: String,
    pub game_version: Option<String>,
    /// Width and height in tiles.
    pub width: u32,
    pub height: u32,
    pub spawn: Option<[i64; 2]>,
    /// Stored macro blocks `[x, y]`.
    pub blocks: Vec<[u32; 2]>,
    pub damaged_blocks: usize,
    pub expert: bool,
    pub created: Option<i64>,
    pub time_of_day: Option<f64>,
    /// Blockheads Server world (64-bit, per-player blockheads).
    pub server: bool,
    pub blockheads: Vec<EditorBlockhead>,
}

/// One object near a tile.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EditorObject {
    /// `dw` record key and index (to edit or find it again).
    pub key: String,
    pub index: usize,
    pub kind: u32,
    pub kind_name: Option<String>,
    pub id: Option<u64>,
    pub x: i64,
    pub y: i64,
    /// Short text (sign text, owner name) when the object has one.
    pub text: Option<String>,
    /// Container contents per slot `[item, count]` (chests, safes…).
    pub slots: Option<Vec<[u32; 2]>>,
}

fn slots(inv: &bhw_content::Inventory) -> Vec<[u32; 2]> {
    inv.slots.iter().map(|st| [st.first().map_or(0, |i| i.kind() as u32), st.len() as u32]).collect()
}

/// The editor's view of a dynamic object (`None` for objects without a position).
pub fn editor_object(ob: &bhw_content::DynObject) -> Option<EditorObject> {
    let (x, y) = ob.pos()?;
    let text = ["text", "ownerName", "name"]
        .iter()
        .find_map(|k| ob.dict.get(k).and_then(plist::Value::as_string))
        .map(str::to_string);
    Some(EditorObject {
        key: ob.key.clone(),
        index: ob.index,
        kind: ob.kind,
        kind_name: bhw_content::names::object_type_name(ob.kind).map(str::to_string),
        id: ob.unique_id(),
        x,
        y,
        text,
        slots: bhw_editor::session::container_inventory(&ob.dict).map(|i| slots(&i)),
    })
}

/// A tile as the inspector shows it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EditorTile {
    pub x: u32,
    pub y: u32,
    /// False outside the generated area (all other fields are 0).
    pub stored: bool,
    pub fg: u8,
    pub bg: u8,
    pub contents: u8,
    pub light: u8,
    pub sun_light: u8,
    pub paint: [u16; 5],
    pub owner: u64,
    pub protected: bool,
    pub objects: Vec<EditorObject>,
}

/// The state shown in the editor's header and history panel.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EditorState {
    /// World name and time of day as they are now (settings edits, undo).
    pub name: Option<String>,
    pub time_of_day: Option<f64>,
    pub history: Vec<Label>,
    pub cursor: usize,
    pub unsaved: usize,
    pub clipboard: Option<[u32; 2]>,
}

/// An edit request from the UI.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum EditRequest {
    Paint {
        kind: OpKind,
        points: Vec<[u32; 2]>,
        brush: Brush,
        #[serde(default)]
        only_solid: bool,
    },
    Fill {
        rect: Rect,
        brush: Brush,
    },
    Clear {
        rect: Rect,
    },
    Replace {
        from: Match,
        to: Brush,
        area: Option<Rect>,
    },
    Copy {
        rect: Rect,
    },
    Cut {
        rect: Rect,
    },
    Paste {
        x: u32,
        y: u32,
        #[serde(default)]
        skip_air: bool,
        #[serde(default)]
        rotate: u8,
        #[serde(default)]
        flip: bool,
    },
    MoveBlockhead {
        player: String,
        id: u64,
        x: u32,
        y: u32,
    },
    SetStats {
        player: String,
        id: u64,
        stats: [f32; 4],
    },
    SetStack {
        player: String,
        id: u64,
        slot: usize,
        item: u16,
        count: usize,
    },
    RemoveBlockhead {
        player: String,
        id: u64,
    },
    SetContainerStack {
        key: String,
        index: usize,
        slot: usize,
        item: u16,
        count: usize,
    },
    SetName {
        name: String,
    },
    SetTimeOfDay {
        hour: f64,
    },
}

/// What an edit did, and the new state.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EditResult {
    pub outcome: Outcome,
    pub state: EditorState,
    /// Macro blocks whose tiles changed (to redraw).
    pub blocks: Vec<[u32; 2]>,
}

/// History moved: the new state and the macro blocks to redraw.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HistoryResult {
    pub state: EditorState,
    pub blocks: Vec<[u32; 2]>,
}

fn touched(o: &mut Open) -> Vec<[u32; 2]> {
    o.session.take_touched().into_iter().map(|(x, y)| [x, y]).collect()
}

fn blockheads(s: &Session) -> Vec<EditorBlockhead> {
    s.world
        .players
        .list()
        .into_iter()
        .map(|b| {
            let inventory = s.world.players.inventory(&b.player, b.unique_id).map(slots);
            EditorBlockhead {
                player: b.player,
                id: b.unique_id,
                name: b.name,
                x: b.pos.0,
                y: b.pos.1,
                stats: b.stats.map(|st| [st.health, st.happiness, st.fullness, st.energy]),
                inventory,
            }
        })
        .collect()
}

fn state(o: &Open) -> EditorState {
    let (history, cursor) = o.session.history();
    let rec = o.session.world.record.as_ref();
    EditorState {
        name: rec.and_then(|r| r.name()).map(str::to_string),
        time_of_day: rec.and_then(|r| r.time_of_day()),
        history,
        cursor,
        unsaved: o.session.unsaved(),
        clipboard: o.clipboard.as_ref().map(|c| [c.width, c.height]),
    }
}

fn not_open(id: &str) -> AppError {
    AppError::with(ErrorCode::NotFound, format!("editor session {id}"))
}

impl Core {
    fn editor(&self, id: &str) -> Result<Arc<Mutex<Open>>> {
        lock(&self.editors).get(id).cloned().ok_or_else(|| not_open(id))
    }

    /// Opens library world `lib_id` for editing.
    pub fn editor_open(&self, lib_id: &str) -> Result<EditorWorld> {
        let b = self.lib()?.open_world(lib_id)?;
        let files = to_file_map(&b.world)?;
        let save = AnySave::from_files(&b.world.id, files.clone())?;
        let (env, layout, server) = match save {
            AnySave::Client(c) => (c.world_db, c.layout, false),
            AnySave::Server(s) => (s.world_db, Layout::Bits64, true),
        };
        let content = WorldContent::load(env)?;
        if content.record.as_ref().is_some_and(|r| r.remote()) {
            return Err(AppError::with(
                ErrorCode::NotAllowed,
                "a multiplayer world cached by the game cannot be edited",
            ));
        }
        let mut session = Session::new(content);
        let positions: Vec<[u32; 2]> = session.world.blocks.positions().map(|p| [p.0, p.1]).collect();
        // decode every block once to find damaged ones
        let damaged = positions.iter().filter(|p| session.world.blocks.block((p[0], p[1])).is_err()).count();
        let rec = session.world.record.clone();
        let id = format!("e{}", now_ms());
        let world = EditorWorld {
            session: id.clone(),
            lib_id: lib_id.to_string(),
            world_id: b.world.id.clone(),
            name: rec
                .as_ref()
                .and_then(|r| r.name())
                .map(str::to_string)
                .or(b.manifest.world.name.clone())
                .unwrap_or_default(),
            game_version: b.manifest.world.game_version.clone(),
            width: session.width(),
            height: session.height(),
            spawn: rec.as_ref().and_then(|r| r.spawn()).map(|(x, y)| [x, y]),
            blocks: positions,
            damaged_blocks: damaged,
            expert: rec.as_ref().is_some_and(|r| r.expert()),
            created: rec.as_ref().and_then(|r| r.created()),
            time_of_day: rec.as_ref().and_then(|r| r.time_of_day()),
            server,
            blockheads: blockheads(&session),
        };
        let open = Open { lib_id: lib_id.to_string(), session, files, layout, clipboard: None };
        lock(&self.editors).insert(id, Arc::new(Mutex::new(open)));
        Ok(world)
    }

    /// Closes a session, dropping unsaved changes.
    pub fn editor_close(&self, id: &str) {
        lock(&self.editors).remove(id);
    }

    /// Tile data of a rectangle of macro blocks (see [`bhw_editor::view::region`]).
    pub fn editor_region(&self, id: &str, mx0: u32, my0: u32, mx1: u32, my1: u32) -> Result<Vec<u8>> {
        let o = self.editor(id)?;
        let mut o = lock(&o);
        Ok(bhw_editor::view::region(&mut o.session, mx0, my0, mx1, my1)?)
    }

    /// The whole world in plain colours, one pixel per `scale` tiles: `u32 width, u32 height` (little-endian),
    /// then RGBA pixels, top row first.
    pub fn editor_overview(&self, id: &str, scale: u32) -> Result<Vec<u8>> {
        let o = self.editor(id)?;
        let mut o = lock(&o);
        let img = bhw_editor::view::overview(&mut o.session, scale.clamp(1, 64));
        let mut out = Vec::with_capacity(8 + img.pixels.len());
        out.extend_from_slice(&img.width.to_le_bytes());
        out.extend_from_slice(&img.height.to_le_bytes());
        out.extend_from_slice(&img.pixels);
        Ok(out)
    }

    /// One tile and the objects on it.
    pub fn editor_tile(&self, id: &str, x: u32, y: u32) -> Result<EditorTile> {
        let o = self.editor(id)?;
        let mut o = lock(&o);
        let t = o.session.tile(x, y)?;
        let owner = t.map_or(0, |t| t.owner());
        let (all, _) = o.session.world.objects.all();
        let objects = all
            .iter()
            .filter(|ob| {
                (owner != 0 && ob.unique_id() == Some(owner))
                    || ob.pos().is_some_and(|(ox, oy)| (ox - x as i64).abs() <= 1 && (oy - y as i64).abs() <= 1)
            })
            .filter_map(editor_object)
            .collect();
        let t = t.unwrap_or_default();
        let stored = o.session.tile(x, y)?.is_some();
        Ok(EditorTile {
            x,
            y,
            stored,
            fg: t.fg(),
            bg: t.bg(),
            contents: t.contents(),
            light: t.light(),
            sun_light: t.sun_light(),
            paint: Face::ALL.map(|f| t.paint(f)),
            owner: t.owner(),
            protected: stored && bhw_editor::brush::is_protected(&t),
            objects,
        })
    }

    /// Names, plain colours and atlas cells of every id.
    pub fn editor_catalog(&self) -> Catalog {
        Catalog::new(&Atlas::builtin())
    }

    /// The user's own atlas image (`tiles` = `HDTex/TileMap.png`, `items` = `HDTex/Items.png`), `None` when the
    /// game files are not set up.
    pub fn editor_atlas(&self, which: &str) -> Result<Option<Vec<u8>>> {
        let rel = match which {
            "tiles" => "HDTex/TileMap.png",
            "items" => "HDTex/Items.png",
            _ => return Err(AppError::not_found(which)),
        };
        match bhw_assets::GameFiles::new(&self.cfg.cache_dir).file(rel) {
            Ok(Some(p)) => Ok(Some(std::fs::read(p)?)),
            _ => Ok(None),
        }
    }

    /// Current history and unsaved count.
    pub fn editor_state(&self, id: &str) -> Result<EditorState> {
        let o = self.editor(id)?;
        let o = lock(&o);
        Ok(state(&o))
    }

    /// The blockheads with their positions, needs and inventories.
    pub fn editor_blockheads(&self, id: &str) -> Result<Vec<EditorBlockhead>> {
        let o = self.editor(id)?;
        let o = lock(&o);
        Ok(blockheads(&o.session))
    }

    /// Applies one edit.
    pub fn editor_edit(&self, id: &str, req: EditRequest) -> Result<EditResult> {
        let o = self.editor(id)?;
        let mut o = lock(&o);
        let st = self.settings();
        o.session.options = bhw_editor::EditOptions { generate: st.editor_generate, objects: st.editor_objects };
        let none = Outcome::default();
        let outcome = match req {
            EditRequest::Paint { kind, points, brush, only_solid } => {
                let pts: Vec<(u32, u32)> = points.iter().map(|p| (p[0], p[1])).collect();
                o.session.paint(kind, &pts, brush, only_solid)?
            }
            EditRequest::Fill { rect, brush } => o.session.fill(rect, brush)?,
            EditRequest::Clear { rect } => o.session.clear(rect)?,
            EditRequest::Replace { from, to, area } => o.session.replace(from, to, area)?,
            EditRequest::Copy { rect } => {
                let c = o.session.copy(rect)?;
                o.clipboard = Some(c);
                none
            }
            EditRequest::Cut { rect } => {
                let (c, out) = o.session.cut(rect)?;
                o.clipboard = Some(c);
                out
            }
            EditRequest::Paste { x, y, skip_air, rotate, flip } => {
                let mut c =
                    o.clipboard.clone().ok_or_else(|| AppError::with(ErrorCode::NotAllowed, "nothing copied"))?;
                for _ in 0..rotate % 4 {
                    c = c.rotated();
                }
                if flip {
                    c = c.flipped();
                }
                o.session.paste(&c, x, y, skip_air)?
            }
            EditRequest::MoveBlockhead { player, id, x, y } => {
                o.session.move_blockhead(&player, id, x, y)?;
                none
            }
            EditRequest::SetStats { player, id, stats } => {
                let [health, happiness, fullness, energy] = stats;
                o.session.set_stats(&player, id, Stats { health, happiness, fullness, energy })?;
                none
            }
            EditRequest::SetStack { player, id, slot, item, count } => {
                o.session.set_stack(&player, id, slot, item, count)?;
                none
            }
            EditRequest::RemoveBlockhead { player, id } => {
                o.session.remove_blockhead(&player, id)?;
                none
            }
            EditRequest::SetContainerStack { key, index, slot, item, count } => {
                o.session.set_container_stack(&key, index, slot, item, count)?;
                none
            }
            EditRequest::SetName { name } => {
                o.session.set_name(&name)?;
                none
            }
            EditRequest::SetTimeOfDay { hour } => {
                o.session.set_time_of_day(hour)?;
                none
            }
        };
        Ok(EditResult { outcome, state: state(&o), blocks: touched(&mut o) })
    }

    /// Finds tiles: at most `limit` positions and the total count.
    pub fn editor_find(&self, id: &str, m: Match, area: Option<Rect>, limit: usize) -> Result<(Vec<[u32; 2]>, u64)> {
        let o = self.editor(id)?;
        let mut o = lock(&o);
        let (found, n) = o.session.find(m, area, limit.min(10_000))?;
        Ok((found.into_iter().map(|(x, y)| [x, y]).collect(), n))
    }

    /// Undoes or redoes until `to` history steps are applied.
    pub fn editor_history(&self, id: &str, to: usize) -> Result<HistoryResult> {
        let o = self.editor(id)?;
        let mut o = lock(&o);
        o.session.go_to(to)?;
        Ok(HistoryResult { state: state(&o), blocks: touched(&mut o) })
    }

    /// Tiles changed by the applied steps (for the compare view).
    pub fn editor_changes(&self, id: &str) -> Result<Vec<[u32; 2]>> {
        let o = self.editor(id)?;
        let o = lock(&o);
        Ok(o.session.changed_tiles().into_iter().map(|(x, y)| [x, y]).collect())
    }

    /// Saves the working copy: back into its library file, or (`as_new`) as a new world with a new id and name.
    pub fn editor_save(&self, id: &str, as_new: bool) -> Result<World> {
        let o = self.editor(id)?;
        let mut o = lock(&o);
        let env = o.session.world.to_env()?;
        let mut files = o.files.clone();
        files.insert(WORLD_DB.to_string(), bhw_lmdb::write_env(&env, o.layout)?);
        let lib = self.lib()?;
        let world_id = lib.open_world(&o.lib_id)?.world.id;
        let mut wf = from_file_map(&world_id, files)?;
        let target = if as_new {
            let dup = lib.duplicate(&o.lib_id)?;
            crate::worlds::set_identity(&mut wf, Some(&dup.world_id), Some(&dup.base.name))?;
            dup.base.id
        } else {
            o.lib_id.clone()
        };
        let w = lib.save_edit(&target, &wf)?;
        if !as_new {
            o.files = to_file_map(&wf)?;
            o.session.save()?;
        }
        Ok(w)
    }
}

/// What [`Core::editor_search`] looks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchKind {
    /// Containers (objects with item slots).
    Chests,
    /// Signs (objects with a text).
    Signs,
    /// Every object.
    Objects,
}

impl Core {
    /// Objects matching `kind` whose type name or text contains `text` (case-insensitive), at most 500.
    pub fn editor_search(&self, id: &str, kind: SearchKind, text: &str) -> Result<Vec<EditorObject>> {
        let o = self.editor(id)?;
        let mut o = lock(&o);
        let q = text.trim().to_lowercase();
        let (all, _) = o.session.world.objects.all();
        Ok(all
            .iter()
            .filter_map(editor_object)
            .filter(|e| match kind {
                SearchKind::Chests => e.slots.is_some(),
                SearchKind::Signs => e.text.is_some() && e.kind_name.as_deref() != Some("blockhead"),
                SearchKind::Objects => true,
            })
            .filter(|e| {
                q.is_empty()
                    || e.kind_name.as_deref().unwrap_or_default().replace('_', " ").contains(&q)
                    || e.text.as_deref().is_some_and(|t| t.to_lowercase().contains(&q))
            })
            .take(500)
            .collect())
    }
}

/// A schematic file in the Worlds folder's `schematics/`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SchematicInfo {
    /// File name (`*.bhwschem`).
    pub file: String,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub source_world: Option<String>,
    /// Unix seconds.
    pub created: u64,
}

/// File extension of schematics.
pub const SCHEMATIC_EXT: &str = "bhwschem";

fn schematic_info(file: &str, s: &Schematic) -> SchematicInfo {
    SchematicInfo {
        file: file.to_string(),
        name: s.meta.name.clone(),
        width: s.width,
        height: s.height,
        source_world: s.meta.source_world.clone(),
        created: s.meta.created,
    }
}

fn schematic_error(e: bhw_editor::Error) -> AppError {
    AppError::with(ErrorCode::WorldDamaged, e)
}

/// A file name for a schematic called `name` that is not taken in `dir`.
fn schematic_file(dir: &std::path::Path, name: &str) -> String {
    let base: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() || " -_()".contains(c) { c } else { '_' })
        .collect::<String>()
        .trim()
        .chars()
        .take(60)
        .collect();
    let base = if base.is_empty() { "Schematic".to_string() } else { base };
    let mut n = 1;
    loop {
        let f = if n == 1 { format!("{base}.{SCHEMATIC_EXT}") } else { format!("{base} ({n}).{SCHEMATIC_EXT}") };
        if !dir.join(&f).exists() {
            return f;
        }
        n += 1;
    }
}

impl Core {
    fn schematics_dir(&self) -> std::path::PathBuf {
        self.settings().worlds_dir().join("schematics")
    }

    fn schematic_path(&self, file: &str) -> Result<std::path::PathBuf> {
        let p = crate::util::child(&self.schematics_dir(), file)?;
        if p.extension().and_then(|e| e.to_str()) != Some(SCHEMATIC_EXT) {
            return Err(AppError::not_found(file));
        }
        Ok(p)
    }

    /// The saved schematics, newest first (unreadable files are left out).
    pub fn editor_schematics(&self) -> Result<Vec<SchematicInfo>> {
        let dir = self.schematics_dir();
        let Ok(rd) = std::fs::read_dir(&dir) else { return Ok(Vec::new()) };
        let mut out: Vec<SchematicInfo> = rd
            .filter_map(|e| {
                let e = e.ok()?;
                let file = e.file_name().to_string_lossy().into_owned();
                if !file.ends_with(&format!(".{SCHEMATIC_EXT}")) {
                    return None;
                }
                let s = Schematic::from_bytes(&std::fs::read(e.path()).ok()?).ok()?;
                Some(schematic_info(&file, &s))
            })
            .collect();
        out.sort_by(|a, b| b.created.cmp(&a.created).then(a.file.cmp(&b.file)));
        Ok(out)
    }

    fn write_schematic(&self, mut s: Schematic, name: &str) -> Result<SchematicInfo> {
        let dir = self.schematics_dir();
        std::fs::create_dir_all(&dir)?;
        s.meta.name = name.trim().to_string();
        s.meta.created = now_ms() / 1000;
        let file = schematic_file(&dir, name);
        std::fs::write(dir.join(&file), s.to_bytes())?;
        Ok(schematic_info(&file, &s))
    }

    /// Saves a rectangle of the world (or the clipboard, when `rect` is `None`) as a schematic file.
    pub fn editor_save_schematic(&self, id: &str, rect: Option<Rect>, name: &str) -> Result<SchematicInfo> {
        let o = self.editor(id)?;
        let mut o = lock(&o);
        let s = match rect {
            Some(r) => o.session.copy(r)?,
            None => o.clipboard.clone().ok_or_else(|| AppError::with(ErrorCode::NotAllowed, "nothing copied"))?,
        };
        drop(o);
        self.write_schematic(s, name)
    }

    /// Puts a saved schematic on the session's clipboard (ready to paste).
    pub fn editor_use_schematic(&self, id: &str, file: &str) -> Result<EditorState> {
        let s = Schematic::from_bytes(&std::fs::read(self.schematic_path(file)?)?).map_err(schematic_error)?;
        let o = self.editor(id)?;
        let mut o = lock(&o);
        o.clipboard = Some(s);
        Ok(state(&o))
    }

    /// Copies a `.bhwschem` file from elsewhere into the schematics folder.
    pub fn editor_import_schematic(&self, path: &str) -> Result<SchematicInfo> {
        let s = Schematic::from_bytes(&std::fs::read(path)?).map_err(schematic_error)?;
        let name = s.meta.name.clone();
        self.write_schematic(s, &name)
    }

    /// Copies a saved schematic to `dest` (a path chosen by the user).
    pub fn editor_export_schematic(&self, file: &str, dest: &str) -> Result<()> {
        std::fs::copy(self.schematic_path(file)?, dest)?;
        Ok(())
    }

    /// Deletes a saved schematic.
    pub fn editor_delete_schematic(&self, file: &str) -> Result<()> {
        std::fs::remove_file(self.schematic_path(file)?)?;
        Ok(())
    }
}
