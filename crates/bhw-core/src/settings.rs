//! User settings (`Settings` in `types.ts`), stored as JSON in the app's config folder.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

use crate::error::Result;

/// Settings as the UI sees them.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Settings {
    pub first_run_done: bool,
    pub worlds_folder: String,
    pub backup_before_write: bool,
    pub backups_to_keep: u32,
    pub root_mode: String,
    pub theme: String,
    pub lang: Option<String>,
    pub reduce_motion: bool,
    pub auto_update: bool,
    pub pc_folder: Option<String>,
    pub skipped_version: Option<String>,
    pub last_update_check: Option<u64>,
}

impl Settings {
    /// Defaults with the given Worlds folder.
    pub fn defaults(worlds_folder: &Path) -> Settings {
        Settings {
            first_run_done: false,
            worlds_folder: worlds_folder.to_string_lossy().into_owned(),
            backup_before_write: true,
            backups_to_keep: 5,
            root_mode: "auto".into(),
            theme: "system".into(),
            lang: None,
            reduce_motion: false,
            auto_update: true,
            pc_folder: None,
            skipped_version: None,
            last_update_check: None,
        }
    }

    /// Applies a partial update (`Partial<Settings>`); unknown keys are ignored, bad types rejected.
    pub fn merge(&self, patch: &Map<String, Value>) -> Result<Settings> {
        let mut v = serde_json::to_value(self)?;
        let obj = v.as_object_mut().expect("a struct serializes to an object");
        for (k, val) in patch {
            if let Some(slot) = obj.get_mut(k) {
                *slot = val.clone();
            }
        }
        let mut s: Settings = serde_json::from_value(v)?;
        // `root_mode`: "auto" (root when it works, else the backup method) or "never" (always the backup method);
        // older values such as "always" mean "auto".
        if s.root_mode != "never" {
            s.root_mode = "auto".into();
        }
        Ok(s)
    }

    /// Loads saved settings, falling back to `defaults` for a missing or broken file.
    pub fn load(file: &Path, defaults: Settings) -> Settings {
        std::fs::read(file)
            .ok()
            .and_then(|b| serde_json::from_slice::<Map<String, Value>>(&b).ok())
            .and_then(|m| defaults.merge(&m).ok())
            .unwrap_or(defaults)
    }

    /// Writes the settings file.
    pub fn save(&self, file: &Path) -> Result<()> {
        std::fs::create_dir_all(file.parent().unwrap_or(Path::new("")))?;
        std::fs::write(file, serde_json::to_vec_pretty(self)?)?;
        Ok(())
    }

    /// The Worlds folder.
    pub fn worlds_dir(&self) -> PathBuf {
        PathBuf::from(&self.worlds_folder)
    }
}
