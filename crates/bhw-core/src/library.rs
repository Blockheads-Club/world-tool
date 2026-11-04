//! The Worlds library: `.bhworld` files in `<Worlds folder>/worlds` (see [`bhw_world::Library`]) plus an index
//! (`.bhwt-index.json`) with what the UI lists but a manifest does not hold (mode, dates, origin), rebuilt for files
//! that changed or were added by hand.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use bhw_save::world::SaveMeta;
use bhw_world::bhworld::{open_bhworld, BhWorld, ExportInfo, Limits, PREVIEW};
use bhw_world::{SourceKind, WorldFiles};

use crate::error::Result;
use crate::types::{DeviceWorld, GameMode, Origin, World};
use crate::util::{child, file_name, mtime_ms, now_ms};
use crate::worlds::{data_url, inspect, WorldInfo};

const INDEX: &str = ".bhwt-index.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct IndexEntry {
    len: u64,
    modified: u64,
    world: World,
}

type Index = BTreeMap<String, IndexEntry>;

/// Where a world being added comes from.
#[derive(Debug, Clone)]
pub struct AddMeta {
    pub origin: Origin,
    pub source: String,
    pub source_detail: Option<String>,
    pub game_version: Option<String>,
    pub kind: SourceKind,
    pub device_name: Option<String>,
}

/// The library at a Worlds folder.
#[derive(Debug, Clone)]
pub struct Lib {
    pub inner: bhw_world::Library,
}

/// Origin shown for a manifest source kind.
pub fn origin_of(kind: SourceKind) -> Origin {
    match kind {
        SourceKind::AndroidBackup | SourceKind::AndroidRoot => Origin::Phone,
        SourceKind::BlockheadsPc => Origin::Pc,
        SourceKind::IosBackup => Origin::Ios,
        _ => Origin::File,
    }
}

/// The stored map preview of a `.bhworld` (cheap: only that zip entry is read).
pub fn read_preview(path: &Path) -> Option<Vec<u8>> {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(path).ok()?).ok()?;
    let entry = zip.by_name(PREVIEW).ok()?;
    let mut v = Vec::new();
    entry.take(Limits::default().max_preview).read_to_end(&mut v).ok()?;
    Some(v)
}

fn stamp(path: &Path) -> (u64, u64) {
    (std::fs::metadata(path).map_or(0, |m| m.len()), mtime_ms(path))
}

impl Lib {
    /// Opens (creating) the library in `root`.
    pub fn open(root: &Path) -> Result<Lib> {
        Ok(Lib { inner: bhw_world::Library::open(root)? })
    }

    fn index_file(&self) -> PathBuf {
        self.inner.worlds_dir().join(INDEX)
    }

