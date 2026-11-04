// Sets the release version in every file that carries it: `bun scripts/set-version.ts 0.2.0`.
// semantic-release runs it in its prepare step (.releaserc.json); the files are the `assets` of @semantic-release/git.
import { readFileSync, writeFileSync } from 'node:fs';
import { setVersion } from './lib/release';

const next = process.argv[2] ?? '';
const out = setVersion((f) => readFileSync(f, 'utf8'), next);
for (const [file, text] of Object.entries(out)) writeFileSync(file, text);
console.log(`version -> ${next}: ${Object.keys(out).join(', ')}`);
