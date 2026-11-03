//! The world library: a folder with exported worlds and full device backups.
//!
//! ```text
//! <root>/worlds/<name>-<id8>.bhworld
//! <root>/backups/<time>-<device>.ab        full `adb backup` as taken from the device
//! <root>/backups/<time>-<device>.ab.json   sidecar: BackupMeta
//! ```
//!
//! Unpacked world folders are not part of the library (yet); import them with [`crate::files::WorldFiles`] and
//! [`Library::add_world`].

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::bhworld::{open_bhworld, open_manifest, save_with_manifest, ExportInfo, Limits, Manifest};
use crate::files::WorldFiles;
use crate::meta::{new_world_id_excluding, unique_name, WorldMetaEditor};
use crate::time::now_rfc3339;
use crate::{Error, Result};

const WORLD_EXT: &str = "bhworld";
const BACKUP_EXT: &str = "ab";

/// A `.bhworld` in the library.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryWorld {
    /// File path.
    pub path: PathBuf,
    /// Manifest, if the file could be read.
    pub manifest: Option<Manifest>,
    /// Why the manifest could not be read.
    pub error: Option<String>,
}

/// Sidecar of a stored device backup.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupMeta {
    /// Device name.
    pub device: String,
    /// When the backup was taken (RFC 3339).
    pub created_at: String,
    /// File size in bytes.
    pub size: u64,
    /// Why it was taken (e.g. "before restoring world X").
    pub reason: String,
    /// Password protected.
    pub encrypted: bool,
}

/// A stored device backup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryBackup {
    /// `.ab` path.
    pub path: PathBuf,
    /// Sidecar contents, if present and valid.
    pub meta: Option<BackupMeta>,
}

/// A library rooted at a directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Library {
    root: PathBuf,
}

/// File-name-safe version of `s` (keeps letters/digits incl. non-ASCII, `-`, `_`, `.`, spaces → `-`).
pub fn slug(s: &str) -> String {
    let mapped: String = s
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' || c == '.' { c } else { '-' })
        .take(60)
        .collect();
    let t = mapped.trim_matches(|c| c == '-' || c == '.');
    if t.is_empty() {
        "world".to_string()
    } else {
        t.to_string()
    }
}

fn files_with_ext(dir: &Path, ext: &str) -> Result<Vec<PathBuf>> {
    let mut v: Vec<PathBuf> = Vec::new();
    for e in fs::read_dir(dir)? {
        let p = e?.path();
        if p.is_file() && p.extension().is_some_and(|x| x == ext) {
            v.push(p);
        }
    }
    v.sort();
    Ok(v)
}

/// `dir/<stem>.<ext>`, or `dir/<stem>-2.<ext>`, ... if taken.
fn free_path(dir: &Path, stem: &str, ext: &str) -> PathBuf {
    let mut p = dir.join(format!("{stem}.{ext}"));
    let mut i = 2;
    while p.exists() {
        p = dir.join(format!("{stem}-{i}.{ext}"));
        i += 1;
    }
    p
}

impl Library {
    /// Opens (creating the folders if needed) a library at `root`.
    pub fn open(root: &Path) -> Result<Self> {
        fs::create_dir_all(root.join("worlds"))?;
        fs::create_dir_all(root.join("backups"))?;
        Ok(Self { root: root.to_path_buf() })
    }

    /// `<root>/worlds`.
    pub fn worlds_dir(&self) -> PathBuf {
        self.root.join("worlds")
    }

    /// `<root>/backups`.
    pub fn backups_dir(&self) -> PathBuf {
        self.root.join("backups")
    }

    fn ensure_inside(&self, dir: &Path, path: &Path, ext: &str) -> Result<()> {
        let parent = path.parent().map(fs::canonicalize).transpose()?;
        if parent.as_deref() != Some(fs::canonicalize(dir)?.as_path()) || path.extension().is_none_or(|x| x != ext) {
            return Err(Error::InvalidPath(path.display().to_string()));
        }
        Ok(())
    }

