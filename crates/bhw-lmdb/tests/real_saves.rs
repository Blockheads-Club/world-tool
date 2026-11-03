//! Round-trip test over real game saves. Skipped unless `BHW_REAL_SAVES` points at a `saves` directory
//! (containing `<id>/world_db/data.mdb`). The saves are only read; copies go to a temporary directory.

mod common;

use std::path::PathBuf;

use bhw_lmdb::{convert, write_env, Env, Layout};

fn worlds() -> Vec<PathBuf> {
    let Ok(dir) = std::env::var("BHW_REAL_SAVES") else {
        eprintln!("BHW_REAL_SAVES not set, skipping");
        return Vec::new();
    };
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)
        .expect("read BHW_REAL_SAVES")
        .map(|e| e.unwrap().path().join("world_db").join("data.mdb"))
        .filter(|p| p.is_file())
        .collect();
    out.sort();
    out
}

#[test]
fn real_saves_roundtrip() {
    for path in worlds() {
        let bytes = std::fs::read(&path).unwrap();
        let env = Env::open(&bytes).unwrap();
        assert_eq!(env.layout(), Layout::Bits32, "{path:?}");
        let contents = env.read_all().unwrap();
        // Real LMDB of the native word size must agree with our reader on the original file.
        if env.layout() == Layout::native() {
            assert!(common::real_read(&bytes).same_data(&contents), "{path:?}: real LMDB disagrees");
        }
        for layout in [Layout::Bits32, Layout::Bits64] {
            let out = write_env(&contents, layout).unwrap();
            let back = Env::open(&out).unwrap();
            assert_eq!(back.layout(), layout);
            assert_eq!(back.map_size(), env.map_size());
            assert_eq!(back.page_size(), env.page_size());
            assert_eq!(back.read_all().unwrap(), contents, "{path:?} {layout:?}");
            assert_eq!(convert(&out, Layout::Bits32).unwrap(), write_env(&contents, Layout::Bits32).unwrap());
            if layout == Layout::native() {
                assert!(common::real_read(&out).same_data(&contents), "{path:?}: real LMDB reads ours differently");
                common::real_append_check(&out);
            }
        }
        eprintln!("ok {path:?}: {} named DBs", contents.named.len());
    }
}
