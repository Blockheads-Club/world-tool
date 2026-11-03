use super::{ab_of, adb, good_ab, sh, tar_of};
use crate::error::DeviceError;
use crate::fake::{Reply, ScriptedRunner};
use crate::transfer::*;
use std::io::{self, Read, Write};
use std::time::Duration;

fn bu() -> AbOptions {
    AbOptions { method: AbMethod::BuStream, if_running: IfRunning::Ignore }
}
fn cmd() -> AbOptions {
    AbOptions { method: AbMethod::AdbCommand, if_running: IfRunning::Ignore }
}

/// Writes `data` to the file after `-f` when the scripted backup starts.
fn writes_file(data: Vec<u8>) -> impl Fn(&[String]) + Send + Sync + 'static {
    move |args: &[String]| {
        let i = args.iter().position(|a| a == "-f").unwrap();
        std::fs::write(&args[i + 1], &data).unwrap();
    }
}

fn run_backup(
    r: &ScriptedRunner,
    opts: AbOptions,
    cancel: &CancelToken,
) -> (crate::Result<BackupSummary>, Vec<u8>, Vec<Progress>) {
    let mut out = Vec::new();
    let mut seen = Vec::new();
    let res = adb(r).backup("S", &mut out, opts, &mut |p| seen.push(p), cancel);
    (res, out, seen)
}

#[test]
fn checks_backup_streams() {
    assert!(matches!(check_backup(b"", 0), Err(DeviceError::DeclinedOnPhone)));
    assert!(matches!(check_backup(b"ANDROID", 7), Err(DeviceError::EmptyBackup)));
    assert!(matches!(check_backup(b"error: x", 8), Err(DeviceError::NotABackup)));
    assert!(matches!(check_backup(b"ANDROID BACKUP\n5\n", 17), Err(DeviceError::EmptyBackup)));
    let empty = ab_of(&[0u8; 1024]);
    assert!(matches!(check_backup(&empty, empty.len() as u64), Err(DeviceError::EmptyBackup)));
    let good = good_ab();
    assert_eq!(check_backup(&good, good.len() as u64).unwrap(), false);
    // Only the start is known: fine.
    assert_eq!(check_backup(&good[..30], 1 << 20).unwrap(), false);
    let raw = b"ANDROID BACKUP\n5\n0\nnone\nabc".to_vec();
    assert_eq!(check_backup(&raw, raw.len() as u64).unwrap(), false);
    let raw = b"ANDROID BACKUP\n5\n0\nnone\n\0\0".to_vec();
    assert!(matches!(check_backup(&raw, raw.len() as u64), Err(DeviceError::EmptyBackup)));
    let enc = b"ANDROID BACKUP\n5\n1\nAES-256\nsalt\nck\n10000\niv\nkey\n".to_vec();
    let mut small = enc.clone();
    small.extend([1u8; 32]);
    assert!(matches!(check_backup(&small, small.len() as u64), Err(DeviceError::EmptyBackup)));
    let mut big = enc.clone();
    big.extend([1u8; 200]);
    assert_eq!(check_backup(&big, big.len() as u64).unwrap(), true);
    assert_eq!(check_backup(&enc, 1 << 20).unwrap(), true);
}

#[test]
fn backup_via_adb_command_tails_the_file() {
    let r = ScriptedRunner::new();
    r.on(
        "backup -f",
        Reply::ok("")
            .stderr("Now unlock your device and confirm the backup operation.\n")
            .on_spawn(writes_file(good_ab())),
    );
    let (res, out, seen) = run_backup(&r, cmd(), &CancelToken::new());
    let s = res.unwrap();
    assert_eq!(out, good_ab());
    assert_eq!((s.bytes, s.encrypted), (good_ab().len() as u64, false));
    assert_eq!(seen.first().unwrap().phase, Phase::WaitingForPhone);
    assert_eq!(seen.last().unwrap().phase, Phase::Done);
    assert!(r.lines()[0].ends_with("-noapk -noobb -noshared com.noodlecake.blockheads"));
}

