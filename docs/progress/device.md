# Devices stream (`crates/bhw-device`)

Branch `feat/device`. Build with `CARGO_TARGET_DIR=/home/kahverengi/Projects/blockheads-club/world-tool/target`.
Moves bytes only: raw `.ab` streams and plain directories in/out; parsing is `bhw-backup` / `bhw-world`'s job.

## Transport decision (2026-11-03): spawn the `adb` binary

Evaluated `adb_client` 3.2.3 (MIT, crates.io) source:
- no `backup:`/`restore:` services (the core of this tool) and no `exec-in`; would need raw service plumbing;
- has `pair`, `connect`, `mdns services`, shell with exit status (shell v2) — but only through a running adb
  server (TCP mode) or direct USB via `rusb` (needs our own RSA key = second authorization prompt on the phone,
  conflicts with a running adb server, Windows needs WinUSB driver anyway);
- `rust-version = 1.88` > workspace 1.86.

So we spawn `adb` (bundled sidecar path from config, else `$ANDROID_HOME`/`$ANDROID_SDK_ROOT/platform-tools`, else
`PATH`). It is what users already authorized, supports every feature (backup/restore, exec-out/exec-in, pair,
connect, mdns) and is what Google maintains. Everything goes through `process::Runner` / `process::Child`
traits; `fake::ScriptedRunner` scripts adb answers for tests (exported so the app can use it too).

## API summary
- `Adb::locate(sidecar)`, `Adb::new(program, runner)`, `timeouts {command, confirm, stall}`, `run`, `shell`
  (exit code via `__BHW_RC=$?` marker, works without shell v2), `shell_ok`, `version`, `start/kill_server`.
- Discovery: `list_devices` (`adb devices -l`: serial, state device/unauthorized/offline/no permissions/other,
  usb/wifi/emulator, model, transport id), `device_details` (model, Android release/SDK, game versionName/
  versionCode/uid/dataDir via one `getprop` + `dumpsys package` round trip), `GameInfo::supported` (1.7.4–1.7.6),
  `probe_root` (`id` as adb root, `su -c`, `su 0`), `discover(check_root)`, `DeviceWatcher::poll` /
  `spawn(interval, cb)` → `Added/Changed/Removed`.
- Game process: `game_running` (`pidof`), `stop_game` (`am force-stop`), `IfRunning::{Fail, Stop, Ignore}`.
- `.ab`: `backup(serial, &mut Write, AbOptions, progress, &CancelToken)` / `backup_to_file`, `restore(serial,
  &mut Read, ...)`. `AbMethod::AdbCommand` (default: `adb backup -f <tmp>` tailed while it grows; `adb restore
  <staged tmp>`) or `AbMethod::BuStream` (`exec-out bu backup` / `exec-in bu restore`, no temp file).
  Watchdog thread: cancel, confirm timeout (no byte yet), stall (bytes stopped ⇒ `Disconnected`).
  `check_backup`: 0 bytes ⇒ `DeclinedOnPhone`; header only / empty tar / tiny encrypted payload ⇒ `EmptyBackup`.
- Root: `root_find_saves` (`find` for `saves/<32hex>/world_db` under `/data/data/<pkg>` and `/data/user/0/<pkg>`,
  fallback `files/Library/Application Support/saves`), `root_pull(rel → local dir)` via `exec-out su -c tar -cf -`
  unpacked with the `tar` crate (path-traversal safe), `root_push(local dir → rel)` via `exec-in su -c tar -xf -`
  into `.bhw-incoming`, then swap in, `chown -R <stat uid:gid>`, `restorecon -R`, `chcon` with the dir's context.
- Wi-Fi: `pair`, `connect`, `disconnect`, `mdns_available`, `mdns_services` (pairing/connect/legacy),
  `paired_hosts` / `forget_paired_host` (edits `~/.android/adb_known_hosts.pb`; then `kill_server`).
- Blockheads PC: `PcInstall::candidates_with/locate` (`BHPC_DATA_DIR` → launcher.toml `data_dir` in
  `BHPC_CONFIG_DIR` or `<config>/blockheads-pc` → `<data dir>/blockheads-pc`: `~/.local/share`, `%APPDATA%`,
  `~/Library/Application Support`), `support_dir/saves_dir/game_db`, `list_worlds`, `read_world`,
  `write_world(id, src, overwrite, running)` (temp sibling + rename), `pc::game_running(runner, windows)`
  (`ps -A -o comm=` / `tasklist`, process `bhpc-run`).
