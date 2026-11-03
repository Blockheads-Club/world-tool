//! Synthesized saves, backups and a scripted phone for the bhw-core tests (no real game files).
#![allow(dead_code)]

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bhw_backup::{BackupOptions, BackupWriter, EntryKind, TarWriter};
use bhw_core::{Core, CoreConfig};
use bhw_device::adb::Timeouts;
use bhw_device::fake::{Reply, ScriptedRunner};
use bhw_device::Adb;
use bhw_lmdb::{Database, EnvContents, Layout};
use bhw_save::blocks::MacroBlock;
use bhw_save::plistio::{to_gnustep_xml, to_xml, Dialect};
use bhw_save::world::from_file_map;
use bhw_save::{FileMap, GameDb};
use bhw_world::WorldFiles;
use plist::{Dictionary, Value};

pub const A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
pub const B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
pub const C: &str = "cccccccccccccccccccccccccccccccc";
pub const D: &str = "dddddddddddddddddddddddddddddddd";
pub const PREFIX: &str = "apps/com.noodlecake.blockheads/f/Library/Application Support/";

fn date(s: &str) -> Value {
    Value::Date(plist::Date::from_xml_format(s).unwrap())
}

pub fn record(id: &str, name: &str, remote: bool) -> Dictionary {
    let mut d = Dictionary::new();
    d.insert("saveID".into(), Value::String(id.into()));
    d.insert("worldName".into(), Value::String(name.into()));
    d.insert("creationDate".into(), date("2026-10-20T08:48:09Z"));
    d.insert("saveDate".into(), date("2026-10-21T10:15:46Z"));
    d.insert("expertMode".into(), Value::Boolean(name.contains("Expert")));
    d.insert("worldWidthMacro".into(), Value::Integer(4.into()));
    d.insert("randomSeed".into(), Value::Integer(17.into()));
    d.insert("saveVersion".into(), Value::Integer(1100.into()));
    d.insert("remoteGame".into(), Value::Boolean(remote));
    d
}

pub fn client_record(id: &str, name: &str) -> Vec<u8> {
    to_xml(&Value::Dictionary(record(id, name, false)), Dialect::Apple).unwrap()
}

fn blocks() -> Database {
    let mut a = MacroBlock::empty(1, 1);
    for ly in 0..32 {
        for lx in 0..32 {
            a.tile_bytes_mut(lx, ly)[0] = if ly < 10 { 6 } else { 2 };
        }
    }
    Database::new(0).with(b"1_1".to_vec(), a.encode())
}

pub fn client_env(id: &str, name: &str, remote: bool, with_blocks: bool) -> EnvContents {
    let mut e = EnvContents::new(4096, 64 << 20);
    if with_blocks {
        e.named.insert(b"blocks".to_vec(), blocks());
    }
    let rec = to_xml(&Value::Dictionary(record(id, name, remote)), Dialect::Apple).unwrap();
    e.named.insert(b"main".to_vec(), Database::new(0).with(b"worldv2".to_vec(), rec));
    e
}

/// A game world (32-bit LMDB) in memory.
pub fn client_files(id: &str, name: &str) -> FileMap {
    let mut f = FileMap::new();
    f.insert(
        "world_db/data.mdb".into(),
        bhw_lmdb::write_env(&client_env(id, name, false, true), Layout::Bits32).unwrap(),
    );
    f.insert("map/default/1_1.png".into(), b"\x89PNG fake".to_vec());
    f
}

pub fn client_world(id: &str, name: &str) -> WorldFiles {
    from_file_map(id, client_files(id, name)).unwrap()
}

/// A Blockheads Server world (64-bit LMDB + server files).
pub fn server_files(id: &str, name: &str) -> FileMap {
    let mut r = record(id, name, false);
    r.insert("hostPort".into(), Value::String("15151".into()));
    let mut e = EnvContents::new(4096, 1 << 30);
    e.named.insert(b"blocks".to_vec(), blocks());
    e.named.insert(
        b"main".to_vec(),
        Database::new(0).with(b"worldv2".to_vec(), to_gnustep_xml(&Value::Dictionary(r.clone())).unwrap()),
    );
    let mut f = FileMap::new();
    f.insert("world_db/data.mdb".into(), bhw_lmdb::write_env(&e, Layout::Bits64).unwrap());
    let mut sdb = EnvContents::new(4096, 128 << 20);
    sdb.named.insert(b"main".to_vec(), Database::new(0).with(b"recentPlayers".to_vec(), b"y".to_vec()));
    f.insert("server_db/data.mdb".into(), bhw_lmdb::write_env(&sdb, Layout::Bits64).unwrap());
    f.insert("worldv2".into(), to_gnustep_xml(&Value::Dictionary(r)).unwrap());
    f.insert("adminlist.txt".into(), b"header\n".to_vec());
    f
}

pub fn game_db_bytes(worlds: &[(&str, &str)]) -> Vec<u8> {
    let mut g = GameDb::new();
    for (id, name) in worlds {
        g.register(id, &client_record(id, name));
    }
    g.to_bytes().unwrap()
}

