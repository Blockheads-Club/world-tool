//! One world as a set of files.
//!
//! A game world (Android, Blockheads PC) is a directory `saves/<32 hex id>/` containing `world_db/data.mdb`,
//! `world_db/lock.mdb` and `map/default/*.png`. A Blockheads Server world folder additionally has `server_db/` and
//! `lightBlocks/` LMDB environments (64-bit layout); converting those needs `bhw-lmdb` (see
//! [`crate::meta::ServerWorldConverter`]), here they are only modelled as files.
//!
//! Writing a world into a saves directory while the game is running is not safe (LMDB); callers must make sure the
//! game is closed.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use bhw_backup::saves::is_world_id;
use bhw_backup::{is_safe_rel_path, Payload};

use crate::{Error, Result};

/// File whose presence marks a world.
pub const MARKER: &str = "world_db/data.mdb";
/// Directories only Blockheads Server worlds have.
pub const SERVER_DIRS: [&str; 2] = ["server_db", "lightBlocks"];

/// One file of a world.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorldFile {
    /// Path relative to the world directory, `/`-separated (e.g. `world_db/data.mdb`).
    pub path: String,
    /// Contents.
    pub data: Payload,
}

/// A world: its id and its files (sorted by path, unique, safe relative paths).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorldFiles {
    /// 32 lowercase hex id (the directory name in `saves/`).
    pub id: String,
    /// Files, sorted by path.
    pub files: Vec<WorldFile>,
}

fn check_id(id: &str) -> Result<()> {
    if is_world_id(id) {
        Ok(())
    } else {
        Err(Error::InvalidId(id.to_string()))
    }
}

impl WorldFiles {
    /// Validates the id and paths and sorts the files.
    pub fn new(id: String, mut files: Vec<WorldFile>) -> Result<Self> {
        check_id(&id)?;
        for f in &files {
            if !is_safe_rel_path(&f.path) {
                return Err(Error::InvalidPath(f.path.clone()));
            }
        }
        files.sort_by(|a, b| a.path.cmp(&b.path));
        for pair in files.windows(2) {
            if pair[0].path == pair[1].path {
                return Err(Error::InvalidPath(format!("duplicate {}", pair[0].path)));
            }
        }
        Ok(Self { id, files })
    }

    /// The file at `path`.
    pub fn get(&self, path: &str) -> Option<&WorldFile> {
        self.files.binary_search_by(|f| f.path.as_str().cmp(path)).ok().map(|i| &self.files[i])
    }

    /// `true` if `world_db/data.mdb` is present.
    pub fn is_world(&self) -> bool {
        self.get(MARKER).is_some()
    }

    /// `true` if the world has Blockheads Server databases.
    pub fn is_server_world(&self) -> bool {
        self.files.iter().any(|f| SERVER_DIRS.iter().any(|d| f.path.split('/').next() == Some(d)))
    }

    /// Sum of all file sizes.
    pub fn total_size(&self) -> Result<u64> {
        let mut total = 0;
        for f in &self.files {
            total += f.data.len()?;
        }
        Ok(total)
    }

    /// Copy with every file loaded into memory.
    pub fn to_memory(&self) -> Result<Self> {
        let mut files = Vec::with_capacity(self.files.len());
        for f in &self.files {
            files.push(WorldFile { path: f.path.clone(), data: Payload::Bytes(f.data.read_all()?) });
        }
        Ok(Self { id: self.id.clone(), files })
    }

    /// Loads every regular file below `dir` (symlinks are skipped) as [`Payload::File`].
    pub fn load_dir(dir: &Path, id: String) -> Result<Self> {
        let mut files = Vec::new();
        let mut stack = vec![(dir.to_path_buf(), String::new())];
        while let Some((d, rel)) = stack.pop() {
            for entry in fs::read_dir(&d)? {
                let entry = entry?;
                let name = entry.file_name();
                let name = name.to_str().ok_or_else(|| Error::InvalidPath(name.to_string_lossy().into_owned()))?;
                let path = if rel.is_empty() { name.to_string() } else { format!("{rel}/{name}") };
                let ft = entry.file_type()?;
                if ft.is_dir() {
                    stack.push((entry.path(), path));
                } else if ft.is_file() {
                    files.push(WorldFile { path, data: Payload::File(entry.path()) });
                }
            }
        }
        Self::new(id, files)
    }

    /// Loads world `id` from a saves directory (`<saves>/<id>/`).
    pub fn from_saves_dir(saves: &Path, id: &str) -> Result<Self> {
        check_id(id)?;
        let dir = saves.join(id);
        let w = Self::load_dir(&dir, id.to_string())?;
        if !w.is_world() {
            return Err(Error::NotAWorld(dir));
        }
        Ok(w)
    }

