import type { AppStore } from '../store.svelte';
import type { AdbDownload, AdbStatus, AppError, Progress } from '../types';
import { toAppError } from '../errors';
import { opId } from '../util';

export type AdbStage = 'loading' | 'offer' | 'downloading' | 'done' | 'error';

/**
 * "Get the phone tool" dialog: BH World Tool does not ship Google's adb, so on first run it offers to download the
 * official SDK Platform-Tools from Google (source and size shown first; progress; cancel).
 */
export class AdbFlow {
  status = $state<AdbStatus | null>(null);
  isOpen = $state(false);
  stage = $state<AdbStage>('loading');
  info = $state<AdbDownload | null>(null);
  progress = $state<Progress | null>(null);
  error = $state<AppError | null>(null);
  private op = '';

  constructor(private app: AppStore) {}

  /** adb is missing (and can be downloaded on this OS). */
  get missing(): boolean {
    return this.status !== null && !this.status.found;
  }

  async load() {
    this.status = await this.app.api.adbStatus();
  }

  /** Opens the offer and asks the server for the size. */
  async open() {
    this.isOpen = true;
    this.stage = 'loading';
    this.error = null;
    this.progress = null;
    try {
      this.info = await this.app.api.adbDownloadInfo();
      this.stage = 'offer';
    } catch (e) {
      this.error = toAppError(e);
      this.stage = 'error';
    }
  }

  async download() {
    this.op = opId('adb');
    this.stage = 'downloading';
    this.error = null;
    this.progress = null;
    try {
      this.status = await this.app.api.downloadAdb(this.op, (p) => (this.progress = p));
      this.stage = 'done';
      void this.app.refreshDevices();
    } catch (e) {
      const err = toAppError(e);
      if (err.code === 'cancelled') {
        this.stage = 'offer';
        return;
      }
      this.error = err;
      this.stage = 'error';
    }
  }

  /** Closes the dialog (cancelling a running download). */
  close() {
    if (this.stage === 'downloading') void this.app.api.cancel(this.op);
    this.isOpen = false;
  }
}
