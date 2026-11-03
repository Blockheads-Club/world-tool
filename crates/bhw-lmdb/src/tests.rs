//! Unit tests with writer-built fixtures for both layouts, including corrupted files.

use crate::format::{DbFlags, DbRecord, Meta};
use crate::layout::{node_flags, page_flags, Layout};
use crate::{convert, read_file, write_env, Database, Env, EnvContents, Error, Result};

const LAYOUTS: [Layout; 2] = [Layout::Bits32, Layout::Bits64];

fn le(i: u32) -> Vec<u8> {
    i.to_le_bytes().to_vec()
}

fn fixture() -> EnvContents {
    let mut c = EnvContents::new(4096, 1 << 20);
    c.main = Database::new(0).with("m", "main");
    c.named.insert(b"a".to_vec(), Database::new(0).with("k", vec![9u8; 5000]).with("s", "small"));
    let mut dup = Database::new(DbFlags::DUPSORT).with("x", "3").with("x", "1").with("x", "2");
    for i in 0..100u32 {
        dup.entries.push((b"y".to_vec(), [le(i), vec![1; 200]].concat()));
    }
    c.named.insert(b"dup".to_vec(), dup);
    let mut fixed = Database::new(DbFlags::DUPSORT | DbFlags::DUPFIXED).with("x", le(2)).with("x", le(1));
    for i in 0..3000u32 {
        fixed.entries.push((b"z".to_vec(), le(i).iter().rev().copied().collect()));
    }
    c.named.insert(b"fixed".to_vec(), fixed);
    let mut many = Database::new(0);
    for i in 0..500u32 {
        many.entries.push((format!("k{i:04}").into_bytes(), vec![3; 20]));
    }
    c.named.insert(b"many".to_vec(), many);
    c
}

/// A writer-built file plus helpers to poke at it.
struct Fx {
    l: Layout,
    b: Vec<u8>,
    ps: usize,
    hdr: usize,
}

impl Fx {
    fn new(l: Layout) -> Fx {
        Fx { l, b: write_env(&fixture(), l).unwrap(), ps: 4096, hdr: l.page_header_size() }
    }

    fn root(&self, name: &[u8]) -> u64 {
        let env = Env::open(&self.b).unwrap();
        if name.is_empty() {
            env.main_db().root.unwrap()
        } else {
            env.open_db(name).unwrap().root.unwrap()
        }
    }

    /// File offset of node `i` on page `pgno`.
    fn node(&self, pgno: u64, i: usize) -> usize {
        let p = pgno as usize * self.ps;
        p + u16::from_le_bytes([self.b[p + self.hdr + 2 * i], self.b[p + self.hdr + 2 * i + 1]]) as usize
    }

    /// File offset of the data of node `i` on page `pgno`.
    fn node_data(&self, pgno: u64, i: usize) -> usize {
        let n = self.node(pgno, i);
        n + 8 + u16::from_le_bytes([self.b[n + 6], self.b[n + 7]]) as usize
    }

    fn set16(&mut self, off: usize, v: u16) {
        self.b[off..off + 2].copy_from_slice(&v.to_le_bytes());
    }

    fn set_word(&mut self, off: usize, v: u64) {
        self.l.put_word(&mut self.b, off, v);
    }

    /// Page-header field offset (`pad` = -8, `flags` = -6, `lower` = -4, `upper` = -2 from the header end).
    fn hdr_field(&self, pgno: u64, back: usize) -> usize {
        pgno as usize * self.ps + self.hdr - back
    }

    /// Offset of the `md_root` word inside an `MDB_db` at `db`.
    fn db_root(&self, db: usize) -> usize {
        db + 8 + 4 * self.l.word()
    }

    /// Iterates database `name` (empty = main) fully, returning the first error.
    fn walk(&self, name: &[u8]) -> Result<usize> {
        let env = Env::open(&self.b)?;
        let db = if name.is_empty() { env.main_db() } else { env.open_db(name)? };
        let mut n = 0;
        for e in env.iter(&db) {
            e?;
            n += 1;
        }
        Ok(n)
    }
}

fn bad_page(r: Result<usize>, needle: &str) {
    match r {
        Err(Error::BadPage { reason, .. }) => assert!(reason.contains(needle), "{reason:?} lacks {needle:?}"),
        other => panic!("expected BadPage({needle}), got {other:?}"),
    }
}

