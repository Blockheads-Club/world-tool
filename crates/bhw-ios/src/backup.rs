//! Opening a backup, reading `Manifest.db` and extracting The Blockheads files.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use bhw_world::{Payload, WorldFile, WorldFiles};
use plist::Value;
use rusqlite::{Connection, MAIN_DB};
use sha1::{Digest, Sha1};

use crate::crypto::{aes_unwrap, cbc_decrypt, unpadded_len};
use crate::info::{read_backup_info, read_plist, BackupInfo};
use crate::keybag::{ClassKeys, Keybag};
use crate::mbfile::MbFile;
use crate::{Error, Result};

/// Prefix of app container domains.
pub const APP_DOMAIN: &str = "AppDomain-";
/// The game's data directory inside its container.
pub const APP_SUPPORT: &str = "Library/Application Support/";
const ENTRIES_SQL: &str =
    "SELECT fileID, domain, relativePath, flags, file FROM Files WHERE domain = ?1 ORDER BY relativePath";
const APPS_SQL: &str = "SELECT DISTINCT domain, relativePath FROM Files WHERE domain LIKE 'AppDomain-%' \
     AND relativePath LIKE 'Library/Application Support/saves/%/world_db/data.mdb'";
/// `Files.flags` value of a regular file.
const FLAG_FILE: i64 = 1;

/// The backup file id of a domain and relative path: `sha1("<domain>-<relativePath>")` in lowercase hex.
pub fn file_id(domain: &str, relative_path: &str) -> String {
    let digest = Sha1::digest(format!("{domain}-{relative_path}").as_bytes());
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// One row of the `Files` table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestEntry {
    /// `fileID` (hex; names the stored file).
    pub file_id: String,
    /// Domain, e.g. `AppDomain-<bundle id>`.
    pub domain: String,
    /// Path inside the domain.
    pub relative_path: String,
    /// 1 = file, 2 = directory, 4 = symlink.
    pub flags: i64,
    /// Archived `MBFile`.
    pub file: Vec<u8>,
}

/// The Blockheads data found in a backup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockheadsData {
    /// Bundle id of the app the data came from.
    pub bundle_id: String,
    /// Worlds (`saves/<id>/` that contain `world_db/data.mdb`), sorted by id.
    pub worlds: Vec<WorldFiles>,
    /// Files of `game_db/` (the game's world list), paths relative to `game_db/`, sorted.
    pub game_db: Vec<WorldFile>,
}

/// An opened (and, if encrypted, unlocked) backup.
pub struct IosBackup {
    info: BackupInfo,
    keys: Option<ClassKeys>,
    db: Connection,
}

fn unwrap_key(keys: &ClassKeys, class: u32, wrapped: &[u8]) -> Result<[u8; 32]> {
    let ck = keys.get(&class).ok_or_else(|| Error::Keybag(format!("no key for protection class {class}")))?;
    aes_unwrap(ck, wrapped)
        .and_then(|k| k.try_into().ok())
        .ok_or_else(|| Error::Corrupt(format!("cannot unwrap a class {class} file key")))
}

fn decrypt(key: &[u8; 32], data: &[u8], what: &str) -> Result<Vec<u8>> {
    cbc_decrypt(key, &[0; 16], data).ok_or_else(|| Error::Corrupt(format!("{what}: length not a multiple of 16")))
}

/// Decrypts an encrypted `Manifest.db` with `ManifestKey` (4-byte little-endian class + wrapped key).
fn decrypt_manifest_db(keys: &ClassKeys, manifest_key: &[u8], data: &[u8]) -> Result<Vec<u8>> {
    if manifest_key.len() < 4 {
        return Err(Error::Corrupt("ManifestKey too short".into()));
    }
    let class = u32::from_le_bytes(manifest_key[..4].try_into().unwrap());
    let key = unwrap_key(keys, class, &manifest_key[4..])?;
    let mut plain = decrypt(&key, data, "Manifest.db")?;
    // SQLite files are a whole number of pages (>= 512 bytes); drop PKCS#7 padding only when it restores that.
    if let Some(n) = unpadded_len(&plain).filter(|n| n % 512 == 0) {
        plain.truncate(n);
    }
    Ok(plain)
}

