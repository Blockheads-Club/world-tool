//! Typed errors, one per state the app shows to the user.

use std::io;
use std::path::PathBuf;

/// Everything that can go wrong while talking to a phone or the Blockheads PC data folder.
#[derive(Debug, thiserror::Error)]
pub enum DeviceError {
    /// No `adb` binary: not bundled, not on `PATH`, not in the Android SDK.
    #[error("adb was not found")]
    AdbNotFound,
    /// The phone has not accepted this computer's key ("Allow USB debugging?" not confirmed yet).
    #[error("device {serial} has not authorized this computer")]
    Unauthorized { serial: String },
    /// The OS does not let adb open the USB device (Linux udev rules, Windows driver).
    #[error("no permission to open USB device {serial}: {detail}")]
    NoPermissions { serial: String, detail: String },
    /// adb lists the device as offline.
    #[error("device {serial} is offline")]
    Offline { serial: String },
    /// The serial is not (or no longer) connected.
    #[error("device {serial} was not found")]
    DeviceNotFound { serial: String },
    /// `com.noodlecake.blockheads` is not installed on the device.
    #[error("The Blockheads is not installed")]
    GameNotInstalled,
    /// The installed game is not one of the versions whose saves we understand.
    #[error("The Blockheads {found} is not supported (supported: 1.7.4 to 1.7.6)")]
    UnsupportedVersion { found: String },
    /// The game is running; its saves may change under us.
    #[error("The Blockheads is running")]
    GameRunning,
    /// The game is in Android's "stopped" state (force-stopped, or never opened since install), so `adb backup`
    /// would skip it and return an empty backup. Fix: open the game once, go back to the home screen, retry.
    #[error("The Blockheads is stopped: open it once on the phone, go back to the home screen, then try again")]
    GameStopped,
    /// The backup ended with no data at all: the user tapped "Do not back up" (or the phone refused silently).
    #[error("the backup was declined on the phone")]
    DeclinedOnPhone,
    /// Nobody confirmed the backup/restore screen on the phone in time.
    #[error("the backup/restore was not confirmed on the phone in time")]
    ConfirmTimeout,
    /// The backup password was wrong. Detected by the caller that decrypts the `.ab`; listed here so the
    /// app has one error type.
    #[error("wrong backup password")]
    WrongPassword,
    /// The phone produced a backup without any files: Android's backup feature is broken or disabled for
    /// this app on this device (use root or another way).
    #[error("the phone produced an empty backup")]
    EmptyBackup,
    /// The stream from adb did not start with an Android backup header.
    #[error("adb did not return an Android backup")]
    NotABackup,
    /// The device went away (cable, Wi-Fi, adb server) during a transfer.
    #[error("the device disconnected during the transfer")]
    Disconnected,
    /// The local disk is full.
    #[error("the disk is full")]
    DiskFull,
    /// The caller cancelled.
    #[error("cancelled")]
    Cancelled,
    /// `su` is missing or was denied.
    #[error("root access is not available")]
    RootUnavailable,
    /// No `saves` directory was found in the app's data.
    #[error("no Blockheads saves were found")]
    SavesNotFound,
    /// A world with this id is already there and overwriting was not asked for.
    #[error("world {0} already exists")]
    WorldExists(String),
    /// A local path that should exist does not.
    #[error("not found: {0}")]
    PathNotFound(PathBuf),
    /// `adb pair` failed (wrong code, expired port, ...).
    #[error("pairing failed: {0}")]
    PairFailed(String),
    /// `adb connect` failed.
    #[error("connecting failed: {0}")]
    ConnectFailed(String),
    /// A device-side command failed.
    #[error("command failed ({code}): {message}")]
    Command { code: i32, message: String },
    /// A command did not finish in time.
    #[error("adb did not answer in time")]
    Timeout,
    /// Any other I/O error.
    #[error(transparent)]
    Io(#[from] io::Error),
}

/// Result alias.
pub type Result<T, E = DeviceError> = std::result::Result<T, E>;

/// Maps an error from writing to local storage: "no space left" becomes [`DeviceError::DiskFull`].
pub fn local_write_error(e: io::Error) -> DeviceError {
    // ENOSPC (28) on Unix, ERROR_HANDLE_DISK_FULL (39) / ERROR_DISK_FULL (112) on Windows.
    let raw_full = match e.raw_os_error() {
        Some(28) => cfg!(unix),
        Some(39 | 112) => cfg!(windows),
        _ => false,
    };
    if raw_full || e.kind() == io::ErrorKind::StorageFull {
        DeviceError::DiskFull
    } else {
        DeviceError::Io(e)
    }
}

/// Recognises the adb client's own error messages (stderr, sometimes stdout).
pub fn classify_adb_message(text: &str, serial: &str) -> Option<DeviceError> {
    let t = text.to_ascii_lowercase();
    let serial = serial.to_string();
    if t.contains("unauthorized") {
        Some(DeviceError::Unauthorized { serial })
    } else if t.contains("insufficient permissions") || t.contains("no permissions") {
        Some(DeviceError::NoPermissions { serial, detail: text.trim().to_string() })
    } else if t.contains("device offline") {
        Some(DeviceError::Offline { serial })
    } else if (t.contains("not found") && t.contains("device")) || t.contains("no devices/emulators found") {
        Some(DeviceError::DeviceNotFound { serial })
    } else if t.contains("error: closed")
        || t.contains("protocol fault")
        || t.contains("connection reset")
        || t.contains("broken pipe")
        || t.contains("device disconnected")
    {
        Some(DeviceError::Disconnected)
    } else {
        None
    }
}
