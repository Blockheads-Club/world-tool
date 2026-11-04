//! A whole world's contents, loaded from a `world_db` and written back record by record.

use bhw_lmdb::{Database, EnvContents};

use crate::blocks::BlockStore;
use crate::error::{Error, Result};
use crate::objects::Objects;
use crate::players::Players;
use crate::record::WorldRecord;

/// The contents of one world.
#[derive(Debug, Clone)]
pub struct WorldContent {
    /// The environment as loaded; records not modelled here are written back from it unchanged.
    base: EnvContents,
    /// Terrain.
    pub blocks: BlockStore,
    /// Dynamic objects.
    pub objects: Objects,
    /// Blockheads and inventories.
    pub players: Players,
    /// The world record, if the world has one.
    pub record: Option<WorldRecord>,
    record_saved: Option<WorldRecord>,
}

fn entries<'a>(env: &'a EnvContents, db: &str) -> &'a [(Vec<u8>, Vec<u8>)] {
    env.named.get(db.as_bytes()).map_or(&[], |d| d.entries.as_slice())
}

impl WorldContent {
    /// Loads a `world_db`. Fails only if the world record exists and cannot be parsed; damaged blocks and
    /// objects are reported when they are read.
    pub fn load(env: EnvContents) -> Result<WorldContent> {
        let record = bhw_save::save::get(&env, "main", "worldv2").map(WorldRecord::decode).transpose()?;
        Ok(WorldContent {
            blocks: BlockStore::new(entries(&env, "blocks")),
            objects: Objects::new(entries(&env, "dw")),
            players: Players::new(entries(&env, "main")),
            record_saved: record.clone(),
            record,
            base: env,
        })
    }

    /// World width in tiles: from the record, else from the stored blocks.
    pub fn width(&self) -> u32 {
        let macros = self
            .record
            .as_ref()
            .and_then(WorldRecord::width_macro)
            .unwrap_or_else(|| self.blocks.positions().map(|p| p.0 + 1).max().unwrap_or(0));
        macros * bhw_save::blocks::MACRO_SIZE
    }

    /// True if anything changed since loading (or since the last [`WorldContent::to_env`]).
    pub fn is_dirty(&self) -> bool {
        self.blocks.is_dirty() || self.objects.is_dirty() || self.players.is_dirty() || self.record != self.record_saved
    }

    /// The environment with all changes applied. Unchanged records keep their original bytes and order; changed
    /// records replace their old values in place; new records are appended.
    pub fn to_env(&mut self) -> Result<EnvContents> {
        let blocks = self.blocks.entries();
        let dw = self.objects.entries()?;
        let mut main_changes = self.players.take_changed()?;
        if let Some(r) = &self.record {
            if Some(r) != self.record_saved.as_ref() {
                main_changes.push((b"worldv2".to_vec(), r.encode()?));
                self.record_saved = Some(r.clone());
            }
        }
        let base = &mut self.base;
        replace_all(base, "blocks", blocks);
        replace_all(base, "dw", dw);
        if main_changes.is_empty() {
            return Ok(self.base.clone());
        }
        let main = base.named.entry(b"main".to_vec()).or_insert_with(|| Database::new(0));
        for (k, v) in main_changes {
            match main.entries.iter_mut().find(|(key, _)| *key == k) {
                Some(e) => e.1 = v,
                None => main.entries.push((k, v)),
            }
        }
        Ok(self.base.clone())
    }

    /// `x` wrapped around the world's width.
    pub fn wrap_x(&self, x: i64) -> Result<u32> {
        let w = self.width() as i64;
        if w == 0 {
            return Err(Error::Invalid("the world has no blocks".into()));
        }
        Ok(x.rem_euclid(w) as u32)
    }
}