fn open_db(bytes: &[u8]) -> Result<Connection> {
    let mut db = Connection::open_in_memory()?;
    db.deserialize_read_exact(MAIN_DB, bytes, bytes.len(), true)?;
    db.query_row("SELECT count(*) FROM Files", [], |_| Ok(()))?;
    Ok(db)
}

impl IosBackup {
    /// Opens the backup in `dir`. Encrypted backups need `password` ([`Error::PasswordRequired`],
    /// [`Error::WrongPassword`]); for unencrypted ones it is ignored. Key derivation of iOS 10.2+ backups runs
    /// 10 million PBKDF2-SHA256 rounds, so this takes seconds for encrypted backups.
    pub fn open(dir: &Path, password: Option<&str>) -> Result<IosBackup> {
        let info = read_backup_info(dir)?;
        let db_path = dir.join("Manifest.db");
        if !db_path.is_file() {
            if dir.join("Manifest.mbdb").is_file() {
                return Err(Error::Unsupported("Manifest.mbdb backup (iOS 9 or older)".into()));
            }
            return Err(Error::MissingFile("Manifest.db".into()));
        }
        let raw = fs::read(&db_path)?;
        let (keys, bytes) = if info.encrypted {
            let password = password.ok_or(Error::PasswordRequired)?;
            let manifest = read_plist(&dir.join("Manifest.plist"))?;
            let kb = manifest
                .get("BackupKeyBag")
                .and_then(Value::as_data)
                .ok_or_else(|| Error::Keybag("Manifest.plist has no BackupKeyBag".into()))?;
            let keys = Keybag::parse(kb)?.unlock(password)?;
            let bytes = match manifest.get("ManifestKey").and_then(Value::as_data) {
                Some(mk) => decrypt_manifest_db(&keys, mk, &raw)?,
                // iOS 10.0/10.1 encrypted backups: Manifest.db itself is not encrypted.
                None => raw,
            };
            (Some(keys), bytes)
        } else {
            (None, raw)
        };
        Ok(IosBackup { info, keys, db: open_db(&bytes)? })
    }

    /// Summary of the backup.
    pub fn info(&self) -> &BackupInfo {
        &self.info
    }

    /// Rows of `Files` in `domain` whose path starts with `prefix`, sorted by path.
    pub fn entries(&self, domain: &str, prefix: &str) -> Result<Vec<ManifestEntry>> {
        let mut stmt = self.db.prepare(ENTRIES_SQL)?;
        let rows = stmt.query_map([domain], entry_from_row)?;
        let mut out = Vec::new();
        for row in rows {
            let row = row?;
            if row.relative_path.starts_with(prefix) {
                out.push(row);
            }
        }
        Ok(out)
    }

