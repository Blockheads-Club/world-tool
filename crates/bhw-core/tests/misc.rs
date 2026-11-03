//! Game files, iPhone backups, Wi-Fi, settings, activity, errors and helpers.

mod common;

use std::path::Path;
use std::sync::Arc;

use bhw_core::activity::{ActivityLog, Entry};
use bhw_core::util::{child, file_name, mtime_ms, unique_id, Ops, TempPath};
use bhw_core::{
    ActivityKind, ActivityResult, AppError, DeviceKind, ErrorCode, GameFilesState, ReadRequest, Settings, Source,
    Stage, TransferRequest,
};
use bhw_device::fake::Reply;
use bhw_device::DeviceError;
use bhw_ios::fixture::{build, Enc};
use bhw_lmdb::Layout;
use common::*;

const DOM: &str = "AppDomain-com.majicjungle.blockheads";

fn ios_backup(dir: &Path, enc: Enc) {
    let support = "Library/Application Support/";
    let mut db64 = client_env(A, "iPhone World", false, true);
    db64.map_size = 1 << 30;
    let world = bhw_lmdb::write_env(&db64, Layout::Bits64).unwrap();
    let gdb = game_db_bytes(&[(A, "iPhone World")]);
    let remote = bhw_lmdb::write_env(&client_env(B, "Remote", true, true), Layout::Bits64).unwrap();
    let rows: Vec<(String, String, i64, Vec<u8>)> = vec![
        (DOM.into(), format!("{support}saves/{A}/world_db/data.mdb"), 1, world),
        (DOM.into(), format!("{support}saves/{B}/world_db/data.mdb"), 1, remote),
        (DOM.into(), format!("{support}game_db/data.mdb"), 1, gdb),
    ];
    let r: Vec<bhw_ios::fixture::Row> =
        rows.iter().map(|(a, b, c, d)| (a.as_str(), b.as_str(), *c, d.as_slice())).collect();
    build(dir, enc, &r);
}

fn ready(core: &bhw_core::Core) -> bhw_core::GameFiles {
    core.game_files()
}

#[test]
fn game_files_states() {
    let env = Env::new(&PhoneData::new(&[]));
    let core = env.core();
    assert_eq!(ready(&core).state, GameFilesState::NotSet);
    assert_eq!(core.reextract("op", &mut |_| {}).unwrap_err().code, ErrorCode::ApkMissing);
    let apk_path = env.path("dl/TheBlockheads.apk");
    write(&apk_path, &apk("1.7.6"));
    let (ev, mut on) = events();
    let g = core.use_apk("op", &apk_path.display().to_string(), &mut on).unwrap();
    assert_eq!(
        (g.state, g.version.as_deref(), g.apk_name.as_deref()),
        (GameFilesState::Ready, Some("1.7.6"), Some("TheBlockheads.apk"))
    );
    assert!(g.images.unwrap() > 0 && g.size.unwrap() > 0);
    let stages: Vec<Stage> = ev.lock().unwrap().iter().map(|p| p.stage).collect();
    assert_eq!((stages[0], *stages.last().unwrap()), (Stage::ApkChecking, Stage::ApkExtracting));
    assert_eq!(core.reextract("op", &mut |_| {}).unwrap().state, GameFilesState::Ready);

    // Damaged (a file gone, source there), missing (source gone too), unsupported (old manifest).
    let root = env.path("cache/game-files");
    let cur = root.join(std::fs::read_to_string(root.join("current")).unwrap().trim());
    let tile = cur.join("HDTex/TileMap.png");
    std::fs::remove_file(&tile).unwrap();
    assert_eq!(ready(&core).state, GameFilesState::Damaged);
    std::fs::rename(&apk_path, env.path("dl/moved.apk")).unwrap();
    let g = ready(&core);
    assert_eq!((g.state, g.version.as_deref()), (GameFilesState::Missing, Some("1.7.6")));
    assert_eq!(core.reextract("op", &mut |_| {}).unwrap_err().code, ErrorCode::ApkMissing);
    let m = cur.join("manifest.json");
    let text = std::fs::read_to_string(&m).unwrap().replace("\"1.7.6\"", "\"1.6.0\"");
    std::fs::write(&m, text).unwrap();
    assert_eq!(ready(&core).state, GameFilesState::Unsupported);
    assert_eq!(core.clear_game_files().unwrap().state, GameFilesState::NotSet);

    // Wrong app, unsupported version, junk.
    for (name, bytes, code) in [
        ("other.apk", bhw_assets::apk::testutil::apk("com.example.other", "1.0", true), ErrorCode::ApkWrongApp),
        ("old.apk", apk("1.6.0"), ErrorCode::ApkUnsupported),
        ("junk.apk", b"junk".to_vec(), ErrorCode::ApkDamaged),
    ] {
        let p = env.path(&format!("dl/{name}"));
        write(&p, &bytes);
        let e = core.use_apk("op", &p.display().to_string(), &mut |_| {}).unwrap_err();
        assert_eq!((e.code, e.extra.name.as_deref()), (code, Some(name)));
    }
    // A broken pointer file.
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("current"), "../x").unwrap();
    assert_eq!(ready(&core).state, GameFilesState::Damaged);
}

