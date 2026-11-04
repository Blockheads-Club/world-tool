import { inTauri, tauriBackend, type Backend, type DragDrop, type OnProgress } from './api';
import type {
  Activity,
  AppError,
  Backup,
  Choice,
  Device,
  DeviceWorld,
  ErrorCode,
  GameFiles,
  ImportItem,
  IosBackup,
  PairedDevice,
  PlanItem,
  Settings,
  Source,
  TransferPlan,
  Unlisten,
  World
} from './types';
import { mockBlockheads, mockCatalog, mockOverview, mockRegion, mockTileInfo, mockWorld } from './editor/mock-world';

/**
 * In-memory backend for development (`bun run dev` outside Tauri), the unit tests and the e2e tests. It behaves like
 * the real one: progress events over time, the phone "confirms" after a moment, every write makes a backup first,
 * iPhone backups are read-only. `fail` makes the next call of an operation reject with an error code, so every error
 * screen can be reached (dev: `?fail=read:declined`).
 */

const MB = 1024 * 1024;
const DAY = 86_400_000;
const err = (code: ErrorCode, extra: Partial<AppError> = {}): AppError => ({ code, ...extra });

/** A small stand-in map preview: sky, a hilly surface and ground layers (our own art, not game graphics). */
export function mockPreview(seed: string, biome: 'grass' | 'cave' | 'snow' | 'desert'): string {
  let h = 7;
  for (const c of seed) h = (h * 31 + c.charCodeAt(0)) >>> 0;
  const top = { grass: '#5FB548', cave: '#7A7068', snow: '#F2F6FA', desert: '#E9C46A' }[biome];
  const sky = biome === 'cave' ? '#3A312B' : '#8FCBEA';
  let path = 'M0 40';
  for (let x = 0; x <= 210; x += 10) path += ` L${x} ${30 + ((h >> (x % 17)) % 12)}`;
  path += ' L210 80 L0 80 Z';
  const svg =
    `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 210 80" shape-rendering="crispEdges">` +
    `<rect width="210" height="80" fill="${sky}"/><path d="${path}" fill="${top}"/>` +
    `<rect y="52" width="210" height="28" fill="#8A5A36"/><rect y="66" width="210" height="14" fill="#6E625A"/></svg>`;
  return `data:image/svg+xml,${encodeURIComponent(svg)}`;
}

export interface MockOptions {
  /** Milliseconds per progress step (0 in tests). */
  tick?: number;
  /** Start-up data: `full` (as in the designs), `empty` (first run, nothing found) or `fresh` (first run, devices). */
  scenario?: 'full' | 'empty' | 'fresh';
  now?: number;
}

type FailKey =
  | 'read'
  | 'export'
  | 'transfer'
  | 'restore'
  | 'apk'
  | 'pair'
  | 'update'
  | 'check'
  | 'import'
  | 'source'
  | 'ios'
  | 'editor';

export class MockBackend implements Backend {
  tick: number;
  now: number;
  /** The next call of the operation rejects with this error. */
  fail: Partial<Record<FailKey, AppError>> = {};
  /** Dev / e2e: a newer version is offered. */
  update: { version: string; notes: string[] } | null = null;
  settings: Settings;
  devices: Device[];
  worlds: World[];
  phoneWorlds: Record<string, DeviceWorld[]>;
  backups: Backup[];
  activity: Activity[];
  game: GameFiles;
  paired: PairedDevice[];
  ios: IosBackup[];
  private reads = new Map<string, { device: Device; worlds: DeviceWorld[] }>();
  private plans = new Map<string, { plan: TransferPlan; source: Source; worlds: DeviceWorld[] }>();
  private cancelled = new Set<string>();
  private seq = 0;
  private deviceCbs = new Set<(d: Device[]) => void>();
  private openCbs = new Set<(p: string[]) => void>();
  private dropCbs = new Set<(e: DragDrop) => void>();

