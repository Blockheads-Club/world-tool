//! Round trips over real saves. Skipped unless `BHW_REAL_SAVES` names a `saves/` folder (never committed).

use bhw_content::WorldContent;
use bhw_save::ClientSave;

#[test]
fn real_saves_round_trip() {
    let Some(dir) = std::env::var_os("BHW_REAL_SAVES") else {
        eprintln!("BHW_REAL_SAVES not set; skipped");
        return;
    };
    let mut n = 0;
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let Ok(save) = ClientSave::open(&path) else { continue };
        let original = save.world_db.clone();
        let mut w = WorldContent::load(save.world_db).unwrap();
        assert!(w.blocks.damaged().next().is_none());
        assert_eq!(w.to_env().unwrap(), original, "{path:?}: unchanged world must keep every byte");
        let (objs, bad) = w.objects.all();
        assert!(bad.is_empty(), "{path:?}: {bad:?}");
        assert!(w.players.damaged.is_empty(), "{path:?}");
        // change one tile in the first block and check only that block's record changed
        let first = w.blocks.positions().next();
        if let Some(pos) = first {
            let (x, y) = (pos.0 * 32, pos.1 * 32);
            let mut t = w.blocks.tile(x, y).unwrap().unwrap();
            t.set_fg(if t.fg() == 6 { 7 } else { 6 });
            w.blocks.set_tile(x, y, t).unwrap();
            let out = w.to_env().unwrap();
            let (a, b) = (&original.named[&b"blocks"[..]].entries, &out.named[&b"blocks"[..]].entries);
            assert_eq!(a.len(), b.len());
            assert_eq!(a.iter().zip(b).filter(|(p, q)| p != q).count(), 1);
            bhw_lmdb::write_env(&out, bhw_lmdb::Layout::Bits32).unwrap();
        }
        eprintln!("{path:?}: {} blocks, {} objects, {} blockheads", w.blocks.len(), objs.len(), w.players.list().len());
        n += 1;
    }
    assert!(n > 0);
}
