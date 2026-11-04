# Shipping adb with the app

BH World Tool talks to phones through Google's `adb` client (see `docs/progress/device.md` for why the binary and
not a Rust adb library). `bhw_device::Adb::locate(sidecar)` looks for it in this order:

1. the bundled sidecar: `adb` / `adb.exe` **next to the app executable** (`app/src-tauri/src/lib.rs`,
   `sidecar_adb()`);
2. `$ANDROID_HOME/platform-tools`, `$ANDROID_SDK_ROOT/platform-tools`;
3. every `PATH` entry.

When none is found the core reports `adb_missing` and lists only Blockheads PC / iPhone backups.

## Packaging (release CI; no binaries are committed)

`scripts/fetch-adb.sh <target triple> <out.json>` (run by `.github/workflows/release.yml` before `tauri build`)
downloads the pinned SDK Platform-Tools zip (`platform-tools_r37.0.1-{win,darwin,linux}.zip` from
`dl.google.com/android/repository/`; license: Android SDK license, redistributable as part of an app), checks its
SHA-256 against the value in the script, copies the files into the ignored `app/src-tauri/binaries/` and writes a
Tauri config fragment that the build merges in with `--config`:

| OS | Files | Tauri config |
| --- | --- | --- |
| Windows | `adb.exe`, `AdbWinApi.dll`, `AdbWinUsbApi.dll`, `libwinpthread-1.dll` | `externalBin: ["binaries/adb"]` (file `binaries/adb-x86_64-pc-windows-msvc.exe`), the DLLs as `resources` (next to the exe) |
| macOS | `adb` (universal in the zip) | `externalBin`, file `binaries/adb-<aarch64\|x86_64>-apple-darwin` per build |
| Linux | `adb` | `externalBin` for the **AppImage only**: a deb/rpm sidecar would be `/usr/bin/adb` and clash with the distro's adb package, so deb/rpm users install adb from their distro (found on `PATH`) |

Every bundle also carries the Platform-Tools `NOTICE.txt` as resource `adb-NOTICE.txt`. To move to a newer
Platform-Tools, change `version` and the three checksums in the script (`sha256sum` of the downloaded zips).

Tauri strips the target-triple suffix when it installs an `externalBin`, so at run time the file is
`<exe dir>/adb[.exe]`, which is what `sidecar_adb()` returns. `externalBin` stays out of `tauri.conf.json` (local
builds and CI checks have no files); only the release build adds it.

Notes:
- A bundled adb may fight with another adb version over the server port 5037 (`hints::is_server_version_conflict`);
  the Troubleshooting guide offers "Restart adb".
- Linux users still need udev rules for USB (the device hints say so); Windows needs the phone's USB driver.
