//! On-disk layout parameters of an LMDB data file.
//!
//! All references are to `libraries/liblmdb/mdb.c` / `lmdb.h` of LMDB 0.9.70 (`mdb.master`, as vendored by the
//! `lmdb-master-sys` 0.2.6 crate). The data-file format (`MDB_DATA_VERSION` 1, `mdb.c:664`) is the same as the
//! LMDB 0.9.x releases.
//!
//! LMDB writes its structs in host byte order with host word sizes, so the file format depends on the build:
//!
//! * `pgno_t` and `txnid_t` are `MDB_ID` (`mdb.c:577`, `mdb.c:582`), which is `mdb_size_t` (`midl.h:46`), which
//!   is `size_t` unless `MDB_VL32` is defined (`lmdb.h:196`). Neither the game nor `lmdb-master-sys` defines
//!   `MDB_VL32`, so these are 4 bytes on 32-bit builds (the game: ARMv7 Android, 32-bit PC) and 8 bytes on 64-bit
//!   builds (Blockheads Server, x86_64).
//! * `MDB_meta.mm_address` is a `void *` (`mdb.c:1268`) and `mm_mapsize` an `mdb_size_t`.
//! * `MDB_page` (`mdb.c:1005`) is `{ pgno_t mp_pgno; uint16_t mp_pad; uint16_t mp_flags; union { struct { indx_t
//!   lower, upper; }; uint32_t pb_pages; } }`, so `PAGEHDRSZ` (`mdb.c:1060`) is 12 bytes (32-bit) or 16 bytes
//!   (64-bit) and the meta struct (and its magic `0xBEEFC0DE`, `mdb.c:661`) starts at file offset 12 or 16.
//!
//! Every field of every on-disk struct is naturally aligned in both layouts (no `uint64_t` exists in the 32-bit
//! layout), so there is no ABI padding difference between ARMv7 EABI and i386. Only little-endian files are
//! supported (all relevant platforms are little-endian).

/// Which on-disk layout an LMDB data file uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Layout {
    /// 32-bit `size_t` (The Blockheads game on Android/PC). Page header 12 bytes, magic at offset 12.
    Bits32,
    /// 64-bit `size_t` (Blockheads Server on x86_64). Page header 16 bytes, magic at offset 16.
    Bits64,
}

impl Layout {
    /// The layout real LMDB produces when compiled for the current target.
    pub const fn native() -> Layout {
        if cfg!(target_pointer_width = "64") {
            Layout::Bits64
        } else {
            Layout::Bits32
        }
    }

    /// Size in bytes of `size_t` / `pgno_t` / `txnid_t` / `mdb_size_t`.
    pub const fn word(self) -> usize {
        match self {
            Layout::Bits32 => 4,
            Layout::Bits64 => 8,
        }
    }

    /// `PAGEHDRSZ` (`mdb.c:1060`): `offsetof(MDB_page, mp_ptrs)` = word + 2 + 2 + 4.
    pub const fn page_header_size(self) -> usize {
        self.word() + 8
    }

    /// `sizeof(MDB_db)` (`mdb.c:1224`): `uint32_t md_pad; uint16_t md_flags; uint16_t md_depth;` followed by
    /// `pgno_t md_branch_pages, md_leaf_pages, md_overflow_pages; mdb_size_t md_entries; pgno_t md_root;`.
    /// 28 bytes (32-bit) or 48 bytes (64-bit).
    pub const fn db_size(self) -> usize {
        8 + 5 * self.word()
    }

    /// `sizeof(MDB_meta)` (`mdb.c:1255`): `uint32_t mm_magic, mm_version; void *mm_address; mdb_size_t
    /// mm_mapsize; MDB_db mm_dbs[2]; pgno_t mm_last_pg; txnid_t mm_txnid;`. 80 bytes (32-bit) or 136 (64-bit).
    pub const fn meta_size(self) -> usize {
        8 + 2 * self.word() + 2 * self.db_size() + 2 * self.word()
    }

    /// `me_nodemax` (`mdb.c:5141`): the largest node that is kept inline on a page;
    /// `((psize - PAGEHDRSZ) / MDB_MINKEYS) & -2) - sizeof(indx_t)` with `MDB_MINKEYS` = 2 (`mdb.c:655`).
    pub const fn node_max(self, page_size: u32) -> usize {
        (((page_size as usize - self.page_header_size()) / 2) & !1) - 2
    }

    /// `OVPAGES(size, psize)` (`mdb.c:1094`): pages needed for an overflow value of `size` bytes.
    pub const fn overflow_pages(self, size: usize, page_size: u32) -> usize {
        (self.page_header_size() - 1 + size) / page_size as usize + 1
    }

    /// Largest value an `mdb_size_t` can hold in this layout.
    pub const fn max_word(self) -> u64 {
        match self {
            Layout::Bits32 => u32::MAX as u64,
            Layout::Bits64 => u64::MAX,
        }
    }

    /// `P_INVALID` (`mdb.c:734`): `~(pgno_t)0`, the root of an empty tree.
    pub const fn invalid_pgno(self) -> u64 {
        self.max_word()
    }

    /// Detects the layout from the first bytes of a data file by looking for `MDB_MAGIC` right after the page
    /// header of meta page 0. Returns `None` if neither position holds the magic.
    pub fn detect(data: &[u8]) -> Option<Layout> {
        [Layout::Bits32, Layout::Bits64].into_iter().find(|l| {
            let off = l.page_header_size();
            data.get(off..off + 4).is_some_and(|m| u32::from_le_bytes(m.try_into().unwrap()) == MDB_MAGIC)
        })
    }

