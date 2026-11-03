# LMDB stream (`crates/bhw-lmdb`)

Branch `feat/lmdb`. Build with `CARGO_TARGET_DIR=/home/kahverengi/Projects/blockheads-club/world-tool/target`.

## 2026-11-03

Done:
- Crate skeleton: `layout` (32/64 layout constants, mdb.c refs), `format` (MDB_db / MDB_meta encode/decode,
  DbFlags + comparators), `reader` (Env over `&[u8]`, meta pick, B+tree iteration incl. overflow, DUPSORT sub-pages
  and sub-DBs, LEAF2), `writer` (compact bulk-load writer like `mdb_copy -c`, DUPSORT/DUPFIXED supported), `convert`.
- `tests/common/mod.rs`: real-LMDB (lmdb-master-sys 0.2.6 = LMDB 0.9.70) helper; real LMDB uses the *native* layout,
  so `cargo test --target i686-unknown-linux-gnu -p bhw-lmdb` cross-checks the 32-bit layout with real 32-bit LMDB
  (gcc -m32 multilib works on this machine).
- `tests/real_saves.rs` (gated by `BHW_REAL_SAVES=<.../saves>`): all 15 local worlds read, rewritten 32/64-bit,
  re-read identical; real LMDB (x86_64 and i686) reads ours, appends 3 txns, compacts; ours reads the result.

Next:
- Unit tests for every error path + synthetic real-LMDB cross tests (dupsort, dupfixed, integerkey, reversekey,
  overflow, multi-level trees, non-empty free DB). Reach 100% line coverage (`cargo llvm-cov -p bhw-lmdb`).

Open questions: none yet.
