# Game files from the user's APK

Phase 2 (world viewer/editor) draws blocks, items and creatures with graphics taken from the user's **own** The
Blockheads APK. Nothing is bundled, downloaded or committed: `crates/bhw-assets` validates the APK the user picks and
copies the graphics into the app's cache directory. This page records what those files are (research on 1.7.4, 1.7.5,
1.7.6, 2026-11-03). No game graphics are in this repository.

## Supported APKs

| versionName | versionCode | Package |
| --- | --- | --- |
| 1.7.4 | 1548230154 | `com.noodlecake.blockheads` |
| 1.7.5 | 1557164436 | `com.noodlecake.blockheads` |
| 1.7.6 | 1564553369 | `com.noodlecake.blockheads` |

The three APKs are single (non-split) APKs of ~95 MB. Their `assets/GameResources/` trees are **identical** (723
entries, same CRC-32 for every entry), so one atlas description covers all supported versions. Every
`GameResources` entry is *stored* (uncompressed) in the zip.

Accepted sources: a single `.apk`, an `.apks`/`.xapk` bundle (zip of APKs, nested APKs stored or deflated) or a
folder of base + split APKs. The base APK is the one whose manifest has no `split` attribute. Errors:
`NotAnApk`, `WrongApp(package)`, `Unsupported(versionName)`, `Damaged(reason)` (corrupt zip/manifest/CRC),
`Incomplete(missing)` (no base APK, or the required atlases are in no part of the set).

## What is extracted

Every `.png` and bitmap-font `.fnt` under `assets/GameResources/` except the `instructions/` help pages: **461
files, 22 798 959 bytes** (HD + standard resolution). Skipped: audio (`.wav`, 152 files), music (`.mp4`, 9),
shaders (`.fsh`/`.vsh`, 92), fonts in `.ttf`/`.otf`, `defaultPrices`, HTML, nibs, dex/native code and Android `res/`.

Cache layout: `<cache>/game-files/<versionName>-<first 12 hex of sha256>/` with the files at their path relative to
`assets/GameResources/` (e.g. `HDTex/TileMap.png`) and `manifest.json` (source path/kind, sha256, package, version,
`extracted_at` unix seconds, per file: path, size, sha256, PNG width/height). `<cache>/game-files/current` names the
active directory. Extraction goes to `.tmp-*` and is renamed into place when complete.

Required for the editor (missing ⇒ `Incomplete`): `HDTex/TileMap.png`, `HDTex/Items.png`,
`HDTex/TileDestruct.png`, `HDTex/TileReflect.png`.

## Resolution variants

- Standard resolution: 299 PNGs in `GameResources/` (incl. UI), 24 in `skins/`, 3 in `UI/`, 5 font pages in
  `Fonts/`.
- `GameResources/HDTex/*.png`: 103 HD textures; `HDTex/skins/*`: 22 HD skins.
- HD/standard scale is **4×** for 97 pairs, **8×** for the 22 skins (32×16 → 256×128), and odd ratios for a few
  (`Sun` 16→512, `yakNeck` 16×16→128×64, `sharkSideFin` 2×, `bannedPainting`/`hat_santa`/`tulipFlower` 1×).
  So the editor must not assume one scale factor: use the HD file's own dimensions and the game-space size of the
  sprite (standard file size) to compute scale.
- 206 standard-resolution PNGs (outside `instructions/`) have no HD variant (mostly UI; also `ItemNormals.png`, `light.png`, `Sky*.png`,
  `cloud1–6.png`, `GroundColor*.png`/`CloudColor*.png` 64×1 gradients, `skins/legs.png`, creature parts like
  `dodoWings.png`, `jet1–3.png`, `sail.png`).

## Atlases

| File | Std size | HD size | Structure |
| --- | --- | --- | --- |
| `TileMap.png` | 512×512 | 2048×2048 | Regular grid of **16 px** cells (HD **64 px**), 31 × 26 cells used (x < 496, y < 416; rest transparent), ~775 of 806 used cells non-empty. Strong colour discontinuities on every 16 px (HD 64 px) line, none in between ⇒ one tile texture per cell. Neighbouring cells often touch (seamless terrain textures and edge/corner variants), so cells cannot be found by transparency alone — use the fixed grid. |
| `Items.png` | 512×256 | 2048×1024 | Regular grid of **16 px** (HD **64 px**) icons, 32 × 16 cells, 345 non-empty; almost no pixels cross cell borders. |
| `TileDestruct.png` | 512×512 | 2048×2048 | Fully opaque noise/crack texture with no grid edges (likely sampled as an overlay for damage, not per cell). |
| `TileReflect.png` | 512×512 | 2048×2048 | Fully opaque, strong 16 px (HD 64 px) grid edges: per-cell reflection/specular map aligned with `TileMap`. |
| `ItemNormals.png` | 128×128 | – | Fully opaque normal-map-like texture. |
| `light.png` | 512×256 | – | A few soft light blobs on transparent background, no grid. |
| `TileButtons.png` | 256×256 | – | Almost empty (a 32 px corner glyph). |
| `Moon`/`Sun` | 128² / 16² | 512² / 512² | Single sprites. |

Creature, clothing and armour parts are separate single-sprite PNGs (e.g. `dodoBody` 32×16 → HD 128×64,
`body_ct` 32×16 → 128×64); skins are 32×16 (HD 256×128) per face/hair/singlet variant.

## Sprite-frame metadata

There is **no** sprite-frame metadata (Cocos2D/TexturePacker `.plist`, JSON or XML) for the atlases in the APK. The
only plists are `assets/Info.plist` and `assets/GKAchievements.plist` (iOS leftovers); the `.nib` files are UI
layouts. The only frame descriptions are the 5 BMFont text files in `Fonts/*.fnt` (glyph rectangles for the
`04b03` and `Blockheads` bitmap fonts, `scaleW/scaleH` 512).

## What the editor still needs (not in the APK as data)

The mapping *tile type (+ sub-type, neighbours, damage, ownership…) → TileMap cell(s)* and *item type → Items cell*
lives in the game's native render code (`lib/armeabi-v7a`), together with the per-type rules for edge
and corner variants, foreground/background layers, contents (ore, water levels) and how `TileReflect` /
`TileDestruct` / `light` are combined. It has to be written down as a hand-made table (cell coordinates per tile
type and variant) from clean-room observation of the running game, not copied from decompiled code. Creature
rendering likewise needs part offsets/pivots and animation, which are also in code.