  constructor(opts: MockOptions = {}) {
    this.tick = opts.tick ?? 120;
    this.now = opts.now ?? Date.now();
    const scenario = opts.scenario ?? 'full';
    const t = this.now;
    const pv = mockPreview;
    const dw = (id: string, name: string, size: number, v: string, played: number | null, created: number, mode: 'normal' | 'expert', biome: Parameters<typeof mockPreview>[1] | null): DeviceWorld => ({
      id, name, size: size * MB, game_version: v, last_played: played, created, mode, preview: biome ? pv(name, biome) : null
    });
    this.settings = {
      first_run_done: scenario === 'full',
      worlds_folder: '/home/ada/Documents/BH World Tool',
      backup_before_write: true,
      backups_to_keep: 5,
      root_mode: 'auto',
      theme: 'system',
      lang: null,
      reduce_motion: false,
      auto_update: true,
      pc_folder: null,
      skipped_version: null,
      last_update_check: t - 6 * 3_600_000
    };
    const dev = (d: Partial<Device> & Pick<Device, 'id' | 'name' | 'kind'>): Device => ({
      model: '', os_version: null, status: 'ready', connection: 'usb', address: null, game_version: '1.7.6', root: false,
      read_only: false, encrypted: false, last_seen: null, world_count: null, path: null, backup_date: null, ...d
    });
    this.devices =
      scenario === 'empty'
        ? []
        : [
            dev({ id: 'pixel8', kind: 'android', name: 'Ada’s Pixel 8', model: 'Google Pixel 8', os_version: '15' }),
            dev({ id: 'tab', kind: 'android', name: 'Galaxy Tab S6 Lite', model: 'Samsung SM-P610', os_version: '13', connection: 'wifi', address: '192.168.1.42:37115', game_version: '1.7.5' }),
            dev({ id: 'redmi', kind: 'android', name: 'Redmi Note 9', model: 'Xiaomi M2003J15SC', os_version: '12', game_version: '1.7.4', root: true }),
            dev({ id: 'moto', kind: 'android', name: 'moto g(7)', model: 'Motorola', os_version: '10', status: 'unauthorized', game_version: null }),
            dev({ id: 'pixel6a', kind: 'android', name: 'Pixel 6a', model: 'Google Pixel 6a', os_version: '14', status: 'offline', connection: 'wifi', game_version: null, last_seen: t - 2 * DAY }),
            dev({ id: 'pc', kind: 'pc', name: 'Blockheads PC', model: '', connection: 'local', world_count: 5, path: '/home/ada/.local/share/Blockheads PC' }),
            dev({ id: 'iphone', kind: 'ios', name: 'Ada’s iPhone', model: 'iPhone 13', os_version: '17.6', connection: 'backup', read_only: true, encrypted: true, world_count: 2, backup_date: t - 22 * DAY })
          ];
    this.phoneWorlds = {
      pixel8: [
        dw('w-sunny', 'Sunny Isle', 48, '1.7.6', t - 3_600_000, t - 600 * DAY, 'normal', 'grass'),
        dw('w-deep', 'Deep Mine', 132, '1.7.6', t - DAY, t - 1000 * DAY, 'expert', 'cave'),
        dw('w-castle', 'Castle Build', 22, '1.7.6', t - 6 * DAY, t - 90 * DAY, 'normal', 'snow'),
        dw('w-desert', 'Desert Run', 64, '1.7.5', t - 63 * DAY, t - 520 * DAY, 'expert', 'desert'),
        dw('w-new', 'New World', 1.2, '1.7.6', null, t - 4 * DAY, 'normal', null)
      ],
      tab: [dw('w-desert', 'Desert Run', 64, '1.7.5', t - 63 * DAY, t - 520 * DAY, 'expert', 'desert'), dw('w-tab2', 'Lake House', 17, '1.7.5', t - 70 * DAY, t - 200 * DAY, 'normal', 'grass')],
      redmi: [dw('w-redmi', 'Root Farm', 9, '1.7.4', t - 30 * DAY, t - 300 * DAY, 'normal', 'grass')],
      pc: [
        dw('w-sky', 'Skyblock', 6, '1.7.6', t - 5 * DAY, t - 40 * DAY, 'normal', 'grass'),
        dw('w-pc2', 'Winter Base', 31, '1.7.6', t - 9 * DAY, t - 80 * DAY, 'normal', 'snow'),
        dw('w-pc3', 'Mines of Moria', 77, '1.7.6', t - 12 * DAY, t - 300 * DAY, 'expert', 'cave'),
        dw('w-pc4', 'Beach', 12, '1.7.6', t - 20 * DAY, t - 100 * DAY, 'normal', 'desert'),
        dw('w-pc5', 'Test', 1, '1.7.6', null, t - 2 * DAY, 'normal', null)
      ],
      iphone: [dw('w-bay', 'Pirate Bay', 25, '1.7.6', t - 23 * DAY, t - 400 * DAY, 'normal', 'grass'), dw('w-old', 'Old Farm', 18, '1.7.6', t - 25 * DAY, t - 700 * DAY, 'normal', 'grass')]
    };
    const lib = (w: DeviceWorld, origin: World['origin'], source: string, detail: string | null, added: number): World => ({
      ...w, world_id: `${w.id.replace(/\W/g, '')}a41f0000000000000000000009c2`.slice(0, 32), origin, source, source_detail: detail, added, edited: null
    });
    const p8 = this.phoneWorlds.pixel8;
    this.worlds =
      scenario === 'full'
        ? [
            lib(p8[0], 'phone', 'Ada’s Pixel 8', 'USB', t - 600_000),
            lib(p8[1], 'phone', 'Ada’s Pixel 8', 'USB', t - 600_000),
            lib(p8[2], 'phone', 'Ada’s Pixel 8', 'USB', t - 600_000),
            lib(dw('w-pirate', 'Pirate Cove', 37, '1.7.6', t - 5 * DAY, t - 50 * DAY, 'normal', 'grass'), 'file', 'Pirate Cove.bhworld', null, t - 4 * DAY),
            lib(this.phoneWorlds.pc[0], 'pc', 'Blockheads PC', null, t - 5 * DAY),
            lib(this.phoneWorlds.iphone[1], 'ios', 'Ada’s iPhone backup', null, t - 22 * DAY),
            lib(this.phoneWorlds.tab[0], 'phone', 'Galaxy Tab S6 Lite', 'Wi-Fi', t - 63 * DAY)
          ]
        : [];
    this.backups =
      scenario === 'full'
        ? [
            { id: 'b1', device_id: 'pixel8', device: 'Ada’s Pixel 8', reason: 'before_write', reason_count: 7, time: t - 300_000, worlds: 5, size: 270 * MB, automatic: true },
            { id: 'b2', device_id: 'pixel8', device: 'Ada’s Pixel 8', reason: 'read', reason_count: null, time: t - 900_000, worlds: 5, size: 270 * MB, automatic: true },
            { id: 'b3', device_id: 'tab', device: 'Galaxy Tab S6 Lite', reason: 'before_write', reason_count: 1, time: t - 63 * DAY, worlds: 2, size: 81 * MB, automatic: true }
          ]
        : [];
    const act = (id: string, ago: number, a: Omit<Activity, 'id' | 'time'>): Activity => ({ id, time: t - ago, ...a });
    this.activity =
      scenario === 'full'
        ? [
            act('a1', 60_000, { kind: 'copy', result: 'done', device: 'Ada’s Pixel 8', worlds: ['Sunny Isle', 'Pirate Cove'], size: 85 * MB, detail: null, error: null }),
            act('a2', 300_000, { kind: 'backup', result: 'done', device: 'Ada’s Pixel 8', worlds: [], size: 270 * MB, detail: null, error: null }),
            act('a3', 600_000, { kind: 'copy', result: 'failed', device: 'Galaxy Tab S6 Lite', worlds: ['Desert Run'], size: null, detail: 'E_DEVICE_GONE · adb: device offline', error: 'disconnected' }),
            act('a4', 900_000, { kind: 'read', result: 'done', device: 'Ada’s Pixel 8', worlds: ['Sunny Isle', 'Deep Mine', 'Castle Build', 'Desert Run', 'New World'], size: 132 * MB, detail: null, error: null }),
            act('a5', DAY, { kind: 'read', result: 'stopped', device: 'moto g(7)', worlds: [], size: null, detail: 'E_USER_DECLINED', error: 'declined' }),
            act('a6', 5 * DAY, { kind: 'import', result: 'done', device: 'Blockheads PC', worlds: ['Skyblock'], size: 6 * MB, detail: null, error: null })
          ]
        : [];
    this.game =
      scenario === 'full'
        ? { state: 'ready', apk_path: '/home/ada/Downloads/TheBlockheads.apk', apk_name: 'TheBlockheads.apk', version: '1.7.6', images: 1084, size: 38 * MB }
        : { state: 'not_set', apk_path: null, apk_name: null, version: null, images: null, size: null };
    this.paired = scenario === 'empty' ? [] : [
      { id: 'tab', name: 'Galaxy Tab S6 Lite', address: '192.168.1.42:37115' },
      { id: 'pixel6a', name: 'Pixel 6a', address: '192.168.1.57:40021' }
    ];
    this.ios = [
      { id: 'iphone', name: 'Ada’s iPhone', model: 'iPhone 13 · iOS 17.6', date: t - 22 * DAY, encrypted: true },
      { id: 'ipad', name: 'Family iPad', model: 'iPad (9th gen) · iPadOS 16', date: t - 305 * DAY, encrypted: false }
    ];
  }

