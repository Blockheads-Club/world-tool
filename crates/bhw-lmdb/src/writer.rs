//! Builds a fresh, compact LMDB data file, the way `mdb_env_copy2(..., MDB_CP_COMPACT)` does
//! (`mdb_env_copyfd1`, `mdb.c:10537`): meta page 0 describes an empty environment with txnid 0, meta page 1 holds
//! the real roots with txnid 1 (`mdb.c:10635`), the free DB is empty and every page after the metas is in use.
//!
//! Trees are bulk-loaded bottom-up with full pages. Node encoding follows `mdb_node_add` (`mdb.c:8458`): nodes are
//! `EVEN(NODESIZE + key + data)` bytes, stacked from the end of the page; leaf data larger than `me_nodemax`
//! (`mdb.c:5141`) goes to overflow pages (`F_BIGDATA`). The first node of every branch page has an empty key, as
//! LMDB writes it ("First branch index doesn't need key data", `mdb.c:10096`).
//!
//! Sorted duplicates (`MDB_DUPSORT`) are stored like `mdb_cursor_put` does (`mdb.c:7895`..`8000`): a single value
//! as a plain node, several values in a `P_SUBP` sub-page embedded in the node (`F_DUPDATA`), or, if that node
//! would exceed `me_nodemax`, in a sub-DB (`F_DUPDATA|F_SUBDATA`) whose keys are the values. `MDB_DUPFIXED`
//! duplicates use `P_LEAF2` pages.

use std::borrow::Cow;

use crate::error::{lossy, Error, Result};
use crate::format::{DbFlags, DbRecord, Meta};
use crate::layout::{even, node_flags, page_flags, Layout, MAX_KEY_SIZE, MAX_PAGE_SIZE, MIN_PAGE_SIZE, NODE_SIZE};
use crate::model::{Database, EnvContents};

/// Writes `contents` as a complete `data.mdb` image in the given layout.
///
/// The map size is taken from `contents` but raised to the file size if smaller (LMDB does the same on open,
/// `mdb_env_open2`). Errors if a key/value cannot be stored by LMDB, entries are duplicated, or sizes do not
/// fit the 32-bit layout.
pub fn write_env(contents: &EnvContents, layout: Layout) -> Result<Vec<u8>> {
    let ps = contents.page_size;
    if !ps.is_power_of_two() || !(MIN_PAGE_SIZE..=MAX_PAGE_SIZE).contains(&ps) {
        return Err(Error::BadPageSize(ps));
    }
    if contents.main.flags.has(DbFlags::DUPSORT) && !contents.named.is_empty() {
        return Err(Error::Unsupported("named databases inside a MDB_DUPSORT main database".into()));
    }
    let mut w = Writer {
        layout,
        psize: ps as usize,
        hdr: layout.page_header_size(),
        nodemax: layout.node_max(ps),
        out: vec![0u8; 2 * ps as usize],
    };
    let mut records = Vec::with_capacity(contents.named.len());
    for (name, db) in &contents.named {
        records.push((name.as_slice(), w.write_db(db, Some(name))?.encode(layout)));
    }
    let mut main = w.write_db_with(&contents.main, None, &records)?;
    main.entries += records.len() as u64;

    let last_pgno = (w.out.len() / w.psize - 1) as u64;
    let map_size = contents.map_size.max(w.out.len() as u64);
    if map_size > layout.max_word() {
        return Err(Error::TooLarge("map size"));
    }
    let mut meta = Meta::init(ps, map_size, contents.env_flags);
    w.put_meta(0, &meta);
    meta.main_db = main;
    meta.last_pgno = last_pgno;
    meta.txnid = 1;
    w.put_meta(1, &meta);
    Ok(w.out)
}

/// Re-encodes an LMDB data file in another layout (or the same one, which compacts it), keeping page size,
/// map size, environment flags and all databases.
pub fn convert(src: &[u8], target: Layout) -> Result<Vec<u8>> {
    let contents = crate::reader::Env::open(src)?.read_all()?;
    write_env(&contents, target)
}

/// One leaf node ready to be placed on a page.
struct LeafNode<'a> {
    key: &'a [u8],
    flags: u16,
    /// `mn_lo | mn_hi << 16`: data size.
    dsize: u32,
    /// Node data bytes (inline data, overflow pgno, `MDB_db` or sub-page).
    data: Cow<'a, [u8]>,
}

