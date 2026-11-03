# APK game files stream (`crates/bhw-assets`)

Branch `feat/assets`. Build with `CARGO_TARGET_DIR=/home/kahverengi/Projects/blockheads-club/world-tool/target`.
Research results: `docs/game-files.md`.

## 2026-11-03

Done:
- `axml`: read-only binary manifest parser (UTF-8/UTF-16 pools, resource-map id matching for obfuscated names,
  string/int typed values) → package, versionName, versionCode, split.
- `apk`: `ApkSet::open` / `inspect` for `.apk`, `.apks`/`.xapk` (nested APKs stored → read in place, deflated →
  anonymous temp file) and folders of split APKs; errors `NotAnApk`, `WrongApp`, `Unsupported`, `Damaged`,
  `Incomplete`. Selection rule `game_file_rel`: `.png` + `.fnt` under `assets/GameResources/` except
  `instructions/` (461 files, 22.8 MB on 1.7.4–1.7.6).
- `store::GameFiles`: extract with progress/cancel into `.tmp-*` → rename to `<versionName>-<sha12>/`, `current`
  pointer, `manifest.json` (per-file size/sha256/PNG dims), `status` (NotSet / Ready / Unsupported / Damaged /
  MissingSource), `reextract`, `clear`, `file(rel)`.
- Gated real test `tests/real_apk.rs` (`BHW_APK`, `BHW_APK_DIR`): all three user APKs pass; other apps' APKs in the
  folder are rejected with `WrongApp`.
- Coverage `cargo llvm-cov -p bhw-assets`: 100 % lines (1119/1119), 100 % functions, 97.3 % regions. Progress
  callbacks are `&mut dyn FnMut` and copies use `dyn Read/Write` so no uncovered generic instantiations remain.

Next (other streams):
- Tauri commands + settings UI ("Game files from your APK": pick file/folder, progress, cancel, status, clear).
- Tile type → TileMap cell table for the editor (needs clean-room observation, see `docs/game-files.md`).

Open questions:
- Should the cache keep several versions at once? Currently only the active extraction is kept.
