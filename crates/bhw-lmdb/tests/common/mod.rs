//! Thin test-only wrapper around real LMDB (`lmdb-master-sys`, LMDB 0.9.70) for cross-checking.
//! Real LMDB uses the layout of the target it is compiled for ([`bhw_lmdb::Layout::native`]); run the tests with
//! `--target i686-unknown-linux-gnu` to cross-check the 32-bit layout.
#![allow(dead_code)]

use std::collections::BTreeMap;
use std::ffi::CString;
use std::path::{Path, PathBuf};
use std::ptr;

use bhw_lmdb::{Database, EnvContents};
use lmdb_master_sys as ffi;

fn ok(rc: i32, what: &str) {
    assert_eq!(rc, 0, "{what} failed: {rc}");
}

fn val(b: &[u8]) -> ffi::MDB_val {
    ffi::MDB_val { mv_size: b.len(), mv_data: b.as_ptr() as *mut _ }
}

unsafe fn bytes<'a>(v: &ffi::MDB_val) -> &'a [u8] {
    if v.mv_size == 0 {
        &[]
    } else {
        std::slice::from_raw_parts(v.mv_data as *const u8, v.mv_size)
    }
}

/// An open real-LMDB environment on a single file (`MDB_NOSUBDIR`).
pub struct RealEnv {
    env: *mut ffi::MDB_env,
    pub dir: tempfile::TempDir,
}

impl RealEnv {
    /// Opens (or creates, if `bytes` is `None`) `data.mdb` in a fresh temporary directory.
    pub fn open(bytes: Option<&[u8]>, map_size: usize, readonly: bool) -> RealEnv {
        let dir = tempfile::tempdir().unwrap();
        if let Some(b) = bytes {
            std::fs::write(dir.path().join("data.mdb"), b).unwrap();
        }
        let mut env = ptr::null_mut();
        unsafe {
            ok(ffi::mdb_env_create(&mut env), "env_create");
            ok(ffi::mdb_env_set_maxdbs(env, 64), "set_maxdbs");
            if map_size > 0 {
                ok(ffi::mdb_env_set_mapsize(env, map_size), "set_mapsize");
            }
            let path = CString::new(dir.path().join("data.mdb").to_str().unwrap()).unwrap();
            let mut flags = ffi::MDB_NOSUBDIR | ffi::MDB_NOSYNC;
            if readonly {
                flags |= ffi::MDB_RDONLY;
            }
            ok(ffi::mdb_env_open(env, path.as_ptr(), flags, 0o600), "env_open");
        }
        RealEnv { env, dir }
    }

    pub fn path(&self) -> PathBuf {
        self.dir.path().join("data.mdb")
    }

    pub fn file_bytes(&self) -> Vec<u8> {
        std::fs::read(self.path()).unwrap()
    }

    pub fn txn(&self, readonly: bool) -> Txn {
        let mut txn = ptr::null_mut();
        let flags = if readonly { ffi::MDB_RDONLY } else { 0 };
        unsafe { ok(ffi::mdb_txn_begin(self.env, ptr::null_mut(), flags, &mut txn), "txn_begin") };
        Txn { txn }
    }

    /// `mdb_env_copy2(MDB_CP_COMPACT)` into `dst`.
    pub fn copy_compact(&self, dst: &Path) {
        let p = CString::new(dst.to_str().unwrap()).unwrap();
        unsafe { ok(ffi::mdb_env_copy2(self.env, p.as_ptr(), ffi::MDB_CP_COMPACT), "env_copy2") };
    }

    /// (map size, last pgno, last txnid) from `mdb_env_info`.
    pub fn info(&self) -> (usize, usize, usize) {
        let mut info: ffi::MDB_envinfo = unsafe { std::mem::zeroed() };
        unsafe { ok(ffi::mdb_env_info(self.env, &mut info), "env_info") };
        (info.me_mapsize, info.me_last_pgno, info.me_last_txnid)
    }

    /// Reads everything through real LMDB into the crate's model.
    pub fn read_all(&self) -> EnvContents {
        let txn = self.txn(true);
        let main_dbi = txn.dbi(None, 0).unwrap();
        let main_flags = txn.flags(main_dbi);
        let mut main = Database::new(main_flags);
        let mut named = BTreeMap::new();
        for (k, v) in txn.all(main_dbi) {
            let name = CString::new(k.clone()).ok();
            match name.and_then(|n| txn.dbi(Some(n.to_bytes()), 0)) {
                Some(dbi) => {
                    let mut db = Database::new(txn.flags(dbi));
                    db.entries = txn.all(dbi);
                    named.insert(k, db);
                }
                None => main.entries.push((k, v)),
            }
        }
        let (map_size, _, _) = self.info();
        let mut c = EnvContents::new(4096, map_size as u64);
        c.main = main;
        c.named = named;
        txn.abort();
        c
    }
}

impl Drop for RealEnv {
    fn drop(&mut self) {
        unsafe { ffi::mdb_env_close(self.env) };
    }
}

pub struct Txn {
    txn: *mut ffi::MDB_txn,
}

impl Txn {
    /// Opens a DB; `None` if it does not exist or is not a named DB.
    pub fn dbi(&self, name: Option<&[u8]>, flags: u32) -> Option<ffi::MDB_dbi> {
        let c = name.map(|n| CString::new(n).unwrap());
        let mut dbi = 0;
        let rc =
            unsafe { ffi::mdb_dbi_open(self.txn, c.as_ref().map_or(ptr::null(), |c| c.as_ptr()), flags, &mut dbi) };
        (rc == 0).then_some(dbi)
    }

