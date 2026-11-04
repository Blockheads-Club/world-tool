# Editor graphics: the tile/item → atlas table

The editor draws tiles and items with the user's own game graphics (`HDTex/TileMap.png`, `HDTex/Items.png`,
extracted by `bhw-assets`, see `game-files.md`). The APK has no data saying which atlas cell shows which tile type,
contents or item; the game decides that in its native code, which we do not read. So BH World Tool keeps its own
table, `crates/bhw-editor/data/atlas.json`, made by observation:

1. **By eye (`"src": "eye"`).** The atlases were cut into their 16 px (HD 64 px) grid and labelled with column/row
   numbers; cells whose picture clearly shows a material named by a tile type (dirt, sand, snow, red bricks, lava,
   gems…) were noted. Several entries are uncertain (e.g. limestone vs. marble blocks).
2. **In the game (`"src": "game"`).** Planned: a synthetic world with known tiles is loaded in the game on the test
   emulator and the rendered tiles are compared with the atlas cells; matching entries are marked `game`, wrong ones
   corrected.

Rules:

- Ids without an entry are drawn with BH World Tool's own plain colours (`bhw_save::tiles` for tile types,
  `bhw_editor::atlas::contents_color` for ores and gems) and a label in the inspector, so the editor works without
  game files and with an incomplete table.
- A tile is drawn as its back wall cell (darkened) when the foreground is air, then its foreground cell, then its
  contents cell on top. Edge and corner variants, damage, lighting and animations are not reproduced.
- The table is our own data (GPL); it never contains pixels from the game.

## Coverage of the table (2026-11-04)

Tile types: 31 of 77 (all `eye`). Contents: 15 of 148 (`eye`). Items: 9 of ~430 (`eye`).
