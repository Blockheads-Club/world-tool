//! iPhone/iPad backups (Finder/iTunes) through [`bhw_ios`]: listed, opened (decrypted with the backup password)
//! and then offered as a read-only device whose worlds can be saved, exported or copied elsewhere.

use std::path::{Path, PathBuf};

use bhw_ios::{list_backups, read_backup_info, BackupInfo};
use bhw_save::GameDb;

use crate::error::{AppError, ErrorCode, Result};
use crate::read::{password, summarise, ReadSession};
use crate::types::{Connection, Device, DeviceKind, DeviceStatus, IosBackup};
use crate::util::{lock, unique_id};
use crate::worlds::to_client;
use crate::Core;

/// Backups in `dir`: the folder of one backup, or a folder holding backups.
pub fn backups_in(dir: &Path) -> Result<Vec<BackupInfo>> {
    if dir.join("Manifest.plist").is_file() {
        return Ok(vec![read_backup_info(dir)?]);
    }
    Ok(list_backups(dir)?)
}

fn model(info: &BackupInfo) -> String {
    let product = info.product_type.clone().unwrap_or("iPhone".into());
    info.ios_version.as_ref().map_or(product.clone(), |v| format!("{product} · iOS {v}"))
}

fn date_ms(info: &BackupInfo) -> u64 {
    info.date.unwrap_or(0).max(0) as u64 * 1000
}

impl Core {
    /// iPhone/iPad backups in `folder`, or in the default Finder/iTunes folders.
    pub fn ios_backups(&self, folder: Option<&str>) -> Result<Vec<IosBackup>> {
        let infos = match folder {
            Some(f) => backups_in(Path::new(f))?,
            None => (self.cfg.ios_locator)().iter().filter_map(|d| backups_in(d).ok()).flatten().collect(),
        };
        Ok(infos
            .into_iter()
            .map(|i| IosBackup {
                id: i.path.display().to_string(),
                name: i.device_name.clone().unwrap_or("iPhone".into()),
                model: model(&i),
                date: date_ms(&i),
                encrypted: i.encrypted,
            })
            .collect())
    }

    /// Opens a backup (`id` from [`Core::ios_backups`]); it then shows up as a read-only device whose worlds are
    /// already read.
    pub fn ios_open(&self, id: &str, pw: &str) -> Result<Device> {
        let dir = PathBuf::from(id);
        let opened = bhw_ios::IosBackup::open(&dir, password(pw).as_deref())
            .and_then(|b| Ok((b.info().clone(), b.blockheads()?)));
        let (info, data) = match opened {
            Ok(x) => x,
            Err(e) => {
                let mut err = AppError::from(e);
                if err.code == ErrorCode::WrongPassword {
                    let mut tries = lock(&self.ios_attempts);
                    let n = tries.entry(id.to_string()).or_default();
                    *n += 1;
                    err.extra.attempt = Some(*n);
                }
                return Err(err);
            }
        };
        lock(&self.ios_attempts).remove(id);
        let game_db = data
            .game_db
            .iter()
            .find(|f| f.path == "data.mdb")
            .and_then(|f| GameDb::from_bytes(&f.data.read_all().ok()?).ok());
        let worlds = data.worlds.into_iter().map(|w| to_client(w, true)).collect::<Result<Vec<_>>>()?;
        let worlds = summarise(worlds, game_db.as_ref());
        let device = Device {
            id: format!("ios:{}", info.id),
            kind: DeviceKind::Ios,
            name: info.device_name.clone().unwrap_or("iPhone".into()),
            model: model(&info),
            os_version: info.ios_version.clone(),
            status: DeviceStatus::Ready,
            connection: Connection::Backup,
            address: None,
            game_version: None,
            root: false,
            read_only: true,
            encrypted: info.encrypted,
            last_seen: None,
            world_count: Some(worlds.len()),
            path: Some(id.to_string()),
            backup_date: Some(date_ms(&info)),
        };
        self.drop_sessions(&device.id);
        self.add_session(ReadSession {
            id: unique_id("r"),
            device: device.clone(),
            backup: None,
            password: None,
            game_version: String::new(),
            worlds,
        });
        let mut ios = lock(&self.ios);
        ios.retain(|d| d.id != device.id);
        ios.push(device.clone());
        Ok(device)
    }
}
