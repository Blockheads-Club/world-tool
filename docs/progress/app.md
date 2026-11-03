# Desktop app UI (Phase 1) — progress

## 2026-11-03 (WIP, not yet compiling)
Done (files under `app/`):
- Scaffold: package.json (versions as blockheads-pc), vite (100 % coverage thresholds), svelte.config, tsconfig,
  app.html, app.css (Sunlit Blocks tokens + design classes), icons (static/wt-icon*.svg, bhworld-file.svg from
  WT-System), fonts.
- `src/lib/types.ts`, `src/lib/api.ts` (Backend interface, tauriBackend via invoke + Channel, COMMANDS list),
  `src/lib/mock-api.ts` (MockBackend, scenarios full/empty/fresh, `fail` injection, `mockFromUrl`).
- i18n.svelte.ts, theme.svelte.ts, format.ts, util.ts, errors.ts, icons.ts, store.svelte.ts,
  flows/{read,transfer,import,gamefiles,update}.svelte.ts.
- UI: Icon, Button, Badge, Toggle, Toasts, Seg, Check, Notice, Progress, Steps, Empty, Dialog, Menu, Phone,
  PhoneWait, Thumb. Layout: TitleBar, Sidebar, Header. App.svelte, routes.
- Screens: FirstRun, LegalDialog, Devices, DeviceRow, Connect, ReadWorlds, Transfer, Worlds, ImportDialog,
  DropOverlay, EditorPlaceholder, GameFilesDialog.

Next (exact):
1. Write screens: ErrorDialog (9 WT-Errors codes), UpdateUi (popup, wait dialog, popover), Backups (+restore dialog,
   store.restoreId), Activity, Settings (states: settings / update available / game files not set / unsupported,
   paired devices dialog, licences/legal).
2. Write `messages.ts` (en, export `Key`): grep all `t('…')` and `as Key` template keys in src; then tr/ru/kk
   (delegate translations to subagents).
3. `bun run check` clean; tests (component tests per state, mock tests, i18n completeness); coverage 100 %.
4. `src-tauri` minimal shell (own [workspace], stub commands returning not_implemented, .bhworld association,
   identifier club.blockheads.worldtool); clippy with CARGO_TARGET_DIR=../world-tool/target.
