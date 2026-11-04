//! Blockheads and their inventories (the `main` database).
//!
//! - Single player: `blockheads` = `{dynamicObjects: [ {name, uniqueID, pos_x, pos_y, floatPos, skinOptions, state,
//!   …} ]}` and `blockhead_<uniqueID>_inventory` per blockhead (plain XML).
//! - Blockheads Server: per player `<playerID>_blockheads` and `<playerID>_blockhead_<uniqueID>_inventory` (gzip).
//!
//! An inventory is an array of slots; a slot is an array of item records (`<data>`), one per item in the stack. An
//! item record is 8 bytes `{u16 type, u16 dataA, u16 dataB, u8 selected sub-slot, u8 flags}` (little-endian),
//! optionally followed by a gzip property list `{s: sub-item slots, d: object dictionary}` for containers. Item
//! bytes this tool does not name are kept.
//!
//! The `state` data of a blockhead holds its needs as little-endian `f32` in 0..1: health at 0x20, happiness at 0x24,
//! fullness (food) at 0x28, energy at 0x2c.

use std::collections::BTreeMap;

use plist::{Dictionary, Value};

use crate::error::{Error, Result};
use crate::stored::{self, Encoding};

/// One item (one entry of a stack).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item(pub Vec<u8>);

impl Item {
    /// A plain item of type `kind`.
    pub fn new(kind: u16) -> Item {
        let mut v = vec![0; 8];
        v[..2].copy_from_slice(&kind.to_le_bytes());
        Item(v)
    }

    fn u16_at(&self, o: usize) -> u16 {
        u16::from_le_bytes([self.0.get(o).copied().unwrap_or(0), self.0.get(o + 1).copied().unwrap_or(0)])
    }

    /// Item type.
    pub fn kind(&self) -> u16 {
        self.u16_at(0)
    }
    /// `dataA` (e.g. wear, colour).
    pub fn data_a(&self) -> u16 {
        self.u16_at(2)
    }
    /// `dataB`.
    pub fn data_b(&self) -> u16 {
        self.u16_at(4)
    }
    /// True if the item carries extra data (container contents, object dictionary).
    pub fn has_extras(&self) -> bool {
        self.0.len() > 8
    }
}

/// An inventory: slots of stacks.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Inventory {
    /// Slots; an empty slot is an empty stack.
    pub slots: Vec<Vec<Item>>,
}

impl Inventory {
    /// Parses an inventory property list (an array of arrays of data).
    pub fn from_value(v: &Value) -> Result<Inventory> {
        let slots = v.as_array().ok_or_else(|| Error::Format("inventory is not an array".into()))?;
        let mut out = Vec::with_capacity(slots.len());
        for s in slots {
            let stack = s.as_array().ok_or_else(|| Error::Format("inventory slot is not an array".into()))?;
            out.push(stack.iter().filter_map(Value::as_data).map(|d| Item(d.to_vec())).collect());
        }
        Ok(Inventory { slots: out })
    }

    /// The property list form.
    pub fn to_value(&self) -> Value {
        Value::Array(
            self.slots.iter().map(|s| Value::Array(s.iter().map(|i| Value::Data(i.0.clone())).collect())).collect(),
        )
    }

    /// Fills slot `slot` with `count` items of type `kind` (count 0 empties it). The first item keeps the bytes of
    /// the slot's old first item when it has the same type.
    pub fn set_stack(&mut self, slot: usize, kind: u16, count: usize) -> Result<()> {
        let s = self.slots.get_mut(slot).ok_or_else(|| Error::Invalid(format!("no inventory slot {slot}")))?;
        let template = s.first().filter(|i| i.kind() == kind && !i.has_extras()).cloned();
        let mut stack = Vec::with_capacity(count);
        for i in 0..count {
            match (&template, i) {
                (Some(t), 0) => stack.push(t.clone()),
                (Some(t), _) => {
                    let mut it = t.clone();
                    it.0[7] = 0;
                    stack.push(it)
                }
                (None, _) => stack.push(Item::new(kind)),
            }
        }
        *s = stack;
        Ok(())
    }
}

/// A blockhead's needs, each 0..1.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stats {
    /// Health.
    pub health: f32,
    /// Happiness.
    pub happiness: f32,
    /// Fullness (food).
    pub fullness: f32,
    /// Energy.
    pub energy: f32,
}

const STAT_OFFSETS: [usize; 4] = [0x20, 0x24, 0x28, 0x2c];

