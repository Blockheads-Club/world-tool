//! What a tool puts into a tile ([`Brush`]), which tiles it may change, and tile matching for replace/search.

use bhw_content::names::contents_name;
use bhw_content::tile::{TileData, AIR};
use serde::{Deserialize, Serialize};

/// Water tile type.
pub const WATER: u8 = 3;
/// Lava tile type.
pub const LAVA: u8 = 31;

/// The layer a brush paints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Layer {
    /// The foreground tile (and its contents).
    Front,
    /// The back wall.
    Back,
}

/// A block to place.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Brush {
    /// Layer to paint.
    pub layer: Layer,
    /// Tile type.
    pub tile: u8,
    /// Contents placed with a front tile (ore in rock…); ignored for the back layer.
    #[serde(default)]
    pub contents: u8,
}

impl Brush {
    /// A front tile without contents.
    pub fn front(tile: u8) -> Brush {
        Brush { layer: Layer::Front, tile, contents: 0 }
    }

    /// The brush that removes the foreground (air, no contents).
    pub fn eraser() -> Brush {
        Brush::front(AIR)
    }

    /// The tile after painting `t`.
    pub fn apply(&self, t: &TileData) -> TileData {
        let mut n = *t;
        match self.layer {
            Layer::Front => {
                n.set_fg(self.tile);
                n.set_contents(self.contents);
                // what is left to dig out: full for every solid tile, 0 for air (as the game stores it)
                n.0[4] = if self.tile == AIR { 0 } else { 255 };
                n.0[5] = 0; // gather progress
                n.0[0x16] = 0; // on fire
            }
            Layer::Back => n.set_bg(self.tile),
        }
        n
    }

    /// A brush is valid if its tile id is known and its contents id is placeable.
    pub fn is_valid(&self) -> bool {
        bhw_content::names::tile_name(self.tile).is_some()
            && (self.layer == Layer::Back || self.contents == 0 || placeable_contents(self.contents))
    }
}

/// Contents ids that are plain materials (ores, gems, flint, clay, charcoal) and may be painted.
pub fn placeable_contents(c: u8) -> bool {
    matches!(c, 1 | 2 | 45 | 51 | 53 | 55 | 57 | 59 | 61..=65 | 77 | 106 | 107)
}

/// Contents ids that stand for nothing but material or a generator promise, i.e. no dynamic object belongs to them.
pub fn plain_contents(c: u8) -> bool {
    placeable_contents(c) || matches!(c, 0 | 52 | 54 | 56 | 58 | 60 | 94 | 108 | 126..=145)
}

/// True if a tool must not change `t`: it is part of a dynamic object (owner set, or contents such as a tree,
/// plant, workbench, door, ladder, torch…). Changing it would leave the object behind in `dw`.
pub fn is_protected(t: &TileData) -> bool {
    t.owner() != 0 || !plain_contents(t.contents())
}

/// True for tiles that are open (air, water, lava) rather than solid.
pub fn is_open(t: &TileData) -> bool {
    matches!(t.fg(), AIR | WATER | LAVA)
}

/// Which tiles a replace or search matches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Match {
    /// Layer compared.
    pub layer: Layer,
    /// Tile type.
    pub tile: u8,
    /// For the front layer: contents that must match too (`None` = any contents).
    #[serde(default)]
    pub contents: Option<u8>,
}

impl Match {
    /// True if `t` matches.
    pub fn matches(&self, t: &TileData) -> bool {
        match self.layer {
            Layer::Front => t.fg() == self.tile && self.contents.is_none_or(|c| c == t.contents()),
            Layer::Back => t.bg() == self.tile,
        }
    }
}

/// Short English description of a brush for history labels (`rock`, `rock + copper`, `back wall rock`).
pub fn describe(tile: u8, contents: u8, layer: Layer) -> String {
    let name = bhw_content::names::tile_name(tile).unwrap_or("unknown").to_string();
    match (layer, contents) {
        (Layer::Back, _) => format!("back wall {name}"),
        (Layer::Front, 0) => name,
        (Layer::Front, c) => format!("{name} + {}", contents_name(c).unwrap_or("unknown")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tile(fg: u8, bg: u8, contents: u8) -> TileData {
        let mut t = TileData::default();
        t.set_fg(fg);
        t.set_bg(bg);
        t.set_contents(contents);
        t.0[4] = 7;
        t
    }

    #[test]
    fn brushes() {
        let t = tile(1, 1, 61);
        let n = Brush::front(6).apply(&t);
        assert_eq!((n.fg(), n.bg(), n.contents(), n.0[4]), (6, 1, 0, 255));
        assert_eq!(Brush::eraser().apply(&t).0[4], 0);
        let n = Brush { layer: Layer::Back, tile: 9, contents: 5 }.apply(&t);
        assert_eq!((n.fg(), n.bg(), n.contents(), n.0[4]), (1, 9, 61, 7));
        assert_eq!(Brush::eraser().apply(&t).fg(), AIR);
        assert!(Brush::front(6).is_valid());
        assert!(!Brush::front(0).is_valid());
        assert!(Brush { layer: Layer::Front, tile: 1, contents: 61 }.is_valid());
        assert!(!Brush { layer: Layer::Front, tile: 1, contents: 46 }.is_valid());
        assert!(Brush { layer: Layer::Back, tile: 1, contents: 46 }.is_valid());
    }

    #[test]
    fn protection() {
        assert!(!is_protected(&tile(1, 1, 61)));
        assert!(!is_protected(&tile(1, 1, 52)));
        assert!(is_protected(&tile(2, 2, 4)), "tree trunk");
        let mut owned = tile(1, 1, 0);
        owned.0[0x28] = 1;
        assert!(is_protected(&owned));
        assert!(is_open(&tile(2, 1, 0)) && is_open(&tile(3, 1, 0)) && is_open(&tile(31, 1, 0)));
        assert!(!is_open(&tile(1, 1, 0)));
    }

    #[test]
    fn matching() {
        let t = tile(1, 2, 61);
        assert!(Match { layer: Layer::Front, tile: 1, contents: None }.matches(&t));
        assert!(Match { layer: Layer::Front, tile: 1, contents: Some(61) }.matches(&t));
        assert!(!Match { layer: Layer::Front, tile: 1, contents: Some(0) }.matches(&t));
        assert!(!Match { layer: Layer::Front, tile: 6, contents: None }.matches(&t));
        assert!(Match { layer: Layer::Back, tile: 2, contents: None }.matches(&t));
        assert_eq!(describe(1, 0, Layer::Front), "rock");
        assert_eq!(describe(1, 61, Layer::Front), "rock + copper");
        assert_eq!(describe(1, 200, Layer::Front), "rock + unknown");
        assert_eq!(describe(250, 0, Layer::Back), "back wall unknown");
    }
}
