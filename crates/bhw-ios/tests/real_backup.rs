//! Reads The Blockheads worlds from a real iPhone/iPad backup.
//!
//! Skipped unless `BHW_IOS_BACKUP` points at one backup directory (`.../MobileSync/Backup/<udid>`) or a backup root.
//! Encrypted backups also need `BHW_IOS_PASSWORD`. Nothing is written anywhere; the backup is only read. Run with
//! `--release` for encrypted backups (key derivation is 10 million PBKDF2 rounds).

use std::path::PathBuf;

use bhw_ios::{list_backups, IosBackup};
use bhw_save::{summarize_any, AnySave, GameDb};

#[test]
fn real_backup_worlds_summarize() {
    let Some(dir) = std::env::var_os("BHW_IOS_BACKUP").map(PathBuf::from) else {
        eprintln!("BHW_IOS_BACKUP not set, skipping");
        return;
    };
    let password = std::env::var("BHW_IOS_PASSWORD").ok();
    let backups = list_backups(&dir).unwrap();
    assert!(!backups.is_empty(), "no backups in {}", dir.display());
    for info in backups {
        eprintln!(
            "{}: {:?} {:?} iOS {:?} date {:?} encrypted {}",
            info.id, info.device_name, info.product_type, info.ios_version, info.date, info.encrypted
        );
        if info.encrypted && password.is_none() {
            eprintln!("  encrypted and BHW_IOS_PASSWORD not set, skipping");
            continue;
        }
        let backup = IosBackup::open(&info.path, password.as_deref()).unwrap();
        let apps = backup.blockheads_apps().unwrap();
        eprintln!("  Blockheads apps: {apps:?}");
        for app in apps {
            let data = backup.blockheads_for(&app).unwrap();
            let gdb = data
                .game_db
                .iter()
                .find(|f| f.path == "data.mdb")
                .map(|f| GameDb::from_bytes(&f.data.read_all().unwrap()).unwrap());
            for w in &data.worlds {
                let files = bhw_save::world::to_file_map(w).unwrap();
                let save = AnySave::from_files(&w.id, files).unwrap();
                let s = summarize_any(&save, gdb.as_ref());
                eprintln!("  world {} {:?} kind {:?} problems {:?}", w.id, s.name, s.kind, s.problems);
            }
        }
    }
}