/// What a phone has: worlds (id, name, remote?) and whether there is a `game_db`.
#[derive(Clone)]
pub struct PhoneData {
    pub worlds: Vec<(String, String, bool)>,
    pub game_db: bool,
    pub extra: Vec<(String, Vec<u8>)>,
}

impl PhoneData {
    pub fn new(worlds: &[(&str, &str)]) -> PhoneData {
        PhoneData {
            worlds: worlds.iter().map(|(i, n)| (i.to_string(), n.to_string(), false)).collect(),
            game_db: true,
            extra: Vec::new(),
        }
    }
}

/// An `adb backup` of the game, optionally encrypted.
pub fn phone_ab(p: &PhoneData, password: Option<&str>) -> Vec<u8> {
    let mut out = Vec::new();
    let opts = BackupOptions { version: 5, password: password.map(str::to_string), rounds: 10, ..Default::default() };
    let mut w = BackupWriter::create(Box::new(&mut out), &opts).unwrap();
    let mut t = TarWriter::new(&mut w);
    let mut add = |path: &str, data: Option<&[u8]>| {
        let (kind, d) = match data {
            Some(d) => (EntryKind::File, d),
            None => (EntryKind::Directory, &b""[..]),
        };
        let mut r: &[u8] = d;
        t.append(path, kind, 0o600, 0, d.len() as u64, &mut r).unwrap();
    };
    add("apps/com.noodlecake.blockheads/_manifest", Some(b"manifest"));
    add(&format!("{PREFIX}saves"), None);
    for (id, name, remote) in &p.worlds {
        let env = client_env(id, name, *remote, true);
        let db = bhw_lmdb::write_env(&env, Layout::Bits32).unwrap();
        add(&format!("{PREFIX}saves/{id}"), None);
        add(&format!("{PREFIX}saves/{id}/world_db"), None);
        add(&format!("{PREFIX}saves/{id}/world_db/data.mdb"), Some(&db));
    }
    for (path, data) in &p.extra {
        add(&format!("{PREFIX}{path}"), Some(data));
    }
    if p.game_db {
        let names: Vec<(&str, &str)> = p.worlds.iter().map(|(i, n, _)| (i.as_str(), n.as_str())).collect();
        add(&format!("{PREFIX}game_db"), None);
        add(&format!("{PREFIX}game_db/data.mdb"), Some(&game_db_bytes(&names)));
    }
    drop(add);
    t.finish().unwrap();
    w.finish().unwrap();
    out
}

/// A minimal valid APK of the game (synthetic manifest and images).
pub fn apk(version: &'static str) -> Vec<u8> {
    bhw_assets::apk::testutil::apk("com.noodlecake.blockheads", version, true)
}

/// A device shell reply (stdout + exit-code marker).
pub fn sh(stdout: &str, code: i32) -> Reply {
    Reply::ok(format!("{stdout}__BHW_RC={code}\n"))
}

pub const DEVICES: &str = "List of devices attached\n\
S1\tdevice usb:1-1 product:p model:Pixel_8 device:d transport_id:1\n\
10.0.0.2:5555\tdevice product:p model:Tab transport_id:2\n\
U1\tunauthorized usb:1-2 transport_id:3\n\
O1\toffline transport_id:4\n";

pub const DETAILS: &str = "model=Pixel_8\nrelease=14\nsdk=34\n    versionName=1.7.6\n    versionCode=1\n    \
User 0: installed=true stopped=false\n";

/// A scripted phone: `backup -f` writes the current `.ab`, `restore` replaces it.
pub struct Phone {
    pub runner: ScriptedRunner,
    pub ab: Arc<Mutex<Vec<u8>>>,
}

impl Phone {
    /// Custom rules first (they win), then the defaults.
    pub fn new(data: &PhoneData, custom: impl FnOnce(&ScriptedRunner)) -> Phone {
        let runner = ScriptedRunner::new();
        custom(&runner);
        let ab = Arc::new(Mutex::new(phone_ab(data, None)));
        let (w, r) = (ab.clone(), ab.clone());
        runner
            .on("devices -l", Reply::ok(DEVICES))
            .on("getprop", sh(DETAILS, 0))
            .on("pidof", sh("", 1))
            .on("input keyevent", sh("", 0))
            .on("am kill", sh("", 0))
            .on("am force-stop", sh("", 0))
            .on(
                "backup -f",
                Reply::ok("").on_spawn(move |args| {
                    let i = args.iter().position(|a| a == "-f").unwrap();
                    std::fs::write(&args[i + 1], &*w.lock().unwrap()).unwrap();
                }),
            )
            .on(
                " restore ",
                Reply::ok("").on_spawn(move |args| {
                    *r.lock().unwrap() = std::fs::read(args.last().unwrap()).unwrap();
                }),
            )
            .on("pm path", sh("package:/data/app/x/base.apk\n", 0))
            .on(
                " pull ",
                Reply::ok("1 file pulled").on_spawn(|args| std::fs::write(args.last().unwrap(), apk("1.7.6")).unwrap()),
            )
            .on(
                "mdns services",
                Reply::ok(
                    "List of discovered mdns services\nadb-S-x\t_adb-tls-pairing._tcp.\t10.0.0.2:37000\n\
                     adb-S-y\t_adb-tls-connect._tcp.\t10.0.0.2:41000\nfoo\t_other._tcp\t1.2.3.4:1\n",
                ),
            )
            .on("pair ", Reply::ok("Successfully paired to 10.0.0.2:37000"))
            .on("connect ", Reply::ok("connected to 10.0.0.2:41000"))
            .on("kill-server", Reply::ok(""))
            .on("start-server", Reply::ok(""))
            .on("version", Reply::ok("Android Debug Bridge version 1.0.41\nVersion 36\n"));
        Phone { runner, ab }
    }

