//! What a tool puts into a tile ([`Brush`]), which tiles it may change, and tile matching for replace/search.

use bhw_content::tile::{TileData, AIR};
use serde::{Deserialize, Serialize};

/// Rock tile type.
pub const ROCK: u8 = 1;
/// Dirt tile type.
pub const DIRT: u8 = 6;
/// Limestone tile type.
pub const LIMESTONE: u8 = 12;
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
                set_material(&mut n, self.contents);
                // what is left to dig out: full for every solid tile, 0 for air (as the game stores it)
                n.0[4] = if self.tile == AIR { 0 } else { 255 };
                n.0[5] = 0; // gather progress
                n.0[0x16] = 0; // on fire
            }
            Layer::Back => n.set_bg(self.tile),
        }
        n
    }

    /// A brush is valid if its tile id is known and its contents id is placeable in that tile.
    pub fn is_valid(&self) -> bool {
        bhw_content::names::tile_name(self.tile).is_some()
            && (self.layer == Layer::Back || self.contents == 0 || host_tile(self.contents) == Some(self.tile))
    }
}

/// The tile type a placeable material lives in, as the game generates it (seen in real worlds and in the game):
/// flint and clay in dirt, oil in limestone, the metals and coal in rock, gems in the air of caves (stored as a
/// gem "promise" in the foreground contents byte, which the game turns into a glowing gem when a blockhead comes
/// near or light reaches it). `None` for every other id: charcoal and the like are never placed by the game and
/// do not show when placed in a tile.
pub fn host_tile(c: u8) -> Option<u8> {
    match c {
        1 | 2 => Some(DIRT),
        64 => Some(LIMESTONE),
        61..=63 | 65 | 77 | 106 | 107 => Some(ROCK),
        c if gem(c) => Some(AIR),
        _ => None,
    }
}

/// Contents ids that are plain materials (ores, gems, flint, clay, oil) and may be painted.
pub fn placeable_contents(c: u8) -> bool {
    host_tile(c).is_some()
}

/// True for the gem ids (ruby, emerald, sapphire, amethyst, diamond).
fn gem(c: u8) -> bool {
    matches!(c, 51 | 53 | 55 | 57 | 59)
}

/// The material a tile shows: its contents, or the gem in its foreground contents byte (a gem or a gem promise,
/// reported as the gem).
pub fn material(t: &TileData) -> u8 {
    match (t.contents(), t.fg_contents()) {
        (0, f @ GEM_FIRST..=GEM_LAST) => f - (f % 2 == 0) as u8,
        (c, _) => c,
    }
}

/// True if `t` holds a gem the game has already uncovered (it has a light object of its own in `dw`).
pub fn uncovered_gem(t: &TileData) -> bool {
    gem(t.fg_contents())
}

/// Puts material `c` (0 = none) into `n`: a gem as the promise in the foreground contents byte, everything else
/// in the contents byte. Any gem the tile held before is removed.
pub fn set_material(n: &mut TileData, c: u8) {
    if (GEM_FIRST..=GEM_LAST).contains(&n.fg_contents()) {
        n.set_fg_contents(0);
    }
    if gem(c) {
        n.set_contents(0);
        n.set_fg_contents(c + 1);
    } else {
        n.set_contents(c);
    }
}

/// First and last gem ids in the foreground contents byte (ruby 0x33 … diamond promise 0x3c).
const GEM_FIRST: u8 = 0x33;
const GEM_LAST: u8 = 0x3c;

/// Contents ids that stand for nothing but material or a generator promise, i.e. no dynamic object belongs to them.
pub fn plain_contents(c: u8) -> bool {
    placeable_contents(c) || matches!(c, 0 | 45 | 52 | 54 | 56 | 58 | 60 | 94 | 108 | 126..=145)
}

/// True if a tool must not change `t`: it is part of a dynamic object (owner set, or contents such as a tree,
/// plant, workbench, door, ladder, torch…). Changing it would leave the object behind in `dw`.
/// An uncovered gem counts as an object: its light lives in `dw`.
pub fn is_protected(t: &TileData) -> bool {
    t.owner() != 0 || !plain_contents(t.contents()) || uncovered_gem(t)
}

