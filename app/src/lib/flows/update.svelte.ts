import type { AppStore } from '../store.svelte';
import type { UpdateInfo } from '../types';
import { opId } from '../util';

export type UpdateStage = 'idle' | 'checking' | 'available' | 'downloading' | 'ready' | 'latest' | 'failed';

/** Update check, popup, title-bar chip, download and restart (WT-Update). */
export class UpdateFlow {
  stage = $state<UpdateStage>('idle');
  info = $state<UpdateInfo | null>(null);
  percent = $state(0);
  /** The big popup (available / downloading). */
  popup = $state(false);
  /** The small "latest / couldn't check" popover under the chip. */
  popover = $state(false);
  /** "Restart after the transfer?" */
  waitDialog = $state(false);
  restartWhenDone = $state(false);

  constructor(private app: AppStore) {}

  /** Chip in the title bar while the popup is hidden. */
  get chip(): boolean {
    return !this.popup && (this.stage === 'available' || this.stage === 'downloading' || this.stage === 'ready' || this.popover);
  }

  /** `auto`: the daily check at start-up (a skipped version stays quiet, errors stay quiet). */
  async check(auto = false) {
    this.stage = 'checking';
    try {
      const info = await this.app.api.checkUpdate();
      this.app.settings = await this.app.api.getSettings();
      if (info && !(auto && this.app.settings.skipped_version === info.version)) {
        this.info = info;
        this.stage = 'available';
        this.popup = auto;
      } else {
        this.stage = 'latest';
        this.popover = !auto;
      }
    } catch {
      this.stage = 'failed';
      this.popover = !auto;
    }
  }

  openChip() {
    if (this.stage === 'available' || this.stage === 'downloading') this.popup = true;
    else if (this.stage === 'ready') void this.restart();
    else this.popover = !this.popover;
  }

  later() {
    this.popup = false;
  }

  async skip() {
    this.popup = false;
    this.stage = 'idle';
    await this.app.saveSettings({ skipped_version: (this.info as UpdateInfo).version });
  }

  async download() {
    this.popup = false;
    this.stage = 'downloading';
    this.percent = 0;
    try {
      await this.app.api.downloadUpdate(opId('update'), (p) => {
        this.percent = Math.round(((p.done ?? 0) / (p.total ?? 1)) * 100);
      });
      this.stage = 'ready';
    } catch {
      this.stage = 'failed';
      this.popover = true;
    }
  }

  async restart() {
    if (this.app.busy) {
      this.waitDialog = true;
      return;
    }
    await this.app.api.restartApp();
  }

  waitForTransfer() {
    this.waitDialog = false;
    this.restartWhenDone = true;
  }

  /** A transfer finished: restart now if that was asked for. */
  afterBusy() {
    if (this.restartWhenDone) {
      this.restartWhenDone = false;
      void this.app.api.restartApp();
    }
  }
}
