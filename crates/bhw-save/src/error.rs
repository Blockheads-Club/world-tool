//! Error type.

use std::path::PathBuf;

/// Errors of this crate.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A file system operation failed.
    #[error("{path}: {source}")]
    Io {
        /// The file or directory involved.
        path: PathBuf,
        /// The underlying error.
        source: std::io::Error,
    },
    /// An LMDB file could not be read or written.
    #[error("LMDB: {0}")]
    Lmdb(#[from] bhw_lmdb::Error),
    /// A property list could not be parsed or written.
    #[error("property list: {0}")]
    Plist(#[from] plist::Error),
    /// The save is not laid out or encoded the way the game does it.
    #[error("{0}")]
    Format(String),
    /// The operation is not possible for this save.
    #[error("unsupported: {0}")]
    Unsupported(String),
}

/// Result alias.
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Wraps an I/O error with the path it happened on.
pub(crate) fn io(path: impl Into<PathBuf>) -> impl FnOnce(std::io::Error) -> Error {
    let path = path.into();
    move |source| Error::Io { path, source }
}
