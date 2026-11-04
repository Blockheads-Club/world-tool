# World viewer/editor (Phase 2) — progress

Branch `feat/editor`, worktree `../world-tool-wt/editor`. Design boards: `WT-Editor`, `WT-GameFiles`, `WT-Library`,
`WT-Errors`, `WT-Small` (design-ref, not in the repo).

Plan: (1) `crates/bhw-content` world contents; (2) `crates/bhw-editor` edit model (selection, operations,
undo/redo, schematics); (3) renderer data (tile → atlas table + colour fallback, chunk data, minimap);
(4) Tauri commands + editor UI; (5) emulator check that an edited world loads in the game.

## 2026-11-04 — step 1: bhw-content

### Done
- `crates/bhw-content`: `WorldContent::load(EnvContents)` / `to_env()`. Terrain `BlockStore` (records decoded on
  demand, bounded cache, dirty blocks re-encoded with update counter + 1), `TileData` (all 64-byte fields named,
  unknown bytes kept), `Objects` (`dw` records, parse/set/insert/remove, emptied records dropped), `Players`
  (single-player `blockheads` + `blockhead_<id>_inventory`, server `<pid>_blockheads` + `<pid>_blockhead_<id>_inventory`,
  gzip/dialect kept; position, needs in `state`, inventory stacks), `WorldRecord` (name, id, mode, spawn, width,
  dates, time of day). Name tables (contents, object types, item ids and names) generated from our own Blockheads
  Server crate.
- Unchanged records keep their bytes and order; only changed records are rewritten.
- Tests: synthetic fixtures; `tests/real_saves.rs` (env `BHW_REAL_SAVES`) — all 15 local Blockheads PC worlds
  round-trip byte-identically and a one-tile edit changes exactly one block record.
- Coverage `cargo llvm-cov -p bhw-content`: 100 % lines.

### Next
- Step 2: `bhw-editor` crate.

### Open questions
- Length of a game day in `worldTime` (assumed 1440 s) — confirm in the game before shipping the time-of-day setting.
- Byte 7 of an inventory item record is 0x11 on the first item of a stack in real saves (server writes 0); kept as is.
- Whether the game accepts an edited block whose version byte is 0 (freshly generated) — check on the emulator.

## 2026-11-04 — step 2: bhw-editor edit model

### Done
- `crates/bhw-editor`: `Session` over `WorldContent` with operations pencil/brush/eraser strokes (`paint`), `fill`,
  `clear`, `find`/`replace` (selection or whole world), `copy`/`cut`/`paste` (skip air), blockhead move/needs,
  inventory stacks, world name and time of day. Every operation is one history step of before/after values;
  `undo`/`redo`/`go_to`, unsaved count, `changed_tiles` (compare view), `save()` → `EnvContents`.
- Safety rules: tiles that belong to a dynamic object (owner id set or object contents such as trees, plants,
  workbenches, doors, ladders, torches) are never changed and are counted as `protected`; ungenerated areas are
  counted as `missing`; one step holds at most 1 000 000 tile changes (rolled back if exceeded); brushes may only
  place plain contents (ores, gems, flint, clay, charcoal).
- Schematics: `Schematic` (rotate, flip) and the `.bhwschem` file format, documented in `docs/schematic-format.md`.
- `bhw-content` got a `test-util` feature with the synthetic fixtures.
- Coverage: 100 % lines in `bhw-content`; `bhw-editor` 100 % of lines in `--show-missing-lines`
  (the summary of the shared target dir counts 2 lines in `session.rs` that no report lists).

## 2026-11-04 — step 3: render data and atlas table (draft)

### Done
- `bhw_editor::view`: `block_view` (4 bytes per tile: fg, back wall, contents, light), `region` (binary message for
  up to 256 blocks), `overview` (whole world in plain colours at 1 px per n tiles, for the minimap).
- `bhw_editor::atlas`: table `data/atlas.json` (tile/contents/item → atlas cell, `src` eye/game), validation,
  `Catalog` (names, plain colours, cells, placeable contents) for the UI. `docs/editor-atlas.md` explains how the
  table is made; current coverage: 31 tile types, 15 contents, 9 items, all by eye (not yet confirmed in the game).
- Tried: the game's explored-map tiles (`map/default/*.png`) as colour reference — they are lit/darkened by the
  game's light, useless for colour matching. The game's world thumbnail is smoothed and lit too.

### Next
- Step 4: `bhw-core` editor service + Tauri commands + editor UI (read-only viewer first).
- Emulator calibration of the table (the emulator was in use by `feat/phase1` on 2026-11-04).
