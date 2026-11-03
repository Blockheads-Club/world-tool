# BH World Tool (Blockheads World Tool)

Desktop app (Windows, macOS, Linux) that copies The Blockheads worlds between Android phones (adb backup/restore over
USB or Wi-Fi, or root), the local Blockheads PC install, iPhone/iPad backups (read-only) and files on the computer
(`.bhworld`, Blockheads Server world folders). Phase 2 adds a standalone world viewer/editor that draws graphics
extracted from the user's own APK. GPL-3.0-or-later. Not affiliated with Majic Jungle / Noodlecake.

## Rules
- Package manager **bun** (never npm). Rust via rustup stable. UI: SvelteKit 2 + Svelte 5 + TS, adapter-static,
  Tauri 2 desktop. English UI by default, i18n en/tr/ru/kk.
- **100% test coverage, always** (Rust: `cargo llvm-cov`; app: vitest thresholds). Never lower thresholds; report
  coverage honestly.
- Conventional Commits. Push only to `dev`.
- Legal: never commit game binaries, APKs, extracted game graphics, decompiled code, real game saves made by other
  people, or private RE material (`~/Projects/reverse-engineering`). Do not reference private paths in committed
  files. Tests needing real saves/APKs must skip cleanly unless env vars point at them (`BHW_REAL_SAVES`, `BHW_APK`).
- Never upload user data anywhere.

## Layout
- `crates/bhw-lmdb` — pure-Rust LMDB file reader/writer for both 32-bit (game: Android/PC, ARMv7) and 64-bit
  (Blockheads Server) on-disk layouts.
- `crates/bhw-backup` — Android `adb backup` (.ab) read/write incl. encryption; tar handling.
- `crates/bhw-world` — world discovery/metadata in game saves, `.bhworld` format, conversions.
- `crates/bhw-assets` — validates the user's own APK (AXML manifest, `.apks`/`.xapk`, split folders) and extracts
  its graphics into a local cache (`docs/game-files.md`).
- `app/` — SvelteKit UI + `app/src-tauri` Tauri shell.
- `docs/progress/*.md` — per-stream progress logs (resume points). Keep them current.

## Known facts
- Game saves (Android, Blockheads PC): `.../saves/<32-hex id>/world_db/data.mdb` + `map/` dir; LMDB meta magic
  `0xBEEFC0DE` at offset 12 ⇒ 32-bit layout. Blockheads Server saves: magic at offset 16 (64-bit) and also have
  `server_db/`, `lightBlocks/`.
- Android APK: package `com.noodlecake.blockheads`, targetSdk 27, allowBackup default true, armeabi-v7a only.
