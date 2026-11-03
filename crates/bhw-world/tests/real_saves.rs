//! Round trip of real game saves through `.bhworld` and through an Android backup rewrite.
//!
//! Skipped unless `BHW_REAL_SAVES` points at a saves directory (e.g. Blockheads PC's
//! `<home>/Library/Application Support/saves`). Nothing from that directory is written anywhere except the system
//! temp dir, which is cleaned up.

use std::io::Cursor;
use std::path::PathBuf;

use bhw_backup::{BackupOptions, BackupWriter, EntryKind, TarWriter};
use bhw_world::backup::{load_backup, put_worlds};
use bhw_world::bhworld::{read_bhworld, write_bhworld};
use bhw_world::{ExportInfo, Limits, Source, SourceKind, WorldFiles};

fn saves_dir() -> Option<PathBuf> {
    let dir = PathBuf::from(std::env::var_os("BHW_REAL_SAVES")?);
    assert!(dir.is_dir(), "BHW_REAL_SAVES is not a directory");
    Some(dir)
}

#[test]
fn real_worlds_survive_bhworld_round_trip() {
    let Some(dir) = saves_dir() else {
        eprintln!("BHW_REAL_SAVES not set, skipping");
        return;
    };
    let worlds = WorldFiles::list_saves_dir(&dir).unwrap();
    assert!(!worlds.is_empty(), "no worlds in {}", dir.display());
    let info = ExportInfo {
        name: None,
        game_version: None,
        source: Source { kind: SourceKind::BlockheadsPc, device_name: None },
        created_at: None,
    };
    for w in &worlds {
        let mem = w.to_memory().unwrap();
        let mut zip = Cursor::new(Vec::new());
        write_bhworld(&mut zip, &mem, &info, None).unwrap();
        let back = read_bhworld(&mut Cursor::new(zip.into_inner()), &Limits::default()).unwrap();
        assert_eq!(back.world, mem, "world {}", w.id);
        // And back into a saves directory.
        let tmp = std::env::temp_dir().join(format!("bhw-real-{}-{}", std::process::id(), w.id));
        back.world.save_to_saves_dir(&tmp, false).unwrap();
        assert_eq!(WorldFiles::from_saves_dir(&tmp, &w.id).unwrap().to_memory().unwrap(), mem);
        std::fs::remove_dir_all(&tmp).unwrap();
    }
    eprintln!("{} worlds round-tripped", worlds.len());
}

#[test]
fn real_worlds_survive_backup_rewrite() {
    let Some(dir) = saves_dir() else {
        eprintln!("BHW_REAL_SAVES not set, skipping");
        return;
    };
    let worlds: Vec<WorldFiles> =
        WorldFiles::list_saves_dir(&dir).unwrap().iter().map(|w| w.to_memory().unwrap()).collect();
    // A synthetic device backup with an empty saves dir next to game_db, encrypted like a real one could be.
    let prefix = "apps/com.noodlecake.blockheads/f/Library/Application Support/";
    let mut ab = Vec::new();
    let opts = BackupOptions { password: Some("pw".into()), ..Default::default() };
    let mut w = BackupWriter::create(Box::new(&mut ab), &opts).unwrap();
    let mut t = TarWriter::new(&mut w);
    for (p, kind, d) in [
        ("apps/com.noodlecake.blockheads/_manifest".to_string(), EntryKind::File, &b"m"[..]),
        (format!("{prefix}game_db"), EntryKind::Directory, b""),
        (format!("{prefix}saves"), EntryKind::Directory, b""),
    ] {
        let mut r: &[u8] = d;
        t.append(&p, kind, 0o700, 0, d.len() as u64, &mut r).unwrap();
    }
    t.finish().unwrap();
    w.finish().unwrap();
    let (_, saves) = load_backup(Box::new(&ab[..]), Some("pw")).unwrap();
    assert!(saves[0].worlds.is_empty());
    let mut out = Vec::new();
    put_worlds(Box::new(&ab[..]), Some("pw"), Box::new(&mut out), Some("pw2"), &saves[0].prefix, &worlds, &[]).unwrap();
    let (_, after) = load_backup(Box::new(&out[..]), Some("pw2")).unwrap();
    assert_eq!(after[0].worlds, worlds);
}
