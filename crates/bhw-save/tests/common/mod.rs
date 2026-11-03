//! Synthesized saves built with bhw-lmdb (no real game files).
#![allow(dead_code)]

use bhw_lmdb::{Database, EnvContents, Layout};
use bhw_save::blocks::MacroBlock;
use bhw_save::plistio::{gzip, to_gnustep_xml, to_xml, Dialect};
use bhw_save::FileMap;
use plist::{Dictionary, Value};

pub const CID: &str = "0123456789abcdef0123456789abcdef";
pub const SID: &str = "fedcba9876543210fedcba9876543210";

pub fn date(s: &str) -> Value {
    Value::Date(plist::Date::from_xml_format(s).unwrap())
}

pub fn record(id: &str, name: &str) -> Dictionary {
    let mut d = Dictionary::new();
    d.insert("saveID".into(), Value::String(id.into()));
    d.insert("worldName".into(), Value::String(name.into()));
    d.insert("creationDate".into(), date("2026-10-20T08:48:09Z"));
    d.insert("saveDate".into(), date("2026-10-21T10:15:46Z"));
    d.insert("expertMode".into(), Value::Boolean(true));
    d.insert("migrationComplete_1.7".into(), Value::Boolean(true));
    d.insert("worldWidthMacro".into(), Value::Integer(4.into()));
    d.insert("randomSeed".into(), Value::Integer(1792486087.into()));
    d.insert("saveVersion".into(), Value::Integer(1100.into()));
    d.insert("worldTime".into(), Value::Real(2926.0842006206512));
    d.insert("startPortalPos.x".into(), Value::Integer(40.into()));
    d.insert("startPortalPos.y".into(), Value::Integer(35.into()));
    d.insert("remoteGame".into(), Value::Boolean(false));
    d
}

pub fn blockheads(list: &[(&str, i64)]) -> Vec<u8> {
    let objs = list
        .iter()
        .map(|(n, id)| {
            let mut o = Dictionary::new();
            o.insert("name".into(), Value::String((*n).into()));
            o.insert("uniqueID".into(), Value::Integer((*id).into()));
            Value::Dictionary(o)
        })
        .collect();
    let mut d = Dictionary::new();
    d.insert("dynamicObjects".into(), Value::Array(objs));
    to_xml(&Value::Dictionary(d), Dialect::Apple).unwrap()
}

pub fn inventory() -> Vec<u8> {
    to_xml(&Value::Array(vec![Value::Data(vec![1, 0, 0, 0, 0, 0, 0, 0x11])]), Dialect::Apple).unwrap()
}

/// Blocks: (1,1) grass over dirt with a cave; (2,1) all rock; (0,0) unknown tile id 200.
pub fn blocks_db() -> Database {
    let mut a = MacroBlock::empty(1, 1);
    for ly in 0..32 {
        for lx in 0..32 {
            let t = a.tile_bytes_mut(lx, ly);
            let (front, back) = match ly {
                0..=9 => (6, 6),
                10 if lx < 4 => (2, 6), // cave: air in front of dirt
                10..=19 => (6, 6),
                20 => (27, 27),
                _ => (2, 2),
            };
            t[0] = front;
            t[1] = back;
        }
    }
    let mut b = MacroBlock::empty(2, 1);
    for ly in 0..32 {
        for lx in 0..32 {
            b.tile_bytes_mut(lx, ly)[0] = 1;
        }
    }
    let mut c = MacroBlock::empty(0, 0);
    c.tile_bytes_mut(0, 0)[0] = 200;
    Database::new(0)
        .with(b"1_1".to_vec(), a.encode())
        .with(b"2_1".to_vec(), b.encode())
        .with(b"0_0".to_vec(), c.encode())
}

pub fn dw_db() -> Database {
    let mut o = Dictionary::new();
    o.insert("uniqueID".into(), Value::Integer(3.into()));
    let mut d = Dictionary::new();
    d.insert("dynamicObjects".into(), Value::Array(vec![Value::Dictionary(o)]));
    Database::new(0).with(b"1_1/45".to_vec(), to_xml(&Value::Dictionary(d), Dialect::Apple).unwrap())
}