    pub fn flags(&self, dbi: ffi::MDB_dbi) -> u16 {
        let mut f = 0;
        unsafe { ok(ffi::mdb_dbi_flags(self.txn, dbi, &mut f), "dbi_flags") };
        f as u16
    }

    pub fn put(&self, dbi: ffi::MDB_dbi, k: &[u8], v: &[u8]) {
        let (mut k, mut v) = (val(k), val(v));
        unsafe { ok(ffi::mdb_put(self.txn, dbi, &mut k, &mut v, 0), "put") };
    }

    /// Deletes a key (all duplicates); returns false if missing and panics on any other error.
    pub fn del(&self, dbi: ffi::MDB_dbi, k: &[u8]) -> bool {
        let mut k = val(k);
        match unsafe { ffi::mdb_del(self.txn, dbi, &mut k, ptr::null_mut()) } {
            ffi::MDB_NOTFOUND => false,
            rc => {
                ok(rc, "del");
                true
            }
        }
    }

    /// All pairs in order (every duplicate).
    pub fn all(&self, dbi: ffi::MDB_dbi) -> Vec<(Vec<u8>, Vec<u8>)> {
        let mut out = Vec::new();
        unsafe {
            let mut cur = ptr::null_mut();
            ok(ffi::mdb_cursor_open(self.txn, dbi, &mut cur), "cursor_open");
            let (mut k, mut v) = (val(&[]), val(&[]));
            let mut op = ffi::MDB_FIRST;
            loop {
                let rc = ffi::mdb_cursor_get(cur, &mut k, &mut v, op);
                if rc == ffi::MDB_NOTFOUND {
                    break;
                }
                ok(rc, "cursor_get");
                out.push((bytes(&k).to_vec(), bytes(&v).to_vec()));
                op = ffi::MDB_NEXT;
            }
            ffi::mdb_cursor_close(cur);
        }
        out
    }

    pub fn commit(self) {
        unsafe { ok(ffi::mdb_txn_commit(self.txn), "txn_commit") };
        std::mem::forget(self);
    }

    pub fn abort(self) {
        unsafe { ffi::mdb_txn_abort(self.txn) };
        std::mem::forget(self);
    }
}

impl Drop for Txn {
    fn drop(&mut self) {
        unsafe { ffi::mdb_txn_abort(self.txn) };
    }
}

/// Real LMDB's view of a data file image.
pub fn real_read(bytes: &[u8]) -> EnvContents {
    RealEnv::open(Some(bytes), 0, true).read_all()
}

/// Writes `contents` with real LMDB (native layout, OS page size) and returns the file image.
pub fn real_write(contents: &EnvContents) -> Vec<u8> {
    let env = RealEnv::open(None, contents.map_size as usize, false);
    let txn = env.txn(false);
    let main = txn.dbi(None, contents.main.flags.0 as u32 | ffi::MDB_CREATE).unwrap();
    for (name, db) in &contents.named {
        let dbi = txn.dbi(Some(name), db.flags.0 as u32 | ffi::MDB_CREATE).unwrap();
        for (k, v) in &db.entries {
            txn.put(dbi, k, v);
        }
    }
    for (k, v) in &contents.main.entries {
        txn.put(main, k, v);
    }
    txn.commit();
    env.file_bytes()
}

/// Opens `bytes` with real LMDB for writing (like the game continuing to play on a converted world), adds,
/// overwrites and deletes data in several transactions, and checks real LMDB and our reader agree afterwards.
/// Also runs real LMDB's compacting copy over it.
pub fn real_append_check(bytes: &[u8]) {
    let ours = bhw_lmdb::Env::open(bytes).unwrap().read_all().unwrap();
    let env = RealEnv::open(Some(bytes), 0, false);
    let (map, _, txnid) = env.info();
    assert_eq!(map as u64, ours.map_size);
    assert_eq!(txnid, 1);
    for round in 0u32..3 {
        let txn = env.txn(false);
        for (name, db) in &ours.named {
            let dbi = txn.dbi(Some(name), 0).unwrap();
            let n = if db.flags.0 == 0 { 200 } else { 0 };
            for i in 0..n {
                let k = format!("zz-new-{round}-{i:05}");
                txn.put(dbi, k.as_bytes(), &vec![round as u8; (i as usize * 37) % 5000]);
            }
            let first = txn.all(dbi).into_iter().next().map(|(k, _)| k);
            if let Some(k) = first {
                assert!(txn.del(dbi, &k));
            }
        }
        let main = txn.dbi(None, 0).unwrap();
        if !ours.main.flags.has(bhw_lmdb::DbFlags::INTEGERKEY) {
            txn.put(main, format!("main-new-{round}").as_bytes(), b"v");
            let created = txn.dbi(Some(format!("created-{round}").as_bytes()), ffi::MDB_CREATE).unwrap();
            txn.put(created, b"k", b"v");
        }
        txn.commit();
    }
    let real = env.read_all();
    let after = env.file_bytes();
    let mine = bhw_lmdb::Env::open(&after).unwrap();
    assert_eq!(mine.last_txnid(), 4);
    let mine = mine.read_all().unwrap();
    assert!(mine.same_data(&real), "our reader disagrees with real LMDB after real LMDB appended");
    let copy = env.dir.path().join("compact.mdb");
    env.copy_compact(&copy);
    let compact = std::fs::read(&copy).unwrap();
    assert!(bhw_lmdb::Env::open(&compact).unwrap().read_all().unwrap().same_data(&real));
}