    /// All `.bhworld` files, sorted by file name; unreadable ones are listed with their error.
    pub fn list_worlds(&self) -> Result<Vec<LibraryWorld>> {
        let mut out = Vec::new();
        for path in files_with_ext(&self.worlds_dir(), WORLD_EXT)? {
            let (manifest, error) = match open_manifest(&path, &Limits::default()) {
                Ok(m) => (Some(m), None),
                Err(e) => (None, Some(e.to_string())),
            };
            out.push(LibraryWorld { path, manifest, error });
        }
        Ok(out)
    }

    fn world_path(&self, name: Option<&str>, id: &str) -> PathBuf {
        free_path(&self.worlds_dir(), &format!("{}-{}", slug(name.unwrap_or("world")), &id[..8]), WORLD_EXT)
    }

    /// Exports `world` into the library.
    pub fn add_world(&self, world: &WorldFiles, info: &ExportInfo, preview: Option<&[u8]>) -> Result<PathBuf> {
        let path = self.world_path(info.name.as_deref(), &world.id);
        save_with_manifest(&path, world, Manifest::new(&world.id, info), preview)?;
        Ok(path)
    }

    /// Copies an existing `.bhworld` into the library after validating it completely.
    pub fn import_file(&self, src: &Path) -> Result<PathBuf> {
        let b = open_bhworld(src, &Limits::default())?;
        let path = self.world_path(b.manifest.world.name.as_deref(), &b.world.id);
        fs::copy(src, &path)?;
        Ok(path)
    }

    fn names(&self) -> Result<(Vec<String>, Vec<String>)> {
        let mut names = Vec::new();
        let mut ids = Vec::new();
        for m in self.list_worlds()?.into_iter().filter_map(|w| w.manifest) {
            names.extend(m.world.name);
            ids.push(m.world.id);
        }
        Ok((names, ids))
    }

    /// Rewrites `path` with a new manifest/world into a new file and deletes the old one if the name changed.
    fn rewrite(
        &self,
        path: &Path,
        id: String,
        name: &str,
        editor: Option<&dyn WorldMetaEditor>,
        keep: bool,
    ) -> Result<PathBuf> {
        let b = open_bhworld(path, &Limits::default())?;
        let mut world = b.world;
        let new_id = (id != world.id).then_some(id.as_str());
        if let Some(ed) = editor {
            ed.set_identity(&mut world, new_id, Some(name))?;
        }
        world.id = id;
        let mut manifest = b.manifest;
        manifest.world.name = Some(name.to_string());
        let target = self.world_path(Some(name), &world.id);
        save_with_manifest(&target, &world, manifest, b.preview.as_deref())?;
        if !keep {
            fs::remove_file(path)?;
        }
        Ok(target)
    }

    /// Renames a world (manifest name and file name). `editor` also updates the name stored in the world's databases.
    pub fn rename_world(&self, path: &Path, new_name: &str, editor: Option<&dyn WorldMetaEditor>) -> Result<PathBuf> {
        self.ensure_inside(&self.worlds_dir(), path, WORLD_EXT)?;
        let id = open_manifest(path, &Limits::default())?.world.id;
        self.rewrite(path, id, new_name, editor, false)
    }

    /// Duplicates a world with a new random id and a unique name (`"Name (2)"`).
    pub fn duplicate_world(&self, path: &Path, editor: Option<&dyn WorldMetaEditor>) -> Result<PathBuf> {
        self.ensure_inside(&self.worlds_dir(), path, WORLD_EXT)?;
        let m = open_manifest(path, &Limits::default())?;
        let (names, ids) = self.names()?;
        let taken: Vec<&str> = names.iter().map(String::as_str).collect();
        let name = unique_name(m.world.name.as_deref().unwrap_or("World"), &taken);
        let ids: Vec<&str> = ids.iter().map(String::as_str).collect();
        let id = new_world_id_excluding(&ids);
        self.rewrite(path, id, &name, editor, true)
    }

    /// Deletes a world file of this library.
    pub fn delete_world(&self, path: &Path) -> Result<()> {
        self.ensure_inside(&self.worlds_dir(), path, WORLD_EXT)?;
        fs::remove_file(path)?;
        Ok(())
    }

