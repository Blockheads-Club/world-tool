use crate::devices::{parse_devices, parse_key_values};
use crate::error::*;
use crate::hints::*;
use std::io;

#[test]
fn errors_classify() {
    assert!(matches!(local_write_error(io::ErrorKind::StorageFull.into()), DeviceError::DiskFull));
    assert!(matches!(
        local_write_error(io::Error::from_raw_os_error(if cfg!(windows) { 112 } else { 28 })),
        DeviceError::DiskFull
    ));
    assert!(matches!(
        local_write_error(io::Error::from_raw_os_error(if cfg!(windows) { 28 } else { 112 })),
        DeviceError::Io(_)
    ));
    assert!(matches!(local_write_error(io::ErrorKind::Other.into()), DeviceError::Io(_)));
    let c = |t| classify_adb_message(t, "S");
    assert!(matches!(c("error: device unauthorized."), Some(DeviceError::Unauthorized { .. })));
    assert!(matches!(c("error: insufficient permissions for device"), Some(DeviceError::NoPermissions { .. })));
    assert!(matches!(c("error: device offline"), Some(DeviceError::Offline { .. })));
    assert!(matches!(c("error: device 'S' not found"), Some(DeviceError::DeviceNotFound { .. })));
    assert!(matches!(c("adb: no devices/emulators found"), Some(DeviceError::DeviceNotFound { .. })));
    assert!(matches!(c("error: closed"), Some(DeviceError::Disconnected)));
    assert!(c("all good").is_none());
    assert_eq!(
        DeviceError::UnsupportedVersion { found: "1.6".into() }.to_string(),
        "The Blockheads 1.6 is not supported (supported: 1.7.4 to 1.7.6)"
    );
}

#[test]
fn hints() {
    let list = parse_devices("A unauthorized\nB offline\nC no permissions (udev)\nD device\nE no permissions x\n");
    assert_eq!(
        hints_for(HostOs::Linux, &list),
        vec![Hint::AcceptRsaPrompt, Hint::ReconnectOffline, Hint::LinuxUdevRules]
    );
    assert_eq!(hints_for(HostOs::Windows, &list[2..3]), vec![Hint::WindowsUsbDriver]);
    assert_eq!(hints_for(HostOs::Windows, &[]), vec![Hint::EnableUsbDebugging, Hint::WindowsUsbDriver]);
    assert_eq!(hints_for(HostOs::MacOs, &[]), vec![Hint::EnableUsbDebugging]);
    let _ = HostOs::current();
    assert!(is_server_version_conflict("adb server version (41) doesn't match this client (39); killing..."));
    assert!(!is_server_version_conflict("ok"));
    assert_eq!(parse_key_values("a=1 a=2 b=3 c").len(), 2);
}
