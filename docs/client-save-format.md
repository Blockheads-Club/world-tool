# The Blockheads client save format (game 1.7)

This describes how the game itself (Android, and Blockheads PC, which runs the Android build) stores worlds,
and how that differs from Blockheads Server worlds. It was written from our own inspection of save files
(15 Blockheads PC worlds, their `game_db`, and one world written by our own Blockheads Server). It is based on the
files alone. `crates/bhw-save` implements it.

Status tags: **[obs]** seen in real files; **[inf]** inferred from the files, not proven by experiment.

## 1. Files

The game's data directory (`Library/Application Support/` on Blockheads PC; the app's private files on Android)
contains:

```
game_db/                 LMDB environment: the world list (section 6)              [obs]
saves/<saveID>/          one folder per world                                      [obs]
    world_db/data.mdb    LMDB environment: the whole world (sections 3–5)
    world_db/lock.mdb    LMDB lock file (recreated by LMDB; never copy it)
    map/default/         explored-map tiles of the local player (section 7)
    map/<playerID>/      in a cached multiplayer world: tiles per player id
game/                    unrelated: small gzip XML plists named by hashes, and `recentConnections`
                         (an XML plist array of servers joined: host, port, playerID, saveID, userName,
                         worldName, connectionDate)                                   [obs]
```

- `<saveID>` is 32 lower-case hex digits and equals the `saveID` in the world record. [obs]
- No `server_db/`, `lightBlocks/`, `worldv2` file or text lists, which Blockheads Server worlds have. [obs]
- A folder without a world record can exist: one sample has an empty `world_db` (only empty `dw` and `main`
  databases, no `blocks`) and no `game_db` entry, apparently a world whose creation was abandoned. [obs]

## 2. LMDB environments

| env | layout | page | map size | named databases |
| --- | --- | --- | --- | --- |
| `saves/<id>/world_db` | 32-bit (ARMv7 build) | 4096 | 64 MiB | `blocks`, `dw`, `main` |
| `game_db` | 32-bit | 4096 | 32 MiB | `main` |

The 32-bit layout is recognised by the meta page magic `0xBEEFC0DE` at file offset 12 (64-bit: offset 16).
All named databases are created without flags (memcmp key order, no duplicates). The persistent environment flags
are `0x08`. The map size is only a stored number; the game grows it when it opens a larger file [inf], so a world
converted from a server may carry a larger map size. A 64-bit iOS build would write the 64-bit layout [inf]; the
reader handles both.

## 3. `blocks`: terrain

Identical to Blockheads Server [obs]:

- Key `"<x>_<y>"`: macro block coordinates in decimal, no padding. A macro block is 32 × 32 tiles. `x` runs from 0
  to `worldWidthMacro - 1`, `y` from 0 (bottom of the world) to 31; the world is 1024 tiles high.
- Only blocks the game generated or loaded are stored: 213–1410 blocks in the samples, scattered over the whole
  width (the generator probes far away from the spawn), mostly at `y` 15–18.
- Value: a gzip stream that inflates to exactly 65541 bytes: 1024 tiles × 64 bytes, then a version byte and a
  little-endian `u32` update counter. Tile `i` is at local `(i % 32, i / 32)`, row 0 at the bottom.
- Observed trailers: `00 00000000` (freshly generated) and `08 nn000000` (blocks the 1.7 game updated; counter
  up to 0x25).
- The gzip header of client blocks has OS byte 7; the server writes OS byte 3. Both read either.
- Tile record: byte 0 foreground tile type, byte 1 back wall type, byte 3 contents, byte 4 "partial content left"
  (255 for every non-air tile in real saves, 0 for air), bytes 0x28–0x2f the id of the dynamic object that owns
  the tile (trees own their trunk and leaf tiles); all fields are named in `crates/bhw-content/src/tile.rs`.
  Tile type ids are listed in `crates/bhw-save/src/tiles.rs`.
- A block the editor creates where the game has not generated the world (filled with the world's sky tile, version
  0, counter 0) loads in 1.7.6; the game keeps its contents, raises the version to 8 and generates the blocks
  around it as usual (emulator, 2026-11-05).

## 4. `dw`: dynamic objects

