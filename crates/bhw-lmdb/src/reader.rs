//! Read-only access to an LMDB data file held in memory.

use std::collections::BTreeMap;

use crate::error::{bad_page, lossy, Error, Result};
use crate::format::{DbRecord, Meta};
use crate::layout::{node_flags, page_flags, Layout, CURSOR_STACK, MAX_PAGE_SIZE, MDB_DATA_VERSION, MDB_MAGIC};
use crate::layout::{MIN_PAGE_SIZE, NODE_SIZE};
use crate::model::{Database, EnvContents};

/// An opened LMDB data file (`data.mdb`) borrowed from a byte slice.
#[derive(Debug, Clone)]
pub struct Env<'a> {
    data: &'a [u8],
    layout: Layout,
    metas: [Meta; 2],
    current: usize,
}

impl<'a> Env<'a> {
    /// Opens a data file: detects the layout, validates both meta pages like `mdb_env_read_header`
    /// (`mdb.c:4285`: `P_META` flag, magic, version) and picks the one with the higher txnid
    /// (`mdb_env_pick_meta`, `mdb.c:4545`; meta 0 wins a tie).
    pub fn open(data: &'a [u8]) -> Result<Env<'a>> {
        let layout = Layout::detect(data).ok_or(Error::NotLmdb)?;
        let hdr = layout.page_header_size();
        let meta_end = hdr + layout.meta_size();
        let read = |index: usize, off: usize| -> Result<Meta> {
            let page = data
                .get(off..off + meta_end)
                .ok_or(Error::Truncated { needed: (off + meta_end) as u64, len: data.len() as u64 })?;
            let flags = u16::from_le_bytes([page[hdr - 6], page[hdr - 5]]);
            let meta = Meta::decode(layout, &page[hdr..]);
            let bad = |reason: String| Err(Error::BadMeta { index, reason });
            if flags & page_flags::META == 0 {
                return bad(format!("page flags {flags:#x} lack P_META"));
            }
            if meta.magic != MDB_MAGIC {
                return bad(format!("magic {:#x}", meta.magic));
            }
            if meta.version != MDB_DATA_VERSION {
                return bad(format!("data version {}", meta.version));
            }
            let ps = meta.page_size();
            if !ps.is_power_of_two() || !(MIN_PAGE_SIZE..=MAX_PAGE_SIZE).contains(&ps) {
                return Err(Error::BadPageSize(ps));
            }
            Ok(meta)
        };
        let m0 = read(0, 0)?;
        let m1 = read(1, m0.page_size() as usize)?;
        let current = usize::from(m1.txnid > m0.txnid);
        Ok(Env { data, layout, metas: [m0, m1], current })
    }

    /// The on-disk layout.
    pub fn layout(&self) -> Layout {
        self.layout
    }

    /// Both meta pages as stored.
    pub fn metas(&self) -> &[Meta; 2] {
        &self.metas
    }

    /// Index (0 or 1) of the meta page in use.
    pub fn meta_index(&self) -> usize {
        self.current
    }

    /// The meta page in use.
    pub fn meta(&self) -> &Meta {
        &self.metas[self.current]
    }

    /// Page size.
    pub fn page_size(&self) -> u32 {
        self.meta().page_size()
    }

    /// Map size recorded in the meta page.
    pub fn map_size(&self) -> u64 {
        self.meta().map_size
    }

    /// Id of the last committed transaction.
    pub fn last_txnid(&self) -> u64 {
        self.meta().txnid
    }

    /// Last used page number.
    pub fn last_pgno(&self) -> u64 {
        self.meta().last_pgno
    }

    /// Persistent environment flags (`mm_flags`, i.e. the free DB's `md_flags`).
    pub fn env_flags(&self) -> u16 {
        self.meta().free_db.flags.0
    }

    /// The main (unnamed) database.
    pub fn main_db(&self) -> DbRecord {
        self.meta().main_db
    }

    /// The free-page database.
    pub fn free_db(&self) -> DbRecord {
        self.meta().free_db
    }

    /// All named databases (main-DB records with `F_SUBDATA`), in key order.
    pub fn named_dbs(&self) -> Result<Vec<(&'a [u8], DbRecord)>> {
        let mut out = Vec::new();
        let mut it = self.iter(&self.main_db());
        while let Some(e) = it.next_entry() {
            let (k, v, f) = e?;
            if f & (node_flags::SUBDATA | node_flags::DUPDATA) == node_flags::SUBDATA {
                out.push((k, self.decode_db(v, it.last_pgno)?));
            }
        }
        Ok(out)
    }

    /// Looks up a named database.
    pub fn open_db(&self, name: &[u8]) -> Result<DbRecord> {
        self.named_dbs()?
            .into_iter()
            .find(|(n, _)| *n == name)
            .map(|(_, d)| d)
            .ok_or_else(|| Error::NoSuchDb(lossy(name)))
    }

    /// Iterates all key/value pairs of `db` in order. For `MDB_DUPSORT` databases every duplicate is yielded
    /// as its own pair. Named-DB records in the main DB are yielded with their raw `MDB_db` bytes (as LMDB does).
    pub fn iter(&self, db: &DbRecord) -> Iter<'a> {
        Iter { env: self.clone(), outer: Tree::new(db.root, false), dups: None, last_pgno: 0 }
    }