#[test]
fn roundtrip_both_layouts() {
    let mut expected = fixture();
    for db in std::iter::once(&mut expected.main).chain(expected.named.values_mut()) {
        let f = db.flags;
        db.entries.sort_by(|a, b| f.cmp_keys(&a.0, &b.0).then_with(|| f.cmp_dups(&a.1, &b.1)));
    }
    for l in LAYOUTS {
        let fx = Fx::new(l);
        let env = Env::open(&fx.b).unwrap();
        assert_eq!(env.layout(), l);
        assert_eq!(env.meta_index(), 1);
        assert_eq!(env.metas()[0].txnid, 0);
        assert_eq!(env.metas()[0].main_db.root, None);
        assert_eq!(env.last_txnid(), 1);
        assert_eq!(env.last_pgno() as usize, fx.b.len() / 4096 - 1);
        assert_eq!(env.env_flags(), DbFlags::INTEGERKEY);
        assert_eq!(env.free_db().root, None);
        assert_eq!(env.map_size(), 1 << 20);
        assert_eq!(env.read_all().unwrap(), expected);
        let names: Vec<&[u8]> = env.named_dbs().unwrap().into_iter().map(|(n, _)| n).collect();
        assert_eq!(names, [&b"a"[..], b"dup", b"fixed", b"many"]);
        let many = env.open_db(b"many").unwrap();
        assert_eq!(many.depth, 2);
        assert_eq!(many.entries, 500);
        let fixed = env.open_db(b"fixed").unwrap();
        assert_eq!(fixed.entries, 3002);
        assert!(matches!(env.open_db(b"nope"), Err(Error::NoSuchDb(n)) if n == "nope"));
        // Page accounting (as LMDB's audit, mdb.c:2018): every page after the metas belongs to exactly one tree.
        let mut pages = 2 + env.main_db().leaf_pages + env.main_db().branch_pages;
        for (_, d) in env.named_dbs().unwrap() {
            pages += d.leaf_pages + d.branch_pages + d.overflow_pages + dup_subdb_pages(&env, &d);
        }
        assert_eq!(pages, env.last_pgno() + 1);
        // Converting to the other layout and back is lossless and deterministic.
        let other = if l == Layout::Bits32 { Layout::Bits64 } else { Layout::Bits32 };
        let there = convert(&fx.b, other).unwrap();
        assert_eq!(convert(&there, l).unwrap(), fx.b);
    }
}

/// Pages of the sorted-duplicate sub-DBs of `db` (counted in their own records). Fixture DBs have depth 1.
fn dup_subdb_pages(env: &Env, db: &DbRecord) -> u64 {
    if !db.flags.has(DbFlags::DUPSORT) {
        return 0;
    }
    assert_eq!(db.depth, 1);
    let l = env.layout();
    let p = env.page(db.root.unwrap()).unwrap();
    (0..p.n)
        .map(|i| p.node(l.page_header_size(), i).unwrap())
        .filter(|n| n.flags & node_flags::SUBDATA != 0)
        .map(|n| DbRecord::decode(l, n.rest))
        .map(|r| r.leaf_pages + r.branch_pages + r.overflow_pages)
        .sum()
}

#[test]
fn open_errors() {
    assert!(matches!(Env::open(&[]), Err(Error::NotLmdb)));
    for l in LAYOUTS {
        let fx = Fx::new(l);
        let h = fx.hdr;
        assert!(matches!(Env::open(&fx.b[..h + 10]), Err(Error::Truncated { .. })));
        assert!(matches!(Env::open(&fx.b[..4096 + 10]), Err(Error::Truncated { .. })));
        let mut f = Fx::new(l);
        f.set16(h - 6, 0);
        assert!(matches!(Env::open(&f.b), Err(Error::BadMeta { index: 0, .. })));
        let mut f = Fx::new(l);
        f.b[4096 + h..4096 + h + 4].fill(0);
        assert!(matches!(Env::open(&f.b), Err(Error::BadMeta { index: 1, .. })));
        let mut f = Fx::new(l);
        f.b[h + 4] = 2;
        assert!(matches!(Env::open(&f.b), Err(Error::BadMeta { index: 0, .. })));
        let mut f = Fx::new(l);
        let pad = h + 8 + 2 * l.word();
        f.b[pad..pad + 4].copy_from_slice(&1000u32.to_le_bytes());
        assert!(matches!(Env::open(&f.b), Err(Error::BadPageSize(1000))));
    }
}

