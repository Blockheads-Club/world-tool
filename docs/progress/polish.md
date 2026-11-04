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
- (2) `bhw_world::library::windows_safe(stem, fallback)`: no trailing dots/spaces, reserved device names (CON, PRN,
  AUX, NUL, COM1–9, LPT1–9 incl. ¹²³, any case, also before an extension) get a `_`. Used by `slug()` (exported
  `.bhworld` files, library files, backup names; it already replaced `<>:"/\|?*`, control chars and capped at 60
  chars) and by schematic file names (`schematic_file`). The schematic save dialog gets the already-safe file name.
- (3) Update download cancellable: `download_update` registers its `op` with the core (`Core::start_op`) and races
  the updater's download against the cancel flag (`update::unless_cancelled`, tokio `select!`; dropping the
  download future stops it). The design (WT-Update) only shows the "Downloading n %" chip; clicking it during a
  download used to reopen the "available" popup (with "Update and restart" again). It now shows the download
  progress with "Cancel download" / "Hide" (same pattern as the adb download dialog). Cancelled → the update is
  offered again (chip "Update x.y.z"), no error popover.
- (5) Restore check: the UI did show "Tap “Back up my data” once more" while it happened, but nothing said so
  beforehand, and "Your phone asks twice" (undo hint) was wrong (a restore is checked too). Now: the "Before we write"
  list says the phone asks three times and why; the phone panel's badge counts "1 of 3 / 2 of 3 / 3 of 3" (copy and
  backup restore); the check text starts "Last one:"; the backup-restore dialog has the same three-taps hint (not
  for root targets); undo hint says three times.
- (4) Translations: tr reviewed by hand (messages + editor names): editor strings switched to the formal register
  the rest of the app uses, clumsy/calqued strings fixed (e.g. "Kalsın"→"Kalan" count label, "Tuzak kapı"→"Kapak",
  furnace/kiln/stove names, "Aygıtlar"→"Cihazlar", iTunes’da), see commit. ru/kk: reviewed by two subagents.
  Key/placeholder/plural sync across languages is already enforced (`unit.test.ts` + `Record<Key, string>` types).
