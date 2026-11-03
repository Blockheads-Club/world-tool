//! Blockheads PC (the blockheads-pc launcher + `bhpc-run`) as a source/destination of worlds.
//!
//! Data directory, in the order the launcher uses: `BHPC_DATA_DIR`; the `data_dir` in the launcher's
//! `launcher.toml` (in `BHPC_CONFIG_DIR`, else `<config dir>/blockheads-pc`); else `<data dir>/blockheads-pc`:
//! `$XDG_DATA_HOME` or `~/.local/share` on Linux, `%APPDATA%` on Windows, `~/Library/Application Support` on
//! macOS. Saves live in `<data>/home/Library/Application Support/saves/<32 hex>/`, the global `game_db` next to
//! `saves`.

use crate::error::{local_write_error, DeviceError, Result};
use crate::process::{run_collect, Runner};
use crate::root::is_world_id;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// The game process (the launcher's sidecar).
pub const GAME_PROCESS: &str = "bhpc-run";

/// Where a data directory came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataDirSource {
    Env,
    LauncherConfig,
    PlatformDefault,
    Explicit,
}

/// A Blockheads PC data directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PcInstall {
    pub data_dir: PathBuf,
    pub source: DataDirSource,
}

/// A world in the saves folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PcWorld {
    pub id: String,
    pub path: PathBuf,
}

/// Reads `data_dir` from launcher.toml text.
pub fn launcher_data_dir(toml_text: &str) -> Option<PathBuf> {
    let table: toml::Table = toml_text.parse().ok()?;
    table.get("data_dir")?.as_str().filter(|s| !s.is_empty()).map(PathBuf::from)
}

impl PcInstall {
    /// A data directory chosen by the user (e.g. a portable install).
    pub fn at(data_dir: impl Into<PathBuf>) -> PcInstall {
        PcInstall { data_dir: data_dir.into(), source: DataDirSource::Explicit }
    }

    /// Candidate data directories in priority order. `config_dir` / `data_dir` are the platform's base dirs.
    pub fn candidates_with(
        env: &dyn Fn(&str) -> Option<OsString>,
        config_dir: Option<PathBuf>,
        data_dir: Option<PathBuf>,
    ) -> Vec<PcInstall> {
        let mut out = Vec::new();
        if let Some(d) = env("BHPC_DATA_DIR").filter(|d| !d.is_empty()) {
            out.push(PcInstall { data_dir: d.into(), source: DataDirSource::Env });
        }
        let launcher_dir = env("BHPC_CONFIG_DIR")
            .filter(|d| !d.is_empty())
            .map(PathBuf::from)
            .or_else(|| config_dir.map(|c| c.join("blockheads-pc")));
        if let Some(d) = launcher_dir
            .and_then(|d| std::fs::read_to_string(d.join("launcher.toml")).ok())
            .and_then(|t| launcher_data_dir(&t))
        {
            out.push(PcInstall { data_dir: d, source: DataDirSource::LauncherConfig });
        }
        if let Some(d) = data_dir {
            out.push(PcInstall { data_dir: d.join("blockheads-pc"), source: DataDirSource::PlatformDefault });
        }
        out
    }

    /// The first candidate that has been used (has a `home` dir), from the real environment.
    pub fn locate() -> Option<PcInstall> {
        PcInstall::candidates_with(&|k| std::env::var_os(k), dirs::config_dir(), dirs::data_dir())
            .into_iter()
            .find(PcInstall::exists)
    }

    /// Whether the game has run with this data directory.
    pub fn exists(&self) -> bool {
        self.data_dir.join("home").is_dir()
    }

    /// `<data>/home/Library/Application Support`.
    pub fn support_dir(&self) -> PathBuf {
        self.data_dir.join("home").join("Library").join("Application Support")
    }

    /// The saves folder.
    pub fn saves_dir(&self) -> PathBuf {
        self.support_dir().join("saves")
    }

    /// The global `game_db`.
    pub fn game_db(&self) -> PathBuf {
        self.support_dir().join("game_db")
    }

