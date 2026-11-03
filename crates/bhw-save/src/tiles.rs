//! Tile type ids (the `typeIndex` / `backWallTypeIndex` bytes of a tile) and the preview colours used for them.
//!
//! The colours are this tool's own choice; no game graphics are involved.

/// Tile type ids as stored in saves (game version 1.7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TileType(pub u8);

impl TileType {
    #[allow(missing_docs)]
    pub const ROCK: TileType = TileType(1);
    #[allow(missing_docs)]
    pub const AIR: TileType = TileType(2);
    #[allow(missing_docs)]
    pub const WATER: TileType = TileType(3);
    #[allow(missing_docs)]
    pub const ICE: TileType = TileType(4);
    #[allow(missing_docs)]
    pub const SNOW: TileType = TileType(5);
    #[allow(missing_docs)]
    pub const DIRT: TileType = TileType(6);
    #[allow(missing_docs)]
    pub const SAND: TileType = TileType(7);
    #[allow(missing_docs)]
    pub const SAND_BEACH: TileType = TileType(8);
    #[allow(missing_docs)]
    pub const WOOD: TileType = TileType(9);
    #[allow(missing_docs)]
    pub const COBBLESTONE: TileType = TileType(10);
    #[allow(missing_docs)]
    pub const RED_BRICKS: TileType = TileType(11);
    #[allow(missing_docs)]
    pub const LIMESTONE: TileType = TileType(12);
    #[allow(missing_docs)]
    pub const LIMESTONE_BLOCK: TileType = TileType(13);
    #[allow(missing_docs)]
    pub const MARBLE: TileType = TileType(14);
    #[allow(missing_docs)]
    pub const MARBLE_BLOCK: TileType = TileType(15);
    #[allow(missing_docs)]
    pub const TIME_CRYSTAL: TileType = TileType(16);
    #[allow(missing_docs)]
    pub const SANDSTONE: TileType = TileType(17);
    #[allow(missing_docs)]
    pub const SANDSTONE_BLOCK: TileType = TileType(18);
    #[allow(missing_docs)]
    pub const RED_MARBLE: TileType = TileType(19);
    #[allow(missing_docs)]
    pub const RED_MARBLE_BLOCK: TileType = TileType(20);
    #[allow(missing_docs)]
    pub const FLAX_MAT: TileType = TileType(21);
    #[allow(missing_docs)]
    pub const FLAX_MAT_YELLOW: TileType = TileType(22);
    #[allow(missing_docs)]
    pub const FLAX_MAT_RED: TileType = TileType(23);
    #[allow(missing_docs)]
    pub const GLASS: TileType = TileType(24);
    #[allow(missing_docs)]
    pub const INITIAL_PORTAL_BASE_STONE: TileType = TileType(25);
    #[allow(missing_docs)]
    pub const GOLD_BLOCK: TileType = TileType(26);
    #[allow(missing_docs)]
    pub const DIRT_GRASS: TileType = TileType(27);
    #[allow(missing_docs)]
    pub const DIRT_GRASS_FROZEN: TileType = TileType(28);
    #[allow(missing_docs)]
    pub const LAPIS: TileType = TileType(29);
    #[allow(missing_docs)]
    pub const LAPIS_BLOCK: TileType = TileType(30);
    #[allow(missing_docs)]
    pub const LAVA: TileType = TileType(31);
    #[allow(missing_docs)]
    pub const WOODEN_PLATFORM: TileType = TileType(32);
    #[allow(missing_docs)]
    pub const INITIAL_PORTAL_BASE_STONE_AMETHYST: TileType = TileType(33);
    #[allow(missing_docs)]
    pub const INITIAL_PORTAL_BASE_STONE_SAPPHIRE: TileType = TileType(34);
    #[allow(missing_docs)]
    pub const INITIAL_PORTAL_BASE_STONE_EMERALD: TileType = TileType(35);
    #[allow(missing_docs)]
    pub const INITIAL_PORTAL_BASE_STONE_RUBY: TileType = TileType(36);
    #[allow(missing_docs)]
    pub const INITIAL_PORTAL_BASE_STONE_DIAMOND: TileType = TileType(37);
    #[allow(missing_docs)]
    pub const NORTH_POLE: TileType = TileType(38);
    #[allow(missing_docs)]
    pub const SOUTH_POLE: TileType = TileType(39);
    #[allow(missing_docs)]
    pub const WEST_POLE: TileType = TileType(40);
    #[allow(missing_docs)]
    pub const EAST_POLE: TileType = TileType(41);
    #[allow(missing_docs)]
    pub const PLACED_PORTAL_BASE_STONE: TileType = TileType(42);
    #[allow(missing_docs)]
    pub const PLACED_PORTAL_BASE_STONE_AMETHYST: TileType = TileType(43);
    #[allow(missing_docs)]
    pub const PLACED_PORTAL_BASE_STONE_SAPPHIRE: TileType = TileType(44);
    #[allow(missing_docs)]
    pub const PLACED_PORTAL_BASE_STONE_EMERALD: TileType = TileType(45);
    #[allow(missing_docs)]
    pub const PLACED_PORTAL_BASE_STONE_RUBY: TileType = TileType(46);
    #[allow(missing_docs)]
    pub const PLACED_PORTAL_BASE_STONE_DIAMOND: TileType = TileType(47);
    #[allow(missing_docs)]
    pub const COMPOST: TileType = TileType(48);
    #[allow(missing_docs)]
    pub const COMPOST_GRASS: TileType = TileType(49);
    #[allow(missing_docs)]
    pub const COMPOST_GRASS_FROZEN: TileType = TileType(50);
    #[allow(missing_docs)]
    pub const BASALT: TileType = TileType(51);
    #[allow(missing_docs)]
    pub const BASALT_BLOCK: TileType = TileType(52);
    #[allow(missing_docs)]
    pub const COPPER_BLOCK: TileType = TileType(53);
    #[allow(missing_docs)]
    pub const TIN_BLOCK: TileType = TileType(54);
    #[allow(missing_docs)]
    pub const BRONZE_BLOCK: TileType = TileType(55);
    #[allow(missing_docs)]
    pub const IRON_BLOCK: TileType = TileType(56);
    #[allow(missing_docs)]
    pub const STEEL_BLOCK: TileType = TileType(57);
    #[allow(missing_docs)]
    pub const BLACK_SAND: TileType = TileType(58);
    #[allow(missing_docs)]
    pub const BLACK_GLASS: TileType = TileType(59);
    #[allow(missing_docs)]
    pub const TRADE_PORTAL_BASE_STONE: TileType = TileType(60);
    #[allow(missing_docs)]
    pub const TRADE_PORTAL_BASE_STONE_AMETHYST: TileType = TileType(61);
    #[allow(missing_docs)]
    pub const TRADE_PORTAL_BASE_STONE_SAPPHIRE: TileType = TileType(62);
    #[allow(missing_docs)]
    pub const TRADE_PORTAL_BASE_STONE_EMERALD: TileType = TileType(63);
    #[allow(missing_docs)]
    pub const TRADE_PORTAL_BASE_STONE_RUBY: TileType = TileType(64);
    #[allow(missing_docs)]
    pub const TRADE_PORTAL_BASE_STONE_DIAMOND: TileType = TileType(65);
    #[allow(missing_docs)]
    pub const FALLEN_LEAVES: TileType = TileType(66);
    #[allow(missing_docs)]
    pub const PLATINUM_BLOCK: TileType = TileType(67);
    #[allow(missing_docs)]
    pub const TITANIUM_BLOCK: TileType = TileType(68);
    #[allow(missing_docs)]
    pub const CARBON_FIBER_BLOCK: TileType = TileType(69);
    #[allow(missing_docs)]
    pub const GRAVEL: TileType = TileType(70);
    #[allow(missing_docs)]
    pub const AMETHYST_BLOCK: TileType = TileType(71);
    #[allow(missing_docs)]
    pub const SAPPHIRE_BLOCK: TileType = TileType(72);
    #[allow(missing_docs)]
    pub const EMERALD_BLOCK: TileType = TileType(73);
    #[allow(missing_docs)]
    pub const RUBY_BLOCK: TileType = TileType(74);
    #[allow(missing_docs)]
    pub const DIAMOND_BLOCK: TileType = TileType(75);
    #[allow(missing_docs)]
    pub const PLASTER: TileType = TileType(76);
    #[allow(missing_docs)]
    pub const LUMINOUS_PLASTER: TileType = TileType(77);
    /// One past the highest known id.
    pub const COUNT: u8 = 78;

