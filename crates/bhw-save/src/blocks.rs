//! Terrain chunks: the `blocks` database.
//!
//! The world is cut into macro blocks of 32 × 32 tiles. Each stored macro block is one record: key `"<x>_<y>"`
//! (decimal macro coordinates, `y` counted upwards from the bottom of the world), value a gzip stream that
//! inflates to 1024 tiles of 64 bytes followed by a one-byte block version and a little-endian `u32` update
//! counter. Inside a block, tile `i` is at local `(i % 32, i / 32)`, row 0 being the bottom row. Game clients and
//! Blockheads Server use the same encoding.

use std::io::{Read, Write};

use crate::error::{Error, Result};

/// Tiles per macro block side.
pub const MACRO_SIZE: u32 = 32;
/// Bytes per tile record.
pub const TILE_BYTES: usize = 64;
/// Tiles per macro block.
pub const BLOCK_TILES: usize = (MACRO_SIZE * MACRO_SIZE) as usize;
/// Inflated size of a block record: tiles, version byte, update counter.
pub const BLOCK_RAW_LEN: usize = BLOCK_TILES * TILE_BYTES + 1 + 4;
/// The world is this many macro blocks (1024 tiles) high.
pub const WORLD_HEIGHT_MACROS: u32 = 32;

/// Parses a `blocks` key `"<x>_<y>"`.
pub fn parse_block_key(key: &[u8]) -> Option<(u32, u32)> {
    let s = std::str::from_utf8(key).ok()?;
    let (x, y) = s.split_once('_')?;
    let num = |t: &str| if !t.is_empty() && t.bytes().all(|b| b.is_ascii_digit()) { t.parse().ok() } else { None };
    Some((num(x)?, num(y)?))
}

/// The key of the block at macro position `(x, y)`.
pub fn block_key(x: u32, y: u32) -> Vec<u8> {
    format!("{x}_{y}").into_bytes()
}

/// One decoded macro block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacroBlock {
    /// Macro x (0 at the world's left edge).
    pub x: u32,
    /// Macro y (0 at the bottom of the world).
    pub y: u32,
    raw: Vec<u8>,
}

impl MacroBlock {
    /// A block of all-zero tiles (version 0).
    pub fn empty(x: u32, y: u32) -> MacroBlock {
        MacroBlock { x, y, raw: vec![0; BLOCK_RAW_LEN] }
    }

    /// Decodes one `blocks` record.
    pub fn decode(key: &[u8], value: &[u8]) -> Result<MacroBlock> {
        let (x, y) = parse_block_key(key)
            .ok_or_else(|| Error::Format(format!("bad block key {:?}", String::from_utf8_lossy(key))))?;
        let mut raw = Vec::with_capacity(BLOCK_RAW_LEN);
        flate2::read::GzDecoder::new(value)
            .read_to_end(&mut raw)
            .map_err(|e| Error::Format(format!("block {x}_{y}: {e}")))?;
        if raw.len() != BLOCK_RAW_LEN {
            return Err(Error::Format(format!(
                "block {x}_{y}: {} bytes after inflating, expected {BLOCK_RAW_LEN}",
                raw.len()
            )));
        }
        Ok(MacroBlock { x, y, raw })
    }

    /// Encodes the block the way the game stores it (gzip, no file name, mtime 0).
    pub fn encode(&self) -> Vec<u8> {
        let mut gz = flate2::GzBuilder::new().operating_system(3).write(Vec::new(), flate2::Compression::default());
        gz.write_all(&self.raw).expect("writing to a Vec cannot fail");
        gz.finish().expect("writing to a Vec cannot fail")
    }

    /// The inflated record (tiles, version, update counter).
    pub fn raw(&self) -> &[u8] {
        &self.raw
    }

    /// Block format version (8 for blocks the 1.7 game has updated, 0 for freshly generated ones).
    pub fn version(&self) -> u8 {
        self.raw[BLOCK_TILES * TILE_BYTES]
    }

    /// Update counter.
    pub fn update_index(&self) -> u32 {
        let o = BLOCK_TILES * TILE_BYTES + 1;
        u32::from_le_bytes(self.raw[o..o + 4].try_into().expect("4 bytes"))
    }