    /// Worlds (32-hex dirs with a `world_db`), sorted by id.
    pub fn list_worlds(&self) -> Result<Vec<PcWorld>> {
        let rd = match std::fs::read_dir(self.saves_dir()) {
            Ok(rd) => rd,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e.into()),
        };
        let mut worlds: Vec<PcWorld> = rd
            .flatten()
            .filter_map(|e| {
                let id = e.file_name().to_string_lossy().into_owned();
                let path = e.path();
                (is_world_id(&id) && path.join("world_db").is_dir()).then_some(PcWorld { id, path })
            })
            .collect();
        worlds.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(worlds)
    }

    /// Copies world `id` into `dest` (created; becomes the world dir). Returns the files copied.
    pub fn read_world(&self, id: &str, dest: &Path) -> Result<usize> {
        let src = self.saves_dir().join(id);
        if !is_world_id(id) || !src.is_dir() {
            return Err(DeviceError::PathNotFound(src));
        }
        copy_dir(&src, dest)
    }

    /// Writes the world dir `src` as world `id`. Refuses while the game runs (`running` is the caller's process
    /// check, see [`game_running`]). Copies to a temporary sibling first, then swaps it in.
    pub fn write_world(&self, id: &str, src: &Path, overwrite: bool, running: bool) -> Result<usize> {
        if running {
            return Err(DeviceError::GameRunning);
        }
        if !is_world_id(id) || !src.join("world_db").is_dir() {
            return Err(DeviceError::PathNotFound(src.join("world_db")));
        }
        let saves = self.saves_dir();
        let dest = saves.join(id);
        if dest.exists() && !overwrite {
            return Err(DeviceError::WorldExists(id.to_string()));
        }
        std::fs::create_dir_all(&saves).map_err(local_write_error)?;
        let incoming = saves.join(format!(".bhw-incoming-{id}"));
        let old = saves.join(format!(".bhw-replaced-{id}"));
        for d in [&incoming, &old] {
            if d.exists() {
                std::fs::remove_dir_all(d)?;
            }
        }
        let n = copy_dir(src, &incoming).inspect_err(|_| {
            let _ = std::fs::remove_dir_all(&incoming);
        })?;
        if dest.exists() {
            std::fs::rename(&dest, &old)?;
        }
        std::fs::rename(&incoming, &dest)?;
        if old.exists() {
            std::fs::remove_dir_all(&old)?;
        }
        Ok(n)
    }
}

/// Copies the tree `src` into `dst` (created); returns the number of files. Symlinks are skipped.
pub fn copy_dir(src: &Path, dst: &Path) -> Result<usize> {
    std::fs::create_dir_all(dst).map_err(local_write_error)?;
    let mut n = 0;
    for e in std::fs::read_dir(src)? {
        let e = e?;
        let to = dst.join(e.file_name());
        let ty = e.file_type()?;
        if ty.is_dir() {
            n += copy_dir(&e.path(), &to)?;
        } else if ty.is_file() {
            std::fs::copy(e.path(), &to).map_err(local_write_error)?;
            n += 1;
        }
    }
    Ok(n)
}

/// Process names from `ps -A -o comm=` (Unix; full paths on macOS) or `tasklist /FO CSV /NH` (Windows).
pub fn parse_process_list(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|l| {
            let l = l.trim();
            let name = if let Some(rest) = l.strip_prefix('"') { rest.split('"').next()? } else { l };
            let base = name.rsplit(['/', '\\']).next()?;
            (!base.is_empty()).then(|| base.trim_end_matches(".exe").to_string())
        })
        .collect()
}

/// Whether `bhpc-run` is running, using `ps` / `tasklist` started through `runner`.
pub fn game_running(runner: &dyn Runner, windows: bool) -> Result<bool> {
    let (program, args): (&str, &[&str]) =
        if windows { ("tasklist", &["/FO", "CSV", "/NH"]) } else { ("ps", &["-A", "-o", "comm="]) };
    let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    let out = run_collect(runner, Path::new(program), &args, Duration::from_secs(10))?.ok_or(DeviceError::Timeout)?;
    if out.code != 0 {
        return Err(DeviceError::Command { code: out.code, message: out.stderr.trim().to_string() });
    }
    Ok(parse_process_list(&out.stdout).iter().any(|p| p == GAME_PROCESS))
}