    /// The constant's name in lower case (e.g. `dirt_grass`), or `None` for an unknown id.
    pub fn name(self) -> Option<&'static str> {
        NAMES.get(self.0 as usize).copied().flatten()
    }

    /// Preview colour (opaque RGB), or `None` for an unknown id.
    pub fn color(self) -> Option<[u8; 3]> {
        COLORS.get(self.0 as usize).copied().flatten()
    }
}

const NAMES: [Option<&str>; 78] = [
    None,
    Some("rock"),
    Some("air"),
    Some("water"),
    Some("ice"),
    Some("snow"),
    Some("dirt"),
    Some("sand"),
    Some("sand_beach"),
    Some("wood"),
    Some("cobblestone"),
    Some("red_bricks"),
    Some("limestone"),
    Some("limestone_block"),
    Some("marble"),
    Some("marble_block"),
    Some("time_crystal"),
    Some("sandstone"),
    Some("sandstone_block"),
    Some("red_marble"),
    Some("red_marble_block"),
    Some("flax_mat"),
    Some("flax_mat_yellow"),
    Some("flax_mat_red"),
    Some("glass"),
    Some("initial_portal_base_stone"),
    Some("gold_block"),
    Some("dirt_grass"),
    Some("dirt_grass_frozen"),
    Some("lapis"),
    Some("lapis_block"),
    Some("lava"),
    Some("wooden_platform"),
    Some("initial_portal_base_stone_amethyst"),
    Some("initial_portal_base_stone_sapphire"),
    Some("initial_portal_base_stone_emerald"),
    Some("initial_portal_base_stone_ruby"),
    Some("initial_portal_base_stone_diamond"),
    Some("north_pole"),
    Some("south_pole"),
    Some("west_pole"),
    Some("east_pole"),
    Some("placed_portal_base_stone"),
    Some("placed_portal_base_stone_amethyst"),
    Some("placed_portal_base_stone_sapphire"),
    Some("placed_portal_base_stone_emerald"),
    Some("placed_portal_base_stone_ruby"),
    Some("placed_portal_base_stone_diamond"),
    Some("compost"),
    Some("compost_grass"),
    Some("compost_grass_frozen"),
    Some("basalt"),
    Some("basalt_block"),
    Some("copper_block"),
    Some("tin_block"),
    Some("bronze_block"),
    Some("iron_block"),
    Some("steel_block"),
    Some("black_sand"),
    Some("black_glass"),
    Some("trade_portal_base_stone"),
    Some("trade_portal_base_stone_amethyst"),
    Some("trade_portal_base_stone_sapphire"),
    Some("trade_portal_base_stone_emerald"),
    Some("trade_portal_base_stone_ruby"),
    Some("trade_portal_base_stone_diamond"),
    Some("fallen_leaves"),
    Some("platinum_block"),
    Some("titanium_block"),
    Some("carbon_fiber_block"),
    Some("gravel"),
    Some("amethyst_block"),
    Some("sapphire_block"),
    Some("emerald_block"),
    Some("ruby_block"),
    Some("diamond_block"),
    Some("plaster"),
    Some("luminous_plaster"),
];

