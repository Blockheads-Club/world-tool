//! Application logic of BH World Tool, independent of the GUI: one [`Core`] service whose methods match the UI's
//! backend commands (`app/src/lib/api.ts`). State: settings, the Worlds library (`.bhworld` files + index), read
//! sessions (a fetched device backup kept in a temporary file), transfer plans, the backups store, the activity log
//! and the cancel registry. Devices go through [`bhw_device`] (the `adb` binary behind a runner trait, so tests use
//! scripted adb answers).

pub mod activity;
pub mod adb_tools;
pub mod backups;
mod devices;
pub mod error;
mod files;
mod gamefiles;
mod ios;
pub mod library;
pub mod read;
mod root;
pub mod settings;
mod transfer;
pub mod types;
pub mod util;
pub mod verify;
pub mod worlds;

pub use error::{AppError, ErrorCode, Result};
pub use settings::Settings;
pub use types::*;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use bhw_device::process::Runner;
use bhw_device::{Adb, PcInstall, RootStyle};
use serde_json::{Map, Value};

use crate::activity::ActivityLog;
use crate::library::Lib;
use crate::util::{lock, Ops};

/// Finds the default Blockheads PC data folder (injectable for tests).
pub type PcLocator = Arc<dyn Fn() -> Option<PathBuf> + Send + Sync>;

/// Makes an [`Adb`] for a binary path (real processes in the app, a scripted runner in tests).
pub type AdbFactory = Arc<dyn Fn(&Path) -> Adb + Send + Sync>;

/// Root access of a phone, probed once per connection (`None` inside = no root).
type RootProbe = Arc<OnceLock<Option<RootStyle>>>;

/// Everything the core needs from its host.
pub struct CoreConfig {
    /// `settings.json`, `activity.json`.
    pub config_dir: PathBuf,
    /// Read sessions, temporary files, the game files cache, APKs pulled from phones.
    pub cache_dir: PathBuf,
    /// Worlds folder used until the user picks one.
    pub default_worlds_folder: PathBuf,
    /// Persistent app data (a downloaded adb in `platform-tools/`).
    pub data_dir: PathBuf,
    /// The adb client, `None` when it was not found (the UI then offers to download it).
    pub adb: Option<Adb>,
    /// Makes an adb client for a binary (after downloading Platform-Tools).
    pub make_adb: AdbFactory,
    /// Downloads (Google's Platform-Tools).
    pub fetch: Arc<dyn adb_tools::Fetch>,
    /// Starts local processes (is Blockheads PC running?).
    pub runner: Arc<dyn Runner>,
    /// Default Blockheads PC data folder (when the settings name none).
    pub pc_locator: PcLocator,
    /// Folders with iPhone/iPad backups (Finder/iTunes defaults).
    pub ios_locator: Arc<dyn Fn() -> Vec<PathBuf> + Send + Sync>,
    /// adb's user folder (`~/.android`) with the paired-hosts list.
    pub android_dir: Option<PathBuf>,
    /// App version and OS for the diagnostics.
    pub app_version: String,
}

/// The application service.
pub struct Core {
    cfg: CoreConfig,
    adb: Mutex<Option<Adb>>,
    root: Mutex<HashMap<String, RootProbe>>,
    settings: Mutex<Settings>,
    activity: ActivityLog,
    ops: Ops,
    reads: Mutex<Vec<Arc<read::ReadSession>>>,
    plans: Mutex<HashMap<String, Arc<transfer::PlanState>>>,
    devices: Mutex<Vec<Device>>,
    /// Opened iPhone/iPad backups (read-only devices).
    ios: Mutex<Vec<Device>>,
    /// Wrong password attempts per iOS backup.
    ios_attempts: Mutex<HashMap<String, u32>>,
}

impl Core {
    /// Loads settings and the activity log.
    pub fn new(cfg: CoreConfig) -> Core {
        let settings =
            Settings::load(&cfg.config_dir.join("settings.json"), Settings::defaults(&cfg.default_worlds_folder));
        let activity = ActivityLog::open(&cfg.config_dir.join("activity.json"));
        // Read sessions of an earlier run are gone with their state.
        let _ = std::fs::remove_dir_all(cfg.cache_dir.join("reads"));
        let _ = std::fs::remove_dir_all(cfg.cache_dir.join("tmp"));
        Core {
            adb: Mutex::new(cfg.adb.clone()),
            root: Mutex::new(HashMap::new()),
            cfg,
            settings: Mutex::new(settings),
            activity,
            ops: Ops::default(),
            reads: Mutex::new(Vec::new()),
            plans: Mutex::new(HashMap::new()),
            devices: Mutex::new(Vec::new()),
            ios: Mutex::new(Vec::new()),
            ios_attempts: Mutex::new(HashMap::new()),
        }
    }

    /// Current settings.
    pub fn settings(&self) -> Settings {
        lock(&self.settings).clone()
    }

    /// Applies and saves a partial settings update.
    pub fn set_settings(&self, patch: &Map<String, Value>) -> Result<Settings> {
        let mut s = lock(&self.settings);
        let next = s.merge(patch)?;
        next.save(&self.cfg.config_dir.join("settings.json"))?;
        *s = next.clone();
        Ok(next)
    }

    /// Cancels a running operation (no error when it already ended).
    pub fn cancel(&self, op: &str) {
        self.ops.cancel(op);
    }

    /// The activity log, newest first.
    pub fn list_activity(&self) -> Vec<Activity> {
        self.activity.list()
    }

    /// Empties the activity log.
    pub fn clear_activity(&self) -> Result<()> {
        self.activity.clear()
    }

    /// The Worlds library.
    pub fn lib(&self) -> Result<Lib> {
        Lib::open(&self.settings().worlds_dir())
    }

    fn adb(&self) -> Result<Adb> {
        lock(&self.adb).clone().ok_or(AppError::from(bhw_device::DeviceError::AdbNotFound))
    }

    /// The Blockheads PC data folder in use, if the game has run there.
    fn pc(&self) -> Option<PcInstall> {
        let dir = self.settings().pc_folder.map(PathBuf::from).or_else(|| (self.cfg.pc_locator)())?;
        Some(PcInstall::at(dir)).filter(PcInstall::exists)
    }

    /// A folder for temporary files of an operation.
    fn tmp_dir(&self) -> Result<PathBuf> {
        let d = self.cfg.cache_dir.join("tmp");
        std::fs::create_dir_all(&d)?;
        Ok(d)
    }
}
