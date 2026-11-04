// Writes the Tauri updater manifest latest.json into a folder of release assets (release.yml, publish job).
//   bun scripts/updater-manifest.ts <dir> <version> <tag> [notes-file]
// Every `<asset>.sig` in <dir> is the minisign signature of <asset>; the logic lives in lib/release.ts (tested).
import { readdirSync, readFileSync, statSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { buildUpdaterManifest, type SignedFile } from './lib/release';

const [dir, version, tag, notesFile] = process.argv.slice(2);
if (!dir || !version || !tag) throw new Error('usage: bun scripts/updater-manifest.ts <dir> <version> <tag> [notes-file]');
const files: Record<string, SignedFile> = {};
for (const f of readdirSync(dir)) {
  if (!f.endsWith('.sig')) continue;
  const asset = f.slice(0, -4);
  files[asset] = { signature: readFileSync(join(dir, f), 'utf8'), size: statSync(join(dir, asset)).size };
}
const notes = notesFile ? readFileSync(notesFile, 'utf8') : '';
const manifest = buildUpdaterManifest({ version, tag, files, notes, pubDate: new Date().toISOString() });
writeFileSync(join(dir, 'latest.json'), JSON.stringify(manifest, null, 2) + '\n');
console.log(`latest.json: ${Object.keys(manifest.platforms).join(', ')}`);
