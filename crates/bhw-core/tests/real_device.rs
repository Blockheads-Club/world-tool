//! End-to-end check against a real phone or emulator with The Blockheads installed. Skipped unless
//! `BHW_DEVICE_SERIAL` names a connected device:
//!
//! ```sh
//! BHW_DEVICE_SERIAL=emulator-5600 cargo test -p bhw-core --test real_device -- --nocapture
//! ```
//!
//! It reads the worlds, saves them to a temporary library, exports them, copies one back to the phone as a
//! "keep both" copy and a converted Blockheads Server export of the other one as well (each: fresh backup →
//! rewrite → restore), reads again to verify, then restores the automatic backup so the phone ends as it started. The confirm screens ("Back up my data" / "Restore my data") are tapped through
//! `adb shell input tap` on the `com.android.backupconfirm` allow button, so this needs an unlocked screen and
//! **changes the device's game data**: use a test device or emulator. `BHW_DEVICE_SCREENSHOT=<file.png>` also opens
//! the game after the copy and saves a screenshot (its world list should show the copy); `BHW_DEVICE_KEEP_COPY=1`
//! skips the final restore so the copies can be played. Every copy also checks the restore with one more backup, and
//! one copy is declined on purpose ("Do not restore") to see that reported. `BHW_DEVICE_ROOT=1` then repeats a read,
//! copy and restore through root (`adb root`; see `root_phase`).

use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use bhw_core::{Choice, Core, CoreConfig, ExportFormat, ExportRequest, ReadRequest, Source, TransferRequest};
use bhw_device::process::SystemRunner;
use bhw_device::Adb;

fn adb_out(serial: &str, args: &[&str]) -> String {
    let out = Command::new("adb").arg("-s").arg(serial).args(args).output().expect("adb");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// Bounds centre of the backupconfirm allow button from a UI dump, else the portrait 1080x2280 default.
fn allow_button(serial: &str) -> (u32, u32) {
    confirm_button(serial, "button_allow").unwrap_or((810, 2074))
}

/// Bounds centre of a backupconfirm button (`button_allow` / `button_deny`) from a UI dump.
fn confirm_button(serial: &str, id: &str) -> Option<(u32, u32)> {
    adb_out(serial, &["shell", "uiautomator", "dump", "/sdcard/bhw-ui.xml"]);
    let xml = adb_out(serial, &["shell", "cat", "/sdcard/bhw-ui.xml"]);
    let at = xml.find(&format!("com.android.backupconfirm:id/{id}")).and_then(|i| {
        let b = &xml[i..];
        let s = b.find("bounds=\"[")? + 9;
        let e = b[s..].find('"')? + s;
        let nums: Vec<u32> = b[s..e].split(|c: char| !c.is_ascii_digit()).filter_map(|x| x.parse().ok()).collect();
        (nums.len() == 4).then(|| ((nums[0] + nums[2]) / 2, (nums[1] + nums[3]) / 2))
    });
    at
}

/// Taps "Back up my data" / "Restore my data" whenever the confirm screen is in front, until dropped.
struct Tapper(Arc<AtomicBool>, Option<std::thread::JoinHandle<()>>);

impl Tapper {
    fn start(serial: &str) -> Tapper {
        Tapper::with_plan(serial, Vec::new())
    }

    /// Taps through confirm screens; `plan[i] == false` taps "Do not ..." on the i-th screen instead.
    fn with_plan(serial: &str, plan: Vec<bool>) -> Tapper {
        let stop = Arc::new(AtomicBool::new(false));
        let (s, serial) = (stop.clone(), serial.to_string());
        let t = std::thread::spawn(move || {
            let mut screen = 0;
            while !s.load(Ordering::SeqCst) {
                let focus = adb_out(&serial, &["shell", "dumpsys", "window"]);
                if focus.lines().any(|l| l.contains("mCurrentFocus") && l.contains("com.android.backupconfirm")) {
                    std::thread::sleep(Duration::from_millis(800));
                    let allow = plan.get(screen).copied().unwrap_or(true);
                    screen += 1;
                    let (x, y) = if allow {
                        allow_button(&serial)
                    } else {
                        confirm_button(&serial, "button_deny").expect("deny button")
                    };
                    eprintln!("tapping {} at {x},{y}", if allow { "allow" } else { "deny" });
                    adb_out(&serial, &["shell", "input", "tap", &x.to_string(), &y.to_string()]);
                    std::thread::sleep(Duration::from_secs(3));
                }
                std::thread::sleep(Duration::from_millis(700));
            }
        });
        Tapper(stop, Some(t))
    }
}

impl Drop for Tapper {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
        if let Some(t) = self.1.take() {
            let _ = t.join();
        }
    }
}

