# Release stream: CI, releases, in-app updater

Branch `feat/release`, worktree `../world-tool-wt/release`. Modelled on blockheads-patcher (ci / auto-release /
release workflows, semantic-release, Tauri updater) and blockheads-server (workspace coverage job).

## How it works
- `.github/workflows/ci.yml` (push to dev/main, PRs, `workflow_dispatch`):
  - `rust-lint`: crates workspace `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked -D warnings`.
  - `rust-coverage`: `cargo llvm-cov --workspace --fail-under-lines 100 --fail-under-functions 100` (all crate tests;
    real-device/real-save tests skip). Java 21 installed so `bhw-backup/tests/java_xcheck.rs` really runs.
  - `lmdb-i686`: `gcc-multilib` + `cargo test --target i686-unknown-linux-gnu -p bhw-lmdb` (real 32-bit LMDB).
  - `app`: `bun run check` (svelte-check) + `bun run test` (vitest, 100 % thresholds in `app/vite.config.ts`).
  - `e2e`: Playwright smoke test on the mock backend.
  - `scripts`: root `bun test --coverage scripts` (100 %, `bunfig.toml`) for the release helpers.
  - `tauri`: `app/src-tauri` is its own workspace → its own `cargo fmt --check`, clippy -D warnings, `cargo test`.
  - `desktop-check`: Windows + macOS `cargo check` of src-tauri and `cargo test --workspace` of the crates.
- `.github/workflows/auto-release.yml`: after a green ci run for a push to `dev`, `bunx semantic-release`
  (`.releaserc.json`, deps in the root `package.json`) bumps the version (`scripts/set-version.ts`: app/package.json,
  tauri.conf.json, src-tauri Cargo.toml + Cargo.lock), commits `chore(release): vX.Y.Z`, tags, opens a draft release
  with the notes, then dispatches `release.yml -f tag=… -f publish=true`.
- `.github/workflows/release.yml`: builds windows-x64 (nsis, msi), linux-x64 on 22.04 (AppImage with adb; deb + rpm
  without, see `docs/adb-sidecar.md`), macos-arm64 and macos-x64 (app, dmg); signs updater artifacts when the
  `TAURI_SIGNING_PRIVATE_KEY*` secrets exist; publish job normalises names, writes `latest.json`
  (`scripts/updater-manifest.ts`, notes = release body), `SHA256SUMS.txt`, uploads everything, publishes.
  Tags with a dash (`v0.2.0-rc.1`) are prereleases; plain `v0.x.y` are normal releases. Manual dispatch with an
  empty tag = dry run (workflow artifacts only).
- adb: `scripts/fetch-adb.sh` downloads pinned Platform-Tools r37.0.1 (SHA-256 checked) per target and returns the
  `externalBin`/`resources` config fragment.
- Updater: `tauri-plugin-updater` in `app/src-tauri` (`src/update.rs` + commands in `lib.rs`). Endpoint
  `https://github.com/Blockheads-Club/world-tool/releases/latest/download/latest.json` (latest non-prerelease),
  public key in `tauri.conf.json` `plugins.updater.pubkey`. `check_update` → `UpdateInfo` (version, date ms, size
  from our manifest's per-platform `size`, notes = bullet lines of the release body) and records
  `last_update_check`; `download_update` downloads + verifies with `Progress{stage: downloading}`; `restart_app`
  installs the downloaded package, then restarts (Windows: passive installer restarts the app).
- `.bhworld` association: `bundle.fileAssociations` + macOS `exportedType` (UTI `club.blockheads.worldtool.bhworld`)
  + a shared-mime-info XML (`app/src-tauri/linux/bh-world-tool-mime.xml`) installed by deb/rpm + a desktop template
  (`linux/bh-world-tool.desktop`) whose `Exec` has `%F` (Tauri's default has none, so a double-clicked file was not
  passed to the app).

## Secrets (repo Blockheads-Club/world-tool → Settings → Secrets → Actions)
- `TAURI_SIGNING_PRIVATE_KEY`: content of `release-secrets/updater.key` (outside the repo, next to the worktrees).
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`: content of `release-secrets/updater.key.password`.
- Public key (`updater.key.pub`) is already in `tauri.conf.json`. Losing the private key means installed apps can
  never auto-update again: back it up.

## Log
- 2026-11-04: workflows, semantic-release config, set-version/updater-manifest scripts (+ tests), fetch-adb,
  updater commands, mime XML + exportedType. Keypair generated (minisign, password-protected). actionlint clean.
  Crates workspace coverage today: 100 % lines (23 667), 100 % functions. src-tauri: `update.rs` lines fully hit
  by its tests (generic `Slot` instantiations with the real `Update` count as missed functions); `lib.rs` command
  glue ~20 %, no gate for src-tauri yet (same as before).

- 2026-11-04 (later): local release build on this Arch machine with the real key: deb + rpm built and signed
  (`.deb.sig`, `.rpm.sig` written, so `linux-x86_64-deb/-rpm` manifest keys work); deb contains the mime XML, the
  `%F` desktop file and no adb. AppImage: AppDir has `usr/bin/adb` next to the exe, but linuxdeploy fails on Arch
  (known host issue; CI uses ubuntu-22.04 like the patcher). `updater-manifest.ts` run on CI-style names gives all
  10 platform keys. vitest 90/90 with 100 % thresholds, svelte-check clean, src-tauri clippy -D warnings clean.

## Open
- macOS bundles are not signed/notarised (Gatekeeper warns; "Open" via right-click). Needs Apple Developer ID
  secrets (`APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`, `APPLE_ID`,
  `APPLE_PASSWORD`, `APPLE_TEAM_ID`) if wanted. Windows installers are not Authenticode-signed (SmartScreen).
- No arm64 Windows/Linux builds (patcher has them; add matrix rows + `UPDATER_PLATFORMS` entries if wanted).
- src-tauri coverage gate (lib.rs is untested command glue; needs mock-runtime tests first).
- First real release run not done (nothing pushed from this stream).
- 2026-11-04 (lead): per user decisions, adb is never bundled (removed scripts/fetch-adb.sh and the release step;
  feat/phase1 adds download-on-first-run), and auto-release is paused (workflow_dispatch only) until v0.1.0, which
  ships with the Phase 2 editor. Before the first release: set TAURI_SIGNING_PRIVATE_KEY(_PASSWORD) secrets, tag
  v0.1.0, re-enable the workflow_run trigger.