    /// All worlds in a saves directory, sorted by id. Entries that are not world directories are ignored.
    pub fn list_saves_dir(saves: &Path) -> Result<Vec<Self>> {
        let mut out = Vec::new();
        for entry in fs::read_dir(saves)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if !is_world_id(&name) || !entry.file_type()?.is_dir() || !entry.path().join(MARKER).is_file() {
                continue;
            }
            out.push(Self::load_dir(&entry.path(), name)?);
        }
        out.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(out)
    }

    /// Loads a Blockheads Server world folder. The id is `id` if given, else the folder name (which must be a world
    /// id).
    pub fn from_server_world_dir(dir: &Path, id: Option<&str>) -> Result<Self> {
        let name = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let id = id.map(str::to_string).unwrap_or(name);
        check_id(&id)?;
        let w = Self::load_dir(dir, id)?;
        if !w.is_world() {
            return Err(Error::NotAWorld(dir.to_path_buf()));
        }
        Ok(w)
    }

    fn write_files(&self, dir: &Path) -> Result<()> {
        fs::create_dir_all(dir)?;
        for f in &self.files {
            let target = dir.join(&f.path);
            fs::create_dir_all(target.parent().expect("file paths have a parent"))?;
            let mut out = fs::File::create(&target)?;
            io::copy(&mut f.data.open()?, &mut out)?;
            out.sync_all()?;
        }
        Ok(())
    }

    /// Writes the files into a new directory `dir` (must not exist). Files are written to a temporary sibling first and
    /// renamed into place, so `dir` never holds a half-written world.
    pub fn write_dir(&self, dir: &Path) -> Result<()> {
        if dir.exists() {
            return Err(Error::AlreadyExists(dir.to_path_buf()));
        }
        let tmp = sibling(dir, "bhw-tmp");
        remove_if_exists(&tmp)?;
        if let Err(e) = self.write_files(&tmp) {
            fs::remove_dir_all(&tmp)?;
            return Err(e);
        }
        fs::rename(&tmp, dir)?;
        Ok(())
    }

    /// Writes the world to `<saves>/<id>/`. An existing world with the same id is an error unless `replace`, in which
    /// case it is swapped out only after the new copy is complete.
    pub fn save_to_saves_dir(&self, saves: &Path, replace: bool) -> Result<PathBuf> {
        let target = saves.join(&self.id);
        if !target.exists() {
            self.write_dir(&target)?;
            return Ok(target);
        }
        if !replace {
            return Err(Error::AlreadyExists(target));
        }
        let staged = sibling(&target, "bhw-new");
        let old = sibling(&target, "bhw-old");
        remove_if_exists(&staged)?;
        remove_if_exists(&old)?;
        self.write_dir(&staged)?;
        fs::rename(&target, &old)?;
        fs::rename(&staged, &target)?;
        fs::remove_dir_all(&old)?;
        Ok(target)
    }
}

/// `<parent>/.<name>.<tag>`.
fn sibling(dir: &Path, tag: &str) -> PathBuf {
    let name = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    dir.with_file_name(format!(".{name}.{tag}"))
}

