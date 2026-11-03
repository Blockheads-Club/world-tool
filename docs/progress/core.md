# Core stream (`crates/bhw-core` + `app/src-tauri` wiring)

Branch `feat/core`. Build with `CARGO_TARGET_DIR=/home/kahverengi/Projects/blockheads-club/world-tool/target`
(also for `app/src-tauri`).

## 2026-11-03

- Started. Merged `dev` (bhw-assets, bhw-device GameStopped fix).
- Plan: `bhw-core` = all app logic behind a `Core` service (settings, activity log, library index, read sessions,
  transfer plans, backups store, game files, cancel registry); `src-tauri` = thin glue (blocking tasks, Channel
  progress, devices-changed polling, pickers/opener).
