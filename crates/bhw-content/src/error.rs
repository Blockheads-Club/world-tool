//! Error type.

/// Errors of this crate.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A record is not encoded the way the game does it.
    #[error("{0}")]
    Format(String),
    /// A property list could not be parsed or written.
    #[error("property list: {0}")]
    Plist(#[from] plist::Error),
    /// The change is not possible (e.g. the area was never generated).
    #[error("{0}")]
    Invalid(String),
}

/// Result alias.
pub type Result<T, E = Error> = std::result::Result<T, E>;

impl From<bhw_save::Error> for Error {
    fn from(e: bhw_save::Error) -> Error {
        Error::Format(e.to_string())
    }
}
