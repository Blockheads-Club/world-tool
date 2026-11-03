//! Reading worlds from a device: an `adb backup` kept in a temporary file (Android) or the saves folder
//! (Blockheads PC), summarised into a read session.

use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use bhw_device::{AbMethod, AbOptions, IfRunning, Phase};
use bhw_save::GameDb;
use bhw_world::backup::{load_backup, BackupSaves};
use bhw_world::{SourceKind, WorldFiles};

use crate::activity::Entry;
use crate::error::{AppError, Result};
use crate::library::AddMeta;
use crate::types::{
    ActivityKind, Connection, Device, DeviceKind, DeviceWorld, Origin, Progress, ReadRequest, ReadResult, Stage, World,
};
use crate::util::{lock, unique_id, Cancel, TempPath, Throttle};
use crate::worlds::{inspect, WorldInfo};
use crate::Core;

/// Where the game keeps its data inside an `adb backup`.
pub const SUPPORT_PREFIX: &str = "apps/com.noodlecake.blockheads/f/Library/Application Support/";
/// Read sessions kept (older ones are dropped with their files).
pub const MAX_SESSIONS: usize = 4;

/// One world of a read.
#[derive(Debug, Clone)]
pub struct SessionWorld {
    pub info: WorldInfo,
    pub files: WorldFiles,
}

/// What a read found.
#[derive(Debug)]
pub struct ReadSession {
    pub id: String,
    pub device: Device,
    /// The device backup (Android), removed with the session.
    pub backup: Option<TempPath>,
    pub password: Option<String>,
    pub game_version: String,
    pub worlds: Vec<SessionWorld>,
}

impl ReadSession {
    /// World `id` (its world id).
    pub fn world(&self, id: &str) -> Result<&SessionWorld> {
        self.worlds.iter().find(|w| w.info.world_id == id).ok_or_else(|| AppError::not_found(id))
    }

    /// The worlds as the UI lists them.
    pub fn device_worlds(&self) -> Vec<DeviceWorld> {
        self.worlds.iter().map(|w| w.info.device_world(&w.info.world_id, &self.game_version)).collect()
    }

    /// Size of the device backup (or of all worlds).
    pub fn size(&self) -> u64 {
        match &self.backup {
            Some(b) => std::fs::metadata(&b.0).map_or(0, |m| m.len()),
            None => self.worlds.iter().map(|w| w.info.size).sum(),
        }
    }
}

/// The game's saves in a device backup: the saves folder of the game's package (or of the first app with saves).
pub fn pick_saves(saves: Vec<BackupSaves>) -> Option<BackupSaves> {
    let mut saves = saves.into_iter();
    let first = saves.next()?;
    if first.package.as_deref() == Some(bhw_device::PACKAGE) {
        return Some(first);
    }
    Some(saves.find(|s| s.package.as_deref() == Some(bhw_device::PACKAGE)).unwrap_or(first))
}

/// The `game_db` of a backup's saves folder.
pub fn backup_game_db(saves: &BackupSaves) -> Option<GameDb> {
    let f = saves.game_db.iter().find(|f| f.path == "data.mdb")?;
    GameDb::from_bytes(&f.data.read_all().ok()?).ok()
}

/// Summaries of client worlds; worlds that cannot be read and multiplayer caches are left out.
pub fn summarise(worlds: Vec<WorldFiles>, game_db: Option<&GameDb>) -> Vec<SessionWorld> {
    worlds
        .into_iter()
        .filter_map(|files| {
            let info = inspect(&files, game_db).ok().filter(|i| !i.remote)?;
            Some(SessionWorld { info, files })
        })
        .collect()
}

/// Opens a backup file.
pub fn open_ab(path: &Path, password: Option<&str>) -> Result<Vec<BackupSaves>> {
    let f = std::fs::File::open(path)?;
    Ok(load_backup(Box::new(BufReader::new(f)), password)?.1)
}

/// `Some(password)` unless empty.
pub fn password(p: &str) -> Option<String> {
    Some(p.to_string()).filter(|p| !p.is_empty())
}

impl Core {
    /// The newest read session of a device.
    pub(crate) fn latest_session(&self, device_id: &str) -> Option<Arc<ReadSession>> {
        lock(&self.reads).iter().rev().find(|s| s.device.id == device_id).cloned()
    }

    fn session(&self, read_id: &str) -> Result<Arc<ReadSession>> {
        lock(&self.reads).iter().find(|s| s.id == read_id).cloned().ok_or_else(|| AppError::not_found(read_id))
    }

    pub(crate) fn add_session(&self, s: ReadSession) -> Arc<ReadSession> {
        let s = Arc::new(s);
        let mut reads = lock(&self.reads);
        reads.push(s.clone());
        let extra = reads.len().saturating_sub(MAX_SESSIONS);
        reads.drain(..extra);
        s
    }

    /// Forgets what is known about a device's worlds (after writing to it).
    pub(crate) fn drop_sessions(&self, device_id: &str) {
        lock(&self.reads).retain(|s| s.device.id != device_id);
    }

    fn reads_dir(&self) -> Result<PathBuf> {
        let d = self.cfg.cache_dir.join("reads");
        std::fs::create_dir_all(&d)?;
        Ok(d)
    }

