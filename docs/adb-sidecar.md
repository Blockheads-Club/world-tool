# Shipping adb with the app

BH World Tool talks to phones through Google's `adb` client (see `docs/progress/device.md` for why the binary and
not a Rust adb library). `bhw_device::Adb::locate(sidecar)` looks for it in this order:

1. the bundled sidecar: `adb` / `adb.exe` **next to the app executable** (`app/src-tauri/src/lib.rs`,
   `sidecar_adb()`);
2. `$ANDROID_HOME/platform-tools`, `$ANDROID_SDK_ROOT/platform-tools`;
3. every `PATH` entry.

When none is found the core reports `adb_missing` and lists only Blockheads PC / iPhone backups.

## Packaging (not done yet; no binaries are committed)

The release workflow downloads the official SDK Platform-Tools zip for each OS from
`https://dl.google.com/android/repository/platform-tools-latest-{windows,darwin,linux}.zip` (license: Android SDK
license, redistributable as part of an app), verifies its checksum, and places the files before `tauri build`:

| OS | Files | Tauri config |
| --- | --- | --- |
| Windows | `adb.exe`, `AdbWinApi.dll`, `AdbWinUsbApi.dll` | `bundle.externalBin: ["binaries/adb"]` (file `binaries/adb-x86_64-pc-windows-msvc.exe`), the two DLLs as `bundle.resources` copied next to the exe |
| macOS | `adb` (universal) | `externalBin` with `binaries/adb-aarch64-apple-darwin` and `-x86_64-apple-darwin`; signed and notarised with the app |
| Linux | `adb` | `externalBin` `binaries/adb-x86_64-unknown-linux-gnu`; AppImage/deb place it next to the exe |

Tauri strips the target-triple suffix when it installs an `externalBin`, so at run time the file is
`<exe dir>/adb[.exe]`, which is what `sidecar_adb()` returns. `externalBin` must not be added to
`tauri.conf.json` before the CI step exists (the build fails without the files).

Notes:
- A bundled adb may fight with another adb version over the server port 5037 (`hints::is_server_version_conflict`);
  the Troubleshooting guide offers "Restart adb".
- Linux users still need udev rules for USB (the device hints say so); Windows needs the phone's USB driver.
