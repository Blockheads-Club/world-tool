# Progress logs

One file per work stream. Each stream appends dated entries: what is done, what is next, open questions.
After an interruption (machine asleep), read the stream's file and `git log` on its branch to resume.

| Stream | Branch | Worktree | File |
| --- | --- | --- | --- |
| LMDB 32/64 reader+writer | `feat/lmdb` | `../world-tool-wt/lmdb` | `lmdb.md` |
| Android backup (.ab) + world files | `feat/backup` | `../world-tool-wt/backup` | `backup.md` |
| Desktop app UI (Phase 1) | `feat/app` | `../world-tool-wt/app` | `app.md` |
| Save contents (client schema, metadata, client↔server) | `feat/save` | `../world-tool-wt/save` | `save.md` |
| Devices (adb, root, Wi-Fi, Blockheads PC) | `feat/device` | `../world-tool-wt/device` | `device.md` |
| APK game files (validate, extract) | `feat/assets` | `../world-tool-wt/assets` | `assets.md` |
| iOS backups (read-only) | `feat/ios` | `../world-tool-wt/ios` | `ios.md` |
| Core service + Tauri wiring | `feat/core` | `../world-tool-wt/core` | `core.md` |
| App logic + Tauri wiring | `feat/core` | `../world-tool-wt/core` | `core.md` |
| World viewer/editor (Phase 2) | `feat/editor` | `../world-tool-wt/editor` | `editor.md` |

lmdb: done, merged into dev (8a70b84).
backup: done, merged into dev (1171c7e).
device: done, merged into dev (b3bb380); adb faked in tests, real-phone run pending (`device.md`).
core: done, merged into dev (bhw-core + Tauri wiring; emulator E2E passes).
