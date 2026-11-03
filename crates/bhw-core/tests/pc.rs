//! Blockheads PC: listing, reading, writing, backups and restores.

mod common;

use bhw_core::{BackupReason, Choice, DeviceKind, ErrorCode, ReadRequest, Source, TransferRequest};
use bhw_device::fake::Reply;
use bhw_save::GameDb;
use common::*;

fn pc_env(worlds: &[(&str, &str)], game_db: bool, running: bool) -> Env {
    let mut env = Env::with(&PhoneData::new(&[(C, "Phone World")]), |_| {}, |l| {
        if running {
            l.on("comm=", Reply::ok("bhpc-run\n"));
        }
    });
    env.pc = Some(pc_dir(env.tmp.path(), worlds, game_db));
    env
}

fn support(env: &Env) -> std::path::PathBuf {
    env.pc.clone().unwrap().join("home/Library/Application Support")
}

fn pc_ids(env: &Env) -> Vec<String> {
    let mut v: Vec<String> =
        std::fs::read_dir(support(env).join("saves")).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    v.sort();
    v
}

fn registered(env: &Env) -> Vec<String> {
    GameDb::open(&support(env).join("game_db")).unwrap().world_ids()
}

#[test]
fn read_and_write_pc() {
    let env = pc_env(&[(A, "Skyblock"), (B, "Beach")], true, false);
    let core = env.core();
    let pc = core.list_devices().into_iter().find(|d| d.kind == DeviceKind::Pc).unwrap();
    assert_eq!((pc.id.as_str(), pc.world_count), ("pc", Some(2)));
    let (ev, mut on) = events();
    let r = core.read("op", &ReadRequest { device_id: "pc".into(), password: String::new() }, &mut on).unwrap();
    assert_eq!(r.worlds.len(), 2);
    assert_eq!(ev.lock().unwrap().len(), 2);
    let saved = core.save_to_library(&r.read_id, &[A.to_string()]).unwrap();
    assert_eq!((saved[0].origin, saved[0].source_detail.clone()), (bhw_core::Origin::Pc, None));

    // Phone world → PC (new), PC world → PC again as a copy.
    core.read("op", &ReadRequest { device_id: "S1".into(), password: String::new() }, &mut |_| {}).unwrap();
    let plan = core.transfer_plan(&Source::Device { device_id: "S1".into() }, &[C.to_string()], "pc").unwrap();
    assert!(!plan.needs_phone);
    assert!(plan.backup_size > 0);
    let (ev, mut on) = events();
    let res = core.transfer("op", &TransferRequest { plan_id: plan.plan_id, choices: Default::default() }, &mut on).unwrap();
    assert_eq!((res.added.clone(), res.total), (vec!["Phone World".to_string()], 3));
    assert!(ev.lock().unwrap().iter().any(|p| p.stage == bhw_core::Stage::Writing));
    assert_eq!(pc_ids(&env), [A, B, C]);
    assert_eq!(registered(&env).len(), 3);
    let backup = res.backup_id.unwrap();
    let b = core.list_backups().unwrap();
    assert_eq!((b[0].reason, b[0].worlds, b[0].device.as_str()), (BackupReason::BeforeWrite, 2, "Blockheads PC"));

    let src = Source::Device { device_id: "pc".into() };
    let plan = core.transfer_plan(&src, &[A.to_string(), B.to_string()], "pc").unwrap();
    let choices = [(B.to_string(), Choice::Replace)].into_iter().collect();
    let res = core.transfer("op", &TransferRequest { plan_id: plan.plan_id, choices }, &mut |_| {}).unwrap();
    assert_eq!((res.kept_both, res.replaced), (vec!["Skyblock (2)".to_string()], vec!["Beach".to_string()]));
    assert_eq!(pc_ids(&env).len(), 4);

    // Restore the first backup: back to A and B only.
    let (ev, mut on) = events();
    core.restore_backup("op", &backup, "pc", &mut on).unwrap();
    assert_eq!(pc_ids(&env), [A, B]);
    let mut reg = registered(&env);
    reg.sort();
    assert_eq!(reg, [A, B]);
    assert!(ev.lock().unwrap().iter().any(|p| p.stage == bhw_core::Stage::Writing));
    assert!(core.list_backups().unwrap().iter().any(|b| b.reason == BackupReason::BeforeRestore));
    assert_eq!(core.list_activity()[0].kind, bhw_core::ActivityKind::Restore);

    // A phone backup restored to the PC (no game_db in it: one is built).
    let mut data = PhoneData::new(&[(D, "From Phone")]);
    data.game_db = false;
    let dir = core.lib().unwrap().inner.backups_dir();
    std::fs::write(dir.join("phone.ab"), phone_ab(&data, None)).unwrap();
    core.restore_backup("op", "phone.ab", "pc", &mut |_| {}).unwrap();
    assert_eq!(pc_ids(&env), [D]);
    assert_eq!(registered(&env), [D]);
    // An empty backup empties the PC.
    std::fs::write(dir.join("empty.ab"), phone_ab(&PhoneData { worlds: vec![], game_db: false, extra: vec![] }, None)).unwrap();
    let mut b = bhw_backup::BackupWriter::create(Box::new(std::fs::File::create(dir.join("bare.ab")).unwrap()), &Default::default()).unwrap();
    bhw_backup::TarWriter::new(&mut b).finish().unwrap();
    b.finish().unwrap();
    core.restore_backup("op", "bare.ab", "pc", &mut |_| {}).unwrap();
    assert!(pc_ids(&env).is_empty());
}

