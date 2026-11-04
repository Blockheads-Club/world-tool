//! Rooted phones: direct reads and writes of the game's data (scripted `adb root` phone backed by a local folder).

mod common;

use std::time::{Duration, Instant};

use bhw_core::{BackupReason, Choice, ErrorCode, ReadRequest, Source, Stage, TransferRequest};
use bhw_device::fake::Reply;
use common::*;

fn read(core: &bhw_core::Core) -> Result<bhw_core::ReadResult, bhw_core::AppError> {
    core.read("op", &ReadRequest { device_id: "S1".into(), password: String::new() }, &mut |_| {})
}

fn stages(ev: &std::sync::Mutex<Vec<bhw_core::Progress>>) -> Vec<Stage> {
    ev.lock().unwrap().iter().map(|p| p.stage).collect()
}

fn never(core: &bhw_core::Core) {
    core.set_settings(serde_json::json!({ "root_mode": "never" }).as_object().unwrap()).unwrap();
}

#[test]
fn reads_copies_and_restores_through_root() {
    let env = Env::rooted(&PhoneData::new(&[(A, "Sunny Isle"), (B, "Castle")]), |_| {});
    let core = env.core();
    // Read: no phone screens.
    let (ev, mut on) = events();
    let r = core.read("op", &ReadRequest { device_id: "S1".into(), password: String::new() }, &mut on).unwrap();
    let mut names: Vec<_> = r.worlds.iter().map(|w| w.name.clone()).collect();
    names.sort();
    assert_eq!(names, ["Castle", "Sunny Isle"]);
    assert!(!stages(&ev).contains(&Stage::PhoneBackup));
    assert!(stages(&ev).contains(&Stage::Reading));
    assert!(env.phone.runner.lines().iter().all(|l| !l.contains("backup -f")));
    let dev = core.list_devices().into_iter().find(|d| d.id == "S1").unwrap();
    assert!(dev.root);
    // Saving to the library reads the local copy.
    let saved = core.save_to_library(&r.read_id, &[A.to_string()]).unwrap();
    assert_eq!(saved[0].base.name, "Sunny Isle");

    // Copy: keep both + a new world.
    let lib = core.lib().unwrap();
    let w = client_world(D, "New World");
    let meta = bhw_core::library::AddMeta {
        origin: bhw_core::Origin::File,
        source: "x".into(),
        source_detail: None,
        game_version: None,
        kind: bhw_world::SourceKind::Bhworld,
        device_name: None,
    };
    lib.add(&w, &bhw_core::worlds::inspect(&w, None).unwrap(), &meta).unwrap();
    let castle = client_world(C, "Castle");
    lib.add(&castle, &bhw_core::worlds::inspect(&castle, None).unwrap(), &meta).unwrap();
    let ids: Vec<String> = lib.list().unwrap().into_iter().map(|w| w.base.id).collect();
    let plan = core.transfer_plan(&Source::Library, &ids, "S1").unwrap();
    assert!(!plan.needs_phone);
    let key = |n: &str| ids.iter().find(|i| plan.items.iter().any(|p| &p.key == *i && p.name == n)).unwrap().clone();
    let choices = [(key("Sunny Isle"), Choice::KeepBoth), (key("Castle"), Choice::Replace)].into_iter().collect();
    let (ev, mut on) = events();
    let res = core.transfer("op", &TransferRequest { plan_id: plan.plan_id, choices }, &mut on).unwrap();
    assert_eq!(
        (res.added.clone(), res.kept_both.clone()),
        (vec!["New World".to_string()], vec!["Sunny Isle (2)".to_string()])
    );
    assert_eq!(device_names(&env.dev()), ["Castle", "New World", "Sunny Isle", "Sunny Isle (2)"]);
    let gdb = bhw_save::GameDb::open(&env.dev().join("game_db")).unwrap().world_ids();
    assert_eq!(gdb.len(), 4);
    assert!(env.dev().join(format!("saves/{D}/world_db/lock.mdb")).is_file());
    assert!(!env.dev().join(format!("saves/{B}")).exists() && env.dev().join(format!("saves/{C}")).is_dir());
    assert!(env.dev().join("game").is_dir(), "other folders stay");
    let st = stages(&ev);
    for s in [Stage::BackingUp, Stage::Preparing, Stage::Writing, Stage::Verifying, Stage::Checking] {
        assert!(st.contains(&s), "{s:?} missing: {st:?}");
    }
    assert!(!st.contains(&Stage::PhoneBackup) && !st.contains(&Stage::PhoneRestore));
    let lines = env.phone.runner.lines();
    assert!(lines.iter().any(|l| l.contains("chown -R 10123:10123") && l.contains("restorecon -R")));
    assert!(lines.iter().any(|l| l.contains("am kill") || l.contains("pidof")));
    // The automatic backup is the copy taken before writing.
    let b = core.list_backups().unwrap();
    assert_eq!((b.len(), b[0].reason, b[0].worlds), (1, BackupReason::BeforeWrite, 2));
    assert_eq!(b[0].id, res.backup_id.clone().unwrap());
    // The phone's session is up to date.
    assert_eq!(core.source_worlds(&Source::Device { device_id: "S1".into() }).unwrap().len(), 4);

    // Restore it: back to the two worlds, with a before-restore copy stored.
    let (ev, mut on) = events();
    core.restore_backup("op", &b[0].id, "S1", &mut on).unwrap();
    assert_eq!(device_names(&env.dev()), ["Castle", "Sunny Isle"]);
    let gdb = bhw_save::GameDb::open(&env.dev().join("game_db")).unwrap().world_ids();
    assert_eq!(gdb.len(), 2);
    assert!(stages(&ev).contains(&Stage::Verifying));
    assert!(core.list_backups().unwrap().iter().any(|b| b.reason == BackupReason::BeforeRestore));
    assert!(env.phone.runner.lines().iter().all(|l| !l.contains("backup -f") && !l.contains(" restore ")));
}

