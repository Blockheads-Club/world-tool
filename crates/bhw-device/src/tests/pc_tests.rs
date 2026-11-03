use crate::error::DeviceError;
use crate::fake::{Reply, ScriptedRunner};
use crate::pc::*;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

const A: &str = "0123456789abcdef0123456789abcdef";
const B: &str = "fedcba9876543210fedcba9876543210";

fn make_world(dir: &Path) {
    std::fs::create_dir_all(dir.join("world_db")).unwrap();
    std::fs::create_dir_all(dir.join("map")).unwrap();
    std::fs::write(dir.join("world_db/data.mdb"), b"d").unwrap();
    std::fs::write(dir.join("map/0_0"), b"m").unwrap();
}

#[test]
fn locates_data_dirs() {
    assert_eq!(launcher_data_dir("data_dir = 'C:\\Users\\x\\bh'\n"), Some(PathBuf::from("C:\\Users\\x\\bh")));
    assert_eq!(launcher_data_dir("data_dir = \"\""), None);
    assert_eq!(launcher_data_dir("not toml ="), None);
    let cfg = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(cfg.path().join("blockheads-pc")).unwrap();
    std::fs::write(cfg.path().join("blockheads-pc/launcher.toml"), "data_dir = \"/games/bh\"\n").unwrap();
    let env = |k: &str| -> Option<OsString> { (k == "BHPC_DATA_DIR").then(|| "/env/bh".into()) };
    let c = PcInstall::candidates_with(&env, Some(cfg.path().into()), Some("/share".into()));
    let got: Vec<_> = c.iter().map(|p| (p.data_dir.clone(), p.source)).collect();
    assert_eq!(
        got,
        vec![
            (PathBuf::from("/env/bh"), DataDirSource::Env),
            (PathBuf::from("/games/bh"), DataDirSource::LauncherConfig),
            (Path::new("/share").join("blockheads-pc"), DataDirSource::PlatformDefault),
        ]
    );
    // BHPC_CONFIG_DIR points at the launcher dir itself.
    let launcher = cfg.path().join("blockheads-pc");
    let env2 = move |k: &str| -> Option<OsString> { (k == "BHPC_CONFIG_DIR").then(|| launcher.clone().into()) };
    assert_eq!(PcInstall::candidates_with(&env2, None, None)[0].source, DataDirSource::LauncherConfig);
    assert!(PcInstall::candidates_with(&|_| None, None, None).is_empty());
    if let Some(p) = PcInstall::locate() {
        assert!(p.exists());
    }
}

#[test]
fn lists_reads_and_writes_worlds() {
    let root = tempfile::tempdir().unwrap();
    let pc = PcInstall::at(root.path());
    assert_eq!(pc.source, DataDirSource::Explicit);
    assert!(!pc.exists());
    assert!(pc.list_worlds().unwrap().is_empty());
    assert!(pc.game_db().ends_with("Application Support/game_db"));
    let saves = pc.saves_dir();
    make_world(&saves.join(A));
    std::fs::create_dir_all(saves.join("notaworld")).unwrap();
    std::fs::create_dir_all(saves.join(B)).unwrap();
    make_world(&saves.join("00000000000000000000000000000000"));
    std::fs::write(saves.join("ffffffffffffffffffffffffffffffff"), b"file").unwrap();
    assert!(pc.exists());
    let w = pc.list_worlds().unwrap();
    assert_eq!(w.iter().map(|w| w.id.as_str()).collect::<Vec<_>>(), vec!["00000000000000000000000000000000", A]);
    let out = tempfile::tempdir().unwrap();
    assert_eq!(pc.read_world(A, &out.path().join("w")).unwrap(), 2);
    assert!(matches!(pc.read_world("zz", out.path()), Err(DeviceError::PathNotFound(_))));
    assert!(matches!(pc.read_world(B.replace('f', "e").as_str(), out.path()), Err(DeviceError::PathNotFound(_))));
    let src = out.path().join("w");
    #[cfg(unix)]
    std::os::unix::fs::symlink("/etc/hostname", src.join("link")).unwrap();
    assert!(matches!(pc.write_world(B, &src, false, true), Err(DeviceError::GameRunning)));
    assert!(matches!(pc.write_world("x", &src, false, false), Err(DeviceError::PathNotFound(_))));
    assert!(matches!(pc.write_world(B, out.path(), false, false), Err(DeviceError::PathNotFound(_))));
    assert!(matches!(pc.write_world(A, &src, false, false), Err(DeviceError::WorldExists(_))));
    std::fs::create_dir_all(saves.join(format!(".bhw-incoming-{A}"))).unwrap();
    std::fs::create_dir_all(saves.join(format!(".bhw-replaced-{A}"))).unwrap();
    std::fs::write(src.join("world_db/data.mdb"), b"new").unwrap();
    assert_eq!(pc.write_world(A, &src, true, false).unwrap(), 2);
    assert_eq!(std::fs::read(saves.join(A).join("world_db/data.mdb")).unwrap(), b"new");
    assert!(!saves.join(format!(".bhw-replaced-{A}")).exists());
    let fresh = PcInstall::at(root.path().join("fresh"));
    assert_eq!(fresh.write_world(B, &src, false, false).unwrap(), 2);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let f = src.join("world_db/data.mdb");
        std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o000)).unwrap();
        assert!(pc.write_world(A, &src, true, false).is_err());
        std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(!saves.join(format!(".bhw-incoming-{A}")).exists());
    }
    // A saves path that is a file.
    let bad = PcInstall::at(root.path().join("bad"));
    std::fs::create_dir_all(bad.support_dir()).unwrap();
    std::fs::write(bad.saves_dir(), b"").unwrap();
    assert!(bad.list_worlds().is_err());
    assert!(copy_dir(&root.path().join("missing"), &root.path().join("m2")).is_err());
}

#[test]
fn detects_running_game() {
    let names = parse_process_list(
        "systemd\n/Applications/X.app/Contents/MacOS/bhpc-run\n\n\"bhpc-run.exe\",\"12\",\"Console\"\n/\n",
    );
    assert_eq!(names, vec!["systemd", "bhpc-run", "bhpc-run"]);
    let r = ScriptedRunner::new();
    r.once("-A -o comm=", Reply::ok("init\nbhpc-run\n"));
    r.on("-A -o comm=", Reply::ok("init\n"));
    r.on("/FO CSV", Reply::fail(1, "denied"));
    assert!(game_running(&r, false).unwrap());
    assert!(!game_running(&r, false).unwrap());
    assert!(matches!(game_running(&r, true), Err(DeviceError::Command { .. })));
    assert_eq!(r.calls()[2].program, "tasklist");
    #[cfg(unix)]
    assert!(game_running(&crate::process::SystemRunner, false).is_ok());
}