#[test]
fn apk_from_the_phone() {
    let mut env = Env::new(&PhoneData::new(&[]));
    env.pc = Some(pc_dir(env.tmp.path(), &[], false));
    let core = env.core();
    let (ev, mut on) = events();
    let g = core.apk_from_device("op", "S1", &mut on).unwrap();
    assert_eq!(g.state, GameFilesState::Ready);
    assert!(ev.lock().unwrap().iter().any(|p| p.stage == Stage::ApkPulling));
    assert_eq!(core.apk_from_device("op", "pc", &mut |_| {}).unwrap_err().code, ErrorCode::NotFound);

    // Split APKs are pulled into a folder; failures.
    let env = Env::with(
        &PhoneData::new(&[]),
        |r| {
            r.once(
                "pm path",
                sh("package:/data/app/x/base.apk\npackage:/data/app/x/split_config.armeabi_v7a.apk\n", 0),
            );
            r.once(" pull ", Reply::ok("").on_spawn(|a| std::fs::write(a.last().unwrap(), apk("1.7.6")).unwrap()));
            r.once(
                " pull ",
                Reply::ok("").on_spawn(|a| {
                    std::fs::write(a.last().unwrap(), bhw_assets::apk::testutil::split("config.armeabi_v7a")).unwrap()
                }),
            );
            r.once("pm path", sh("", 0));
            r.once(" pull ", Reply::fail(1, "adb: error: failed to stat"));
        },
        |_| {},
    );
    let core = env.core();
    let g = core.apk_from_device("op", "S1", &mut |_| {}).unwrap();
    assert_eq!(g.apk_name.as_deref(), Some("S1"));
    let e = core.apk_from_device("op", "S1", &mut |_| {}).unwrap_err();
    assert_eq!(e.code, ErrorCode::GameNotInstalled);
    let e = core.apk_from_device("op", "S1", &mut |_| {}).unwrap_err();
    assert_eq!((e.code, e.extra.device.as_deref()), (ErrorCode::Io, Some("Pixel 8")));
}

