import { Channel, invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import type {
  Activity,
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
  World
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
  'open_device_manager', 'ios_backups', 'ios_open', 'diagnostics',
  'read_worlds', 'save_to_library', 'export_worlds', 'free_space', 'cancel',
  'source_worlds', 'transfer_plan', 'transfer',
  'list_worlds', 'rename_world', 'duplicate_world', 'delete_world', 'show_in_folder', 'inspect_import', 'import_worlds',
  'list_backups', 'restore_backup', 'delete_backup',
  'list_activity', 'clear_activity',
  'game_files', 'use_apk', 'apk_from_device', 'reextract', 'clear_game_files',
  'pick_folder', 'pick_import', 'pick_apk',
  'check_update', 'download_update', 'restart_app', 'take_open_files'
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
