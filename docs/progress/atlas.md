# Atlas, material rules and blockhead moves — progress

Branch `feat/atlas` (worktree `../world-tool-wt/atlas`, from `dev`). Method: `docs/editor-atlas.md` (test worlds
made with the editor's own `Session` code, opened in 1.7.6 on the `bhwt_api30` emulator, screenshots compared with
the user's own atlas; scripts are local scratch, not committed).

## 2026-11-05 — done (stop point)

### Materials (why gems, flint, clay, oil did not show)
- Real worlds (15 PC saves) and the clean-room server's world generator: flint/clay only in dirt (and grassy dirt),
  oil only in limestone, ores in rock. Gems are **not** contents of a rock tile: caves hold a gem promise in the
  foreground contents byte (0x0b) of an air/water tile; the game uncovers it (gem + a light object in `dw`) when
  light or a blockhead comes near. Charcoal (45) never occurs and does not show.
- `bhw_editor::brush`: `host_tile` (the tile each material lives in), `Brush::is_valid` only accepts those pairs,
  `set_material` writes gems as their promise (and removes a gem when the tile is painted over), `material` reads
  either byte (view, replace/search matching, schematics), `detached` turns an uncovered gem back into a promise
  when its tile is edited with objects allowed; uncovered gems count as protected (their light is a `dw` object).
  Catalog: `tile` of placeable contents = host tile; the UI palette and the picker use it.
- Emulator: flint, clay, oil, platinum, titanium and all five gems (placed by the editor's own brush) show in the
  game; charcoal does not.

### Atlas
- Confirmed in the game: contents 1 [0,9], 2 [2,9], 64 [14,2], 106 [29,4], 107 [27,6], gems 51/53/55/57/59
  [28,3]/[29,3]/[25,3]/[26,3]/[27,3]; tiles 58 [25,2], 68 [24,4], 70 [30,14]; limestone 12 [2,2] vs marble 14
  [3,2], sandstone 17 [15,2], red marble 19, blocks 13/15/18 re-checked.
- No cell on purpose: 21–23 flax mats and 66 fallen leaves (no front face drawn on their own), 56/57/76/77 (white
  cells the game tints; plain colours used, iron/steel colours adjusted to the game), air.
- Counts: tiles 69 with a cell (66 game, 3 eye: 3, 4, 21); contents 18 (15 game, 3 eye: 66, 70, 78).

### Blockhead move ("invisible blockhead")
- Cause: the game opens at the saved camera (`translation`), not at the blockheads; a blockhead moved away was
  just off screen (selecting it showed it). Also seen: a blockhead put high in the air or in an ungenerated area
  fell and died, one put in a flooded cave started to drown, one in an unlit cave is black (game behaviour).
- `Session::move_blockhead` now returns the spot used: up out of solid ground and water to the first air tile,
  down to whatever is below; refused outside stored blocks or above lava. It sets `translation` to the blockhead in
  the same history step (undo restores the camera). Emulator: far move in the same row of blocks, inside rock,
  in the air, across the world wrap (x 16383 → 1) all load with the blockhead on screen.
- UI: refused edits (core code `not_allowed`) showed the phone's "allow this computer" dialog; the editor now shows
  its own toast (`edv.moveRefused`, `edv.editRefused`, en/tr/ru/kk).

### Checks
- Rust: bhw-content, bhw-editor 100 % lines/functions (the shared target's summary counts 1 line in `session.rs`
  that `--show-missing-lines`/`--text` do not list — same artefact as noted in `editor.md`); bhw-core editor tests
  pass; clippy -D warnings, fmt. App: vitest 121 tests, 100 % (`--maxWorkers=2`), svelte-check 0/0.
- Emulator left with its two original worlds, then shut down.

### Open questions
- Flax mats / fallen leaves: how the game draws them (maybe only on top of specific tiles or as floor decals).
- Tinted tiles (iron, steel, plaster, luminous plaster): which white cell the game uses — not distinguishable from
  screenshots; plain colours meanwhile.
- Ice melted in the test spot (warm biome); re-check in a cold biome. Water/lava are drawn specially.
- Should the editor also mark the destination as explored / lit? The game shows unexplored land dark until the
  blockhead walks there; left as the game does it.
- Other editor refusals still send English detail text only (the toast is generic now).

### Next
- Merge `feat/atlas` into `dev` after review.

## 2026-11-05 (night) — leftovers done on `feat/finish`
- Water 3 → [14,0] (wavy pair, game), ice 4 → [12,3] (game, seen before it melted; no cold biome in any save),
  flax mats / fallen leaves are not drawn by the game at all, iron/steel/plaster faces are uniform: plain colours
  measured in the game. Details in `docs/editor-atlas.md` and `docs/progress/finish.md`.