impl Stats {
    /// Reads the needs from `state` data (`None` if too short).
    pub fn from_state(state: &[u8]) -> Option<Stats> {
        if state.len() < 0x30 {
            return None;
        }
        let f = |o: usize| f32::from_le_bytes(state[o..o + 4].try_into().expect("4 bytes"));
        Some(Stats { health: f(0x20), happiness: f(0x24), fullness: f(0x28), energy: f(0x2c) })
    }

    /// Writes the needs into `state` data (which must hold them).
    pub fn write(&self, state: &mut [u8]) {
        for (o, v) in STAT_OFFSETS.iter().zip([self.health, self.happiness, self.fullness, self.energy]) {
            state[*o..*o + 4].copy_from_slice(&v.to_le_bytes());
        }
    }
}

/// A blockhead as listed by [`Players::list`].
#[derive(Debug, Clone, PartialEq)]
pub struct Blockhead {
    /// Player id on a server world, `""` for the single player.
    pub player: String,
    /// `uniqueID`.
    pub unique_id: u64,
    /// Name.
    pub name: String,
    /// Tile position.
    pub pos: (i64, i64),
    /// Needs, if the state data holds them.
    pub stats: Option<Stats>,
    /// True if an inventory record exists.
    pub has_inventory: bool,
}

/// Which record a `main` key is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlayerKey {
    /// Blockheads list of a player (`""` = single player).
    Blockheads(String),
    /// Inventory of (player, blockhead uniqueID).
    Inventory(String, u64),
}

/// Classifies a `main` key.
pub fn classify(key: &str) -> Option<PlayerKey> {
    if key == "blockheads" {
        return Some(PlayerKey::Blockheads(String::new()));
    }
    if let Some(p) = key.strip_suffix("_blockheads") {
        return (!p.is_empty() && p != "local").then(|| PlayerKey::Blockheads(p.to_string()));
    }
    let rest = key.strip_suffix("_inventory")?;
    let (prefix, id) = rest.rsplit_once('_')?;
    let id: u64 = id.parse().ok()?;
    if prefix == "blockhead" {
        return Some(PlayerKey::Inventory(String::new(), id));
    }
    let p = prefix.strip_suffix("_blockhead").filter(|p| !p.is_empty())?;
    Some(PlayerKey::Inventory(p.to_string(), id))
}

fn blockheads_key(player: &str) -> String {
    if player.is_empty() {
        "blockheads".into()
    } else {
        format!("{player}_blockheads")
    }
}

fn inventory_key(player: &str, id: u64) -> String {
    if player.is_empty() {
        format!("blockhead_{id}_inventory")
    } else {
        format!("{player}_blockhead_{id}_inventory")
    }
}

#[derive(Debug, Clone)]
struct Rec<T> {
    value: T,
    enc: Encoding,
    dirty: bool,
}

/// All blockheads and inventories of a world.
#[derive(Debug, Clone, Default)]
pub struct Players {
    lists: BTreeMap<String, Rec<Vec<Dictionary>>>,
    inventories: BTreeMap<(String, u64), Rec<Inventory>>,
    /// Keys that looked like player records but could not be parsed.
    pub damaged: Vec<String>,
    /// Inventory records to delete when writing (their blockhead was removed).
    removed: Vec<String>,
}

impl Players {
    /// Reads the player records among the `main` entries.
    pub fn new(entries: &[(Vec<u8>, Vec<u8>)]) -> Players {
        let mut p = Players::default();
        for (k, v) in entries {
            let Some(key) = std::str::from_utf8(k).ok() else { continue };
            let Some(kind) = classify(key) else { continue };
            let parsed = stored::decode(v);
            let ok = match (kind, parsed) {
                (PlayerKey::Blockheads(pl), Ok((val, enc))) => val
                    .as_dictionary()
                    .and_then(|d| d.get("dynamicObjects"))
                    .and_then(Value::as_array)
                    .map(|a| {
                        let list = a.iter().map(|o| o.as_dictionary().cloned().unwrap_or_default()).collect();
                        p.lists.insert(pl, Rec { value: list, enc, dirty: false });
                    })
                    .is_some(),
                (PlayerKey::Inventory(pl, id), Ok((val, enc))) => Inventory::from_value(&val)
                    .map(|inv| {
                        p.inventories.insert((pl, id), Rec { value: inv, enc, dirty: false });
                    })
                    .is_ok(),
                (_, Err(_)) => false,
            };
            if !ok {
                p.damaged.push(key.to_string());
            }
        }
        p
    }

