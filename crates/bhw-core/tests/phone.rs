//! Android flows against a scripted phone: read, save, transfer (all choices), restore, backups.

mod common;

use bhw_core::{
    BackupReason, Choice, ConflictKind, DeviceKind, ErrorCode, PlanResult, ReadRequest, Source, Stage, TransferRequest,
};
use bhw_device::fake::Reply;
use common::*;

fn read(core: &bhw_core::Core, id: &str) -> bhw_core::ReadResult {
    core.read("op", &ReadRequest { device_id: id.into(), password: String::new() }, &mut |_| {}).unwrap()
}

#[test]
fn read_save_and_list() {
    let mut data = PhoneData::new(&[(A, "Sunny Isle"), (B, "Deep Expert")]);
    data.worlds.push((C.into(), "Remote".into(), true));
    let env = Env::new(&data);
    let core = env.core();
    let (ev, mut on) = events();
    let r = core.read("op1", &ReadRequest { device_id: "S1".into(), password: String::new() }, &mut on).unwrap();
    let mut names: Vec<_> = r.worlds.iter().map(|w| w.name.as_str()).collect();
    names.sort();
    assert_eq!(names, ["Deep Expert", "Sunny Isle"], "remote caches are left out");
    let deep = r.worlds.iter().find(|w| w.id == B).unwrap();
    assert_eq!(deep.mode, bhw_core::GameMode::Expert);
    assert_eq!(deep.game_version, "1.7.6");
    assert!(deep.preview.as_deref().unwrap().starts_with("data:image/png;base64,"));
    assert!(deep.last_played.unwrap() > deep.created);
    let stages: Vec<Stage> = ev.lock().unwrap().iter().map(|p| p.stage).collect();
    assert_eq!(stages.first(), Some(&Stage::PhoneBackup));
    assert_eq!(stages.last(), Some(&Stage::Reading));
    // The device now knows its world count.
    let d = core.list_devices().into_iter().find(|d| d.id == "S1").unwrap();
    assert_eq!(d.world_count, Some(2));
    // Source worlds of the phone = the last read.
    let src = core.source_worlds(&Source::Device { device_id: "S1".into() }).unwrap();
    assert_eq!(src.len(), 2);
    assert_eq!(core.source_worlds(&Source::Device { device_id: "S9".into() }).unwrap_err().code, ErrorCode::NotFound);

    let saved = core.save_to_library(&r.read_id, &[A.to_string()]).unwrap();
    assert_eq!(saved[0].base.name, "Sunny Isle");
    assert_eq!(saved[0].origin, bhw_core::Origin::Phone);
    assert_eq!(saved[0].source_detail.as_deref(), Some("USB"));
    assert!(saved[0].base.preview.is_some());
    assert_eq!(core.save_to_library(&r.read_id, &[D.to_string()]).unwrap_err().code, ErrorCode::NotFound);
    assert_eq!(core.save_to_library("nope", &[]).unwrap_err().code, ErrorCode::NotFound);
    let lib = core.lib().unwrap().list().unwrap();
    assert_eq!(lib.len(), 1);
    assert_eq!(lib[0].world_id, A);
    let acts = core.list_activity();
    assert_eq!(acts[0].kind, bhw_core::ActivityKind::Read);
    assert_eq!(acts[0].result, bhw_core::ActivityResult::Done);
}

#[test]
fn read_failures() {
    let env = Env::with(
        &PhoneData::new(&[]),
        |r| {
            r.once("backup -f", Reply::ok(""));
        },
        |_| {},
    );
    let core = env.core();
    // Unauthorized / offline / unknown.
    for (id, code) in [("U1", ErrorCode::NotAllowed), ("O1", ErrorCode::Disconnected), ("X", ErrorCode::NotFound)] {
        let e =
            core.read("op", &ReadRequest { device_id: id.into(), password: String::new() }, &mut |_| {}).unwrap_err();
        assert_eq!(e.code, code, "{id}");
    }
    // A 0-byte backup: declined on the phone (logged as stopped).
    let e = core.read("op", &ReadRequest { device_id: "S1".into(), password: String::new() }, &mut |_| {}).unwrap_err();
    assert_eq!(e.code, ErrorCode::Declined);
    assert_eq!(e.extra.device.as_deref(), Some("Pixel 8"));
    assert_eq!(core.list_activity()[0].result, bhw_core::ActivityResult::Stopped);
    assert_eq!(core.list_activity()[1].result, bhw_core::ActivityResult::Failed);
    // A phone without worlds or saves folder reads as empty.
    let r = read(&core, "S1");
    assert!(r.worlds.is_empty());
}