    /// Stores a device backup (copied; `src` is removed afterwards if `move_file`) with a JSON sidecar. The file must
    /// be a valid `.ab` header.
    pub fn add_backup(&self, src: &Path, device: &str, reason: &str, move_file: bool) -> Result<LibraryBackup> {
        let header = bhw_backup::header::read_header(&mut io::BufReader::new(fs::File::open(src)?))?;
        let created_at = now_rfc3339();
        let stem = format!("{}-{}", created_at.replace(':', "-"), slug(device));
        let path = free_path(&self.backups_dir(), &stem, BACKUP_EXT);
        let size = fs::copy(src, &path)?;
        let meta = BackupMeta {
            device: device.into(),
            created_at,
            size,
            reason: reason.into(),
            encrypted: header.encryption != bhw_backup::Encryption::None,
        };
        fs::write(sidecar(&path), serde_json::to_vec_pretty(&meta)?)?;
        if move_file {
            fs::remove_file(src)?;
        }
        Ok(LibraryBackup { path, meta: Some(meta) })
    }

    /// Stored backups, sorted by file name (i.e. by time).
    pub fn list_backups(&self) -> Result<Vec<LibraryBackup>> {
        let mut out = Vec::new();
        for path in files_with_ext(&self.backups_dir(), BACKUP_EXT)? {
            let meta = fs::read(sidecar(&path)).ok().and_then(|b| serde_json::from_slice(&b).ok());
            out.push(LibraryBackup { path, meta });
        }
        Ok(out)
    }

    /// Deletes a stored backup and its sidecar.
    pub fn delete_backup(&self, path: &Path) -> Result<()> {
        self.ensure_inside(&self.backups_dir(), path, BACKUP_EXT)?;
        fs::remove_file(path)?;
        let s = sidecar(path);
        if s.exists() {
            fs::remove_file(s)?;
        }
        Ok(())
    }
}