    /// All blockheads, single player first, then by player id.
    pub fn list(&self) -> Vec<Blockhead> {
        let mut out = Vec::new();
        for (player, rec) in &self.lists {
            for d in &rec.value {
                let int = |k: &str| d.get(k).and_then(Value::as_signed_integer).unwrap_or(0);
                let id = d.get("uniqueID").and_then(Value::as_unsigned_integer).unwrap_or(0);
                out.push(Blockhead {
                    player: player.clone(),
                    unique_id: id,
                    name: d.get("name").and_then(Value::as_string).unwrap_or_default().to_string(),
                    pos: (int("pos_x"), int("pos_y")),
                    stats: d.get("state").and_then(Value::as_data).and_then(Stats::from_state),
                    has_inventory: self.inventories.contains_key(&(player.clone(), id)),
                });
            }
        }
        out
    }

    fn dict_mut(&mut self, player: &str, id: u64) -> Result<&mut Dictionary> {
        let rec = self.lists.get_mut(player).ok_or_else(|| Error::Invalid(format!("no player {player:?}")))?;
        let d = rec
            .value
            .iter_mut()
            .find(|d| d.get("uniqueID").and_then(Value::as_unsigned_integer) == Some(id))
            .ok_or_else(|| Error::Invalid(format!("no blockhead {id}")))?;
        rec.dirty = true;
        Ok(d)
    }

    /// The stored dictionary of a blockhead.
    pub fn dict(&self, player: &str, id: u64) -> Option<&Dictionary> {
        self.lists
            .get(player)?
            .value
            .iter()
            .find(|d| d.get("uniqueID").and_then(Value::as_unsigned_integer) == Some(id))
    }

    /// Replaces the stored dictionary of a blockhead (used by undo).
    pub fn set_dict(&mut self, player: &str, id: u64, dict: Dictionary) -> Result<()> {
        *self.dict_mut(player, id)? = dict;
        Ok(())
    }

    /// Moves a blockhead to tile `(x, y)` (`pos_x`/`pos_y` and `floatPos`, standing at the tile's centre).
    pub fn set_pos(&mut self, player: &str, id: u64, x: i64, y: i64) -> Result<()> {
        let d = self.dict_mut(player, id)?;
        d.insert("pos_x".into(), Value::Integer(x.into()));
        d.insert("pos_y".into(), Value::Integer(y.into()));
        d.insert("floatPos".into(), Value::Array(vec![Value::Real(x as f64 + 0.5), Value::Real(y as f64)]));
        Ok(())
    }

    /// Sets a blockhead's needs.
    pub fn set_stats(&mut self, player: &str, id: u64, stats: Stats) -> Result<()> {
        let d = self.dict_mut(player, id)?;
        let state = d
            .get_mut("state")
            .and_then(|v| match v {
                Value::Data(b) if b.len() >= 0x30 => Some(b),
                _ => None,
            })
            .ok_or_else(|| Error::Invalid(format!("blockhead {id} has no state")))?;
        stats.write(state);
        Ok(())
    }

    /// A blockhead's inventory.
    pub fn inventory(&self, player: &str, id: u64) -> Option<&Inventory> {
        self.inventories.get(&(player.to_string(), id)).map(|r| &r.value)
    }

    /// Replaces a blockhead's inventory (created with the player's encoding if missing).
    pub fn set_inventory(&mut self, player: &str, id: u64, inv: Inventory) {
        let enc = self.lists.get(player).map_or(Encoding::CLIENT, |l| l.enc);
        let rec = self.inventories.entry((player.to_string(), id)).or_insert(Rec {
            value: Inventory::default(),
            enc,
            dirty: true,
        });
        rec.value = inv;
        rec.dirty = true;
    }

    /// True if a record changed.
    pub fn is_dirty(&self) -> bool {
        self.lists.values().any(|r| r.dirty) || self.inventories.values().any(|r| r.dirty) || !self.removed.is_empty()
    }

    /// Removes a blockhead and its inventory; returns its index in the list, its dictionary and inventory.
    pub fn remove(&mut self, player: &str, id: u64) -> Result<(usize, Dictionary, Option<Inventory>)> {
        let rec = self.lists.get_mut(player).ok_or_else(|| Error::Invalid(format!("no player {player:?}")))?;
        let index = rec
            .value
            .iter()
            .position(|d| d.get("uniqueID").and_then(Value::as_unsigned_integer) == Some(id))
            .ok_or_else(|| Error::Invalid(format!("no blockhead {id}")))?;
        let dict = rec.value.remove(index);
        rec.dirty = true;
        let inv = self.inventories.remove(&(player.to_string(), id)).map(|r| r.value);
        if inv.is_some() {
            self.removed.push(inventory_key(player, id));
        }
        Ok((index, dict, inv))
    }

