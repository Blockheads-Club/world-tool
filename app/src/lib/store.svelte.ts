import type { Backend } from './api';
import { detectLang, setLang, t } from './i18n.svelte';
import { setReduceMotion, setTheme } from './theme.svelte';
import type { AppError, AppInfo, Device, ErrorCode, GameFiles, Settings, World } from './types';
import { toAppError } from './errors';
import { toast } from './ui/toast.svelte';
import { ReadFlow } from './flows/read.svelte';
import { TransferFlow } from './flows/transfer.svelte';
import { ImportFlow } from './flows/import.svelte';
import { GameFilesFlow } from './flows/gamefiles.svelte';
import { UpdateFlow } from './flows/update.svelte';
import { AdbFlow } from './flows/adb.svelte';

export type Screen = 'devices' | 'connect' | 'read' | 'transfer' | 'worlds' | 'backups' | 'activity' | 'settings' | 'editor';
export type Guide = 'usb' | 'wifi' | 'root' | 'pc' | 'ios' | 'driver' | 'trouble';

/** An error shown as a dialog (WT-Errors), with what "Try again" / the main action does. */
export interface ShownError {
  error: AppError;
  /** Main action; absent = only "Close". */
  retry?: () => void;
}

/** Error codes that are shown as a dialog; the rest become a toast. */
const DIALOG_CODES: ErrorCode[] = [
  'not_allowed', 'game_not_installed', 'unsupported_version', 'declined', 'wrong_password', 'backup_empty',
  'disconnected', 'disk_full', 'world_damaged', 'restore_declined'
];

export class AppStore {
  api!: Backend;
  info = $state<AppInfo>({ version: '', os: 'windows' });
  settings = $state<Settings | null>(null);
  booted = $state(false);
  screen = $state<Screen>('devices');
  guide = $state<Guide>('usb');
  devices = $state<Device[]>([]);
  scanning = $state(false);
  worlds = $state<World[] | null>(null);
  game = $state<GameFiles | null>(null);
  shown = $state<ShownError | null>(null);
  /** World open in the editor placeholder. */
  editing = $state<World | null>(null);
  /** Files dragged over the window (count), null when none. */
  dragging = $state<number | null>(null);
  /** "Edited just now" notice after the editor saved a world (Phase 2). */
  savedEdit = $state<{ world: string; changes: number } | null>(null);
  /** Backups screen opens the restore dialog of this backup (from "Restore backup…" after a write). */
  restoreId = $state<string | null>(null);

  read = new ReadFlow(this);
  transfer = new TransferFlow(this);
  importer = new ImportFlow(this);
  gameFiles = new GameFilesFlow(this);
  update = new UpdateFlow(this);
  adb = new AdbFlow(this);

  private unlisten: (() => void)[] = [];

  /** Connect to a backend and load the start-up state. */
  async init(api: Backend, languages: readonly string[] = navigator.languages) {
    this.api = api;
    const [info, settings] = await Promise.all([api.appInfo(), api.getSettings()]);
    this.info = info;
    this.settings = settings;
    setLang(settings.lang ?? detectLang(languages));
    setTheme(settings.theme);
    setReduceMotion(settings.reduce_motion);
    this.booted = true;
    this.unlisten.push(
      await api.onDevicesChanged((d) => (this.devices = d)),
      await api.onOpenFiles((paths) => this.openFiles(paths)),
      await api.onDragDrop((e) => {
        if (e.type === 'over') this.dragging = e.count;
        else if (e.type === 'leave') this.dragging = null;
        else {
          this.dragging = null;
          this.dropped(e.paths);
        }
      })
    );
    void this.refreshDevices();
    void this.adb.load();
    void this.loadGameFiles();
    if (settings.auto_update) void this.update.check(true);
  }

  /** Stop listening (tests). */
  dispose() {
    this.unlisten.splice(0).forEach((u) => u());
  }

  async saveSettings(patch: Partial<Settings>) {
    this.settings = await this.api.setSettings(patch);
    if (patch.theme) setTheme(patch.theme);
    if (patch.reduce_motion !== undefined) setReduceMotion(patch.reduce_motion);
    if (patch.lang) setLang(patch.lang);
  }

  go(screen: Screen) {
    this.screen = screen;
    if (screen === 'worlds') void this.loadWorlds();
    if (screen === 'devices') void this.refreshDevices();
  }

  openGuide(guide: Guide) {
    this.guide = guide;
    this.screen = 'connect';
  }

  async refreshDevices() {
    this.scanning = true;
    try {
      this.devices = await this.api.listDevices();
    } catch (e) {
      this.fail(e);
    } finally {
      this.scanning = false;
    }
  }

  async loadWorlds() {
    try {
      this.worlds = await this.api.listWorlds();
    } catch (e) {
      this.fail(e);
    }
  }

  async loadGameFiles() {
    this.game = await this.api.gameFiles();
  }

  /** Show a backend failure: a dialog for the designed errors, a toast for the rest; `cancelled` is silent. */
  fail(e: unknown, retry?: () => void) {
    const error = toAppError(e);
    if (error.code === 'cancelled') return;
    if (error.code === 'adb_missing') {
      void this.adb.open();
      return;
    }
    if (DIALOG_CODES.includes(error.code)) this.shown = { error, retry };
    else toast(t(`err.${error.code}`), undefined, 8000);
  }

  /** Files opened with the app (double-clicked `.bhworld`) or dropped on the window. */
  openFiles(paths: string[]) {
    if (paths.length === 0) return;
    this.go('worlds');
    void this.importer.open(paths);
  }

  dropped(paths: string[]) {
    if (this.gameFiles.isOpen && this.gameFiles.wantsFile) void this.gameFiles.useFile(paths[0]);
    else this.openFiles(paths);
  }

  /** Worlds → Edit: the editor needs the game graphics first (WT-GameFiles). */
  edit(world: World) {
    if (this.game?.state === 'ready') this.openEditor(world);
    else this.gameFiles.open(world);
  }

  openEditor(world: World) {
    this.editing = world;
    this.screen = 'editor';
  }

  /** Android phones and tablets that can be read or written now. */
  readyPhones = $derived(this.devices.filter((d) => d.kind === 'android' && d.status === 'ready'));

  /** The device shown in the sidebar card: the first ready phone, else Blockheads PC. */
  connected = $derived(this.readyPhones[0] ?? this.devices.find((d) => d.kind === 'pc' && d.status === 'ready') ?? null);

  /** Any long write or read is running (an update restart must wait). */
  get busy(): boolean {
    return this.read.running || this.transfer.running;
  }
}

export const store = new AppStore();
