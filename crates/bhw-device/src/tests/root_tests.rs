use super::{adb, sh, tar_of};
use crate::adb::RootStyle;
use crate::error::DeviceError;
use crate::fake::{Reply, ScriptedRunner};
use crate::root::*;
use crate::transfer::{CancelToken, IfRunning};
use std::io::Read;
use std::time::Duration;

const SUP: &str = "/data/data/com.noodlecake.blockheads/files/Library/Application Support";
const ID: &str = "0123456789abcdef0123456789abcdef";

#[test]
fn parses_world_db_lists() {
    let text = format!(
        "{SUP}/saves/{ID}/world_db\n{SUP}/saves/{ID}/world_db\n{SUP}/saves/notahexid/world_db\n\
         /x/saves/ffffffffffffffffffffffffffffffff/world_db\n{SUP}/saves/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/world_db\n\
         /other/world_db\nworld_db/world_db\n/a/b/{ID}/world_db\nnoise\n"
    );
    let s = parse_world_dbs(&text).unwrap();
    assert_eq!(s.support_dir, SUP);
    assert_eq!(s.saves_dir, format!("{SUP}/saves"));
    assert_eq!(s.worlds, vec!["0123456789abcdef0123456789abcdef".to_string(), "a".repeat(32)]);
    assert!(parse_world_dbs("nothing").is_none());
    assert!(is_world_id(ID) && !is_world_id("xyz"));
}

#[test]
fn finds_saves() {
    let r = ScriptedRunner::new();
    r.on("find", sh(&format!("{SUP}/saves/{ID}/world_db\n"), 0));
    let s = adb(&r).root_find_saves("S", RootStyle::SuDashC).unwrap();
    assert_eq!(s.worlds, vec![ID.to_string()]);
    assert!(r.lines()[0].contains("su -c"));
    let r = ScriptedRunner::new();
    r.on("find", sh("", 0));
    r.once("[ -d", sh("yes\n", 0));
    r.on("[ -d", sh("", 0));
    let a = adb(&r);
    let s = a.root_find_saves("S", RootStyle::AdbRoot).unwrap();
    assert_eq!((s.support_dir.as_str(), s.worlds.len()), (SUP, 0));
    assert!(matches!(a.root_find_saves("S", RootStyle::AdbRoot), Err(DeviceError::SavesNotFound)));
    let r = ScriptedRunner::new();
    r.on("find", sh("Permission denied", 1));
    assert!(matches!(adb(&r).root_find_saves("S", RootStyle::SuDashC), Err(DeviceError::RootUnavailable)));
    let r = ScriptedRunner::new();
    r.on("find", Reply::hang());
    assert!(matches!(adb(&r).root_find_saves("S", RootStyle::SuDashC), Err(DeviceError::RootUnavailable)));
    let r = ScriptedRunner::new();
    r.on("find", Reply::fail(1, "error: device unauthorized."));
    assert!(matches!(adb(&r).root_find_saves("S", RootStyle::SuDashC), Err(DeviceError::Unauthorized { .. })));
}

fn pull(r: &ScriptedRunner, dest: &std::path::Path, cancel: &CancelToken) -> crate::Result<usize> {
    let mut last = 0;
    let n = adb(r).root_pull("S", RootStyle::SuDashC, SUP, "saves", dest, &mut |p| last = p.bytes, cancel);
    if n.is_ok() {
        assert!(last > 0);
    }
    n
}

#[test]
fn pulls_with_tar() {
    let dir = tempfile::tempdir().unwrap();
    let tar = tar_of(&[(&format!("saves/{ID}/world_db/data.mdb"), b"mdb"), ("../evil", b"x"), ("saves/game", b"g")]);
    let r = ScriptedRunner::new();
    r.on("exec-out", Reply::ok(tar));
    assert_eq!(pull(&r, dir.path(), &CancelToken::new()).unwrap(), 2);
    assert_eq!(std::fs::read(dir.path().join(format!("saves/{ID}/world_db/data.mdb"))).unwrap(), b"mdb");
    assert!(r.lines()[0].contains("tar -C"));
    let mut b = tar::Builder::new(Vec::new());
    let mut h = tar::Header::new_gnu();
    h.set_entry_type(tar::EntryType::Directory);
    h.set_size(0);
    h.set_mode(0o755);
    h.set_cksum();
    b.append_data(&mut h, "saves/", &[][..]).unwrap();
    let dirs_only = b.into_inner().unwrap();
    let cases: Vec<(Reply, fn(&DeviceError) -> bool)> = vec![
        (Reply::ok(""), |e| matches!(e, DeviceError::RootUnavailable)),
        (Reply::ok(dirs_only), |e| matches!(e, DeviceError::SavesNotFound)),
        (Reply::ok(vec![7u8; 1024]), |e| matches!(e, DeviceError::Disconnected)),
        (Reply::fail(1, "error: device offline"), |e| matches!(e, DeviceError::Offline { .. })),
    ];
    for (reply, check) in cases {
        let r = ScriptedRunner::new();
        r.on("exec-out", reply);
        let e = pull(&r, dir.path(), &CancelToken::new()).unwrap_err();
        assert!(check(&e), "{e:?}");
    }
    let r = ScriptedRunner::new();
    r.on("exec-out", Reply::hang());
    let c = CancelToken::new();
    c.cancel();
    assert!(matches!(pull(&r, dir.path(), &c), Err(DeviceError::Cancelled)));
    let file = dir.path().join("file");
    std::fs::write(&file, b"").unwrap();
    assert!(pull(&r, &file.join("sub"), &c).is_err());
    let r = ScriptedRunner::new();
    r.on("exec-out", Reply { spawn_error: Some(std::io::ErrorKind::NotFound), ..Reply::default() });
    assert!(matches!(pull(&r, dir.path(), &c), Err(DeviceError::AdbNotFound)));
    #[cfg(unix)]
    {
        // A file in the stream that cannot be written locally.
        use std::os::unix::fs::PermissionsExt;
        let ro = dir.path().join("ro");
        std::fs::create_dir(&ro).unwrap();
        std::fs::set_permissions(&ro, std::fs::Permissions::from_mode(0o500)).unwrap();
        let r = ScriptedRunner::new();
        r.on("exec-out", Reply::ok(tar_of(&[("f", b"x")])));
        let e = pull(&r, &ro, &CancelToken::new());
        std::fs::set_permissions(&ro, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert!(e.is_err());
    }
}

fn world_dir() -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(d.path().join("world_db")).unwrap();
    std::fs::write(d.path().join("world_db/data.mdb"), b"data").unwrap();
    d
}

