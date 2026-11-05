# Phase 1 follow-ups (`feat/phase1`): adb download, restore verification, root path

Build with `CARGO_TARGET_DIR=<repo>/target` (also for `app/src-tauri`).

## 2026-11-04

### Done
- **adb on first run** (no bundling): `bhw_core::adb_tools` — `Fetch` trait (app: reqwest blocking in `src-tauri`,
  tests: fake), `adb_status`, `adb_download_info` (HEAD size), `download_adb` (progress, cancel, size limits, zip
  unpacked into `<app data>/platform-tools/` via a staging dir, `adb version` check, then used at once).
  `adb_tools::locate(data_dir)` = downloaded → ANDROID_HOME/SDK_ROOT → PATH. New error code `download_failed`.
  Tauri commands `adb_status`, `adb_download_info`, `download_adb`; sidecar lookup removed.
- **Verify after restore**: every `adb restore` (copy and restore-backup) is followed by a check backup
  (`phone_check` wait stage, `verifying` stage); written worlds must match byte for byte (`world_db/data.mdb`),
  be registered in `game_db`, removed ones gone. Else `restore_declined` (with backup id). Restoring a stored backup
  without the app's `_manifest` (Blockheads PC / root backups) is rebuilt into a fresh phone backup first (Android
  ignores manifest-less data).
- **Root path**: `bhw_core::root` — probe once per phone (background on first listing, `Device.root`), `root_mode`
  `auto` | `never` (old `always` → `auto`). Read/copy/restore via `tar` over exec-out/exec-in: game stopped
  (`am kill`), saves + game_db pulled, stored as automatic `.ab` backup, edited locally, pushed back, chown +
  restorecon + chcon (context read *before* relabelling — fixed in bhw-device), world ids checked. Falls back to the
  backup method when root is missing/fails before writing.
- bhw-device: `fake::Reply::dynamic` (reply built at spawn time).
- Emulator (`BHW_DEVICE_SERIAL=emulator-5600 BHW_DEVICE_ROOT=1 cargo test -p bhw-core --test real_device --
  --nocapture`, ~195 s): declined restore reported as `restore_declined`; copies verified; root read/copy/restore
  without confirm screens; 40 entries owned by the app uid with `app_data_file:s0:c512,c768`; game starts; ends with
  the original 2 worlds.

- UI: `AdbDialog` (source URL, size, destination, Google terms page, progress, cancel, retry), "Phone tool needed"
  notice on Devices, Settings row (where adb is / download), `adb_missing` opens the dialog; transfer/restore show
  "Tap “Back up my data” once more" (`phone_check`) and the "Check the worlds are on the phone" step;
  `restore_declined` dialog with "Try again"; root setting is Automatic / Backup method. i18n en/tr/ru/kk. Mock:
  `?adb=missing`, `fail.adb`, `fail.verify`.
- Tests: bhw-core 100 % lines (2362/2362) and functions; bhw-device 100 % lines (1374/1374); workspace clippy
  -D warnings + fmt clean; src-tauri clippy clean, 4 tests; app 94 vitest tests, 100 % statements/branches/
  functions/lines, svelte-check 0 errors.

### Next
- ~~Real (non-emulator) rooted phone with Magisk~~ (done 2026-11-05, see "Real phone run" below). Android 6 toybox
  may lack `tar` (root then fails → backup method only if it fails before writing).
- The emulator check backup after restore needs a third tap on the phone; consider `bu`/root alternatives later.
- App vitest suite is timing-sensitive under heavy machine load (default 1 s `findBy` timeouts); `--maxWorkers=2`
  passes reliably.

## Real phone run (2026-11-05)

Galaxy Z Fold4 (SM-F936B), Android 14, game 1.7.6, Magisk root, over Wi-Fi adb. `tests/real_device.rs` with
`BHW_DEVICE_WORLD="MY FIRST WORLD" BHW_DEVICE_ROOT=su` (new: `su` instead of `adb root`; `BHW_DEVICE_WORLD` picks the
world; a single copyable world is enough). Backup method: read, exports, declined restore reported, two "keep both"
copies (one from a server export), copies open in the game, restore to the start. Root: read, copy, owner/SELinux
context of 34 entries match, game starts, restore. Afterwards the phone's saves were checked against a backup taken
before the run (same files; LMDB contents equal key by key, root restores repack the pages).

Found and fixed: `adb exec-in` returns before the phone has unpacked the tar stream (3 MB sent, ~1.1 MB written when
the next command ran; Magisk's `su` relays stdin, Wi-Fi adds to it). The root write moved a half-unpacked folder,
so a new world was missing and the check after writing failed (`Io`). `root_push` now has `tar` write its exit code
to `.bhw-incoming.done` and the move step waits for it (up to the confirm timeout) and refuses unless it is 0.

Seen: the game lists joined multiplayer worlds (`remoteGame`) as "ONLINE WORLD – <host>" with a Join button; the
tool hides them (they are local caches of a server's world). Foldables print a warning before `screencap` output.
