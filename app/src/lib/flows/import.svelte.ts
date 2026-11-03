import { t } from '../i18n.svelte';
import type { AppStore } from '../store.svelte';
import type { Choice, ImportItem } from '../types';
import { toast } from '../ui/toast.svelte';

/** Import world dialog: browse or drop `.bhworld` files and server folders, choose per conflict (WT-Library). */
export class ImportFlow {
  isOpen = $state(false);
  items = $state<ImportItem[]>([]);
  choices = $state<Record<string, Choice>>({});
  busy = $state(false);

  constructor(private app: AppStore) {}

  get importable(): ImportItem[] {
    return this.items.filter((i) => i.status !== 'invalid' && this.choices[i.path] !== 'skip');
  }

  async open(paths: string[] = []) {
    this.isOpen = true;
    this.items = [];
    this.choices = {};
    if (paths.length) await this.add(paths);
  }

  close() {
    this.isOpen = false;
  }

  async add(paths: string[]) {
    try {
      const found = await this.app.api.inspectImport(paths);
      const known = new Set(this.items.map((i) => i.path));
      const fresh = found.filter((i) => !known.has(i.path));
      this.items = [...this.items, ...fresh];
      for (const i of fresh) if (i.status === 'conflict') this.choices[i.path] = 'keep_both';
    } catch (e) {
      this.app.fail(e);
    }
  }

  async browse(folders: boolean) {
    const paths = await this.app.api.pickImport(folders);
    if (paths.length) await this.add(paths);
  }

  async run() {
    this.busy = true;
    try {
      const done = await this.app.api.importWorlds({ paths: this.items.map((i) => i.path), choices: this.choices });
      toast(t('imp.done', { n: done.length }));
      this.isOpen = false;
      await this.app.loadWorlds();
    } catch (e) {
      this.app.fail(e);
    } finally {
      this.busy = false;
    }
  }
}
