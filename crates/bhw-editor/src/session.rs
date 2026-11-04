//! An editing session: one world's contents, the operations on it and their history.

use std::collections::{HashMap, HashSet};

use bhw_content::blocks::WORLD_HEIGHT;
use bhw_content::players::{Inventory, Stats};
use bhw_content::tile::AIR;
use bhw_content::{TileData, WorldContent};
use bhw_lmdb::EnvContents;
use serde::Serialize;

use crate::brush::{describe, is_open, is_protected, Brush, Match};
use crate::error::{Error, Result};
use crate::geom::{Rect, MAX_TILES};
use crate::history::{Change, History, Label, OpKind, Step};
use crate::schematic::{Cell, Schematic};

/// What a tile operation does to one tile: the new tile, or `None` to leave it alone.
type TileFn<'a> = dyn FnMut((u32, u32), &TileData) -> Option<TileData> + 'a;

/// Largest number of tile changes one step may hold.
pub const MAX_CHANGES: u64 = 1_000_000;

/// What an operation did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
pub struct Outcome {
    /// Tiles changed.
    pub changed: u64,
    /// Tiles left alone because they belong to an object (tree, door, chest…) and object editing is off.
    pub protected: u64,
    /// Tiles left alone because the game has not generated that area and generating is off.
    pub missing: u64,
    /// Blocks created in areas the game had not generated.
    pub created: u64,
    /// Dynamic objects removed because one of their tiles changed.
    pub removed: u64,
}

/// What edits may touch beyond plain generated terrain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, serde::Deserialize)]
pub struct EditOptions {
    /// Create blocks where the game has not generated the world yet (filled with open sky first).
    pub generate: bool,
    /// Change tiles that belong to objects (trees, plants, workbenches, doors, chests…); the objects are removed.
    pub objects: bool,
}

/// How far (in tiles) from an object its owned tiles are looked for.
const OWNED_REACH: u32 = 48;

/// An editing session.
#[derive(Debug, Clone)]
pub struct Session {
    /// The world being edited.
    pub world: WorldContent,
    /// What edits may touch.
    pub options: EditOptions,
    history: History,
    max_changes: u64,
}

fn label(kind: OpKind, a: Option<String>, b: Option<String>) -> Label {
    Label { kind, tiles: 0, a, b }
}

impl Session {
    /// Starts a session on a loaded world.
    pub fn new(world: WorldContent) -> Session {
        Session { world, options: EditOptions::default(), history: History::default(), max_changes: MAX_CHANGES }
    }

    /// World width in tiles.
    pub fn width(&self) -> u32 {
        self.world.width()
    }

    /// World height in tiles.
    pub fn height(&self) -> u32 {
        WORLD_HEIGHT
    }

    /// The tile at `(x, y)`, `None` outside the generated area.
    pub fn tile(&mut self, x: u32, y: u32) -> Result<Option<TileData>> {
        if x >= self.width() || y >= self.height() {
            return Ok(None);
        }
        Ok(self.world.blocks.tile(x, y)?)
    }

    /// The history steps and how many of them are applied.
    pub fn history(&self) -> (Vec<Label>, usize) {
        (self.history.steps().iter().map(|s| s.label.clone()).collect(), self.history.cursor())
    }

    /// Number of unsaved steps.
    pub fn unsaved(&self) -> usize {
        self.history.unsaved()
    }

