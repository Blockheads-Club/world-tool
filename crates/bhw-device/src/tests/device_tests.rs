use super::{adb, sh};
use crate::adb::RootStyle;
use crate::devices::*;
use crate::error::DeviceError;
use crate::fake::{Reply, ScriptedRunner};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const LIST: &str = "* daemon not running; starting now at tcp:5037\n\
List of devices attached\n\
R58M123 device usb:1-1 product:beyond1 model:SM_G973F device:beyond1 transport_id:1\n\
192.168.1.5:5555 device product:p model:Pixel_7 device:panther transport_id:2\n\
adb-R58M-abc._adb-tls-connect._tcp unauthorized transport_id:3\n\
0123 no permissions (missing udev rules? user is in the plugdev group); see [http://developer.android.com/tools/device.html] usb:3-1 transport_id:4\n\
emulator-5554 offline\n\
X1 recovery extra:token\n\
\n\
lonely\n\
Z device usb:2-1 junk\n";

const DETAILS: &str = "model=SM_G973F\nrelease=12\nsdk=31\n    versionCode=146 minSdk=9 targetSdk=27\n    \
userId=10234\n    dataDir=/data/user/0/com.noodlecake.blockheads\n    versionName=1.7.6\n    versionName=9\n";

#[test]
fn parses_device_list() {
    let d = parse_devices(LIST);
    assert_eq!(d.len(), 7);
    assert_eq!(d[6].usb.as_deref(), Some("2-1"));
    assert_eq!(d[0].serial, "R58M123");
    assert_eq!(d[0].state, DeviceState::Device);
    assert_eq!(d[0].connection, Connection::Usb);
    assert_eq!(d[0].model.as_deref(), Some("SM_G973F"));
    assert_eq!(d[0].usb.as_deref(), Some("1-1"));
    assert_eq!(d[0].transport_id.as_deref(), Some("1"));
    assert_eq!(d[1].connection, Connection::Wifi);
    assert_eq!(d[1].product.as_deref(), Some("p"));
    assert_eq!((d[2].connection, &d[2].state), (Connection::Wifi, &DeviceState::Unauthorized));
    assert!(matches!(&d[3].state, DeviceState::NoPermissions(m) if m.contains("udev")));
    assert_eq!((d[4].connection, &d[4].state), (Connection::Emulator, &DeviceState::Offline));
    assert_eq!(d[5].state, DeviceState::Other("recovery extra:token".into()));
    assert_eq!(Connection::from_serial(":5555"), Connection::Usb);
    assert_eq!(Connection::from_serial("host:abc"), Connection::Usb);
}

#[test]
fn versions() {
    for (v, ok) in [
        ("1.7.4", true),
        ("1.7.6", true),
        ("1.7.5-beta", true),
        ("1.7.3", false),
        ("1.7", false),
        ("1.8.0", false),
        ("x", false),
    ] {
        assert_eq!(version_supported(v), ok, "{v}");
    }
    let g = GameInfo { version_name: "1.7.2".into(), version_code: None, uid: None, data_dir: None, stopped: false };
    assert!(matches!(g.check_supported(), Err(DeviceError::UnsupportedVersion { found }) if found == "1.7.2"));
    assert!(GameInfo { version_name: "1.7.6".into(), ..g }.check_supported().is_ok());
}

fn scripted() -> ScriptedRunner {
    let r = ScriptedRunner::new();
    r.on("devices -l", Reply::ok(LIST));
    r.on("-s R58M123 shell (echo", sh(DETAILS, 0));
    r.on("-s 192.168.1.5:5555 shell (echo", sh("model=\nrelease=\nsdk=\n", 0));
    r
}

