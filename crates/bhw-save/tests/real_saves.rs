//! Checks against real saves. Skipped unless `BHW_REAL_SAVES` points at a game `saves` directory (its sibling
//! `game_db` is used when present); `BHW_REAL_SERVER_SAVE` may point at one Blockheads Server world folder.
//! Nothing is written next to the real files: all output goes to temporary directories.

#[allow(dead_code, clippy::all)]
#[path = "../../bhw-lmdb/tests/common/mod.rs"]
mod real_lmdb;

use std::path::{Path, PathBuf};

use bhw_lmdb::Layout;
use bhw_save::convert::{client_to_server_files, server_to_client_files};
use bhw_save::save::{env_from, read_dir_files, FileMap, WORLD_DB};
use bhw_save::*;

fn saves_dir() -> Option<PathBuf> {
    let d = PathBuf::from(std::env::var_os("BHW_REAL_SAVES")?);
    assert!(d.is_dir(), "BHW_REAL_SAVES={} is not a directory", d.display());
    Some(d)
}

fn worlds(saves: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(saves)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.join("world_db/data.mdb").is_file())
        .collect();
    v.sort();
    v
}

/// The server output opens with real LMDB (the library Blockheads Server uses) and holds the same records.
fn check_with_real_lmdb(files: &FileMap, path: &str) {
    if Layout::native() != Layout::Bits64 {
        return;
    }
    let ours = env_from(files, path).unwrap().unwrap().0;
    let real = real_lmdb::RealEnv::open(Some(&files[path]), 0, true).read_all();
    assert!(real.same_data(&ours), "{path}: real LMDB sees different data");
}

#[test]
fn client_saves() {
    let Some(saves) = saves_dir() else {
        eprintln!("BHW_REAL_SAVES not set; skipping");
        return;
    };
    let game_db = saves.parent().map(|p| p.join("game_db")).filter(|p| p.join("data.mdb").is_file());
    let gdb = game_db.as_deref().map(|g| GameDb::open(g).unwrap());
    let mut converted = 0;
    for dir in worlds(&saves) {
        let files = read_dir_files(&dir).unwrap();
        let id = dir.file_name().unwrap().to_string_lossy().into_owned();
        let save = AnySave::from_files(&id, files.clone()).unwrap();
        let s = summarize_any(&save, gdb.as_ref());
        eprintln!(
            "{id}: {:?} width={:?} blocks={} blockheads={} registered={:?} remote={} problems={:?}",
            s.name,
            s.world_width_macro,
            s.block_count,
            s.blockheads.len(),
            s.registered_in_game_db,
            s.remote_game,
            s.problems
        );
        assert_eq!(s.kind, Some(SaveKind::Client));
        let record = save.worldv2();
        if let Some(g) = &gdb {
            // a world is listed exactly when it has a record, and the list holds a copy of that record
            assert_eq!(s.registered_in_game_db, Some(record.is_some()), "{id}");
            if let Some(r) = record {
                assert_eq!(g.entry(&id), Some(r), "{id}: game_db entry differs from main/worldv2");
            }
        }
        let (img, bad) = render_map(save.world_db(), MapOptions { tiles_per_pixel: 8, crop_to_content: true });
        assert_eq!(bad, 0, "{id}");
        assert_eq!(img.width > 0, s.block_count > 0, "{id}");
        assert!(img.to_png().len() > 8);
        let Some(record) = record else { continue };
        assert_eq!(s.id, id);
        assert!(s.name.as_deref().is_some_and(|n| !n.is_empty()), "{id}");
        assert!([32, 128, 512, 2048, 8192].contains(&s.world_width_macro.unwrap()), "{id}");
        assert!(s.created.unwrap() <= s.last_saved.unwrap(), "{id}");
        assert_eq!(s.save_version, Some(1100), "{id}");
        assert!(s.seed.is_some() && s.start_portal.is_some(), "{id}");
        assert_eq!(s.has_preview_image, stored_preview(record).is_some(), "{id}");

        let to_server = client_to_server_files(&files, &ConvertOptions::default());
        if s.remote_game {
            assert!(matches!(to_server, Err(Error::Unsupported(_))), "{id}");
            continue;
        }
        let (server, report) = to_server.unwrap();
        assert!(!report.has_losses(), "{id}: {report:?}");
        check_with_real_lmdb(&server, WORLD_DB);
        check_with_real_lmdb(&server, bhw_save::save::SERVER_DB);
        check_with_real_lmdb(&server, bhw_save::save::LIGHT_BLOCKS);
        let ss = summarize_any(&AnySave::from_files(&id, server.clone()).unwrap(), None);
        assert_eq!((ss.kind, &ss.name, ss.block_count), (Some(SaveKind::Server), &s.name, s.block_count));

        let (back, report) = server_to_client_files(&server, &ConvertOptions::default()).unwrap();
        assert!(!report.has_losses(), "{id}: {report:?}");
        let a = env_from(&files, WORLD_DB).unwrap().unwrap();
        let b = env_from(&back, WORLD_DB).unwrap().unwrap();
        assert_eq!(b.1, Layout::Bits32);
        assert!(a.0.same_data(&b.0), "{id}: world_db records changed in the round trip");
        for (p, v) in &files {
            if p != WORLD_DB {
                assert_eq!(back.get(p), Some(v), "{id}: {p} changed in the round trip");
            }
        }
        converted += 1;
    }
    eprintln!("converted {converted} worlds");
}

#[test]
fn server_save() {
    let Some(dir) = std::env::var_os("BHW_REAL_SERVER_SAVE").map(PathBuf::from) else {
        eprintln!("BHW_REAL_SERVER_SAVE not set; skipping");
        return;
    };
    let files = read_dir_files(&dir).unwrap();
    let id = dir.file_name().unwrap().to_string_lossy().into_owned();
    let s = summarize_any(&AnySave::from_files(&id, files.clone()).unwrap(), None);
    eprintln!("{s:?}");
    assert_eq!(s.kind, Some(SaveKind::Server));
    assert!(s.name.is_some() && s.player_count.is_some());
    let (client, report) = server_to_client_files(&files, &ConvertOptions::default()).unwrap();
    eprintln!("{report:#?}");
    assert!(!report.has_losses());
    let (server, _) = client_to_server_files(&client, &ConvertOptions::default()).unwrap();
    let a = env_from(&files, WORLD_DB).unwrap().unwrap().0;
    let b = env_from(&server, WORLD_DB).unwrap().unwrap().0;
    assert!(a.same_data(&b));
    for (p, v) in &files {
        if p != WORLD_DB {
            assert_eq!(server.get(p), Some(v), "{p} changed in the round trip");
        }
    }
    check_with_real_lmdb(&server, WORLD_DB);
}