    /// Finds the value of `key` (the first duplicate for `MDB_DUPSORT`).
    pub fn get(&self, db: &DbRecord, key: &[u8]) -> Result<Option<&'a [u8]>> {
        for e in self.iter(db) {
            let (k, v) = e?;
            match db.flags.cmp_keys(k, key) {
                std::cmp::Ordering::Less => continue,
                std::cmp::Ordering::Equal => return Ok(Some(v)),
                std::cmp::Ordering::Greater => break,
            }
        }
        Ok(None)
    }

    /// Reads the whole environment into memory.
    pub fn read_all(&self) -> Result<EnvContents> {
        let main = self.main_db();
        let mut contents = EnvContents {
            page_size: self.page_size(),
            map_size: self.map_size(),
            env_flags: self.env_flags(),
            main: Database::new(main.flags.0),
            named: BTreeMap::new(),
        };
        let mut it = self.iter(&main);
        while let Some(e) = it.next_entry() {
            let (k, v, f) = e?;
            if f & (node_flags::SUBDATA | node_flags::DUPDATA) == node_flags::SUBDATA {
                let rec = self.decode_db(v, it.last_pgno)?;
                let mut db = Database::new(rec.flags.0);
                for e in self.iter(&rec) {
                    let (k, v) = e?;
                    db.entries.push((k.to_vec(), v.to_vec()));
                }
                contents.named.insert(k.to_vec(), db);
            } else {
                contents.main.entries.push((k.to_vec(), v.to_vec()));
            }
        }
        Ok(contents)
    }

    fn decode_db(&self, v: &[u8], pgno: u64) -> Result<DbRecord> {
        if v.len() != self.layout.db_size() {
            return bad_page(pgno, format!("sub-DB record of {} bytes", v.len()));
        }
        Ok(DbRecord::decode(self.layout, v))
    }

    /// Fetches tree page `pgno` and checks its header.
    pub(crate) fn page(&self, pgno: u64) -> Result<Page<'a>> {
        if pgno < 2 || pgno > self.last_pgno() {
            return bad_page(pgno, format!("page number outside 2..={}", self.last_pgno()));
        }
        let ps = self.page_size() as u64;
        let b = self
            .data
            .get((pgno * ps) as usize..((pgno + 1) * ps) as usize)
            .ok_or(Error::Truncated { needed: (pgno + 1) * ps, len: self.data.len() as u64 })?;
        if self.layout.get_word(b, 0) != pgno {
            return bad_page(pgno, format!("header says page {}", self.layout.get_word(b, 0)));
        }
        Page::parse(self.layout, b, pgno)
    }

    /// The value stored on overflow page `pgno` (`F_BIGDATA`).
    fn overflow(&self, pgno: u64, size: usize) -> Result<&'a [u8]> {
        if pgno < 2 || pgno > self.last_pgno() {
            return bad_page(pgno, "overflow page number out of range");
        }
        let ps = self.page_size() as usize;
        let hdr = self.layout.page_header_size();
        let start = pgno as usize * ps;
        let head = self
            .data
            .get(start..start + hdr)
            .ok_or(Error::Truncated { needed: (start + hdr) as u64, len: self.data.len() as u64 })?;
        let flags = u16::from_le_bytes([head[hdr - 6], head[hdr - 5]]);
        let pages = u32::from_le_bytes(head[hdr - 4..hdr].try_into().unwrap()) as usize;
        if flags & page_flags::OVERFLOW == 0 {
            return bad_page(pgno, format!("expected overflow page, flags {flags:#x}"));
        }
        if pages * ps < hdr + size {
            return bad_page(pgno, format!("{pages} overflow pages too small for {size} bytes"));
        }
        self.data
            .get(start + hdr..start + hdr + size)
            .ok_or(Error::Truncated { needed: (start + hdr + size) as u64, len: self.data.len() as u64 })
    }
}

/// A parsed page (or `P_SUBP` sub-page) header.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Page<'a> {
    b: &'a [u8],
    pgno: u64,
    pub(crate) flags: u16,
    lower: usize,
    pad: usize,
    pub(crate) n: usize,
}

