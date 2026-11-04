//! Cross-checks against real LMDB (native layout; run with `--target i686-unknown-linux-gnu` for 32-bit).

mod common;

use bhw_lmdb::{convert, write_env, Database, DbFlags, Env, EnvContents, Layout};
use common::{real_append_check, real_read, real_write, RealEnv};

fn le(i: u32) -> Vec<u8> {
    i.to_le_bytes().to_vec()
}

/// A environment exercising every structure: overflow values, multi-level trees, all comparators,
/// sorted duplicates in sub-pages and sub-DBs (also `P_LEAF2`), empty DBs.
fn sample() -> EnvContents {
    let mut c = EnvContents::new(4096, 1 << 24);
    c.main = Database::new(0).with("alpha", "1").with("big", vec![7u8; 9000]).with("zeta", "");
    let mut plain = Database::new(0);
    for i in 0..3000u32 {
        plain.entries.push((format!("key-{i:06}").into_bytes(), vec![(i % 251) as u8; (i as usize * 13) % 3100]));
    }
    plain.entries.push((vec![b'k'; 511], vec![1; 2100]));
    c.named.insert(b"plain".to_vec(), plain);
    let mut int = Database::new(DbFlags::INTEGERKEY);
    for i in (0..2000u32).rev() {
        int.entries.push((le(i * 7919), le(i)));
    }
    c.named.insert(b"int".to_vec(), int);
    let mut rev = Database::new(DbFlags::REVERSEKEY);
    for i in 0..500u32 {
        rev.entries.push((format!("{i}-tail").into_bytes(), b"r".to_vec()));
    }
    c.named.insert(b"rev".to_vec(), rev);
    let mut dups = Database::new(DbFlags::DUPSORT);
    dups.entries.push((b"one".to_vec(), b"only".to_vec()));
    for i in 0..3u32 {
        dups.entries.push((b"three".to_vec(), format!("v{i}").into_bytes()));
    }
    for i in 0..300u32 {
        dups.entries.push((b"many".to_vec(), vec![(i % 256) as u8; 1 + (i as usize * 7) % 511]));
    }
    c.named.insert(b"dups".to_vec(), dups);
    let mut fixed = Database::new(DbFlags::DUPSORT | DbFlags::DUPFIXED | DbFlags::INTEGERDUP);
    fixed.entries.push((b"a".to_vec(), le(5)));
    fixed.entries.push((b"a".to_vec(), le(1)));
    fixed.entries.push((b"b".to_vec(), le(9)));
    for i in 0..5000u32 {
        fixed.entries.push((b"c".to_vec(), le(i * 3)));
    }
    c.named.insert(b"fixed".to_vec(), fixed);
    let mut revdup = Database::new(DbFlags::DUPSORT | DbFlags::REVERSEDUP);
    for i in 0..40u32 {
        revdup.entries.push((b"k".to_vec(), format!("{i}x").into_bytes()));
    }
    c.named.insert(b"revdup".to_vec(), revdup);
    c.named.insert(b"empty".to_vec(), Database::new(0));
    c.named.insert(b"emptydup".to_vec(), Database::new(DbFlags::DUPSORT));
    c
}

/// Sorts entries the way LMDB stores them.
fn normalize(mut c: EnvContents) -> EnvContents {
    for db in std::iter::once(&mut c.main).chain(c.named.values_mut()) {
        let f = db.flags;
        db.entries.sort_by(|a, b| f.cmp_keys(&a.0, &b.0).then_with(|| f.cmp_dups(&a.1, &b.1)));
    }
    c
}