#[test]
fn details_and_game() {
    let r = scripted();
    r.on("pidof", sh("1234\n", 0));
    let a = adb(&r);
    let d = a.device_details("R58M123").unwrap();
    assert_eq!(d.model, "SM G973F");
    assert_eq!((d.android_release.as_str(), d.sdk), ("12", Some(31)));
    let g = d.game.unwrap();
    assert_eq!(g.version_name, "1.7.6");
    assert_eq!((g.version_code, g.uid), (Some(146), Some(10234)));
    assert_eq!(g.data_dir.as_deref(), Some("/data/user/0/com.noodlecake.blockheads"));
    assert!(g.supported());
    assert!(matches!(a.game_info("192.168.1.5:5555"), Err(DeviceError::GameNotInstalled)));
    assert!(a.game_info("R58M123").is_ok());
    assert!(a.game_running("R58M123").unwrap());
    assert!(!g.stopped);
    r.on("keyevent HOME", sh("", 0));
    r.on("am kill", sh("", 0));
    r.on("force-stop", sh("", 0));
    assert!(matches!(a.stop_game("R58M123"), Err(DeviceError::GameRunning)));
    a.force_stop_game("R58M123").unwrap();
    let s = ScriptedRunner::new();
    s.on("keyevent HOME", sh("", 0));
    s.on("am kill", sh("", 0));
    s.once("pidof", sh("", 1));
    adb(&s).stop_game("S").unwrap();
    assert!(s.lines().iter().all(|l| !l.contains("force-stop")));
    let s = ScriptedRunner::new();
    s.on("keyevent", sh("", 1));
    assert!(adb(&s).stop_game("S").is_err());
    let s = ScriptedRunner::new();
    s.on("keyevent", sh("", 0));
    s.on("am kill", sh("", 1));
    assert!(adb(&s).stop_game("S").is_err());
    let s = ScriptedRunner::new();
    s.on("shell (echo", sh("    versionName=1.7.6\n    User 0: installed=true stopped=true notLaunched=false\n", 0));
    assert!(adb(&s).game_info("S").unwrap().stopped);
    let r = ScriptedRunner::new();
    r.on("pidof", sh("", 1));
    assert!(!adb(&r).game_running("S").unwrap());
    r.on("devices", Reply::fail(1, "boom"));
    assert!(matches!(adb(&r).list_devices(), Err(DeviceError::Command { .. })));
}

#[test]
fn probes_root() {
    let r = ScriptedRunner::new();
    r.on("shell (id)", sh("uid=2000(shell)", 0));
    r.on("su -c", sh("uid=0(root) gid=0(root)", 0));
    assert_eq!(adb(&r).probe_root("S").unwrap(), Some(RootStyle::SuDashC));
    let r = ScriptedRunner::new();
    r.on("shell (id)", sh("uid=0(root)", 0));
    assert_eq!(adb(&r).probe_root("S").unwrap(), Some(RootStyle::AdbRoot));
    let r = ScriptedRunner::new();
    r.on("shell (id)", sh("uid=2000(shell)", 0));
    r.on("su -c", Reply::hang());
    r.on("su 0", sh("uid=0(root)", 0));
    assert_eq!(adb(&r).probe_root("S").unwrap(), Some(RootStyle::SuUid));
    let r = ScriptedRunner::new();
    r.on("shell (id)", sh("uid=2000(shell)", 0));
    r.on("su", sh("/system/bin/sh: su: not found", 127));
    assert_eq!(adb(&r).probe_root("S").unwrap(), None);
    let r = ScriptedRunner::new();
    r.on("shell", Reply::fail(1, "error: device offline"));
    assert!(matches!(adb(&r).probe_root("S"), Err(DeviceError::Offline { .. })));
}