  // ------------------------------------------------------------ helpers

  private id(prefix: string): string {
    this.seq += 1;
    return `${prefix}${this.seq}`;
  }

  private sleep(): Promise<void> {
    return new Promise((r) => setTimeout(r, this.tick));
  }

  /** Reject with the queued error for `key`, once. */
  private failing(key: FailKey): void {
    const e = this.fail[key];
    if (e) {
      delete this.fail[key];
      throw e;
    }
  }

  /** One step of an operation: sleep, then stop when it was cancelled. */
  private async step(op: string): Promise<void> {
    await this.sleep();
    if (this.cancelled.has(op)) {
      this.cancelled.delete(op);
      throw err('cancelled');
    }
  }

  private device(id: string): Device {
    const d = this.devices.find((x) => x.id === id);
    if (!d) throw err('not_found', { detail: id });
    return d;
  }

  private log(a: Omit<Activity, 'id' | 'time'>): void {
    this.activity.unshift({ id: this.id('act-'), time: Date.now(), ...a });
  }

  private emitDevices(): void {
    for (const cb of this.deviceCbs) cb(this.devices);
  }

  private libWorld(w: DeviceWorld, origin: World['origin'], source: string, detail: string | null): World {
    return { ...w, id: this.id('l'), world_id: w.id.padEnd(32, '0').slice(0, 32), origin, source, source_detail: detail, added: Date.now(), edited: null };
  }