/// `adb backup` skips apps Android marks as stopped: launch the game once if needed.
fn ensure_launched(serial: &str) {
    let pkg = adb_out(serial, &["shell", "dumpsys", "package", "com.noodlecake.blockheads"]);
    if pkg.contains("stopped=true") {
        eprintln!("game is in the stopped state; launching it once");
        adb_out(
            serial,
            &["shell", "monkey", "-p", "com.noodlecake.blockheads", "-c", "android.intent.category.LAUNCHER", "1"],
        );
        std::thread::sleep(Duration::from_secs(12));
        adb_out(serial, &["shell", "input", "keyevent", "HOME"]);
        adb_out(serial, &["shell", "am", "kill", "com.noodlecake.blockheads"]);
    }
}

fn names(core: &Core, serial: &str) -> (String, Vec<String>) {
    let _tap = Tapper::start(serial);
    let req = ReadRequest { device_id: serial.into(), password: String::new() };
    let r = core.read("op-read", &req, &mut |p| eprintln!("read: {p:?}")).expect("read");
    let mut n: Vec<String> = r.worlds.iter().map(|w| w.name.clone()).collect();
    n.sort();
    (r.read_id, n)
}

fn adb() -> Adb {
    let mut adb = Adb::locate(None).expect("adb not found");
    adb.timeouts.confirm = Duration::from_secs(120);
    adb
}

/// A core on real adb with `root_mode` set.
fn core_in(dir: &std::path::Path, root_mode: &str) -> Core {
    let core = Core::new(CoreConfig {
        config_dir: dir.join("config"),
        cache_dir: dir.join("cache"),
        data_dir: dir.join("data"),
        default_worlds_folder: dir.join("Worlds"),
        adb: Some(adb()),
        make_adb: Arc::new(|p| Adb::new(p, Arc::new(SystemRunner))),
        fetch: Arc::new(bhw_core::adb_tools::NoFetch),
        runner: Arc::new(SystemRunner),
        pc_locator: Arc::new(|| None),
        ios_locator: Arc::new(Vec::new),
        android_dir: None,
        app_version: "test".into(),
    });
    let patch = serde_json::json!({ "root_mode": root_mode });
    core.set_settings(patch.as_object().unwrap()).unwrap();
    core
}

