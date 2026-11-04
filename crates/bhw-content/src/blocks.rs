//! Terrain: the `blocks` database as a store of macro blocks (32 × 32 tiles) decoded on demand.
//!
//! Records stay compressed until a tile of theirs is read; a bounded cache keeps recently used blocks decoded.
//! Changed blocks are kept decoded until [`BlockStore::take_dirty`] hands them back for writing. Tiles can only be
//! read and changed in blocks the save holds: areas the game never generated have no record and are refused.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use bhw_save::blocks::{block_key, parse_block_key, MacroBlock, BLOCK_RAW_LEN, MACRO_SIZE, WORLD_HEIGHT_MACROS};

use crate::error::{Error, Result};
use crate::tile::{TileData, TILE_BYTES};

/// World height in tiles.
pub const WORLD_HEIGHT: u32 = MACRO_SIZE * WORLD_HEIGHT_MACROS;

/// Decoded blocks kept besides the changed ones (about 64 KiB each).
pub const CACHE_BLOCKS: usize = 768;

/// Macro block coordinates.
pub type MacroPos = (u32, u32);

/// The macro block holding tile `(x, y)` and the tile's local position in it.
pub fn split(x: u32, y: u32) -> (MacroPos, (u32, u32)) {
    ((x / MACRO_SIZE, y / MACRO_SIZE), (x % MACRO_SIZE, y % MACRO_SIZE))
}

/// The terrain of a world.
#[derive(Debug, Clone, Default)]
pub struct BlockStore {
    stored: BTreeMap<MacroPos, Vec<u8>>,
    cache: HashMap<MacroPos, MacroBlock>,
    dirty: BTreeSet<MacroPos>,
    bad: BTreeSet<MacroPos>,
    /// Records whose key is not `<x>_<y>`; written back unchanged.
    pub foreign: Vec<(Vec<u8>, Vec<u8>)>,
}

impl BlockStore {
    /// A store over the records of a `blocks` database.
    pub fn new(entries: &[(Vec<u8>, Vec<u8>)]) -> BlockStore {
        let mut s = BlockStore::default();
        for (k, v) in entries {
            match parse_block_key(k) {
                Some(p) => {
                    s.stored.insert(p, v.clone());
                }
                None => s.foreign.push((k.clone(), v.clone())),
            }
        }
        s
    }

