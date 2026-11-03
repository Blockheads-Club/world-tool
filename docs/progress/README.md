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

lmdb: done, merged into dev (8a70b84).
<<<<<<< HEAD
backup: done, merged into dev (1171c7e).
=======
device: crate done on `feat/device` (faked adb; real-phone run pending), see `device.md`.
>>>>>>> feat/device