    fn load_index(&self) -> Index {
        std::fs::read(self.index_file()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }

    fn save_index(&self, idx: &Index) -> Result<()> {
        std::fs::write(self.index_file(), serde_json::to_vec(idx)?)?;
        Ok(())
    }

    /// The file of library world `id` (its file name).
    pub fn path(&self, id: &str) -> Result<PathBuf> {
        child(&self.inner.worlds_dir(), id)
    }

    /// Opens and validates library world `id`.
    pub fn open_world(&self, id: &str) -> Result<BhWorld> {
        Ok(open_bhworld(&self.path(id)?, &Limits::default())?)
    }

    /// Reads a file into an index entry; `carry` keeps origin and dates of the world it replaces.
    fn build(path: &Path, carry: Option<&World>) -> Result<World> {
        let b = open_bhworld(path, &Limits::default())?;
        let info = inspect(&b.world, None)?;
        let m = &b.manifest;
        let mut w = World {
            base: info.device_world(&file_name(path), m.world.game_version.as_deref().unwrap_or_default()),
            world_id: b.world.id.clone(),
            origin: origin_of(m.source.kind),
            source: m.source.device_name.clone().unwrap_or(file_name(path)),
            source_detail: None,
            added: mtime_ms(path),
            edited: None,
        };
        w.base.name = m.world.name.clone().unwrap_or(w.base.name);
        w.base.preview = None;
        if let Some(c) = carry {
            (w.origin, w.source, w.source_detail, w.added, w.edited) =
                (c.origin, c.source.clone(), c.source_detail.clone(), c.added, c.edited);
        }
        Ok(w)
    }

    /// A placeholder for a file that cannot be read (listed so the user can remove it).
    fn broken(path: &Path, len: u64) -> World {
        let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        World {
            base: DeviceWorld {
                id: file_name(path),
                name: stem,
                size: len,
                game_version: String::new(),
                last_played: None,
                created: 0,
                mode: GameMode::Normal,
                preview: None,
            },
            world_id: String::new(),
            origin: Origin::File,
            source: file_name(path),
            source_detail: None,
            added: mtime_ms(path),
            edited: None,
        }
    }

    fn entry(path: &Path, carry: Option<&World>) -> IndexEntry {
        let (len, modified) = stamp(path);
        let world = Lib::build(path, carry).unwrap_or_else(|_| Lib::broken(path, len));
        IndexEntry { len, modified, world }
    }

    /// Every world, newest first, with previews.
    pub fn list(&self) -> Result<Vec<World>> {
        let old = self.load_index();
        let mut idx = Index::new();
        for lw in self.inner.list_worlds()? {
            let name = file_name(&lw.path);
            let (len, modified) = stamp(&lw.path);
            let e = match old.get(&name) {
                Some(e) if e.len == len && e.modified == modified => e.clone(),
                _ => Lib::entry(&lw.path, None),
            };
            idx.insert(name, e);
        }
        self.save_index(&idx)?;
        let dir = self.inner.worlds_dir();
        let mut out: Vec<World> = idx
            .into_values()
            .map(|e| {
                let mut w = e.world;
                w.base.preview = read_preview(&dir.join(&w.base.id)).as_deref().map(data_url);
                w
            })
            .collect();
        // One closure (no `then_with`): a tie-break closure only runs when two worlds share a timestamp, which made
        // function coverage depend on file-system timing.
        out.sort_by(|a, b| (b.added, &a.base.id).cmp(&(a.added, &b.base.id)));
        Ok(out)
    }

    fn put_entry(&self, path: &Path, w: &World) -> Result<World> {
        let (len, modified) = stamp(path);
        let mut idx = self.load_index();
        let mut w = w.clone();
        w.base.preview = None;
        idx.insert(file_name(path), IndexEntry { len, modified, world: w.clone() });
        self.save_index(&idx)?;
        w.base.preview = read_preview(path).as_deref().map(data_url);
        Ok(w)
    }

    /// Saves a (client) world into the library.
    pub fn add(&self, world: &WorldFiles, info: &WorldInfo, meta: &AddMeta) -> Result<World> {
        let export = ExportInfo {
            name: Some(info.name.clone()),
            game_version: meta.game_version.clone(),
            source: bhw_world::Source { kind: meta.kind, device_name: meta.device_name.clone() },
            created_at: info.created_rfc3339(),
        };
        let path = self.inner.add_world(world, &export, info.preview_png.as_deref())?;
        let w = World {
            base: info.device_world(&file_name(&path), meta.game_version.as_deref().unwrap_or_default()),
            world_id: world.id.clone(),
            origin: meta.origin,
            source: meta.source.clone(),
            source_detail: meta.source_detail.clone(),
            added: now_ms(),
            edited: None,
        };
        self.put_entry(&path, &w)
    }

    fn carried(&self, id: &str) -> Option<World> {
        self.load_index().remove(id).map(|e| e.world)
    }

    fn reindex(&self, new: &Path, carry: Option<World>) -> Result<World> {
        let e = Lib::entry(new, carry.as_ref());
        self.put_entry(new, &e.world)
    }

    /// Renames world `id` (file, manifest and the name stored in the world).
    pub fn rename(&self, id: &str, name: &str) -> Result<World> {
        let path = self.path(id)?;
        let carry = self.carried(id);
        let new = self.inner.rename_world(&path, name.trim(), Some(&SaveMeta::default()))?;
        self.forget(id)?;
        self.reindex(&new, carry)
    }

    /// Copies world `id` with a new id and name.
    pub fn duplicate(&self, id: &str) -> Result<World> {
        let path = self.path(id)?;
        let carry = self.carried(id).map(|mut w| {
            w.added = now_ms();
            w
        });
        let new = self.inner.duplicate_world(&path, Some(&SaveMeta::default()))?;
        self.reindex(&new, carry)
    }

    fn forget(&self, id: &str) -> Result<()> {
        let mut idx = self.load_index();
        idx.remove(id);
        self.save_index(&idx)
    }

    /// Replaces the world in library file `id` with an edited version (manifest kept, preview redrawn) and marks
    /// it as edited now.
    pub fn save_edit(&self, id: &str, world: &WorldFiles) -> Result<World> {
        let path = self.path(id)?;
        let old = open_bhworld(&path, &Limits::default())?;
        let info = inspect(world, None)?;
        let mut manifest = old.manifest;
        manifest.world.name = Some(info.name.clone());
        bhw_world::bhworld::save_with_manifest(&path, world, manifest, info.preview_png.as_deref())?;
        let carry = self.carried(id).map(|mut w| {
            w.edited = Some(now_ms());
            w
        });
        self.reindex(&path, carry)
    }

    /// Deletes world `id`.
    pub fn delete(&self, id: &str) -> Result<()> {
        self.inner.delete_world(&self.path(id)?)?;
        self.forget(id)
    }
}
