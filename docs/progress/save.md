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

- Tests: `tests/api.rs` (synthesized fixtures built with bhw-lmdb) gives 100% line coverage
  (`cargo llvm-cov -p bhw-save --ignore-filename-regex 'bhw-(lmdb|backup|world)/'`: 1159/1159 lines, 191/191
  functions, 98.4% regions). `tests/real_saves.rs` (gated by `BHW_REAL_SAVES`, optional `BHW_REAL_SERVER_SAVE`):
  15 worlds summarised and rendered, 13 single-player worlds round-tripped client→server→client with every record
  and file identical, server output read by real LMDB 0.9.70 (same lmdb-master-sys as Blockheads Server); the
  server sample round-trips server→client→server identically. Skips cleanly without the env vars.
- `docs/client-save-format.md` written.

Next: nothing required. Possible: synthesize a thumbnail for server-made worlds; confirm open questions in game.

Open questions:
- Does a dedicated server give the single-player blockheads (record `blockheads`) to anyone? It loads them as
  host-less blockheads; not tested in game.
- Is `map/default/` required by the game for an imported world? Conversion creates an empty one.
- Answer for the bhw-world stream: the id is stored in `world_db` `main/worldv2` (`saveID`), in a server's
  `worldv2` file and as the `game_db` key `<id>_worldv2`; keep-both must rewrite them (`SaveMeta::set_identity`
  does the first two; the caller updates `game_db` with `GameDb::register`/`unregister`).
