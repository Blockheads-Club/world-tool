//! Opening saves: the two kinds of world folders and helpers over their LMDB contents.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use bhw_lmdb::{Database, EnvContents, Layout};

use crate::error::{io, Error, Result};

/// LMDB page size used by every Blockheads environment.
pub const PAGE_SIZE: u32 = 4096;
/// Map size of a game client's `world_db` (64 MiB).
pub const CLIENT_WORLD_MAP_SIZE: u64 = 64 << 20;
/// Map size of a game client's `game_db` (32 MiB).
pub const GAME_DB_MAP_SIZE: u64 = 32 << 20;
/// Map size of Blockheads Server's `world_db` and `lightBlocks` (1 GiB).
pub const SERVER_WORLD_MAP_SIZE: u64 = 1 << 30;
/// Map size of Blockheads Server's `server_db` (128 MiB).
pub const SERVER_DB_MAP_SIZE: u64 = 128 << 20;

/// Which program wrote a world folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum SaveKind {
    /// The game (Android, iOS, Blockheads PC): `world_db/` and `map/`.
    Client,
    /// Blockheads Server: `worldv2`, `world_db/`, `server_db/`, `lightBlocks/`, text lists.
    Server,
}

impl SaveKind {
    /// Guesses the kind of a world folder from the files in it: a `worldv2` file or a `server_db`/`lightBlocks`
    /// directory means a server world; a folder with `world_db/` (and no such files) is a client world.
    pub fn detect(dir: &Path) -> Option<SaveKind> {
        if dir.join("worldv2").is_file() || dir.join("server_db").is_dir() || dir.join("lightBlocks").is_dir() {
            Some(SaveKind::Server)
        } else if dir.join("world_db").is_dir() {
            Some(SaveKind::Client)
        } else {
            None
        }
    }
}

/// A world folder held in memory: `/`-separated relative path → contents.
pub type FileMap = BTreeMap<String, Vec<u8>>;

/// Reads every regular file below `dir` (symbolic links are skipped; `lock.mdb` files are skipped because LMDB
/// recreates them and they must not be copied).
pub fn read_dir_files(dir: &Path) -> Result<FileMap> {
    let mut out = FileMap::new();
    let mut stack = vec![(dir.to_path_buf(), String::new())];
    while let Some((d, rel)) = stack.pop() {
        for entry in std::fs::read_dir(&d).map_err(io(&d))? {
            let entry = entry.map_err(io(&d))?;
            let name = entry.file_name().to_string_lossy().into_owned();
            let path = if rel.is_empty() { name.clone() } else { format!("{rel}/{name}") };
            let ft = entry.file_type().map_err(io(entry.path()))?;
            if ft.is_dir() {
                stack.push((entry.path(), path));
            } else if ft.is_file() && name != "lock.mdb" {
                out.insert(path, std::fs::read(entry.path()).map_err(io(entry.path()))?);
            }
        }
    }
    Ok(out)
}

/// Writes `files` below `dir` (which may exist; files in it are overwritten, others are left alone). Empty
/// directories are recorded in a [`FileMap`] as a path ending in `/` with no contents.
pub fn write_dir_files(dir: &Path, files: &FileMap) -> Result<()> {
    std::fs::create_dir_all(dir).map_err(io(dir))?;
    for (rel, data) in files {
        let target = dir.join(rel);
        if rel.ends_with('/') {
            std::fs::create_dir_all(&target).map_err(io(&target))?;
            continue;
        }
        let parent = target.parent().expect("joined paths have a parent");
        std::fs::create_dir_all(parent).map_err(io(parent))?;
        std::fs::write(&target, data).map_err(io(&target))?;
    }
    Ok(())
}

/// Parses the LMDB file at `path` in `files`.
pub fn env_from(files: &FileMap, path: &str) -> Result<Option<(EnvContents, Layout)>> {
    match files.get(path) {
        None => Ok(None),
        Some(bytes) => {
            let env = bhw_lmdb::Env::open(bytes)?;
            Ok(Some((env.read_all()?, env.layout())))
        }
    }
}

