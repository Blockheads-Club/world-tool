//! The Blockheads world files for BH World Tool: one world as a set of files ([`WorldFiles`]), loading/saving it from
//! a saves directory (Blockheads PC layout), an Android backup ([`bhw_backup`]) or a Blockheads Server world folder,
//! the `.bhworld` exchange format ([`bhworld`], spec in `docs/bhworld-format.md`), conflict helpers ([`meta`]) and the
//! on-disk library of exported worlds and device backups ([`library`]).
//!
//! World metadata (name, game version) and server/game conversions live in LMDB databases; they are reached through
//! the traits in [`meta`] so the LMDB crate can be plugged in without this crate depending on it.

pub mod backup;
pub mod bhworld;
pub mod files;
pub mod library;
pub mod meta;
pub mod time;

pub use bhw_backup::Payload;
pub use bhworld::{BhWorld, ExportInfo, Limits, Manifest, Source, SourceKind};
pub use files::{WorldFile, WorldFiles};
pub use library::{BackupMeta, Library, LibraryBackup, LibraryWorld};
pub use meta::{
    keep_both, new_world_id, unique_name, Conflicts, WorldMeta, WorldMetaEditor, WorldMetaSource, WorldSummary,
};

use std::path::PathBuf;

/// Errors of this crate.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// File system error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    /// Error from the backup layer.
    #[error(transparent)]
    Backup(#[from] bhw_backup::Error),
    /// Error from the zip layer.
    #[error("zip error: {0}")]
    Zip(#[from] zip::result::ZipError),
    /// JSON error.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    /// Not a 32 character lowercase hex world id.
    #[error("invalid world id {0:?}")]
    InvalidId(String),
    /// Unsafe or malformed relative path.
    #[error("invalid path {0:?}")]
    InvalidPath(String),
    /// A directory that is not a world (no `world_db/data.mdb`).
    #[error("not a world: {0}")]
    NotAWorld(PathBuf),
    /// Target already exists.
    #[error("already exists: {0}")]
    AlreadyExists(PathBuf),
    /// Malformed `.bhworld` file.
    #[error("invalid .bhworld file: {0}")]
    Format(String),
    /// `.bhworld` written by a newer version of the tool.
    #[error("unsupported .bhworld format version {0}")]
    UnsupportedFormatVersion(u32),
    /// A file's content does not match the manifest.
    #[error("checksum mismatch for {0}")]
    ChecksumMismatch(String),
    /// A size/count/ratio limit was exceeded while opening a `.bhworld` (zip bomb protection).
    #[error("limit exceeded: {0}")]
    LimitExceeded(String),
    /// Error from a metadata plug-in ([`meta::WorldMetaSource`] / [`meta::WorldMetaEditor`]).
    #[error("world metadata: {0}")]
    Meta(String),
}

/// Result alias of this crate.
pub type Result<T, E = Error> = std::result::Result<T, E>;
