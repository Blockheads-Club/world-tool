# iOS backups (bhw-ios) — progress

Branch `feat/ios`, worktree `../world-tool-wt/ios`. Read-only access to The Blockheads worlds inside iPhone/iPad
backups made by Finder/iTunes (unencrypted and encrypted). Format notes: `docs/ios-backup.md`.

## 2026-11-03
- Started: crate skeleton `crates/bhw-ios` (rusqlite bundled + serialize, plist, aes, pbkdf2, sha1, sha2).
- Next: crypto (RFC 3394, CBC), keybag, MBFile, backup listing, Manifest.db, Blockheads extraction, docs.