#[test]
fn backup_via_bu_streams_stdout() {
    let ab = good_ab();
    let r = ScriptedRunner::new();
    r.on("exec-out bu backup", Reply::chunks(vec![ab[..10].to_vec(), ab[10..].to_vec()], Duration::from_millis(5)));
    let (res, out, seen) = run_backup(&r, bu(), &CancelToken::new());
    assert_eq!(res.unwrap().bytes, ab.len() as u64);
    assert_eq!(out, ab);
    assert!(seen.iter().any(|p| p.phase == Phase::Transferring && p.bytes == 10));
    let r = ScriptedRunner::new();
    r.on("exec-out", Reply { read_error: Some(io::ErrorKind::Interrupted), ..Reply::ok(good_ab()) });
    assert!(run_backup(&r, bu(), &CancelToken::new()).0.is_ok());
}

#[test]
fn backup_failures() {
    let cancel = CancelToken::new();
    // Nobody confirms.
    let r = ScriptedRunner::new();
    r.on("backup -f", Reply::hang());
    assert!(matches!(run_backup(&r, cmd(), &cancel).0, Err(DeviceError::ConfirmTimeout)));
    // Stream stops halfway.
    let r = ScriptedRunner::new();
    r.on("exec-out", Reply::ok(&good_ab()[..20]).then_hang());
    assert!(matches!(run_backup(&r, bu(), &cancel).0, Err(DeviceError::Disconnected)));
    // Cancelled.
    let r = ScriptedRunner::new();
    r.on("exec-out", Reply::hang());
    let c = CancelToken::new();
    c.cancel();
    assert!(c.is_cancelled());
    assert!(matches!(run_backup(&r, bu(), &c).0, Err(DeviceError::Cancelled)));
    // adb errors.
    let cases: Vec<(Reply, fn(&DeviceError) -> bool)> = vec![
        (Reply::fail(1, "error: device unauthorized.\n"), |e| matches!(e, DeviceError::Unauthorized { .. })),
        (Reply::ok(&good_ab()[..20]).code(1), |e| matches!(e, DeviceError::Disconnected)),
        (Reply::ok(&good_ab()[..20]).stderr("error: device 'S' not found"), |e| matches!(e, DeviceError::Disconnected)),
        (Reply::fail(1, "error: device 'S' not found"), |e| matches!(e, DeviceError::DeviceNotFound { .. })),
        (Reply::fail(3, "mystery"), |e| matches!(e, DeviceError::Command { code: 3, .. })),
        (Reply::ok(""), |e| matches!(e, DeviceError::DeclinedOnPhone)),
        (Reply::ok("ANDROID BACKUP\n5\n1\nnone\n"), |e| matches!(e, DeviceError::EmptyBackup)),
        (Reply { read_error: Some(io::ErrorKind::ConnectionReset), ..Reply::ok("ANDROID") }, |e| {
            matches!(e, DeviceError::Disconnected)
        }),
        (Reply { spawn_error: Some(io::ErrorKind::NotFound), ..Reply::default() }, |e| {
            matches!(e, DeviceError::AdbNotFound)
        }),
    ];
    for (reply, check) in cases {
        let r = ScriptedRunner::new();
        r.on("exec-out", reply);
        let e = run_backup(&r, bu(), &cancel).0.unwrap_err();
        assert!(check(&e), "{e:?}");
    }
}

struct Full(io::ErrorKind, bool);
impl Write for Full {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        if self.1 {
            Ok(b.len())
        } else {
            Err(self.0.into())
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        Err(self.0.into())
    }
}