/// Page statistics of one tree.
#[derive(Default)]
struct Stats {
    depth: u16,
    branch: u64,
    leaf: u64,
    overflow: u64,
    root: Option<u64>,
}

/// A page under construction.
struct PageBuf {
    b: Vec<u8>,
    hdr: usize,
    lower: usize,
    upper: usize,
    n: usize,
    flags: u16,
    pad: u16,
    /// Offsets of embedded sub-pages, whose `mp_pgno` is set to this page's number when it is placed.
    subpages: Vec<usize>,
}

impl PageBuf {
    fn new(size: usize, hdr: usize, flags: u16, pad: u16) -> PageBuf {
        PageBuf { b: vec![0; size], hdr, lower: hdr, upper: size, n: 0, flags, pad, subpages: Vec::new() }
    }

    fn room(&self) -> usize {
        self.upper - self.lower
    }

    /// Appends a node (`mdb_node_add`): pointer at `lower`, node body below `upper`.
    fn add_node(&mut self, flags: u16, lo_hi: u32, key: &[u8], data: &[u8]) {
        let size = even(NODE_SIZE + key.len() + data.len());
        self.upper -= size;
        let o = self.upper;
        self.b[self.lower..self.lower + 2].copy_from_slice(&(o as u16).to_le_bytes());
        self.lower += 2;
        self.n += 1;
        self.b[o..o + 2].copy_from_slice(&(lo_hi as u16).to_le_bytes());
        self.b[o + 2..o + 4].copy_from_slice(&((lo_hi >> 16) as u16).to_le_bytes());
        self.b[o + 4..o + 6].copy_from_slice(&flags.to_le_bytes());
        self.b[o + 6..o + 8].copy_from_slice(&(key.len() as u16).to_le_bytes());
        self.b[o + 8..o + 8 + key.len()].copy_from_slice(key);
        self.b[o + 8 + key.len()..o + 8 + key.len() + data.len()].copy_from_slice(data);
        if flags & (node_flags::DUPDATA | node_flags::SUBDATA) == node_flags::DUPDATA {
            self.subpages.push(o + 8 + key.len());
        }
    }

    /// Appends a fixed-size key to a `P_LEAF2` page (`mdb_node_add`, `mdb.c:8479`): `lower += 2`,
    /// `upper -= ksize - 2`.
    fn add_leaf2(&mut self, key: &[u8]) {
        let o = self.hdr + self.n * key.len();
        self.b[o..o + key.len()].copy_from_slice(key);
        self.n += 1;
        self.lower += 2;
        self.upper = self.upper + 2 - key.len();
    }

    /// Writes the page header (`MDB_page`, `mdb.c:1005`) and returns the bytes.
    fn finish(mut self, layout: Layout, pgno: u64) -> Vec<u8> {
        let h = self.hdr;
        layout.put_word(&mut self.b, 0, pgno);
        self.b[h - 8..h - 6].copy_from_slice(&self.pad.to_le_bytes());
        self.b[h - 6..h - 4].copy_from_slice(&self.flags.to_le_bytes());
        self.b[h - 4..h - 2].copy_from_slice(&(self.lower as u16).to_le_bytes());
        self.b[h - 2..h].copy_from_slice(&(self.upper as u16).to_le_bytes());
        for o in std::mem::take(&mut self.subpages) {
            layout.put_word(&mut self.b, o, pgno);
        }
        self.b
    }
}

struct Writer {
    layout: Layout,
    psize: usize,
    hdr: usize,
    nodemax: usize,
    out: Vec<u8>,
}

/// Checks a key/value size against LMDB's limits: keys and `MDB_DUPSORT` values are 1..=`MDB_MAXKEYSIZE`
/// (`mdb.c:7740`), other values fit the 32-bit node size field (`MAXDATASIZE`, `mdb.c:703`).
pub(crate) fn check_size(db: Option<&[u8]>, what: &str, len: u64, min: u64, max: u64) -> Result<()> {
    if len < min || len > max {
        let db = db.map(lossy).unwrap_or_else(|| "<main>".into());
        return Err(Error::BadValue(format!("{what} of {len} bytes in {db} (LMDB allows {min}..={max})")));
    }
    Ok(())
}

