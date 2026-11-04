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

## 2026-11-04 — step 4a: core service, Tauri commands, read-only viewer UI

### Done
- `bhw-core::editor`: sessions on Worlds-library worlds (`editor_open/close/region/overview/tile/catalog/atlas/
  state/blockheads/edit/find/history/changes/save`). Save writes back into the same `.bhworld` (manifest kept, preview
  redrawn, `edited` set) or as a new world (duplicate id/name + `set_identity`); server worlds stay 64-bit. Cached
  multiplayer worlds are refused. `Lib::save_edit`. Tests in `crates/bhw-core/tests/editor.rs` (every request kind,
  undo/redo, save, save as new, server and remote worlds, atlas from the game files cache); `editor.rs` and
  `library.rs` at 100 % lines.
- Tauri: 14 `editor_*` commands (binary bodies via `ipc::Response` for regions/overview/atlas), ACL + build list.
- UI (`app/src/lib/editor/`, `screens/Editor.svelte`, replaces the placeholder): `EditorFlow` (open → working copy,
  camera, row-wise block loading ≤128 blocks per request, atlas decode), `MapView` (cached per-block canvases,
  plain colours or atlas cells at ≥4 px/tile, back walls darkened, light layer, spawn/blockhead/selection marks;
  drag pan, wheel zoom at cursor, arrows/±/Enter keys), `Minimap` (overview RGBA window, click to move), layers,
  zoom buttons, Go to (G) dialog, spawn (Home), Tile inspector (name, back wall, contents, light, objects,
  protection, copy coordinates), Players (needs, inventory with item atlas icons), World (name, mode, size, time,
  version, created, id), status bar (game graphics vs plain colours), version/damaged notices, opening and error
  states. i18n en/tr/ru/kk (`edv.*`). Mock backend serves a synthetic world (`editor/mock-world.ts`).
- vitest: 108 tests, 100 % statements/branches/functions/lines (`bunx vitest run --coverage --maxWorkers=2`).
  Note: with full parallelism on this busy machine (load 10–25 from other agents) a few older screen tests time
  out at random (gamefiles/settings/read/update); they pass alone and with 2 workers. svelte-check 0/0.

### Next
- Edit tools in the UI (pencil/brush/eraser/select/fill/replace/paste, palette, history panel, save dialog).
- Emulator check of an edited world; atlas table calibration in the game.

## 2026-11-04 — milestone: steps 1–3 + read-only viewer (stop point, reported)

### State
- Commits on `feat/editor` (not pushed): content crate, edit model, render data/atlas table, core service, viewer UI.
- Checked with the user's own 1.7.6 atlas locally (not committed): the viewer draws textures; dirt/grass/rock/ore
  cells look plausible, but the table is only by eye.
- `cargo test --workspace --no-fail-fast`: all pass except `bhw-backup/tests/java_xcheck.rs` (JDK 27 on this
  machine; not touched by this stream). clippy -D warnings and rustfmt clean.

### Next (in order)
1. Edit tools in the UI (WT-Editor states): tool bar (I/P/B/E/S/F/R/V), block palette with search, brush size/shape,
   selection rectangle + copy/cut/paste/rotate/flip/fill/clear, replace with preview, history panel (go to step),
   unsaved count, save dialog (Save to Worlds / Save as new world / Don't save), blockhead move/needs/inventory
   editing with an item picker, world name/time settings, compare view (`editor_changes`), search panel,
   schematics saved as `.bhwschem` files (core commands still to add), full screen (F11), Tab hides panels.
2. Emulator: inject an edited synthetic world into `bhwt_api30`, confirm it loads (block version 0 vs 8, update
   counter, light fields), and calibrate the atlas table (`src: game`).
3. Horizontal wrap of the world in the map view (the game's worlds wrap around).

### Open questions for the user
- Time-of-day setting: day length in `worldTime` is assumed to be 1440 s — keep the setting hidden until confirmed?
- Tile/item names: the UI shows the game's English ids prettified ("Dirt grass"); translate tile names too?
- Should edits be allowed in areas the game never generated (we refuse now), and on tiles that belong to objects
  (trees, doors, chests: protected now)?
