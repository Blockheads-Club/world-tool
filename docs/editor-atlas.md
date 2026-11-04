# Editor graphics: the tile/item → atlas table

The editor draws tiles and items with the user's own game graphics (`HDTex/TileMap.png`, `HDTex/Items.png`,
extracted by `bhw-assets`, see `game-files.md`). The APK has no data saying which atlas cell shows which tile type,
contents or item; the game decides that in its native code, which we do not read. So BH World Tool keeps its own
table, `crates/bhw-editor/data/atlas.json`, made by observation:

1. **By eye (`"src": "eye"`).** The atlases were cut into their 16 px (HD 64 px) grid and labelled with column/row
   numbers; cells whose picture clearly shows a material named by a tile type were noted.
2. **In the game (`"src": "game"`).** A copy of a test world on the emulator (our own edits: rows of known tile
   types and ores floating in daylight above five blockheads, each blockhead holding eight known items) was opened
   in the game; screenshots were cut into tiles and compared with the atlas cells (a small script ranks the cells,
   the final match is checked by eye). Confirmed entries are marked `game`, wrong guesses were corrected.
   - Items: the icons follow a grid by id: column `id % 32`, row `id / 32` of `Items.png` (`items_grid`), seen for
     ids 3–38, 100–139 and 200–232. Items from 1024 on are blocks and show the cube of a tile (`item_tiles`).
   - Ores in rock (copper, tin, iron, coal, gold, platinum, titanium) show their overlay cell.
   - Materials only show in the tile the game keeps them in (2026-11-05, see `client-save-format.md` §3): flint
     and clay in dirt (full dirt cells with streaks, [0,9] and [2,9]), oil in limestone ([14,2]); gems are not
     contents of a solid tile at all but crystals in the air of caves (cells [25..29,3]), shown once the game has
     uncovered the gem promise. Charcoal (contents 45) placed in dirt or rock shows nothing and is not offered.
   - Flax mats (21–23) and fallen leaves (66) placed as tiles are not drawn at all (2026-11-05): floating in the
     air nothing shows, and standing on grass they only cut a gap into the grass's top face. None of the 17 real
     saves (15 PC, 2 emulator) holds any of them, so the game seems to keep them for something else; 21 keeps its
     `eye` cell so the editor shows the tile, 22, 23 and 66 stay without one.
   - Iron (56), steel (57), plaster (76) and luminous plaster (77) blocks show a uniform face without any texture
     (in daylight next to a brick block of known cell, emulator): whichever white cell the game tints, it looks
     like a plain colour, so the editor draws them with plain colours measured from the game (face colour divided
     by the brick's daylight factor 0.85): iron (86, 86, 87), steel (172, 171, 173), plaster and luminous plaster
     white.
   - Water (3) is drawn translucent with the wavy cell pair [14,0]–[15,0] (like lava's [14,1]–[15,1]); the flat
     cyan [0,3] is not it. Ice (4) is the pale bevelled cell [12,3], seen in the seconds before it melted: no
     real save has a cold biome or any snow or ice (the zone byte only holds 1–3), and placed ice melts into water
     within about 10 s in the test world.

Rules:

- Ids without an entry are drawn with BH World Tool's own plain colours (`bhw_save::tiles` for tile types,
  `bhw_editor::atlas::contents_color` for ores and gems) and a label in the inspector, so the editor works without
  game files and with an incomplete table.
- A tile is drawn as its back wall cell (darkened) when the foreground is air, then its foreground cell, then its
  contents cell on top. Edge and corner variants, damage, lighting and animations are not reproduced.
- The table is our own data (GPL); it never contains pixels from the game.

## Coverage of the table (2026-11-05, night)

Tile types: 69 of 77 have a cell (68 confirmed in the game: all but flax mat 21, which the game does not draw;
air needs none; 22, 23, 56, 57, 66, 76, 77 use plain colours). Contents: 18 of 148 have a cell (15 confirmed), including every material the palette offers. The other
contents are parts of objects (trees, plants, doors, torches…) that the game draws from `dw` objects; they are
shown with the tile and the inspector names them. Items: every id below 512 by the grid rule (confirmed on
samples), 33 block items (1024+) drawn as their tile.
