// Release helpers used by the CLI scripts in scripts/ (tested in release.test.ts, 100 % coverage).

export const REPO = 'Blockheads-Club/world-tool';

const esc = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
const nl = '\\r?\\n';

/** The files that carry the app version and the pattern around it (`before`, version, `after`). */
export function versionEdits(current: string): [string, RegExp][] {
  const v = esc(current);
  return [
    ['app/package.json', new RegExp(`("version": ")${v}(")`)],
    ['app/src-tauri/tauri.conf.json', new RegExp(`("version": ")${v}(")`)],
    ['app/src-tauri/Cargo.toml', new RegExp(`(${nl}version = ")${v}(")`)],
    ['app/src-tauri/Cargo.lock', new RegExp(`(name = "bh-world-tool-app"${nl}version = ")${v}(")`)]
  ];
}

export const VERSION_RE = /^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$/;

/**
 * Sets `next` in every file of `versionEdits`. Every pattern must match, so a moved version string fails the
 * release instead of shipping a half-bumped tree. Returns the new contents by file.
 */
export function setVersion(read: (file: string) => string, next: string): Record<string, string> {
  if (!VERSION_RE.test(next)) throw new Error(`not a version: ${next}`);
  const current: string = JSON.parse(read('app/package.json')).version;
  const out: Record<string, string> = {};
  for (const [file, re] of versionEdits(current)) {
    const text = read(file);
    if (!re.test(text)) throw new Error(`${file}: ${current} not found (${re})`);
    out[file] = text.replace(re, (_, before: string, after: string) => before + next + after);
  }
  return out;
}

/**
 * Tauri updater platform keys and the release file each installs. The updater first looks for
 * `{os}-{arch}-{bundle}` (the bundle the app was installed from), then `{os}-{arch}`.
 * `required` platforms must be in every release; the others are added when their signed file exists.
 */
export const UPDATER_PLATFORMS: { key: string; suffix: string; required: boolean }[] = [
  { key: 'windows-x86_64', suffix: '_x64-setup.exe', required: true },
  { key: 'windows-x86_64-nsis', suffix: '_x64-setup.exe', required: false },
  { key: 'windows-x86_64-msi', suffix: '_x64_en-US.msi', required: false },
  { key: 'windows-aarch64', suffix: '_arm64-setup.exe', required: true },
  { key: 'windows-aarch64-nsis', suffix: '_arm64-setup.exe', required: false },
  { key: 'windows-aarch64-msi', suffix: '_arm64_en-US.msi', required: false },
  { key: 'linux-x86_64', suffix: '_amd64.AppImage', required: true },
  { key: 'linux-x86_64-appimage', suffix: '_amd64.AppImage', required: false },
  { key: 'linux-x86_64-deb', suffix: '_amd64.deb', required: false },
  { key: 'linux-x86_64-rpm', suffix: '.x86_64.rpm', required: false },
  { key: 'darwin-x86_64', suffix: '_x64.app.tar.gz', required: true },
  { key: 'darwin-x86_64-app', suffix: '_x64.app.tar.gz', required: false },
  { key: 'darwin-aarch64', suffix: '_aarch64.app.tar.gz', required: true },
  { key: 'darwin-aarch64-app', suffix: '_aarch64.app.tar.gz', required: false }
];

export interface SignedFile {
  signature: string;
  size: number;
}

export interface UpdaterManifest {
  version: string;
  notes: string;
  pub_date: string;
  platforms: Record<string, { signature: string; url: string; size: number }>;
}

/**
 * The updater's `latest.json` for a release. `files` maps release asset names to their `.sig` content and size
 * (only assets with a signature). Throws when a required platform has no signed file.
 */
export function buildUpdaterManifest(opts: {
  version: string;
  tag: string;
  files: Record<string, SignedFile>;
  notes?: string;
  pubDate: string;
}): UpdaterManifest {
  const base = `https://github.com/${REPO}/releases/download/${opts.tag}/`;
  const platforms: UpdaterManifest['platforms'] = {};
  for (const { key, suffix, required } of UPDATER_PLATFORMS) {
    const name = Object.keys(opts.files).find((n) => n.endsWith(suffix));
    if (!name) {
      if (required) throw new Error(`no signed updater file for ${key} (*${suffix})`);
      continue;
    }
    const f = opts.files[name]!;
    platforms[key] = { signature: f.signature.trim(), url: base + encodeURIComponent(name), size: f.size };
  }
  return { version: opts.version, notes: opts.notes ?? '', pub_date: opts.pubDate, platforms };
}
