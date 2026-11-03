//! The activity log (`activity.json` in the app's config folder), newest first.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::error::{AppError, ErrorCode, Result};
use crate::types::{Activity, ActivityKind, ActivityResult};
use crate::util::{lock, now_ms, unique_id};

/// Entries kept at most.
pub const MAX_ENTRIES: usize = 1000;

/// The log, mirrored in a JSON file.
#[derive(Debug)]
pub struct ActivityLog {
    file: PathBuf,
    items: Mutex<Vec<Activity>>,
}

/// What to log about one operation (filled in as it runs).
#[derive(Debug, Clone)]
pub struct Entry {
    pub kind: ActivityKind,
    pub device: String,
    pub worlds: Vec<String>,
    pub size: Option<u64>,
}

impl Entry {
    /// An entry for `device`.
    pub fn new(kind: ActivityKind, device: &str) -> Entry {
        Entry { kind, device: device.to_string(), worlds: Vec::new(), size: None }
    }
}

impl ActivityLog {
    /// Loads `file` (a missing or broken file gives an empty log).
    pub fn open(file: &Path) -> ActivityLog {
        let items = std::fs::read(file).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        ActivityLog { file: file.to_path_buf(), items: Mutex::new(items) }
    }

    /// All entries, newest first.
    pub fn list(&self) -> Vec<Activity> {
        lock(&self.items).clone()
    }

    fn save(&self, items: &[Activity]) -> Result<()> {
        if let Some(dir) = self.file.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&self.file, serde_json::to_vec(items)?)?;
        Ok(())
    }

    /// Adds an entry (the file is written best effort: a log that cannot be saved must not fail the operation).
    pub fn add(&self, e: &Entry, result: ActivityResult, err: Option<&AppError>) -> Activity {
        let a = Activity {
            id: unique_id("act-"),
            time: now_ms(),
            kind: e.kind,
            result,
            device: e.device.clone(),
            worlds: e.worlds.clone(),
            size: e.size,
            detail: err.map(|x| x.to_string()),
            error: err.map(|x| x.code),
        };
        let mut items = lock(&self.items);
        items.insert(0, a.clone());
        items.truncate(MAX_ENTRIES);
        let _ = self.save(&items);
        a
    }

    /// Logs how `r` ended and passes it on.
    pub fn record<T>(&self, e: &Entry, r: Result<T>) -> Result<T> {
        match &r {
            Ok(_) => self.add(e, ActivityResult::Done, None),
            Err(x) if matches!(x.code, ErrorCode::Cancelled | ErrorCode::Declined) => {
                self.add(e, ActivityResult::Stopped, Some(x))
            }
            Err(x) => self.add(e, ActivityResult::Failed, Some(x)),
        };
        r
    }

    /// Empties the log.
    pub fn clear(&self) -> Result<()> {
        let mut items = lock(&self.items);
        items.clear();
        self.save(&items)
    }
}