/// Encodes an environment as a `data.mdb` image.
pub fn env_bytes(contents: &EnvContents, layout: Layout) -> Result<Vec<u8>> {
    Ok(bhw_lmdb::write_env(contents, layout)?)
}

/// Writes a file through a temporary sibling and a rename.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".bhw-tmp");
    let tmp = PathBuf::from(tmp);
    std::fs::write(&tmp, bytes).map_err(io(&tmp))?;
    std::fs::rename(&tmp, path).map_err(io(path))
}

/// A map size of at least `min` that leaves room to grow beyond `data_len` (a power of two ≥ 2 × data).
pub fn map_size_for(data_len: usize, min: u64) -> u64 {
    min.max((data_len as u64 * 2).next_power_of_two())
}

/// The value of `key` in the named database `db`.
pub fn get<'a>(env: &'a EnvContents, db: &str, key: &str) -> Option<&'a [u8]> {
    env.named.get(db.as_bytes())?.entries.iter().find(|(k, _)| k == key.as_bytes()).map(|(_, v)| v.as_slice())
}

/// Sets `key` in the named database `db` (created with no flags if missing), replacing any old value.
pub fn put(env: &mut EnvContents, db: &str, key: &[u8], value: Vec<u8>) {
    let d = env.named.entry(db.as_bytes().to_vec()).or_insert_with(|| Database::new(0));
    match d.entries.iter_mut().find(|(k, _)| k == key) {
        Some(e) => e.1 = value,
        None => d.entries.push((key.to_vec(), value)),
    }
}

/// Removes `key` from the named database `db`; true if it was there.
pub fn remove(env: &mut EnvContents, db: &str, key: &[u8]) -> bool {
    match env.named.get_mut(db.as_bytes()) {
        Some(d) => {
            let before = d.entries.len();
            d.entries.retain(|(k, _)| k != key);
            d.entries.len() != before
        }
        None => false,
    }
}

