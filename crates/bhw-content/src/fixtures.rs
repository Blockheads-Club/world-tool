//! Synthetic fixtures for tests (this crate's and, with the `test-util` feature, other crates').

#![allow(missing_docs)]

use bhw_lmdb::{Database, EnvContents};
use bhw_save::blocks::{block_key, MacroBlock};
use plist::{Dictionary, Value};

use crate::players::{Inventory, Item, Stats};
use crate::stored::{self, Encoding};

pub fn block_record(x: u32, y: u32, fg: u8, bg: u8) -> (Vec<u8>, Vec<u8>) {
    let mut b = MacroBlock::empty(x, y);
    for ly in 0..32 {
        for lx in 0..32 {
            let t = b.tile_bytes_mut(lx, ly);
            t[0] = fg;
            t[1] = bg;
        }
    }
    (block_key(x, y), b.encode())
}

pub fn object(id: u64, x: i64, y: i64) -> Value {
    let mut d = Dictionary::new();
    d.insert("uniqueID".into(), Value::Integer(id.into()));
    d.insert("pos_x".into(), Value::Integer(x.into()));
    d.insert("pos_y".into(), Value::Integer(y.into()));
    Value::Dictionary(d)
}

pub fn objects_record(objs: Vec<Value>) -> Vec<u8> {
    let mut root = Dictionary::new();
    root.insert("dynamicObjects".into(), Value::Array(objs));
    stored::encode(&Value::Dictionary(root), Encoding::CLIENT).unwrap()
}

pub fn blockhead(id: u64, name: &str, x: i64, y: i64) -> Value {
    let mut d = Dictionary::new();
    d.insert("uniqueID".into(), Value::Integer(id.into()));
    d.insert("name".into(), Value::String(name.into()));
    d.insert("pos_x".into(), Value::Integer(x.into()));
    d.insert("pos_y".into(), Value::Integer(y.into()));
    let mut state = vec![0u8; 0x6c];
    Stats { health: 1.0, happiness: 0.5, fullness: 0.75, energy: 0.25 }.write(&mut state);
    d.insert("state".into(), Value::Data(state));
    Value::Dictionary(d)
}

pub fn blockheads_record(objs: Vec<Value>, enc: Encoding) -> Vec<u8> {
    let mut root = Dictionary::new();
    root.insert("dynamicObjects".into(), Value::Array(objs));
    stored::encode(&Value::Dictionary(root), enc).unwrap()
}

pub fn inventory_record(enc: Encoding) -> Vec<u8> {
    let mut first = Item::new(4).0;
    first[7] = 0x11;
    let inv = Inventory { slots: vec![vec![Item(first), Item::new(4)], vec![], vec![Item::new(12)]] };
    stored::encode(&inv.to_value(), enc).unwrap()
}

pub fn world_record(name: &str, width: i64) -> Vec<u8> {
    let mut d = Dictionary::new();
    d.insert("worldName".into(), Value::String(name.into()));
    d.insert("saveID".into(), Value::String("0123456789abcdef0123456789abcdef".into()));
    d.insert("worldWidthMacro".into(), Value::Integer(width.into()));
    d.insert("randomSeed".into(), Value::Integer(42.into()));
    d.insert("saveVersion".into(), Value::Integer(1100.into()));
    d.insert("startPortalPos.x".into(), Value::Integer(40.into()));
    d.insert("startPortalPos.y".into(), Value::Integer(520.into()));
    d.insert("worldTime".into(), Value::Real(3000.0));
    d.insert("expertMode".into(), Value::Boolean(true));
    let t = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
    d.insert("creationDate".into(), Value::Date(t.into()));
    let before = std::time::UNIX_EPOCH - std::time::Duration::from_secs(100);
    d.insert("saveDate".into(), Value::Date(before.into()));
    stored::encode(&Value::Dictionary(d), Encoding::CLIENT).unwrap()
}

pub fn world_env() -> EnvContents {
    let mut env = EnvContents::new(4096, 64 << 20);
    env.named.insert(
        b"blocks".to_vec(),
        Database { flags: Default::default(), entries: vec![block_record(0, 16, 1, 1), block_record(1, 16, 2, 2)] },
    );
    env.named.insert(b"dw".to_vec(), Database::new(0).with("0_16/46", objects_record(vec![object(5, 3, 520)])));
    env.named.insert(
        b"main".to_vec(),
        Database::new(0)
            .with("worldv2", world_record("TEST", 4))
            .with("blockheads", blockheads_record(vec![blockhead(9, "ADA", 40, 530)], Encoding::CLIENT))
            .with("blockhead_9_inventory", inventory_record(Encoding::CLIENT))
            .with("dynamicWorldv2", b"kept".to_vec()),
    );
    env
}