pub fn client_record(id: &str, name: &str) -> Vec<u8> {
    let mut r = record(id, name);
    r.insert("previewImageData".into(), Value::Data(vec![7; 320 * 320 * 4]));
    r.insert("blockheadDatasv2".into(), Value::Array(vec![Value::Data(vec![1; 44])]));
    to_xml(&Value::Dictionary(r), Dialect::Apple).unwrap()
}

pub fn client_env(id: &str, name: &str) -> EnvContents {
    let mut e = EnvContents::new(4096, 64 << 20);
    e.named.insert(b"blocks".to_vec(), blocks_db());
    e.named.insert(b"dw".to_vec(), dw_db());
    e.named.insert(
        b"main".to_vec(),
        Database::new(0)
            .with(b"worldv2".to_vec(), client_record(id, name))
            .with(b"blockheads".to_vec(), blockheads(&[("ALAN", 5)]))
            .with(b"blockhead_5_inventory".to_vec(), inventory())
            .with(b"dynamicWorldv2".to_vec(), b"<plist><dict/></plist>".to_vec()),
    );
    e
}

pub fn client_files(id: &str, name: &str) -> FileMap {
    let mut f = FileMap::new();
    f.insert("world_db/data.mdb".into(), bhw_lmdb::write_env(&client_env(id, name), Layout::Bits32).unwrap());
    f.insert("world_db/lock.mdb".into(), vec![0; 16]);
    f.insert("map/default/33_33.png".into(), b"\x89PNG fake".to_vec());
    f
}

pub fn server_env(id: &str, name: &str) -> EnvContents {
    let mut r = record(id, name);
    r.insert("hostPort".into(), Value::String("15151".into()));
    r.insert("customRulesDict".into(), Value::Dictionary(Dictionary::new()));
    let mut e = EnvContents::new(4096, 1 << 30);
    e.named.insert(b"blocks".to_vec(), blocks_db());
    e.named.insert(b"dw".to_vec(), dw_db());
    e.named.insert(
        b"main".to_vec(),
        Database::new(0)
            .with(b"worldv2".to_vec(), to_gnustep_xml(&Value::Dictionary(r)).unwrap())
            .with(b"blockheads".to_vec(), blockheads(&[]))
            .with(b"p1_blockheads".to_vec(), gzip(&blockheads(&[("SCOTT", 7), ("NOINV", 8)])))
            .with(b"p1_blockhead_7_inventory".to_vec(), gzip(&inventory())),
    );
    e
}

pub fn server_files(id: &str, name: &str) -> FileMap {
    let mut f = FileMap::new();
    f.insert("world_db/data.mdb".into(), bhw_lmdb::write_env(&server_env(id, name), Layout::Bits64).unwrap());
    let mut sdb = EnvContents::new(4096, 128 << 20);
    sdb.named.insert(
        b"main".to_vec(),
        Database::new(0).with(b"p1_info".to_vec(), b"x".to_vec()).with(b"recentPlayers".to_vec(), b"y".to_vec()),
    );
    f.insert("server_db/data.mdb".into(), bhw_lmdb::write_env(&sdb, Layout::Bits64).unwrap());
    let mut lb = EnvContents::new(4096, 1 << 30);
    lb.named.insert(b"main".to_vec(), Database::new(0).with(b"p1_1_1".to_vec(), vec![0xff; 1024]));
    f.insert("lightBlocks/data.mdb".into(), bhw_lmdb::write_env(&lb, Layout::Bits64).unwrap());
    let mut file = record(id, name);
    file.remove("randomSeed");
    f.insert("worldv2".into(), to_gnustep_xml(&Value::Dictionary(file)).unwrap());
    f.insert("adminlist.txt".into(), b"header\n".to_vec());
    f
}

/// Reads the LMDB file at `path` of a file map.
pub fn env(files: &FileMap, path: &str) -> (EnvContents, Layout) {
    bhw_save::save::env_from(files, path).unwrap().unwrap()
}

/// Same databases and pairs, ignoring entry order.
pub fn same(a: &EnvContents, b: &EnvContents) -> bool {
    let norm = |e: &EnvContents| {
        let mut e = e.clone();
        for d in e.named.values_mut() {
            d.entries.sort();
        }
        e.main.entries.sort();
        (e.main, e.named)
    };
    norm(a) == norm(b)
}