fn push(r: &ScriptedRunner, src: &std::path::Path) -> crate::Result<u64> {
    adb(r).root_push(
        "S",
        RootStyle::SuDashC,
        src,
        SUP,
        &format!("saves/{ID}"),
        IfRunning::Ignore,
        &mut |_| {},
        &CancelToken::new(),
    )
}

#[test]
fn pushes_with_tar_and_fixes_owner() {
    let src = world_dir();
    let r = ScriptedRunner::new();
    r.on("stat -c", sh("10234:10234\n", 0));
    r.on("exec-in", Reply::default());
    r.on("chown -R", sh("", 0));
    let n = push(&r, src.path()).unwrap();
    let calls = r.calls();
    let sent = calls[1].stdin_bytes();
    assert_eq!(n, sent.len() as u64);
    let mut names = Vec::new();
    for e in tar::Archive::new(&sent[..]).entries().unwrap() {
        let mut e = e.unwrap();
        names.push(e.path().unwrap().display().to_string());
        let mut v = Vec::new();
        e.read_to_end(&mut v).unwrap();
    }
    assert!(names.contains(&"x/world_db/data.mdb".to_string()), "{names:?}");
    let fin = calls[2].line();
    assert!(fin.contains("chown -R 10234:10234") && fin.contains("restorecon -R") && fin.contains("chcon -R"));
    // `exec-in` can return before the phone has unpacked everything: tar's exit code is awaited before moving.
    assert!(calls[1].line().contains("echo $? > ") && calls[1].line().contains(".bhw-incoming.done"));
    let (wait, mv) = (fin.find("while [ ! -s ").unwrap(), fin.find("mv ").unwrap());
    assert!(wait < mv && fin.contains("= 0 ] ||"), "{fin}");
    // Failures.
    assert!(matches!(push(&r, &src.path().join("missing")), Err(DeviceError::PathNotFound(_))));
    let r2 = ScriptedRunner::new();
    r2.on("pidof", sh("5", 0));
    let e = adb(&r2).root_push(
        "S",
        RootStyle::SuDashC,
        src.path(),
        SUP,
        "saves",
        IfRunning::Fail,
        &mut |_| {},
        &CancelToken::new(),
    );
    assert!(matches!(e, Err(DeviceError::GameRunning)));
    let r = ScriptedRunner::new();
    r.on("stat -c", sh("", 1));
    assert!(matches!(push(&r, src.path()), Err(DeviceError::RootUnavailable)));
    let r = ScriptedRunner::new();
    r.on("stat -c", sh("1:1\n", 0));
    r.on("exec-in", Reply::default());
    r.on("chown -R", sh("mv: failed", 1));
    assert!(matches!(push(&r, src.path()), Err(DeviceError::Command { code: 1, .. })));
    let r = ScriptedRunner::new();
    r.on("stat -c", sh("1:1\n", 0));
    r.on("exec-in", Reply::fail(1, "error: device unauthorized."));
    assert!(matches!(push(&r, src.path()), Err(DeviceError::Unauthorized { .. })));
    let r = ScriptedRunner::new();
    r.on("stat -c", sh("1:1\n", 0));
    r.on("exec-in", Reply { stdin_limit: Some(10), ..Reply::hang() });
    assert!(matches!(push(&r, src.path()), Err(DeviceError::Disconnected)));
    let r = ScriptedRunner::new();
    r.on("stat -c", sh("1:1\n", 0));
    r.on("exec-in", Reply::hang());
    let mut a = adb(&r);
    a.timeouts.stall = Duration::from_secs(10);
    let e = a.root_push(
        "S",
        RootStyle::SuDashC,
        src.path(),
        SUP,
        "saves",
        IfRunning::Ignore,
        &mut |_| {},
        &CancelToken::new(),
    );
    assert!(matches!(e, Err(DeviceError::Disconnected)));
    let r = ScriptedRunner::new();
    r.on("stat -c", sh("1:1\n", 0));
    r.on("exec-in", Reply { spawn_error: Some(std::io::ErrorKind::NotFound), ..Reply::default() });
    assert!(matches!(push(&r, src.path()), Err(DeviceError::AdbNotFound)));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let bad = world_dir();
        let f = bad.path().join("world_db/data.mdb");
        std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o000)).unwrap();
        let r = ScriptedRunner::new();
        r.on("stat -c", sh("1:1\n", 0));
        r.on("exec-in", Reply::default());
        assert!(matches!(push(&r, bad.path()), Err(DeviceError::Io(_))));
        std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o644)).unwrap();
    }
}
