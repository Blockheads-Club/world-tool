//! Schematics: a rectangle of tiles copied out of a world, kept in memory (clipboard) or saved as a `.bhwschem`
//! file. The file format is documented in `docs/schematic-format.md`.

use std::io::{Read, Write};

use bhw_content::tile::{Face, TileData};
use serde::{Deserialize, Serialize};

use crate::brush::{material, plain_contents, set_material};
use crate::error::{refused, Refusal, Result};

/// File magic.
pub const MAGIC: &[u8; 8] = b"BHWSCHEM";
/// Current format version.
pub const VERSION: u32 = 1;
/// Bytes per cell in the file.
pub const CELL_BYTES: usize = 16;
/// Largest side accepted from a file.
pub const MAX_SIDE: u32 = 4096;
/// Largest header accepted from a file.
pub const MAX_HEADER: u32 = 64 * 1024;

/// One tile of a schematic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Cell {
    /// False for tiles that were outside the stored world when copied; pasting leaves those alone.
    pub present: bool,
    /// Foreground tile type.
    pub fg: u8,
    /// Back wall type.
    pub bg: u8,
    /// Contents (only plain materials; object contents are dropped when copying).
    pub contents: u8,
    /// Paint of the five faces.
    pub paint: [u16; 5],
}

impl Cell {
    /// The cell for a world tile.
    pub fn from_tile(t: &TileData) -> Cell {
        let contents = if plain_contents(t.contents()) && t.owner() == 0 { material(t) } else { 0 };
        Cell { present: true, fg: t.fg(), bg: t.bg(), contents, paint: Face::ALL.map(|f| t.paint(f)) }
    }

    /// `t` with this cell's values (light and other fields are kept; the game recomputes them).
    pub fn apply(&self, t: &TileData) -> TileData {
        let mut n = *t;
        n.set_fg(self.fg);
        n.set_bg(self.bg);
        set_material(&mut n, self.contents);
        for (f, p) in Face::ALL.iter().zip(self.paint) {
            n.set_paint(*f, p);
        }
        n.0[4] = if self.fg == bhw_content::tile::AIR { 0 } else { 255 };
        n.0[5] = 0;
        n.0[0x16] = 0;
        n
    }
}

/// Descriptive header of a schematic file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Meta {
    /// Name given by the user.
    pub name: String,
    /// Name of the world it was copied from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_world: Option<String>,
    /// Unix seconds when it was made.
    #[serde(default)]
    pub created: u64,
}

#[derive(Serialize, Deserialize)]
struct Header {
    format: String,
    version: u32,
    width: u32,
    height: u32,
    game: String,
    #[serde(flatten)]
    meta: Meta,
}

/// A rectangle of tiles, rows from the bottom.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Schematic {
    /// Width in tiles.
    pub width: u32,
    /// Height in tiles.
    pub height: u32,
    /// `width * height` cells, row by row from the bottom.
    pub cells: Vec<Cell>,
    /// Name and origin.
    pub meta: Meta,
}

impl Schematic {
    /// An empty (all absent) schematic.
    pub fn new(width: u32, height: u32) -> Schematic {
        Schematic { width, height, cells: vec![Cell::default(); (width * height) as usize], meta: Meta::default() }
    }

    /// The cell at `(x, y)` (from the bottom-left).
    pub fn cell(&self, x: u32, y: u32) -> &Cell {
        &self.cells[(y * self.width + x) as usize]
    }

    /// Mutable cell at `(x, y)`.
    pub fn cell_mut(&mut self, x: u32, y: u32) -> &mut Cell {
        &mut self.cells[(y * self.width + x) as usize]
    }

    /// Number of present cells.
    pub fn present(&self) -> u64 {
        self.cells.iter().filter(|c| c.present).count() as u64
    }

    /// Rotated a quarter turn clockwise.
    pub fn rotated(&self) -> Schematic {
        let mut out = Schematic::new(self.height, self.width);
        out.meta = self.meta.clone();
        for y in 0..self.height {
            for x in 0..self.width {
                // (x, y) from the bottom-left → clockwise: new x = y, new y = width - 1 - x
                *out.cell_mut(y, self.width - 1 - x) = *self.cell(x, y);
            }
        }
        out
    }

