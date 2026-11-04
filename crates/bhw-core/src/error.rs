//! The one error type of the app: what the UI shows (`AppError` in `app/src/lib/types.ts`).

use serde::{Deserialize, Serialize};
use std::io;

/// Picks the dialog or message in the UI (`ErrorCode` in `types.ts`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    NotAllowed,
    GameNotInstalled,
    UnsupportedVersion,
    Declined,
    WrongPassword,
    BackupEmpty,
    Disconnected,
    DiskFull,
    WorldDamaged,
    ApkWrongApp,
    ApkUnsupported,
    ApkDamaged,
    ApkMissing,
    PairFailed,
    Offline,
    Cancelled,
    NotFound,
    NotImplemented,
    Io,
    /// The game is running (Blockheads PC, or a phone where it could not be stopped).
    GameRunning,
    /// Android's "stopped" state: open the game once on the phone, then try again.
    GameStopped,
    /// iPhone/iPad backups are not readable yet.
    NotSupportedYet,
    /// No adb binary was found.
    AdbMissing,
    /// Downloading Google's Platform-Tools (adb) failed.
    DownloadFailed,
    /// After `adb restore` the phone does not have the worlds: "Restore my data" was probably not tapped.
    RestoreDeclined,
}

/// A failure as the UI receives it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppError {
    pub code: ErrorCode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// The rarer fields (boxed so `Result<_, AppError>` stays small).
    #[serde(flatten)]
    pub extra: Box<Extra>,
}

/// Fields that fill in some dialogs.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Extra {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub needed: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub free: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disk: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub percent: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backup_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attempt: Option<u32>,
}

/// Result alias of this crate.
pub type Result<T, E = AppError> = std::result::Result<T, E>;

impl AppError {
    /// An error with only a code.
    pub fn new(code: ErrorCode) -> AppError {
        AppError { code, detail: None, extra: Box::default() }
    }

    /// `code` with a technical detail line.
    pub fn with(code: ErrorCode, detail: impl ToString) -> AppError {
        AppError::new(code).detail(detail)
    }

    /// Sets the detail line.
    pub fn detail(mut self, d: impl ToString) -> AppError {
        self.detail = Some(d.to_string());
        self
    }

    /// Sets the device name (kept when already set).
    pub fn device(mut self, d: &str) -> AppError {
        self.extra.device.get_or_insert_with(|| d.to_string());
        self
    }

    /// Sets the backup that can undo a broken write.
    pub fn backup(mut self, id: Option<&str>) -> AppError {
        self.extra.backup_id = id.map(str::to_string);
        self
    }

    /// Sets the file or world name.
    pub fn name(mut self, n: impl ToString) -> AppError {
        self.extra.name = Some(n.to_string());
        self
    }

    /// Sets the version found.
    pub fn version(mut self, v: impl ToString) -> AppError {
        self.extra.version = Some(v.to_string());
        self
    }

    /// Something that does not exist (any more).
    pub fn not_found(what: impl ToString) -> AppError {
        AppError::with(ErrorCode::NotFound, what)
    }

    /// Generic failure.
    pub fn io(what: impl ToString) -> AppError {
        AppError::with(ErrorCode::Io, what)
    }
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let code = serde_json::to_value(self.code).unwrap_or_default();
        write!(f, "{}", code.as_str().unwrap_or("io"))?;
        if let Some(d) = &self.detail {
            write!(f, ": {d}")?;
        }
        Ok(())
    }
}

impl std::error::Error for AppError {}

impl From<io::Error> for AppError {
    fn from(e: io::Error) -> AppError {
        let detail = e.to_string();
        if e.kind() == io::ErrorKind::NotFound {
            return AppError::with(ErrorCode::NotFound, detail);
        }
        match bhw_device::error::local_write_error(e) {
            bhw_device::DeviceError::DiskFull => AppError::with(ErrorCode::DiskFull, detail),
            _ => AppError::io(detail),
        }
    }
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> AppError {
        AppError::io(e)
    }
}

