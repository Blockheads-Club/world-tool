//! Device discovery: `adb devices -l`, device properties, the installed game, root, and polling for changes.

use crate::adb::{Adb, RootStyle, PACKAGE};
use crate::error::{DeviceError, Result};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// adb's state for a device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceState {
    /// Ready.
    Device,
    /// Waiting for "Allow USB debugging?" on the phone.
    Unauthorized,
    /// Connected but not answering.
    Offline,
    /// adb may not open the USB device (Linux udev rules / Windows driver); adb's message.
    NoPermissions(String),
    /// Anything else (`recovery`, `sideload`, `bootloader`, `authorizing`, `connecting`, ...).
    Other(String),
}

impl DeviceState {
    fn parse(s: &str) -> DeviceState {
        match s {
            "device" => DeviceState::Device,
            "unauthorized" => DeviceState::Unauthorized,
            "offline" => DeviceState::Offline,
            s if s.starts_with("no permissions") => DeviceState::NoPermissions(s.to_string()),
            s => DeviceState::Other(s.to_string()),
        }
    }
}

/// How the device is attached.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Connection {
    Usb,
    Wifi,
    Emulator,
}

impl Connection {
    /// Guesses from the serial: `host:port` and mDNS names are Wi-Fi, `emulator-N` the emulator.
    pub fn from_serial(serial: &str) -> Connection {
        if serial.starts_with("emulator-") {
            Connection::Emulator
        } else if serial.contains("._adb-tls-connect._tcp")
            || serial.rsplit_once(':').is_some_and(|(h, p)| !h.is_empty() && p.parse::<u16>().is_ok())
        {
            Connection::Wifi
        } else {
            Connection::Usb
        }
    }
}

/// One line of `adb devices -l`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListedDevice {
    pub serial: String,
    pub state: DeviceState,
    pub connection: Connection,
    /// `model:` (underscores as adb prints them).
    pub model: Option<String>,
    pub product: Option<String>,
    /// `usb:` bus path (USB only).
    pub usb: Option<String>,
    pub transport_id: Option<String>,
}

const KEYS: [&str; 5] = ["usb", "product", "model", "device", "transport_id"];

/// Parses `adb devices -l`.
pub fn parse_devices(text: &str) -> Vec<ListedDevice> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("List of devices") || line.starts_with('*') {
            continue;
        }
        let Some((serial, rest)) = line.split_once(char::is_whitespace) else { continue };
        let mut props = BTreeMap::new();
        let mut state_words = Vec::new();
        for tok in rest.split_whitespace() {
            match tok.split_once(':') {
                Some((k, v)) if KEYS.contains(&k) => {
                    props.insert(k, v.to_string());
                }
                _ if props.is_empty() => state_words.push(tok),
                _ => {}
            }
        }
        out.push(ListedDevice {
            serial: serial.to_string(),
            state: DeviceState::parse(&state_words.join(" ")),
            connection: Connection::from_serial(serial),
            model: props.get("model").cloned(),
            product: props.get("product").cloned(),
            usb: props.get("usb").cloned(),
            transport_id: props.get("transport_id").cloned(),
        });
    }
    out
}

/// The installed game.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameInfo {
    pub version_name: String,
    pub version_code: Option<u32>,
    /// The app's Linux uid (owner of its data).
    pub uid: Option<u32>,
    /// `/data/user/0/com.noodlecake.blockheads` or similar.
    pub data_dir: Option<String>,
    /// Android's "stopped" flag for user 0 (`stopped=true` in `dumpsys package`): set by `am force-stop` or
    /// when the app was never launched since install. `adb backup` skips stopped apps.
    pub stopped: bool,
}

impl GameInfo {
    /// Whether this version's saves are supported (1.7.4 to 1.7.6).
    pub fn supported(&self) -> bool {
        version_supported(&self.version_name)
    }

    /// `Ok` for a supported version, else [`DeviceError::UnsupportedVersion`].
    pub fn check_supported(&self) -> Result<()> {
        if self.supported() {
            Ok(())
        } else {
            Err(DeviceError::UnsupportedVersion { found: self.version_name.clone() })
        }
    }
}

/// 1.7.4, 1.7.5 and 1.7.6 (any suffix after the third number is ignored).
pub fn version_supported(name: &str) -> bool {
    let nums: Vec<u32> = name
        .split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .take(3)
        .map(|s| s.parse().unwrap_or(u32::MAX))
        .collect();
    matches!(nums.as_slice(), [1, 7, 4..=6])
}