- Errors (`DeviceError`): AdbNotFound, Unauthorized, NoPermissions, Offline, DeviceNotFound, GameNotInstalled,
  UnsupportedVersion, GameRunning, DeclinedOnPhone, ConfirmTimeout, WrongPassword (for callers), EmptyBackup,
  NotABackup, Disconnected, DiskFull (ENOSPC / Windows 39, 112 / StorageFull), Cancelled, RootUnavailable,
  SavesNotFound, WorldExists, PathNotFound, PairFailed, ConnectFailed, Command, Timeout, Io.
- Hints (`hints::hints_for(os, devices)`): enable USB debugging; Windows driver (Windows adb lists *nothing*
  when the ADB interface has no driver; Device Manager shows "ADB Interface"/"Other devices" with a warning);
  Linux udev rules for `no permissions`; accept the RSA prompt; reconnect offline; adb server version conflict.
  Windows paths: `adb.exe`, `CREATE_NO_WINDOW` on spawn, `tasklist` CSV parsing, `std::env::split_paths`.

## 2026-11-03

Done:
- Crate with all of the above; 38 unit tests on scripted adb; `cargo llvm-cov -p bhw-device`: 100% lines, 100%
  functions (~97.7% regions).
- Gated tests: `tests/real_device.rs` (`BHW_DEVICE_SERIAL=<serial>`, optional `BHW_DEVICE_METHOD=bu`,
  `BHW_DEVICE_KEEP=<path>`, `BHW_DEVICE_ROOT=1`): list → details → backup to a temp file → size.
  `tests/real_pc.rs` (`BHW_REAL_SAVES=.../home/Library/Application Support/saves`): read-only, 15 local worlds OK.

Verified for real: SystemRunner process handling (sh/cat/sleep/ps), Blockheads PC data dir layout and reading the
15 local worlds, adb_known_hosts.pb format (matches the local file), `adb mdns check/services` output format.
Only faked (no phone attached): every adb device interaction — devices -l parsing, dumpsys parsing, backup/
restore, bu streaming, root tar, pair/connect.

Next:
- Run `tests/real_device.rs` with a phone: confirm `adb backup -f` tailing, whether `exec-out bu backup` works
  (then maybe make it default: no temp file), what a declined backup really produces (0 bytes vs header).
- Root on a real device: confirm saves path, toybox `tar` availability (Android 6 may lack it), `ls -Z` format.

Open questions:
- Declined vs broken: both can produce 0 bytes on some Android versions; we report `DeclinedOnPhone` for 0 bytes,
  `EmptyBackup` for header-only/empty tar.
- `adb restore` gives no signal when the user declines; callers should re-read the device to verify.
- Restore via `bu` stream: when the phone never confirms, bytes already sit in the pipe buffer, so it surfaces
  as `Disconnected` (stall) rather than `ConfirmTimeout`.

## 2026-11-03: never force-stop before a backup (`fix/device-stop`)

Verified on an Android 11 emulator: `adb backup` skips an app in Android's "stopped" state (after `am force-stop`, or
never launched since install) and returns a 47-byte header-only file (logcat: "Package ... is not eligible for
backup, removing").
- `Adb::stop_game` now sends `input keyevent HOME`, then `am kill` (does not set the stopped flag) and re-checks
  `pidof`; still running ⇒ `DeviceError::GameRunning`. `Adb::force_stop_game` keeps `am force-stop` and is used only
  by `restore` with `IfRunning::Stop` (the restore wipes the data anyway). `check_running` (backup, root) is safe.
- `GameInfo.stopped` parsed from the same `dumpsys package` round trip (`stopped=` of the first `User` line).
  `backup` checks it first and fails with `DeviceError::GameStopped`; `hints::hint_for_error` maps it to
  `Hint::OpenGameOnce` ("Open The Blockheads once on the phone, go back to the home screen, then try again").
  `check_backup`'s `EmptyBackup` stays for other cases.
- Tests: `tests::phone()` scripts a non-stopped game for backup tests; line coverage 100%.

## 2026-11-03 (core stream, emulator)
- `stop_game` retries `am kill` every 500 ms for `Timeouts::stop` (new field, default 30 s): the game stays in
  Android's "previous app" state for several seconds after HOME and `am kill` ignores it meanwhile.
- Backup/restore via `adb backup -f` / `adb restore` verified on the API 30 emulator through bhw-core's
  `tests/real_device.rs` (confirm screens tapped by the test).
