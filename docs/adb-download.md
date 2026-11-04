# adb: downloaded on first run, never shipped

BH World Tool talks to phones through Google's `adb` client (see `docs/progress/device.md` for why the binary and
not a Rust adb library). The Android SDK Platform-Tools license does not allow redistributing it inside our app,
so **no adb is bundled**. Instead:

1. At start `bhw_core::adb_tools::locate(<app data dir>)` looks for adb in this order:
   - `<app data dir>/platform-tools/adb[.exe]` (an earlier download by this app);
   - `$ANDROID_HOME/platform-tools`, `$ANDROID_SDK_ROOT/platform-tools`;
   - every `PATH` entry.
2. When none is found, the Devices screen shows "Phone tool needed" and an `adb_missing` error opens the same
   dialog. The dialog (`AdbDialog.svelte`) first asks Google for the size (`adb_download_info`, an HTTP HEAD) and
   shows the source URL, the size, the destination folder and Google's terms page; nothing is downloaded before the
   user clicks **Download**.
3. `download_adb` fetches Google's stable URL for the OS:
   `https://dl.google.com/android/repository/platform-tools-latest-{windows,darwin,linux}.zip`
   (progress, cancel; at most 200 MB downloaded, 200 MB unpacked). Only entries below `platform-tools/` are
   unpacked (path-traversal safe), Unix exec bits are kept, into a staging folder; then `adb version` must run, and
   only then the staging folder replaces `<app data dir>/platform-tools/`. The core switches to the new adb at once
   (no restart).

| OS | App data dir (Tauri `app_data_dir`) |
| --- | --- |
| Windows | `%APPDATA%\club.blockheads.worldtool\platform-tools\` (`adb.exe`, `AdbWinApi.dll`, `AdbWinUsbApi.dll`) |
| macOS | `~/Library/Application Support/club.blockheads.worldtool/platform-tools/` |
| Linux | `~/.local/share/club.blockheads.worldtool/platform-tools/` |

Code: `crates/bhw-core/src/adb_tools.rs` (logic, behind the `Fetch` trait; tests use a fake server, no network),
`app/src-tauri/src/lib.rs` (`HttpFetch`: blocking `reqwest` with rustls), commands `adb_status`,
`adb_download_info`, `download_adb`.

Notes:
- Google publishes no checksum for the `-latest` zips; integrity comes from HTTPS, the zip CRCs and the `adb version`
  check.
- A downloaded adb may fight with another adb version over the server port 5037
  (`hints::is_server_version_conflict`); the Troubleshooting guide offers "Restart adb".
- Linux users still need udev rules for USB (the device hints say so); Windows needs the phone's USB driver.
- Settings → Connection shows where adb was found and offers the download while it is missing.