  /** The automatic backup of a device before it is written. */
  private makeBackup(d: Device, reason: Backup['reason'], count: number | null): Backup {
    const worlds = this.phoneWorlds[d.id] ?? [];
    const b: Backup = {
      id: this.id('backup-'), device_id: d.id, device: d.name, reason, reason_count: count, time: Date.now(),
      worlds: worlds.length, size: worlds.reduce((s, w) => s + w.size, 0) + 20 * MB, automatic: true
    };
    this.backups.unshift(b);
    const mine = this.backups.filter((x) => x.device_id === d.id);
    for (const old of mine.slice(this.settings.backups_to_keep)) this.backups.splice(this.backups.indexOf(old), 1);
    this.log({ kind: 'backup', result: 'done', device: d.name, worlds: [], size: b.size, detail: null, error: null });
    return b;
  }

  /** Waiting for "Back up my data" / "Restore my data" (skipped with root or for Blockheads PC). */
  private async phone(op: string, d: Device, stage: 'phone_backup' | 'phone_restore', onProgress: OnProgress): Promise<void> {
    if (d.kind !== 'android' || (d.root && this.settings.root_mode !== 'never')) return;
    for (let s = 0; s < 3; s++) {
      onProgress({ stage, elapsed: s });
      await this.step(op);
    }
  }

  private async bytes(op: string, stage: 'reading' | 'writing' | 'backing_up', total: number, onProgress: OnProgress): Promise<void> {
    for (let i = 0; i <= 4; i++) {
      onProgress({ stage, done: Math.round((total * i) / 4), total, eta: (4 - i) * 10 });
      await this.step(op);
    }
  }

  // ------------------------------------------------------------ app / settings

  async appInfo() {
    return { version: '0.1.0', os: 'linux' as const };
  }

  async getSettings() {
    return { ...this.settings };
  }

  async setSettings(patch: Partial<Settings>) {
    Object.assign(this.settings, patch);
    return { ...this.settings };
  }

  // ------------------------------------------------------------ devices

  async listDevices() {
    await this.sleep();
    return this.devices.map((d) => ({ ...d }));
  }

  async onDevicesChanged(cb: (d: Device[]) => void): Promise<Unlisten> {
    this.deviceCbs.add(cb);
    return () => this.deviceCbs.delete(cb);
  }

  /** Test / dev helper: plug a device in (or change it) and tell the listeners. */
  plug(d: Device): void {
    this.devices = [...this.devices.filter((x) => x.id !== d.id), d];
    this.emitDevices();
  }

  async wifiDiscover() {
    return this.devices.filter((d) => d.connection === 'wifi' && d.status === 'ready' && d.address).map((d) => ({ name: d.name, address: d.address as string }));
  }

  async wifiPair(address: string, code: string) {
    await this.sleep();
    this.failing('pair');
    if (!/^[\w.-]+:\d+$/.test(address.trim()) || !/^\d{6}$/.test(code.replace(/\s/g, ''))) throw err('pair_failed', { detail: address });
    const known = this.devices.find((d) => d.address === address.trim());
    const d: Device = known ?? {
      id: this.id('wifi'), kind: 'android', name: `Android (${address.split(':')[0]})`, model: 'Android', os_version: '14', status: 'ready',
      connection: 'wifi', address: address.trim(), game_version: '1.7.6', root: false, read_only: false, encrypted: false, last_seen: null,
      world_count: null, path: null, backup_date: null
    };
    if (!this.paired.some((p) => p.address === d.address)) this.paired.push({ id: d.id, name: d.name, address: d.address as string });
    this.plug({ ...d, status: 'ready' });
    return d;
  }

  async pairedDevices() {
    return [...this.paired];
  }

  async forgetPaired(id: string) {
    this.paired = this.paired.filter((p) => p.id !== id);
  }

  async restartAdb() {
    await this.sleep();
  }

  async openDeviceManager() {}

  async iosBackups(_folder: string | null) {
    return [...this.ios];
  }

  async iosOpen(id: string, password: string) {
    await this.sleep();
    this.failing('ios');
    const b = this.ios.find((x) => x.id === id);
    if (!b) throw err('not_found');
    if (b.encrypted && password.length === 0) throw err('wrong_password', { attempt: 1 });
    const known = this.devices.find((d) => d.id === id);
    const d: Device = known ?? {
      id, kind: 'ios', name: b.name, model: b.model, os_version: null, status: 'ready', connection: 'backup', address: null, game_version: '1.7.6',
      root: false, read_only: true, encrypted: b.encrypted, last_seen: null, world_count: 0, path: null, backup_date: b.date
    };
    if (!known) this.plug(d);
    return d;
  }

