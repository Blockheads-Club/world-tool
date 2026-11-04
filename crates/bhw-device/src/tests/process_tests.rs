use crate::fake::{Reply, ScriptedRunner};
#[cfg(unix)]
use crate::process::SystemRunner;
use crate::process::{run_collect, Runner};
use std::io::{Read, Write};
use std::path::Path;
use std::time::Duration;

fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| x.to_string()).collect()
}

#[cfg(unix)]
#[test]
fn system_runner_collects_output_and_code() {
    let out = run_collect(
        &SystemRunner,
        Path::new("sh"),
        &s(&["-c", "echo hi; echo err >&2; exit 3"]),
        Duration::from_secs(10),
    )
    .unwrap()
    .unwrap();
    assert_eq!((out.code, out.stdout.as_str(), out.stderr.as_str()), (3, "hi\n", "err\n"));
}

#[cfg(unix)]
#[test]
fn system_runner_pipes_stdin_and_kills() {
    let child = SystemRunner.spawn(Path::new("cat"), &[], true).unwrap();
    let mut stdin = child.take_stdin().unwrap();
    assert!(child.take_stdin().is_none());
    stdin.write_all(b"abc").unwrap();
    drop(stdin);
    let mut out = String::new();
    child.take_stdout().unwrap().read_to_string(&mut out).unwrap();
    assert!(child.take_stdout().is_none());
    assert!(child.take_stderr().is_some());
    assert!(child.take_stderr().is_none());
    assert_eq!(out, "abc");
    let sleeper = SystemRunner.spawn(Path::new("sleep"), &s(&["5"]), false).unwrap();
    assert!(sleeper.take_stdin().is_none());
    assert_eq!(sleeper.try_wait().unwrap(), None);
    sleeper.kill();
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(sleeper.try_wait().unwrap(), Some(-1));
    sleeper.kill();
}

#[cfg(unix)]
#[test]
fn run_collect_times_out_and_reports_missing_program() {
    assert!(run_collect(&SystemRunner, Path::new("sleep"), &s(&["5"]), Duration::from_millis(50)).unwrap().is_none());
    let e = run_collect(&SystemRunner, Path::new("/nonexistent/adb"), &[], Duration::from_secs(1)).err().unwrap();
    assert_eq!(e.kind(), std::io::ErrorKind::NotFound);
}

#[test]
fn placeholder_streams() {
    let mut v = Vec::new();
    crate::process::empty_reader().read_to_end(&mut v).unwrap();
    assert!(v.is_empty());
    crate::process::null_writer().write_all(b"x").unwrap();
}

#[test]
fn scripted_runner_matches_and_records() {
    let r = ScriptedRunner::new();
    r.once("a b", Reply::ok("first").stderr("e").code(2)).on("a", Reply::ok("again"));
    let out = run_collect(&r, Path::new("adb"), &s(&["a", "b"]), Duration::from_secs(1)).unwrap().unwrap();
    assert_eq!((out.code, out.stdout.as_str(), out.stderr.as_str()), (2, "first", "e"));
    let out = run_collect(&r, Path::new("adb"), &s(&["a", "b"]), Duration::from_secs(1)).unwrap().unwrap();
    assert_eq!(out.stdout, "again");
    assert_eq!(r.lines(), vec!["a b", "a b"]);
    assert_eq!(r.calls()[0].program, "adb");
    assert!(r.spawn(Path::new("adb"), &s(&["zzz"]), false).is_err());
    let failing = ScriptedRunner::new();
    failing.on("x", Reply { spawn_error: Some(std::io::ErrorKind::NotFound), ..Reply::default() });
    assert!(failing.spawn(Path::new("adb"), &s(&["x"]), false).is_err());
}

#[test]
fn fake_child_streams_in_small_reads_and_blocks_stdin() {
    let r = ScriptedRunner::new();
    r.on("c", Reply::chunks(vec![b"abcdef".to_vec()], Duration::ZERO));
    r.on("h", Reply { stdin_limit: Some(2), ..Reply::hang() });
    let c = r.spawn(Path::new("adb"), &s(&["c"]), false).unwrap();
    let mut out = c.take_stdout().unwrap();
    let mut buf = [0u8; 4];
    assert_eq!(out.read(&mut buf).unwrap(), 4);
    assert_eq!(c.try_wait().unwrap(), None);
    let mut rest = Vec::new();
    out.read_to_end(&mut rest).unwrap();
    assert_eq!(rest, b"ef");
    assert_eq!(c.try_wait().unwrap(), Some(0));
    let h = r.spawn(Path::new("adb"), &s(&["h"]), true).unwrap();
    let mut stdin = h.take_stdin().unwrap();
    assert!(h.take_stdin().is_none());
    assert_eq!(stdin.write(b"xyz").unwrap(), 2);
    stdin.flush().unwrap();
    let h2 = h.clone();
    let t = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(50));
        h2.kill();
    });
    assert!(stdin.write(b"z").is_err());
    t.join().unwrap();
    assert_eq!(r.calls()[1].stdin_bytes(), b"xy");
    // Reads after a kill end the stream.
    assert_eq!(h.take_stdout().unwrap().read(&mut buf).unwrap(), 0);
    let hooked = ScriptedRunner::new();
    let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let s2 = seen.clone();
    hooked.on("k", Reply::ok("").then_hang().on_spawn(move |a| s2.lock().unwrap().extend(a.to_vec())));
    let k = hooked.spawn(Path::new("adb"), &s(&["k"]), false).unwrap();
    assert_eq!(*seen.lock().unwrap(), vec!["k".to_string()]);
    k.kill();
    assert_eq!(k.try_wait().unwrap(), Some(-1));
    // Replies built at spawn time see the arguments.
    let dynamic = ScriptedRunner::new();
    dynamic.on("d", Reply::dynamic(|a| Reply::ok(a.join("+"))));
    let d = dynamic.spawn(Path::new("adb"), &s(&["d", "x"]), false).unwrap();
    let mut out = String::new();
    d.take_stdout().unwrap().read_to_string(&mut out).unwrap();
    assert_eq!(out, "d+x");
}
