//! Small helpers: time, ids, locks, cancel registry, paths.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::{AppError, ErrorCode, Result};

/// Unix time in milliseconds.
pub fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// File modification time in Unix ms (0 when unknown).
pub fn mtime_ms(path: &Path) -> u64 {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_millis() as u64)
}

static SEQ: AtomicU64 = AtomicU64::new(0);

/// A process-unique id: `<prefix><ms>-<n>`.
pub fn unique_id(prefix: &str) -> String {
    format!("{prefix}{}-{}", now_ms(), SEQ.fetch_add(1, Ordering::SeqCst))
}

/// Locks, ignoring poisoning (a panicked holder leaves consistent data in this crate).
pub fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// `dir/<name>` for a plain file name from the UI (no separators, not hidden), else `not_found`.
pub fn child(dir: &Path, name: &str) -> Result<PathBuf> {
    if name.is_empty() || name.starts_with('.') || name.contains(['/', '\\']) {
        return Err(AppError::not_found(name));
    }
    let p = dir.join(name);
    if !p.exists() {
        return Err(AppError::not_found(name));
    }
    Ok(p)
}

/// The last path component as text.
pub fn file_name(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.display().to_string())
}

/// Cancel flags of running operations, by the UI's op id. One flag serves both the device and the game-files
/// layer.
#[derive(Debug, Default)]
pub struct Ops {
    running: Mutex<HashMap<String, Cancel>>,
}

/// Cancel flags handed to the lower layers.
#[derive(Debug, Clone, Default)]
pub struct Cancel {
    pub device: bhw_device::CancelToken,
    pub assets: bhw_assets::CancelToken,
}

impl Cancel {
    /// Whether the operation was cancelled.
    pub fn is_cancelled(&self) -> bool {
        self.device.is_cancelled()
    }

    /// `Err(cancelled)` once cancelled.
    pub fn check(&self) -> Result<()> {
        if self.is_cancelled() {
            return Err(AppError::new(ErrorCode::Cancelled));
        }
        Ok(())
    }
}

/// Unregisters the operation when dropped.
pub struct OpGuard<'a> {
    ops: &'a Ops,
    op: String,
    pub cancel: Cancel,
}

impl Drop for OpGuard<'_> {
    fn drop(&mut self) {
        lock(&self.ops.running).remove(&self.op);
    }
}

impl Ops {
    /// Registers a running operation.
    pub fn start(&self, op: &str) -> OpGuard<'_> {
        let cancel = Cancel::default();
        lock(&self.running).insert(op.to_string(), cancel.clone());
        OpGuard { ops: self, op: op.to_string(), cancel }
    }

    /// Cancels `op`; false when it is not running.
    pub fn cancel(&self, op: &str) -> bool {
        match lock(&self.running).get(op) {
            Some(c) => {
                c.device.cancel();
                c.assets.cancel();
                true
            }
            None => false,
        }
    }
}

/// Limits progress events while bytes move: the first one passes, then one per [`Throttle::STEP`] bytes.
#[derive(Debug, Default)]
pub struct Throttle(u64);

impl Throttle {
    /// Bytes between two events.
    pub const STEP: u64 = 1 << 20;

    /// Whether an event for `bytes` should be sent (`force` for the last one).
    pub fn pass(&mut self, bytes: u64, force: bool) -> bool {
        let ok = force || bytes >= self.0;
        if ok {
            self.0 = bytes + Throttle::STEP;
        }
        ok
    }
}

/// Removes a file or folder when dropped (temporary files of an operation).
#[derive(Debug)]
pub struct TempPath(pub PathBuf);

impl Drop for TempPath {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