/// One raw node.
pub(crate) struct Node<'a> {
    pub(crate) flags: u16,
    pub(crate) lo_hi: u32,
    key: &'a [u8],
    pub(crate) rest: &'a [u8],
}

impl<'a> Page<'a> {
    fn parse(layout: Layout, b: &'a [u8], pgno: u64) -> Result<Page<'a>> {
        let hdr = layout.page_header_size();
        if b.len() < hdr {
            return bad_page(pgno, "sub-page shorter than a page header");
        }
        let pad = u16::from_le_bytes([b[hdr - 8], b[hdr - 7]]) as usize;
        let flags = u16::from_le_bytes([b[hdr - 6], b[hdr - 5]]);
        let lower = u16::from_le_bytes([b[hdr - 4], b[hdr - 3]]) as usize;
        let upper = u16::from_le_bytes([b[hdr - 2], b[hdr - 1]]) as usize;
        if flags & (page_flags::BRANCH | page_flags::LEAF) == 0
            || flags & (page_flags::BRANCH | page_flags::LEAF) == (page_flags::BRANCH | page_flags::LEAF)
        {
            return bad_page(pgno, format!("not a branch or leaf page (flags {flags:#x})"));
        }
        if lower < hdr || lower > upper || upper > b.len() || (lower - hdr) % 2 != 0 {
            return bad_page(pgno, format!("bad bounds lower={lower} upper={upper}"));
        }
        let n = (lower - hdr) / 2;
        let leaf2 = flags & page_flags::LEAF2 != 0;
        if leaf2 && (flags & page_flags::LEAF == 0 || hdr + n * pad > b.len()) {
            return bad_page(pgno, "bad LEAF2 page");
        }
        Ok(Page { b, pgno, flags, lower, pad, n })
    }

    fn is_leaf(&self) -> bool {
        self.flags & page_flags::LEAF != 0
    }

    fn is_leaf2(&self) -> bool {
        self.flags & page_flags::LEAF2 != 0
    }

    /// Key `i` of a `P_LEAF2` page (`LEAF2KEY`, `mdb.c:1218`).
    fn leaf2_key(&self, hdr: usize, i: usize) -> &'a [u8] {
        &self.b[hdr + i * self.pad..hdr + (i + 1) * self.pad]
    }

    /// Node `i` (`NODEPTR`, `mdb.c:1162`).
    pub(crate) fn node(&self, hdr: usize, i: usize) -> Result<Node<'a>> {
        let ofs = u16::from_le_bytes([self.b[hdr + 2 * i], self.b[hdr + 2 * i + 1]]) as usize;
        if ofs < self.lower || ofs + NODE_SIZE > self.b.len() {
            return bad_page(self.pgno, format!("node {i} offset {ofs} out of bounds"));
        }
        let h = &self.b[ofs..ofs + NODE_SIZE];
        let u = |j: usize| u16::from_le_bytes([h[j], h[j + 1]]);
        let ksize = u(6) as usize;
        let kend = ofs + NODE_SIZE + ksize;
        if kend > self.b.len() {
            return bad_page(self.pgno, format!("node {i} key overruns page"));
        }
        Ok(Node {
            flags: u(4),
            lo_hi: u(0) as u32 | (u(2) as u32) << 16,
            key: &self.b[ofs + NODE_SIZE..kend],
            rest: &self.b[kend..],
        })
    }
}

/// Depth-first cursor over one B+tree (file pages, or a single sub-page).
#[derive(Debug)]
struct Tree<'a> {
    root: Option<u64>,
    sub: Option<Page<'a>>,
    stack: Vec<(Page<'a>, usize)>,
    started: bool,
    done: bool,
    in_dup: bool,
}

/// Key, value and leaf node flags.
type FlaggedPair<'a> = (&'a [u8], &'a [u8], u16);

/// Leaf item before value resolution: key, node flags, inline data region, node data size.
type RawItem<'a> = (&'a [u8], u16, &'a [u8], usize);

