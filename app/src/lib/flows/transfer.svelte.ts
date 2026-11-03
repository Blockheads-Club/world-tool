import type { AppStore } from '../store.svelte';
import type { Choice, Device, DeviceWorld, Progress, Source, TransferPlan, TransferResult } from '../types';
import { isAppError } from '../types';
import { opId } from '../util';

export type TransferStage = 'choose' | 'review' | 'before' | 'running' | 'done';
/** "library", "files" or a device id. */
export type FromKey = string;

/** Move worlds between devices, Worlds and files (WT-Import). */
export class TransferFlow {
  stage = $state<TransferStage>('choose');
  from = $state<FromKey>('library');
  files = $state<string[]>([]);
  to = $state<string | null>(null);
  /** Worlds the source offers; null while loading, 'unread' for a phone that was not read yet. */
  available = $state<DeviceWorld[] | 'unread' | null>(null);
  picked = $state<string[]>([]);
  plan = $state<TransferPlan | null>(null);
  choices = $state<Record<string, Choice>>({});
  gameClosed = $state(false);
  progress = $state<Progress | null>(null);
  /** Stages seen while running (for the step list). */
  seen = $state<string[]>([]);
  result = $state<TransferResult | null>(null);
  private op = '';

  constructor(private app: AppStore) {}

  get source(): Source {
    if (this.from === 'library') return { kind: 'library' };
    if (this.from === 'files') return { kind: 'files', paths: this.files };
    return { kind: 'device', device_id: this.from };
  }

  get target(): Device | null {
    return this.app.devices.find((d) => d.id === this.to) ?? null;
  }

  get running(): boolean {
    return this.stage === 'running';
  }

  get pickedWorlds(): DeviceWorld[] {
    return Array.isArray(this.available) ? this.available.filter((w) => this.picked.includes(w.id)) : [];
  }

  /** A device can be written when it is ready, not read-only and not the source. */
  canWrite(d: Device): boolean {
    return d.status === 'ready' && !d.read_only && d.id !== this.from;
  }

  /** Open Transfer, optionally with a preset source, target and worlds. */
  start(opts: { from?: FromKey; to?: string | null; worlds?: string[]; files?: string[] } = {}) {
    this.stage = 'choose';
    this.from = opts.from ?? 'library';
    this.files = opts.files ?? [];
    this.to = opts.to ?? null;
    this.plan = null;
    this.result = null;
    this.gameClosed = false;
    this.app.screen = 'transfer';
    void this.loadSource(opts.worlds);
  }

  async setFrom(key: FromKey) {
    if (key === 'files') {
      const paths = await this.app.api.pickImport(false);
      if (paths.length === 0) return;
      this.files = paths;
    }
    this.from = key;
    if (this.to === key) this.to = null;
    await this.loadSource();
  }

  async loadSource(preset?: string[]) {
    this.available = null;
    try {
      const worlds = await this.app.api.sourceWorlds(this.source);
      this.available = worlds;
      this.picked = preset ?? worlds.slice(0, 2).map((w) => w.id);
    } catch (e) {
      if (isAppError(e) && e.code === 'not_found') this.available = 'unread';
      else {
        this.available = [];
        this.app.fail(e);
      }
      this.picked = [];
    }
  }

  togglePick(id: string, on: boolean) {
    this.picked = on ? [...this.picked.filter((x) => x !== id), id] : this.picked.filter((x) => x !== id);
  }

  async review() {
    try {
      this.plan = await this.app.api.transferPlan(this.source, this.picked, this.to as string);
      this.choices = Object.fromEntries(this.plan.items.filter((i) => i.result === 'conflict').map((i) => [i.key, 'keep_both' as Choice]));
      this.stage = 'review';
    } catch (e) {
      this.app.fail(e);
    }
  }

  /** Counts for the review summary with the current choices. */
  get summary() {
    const items = this.plan!.items;
    const conflicts = items.filter((i) => i.result === 'conflict');
    const skipped = conflicts.filter((i) => this.choices[i.key] === 'skip').length;
    const replaced = conflicts.filter((i) => this.choices[i.key] === 'replace').length;
    // A conflicting world on the target stays (or is replaced in place); "keep both" adds the copy.
    const added = items.filter((i) => i.result === 'added').length + conflicts.length - skipped - replaced;
    const stay = items.filter((i) => i.result === 'stays').length + conflicts.length;
    return { added, stay, conflicts: conflicts.length, replaced, skipped, total: stay + added };
  }

  async write() {
    const plan = this.plan as TransferPlan;
    this.op = opId('transfer');
    this.stage = 'running';
    this.progress = null;
    this.seen = [];
    try {
      this.result = await this.app.api.transfer(this.op, { plan_id: plan.plan_id, choices: this.choices }, (p) => {
        this.progress = p;
        if (!this.seen.includes(p.stage)) this.seen = [...this.seen, p.stage];
      });
      this.stage = 'done';
      this.app.update.afterBusy();
      void this.app.refreshDevices();
    } catch (e) {
      this.stage = 'before';
      this.app.fail(e, () => void this.write());
    }
  }

  async cancel() {
    await this.app.api.cancel(this.op);
  }
}
