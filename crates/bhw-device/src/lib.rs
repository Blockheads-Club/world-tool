//! Moves The Blockheads saves between this computer and Android devices (adb backup/restore over USB or Wi-Fi,
//! or root) and the Blockheads PC data folder. Produces and consumes raw `.ab` byte streams and plain
//! directories; parsing them is other crates' job.
//!
//! The `adb` client binary is spawned through the [`process::Runner`] trait; tests use [`fake::ScriptedRunner`].

pub mod adb;
pub mod devices;
pub mod error;
pub mod fake;
pub mod hints;
pub mod pc;
pub mod process;
pub mod root;
pub mod transfer;
pub mod wifi;

pub use adb::{Adb, RootStyle, Timeouts, PACKAGE};
pub use devices::{Connection, Device, DeviceDetails, DeviceEvent, DeviceState, DeviceWatcher, GameInfo, ListedDevice};
pub use error::{DeviceError, Result};
pub use pc::PcInstall;
pub use transfer::{AbMethod, AbOptions, BackupSummary, CancelToken, IfRunning, Phase, Progress};

#[cfg(test)]
mod tests;