    /// Path of the stored file of `entry` (`<xx>/<fileID>`, or `<fileID>` in old flat backups).
    fn stored_path(&self, entry: &ManifestEntry) -> Result<PathBuf> {
        let id = &entry.file_id;
        if id.len() < 2 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(Error::Corrupt(format!("bad fileID {id:?}")));
        }
        let dir = &self.info.path;
        [dir.join(&id[..2]).join(id), dir.join(id)]
            .into_iter()
            .find(|p| p.is_file())
            .ok_or_else(|| Error::MissingFile(format!("{}/{}", entry.domain, entry.relative_path)))
    }

    /// Contents of a file entry: the stored file itself for unencrypted backups, decrypted bytes otherwise.
    pub fn read_file(&self, entry: &ManifestEntry) -> Result<Payload> {
        let path = self.stored_path(entry)?;
        let Some(keys) = &self.keys else {
            return Ok(Payload::File(path));
        };
        let what = format!("{}/{}", entry.domain, entry.relative_path);
        let mb = MbFile::parse(&entry.file)?;
        let wrapped = mb.wrapped_key.ok_or_else(|| Error::Corrupt(format!("{what}: no EncryptionKey")))?;
        let key = unwrap_key(keys, mb.protection_class, &wrapped)?;
        let mut data = decrypt(&key, &fs::read(path)?, &what)?;
        if mb.size > data.len() as u64 {
            return Err(Error::Corrupt(format!("{what}: shorter than its recorded size")));
        }
        data.truncate(mb.size as usize);
        Ok(Payload::Bytes(data))
    }

    /// Bundle ids of apps whose container holds Blockheads saves (`Library/Application Support/saves/<32 hex>/
    /// world_db/data.mdb`), sorted.
    pub fn blockheads_apps(&self) -> Result<Vec<String>> {
        let mut stmt = self.db.prepare(APPS_SQL)?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        let mut apps = BTreeSet::new();
        for row in rows {
            let (domain, path) = row?;
            if world_of_marker(&path).is_some() {
                if let Some(bundle) = domain.strip_prefix(APP_DOMAIN) {
                    apps.insert(bundle.to_string());
                }
            }
        }
        Ok(apps.into_iter().collect())
    }

    /// The Blockheads data of the first app found by [`IosBackup::blockheads_apps`] ([`Error::NoBlockheads`] if
    /// there is none).
    pub fn blockheads(&self) -> Result<BlockheadsData> {
        let bundle = self.blockheads_apps()?.into_iter().next().ok_or(Error::NoBlockheads)?;
        self.blockheads_for(&bundle)
    }

    /// The Blockheads data in the container of `bundle_id`.
    pub fn blockheads_for(&self, bundle_id: &str) -> Result<BlockheadsData> {
        let entries = self.entries(&format!("{APP_DOMAIN}{bundle_id}"), APP_SUPPORT)?;
        let world_ids: BTreeSet<&str> = entries.iter().filter_map(|e| world_of_marker(&e.relative_path)).collect();
        let mut worlds: BTreeMap<String, Vec<WorldFile>> = BTreeMap::new();
        let mut game_db = Vec::new();
        for e in entries.iter().filter(|e| e.flags == FLAG_FILE) {
            let rest = &e.relative_path[APP_SUPPORT.len()..];
            if let Some(p) = rest.strip_prefix("game_db/") {
                game_db.push(WorldFile { path: p.to_string(), data: self.read_file(e)? });
            } else if let Some((id, p)) = rest.strip_prefix("saves/").and_then(|r| r.split_once('/')) {
                if world_ids.contains(id) {
                    let file = WorldFile { path: p.to_string(), data: self.read_file(e)? };
                    worlds.entry(id.to_string()).or_default().push(file);
                }
            }
        }
        let worlds =
            worlds.into_iter().map(|(id, files)| WorldFiles::new(id, files)).collect::<std::result::Result<_, _>>()?;
        Ok(BlockheadsData { bundle_id: bundle_id.to_string(), worlds, game_db })
    }
}

fn entry_from_row(r: &rusqlite::Row) -> rusqlite::Result<ManifestEntry> {
    Ok(ManifestEntry {
        file_id: r.get(0)?,
        domain: r.get(1)?,
        relative_path: r.get(2)?,
        flags: r.get(3)?,
        file: r.get::<_, Option<Vec<u8>>>(4)?.unwrap_or_default(),
    })
}