/// Everything known about a ready device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceDetails {
    /// `ro.product.model`.
    pub model: String,
    /// `ro.build.version.release`, e.g. "14".
    pub android_release: String,
    /// `ro.build.version.sdk`, e.g. 34.
    pub sdk: Option<u32>,
    /// `None` when the game is not installed.
    pub game: Option<GameInfo>,
}

/// A device with whatever could be found out about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    pub listed: ListedDevice,
    /// Only for devices in the `device` state.
    pub details: Option<DeviceDetails>,
    /// Only when root was probed: `Some(None)` = probed, no root.
    pub root: Option<Option<RootStyle>>,
}

/// Parses `key=value` tokens from command output; the first occurrence of each key wins.
pub fn parse_key_values(text: &str) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    for tok in text.split_whitespace() {
        if let Some((k, v)) = tok.split_once('=') {
            map.entry(k.to_string()).or_insert_with(|| v.to_string());
        }
    }
    map
}

/// Pause between two `am kill` tries of [`Adb::stop_game`].
pub const STOP_PAUSE: std::time::Duration = std::time::Duration::from_millis(500);

impl Adb {
    /// `adb devices -l`.
    pub fn list_devices(&self) -> Result<Vec<ListedDevice>> {
        let out = self.run(&["devices", "-l"])?;
        if out.code != 0 {
            return Err(DeviceError::Command { code: out.code, message: out.stderr.trim().to_string() });
        }
        Ok(parse_devices(&out.stdout))
    }

    /// Model, Android version and the installed game, in one shell round trip.
    pub fn device_details(&self, serial: &str) -> Result<DeviceDetails> {
        let cmd = format!(
            "echo \"model=$(getprop ro.product.model | tr ' ' _)\"; \
             echo \"release=$(getprop ro.build.version.release)\"; \
             echo \"sdk=$(getprop ro.build.version.sdk)\"; \
             dumpsys package {PACKAGE} | grep -E '(versionName|versionCode|userId|dataDir|stopped)='"
        );
        let out = self.shell(serial, &cmd)?;
        let kv = parse_key_values(&out.stdout);
        let get = |k: &str| kv.get(k).cloned().unwrap_or_default();
        let game = kv.get("versionName").map(|v| GameInfo {
            version_name: v.clone(),
            version_code: kv.get("versionCode").and_then(|c| c.parse().ok()),
            uid: kv.get("userId").and_then(|c| c.parse().ok()),
            data_dir: kv.get("dataDir").cloned(),
            stopped: kv.get("stopped").is_some_and(|s| s == "true"),
        });
        Ok(DeviceDetails {
            model: get("model").replace('_', " "),
            android_release: get("release"),
            sdk: get("sdk").parse().ok(),
            game,
        })
    }

    /// The installed game, or [`DeviceError::GameNotInstalled`].
    pub fn game_info(&self, serial: &str) -> Result<GameInfo> {
        self.device_details(serial)?.game.ok_or(DeviceError::GameNotInstalled)
    }

    /// Whether the game's process is alive (`pidof`).
    pub fn game_running(&self, serial: &str) -> Result<bool> {
        let out = self.shell(serial, &format!("pidof {PACKAGE}"))?;
        Ok(out.code == 0 && !out.stdout.trim().is_empty())
    }

    /// Stops the game without setting Android's "stopped" flag (which would make `adb backup` skip it): sends it
    /// to the background (`input keyevent HOME`), then `am kill` (only kills background processes). Fails with
    /// [`DeviceError::GameRunning`] when it is still alive afterwards.
    ///
    /// `am kill` only works once the process has left the "previous app" state, so it is retried every
    /// [`STOP_PAUSE`] for up to [`Timeouts::stop`](crate::Timeouts::stop).
    pub fn stop_game(&self, serial: &str) -> Result<()> {
        self.shell_ok(serial, "input keyevent HOME")?;
        let deadline = std::time::Instant::now() + self.timeouts.stop;
        loop {
            self.shell_ok(serial, &format!("am kill {PACKAGE}"))?;
            if !self.game_running(serial)? {
                return Ok(());
            }
            if std::time::Instant::now() >= deadline {
                return Err(DeviceError::GameRunning);
            }
            std::thread::sleep(STOP_PAUSE);
        }
    }

    /// `am force-stop` the game. Sets the "stopped" flag, so only use it before a restore (never before a backup).
    pub fn force_stop_game(&self, serial: &str) -> Result<()> {
        self.shell_ok(serial, &format!("am force-stop {PACKAGE}")).map(drop)
    }