  async diagnostics() {
    const lines = this.devices.map((d) => `${d.name} · ${d.model} · ${d.connection} · ${d.status} · game ${d.game_version ?? '-'}`);
    return [`BH World Tool 0.1.0 · linux`, `adb: running`, ...lines].join('\n');
  }

  // ------------------------------------------------------------ read / export

  async read(op: string, req: { device_id: string; password: string }, onProgress: OnProgress) {
    const d = this.device(req.device_id);
    if (d.status === 'unauthorized') throw err('not_allowed', { device: d.name });
    if (d.status === 'offline') throw err('disconnected', { device: d.name, percent: 0 });
    if (d.kind === 'android' && !d.game_version) throw err('game_not_installed', { device: d.name });
    await this.phone(op, d, 'phone_backup', onProgress);
    this.failing('read');
    const worlds = (this.phoneWorlds[d.id] ?? []).map((w) => ({ ...w }));
    await this.bytes(op, 'reading', worlds.reduce((s, w) => s + w.size, 0) || MB, onProgress);
    const read_id = this.id('r');
    this.reads.set(read_id, { device: d, worlds });
    this.log({ kind: 'read', result: 'done', device: d.name, worlds: worlds.map((w) => w.name), size: worlds.reduce((s, w) => s + w.size, 0), detail: null, error: null });
    return { read_id, worlds };
  }

  private takeFromRead(readId: string, ids: string[]): World[] {
    const r = this.reads.get(readId);
    if (!r) throw err('not_found', { detail: readId });
    const origin: World['origin'] = r.device.kind === 'pc' ? 'pc' : r.device.kind === 'ios' ? 'ios' : 'phone';
    const source = r.device.kind === 'ios' ? `${r.device.name} backup` : r.device.name;
    const detail = r.device.connection === 'usb' ? 'USB' : r.device.connection === 'wifi' ? 'Wi-Fi' : null;
    const saved = r.worlds.filter((w) => ids.includes(w.id)).map((w) => this.libWorld(w, origin, source, detail));
    this.worlds.unshift(...saved);
    return saved;
  }

  async saveToLibrary(readId: string, worldIds: string[]) {
    return this.takeFromRead(readId, worldIds);
  }

  async exportWorlds(op: string, req: { read_id: string | null; world_ids: string[]; format: 'bhworld' | 'server'; folder: string }, onProgress: OnProgress) {
    this.failing('export');
    const saved = req.read_id ? this.takeFromRead(req.read_id, req.world_ids) : [];
    const list = req.read_id ? saved : this.worlds.filter((w) => req.world_ids.includes(w.id));
    const files = [];
    for (const [i, w] of list.entries()) {
      const name = req.format === 'bhworld' ? `${w.name}.bhworld` : w.name;
      onProgress({ stage: 'exporting', item: name, index: i + 1, count: list.length, done: i, total: list.length });
      await this.step(op);
      onProgress({ stage: 'preview', item: name, index: i + 1, count: list.length, done: i + 0.5, total: list.length });
      await this.step(op);
      files.push({ name, size: Math.round(w.size * 0.88) });
    }
    this.log({ kind: 'export', result: 'done', device: 'Worlds', worlds: list.map((w) => w.name), size: files.reduce((s, f) => s + f.size, 0), detail: null, error: null });
    return { folder: req.folder, files, saved };
  }

  async freeSpace(_folder: string) {
    return 41 * 1024 * MB;
  }

  async cancel(op: string) {
    this.cancelled.add(op);
  }

  // ------------------------------------------------------------ transfer

  async sourceWorlds(source: Source): Promise<DeviceWorld[]> {
    this.failing('source');
    if (source.kind === 'library') return this.worlds.map((w) => ({ ...w }));
    if (source.kind === 'files') return (await this.inspectImport(source.paths)).filter((i) => i.kind !== 'invalid').map((i, n) => ({
      id: i.path, name: i.name.replace(/\.bhworld$/, ''), size: i.size, game_version: i.game_version as string, last_played: null, created: this.now - n * DAY, mode: 'normal', preview: null
    }));
    const d = this.device(source.device_id);
    if (d.kind === 'android') {
      const r = [...this.reads.values()].reverse().find((x) => x.device.id === d.id);
      if (!r) throw err('not_found', { device: d.name });
      return r.worlds.map((w) => ({ ...w }));
    }
    return (this.phoneWorlds[d.id] ?? []).map((w) => ({ ...w }));
  }