#[test]
fn iphone_backups() {
    let env = Env::new(&PhoneData::new(&[]));
    let core = env.core();
    assert!(core.ios_backups(None).unwrap().is_empty());
    let plain = env.path("ios/udid-plain");
    ios_backup(&plain, Enc::None);
    ios_backup(&env.path("ios/udid-locked"), Enc::Modern("pw"));
    std::fs::create_dir_all(env.path("ios/not-a-backup")).unwrap();
    let list = core.ios_backups(None).unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!((list[0].name.as_str(), list[0].model.as_str()), ("Test iPhone", "iPhone · iOS 17.0"));
    let one = core.ios_backups(Some(&plain.display().to_string())).unwrap();
    assert_eq!((one.len(), one[0].encrypted), (1, false));
    assert!(core.ios_backups(Some(&env.path("nope").display().to_string())).is_err());

    let d = core.ios_open(&one[0].id, "").unwrap();
    assert_eq!((d.kind, d.read_only, d.world_count), (DeviceKind::Ios, true, Some(1)));
    assert!(core.list_devices().iter().any(|x| x.id == d.id));
    let r = core.read("op", &ReadRequest { device_id: d.id.clone(), password: String::new() }, &mut |_| {}).unwrap();
    assert_eq!(r.worlds[0].name, "iPhone World");
    let saved = core.save_to_library(&r.read_id, &[A.to_string()]).unwrap();
    assert_eq!((saved[0].origin, saved[0].source.as_str()), (bhw_core::Origin::Ios, "Test iPhone backup"));
    // Read-only as a target, fine as a source (the 64-bit world becomes 32-bit on the phone).
    let src = Source::Device { device_id: d.id.clone() };
    assert_eq!(core.transfer_plan(&src, &[A.to_string()], &d.id).unwrap_err().code, ErrorCode::NotSupportedYet);
    std::fs::write(core.lib().unwrap().inner.backups_dir().join("x.ab"), b"x").unwrap();
    assert_eq!(core.restore_backup("op", "x.ab", &d.id, &mut |_| {}).unwrap_err().code, ErrorCode::NotSupportedYet);
    let plan = core.transfer_plan(&src, &[A.to_string()], "S1").unwrap();
    core.transfer("op", &TransferRequest { plan_id: plan.plan_id, choices: Default::default() }, &mut |_| {}).unwrap();
    assert_eq!(env.phone.names(), ["iPhone World"]);

    // Encrypted: wrong passwords are counted.
    let locked = env.path("ios/udid-locked").display().to_string();
    for n in 1..=2 {
        let e = core.ios_open(&locked, "bad").unwrap_err();
        assert_eq!((e.code, e.extra.attempt), (ErrorCode::WrongPassword, Some(n)));
    }
    assert_eq!(core.ios_open(&locked, "pw").unwrap().world_count, Some(1));
    // Old read sessions are dropped: the backup must be opened again.
    for _ in 0..4 {
        core.read("op", &ReadRequest { device_id: "S1".into(), password: String::new() }, &mut |_| {}).unwrap();
    }
    let e = core.read("op", &ReadRequest { device_id: d.id, password: String::new() }, &mut |_| {}).unwrap_err();
    assert_eq!(e.code, ErrorCode::NotFound);
}

#[test]
fn wifi_and_paired_devices() {
    let env = Env::new(&PhoneData::new(&[]));
    let core = env.core();
    let c = core.wifi_discover().unwrap();
    assert_eq!(c.len(), 2);
    assert_eq!(c[0].address, "10.0.0.2:37000");
    let d = core.wifi_pair(" 10.0.0.2:37000 ", "123 456").unwrap();
    assert_eq!(d.id, "10.0.0.2:5555");
    let e = core.wifi_pair("10.9.9.9:1", "123456").unwrap_err();
    assert_eq!(e.code, ErrorCode::PairFailed);
    assert!(core.paired_devices().unwrap().is_empty());
    let dir = env.path("android");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        bhw_device::wifi::known_hosts_file(&dir),
        bhw_device::wifi::encode_known_hosts(&["adb-guid".into()]),
    )
    .unwrap();
    assert_eq!(core.paired_devices().unwrap()[0].id, "adb-guid");
    core.forget_paired("adb-guid").unwrap();
    assert_eq!(core.forget_paired("adb-guid").unwrap_err().code, ErrorCode::NotFound);
    core.restart_adb().unwrap();
    assert!(core.diagnostics().contains("Android Debug Bridge version 1.0.41"));

    let no_dirs = bhw_core::Core::new(bhw_core::CoreConfig { android_dir: None, adb: None, ..env.config(false) });
    assert!(no_dirs.paired_devices().unwrap().is_empty());
    assert_eq!(no_dirs.forget_paired("x").unwrap_err().code, ErrorCode::NotFound);
    assert_eq!(no_dirs.restart_adb().unwrap_err().code, ErrorCode::AdbMissing);

    let env = Env::with(
        &PhoneData::new(&[]),
        |r| {
            r.on("pair ", Reply::ok("Failed: wrong code"));
            r.on("version", Reply::fail(1, "boom").then_hang());
        },
        |_| {},
    );
    let mut cfg = env.config(true);
    let mut adb = env.phone.adb();
    adb.timeouts.command = std::time::Duration::from_millis(200);
    cfg.adb = Some(adb);
    let core = bhw_core::Core::new(cfg);
    assert_eq!(core.wifi_pair("10.0.0.2:37000", "1").unwrap_err().code, ErrorCode::PairFailed);
    assert!(core.diagnostics().contains("adb: adb ("));
}

