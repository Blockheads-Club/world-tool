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
