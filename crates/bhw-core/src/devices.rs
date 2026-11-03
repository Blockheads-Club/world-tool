//! Devices: Android phones through adb, the local Blockheads PC install, Wi-Fi pairing and diagnostics.

use bhw_device::wifi::{forget_paired_host, paired_hosts, MdnsKind};
use bhw_device::DeviceState;

use crate::error::{AppError, ErrorCode, Result};
use crate::types::{Connection, Device, DeviceKind, DeviceStatus, PairedDevice, WifiCandidate};
use crate::util::lock;
use crate::Core;

/// Id of the Blockheads PC "device".
pub const PC_ID: &str = "pc";

impl Core {
    fn android_device(&self, d: bhw_device::Device) -> Device {
        let l = d.listed;
        let details = d.details;
        let status = match l.state {
            DeviceState::Device => DeviceStatus::Ready,
            DeviceState::Unauthorized => DeviceStatus::Unauthorized,
            _ => DeviceStatus::Offline,
        };
        let wifi = l.connection == bhw_device::Connection::Wifi;
        let model = details
            .as_ref()
            .map(|x| x.model.clone())
            .filter(|m| !m.is_empty())
            .or_else(|| l.model.as_ref().map(|m| m.replace('_', " ")))
            .unwrap_or_else(|| l.serial.clone());
        let world_count = self.latest_session(&l.serial).map(|s| s.worlds.len());
        Device {
            id: l.serial.clone(),
            kind: DeviceKind::Android,
            name: model.clone(),
            model,
            os_version: details.as_ref().map(|x| x.android_release.clone()).filter(|r| !r.is_empty()),
            status,
            connection: if wifi { Connection::Wifi } else { Connection::Usb },
            address: wifi.then(|| l.serial.clone()),
            game_version: details.and_then(|x| x.game).map(|g| g.version_name),
            root: false,
            read_only: false,
            encrypted: false,
            last_seen: None,
            world_count,
            path: None,
            backup_date: None,
        }
    }

    fn pc_device(&self) -> Option<Device> {
        let pc = self.pc()?;
        let path = pc.data_dir.display().to_string();
        Some(Device {
            id: PC_ID.into(),
            kind: DeviceKind::Pc,
            name: "Blockheads PC".into(),
            model: path.clone(),
            os_version: None,
            status: DeviceStatus::Ready,
            connection: Connection::Local,
            address: None,
            game_version: None,
            root: false,
            read_only: false,
            encrypted: false,
            last_seen: None,
            world_count: pc.list_worlds().ok().map(|w| w.len()),
            path: Some(path),
            backup_date: None,
        })
    }

    /// Phones adb sees (with model, Android and game version) and Blockheads PC when it is installed. adb
    /// problems leave the phones out (the diagnostics say why).
    pub fn list_devices(&self) -> Vec<Device> {
        let phones = self.adb().and_then(|a| Ok(a.discover(false)?)).unwrap_or_default();
        let mut out: Vec<Device> = phones.into_iter().map(|d| self.android_device(d)).collect();
        out.extend(self.pc_device());
        *lock(&self.devices) = out.clone();
        out
    }

    /// A device by id, freshly listed.
    pub(crate) fn device(&self, id: &str) -> Result<Device> {
        self.list_devices().into_iter().find(|d| d.id == id).ok_or_else(|| AppError::not_found(id))
    }

    /// A device that can be used now.
    pub(crate) fn ready_device(&self, id: &str) -> Result<Device> {
        let d = self.device(id)?;
        let code = match (d.kind, d.status) {
            (DeviceKind::Ios, _) => return Err(crate::read::ios_not_supported().device(&d.name)),
            (_, DeviceStatus::Unauthorized) => ErrorCode::NotAllowed,
            (_, DeviceStatus::Offline) => ErrorCode::Disconnected,
            _ => return Ok(d),
        };
        Err(AppError::new(code).device(&d.name))
    }

    /// iPhone/iPad backups (not readable yet).
    pub fn ios_backups(&self, _folder: Option<&str>) -> Result<Vec<Device>> {
        Err(crate::read::ios_not_supported())
    }

    /// Opens an iPhone/iPad backup (not readable yet).
    pub fn ios_open(&self, _id: &str, _password: &str) -> Result<Device> {
        Err(crate::read::ios_not_supported())
    }

    /// Phones offering wireless debugging (pairing screen open, or already paired).
    pub fn wifi_discover(&self) -> Result<Vec<WifiCandidate>> {
        let services = self.adb()?.mdns_services()?;
        Ok(services
            .into_iter()
            .filter(|s| matches!(s.kind, MdnsKind::Pairing | MdnsKind::Connect))
            .map(|s| WifiCandidate { name: s.name, address: s.address })
            .collect())
    }

    /// Pairs with a phone ("Pair device with pairing code"), connects to it and returns it.
    pub fn wifi_pair(&self, address: &str, code: &str) -> Result<Device> {
        let adb = self.adb()?;
        let address = address.trim();
        let code: String = code.chars().filter(|c| !c.is_whitespace()).collect();
        adb.pair(address, &code)?;
        let host = address.rsplit_once(':').map_or(address, |(h, _)| h);
        let services = adb.mdns_services().unwrap_or_default();
        if let Some(s) =
            services.iter().find(|s| s.kind == MdnsKind::Connect && s.address.starts_with(&format!("{host}:")))
        {
            adb.connect(&s.address)?;
        }
        self.list_devices().into_iter().find(|d| d.kind == DeviceKind::Android && d.id.starts_with(host)).ok_or_else(
            || {
                AppError::with(
                    ErrorCode::PairFailed,
                    "paired, but the phone did not connect; open Wireless debugging on it",
                )
            },
        )
    }

    /// Computers keys adb remembers for wireless debugging.
    pub fn paired_devices(&self) -> Result<Vec<PairedDevice>> {
        let Some(dir) = &self.cfg.android_dir else { return Ok(Vec::new()) };
        Ok(paired_hosts(dir)?
            .into_iter()
            .map(|g| PairedDevice { id: g.clone(), name: g, address: String::new() })
            .collect())
    }

    /// Forgets a paired phone (restarts the adb server so it takes effect).
    pub fn forget_paired(&self, id: &str) -> Result<()> {
        let dir = self.cfg.android_dir.as_ref().ok_or_else(|| AppError::not_found(id))?;
        if !forget_paired_host(dir, id)? {
            return Err(AppError::not_found(id));
        }
        let _ = self.adb().map(|a| a.kill_server());
        Ok(())
    }

    /// Restarts the adb server.
    pub fn restart_adb(&self) -> Result<()> {
        let adb = self.adb()?;
        let _ = adb.kill_server();
        adb.start_server()?;
        Ok(())
    }

    /// A text report for support: versions, adb, devices, folders.
    pub fn diagnostics(&self) -> String {
        let mut lines = vec![format!("BH World Tool {} · {}", self.cfg.app_version, std::env::consts::OS)];
        lines.push(match self.adb() {
            Ok(a) => match a.version() {
                Ok(v) => format!("adb: {v} ({})", a.program().display()),
                Err(e) => format!("adb: {} ({e})", a.program().display()),
            },
            Err(e) => format!("adb: {e}"),
        });
        for d in self.list_devices() {
            lines.push(format!(
                "{} · {} · {:?} · {:?} · game {}",
                d.id,
                d.model,
                d.connection,
                d.status,
                d.game_version.as_deref().unwrap_or("-")
            ));
        }
        lines.push(format!("Worlds folder: {}", self.settings().worlds_folder));
        lines.join("\n")
    }
}