    /// Puts a removed blockhead back (undo of [`Players::remove`]).
    pub fn restore(&mut self, player: &str, index: usize, dict: Dictionary, inv: Option<Inventory>) -> Result<()> {
        let rec = self.lists.get_mut(player).ok_or_else(|| Error::Invalid(format!("no player {player:?}")))?;
        let id = dict.get("uniqueID").and_then(Value::as_unsigned_integer).unwrap_or(0);
        rec.value.insert(index.min(rec.value.len()), dict);
        rec.dirty = true;
        if let Some(inv) = inv {
            let key = inventory_key(player, id);
            self.removed.retain(|k| *k != key);
            self.set_inventory(player, id, inv);
        }
        Ok(())
    }

    /// Keys of records to delete; they count as deleted afterwards.
    pub fn take_removed(&mut self) -> Vec<String> {
        std::mem::take(&mut self.removed)
    }

    /// The changed records, encoded; they count as saved afterwards.
    pub fn take_changed(&mut self) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
        let mut out = Vec::new();
        for (player, r) in self.lists.iter_mut().filter(|(_, r)| r.dirty) {
            let mut root = Dictionary::new();
            root.insert(
                "dynamicObjects".into(),
                Value::Array(r.value.iter().cloned().map(Value::Dictionary).collect()),
            );
            out.push((blockheads_key(player).into_bytes(), stored::encode(&Value::Dictionary(root), r.enc)?));
            r.dirty = false;
        }
        for ((player, id), r) in self.inventories.iter_mut().filter(|(_, r)| r.dirty) {
            out.push((inventory_key(player, *id).into_bytes(), stored::encode(&r.value.to_value(), r.enc)?));
            r.dirty = false;
        }
        Ok(out)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use bhw_save::plistio::Dialect;

    pub use crate::fixtures::blockhead;

    pub use crate::fixtures::blockheads_record as list;

    pub use crate::fixtures::inventory_record as inventory;

    #[test]
    fn keys() {
        assert_eq!(classify("blockheads"), Some(PlayerKey::Blockheads(String::new())));
        assert_eq!(classify("ab12_blockheads"), Some(PlayerKey::Blockheads("ab12".into())));
        assert_eq!(classify("local_blockheads"), None);
        assert_eq!(classify("_blockheads"), None);
        assert_eq!(classify("blockhead_183_inventory"), Some(PlayerKey::Inventory(String::new(), 183)));
        assert_eq!(classify("p1_blockhead_7_inventory"), Some(PlayerKey::Inventory("p1".into(), 7)));
        assert_eq!(classify("_blockhead_7_inventory"), None);
        assert_eq!(classify("x_7_inventory"), None);
        assert_eq!(classify("blockhead_x_inventory"), None);
        assert_eq!(classify("inventory"), None);
        assert_eq!(classify("worldv2"), None);
        assert_eq!(inventory_key("", 3), "blockhead_3_inventory");
        assert_eq!(inventory_key("p", 3), "p_blockhead_3_inventory");
        assert_eq!(blockheads_key("p"), "p_blockheads");
    }

    #[test]
    fn items() {
        let i = Item(vec![4, 0, 1, 0, 2, 0, 0, 0, 9]);
        assert_eq!((i.kind(), i.data_a(), i.data_b(), i.has_extras()), (4, 1, 2, true));
        assert_eq!(Item(vec![7]).kind(), 7);
        assert!(Inventory::from_value(&Value::Integer(1.into())).is_err());
        assert!(Inventory::from_value(&Value::Array(vec![Value::Integer(1.into())])).is_err());
        let (v, _) = stored::decode(&inventory(Encoding::CLIENT)).unwrap();
        let mut inv = Inventory::from_value(&v).unwrap();
        assert_eq!(inv.slots[0].len(), 2);
        inv.set_stack(0, 4, 3).unwrap();
        assert_eq!(inv.slots[0][0].0[7], 0x11);
        assert_eq!(inv.slots[0][2].0[7], 0);
        inv.set_stack(1, 21, 2).unwrap();
        assert_eq!(inv.slots[1], vec![Item::new(21), Item::new(21)]);
        inv.set_stack(2, 12, 0).unwrap();
        assert!(inv.slots[2].is_empty());
        assert!(inv.set_stack(9, 1, 1).is_err());
        assert_eq!(Stats::from_state(&[0; 4]), None);
    }