  async transferPlan(source: Source, worldIds: string[], targetId: string) {
    const target = this.device(targetId);
    if (target.read_only) throw err('io', { detail: 'read-only target', device: target.name });
    const all = await this.sourceWorlds(source);
    const picked = all.filter((w) => worldIds.includes(w.id));
    const there = this.phoneWorlds[target.id] ?? [];
    const items: PlanItem[] = [];
    const clashed = new Set<string>();
    for (const w of picked) {
      const same = there.find((x) => x.name === w.name || x.id === (w as World).world_id || x.id === w.id);
      if (same) {
        clashed.add(same.id);
        items.unshift({ key: w.id, name: w.name, size: w.size, result: 'conflict', conflict: same.name === w.name ? 'same_name' : 'same_id', copy_name: `${w.name} (2)` });
      } else items.push({ key: w.id, name: w.name, size: w.size, result: 'added', conflict: null, copy_name: null });
    }
    // A world on the target that clashes is shown once, as the conflict row.
    const stays = there.filter((w) => !clashed.has(w.id));
    items.splice(items.filter((i) => i.result === 'conflict').length, 0, ...stays.map((w) => ({ key: `stay:${w.id}`, name: w.name, size: w.size, result: 'stays' as const, conflict: null, copy_name: null })));
    const plan: TransferPlan = {
      plan_id: this.id('p'), target: target.name, total_after: there.length + picked.length, items,
      size: items.reduce((s, i) => s + i.size, 0), backup_name: target.name, backup_size: there.reduce((s, w) => s + w.size, 0) + 20 * MB,
      needs_phone: target.kind === 'android' && !(target.root && this.settings.root_mode !== 'never')
    };
    this.plans.set(plan.plan_id, { plan, source, worlds: picked });
    return plan;
  }

  async transfer(op: string, req: { plan_id: string; choices: Record<string, Choice> }, onProgress: OnProgress) {
    const p = this.plans.get(req.plan_id);
    if (!p) throw err('not_found', { detail: req.plan_id });
    const target = this.devices.find((d) => d.name === p.plan.target) as Device;
    let backup: Backup | null = null;
    if (this.settings.backup_before_write) {
      await this.phone(op, target, 'phone_backup', onProgress);
      await this.bytes(op, 'backing_up', p.plan.backup_size, onProgress);
      backup = this.makeBackup(target, 'before_write', p.plan.total_after);
    }
    onProgress({ stage: 'preparing' });
    await this.step(op);
    await this.phone(op, target, 'phone_restore', onProgress);
    try {
      this.failing('transfer');
    } catch (e) {
      const ae = e as AppError;
      this.log({ kind: 'copy', result: 'failed', device: target.name, worlds: p.worlds.map((w) => w.name), size: null, detail: ae.detail ?? ae.code, error: ae.code });
      throw { ...ae, device: target.name, backup_id: backup?.id };
    }
    await this.bytes(op, 'writing', p.plan.size, onProgress);
    onProgress({ stage: 'checking' });
    await this.step(op);
    const there = (this.phoneWorlds[target.id] ??= []);
    const res = { target: target.name, total: 0, added: [] as string[], replaced: [] as string[], kept_both: [] as string[], backup_id: backup?.id ?? null, backup_time: backup?.time ?? null };
    for (const w of p.worlds) {
      const item = p.plan.items.find((i) => i.key === w.id) as PlanItem;
      const choice = item.result === 'conflict' ? (req.choices[w.id] ?? 'keep_both') : 'keep_both';
      if (choice === 'skip') continue;
      if (item.result === 'conflict' && choice === 'replace') {
        there.splice(there.findIndex((x) => x.name === w.name || x.id === w.id), 1, { ...w });
        res.replaced.push(w.name);
      } else if (item.result === 'conflict') {
        there.push({ ...w, id: this.id('w'), name: item.copy_name as string });
        res.kept_both.push(item.copy_name as string);
      } else {
        there.push({ ...w });
        res.added.push(w.name);
      }
    }
    res.total = there.length;
    if (target.kind === 'pc') target.world_count = there.length;
    this.log({ kind: 'copy', result: 'done', device: target.name, worlds: p.worlds.map((w) => w.name), size: p.plan.size, detail: null, error: null });
    return res;
  }

  // ------------------------------------------------------------ worlds

  async listWorlds() {
    await this.sleep();
    return this.worlds.map((w) => ({ ...w }));
  }

  private world(id: string): World {
    const w = this.worlds.find((x) => x.id === id);
    if (!w) throw err('not_found', { detail: id });
    return w;
  }

  async renameWorld(id: string, name: string) {
    const w = this.world(id);
    w.name = name.trim() || w.name;
    return { ...w };
  }

  async duplicateWorld(id: string) {
    const w = this.world(id);
    const copy: World = { ...w, id: this.id('l'), name: `${w.name} (2)`, added: Date.now(), edited: null };
    this.worlds.splice(this.worlds.indexOf(w) + 1, 0, copy);
    return { ...copy };
  }

  async deleteWorld(id: string) {
    this.worlds.splice(this.worlds.indexOf(this.world(id)), 1);
  }

  async showInFolder(_path: string) {}