const COLORS: [Option<[u8; 3]>; 78] = [
    None,
    Some([110, 110, 115]),
    Some([150, 200, 240]),
    Some([40, 90, 200]),
    Some([170, 220, 245]),
    Some([240, 245, 250]),
    Some([120, 80, 45]),
    Some([220, 200, 130]),
    Some([230, 210, 150]),
    Some([150, 100, 50]),
    Some([130, 130, 130]),
    Some([160, 60, 45]),
    Some([200, 195, 170]),
    Some([210, 205, 180]),
    Some([225, 225, 225]),
    Some([235, 235, 235]),
    Some([120, 230, 230]),
    Some([200, 170, 110]),
    Some([210, 180, 120]),
    Some([170, 70, 70]),
    Some([185, 80, 80]),
    Some([200, 190, 150]),
    Some([220, 200, 60]),
    Some([190, 50, 50]),
    Some([190, 225, 235]),
    Some([70, 70, 90]),
    Some([230, 190, 40]),
    Some([80, 150, 50]),
    Some([170, 200, 190]),
    Some([40, 70, 160]),
    Some([50, 80, 180]),
    Some([240, 90, 20]),
    Some([170, 120, 60]),
    Some([150, 90, 200]),
    Some([50, 90, 220]),
    Some([40, 190, 90]),
    Some([210, 30, 60]),
    Some([180, 240, 250]),
    Some([220, 40, 40]),
    Some([220, 40, 40]),
    Some([220, 40, 40]),
    Some([220, 40, 40]),
    Some([70, 70, 90]),
    Some([150, 90, 200]),
    Some([50, 90, 220]),
    Some([40, 190, 90]),
    Some([210, 30, 60]),
    Some([180, 240, 250]),
    Some([80, 55, 35]),
    Some([70, 140, 45]),
    Some([160, 195, 185]),
    Some([60, 60, 65]),
    Some([75, 75, 80]),
    Some([200, 120, 70]),
    Some([180, 185, 190]),
    Some([180, 130, 60]),
    Some([160, 160, 165]),
    Some([140, 150, 160]),
    Some([50, 50, 55]),
    Some([40, 40, 50]),
    Some([90, 80, 60]),
    Some([150, 90, 200]),
    Some([50, 90, 220]),
    Some([40, 190, 90]),
    Some([210, 30, 60]),
    Some([180, 240, 250]),
    Some([170, 110, 40]),
    Some([215, 220, 225]),
    Some([190, 195, 205]),
    Some([35, 35, 40]),
    Some([140, 135, 125]),
    Some([150, 90, 200]),
    Some([50, 90, 220]),
    Some([40, 190, 90]),
    Some([210, 30, 60]),
    Some([180, 240, 250]),
    Some([235, 230, 220]),
    Some([250, 250, 210]),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table() {
        assert_eq!(TileType::DIRT_GRASS.name(), Some("dirt_grass"));
        assert_eq!(TileType(0).name(), None);
        assert_eq!(TileType(200).name(), None);
        assert_eq!(TileType::AIR.color(), Some([150, 200, 240]));
        assert_eq!(TileType(TileType::COUNT).color(), None);
        for i in 1..TileType::COUNT {
            assert!(TileType(i).name().is_some() && TileType(i).color().is_some(), "{i}");
        }
    }
}
