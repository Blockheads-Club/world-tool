import { describe, expect, test } from 'bun:test';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { buildUpdaterManifest, setVersion, UPDATER_PLATFORMS, versionEdits } from './lib/release';

const root = join(import.meta.dir, '..');
const read = (f: string) => readFileSync(join(root, f), 'utf8');

describe('setVersion', () => {
  test('bumps every file of the real tree', () => {
    const current = JSON.parse(read('app/package.json')).version as string;
    const out = setVersion(read, '9.8.7-rc.1');
    expect(Object.keys(out)).toEqual(versionEdits(current).map(([f]) => f));
    expect(JSON.parse(out['app/package.json']!).version).toBe('9.8.7-rc.1');
    expect(JSON.parse(out['app/src-tauri/tauri.conf.json']!).version).toBe('9.8.7-rc.1');
    expect(out['app/src-tauri/Cargo.toml']).toContain('\nversion = "9.8.7-rc.1"');
    expect(out['app/src-tauri/Cargo.lock']).toContain('name = "bh-world-tool-app"\nversion = "9.8.7-rc.1"');
    // Only the app's own lock entry changes.
    const diff = out['app/src-tauri/Cargo.lock']!.length - read('app/src-tauri/Cargo.lock').length;
    expect(diff).toBe('9.8.7-rc.1'.length - current.length);
  });

  test('rejects bad versions and moved version strings', () => {
    expect(() => setVersion(read, 'v1.0')).toThrow('not a version');
    const broken = (f: string) => (f.endsWith('Cargo.toml') ? '[package]\n' : read(f));
    expect(() => setVersion(broken, '1.0.0')).toThrow('app/src-tauri/Cargo.toml');
  });
});

describe('buildUpdaterManifest', () => {
  const v = '0.2.0';
  const required = {
    [`BH.World.Tool_${v}_x64-setup.exe`]: { signature: 'win\n', size: 10 },
    [`BH.World.Tool_${v}_amd64.AppImage`]: { signature: 'lin', size: 20 },
    [`BH.World.Tool_${v}_x64.app.tar.gz`]: { signature: 'macx', size: 30 },
    [`BH.World.Tool_${v}_aarch64.app.tar.gz`]: { signature: 'maca', size: 40 }
  };

  test('required platforms, bundle-specific keys and URLs', () => {
    const m = buildUpdaterManifest({
      version: v,
      tag: 'v0.2.0',
      pubDate: '2026-11-04T00:00:00Z',
      notes: '* x',
      files: { ...required, [`BH.World.Tool_${v}_amd64.deb`]: { signature: 'deb', size: 5 } }
    });
    expect(m.version).toBe(v);
    expect(m.notes).toBe('* x');
    expect(m.platforms['windows-x86_64']).toEqual({
      signature: 'win',
      size: 10,
      url: `https://github.com/Blockheads-Club/world-tool/releases/download/v0.2.0/BH.World.Tool_${v}_x64-setup.exe`
    });
    expect(m.platforms['windows-x86_64-nsis']).toEqual(m.platforms['windows-x86_64']!);
    expect(m.platforms['linux-x86_64-deb']!.size).toBe(5);
    expect(m.platforms['linux-x86_64-rpm']).toBeUndefined();
    expect(m.platforms['darwin-aarch64']!.signature).toBe('maca');
    const keys = UPDATER_PLATFORMS.filter((p) => p.required).map((p) => p.key);
    for (const k of keys) expect(m.platforms[k]).toBeDefined();
  });

  test('empty notes and a missing required platform', () => {
    const m = buildUpdaterManifest({ version: v, tag: 'v0.2.0', pubDate: 'd', files: required });
    expect(m.notes).toBe('');
    const { [`BH.World.Tool_${v}_amd64.AppImage`]: _, ...noLinux } = required;
    expect(() => buildUpdaterManifest({ version: v, tag: 't', pubDate: 'd', files: noLinux })).toThrow('linux-x86_64');
  });
});