    /// Applies `f` to the tiles at `points` as one step. `f` gets the position and tile and returns the new tile,
    /// or `None` to leave it alone. Tiles of objects and of ungenerated areas are skipped and counted unless the
    /// [`EditOptions`] allow them (then objects are removed and blocks created).
    fn edit_tiles(&mut self, mut label: Label, points: Vec<(u32, u32)>, f: &mut TileFn<'_>) -> Result<Outcome> {
        let (w, h) = (self.width(), self.height());
        let mut out = Outcome::default();
        let mut changes = Vec::new();
        let mut seen = HashSet::new();
        let mut sky: Option<TileData> = None;
        let mut occupied: Option<HashSet<(u32, u32)>> = None;
        for (x, y) in points {
            if !seen.insert((x, y)) {
                continue;
            }
            let t = if x < w && y < h { self.world.blocks.tile(x, y)? } else { None };
            let t = match t {
                Some(t) => t,
                None if x < w && y < h && self.options.generate => {
                    let fill = *sky.get_or_insert_with(|| self.world.blocks.sky_tile());
                    if f((x, y), &fill).filter(|n| *n != fill).is_none() {
                        continue;
                    }
                    let pos = bhw_content::blocks::split(x, y).0;
                    self.world.blocks.create(pos, &fill)?;
                    changes.push(Change::BlockCreated { pos, fill });
                    out.created += 1;
                    fill
                }
                None => {
                    out.missing += 1;
                    continue;
                }
            };
            let Some(mut n) = f((x, y), &t).filter(|n| *n != t) else { continue };
            let objects_here = !is_open(&n) && {
                let occ = occupied.get_or_insert_with(|| {
                    let (all, _) = self.world.objects.all();
                    all.iter()
                        .filter_map(|o| o.pos())
                        .filter(|p| p.0 >= 0 && p.1 >= 0)
                        .map(|p| (p.0 as u32, p.1 as u32))
                        .collect()
                });
                occ.contains(&(x, y))
            };
            if is_protected(&t) || objects_here {
                if !self.options.objects {
                    out.protected += 1;
                    continue;
                }
                out.removed += self.detach(x, y, &t, &mut changes)?;
                if let Some(occ) = occupied.as_mut() {
                    occ.remove(&(x, y));
                }
                let mut base = t;
                base.clear_owner();
                if !crate::brush::plain_contents(base.contents()) {
                    base.set_contents(0);
                }
                n = f((x, y), &base).unwrap_or(base);
            }
            if changes.len() as u64 >= self.max_changes {
                self.revert(&changes)?;
                return Err(Error::Invalid(format!("more than {} tiles in one change", self.max_changes)));
            }
            self.world.blocks.set_tile(x, y, n)?;
            changes.push(Change::Tile { x, y, before: t, after: n });
        }
        out.changed = changes.iter().filter(|c| matches!(c, Change::Tile { .. })).count() as u64;
        if !changes.is_empty() {
            label.tiles = out.changed;
            self.history.push(Step { label, changes });
        }
        Ok(out)
    }

    /// Removes the objects tile `(x, y)` belongs to (its owner, and objects standing on it) and clears the other
    /// tiles they own (contents and owner). Returns how many objects were removed.
    fn detach(&mut self, x: u32, y: u32, t: &TileData, changes: &mut Vec<Change>) -> Result<u64> {
        let owner = t.owner();
        let (all, _) = self.world.objects.all();
        let mut gone: Vec<_> = all
            .into_iter()
            .filter(|o| (owner != 0 && o.unique_id() == Some(owner)) || o.pos() == Some((x as i64, y as i64)))
            .collect();
        // remove from the end of each record so the indexes of the others stay valid
        gone.sort_by(|a, b| (&b.key, b.index).cmp(&(&a.key, a.index)));
        let mut n = 0;
        for o in gone {
            let dict = self.world.objects.remove(&o.key, o.index)?;
            changes.push(Change::Object { key: o.key.clone(), index: o.index, before: Some(dict), after: None });
            n += 1;
            let Some(id) = o.unique_id() else { continue };
            let (cx, cy) = o.pos().map_or((x, y), |(px, py)| (px.max(0) as u32, py.max(0) as u32));
            let (w, h) = (self.width(), self.height());
            for ty in cy.saturating_sub(OWNED_REACH)..(cy + OWNED_REACH).min(h) {
                for tx in cx.saturating_sub(OWNED_REACH)..(cx + OWNED_REACH).min(w) {
                    if (tx, ty) == (x, y) {
                        continue;
                    }
                    let Some(ot) = self.world.blocks.tile(tx, ty)? else { continue };
                    if ot.owner() != id {
                        continue;
                    }
                    let mut nt = ot;
                    nt.clear_owner();
                    if !crate::brush::plain_contents(nt.contents()) {
                        nt.set_contents(0);
                    }
                    self.world.blocks.set_tile(tx, ty, nt)?;
                    changes.push(Change::Tile { x: tx, y: ty, before: ot, after: nt });
                }
            }
        }
        Ok(n)
    }