#[test]
fn the_backup_method_is_used_when_root_is_off_or_fails() {
    // Root mode "never": the backup screens.
    let env = Env::rooted(&PhoneData::new(&[(A, "One")]), |_| {});
    let core = env.core();
    never(&core);
    let (ev, mut on) = events();
    core.read("op", &ReadRequest { device_id: "S1".into(), password: String::new() }, &mut on).unwrap();
    assert!(stages(&ev).contains(&Stage::PhoneBackup));
    assert!(!core.list_devices()[0].root);
    // Old "always" values count as "auto".
    let s = core.set_settings(serde_json::json!({ "root_mode": "always" }).as_object().unwrap()).unwrap();
    assert_eq!(s.root_mode, "auto");

    // `su` refused (the find fails): backup method, and root is not tried again.
    let env = Env::rooted(&PhoneData::new(&[(A, "One")]), |r| {
        r.on("find", sh("Permission denied", 1));
    });
    let core = env.core();
    let (ev, mut on) = events();
    core.read("op", &ReadRequest { device_id: "S1".into(), password: String::new() }, &mut on).unwrap();
    assert!(stages(&ev).contains(&Stage::PhoneBackup));
    assert!(!core.list_devices().iter().find(|d| d.id == "S1").unwrap().root);
    read(&core).unwrap();
    assert_eq!(env.phone.runner.lines().iter().filter(|l| l.contains("find")).count(), 1);

    // Other failures finding the saves: the backup method (which then reports its own problem, if any).
    let env = Env::rooted(&PhoneData::new(&[(A, "One")]), |r| {
        r.on("find", Reply::fail(1, "something odd"));
    });
    let core = env.core();
    assert_eq!(read(&core).unwrap().worlds.len(), 1);

    // No root on a normal phone.
    let env = Env::new(&PhoneData::new(&[(A, "One")]));
    let core = env.core();
    let (ev, mut on) = events();
    core.read("op", &ReadRequest { device_id: "S1".into(), password: String::new() }, &mut on).unwrap();
    assert!(stages(&ev).contains(&Stage::PhoneBackup));
}

