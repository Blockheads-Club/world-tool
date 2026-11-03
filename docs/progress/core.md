# Core stream (`crates/bhw-core` + `app/src-tauri` wiring)

Branch `feat/core`. Build with `CARGO_TARGET_DIR=/home/kahverengi/Projects/blockheads-club/world-tool/target`
(also for `app/src-tauri`).

## 2026-11-03

- Started. Merged `dev` (bhw-assets, bhw-device GameStopped fix).
- Plan: `bhw-core` = all app logic behind a `Core` service (settings, activity log, library index, read sessions,
  transfer plans, backups store, game files, cancel registry); `src-tauri` = thin glue (blocking tasks, Channel
  progress, devices-changed polling, pickers/opener).
- Done: `bhw-core` modules (error/types/settings/activity/util/worlds/library/devices/read/transfer/backups/files/
  gamefiles). `tests/real_device.rs` (gated `BHW_DEVICE_SERIAL`) passes on the API 30 emulator: read 2 worlds →
  save to library → export .bhworld + server folders → copy ROCKY OCEAN back as "keep both" (fresh backup stored as
  automatic backup, rewrite incl. game_db, restore; 2 confirm taps) → re-read shows "ROCKY OCEAN (2)" → restore the
  automatic backup → re-read shows the original 2 worlds. ~50 s.
- Fix in bhw-device: `stop_game` retries `am kill` (the game process needs a moment after HOME; seen as
  GameRunning on the emulator).
- Next: merge dev (bhw-ios) and wire iOS read-only; unit tests to 100 %; src-tauri glue; UI error codes.