    /// Which way to become root, if any: `adb root` (shell is uid 0), `su -c`, `su 0`. A root manager may show a
    /// grant dialog on the phone, so this waits up to the confirm timeout for each try.
    pub fn probe_root(&self, serial: &str) -> Result<Option<RootStyle>> {
        for style in [RootStyle::AdbRoot, RootStyle::SuDashC, RootStyle::SuUid] {
            let wait = if style == RootStyle::AdbRoot { self.timeouts.command } else { self.timeouts.confirm };
            match self.shell_timeout(serial, &style.wrap("id"), wait) {
                Ok(out) if out.code == 0 && out.stdout.contains("uid=0(") => return Ok(Some(style)),
                Ok(_) | Err(DeviceError::Timeout) => {}
                Err(e) => return Err(e),
            }
        }
        Ok(None)
    }

    /// Lists devices and fills in details for ready ones; `check_root` also probes root (may prompt on the phone).
    pub fn discover(&self, check_root: bool) -> Result<Vec<Device>> {
        self.list_devices()?.into_iter().map(|listed| self.describe(listed, check_root)).collect()
    }

    /// Details (and optionally root) for one listed device. Devices that vanish meanwhile keep `details: None`.
    pub fn describe(&self, listed: ListedDevice, check_root: bool) -> Result<Device> {
        let mut dev = Device { listed, details: None, root: None };
        if dev.listed.state != DeviceState::Device {
            return Ok(dev);
        }
        match self.device_details(&dev.listed.serial) {
            Ok(d) => dev.details = Some(d),
            Err(DeviceError::Command { .. } | DeviceError::DeviceNotFound { .. } | DeviceError::Offline { .. }) => {
                return Ok(dev)
            }
            Err(e) => return Err(e),
        }
        if check_root {
            dev.root = Some(self.probe_root(&dev.listed.serial)?);
        }
        Ok(dev)
    }
}

/// A change seen by [`DeviceWatcher::poll`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceEvent {
    Added(Device),
    /// State or details changed.
    Changed(Device),
    Removed(String),
}

/// Polls `adb devices -l` and reports what changed since the previous poll.
#[derive(Debug)]
pub struct DeviceWatcher {
    adb: Adb,
    known: BTreeMap<String, Device>,
}

impl DeviceWatcher {
    /// Nothing known yet: the first poll reports every device as added.
    pub fn new(adb: Adb) -> DeviceWatcher {
        DeviceWatcher { adb, known: BTreeMap::new() }
    }

    /// Devices as of the last poll.
    pub fn devices(&self) -> Vec<Device> {
        self.known.values().cloned().collect()
    }

    /// Lists devices; describes new ones and ones whose listing changed.
    pub fn poll(&mut self) -> Result<Vec<DeviceEvent>> {
        let listed = self.adb.list_devices()?;
        let mut events = Vec::new();
        let mut next = BTreeMap::new();
        for l in listed {
            let serial = l.serial.clone();
            let dev = match self.known.remove(&serial) {
                Some(old) if old.listed == l => old,
                Some(_) => {
                    let d = self.adb.describe(l, false)?;
                    events.push(DeviceEvent::Changed(d.clone()));
                    d
                }
                None => {
                    let d = self.adb.describe(l, false)?;
                    events.push(DeviceEvent::Added(d.clone()));
                    d
                }
            };
            next.insert(serial, dev);
        }
        events.extend(std::mem::take(&mut self.known).into_keys().map(DeviceEvent::Removed));
        self.known = next;
        Ok(events)
    }

    /// Polls every `interval` on a thread, calling `on_events` with non-empty batches (and errors); stops when
    /// the handle is dropped or stopped.
    pub fn spawn(
        mut self,
        interval: Duration,
        on_events: impl Fn(Result<Vec<DeviceEvent>>) + Send + 'static,
    ) -> WatchHandle {
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let thread = std::thread::spawn(move || {
            while !flag.load(Ordering::SeqCst) {
                match self.poll() {
                    Ok(ev) if ev.is_empty() => {}
                    r => on_events(r),
                }
                let mut slept = Duration::ZERO;
                while slept < interval && !flag.load(Ordering::SeqCst) {
                    std::thread::sleep(crate::process::TICK);
                    slept += crate::process::TICK;
                }
            }
        });
        WatchHandle { stop, thread: Some(thread) }
    }
}

/// Stops a [`DeviceWatcher::spawn`] thread on drop.
#[derive(Debug)]
pub struct WatchHandle {
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl WatchHandle {
    /// Stops and joins the thread.
    pub fn stop(self) {}
}

impl Drop for WatchHandle {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}