    /// Mirrored left to right.
    pub fn flipped(&self) -> Schematic {
        let mut out = self.clone();
        for y in 0..self.height {
            for x in 0..self.width {
                *out.cell_mut(self.width - 1 - x, y) = *self.cell(x, y);
            }
        }
        out
    }

    /// Encodes as a `.bhwschem` file.
    pub fn to_bytes(&self) -> Vec<u8> {
        let header = Header {
            format: "bhw-schematic".into(),
            version: VERSION,
            width: self.width,
            height: self.height,
            game: "1.7".into(),
            meta: self.meta.clone(),
        };
        let json = serde_json::to_vec(&header).expect("serializable");
        let mut cells = Vec::with_capacity(self.cells.len() * CELL_BYTES);
        for c in &self.cells {
            cells.extend_from_slice(&[c.present as u8, c.fg, c.bg, c.contents]);
            for p in c.paint {
                cells.extend_from_slice(&p.to_le_bytes());
            }
            cells.extend_from_slice(&[0, 0]);
        }
        let mut gz = flate2::GzBuilder::new().write(Vec::new(), flate2::Compression::default());
        gz.write_all(&cells).expect("writing to a Vec cannot fail");
        let body = gz.finish().expect("writing to a Vec cannot fail");
        let mut out = Vec::with_capacity(16 + json.len() + body.len());
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&(json.len() as u32).to_le_bytes());
        out.extend_from_slice(&json);
        out.extend_from_slice(&body);
        out
    }

    /// Decodes a `.bhwschem` file.
    pub fn from_bytes(bytes: &[u8]) -> Result<Schematic> {
        let bad = |m: &str| refused(Refusal::BadSchematic, format!("schematic: {m}"));
        if bytes.len() < 12 || &bytes[..8] != MAGIC {
            return Err(bad("not a schematic file"));
        }
        let hlen = u32::from_le_bytes(bytes[8..12].try_into().expect("4 bytes"));
        if hlen > MAX_HEADER || bytes.len() < 12 + hlen as usize {
            return Err(bad("damaged header"));
        }
        let h: Header = serde_json::from_slice(&bytes[12..12 + hlen as usize]).map_err(|e| bad(&e.to_string()))?;
        if h.format != "bhw-schematic" {
            return Err(bad("not a schematic file"));
        }
        if h.version > VERSION {
            return Err(refused(
                Refusal::NewerSchematic,
                format!("schematic: made by a newer version (format {})", h.version),
            ));
        }
        if h.width == 0 || h.height == 0 || h.width > MAX_SIDE || h.height > MAX_SIDE {
            return Err(bad("bad size"));
        }
        let want = (h.width * h.height) as usize * CELL_BYTES;
        let mut cells = Vec::with_capacity(want);
        flate2::read::GzDecoder::new(&bytes[12 + hlen as usize..])
            .take(want as u64 + 1)
            .read_to_end(&mut cells)
            .map_err(|e| bad(&e.to_string()))?;
        if cells.len() != want {
            return Err(bad("damaged tile data"));
        }
        let cells = cells
            .chunks_exact(CELL_BYTES)
            .map(|c| Cell {
                present: c[0] != 0,
                fg: c[1],
                bg: c[2],
                contents: c[3],
                paint: std::array::from_fn(|i| u16::from_le_bytes([c[4 + 2 * i], c[5 + 2 * i]])),
            })
            .collect();
        Ok(Schematic { width: h.width, height: h.height, cells, meta: h.meta })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Schematic {
        let mut s = Schematic::new(3, 2);
        for (i, c) in s.cells.iter_mut().enumerate() {
            *c = Cell { present: i != 5, fg: i as u8 + 1, bg: 2, contents: 0, paint: [i as u16, 0, 0, 0, 9] };
        }
        s.meta = Meta { name: "House".into(), source_world: Some("SUNNY".into()), created: 5 };
        s
    }

    #[test]
    fn cells() {
        let mut t = TileData::default();
        t.set_fg(1);
        t.set_bg(1);
        t.set_contents(61);
        t.set_paint(Face::Left, 3);
        let c = Cell::from_tile(&t);
        assert_eq!((c.fg, c.bg, c.contents, c.paint[3]), (1, 1, 61, 3));
        t.set_contents(46);
        assert_eq!(Cell::from_tile(&t).contents, 0, "object contents are not copied");
        t.set_contents(61);
        t.0[0x28] = 1;
        assert_eq!(Cell::from_tile(&t).contents, 0, "owned tiles lose their contents");
        let mut d = TileData::default();
        d.0[4] = 5;
        let n = c.apply(&d);
        assert_eq!((n.fg(), n.bg(), n.contents(), n.paint(Face::Left), n.0[4]), (1, 1, 61, 3, 255));
        assert_eq!(Cell { fg: 2, ..c }.apply(&d).0[4], 0);
    }

    #[test]
    fn transforms() {
        let s = sample();
        assert_eq!(s.present(), 5);
        let r = s.rotated();
        assert_eq!((r.width, r.height), (2, 3));
        // bottom-left (0,0) of the original goes to the top-left (0, 2)
        assert_eq!(r.cell(0, 2), s.cell(0, 0));
        assert_eq!(r.cell(1, 0), s.cell(2, 1));
        assert_eq!(r.rotated().rotated().rotated(), s);
        let f = s.flipped();
        assert_eq!(f.cell(0, 0), s.cell(2, 0));
        assert_eq!(f.flipped(), s);
    }

    #[test]
    fn files() {
        let s = sample();
        let b = s.to_bytes();
        assert_eq!(&b[..8], MAGIC);
        assert_eq!(Schematic::from_bytes(&b).unwrap(), s);
        let err = |b: &[u8]| Schematic::from_bytes(b).unwrap_err().to_string();
        assert!(err(b"short").contains("not a schematic"));
        assert!(err(b"XXXXXXXX\0\0\0\0").contains("not a schematic"));
        let mut big = b.clone();
        big[8..12].copy_from_slice(&(MAX_HEADER + 1).to_le_bytes());
        assert!(err(&big).contains("damaged header"));
        let with = |json: &str, body: &[u8]| {
            let mut v = MAGIC.to_vec();
            v.extend_from_slice(&(json.len() as u32).to_le_bytes());
            v.extend_from_slice(json.as_bytes());
            v.extend_from_slice(body);
            v
        };
        assert!(err(&with("{", &[])).starts_with("schematic"));
        let h = |fmt: &str, ver: u32, w: u32, hh: u32| {
            format!(r#"{{"format":"{fmt}","version":{ver},"width":{w},"height":{hh},"game":"1.7","name":"x"}}"#)
        };
        assert!(err(&with(&h("other", 1, 1, 1), &[])).contains("not a schematic"));
        assert!(err(&with(&h("bhw-schematic", 2, 1, 1), &[])).contains("newer version"));
        let why = |b: &[u8]| Schematic::from_bytes(b).unwrap_err().refusal();
        assert_eq!(why(&with(&h("bhw-schematic", 2, 1, 1), &[])), Some(Refusal::NewerSchematic));
        assert_eq!(why(b"short"), Some(Refusal::BadSchematic));
        assert!(err(&with(&h("bhw-schematic", 1, 0, 1), &[])).contains("bad size"));
        assert!(err(&with(&h("bhw-schematic", 1, 1, MAX_SIDE + 1), &[])).contains("bad size"));
        assert!(err(&with(&h("bhw-schematic", 1, 1, 1), b"junk")).starts_with("schematic"));
        let mut gz = flate2::GzBuilder::new().write(Vec::new(), flate2::Compression::default());
        gz.write_all(&[0; 5]).unwrap();
        assert!(err(&with(&h("bhw-schematic", 1, 1, 1), &gz.finish().unwrap())).contains("damaged tile data"));
    }
}
