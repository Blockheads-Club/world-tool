# Save contents stream (`crates/bhw-save`)

Branch `feat/save`. Build with `CARGO_TARGET_DIR=/home/kahverengi/Projects/blockheads-club/world-tool/target`.

## 2026-11-03

Done:
- Analysed the 15 local Blockheads PC saves, their `game_db` and a Blockheads Server save (dev tools:
  `examples/dump.rs`, `examples/export.rs`, `examples/inspect.rs`).
- Crate modules: `blocks` (chunk codec), `tiles` (tile ids + own colours), `plistio` (Apple/GNUstep XML),
  `save` (client/server world in memory as `FileMap`), `summary`, `preview` (map PNG + stored thumbnail),
  `gamedb` (world list), `identity` (id/name), `convert` (client <-> server with extras folders), `world`
  (bhw-world trait impls: `SaveMeta`).
- Merged dev (bhw-backup, bhw-world).

Next: tests to 100% line coverage with synthesized fixtures; gated real-save tests; `docs/client-save-format.md`.

Open questions:
- Does a dedicated server give the single-player blockheads (record `blockheads`) to anyone? It loads them as
  host-less blockheads; not tested in game.
- Is `map/default/` required by the game for an imported world? Conversion creates an empty one.
