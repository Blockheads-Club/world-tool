//! Real-phone checks. Skipped unless `BHW_DEVICE_SERIAL` names a connected device:
//!
//! ```sh
//! BHW_DEVICE_SERIAL=R58M123 cargo test -p bhw-device --test real_device -- --nocapture
//! ```
//!
//! The backup test needs the user to confirm "Back up my data" on the phone (leave the password empty, or the
//! file is encrypted). It only reads from the phone. `BHW_DEVICE_ROOT=1` also lists the saves dir as root (read-only).

use bhw_device::{AbMethod, AbOptions, Adb, CancelToken, IfRunning};
use std::time::Duration;

fn setup() -> Option<(Adb, String)> {
    let serial = std::env::var("BHW_DEVICE_SERIAL").ok().filter(|s| !s.is_empty())?;
    let mut adb = Adb::locate(None).expect("adb not found");
    adb.timeouts.confirm = Duration::from_secs(120);
    Some((adb, serial))
}

#[test]
fn list_details_and_backup() {
    let Some((adb, serial)) = setup() else {
        eprintln!("BHW_DEVICE_SERIAL not set; skipping");
        return;
    };
    eprintln!("adb: {} ({})", adb.version().unwrap(), adb.program().display());
    let devices = adb.discover(false).unwrap();
    for d in &devices {
        eprintln!("{d:#?}");
    }
    let dev = devices.iter().find(|d| d.listed.serial == serial).expect("serial not listed");
    let game = dev.details.as_ref().and_then(|d| d.game.clone()).expect("game not installed");
    eprintln!("game {} supported={}", game.version_name, game.supported());
    eprintln!("game running: {}", adb.game_running(&serial).unwrap());
    let method = match std::env::var("BHW_DEVICE_METHOD").as_deref() {
        Ok("bu") => AbMethod::BuStream,
        _ => AbMethod::AdbCommand,
    };
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("blockheads.ab");
    eprintln!("confirm the backup on the phone now ({method:?})");
    let opts = AbOptions { method, if_running: IfRunning::Ignore };
    let mut last = 0;
    let r = adb.backup_to_file(&serial, &path, opts, &mut |p| last = p.bytes, &CancelToken::new());
    eprintln!("backup result: {r:?}; last progress {last} bytes");
    let summary = r.unwrap();
    eprintln!("backup size: {} bytes, encrypted: {}", summary.bytes, summary.encrypted);
    assert_eq!(std::fs::metadata(&path).unwrap().len(), summary.bytes);
    if let Ok(keep) = std::env::var("BHW_DEVICE_KEEP") {
        std::fs::copy(&path, keep).unwrap();
    }
}

#[test]
fn root_lists_saves() {
    let Some((adb, serial)) = setup() else { return };
    if std::env::var("BHW_DEVICE_ROOT").is_err() {
        return;
    }
    let root = adb.probe_root(&serial).unwrap().expect("no root");
    let saves = adb.root_find_saves(&serial, root).unwrap();
    eprintln!("{saves:#?}");
}
