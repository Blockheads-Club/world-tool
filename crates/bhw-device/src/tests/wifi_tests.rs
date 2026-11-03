use super::adb;
use crate::error::DeviceError;
use crate::fake::{Reply, ScriptedRunner};
use crate::wifi::*;
use std::path::PathBuf;

#[test]
fn parses_mdns() {
    let text = "List of discovered mdns services\nadb-R58-AbC\t_adb-tls-connect._tcp\t192.168.1.5:37001\n\
                adb-R58-AbC\t_adb-tls-pairing._tcp.\t192.168.1.5:37002\nx\t_adb._tcp\t1.2.3.4:5555\ny\t_foo._tcp\t1:2\n";
    let s = parse_mdns_services(text);
    assert_eq!(s.len(), 4);
    assert_eq!(
        s[0],
        MdnsService { name: "adb-R58-AbC".into(), kind: MdnsKind::Connect, address: "192.168.1.5:37001".into() }
    );
    assert_eq!(s[1].kind, MdnsKind::Pairing);
    assert_eq!(s[2].kind, MdnsKind::Legacy);
    assert_eq!(s[3].kind, MdnsKind::Other("_foo._tcp".into()));
}

#[test]
fn pair_connect_mdns() {
    let r = ScriptedRunner::new();
    r.on("pair 1.2.3.4:1 123456", Reply::ok("Successfully paired to 1.2.3.4:1 [guid=adb-X]\n"));
    r.on("pair", Reply::fail(1, "Failed: Wrong password or connection was dropped.\n"));
    r.on("connect 1.2.3.4:5", Reply::ok("connected to 1.2.3.4:5\n"));
    r.on("connect 1.2.3.4:6", Reply::ok("already connected to 1.2.3.4:6\n"));
    r.on("connect 1.2.3.4:7", Reply::ok("failed to connect to '1.2.3.4:7': Connection refused\n"));
    r.on("connect", Reply::ok("cannot connect to 9: no route\n"));
    r.on("disconnect", Reply::ok("disconnected 1.2.3.4:5\n"));
    r.once("mdns check", Reply::ok("mdns daemon version [Openscreen discovery 0.0.0]\n"));
    r.on("mdns check", Reply::fail(1, "ERROR: mdns discovery disabled"));
    r.once("mdns services", Reply::ok("List of discovered mdns services\nn\t_adb-tls-connect._tcp\t1.2.3.4:5\n"));
    r.on("mdns services", Reply::fail(1, "unknown command"));
    let a = adb(&r);
    assert!(a.pair("1.2.3.4:1", "123456").unwrap().contains("guid=adb-X"));
    assert!(matches!(a.pair("1.2.3.4:1", "000000"), Err(DeviceError::PairFailed(m)) if m.contains("Wrong password")));
    assert_eq!(a.connect("1.2.3.4:5").unwrap(), "1.2.3.4:5");
    assert!(a.connect("1.2.3.4:6").is_ok());
    assert!(matches!(a.connect("1.2.3.4:7"), Err(DeviceError::ConnectFailed(_))));
    assert!(matches!(a.connect("9"), Err(DeviceError::ConnectFailed(_))));
    a.disconnect("1.2.3.4:5").unwrap();
    assert!(a.mdns_available().unwrap());
    assert!(!a.mdns_available().unwrap());
    assert_eq!(a.mdns_services().unwrap().len(), 1);
    assert!(a.mdns_services().is_err());
}

#[test]
fn known_hosts_file_roundtrip() {
    let env = |k: &str| (k == "ANDROID_USER_HOME").then(|| "/u".into());
    assert_eq!(android_user_dir(&env, Some("/h".into())), Some(PathBuf::from("/u")));
    assert_eq!(android_user_dir(&|_| None, Some("/h".into())), Some(PathBuf::from("/h/.android")));
    assert_eq!(android_user_dir(&|_| Some("".into()), None), None);
    let real = [0x0a, 0x18, 0x0a, 0x16];
    let mut data = real.to_vec();
    data.extend(b"adb-RFCT80DYJFJ-bPBhQK");
    assert_eq!(parse_known_hosts(&data).unwrap(), vec!["adb-RFCT80DYJFJ-bPBhQK".to_string()]);
    let long = "g".repeat(200);
    let hosts = vec!["a".to_string(), long.clone()];
    assert_eq!(parse_known_hosts(&encode_known_hosts(&hosts)).unwrap(), hosts);
    // Unknown fields (varint, other numbers) are skipped; an empty HostInfo is an empty guid.
    let mut odd = vec![0x10, 0x96, 0x01, 0x12, 0x01, b'z', 0x0a, 0x00];
    odd.extend(encode_known_hosts(&["b".into()]));
    assert_eq!(parse_known_hosts(&odd).unwrap(), vec!["".to_string(), "b".into()]);
    for bad in [&[0x0b][..], &[0x0a, 0x05, 1], &[0x0a], &[0xff; 11], &[0x0a, 0x01, 0x0b]] {
        assert!(parse_known_hosts(bad).is_none(), "{bad:?}");
    }
    let dir = tempfile::tempdir().unwrap();
    assert!(paired_hosts(dir.path()).unwrap().is_empty());
    std::fs::write(known_hosts_file(dir.path()), encode_known_hosts(&hosts)).unwrap();
    assert!(!forget_paired_host(dir.path(), "nope").unwrap());
    assert!(forget_paired_host(dir.path(), "a").unwrap());
    assert_eq!(paired_hosts(dir.path()).unwrap(), vec![long]);
    std::fs::write(known_hosts_file(dir.path()), [0x0b]).unwrap();
    assert!(paired_hosts(dir.path()).is_err());
    std::fs::remove_file(known_hosts_file(dir.path())).unwrap();
    std::fs::create_dir(known_hosts_file(dir.path())).unwrap();
    assert!(paired_hosts(dir.path()).is_err());
}