    /// Positions of all stored blocks, in order.
    pub fn positions(&self) -> impl Iterator<Item = MacroPos> + '_ {
        self.stored.keys().copied()
    }

    /// Number of stored blocks.
    pub fn len(&self) -> usize {
        self.stored.len()
    }

    /// True if no block is stored.
    pub fn is_empty(&self) -> bool {
        self.stored.is_empty()
    }

    /// True if the block at `pos` is stored.
    pub fn has(&self, pos: MacroPos) -> bool {
        self.stored.contains_key(&pos)
    }

    /// Blocks whose record could not be decoded so far.
    pub fn damaged(&self) -> impl Iterator<Item = MacroPos> + '_ {
        self.bad.iter().copied()
    }

    /// The decoded block at `pos`: `Ok(None)` if it is not stored, an error if its record is damaged.
    pub fn block(&mut self, pos: MacroPos) -> Result<Option<&MacroBlock>> {
        if !self.load(pos)? {
            return Ok(None);
        }
        Ok(self.cache.get(&pos))
    }

    fn load(&mut self, pos: MacroPos) -> Result<bool> {
        if self.cache.contains_key(&pos) {
            return Ok(true);
        }
        let Some(raw) = self.stored.get(&pos) else { return Ok(false) };
        let b = MacroBlock::decode(&block_key(pos.0, pos.1), raw).inspect_err(|_| {
            self.bad.insert(pos);
        })?;
        if self.cache.len() >= CACHE_BLOCKS + self.dirty.len() {
            let dirty = &self.dirty;
            self.cache.retain(|p, _| dirty.contains(p));
        }
        self.cache.insert(pos, b);
        Ok(true)
    }

    /// The tile at world position `(x, y)` (`y = 0` at the bottom), `Ok(None)` outside the stored blocks.
    pub fn tile(&mut self, x: u32, y: u32) -> Result<Option<TileData>> {
        let (pos, (lx, ly)) = split(x, y);
        Ok(self.block(pos)?.map(|b| TileData::from_slice(b.tile(lx, ly).0)))
    }

    /// Replaces the tile at `(x, y)`; returns the old one. Fails outside the stored blocks.
    pub fn set_tile(&mut self, x: u32, y: u32, t: TileData) -> Result<TileData> {
        let (pos, (lx, ly)) = split(x, y);
        if !self.load(pos)? {
            return Err(Error::Invalid(format!("tile {x}, {y} is in an area the game has not generated")));
        }
        let b = self.cache.get_mut(&pos).expect("loaded");
        let bytes = b.tile_bytes_mut(lx, ly);
        let old = TileData::from_slice(bytes);
        if old != t {
            bytes.copy_from_slice(&t.0);
            self.dirty.insert(pos);
        }
        Ok(old)
    }

    /// The 1024 tiles of a block as raw bytes (bottom row first), `Ok(None)` if not stored.
    pub fn block_tiles(&mut self, pos: MacroPos) -> Result<Option<&[u8]>> {
        Ok(self.block(pos)?.map(|b| &b.raw()[..BLOCK_RAW_LEN - 5]))
    }

    /// Stores a new block at `pos` (where the game has not generated one yet) with every tile set to `fill`,
    /// block version 0 and update counter 0, like a block the game has just generated. Fails if one is stored.
    pub fn create(&mut self, pos: MacroPos, fill: &TileData) -> Result<()> {
        if self.stored.contains_key(&pos) {
            return Err(Error::Invalid(format!("block {}_{} exists", pos.0, pos.1)));
        }
        let mut raw = Vec::with_capacity(BLOCK_RAW_LEN);
        for _ in 0..MACRO_SIZE * MACRO_SIZE {
            raw.extend_from_slice(&fill.0);
        }
        raw.extend_from_slice(&[0; 5]);
        let key = block_key(pos.0, pos.1);
        let b = MacroBlock::decode(&key, &crate::stored::gzip(&raw)).expect("own encoding");
        self.stored.insert(pos, b.encode());
        self.cache.insert(pos, b);
        self.dirty.insert(pos);
        Ok(())
    }

    /// Removes a stored block (to undo [`BlockStore::create`]); true if it was there.
    pub fn remove(&mut self, pos: MacroPos) -> bool {
        self.cache.remove(&pos);
        self.dirty.remove(&pos);
        self.bad.remove(&pos);
        self.stored.remove(&pos).is_some()
    }

    /// A tile to fill new blocks with: the open sky (air in front of air) of the highest stored block row, with
    /// its zone and climate bytes, no contents and no owner; plain air when the world has no sky tile.
    pub fn sky_tile(&mut self) -> TileData {
        let mut tops: Vec<MacroPos> = self.stored.keys().copied().collect();
        tops.sort_by_key(|p| std::cmp::Reverse(p.1));
        for pos in tops.into_iter().take(64) {
            let Ok(Some(b)) = self.block(pos) else { continue };
            let t = TileData::from_slice(b.tile(0, MACRO_SIZE - 1).0);
            if t.fg() == crate::tile::AIR && t.bg() == crate::tile::AIR {
                let mut t = t;
                t.set_contents(0);
                t.clear_owner();
                return t;
            }
        }
        let mut t = TileData::default();
        t.set_fg(crate::tile::AIR);
        t.set_bg(crate::tile::AIR);
        t
    }

    /// True if any block changed.
    pub fn is_dirty(&self) -> bool {
        !self.dirty.is_empty()
    }

    /// Encodes the changed blocks (update counter + 1), stores them as the new originals and returns their records.
    pub fn take_dirty(&mut self) -> Vec<(Vec<u8>, Vec<u8>)> {
        let mut out = Vec::new();
        for pos in std::mem::take(&mut self.dirty) {
            let b = self.cache.get_mut(&pos).expect("dirty blocks stay cached");
            let raw_len = b.raw().len();
            let counter = b.update_index().wrapping_add(1);
            let mut raw = b.raw().to_vec();
            raw[raw_len - 4..].copy_from_slice(&counter.to_le_bytes());
            let key = block_key(pos.0, pos.1);
            *b = MacroBlock::decode(&key, &crate::stored::gzip(&raw)).expect("own encoding");
            let enc = b.encode();
            self.stored.insert(pos, enc.clone());
            out.push((key, enc));
        }
        out
    }

    /// All records (stored blocks and foreign keys) as they are now, changed blocks encoded.
    pub fn entries(&mut self) -> Vec<(Vec<u8>, Vec<u8>)> {
        self.take_dirty();
        let mut v: Vec<_> = self.stored.iter().map(|(p, raw)| (block_key(p.0, p.1), raw.clone())).collect();
        v.extend(self.foreign.iter().cloned());
        v
    }
}

