//! The data the UI exchanges with the backend; mirrors `app/src/lib/types.ts` field for field (snake_case).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::error::ErrorCode;

/// Kind of device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceKind {
    Android,
    Pc,
    Ios,
}

/// Whether a device can be used.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceStatus {
    Ready,
    Unauthorized,
    Offline,
}

/// How a device is attached.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Connection {
    Usb,
    Wifi,
    Local,
    Backup,
}

/// A phone, Blockheads PC or an iPhone backup.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Device {
    pub id: String,
    pub kind: DeviceKind,
    pub name: String,
    pub model: String,
    pub os_version: Option<String>,
    pub status: DeviceStatus,
    pub connection: Connection,
    pub address: Option<String>,
    pub game_version: Option<String>,
    pub root: bool,
    pub read_only: bool,
    pub encrypted: bool,
    pub last_seen: Option<u64>,
    pub world_count: Option<usize>,
    pub path: Option<String>,
    pub backup_date: Option<u64>,
}

/// A device found by Wi-Fi discovery.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WifiCandidate {
    pub name: String,
    pub address: String,
}

/// A computer key the phone accepted earlier (adb's known hosts).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairedDevice {
    pub id: String,
    pub name: String,
    pub address: String,
}

/// An iPhone/iPad backup found on this computer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IosBackup {
    pub id: String,
    pub name: String,
    pub model: String,
    pub date: u64,
    pub encrypted: bool,
}

/// Game mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GameMode {
    Normal,
    Expert,
}

/// Where a library world came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    Phone,
    Pc,
    Ios,
    File,
}

/// A world found on a device or offered by a source.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeviceWorld {
    pub id: String,
    pub name: String,
    pub size: u64,
    pub game_version: String,
    pub last_played: Option<u64>,
    pub created: u64,
    pub mode: GameMode,
    pub preview: Option<String>,
}

/// A world in the library.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct World {
    #[serde(flatten)]
    pub base: DeviceWorld,
    pub world_id: String,
    pub origin: Origin,
    pub source: String,
    pub source_detail: Option<String>,
    pub added: u64,
    pub edited: Option<u64>,
}

/// Export format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportFormat {
    Bhworld,
    Server,
}

/// One written file or folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportedFile {
    pub name: String,
    pub size: u64,
}

/// Step of a long operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    PhoneBackup,
    PhoneRestore,
    Reading,
    BackingUp,
    Preparing,
    Writing,
    Checking,
    Exporting,
    Preview,
    ApkChecking,
    ApkExtracting,
    ApkPulling,
    Downloading,
}

/// One progress event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Progress {
    pub stage: Stage,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub done: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub index: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub count: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub eta: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub elapsed: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

impl Progress {
    /// Only the stage.
    pub fn stage(stage: Stage) -> Progress {
        Progress {
            stage,
            done: None,
            total: None,
            item: None,
            index: None,
            count: None,
            eta: None,
            elapsed: None,
            path: None,
            version: None,
        }
    }

    /// Bytes or items done of a total.
    pub fn bytes(stage: Stage, done: u64, total: Option<u64>) -> Progress {
        Progress { done: Some(done), total, ..Progress::stage(stage) }
    }

    /// Item `index` (1-based) of `count`.
    pub fn item(stage: Stage, item: &str, index: usize, count: usize) -> Progress {
        Progress {
            item: Some(item.to_string()),
            index: Some(index),
            count: Some(count),
            done: Some(index as u64 - 1),
            total: Some(count as u64),
            ..Progress::stage(stage)
        }
    }
}

/// Read worlds from a device.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadRequest {
    pub device_id: String,
    #[serde(default)]
    pub password: String,
}

/// Worlds found by a read.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReadResult {
    pub read_id: String,
    pub worlds: Vec<DeviceWorld>,
}

/// Export worlds of the library or of a read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportRequest {
    pub read_id: Option<String>,
    pub world_ids: Vec<String>,
    pub format: ExportFormat,
    pub folder: String,
}

/// What an export wrote.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExportResult {
    pub folder: String,
    pub files: Vec<ExportedFile>,
    pub saved: Vec<World>,
}

/// Where worlds come from in a transfer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Source {
    Library,
    Files { paths: Vec<String> },
    Device { device_id: String },
}

/// What to do with a world that clashes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Choice {
    #[default]
    KeepBoth,
    Replace,
    Skip,
}

/// Row of a transfer plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanResult {
    Stays,
    Added,
    Conflict,
}

/// Why worlds clash.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictKind {
    SameName,
    SameId,
}

/// One row of a transfer plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanItem {
    pub key: String,
    pub name: String,
    pub size: u64,
    pub result: PlanResult,
    pub conflict: Option<ConflictKind>,
    pub copy_name: Option<String>,
}

/// What a transfer will do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferPlan {
    pub plan_id: String,
    pub target: String,
    pub total_after: usize,
    pub items: Vec<PlanItem>,
    pub size: u64,
    pub backup_name: String,
    pub backup_size: u64,
    pub needs_phone: bool,
}

/// Run a plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferRequest {
    pub plan_id: String,
    #[serde(default)]
    pub choices: BTreeMap<String, Choice>,
}

/// What a transfer did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TransferResult {
    pub target: String,
    pub total: usize,
    pub added: Vec<String>,
    pub replaced: Vec<String>,
    pub kept_both: Vec<String>,
    pub backup_id: Option<String>,
    pub backup_time: Option<u64>,
}

/// Kind of a file offered for import.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportKind {
    Bhworld,
    Server,
    Invalid,
}

/// Import status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportStatus {
    New,
    Conflict,
    Invalid,
}

/// A file or folder offered for import.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportItem {
    pub path: String,
    pub name: String,
    pub kind: ImportKind,
    pub size: u64,
    pub game_version: Option<String>,
    pub status: ImportStatus,
    pub conflict: Option<ConflictKind>,
    pub existing: Option<String>,
}

/// Import files into the library.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportRequest {
    #[serde(default)]
    pub choices: BTreeMap<String, Choice>,
    pub paths: Vec<String>,
}

/// Why a backup was made.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackupReason {
    BeforeWrite,
    Read,
    BeforeRestore,
}

/// A stored device backup.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Backup {
    pub id: String,
    pub device_id: String,
    pub device: String,
    pub reason: BackupReason,
    pub reason_count: Option<usize>,
    pub time: u64,
    pub worlds: usize,
    pub size: u64,
    pub automatic: bool,
}

/// Kind of an activity entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivityKind {
    Read,
    Copy,
    Backup,
    Restore,
    Import,
    Export,
}

/// How it ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivityResult {
    Done,
    Failed,
    Stopped,
}

/// One entry of the activity log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Activity {
    pub id: String,
    pub time: u64,
    pub kind: ActivityKind,
    pub result: ActivityResult,
    pub device: String,
    pub worlds: Vec<String>,
    pub size: Option<u64>,
    pub detail: Option<String>,
    pub error: Option<ErrorCode>,
}

/// State of the game files cache.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GameFilesState {
    NotSet,
    Ready,
    Unsupported,
    Damaged,
    Missing,
}

/// Game files from the user's APK.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameFiles {
    pub state: GameFilesState,
    pub apk_path: Option<String>,
    pub apk_name: Option<String>,
    pub version: Option<String>,
    pub images: Option<usize>,
    pub size: Option<u64>,
}