#[test]
fn ours_reads_real_lmdb() {
    let sample = normalize(sample());
    let bytes = real_write(&sample);
    let env = Env::open(&bytes).unwrap();
    assert_eq!(env.layout(), Layout::native());
    let ours = env.read_all().unwrap();
    assert!(ours.same_data(&sample));
    assert_eq!(ours.map_size, sample.map_size);
    // mm_flags = me_flags & 0xffff | MDB_INTEGERKEY (mdb.c:4351): our helper opens with MDB_NOSUBDIR (0x4000).
    assert_eq!(ours.env_flags, DbFlags::INTEGERKEY | 0x4000);
    let plain = env.open_db(b"plain").unwrap();
    assert_eq!(env.get(&plain, b"key-000017").unwrap(), Some(&vec![17u8; 221][..]));
    assert_eq!(env.get(&plain, b"key-0000175").unwrap(), None);
    assert_eq!(env.get(&plain, b"zzz").unwrap(), None);
    assert!(plain.depth >= 2);
    let fixed = env.open_db(b"fixed").unwrap();
    assert_eq!(env.get(&fixed, b"a").unwrap(), Some(&le(1)[..]));
}

#[test]
fn ours_reads_real_lmdb_with_free_pages() {
    let sample = normalize(sample());
    // Real LMDB writes with the OS page size: with 16 KiB pages (macOS on Apple silicon) the copy-on-write rounds
    // below outgrow the sample's 16 MiB map (MDB_MAP_FULL), so give them room.
    let env = RealEnv::open(Some(&real_write(&sample)), 1 << 26, false);
    for round in 0..3 {
        let txn = env.txn(false);
        let dbi = txn.dbi(Some(b"plain"), 0).unwrap();
        for i in (round..3000).step_by(3 + round) {
            txn.del(dbi, format!("key-{i:06}").as_bytes());
        }
        let dups = txn.dbi(Some(b"dups"), 0).unwrap();
        txn.del(dups, b"many");
        txn.commit();
    }
    let bytes = env.file_bytes();
    let ours = Env::open(&bytes).unwrap();
    assert!(ours.free_db().root.is_some(), "expected a non-empty free DB");
    assert_eq!(ours.last_txnid(), 4);
    let real = env.read_all();
    let mine = ours.read_all().unwrap();
    assert!(mine.same_data(&real));
    // Compacting our way gives the same data and an empty free DB.
    let compact = write_env(&mine, Layout::native()).unwrap();
    let back = Env::open(&compact).unwrap();
    assert!(back.free_db().root.is_none());
    assert!(back.read_all().unwrap().same_data(&real));
    assert!(compact.len() < bytes.len());
}

#[test]
fn real_lmdb_reads_and_extends_ours() {
    let sample = normalize(sample());
    let out = write_env(&sample, Layout::native()).unwrap();
    assert!(real_read(&out).same_data(&sample));
    real_append_check(&out);
    // Real LMDB's own accounting (used by its compacting copy) must match our page counts.
    let env = RealEnv::open(Some(&out), 0, true);
    let (map, last, txnid) = env.info();
    assert_eq!((map, last, txnid), (1 << 24, out.len() / 4096 - 1, 1));
    let copy = env.dir.path().join("c.mdb");
    env.copy_compact(&copy);
    assert_eq!(std::fs::read(copy).unwrap().len(), out.len());
}

#[test]
fn other_page_sizes_are_readable_by_real_lmdb() {
    for ps in [1024u32, 2048, 16384] {
        let mut s = normalize(sample());
        s.page_size = ps;
        let plain = s.named.get_mut(&b"plain"[..]).unwrap();
        plain.entries.retain(|(k, _)| k.len() < 100);
        let out = write_env(&s, Layout::native()).unwrap();
        assert_eq!(Env::open(&out).unwrap().page_size(), ps);
        assert!(real_read(&out).same_data(&s), "page size {ps}");
        real_append_check(&out);
    }
}

#[test]
fn convert_between_layouts() {
    let sample = normalize(sample());
    let native = real_write(&sample);
    let other = match Layout::native() {
        Layout::Bits32 => Layout::Bits64,
        Layout::Bits64 => Layout::Bits32,
    };
    let foreign = convert(&native, other).unwrap();
    let env = Env::open(&foreign).unwrap();
    assert_eq!(env.layout(), other);
    assert!(env.read_all().unwrap().same_data(&sample));
    let back = convert(&foreign, Layout::native()).unwrap();
    assert!(real_read(&back).same_data(&sample));
    real_append_check(&back);
}