impl<'a> Tree<'a> {
    fn new(root: Option<u64>, in_dup: bool) -> Tree<'a> {
        Tree { root, sub: None, stack: Vec::new(), started: false, done: false, in_dup }
    }

    fn next(&mut self, env: &Env<'a>) -> Option<Result<(RawItem<'a>, u64)>> {
        if self.done {
            return None;
        }
        let r = self.step(env);
        if !matches!(r, Some(Ok(_))) {
            self.done = true;
        }
        r
    }

    fn push(&mut self, env: &Env<'a>, pgno: u64) -> Result<()> {
        if self.stack.len() >= CURSOR_STACK {
            return Err(Error::TooDeep(CURSOR_STACK));
        }
        let p = env.page(pgno)?;
        if p.flags & page_flags::LEAF2 != 0 && !self.in_dup {
            return bad_page(pgno, "LEAF2 page outside a sorted-duplicate sub-DB");
        }
        self.stack.push((p, 0));
        Ok(())
    }

    fn step(&mut self, env: &Env<'a>) -> Option<Result<(RawItem<'a>, u64)>> {
        let hdr = env.layout.page_header_size();
        if !self.started {
            self.started = true;
            match (self.sub, self.root) {
                (Some(p), _) => self.stack.push((p, 0)),
                (None, Some(root)) => {
                    if let Err(e) = self.push(env, root) {
                        return Some(Err(e));
                    }
                }
                (None, None) => return None,
            }
        }
        loop {
            let (page, idx) = *self.stack.last()?;
            if idx >= page.n {
                self.stack.pop();
                continue;
            }
            self.stack.last_mut().unwrap().1 += 1;
            if page.is_leaf2() {
                return Some(Ok(((page.leaf2_key(hdr, idx), 0, &[], 0), page.pgno)));
            }
            let node = match page.node(hdr, idx) {
                Ok(n) => n,
                Err(e) => return Some(Err(e)),
            };
            if page.is_leaf() {
                return Some(Ok(((node.key, node.flags, node.rest, node.lo_hi as usize), page.pgno)));
            }
            let mut child = node.lo_hi as u64;
            if env.layout == Layout::Bits64 {
                child |= (node.flags as u64) << 32;
            }
            if let Err(e) = self.push(env, child) {
                return Some(Err(e));
            }
        }
    }
}

/// Iterator over the key/value pairs of one database. Created by [`Env::iter`].
#[derive(Debug)]
pub struct Iter<'a> {
    env: Env<'a>,
    outer: Tree<'a>,
    dups: Option<(&'a [u8], Tree<'a>)>,
    last_pgno: u64,
}

impl<'a> Iter<'a> {
    /// Next pair with the leaf node flags (`F_SUBDATA` marks named-DB records).
    fn next_entry(&mut self) -> Option<Result<FlaggedPair<'a>>> {
        loop {
            if let Some((key, tree)) = &mut self.dups {
                match tree.next(&self.env) {
                    Some(Ok(((v, _, _, _), _))) => return Some(Ok((key, v, node_flags::DUPDATA))),
                    Some(Err(e)) => {
                        self.outer.done = true;
                        return Some(Err(e));
                    }
                    None => self.dups = None,
                }
            }
            let ((key, flags, rest, dsize), pgno) = match self.outer.next(&self.env)? {
                Ok(x) => x,
                Err(e) => return Some(Err(e)),
            };
            self.last_pgno = pgno;
            match self.resolve(key, flags, rest, dsize, pgno) {
                Ok(Some(v)) => return Some(Ok((key, v, flags))),
                Ok(None) => continue,
                Err(e) => {
                    self.outer.done = true;
                    return Some(Err(e));
                }
            }
        }
    }

    /// Turns a leaf node into its value, or sets up duplicate iteration (returns `None`).
    fn resolve(
        &mut self,
        key: &'a [u8],
        flags: u16,
        rest: &'a [u8],
        dsize: usize,
        pgno: u64,
    ) -> Result<Option<&'a [u8]>> {
        let layout = self.env.layout;
        if flags & node_flags::BIGDATA != 0 {
            if flags & (node_flags::SUBDATA | node_flags::DUPDATA) != 0 || rest.len() < layout.word() {
                return bad_page(pgno, "bad F_BIGDATA node");
            }
            return self.env.overflow(layout.get_word(rest, 0), dsize).map(Some);
        }
        let data = rest.get(..dsize).ok_or(Error::BadPage { pgno, reason: "node data overruns page".into() })?;
        if flags & node_flags::DUPDATA == 0 {
            return Ok(Some(data));
        }
        let tree = if flags & node_flags::SUBDATA != 0 {
            let rec = self.env.decode_db(data, pgno)?;
            Tree::new(rec.root, true)
        } else {
            let sub = Page::parse(layout, data, pgno)?;
            if sub.flags & page_flags::SUBP == 0 || !sub.is_leaf() {
                return bad_page(pgno, format!("bad sub-page flags {:#x}", sub.flags));
            }
            let mut t = Tree::new(None, true);
            t.sub = Some(sub);
            t
        };
        self.dups = Some((key, tree));
        Ok(None)
    }
}

impl<'a> Iterator for Iter<'a> {
    type Item = Result<(&'a [u8], &'a [u8])>;

    fn next(&mut self) -> Option<Self::Item> {
        self.next_entry().map(|r| r.map(|(k, v, _)| (k, v)))
    }
}

/// Convenience: reads a data file from disk into memory and parses it completely.
pub fn read_file(path: impl AsRef<std::path::Path>) -> Result<EnvContents> {
    let bytes = std::fs::read(path)?;
    Env::open(&bytes)?.read_all()
}
