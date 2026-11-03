//! The game's global database `game_db/` (next to `saves/`): the world list.
//!
//! `game_db` is a 32-bit LMDB environment (map size 32 MiB) with one named database, `main`. For every world the
//! game shows in its world list it holds `"<saveID>_worldv2"`, a byte-for-byte copy of that world's
//! `world_db` `main/worldv2` record (name, dates, mode, seed, thumbnail). A world folder in `saves/` without such an
//! entry is not listed, so a world copied onto a device must also be registered here.

use std::path::Path;

use bhw_lmdb::{Database, EnvContents, Layout};
use plist::Value;

use crate::error::{io, Error, Result};
use crate::plistio::{parse_dict, to_xml, Dialect};
use crate::save::{env_bytes, map_size_for, write_atomic, GAME_DB_MAP_SIZE, PAGE_SIZE};

const SUFFIX: &str = "_worldv2";

/// `game_db` contents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameDb {
    /// The whole environment (other records are kept as they are).
    pub contents: EnvContents,
    /// Layout of the file it was read from (32-bit on Android and PC).
    pub layout: Layout,
}

impl Default for GameDb {
    fn default() -> GameDb {
        GameDb::new()
    }
}

impl GameDb {
    /// An empty `game_db` like a fresh install's.
    pub fn new() -> GameDb {
        let mut contents = EnvContents::new(PAGE_SIZE, GAME_DB_MAP_SIZE);
        contents.named.insert(b"main".to_vec(), Database::new(0));
        GameDb { contents, layout: Layout::Bits32 }
    }

    /// Parses a `data.mdb` image.
    pub fn from_bytes(bytes: &[u8]) -> Result<GameDb> {
        let env = bhw_lmdb::Env::open(bytes)?;
        Ok(GameDb { contents: env.read_all()?, layout: env.layout() })
    }

    /// Reads `<dir>/data.mdb` (`dir` is the `game_db` directory).
    pub fn open(dir: &Path) -> Result<GameDb> {
        let path = dir.join("data.mdb");
        GameDb::from_bytes(&std::fs::read(&path).map_err(io(&path))?)
    }

    /// Encodes the database (in the layout it was read in), growing the map size if the data needs it.
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let mut c = self.contents.clone();
        let data: usize = self.main().map_or(0, |d| d.entries.iter().map(|(k, v)| k.len() + v.len()).sum());
        c.map_size = map_size_for(data, c.map_size);
        env_bytes(&c, self.layout)
    }

    /// Writes `<dir>/data.mdb` (atomically; `lock.mdb` is left alone). The game must not be running.
    pub fn save(&self, dir: &Path) -> Result<()> {
        std::fs::create_dir_all(dir).map_err(io(dir))?;
        write_atomic(&dir.join("data.mdb"), &self.to_bytes()?)
    }

    fn main(&self) -> Option<&Database> {
        self.contents.named.get(&b"main"[..])
    }

    /// Ids of all registered worlds, in key order.
    pub fn world_ids(&self) -> Vec<String> {
        self.main()
            .into_iter()
            .flat_map(|d| d.entries.iter())
            .filter_map(|(k, _)| std::str::from_utf8(k).ok()?.strip_suffix(SUFFIX).map(str::to_string))
            .collect()
    }

    /// The stored world record of `id`.
    pub fn entry(&self, id: &str) -> Option<&[u8]> {
        let key = format!("{id}{SUFFIX}");
        self.main()?.entries.iter().find(|(k, _)| k == key.as_bytes()).map(|(_, v)| v.as_slice())
    }

    /// Adds or replaces the entry of `id` with a world record (the world's `main/worldv2` bytes).
    pub fn register(&mut self, id: &str, worldv2: &[u8]) {
        crate::save::put(&mut self.contents, "main", format!("{id}{SUFFIX}").as_bytes(), worldv2.to_vec());
    }

    /// Removes the entry of `id`; true if there was one.
    pub fn unregister(&mut self, id: &str) -> bool {
        crate::save::remove(&mut self.contents, "main", format!("{id}{SUFFIX}").as_bytes())
    }

    /// Changes the `worldName` of a registered world; false if `id` is not registered. Note that the world's own
    /// `main/worldv2` holds the name too (see [`crate::identity::set_identity`]); the game overwrites this entry
    /// with that record when it next saves the world.
    pub fn rename(&mut self, id: &str, name: &str) -> Result<bool> {
        let Some(old) = self.entry(id) else { return Ok(false) };
        let dialect = Dialect::detect(old);
        let mut d = parse_dict(old)?;
        d.insert("worldName".into(), Value::String(name.into()));
        let bytes = to_xml(&Value::Dictionary(d), dialect)?;
        self.register(id, &bytes);
        Ok(true)
    }

    /// Registers a world record after checking that it parses and that its `saveID` is `id`.
    pub fn register_checked(&mut self, id: &str, worldv2: &[u8]) -> Result<()> {
        let d = parse_dict(worldv2)?;
        match d.get("saveID").and_then(Value::as_string) {
            Some(s) if s == id => {
                self.register(id, worldv2);
                Ok(())
            }
            other => Err(Error::Format(format!("world record has saveID {other:?}, expected {id:?}"))),
        }
    }
}