  async inspectImport(paths: string[]): Promise<ImportItem[]> {
    this.failing('import');
    return paths.map((path) => {
      const name = path.split(/[\\/]/).pop() as string;
      const isWorld = name.endsWith('.bhworld');
      const isFolder = !name.includes('.');
      if (!isWorld && !isFolder) return { path, name, kind: 'invalid', size: 0, game_version: null, status: 'invalid', conflict: null, existing: null };
      const base = name.replace(/\.bhworld$/, '');
      const same = this.worlds.find((w) => w.name === base);
      const size = (base.length * 3 + 1) * MB;
      return {
        path, name, kind: isWorld ? 'bhworld' : 'server', size, game_version: '1.7.6', status: same ? 'conflict' : 'new',
        conflict: same ? (isWorld ? 'same_id' : 'same_name') : null, existing: same ? same.name : null
      } satisfies ImportItem;
    });
  }

  async importWorlds(req: { choices: Record<string, Choice>; paths: string[] }) {
    const items = (await this.inspectImport(req.paths)).filter((i) => i.status !== 'invalid');
    const out: World[] = [];
    for (const i of items) {
      const choice = i.status === 'conflict' ? (req.choices[i.path] ?? 'keep_both') : 'keep_both';
      if (choice === 'skip') continue;
      const base = i.name.replace(/\.bhworld$/, '');
      const w: World = this.libWorld(
        { id: base, name: base, size: i.size, game_version: i.game_version as string, last_played: null, created: Date.now(), mode: 'normal', preview: mockPreview(base, 'grass') },
        'file', i.name, i.kind === 'server' ? 'server' : null
      );
      if (choice === 'replace') this.worlds.splice(this.worlds.findIndex((x) => x.name === base), 1);
      else if (i.status === 'conflict') w.name = `${base} (2)`;
      this.worlds.unshift(w);
      out.push(w);
    }
    this.log({ kind: 'import', result: 'done', device: 'Worlds', worlds: out.map((w) => w.name), size: out.reduce((s, w) => s + w.size, 0), detail: null, error: null });
    return out;
  }

  // ------------------------------------------------------------ backups / activity

  async listBackups() {
    return this.backups.map((b) => ({ ...b }));
  }

  async restoreBackup(op: string, backupId: string, deviceId: string, onProgress: OnProgress) {
    const b = this.backups.find((x) => x.id === backupId);
    if (!b) throw err('not_found', { detail: backupId });
    const d = this.device(deviceId);
    await this.phone(op, d, 'phone_backup', onProgress);
    this.makeBackup(d, 'before_restore', null);
    await this.phone(op, d, 'phone_restore', onProgress);
    this.failing('restore');
    await this.bytes(op, 'writing', b.size, onProgress);
    this.log({ kind: 'restore', result: 'done', device: d.name, worlds: [], size: b.size, detail: null, error: null });
  }

  async deleteBackup(id: string) {
    this.backups = this.backups.filter((b) => b.id !== id);
  }

  async listActivity() {
    return this.activity.map((a) => ({ ...a }));
  }

  async clearActivity() {
    this.activity = [];
  }

  // ------------------------------------------------------------ game files

  async gameFiles() {
    return { ...this.game };
  }

  private async extract(op: string, path: string, onProgress: OnProgress): Promise<GameFiles> {
    const name = path.split(/[\\/]/).pop() as string;
    for (let i = 0; i < 3; i++) {
      onProgress({ stage: 'apk_checking', index: i, path, item: name, total: 82 * MB });
      await this.step(op);
    }
    this.failing('apk');
    if (!/blockheads/i.test(name)) throw err('apk_wrong_app', { name });
    for (let i = 0; i <= 4; i++) {
      onProgress({ stage: 'apk_extracting', done: i * 165, total: 660, item: 'blocks', version: '1.7.6', eta: (4 - i) * 5 });
      await this.step(op);
    }
    this.game = { state: 'ready', apk_path: path, apk_name: name, version: '1.7.6', images: 1084, size: 38 * MB };
    return { ...this.game };
  }

  async useApk(op: string, path: string, onProgress: OnProgress) {
    return this.extract(op, path, onProgress);
  }

  async apkFromDevice(op: string, deviceId: string, onProgress: OnProgress) {
    const d = this.device(deviceId);
    for (let i = 0; i <= 2; i++) {
      onProgress({ stage: 'apk_pulling', done: i, total: 2, item: d.name });
      await this.step(op);
    }
    return this.extract(op, `${this.settings.worlds_folder}/game/TheBlockheads.apk`, onProgress);
  }

  async reextract(op: string, onProgress: OnProgress) {
    if (!this.game.apk_path) throw err('apk_missing');
    return this.extract(op, this.game.apk_path, onProgress);
  }

  async clearGameFiles() {
    this.game = { ...this.game, state: 'not_set', images: null, size: null, version: null };
    return { ...this.game };
  }