impl From<bhw_device::DeviceError> for AppError {
    fn from(e: bhw_device::DeviceError) -> AppError {
        use bhw_device::DeviceError as D;
        let detail = e.to_string();
        let code = match e {
            D::Io(e) => return e.into(),
            D::AdbNotFound => ErrorCode::AdbMissing,
            D::Unauthorized { .. } | D::NoPermissions { .. } => ErrorCode::NotAllowed,
            D::Offline { .. } | D::Disconnected => ErrorCode::Disconnected,
            D::DeviceNotFound { .. } | D::PathNotFound(_) | D::SavesNotFound => ErrorCode::NotFound,
            D::GameNotInstalled => ErrorCode::GameNotInstalled,
            D::UnsupportedVersion { found } => {
                return AppError::with(ErrorCode::UnsupportedVersion, detail).version(found)
            }
            D::GameRunning => ErrorCode::GameRunning,
            D::GameStopped => ErrorCode::GameStopped,
            D::DeclinedOnPhone | D::ConfirmTimeout => ErrorCode::Declined,
            D::WrongPassword => ErrorCode::WrongPassword,
            D::EmptyBackup => ErrorCode::BackupEmpty,
            D::DiskFull => ErrorCode::DiskFull,
            D::Cancelled => ErrorCode::Cancelled,
            D::PairFailed(_) | D::ConnectFailed(_) => ErrorCode::PairFailed,
            _ => ErrorCode::Io,
        };
        AppError::with(code, detail)
    }
}

impl From<bhw_backup::Error> for AppError {
    fn from(e: bhw_backup::Error) -> AppError {
        use bhw_backup::Error as B;
        match e {
            B::Io(e) => e.into(),
            B::WrongPassword | B::PasswordRequired => AppError::with(ErrorCode::WrongPassword, e),
            e => AppError::io(e),
        }
    }
}

impl From<bhw_world::Error> for AppError {
    fn from(e: bhw_world::Error) -> AppError {
        use bhw_world::Error as W;
        match e {
            W::Io(e) => e.into(),
            W::Backup(e) => e.into(),
            W::AlreadyExists(_) | W::InvalidId(_) | W::InvalidPath(_) => AppError::io(e),
            e => AppError::with(ErrorCode::WorldDamaged, e),
        }
    }
}

impl From<bhw_save::Error> for AppError {
    fn from(e: bhw_save::Error) -> AppError {
        match e {
            bhw_save::Error::Io { source, path } => {
                let mut a = AppError::from(source);
                a.detail = Some(format!("{}: {}", path.display(), a.detail.unwrap_or_default()));
                a
            }
            e => AppError::with(ErrorCode::WorldDamaged, e),
        }
    }
}

impl From<bhw_lmdb::Error> for AppError {
    fn from(e: bhw_lmdb::Error) -> AppError {
        AppError::with(ErrorCode::WorldDamaged, e)
    }
}

impl From<bhw_ios::Error> for AppError {
    fn from(e: bhw_ios::Error) -> AppError {
        use bhw_ios::Error as I;
        let detail = e.to_string();
        let code = match e {
            I::Io(e) => return e.into(),
            I::World(e) => return e.into(),
            I::PasswordRequired | I::WrongPassword => ErrorCode::WrongPassword,
            I::NoBlockheads => ErrorCode::GameNotInstalled,
            I::Unsupported(_) => ErrorCode::NotSupportedYet,
            I::NotABackup(_) | I::MissingFile(_) => ErrorCode::NotFound,
            _ => ErrorCode::Io,
        };
        AppError::with(code, detail)
    }
}

impl From<bhw_assets::Error> for AppError {
    fn from(e: bhw_assets::Error) -> AppError {
        use bhw_assets::Error as A;
        let detail = e.to_string();
        match e {
            A::Io(e) => e.into(),
            A::WrongApp(_) => AppError::with(ErrorCode::ApkWrongApp, detail),
            A::Unsupported(v) => AppError::with(ErrorCode::ApkUnsupported, detail).version(v),
            A::MissingSource(_) | A::NotSet => AppError::with(ErrorCode::ApkMissing, detail),
            A::Cancelled => AppError::with(ErrorCode::Cancelled, detail),
            _ => AppError::with(ErrorCode::ApkDamaged, detail),
        }
    }
}