/// `t` without the parts that belong to dynamic objects: owner cleared, object contents removed, an uncovered gem
/// turned back into its promise (the game uncovers it again and makes a new light for it).
pub fn detached(t: &TileData) -> TileData {
    let mut n = *t;
    n.clear_owner();
    if !plain_contents(n.contents()) {
        n.set_contents(0);
    }
    if uncovered_gem(&n) {
        n.set_fg_contents(n.fg_contents() + 1);
    }
    n
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
            Layer::Front => t.fg() == self.tile && self.contents.is_none_or(|c| c == material(t)),
            Layer::Back => t.bg() == self.tile,
        }
    }
}

/// A token naming a block in history labels, translated by the UI: `t:<tile>` (front), `t:<tile>:<contents>`
/// (front with contents), `b:<tile>` (back wall).
pub fn describe(tile: u8, contents: u8, layer: Layer) -> String {
    match (layer, contents) {
        (Layer::Back, _) => format!("b:{tile}"),
        (Layer::Front, 0) => format!("t:{tile}"),
        (Layer::Front, c) => format!("t:{tile}:{c}"),
    }
}

/// The token of an item in history labels (`i:<id>`).
pub fn describe_item(item: u16) -> String {
    format!("i:{item}")
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
    fn materials_in_their_tiles() {
        let ok = |tile, contents| Brush { layer: Layer::Front, tile, contents }.is_valid();
        assert!(ok(6, 1) && ok(6, 2) && ok(12, 64) && ok(1, 106) && ok(1, 107) && ok(1, 77) && ok(2, 51));
        assert!(!ok(1, 1) && !ok(1, 2) && !ok(1, 64) && !ok(6, 61) && !ok(1, 51) && !ok(1, 45) && !ok(6, 45));
        assert_eq!(host_tile(59), Some(AIR));
        assert_eq!(host_tile(45), None);
        assert!(!placeable_contents(45) && plain_contents(45));
        // a gem goes into the foreground contents byte as a promise, the contents byte stays empty
        let rock = tile(1, 1, 61);
        let g = Brush { layer: Layer::Front, tile: AIR, contents: 51 }.apply(&rock);
        assert_eq!((g.fg(), g.contents(), g.fg_contents(), g.0[4]), (AIR, 0, 0x34, 0));
        assert_eq!(material(&g), 51);
        assert!(!uncovered_gem(&g) && !is_protected(&g));
        // painting over a gem removes it; other foreground contents stay
        let n = Brush::front(1).apply(&g);
        assert_eq!((n.fg_contents(), material(&n)), (0, 0));
        let mut plant = tile(2, 2, 0);
        plant.set_fg_contents(0x7f);
        assert_eq!(Brush::front(1).apply(&plant).fg_contents(), 0x7f);
        assert_eq!(material(&plant), 0);
        // an uncovered gem has a light object: protected, and detaching turns it back into its promise
        let mut lit = tile(2, 1, 0);
        lit.set_fg_contents(0x3b);
        assert!(uncovered_gem(&lit) && is_protected(&lit));
        assert_eq!(material(&lit), 59);
        let d = detached(&lit);
        assert_eq!((d.fg_contents(), is_protected(&d)), (0x3c, false));
        let mut owned = tile(2, 2, 4);
        owned.0[0x28] = 3;
        let d = detached(&owned);
        assert_eq!((d.owner(), d.contents()), (0, 0));
        assert_eq!(detached(&tile(1, 1, 61)).contents(), 61);
        assert!(Match { layer: Layer::Front, tile: AIR, contents: Some(51) }.matches(&g));
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
        assert_eq!(describe(1, 0, Layer::Front), "t:1");
        assert_eq!(describe(1, 61, Layer::Front), "t:1:61");
        assert_eq!(describe(250, 0, Layer::Back), "b:250");
        assert_eq!(describe_item(21), "i:21");
    }
}
