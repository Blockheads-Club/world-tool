//! Error type.

/// Errors returned by the reader and writer.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The data is shorter than the format requires.
    #[error("file truncated: need {needed} bytes, have {len}")]
    Truncated {
        /// Bytes required.
        needed: u64,
        /// Bytes available.
        len: u64,
    },
    /// No `MDB_MAGIC` found at offset 12 (32-bit) or 16 (64-bit).
    #[error("not an LMDB data file (no 0xBEEFC0DE magic at offset 12 or 16)")]
    NotLmdb,
    /// A meta page is invalid (wrong flags, magic or version).
    #[error("meta page {index} invalid: {reason}")]
    BadMeta {
        /// Meta page index (0 or 1).
        index: usize,
        /// What is wrong.
        reason: String,
    },
    /// The page size is not a supported power of two.
    #[error("unsupported page size {0}")]
    BadPageSize(u32),
    /// A page is malformed.
    #[error("page {pgno} invalid: {reason}")]
    BadPage {
        /// Page number (for a sub-page: the page holding it).
        pgno: u64,
        /// What is wrong.
        reason: String,
    },
    /// The tree is deeper than LMDB allows (or contains a cycle).
    #[error("tree deeper than {0} levels (cycle?)")]
    TooDeep(usize),
    /// A requested named database does not exist.
    #[error("no database named {0:?}")]
    NoSuchDb(String),
    /// A key/value is not storable by LMDB.
    #[error("{0}")]
    BadValue(String),
    /// Two entries have the same key (or the same key and value in a `MDB_DUPSORT` database).
    #[error("duplicate key {key:?} in database {db:?}")]
    DuplicateKey {
        /// Database name (`None` = main DB).
        db: Option<String>,
        /// The key (lossy UTF-8).
        key: String,
    },
    /// A size does not fit the chosen layout (e.g. map size > 4 GiB for 32-bit).
    #[error("{0} does not fit the 32-bit layout")]
    TooLarge(&'static str),
    /// Structure combination not supported.
    #[error("unsupported: {0}")]
    Unsupported(String),
    /// I/O error from the file helpers.
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Result alias.
pub type Result<T> = std::result::Result<T, Error>;

pub(crate) fn bad_page<T>(pgno: u64, reason: impl Into<String>) -> Result<T> {
    Err(Error::BadPage { pgno, reason: reason.into() })
}

pub(crate) fn lossy(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}
