# Polish (feat/polish)

Worktree `../world-tool-wt/polish`, branch `feat/polish` (from `dev`).

Tasks: (1) real Blockheads PC test, (2) Windows-safe file names, (3) cancellable update download, (4) translation
review tr/ru/kk, (5) restore-check wording.

## 2026-11-05

- (1) Blockheads PC on this machine: data dir `~/.local/share/blockheads-pc` (from `launcher.toml` `data_dir` in
  `~/.config/blockheads-pc`), saves in `home/Library/Application Support/saves`, `game_db` next to it: matches
  `bhw-device::pc`. The AppImage's game process is `usr/bin/bhpc-run` (matches `GAME_PROCESS`).
  Found on the real data: a multiplayer cache world ("TEST", `remoteGame`) counted in the PC card's world count
  but (correctly) not listed by a read → count now skips `remoteGame` entries (`GameDb::is_remote`); a leftover
  world folder (no `main/worldv2`, not in `game_db`; the game does not list it) was listed as "World" and failed
  to copy (`world_damaged`) → reads now skip folders with no world record at all (`inspect_record`).
  New env-gated test `crates/bhw-core/tests/real_pc.rs` (`BHW_PC_DIR`): copies `home/` to a temp dir, runs read,
  library, `.bhworld` export, copy in (keep both), editor edit + save + copy over, restore of the automatic backup
  (LMDB contents equal to the start), and checks the real folder is byte-identical afterwards. Passes. With
  `BHW_PC_KEEP=<dir>` the edited copy was opened in the real game headless (AppImage's `bhpc-run --data-dir`,
  Xvfb, scripted clicks): "SUNSET VALLEYS (2)" is in the world list and loads with the edited stone platform.
  Real data verified unchanged by sha256 (131 files) + config + APK.