/// World id of a `.../saves/<32 hex>/world_db/data.mdb` path.
fn world_of_marker(path: &str) -> Option<&str> {
    let rest = path.strip_prefix(APP_SUPPORT)?.strip_prefix("saves/")?;
    let (id, tail) = rest.split_once('/')?;
    (tail == bhw_world::files::MARKER && id.len() == 32 && id.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')))
        .then_some(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture::{self, Enc, Row};
    use plist::Dictionary;

    const ID: &str = "0123456789abcdef0123456789abcdef";
    const ID2: &str = "fedcba9876543210fedcba9876543210";
    const DOM: &str = "AppDomain-com.example.blockheads";

    fn p(s: &str) -> String {
        format!("{APP_SUPPORT}{s}")
    }

    fn rows(paths: &[String]) -> Vec<(String, Vec<u8>)> {
        paths.iter().map(|s| (s.clone(), format!("data of {s}").into_bytes())).collect()
    }

    fn game_rows() -> Vec<(String, String, i64, Vec<u8>)> {
        let mut out: Vec<(String, String, i64, Vec<u8>)> = rows(&[
            p(&format!("saves/{ID}/world_db/data.mdb")),
            p(&format!("saves/{ID}/world_db/lock.mdb")),
            p(&format!("saves/{ID}/map/default/0_0.png")),
            p(&format!("saves/{ID2}/world_db/data.mdb")),
            p("saves/not-a-world/world_db/data.mdb"),
            p("saves/abcdefabcdefabcdefabcdefabcdefab/notes.txt"),
            p("game_db/data.mdb"),
            p("game/settings"),
            "Documents/other".into(),
        ])
        .into_iter()
        .map(|(path, data)| (DOM.to_string(), path, 1, data))
        .collect();
        out.push((DOM.into(), p(&format!("saves/{ID}/map")), 2, vec![]));
        out.push(("AppDomain-zzz.other".into(), "Library/x".into(), 1, b"x".to_vec()));
        out.push(("HomeDomain".into(), p(&format!("saves/{ID}/world_db/data.mdb")), 1, b"y".to_vec()));
        // SQL LIKE is case-insensitive; this is not an app domain.
        out.push(("APPDOMAIN-x".into(), p(&format!("saves/{ID}/world_db/data.mdb")), 1, b"y".to_vec()));
        out
    }

    fn make(enc: Enc, rows: &[(String, String, i64, Vec<u8>)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let r: Vec<Row> = rows.iter().map(|(d, p, f, data)| (d.as_str(), p.as_str(), *f, data.as_slice())).collect();
        fixture::build(dir.path(), enc, &r);
        dir
    }

    fn bytes(p: &Payload) -> Vec<u8> {
        p.read_all().unwrap()
    }

    fn check(b: &IosBackup, want_file_payload: bool) {
        assert_eq!(b.blockheads_apps().unwrap(), ["com.example.blockheads"]);
        let data = b.blockheads().unwrap();
        assert_eq!(data.bundle_id, "com.example.blockheads");
        assert_eq!(data.worlds.iter().map(|w| w.id.as_str()).collect::<Vec<_>>(), [ID, ID2]);
        let w = &data.worlds[0];
        assert_eq!(
            w.files.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(),
            ["map/default/0_0.png", "world_db/data.mdb", "world_db/lock.mdb"]
        );
        let want = format!("data of {}", p(&format!("saves/{ID}/world_db/data.mdb")));
        assert_eq!(bytes(&w.get("world_db/data.mdb").unwrap().data), want.as_bytes());
        assert_eq!(matches!(w.files[0].data, Payload::File(_)), want_file_payload);
        assert_eq!(data.game_db.len(), 1);
        assert_eq!(data.game_db[0].path, "data.mdb");
        assert_eq!(bytes(&data.game_db[0].data), format!("data of {}", p("game_db/data.mdb")).as_bytes());
    }

    #[test]
    fn unencrypted() {
        let dir = make(Enc::None, &game_rows());
        let b = IosBackup::open(dir.path(), Some("ignored")).unwrap();
        assert!(!b.info().encrypted);
        assert_eq!(b.info().device_name.as_deref(), Some("Test iPhone"));
        check(&b, true);
        assert_eq!(b.entries("HomeDomain", "").unwrap().len(), 1);
    }

    #[test]
    fn encrypted_modern_and_legacy() {
        for enc in [Enc::Modern("secret"), Enc::Legacy("secret")] {
            let dir = make(enc, &game_rows());
            let b = IosBackup::open(dir.path(), Some("secret")).unwrap();
            assert!(b.info().encrypted);
            check(&b, false);
            assert!(matches!(IosBackup::open(dir.path(), Some("wrong")), Err(Error::WrongPassword)));
            assert!(matches!(IosBackup::open(dir.path(), None), Err(Error::PasswordRequired)));
        }
    }

    #[test]
    fn flat_layout_and_missing_files() {
        let rows = game_rows();
        let dir = make(Enc::None, &rows);
        let id = file_id(DOM, &p("game_db/data.mdb"));
        fs::rename(dir.path().join(&id[..2]).join(&id), dir.path().join(&id)).unwrap();
        let b = IosBackup::open(dir.path(), None).unwrap();
        check(&b, true);
        let id = file_id(DOM, &p(&format!("saves/{ID2}/world_db/data.mdb")));
        fs::remove_file(dir.path().join(&id[..2]).join(&id)).unwrap();
        assert!(matches!(b.blockheads(), Err(Error::MissingFile(m)) if m.contains(ID2)));
        let bad = |id: &str| ManifestEntry {
            file_id: id.into(),
            domain: "d".into(),
            relative_path: "p".into(),
            flags: 1,
            file: vec![],
        };
        assert!(matches!(b.read_file(&bad("../../etc")), Err(Error::Corrupt(_))));
        assert!(matches!(b.read_file(&bad("a")), Err(Error::Corrupt(_))));
    }

    #[test]
    fn no_blockheads_and_bad_world() {
        let dir = make(Enc::None, &[(DOM.into(), "Library/x".into(), 1, b"x".to_vec())]);
        let b = IosBackup::open(dir.path(), None).unwrap();
        assert!(b.blockheads_apps().unwrap().is_empty());
        assert!(matches!(b.blockheads(), Err(Error::NoBlockheads)));
        let mut rows = game_rows();
        rows.push((DOM.into(), p(&format!("saves/{ID}/world_db/../x")), 1, b"z".to_vec()));
        let dir = make(Enc::None, &rows);
        let b = IosBackup::open(dir.path(), None).unwrap();
        assert!(matches!(b.blockheads(), Err(Error::World(_))));
    }

    #[test]
    fn open_errors() {
        let dir = make(Enc::None, &game_rows());
        fs::remove_file(dir.path().join("Manifest.db")).unwrap();
        assert!(matches!(IosBackup::open(dir.path(), None), Err(Error::MissingFile(_))));
        fs::write(dir.path().join("Manifest.mbdb"), b"mbdb").unwrap();
        assert!(matches!(IosBackup::open(dir.path(), None), Err(Error::Unsupported(_))));
        fs::write(dir.path().join("Manifest.db"), b"not sqlite").unwrap();
        assert!(matches!(IosBackup::open(dir.path(), None), Err(Error::Sqlite(_))));
        assert!(matches!(IosBackup::open(&dir.path().join("nope"), None), Err(Error::NotABackup(_))));
    }

    fn edit_manifest(dir: &Path, f: impl FnOnce(&mut Dictionary)) {
        let mut m = read_plist(&dir.join("Manifest.plist")).unwrap();
        f(&mut m);
        Value::Dictionary(m).to_file_binary(dir.join("Manifest.plist")).unwrap();
    }

    #[test]
    fn encrypted_manifest_errors() {
        let open = |dir: &tempfile::TempDir| IosBackup::open(dir.path(), Some("pw"));
        let dir = make(Enc::Modern("pw"), &game_rows());
        let mk = read_plist(&dir.path().join("Manifest.plist")).unwrap()["ManifestKey"].as_data().unwrap().to_vec();

        let set_mk = |v: Vec<u8>| edit_manifest(dir.path(), |m| _ = m.insert("ManifestKey".into(), Value::Data(v)));
        set_mk(mk[..3].to_vec());
        assert!(matches!(open(&dir), Err(Error::Corrupt(_))));
        let mut other_class = mk.clone();
        other_class[0] = 9;
        set_mk(other_class);
        assert!(matches!(open(&dir), Err(Error::Keybag(m)) if m.contains("class 9")));
        let mut bad_wrap = mk.clone();
        bad_wrap[10] ^= 1;
        set_mk(bad_wrap);
        assert!(matches!(open(&dir), Err(Error::Corrupt(_))));
        set_mk(mk);
        fs::write(dir.path().join("Manifest.db"), [0u8; 17]).unwrap();
        assert!(matches!(open(&dir), Err(Error::Corrupt(_))));
        fs::write(dir.path().join("Manifest.db"), [0u8; 32]).unwrap();
        assert!(matches!(open(&dir), Err(Error::Sqlite(_))));
        edit_manifest(dir.path(), |m| _ = m.remove("BackupKeyBag"));
        assert!(matches!(open(&dir), Err(Error::Keybag(_))));
    }

    #[test]
    fn manifest_db_without_padding() {
        let dir = make(Enc::Modern("pw"), &game_rows());
        let r = game_rows();
        let r: Vec<Row> = r.iter().map(|(d, p, f, data)| (d.as_str(), p.as_str(), *f, data.as_slice())).collect();
        let db = fixture::manifest_db(&r, true);
        fs::write(dir.path().join("Manifest.db"), crate::crypto::cbc_encrypt(&[9; 32], &[0; 16], &db)).unwrap();
        check(&IosBackup::open(dir.path(), Some("pw")).unwrap(), false);
    }

    #[test]
    fn encrypted_file_errors() {
        let dir = make(Enc::Legacy("pw"), &game_rows());
        let b = IosBackup::open(dir.path(), Some("pw")).unwrap();
        let mut e = b.entries(DOM, &p("game_db/")).unwrap().remove(0);
        let stored = dir.path().join(&e.file_id[..2]).join(&e.file_id);
        let good = fs::read(&stored).unwrap();
        fs::write(&stored, &good[..good.len() - 1]).unwrap();
        assert!(matches!(b.read_file(&e), Err(Error::Corrupt(m)) if m.contains("multiple of 16")));
        fs::write(&stored, &good[..good.len() - 16]).unwrap();
        assert!(matches!(b.read_file(&e), Err(Error::Corrupt(m)) if m.contains("recorded size")));
        let blob = e.file.clone();
        e.file = crate::mbfile::build(&MbFile { size: 1, ..Default::default() }, "x");
        assert!(matches!(b.read_file(&e), Err(Error::Corrupt(m)) if m.contains("no EncryptionKey")));
        e.file = blob;
        fs::write(&stored, good).unwrap();
        assert!(b.read_file(&e).is_ok());
    }

    #[test]
    fn file_ids_and_markers() {
        // Well-known id of the address book database in public forensics literature.
        assert_eq!(
            file_id("HomeDomain", "Library/AddressBook/AddressBook.sqlitedb"),
            "31bb7ba8914766d4ba40d6dfb6113c8b614be442"
        );
        assert_eq!(world_of_marker(&p(&format!("saves/{ID}/world_db/data.mdb"))), Some(ID));
        assert_eq!(world_of_marker(&p(&format!("saves/{}/world_db/data.mdb", ID.to_uppercase()))), None);
        assert_eq!(world_of_marker(&p(&format!("saves/{ID}/world_db/lock.mdb"))), None);
        assert_eq!(world_of_marker(&p(&format!("saves/{ID}"))), None);
        assert_eq!(world_of_marker("x"), None);
    }
}