#[test]
fn settings_and_activity() {
    let env = Env::new(&PhoneData::new(&[]));
    let core = env.core();
    assert_eq!(core.settings().worlds_folder, env.path("Worlds").display().to_string());
    let s = core.set_settings(serde_json::json!({ "theme": "dark", "nope": 1 }).as_object().unwrap()).unwrap();
    assert_eq!(s.theme, "dark");
    let e = core.set_settings(serde_json::json!({ "backups_to_keep": "x" }).as_object().unwrap()).unwrap_err();
    assert_eq!(e.code, ErrorCode::Io);
    let reloaded = env.core();
    assert_eq!(reloaded.settings().theme, "dark");
    std::fs::write(env.path("config/settings.json"), b"broken").unwrap();
    assert_eq!(env.core().settings().theme, "system");
    // Saving next to a file that is in the way fails.
    let blocked = Settings::defaults(Path::new("/w"));
    write(&env.path("file"), b"x");
    assert!(blocked.save(&env.path("file/settings.json")).is_err());
    assert!(blocked.save(Path::new("settings-relative-test.json")).is_ok());
    std::fs::remove_file("settings-relative-test.json").unwrap();

    // Activity: capped, cleared, saved best effort.
    let log = ActivityLog::open(&env.path("act/activity.json"));
    let e = Entry::new(ActivityKind::Copy, "Phone");
    for _ in 0..(bhw_core::activity::MAX_ENTRIES + 2) {
        log.add(&e, ActivityResult::Done, None);
    }
    assert_eq!(ActivityLog::open(&env.path("act/activity.json")).list().len(), bhw_core::activity::MAX_ENTRIES);
    let err = AppError::new(ErrorCode::Cancelled);
    assert!(log.record::<()>(&e, Err(err)).is_err());
    assert_eq!(log.list()[0].result, ActivityResult::Stopped);
    log.clear().unwrap();
    assert!(log.list().is_empty());
    let bad = ActivityLog::open(&env.path("file/activity.json"));
    bad.add(&e, ActivityResult::Failed, Some(&AppError::io("x")));
    assert!(bad.clear().is_err());
    let rel = ActivityLog::open(Path::new("activity-relative-test.json"));
    rel.clear().unwrap();
    std::fs::remove_file("activity-relative-test.json").unwrap();
    core.clear_activity().unwrap();
    assert!(core.list_activity().is_empty());
}

