//! Checking the phone after `adb restore` (Android does not say when "Restore my data" was declined).

mod common;

use bhw_core::verify::{mismatch, Expect};
use bhw_core::{ErrorCode, ReadRequest, Source, Stage, TransferRequest};
use bhw_device::fake::Reply;
use common::*;

fn read(core: &bhw_core::Core, id: &str) -> bhw_core::ReadResult {
    core.read("op", &ReadRequest { device_id: id.into(), password: String::new() }, &mut |_| {}).unwrap()
}

#[test]
fn a_declined_restore_is_reported_and_can_be_retried() {
    // The first restore does nothing on the phone (declined), the retry works.
    let env = Env::with(
        &PhoneData::new(&[(A, "One")]),
        |r| {
            r.once(" restore ", Reply::ok(""));
        },
        |_| {},
    );
    let core = env.core();
    read(&core, "S1");
    let src = Source::Device { device_id: "S1".into() };
    let plan = core.transfer_plan(&src, &[A.to_string()], "S1").unwrap();
    let req = TransferRequest { plan_id: plan.plan_id.clone(), choices: Default::default() };
    let (ev, mut on) = events();
    let e = core.transfer("op", &req, &mut on).unwrap_err();
    assert_eq!(e.code, ErrorCode::RestoreDeclined);
    assert!(e.detail.as_deref().unwrap().contains("missing"), "{e:?}");
    assert!(e.extra.backup_id.is_some());
    assert_eq!(e.extra.device.as_deref(), Some("Pixel 8"));
    let stages: Vec<Stage> = ev.lock().unwrap().iter().map(|p| p.stage).collect();
    assert!(stages.contains(&Stage::Verifying) && stages.contains(&Stage::PhoneCheck), "{stages:?}");
    assert_eq!(env.phone.names(), ["One"]);
    assert_eq!(core.list_activity()[0].error, Some(ErrorCode::RestoreDeclined));
    // Nothing is known about the phone any more: a device source needs a new read.
    assert_eq!(core.source_worlds(&src).unwrap_err().code, ErrorCode::NotFound);
    // Read again, then retry the same plan: works, and the session is the checked phone state.
    read(&core, "S1");
    let res = core.transfer("op", &req, &mut |_| {}).unwrap();
    assert_eq!(res.kept_both, ["One (2)"]);
    assert_eq!(env.phone.names(), ["One", "One (2)"]);
    assert_eq!(core.source_worlds(&src).unwrap().len(), 2);
}

#[test]
fn a_declined_backup_restore_is_reported() {
    let env = Env::new(&PhoneData::new(&[(A, "One")]));
    let core = env.core();
    read(&core, "S1");
    let src = Source::Device { device_id: "S1".into() };
    let plan = core.transfer_plan(&src, &[A.to_string()], "S1").unwrap();
    let res = core.transfer("op", &TransferRequest { plan_id: plan.plan_id, choices: Default::default() }, &mut |_| {});
    let backup = res.unwrap().backup_id.unwrap();
    assert_eq!(env.phone.names(), ["One", "One (2)"]);
    // Declined: "One (2)" is still there.
    env.phone.decline.store(true, std::sync::atomic::Ordering::SeqCst);
    let e = core.restore_backup("op", &backup, "S1", &mut |_| {}).unwrap_err();
    assert_eq!(e.code, ErrorCode::RestoreDeclined);
    assert!(e.detail.unwrap().contains("still there"));
    core.restore_backup("op", &backup, "S1", &mut |_| {}).unwrap();
    assert_eq!(env.phone.names(), ["One"]);
}

#[test]
fn a_backup_without_manifest_is_put_into_a_fresh_backup() {
    // A Blockheads PC backup (no `_manifest`) restored to a phone.
    let mut env = Env::new(&PhoneData::new(&[(A, "Phone World")]));
    env.pc = Some(pc_dir(env.tmp.path(), &[(B, "PC World"), (C, "Other")], true));
    let core = env.core();
    let plan = core.transfer_plan(&Source::Library, &[], "pc").unwrap();
    core.transfer("op", &TransferRequest { plan_id: plan.plan_id, choices: Default::default() }, &mut |_| {}).unwrap();
    let pc_backup = core.list_backups().unwrap().into_iter().find(|b| b.device_id == "pc").expect("pc backup").id;
    let (ev, mut on) = events();
    core.restore_backup("op", &pc_backup, "S1", &mut on).unwrap();
    assert_eq!(env.phone.names(), ["Other", "PC World"]);
    let mut reg = env.phone.registered();
    reg.sort();
    assert_eq!(reg, [B, C]);
    assert!(env.phone.all_locked());
    let stages: Vec<Stage> = ev.lock().unwrap().iter().map(|p| p.stage).collect();
    assert!(stages.contains(&Stage::Preparing) && stages.contains(&Stage::Verifying));
    // An empty one (no saves at all) empties the phone's saves.
    let dir = core.lib().unwrap().inner.backups_dir();
    let pc = tempfile::tempdir().unwrap();
    bhw_core::backups::write_pc_backup(&bhw_device::PcInstall::at(pc.path()), &dir.join("bare.ab")).unwrap();
    assert!(!bhw_core::backups::has_manifest(&dir.join("bare.ab"), None).unwrap());
    core.restore_backup("op", "bare.ab", "S1", &mut |_| {}).unwrap();
    assert!(env.phone.names().is_empty());
}

#[test]
fn mismatches() {
    let saves = |worlds: Vec<bhw_world::WorldFiles>, registered: &[(&str, &str)]| bhw_world::backup::BackupSaves {
        prefix: PREFIX.to_string() + "saves/",
        package: None,
        domain: None,
        worlds,
        game_db: vec![bhw_world::WorldFile {
            path: "data.mdb".into(),
            data: bhw_backup::Payload::Bytes(game_db_bytes(registered)),
        }],
    };
    let a = client_world(A, "One");
    let ok = Expect { present: vec![a.clone()], absent: vec![B.into()], registered: vec![A.into()] };
    assert_eq!(mismatch(&ok, Some(&saves(vec![a.clone()], &[(A, "One")]))), None);
    assert!(mismatch(&ok, None).unwrap().contains("missing"));
    let other = client_world(A, "Changed");
    assert!(mismatch(&ok, Some(&saves(vec![other], &[(A, "One")]))).unwrap().contains("not replaced"));
    let b = client_world(B, "Two");
    assert!(mismatch(&ok, Some(&saves(vec![a.clone(), b], &[(A, "One")]))).unwrap().contains("still there"));
    assert!(mismatch(&ok, Some(&saves(vec![a], &[]))).unwrap().contains("world list"));
}
