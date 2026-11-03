//! Synthetic backups for tests, written with this crate's own wrap/encrypt code (checked against RFC 3394 and NIST
//! vectors in `crypto`).

use std::fs;
use std::path::Path;

use plist::{Dictionary, Value};
use rusqlite::{Connection, MAIN_DB};

use crate::backup::file_id;
use crate::crypto::{aes_wrap, cbc_encrypt, pad};
use crate::keybag;
use crate::mbfile::{self, MbFile};

/// Protection class of fixture files.
pub const FILE_CLASS: u32 = 3;
/// Protection class of the fixture `ManifestKey`.
pub const MANIFEST_CLASS: u32 = 4;

/// How the fixture is encrypted.
#[derive(Clone, Copy)]
pub enum Enc {
    /// Not encrypted.
    None,
    /// iOS 10.2+ (DPSL/DPIC + ManifestKey).
    Modern(&'static str),
    /// Older keybag (single PBKDF2) and no ManifestKey (Manifest.db in clear).
    Legacy(&'static str),
}

/// One `Files` row: domain, relative path, flags, contents.
pub type Row<'a> = (&'a str, &'a str, i64, &'a [u8]);

fn class_key(class: u32) -> [u8; 32] {
    [class as u8 * 17; 32]
}

/// Builds the `Manifest.db` bytes for `rows` (MBFile blobs carry wrapped keys when `encrypted`).
pub fn manifest_db(rows: &[Row], encrypted: bool) -> Vec<u8> {
    let db = Connection::open_in_memory().unwrap();
    db.execute_batch(
        "CREATE TABLE Files (fileID TEXT PRIMARY KEY, domain TEXT, relativePath TEXT, flags INTEGER, file BLOB);
         CREATE TABLE Properties (key TEXT PRIMARY KEY, value BLOB);",
    )
    .unwrap();
    for (i, (domain, path, flags, data)) in rows.iter().enumerate() {
        let wrapped_key = encrypted.then(|| aes_wrap(&class_key(FILE_CLASS), &file_key(i)));
        let mb = MbFile { size: data.len() as u64, mtime: 1_700_000_000, protection_class: FILE_CLASS, wrapped_key };
        db.execute(
            "INSERT INTO Files VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![file_id(domain, path), domain, path, flags, mbfile::build(&mb, path)],
        )
        .unwrap();
    }
    db.serialize(MAIN_DB).unwrap().to_vec()
}

/// Per-file key of row `i`.
pub fn file_key(i: usize) -> [u8; 32] {
    [i as u8 + 100; 32]
}

/// Writes a backup into `dir`.
pub fn build(dir: &Path, enc: Enc, rows: &[Row]) {
    let encrypted = !matches!(enc, Enc::None);
    let mut manifest = Dictionary::new();
    manifest.insert("IsEncrypted".into(), Value::Boolean(encrypted));
    manifest.insert("Version".into(), Value::String("10.0".into()));
    let mut db = manifest_db(rows, encrypted);
    let classes: Vec<(u32, [u8; 32])> = (1..=4).map(|c| (c, class_key(c))).collect();
    match enc {
        Enc::None => {}
        Enc::Modern(pw) => {
            let kb = keybag::build(pw, b"salt-salt-salt-salt", 2, Some((b"dpsl-dpsl-dpsl-d", 3)), &classes);
            manifest.insert("BackupKeyBag".into(), Value::Data(kb));
            let mk = [9u8; 32];
            let mut mkv = MANIFEST_CLASS.to_le_bytes().to_vec();
            mkv.extend(aes_wrap(&class_key(MANIFEST_CLASS), &mk));
            manifest.insert("ManifestKey".into(), Value::Data(mkv));
            db = cbc_encrypt(&mk, &[0; 16], &pad(&db));
        }
        Enc::Legacy(pw) => {
            let kb = keybag::build(pw, b"salt", 3, None, &classes);
            manifest.insert("BackupKeyBag".into(), Value::Data(kb));
        }
    }
    fs::create_dir_all(dir).unwrap();
    Value::Dictionary(manifest).to_file_binary(dir.join("Manifest.plist")).unwrap();
    let mut info = Dictionary::new();
    info.insert("Device Name".into(), Value::String("Test iPhone".into()));
    info.insert("Product Version".into(), Value::String("17.0".into()));
    Value::Dictionary(info).to_file_xml(dir.join("Info.plist")).unwrap();
    fs::write(dir.join("Manifest.db"), db).unwrap();
    for (i, (domain, path, flags, data)) in rows.iter().enumerate() {
        if *flags != 1 {
            continue;
        }
        let id = file_id(domain, path);
        let stored = if encrypted { cbc_encrypt(&file_key(i), &[0; 16], &pad(data)) } else { data.to_vec() };
        fs::create_dir_all(dir.join(&id[..2])).unwrap();
        fs::write(dir.join(&id[..2]).join(&id), stored).unwrap();
    }
}