#[test]
fn errors_map_to_ui_codes() {
    use std::io;
    let d = |e: DeviceError| AppError::from(e).code;
    let s = || "S".to_string();
    assert_eq!(d(DeviceError::AdbNotFound), ErrorCode::AdbMissing);
    assert_eq!(d(DeviceError::Unauthorized { serial: s() }), ErrorCode::NotAllowed);
    assert_eq!(d(DeviceError::NoPermissions { serial: s(), detail: s() }), ErrorCode::NotAllowed);
    assert_eq!(d(DeviceError::Offline { serial: s() }), ErrorCode::Disconnected);
    assert_eq!(d(DeviceError::Disconnected), ErrorCode::Disconnected);
    assert_eq!(d(DeviceError::DeviceNotFound { serial: s() }), ErrorCode::NotFound);
    assert_eq!(d(DeviceError::SavesNotFound), ErrorCode::NotFound);
    assert_eq!(d(DeviceError::GameNotInstalled), ErrorCode::GameNotInstalled);
    let e = AppError::from(DeviceError::UnsupportedVersion { found: "1.6".into() });
    assert_eq!((e.code, e.extra.version.as_deref()), (ErrorCode::UnsupportedVersion, Some("1.6")));
    assert_eq!(d(DeviceError::GameRunning), ErrorCode::GameRunning);
    assert_eq!(d(DeviceError::GameStopped), ErrorCode::GameStopped);
    assert_eq!(d(DeviceError::ConfirmTimeout), ErrorCode::Declined);
    assert_eq!(d(DeviceError::WrongPassword), ErrorCode::WrongPassword);
    assert_eq!(d(DeviceError::EmptyBackup), ErrorCode::BackupEmpty);
    assert_eq!(d(DeviceError::DiskFull), ErrorCode::DiskFull);
    assert_eq!(d(DeviceError::Cancelled), ErrorCode::Cancelled);
    assert_eq!(d(DeviceError::ConnectFailed(s())), ErrorCode::PairFailed);
    assert_eq!(d(DeviceError::Timeout), ErrorCode::Io);
    assert_eq!(d(DeviceError::Io(io::Error::from(io::ErrorKind::NotFound))), ErrorCode::NotFound);
    assert_eq!(AppError::from(io::Error::from(io::ErrorKind::StorageFull)).code, ErrorCode::DiskFull);
    assert_eq!(AppError::from(io::Error::other("x")).code, ErrorCode::Io);

    let b = |e: bhw_backup::Error| AppError::from(e).code;
    assert_eq!(b(bhw_backup::Error::PasswordRequired), ErrorCode::WrongPassword);
    assert_eq!(b(bhw_backup::Error::Io(io::Error::other("x"))), ErrorCode::Io);
    assert_eq!(b(bhw_backup::Error::NotABackup), ErrorCode::Io);
    let w = |e: bhw_world::Error| AppError::from(e).code;
    assert_eq!(w(bhw_world::Error::Io(io::Error::from(io::ErrorKind::NotFound))), ErrorCode::NotFound);
    assert_eq!(w(bhw_world::Error::Backup(bhw_backup::Error::WrongPassword)), ErrorCode::WrongPassword);
    assert_eq!(w(bhw_world::Error::InvalidId("x".into())), ErrorCode::Io);
    assert_eq!(w(bhw_world::Error::Format("x".into())), ErrorCode::WorldDamaged);
    let e = AppError::from(bhw_save::Error::Io { path: "/p".into(), source: io::Error::other("x") });
    assert_eq!((e.code, e.detail.as_deref()), (ErrorCode::Io, Some("/p: x")));
    assert_eq!(AppError::from(bhw_save::Error::Format("x".into())).code, ErrorCode::WorldDamaged);
    let a = |e: bhw_assets::Error| AppError::from(e).code;
    assert_eq!(a(bhw_assets::Error::NotSet), ErrorCode::ApkMissing);
    assert_eq!(a(bhw_assets::Error::Cancelled), ErrorCode::Cancelled);
    assert_eq!(a(bhw_assets::Error::Io(io::Error::other("x"))), ErrorCode::Io);
    let i = |e: bhw_ios::Error| AppError::from(e).code;
    assert_eq!(i(bhw_ios::Error::Io(io::Error::other("x"))), ErrorCode::Io);
    assert_eq!(i(bhw_ios::Error::World(bhw_world::Error::Format("x".into()))), ErrorCode::WorldDamaged);
    assert_eq!(i(bhw_ios::Error::NoBlockheads), ErrorCode::GameNotInstalled);
    assert_eq!(i(bhw_ios::Error::Unsupported("x".into())), ErrorCode::NotSupportedYet);
    assert_eq!(i(bhw_ios::Error::MissingFile("x".into())), ErrorCode::NotFound);
    assert_eq!(i(bhw_ios::Error::Corrupt("x".into())), ErrorCode::Io);
    assert_eq!(AppError::from(bhw_lmdb::convert(b"junk", Layout::Bits32).unwrap_err()).code, ErrorCode::WorldDamaged);
    assert_eq!(AppError::from(serde_json::from_str::<u8>("x").unwrap_err()).code, ErrorCode::Io);

    assert_eq!(AppError::new(ErrorCode::GameStopped).to_string(), "game_stopped");
    assert_eq!(AppError::io("boom").to_string(), "io: boom");
    let j = serde_json::to_value(AppError::new(ErrorCode::Declined).device("P").backup(Some("b")).name("n")).unwrap();
    assert_eq!(j, serde_json::json!({ "code": "declined", "device": "P", "backup_id": "b", "name": "n" }));
}