#[test]
fn read_without_saves_folder_and_wrong_password() {
    let env = Env::new(&PhoneData::new(&[]));
    *env.phone.ab.lock().unwrap() = {
        // Only the manifest: no saves folder at all.
        let mut out = Vec::new();
        let mut w = bhw_backup::BackupWriter::create(Box::new(&mut out), &Default::default()).unwrap();
        let mut t = bhw_backup::TarWriter::new(&mut w);
        let mut r: &[u8] = b"m";
        t.append("apps/com.noodlecake.blockheads/_manifest", bhw_backup::EntryKind::File, 0o600, 0, 1, &mut r).unwrap();
        t.finish().unwrap();
        w.finish().unwrap();
        out
    };
    let core = env.core();
    assert!(read(&core, "S1").worlds.is_empty());
    *env.phone.ab.lock().unwrap() = phone_ab(&PhoneData::new(&[(A, "Locked")]), Some("pw"));
    let e = core.read("op", &ReadRequest { device_id: "S1".into(), password: "bad".into() }, &mut |_| {}).unwrap_err();
    assert_eq!(e.code, ErrorCode::WrongPassword);
    let r = core.read("op", &ReadRequest { device_id: "S1".into(), password: "pw".into() }, &mut |_| {}).unwrap();
    assert_eq!(r.worlds[0].name, "Locked");
}

#[test]
fn transfer_to_phone_with_every_choice() {
    let env = Env::new(&PhoneData::new(&[(A, "Sunny Isle"), (B, "Castle")]));
    let core = env.core();
    // Library: A (same id as the phone's), "Castle" under another id, and a new world D.
    let r = read(&core, "S1");
    core.save_to_library(&r.read_id, &[A.to_string()]).unwrap();
    let lib = core.lib().unwrap();
    let meta = bhw_core::library::AddMeta {
        origin: bhw_core::Origin::File,
        source: "x.bhworld".into(),
        source_detail: None,
        game_version: Some("1.7.6".into()),
        kind: bhw_world::SourceKind::Bhworld,
        device_name: None,
    };
    for (id, name) in [(C, "Castle"), (D, "New World")] {
        let w = client_world(id, name);
        let info = bhw_core::worlds::inspect(&w, None).unwrap();
        lib.add(&w, &info, &meta).unwrap();
    }
    let all = lib.list().unwrap();
    let key = |n: &str| all.iter().find(|w| w.base.name == n).unwrap().base.id.clone();
    let ids = vec![key("Sunny Isle"), key("Castle"), key("New World")];
    let plan = core.transfer_plan(&Source::Library, &ids, "S1").unwrap();
    assert!(plan.needs_phone);
    assert_eq!(plan.total_after, 5);
    let mut kinds: Vec<_> = plan.items.iter().map(|i| (i.name.as_str(), i.result, i.conflict)).collect();
    kinds.sort_by_key(|k| k.0);
    assert_eq!(
        kinds,
        [
            ("Castle", PlanResult::Conflict, Some(ConflictKind::SameName)),
            ("New World", PlanResult::Added, None),
            ("Sunny Isle", PlanResult::Conflict, Some(ConflictKind::SameId)),
        ]
    );
    assert_eq!(plan.items[2].result, PlanResult::Added, "new worlds come last");
    let sunny = plan.items.iter().find(|i| i.name == "Sunny Isle").unwrap();
    assert_eq!(sunny.copy_name.as_deref(), Some("Sunny Isle (2)"));

    // Keep both for the same id, replace for the same name.
    let choices = [(key("Sunny Isle"), Choice::KeepBoth), (key("Castle"), Choice::Replace)].into_iter().collect();
    let (ev, mut on) = events();
    let res = core.transfer("op-t", &TransferRequest { plan_id: plan.plan_id.clone(), choices }, &mut on).unwrap();
    let first = res.backup_id.clone().unwrap();
    assert_eq!(res.added, ["New World"]);
    assert_eq!(res.replaced, ["Castle"]);
    assert_eq!(res.kept_both, ["Sunny Isle (2)"]);
    assert_eq!(res.total, 4);
    assert_eq!(res.target, "Pixel 8");
    assert_eq!(env.phone.names(), ["Castle", "New World", "Sunny Isle", "Sunny Isle (2)"]);
    let mut reg = env.phone.registered();
    reg.sort();
    assert_eq!(reg.len(), 4);
    assert!(reg.contains(&C.to_string()) && !reg.contains(&B.to_string()));
    let stages: Vec<Stage> = ev.lock().unwrap().iter().map(|p| p.stage).collect();
    for s in
        [Stage::PhoneBackup, Stage::BackingUp, Stage::Preparing, Stage::PhoneRestore, Stage::Writing, Stage::Checking]
    {
        assert!(stages.contains(&s), "{s:?} missing: {stages:?}");
    }
    // The automatic backup is stored and listed.
    let backups = core.list_backups().unwrap();
    assert_eq!(backups.len(), 1);
    assert_eq!(backups[0].id, res.backup_id.clone().unwrap());
    assert_eq!(
        (backups[0].reason, backups[0].reason_count, backups[0].worlds),
        (BackupReason::BeforeWrite, Some(3), 2)
    );
    assert_eq!(backups[0].device_id, "S1");
    // The phone's session reflects the write without a new read.
    assert_eq!(core.source_worlds(&Source::Device { device_id: "S1".into() }).unwrap().len(), 4);

    // Skip leaves the phone alone; an unknown plan is not found.
    let plan = core.transfer_plan(&Source::Library, &[key("Castle")], "S1").unwrap();
    let choices = [(key("Castle"), Choice::Skip)].into_iter().collect();
    let res = core.transfer("op", &TransferRequest { plan_id: plan.plan_id, choices }, &mut |_| {}).unwrap();
    assert!(res.added.is_empty() && res.replaced.is_empty());
    assert_eq!(res.total, 4);
    let e = core.transfer("op", &TransferRequest { plan_id: "nope".into(), choices: Default::default() }, &mut |_| {});
    assert_eq!(e.unwrap_err().code, ErrorCode::NotFound);

    assert!(res.backup_id.is_some());
    // Restoring the first automatic backup puts the phone back (after backing it up again).
    core.restore_backup("op-r", &first, "S1", &mut |_| {}).unwrap();
    assert_eq!(env.phone.names(), ["Castle", "Sunny Isle"]);
    let reasons: Vec<_> = core.list_backups().unwrap().iter().map(|b| b.reason).collect();
    assert!(reasons.contains(&BackupReason::BeforeRestore));
}

