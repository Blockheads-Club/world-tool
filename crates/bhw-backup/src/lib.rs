//! Android `adb backup` files (`.ab`) for BH World Tool.
//!
//! An `.ab` file is a short text header followed by a (optionally AES-256-CBC encrypted, optionally zlib compressed)
//! tar stream:
//!
//! ```text
//! ANDROID BACKUP\n
//! <version>\n            1..=5 (5 since Android 9)
//! <compressed>\n         0 or 1 (zlib / java.util.zip.Deflater stream)
//! <encryption>\n         "none" or "AES-256"
//! [AES-256 only:]
//! <user password salt, hex>\n
//! <master key checksum salt, hex>\n
//! <PBKDF2 rounds>\n
//! <user key IV, hex>\n
//! <encrypted master key blob, hex>\n
//! <payload>
//! ```
//!
//! The layout follows AOSP `PerformAdbBackupTask` (writer) and `PerformAdbRestoreTask` /
//! `UserBackupManagerService` (reader, `parseBackupFileHeaderAndReturnTarStream`, `decodeAesHeaderAndInitialize`,
//! `attemptMasterKeyDecryption`), plus `PasswordUtils` (`buildPasswordKey`, `makeKeyChecksum`). See [`crypto`] for the
//! key derivation details and [`tar`] for the tar dialect Android reads back.
//!
//! Everything is streaming: [`open_backup`] returns a `Read` of the decrypted, decompressed tar and
//! [`BackupWriter`] takes tar bytes and encodes them on the fly, so multi-hundred-megabyte backups never need to be held
//! in memory. [`rewrite_backup`] combines both to edit a backup in one pass, and [`saves`] knows where
//! The Blockheads keeps its worlds inside a backup.

pub mod crypto;
pub mod edit;
pub mod header;
pub mod hex;
pub mod saves;
pub mod stream;
pub mod tar;

pub use edit::{apply_tar_edit, is_safe_rel_path, rewrite_backup, EditStats, NewEntry, Payload, TarEdit};
pub use header::{AesParams, BackupHeader, Encryption};
pub use saves::{EntryInfo, SavesContent, SavesDir, WorldEntry, BLOCKHEADS_PACKAGE};
pub use stream::{open_backup, BackupOptions, BackupWriter};
pub use tar::{EntryKind, TarHeader, TarReader, TarWriter};

/// Errors of this crate.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Underlying I/O error (also used for corrupt compressed/encrypted payloads, which surface while streaming).
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    /// The file does not start with `ANDROID BACKUP`.
    #[error("not an Android backup file")]
    NotABackup,
    /// Backup format version this crate does not know.
    #[error("unsupported Android backup version {0}")]
    UnsupportedVersion(u32),
    /// Malformed header line.
    #[error("malformed backup header: {0}")]
    BadHeader(String),
    /// Encryption algorithm other than `none` / `AES-256`.
    #[error("unsupported backup encryption {0:?}")]
    UnsupportedEncryption(String),
    /// The backup is encrypted but no password was given.
    #[error("this backup is encrypted; a password is required")]
    PasswordRequired,
    /// The password does not decrypt the master key (or its checksum does not match).
    #[error("wrong backup password")]
    WrongPassword,
    /// Malformed tar stream.
    #[error("malformed tar data: {0}")]
    Tar(String),
    /// A rewrite wanted to insert entries but none of the anchor directories exists in the backup.
    #[error("cannot place new files: none of {0:?} is in the backup")]
    AnchorNotFound(Vec<String>),
    /// Invalid argument (bad path, bad world id, size mismatch, ...).
    #[error("invalid input: {0}")]
    Invalid(String),
}

/// Result alias of this crate.
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Fills `buf` with OS randomness.
pub fn random_bytes(buf: &mut [u8]) {
    getrandom::fill(buf).expect("the operating system random number generator failed");
}