/// Number of tile bytes in a block.
pub const BLOCK_TILE_BYTES: usize = TILE_BYTES * (MACRO_SIZE * MACRO_SIZE) as usize;

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A stored block record whose tiles all have foreground `fg` and back wall `bg`.
    pub use crate::fixtures::block_record as record;

    #[test]
    fn read_write() {
        let entries = vec![record(0, 0, 1, 1), record(1, 0, 2, 2), (b"junk".to_vec(), b"x".to_vec())];
        let mut s = BlockStore::new(&entries);
        assert_eq!(s.len(), 2);
        assert!(!s.is_empty() && s.has((1, 0)) && !s.has((5, 5)));
        assert_eq!(s.positions().collect::<Vec<_>>(), vec![(0, 0), (1, 0)]);
        assert_eq!(s.foreign.len(), 1);
        assert_eq!(s.tile(33, 5).unwrap().unwrap().fg(), 2);
        assert_eq!(s.tile(500, 5).unwrap(), None);
        assert_eq!(s.block_tiles((0, 0)).unwrap().unwrap().len(), BLOCK_TILE_BYTES);
        assert!(s.block_tiles((9, 9)).unwrap().is_none());
        assert!(!s.is_dirty());
        let mut t = s.tile(3, 4).unwrap().unwrap();
        let same = s.set_tile(3, 4, t).unwrap();
        assert_eq!(same, t);
        assert!(!s.is_dirty());
        t.set_fg(9);
        assert_eq!(s.set_tile(3, 4, t).unwrap().fg(), 1);
        assert!(s.is_dirty());
        assert!(s.set_tile(900, 4, t).is_err());
        let out = s.take_dirty();
        assert_eq!(out.len(), 1);
        let b = MacroBlock::decode(&out[0].0, &out[0].1).unwrap();
        assert_eq!(b.update_index(), 1);
        assert_eq!(b.tile(3, 4).tile_type(), 9);
        assert!(!s.is_dirty());
        assert_eq!(s.tile(3, 4).unwrap().unwrap().fg(), 9);
        let all = s.entries();
        assert_eq!(all.len(), 3);
        assert_eq!(all[0].1, out[0].1);
        assert_eq!(all[1], entries[1]);
    }

    #[test]
    fn create_remove_sky() {
        let mut s = BlockStore::new(&[record(0, 0, 1, 1), record(3, 30, 2, 2), record(4, 31, 2, 1)]);
        let sky = s.sky_tile();
        assert_eq!((sky.fg(), sky.bg()), (2, 2));
        assert!(s.create((3, 30), &sky).is_err());
        s.create((7, 30), &sky).unwrap();
        assert!(s.has((7, 30)) && s.is_dirty());
        assert_eq!(s.tile(7 * 32 + 3, 30 * 32 + 3).unwrap().unwrap(), sky);
        let rec = s.entries();
        let b = MacroBlock::decode(&block_key(7, 30), &rec.iter().find(|r| r.0 == b"7_30").unwrap().1).unwrap();
        assert_eq!((b.version(), b.update_index()), (0, 1));
        assert!(s.remove((7, 30)));
        assert!(!s.remove((7, 30)));
        assert_eq!(s.entries().len(), 3);
        let mut none = BlockStore::new(&[record(0, 0, 1, 1), (b"1_0".to_vec(), b"bad".to_vec())]);
        let t = none.sky_tile();
        assert_eq!((t.fg(), t.bg(), t.zone()), (2, 2, 0));
    }

    #[test]
    fn damaged_and_cache() {
        let entries = vec![(b"0_0".to_vec(), b"bad".to_vec())];
        let mut s = BlockStore::new(&entries);
        assert!(s.tile(0, 0).is_err());
        assert!(s.set_tile(0, 0, TileData::default()).is_err());
        assert_eq!(s.damaged().collect::<Vec<_>>(), vec![(0, 0)]);
        // fill the cache past its bound: dirty blocks survive eviction
        let entries: Vec<_> = (0..CACHE_BLOCKS as u32 + 2).map(|x| record(x, 0, 1, 1)).collect();
        let mut s = BlockStore::new(&entries);
        s.set_tile(0, 0, TileData::default()).unwrap();
        for x in 1..CACHE_BLOCKS as u32 + 2 {
            s.tile(x * 32, 0).unwrap();
        }
        assert!(s.cache.len() <= CACHE_BLOCKS + 1);
        assert_eq!(s.take_dirty().len(), 1);
        assert_eq!(split(70, 33), ((2, 1), (6, 1)));
        assert_eq!(WORLD_HEIGHT, 1024);
    }
}