#[test]
fn backup_disk_full_and_running_game() {
    let r = ScriptedRunner::new();
    r.on("exec-out", Reply::ok(good_ab()));
    let a = adb(&r);
    let e = a.backup("S", &mut Full(io::ErrorKind::StorageFull, false), bu(), &mut |_| {}, &CancelToken::new());
    assert!(matches!(e, Err(DeviceError::DiskFull)));
    let e = a.backup("S", &mut Full(io::ErrorKind::StorageFull, true), bu(), &mut |_| {}, &CancelToken::new());
    assert!(matches!(e, Err(DeviceError::DiskFull)));
    #[cfg(target_os = "linux")]
    {
        let mut full = std::fs::OpenOptions::new().write(true).open("/dev/full").unwrap();
        let e = a.backup("S", &mut full, bu(), &mut |_| {}, &CancelToken::new());
        assert!(matches!(e, Err(DeviceError::DiskFull)), "{e:?}");
    }
    let r = ScriptedRunner::new();
    r.on("pidof", sh("42\n", 0));
    r.on("force-stop", sh("", 0));
    r.on("exec-out", Reply::ok(good_ab()));
    let a = adb(&r);
    let fail = AbOptions { if_running: IfRunning::Fail, ..bu() };
    assert!(matches!(
        a.backup("S", &mut Vec::new(), fail, &mut |_| {}, &CancelToken::new()),
        Err(DeviceError::GameRunning)
    ));
    let stop = AbOptions { if_running: IfRunning::Stop, ..bu() };
    a.backup("S", &mut Vec::new(), stop, &mut |_| {}, &CancelToken::new()).unwrap();
    assert!(r.lines().iter().any(|l| l.contains("am force-stop com.noodlecake.blockheads")));
    assert_eq!(AbOptions::default().if_running, IfRunning::Fail);
}

#[test]
fn backup_to_file_cleans_up() {
    let dir = tempfile::tempdir().unwrap();
    let r = ScriptedRunner::new();
    r.once("exec-out", Reply::ok(good_ab()));
    r.on("exec-out", Reply::ok(""));
    let a = adb(&r);
    let p = dir.path().join("w.ab");
    a.backup_to_file("S", &p, bu(), &mut |_| {}, &CancelToken::new()).unwrap();
    assert_eq!(std::fs::read(&p).unwrap(), good_ab());
    assert!(a.backup_to_file("S", &p, bu(), &mut |_| {}, &CancelToken::new()).is_err());
    assert!(!p.exists());
    assert!(a.backup_to_file("S", &dir.path().join("no/such/x.ab"), bu(), &mut |_| {}, &CancelToken::new()).is_err());
}

struct BadRead(bool);
impl Read for BadRead {
    fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
        if std::mem::replace(&mut self.0, false) {
            Err(io::ErrorKind::Interrupted.into())
        } else {
            Err(io::ErrorKind::InvalidData.into())
        }
    }
}

#[test]
fn restore_via_adb_command() {
    let staged = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let s2 = staged.clone();
    let r = ScriptedRunner::new();
    r.on(
        "restore",
        Reply::ok("Now unlock your device and confirm the restore operation.\n").on_spawn(move |a| {
            *s2.lock().unwrap() = std::fs::read(a.last().unwrap()).unwrap();
        }),
    );
    let a = adb(&r);
    let mut seen = Vec::new();
    let n = a.restore("S", &mut &good_ab()[..], cmd(), &mut |p| seen.push(p), &CancelToken::new()).unwrap();
    assert_eq!(n, good_ab().len() as u64);
    assert_eq!(*staged.lock().unwrap(), good_ab());
    assert_eq!(seen[0].phase, Phase::Staging);
    assert!(seen.iter().any(|p| p.phase == Phase::WaitingForPhone && p.total == Some(n)));
    // Never finishes.
    let r = ScriptedRunner::new();
    r.on("restore", Reply::hang());
    let e = adb(&r).restore("S", &mut &b"x"[..], cmd(), &mut |_| {}, &CancelToken::new());
    assert!(matches!(e, Err(DeviceError::ConfirmTimeout)));
    // Cancelled while waiting.
    let c = CancelToken::new();
    let c2 = c.clone();
    let t = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(50));
        c2.cancel();
    });
    let e = adb(&r).restore("S", &mut &b"x"[..], cmd(), &mut |_| {}, &c);
    t.join().unwrap();
    assert!(matches!(e, Err(DeviceError::Cancelled)));
    // Cancelled while staging, input error, adb error.
    assert!(matches!(adb(&r).restore("S", &mut &b"x"[..], cmd(), &mut |_| {}, &c), Err(DeviceError::Cancelled)));
    assert!(matches!(
        adb(&r).restore("S", &mut BadRead(true), cmd(), &mut |_| {}, &CancelToken::new()),
        Err(DeviceError::Io(_))
    ));
    let r = ScriptedRunner::new();
    r.on("restore", Reply::fail(1, "error: device offline"));
    let e = adb(&r).restore("S", &mut &b"x"[..], cmd(), &mut |_| {}, &CancelToken::new());
    assert!(matches!(e, Err(DeviceError::Disconnected)), "{e:?}");
    let r = ScriptedRunner::new();
    r.on("pidof", sh("1", 0));
    let e = adb(&r).restore("S", &mut &b"x"[..], AbOptions::default(), &mut |_| {}, &CancelToken::new());
    assert!(matches!(e, Err(DeviceError::GameRunning)));
    let r = ScriptedRunner::new();
    r.on("restore", Reply { spawn_error: Some(io::ErrorKind::NotFound), ..Reply::default() });
    let e = adb(&r).restore("S", &mut &b"x"[..], cmd(), &mut |_| {}, &CancelToken::new());
    assert!(matches!(e, Err(DeviceError::AdbNotFound)));
}

