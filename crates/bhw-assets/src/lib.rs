//! "Game files from your APK": validates the user's own The Blockheads APK (single APK, `.apks`/`.xapk` bundle or a
//! folder of base + split APKs) and extracts the graphics the world editor needs from `assets/GameResources/` into a
//! local per-version cache. Nothing is bundled, downloaded or uploaded.
//!
//! - [`apk::inspect`] / [`apk::ApkSet::open`]: validation ([`ApkInfo`]).
//! - [`GameFiles`]: the cache (`<cache>/game-files/<versionName>-<sha256 prefix>/`), with extraction (progress,
//!   cancel, atomic completion), [`Status`], re-extract and clear.
//! - [`axml`]: the binary `AndroidManifest.xml` reader.

pub mod apk;
pub mod axml;
pub mod store;

pub use apk::{ApkInfo, ApkSet, SourceKind, PACKAGE, REQUIRED_FILES, SUPPORTED_VERSIONS};
pub use store::{CancelToken, FileEntry, GameFiles, Manifest, Progress, Status};

use std::path::PathBuf;

/// Errors of this crate.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The file is not an APK (nor an APK bundle or a folder with APKs).
    #[error("not an APK file")]
    NotAnApk,
    /// An APK of another app.
    #[error("this APK is not The Blockheads (package {0})")]
    WrongApp(String),
    /// The Blockheads in a version this tool cannot read.
    #[error("unsupported The Blockheads version {0}")]
    Unsupported(String),
    /// The APK is corrupt or truncated.
    #[error("the APK is damaged: {0}")]
    Damaged(String),
    /// A split APK set without the parts that hold the game files.
    #[error("the APK set is incomplete; missing: {}", .0.join(", "))]
    Incomplete(Vec<String>),
    /// The APK the game files were extracted from is gone (re-extract needs it).
    #[error("the source APK is missing: {}", .0.display())]
    MissingSource(PathBuf),
    /// No game files have been extracted yet.
    #[error("no game files have been set up")]
    NotSet,
    /// Cancelled through a [`CancelToken`].
    #[error("cancelled")]
    Cancelled,
    /// File system error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    /// Corrupt cache manifest.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

/// Result alias.
pub type Result<T, E = Error> = std::result::Result<T, E>;
