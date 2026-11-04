/**
 * Types shared by the UI and the backend (Tauri commands in `src-tauri`, or the in-memory mock). Field names are
 * snake_case where they cross the Tauri boundary so the Rust side can derive serde without renames.
 */

export type Os = 'windows' | 'macos' | 'linux';
export type ThemeChoice = 'system' | 'light' | 'dark';
/** `auto`: root when the phone allows it, else the backup method; `never`: always the backup method. */
export type RootMode = 'auto' | 'never';
export type Lang = 'en' | 'tr' | 'ru' | 'kk';

export interface AppInfo {
  version: string;
  os: Os;
}

// ---------------------------------------------------------------- errors

/** Every failure the backend reports. `code` picks the dialog (WT-Errors) or message; the rest fills it in. */
export type ErrorCode =
  | 'not_allowed' // adb: device unauthorized
  | 'game_not_installed'
  | 'unsupported_version'
  | 'declined' // "Do not back up" / timed out on the phone
  | 'wrong_password'
  | 'backup_empty' // the phone sent a 0-byte backup
  | 'disconnected'
  | 'disk_full'
  | 'world_damaged'
  | 'apk_wrong_app'
  | 'apk_unsupported'
  | 'apk_damaged'
  | 'apk_missing'
  | 'pair_failed'
  | 'offline'
  | 'cancelled'
  | 'not_found'
  | 'not_implemented'
  | 'io'
  | 'game_running' // Blockheads PC (or the game on the phone) is running
  | 'game_stopped' // Android "stopped" state: open the game once, then retry
  | 'not_supported_yet' // e.g. writing to an iPhone/iPad backup
  | 'adb_missing'
  | 'download_failed' // downloading Google's Platform-Tools (adb) failed
  | 'restore_declined'; // after adb restore the worlds are not on the phone ("Restore my data" not tapped)

export interface AppError {
  code: ErrorCode;
  /** Technical line for "Error details" / "Copy details" (never shown as the main text). */
  detail?: string;
  device?: string;
  /** Game version found (unsupported_version / apk_unsupported). */
  version?: string;
  /** Bytes needed / free (disk_full). */
  needed?: number;
  free?: number;
  /** Disk / drive name (disk_full). */
  disk?: string;
  /** Percent done when it broke (disconnected). */
  percent?: number;
  /** Backup that can undo a broken write. */
  backup_id?: string;
  /** File or world name (world_damaged, apk_*). */
  name?: string;
  /** How many times the password was wrong (wrong_password). */
  attempt?: number;
}

export function isAppError(e: unknown): e is AppError {
  return typeof e === 'object' && e !== null && typeof (e as AppError).code === 'string';
}

// ---------------------------------------------------------------- devices

export type DeviceKind = 'android' | 'pc' | 'ios';
export type DeviceStatus = 'ready' | 'unauthorized' | 'offline';
export type Connection = 'usb' | 'wifi' | 'local' | 'backup';

export interface Device {
  id: string;
  kind: DeviceKind;
  name: string;
  /** "Google Pixel 8 · Android 15", the PC data folder, or "iPhone 13 · iOS 17.6 backup · 12 Oct 2026". */
  model: string;
  /** Android / iOS version, if known. */
  os_version: string | null;
  status: DeviceStatus;
  connection: Connection;
  /** "192.168.1.42:37115" for Wi-Fi. */
  address: string | null;
  /** The Blockheads version installed, null when not installed or unknown. */
  game_version: string | null;
  /** Android: root shell allowed. */
  root: boolean;
  /** iOS backups are read-only. */
  read_only: boolean;
  encrypted: boolean;
  /** Unix ms when it was last seen (offline Wi-Fi devices). */
  last_seen: number | null;
  /** Worlds already known on the device (Blockheads PC is read automatically), null when it must be read first. */
  world_count: number | null;
  /** Blockheads PC data folder. */
  path: string | null;
  /** Date of the iOS backup (unix ms). */
  backup_date: number | null;
}

export interface WifiCandidate {
  name: string;
  address: string;
}

export interface PairedDevice {
  id: string;
  name: string;
  address: string;
}

export interface IosBackup {
  id: string;
  name: string;
  model: string;
  date: number;
  encrypted: boolean;
}

// ---------------------------------------------------------------- worlds

export type GameMode = 'normal' | 'expert';
/** Where a library world came from (filter chips in Worlds). */
export type Origin = 'phone' | 'pc' | 'ios' | 'file';