#[test]
fn read_copy_and_restore() {
    let Some(serial) = std::env::var("BHW_DEVICE_SERIAL").ok().filter(|s| !s.is_empty()) else {
        eprintln!("BHW_DEVICE_SERIAL not set; skipping");
        return;
    };
    ensure_launched(&serial);
    let tmp = tempfile::tempdir().unwrap();
    let core = core_in(tmp.path(), "never");

    let dev = core.list_devices().into_iter().find(|d| d.id == serial).expect("device listed");
    eprintln!("device: {dev:?}");

    // 1. Read.
    let (read_id, before) = names(&core, &serial);
    eprintln!("worlds on the device: {before:?}");
    assert!(before.contains(&"ROCKY OCEAN".to_string()), "ROCKY OCEAN missing: {before:?}");

    // 2. Library + exports.
    let ids: Vec<String> =
        core.source_worlds(&Source::Device { device_id: serial.clone() }).unwrap().into_iter().map(|w| w.id).collect();
    let saved = core.save_to_library(&read_id, &ids).unwrap();
    assert_eq!(saved.len(), before.len());
    for format in [ExportFormat::Bhworld, ExportFormat::Server] {
        let req = ExportRequest {
            read_id: None,
            world_ids: saved.iter().map(|w| w.base.id.clone()).collect(),
            format,
            folder: tmp.path().join(format!("export-{format:?}")).display().to_string(),
        };
        let r = core.export_worlds("op-export", &req, &mut |_| {}).unwrap();
        eprintln!("exported {format:?}: {:?}", r.files);
        assert_eq!(r.files.len(), saved.len());
    }

    // 3a. "Do not restore" on the phone: the check backup finds no copy and says so.
    let rocky = saved.iter().find(|w| w.base.name == "ROCKY OCEAN").unwrap();
    {
        let plan = core.transfer_plan(&Source::Library, std::slice::from_ref(&rocky.base.id), &serial).unwrap();
        let req = TransferRequest { plan_id: plan.plan_id, choices: Default::default() };
        let _tap = Tapper::with_plan(&serial, vec![true, false]);
        let e = core.transfer("op-declined", &req, &mut |p| eprintln!("declined copy: {p:?}")).unwrap_err();
        eprintln!("declined restore: {e:?}");
        assert_eq!(e.code, bhw_core::ErrorCode::RestoreDeclined);
    }
    let (_, unchanged) = names(&core, &serial);
    assert_eq!(unchanged, before);

    // 3. Copy ROCKY OCEAN back as a second copy.
    let plan = core.transfer_plan(&Source::Library, std::slice::from_ref(&rocky.base.id), &serial).unwrap();
    eprintln!("plan: {plan:?}");
    let copy_name = plan.items.iter().find(|i| i.key == rocky.base.id).unwrap().copy_name.clone().unwrap();
    let req = TransferRequest {
        plan_id: plan.plan_id.clone(),
        choices: [(rocky.base.id.clone(), Choice::KeepBoth)].into_iter().collect(),
    };
    let res = {
        let _tap = Tapper::start(&serial);
        core.transfer("op-copy", &req, &mut |p| eprintln!("copy: {p:?}")).expect("transfer")
    };
    eprintln!("transfer: {res:?}");
    assert_eq!(res.kept_both, vec![copy_name.clone()]);
    let backup_id = res.backup_id.clone().expect("automatic backup");

    // 3b. The Blockheads Server folder exported from the other world goes back too (server → game conversion; same
    // world id, so it becomes a "keep both" copy as well).
    let other = saved.iter().find(|w| w.base.name != "ROCKY OCEAN").unwrap();
    let folder = tmp.path().join("export-Server").join(&other.world_id).display().to_string();
    let src = Source::Files { paths: vec![folder.clone()] };
    let plan = core.transfer_plan(&src, std::slice::from_ref(&folder), &serial).unwrap();
    let server_copy = plan.items.iter().find(|i| i.key == folder).unwrap().copy_name.clone().unwrap();
    let req = TransferRequest { plan_id: plan.plan_id, choices: Default::default() };
    let res = {
        let _tap = Tapper::start(&serial);
        core.transfer("op-copy2", &req, &mut |_| {}).expect("transfer of the server folder")
    };
    eprintln!("transfer (server folder): {res:?}");
    assert_eq!(res.kept_both, vec![server_copy.clone()]);

    // Optional: open the game and save a screenshot of its world list (`BHW_DEVICE_SCREENSHOT=<file.png>`).
    if let Ok(shot) = std::env::var("BHW_DEVICE_SCREENSHOT") {
        adb_out(
            &serial,
            &["shell", "monkey", "-p", "com.noodlecake.blockheads", "-c", "android.intent.category.LAUNCHER", "1"],
        );
        std::thread::sleep(Duration::from_secs(30));
        let png = Command::new("adb").args(["-s", &serial, "exec-out", "screencap", "-p"]).output().unwrap().stdout;
        std::fs::write(&shot, png).unwrap();
        adb_out(&serial, &["shell", "input", "keyevent", "HOME"]);
        std::thread::sleep(Duration::from_secs(2));
    }

    // 4. Read again: the copy is there.
    let (_, after) = names(&core, &serial);
    eprintln!("after copy: {after:?}");
    assert!(after.contains(&copy_name), "{copy_name} missing after the copy: {after:?}");
    assert!(after.contains(&server_copy), "{server_copy} missing after the copy: {after:?}");
    assert_eq!(after.len(), before.len() + 2);

    // 5. Restore the automatic backup: back to the start (`BHW_DEVICE_KEEP_COPY=1` leaves the copy on the device to
    // try it in the game).
    if std::env::var("BHW_DEVICE_KEEP_COPY").is_ok() {
        return;
    }
    {
        let _tap = Tapper::start(&serial);
        core.restore_backup("op-restore", &backup_id, &serial, &mut |p| eprintln!("restore: {p:?}")).expect("restore");
    }
    let (_, restored) = names(&core, &serial);
    eprintln!("after restore: {restored:?}");
    assert_eq!(restored, before);
    for a in core.list_activity() {
        eprintln!("activity: {:?} {:?} {:?} {:?}", a.kind, a.result, a.worlds, a.error);
    }
    if std::env::var("BHW_DEVICE_ROOT").is_ok() {
        root_phase(&serial, &before);
    }
}

const SUPPORT: &str = "/data/data/com.noodlecake.blockheads/files/Library/Application Support";

