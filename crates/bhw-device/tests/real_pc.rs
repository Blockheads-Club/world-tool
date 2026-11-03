//! Reads the real Blockheads PC saves, read-only. Skipped unless `BHW_REAL_SAVES` points at a
//! `.../home/Library/Application Support/saves` directory of a Blockheads PC data folder.

use bhw_device::PcInstall;
use std::path::Path;

#[test]
fn reads_real_pc_worlds() {
    let Some(saves) = std::env::var_os("BHW_REAL_SAVES") else {
        eprintln!("BHW_REAL_SAVES not set; skipping");
        return;
    };
    let saves = Path::new(&saves);
    let Some(data) = saves.ancestors().nth(4).filter(|d| d.join("home/Library/Application Support/saves") == saves)
    else {
        eprintln!("BHW_REAL_SAVES is not a Blockheads PC saves dir; skipping");
        return;
    };
    let pc = PcInstall::at(data);
    assert!(pc.exists());
    let worlds = pc.list_worlds().unwrap();
    eprintln!("{} worlds", worlds.len());
    let out = tempfile::tempdir().unwrap();
    for w in &worlds {
        let n = pc.read_world(&w.id, &out.path().join(&w.id)).unwrap();
        assert!(n > 0 && out.path().join(&w.id).join("world_db/data.mdb").is_file());
    }
}