#[test]
fn record_and_page_errors() {
    for l in LAYOUTS {
        let fx = Fx::new(l);
        let main = fx.root(b"");
        let a = fx.root(b"a");

        // Named-DB record of the wrong size.
        let mut f = Fx::new(l);
        let n = f.node(main, 0);
        f.set16(n, 5);
        bad_page(Env::open(&f.b).unwrap().named_dbs().map(|v| v.len()), "sub-DB record");
        bad_page(Env::open(&f.b).unwrap().read_all().map(|_| 0), "sub-DB record");
        assert!(matches!(convert(&f.b, l), Err(Error::BadPage { .. })));
        assert!(matches!(convert(&[], l), Err(Error::NotLmdb)));

        // Root page number outside the file's pages.
        let mut f = Fx::new(l);
        let d = f.node_data(main, 0);
        let r = f.db_root(d);
        f.set_word(r, 1);
        bad_page(f.walk(b"a"), "outside");

        // Page header with the wrong number.
        let mut f = Fx::new(l);
        f.set_word(a as usize * 4096, 999);
        bad_page(f.walk(b"a"), "header says");

        // File shorter than last_pgno says (main root is the last page).
        let mut f = Fx::new(l);
        f.b.truncate(f.b.len() - 4096);
        assert!(matches!(f.walk(b""), Err(Error::Truncated { .. })));

        // Page flags.
        for (flags, needle) in [
            (0, "not a branch or leaf"),
            (page_flags::BRANCH | page_flags::LEAF, "not a branch or leaf"),
            (page_flags::BRANCH | page_flags::LEAF2, "bad LEAF2"),
            (page_flags::LEAF | page_flags::LEAF2, "LEAF2 page outside"),
        ] {
            let mut f = Fx::new(l);
            let o = f.hdr_field(a, 6);
            f.set16(o, flags);
            bad_page(f.walk(b"a"), needle);
        }
        let mut f = Fx::new(l);
        let o = f.hdr_field(a, 4);
        f.set16(o, 0);
        bad_page(f.walk(b"a"), "bad bounds");

        // Node pointer and key size.
        let mut f = Fx::new(l);
        let o = a as usize * 4096 + f.hdr;
        f.set16(o, 0);
        bad_page(f.walk(b"a"), "offset");
        let mut f = Fx::new(l);
        let n = f.node(a, 0);
        f.set16(n + 6, 5000);
        bad_page(f.walk(b"a"), "key overruns");
    }
}

#[test]
fn overflow_and_node_errors() {
    for l in LAYOUTS {
        let fx = Fx::new(l);
        let a = fx.root(b"a");
        let k = fx.node_data(a, 0);
        let ov = l.get_word(&fx.b, k);

        let mut f = Fx::new(l);
        f.set_word(k, 1);
        bad_page(f.walk(b"a"), "overflow page number");
        let mut f = Fx::new(l);
        f.set_word(k, a);
        bad_page(f.walk(b"a"), "expected overflow");
        let mut f = Fx::new(l);
        let o = f.hdr_field(ov, 4);
        f.b[o..o + 4].fill(0);
        bad_page(f.walk(b"a"), "too small");
        // Overflow page beyond the end of a file whose meta claims more pages.
        let mut f = Fx::new(l);
        let last = 4096 + f.hdr + 8 + 2 * l.word() + 2 * l.db_size();
        f.set_word(last, 100_000);
        f.set_word(k, 50_000);
        assert!(matches!(f.walk(b"a"), Err(Error::Truncated { .. })));
        // F_BIGDATA combined with F_SUBDATA.
        let mut f = Fx::new(l);
        let n = f.node(a, 0);
        f.set16(n + 4, node_flags::BIGDATA | node_flags::SUBDATA);
        bad_page(f.walk(b"a"), "F_BIGDATA");
        // Inline data larger than the page.
        let mut f = Fx::new(l);
        let n = f.node(a, 1);
        f.set16(n, 60000);
        bad_page(f.walk(b"a"), "overruns page");
    }
}