#[test]
fn helpers() {
    let tmp = tempfile::tempdir().unwrap();
    assert_ne!(unique_id("x"), unique_id("x"));
    assert_eq!(mtime_ms(&tmp.path().join("none")), 0);
    assert_eq!(file_name(Path::new("/")), "/");
    std::fs::write(tmp.path().join("f"), b"x").unwrap();
    assert!(child(tmp.path(), "f").is_ok());
    assert_eq!(child(tmp.path(), "g").unwrap_err().code, ErrorCode::NotFound);
    {
        let _t = TempPath(tmp.path().join("f"));
    }
    assert!(!tmp.path().join("f").exists());
    let ops = Ops::default();
    {
        let g = ops.start("a");
        assert!(g.cancel.check().is_ok());
        assert!(ops.cancel("a"));
        assert_eq!(g.cancel.check().unwrap_err().code, ErrorCode::Cancelled);
    }
    assert!(!ops.cancel("a"));
    let mut t = bhw_core::util::Throttle::default();
    assert!(t.pass(10, false));
    assert!(!t.pass(20, false));
    assert!(t.pass(20, true));
    assert!(t.pass(30 + bhw_core::util::Throttle::STEP, false));
    // Origins and saves folders.
    use bhw_world::SourceKind as K;
    let o: Vec<_> = [K::AndroidBackup, K::AndroidRoot, K::BlockheadsPc, K::IosBackup, K::Bhworld]
        .into_iter()
        .map(bhw_core::library::origin_of)
        .collect();
    use bhw_core::Origin as O;
    assert_eq!(o, [O::Phone, O::Phone, O::Pc, O::Ios, O::File]);
    let saves = |pkg: Option<&str>, prefix: &str| bhw_world::backup::BackupSaves {
        prefix: prefix.into(),
        package: pkg.map(str::to_string),
        domain: None,
        worlds: vec![],
        game_db: vec![],
    };
    let pick = bhw_core::read::pick_saves;
    assert_eq!(pick(vec![]), None);
    let game = Some("com.noodlecake.blockheads");
    assert_eq!(pick(vec![saves(None, "a/"), saves(game, "b/")]).unwrap().prefix, "b/");
    assert_eq!(pick(vec![saves(None, "a/"), saves(None, "b/")]).unwrap().prefix, "a/");
    // Poisoned locks are still usable.
    let m = Arc::new(std::sync::Mutex::new(1));
    let m2 = m.clone();
    let _ = std::thread::spawn(move || {
        let _g = m2.lock().unwrap();
        panic!("poison");
    })
    .join();
    assert_eq!(*bhw_core::util::lock(&m), 1);
}
