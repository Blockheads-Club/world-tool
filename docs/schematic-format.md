# Schematic files (`.bhwschem`)

A schematic is a rectangle of tiles copied from a world in BH World Tool's editor, to paste into the same or another
world. The format is this tool's own (it contains no game data beyond tile ids). `crates/bhw-editor/src/schematic.rs`
implements it.

## Layout

| offset | size | content |
| --- | --- | --- |
| 0 | 8 | magic `BHWSCHEM` |
| 8 | 4 | header length `n` (u32, little-endian, at most 65536) |
| 12 | n | header: UTF-8 JSON object |
| 12 + n | rest | gzip stream: `width × height` cells of 16 bytes |

Header fields:

| key | type | meaning |
| --- | --- | --- |
| `format` | string | always `"bhw-schematic"` |
| `version` | integer | format version, currently `1`; readers refuse higher versions |
| `width`, `height` | integer | size in tiles, 1..4096 each |
| `game` | string | game version family the ids belong to (`"1.7"`) |
| `name` | string | name given by the user |
| `source_world` | string, optional | name of the world it was copied from |
| `created` | integer | Unix seconds |

Unknown header keys are ignored (later versions may add some without raising `version`).

Cells are stored row by row from the **bottom** row (the game's `y` grows upwards), left to right. A cell:

| offset | size | content |
| --- | --- | --- |
| 0 | 1 | 1 = present, 0 = absent (the tile was outside the generated world; pasting leaves the target alone) |
| 1 | 1 | foreground tile type |
| 2 | 1 | back wall tile type |
| 3 | 1 | contents id |
| 4 | 5 × 2 | paint of the front, top, right, left and bottom faces (u16 LE, 0 = none) |
| 14 | 2 | reserved, 0 |

## What is copied

Only terrain: tile types, back walls, plain contents (ores, gems, flint, clay, charcoal) and paint. Dynamic objects
(trees, plants, workbenches, chests, doors, ladders, torches, signs…) live in the world's `dw` database and are not
copied; their tiles are copied without their contents. Light, temperature and other derived tile fields are not
stored; the game recomputes them. Pasting never overwrites tiles that belong to an object in the target world.
