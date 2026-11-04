//! Error type.

/// Errors of this crate.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The world's contents could not be read or written.
    #[error(transparent)]
    Content(#[from] bhw_content::Error),
    /// A schematic file is not valid.
    #[error("schematic: {0}")]
    Schematic(String),
    /// The request is not possible.
    #[error("{0}")]
    Invalid(String),
}

/// Result alias.
pub type Result<T, E = Error> = std::result::Result<T, E>;