fn remove_if_exists(p: &Path) -> io::Result<()> {
    if p.exists() {
        fs::remove_dir_all(p)?;
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) const ID: &str = "0123456789abcdef0123456789abcdef";
    pub(crate) const ID2: &str = "fedcba9876543210fedcba9876543210";

    pub(crate) fn mem(path: &str, data: &[u8]) -> WorldFile {
        WorldFile { path: path.into(), data: Payload::Bytes(data.to_vec()) }
    }

    pub(crate) fn sample(id: &str) -> WorldFiles {
        WorldFiles::new(
            id.into(),
            vec![
                mem("world_db/data.mdb", b"DATA"),
                mem("world_db/lock.mdb", b"LOCK"),
                mem("map/default/0_0.png", b"PNG"),
            ],
        )
        .unwrap()
    }

    #[test]
    fn new_validates() {
        assert!(matches!(WorldFiles::new("x".into(), vec![]), Err(Error::InvalidId(_))));
        assert!(matches!(WorldFiles::new(ID.into(), vec![mem("../a", b"")]), Err(Error::InvalidPath(_))));
        assert!(matches!(WorldFiles::new(ID.into(), vec![mem("a", b""), mem("a", b"")]), Err(Error::InvalidPath(_))));
        let w = sample(ID);
        assert_eq!(w.files[0].path, "map/default/0_0.png");
        assert!(w.is_world());
        assert!(!w.is_server_world());
        assert_eq!(w.total_size().unwrap(), 11);
        assert!(w.get("nope").is_none());
        let s = WorldFiles::new(ID.into(), vec![mem("server_db/data.mdb", b"")]).unwrap();
        assert!(s.is_server_world());
        assert!(!s.is_world());
    }

    #[test]
    fn saves_dir_round_trip_and_replace() {
        let t = tempfile::tempdir().unwrap();
        let saves = t.path();
        let w = sample(ID);
        assert_eq!(w.save_to_saves_dir(saves, false).unwrap(), saves.join(ID));
        assert!(matches!(w.save_to_saves_dir(saves, false), Err(Error::AlreadyExists(_))));
        assert!(matches!(w.write_dir(&saves.join(ID)), Err(Error::AlreadyExists(_))));
        // Noise that listing must ignore.
        fs::create_dir_all(saves.join(ID2).join("map")).unwrap();
        fs::write(saves.join("notes.txt"), b"x").unwrap();
        fs::create_dir_all(saves.join("game_db")).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(saves.join("notes.txt"), saves.join(ID).join("link")).unwrap();
        let listed = WorldFiles::list_saves_dir(saves).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].to_memory().unwrap(), w);
        assert!(matches!(WorldFiles::from_saves_dir(saves, ID2), Err(Error::NotAWorld(_))));
        assert!(matches!(WorldFiles::from_saves_dir(saves, "x"), Err(Error::InvalidId(_))));
        // Replace with different content; stale staging dirs are cleaned up.
        fs::create_dir_all(saves.join(format!(".{ID}.bhw-new"))).unwrap();
        fs::create_dir_all(saves.join(format!(".{ID}.bhw-old"))).unwrap();
        let w2 = WorldFiles::new(ID.into(), vec![mem("world_db/data.mdb", b"NEW")]).unwrap();
        w2.save_to_saves_dir(saves, true).unwrap();
        assert_eq!(WorldFiles::from_saves_dir(saves, ID).unwrap().to_memory().unwrap(), w2);
        assert!(!saves.join(format!(".{ID}.bhw-old")).exists());
        // A failing write leaves nothing behind.
        let bad = WorldFiles::new(
            ID2.into(),
            vec![WorldFile { path: "world_db/data.mdb".into(), data: Payload::File(saves.join("missing")) }],
        )
        .unwrap();
        fs::remove_dir_all(saves.join(ID2)).unwrap();
        assert!(bad.save_to_saves_dir(saves, false).is_err());
        assert!(!saves.join(ID2).exists());
        assert!(!saves.join(format!(".{ID2}.bhw-tmp")).exists());
        assert!(bad.total_size().is_err());
        assert!(bad.to_memory().is_err());
        assert!(WorldFiles::list_saves_dir(&saves.join("missing")).is_err());
    }

    #[test]
    fn list_is_sorted_by_id() {
        let t = tempfile::tempdir().unwrap();
        sample(ID2).save_to_saves_dir(t.path(), false).unwrap();
        sample(ID).save_to_saves_dir(t.path(), false).unwrap();
        let ids: Vec<String> = WorldFiles::list_saves_dir(t.path()).unwrap().into_iter().map(|w| w.id).collect();
        assert_eq!(ids, [ID, ID2]);
    }

    // APFS (macOS) refuses non-UTF-8 file names, so the case cannot be created there.
    #[cfg(all(unix, not(target_os = "macos")))]
    #[test]
    fn non_utf8_names_are_rejected() {
        use std::os::unix::ffi::OsStrExt;
        let t = tempfile::tempdir().unwrap();
        fs::write(t.path().join(std::ffi::OsStr::from_bytes(b"\xff")), b"").unwrap();
        assert!(matches!(WorldFiles::load_dir(t.path(), ID.into()), Err(Error::InvalidPath(_))));
    }

    #[test]
    fn server_world_dir() {
        let t = tempfile::tempdir().unwrap();
        let dir = t.path().join(ID);
        WorldFiles::new(ID.into(), vec![mem("world_db/data.mdb", b"W"), mem("server_db/data.mdb", b"S")])
            .unwrap()
            .write_dir(&dir)
            .unwrap();
        let w = WorldFiles::from_server_world_dir(&dir, None).unwrap();
        assert!(w.is_server_world());
        assert_eq!(WorldFiles::from_server_world_dir(&dir, Some(ID2)).unwrap().id, ID2);
        let other = t.path().join("My Server World");
        fs::create_dir_all(&other).unwrap();
        assert!(matches!(WorldFiles::from_server_world_dir(&other, None), Err(Error::InvalidId(_))));
        assert!(matches!(WorldFiles::from_server_world_dir(&other, Some(ID)), Err(Error::NotAWorld(_))));
        assert!(matches!(WorldFiles::from_server_world_dir(Path::new("/"), None), Err(Error::InvalidId(_))));
        assert_eq!(sibling(Path::new("/"), "x"), Path::new("/..x"));
    }
}