fn sidecar(ab: &Path) -> PathBuf {
    let mut s = ab.as_os_str().to_owned();
    s.push(".json");
    PathBuf::from(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bhworld::tests::info;
    use crate::files::tests::{sample, ID, ID2};
    use std::cell::RefCell;

    #[test]
    fn slugs() {
        assert_eq!(slug("My World!"), "My-World");
        assert_eq!(slug("Dünya 2"), "Dünya-2");
        assert_eq!(slug("../.."), "world");
        assert_eq!(slug(&"a".repeat(100)).len(), 60);
    }

    #[derive(Default)]
    struct Recorder(RefCell<Vec<(Option<String>, Option<String>)>>);
    impl WorldMetaEditor for Recorder {
        fn set_identity(&self, _w: &mut WorldFiles, id: Option<&str>, name: Option<&str>) -> Result<()> {
            self.0.borrow_mut().push((id.map(str::to_string), name.map(str::to_string)));
            Ok(())
        }
    }

    struct Fails;
    impl WorldMetaEditor for Fails {
        fn set_identity(&self, _w: &mut WorldFiles, _id: Option<&str>, _name: Option<&str>) -> Result<()> {
            Err(Error::Meta("read-only".into()))
        }
    }

    #[test]
    fn worlds_lifecycle() {
        let t = tempfile::tempdir().unwrap();
        let lib = Library::open(t.path()).unwrap();
        let p1 = lib.add_world(&sample(ID), &info("Home"), Some(b"png")).unwrap();
        assert_eq!(p1.file_name().unwrap(), "Home-01234567.bhworld");
        let p2 = lib.add_world(&sample(ID), &info("Home"), None).unwrap();
        assert_eq!(p2.file_name().unwrap(), "Home-01234567-2.bhworld");
        fs::write(lib.worlds_dir().join("broken.bhworld"), b"junk").unwrap();
        fs::write(lib.worlds_dir().join("notes.txt"), b"x").unwrap();
        let list = lib.list_worlds().unwrap();
        assert_eq!(list.len(), 3);
        assert!(list.iter().any(|w| w.error.is_some() && w.manifest.is_none()));

        // Rename.
        let rec = Recorder::default();
        let r = lib.rename_world(&p2, "Castle", Some(&rec)).unwrap();
        assert!(!p2.exists());
        assert_eq!(open_manifest(&r, &Limits::default()).unwrap().world.name.as_deref(), Some("Castle"));
        assert_eq!(rec.0.borrow()[0], (None, Some("Castle".into())));
        // Duplicate: new id, "Home (2)", preview kept.
        let d = lib.duplicate_world(&p1, Some(&rec)).unwrap();
        let b = open_bhworld(&d, &Limits::default()).unwrap();
        assert_ne!(b.world.id, ID);
        assert_eq!(b.manifest.world.name.as_deref(), Some("Home (2)"));
        assert_eq!(b.preview.as_deref(), Some(&b"png"[..]));
        assert_eq!(rec.0.borrow()[1].0.as_deref(), Some(b.world.id.as_str()));
        assert!(p1.exists());
        // Duplicate without editor and without name.
        let mut anon = info("x");
        anon.name = None;
        let pa = lib.add_world(&sample(ID2), &anon, None).unwrap();
        assert_eq!(pa.file_name().unwrap(), "world-fedcba98.bhworld");
        let da = lib.duplicate_world(&pa, None).unwrap();
        assert_eq!(open_manifest(&da, &Limits::default()).unwrap().world.name.as_deref(), Some("World"));
        // Editor errors propagate and leave the file alone.
        assert!(matches!(lib.rename_world(&p1, "Z", Some(&Fails)), Err(Error::Meta(_))));
        assert!(p1.exists());
        // Import.
        let ext = t.path().join("ext.bhworld");
        fs::copy(&p1, &ext).unwrap();
        let imported = lib.import_file(&ext).unwrap();
        assert!(imported.starts_with(lib.worlds_dir()));
        assert!(lib.import_file(&lib.worlds_dir().join("broken.bhworld")).is_err());
        // Delete, with containment checks.
        lib.delete_world(&imported).unwrap();
        assert!(!imported.exists());
        assert!(matches!(lib.delete_world(&ext), Err(Error::InvalidPath(_))));
        assert!(matches!(lib.delete_world(&lib.worlds_dir().join("notes.txt")), Err(Error::InvalidPath(_))));
        assert!(lib.delete_world(&lib.worlds_dir().join("missing.bhworld")).is_err());
        assert!(matches!(lib.delete_world(Path::new("x.bhworld")), Err(Error::InvalidPath(_)) | Err(Error::Io(_))));
    }

    #[test]
    fn backups_lifecycle() {
        let t = tempfile::tempdir().unwrap();
        let lib = Library::open(&t.path().join("lib")).unwrap();
        let src = t.path().join("phone.ab");
        fs::write(&src, b"ANDROID BACKUP\n5\n1\nnone\nxyz").unwrap();
        let b = lib.add_backup(&src, "Pixel 7", "before restore", false).unwrap();
        assert!(src.exists());
        let meta = b.meta.clone().unwrap();
        assert_eq!((meta.size, meta.encrypted, meta.device.as_str()), (27, false, "Pixel 7"));
        assert!(b.path.file_name().unwrap().to_string_lossy().ends_with("-Pixel-7.ab"));
        let b2 = lib.add_backup(&src, "Pixel 7", "again", true).unwrap();
        assert!(!src.exists());
        assert_ne!(b.path, b2.path);
        let enc = t.path().join("enc.ab");
        let mut h = Vec::new();
        bhw_backup::header::write_header(
            &mut h,
            &bhw_backup::BackupHeader {
                version: 5,
                compressed: true,
                encryption: bhw_backup::Encryption::Aes256(bhw_backup::AesParams {
                    user_salt: vec![1],
                    checksum_salt: vec![2],
                    rounds: 1,
                    user_iv: vec![0; 16],
                    master_key_blob: vec![0; 16],
                }),
            },
        )
        .unwrap();
        fs::write(&enc, h).unwrap();
        assert!(lib.add_backup(&enc, "x", "y", false).unwrap().meta.unwrap().encrypted);
        fs::write(&enc, b"nope").unwrap();
        assert!(lib.add_backup(&enc, "x", "y", false).is_err());
        // A backup without sidecar is still listed.
        fs::remove_file(sidecar(&b2.path)).unwrap();
        let list = lib.list_backups().unwrap();
        assert_eq!(list.len(), 3);
        assert!(list.iter().any(|l| l.path == b2.path && l.meta.is_none()));
        lib.delete_backup(&b.path).unwrap();
        lib.delete_backup(&b2.path).unwrap();
        assert!(!sidecar(&b.path).exists());
        assert_eq!(lib.list_backups().unwrap().len(), 1);
        assert!(matches!(lib.delete_backup(&enc), Err(Error::InvalidPath(_))));
    }
}