#[test]
fn dup_and_tree_errors() {
    for l in LAYOUTS {
        let fx = Fx::new(l);
        let dup = fx.root(b"dup");
        let fixed = fx.root(b"fixed");
        let many = fx.root(b"many");

        // Sub-page smaller than a page header.
        let mut f = Fx::new(l);
        let n = f.node(dup, 0);
        f.set16(n, 4);
        bad_page(f.walk(b"dup"), "shorter than a page header");
        // Sub-page without P_SUBP.
        let mut f = Fx::new(l);
        let sub = f.node_data(dup, 0);
        f.set16(sub + f.hdr - 6, page_flags::LEAF);
        bad_page(f.walk(b"dup"), "sub-page flags");
        // LEAF2 sub-page whose keys overrun it.
        let mut f = Fx::new(l);
        let sub = f.node_data(fixed, 0);
        f.set16(sub + f.hdr - 8, 4000);
        bad_page(f.walk(b"fixed"), "bad LEAF2");
        // Sorted-duplicate sub-DB with a bad root: error surfaces from the inner tree, then iteration ends.
        let mut f = Fx::new(l);
        let rec = f.node_data(dup, 1);
        let r = f.db_root(rec);
        f.set_word(r, 1);
        let env = Env::open(&f.b).unwrap();
        let mut it = env.iter(&env.open_db(b"dup").unwrap());
        assert!(it.by_ref().take(3).all(|e| e.is_ok()));
        assert!(matches!(it.next(), Some(Err(Error::BadPage { .. }))));
        assert!(it.next().is_none());
        // Branch child out of range.
        let mut f = Fx::new(l);
        let n = f.node(many, 0);
        f.set16(n, 1);
        f.set16(n + 2, 0);
        bad_page(f.walk(b"many"), "outside");
        // Branch pointing at itself: a cycle.
        let mut f = Fx::new(l);
        let n = f.node(many, 0);
        f.set16(n, many as u16);
        assert!(matches!(f.walk(b"many"), Err(Error::TooDeep(32))));
        // A broken node in the first leaf: `get` reports the error.
        let mut f = Fx::new(l);
        let leaf = Env::open(&f.b).unwrap().page(many).unwrap();
        let first_leaf = leaf.node(f.hdr, 1).unwrap().lo_hi as u64;
        let o = first_leaf as usize * 4096 + f.hdr;
        f.set16(o, 0);
        let env = Env::open(&f.b).unwrap();
        let m = env.open_db(b"many").unwrap();
        assert!(matches!(env.get(&m, b"k0499"), Err(Error::BadPage { .. })));
    }
}

#[test]
fn get_and_read_file() {
    let fx = Fx::new(Layout::Bits32);
    let env = Env::open(&fx.b).unwrap();
    let a = env.open_db(b"a").unwrap();
    assert_eq!(env.get(&a, b"k").unwrap().map(|v| v.len()), Some(5000));
    assert_eq!(env.get(&a, b"j").unwrap(), None);
    assert_eq!(env.get(&a, b"z").unwrap(), None);
    assert_eq!(env.get(&env.main_db(), b"m").unwrap(), Some(&b"main"[..]));
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("data.mdb");
    std::fs::write(&p, &fx.b).unwrap();
    assert_eq!(read_file(&p).unwrap(), env.read_all().unwrap());
    assert!(matches!(read_file(dir.path().join("missing")), Err(Error::Io(_))));
}