    #[test]
    fn players() {
        let gz = Encoding { gzip: true, binary: false, dialect: Dialect::GnuStep };
        let entries = vec![
            (
                b"blockheads".to_vec(),
                list(vec![blockhead(183, "AHMO", 37875, 517), Value::Integer(1.into())], Encoding::CLIENT),
            ),
            (b"blockhead_183_inventory".to_vec(), inventory(Encoding::CLIENT)),
            (b"p1_blockheads".to_vec(), list(vec![blockhead(7, "BEN", 10, 600)], gz)),
            (b"p1_blockhead_7_inventory".to_vec(), inventory(gz)),
            (b"p2_blockheads".to_vec(), b"broken".to_vec()),
            (b"p3_blockheads".to_vec(), stored::encode(&Value::Integer(1.into()), Encoding::CLIENT).unwrap()),
            (b"blockhead_9_inventory".to_vec(), stored::encode(&Value::Integer(1.into()), Encoding::CLIENT).unwrap()),
            (b"worldv2".to_vec(), b"ignored".to_vec()),
            (vec![0xff], b"ignored".to_vec()),
        ];
        let mut p = Players::new(&entries);
        assert_eq!(p.damaged, vec!["p2_blockheads", "p3_blockheads", "blockhead_9_inventory"]);
        let l = p.list();
        assert_eq!(l.len(), 3);
        assert_eq!(
            (l[0].name.as_str(), l[0].unique_id, l[0].pos, l[0].has_inventory),
            ("AHMO", 183, (37875, 517), true)
        );
        assert_eq!(l[0].stats.unwrap().fullness, 0.75);
        assert_eq!(l[1].unique_id, 0);
        assert_eq!(l[1].stats, None);
        assert_eq!(l[2].player, "p1");
        assert!(p.dict("", 183).is_some());
        assert!(p.dict("zz", 183).is_none());
        assert_eq!(p.inventory("p1", 7).unwrap().slots.len(), 3);
        assert!(!p.is_dirty());
        assert!(p.take_changed().unwrap().is_empty());

        p.set_pos("", 183, 100, 600).unwrap();
        assert!(p.set_pos("", 999, 1, 1).is_err());
        assert!(p.set_pos("zz", 1, 1, 1).is_err());
        let s = Stats { health: 0.1, happiness: 0.2, fullness: 0.3, energy: 0.4 };
        p.set_stats("p1", 7, s).unwrap();
        assert!(p.set_stats("", 0, s).is_err());
        let d = p.dict("p1", 7).unwrap().clone();
        let mut short = d.clone();
        short.insert("state".into(), Value::Data(vec![0; 4]));
        p.set_dict("p1", 7, short).unwrap();
        assert!(p.set_stats("p1", 7, s).is_err());
        p.set_dict("p1", 7, d).unwrap();
        let mut inv = p.inventory("", 183).unwrap().clone();
        inv.set_stack(1, 21, 5).unwrap();
        p.set_inventory("", 183, inv);
        p.set_inventory("p1", 8, Inventory::default());
        p.set_inventory("p9", 1, Inventory::default());
        assert!(p.is_dirty());
        let changed = p.take_changed().unwrap();
        assert!(!p.is_dirty());
        let keys: Vec<_> = changed.iter().map(|(k, _)| String::from_utf8(k.clone()).unwrap()).collect();
        assert_eq!(
            keys,
            vec![
                "blockheads",
                "p1_blockheads",
                "blockhead_183_inventory",
                "p1_blockhead_8_inventory",
                "p9_blockhead_1_inventory"
            ]
        );
        assert!(stored::is_gzip(&changed[1].1));
        let p2 = Players::new(&changed);
        let l = p2.list();
        assert_eq!(l[0].pos, (100, 600));
        assert_eq!(p2.dict("", 183).unwrap().get("floatPos").unwrap().as_array().unwrap()[0], Value::Real(100.5));
        assert_eq!(l[2].stats, Some(s));
        assert_eq!(p2.inventory("", 183).unwrap().slots[1].len(), 5);

        // remove and restore
        let mut p3 = p2.clone();
        let (i, d, inv) = p3.remove("", 183).unwrap();
        assert_eq!(i, 0);
        assert!(inv.is_some() && p3.is_dirty());
        assert!(p3.remove("", 183).is_err());
        assert!(p3.remove("zz", 1).is_err());
        p3.restore("", i, d.clone(), inv.clone()).unwrap();
        assert!(p3.take_removed().is_empty());
        assert_eq!(p3.list()[0].unique_id, 183);
        let (i, d, _) = p3.remove("p1", 7).unwrap();
        assert!(p3.take_removed().is_empty(), "blockhead 7 has no inventory here");
        p3.restore("p1", i, d, None).unwrap();
        assert!(p3.restore("zz", 0, Dictionary::new(), None).is_err());
        let mut p4 = p2.clone();
        p4.remove("p1", 8).unwrap_err();
    }
}
