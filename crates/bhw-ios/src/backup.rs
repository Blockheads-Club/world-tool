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
        let mut stmt = self.db.prepare(
            "SELECT fileID, domain, relativePath, flags, file FROM Files WHERE domain = ?1 ORDER BY relativePath",
        )?;
        let rows = stmt.query_map([domain], |r| {
            Ok(ManifestEntry {
                file_id: r.get(0)?,
                domain: r.get(1)?,
                relative_path: r.get(2)?,
                flags: r.get(3)?,
                file: r.get::<_, Option<Vec<u8>>>(4)?.unwrap_or_default(),
            })
        })?;
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
        let mut stmt = self.db.prepare(
            "SELECT DISTINCT domain, relativePath FROM Files WHERE domain LIKE 'AppDomain-%' \
             AND relativePath LIKE 'Library/Application Support/saves/%/world_db/data.mdb'",
        )?;
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
        let worlds = worlds.into_iter().map(|(id, files)| WorldFiles::new(id, files)).collect::<std::result::Result<_, _>>()?;
        Ok(BlockheadsData { bundle_id: bundle_id.to_string(), worlds, game_db })
    }
}

/// World id of a `.../saves/<32 hex>/world_db/data.mdb` path.
fn world_of_marker(path: &str) -> Option<&str> {
    let rest = path.strip_prefix(APP_SUPPORT)?.strip_prefix("saves/")?;
    let (id, tail) = rest.split_once('/')?;
    (tail == bhw_world::files::MARKER && id.len() == 32 && id.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')))
        .then_some(id)
}