#[test]
fn writer_validation() {
    let base = || EnvContents::new(4096, 1 << 20);
    let err = |c: &EnvContents| write_env(c, Layout::Bits32).unwrap_err();

    let mut c = base();
    c.page_size = 1000;
    assert!(matches!(err(&c), Error::BadPageSize(1000)));
    let mut c = base();
    c.page_size = 65536;
    assert!(matches!(err(&c), Error::BadPageSize(65536)));

    let mut c = base();
    c.main = Database::new(DbFlags::DUPSORT);
    c.named.insert(b"x".to_vec(), Database::new(0));
    assert!(matches!(err(&c), Error::Unsupported(_)));

    let mut c = base();
    c.map_size = 1 << 33;
    assert!(matches!(err(&c), Error::TooLarge(_)));
    assert!(write_env(&c, Layout::Bits64).is_ok());

    for (flags, k, v) in [
        (0, vec![], vec![1]),
        (0, vec![1; 512], vec![1]),
        (DbFlags::DUPSORT, vec![1], vec![]),
        (DbFlags::DUPSORT, vec![1], vec![1; 512]),
    ] {
        let mut c = base();
        c.named.insert(b"n".to_vec(), Database::new(flags).with(k, v));
        assert!(matches!(err(&c), Error::BadValue(m) if m.contains("in n")));
    }
    let mut c = base();
    c.main = Database::new(0).with(vec![], "x");
    assert!(matches!(err(&c), Error::BadValue(m) if m.contains("<main>")));
    let max = u32::MAX as u64;
    assert!(super::writer::check_size(None, "value", max, 0, max).is_ok());
    assert!(matches!(super::writer::check_size(None, "value", max + 1, 0, max), Err(Error::BadValue(_))));

    // Duplicates.
    let mut c = base();
    c.main = Database::new(0).with("k", "1").with("k", "2");
    assert!(matches!(err(&c), Error::DuplicateKey { db: None, key } if key == "k"));
    let mut c = base();
    c.named.insert(b"d".to_vec(), Database::new(DbFlags::DUPSORT).with("k", "1").with("k", "1"));
    assert!(matches!(err(&c), Error::DuplicateKey { db: Some(d), .. } if d == "d"));
    let mut c = base();
    c.main = Database::new(0).with("n", "1");
    c.named.insert(b"n".to_vec(), Database::new(0));
    assert!(matches!(err(&c), Error::DuplicateKey { .. }));

    // Sizes that do not fit small pages.
    let mut c = base();
    c.page_size = 1024;
    c.named.insert(b"d".to_vec(), Database::new(DbFlags::DUPSORT).with(vec![1; 511], vec![1; 511]));
    assert!(matches!(err(&c), Error::Unsupported(_)));
    let mut c = base();
    c.page_size = 512;
    c.named.insert(b"n".to_vec(), Database::new(0).with(vec![1; 300], vec![1; 3000]));
    assert!(matches!(err(&c), Error::BadValue(m) if m.contains("does not fit")));

    // DUPFIXED values of different sizes.
    let mut c = base();
    c.named.insert(b"f".to_vec(), Database::new(DbFlags::DUPSORT | DbFlags::DUPFIXED).with("k", "1").with("k", "22"));
    assert!(matches!(err(&c), Error::BadValue(m) if m.contains("differ")));
}

#[test]
fn writer_structures() {
    for l in LAYOUTS {
        // Main DB with sorted duplicates; map size raised to the file size.
        let mut c = EnvContents::new(4096, 0);
        c.main = Database::new(DbFlags::DUPSORT | DbFlags::REVERSEDUP).with("k", "ab").with("k", "ba");
        let out = write_env(&c, l).unwrap();
        let env = Env::open(&out).unwrap();
        assert_eq!(env.map_size(), out.len() as u64);
        let all: Vec<_> = env.iter(&env.main_db()).map(|e| e.unwrap()).collect();
        assert_eq!(all, [(&b"k"[..], &b"ba"[..]), (b"k", b"ab")]);

        // Empty environment.
        let out = write_env(&EnvContents::new(4096, 1 << 20), l).unwrap();
        assert_eq!(out.len(), 2 * 4096);
        let env = Env::open(&out).unwrap();
        assert_eq!(env.iter(&env.main_db()).count(), 0);

        // Six children on a branch level of five-node pages: the last page borrows one node, so
        // no branch page is left with a single key.
        let mut c = EnvContents::new(512, 1 << 20);
        let mut db = Database::new(0);
        for i in 0..24u8 {
            db.entries.push((vec![i; 100], vec![i; 600]));
        }
        c.named.insert(b"b".to_vec(), db.clone());
        let out = write_env(&c, l).unwrap();
        let env = Env::open(&out).unwrap();
        let rec = env.open_db(b"b").unwrap();
        assert_eq!((rec.depth, rec.leaf_pages, rec.branch_pages, rec.overflow_pages), (3, 6, 3, 48));
        let root = env.page(rec.root.unwrap()).unwrap();
        assert_eq!(root.n, 2);
        for i in 0..2 {
            let child = root.node(l.page_header_size(), i).unwrap().lo_hi as u64;
            assert!(env.page(child).unwrap().n >= 2);
        }
        assert_eq!(env.read_all().unwrap().named[&b"b"[..]], db);
    }
}

#[test]
fn error_messages() {
    let errors = [
        Error::Truncated { needed: 2, len: 1 },
        Error::NotLmdb,
        Error::BadMeta { index: 1, reason: "x".into() },
        Error::BadPageSize(3),
        Error::BadPage { pgno: 4, reason: "y".into() },
        Error::TooDeep(32),
        Error::NoSuchDb("n".into()),
        Error::BadValue("v".into()),
        Error::DuplicateKey { db: None, key: "k".into() },
        Error::TooLarge("map size"),
        Error::Unsupported("u".into()),
        Error::Io(std::io::Error::other("io")),
    ];
    for e in errors {
        assert!(!e.to_string().is_empty());
    }
    let m = Meta::init(4096, 1, 8);
    assert_eq!(m.page_size(), 4096);
}
