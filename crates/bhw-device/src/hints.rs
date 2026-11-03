//! What to tell the user when a phone does not show up or cannot be used. The app translates the hint ids.

use crate::devices::{DeviceState, ListedDevice};
use crate::error::DeviceError;

/// The host OS, for OS-specific hints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostOs {
    Windows,
    MacOs,
    Linux,
}

impl HostOs {
    /// This build's OS (anything else counts as Linux).
    pub fn current() -> HostOs {
        CURRENT_OS
    }
}

#[cfg(windows)]
const CURRENT_OS: HostOs = HostOs::Windows;
#[cfg(target_os = "macos")]
const CURRENT_OS: HostOs = HostOs::MacOs;
#[cfg(not(any(windows, target_os = "macos")))]
const CURRENT_OS: HostOs = HostOs::Linux;

/// A hint id for the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hint {
    /// No device at all: enable Developer options + USB debugging, use a data cable, choose "File transfer".
    EnableUsbDebugging,
    /// Windows lists nothing when the ADB interface has no driver: install the OEM / Google USB driver
    /// (Device Manager shows "ADB Interface" or the phone under "Other devices" with a warning sign).
    WindowsUsbDriver,
    /// Linux "no permissions": add udev rules (e.g. the `android-udev` package) and replug.
    LinuxUdevRules,
    /// Unauthorized: unlock the phone and accept "Allow USB debugging?" (tick "Always allow").
    AcceptRsaPrompt,
    /// Offline: replug the cable, or toggle USB debugging; restart the adb server.
    ReconnectOffline,
    /// Another adb (Android Studio, a phone manager app) of a different version keeps restarting the server.
    ConflictingAdb,
    /// The game is in Android's "stopped" state: open The Blockheads once on the phone, go back to the home screen,
    /// then try again.
    OpenGameOnce,
}

/// Hints for the current device list.
pub fn hints_for(os: HostOs, devices: &[ListedDevice]) -> Vec<Hint> {
    let mut hints = Vec::new();
    let mut add = |h| {
        if !hints.contains(&h) {
            hints.push(h)
        }
    };
    if devices.is_empty() {
        add(Hint::EnableUsbDebugging);
        if os == HostOs::Windows {
            add(Hint::WindowsUsbDriver);
        }
    }
    for d in devices {
        match d.state {
            DeviceState::Unauthorized => add(Hint::AcceptRsaPrompt),
            DeviceState::Offline => add(Hint::ReconnectOffline),
            DeviceState::NoPermissions(_) if os == HostOs::Linux => add(Hint::LinuxUdevRules),
            DeviceState::NoPermissions(_) => add(Hint::WindowsUsbDriver),
            _ => {}
        }
    }
    hints
}

/// The hint for an error, if there is one.
pub fn hint_for_error(e: &DeviceError) -> Option<Hint> {
    match e {
        DeviceError::GameStopped => Some(Hint::OpenGameOnce),
        DeviceError::Unauthorized { .. } => Some(Hint::AcceptRsaPrompt),
        DeviceError::Offline { .. } => Some(Hint::ReconnectOffline),
        _ => None,
    }
}

/// Whether adb's output says another adb version is fighting over the server port.
pub fn is_server_version_conflict(adb_output: &str) -> bool {
    adb_output.contains("doesn't match this client") || adb_output.contains("server version")
}