/** A world found on a device by a read, before it is saved to Worlds. */
export interface DeviceWorld {
  id: string;
  name: string;
  size: number;
  game_version: string;
  last_played: number | null;
  created: number;
  mode: GameMode;
  /** data: URL or asset URL of the map preview, null when there is none. */
  preview: string | null;
}

/** A world saved in Worlds (the app's own folder). */
export interface World extends DeviceWorld {
  /** 32-hex world id from the save (same id = same world). */
  world_id: string;
  origin: Origin;
  /** Device name, or the file name for imported files. */
  source: string;
  /** "USB", "Wi-Fi"… for the details panel. */
  source_detail: string | null;
  /** When it was read / imported. */
  added: number;
  /** Set right after the editor saved it (Phase 2). */
  edited: number | null;
}

export type ExportFormat = 'bhworld' | 'server';

export interface ExportedFile {
  name: string;
  size: number;
}

// ---------------------------------------------------------------- operations

/** One progress event of a long operation. `stage` names the step; `done` / `total` are bytes or items. */
export interface Progress {
  stage: Stage;
  done?: number;
  total?: number;
  /** Current item (file or world name). */
  item?: string;
  index?: number;
  count?: number;
  /** Seconds left, if known. */
  eta?: number;
  /** Seconds spent waiting for the phone. */
  elapsed?: number;
  /** File being checked (APK) and the game version found so far. */
  path?: string;
  version?: string;
}

export type Stage =
  | 'phone_backup' // waiting for "Back up my data"
  | 'phone_restore' // waiting for "Restore my data"
  | 'phone_check' // waiting for "Back up my data" of the check after a restore
  | 'verifying' // checking the worlds arrived on the phone
  | 'reading'
  | 'backing_up'
  | 'preparing'
  | 'writing'
  | 'checking'
  | 'exporting'
  | 'preview' // making a map preview
  | 'apk_checking'
  | 'apk_extracting'
  | 'apk_pulling'
  | 'downloading';

export interface ReadRequest {
  device_id: string;
  /** Phone backup password, empty for none. */
  password: string;
}

export interface ReadResult {
  read_id: string;
  worlds: DeviceWorld[];
}

export interface ExportRequest {
  /** Worlds in the library, or from a read (`read_id` + world ids). */
  read_id: string | null;
  world_ids: string[];
  format: ExportFormat;
  folder: string;
}

export interface ExportResult {
  folder: string;
  files: ExportedFile[];
  /** Worlds saved to the library on the way (export from a read). */
  saved: World[];
}

// ---------------------------------------------------------------- transfer

export type Source =
  | { kind: 'library' }
  | { kind: 'files'; paths: string[] }
  | { kind: 'device'; device_id: string };

export type Choice = 'keep_both' | 'replace' | 'skip';

export interface PlanItem {
  /** World id on the source side (library id, file path or device world id). */
  key: string;
  name: string;
  size: number;
  /** stays: already on the target; added: new; conflict: same name or same world id. */
  result: 'stays' | 'added' | 'conflict';
  conflict: 'same_name' | 'same_id' | null;
  /** Name it gets with "keep both". */
  copy_name: string | null;
}

export interface TransferPlan {
  plan_id: string;
  target: string;
  /** Worlds the target has after writing with the default choices. */
  total_after: number;
  items: PlanItem[];
  size: number;
  /** Automatic backup that is made first. */
  backup_name: string;
  backup_size: number;
  /** Writing needs "Restore my data" on the phone. */
  needs_phone: boolean;
}

export interface TransferRequest {
  plan_id: string;
  choices: Record<string, Choice>;
}

export interface TransferResult {
  target: string;
  total: number;
  added: string[];
  replaced: string[];
  /** "Sunny Isle (2)" style names that were added next to an existing world. */
  kept_both: string[];
  backup_id: string | null;
  backup_time: number | null;
}

// ---------------------------------------------------------------- import

export interface ImportItem {
  path: string;
  name: string;
  kind: 'bhworld' | 'server' | 'invalid';
  size: number;
  game_version: string | null;
  status: 'new' | 'conflict' | 'invalid';
  conflict: 'same_name' | 'same_id' | null;
  /** Name of the existing world it clashes with. */
  existing: string | null;
}

export interface ImportRequest {
  choices: Record<string, Choice>;
  paths: string[];
}

// ---------------------------------------------------------------- backups / activity