#[test]
fn pc_without_game_db_and_running_game() {
    let env = pc_env(&[(A, "One")], false, true);
    let core = env.core();
    core.read("op", &ReadRequest { device_id: "S1".into(), password: String::new() }, &mut |_| {}).unwrap();
    let plan = core.transfer_plan(&Source::Device { device_id: "S1".into() }, &[C.to_string()], "pc").unwrap();
    let e = core.transfer("op", &TransferRequest { plan_id: plan.plan_id, choices: Default::default() }, &mut |_| {}).unwrap_err();
    assert_eq!((e.code, e.extra.backup_id.clone()), (ErrorCode::GameRunning, None));
    let dir = core.lib().unwrap().inner.backups_dir();
    std::fs::write(dir.join("x.ab"), phone_ab(&PhoneData::new(&[]), None)).unwrap();
    assert_eq!(core.restore_backup("op", "x.ab", "pc", &mut |_| {}).unwrap_err().code, ErrorCode::GameRunning);

    // Not running, no game_db and no backup before writing.
    let env = pc_env(&[(A, "One")], false, false);
    let core = env.core();
    core.set_settings(serde_json::json!({ "backup_before_write": false }).as_object().unwrap()).unwrap();
    core.read("op", &ReadRequest { device_id: "S1".into(), password: String::new() }, &mut |_| {}).unwrap();
    let plan = core.transfer_plan(&Source::Device { device_id: "S1".into() }, &[C.to_string()], "pc").unwrap();
    let res = core.transfer("op", &TransferRequest { plan_id: plan.plan_id, choices: Default::default() }, &mut |_| {}).unwrap();
    assert_eq!(res.backup_id, None);
    assert_eq!(registered(&env), [C]);
}

#[test]
fn pc_folder_from_settings() {
    let env = pc_env(&[(A, "One")], true, false);
    let core = bhw_core::Core::new(bhw_core::CoreConfig { pc_locator: std::sync::Arc::new(|| None), ..env.config(true) });
    assert!(!core.list_devices().iter().any(|d| d.kind == DeviceKind::Pc));
    let e = core.read("op", &ReadRequest { device_id: "pc".into(), password: String::new() }, &mut |_| {}).unwrap_err();
    assert_eq!(e.code, ErrorCode::NotFound);
    let folder = env.pc.clone().unwrap().display().to_string();
    core.set_settings(serde_json::json!({ "pc_folder": folder }).as_object().unwrap()).unwrap();
    assert!(core.list_devices().iter().any(|d| d.kind == DeviceKind::Pc));
    // A folder where the game never ran is not offered.
    core.set_settings(serde_json::json!({ "pc_folder": env.path("nothing").display().to_string() }).as_object().unwrap()).unwrap();
    assert!(!core.list_devices().iter().any(|d| d.kind == DeviceKind::Pc));
}
