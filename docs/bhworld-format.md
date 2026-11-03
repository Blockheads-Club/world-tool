# `.bhworld` format (version 1)

A `.bhworld` file holds one The Blockheads world for moving it between devices. It is a ZIP archive
(deflate or stored entries, no encryption, ZIP64 allowed for huge files) with this layout:

```text
manifest.json        required, first entry
preview.png          optional
world/<path>         one entry per world file, e.g. world/world_db/data.mdb, world/map/default/12_3.png
```

Directory entries (names ending in `/`) may appear and are ignored. Entries other than the above at the top level are
ignored by version 1 readers (room for additive extensions); everything below `world/` must be listed in the manifest.

## `manifest.json`

UTF-8 JSON, camelCase keys. Unknown keys are ignored by readers.

```json
{
  "format": "bhworld",
  "formatVersion": 1,
  "world": { "id": "0123456789abcdef0123456789abcdef", "name": "Home", "gameVersion": "1.7.6" },
  "source": { "kind": "android-backup", "deviceName": "Pixel 7" },
  "createdAt": "2024-05-01T10:00:00Z",
  "exportedAt": "2026-11-03T18:20:00Z",
  "generator": "bhw-world 0.1.0",
  "files": [
    { "path": "map/default/0_0.png", "size": 1234, "sha256": "…64 lowercase hex…" },
    { "path": "world_db/data.mdb", "size": 2097152, "sha256": "…" }
  ],
  "preview": { "path": "preview.png", "size": 5120, "sha256": "…" }
}
```

| Key | Required | Meaning |
| --- | --- | --- |
| `format` | yes | Always `"bhworld"`. |
| `formatVersion` | yes | `1`. Readers reject versions they do not know (newer) and `0`. Additive, backwards compatible changes keep the version; anything an old reader would misinterpret bumps it. |
| `world.id` | yes | 32 lowercase hex digits: the world's folder name in the game's `saves/` directory. |
| `world.name` | no | Display name. |
| `world.gameVersion` | no | Game version that last saved the world. |
| `source.kind` | yes | `android-backup`, `android-root`, `blockheads-pc`, `blockheads-server`, `ios-backup`, `bhworld` or `unknown`. Readers map unknown values to `unknown`. |
| `source.deviceName` | no | Human readable device name. |
| `createdAt` | no | World creation time, RFC 3339 UTC. |
| `exportedAt` | yes | Export time, RFC 3339 UTC. |
| `generator` | no | Program that wrote the file. |
| `files` | yes | Every file below `world/`, sorted by `path`. `path` is relative to `world/`, `/`-separated, without empty, `.` or `..` components, backslashes, `:` or NUL. `size` in bytes, `sha256` lowercase hex of the contents. |
| `preview` | no | Present iff `preview.png` is; `path` is always `preview.png`. |

## World files

Exactly the files of the world directory, unchanged: for a game world (Android, Blockheads PC) `world_db/data.mdb`,
`world_db/lock.mdb` and `map/default/*.png`; a Blockheads Server world additionally has `server_db/` and `lightBlocks/`
(64-bit LMDB, not loadable by the game without conversion). `world_db/data.mdb` must be present for the archive to be
importable as a world.

## Validation on open (reference implementation: `crates/bhw-world/src/bhworld.rs`)

1. Every entry name is a safe relative path (no traversal, absolute paths, backslashes, drive letters).
2. `manifest.json` is at most 16 MiB (enforced while decompressing), parses, and passes the rules above.
3. The set of `world/` entries equals the manifest's `files` exactly.
4. Limits (defaults): at most 100 000 files, 4 GiB per file, 4 GiB in total, 8 MiB preview; an entry larger than
   1 MiB may not have a compression ratio above 1000:1. Sizes are checked against the manifest, the zip header and
   the decompressed stream (reading stops one byte past the declared size).
5. SHA-256 of every file (and the preview) matches.

## Writing

Writers put `manifest.json` first, then `preview.png`, then the world files in manifest order. PNGs are stored, other
files deflated. Writers write to a temporary file and rename it into place.
