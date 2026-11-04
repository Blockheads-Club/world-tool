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
   - Ores in rock (copper, tin, iron, coal, gold) show their overlay cell; platinum and titanium are visible but
     their cells are not identified yet. Gems, flint, clay, charcoal and oil placed in rock were **not visible** in
     the game (gems probably need a different host or only show once uncovered), so those stay `eye`.
   - Some tile types (flax mats, fallen leaves, plaster) do not draw a front face in that test and stay unconfirmed.

Rules:

- Ids without an entry are drawn with BH World Tool's own plain colours (`bhw_save::tiles` for tile types,
  `bhw_editor::atlas::contents_color` for ores and gems) and a label in the inspector, so the editor works without
  game files and with an incomplete table.
- A tile is drawn as its back wall cell (darkened) when the foreground is air, then its foreground cell, then its
  contents cell on top. Edge and corner variants, damage, lighting and animations are not reproduced.
- The table is our own data (GPL); it never contains pixels from the game.

## Coverage of the table (2026-11-05)

Tile types: 66 of 77 (63 confirmed in the game). Contents: 15 of 148 (5 confirmed). Items: every id below 512 by the
grid rule (confirmed on samples), 33 block items (1024+) drawn as their tile.
