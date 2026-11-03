//! Read-only access to The Blockheads worlds inside iPhone/iPad backups made by Finder or iTunes.
//!
//! * [`default_backup_dirs`] / [`list_backups`] find backups and describe them ([`BackupInfo`]).
//! * [`IosBackup::open`] opens one (unencrypted, or encrypted with its password; iOS 10.2+ format and the older
//!   single-PBKDF2 keybag), reads `Manifest.db` and extracts files.
//! * [`IosBackup::blockheads`] finds the game's app container and returns its worlds as [`bhw_world::WorldFiles`]
//!   plus the `game_db/` files (the game's world list).
//!
//! The format is described in `docs/ios-backup.md`. This crate never writes to a backup.

pub mod backup;
pub mod crypto;
pub mod info;
pub mod keybag;
pub mod mbfile;

/// Synthetic backups for tests (also for other crates' tests with the `test-util` feature).
#[cfg(any(test, feature = "test-util"))]
pub mod fixture;

use std::path::PathBuf;

pub use backup::{file_id, BlockheadsData, IosBackup, ManifestEntry};
pub use info::{candidate_backup_dirs, default_backup_dirs, list_backups, read_backup_info, BackupInfo};

/// Errors of this crate.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// File system error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    /// A plist could not be read.
    #[error("plist error: {0}")]
    Plist(String),
    /// `Manifest.db` could not be read.
    #[error("Manifest.db error: {0}")]
    Sqlite(String),
    /// The directory has no `Manifest.plist`.
    #[error("not an iOS backup: {0}")]
    NotABackup(PathBuf),
    /// The backup is encrypted and no password was given.
    #[error("this backup is encrypted; a password is needed")]
    PasswordRequired,
    /// The backup password is wrong.
    #[error("wrong backup password")]
    WrongPassword,
    /// The keybag is malformed.
    #[error("keybag error: {0}")]
    Keybag(String),
    /// Malformed backup data.
    #[error("corrupt backup: {0}")]
    Corrupt(String),
    /// A file listed in `Manifest.db` (or `Manifest.db` itself) is missing.
    #[error("missing file in backup: {0}")]
    MissingFile(String),
    /// A backup format this crate does not read.
    #[error("unsupported backup format: {0}")]
    Unsupported(String),
    /// No The Blockheads saves in the backup.
    #[error("no The Blockheads worlds in this backup")]
    NoBlockheads,
    /// The extracted files do not form a valid world.
    #[error(transparent)]
    World(#[from] bhw_world::Error),
}

impl From<rusqlite::Error> for Error {
    fn from(e: rusqlite::Error) -> Self {
        Error::Sqlite(e.to_string())
    }
}

/// Result of this crate.
pub type Result<T> = std::result::Result<T, Error>;
