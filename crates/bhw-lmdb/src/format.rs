//! `MDB_db` / `MDB_meta` records, database flags and key comparison.

use std::cmp::Ordering;

use crate::layout::{Layout, MDB_DATA_VERSION, MDB_MAGIC};

/// Database flags as stored in `MDB_db.md_flags` (`lmdb.h:343`..`355`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct DbFlags(pub u16);

impl DbFlags {
    /// `MDB_REVERSEKEY`: keys compared from the end.
    pub const REVERSEKEY: u16 = 0x02;
    /// `MDB_DUPSORT`: sorted duplicate values per key.
    pub const DUPSORT: u16 = 0x04;
    /// `MDB_INTEGERKEY`: keys are native (little-endian) unsigned integers.
    pub const INTEGERKEY: u16 = 0x08;
    /// `MDB_DUPFIXED`: all duplicates of a key have the same size.
    pub const DUPFIXED: u16 = 0x10;
    /// `MDB_INTEGERDUP`: duplicates are native unsigned integers.
    pub const INTEGERDUP: u16 = 0x20;
    /// `MDB_REVERSEDUP`: duplicates compared from the end.
    pub const REVERSEDUP: u16 = 0x40;

    /// True if every bit of `f` is set.
    pub const fn has(self, f: u16) -> bool {
        self.0 & f == f
    }

    /// Key comparison of a database with these flags (`mdb_default_cmp`, `mdb.c:10934`).
    pub fn cmp_keys(self, a: &[u8], b: &[u8]) -> Ordering {
        if self.has(Self::REVERSEKEY) {
            cmp_reverse(a, b)
        } else if self.has(Self::INTEGERKEY) {
            cmp_int(a, b)
        } else {
            a.cmp(b)
        }
    }

    /// Duplicate-value comparison of a `MDB_DUPSORT` database with these flags (`mdb_default_cmp`).
    pub fn cmp_dups(self, a: &[u8], b: &[u8]) -> Ordering {
        if self.has(Self::INTEGERDUP) {
            cmp_int(a, b)
        } else if self.has(Self::REVERSEDUP) {
            cmp_reverse(a, b)
        } else {
            a.cmp(b)
        }
    }
}

/// `mdb_cmp_memnr` (`mdb.c:6069`): compare bytes from the end; on a common suffix the shorter sorts first.
fn cmp_reverse(a: &[u8], b: &[u8]) -> Ordering {
    a.iter().rev().cmp(b.iter().rev())
}

/// `mdb_cmp_cint` / `mdb_cmp_int` / `mdb_cmp_long` (`mdb.c:6000`..`6045`): little-endian unsigned integers of
/// equal size. LMDB requires equal sizes; for unequal sizes we order by numeric value (then size) so the
/// result is still a total order.
fn cmp_int(a: &[u8], b: &[u8]) -> Ordering {
    let n = a.len().max(b.len());
    let at = |s: &[u8], i: usize| s.get(i).copied().unwrap_or(0);
    (0..n).rev().map(|i| at(a, i).cmp(&at(b, i))).find(|o| o.is_ne()).unwrap_or(a.len().cmp(&b.len()))
}

/// One `MDB_db` record (`mdb.c:1224`): a B+tree root and its statistics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DbRecord {
    /// `md_pad`: page size for the free DB of a meta page, fixed value size for `MDB_DUPFIXED` sub-DBs.
    pub pad: u32,
    /// `md_flags`.
    pub flags: DbFlags,
    /// `md_depth`: tree depth (0 for an empty tree).
    pub depth: u16,
    /// `md_branch_pages`.
    pub branch_pages: u64,
    /// `md_leaf_pages`.
    pub leaf_pages: u64,
    /// `md_overflow_pages`.
    pub overflow_pages: u64,
    /// `md_entries`: number of data items.
    pub entries: u64,
    /// `md_root`, `None` when `P_INVALID` (empty tree).
    pub root: Option<u64>,
}

impl DbRecord {
    /// An empty database with the given flags.
    pub const fn empty(flags: u16) -> DbRecord {
        DbRecord {
            pad: 0,
            flags: DbFlags(flags),
            depth: 0,
            branch_pages: 0,
            leaf_pages: 0,
            overflow_pages: 0,
            entries: 0,
            root: None,
        }
    }

    /// Decodes an `MDB_db` from `b` (at least `layout.db_size()` bytes).
    pub(crate) fn decode(layout: Layout, b: &[u8]) -> DbRecord {
        let w = layout.word();
        let word = |i: usize| layout.get_word(b, 8 + i * w);
        let root = word(4);
        DbRecord {
            pad: u32::from_le_bytes(b[0..4].try_into().unwrap()),
            flags: DbFlags(u16::from_le_bytes([b[4], b[5]])),
            depth: u16::from_le_bytes([b[6], b[7]]),
            branch_pages: word(0),
            leaf_pages: word(1),
            overflow_pages: word(2),
            entries: word(3),
            root: (root != layout.invalid_pgno()).then_some(root),
        }
    }

    /// Encodes to `layout.db_size()` bytes. Values must fit the layout (callers check).
    pub(crate) fn encode(&self, layout: Layout) -> Vec<u8> {
        let w = layout.word();
        let mut b = vec![0u8; layout.db_size()];
        b[0..4].copy_from_slice(&self.pad.to_le_bytes());
        b[4..6].copy_from_slice(&self.flags.0.to_le_bytes());
        b[6..8].copy_from_slice(&self.depth.to_le_bytes());
        let words = [
            self.branch_pages,
            self.leaf_pages,
            self.overflow_pages,
            self.entries,
            self.root.unwrap_or(layout.invalid_pgno()),
        ];
        for (i, v) in words.into_iter().enumerate() {
            layout.put_word(&mut b, 8 + i * w, v);
        }
        b
    }
}

