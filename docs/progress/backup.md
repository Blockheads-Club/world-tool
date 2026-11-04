# Stream: Android backup (.ab) + world files (`feat/backup`)

Crates: `crates/bhw-backup`, `crates/bhw-world`. Build with
`CARGO_TARGET_DIR=<repo>/target` (shared target dir, disk is tight).

## 2026-11-03

- Started. Crate skeletons + dependency choices:
  - `aes` 0.8 (RustCrypto, block cipher only; CBC + PKCS#7 done by hand in `crypto.rs` so the decrypting reader can
    stream and hold back the last block for padding removal), `pbkdf2` 0.12 + `sha1` 0.10 (PBKDF2-HMAC-SHA1 as
    Android), `flate2` (workspace, zlib), `getrandom` 0.3 (salts, keys, IVs, world ids). The 0.8/0.10/0.12 RustCrypto
    generation is used because the newer one (aes 0.9, sha2 0.11) needs Rust 1.88+/1.89 and the workspace MSRV is 1.86.
  - Own minimal tar reader/writer instead of the `tar` crate: the rewrite must copy original header blocks byte for
    byte and must emit Android-style pax headers (the `tar` crate writes GNU `L` long names, which Android's restore
    parser rejects).
  - `zip` 4.6 (deflate via flate2/zlib only), `serde`/`serde_json`, `sha2` 0.10 for `.bhworld`.
- Done (bhw-backup, 100 % lines in `cargo llvm-cov -p bhw-backup`): header parse/write, AES-256 (PBKDF2 user key,
  master key blob, v1 8-bit fallback, signed-char UTF-8 checksum quirk), streaming CBC reader/writer, zlib, own tar
  reader/writer (Android pax dialect), `TarEdit`/`rewrite_backup` (byte-identical copy of kept entries, insertion at end
  of innermost anchor run), `saves` discovery (`saves/<32hex>/world_db/data.mdb` anywhere, sibling `game_db/`),
  `world_edit`. Known-answer tests: RFC 6070 PBKDF2-HMAC-SHA1, NIST SP 800-38A CBC-AES256. Cross-check:
  `tests/java_xcheck.rs` runs `tests/xcheck/AbXcheck.java` (JDK SunJCE PBKDF2WithHmacSHA1 + AES/CBC + Deflater) both
  directions; skips without `java`.
- WIP (bhw-world): `lib.rs` (Error, module list), `time.rs`, `meta.rs` (traits WorldMetaSource/WorldMetaEditor/
  ServerWorldConverter, conflicts, unique_name, keep_both) written. NEXT: `files.rs` (WorldFiles: new/get/is_world/
  load_dir/from_saves_dir/list_saves_dir/write_dir (tmp+rename)/save_to_saves_dir(replace)/from_server_world_dir),
  `backup.rs` (worlds_from_backup(SavesContent), load_backup, put_worlds -> world_edit + rewrite_backup),
  `bhworld.rs` (zip: manifest.json first, preview.png, world/…; Limits; validation), `library.rs` (worlds/*.bhworld,
  backups/*.ab + .ab.json sidecar), `docs/bhworld-format.md`, `tests/real_saves.rs` (BHW_REAL_SAVES), coverage 100 %.

## 2026-11-03 (later, after machine sleeps)

- Done (bhw-world): `files.rs` (WorldFiles: new/get/is_world/is_server_world/total_size/to_memory/load_dir/
  from_saves_dir/list_saves_dir/from_server_world_dir/write_dir (temp + rename)/save_to_saves_dir (replace swaps in
  only after the new copy is complete)), `backup.rs` (worlds_from_backup, load_backup, put_worlds), `bhworld.rs`
  (format + validation, Limits), `library.rs` (worlds/*.bhworld list/add/import/rename/duplicate/delete;
  backups/*.ab + .ab.json sidecar add/list/delete), `meta.rs` traits, `docs/bhworld-format.md`.
- Coverage: `cargo llvm-cov -p bhw-backup -p bhw-world` = 100.00 % lines (2765/2765), 100 % functions; regions 97 %.
  `tests/` files are not part of the report (cargo-llvm-cov default).
- Real saves: `BHW_REAL_SAVES=<saves dir> cargo test -p bhw-world --test real_saves` passes on the 15 local
  Blockheads PC worlds (.bhworld round trip byte-identical, save_to_saves_dir, and insertion into a synthetic
  encrypted backup + reload). Skips when unset.
- Open questions / next:
  - Android save path inside `adb backup` is still UNVERIFIED (no phone). Discovery is generic; a device with zero
    worlds and no `game_db` gives no prefix → the app must ask the user to create one world first (or we add a
    verified default once a real backup is seen).
  - Not verified on a device: that restore accepts our inserted entries (header dialect follows AOSP
    `write_tarfile`/`TarBackupReader` from memory of the source; `kept_entries_are_byte_identical` guarantees
    untouched entries). First real test: `adb backup -f a.ab com.noodlecake.blockheads`, rewrite, `adb restore`.
  - Whether the world id is stored inside `world_db` (keep-both copies) → bhw-lmdb, via `WorldMetaEditor`.
  - `lock.mdb` is copied as-is; check whether the game needs/accepts a foreign lock file (LMDB recreates it).
