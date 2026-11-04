import { Channel, invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import type {
  Activity,
  AdbDownload,
  AdbStatus,
  AppInfo,
  Backup,
  Device,
  DeviceWorld,
  ExportRequest,
  ExportResult,
  GameFiles,
  ImportItem,
  ImportRequest,
  IosBackup,
  PairedDevice,
  Progress,
  ReadRequest,
  ReadResult,
  Settings,
  Source,
  TransferPlan,
  TransferRequest,
  TransferResult,
  Unlisten,
  UpdateInfo,
  WifiCandidate,
  World,
  Catalog,
  EditorBlockhead,
  EditorState,
  EditorTile,
  EditorWorld,
  EditorObject,
  EditRequest,
  EditResult,
  HistoryResult,
  Rect,
  SchematicInfo,
  SearchKind,
  TileMatch
} from './types';

export type OnProgress = (p: Progress) => void;

/**
 * Everything the UI asks of the backend. Long operations take an `op` id (made by the UI, used by `cancel`) and an
 * `onProgress` callback (a Tauri `Channel`); they resolve with the result or reject with an `AppError`.
 * Command names (snake_case) are listed in `COMMANDS` and in `src-tauri/src/lib.rs`.
 */
export interface Backend {
  appInfo(): Promise<AppInfo>;
  getSettings(): Promise<Settings>;
  setSettings(patch: Partial<Settings>): Promise<Settings>;

  // Devices and connection guides
  listDevices(): Promise<Device[]>;
  /** Called when a device appears, goes away or changes status. */
  onDevicesChanged(cb: (devices: Device[]) => void): Promise<Unlisten>;
  wifiDiscover(): Promise<WifiCandidate[]>;
  wifiPair(address: string, code: string): Promise<Device>;
  pairedDevices(): Promise<PairedDevice[]>;
  forgetPaired(id: string): Promise<void>;
  restartAdb(): Promise<void>;
  /** Whether adb was found (Android SDK, PATH or an earlier download). */
  adbStatus(): Promise<AdbStatus>;
  /** Source and size of Google's Platform-Tools download (asks the server). */
  adbDownloadInfo(): Promise<AdbDownload>;
  /** Downloads Platform-Tools into the app's data folder and starts using its adb. */
  downloadAdb(op: string, onProgress: OnProgress): Promise<AdbStatus>;
  openDeviceManager(): Promise<void>;
  iosBackups(folder: string | null): Promise<IosBackup[]>;
  /** Opens (decrypts) an iPhone backup; it then shows up as a read-only device. */
  iosOpen(id: string, password: string): Promise<Device>;
  diagnostics(): Promise<string>;

  // Read worlds (WT-Export)
  read(op: string, req: ReadRequest, onProgress: OnProgress): Promise<ReadResult>;
  /** Save worlds of a read to Worlds without exporting. */
  saveToLibrary(readId: string, worldIds: string[]): Promise<World[]>;
  exportWorlds(op: string, req: ExportRequest, onProgress: OnProgress): Promise<ExportResult>;
  freeSpace(folder: string): Promise<number>;
  cancel(op: string): Promise<void>;

  // Transfer (WT-Import)
  /** Worlds a source offers (rejects `not_found` for a phone that was not read yet). */
  sourceWorlds(source: Source): Promise<DeviceWorld[]>;
  transferPlan(source: Source, worldIds: string[], targetId: string): Promise<TransferPlan>;
  transfer(op: string, req: TransferRequest, onProgress: OnProgress): Promise<TransferResult>;

  // Worlds (WT-Library)
  listWorlds(): Promise<World[]>;
  renameWorld(id: string, name: string): Promise<World>;
  duplicateWorld(id: string): Promise<World>;
  deleteWorld(id: string): Promise<void>;
  showInFolder(path: string): Promise<void>;
  /** Paths: dropped or picked `.bhworld` files and server world folders. */
  inspectImport(paths: string[]): Promise<ImportItem[]>;
  importWorlds(req: ImportRequest): Promise<World[]>;

  // Backups
  listBackups(): Promise<Backup[]>;
  restoreBackup(op: string, backupId: string, deviceId: string, onProgress: OnProgress): Promise<void>;
  deleteBackup(id: string): Promise<void>;

  // Activity
  listActivity(): Promise<Activity[]>;
  clearActivity(): Promise<void>;

  // Game files (the user's APK)
  gameFiles(): Promise<GameFiles>;
  useApk(op: string, path: string, onProgress: OnProgress): Promise<GameFiles>;
  apkFromDevice(op: string, deviceId: string, onProgress: OnProgress): Promise<GameFiles>;
  reextract(op: string, onProgress: OnProgress): Promise<GameFiles>;
  clearGameFiles(): Promise<GameFiles>;

  // Pickers (native dialogs; null when cancelled)
  pickFolder(start: string | null): Promise<string | null>;
  pickImport(folders: boolean): Promise<string[]>;
  pickApk(): Promise<string | null>;

  // Updates
  checkUpdate(): Promise<UpdateInfo | null>;
  downloadUpdate(op: string, onProgress: OnProgress): Promise<void>;
  restartApp(): Promise<void>;

  // World editor (Phase 2)
  /** Opens a Worlds world as a working copy. */
  editorOpen(id: string): Promise<EditorWorld>;
  editorClose(session: string): Promise<void>;
  /**
   * Tile data of macro blocks `[x0, y0, x1, y1)`: per block `u16 x, u8 y, u8 state` (0 none, 1 stored, 2 damaged),
   * then 1024 × 4 bytes (fg, back wall, contents, light) when stored.
   */
  editorRegion(session: string, rect: [number, number, number, number]): Promise<ArrayBuffer>;
  /** The whole world in plain colours: `u32 width, u32 height`, then RGBA, top row first. */
  editorOverview(session: string, scale: number): Promise<ArrayBuffer>;
  editorTile(session: string, x: number, y: number): Promise<EditorTile>;
  editorCatalog(): Promise<Catalog>;
  /** The user's own atlas PNG (`tiles` or `items`); empty when the game files are not set up. */
  editorAtlas(which: 'tiles' | 'items'): Promise<ArrayBuffer>;
  editorState(session: string): Promise<EditorState>;
  editorBlockheads(session: string): Promise<EditorBlockhead[]>;
  editorEdit(session: string, req: EditRequest): Promise<EditResult>;
  /** Tiles matching `target` in `area` (whole world if null): up to `limit` positions and the total count. */
  editorFind(session: string, target: TileMatch, area: Rect | null, limit: number): Promise<[[number, number][], number]>;
  /** Undo / redo until `to` steps are applied. */
  editorHistory(session: string, to: number): Promise<HistoryResult>;
  /** Tiles changed by the applied steps (compare view). */
  editorChanges(session: string): Promise<[number, number][]>;
  /** Saves into Worlds (back into the same world, or `asNew`). */
  editorSave(session: string, asNew: boolean): Promise<World>;
  editorSearch(session: string, kind: SearchKind, text: string): Promise<EditorObject[]>;
  editorSchematics(): Promise<SchematicInfo[]>;
  /** Saves a rectangle (or the clipboard when `rect` is null) as a schematic file. */
  editorSaveSchematic(session: string, rect: Rect | null, name: string): Promise<SchematicInfo>;
  editorUseSchematic(session: string, file: string): Promise<EditorState>;
  editorImportSchematic(path: string): Promise<SchematicInfo>;
  editorExportSchematic(file: string, dest: string): Promise<void>;
  editorDeleteSchematic(file: string): Promise<void>;
  /** File dialog for schematics: open (import) or save (export, suggested `name`). */
  pickSchematic(save: boolean, name: string | null): Promise<string | null>;

  /** Files given to the app by the OS (double-clicked `.bhworld`), at start-up and while running. */
  onOpenFiles(cb: (paths: string[]) => void): Promise<Unlisten>;
  /** Files dragged over the window: `over` with the item count, `drop` with the paths, `leave`. */
  onDragDrop(cb: (e: DragDrop) => void): Promise<Unlisten>;
}

export type DragDrop = { type: 'over'; count: number } | { type: 'drop'; paths: string[] } | { type: 'leave' };

function channel(onProgress: OnProgress): Channel<Progress> {
  const ch = new Channel<Progress>();
  ch.onmessage = onProgress;
  return ch;
}

/** The real backend: Tauri commands of `src-tauri`. */
export const tauriBackend: Backend = {
  appInfo: () => invoke('app_info'),
  getSettings: () => invoke('get_settings'),
  setSettings: (patch) => invoke('set_settings', { patch }),

  listDevices: () => invoke('list_devices'),
  onDevicesChanged: (cb) => listen<Device[]>('devices-changed', (e) => cb(e.payload)),
  wifiDiscover: () => invoke('wifi_discover'),
  wifiPair: (address, code) => invoke('wifi_pair', { address, code }),
  pairedDevices: () => invoke('paired_devices'),
  forgetPaired: (id) => invoke('forget_paired', { id }),
  restartAdb: () => invoke('restart_adb'),
  adbStatus: () => invoke('adb_status'),
  adbDownloadInfo: () => invoke('adb_download_info'),
  downloadAdb: (op, onProgress) => invoke('download_adb', { op, onProgress: channel(onProgress) }),
  openDeviceManager: () => invoke('open_device_manager'),
  iosBackups: (folder) => invoke('ios_backups', { folder }),
  iosOpen: (id, password) => invoke('ios_open', { id, password }),
  diagnostics: () => invoke('diagnostics'),

  read: (op, req, onProgress) => invoke('read_worlds', { op, req, onProgress: channel(onProgress) }),
  saveToLibrary: (readId, worldIds) => invoke('save_to_library', { readId, worldIds }),
  exportWorlds: (op, req, onProgress) => invoke('export_worlds', { op, req, onProgress: channel(onProgress) }),
  freeSpace: (folder) => invoke('free_space', { folder }),
  cancel: (op) => invoke('cancel', { op }),

  sourceWorlds: (source) => invoke('source_worlds', { source }),
  transferPlan: (source, worldIds, targetId) => invoke('transfer_plan', { source, worldIds, targetId }),
  transfer: (op, req, onProgress) => invoke('transfer', { op, req, onProgress: channel(onProgress) }),

  listWorlds: () => invoke('list_worlds'),
  renameWorld: (id, name) => invoke('rename_world', { id, name }),
  duplicateWorld: (id) => invoke('duplicate_world', { id }),
  deleteWorld: (id) => invoke('delete_world', { id }),
  showInFolder: (path) => invoke('show_in_folder', { path }),
  inspectImport: (paths) => invoke('inspect_import', { paths }),
  importWorlds: (req) => invoke('import_worlds', { req }),

  listBackups: () => invoke('list_backups'),
  restoreBackup: (op, backupId, deviceId, onProgress) =>
    invoke('restore_backup', { op, backupId, deviceId, onProgress: channel(onProgress) }),
  deleteBackup: (id) => invoke('delete_backup', { id }),

  listActivity: () => invoke('list_activity'),
  clearActivity: () => invoke('clear_activity'),

  gameFiles: () => invoke('game_files'),
  useApk: (op, path, onProgress) => invoke('use_apk', { op, path, onProgress: channel(onProgress) }),
  apkFromDevice: (op, deviceId, onProgress) => invoke('apk_from_device', { op, deviceId, onProgress: channel(onProgress) }),
  reextract: (op, onProgress) => invoke('reextract', { op, onProgress: channel(onProgress) }),
  clearGameFiles: () => invoke('clear_game_files'),

  pickFolder: (start) => invoke('pick_folder', { start }),
  pickImport: (folders) => invoke('pick_import', { folders }),
  pickApk: () => invoke('pick_apk'),

  checkUpdate: () => invoke('check_update'),
  downloadUpdate: (op, onProgress) => invoke('download_update', { op, onProgress: channel(onProgress) }),
  restartApp: () => invoke('restart_app'),

  editorOpen: (id) => invoke('editor_open', { id }),
  editorClose: (session) => invoke('editor_close', { session }),
  editorRegion: (session, rect) => invoke('editor_region', { session, rect }),
  editorOverview: (session, scale) => invoke('editor_overview', { session, scale }),
  editorTile: (session, x, y) => invoke('editor_tile', { session, x, y }),
  editorCatalog: () => invoke('editor_catalog'),
  editorAtlas: (which) => invoke('editor_atlas', { which }),
  editorState: (session) => invoke('editor_state', { session }),
  editorBlockheads: (session) => invoke('editor_blockheads', { session }),
  editorEdit: (session, req) => invoke('editor_edit', { session, req }),
  editorFind: (session, target, area, limit) => invoke('editor_find', { session, target, area, limit }),
  editorHistory: (session, to) => invoke('editor_history', { session, to }),
  editorChanges: (session) => invoke('editor_changes', { session }),
  editorSave: (session, asNew) => invoke('editor_save', { session, asNew }),
  editorSearch: (session, kind, text) => invoke('editor_search', { session, kind, text }),
  editorSchematics: () => invoke('editor_schematics'),
  editorSaveSchematic: (session, rect, name) => invoke('editor_save_schematic', { session, rect, name }),
  editorUseSchematic: (session, file) => invoke('editor_use_schematic', { session, file }),
  editorImportSchematic: (path) => invoke('editor_import_schematic', { path }),
  editorExportSchematic: (file, dest) => invoke('editor_export_schematic', { file, dest }),
  editorDeleteSchematic: (file) => invoke('editor_delete_schematic', { file }),
  pickSchematic: (save, name) => invoke('pick_schematic', { save, name }),

  onOpenFiles: async (cb) => {
    const off = await listen<string[]>('open-files', (e) => cb(e.payload));
    const first = await invoke<string[]>('take_open_files');
    if (first.length) cb(first);
    return off;
  },
  onDragDrop: (cb) =>
    getCurrentWindow().onDragDropEvent((e) => {
      const p = e.payload;
      if (p.type === 'enter') cb({ type: 'over', count: p.paths.length });
      else if (p.type === 'drop') cb({ type: 'drop', paths: p.paths });
      else if (p.type === 'leave') cb({ type: 'leave' });
    })
};

/** Every Tauri command the UI calls (kept in step with `src-tauri/build.rs` by a test). */
export const COMMANDS = [
  'app_info', 'get_settings', 'set_settings',
  'list_devices', 'wifi_discover', 'wifi_pair', 'paired_devices', 'forget_paired', 'restart_adb',
  'adb_status', 'adb_download_info', 'download_adb',
  'open_device_manager', 'ios_backups', 'ios_open', 'diagnostics',
  'read_worlds', 'save_to_library', 'export_worlds', 'free_space', 'cancel',
  'source_worlds', 'transfer_plan', 'transfer',
  'list_worlds', 'rename_world', 'duplicate_world', 'delete_world', 'show_in_folder', 'inspect_import', 'import_worlds',
  'list_backups', 'restore_backup', 'delete_backup',
  'list_activity', 'clear_activity',
  'game_files', 'use_apk', 'apk_from_device', 'reextract', 'clear_game_files',
  'pick_folder', 'pick_import', 'pick_apk',
  'check_update', 'download_update', 'restart_app', 'take_open_files',
  'editor_open', 'editor_close', 'editor_region', 'editor_overview', 'editor_tile', 'editor_catalog', 'editor_atlas',
  'editor_state', 'editor_blockheads', 'editor_edit', 'editor_find', 'editor_history', 'editor_changes', 'editor_save',
  'editor_search', 'editor_schematics', 'editor_save_schematic', 'editor_use_schematic', 'editor_import_schematic',
  'editor_export_schematic', 'editor_delete_schematic', 'pick_schematic'
] as const;

/** True inside the Tauri webview. */
export function inTauri(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

/** The main window, for the custom title bar. Async so a missing Tauri runtime rejects instead of throwing. */
export const win = {
  minimize: async () => getCurrentWindow().minimize(),
  toggleMaximize: async () => getCurrentWindow().toggleMaximize(),
  close: async () => getCurrentWindow().close(),
  isMaximized: async () => getCurrentWindow().isMaximized(),
  onResized: async (cb: () => void) => getCurrentWindow().onResized(cb),
  onFocus: async (cb: (focused: boolean) => void) => getCurrentWindow().onFocusChanged((e) => cb(e.payload))
};
