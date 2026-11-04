//! One 64-byte tile record of a macro block.
//!
//! Layout (little-endian; offsets in bytes):
//!
//! | offset | size | field |
//! | --- | --- | --- |
//! | 0x00 | 1 | foreground tile type |
//! | 0x01 | 1 | back wall tile type |
//! | 0x02 | 1 | zone (biome) |
//! | 0x03 | 1 | contents (ore, plant, ladder, object marker, …) |
//! | 0x04 | 1 | partial content left |
//! | 0x05 | 1 | gather progress |
//! | 0x06 | 1 | light |
//! | 0x07 | 1 | sun light |
//! | 0x08 | 1 | season offset |
//! | 0x09 | 1 | explored fraction |
//! | 0x0a | 1 | terrain slow factor |
//! | 0x0b | 1 | foreground contents |
//! | 0x0c | 1 | background contents |
//! | 0x0e | 3 × 2 | artificial light red, green, blue |
//! | 0x14 | 2 | artificial heat (signed) |
//! | 0x16 | 1 | on fire |
//! | 0x18 | 4 | owner object id (old, 32-bit) |
//! | 0x1c | 5 × 2 | paint: front, top, right, left, bottom |
//! | 0x28 | 8 | owner object id |
//! | 0x30 | 16 | reserved (zero in vanilla worlds) |
//!
//! Bytes this tool does not name are kept as they are.

/// Bytes per tile record.
pub const TILE_BYTES: usize = 64;

/// Foreground/back wall id of air.
pub const AIR: u8 = 2;

/// One tile record. Setters change only their field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TileData(pub [u8; TILE_BYTES]);

/// The five painted faces of a tile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Face {
    /// Front face.
    Front,
    /// Top face.
    Top,
    /// Right face.
    Right,
    /// Left face.
    Left,
    /// Bottom face.
    Bottom,
}

impl Face {
    /// All faces in storage order.
    pub const ALL: [Face; 5] = [Face::Front, Face::Top, Face::Right, Face::Left, Face::Bottom];

    fn offset(self) -> usize {
        0x1c + 2 * Face::ALL.iter().position(|f| *f == self).expect("listed")
    }
}

impl Default for TileData {
    fn default() -> TileData {
        TileData([0; TILE_BYTES])
    }
}

impl TileData {
    /// A tile from the first 64 bytes of `bytes`. Panics if `bytes` is shorter.
    pub fn from_slice(bytes: &[u8]) -> TileData {
        TileData(bytes[..TILE_BYTES].try_into().expect("64 bytes"))
    }

    fn u16_at(&self, o: usize) -> u16 {
        u16::from_le_bytes([self.0[o], self.0[o + 1]])
    }

    fn set_u16_at(&mut self, o: usize, v: u16) {
        self.0[o..o + 2].copy_from_slice(&v.to_le_bytes());
    }

    /// Foreground tile type.
    pub fn fg(&self) -> u8 {
        self.0[0]
    }
    /// Back wall tile type.
    pub fn bg(&self) -> u8 {
        self.0[1]
    }
    /// Zone (biome) byte.
    pub fn zone(&self) -> u8 {
        self.0[2]
    }
    /// Contents id.
    pub fn contents(&self) -> u8 {
        self.0[3]
    }
    /// Light level.
    pub fn light(&self) -> u8 {
        self.0[6]
    }
    /// Sun light level.
    pub fn sun_light(&self) -> u8 {
        self.0[7]
    }
    /// Foreground contents byte.
    pub fn fg_contents(&self) -> u8 {
        self.0[0xb]
    }
    /// Background contents byte.
    pub fn bg_contents(&self) -> u8 {
        self.0[0xc]
    }
    /// Artificial light (red, green, blue).
    pub fn artificial_light(&self) -> [u16; 3] {
        [self.u16_at(0xe), self.u16_at(0x10), self.u16_at(0x12)]
    }
    /// Artificial heat.
    pub fn heat(&self) -> i16 {
        self.u16_at(0x14) as i16
    }
    /// On-fire byte.
    pub fn on_fire(&self) -> u8 {
        self.0[0x16]
    }
    /// Paint of one face (0 = not painted).
    pub fn paint(&self, face: Face) -> u16 {
        self.u16_at(face.offset())
    }
    /// Id of the dynamic object that owns the tile (0 = none); falls back to the old 32-bit field.
    pub fn owner(&self) -> u64 {
        let new = u64::from_le_bytes(self.0[0x28..0x30].try_into().expect("8 bytes"));
        if new != 0 {
            new
        } else {
            u32::from_le_bytes(self.0[0x18..0x1c].try_into().expect("4 bytes")) as u64
        }
    }

    /// Sets the foreground type.
    pub fn set_fg(&mut self, v: u8) {
        self.0[0] = v;
    }
    /// Sets the back wall type.
    pub fn set_bg(&mut self, v: u8) {
        self.0[1] = v;
    }
    /// Sets the contents id.
    pub fn set_contents(&mut self, v: u8) {
        self.0[3] = v;
    }
    /// Sets the foreground contents byte.
    pub fn set_fg_contents(&mut self, v: u8) {
        self.0[0xb] = v;
    }
    /// Sets the paint of one face.
    pub fn set_paint(&mut self, face: Face, v: u16) {
        self.set_u16_at(face.offset(), v);
    }
    /// Clears both owner fields.
    pub fn clear_owner(&mut self) {
        self.0[0x18..0x1c].fill(0);
        self.0[0x28..0x30].fill(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fields() {
        let mut raw = [0u8; 64];
        for (i, b) in raw.iter_mut().enumerate() {
            *b = i as u8;
        }
        let mut t = TileData::from_slice(&raw);
        assert_eq!((t.fg(), t.bg(), t.zone(), t.contents()), (0, 1, 2, 3));
        assert_eq!((t.light(), t.sun_light(), t.fg_contents(), t.bg_contents()), (6, 7, 0xb, 0xc));
        assert_eq!(t.artificial_light(), [0x0f0e, 0x1110, 0x1312]);
        assert_eq!(t.heat(), 0x1514);
        assert_eq!(t.on_fire(), 0x16);
        assert_eq!(t.paint(Face::Front), 0x1d1c);
        assert_eq!(t.paint(Face::Bottom), 0x2524);
        assert_eq!(t.owner(), u64::from_le_bytes([0x28, 0x29, 0x2a, 0x2b, 0x2c, 0x2d, 0x2e, 0x2f]));
        t.0[0x28..0x30].fill(0);
        assert_eq!(t.owner(), 0x1b1a1918);
        t.set_fg(9);
        t.set_bg(10);
        t.set_contents(11);
        t.set_fg_contents(12);
        assert_eq!(t.fg_contents(), 12);
        t.set_paint(Face::Top, 0xabcd);
        t.clear_owner();
        assert_eq!((t.fg(), t.bg(), t.contents(), t.paint(Face::Top), t.owner()), (9, 10, 11, 0xabcd, 0));
        assert_eq!(t.0[0x30], 0x30, "unnamed bytes are kept");
        assert_eq!(TileData::default().0, [0; 64]);
        let mut h = TileData::default();
        h.0[0x14..0x16].copy_from_slice(&(-3i16).to_le_bytes());
        assert_eq!(h.heat(), -3);
    }
}