#[test]
fn restore_via_bu_streams_stdin() {
    let r = ScriptedRunner::new();
    r.on("exec-in bu restore", Reply::default());
    let a = adb(&r);
    let mut seen = Vec::new();
    let n = a.restore("S", &mut &good_ab()[..], bu(), &mut |p| seen.push(p), &CancelToken::new()).unwrap();
    assert_eq!(n, good_ab().len() as u64);
    assert_eq!(r.calls()[0].stdin_bytes(), good_ab());
    assert!(seen.iter().any(|p| p.phase == Phase::Transferring));
    // Phone never reads: confirm timeout; reads some then stops: disconnected.
    let r = ScriptedRunner::new();
    r.on("exec-in", Reply { stdin_limit: Some(0), ..Reply::hang() });
    assert!(matches!(
        adb(&r).restore("S", &mut &b"x"[..], bu(), &mut |_| {}, &CancelToken::new()),
        Err(DeviceError::ConfirmTimeout)
    ));
    let r = ScriptedRunner::new();
    r.on("exec-in", Reply { stdin_limit: Some(3), ..Reply::hang() });
    let big = [1u8; 100];
    assert!(matches!(
        adb(&r).restore("S", &mut &big[..], bu(), &mut |_| {}, &CancelToken::new()),
        Err(DeviceError::Disconnected)
    ));
    // All sent but adb never exits.
    let r = ScriptedRunner::new();
    r.on("exec-in", Reply::hang());
    let mut a = adb(&r);
    a.timeouts.stall = Duration::from_secs(10);
    assert!(matches!(
        a.restore("S", &mut &b"x"[..], bu(), &mut |_| {}, &CancelToken::new()),
        Err(DeviceError::ConfirmTimeout)
    ));
    // Local read error.
    let r = ScriptedRunner::new();
    r.on("exec-in", Reply::default());
    assert!(matches!(
        adb(&r).restore("S", &mut BadRead(false), bu(), &mut |_| {}, &CancelToken::new()),
        Err(DeviceError::Io(_))
    ));
}

#[test]
fn copy_maps_errors() {
    let c = CancelToken::new();
    let e = copy_cancellable(&mut &b"x"[..], &mut Full(io::ErrorKind::BrokenPipe, false), &c, &mut |_| {});
    assert!(matches!(e, Err(DeviceError::Disconnected)));
    let e = copy_cancellable(&mut &b"x"[..], &mut Full(io::ErrorKind::StorageFull, true), &c, &mut |_| {});
    assert!(matches!(e, Err(DeviceError::DiskFull)));
    let _ = tar_of(&[]);
}