    fn check_area(r: &Rect) -> Result<()> {
        if r.area() > MAX_TILES || r.area() == 0 {
            return Err(Error::Invalid(format!("an area must have 1 to {MAX_TILES} tiles")));
        }
        Ok(())
    }

    fn check_brush(b: &Brush) -> Result<()> {
        if !b.is_valid() {
            return Err(Error::Invalid("unknown block".into()));
        }
        Ok(())
    }

    /// Paints `points` with `brush` (pencil, brush or eraser strokes). `only_solid` skips open tiles (air,
    /// water, lava).
    pub fn paint(&mut self, kind: OpKind, points: &[(u32, u32)], brush: Brush, only_solid: bool) -> Result<Outcome> {
        Self::check_brush(&brush)?;
        let name = describe(brush.tile, brush.contents, brush.layer);
        let l = label(kind, (kind != OpKind::Remove).then_some(name), None);
        self.edit_tiles(l, points.to_vec(), &mut |_, t| (!only_solid || !is_open(t)).then(|| brush.apply(t)))
    }

    /// Fills a rectangle with `brush`.
    pub fn fill(&mut self, r: Rect, brush: Brush) -> Result<Outcome> {
        Self::check_area(&r)?;
        Self::check_brush(&brush)?;
        let l = label(OpKind::Fill, Some(describe(brush.tile, brush.contents, brush.layer)), None);
        self.edit_tiles(l, r.tiles().collect(), &mut |_, t| Some(brush.apply(t)))
    }

    /// Removes the foreground of a rectangle (air, no contents).
    pub fn clear(&mut self, r: Rect) -> Result<Outcome> {
        Self::check_area(&r)?;
        self.edit_tiles(label(OpKind::Clear, None, None), r.tiles().collect(), &mut |_, t| {
            Some(Brush::eraser().apply(t))
        })
    }

    fn area_points(&mut self, area: Option<Rect>) -> Result<Vec<(u32, u32)>> {
        match area {
            Some(r) => {
                Self::check_area(&r)?;
                Ok(r.tiles().collect())
            }
            None => Ok(self
                .world
                .blocks
                .positions()
                .flat_map(|(mx, my)| Rect { x: mx * 32, y: my * 32, w: 32, h: 32 }.tiles().collect::<Vec<_>>())
                .collect()),
        }
    }

    /// Tiles matching `m` in `area` (whole world if `None`): at most `limit` positions, and the total count.
    pub fn find(&mut self, m: Match, area: Option<Rect>, limit: usize) -> Result<(Vec<(u32, u32)>, u64)> {
        let mut found = Vec::new();
        let mut n = 0;
        for (x, y) in self.area_points(area)? {
            if let Some(t) = self.tile(x, y)? {
                if m.matches(&t) {
                    n += 1;
                    if found.len() < limit {
                        found.push((x, y));
                    }
                }
            }
        }
        Ok((found, n))
    }

    /// Replaces tiles matching `m` with `to` in `area` (whole world if `None`).
    pub fn replace(&mut self, m: Match, to: Brush, area: Option<Rect>) -> Result<Outcome> {
        Self::check_brush(&to)?;
        let pts = self.area_points(area)?;
        let l = label(
            OpKind::Replace,
            Some(describe(m.tile, m.contents.unwrap_or(0), m.layer)),
            Some(describe(to.tile, to.contents, to.layer)),
        );
        self.edit_tiles(l, pts, &mut |_, t| m.matches(t).then(|| to.apply(t)))
    }

    /// Copies a rectangle into a schematic.
    pub fn copy(&mut self, r: Rect) -> Result<Schematic> {
        Self::check_area(&r)?;
        let mut s = Schematic::new(r.w, r.h);
        s.meta.source_world = self.world.record.as_ref().and_then(|r| r.name()).map(str::to_string);
        for (x, y) in r.tiles() {
            if let Some(t) = self.tile(x, y)? {
                *s.cell_mut(x - r.x, y - r.y) = Cell::from_tile(&t);
            }
        }
        Ok(s)
    }

