//! Names of the ids stored in saves: tile types, tile contents, dynamic object types and item types.
//!
//! Tile type names come from [`bhw_save::tiles`]; the other tables are generated from Blockheads Club's own
//! Blockheads Server (GPL) and kept in `names_gen.rs`.

use crate::names_gen::{CONTENTS, ITEM_IDS, ITEM_NAMES, OBJECT_TYPES};

/// Snake-case name of a tile type (`dirt_grass`), `None` if unknown.
pub fn tile_name(id: u8) -> Option<&'static str> {
    bhw_save::tiles::TileType(id).name()
}

/// Number of known tile types (ids `1..TILE_COUNT`).
pub const TILE_COUNT: u8 = bhw_save::tiles::TileType::COUNT;

/// Snake-case name of a contents id (`copper`, `apple_tree_trunk`), `None` if unknown; 0 is `nothing`.
pub fn contents_name(id: u8) -> Option<&'static str> {
    CONTENTS.get(id as usize).copied().flatten()
}

/// Number of known contents ids.
pub fn contents_count() -> usize {
    CONTENTS.len()
}

/// Snake-case name of a dynamic object type (`chest`, `npc_dodo`), `None` if unknown.
pub fn object_type_name(id: u32) -> Option<&'static str> {
    OBJECT_TYPES.get(id as usize).copied().flatten()
}

/// Snake-case identifier of an item type (`flint_axe`), `None` if unknown.
pub fn item_id_name(id: u16) -> Option<&'static str> {
    ITEM_IDS.get(id as usize).copied().flatten()
}

/// The game's English name of an item type (`FLINT AXE`), `None` for ids the game calls `UNKNOWN`.
pub fn item_name(id: u16) -> Option<&'static str> {
    ITEM_NAMES.get(id as usize).copied().filter(|n| *n != "UNKNOWN")
}

/// Number of item type ids (`0..item_count()`).
pub fn item_count() -> usize {
    ITEM_NAMES.len()
}

/// Number of sub-item slots of a container item (baskets, chests, clay jugs…), 0 for plain items.
pub fn sub_slots(item: u16) -> usize {
    match item_id_name(item) {
        Some("clothing_slot" | "basket" | "shelf" | "display_cabinet") => 4,
        Some("clay_jug") => 2,
        Some("chest" | "safe" | "chest_gold" | "chest_portal" | "chest_feeder") => 16,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookups() {
        assert_eq!(tile_name(6), Some("dirt"));
        assert_eq!(tile_name(0), None);
        assert_eq!(TILE_COUNT, 78);
        assert_eq!(contents_name(0), Some("nothing"));
        assert_eq!(contents_name(61), Some("copper"));
        assert_eq!(contents_name(250), None);
        assert_eq!(contents_count(), 149);
        assert_eq!(object_type_name(46), Some("chest"));
        assert_eq!(object_type_name(999), None);
        assert_eq!(item_id_name(6), Some("flint_axe"));
        assert_eq!(item_id_name(5000), None);
        assert_eq!(item_name(4), Some("STICK"));
        assert_eq!(item_name(0), None);
        assert_eq!(item_name(5000), None);
        assert_eq!(item_count(), 1106);
        assert_eq!(sub_slots(12), 4);
        assert_eq!(sub_slots(45), 2);
        assert_eq!(sub_slots(4), 0);
        let chest = (0..1106u16).find(|i| item_id_name(*i) == Some("chest")).unwrap();
        assert_eq!(sub_slots(chest), 16);
    }
}
