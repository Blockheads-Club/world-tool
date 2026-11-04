# Finish: refusal reasons, explored land, atlas leftovers — progress

Branch `feat/finish` (worktree `../world-tool-wt/finish`, from `dev`). Not pushed.

## 2026-11-05 (night) — done (resume point)

### 1. Specific refusal reasons
- `bhw_editor::Refusal` (snake_case codes) carried by `Error::Refused { reason, detail }`; every former
  `Error::Invalid`/`Error::Schematic` site has its own code (too_many_changes, area_size, unknown_block,
  material_not_in_tile, no_blockhead, outside_world, not_generated, no_free_tile, lava, no_inventory, no_object,
  no_items, no_record, no_step, stack_too_large, unknown_item, bad_name, region_too_large, bad_schematic,
  newer_schematic); errors of the world's contents that are not damage map to `other`.
- bhw-core: `AppError.reason` (`Extra.reason`, `AppError::refused`); its own refusals `nothing_copied` and
  `multiplayer_world`.
- UI: `refusal()` in `editor/flow.svelte.ts` turns `not_allowed` + reason into `edv.refused.<reason>` (fallback to
  the old general `edv.moveRefused`/`edv.editRefused`); every editor error path goes through it, also the
  "could not open" screen (a multiplayer world no longer shows the phone's "allow this computer" text).
  en/tr/ru/kk (tr formal "siz"; kk uses the existing terms мүкаммал, сызба).
- Skipped tiles (object tiles / ungenerated area with the setting off) were already reported per edit by the
  outcome toast (`edv.n.protected`, `edv.n.missing`); unchanged.

### 2. Explored land around a moved blockhead — not done on purpose (documented)
- Found by observation (emulator, `docs/client-save-format.md` §6): tile byte 0x06 is how far the player explored
  the tile (0 is drawn black); 0x09 always equals it (15 PC saves). The game uncovers a disc of ~12 tiles (+6 fade)
  around a blockhead by itself, which already covers the default-zoom screen.
- Tried: marking a radius of 24 explored as part of the move (implemented, tested, then reverted). The sky became
  visible, but the **ground stayed black**, also right under the blockhead and while the game ran: the game works
  out ground sun light (byte 0x07) only for tiles it uncovers itself and skips tiles already explored. Without
  the marking the ground lights normally. Copying the game's light model (leaves, water, glass, caves) is guesswork,
  so moves keep leaving exploring to the game.
- One load of a pre-explored test world crashed the game (SIGSEGV, tombstone_13); the identical file loaded fine on
  retry and no other run crashed. Older tombstones (09–12) exist from earlier sessions.

### 3. Atlas leftovers
- Water 3 → [14,0] (`game`): drawn translucent with the wavy pair [14,0]/[15,0], like lava [14,1]/[15,1].
- Ice 4 → [12,3] (`game`), seen in the ~10 s before it melts. No cold biome exists in any of the 17 saves (no snow
  or ice tiles; zone byte only 1–3), so ice was checked in the warm test world.
- Flax mats 21–23 and fallen leaves 66: the game draws nothing for them (floating: nothing; on grass: a gap in the
  grass top face). No real save contains them. 21 keeps its `eye` cell.
- Iron 56, steel 57, plaster 76, luminous plaster 77: uniform faces without texture, so the tinted cell cannot be
  told apart and does not matter; plain colours set from the game (face colour / 0.85 daylight factor measured on
  a brick block): iron (86,86,87), steel (172,171,173), plaster (250,246,250), luminous plaster (255,253,255).

### Checks
- Rust: bhw-editor, bhw-content, bhw-core editor/error 100 % lines and functions (`cargo llvm-cov`; run
  `cargo llvm-cov clean --workspace` first — stale profiles from other worktrees double-count files).
  `cargo test --workspace`, clippy -D warnings, fmt, src-tauri clippy/tests pass.
- App: vitest 122 tests, 100 % (`--maxWorkers=2`), svelte-check 0/0.
- Emulator left with its two original worlds (md5 checked), shut down.

### Open questions
- Should the palette still offer flax mats and fallen leaves, given the game draws nothing for them?
- Is the general fallback toast still wanted for `other` (content errors such as a missing inventory slot)?
