//! Where the game's own atlases show each id, and this tool's colour fallback.
//!
//! The table (`data/atlas.json`) is BH World Tool's own data, made by observation (see its `about` text and
//! `docs/editor-atlas.md`); it is not taken from the game's code. Ids without an entry are drawn with plain colours.
//! The atlases themselves come only from the user's APK (`HDTex/TileMap.png`, `HDTex/Items.png`).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// The built-in table.
pub const BUILTIN: &str = include_str!("../data/atlas.json");

/// Columns × rows of `TileMap.png` cells.
pub const TILEMAP_GRID: (u8, u8) = (32, 32);
/// Columns × rows of `Items.png` cells.
pub const ITEMS_GRID: (u8, u8) = (32, 16);

/// One table entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    /// Column and row of the cell.
    pub cell: [u8; 2],
    /// How it was found (`eye`, `game`).
    pub src: String,
}

/// The table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Atlas {
    /// Table format version.
    pub version: u32,
    /// Tile type id → `TileMap.png` cell.
    pub tiles: BTreeMap<u16, Entry>,
    /// Contents id → `TileMap.png` cell (drawn over the tile).
    pub contents: BTreeMap<u16, Entry>,
    /// Item type id → `Items.png` cell.
    pub items: BTreeMap<u16, Entry>,
}

impl Atlas {
    /// Parses and checks a table: every cell must lie inside its atlas grid.
    pub fn parse(json: &str) -> Result<Atlas, String> {
        let a: Atlas = serde_json::from_str(json).map_err(|e| e.to_string())?;
        let check = |m: &BTreeMap<u16, Entry>, grid: (u8, u8), what: &str| {
            m.iter().try_for_each(|(id, e)| {
                if e.cell[0] < grid.0 && e.cell[1] < grid.1 {
                    Ok(())
                } else {
                    Err(format!("{what} {id}: cell {:?} outside the atlas", e.cell))
                }
            })
        };
        check(&a.tiles, TILEMAP_GRID, "tile")?;
        check(&a.contents, TILEMAP_GRID, "contents")?;
        check(&a.items, ITEMS_GRID, "item")?;
        Ok(a)
    }

    /// The built-in table.
    pub fn builtin() -> Atlas {
        Atlas::parse(BUILTIN).expect("the built-in table is valid")
    }
}

/// Plain colour of a contents id (ores and gems get their own colours, everything else none).
pub fn contents_color(id: u8) -> Option<[u8; 3]> {
    Some(match id {
        1 => [60, 60, 70],      // flint
        2 => [190, 120, 90],    // clay
        45 => [40, 40, 40],     // charcoal
        51 => [220, 40, 70],    // ruby
        53 => [40, 200, 100],   // emerald
        55 => [50, 90, 230],    // sapphire
        57 => [160, 90, 220],   // amethyst
        59 => [190, 245, 255],  // diamond
        61 => [210, 120, 60],   // copper
        62 => [200, 210, 220],  // tin
        63 => [170, 100, 80],   // iron
        64 => [20, 20, 20],     // oil
        65 => [30, 30, 30],     // coal
        77 => [240, 200, 40],   // gold
        106 => [225, 230, 235], // platinum
        107 => [150, 160, 175], // titanium
        _ => return None,
    })
}

/// Everything the map renderer and the pickers need about ids, sent to the UI once.
#[derive(Debug, Clone, Serialize)]
pub struct Catalog {
    /// Tile types 0..78: name, plain colour, atlas cell.
    pub tiles: Vec<IdInfo>,
    /// Contents 0..149.
    pub contents: Vec<IdInfo>,
    /// Item types with a name.
    pub items: Vec<IdInfo>,
}

/// One id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct IdInfo {
    /// The id.
    pub id: u16,
    /// Snake-case name (tiles, contents) or the game's English name (items).
    pub name: String,
    /// Plain colour, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<[u8; 3]>,
    /// Atlas cell, if known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cell: Option<[u8; 2]>,
    /// For contents: may be painted with a brush.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub placeable: bool,
}

impl Catalog {
    /// Builds the catalog from a table.
    pub fn new(atlas: &Atlas) -> Catalog {
        use bhw_content::names;
        let cell = |m: &BTreeMap<u16, Entry>, id: u16| m.get(&id).map(|e| e.cell);
        let tiles = (0..names::TILE_COUNT)
            .map(|i| IdInfo {
                id: i as u16,
                name: names::tile_name(i).unwrap_or("none").into(),
                color: bhw_save::tiles::TileType(i).color(),
                cell: cell(&atlas.tiles, i as u16),
                placeable: false,
            })
            .collect();
        let contents = (0..names::contents_count() as u16)
            .map(|i| IdInfo {
                id: i,
                name: names::contents_name(i as u8).unwrap_or("unknown").into(),
                color: contents_color(i as u8),
                cell: cell(&atlas.contents, i),
                placeable: crate::brush::placeable_contents(i as u8),
            })
            .collect();
        let items = (0..names::item_count() as u16)
            .filter_map(|i| {
                Some(IdInfo {
                    id: i,
                    name: names::item_name(i)?.into(),
                    color: None,
                    cell: cell(&atlas.items, i),
                    placeable: false,
                })
            })
            .collect();
        Catalog { tiles, contents, items }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_table() {
        let a = Atlas::builtin();
        assert_eq!(a.version, 1);
        assert!(a.tiles.contains_key(&1));
        assert!(a.tiles.keys().all(|k| *k > 0 && *k < 78), "tile ids exist");
        assert!(a.contents.keys().all(|k| bhw_content::names::contents_name(*k as u8).is_some()));
        assert!(a.items.keys().all(|k| bhw_content::names::item_name(*k).is_some()));
        assert!(a
            .tiles
            .values()
            .chain(a.contents.values())
            .chain(a.items.values())
            .all(|e| e.src == "eye" || e.src == "game"));
        assert!(Atlas::parse("{").is_err());
        let bad = |section: &str, cell: &str| {
            let one = format!(r#"{{"1":{{"cell":{cell},"src":"eye"}}}}"#);
            let part = |name: &str| if name == section { one.clone() } else { "{}".to_string() };
            format!(
                r#"{{"version":1,"tiles":{},"contents":{},"items":{}}}"#,
                part("tiles"),
                part("contents"),
                part("items")
            )
        };
        assert!(Atlas::parse(&bad("tiles", "[32,0]")).unwrap_err().contains("tile 1"));
        assert!(Atlas::parse(&bad("contents", "[0,32]")).unwrap_err().contains("contents 1"));
        assert!(Atlas::parse(&bad("items", "[0,16]")).unwrap_err().contains("item 1"));
        assert!(Atlas::parse(&bad("items", "[31,15]")).is_ok());
    }

    #[test]
    fn catalog() {
        let c = Catalog::new(&Atlas::builtin());
        assert_eq!(c.tiles.len(), 78);
        assert_eq!(c.tiles[0].name, "none");
        assert_eq!(c.tiles[1].cell, Some([0, 1]));
        assert_eq!(c.contents.len(), 149);
        assert!(c.contents[61].placeable && c.contents[61].color.is_some());
        assert!(!c.contents[4].placeable && c.contents[4].color.is_none());
        assert!(c.items.iter().all(|i| i.name != "UNKNOWN"));
        assert!(c.items.len() > 300, "{}", c.items.len());
        let json = serde_json::to_string(&c).unwrap();
        assert!(json.contains("\"placeable\":true"));
        for id in [1, 2, 45, 51, 53, 55, 57, 59, 61, 62, 63, 64, 65, 77, 106, 107] {
            assert!(contents_color(id).is_some(), "{id}");
        }
    }
}