  // ------------------------------------------------------------ pickers

  /** What the next picker returns (tests); by default a fixed example. */
  picks: { folder?: string | null; files?: string[]; apk?: string | null } = {};

  async pickFolder(start: string | null) {
    return 'folder' in this.picks ? (this.picks.folder as string | null) : (start ?? '/home/ada/Documents/Blockheads worlds');
  }

  async pickImport(folders: boolean) {
    return this.picks.files ?? (folders ? ['/home/ada/server/skyblock_world'] : ['/home/ada/Downloads/Frozen Peaks.bhworld', '/home/ada/Downloads/Sunny Isle.bhworld']);
  }

  async pickApk() {
    return 'apk' in this.picks ? (this.picks.apk as string | null) : '/home/ada/Downloads/TheBlockheads.apk';
  }

  // ------------------------------------------------------------ updates

  async checkUpdate() {
    await this.sleep();
    this.failing('check');
    this.settings.last_update_check = Date.now();
    return this.update ? { version: this.update.version, date: this.now, size: 24 * MB, notes: this.update.notes } : null;
  }

  async downloadUpdate(op: string, onProgress: OnProgress) {
    for (let i = 0; i <= 4; i++) {
      onProgress({ stage: 'downloading', done: i * 6 * MB, total: 24 * MB });
      await this.step(op);
    }
    this.failing('update');
  }

  async restartApp() {}

  // ------------------------------------------------------------ world editor

  /** Open editor sessions → library world id. */
  editors = new Map<string, string>();
  /** Dev / tests: macro blocks (`"x,y"`) the mock reports as damaged. */
  damagedBlocks: string[] = [];
  /** Dev / tests: an atlas image for `editor_atlas` (empty = no game files). */
  atlas: ArrayBuffer = new ArrayBuffer(0);

  private editorSession(session: string): string {
    const w = this.editors.get(session);
    if (!w) throw err('not_found', { detail: session });
    return w;
  }

  async editorOpen(id: string) {
    await this.sleep();
    this.failing('editor');
    const w = this.world(id);
    const session = this.id('ed');
    this.editors.set(session, id);
    return mockWorld(session, id, w.name);
  }

  async editorClose(session: string) {
    this.editors.delete(session);
  }

  async editorRegion(session: string, rect: [number, number, number, number]) {
    this.editorSession(session);
    await this.sleep();
    return mockRegion(rect, this.damagedBlocks);
  }

  async editorOverview(session: string, scale: number) {
    this.editorSession(session);
    return mockOverview(scale);
  }

  async editorTile(session: string, x: number, y: number) {
    this.editorSession(session);
    return mockTileInfo(x, y);
  }

  async editorCatalog() {
    return mockCatalog();
  }

  async editorAtlas(_which: 'tiles' | 'items') {
    return this.atlas;
  }

  async editorState(session: string) {
    this.editorSession(session);
    return { history: [], cursor: 0, unsaved: 0, clipboard: null };
  }

  async editorBlockheads(session: string) {
    this.editorSession(session);
    return mockBlockheads();
  }

  // ------------------------------------------------------------ OS integration

  async onOpenFiles(cb: (p: string[]) => void): Promise<Unlisten> {
    this.openCbs.add(cb);
    return () => this.openCbs.delete(cb);
  }

  async onDragDrop(cb: (e: DragDrop) => void): Promise<Unlisten> {
    this.dropCbs.add(cb);
    return () => this.dropCbs.delete(cb);
  }

  /** Test / dev helper: the OS opened files with the app. */
  openFiles(paths: string[]): void {
    for (const cb of this.openCbs) cb(paths);
  }

  /** Test / dev helper: a file drag over the window. */
  drag(e: DragDrop): void {
    for (const cb of this.dropCbs) cb(e);
  }
}

/** The backend for this page: Tauri inside the app, else the mock (dev server, e2e). */
export function chooseBackend(search: string): Backend {
  return inTauri() ? tauriBackend : mockFromUrl(search);
}

/** Dev options from the page URL: `?scenario=empty&tick=50&fail=read:declined&update=0.3.0`. */
export function mockFromUrl(search: string): MockBackend {
  const q = new URLSearchParams(search);
  const scenario = q.get('scenario');
  const m = new MockBackend({
    scenario: scenario === 'empty' || scenario === 'fresh' ? scenario : 'full',
    tick: q.has('tick') ? Number(q.get('tick')) : undefined
  });
  for (const f of q.getAll('fail')) {
    const [key, code] = f.split(':');
    m.fail[key as FailKey] = err(code as ErrorCode, { device: 'Ada’s Pixel 8' });
  }
  const up = q.get('update');
  if (up) m.update = { version: up, notes: ['Faster reading over Wi-Fi.', 'Fixes for Samsung phones that stopped at “Back up my data”.', 'Import world: drop several files at once.'] };
  return m;
}
