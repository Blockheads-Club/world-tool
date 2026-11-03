import type { AppStore } from '../store.svelte';
import type { AppError, GameFiles, Progress, World } from '../types';
import { opId } from '../util';
import { toAppError } from '../errors';

export type GameFilesStage = 'ask' | 'checking' | 'extracting' | 'ready' | 'error';

/** "Add your game file" dialog: the APK step before the editor (WT-GameFiles). */
export class GameFilesFlow {
  isOpen = $state(false);
  stage = $state<GameFilesStage>('ask');
  progress = $state<Progress | null>(null);
  error = $state<AppError | null>(null);
  /** The world to open once the graphics are ready. */
  world = $state<World | null>(null);
  private op = '';

  constructor(private app: AppStore) {}

  /** The dialog waits for a file (a drop goes to it instead of Import world). */
  get wantsFile(): boolean {
    return this.stage === 'ask' || this.stage === 'error';
  }

  /** The earlier APK is gone (graphics cleared and file moved). */
  get missing(): boolean {
    return this.app.game?.state === 'missing';
  }

  open(world: World | null = null) {
    this.world = world;
    this.isOpen = true;
    this.stage = 'ask';
    this.error = null;
    this.progress = null;
  }

  close() {
    if (this.stage === 'checking' || this.stage === 'extracting') void this.app.api.cancel(this.op);
    this.isOpen = false;
  }

  async choose() {
    const path = await this.app.api.pickApk();
    if (path) await this.useFile(path);
  }

  async useFile(path: string) {
    await this.run((op, cb) => this.app.api.useApk(op, path, cb));
  }

  async fromDevice(deviceId: string) {
    await this.run((op, cb) => this.app.api.apkFromDevice(op, deviceId, cb));
  }

  async reextract() {
    this.open();
    await this.run((op, cb) => this.app.api.reextract(op, cb));
  }

  private async run(call: (op: string, cb: (p: Progress) => void) => Promise<GameFiles>) {
    this.op = opId('apk');
    this.stage = 'checking';
    this.error = null;
    this.progress = null;
    try {
      this.app.game = await call(this.op, (p) => {
        this.progress = p;
        this.stage = p.stage === 'apk_extracting' ? 'extracting' : 'checking';
      });
      this.stage = 'ready';
    } catch (e) {
      const err = toAppError(e);
      if (err.code === 'cancelled') {
        this.stage = 'ask';
        return;
      }
      this.error = err;
      this.stage = 'error';
    }
  }

  /** "Open <world>" after the graphics are ready. */
  openWorld() {
    const w = this.world as World;
    this.isOpen = false;
    this.app.openEditor(w);
  }
}
