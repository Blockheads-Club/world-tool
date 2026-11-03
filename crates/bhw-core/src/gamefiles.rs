//! Game files from the user's own APK (for the Phase 2 editor), through [`bhw_assets`].

use std::path::{Path, PathBuf};
use std::time::Duration;

use bhw_assets::Status;
use bhw_device::Adb;

use crate::error::{AppError, ErrorCode, Result};
use crate::types::{DeviceKind, GameFiles, GameFilesState, Progress, Stage};
use crate::util::{file_name, Cancel};
use crate::Core;

/// How long pulling one APK from a phone may take.
pub const PULL_TIMEOUT: Duration = Duration::from_secs(600);

fn files(state: GameFilesState, source: Option<&Path>, version: Option<String>) -> GameFiles {
    GameFiles {
        state,
        apk_path: source.map(|p| p.display().to_string()),
        apk_name: source.map(file_name),
        version,
        images: None,
        size: None,
    }
}

/// APK paths from `pm path` output.
pub fn parse_pm_path(text: &str) -> Vec<String> {
    text.lines().filter_map(|l| l.trim().strip_prefix("package:")).map(str::to_string).collect()
}

impl Core {
    fn assets(&self) -> bhw_assets::GameFiles {
        bhw_assets::GameFiles::new(&self.cfg.cache_dir)
    }

    /// State of the game files cache.
    pub fn game_files(&self) -> GameFiles {
        match self.assets().status() {
            Status::NotSet => files(GameFilesState::NotSet, None, None),
            Status::Ready { version, files: n, bytes, source, .. } => GameFiles {
                images: Some(n),
                size: Some(bytes),
                ..files(GameFilesState::Ready, Some(&source), Some(version))
            },
            Status::Unsupported { version } => files(GameFilesState::Unsupported, None, Some(version)),
            Status::Damaged { source, .. } => files(GameFilesState::Damaged, source.as_deref(), None),
            Status::MissingSource { version, source } => files(GameFilesState::Missing, Some(&source), Some(version)),
        }
    }

    fn extract(&self, path: &Path, progress: &mut dyn FnMut(Progress), cancel: &Cancel) -> Result<GameFiles> {
        let named = |e: bhw_assets::Error| AppError::from(e).name(file_name(path));
        let shown = path.display().to_string();
        progress(Progress {
            path: Some(shown.clone()),
            item: Some(file_name(path)),
            ..Progress::stage(Stage::ApkChecking)
        });
        let info = bhw_assets::apk::inspect(path).map_err(named)?;
        let version = info.version_name.clone();
        progress(Progress {
            path: Some(shown),
            version: Some(version.clone()),
            total: Some(info.bytes),
            ..Progress::stage(Stage::ApkChecking)
        });
        self.assets()
            .extract(
                path,
                &mut |p| {
                    progress(Progress {
                        done: Some(p.files_done as u64),
                        total: Some(p.files_total as u64),
                        item: Some(p.current.clone()),
                        version: Some(version.clone()),
                        ..Progress::stage(Stage::ApkExtracting)
                    })
                },
                &cancel.assets,
            )
            .map_err(named)?;
        Ok(self.game_files())
    }

    /// Validates an APK (or bundle / folder) and extracts its game files.
    pub fn use_apk(&self, op: &str, path: &str, progress: &mut dyn FnMut(Progress)) -> Result<GameFiles> {
        let guard = self.ops.start(op);
        self.extract(Path::new(path), progress, &guard.cancel)
    }

    /// Extracts again from the APK used last time.
    pub fn reextract(&self, op: &str, progress: &mut dyn FnMut(Progress)) -> Result<GameFiles> {
        let guard = self.ops.start(op);
        let source = match self.assets().current()? {
            Some((_, m)) if m.source.exists() => m.source,
            _ => return Err(AppError::new(ErrorCode::ApkMissing)),
        };
        self.extract(&source, progress, &guard.cancel)
    }

    /// Removes the extracted game files.
    pub fn clear_game_files(&self) -> Result<GameFiles> {
        self.assets().clear()?;
        Ok(self.game_files())
    }

    /// Copies the game's APK(s) from a phone (`pm path` + `adb pull`) and extracts the game files.
    pub fn apk_from_device(&self, op: &str, device_id: &str, progress: &mut dyn FnMut(Progress)) -> Result<GameFiles> {
        let guard = self.ops.start(op);
        let d = self.ready_device(device_id)?;
        if d.kind != DeviceKind::Android {
            return Err(AppError::with(ErrorCode::NotFound, "only phones have the game's APK").device(&d.name));
        }
        let adb = self.adb()?;
        let apks = parse_pm_path(&adb.shell_ok(&d.id, &format!("pm path {}", bhw_device::PACKAGE))?);
        if apks.is_empty() {
            return Err(AppError::new(ErrorCode::GameNotInstalled).device(&d.name));
        }
        let dir = self.cfg.cache_dir.join("apk").join(bhw_world::library::slug(&d.id));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir)?;
        let mut pulled: Vec<PathBuf> = Vec::new();
        for (i, apk) in apks.iter().enumerate() {
            guard.cancel.check()?;
            progress(Progress {
                item: Some(d.name.clone()),
                ..Progress::bytes(Stage::ApkPulling, i as u64, Some(apks.len() as u64))
            });
            let dest = dir.join(file_name(Path::new(apk)));
            let args = Adb::device_args(&d.id, &["pull", apk, &dest.to_string_lossy()]);
            let out = adb.run_timeout(&args, PULL_TIMEOUT)?;
            if out.code != 0 {
                return Err(AppError::io(format!("adb pull {apk}: {}", out.stderr.trim())).device(&d.name));
            }
            pulled.push(dest);
        }
        progress(Progress {
            item: Some(d.name.clone()),
            ..Progress::bytes(Stage::ApkPulling, apks.len() as u64, Some(apks.len() as u64))
        });
        let source = if pulled.len() == 1 { pulled.remove(0) } else { dir };
        self.extract(&source, progress, &guard.cancel)
    }
}
