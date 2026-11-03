# iOS backups (bhw-ios) — progress

Branch `feat/ios`, worktree `../world-tool-wt/ios`. Read-only access to The Blockheads worlds inside iPhone/iPad
backups made by Finder/iTunes (unencrypted and encrypted). Format notes: `docs/ios-backup.md`.

## 2026-11-03
- Done: crate `crates/bhw-ios` (rusqlite bundled + serialize, plist, aes, pbkdf2, sha1, sha2; no new crypto crates,
  RFC 3394 implemented in `crypto.rs`).
  - `default_backup_dirs` / `candidate_backup_dirs` (Windows APPDATA + Microsoft Store USERPROFILE, macOS; Linux
    none), `list_backups`, `read_backup_info` (Info/Manifest/Status.plist, binary or XML).
  - `IosBackup::open(dir, password)`: unencrypted; encrypted iOS 10.2+ (DPSL/DPIC + SALT/ITER, ManifestKey) and
    older keybags (single PBKDF2, no ManifestKey). Typed errors `PasswordRequired`, `WrongPassword`,
    `Unsupported` (Manifest.mbdb, iOS <= 9). Decrypted Manifest.db is deserialized in memory only.
  - `blockheads_apps`, `blockheads`, `blockheads_for` → `BlockheadsData { bundle_id, worlds: Vec<WorldFiles>,
    game_db: Vec<WorldFile> }`; `entries`, `read_file` (Payload::File for plain, Payload::Bytes decrypted).
  - Tests: RFC 3394 §4.3/§4.6, NIST SP 800-38A CBC-AES256, known public fileID, synthetic backups (plain, modern
    encrypted, legacy encrypted, flat layout, many corruptions), bhw-save compatibility (GameDb + summarize).
  - Env-gated `tests/real_backup.rs` (`BHW_IOS_BACKUP`, `BHW_IOS_PASSWORD`), skips cleanly.
  - Coverage `cargo llvm-cov -p bhw-ios --ignore-filename-regex 'bhw-(lmdb|backup|world|save|device|assets|core)/'`:
    100 % lines (1024/1024), 100 % functions, 97.97 % regions. fmt + clippy -D warnings clean.
- Open: never run on a real backup (none available). Real DPIC (10M PBKDF2-SHA256 rounds) is slow in debug builds;
  use release builds or `[profile.dev.package]` opt-level overrides for pbkdf2/sha2/hmac if the app needs it.
  MSRV 1.86 not checked with a 1.86 toolchain (not installed; rusqlite 0.37 is edition 2021).
- Next: wire into the Tauri app (source "iPhone/iPad backup", password prompt, worlds read-only); real-backup run.