/// Replaces a database's entries keeping the order of keys that stay and appending new keys.
fn replace_all(env: &mut EnvContents, db: &str, new: Vec<(Vec<u8>, Vec<u8>)>) {
    if new.is_empty() && !env.named.contains_key(db.as_bytes()) {
        return;
    }
    let d = env.named.entry(db.as_bytes().to_vec()).or_insert_with(|| Database::new(0));
    let mut map: std::collections::HashMap<Vec<u8>, Vec<u8>> = new.iter().cloned().collect();
    let mut out = Vec::with_capacity(new.len());
    for (k, _) in &d.entries {
        if let Some(v) = map.remove(k) {
            out.push((k.clone(), v));
        }
    }
    for (k, v) in new {
        if map.contains_key(&k) {
            map.remove(&k);
            out.push((k, v));
        }
    }
    d.entries = out;
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use plist::Value;

    /// A small synthetic world: 4 macro blocks wide, two blocks stored, one object, one blockhead.
    pub use crate::fixtures::world_env as env;

    #[test]
    fn load_edit_save() {
        let original = env();
        let mut w = WorldContent::load(original.clone()).unwrap();
        assert_eq!(w.width(), 128);
        assert!(!w.is_dirty());
        assert_eq!(w.to_env().unwrap(), original, "no change, same bytes");
        assert_eq!(w.wrap_x(-1).unwrap(), 127);

        let mut t = w.blocks.tile(1, 512).unwrap().unwrap();
        t.set_fg(6);
        w.blocks.set_tile(1, 512, t).unwrap();
        w.players.set_pos("", 9, 1, 2).unwrap();
        w.record.as_mut().unwrap().set_name("other").unwrap();
        let mut d = w.objects.all().0[0].dict.clone();
        d.insert("x".into(), Value::Boolean(true));
        w.objects.set("0_16/46", 0, d).unwrap();
        w.objects.insert("1_16/4", 0, plist::Dictionary::new()).unwrap();
        w.players.set_inventory("", 10, Default::default());
        assert!(w.is_dirty());
        let out = w.to_env().unwrap();
        assert!(!w.is_dirty());
        let main = &out.named[&b"main"[..]].entries;
        assert_eq!(
            main.iter().map(|(k, _)| String::from_utf8(k.clone()).unwrap()).collect::<Vec<_>>(),
            vec!["worldv2", "blockheads", "blockhead_9_inventory", "dynamicWorldv2", "blockhead_10_inventory"]
        );
        assert_eq!(main[3].1, b"kept");
        assert_eq!(main[2], original.named[&b"main"[..]].entries[2], "untouched inventory keeps its bytes");
        assert_eq!(out.named[&b"dw"[..]].entries.len(), 2);
        assert_eq!(out.named[&b"blocks"[..]].entries[1], original.named[&b"blocks"[..]].entries[1]);
        let mut back = WorldContent::load(out).unwrap();
        assert_eq!(back.record.as_ref().unwrap().name(), Some("OTHER"));
        assert_eq!(back.blocks.tile(1, 512).unwrap().unwrap().fg(), 6);
        assert_eq!(back.players.list()[0].pos, (1, 2));
    }

    #[test]
    fn edge_cases() {
        let mut w = WorldContent::load(EnvContents::new(4096, 1 << 20)).unwrap();
        assert_eq!(w.width(), 0);
        assert!(w.wrap_x(3).is_err());
        assert!(w.record.is_none());
        let out = w.to_env().unwrap();
        assert!(out.named.is_empty(), "no databases are created for nothing");
        w.players.set_inventory("", 1, Default::default());
        assert!(w.to_env().unwrap().named.contains_key(&b"main"[..]));
        let mut env = env();
        env.named.get_mut(&b"main"[..]).unwrap().entries[0].1 = b"bad".to_vec();
        assert!(WorldContent::load(env).is_err());
        let mut env = self::env();
        env.named.get_mut(&b"main"[..]).unwrap().entries.remove(0);
        let w = WorldContent::load(env).unwrap();
        assert_eq!(w.width(), 64, "from the stored blocks");
        // a record replaced but unchanged is not written
        let mut w = WorldContent::load(self::env()).unwrap();
        w.record.as_mut().unwrap().set_name("TEST").unwrap();
        assert!(!w.is_dirty());
    }
}