impl Writer {
    /// Appends `n` zeroed pages, returning the first page number.
    fn alloc(&mut self, n: usize) -> u64 {
        let pgno = (self.out.len() / self.psize) as u64;
        self.out.resize(self.out.len() + n * self.psize, 0);
        pgno
    }

    fn place(&mut self, page: PageBuf) -> u64 {
        let pgno = self.alloc(1);
        let bytes = page.finish(self.layout, pgno);
        let o = pgno as usize * self.psize;
        self.out[o..o + self.psize].copy_from_slice(&bytes);
        pgno
    }

    fn put_meta(&mut self, index: usize, meta: &Meta) {
        let o = index * self.psize;
        self.layout.put_word(&mut self.out, o, index as u64);
        let h = self.hdr;
        self.out[o + h - 6..o + h - 4].copy_from_slice(&page_flags::META.to_le_bytes());
        self.out[o + h..o + h + self.layout.meta_size()].copy_from_slice(&meta.encode(self.layout));
    }

    fn write_db(&mut self, db: &Database, name: Option<&[u8]>) -> Result<DbRecord> {
        self.write_db_with(db, name, &[])
    }

    /// Writes one database; `records` are extra named-DB records (main DB only).
    fn write_db_with(&mut self, db: &Database, name: Option<&[u8]>, records: &[(&[u8], Vec<u8>)]) -> Result<DbRecord> {
        let flags = db.flags;
        let dupsort = flags.has(DbFlags::DUPSORT);
        for (k, v) in &db.entries {
            check_size(name, "key", k.len() as u64, 1, MAX_KEY_SIZE as u64)?;
            let (what, min, max) =
                if dupsort { ("duplicate value", 1, MAX_KEY_SIZE as u64) } else { ("value", 0, u32::MAX as u64) };
            check_size(name, what, v.len() as u64, min, max)?;
        }
        // Sort: by key, then (DUPSORT) by value. Named-DB records are merged in as keys too.
        let mut items: Vec<SortItem> = db
            .entries
            .iter()
            .map(|(k, v)| (k.as_slice(), Some(v.as_slice()), None))
            .chain(records.iter().map(|(k, r)| (*k, None, Some(r.as_slice()))))
            .collect();
        items.sort_by(|a, b| {
            flags.cmp_keys(a.0, b.0).then_with(|| match (a.1, b.1) {
                (Some(x), Some(y)) if dupsort => flags.cmp_dups(x, y),
                _ => std::cmp::Ordering::Equal,
            })
        });
        let dup_err = |k: &[u8]| Error::DuplicateKey { db: name.map(lossy), key: lossy(k) };
        for pair in items.windows(2) {
            let same_key = flags.cmp_keys(pair[0].0, pair[1].0).is_eq();
            let distinct_dups = match (pair[0].1, pair[1].1) {
                (Some(x), Some(y)) if dupsort => flags.cmp_dups(x, y).is_ne(),
                _ => false,
            };
            if same_key && !distinct_dups {
                return Err(dup_err(pair[0].0));
            }
        }

        // Group duplicates and build leaf nodes.
        let mut nodes = Vec::new();
        let mut overflow = 0u64;
        let mut i = 0;
        while i < items.len() {
            let (key, value, record) = items[i];
            let mut j = i + 1;
            while j < items.len() && flags.cmp_keys(items[j].0, key).is_eq() {
                j += 1;
            }
            let node = match (record, value) {
                (Some(r), _) => LeafNode { key, flags: node_flags::SUBDATA, dsize: r.len() as u32, data: r.into() },
                (None, Some(v)) if j == i + 1 => {
                    if NODE_SIZE + key.len() + v.len() > self.nodemax {
                        if dupsort {
                            return Err(Error::Unsupported("duplicate-sorted item larger than a node".into()));
                        }
                        let pgno = self.write_overflow(v);
                        overflow += self.layout.overflow_pages(v.len(), self.psize as u32) as u64;
                        let mut d = vec![0u8; self.layout.word()];
                        self.layout.put_word(&mut d, 0, pgno);
                        LeafNode { key, flags: node_flags::BIGDATA, dsize: v.len() as u32, data: d.into() }
                    } else {
                        LeafNode { key, flags: 0, dsize: v.len() as u32, data: v.into() }
                    }
                }
                _ => {
                    let values: Vec<&[u8]> = items[i..j].iter().map(|x| x.1.unwrap()).collect();
                    self.dup_node(flags, name, key, &values)?
                }
            };
            if NODE_SIZE + node.key.len() + node.data.len() > self.nodemax {
                return Err(Error::BadValue(format!(
                    "key of {} bytes does not fit a {}-byte page",
                    key.len(),
                    self.psize
                )));
            }
            nodes.push(node);
            i = j;
        }
        let mut stats = self.build_tree(nodes.into_iter().map(Leaf::Node).collect(), page_flags::LEAF, 0);
        stats.overflow += overflow;
        Ok(DbRecord {
            pad: 0,
            flags,
            depth: stats.depth,
            branch_pages: stats.branch,
            leaf_pages: stats.leaf,
            overflow_pages: stats.overflow,
            entries: db.entries.len() as u64,
            root: stats.root,
        })
    }