/// True for a 32-character lower-case hexadecimal world id.
pub fn is_world_id(id: &str) -> bool {
    id.len() == 32 && id.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Guesses the kind of a world held in memory (see [`SaveKind::detect`]).
pub fn detect_kind_files(files: &FileMap) -> Option<SaveKind> {
    let top = |name: &str| files.keys().any(|k| k == name || k.starts_with(&format!("{name}/")));
    if files.contains_key("worldv2") || top("server_db") || top("lightBlocks") {
        Some(SaveKind::Server)
    } else if top("world_db") {
        Some(SaveKind::Client)
    } else {
        None
    }
}

/// A game client world (`saves/<id>/`).
#[derive(Debug, Clone)]
pub struct ClientSave {
    /// Folder name (normally the world id).
    pub dir_name: String,
    /// On-disk layout of `world_db/data.mdb` (32-bit for Android and PC; 64-bit for 64-bit iOS builds).
    pub layout: Layout,
    /// Contents of `world_db`.
    pub world_db: EnvContents,
    /// All other files (`map/…` and anything else), `world_db/data.mdb` excluded.
    pub other_files: FileMap,
    /// Total size of the world's files.
    pub size: u64,
}

impl ClientSave {
    /// Reads a world folder.
    pub fn open(dir: &Path) -> Result<ClientSave> {
        ClientSave::from_files(&dir_name(dir), read_dir_files(dir)?)
    }

    /// Parses a world held in memory.
    pub fn from_files(dir_name: &str, mut files: FileMap) -> Result<ClientSave> {
        let size = files.values().map(|v| v.len() as u64).sum();
        let (world_db, layout) =
            env_from(&files, WORLD_DB)?.ok_or_else(|| Error::Format(format!("{dir_name}: no {WORLD_DB}")))?;
        files.remove(WORLD_DB);
        Ok(ClientSave { dir_name: dir_name.to_string(), layout, world_db, other_files: files, size })
    }

    /// The raw `main/worldv2` record.
    pub fn worldv2(&self) -> Option<&[u8]> {
        get(&self.world_db, "main", "worldv2")
    }
}

/// A Blockheads Server world (`saves/<id>/`).
#[derive(Debug, Clone)]
pub struct ServerSave {
    /// Folder name (normally the world id).
    pub dir_name: String,
    /// Contents of `world_db`.
    pub world_db: EnvContents,
    /// Contents of `server_db` (player records), if present.
    pub server_db: Option<EnvContents>,
    /// Contents of `lightBlocks` (per-player explored areas), if present.
    pub light_blocks: Option<EnvContents>,
    /// All files except `world_db/data.mdb` (so `worldv2`, the text lists, `server_db/data.mdb`, …).
    pub other_files: FileMap,
    /// Total size of the world's files.
    pub size: u64,
}

impl ServerSave {
    /// Reads a world folder.
    pub fn open(dir: &Path) -> Result<ServerSave> {
        ServerSave::from_files(&dir_name(dir), read_dir_files(dir)?)
    }

    /// Parses a world held in memory.
    pub fn from_files(dir_name: &str, mut files: FileMap) -> Result<ServerSave> {
        let size = files.values().map(|v| v.len() as u64).sum();
        let world_db =
            env_from(&files, WORLD_DB)?.ok_or_else(|| Error::Format(format!("{dir_name}: no {WORLD_DB}")))?.0;
        files.remove(WORLD_DB);
        let server_db = env_from(&files, SERVER_DB)?.map(|e| e.0);
        let light_blocks = env_from(&files, LIGHT_BLOCKS)?.map(|e| e.0);
        Ok(ServerSave { dir_name: dir_name.to_string(), world_db, server_db, light_blocks, other_files: files, size })
    }

    /// The `main/worldv2` record, or the `worldv2` file when the world was never saved by the server.
    pub fn worldv2(&self) -> Option<&[u8]> {
        get(&self.world_db, "main", "worldv2").or(self.other_files.get("worldv2").map(Vec::as_slice))
    }
}

/// `world_db/data.mdb`.
pub const WORLD_DB: &str = "world_db/data.mdb";
/// `server_db/data.mdb`.
pub const SERVER_DB: &str = "server_db/data.mdb";
/// `lightBlocks/data.mdb`.
pub const LIGHT_BLOCKS: &str = "lightBlocks/data.mdb";

fn dir_name(dir: &Path) -> String {
    dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

/// A world of either kind.
#[derive(Debug, Clone)]
pub enum AnySave {
    /// A game client world.
    Client(ClientSave),
    /// A Blockheads Server world.
    Server(ServerSave),
}

impl AnySave {
    /// Reads a world folder of either kind.
    pub fn open(dir: &Path) -> Result<AnySave> {
        AnySave::from_files(&dir_name(dir), read_dir_files(dir)?)
    }

    /// Detects the kind with [`detect_kind_files`] and parses the world.
    pub fn from_files(dir_name: &str, files: FileMap) -> Result<AnySave> {
        match detect_kind_files(&files) {
            Some(SaveKind::Client) => Ok(AnySave::Client(ClientSave::from_files(dir_name, files)?)),
            Some(SaveKind::Server) => Ok(AnySave::Server(ServerSave::from_files(dir_name, files)?)),
            None => Err(Error::Format(format!("{dir_name} is not a Blockheads world"))),
        }
    }

    /// `world_db` contents.
    pub fn world_db(&self) -> &EnvContents {
        match self {
            AnySave::Client(c) => &c.world_db,
            AnySave::Server(s) => &s.world_db,
        }
    }

    /// The world record (`main/worldv2`, or a server's `worldv2` file).
    pub fn worldv2(&self) -> Option<&[u8]> {
        match self {
            AnySave::Client(c) => c.worldv2(),
            AnySave::Server(s) => s.worldv2(),
        }
    }
}
