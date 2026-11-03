use super::{adb, sh};
use crate::adb::{find_adb, sh_quote, Adb, RootStyle};
use crate::error::DeviceError;
use crate::fake::{Reply, ScriptedRunner};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[test]
fn finds_adb_in_order() {
    let name = crate::adb::adb_file_name();
    let env = |k: &str| -> Option<OsString> {
        match k {
            "ANDROID_HOME" => Some("".into()),
            "ANDROID_SDK_ROOT" => Some("/sdk".into()),
            "PATH" => Some(std::env::join_paths(["/a", "/b"]).unwrap()),
            _ => None,
        }
    };
    let sdk = Path::new("/sdk").join("platform-tools").join(name);
    let b = Path::new("/b").join(name);
    assert_eq!(
        find_adb(Some(Path::new("/side/adb")), &env, &|p| p == Path::new("/side/adb")),
        Some("/side/adb".into())
    );
    assert_eq!(find_adb(Some(Path::new("/side/adb")), &env, &|p| p == sdk), Some(sdk.clone()));
    assert_eq!(find_adb(None, &env, &|p| p == b), Some(b.clone()));
    assert_eq!(find_adb(None, &|_| None, &|_| true), None);
    match Adb::locate(None) {
        Ok(a) => assert!(a.program().is_file()),
        Err(e) => assert!(matches!(e, DeviceError::AdbNotFound)),
    }
    assert!(matches!(Adb::locate(Some(Path::new("/nonexistent"))), Ok(_) | Err(DeviceError::AdbNotFound)));
}

#[test]
fn quoting_and_root_wrapping() {
    assert_eq!(sh_quote("a'b c"), r"'a'\''b c'");
    assert_eq!(RootStyle::AdbRoot.wrap("id"), "id");
    assert_eq!(RootStyle::SuDashC.wrap("id -u"), "su -c 'id -u'");
    assert_eq!(RootStyle::SuUid.wrap("id"), "su 0 sh -c 'id'");
}

#[test]
fn runs_commands_and_shell() {
    let r = ScriptedRunner::new();
    r.on("version", Reply::ok("Android Debug Bridge version 1.0.41\nVersion 37\n"));
    r.on("start-server", Reply::ok(""));
    r.on("kill-server", Reply::ok(""));
    r.on("shell (true)", sh("out\r\n", 0));
    r.on("shell (false)", sh("", 1));
    r.on("shell (weird)", Reply::ok("__BHW_RC=x"));
    r.on("shell (auth)", Reply::fail(1, "error: device unauthorized.\n"));
    r.on("shell (other)", Reply::fail(1, "something odd"));
    r.on("hang", Reply::hang());
    let a = adb(&r);
    assert!(format!("{a:?}").contains("adb"));
    assert_eq!(a.runner().spawn(Path::new("adb"), &["version".into()], false).is_ok(), true);
    assert_eq!(a.version().unwrap(), "Android Debug Bridge version 1.0.41");
    a.start_server().unwrap();
    a.kill_server().unwrap();
    let out = a.shell("S", "true").unwrap();
    assert_eq!((out.code, out.stdout.as_str()), (0, "out\n"));
    assert_eq!(a.shell_ok("S", "true").unwrap(), "out\n");
    assert!(matches!(a.shell_ok("S", "false"), Err(DeviceError::Command { code: 1, .. })));
    assert_eq!(a.shell("S", "weird").unwrap().code, -1);
    assert!(matches!(a.shell("S", "auth"), Err(DeviceError::Unauthorized { .. })));
    assert!(matches!(a.shell("S", "other"), Err(DeviceError::Command { code: 1, .. })));
    assert!(matches!(a.run_timeout(&["hang".into()], Duration::from_millis(30)), Err(DeviceError::Timeout)));
    assert_eq!(r.calls()[4].args[..3], ["-s".to_string(), "S".into(), "shell".into()]);
}

#[test]
fn spawn_errors_map() {
    assert!(matches!(Adb::spawn_error(std::io::ErrorKind::NotFound.into()), DeviceError::AdbNotFound));
    assert!(matches!(Adb::spawn_error(std::io::ErrorKind::Other.into()), DeviceError::Io(_)));
    let a = Adb::new(PathBuf::from("/nonexistent/adb"), std::sync::Arc::new(crate::process::SystemRunner));
    assert!(matches!(a.run(&["version"]), Err(DeviceError::AdbNotFound)));
}