/// One decoded `MDB_meta` (`mdb.c:1255`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Meta {
    /// `mm_magic`.
    pub magic: u32,
    /// `mm_version`.
    pub version: u32,
    /// `mm_address` (fixed-map address, normally 0).
    pub address: u64,
    /// `mm_mapsize`.
    pub map_size: u64,
    /// `mm_dbs[FREE_DBI]` (`mdb.c:1242`); its `pad` is the page size (`mm_psize`, `mdb.c:1273`) and its flags
    /// are the persistent environment flags (`mm_flags`).
    pub free_db: DbRecord,
    /// `mm_dbs[MAIN_DBI]` (`mdb.c:1244`).
    pub main_db: DbRecord,
    /// `mm_last_pg`: last used page number.
    pub last_pgno: u64,
    /// `mm_txnid`: transaction that committed this meta page.
    pub txnid: u64,
}

impl Meta {
    /// Page size (`mm_psize`).
    pub fn page_size(&self) -> u32 {
        self.free_db.pad
    }

    pub(crate) fn decode(layout: Layout, b: &[u8]) -> Meta {
        let w = layout.word();
        let d = layout.db_size();
        let dbs = 8 + 2 * w;
        let tail = dbs + 2 * d;
        Meta {
            magic: u32::from_le_bytes(b[0..4].try_into().unwrap()),
            version: u32::from_le_bytes(b[4..8].try_into().unwrap()),
            address: layout.get_word(b, 8),
            map_size: layout.get_word(b, 8 + w),
            free_db: DbRecord::decode(layout, &b[dbs..dbs + d]),
            main_db: DbRecord::decode(layout, &b[dbs + d..tail]),
            last_pgno: layout.get_word(b, tail),
            txnid: layout.get_word(b, tail + w),
        }
    }

    pub(crate) fn encode(&self, layout: Layout) -> Vec<u8> {
        let w = layout.word();
        let mut b = vec![0u8; layout.meta_size()];
        b[0..4].copy_from_slice(&self.magic.to_le_bytes());
        b[4..8].copy_from_slice(&self.version.to_le_bytes());
        layout.put_word(&mut b, 8, self.address);
        layout.put_word(&mut b, 8 + w, self.map_size);
        let dbs = 8 + 2 * w;
        let d = layout.db_size();
        b[dbs..dbs + d].copy_from_slice(&self.free_db.encode(layout));
        b[dbs + d..dbs + 2 * d].copy_from_slice(&self.main_db.encode(layout));
        layout.put_word(&mut b, dbs + 2 * d, self.last_pgno);
        layout.put_word(&mut b, dbs + 2 * d + w, self.txnid);
        b
    }

    /// A fresh meta like `mdb_env_init_meta0` (`mdb.c:4344`).
    pub(crate) fn init(page_size: u32, map_size: u64, env_flags: u16) -> Meta {
        let mut free_db = DbRecord::empty(env_flags);
        free_db.pad = page_size;
        Meta {
            magic: MDB_MAGIC,
            version: MDB_DATA_VERSION,
            address: 0,
            map_size,
            free_db,
            main_db: DbRecord::empty(0),
            last_pgno: 1,
            txnid: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comparators() {
        let f = DbFlags(0);
        assert_eq!(f.cmp_keys(b"ab", b"abc"), Ordering::Less);
        assert_eq!(f.cmp_keys(b"b", b"abc"), Ordering::Greater);
        let r = DbFlags(DbFlags::REVERSEKEY);
        assert_eq!(r.cmp_keys(b"za", b"ab"), Ordering::Less);
        assert_eq!(r.cmp_keys(b"a", b"ba"), Ordering::Less);
        assert_eq!(r.cmp_keys(b"ba", b"ba"), Ordering::Equal);
        let i = DbFlags(DbFlags::INTEGERKEY);
        assert_eq!(i.cmp_keys(&256u32.to_le_bytes(), &255u32.to_le_bytes()), Ordering::Greater);
        assert_eq!(i.cmp_keys(&1u32.to_le_bytes(), &1u64.to_le_bytes()), Ordering::Less);
        assert_eq!(i.cmp_keys(&7u32.to_le_bytes(), &7u32.to_le_bytes()), Ordering::Equal);
        let d = DbFlags(DbFlags::DUPSORT | DbFlags::INTEGERDUP);
        assert_eq!(d.cmp_dups(&256u32.to_le_bytes(), &255u32.to_le_bytes()), Ordering::Greater);
        let d = DbFlags(DbFlags::DUPSORT | DbFlags::REVERSEDUP);
        assert_eq!(d.cmp_dups(b"za", b"ab"), Ordering::Less);
        assert_eq!(DbFlags(DbFlags::DUPSORT).cmp_dups(b"a", b"b"), Ordering::Less);
        assert!(d.has(DbFlags::DUPSORT));
        assert!(!d.has(DbFlags::DUPFIXED));
    }

    #[test]
    fn meta_roundtrip() {
        for l in [Layout::Bits32, Layout::Bits64] {
            let mut m = Meta::init(4096, 1 << 26, 8);
            m.main_db = DbRecord { root: Some(5), entries: 3, depth: 1, leaf_pages: 1, ..DbRecord::empty(0) };
            m.txnid = 9;
            m.last_pgno = 5;
            let b = m.encode(l);
            assert_eq!(b.len(), l.meta_size());
            assert_eq!(Meta::decode(l, &b), m);
            assert_eq!(m.page_size(), 4096);
        }
    }
}
