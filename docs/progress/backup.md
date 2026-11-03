# Stream: Android backup (.ab) + world files (`feat/backup`)

Crates: `crates/bhw-backup`, `crates/bhw-world`. Build with
`CARGO_TARGET_DIR=/home/kahverengi/Projects/blockheads-club/world-tool/target` (shared target dir, disk is tight).

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