    /// Copies a rectangle and clears it (one step).
    pub fn cut(&mut self, r: Rect) -> Result<(Schematic, Outcome)> {
        let s = self.copy(r)?;
        let erase: &mut TileFn<'_> = &mut |_, t| Some(Brush::eraser().apply(t));
        let o = self.edit_tiles(label(OpKind::Cut, None, None), r.tiles().collect(), erase)?;
        Ok((s, o))
    }

    /// Pastes a schematic with its bottom-left corner at `(x, y)`. Absent cells, and air cells when `skip_air`,
    /// leave the world alone.
    pub fn paste(&mut self, s: &Schematic, x: u32, y: u32, skip_air: bool) -> Result<Outcome> {
        let mut cells = HashMap::new();
        let mut order = Vec::new();
        for dy in 0..s.height {
            for dx in 0..s.width {
                let c = *s.cell(dx, dy);
                if c.present && !(skip_air && c.fg == AIR) {
                    let p = (x.saturating_add(dx), y.saturating_add(dy));
                    cells.insert(p, c);
                    order.push(p);
                }
            }
        }
        let name = (!s.meta.name.is_empty()).then(|| s.meta.name.clone());
        self.edit_tiles(label(OpKind::Paste, name, None), order, &mut |p, t| Some(cells[&p].apply(t)))
    }

    fn blockhead_name(&self, player: &str, id: u64) -> Option<String> {
        self.world.players.list().into_iter().find(|b| b.player == player && b.unique_id == id).map(|b| b.name)
    }

    fn edit_blockhead(&mut self, player: &str, id: u64, f: &mut dyn FnMut(&mut Session) -> Result<()>) -> Result<()> {
        let before =
            self.world.players.dict(player, id).cloned().ok_or_else(|| Error::Invalid(format!("no blockhead {id}")))?;
        f(self)?;
        let after = self.world.players.dict(player, id).cloned().expect("still there");
        let name = self.blockhead_name(player, id);
        let change = Change::Blockhead { player: player.into(), id, before, after };
        self.history.push(Step { label: label(OpKind::Blockhead, name, None), changes: vec![change] });
        Ok(())
    }

    /// Moves a blockhead to tile `(x, y)`.
    pub fn move_blockhead(&mut self, player: &str, id: u64, x: u32, y: u32) -> Result<()> {
        if x >= self.width() || y >= self.height() {
            return Err(Error::Invalid("outside the world".into()));
        }
        self.edit_blockhead(player, id, &mut |s| Ok(s.world.players.set_pos(player, id, x as i64, y as i64)?))
    }

    /// Sets a blockhead's needs (each clamped to 0..1).
    pub fn set_stats(&mut self, player: &str, id: u64, stats: Stats) -> Result<()> {
        let c = |v: f32| if v.is_finite() { v.clamp(0.0, 1.0) } else { 0.0 };
        let s = Stats {
            health: c(stats.health),
            happiness: c(stats.happiness),
            fullness: c(stats.fullness),
            energy: c(stats.energy),
        };
        self.edit_blockhead(player, id, &mut |me| Ok(me.world.players.set_stats(player, id, s)?))
    }

    /// Sets an inventory slot to `count` items of `item` (0 empties it).
    pub fn set_stack(&mut self, player: &str, id: u64, slot: usize, item: u16, count: usize) -> Result<()> {
        if count > 999 {
            return Err(Error::Invalid("a stack holds at most 999 items".into()));
        }
        if count > 0 && bhw_content::names::item_name(item).is_none() {
            return Err(Error::Invalid(format!("unknown item {item}")));
        }
        let before =
            self.world.players.inventory(player, id).cloned().ok_or_else(|| Error::Invalid("no inventory".into()))?;
        let mut after = before.clone();
        after.set_stack(slot, item, count)?;
        self.apply_inventory(player, id, &after);
        let name = self.blockhead_name(player, id);
        let item_name = bhw_content::names::item_name(item).map(str::to_string);
        let change = Change::Inventory { player: player.into(), id, before, after };
        self.history.push(Step { label: label(OpKind::Inventory, name, item_name), changes: vec![change] });
        Ok(())
    }