    /// Reads one word (`size_t`) at `off` (little-endian).
    pub(crate) fn get_word(self, b: &[u8], off: usize) -> u64 {
        match self {
            Layout::Bits32 => u32::from_le_bytes(b[off..off + 4].try_into().unwrap()) as u64,
            Layout::Bits64 => u64::from_le_bytes(b[off..off + 8].try_into().unwrap()),
        }
    }

    /// Writes one word (`size_t`) at `off` (little-endian). The value must fit (callers check).
    pub(crate) fn put_word(self, b: &mut [u8], off: usize, v: u64) {
        match self {
            Layout::Bits32 => b[off..off + 4].copy_from_slice(&(v as u32).to_le_bytes()),
            Layout::Bits64 => b[off..off + 8].copy_from_slice(&v.to_le_bytes()),
        }
    }
}

/// `MDB_MAGIC` (`mdb.c:661`).
pub const MDB_MAGIC: u32 = 0xBEEF_C0DE;
/// `MDB_DATA_VERSION` (`mdb.c:664`, non-`MDB_DEVEL` builds).
pub const MDB_DATA_VERSION: u32 = 1;
/// `MDB_MAXKEYSIZE` (`mdb.c:689`): keys (and `MDB_DUPSORT` data items) are 1..=511 bytes (`mdb.c:7740`).
pub const MAX_KEY_SIZE: usize = 511;
/// `CURSOR_STACK` (`mdb.c:1421`): maximum tree depth LMDB can handle. Used to detect page cycles.
pub const CURSOR_STACK: usize = 32;
/// `NODESIZE` (`mdb.c:1143`): `offsetof(MDB_node, mn_data)` = `mn_lo, mn_hi, mn_flags, mn_ksize` (4 x u16).
pub const NODE_SIZE: usize = 8;
/// Smallest page size accepted.
pub const MIN_PAGE_SIZE: u32 = 512;
/// Largest page size accepted. 64 KiB pages need `MDB_DEVEL`'s `PAGEBASE` (ITS#7713, `mdb.c:1066`).
pub const MAX_PAGE_SIZE: u32 = 32768;

/// Page flags (`mdb.c:1018`..`1026`).
pub mod page_flags {
    /// `P_BRANCH`: branch page.
    pub const BRANCH: u16 = 0x01;
    /// `P_LEAF`: leaf page.
    pub const LEAF: u16 = 0x02;
    /// `P_OVERFLOW`: first page of an overflow run.
    pub const OVERFLOW: u16 = 0x04;
    /// `P_META`: meta page.
    pub const META: u16 = 0x08;
    /// `P_DIRTY`: dirty page, also set on `P_SUBP` sub-pages.
    pub const DIRTY: u16 = 0x10;
    /// `P_LEAF2`: `MDB_DUPFIXED` leaf, fixed-size keys without node headers.
    pub const LEAF2: u16 = 0x20;
    /// `P_SUBP`: `MDB_DUPSORT` sub-page embedded in a leaf node.
    pub const SUBP: u16 = 0x40;
}

/// Node flags (`mdb.c:1129`..`1131`).
pub mod node_flags {
    /// `F_BIGDATA`: the node data is the page number of an overflow page.
    pub const BIGDATA: u16 = 0x01;
    /// `F_SUBDATA`: the node data is an `MDB_db` (named DB, or sorted-duplicate sub-DB with `F_DUPDATA`).
    pub const SUBDATA: u16 = 0x02;
    /// `F_DUPDATA`: the node has duplicates (sub-page, or sub-DB with `F_SUBDATA`).
    pub const DUPDATA: u16 = 0x04;
}

/// `EVEN(n)` (`mdb.c:740`).
pub(crate) const fn even(n: usize) -> usize {
    (n + 1) & !1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_match_c_structs() {
        assert_eq!(Layout::Bits32.page_header_size(), 12);
        assert_eq!(Layout::Bits64.page_header_size(), 16);
        assert_eq!(Layout::Bits32.db_size(), 28);
        assert_eq!(Layout::Bits64.db_size(), 48);
        assert_eq!(Layout::Bits32.meta_size(), 80);
        assert_eq!(Layout::Bits64.meta_size(), 136);
        assert_eq!(Layout::Bits32.node_max(4096), 2040);
        assert_eq!(Layout::Bits64.node_max(4096), 2038);
        assert_eq!(Layout::Bits32.overflow_pages(4084, 4096), 1);
        assert_eq!(Layout::Bits32.overflow_pages(4085, 4096), 2);
        assert_eq!(Layout::Bits64.overflow_pages(4081, 4096), 2);
        assert_eq!(Layout::Bits32.invalid_pgno(), 0xFFFF_FFFF);
        assert_eq!(Layout::Bits64.invalid_pgno(), u64::MAX);
        assert_eq!(even(7), 8);
        assert_eq!(even(8), 8);
        let n = Layout::native();
        assert_eq!(n.word(), std::mem::size_of::<usize>());
    }

    #[test]
    fn detect_and_words() {
        let mut b = vec![0u8; 32];
        assert_eq!(Layout::detect(&b), None);
        assert_eq!(Layout::detect(&b[..3]), None);
        b[12..16].copy_from_slice(&MDB_MAGIC.to_le_bytes());
        assert_eq!(Layout::detect(&b), Some(Layout::Bits32));
        let mut b = vec![0u8; 32];
        b[16..20].copy_from_slice(&MDB_MAGIC.to_le_bytes());
        assert_eq!(Layout::detect(&b), Some(Layout::Bits64));
        for l in [Layout::Bits32, Layout::Bits64] {
            l.put_word(&mut b, 0, 0x1234_5678);
            assert_eq!(l.get_word(&b, 0), 0x1234_5678);
        }
    }
}
