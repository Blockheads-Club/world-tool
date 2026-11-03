//! Owned, layout-independent contents of an LMDB environment.

use std::collections::BTreeMap;

use crate::format::DbFlags;

/// One database: its flags and all key/value pairs.
///
/// For `MDB_DUPSORT` databases a key appears once per duplicate value. The writer sorts entries with the
/// database's comparator, so the order here does not matter for writing; the reader returns them in order.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Database {
    /// `md_flags` (e.g. [`DbFlags::DUPSORT`]).
    pub flags: DbFlags,
    /// Key/value pairs.
    pub entries: Vec<(Vec<u8>, Vec<u8>)>,
}

impl Database {
    /// An empty database with the given `md_flags`.
    pub fn new(flags: u16) -> Database {
        Database { flags: DbFlags(flags), entries: Vec::new() }
    }

    /// Adds one pair (builder style).
    pub fn with(mut self, key: impl Into<Vec<u8>>, value: impl Into<Vec<u8>>) -> Database {
        self.entries.push((key.into(), value.into()));
        self
    }
}

/// Everything needed to recreate an environment: settings, the main DB's own pairs and all named DBs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvContents {
    /// Page size (`mm_psize`).
    pub page_size: u32,
    /// Map size (`mm_mapsize`).
    pub map_size: u64,
    /// Persistent environment flags (`mm_flags`; LMDB always sets `MDB_INTEGERKEY`, 0x08).
    pub env_flags: u16,
    /// Main (unnamed) DB: its flags and its pairs that are not named-DB records.
    pub main: Database,
    /// Named databases by name.
    pub named: BTreeMap<Vec<u8>, Database>,
}

impl EnvContents {
    /// Empty contents with LMDB's defaults for the given page and map size.
    pub fn new(page_size: u32, map_size: u64) -> EnvContents {
        EnvContents {
            page_size,
            map_size,
            env_flags: DbFlags::INTEGERKEY,
            main: Database::default(),
            named: BTreeMap::new(),
        }
    }

    /// True if both hold the same databases with the same pairs in the same order (ignores page/map size and
    /// environment flags).
    pub fn same_data(&self, other: &EnvContents) -> bool {
        self.main == other.main && self.named == other.named
    }
}