    fn apply_inventory(&mut self, player: &str, id: u64, inv: &Inventory) {
        self.world.players.set_inventory(player, id, inv.clone());
    }

    fn edit_record(&mut self, f: &mut dyn FnMut(&mut bhw_content::WorldRecord) -> Result<()>) -> Result<()> {
        let rec = self.world.record.as_mut().ok_or_else(|| Error::Invalid("the world has no record".into()))?;
        let before = rec.dict.clone();
        f(rec)?;
        let after = rec.dict.clone();
        if after != before {
            self.history.push(Step {
                label: label(OpKind::Settings, None, None),
                changes: vec![Change::Record { before, after }],
            });
        }
        Ok(())
    }

    /// Renames the world.
    pub fn set_name(&mut self, name: &str) -> Result<()> {
        self.edit_record(&mut |r| Ok(r.set_name(name)?))
    }

    /// Sets the time of day (hour 0..24).
    pub fn set_time_of_day(&mut self, hour: f64) -> Result<()> {
        self.edit_record(&mut |r| Ok(r.set_time_of_day(hour)?))
    }

    fn apply_change(&mut self, c: &Change, forward: bool) -> Result<()> {
        match c {
            Change::Tile { x, y, before, after } => {
                self.world.blocks.set_tile(*x, *y, if forward { *after } else { *before })?;
            }
            Change::Blockhead { player, id, before, after } => {
                self.world.players.set_dict(player, *id, if forward { after } else { before }.clone())?;
            }
            Change::Inventory { player, id, before, after } => {
                self.apply_inventory(player, *id, if forward { after } else { before });
            }
            Change::BlockCreated { pos, fill } => {
                if forward {
                    self.world.blocks.create(*pos, fill)?;
                } else {
                    self.world.blocks.remove(*pos);
                }
            }
            Change::Object { key, index, before, after } => match (forward, before, after) {
                (true, _, Some(d)) | (false, Some(d), _) => self.world.objects.insert(key, *index, d.clone())?,
                _ => {
                    self.world.objects.remove(key, *index)?;
                }
            },
            Change::Record { before, after } => {
                self.world.record.as_mut().expect("record changes need a record").dict =
                    if forward { after } else { before }.clone();
            }
        }
        Ok(())
    }

    fn revert(&mut self, changes: &[Change]) -> Result<()> {
        for c in changes.iter().rev() {
            self.apply_change(c, false)?;
        }
        Ok(())
    }

    /// Undoes the last applied step; returns its label.
    pub fn undo(&mut self) -> Result<Option<Label>> {
        let Some(step) = self.history.undo().cloned() else { return Ok(None) };
        self.revert(&step.changes)?;
        Ok(Some(step.label))
    }

    /// Redoes the next undone step; returns its label.
    pub fn redo(&mut self) -> Result<Option<Label>> {
        let Some(step) = self.history.redo().cloned() else { return Ok(None) };
        for c in &step.changes {
            self.apply_change(c, true)?;
        }
        Ok(Some(step.label))
    }

    /// Undoes or redoes until `cursor` steps are applied.
    pub fn go_to(&mut self, cursor: usize) -> Result<()> {
        if cursor > self.history.steps().len() {
            return Err(Error::Invalid("no such step".into()));
        }
        while self.history.cursor() > cursor {
            self.undo()?;
        }
        while self.history.cursor() < cursor {
            self.redo()?;
        }
        Ok(())
    }

    /// Positions of the tiles changed by the applied steps (for the compare view and change markers).
    pub fn changed_tiles(&self) -> Vec<(u32, u32)> {
        let mut set = std::collections::BTreeSet::new();
        for s in &self.history.steps()[..self.history.cursor()] {
            for c in &s.changes {
                if let Change::Tile { x, y, .. } = c {
                    set.insert((*y, *x));
                }
            }
        }
        set.into_iter().map(|(y, x)| (x, y)).collect()
    }

    /// The world with all changes, for saving; afterwards the current state counts as saved.
    pub fn save(&mut self) -> Result<EnvContents> {
        let env = self.world.to_env()?;
        self.history.mark_saved();
        Ok(env)
    }
}

#[cfg(test)]
mod tests;