/// `BHW_DEVICE_ROOT=1`: the same through root (`adb root`, so an emulator or userdebug build): read, copy, check the
/// files belong to the app again, open the game, restore. No confirm screen may appear (no tapper runs).
fn root_phase(serial: &str, before: &[String]) {
    let root = Command::new("adb").args(["-s", serial, "root"]).output().unwrap();
    eprintln!("adb root: {}", String::from_utf8_lossy(&root.stdout).trim());
    std::thread::sleep(Duration::from_secs(3));
    Command::new("adb").args(["-s", serial, "wait-for-device"]).status().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let core = core_in(tmp.path(), "auto");
    let stages = Arc::new(std::sync::Mutex::new(Vec::new()));
    let log = |what: &'static str| {
        let st = stages.clone();
        move |p: bhw_core::Progress| {
            eprintln!("{what}: {p:?}");
            st.lock().unwrap().push(p.stage);
        }
    };
    let req = ReadRequest { device_id: serial.into(), password: String::new() };
    let r = core.read("root-read", &req, &mut log("root read")).expect("root read");
    let mut got: Vec<String> = r.worlds.iter().map(|w| w.name.clone()).collect();
    got.sort();
    assert_eq!(got, before);
    assert!(core.list_devices().iter().any(|d| d.id == serial && d.root), "root not detected");
    let rocky = r.worlds.iter().find(|w| w.name == "ROCKY OCEAN").unwrap().id.clone();
    let src = Source::Device { device_id: serial.into() };
    let plan = core.transfer_plan(&src, std::slice::from_ref(&rocky), serial).unwrap();
    assert!(!plan.needs_phone);
    let req = TransferRequest { plan_id: plan.plan_id, choices: Default::default() };
    let res = core.transfer("root-copy", &req, &mut log("root copy")).expect("root copy");
    eprintln!("root transfer: {res:?}");
    let backup_id = res.backup_id.clone().expect("automatic backup");
    let st = stages.lock().unwrap().clone();
    assert!(!st.contains(&bhw_core::Stage::PhoneBackup) && !st.contains(&bhw_core::Stage::PhoneRestore));

    // Owner and SELinux context of everything below the support dir equal the support dir's.
    let ctx = adb_out(serial, &["shell", &format!("ls -Zd '{SUPPORT}'")]);
    let ctx = ctx.split_whitespace().next().unwrap().to_string();
    let listing = adb_out(serial, &["shell", &format!("ls -lnZR '{SUPPORT}/saves' '{SUPPORT}/game_db'")]);
    let uid = adb_out(serial, &["shell", &format!("stat -c %u '{SUPPORT}'")]).trim().to_string();
    let entries: Vec<&str> = listing.lines().filter(|l| l.starts_with('-') || l.starts_with('d')).collect();
    assert!(!entries.is_empty());
    for l in &entries {
        let f: Vec<&str> = l.split_whitespace().collect();
        assert_eq!((f[2], f[3], f[4]), (uid.as_str(), uid.as_str(), ctx.as_str()), "{l}");
    }
    eprintln!("{} entries owned by {uid} with {ctx}", entries.len());
    let (_, after) = names(&core, serial);
    assert_eq!(after.len(), before.len() + 1, "{after:?}");

    // The game starts and stays up with the written files.
    adb_out(
        serial,
        &["shell", "monkey", "-p", "com.noodlecake.blockheads", "-c", "android.intent.category.LAUNCHER", "1"],
    );
    std::thread::sleep(Duration::from_secs(20));
    assert!(!adb_out(serial, &["shell", "pidof", "com.noodlecake.blockheads"]).trim().is_empty(), "game died");
    if let Ok(shot) = std::env::var("BHW_DEVICE_SCREENSHOT") {
        let png = Command::new("adb").args(["-s", serial, "exec-out", "screencap", "-p"]).output().unwrap().stdout;
        std::fs::write(shot.replace(".png", "-root.png"), png).unwrap();
    }
    adb_out(serial, &["shell", "input", "keyevent", "HOME"]);
    std::thread::sleep(Duration::from_secs(2));

    core.restore_backup("root-restore", &backup_id, serial, &mut log("root restore")).expect("root restore");
    let (_, restored) = names(&core, serial);
    assert_eq!(restored, before);
    let unroot = Command::new("adb").args(["-s", serial, "unroot"]).output().unwrap();
    eprintln!("adb unroot: {}", String::from_utf8_lossy(&unroot.stdout).trim());
    std::thread::sleep(Duration::from_secs(3));
    Command::new("adb").args(["-s", serial, "wait-for-device"]).status().unwrap();
}