Identical to Blockheads Server [obs]: key `"<x>_<y>/<objectType>"` (macro block and object type, e.g. `216_16/45`
for the spawn portal), value an XML plist `{dynamicObjects: [ {…}, … ]}` with the object dictionaries (trees,
plants, animals, items on the ground, workbenches…). 1–20 records per sample world.

## 5. `main`: world record and blockheads

All values are XML plists in Apple's dialect (`-//Apple//DTD PLIST 1.0//EN`, tab indentation). [obs]

| key | content |
| --- | --- |
| `worldv2` | the world record (below) |
| `dynamicWorldv2` | `{activeBlockheadIndex, dynamicObjectIDCount, saveVersion: 8, savedGlowIndices (keyed archive), workbenchHasBeenCrafted}` |
| `blockheads` | `{dynamicObjects: [ {name, uniqueID, pos_x, pos_y, floatPos, skinOptions, state, actions, …} ]}`: the player's blockheads, **uncompressed** |
| `blockhead_<uniqueID>_inventory` | array of slots, each an array of item `<data>` (one per item in the stack), **uncompressed**; one per blockhead |

A client's **cache of a multiplayer world** (record has `remoteGame = true`; folder named after the server's
world id) has an empty `blocks` database and keys `<playerID>_blockheads`, `<playerID>_blockhead_<n>_inventory`
(GNUstep-dialect XML as received from the server) and `local_blockheads` (gzip). [obs] It is not a playable
single-player world and cannot be converted.

Item records are 8 bytes `{u16 type, u16 dataA, u16 dataB, u8 selected sub-slot, u8 ?}` (little-endian), plus a
gzip plist for containers. The last byte is 0x11 or 0xbc on the first item of each stack in saves the game made, 0 on
the others; the game accepts 0 everywhere and writes 0 back for items it loaded that way (emulator), so it looks like
uninitialised padding. Writers use 0. A blockhead's `state` data holds its needs as `f32` at 0x20 (health), 0x24
(happiness), 0x28 (fullness), 0x2c (energy); the game lets fullness and energy go a little above 1.

The day: `worldTime` counts seconds; a day is **3600** of them and `worldTime` 0 is midnight (checked in the game:
3600 shows night, 4500 dawn, 5400 noon).

### The world record (`main/worldv2`)

| key | type | meaning |
| --- | --- | --- |
| `saveID` | string | world id = folder name |
| `worldName` | string | display name (the game stores it upper-case) |
| `creationDate` | date | creation time (UTC, second precision) |
| `saveDate` | date | last save = "last played" |
| `expertMode` | bool | expert mode |
| `customRulesDict` | dict | only for custom-rules worlds |
| `worldWidthMacro` | integer | world width in macro blocks (512 = normal size; 2048 = 4× seen) |
| `randomSeed` | integer | generation seed |
| `saveVersion` | integer | 1100 (save format; the game version itself is not recorded) |
| `worldTime`, `noRainTimer` | real | simulation clocks |
| `startPortalPos.x`, `startPortalPos.y` | integer | spawn portal tile |
| `translation` | array of 2 reals | camera position |
| `portalLevel` | integer | |
| `migrationComplete_1.7`, `runAtLaunch`, `remoteGame` | bool | |
| `distanceOrderedFoodTypes` | data | 31 little-endian int32 |
| `circumNavigateBooleansData`, `foundItems` | data | binary keyed archives of index sets |
| `blockheadDatasv2` | array of data | one entry per blockhead for the world list: 20 bytes skin options, 16 zero bytes, 4 bytes, then the name in ASCII |
| `previewImageData` | data | the thumbnail: 409600 bytes = 320 × 320 RGBA, **bottom row first** (a screenshot of the world around the camera) |
| `tutorial` | dict | only in tutorial-started worlds: `{state, impromptuNightState, wrongToolHasBeenDisplayed}` |
| `cloudGame`, `isOwner` | bool | |
| cached multiplayer worlds only: `host`, `port` (string), `playerID`, `userName`, `connectionDate`, `<playerID>_blockheadDatasv2` | | |

## 6. `game_db`: the world list

`game_db/main` holds `"<saveID>_worldv2"` for every world the game lists. [obs] In all 14 listed samples the value
is **byte-for-byte identical** to that world's `main/worldv2`, including the 400 KB thumbnail. The folder without a
world record had no entry. The game evidently builds its world list (name, dates, thumbnail) from these entries
without opening every world [inf]. So:

