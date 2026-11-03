import { t } from '../i18n.svelte';
import type { AppStore } from '../store.svelte';
import type { Device, DeviceWorld, ExportFormat, ExportResult, Progress, ReadResult } from '../types';
import { toast } from '../ui/toast.svelte';
import { opId } from '../util';

export type ReadStage = 'prepare' | 'phone' | 'reading' | 'choose' | 'options' | 'exporting' | 'done';

/** Read worlds from a device, choose, export (WT-Export). */
export class ReadFlow {
  device = $state<Device | null>(null);
  stage = $state<ReadStage>('prepare');
  gameClosed = $state(false);
  password = $state('');
  progress = $state<Progress | null>(null);
  result = $state<ReadResult | null>(null);
  selected = $state<string[]>([]);
  query = $state('');
  view = $state<'list' | 'grid'>('list');
  format = $state<ExportFormat>('bhworld');
  folder = $state('');
  free = $state<number | null>(null);
  exported = $state<ExportResult | null>(null);
  readAt = $state(0);
  private op = '';

  constructor(private app: AppStore) {}

  /** A phone confirmation is needed (adb backup), not for root, Blockheads PC or iPhone backups. */
  get needsPhone(): boolean {
    const d = this.device;
    return !!d && d.kind === 'android' && !(d.root && this.app.settings?.root_mode !== 'never');
  }

  /** The game must be closed by the user (Blockheads PC is checked by the app, iPhone backups are files). */
  get askClosed(): boolean {
    return this.device?.kind === 'android';
  }

  get running(): boolean {
    return this.stage === 'phone' || this.stage === 'reading' || this.stage === 'exporting';
  }

  get steps(): string[] {
    const s = [t('read.step.prepare'), t('read.step.phone'), t('read.step.read'), t('read.step.choose'), t('read.step.export')];
    return this.needsPhone ? s : s.filter((_, i) => i !== 1);
  }

  get stepIndex(): number {
    const full = { prepare: 0, phone: 1, reading: 2, choose: 3, options: 4, exporting: 4, done: 5 }[this.stage];
    return this.needsPhone || full === 0 ? full : full - 1;
  }

  get shown(): DeviceWorld[] {
    const q = this.query.trim().toLocaleLowerCase();
    const all = this.result?.worlds ?? [];
    return q ? all.filter((w) => w.name.toLocaleLowerCase().includes(q)) : all;
  }

  get selectedWorlds(): DeviceWorld[] {
    return (this.result?.worlds ?? []).filter((w) => this.selected.includes(w.id));
  }

  get selectedSize(): number {
    return this.selectedWorlds.reduce((s, w) => s + w.size, 0);
  }

  start(device: Device) {
    this.device = device;
    this.stage = 'prepare';
    this.gameClosed = false;
    this.password = '';
    this.progress = null;
    this.result = null;
    this.selected = [];
    this.query = '';
    this.exported = null;
    this.app.screen = 'read';
  }

  async begin() {
    const d = this.device as Device;
    this.op = opId('read');
    this.stage = this.needsPhone ? 'phone' : 'reading';
    this.progress = null;
    try {
      this.result = await this.app.api.read(this.op, { device_id: d.id, password: this.password }, (p) => {
        this.progress = p;
        this.stage = p.stage === 'phone_backup' ? 'phone' : 'reading';
      });
      this.readAt = Date.now();
      this.selected = this.result.worlds.map((w) => w.id);
      this.stage = 'choose';
    } catch (e) {
      this.stage = 'prepare';
      this.app.fail(e, () => void this.begin());
    }
  }

  async cancel() {
    await this.app.api.cancel(this.op);
  }

  toggle(id: string, on: boolean) {
    this.selected = on ? [...this.selected.filter((x) => x !== id), id] : this.selected.filter((x) => x !== id);
  }

  toggleAll(on: boolean) {
    this.selected = on ? (this.result?.worlds ?? []).map((w) => w.id) : [];
  }

  async saveOnly() {
    const r = this.result as ReadResult;
    const saved = await this.app.api.saveToLibrary(r.read_id, this.selected);
    toast(t('read.savedToast', { n: saved.length }));
    this.app.go('worlds');
  }

  async options() {
    this.stage = 'options';
    if (!this.folder) this.folder = (await this.app.api.pickFolder(null)) ?? '';
    this.free = await this.app.api.freeSpace(this.folder);
  }

  async browse() {
    const f = await this.app.api.pickFolder(this.folder || null);
    if (f) {
      this.folder = f;
      this.free = await this.app.api.freeSpace(f);
    }
  }

  async exportNow() {
    const r = this.result as ReadResult;
    this.op = opId('export');
    this.stage = 'exporting';
    this.progress = null;
    try {
      this.exported = await this.app.api.exportWorlds(
        this.op,
        { read_id: r.read_id, world_ids: this.selected, format: this.format, folder: this.folder },
        (p) => (this.progress = p)
      );
      this.stage = 'done';
    } catch (e) {
      this.stage = 'options';
      this.app.fail(e, () => void this.browse());
    }
  }

  fileName(w: DeviceWorld): string {
    return this.format === 'bhworld' ? `${w.name}.bhworld` : w.name;
  }
}
