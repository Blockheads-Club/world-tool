# Core stream (`crates/bhw-core` + `app/src-tauri` wiring)

Branch `feat/core`. Build with `CARGO_TARGET_DIR=<repo>/target`
(also for `app/src-tauri`).

## 2026-11-03

- Started. Merged `dev` (bhw-assets, bhw-device GameStopped fix).
- Plan: `bhw-core` = all app logic behind a `Core` service (settings, activity log, library index, read sessions,
  transfer plans, backups store, game files, cancel registry); `src-tauri` = thin glue (blocking tasks, Channel
  progress, devices-changed polling, pickers/opener).
- Done: `bhw-core` modules (error/types/settings/activity/util/worlds/library/devices/read/transfer/backups/files/
  gamefiles). `tests/real_device.rs` (gated `BHW_DEVICE_SERIAL`) passes on the API 30 emulator: read 2 worlds →
  save to library → export .bhworld + server folders → copy ROCKY OCEAN back as "keep both" (fresh backup stored as
  automatic backup, rewrite incl. game_db, restore; 2 confirm taps) → re-read shows "ROCKY OCEAN (2)" → restore the
  automatic backup → re-read shows the original 2 worlds. ~50 s.
- Fix in bhw-device: `stop_game` retries `am kill` (the game process needs a moment after HOME; seen as
  GameRunning on the emulator).
- (superseded by the entry below)

## 2026-11-03 (later) — wiring complete, verified in the game

### Done
- Merged `dev` again (bhw-ios). iPhone/iPad backups: `ios_backups` (default Finder/iTunes folders or a picked
  folder / single backup), `ios_open` (password, wrong attempts counted → `wrong_password` + `attempt`), the opened
  backup becomes a read-only device with an already-read session (source for save/export/transfer; target →
  `not_supported_yet`). 64-bit iOS worlds are converted to 32-bit LMDB (`worlds::to_32bit`).
- Game files wired to bhw-assets (`game_files`, `use_apk`, `reextract`, `clear_game_files`, `apk_from_device` via
  `pm path` + `adb pull`, split APKs into a folder).
- `app/src-tauri` is thin glue over `bhw_core::Core`: blocking tasks, `Channel<Progress>`, cancel registry,
  `devices-changed` polling (3 s), dialog plugin pickers, `show_in_folder` via the OS file manager, adb sidecar
  lookup next to the exe (`docs/adb-sidecar.md`). Settings moved into bhw-core. `rust-version` 1.90 (dialog plugin);
  `[profile.dev.package]` opt-level 3 for pbkdf2/sha2/hmac/aes.
- UI: new error codes `game_running`, `game_stopped`, `not_supported_yet`, `adb_missing` (toasts, en/tr/ru/kk).
- Tests: `crates/bhw-core/tests/{phone,pc,library,misc}.rs` on a scripted phone (`bhw_device::fake`, `backup -f`
  writes the phone's current `.ab`, `restore` replaces it) and synthesized saves/backups/APKs/iOS backups
  (`test-util` features of bhw-assets and bhw-ios). `cargo llvm-cov -p bhw-core --ignore-filename-regex
  'bhw-(lmdb|backup|world|save|device|assets|ios)/'`: **100 % lines (1897/1897), 100 % functions**, ~95.6 % regions.
  bhw-device, bhw-lmdb, bhw-save stay at 100 % lines. Workspace clippy -D warnings + fmt clean; app vitest 100 %,
  svelte-check clean.
- Emulator (`BHW_DEVICE_SERIAL=emulator-5600 cargo test -p bhw-core --test real_device -- --nocapture`, ~65 s):
  read → save → export (.bhworld + server folders) → copy ROCKY OCEAN back as "keep both" → copy the exported
  server folder of MY FIRST WORLD back (server → game conversion, keep both) → re-read: 4 worlds → restore the
  automatic backup → re-read: the original 2. Both copies were opened in the game (`BHW_DEVICE_KEEP_COPY=1`) and
  load; the game's world list shows them.

### Found on the emulator (fixed)
- A copied world crashed the game (SIGSEGV right after "LOAD ATTEMPTS: 1") on its first load. Causes:
  1. compact LMDB files had an empty meta page 0 (like `mdb_copy -c`); LMDB picks the meta page from the lock
     file's last txnid, so next to a zeroed/stale `lock.mdb` the game saw an empty tree. bhw-lmdb now writes the
     current tree into both meta pages.
  2. worlds without `world_db/lock.mdb` crash the game; written worlds get a zero-filled 8 KiB one
     (`transfer::with_lock`, which also re-encodes `data.mdb`).
  3. bhw-save rewrote the whole world record with another XML writer; `identity::rewrite_record` now edits the
     `saveID` / `worldName` strings in place (byte-identical otherwise). Not proven to crash on its own, but safer.
- `am kill` only works after the game left Android's "previous app" state (> 5 s after leaving a world):
  `Adb::stop_game` retries for `Timeouts::stop` (30 s).
- adb restore sets file mtimes from the tar header × 1000 on this image (files show 1970-01-21); harmless.

### Command coverage (src-tauri → bhw-core)
| Implemented | Stub |
| --- | --- |
| app_info, get/set_settings, take_open_files, restart_app, cancel, list_devices (+ devices-changed), wifi_discover, wifi_pair, paired_devices, forget_paired, restart_adb, open_device_manager (Windows), ios_backups, ios_open, diagnostics, read_worlds, save_to_library, export_worlds, free_space, source_worlds, transfer_plan, transfer, list/rename/duplicate/delete_world, show_in_folder, inspect_import, import_worlds, list/restore/delete_backup, list/clear_activity, game_files, use_apk, apk_from_device, reextract, clear_game_files, pick_folder, pick_import, pick_apk | check_update, download_update (`not_implemented`; updater not wired) |

### Next
- Updater (`tauri-plugin-updater`) for check/download_update; CI workflow; ship adb (`docs/adb-sidecar.md`).
- Root read/write path (bhw-device has `root_pull`/`root_push`; core always uses adb backup, `Device.root` is false).
- Progress `elapsed` while waiting on the phone is sent once (no ticker yet).
- Real phone (not emulator) and real iPhone backup runs.

### Open questions
- Restores are not verified by reading the phone again (Android gives no signal when "Restore my data" is
  declined); the core assumes success and keeps the rewritten backup as the phone's read session.
- `game_db` inside the backup is replaced while its old `lock.mdb` stays; fine with both meta pages current, but a
  zeroed lock could be written too.