#[test]
fn root_is_probed_in_the_background_and_forgotten_with_the_phone() {
    let env = Env::rooted(&PhoneData::new(&[(A, "One")]), |r| {
        r.once("devices -l", Reply::ok("List of devices attached\n"));
    });
    let core = env.core();
    // No phone yet.
    assert!(core.list_devices().iter().all(|d| d.id != "S1"));
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let root = core.list_devices().iter().find(|d| d.id == "S1").is_some_and(|d| d.root);
        if root {
            break;
        }
        assert!(Instant::now() < deadline, "root never probed");
        std::thread::sleep(Duration::from_millis(20));
    }
    never(&core);
    assert!(!core.list_devices().iter().find(|d| d.id == "S1").unwrap().root);
    // Without adb nothing is probed.
    let core = bhw_core::Core::new(env.config(false));
    assert!(core.list_devices().iter().all(|d| !d.root));
}

#[test]
fn root_failures() {
    // The game's version is checked first.
    let env = Env::rooted(&PhoneData::new(&[(A, "One")]), |r| {
        r.on("getprop", sh("model=Pixel_8\n    versionName=1.6.0\n", 0));
    });
    assert_eq!(read(&env.core()).unwrap_err().code, ErrorCode::UnsupportedVersion);

    // Pulling the saves breaks.
    let env = Env::rooted(&PhoneData::new(&[(A, "One")]), |r| {
        r.on("-cf - 'saves'", Reply::fail(1, "error: device offline"));
    });
    assert_eq!(read(&env.core()).unwrap_err().code, ErrorCode::Disconnected);

    // Sending the saves breaks.
    let env = Env::rooted(&PhoneData::new(&[(A, "One")]), |r| {
        r.on("exec-in", Reply::fail(1, "error: device offline"));
    });
    let core = env.core();
    read(&core).unwrap();
    let plan = core.transfer_plan(&Source::Device { device_id: "S1".into() }, &[A.to_string()], "S1").unwrap();
    let e = core.transfer("op", &TransferRequest { plan_id: plan.plan_id, choices: Default::default() }, &mut |_| {});
    assert_eq!(e.unwrap_err().code, ErrorCode::Disconnected);

    // No game_db yet (no world registered): the copy creates one.
    let mut data = PhoneData::new(&[]);
    data.game_db = false;
    let env = Env::rooted(&data, |r| {
        r.on("-cf - 'game_db'", Reply::ok(""));
    });
    let core = env.core();
    assert!(read(&core).unwrap().worlds.is_empty());
    let lib = core.lib().unwrap();
    let w = client_world(A, "One");
    let meta = bhw_core::library::AddMeta {
        origin: bhw_core::Origin::File,
        source: "x".into(),
        source_detail: None,
        game_version: None,
        kind: bhw_world::SourceKind::Bhworld,
        device_name: None,
    };
    lib.add(&w, &bhw_core::worlds::inspect(&w, None).unwrap(), &meta).unwrap();
    let ids: Vec<String> = lib.list().unwrap().into_iter().map(|w| w.base.id).collect();
    core.set_settings(serde_json::json!({ "backup_before_write": false }).as_object().unwrap()).unwrap();
    let plan = core.transfer_plan(&Source::Library, &ids, "S1").unwrap();
    let res = core.transfer("op", &TransferRequest { plan_id: plan.plan_id, choices: Default::default() }, &mut |_| {});
    assert_eq!(res.unwrap().backup_id, None);
    assert_eq!(device_names(&env.dev()), ["One"]);
    assert_eq!(bhw_save::GameDb::open(&env.dev().join("game_db")).unwrap().world_ids(), [A]);

    // The write did not arrive (finish step ignored): reported, with the backup to undo it.
    let env = Env::rooted(&PhoneData::new(&[(A, "One")]), |r| {
        r.on("chown -R", sh("", 0));
    });
    let core = env.core();
    read(&core).unwrap();
    let plan = core.transfer_plan(&Source::Device { device_id: "S1".into() }, &[A.to_string()], "S1").unwrap();
    let e = core
        .transfer("op", &TransferRequest { plan_id: plan.plan_id, choices: Default::default() }, &mut |_| {})
        .unwrap_err();
    assert_eq!(e.code, ErrorCode::Io);
    assert!(e.detail.unwrap().contains("after writing"));
    assert!(e.extra.backup_id.is_some());
    // Restoring that backup matches what the phone still has.
    let id = e.extra.backup_id.unwrap();
    core.restore_backup("op", &id, "S1", &mut |_| {}).unwrap();
}