#[test]
fn transfer_from_files_and_new_phone() {
    let mut data = PhoneData::new(&[]);
    data.game_db = false;
    data.extra.push(("game/settings".into(), b"s".to_vec()));
    let env = Env::new(&data);
    let core = env.core();
    // A server world folder and a .bhworld file as sources; the phone was never read (no conflicts known).
    let server = env.path("server/world");
    bhw_save::save::write_dir_files(&server, &server_files(B, "Server World")).unwrap();
    let paths = vec![server.display().to_string(), env.path("missing.bhworld").display().to_string()];
    let src = Source::Files { paths: paths.clone() };
    let offered = core.source_worlds(&src).unwrap();
    assert_eq!(offered.len(), 1);
    assert_eq!(offered[0].name, "Server World");
    let plan = core.transfer_plan(&src, &[paths[0].clone()], "S1").unwrap();
    assert_eq!(plan.backup_size, 0);
    let res = core
        .transfer("op", &TransferRequest { plan_id: plan.plan_id, choices: Default::default() }, &mut |_| {})
        .unwrap();
    assert_eq!(res.added, ["Server World"]);
    assert_eq!(env.phone.names(), ["Server World"]);
    assert_eq!(env.phone.registered(), [B]);
}

#[test]
fn transfer_failures_keep_the_backup_id() {
    let env = Env::with(
        &PhoneData::new(&[(A, "One")]),
        |r| {
            r.once(" restore ", Reply::fail(1, "error: device offline"));
        },
        |_| {},
    );
    let core = env.core();
    let r = read(&core, "S1");
    let src = Source::Device { device_id: "S1".into() };
    // Copy the phone's own world back to it: same id → keep both by default.
    let plan = core.transfer_plan(&src, &[A.to_string()], "S1").unwrap();
    let e = core
        .transfer("op", &TransferRequest { plan_id: plan.plan_id.clone(), choices: Default::default() }, &mut |_| {})
        .unwrap_err();
    assert_eq!(e.code, ErrorCode::Disconnected);
    assert!(e.extra.backup_id.is_some());
    assert_eq!(e.extra.device.as_deref(), Some("Pixel 8"));
    assert_eq!(core.list_activity()[0].error, Some(ErrorCode::Disconnected));
    let _ = r;
    // Without "backup before write" no backup is stored.
    let patch = serde_json::json!({ "backup_before_write": false });
    core.set_settings(patch.as_object().unwrap()).unwrap();
    let res = core
        .transfer("op", &TransferRequest { plan_id: plan.plan_id, choices: Default::default() }, &mut |_| {})
        .unwrap();
    assert_eq!(res.backup_id, None);
    assert_eq!(res.kept_both, ["One (2)"]);
    assert_eq!(env.phone.names(), ["One", "One (2)"]);
}