    pub fn adb(&self) -> Adb {
        let mut a = Adb::new("adb", Arc::new(self.runner.clone()));
        a.timeouts = Timeouts {
            command: Duration::from_secs(5),
            confirm: Duration::from_secs(5),
            stall: Duration::from_secs(5),
        };
        a
    }

    /// World names on the phone now.
    pub fn names(&self) -> Vec<String> {
        let ab = self.ab.lock().unwrap().clone();
        let (_, saves) = bhw_world::backup::load_backup(Box::new(&ab[..]), None).unwrap();
        let mut n: Vec<String> =
            saves[0].worlds.iter().map(|w| bhw_core::worlds::inspect(w, None).unwrap().name).collect();
        n.sort();
        n
    }

    /// Ids registered in the phone's `game_db`.
    pub fn registered(&self) -> Vec<String> {
        let ab = self.ab.lock().unwrap().clone();
        let (_, saves) = bhw_world::backup::load_backup(Box::new(&ab[..]), None).unwrap();
        let f = saves[0].game_db.iter().find(|f| f.path == "data.mdb").unwrap();
        GameDb::from_bytes(&f.data.read_all().unwrap()).unwrap().world_ids()
    }
}

/// A Blockheads PC data folder with worlds.
pub fn pc_dir(root: &Path, worlds: &[(&str, &str)], game_db: bool) -> PathBuf {
    let data = root.join("pc");
    let support = data.join("home/Library/Application Support");
    for (id, name) in worlds {
        bhw_save::save::write_dir_files(&support.join("saves").join(id), &client_files(id, name)).unwrap();
    }
    std::fs::create_dir_all(support.join("saves")).unwrap();
    if game_db {
        std::fs::create_dir_all(support.join("game_db")).unwrap();
        std::fs::write(support.join("game_db/data.mdb"), game_db_bytes(worlds)).unwrap();
        std::fs::write(support.join("game_db/lock.mdb"), [0u8; 8]).unwrap();
    }
    data
}

/// A test environment: temp folders, a scripted phone, a local process runner.
pub struct Env {
    pub tmp: tempfile::TempDir,
    pub phone: Phone,
    pub local: ScriptedRunner,
    pub pc: Option<PathBuf>,
}

impl Env {
    pub fn new(data: &PhoneData) -> Env {
        Env::with(data, |_| {}, |_| {})
    }

    pub fn with(data: &PhoneData, custom: impl FnOnce(&ScriptedRunner), local: impl FnOnce(&ScriptedRunner)) -> Env {
        let tmp = tempfile::tempdir().unwrap();
        let l = ScriptedRunner::new();
        local(&l);
        l.on("comm=", Reply::ok("bash\ninit\n")).on("CSV", Reply::ok("\"x.exe\",\"1\"\n"));
        Env { tmp, phone: Phone::new(data, custom), local: l, pc: None }
    }

    pub fn path(&self, p: &str) -> PathBuf {
        self.tmp.path().join(p)
    }

    pub fn config(&self, adb: bool) -> CoreConfig {
        let pc = self.pc.clone();
        let ios = self.path("ios");
        CoreConfig {
            config_dir: self.path("config"),
            cache_dir: self.path("cache"),
            default_worlds_folder: self.path("Worlds"),
            adb: adb.then(|| self.phone.adb()),
            runner: Arc::new(self.local.clone()),
            pc_locator: Arc::new(move || pc.clone()),
            ios_locator: Arc::new(move || vec![ios.clone()]),
            android_dir: Some(self.path("android")),
            app_version: "0.1.0-test".into(),
        }
    }

    pub fn core(&self) -> Core {
        Core::new(self.config(true))
    }
}

/// Collects progress events.
pub fn events() -> (Arc<Mutex<Vec<bhw_core::Progress>>>, impl FnMut(bhw_core::Progress)) {
    let v = Arc::new(Mutex::new(Vec::new()));
    let w = v.clone();
    (v, move |p| w.lock().unwrap().push(p))
}

/// Writes `bytes` to `path` (creating parents).
pub fn write(path: &Path, bytes: &[u8]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut f = std::fs::File::create(path).unwrap();
    f.write_all(bytes).unwrap();
}
