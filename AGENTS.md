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
- `crates/bhw-core` — all app logic as a service (`Core`) the Tauri commands call; scripted-adb tests.
- `crates/bhw-assets` — validates the user's own APK (AXML manifest, `.apks`/`.xapk`, split folders) and extracts
  its graphics into a local cache (`docs/game-files.md`).
- `app/` — SvelteKit UI + `app/src-tauri` Tauri shell.
- `docs/progress/*.md` — per-stream progress logs (resume points). Keep them current.
- `.github/workflows/` (ci, auto-release, release), `.releaserc.json`, `scripts/` (release helpers, root
  `bun test`) — releases are cut by semantic-release from `dev`; see `docs/progress/release.md`.

## Known facts
- Game saves (Android, Blockheads PC): `.../saves/<32-hex id>/world_db/data.mdb` + `map/` dir; LMDB meta magic
  `0xBEEFC0DE` at offset 12 ⇒ 32-bit layout. Blockheads Server saves: magic at offset 16 (64-bit) and also have
  `server_db/`, `lightBlocks/`.
- Android saves inside `adb backup`: `apps/com.noodlecake.blockheads/f/Library/Application Support/` with `saves/<id>/`,
  `game_db/` (world list; a world must be registered there to show in the game) and `game/`. Verified on an API 30
  emulator, including restore of an injected world that then loaded in the game.
- `adb backup` skips apps in Android's "stopped" state (after `am force-stop` or never launched) and returns a
  header-only file. Never force-stop before a backup; send the game to the background and use `am kill`.
- A world written to the game needs `world_db/lock.mdb` and an LMDB file whose meta pages both hold the current
  tree (LMDB picks the meta page from the lock file's txnid); otherwise 1.7.6 crashes loading it (emulator, see
  `docs/progress/core.md`). bhw-lmdb writes both metas; bhw-core adds a zero-filled lock file.
- `am kill` only kills the game once it left the "previous app" state (seconds after HOME); retry for a while.
- Android gives no signal when "Restore my data" is declined (adb restore exits 0): bhw-core checks every restore with
  a fresh backup (`restore_declined`); verified on the emulator by tapping "Do not restore".
- Root writes: files root creates under the app's data get the creating process's SELinux level, so the app's
  context (`app_data_file:s0:cX,cY`) is read before writing and restored with `chcon -R` (plus `chown`,
  `restorecon`); verified on the emulator (`adb root`), the game starts afterwards.
- adb is not shipped (Platform-Tools license): downloaded from Google on first run (`docs/adb-download.md`).
- Android APK: package `com.noodlecake.blockheads`, targetSdk 27, allowBackup default true, armeabi-v7a only.
- Materials live in fixed tiles (flint/clay in dirt, oil in limestone, ores in rock); gems are a promise in byte
  0x0b of a cave air tile, uncovered by the game. The game opens a world at its saved camera (`translation`), so the
  editor moves the camera with a moved blockhead (`docs/client-save-format.md`, emulator 2026-11-05).
