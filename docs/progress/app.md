# Desktop app UI (Phase 1) — progress

## 2026-11-03 — Phase 1 UI complete on the mock backend

### Done
- `app/` SvelteKit 2 + Svelte 5 + TS (adapter-static), versions as blockheads-pc. Sunlit Blocks tokens in
  `src/app.css`; icons from the design system (`src/lib/icons.ts`); app / small / macOS / `.bhworld` icons in `static/`
  and `src-tauri/icons/` (generated with `bunx tauri icon`).
- Backend contract: `src/lib/types.ts` + `src/lib/api.ts` (`Backend` interface, `tauriBackend` via `invoke` + `Channel`
  for progress, `COMMANDS` list). In-memory `MockBackend` (`src/lib/mock-api.ts`) used by `bun run dev`, unit tests
  and e2e; dev URL options `?scenario=empty|fresh&tick=50&fail=read:declined&update=0.3.0`.
- State: `store.svelte.ts` + flows (`flows/read|transfer|import|gamefiles|update.svelte.ts`).
- Screens (all design states implemented, see below), i18n en/tr/ru/kk (703 keys each; tr/ru/kk translated by
  agents — Android system wording not verified on real devices), light/dark/system theme, reduce-motion switch
  (+ OS setting), keyboard (radio groups with arrows, menus, focus-trapping dialogs), live regions.
- `src-tauri/`: own `[workspace]`, identifier `club.blockheads.worldtool`, productName "BH World Tool",
  undecorated window (custom title bar), `.bhworld` file association, ACL (`permissions/app-commands.toml`,
  `build.rs` lists every command). Real: `app_info`, `get_settings`/`set_settings` (JSON in app config dir),
  `take_open_files` (CLI args; macOS `Opened` event → `open-files`), `restart_app`. Empty lists for
  `list_devices|list_worlds|list_backups|list_activity|paired_devices|ios_backups`, `game_files` = not_set.
  Everything else rejects `{ code: "not_implemented" }`. 4 Rust unit tests; `cargo clippy --all-targets -D warnings`
  clean (CARGO_TARGET_DIR=../world-tool/target).
- Tests: 90 vitest tests (unit, api, mock, every screen), **100 % statements/branches/functions/lines**;
  `bun run check` clean; 2 Playwright e2e smoke tests (`PW_PORT=4319 bunx playwright test`).

### Design states implemented
- FirstRun ✓ · Devices: devices / looking for devices / no device ✓ (language tweak via Settings)
- Connect: USB, Wi-Fi pairing, Rooted Android, Blockheads PC, iPhone backup, Windows USB driver, Troubleshooting ✓
- Export (read worlds): close the game, look at your phone, reading, choose worlds (list/grid), export options,
  exporting, done ✓
- Import (transfer): choose from and to, review (keep both/replace/skip), before writing, look at your phone
  (backup + restore), writing, done, iPhone as destination ✓
- Library: worlds, world details, import world, drop files, after saving an edit (notice; reachable only via
  `store.savedEdit` until Phase 2), empty, loading ✓
- GameFiles: ask, dragging the APK, checking, extracting, ready, not The Blockheads, unsupported, damaged, missing ✓
  (Worlds → Edit → this step → editor placeholder screen)
- Backups: backups, restore a backup (with phone steps), empty ✓ · Activity: history, empty ✓
- Settings: settings, update available, game files not set, game files unsupported ✓ (+ paired devices dialog,
  licences, legal notice)
- Errors: all 9 dialogs ✓ · Update: popup, chip, downloading, ready, transfer running, up to date, check failed ✓
- Small (1024×640): icon rail sidebar, one-column grids ✓ · Editor: Phase 2 (placeholder only)

### Next
1. Wire `src-tauri` commands to `crates/bhw-*` (adb/root/PC/iOS device layer, read/transfer/export/import,
   backups, activity log, APK check/extract) and add `tauri-plugin-dialog` for the pickers, an updater
   (`tauri-plugin-updater`), `show_in_folder` (opener). Emit `devices-changed` events.
2. Rust coverage for `src-tauri` (cargo llvm-cov) once logic lands there (currently 4 tests, no gate).
3. CI workflow for the app (bun install, check, test, e2e, clippy) — not added yet.

### Open questions
- Real-device check of tr/ru/kk Android system strings ("Back up my data" etc.).
- Default folders (Worlds folder = Documents/BH World Tool; Blockheads PC data path) per OS.
- Map previews: backend sends `preview` as data URL or asset URL (CSP allows `data:` and `asset:`).