export interface Backup {
  id: string;
  device_id: string;
  device: string;
  /** Why it was made: before a write, read for export, before a restore. */
  reason: 'before_write' | 'read' | 'before_restore';
  /** Worlds written by the write it protected. */
  reason_count: number | null;
  time: number;
  worlds: number;
  size: number;
  automatic: boolean;
}

export type ActivityKind = 'read' | 'copy' | 'backup' | 'restore' | 'import' | 'export';
export type ActivityResult = 'done' | 'failed' | 'stopped';

export interface Activity {
  id: string;
  time: number;
  kind: ActivityKind;
  result: ActivityResult;
  /** Device (or "Worlds") on the main side. */
  device: string;
  /** World names involved. */
  worlds: string[];
  size: number | null;
  /** Free-form technical line (error code and adb output). */
  detail: string | null;
  error: ErrorCode | null;
}

// ---------------------------------------------------------------- game files

export type GameFilesState = 'not_set' | 'ready' | 'unsupported' | 'damaged' | 'missing';

export interface GameFiles {
  state: GameFilesState;
  apk_path: string | null;
  apk_name: string | null;
  version: string | null;
  images: number | null;
  size: number | null;
}

// ---------------------------------------------------------------- settings / updates

export interface Settings {
  first_run_done: boolean;
  worlds_folder: string;
  backup_before_write: boolean;
  backups_to_keep: number;
  root_mode: RootMode;
  theme: ThemeChoice;
  lang: Lang | null;
  reduce_motion: boolean;
  auto_update: boolean;
  /** Blockheads PC data folder, null = found automatically. */
  pc_folder: string | null;
  skipped_version: string | null;
  last_update_check: number | null;
}

export interface UpdateInfo {
  version: string;
  date: number;
  size: number;
  notes: string[];
}

// ---------------------------------------------------------------- adb (Google's Platform-Tools, downloaded on first run)

export interface AdbStatus {
  found: boolean;
  path: string | null;
  /** Downloaded by this app (into its data folder). */
  managed: boolean;
  /** Where a download comes from; null on an OS without Platform-Tools. */
  url: string | null;
  terms_url: string;
}

/** Shown before the user confirms the download. */
export interface AdbDownload {
  url: string;
  size: number | null;
  terms_url: string;
  folder: string;
}

export type Unlisten = () => void;

// ---- World editor (Phase 2) ----

/** One blockhead in the editor (`player` is `""` in single-player worlds). */
export interface EditorBlockhead {
  player: string;
  id: number;
  name: string;
  x: number;
  y: number;
  /** health, happiness, fullness, energy (0..1). */
  stats: [number, number, number, number] | null;
  /** Per slot `[item type, count]`. */
  inventory: [number, number][] | null;
}

/** A world opened in the editor (a working copy in memory). */
export interface EditorWorld {
  session: string;
  lib_id: string;
  world_id: string;
  name: string;
  game_version: string | null;
  width: number;
  height: number;
  spawn: [number, number] | null;
  /** Stored macro blocks (32 × 32 tiles). */
  blocks: [number, number][];
  damaged_blocks: number;
  expert: boolean;
  /** Unix seconds. */
  created: number | null;
  time_of_day: number | null;
  server: boolean;
  blockheads: EditorBlockhead[];
}

export interface EditorObject {
  kind: number;
  kind_name: string | null;
  id: number | null;
  x: number;
  y: number;
  text: string | null;
}

export interface EditorTile {
  x: number;
  y: number;
  stored: boolean;
  fg: number;
  bg: number;
  contents: number;
  light: number;
  sun_light: number;
  paint: number[];
  owner: number;
  protected: boolean;
  objects: EditorObject[];
}

/** Name, plain colour and atlas cell of one id. */
export interface IdInfo {
  id: number;
  name: string;
  color?: [number, number, number];
  cell?: [number, number];
  placeable?: boolean;
}

export interface Catalog {
  tiles: IdInfo[];
  contents: IdInfo[];
  items: IdInfo[];
}

export type OpKind =
  | 'pencil' | 'brush' | 'remove' | 'fill' | 'clear' | 'replace' | 'paste' | 'cut' | 'blockhead' | 'inventory' | 'settings';

export interface HistoryLabel {
  kind: OpKind;
  tiles: number;
  a?: string;
  b?: string;
}

export interface EditorState {
  history: HistoryLabel[];
  cursor: number;
  unsaved: number;
  clipboard: [number, number] | null;
}