- A world copied into `saves/` must also be registered in `game_db` (key `<id>_worldv2`, value = its world
  record), or the game will not show it [inf, strongly suggested by the data].
- Deleting a world means removing both the folder and the entry. Renaming means changing `worldName` in both
  places (the game rewrites the entry from the world's own record whenever it saves the world).
- `game_db` must only be rewritten while the game is not running (LMDB).

`bhw_save::GameDb` reads, edits (`register`, `register_checked`, `unregister`, `rename`) and writes it,
keeping all other records.

## 7. `map/`: explored-map tiles

`map/default/<x>_<y>.png` are 32 × 32 RGBA PNG images, one per macro block the local player has explored (file
name = macro coordinates). 1–9 per sample world. They are the game's own rendering of the minimap, created by the
game as the player explores. In cached multiplayer worlds the folder is named after the player id instead of
`default`. [obs] Blockheads Server keeps per-player explored areas differently (`lightBlocks`, 1024 bytes per
macro block and player), so these tiles cannot be converted.

## 8. Differences from Blockheads Server worlds

| | game client | Blockheads Server |
| --- | --- | --- |
| LMDB layout | 32-bit | 64-bit |
| map size | `world_db` 64 MiB | `world_db` 1 GiB, `server_db` 128 MiB, `lightBlocks` 1 GiB |
| `blocks`, `dw` | same | same |
| plist dialect | Apple | GNUstep (both programs read both) |
| world record keys | adds `previewImageData`, `blockheadDatasv2` (filled), `tutorial`, `cloudGame`, `isOwner` | adds `hostPort`, `maxPlayers` (strings); `blockheadDatasv2` empty |
| blockheads | `blockheads` + `blockhead_<n>_inventory`, uncompressed | `blockheads` (empty); per player `<playerID>_blockheads` and `<playerID>_blockhead_<n>_inventory`, gzip |
| other files | `map/` tiles | `worldv2` file (small copy of the record), `server_db` (player records, `recentPlayers`), `lightBlocks`, `adminlist.txt` & co., our server's `mods/` |
| world list | `game_db` entry required | none (folders are scanned) |

## 9. Conversion (`bhw_save::convert`)

- **Client → server:** `world_db` records are copied unchanged into a 64-bit file; a `worldv2` file is made from
  the record (GNUstep XML, `migrationComplete_1.7 = true`); empty `server_db` and `lightBlocks` are created. The
  `map/` tiles are kept in `bhw-client-files/` (the server ignores it). Single-player blockheads stay in
  `blockheads`; a server loads them as blockheads without a player. The server drops client-only record keys
  (thumbnail, `tutorial`, …) when it saves. Cached multiplayer worlds are refused.
- **Server → client:** records are copied into a 32-bit file with a 64 MiB (or larger, if needed) map. All
  server-only files go to `bhw-server-files/`. Per-player blockheads stay in their records, which single-player
  ignores; the `adopt_player` option copies one player's blockheads and inventories (uncompressed) to
  `blockheads` / `blockhead_<n>_inventory`. The world has no thumbnail until the game saves it. The caller must
  register it in `game_db`.
- Converting back restores the set-aside folder, so client → server → client and server → client → server keep
  every record and file (checked on all real samples). With `keep_extras = false` (e.g. when writing to a phone)
  the set-aside data is reported as lost instead.

## 10. Identity ("keep both")

The id is stored in the folder name, in `main/worldv2` (`saveID`), in the server's `worldv2` file, and as the key
of the client's `game_db` entry. The name is in `main/worldv2` (`worldName`), the server's `worldv2` file and the
`game_db` entry. No other record contains either. `bhw_save::identity::set_identity` rewrites the record(s);
the caller renames the folder and updates `game_db`.

## 11. Open questions

- Whether the game really ignores folders without a `game_db` entry, and whether it accepts an entry whose
  thumbnail is missing (server-made worlds), should be confirmed in the game.
- What a dedicated server does with single-player blockheads over time (they are loaded without an owner).
- Whether the game recreates `map/default/` itself (conversions create it empty).