    /// `adb backup` of the game into `path`, reporting `phone_backup` while waiting and `stage` while bytes move.
    pub(crate) fn backup_phone(
        &self,
        serial: &str,
        path: &Path,
        stage: Stage,
        total: Option<u64>,
        progress: &mut dyn FnMut(Progress),
        cancel: &Cancel,
    ) -> Result<u64> {
        let adb = self.adb()?;
        let game = adb.game_info(serial)?;
        game.check_supported()?;
        let mut t = Throttle::default();
        let opts = AbOptions { method: AbMethod::AdbCommand, if_running: IfRunning::Stop };
        let mut on = |p: bhw_device::Progress| match p.phase {
            Phase::WaitingForPhone => progress(Progress { elapsed: Some(0), ..Progress::stage(Stage::PhoneBackup) }),
            _ => {
                if t.pass(p.bytes, p.phase == Phase::Done) {
                    progress(Progress::bytes(stage, p.bytes, p.total.or(total)));
                }
            }
        };
        Ok(adb.backup_to_file(serial, path, opts, &mut on, &cancel.device)?.bytes)
    }

    /// A read session from a device backup file (which the session then owns).
    pub(crate) fn session_from_backup(
        &self,
        device: &Device,
        backup: TempPath,
        password: Option<String>,
        game_version: &str,
    ) -> Result<ReadSession> {
        let saves = pick_saves(open_ab(&backup.0, password.as_deref())?);
        let worlds = match saves {
            Some(s) => summarise(s.worlds.clone(), backup_game_db(&s).as_ref()),
            None => Vec::new(),
        };
        Ok(ReadSession {
            id: unique_id("r"),
            device: device.clone(),
            backup: Some(backup),
            password,
            game_version: game_version.to_string(),
            worlds,
        })
    }

    fn read_android(
        &self,
        d: &Device,
        req: &ReadRequest,
        progress: &mut dyn FnMut(Progress),
        cancel: &Cancel,
    ) -> Result<ReadSession> {
        let file = TempPath(self.reads_dir()?.join(format!("{}.ab", unique_id("read-"))));
        let total = self.latest_session(&d.id).map(|s| s.size());
        self.backup_phone(&d.id, &file.0, Stage::Reading, total, progress, cancel)?;
        let version = d.game_version.clone().unwrap_or_default();
        self.session_from_backup(d, file, password(&req.password), &version)
    }

    /// A read of Blockheads PC's saves folder (worlds stay on disk until used).
    pub(crate) fn read_pc(&self, d: &Device, progress: &mut dyn FnMut(Progress)) -> Result<ReadSession> {
        let pc = self.pc().ok_or_else(|| AppError::not_found("Blockheads PC"))?;
        let game_db = GameDb::open(&pc.game_db()).ok();
        let list = pc.list_worlds()?;
        let mut worlds = Vec::new();
        for (i, w) in list.iter().enumerate() {
            progress(Progress::item(Stage::Reading, &w.id, i + 1, list.len()));
            worlds.push(WorldFiles::load_dir(&w.path, w.id.clone())?);
        }
        Ok(ReadSession {
            id: unique_id("r"),
            device: d.clone(),
            backup: None,
            password: None,
            game_version: String::new(),
            worlds: summarise(worlds, game_db.as_ref()),
        })
    }

    /// Reads the worlds of a device.
    pub fn read(&self, op: &str, req: &ReadRequest, progress: &mut dyn FnMut(Progress)) -> Result<ReadResult> {
        let guard = self.ops.start(op);
        let mut entry = Entry::new(ActivityKind::Read, &req.device_id);
        let r = (|| {
            let d = self.ready_device(&req.device_id)?;
            entry.device = d.name.clone();
            let s = match d.kind {
                DeviceKind::Pc => self.add_session(self.read_pc(&d, progress)?),
                DeviceKind::Ios => self.latest_session(&d.id).ok_or_else(|| AppError::not_found(&d.id))?,
                DeviceKind::Android => self
                    .add_session(self.read_android(&d, req, progress, &guard.cancel).map_err(|e| e.device(&d.name))?),
            };
            entry.worlds = s.worlds.iter().map(|w| w.info.name.clone()).collect();
            entry.size = Some(s.worlds.iter().map(|w| w.info.size).sum());
            Ok(ReadResult { read_id: s.id.clone(), worlds: s.device_worlds() })
        })();
        self.activity.record(&entry, r)
    }

    fn add_meta(device: &Device) -> AddMeta {
        let (origin, kind, source) = match device.kind {
            DeviceKind::Pc => (Origin::Pc, SourceKind::BlockheadsPc, device.name.clone()),
            DeviceKind::Ios => (Origin::Ios, SourceKind::IosBackup, format!("{} backup", device.name)),
            DeviceKind::Android => (Origin::Phone, SourceKind::AndroidBackup, device.name.clone()),
        };
        AddMeta {
            origin,
            source,
            source_detail: match device.connection {
                Connection::Usb => Some("USB".into()),
                Connection::Wifi => Some("Wi-Fi".into()),
                _ => None,
            },
            game_version: device.game_version.clone(),
            kind,
            device_name: Some(device.name.clone()),
        }
    }

    /// Saves worlds of a read to the library.
    pub fn save_to_library(&self, read_id: &str, world_ids: &[String]) -> Result<Vec<World>> {
        let s = self.session(read_id)?;
        let lib = self.lib()?;
        let meta = Core::add_meta(&s.device);
        world_ids
            .iter()
            .map(|id| {
                let w = s.world(id)?;
                lib.add(&w.files, &w.info, &meta)
            })
            .collect()
    }
}