    /// The tile at local `(lx, ly)` (`ly = 0` is the bottom row). Panics if out of range.
    pub fn tile(&self, lx: u32, ly: u32) -> Tile<'_> {
        assert!(lx < MACRO_SIZE && ly < MACRO_SIZE, "tile ({lx}, {ly}) outside the block");
        let i = (ly * MACRO_SIZE + lx) as usize * TILE_BYTES;
        Tile(&self.raw[i..i + TILE_BYTES])
    }

    /// Mutable tile bytes at local `(lx, ly)`. Panics if out of range.
    pub fn tile_bytes_mut(&mut self, lx: u32, ly: u32) -> &mut [u8] {
        assert!(lx < MACRO_SIZE && ly < MACRO_SIZE, "tile ({lx}, {ly}) outside the block");
        let i = (ly * MACRO_SIZE + lx) as usize * TILE_BYTES;
        &mut self.raw[i..i + TILE_BYTES]
    }
}

/// A view of one 64-byte tile record. Only the fields needed for previews are named.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tile<'a>(pub &'a [u8]);

impl Tile<'_> {
    /// Foreground tile type (see [`crate::tiles::TileType`]).
    pub fn tile_type(&self) -> u8 {
        self.0[0]
    }
    /// Back wall tile type.
    pub fn back_wall(&self) -> u8 {
        self.0[1]
    }
    /// Contents (ores, plants, …).
    pub fn contents(&self) -> u8 {
        self.0[3]
    }
}

/// Decodes all records of a `blocks` database, skipping (and returning) undecodable ones.
pub fn decode_all<'a>(entries: impl IntoIterator<Item = &'a (Vec<u8>, Vec<u8>)>) -> (Vec<MacroBlock>, Vec<Error>) {
    let mut ok = Vec::new();
    let mut bad = Vec::new();
    for (k, v) in entries {
        match MacroBlock::decode(k, v) {
            Ok(b) => ok.push(b),
            Err(e) => bad.push(e),
        }
    }
    (ok, bad)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys() {
        assert_eq!(parse_block_key(b"198_18"), Some((198, 18)));
        assert_eq!(block_key(3, 0), b"3_0");
        for bad in [&b"1"[..], b"_1", b"1_", b"a_1", b"1_-2", b"\xff_1", b"99999999999_1"] {
            assert_eq!(parse_block_key(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn encode_decode() {
        let mut b = MacroBlock::empty(4, 5);
        b.tile_bytes_mut(1, 2)[0] = 27;
        b.tile_bytes_mut(1, 2)[1] = 6;
        b.tile_bytes_mut(1, 2)[3] = 9;
        let enc = b.encode();
        assert_eq!(&enc[..10], &[0x1f, 0x8b, 8, 0, 0, 0, 0, 0, 0, 3]);
        let d = MacroBlock::decode(b"4_5", &enc).unwrap();
        assert_eq!(d, b);
        let t = d.tile(1, 2);
        assert_eq!((t.tile_type(), t.back_wall(), t.contents()), (27, 6, 9));
        assert_eq!(d.raw().len(), BLOCK_RAW_LEN);
        assert_eq!((d.version(), d.update_index()), (0, 0));
    }

    #[test]
    fn trailer() {
        let mut b = MacroBlock::empty(0, 0);
        b.raw[BLOCK_TILES * TILE_BYTES] = 8;
        b.raw[BLOCK_TILES * TILE_BYTES + 1] = 0x10;
        assert_eq!((b.version(), b.update_index()), (8, 16));
    }

    #[test]
    fn decode_errors() {
        assert!(matches!(MacroBlock::decode(b"x", b""), Err(Error::Format(_))));
        assert!(matches!(MacroBlock::decode(b"1_1", b"not gzip"), Err(Error::Format(_))));
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        gz.write_all(&[0; 10]).unwrap();
        let short = gz.finish().unwrap();
        let e = MacroBlock::decode(b"1_1", &short).unwrap_err();
        assert!(e.to_string().contains("expected 65541"), "{e}");
        let entries = vec![(b"1_1".to_vec(), short), (b"2_2".to_vec(), MacroBlock::empty(2, 2).encode())];
        let (ok, bad) = decode_all(&entries);
        assert_eq!((ok.len(), bad.len()), (1, 1));
    }

    #[test]
    #[should_panic(expected = "outside the block")]
    fn tile_out_of_range() {
        MacroBlock::empty(0, 0).tile(32, 0);
    }

    #[test]
    #[should_panic(expected = "outside the block")]
    fn tile_mut_out_of_range() {
        MacroBlock::empty(0, 0).tile_bytes_mut(0, 32);
    }
}