    /// Writes an overflow run (`P_OVERFLOW`, `mp_pages` = `OVPAGES`) and returns its first page number.
    fn write_overflow(&mut self, v: &[u8]) -> u64 {
        let n = self.layout.overflow_pages(v.len(), self.psize as u32);
        let pgno = self.alloc(n);
        let o = pgno as usize * self.psize;
        let h = self.hdr;
        self.layout.put_word(&mut self.out, o, pgno);
        self.out[o + h - 6..o + h - 4].copy_from_slice(&page_flags::OVERFLOW.to_le_bytes());
        self.out[o + h - 4..o + h].copy_from_slice(&(n as u32).to_le_bytes());
        self.out[o + h..o + h + v.len()].copy_from_slice(v);
        pgno
    }

    /// Node for a key with several sorted duplicate values: sub-page if it fits, else sub-DB.
    fn dup_node<'a>(
        &mut self,
        flags: DbFlags,
        name: Option<&[u8]>,
        key: &'a [u8],
        values: &[&[u8]],
    ) -> Result<LeafNode<'a>> {
        let fixed = flags.has(DbFlags::DUPFIXED);
        let ksize = values[0].len();
        if fixed && values.iter().any(|v| v.len() != ksize) {
            return Err(Error::BadValue(format!(
                "MDB_DUPFIXED values of key {:?} in {:?} differ in size",
                lossy(key),
                name.map(lossy)
            )));
        }
        let size = if fixed {
            self.hdr + values.len() * ksize
        } else {
            self.hdr + values.iter().map(|v| 2 + even(NODE_SIZE + v.len())).sum::<usize>()
        };
        if NODE_SIZE + key.len() + size <= self.nodemax {
            let sub_flags = page_flags::LEAF | page_flags::DIRTY | page_flags::SUBP;
            let mut p = if fixed {
                PageBuf::new(size, self.hdr, sub_flags | page_flags::LEAF2, ksize as u16)
            } else {
                PageBuf::new(size, self.hdr, sub_flags, 0)
            };
            for v in values {
                if fixed {
                    p.add_leaf2(v);
                } else {
                    p.add_node(0, 0, v, &[]);
                }
            }
            // The sub-page's mp_pgno is patched to the leaf's page number when the leaf is placed.
            let data = p.finish(self.layout, 0);
            return Ok(LeafNode { key, flags: node_flags::DUPDATA, dsize: data.len() as u32, data: data.into() });
        }
        // Sub-DB (`prep_subDB`, mdb.c:7964): md_flags = DUPFIXED (+INTEGERKEY for INTEGERDUP), md_pad = size.
        let (leaf_flags, pad, sub_flags) = if fixed {
            let f = DbFlags::DUPFIXED | if flags.has(DbFlags::INTEGERDUP) { DbFlags::INTEGERKEY } else { 0 };
            (page_flags::LEAF | page_flags::LEAF2, ksize, f)
        } else {
            (page_flags::LEAF, 0, 0)
        };
        let leaves = values.iter().map(|v| if fixed { Leaf::Fixed(v) } else { Leaf::Node(key_only(v)) }).collect();
        let stats = self.build_tree(leaves, leaf_flags, pad);
        let rec = DbRecord {
            pad: pad as u32,
            flags: DbFlags(sub_flags),
            depth: stats.depth,
            branch_pages: stats.branch,
            leaf_pages: stats.leaf,
            overflow_pages: 0,
            entries: values.len() as u64,
            root: stats.root,
        };
        let data = rec.encode(self.layout);
        Ok(LeafNode {
            key,
            flags: node_flags::DUPDATA | node_flags::SUBDATA,
            dsize: data.len() as u32,
            data: data.into(),
        })
    }

    /// Bulk-loads a B+tree from sorted leaf entries.
    fn build_tree(&mut self, leaves: Vec<Leaf>, leaf_flags: u16, pad: usize) -> Stats {
        let mut stats = Stats::default();
        // Leaf level: (pgno, first key) of each page.
        let mut level: Vec<(u64, Vec<u8>)> = Vec::new();
        let mut page: Option<(PageBuf, Vec<u8>)> = None;
        for leaf in leaves {
            let (need, key) = match &leaf {
                Leaf::Fixed(k) => (k.len(), *k),
                Leaf::Node(n) => (2 + even(NODE_SIZE + n.key.len() + n.data.len()), n.key),
            };
            if page.as_ref().is_some_and(|(p, _)| p.room() < need) {
                let (p, first) = page.take().unwrap();
                level.push((self.place(p), first));
            }
            let (p, _) =
                page.get_or_insert_with(|| (PageBuf::new(self.psize, self.hdr, leaf_flags, pad as u16), key.to_vec()));
            match leaf {
                Leaf::Fixed(k) => p.add_leaf2(k),
                Leaf::Node(n) => p.add_node(n.flags, n.dsize, n.key, &n.data),
            }
        }
        let Some((p, first)) = page else {
            return stats;
        };
        level.push((self.place(p), first));
        stats.leaf = level.len() as u64;
        stats.depth = 1;
        while level.len() > 1 {
            level = self.branch_level(level);
            stats.branch += level.len() as u64;
            stats.depth += 1;
        }
        stats.root = Some(level[0].0);
        stats
    }

    /// Builds one branch level over `children`.
    fn branch_level(&mut self, children: Vec<(u64, Vec<u8>)>) -> Vec<(u64, Vec<u8>)> {
        // Plan groups greedily; node 0 of each page has an empty key.
        let mut groups: Vec<Vec<usize>> = Vec::new();
        let mut room = 0usize;
        for (i, (_, key)) in children.iter().enumerate() {
            let need = 2 + even(NODE_SIZE + key.len());
            match groups.last_mut() {
                Some(g) if room >= need => {
                    g.push(i);
                    room -= need;
                }
                _ => {
                    groups.push(vec![i]);
                    room = self.psize - self.hdr - (2 + NODE_SIZE);
                }
            }
        }
        // LMDB wants >= 2 keys per branch page (mdb_rebalance, mdb.c:9446): borrow one from the left neighbour.
        let len = groups.len();
        if len > 1 && groups[len - 1].len() == 1 {
            let moved = groups[len - 2].pop().unwrap();
            groups[len - 1].insert(0, moved);
        }
        let mut next = Vec::with_capacity(groups.len());
        for g in groups {
            let mut p = PageBuf::new(self.psize, self.hdr, page_flags::BRANCH, 0);
            for (j, &c) in g.iter().enumerate() {
                let (pgno, key) = &children[c];
                let key: &[u8] = if j == 0 { &[] } else { key };
                let hi = if self.layout == Layout::Bits64 { (pgno >> 32) as u16 } else { 0 };
                p.add_node(hi, *pgno as u32, key, &[]);
            }
            let first = children[g[0]].1.clone();
            next.push((self.place(p), first));
        }
        next
    }
}

/// Sort item: key, value (plain/duplicate entry) or encoded named-DB record.
type SortItem<'a> = (&'a [u8], Option<&'a [u8]>, Option<&'a [u8]>);

enum Leaf<'a> {
    Node(LeafNode<'a>),
    Fixed(&'a [u8]),
}

fn key_only(v: &[u8]) -> LeafNode<'_> {
    LeafNode { key: v, flags: 0, dsize: 0, data: Cow::Borrowed(&[]) }
}