#[test]
fn discovers() {
    let r = scripted();
    r.on("-s R58M123 shell (su", sh("uid=0(root)", 0));
    r.on("-s R58M123 shell (id", sh("uid=2000(shell)", 0));
    r.on("-s 192.168.1.5:5555 shell", sh("", 1));
    r.on("-s Z shell", Reply::fail(1, "error: device 'Z' not found"));
    let a = adb(&r);
    let devs = a.discover(true).unwrap();
    assert_eq!(devs.len(), 7);
    assert_eq!(devs[0].root, Some(Some(RootStyle::SuDashC)));
    assert!(devs[0].details.as_ref().unwrap().game.is_some());
    assert_eq!(devs[1].root, Some(None));
    assert!(devs[2].details.is_none() && devs[2].root.is_none());
    let r = ScriptedRunner::new();
    r.on("devices", Reply::ok("A device\nB device\n"));
    r.on("-s A shell", Reply::fail(1, "error: device 'A' not found"));
    r.on("-s B shell", Reply::fail(1, "weird failure"));
    let devs = adb(&r).discover(false).unwrap();
    assert!(devs.iter().all(|d| d.details.is_none()));
    let r = ScriptedRunner::new();
    r.on("devices", Reply::ok("C device\n"));
    r.on("-s C shell", Reply::fail(1, "error: device unauthorized."));
    assert!(matches!(adb(&r).discover(false), Err(DeviceError::Unauthorized { .. })));
}

#[test]
fn watcher_reports_changes() {
    let r = ScriptedRunner::new();
    r.once("devices", Reply::ok("A device\nB unauthorized\n"));
    r.once("devices", Reply::ok("A device\nB device\n"));
    r.once("devices", Reply::ok("B device\n"));
    r.once("devices", Reply::ok("B device\nC device\n"));
    r.on("-s A shell", sh("model=a\n", 0));
    r.on("-s B shell", sh("model=b\n", 0));
    r.on("-s C shell", Reply::fail(1, "error: device unauthorized."));
    let mut w = DeviceWatcher::new(adb(&r));
    let ev = w.poll().unwrap();
    assert!(
        matches!(&ev[..], [DeviceEvent::Added(a), DeviceEvent::Added(b)] if a.listed.serial == "A" && b.details.is_none())
    );
    let ev = w.poll().unwrap();
    assert!(matches!(&ev[..], [DeviceEvent::Changed(b)] if b.details.as_ref().unwrap().model == "b"));
    assert_eq!(w.poll().unwrap(), vec![DeviceEvent::Removed("A".into())]);
    assert_eq!(w.devices().len(), 1);
    assert!(w.poll().is_err());
    let r = ScriptedRunner::new();
    r.once("devices", Reply::ok("A device\n"));
    r.once("devices", Reply::ok("A unauthorized\n"));
    r.on("-s A shell", Reply::fail(1, "error: device offline"));
    let mut w = DeviceWatcher::new(adb(&r));
    w.poll().unwrap();
    r.once("-s A shell", Reply::fail(1, "error: device unauthorized."));
    // A changed listing whose description fails.
    let r2 = ScriptedRunner::new();
    r2.once("devices", Reply::ok("A offline\n"));
    r2.once("devices", Reply::ok("A device\n"));
    r2.on("-s A shell", Reply::fail(1, "error: device unauthorized."));
    let mut w2 = DeviceWatcher::new(adb(&r2));
    w2.poll().unwrap();
    assert!(w2.poll().is_err());
}

#[test]
fn watcher_thread_delivers_and_stops() {
    let r = ScriptedRunner::new();
    r.once("devices", Reply::ok("A offline\n"));
    r.on("devices", Reply::fail(1, "x"));
    let got = Arc::new(Mutex::new(Vec::new()));
    let g = got.clone();
    let h = DeviceWatcher::new(adb(&r)).spawn(Duration::from_millis(30), move |ev| g.lock().unwrap().push(ev.is_ok()));
    // Wait for the first event plus at least one error; slow CI runners need more than a fixed sleep.
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while got.lock().unwrap().len() < 3 && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    h.stop();
    let got = got.lock().unwrap();
    assert_eq!(got[0], true);
    assert!(got[1..].iter().all(|ok| !ok) && got.len() > 1);
    let quiet = ScriptedRunner::new();
    quiet.on("devices", Reply::ok(""));
    drop(DeviceWatcher::new(adb(&quiet)).spawn(Duration::from_millis(10), |_| panic!("no events expected")));
}