#[test]
fn backups_are_pruned_and_deleted() {
    let env = Env::new(&PhoneData::new(&[(A, "One")]));
    let core = env.core();
    core.set_settings(serde_json::json!({ "backups_to_keep": 1 }).as_object().unwrap()).unwrap();
    let r = read(&core, "S1");
    let src = Source::Device { device_id: "S1".into() };
    for _ in 0..2 {
        let plan = core.transfer_plan(&src, &[A.to_string()], "S1").unwrap();
        let choices = [(A.to_string(), Choice::Replace)].into_iter().collect();
        core.transfer("op", &TransferRequest { plan_id: plan.plan_id, choices }, &mut |_| {}).unwrap();
    }
    let _ = r;
    let b = core.list_backups().unwrap();
    assert_eq!(b.len(), 1);
    assert_eq!(env.phone.names(), ["One"]);
    core.delete_backup(&b[0].id).unwrap();
    assert!(core.list_backups().unwrap().is_empty());
    assert_eq!(core.delete_backup(&b[0].id).unwrap_err().code, ErrorCode::NotFound);
    assert_eq!(core.delete_backup("../x").unwrap_err().code, ErrorCode::NotFound);
    assert_eq!(core.restore_backup("op", "nope.ab", "S1", &mut |_| {}).unwrap_err().code, ErrorCode::NotFound);
    // A backup copied in by hand (no sidecars) is listed too.
    let dir = core.lib().unwrap().inner.backups_dir();
    std::fs::write(dir.join("hand.ab"), phone_ab(&PhoneData::new(&[]), None)).unwrap();
    let b = core.list_backups().unwrap();
    assert_eq!((b[0].id.as_str(), b[0].automatic, b[0].device.as_str()), ("hand.ab", false, ""));
}

#[test]
fn cancel_stops_a_read() {
    let env = Env::with(
        &PhoneData::new(&[(A, "One")]),
        |r| {
            r.once("backup -f", Reply::hang());
        },
        |_| {},
    );
    let core = std::sync::Arc::new(env.core());
    let c = core.clone();
    let t = std::thread::spawn(move || {
        c.read("op-c", &ReadRequest { device_id: "S1".into(), password: String::new() }, &mut |_| {})
    });
    std::thread::sleep(std::time::Duration::from_millis(400));
    core.cancel("op-c");
    core.cancel("not-running");
    assert_eq!(t.join().unwrap().unwrap_err().code, ErrorCode::Cancelled);
}

#[test]
fn devices_listing() {
    let env = Env::new(&PhoneData::new(&[]));
    let core = env.core();
    let d = core.list_devices();
    let s1 = &d[0];
    assert_eq!((s1.kind, s1.name.as_str(), s1.os_version.as_deref()), (DeviceKind::Android, "Pixel 8", Some("14")));
    assert_eq!(s1.game_version.as_deref(), Some("1.7.6"));
    let wifi = d.iter().find(|x| x.id == "10.0.0.2:5555").unwrap();
    assert_eq!((wifi.connection, wifi.address.as_deref()), (bhw_core::Connection::Wifi, Some("10.0.0.2:5555")));
    let u = d.iter().find(|x| x.id == "U1").unwrap();
    assert_eq!(
        (u.status, u.name.as_str(), u.game_version.as_deref()),
        (bhw_core::DeviceStatus::Unauthorized, "U1", None)
    );
    assert_eq!(d.iter().find(|x| x.id == "O1").unwrap().status, bhw_core::DeviceStatus::Offline);
    // Without adb: no phones.
    let core = bhw_core::Core::new(env.config(false));
    assert!(core.list_devices().is_empty());
    assert_eq!(core.wifi_discover().unwrap_err().code, ErrorCode::AdbMissing);
    assert!(core.diagnostics().contains("adb was not found"));
}
