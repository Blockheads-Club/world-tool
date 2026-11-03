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

- Unit tests (`src/tests.rs`) for all error paths, both layouts; synthetic real-LMDB cross tests
  (`tests/real_lmdb.rs`: dupsort/dupfixed/integerkey/reversekey/overflow/multi-level/free pages/page sizes).
  Both pass on x86_64 and on `--target i686-unknown-linux-gnu` (real 32-bit LMDB).
- Coverage (`cargo llvm-cov -p bhw-lmdb`): 100% lines, 100% functions, ~99.6% regions (remaining regions are
  `?` error exits on paths like iterating after a nested error).

- 2026-11-03 (after sleep): clippy clean; full test suite green on x86_64 and i686 (real 32-bit LMDB), with
  `BHW_REAL_SAVES` set (15 worlds) and without (skips). Fixed a few mdb.c line citations. Coverage: 100% lines
  (925/925), 100% functions, 99.59% regions (8 `?` early-exit regions, e.g. `read_file` on a non-LMDB file).

Next: stream complete. Possible follow-ups: `Env::get` uses a linear scan (fine for world sizes); a lenient
mode that falls back to the other meta page if one is corrupt (LMDB itself refuses such files).

Findings:
- `mm_flags` (free DB md_flags) = `me_flags & 0xffff | MDB_INTEGERKEY`, so LMDB opened with MDB_NOSUBDIR persists
  0x4008; game saves have 0x0008. We preserve the source value.
- Real LMDB silently drops empty values in DUPSORT DBs; our writer rejects them (1..=511 bytes required).
- `Cargo.lock` intentionally not committed in this branch (would conflict with sibling streams); add at merge.
