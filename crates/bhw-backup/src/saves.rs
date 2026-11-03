//! Finding The Blockheads' worlds inside a backup and building edits that replace them.
//!
//! The game is an Apportable port of the iOS app. Blockheads PC keeps worlds in
//! `<home>/Library/Application Support/saves/<32 hex id>/` (`world_db/data.mdb`, `world_db/lock.mdb`, `map/...`) next to
//! a global `game_db/`. Where Apportable maps that home directory on Android is **not verified yet** (probably below
//! the `f` (files) domain, possibly `r` or `ef`), so nothing here hard-codes it: discovery looks for
//! `saves/<32 lowercase hex>/world_db/data.mdb` anywhere in the archive and remembers the prefix in front of the
//! `<id>` (the [`SavesDir::prefix`]); writing back uses exactly that prefix.

use std::collections::BTreeMap;
use std::io::Read;

use crate::edit::{is_safe_rel_path, NewEntry, Payload, TarEdit};
use crate::header::BackupHeader;
use crate::stream::open_backup;
use crate::tar::{EntryKind, TarHeader, TarReader};
use crate::{Error, Result};

/// Android package of The Blockheads.
pub const BLOCKHEADS_PACKAGE: &str = "com.noodlecake.blockheads";
/// File whose presence marks a world directory.
pub const WORLD_MARKER: &str = "world_db/data.mdb";
/// Permission bits used for new files (restore masks with 0700).
pub const FILE_MODE: u32 = 0o600;
/// Permission bits used for new directories.
pub const DIR_MODE: u32 = 0o700;

/// Metadata of one tar entry (no data).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryInfo {
    /// Full path in the archive (or relative to a world / `game_db` directory where documented).
    pub path: String,
    /// File, directory, ...
    pub kind: EntryKind,
    /// Size in bytes.
    pub size: u64,
    /// Permission bits.
    pub mode: u32,
    /// Modification time (seconds).
    pub mtime: u64,
}

impl From<&TarHeader> for EntryInfo {
    fn from(h: &TarHeader) -> Self {
        Self { path: h.path.clone(), kind: h.kind, size: h.size, mode: h.mode, mtime: h.mtime }
    }
}

/// One world found in a backup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorldEntry {
    /// 32 lowercase hex id (the directory name).
    pub id: String,
    /// Entries below the world directory, paths relative to it (e.g. `world_db/data.mdb`).
    pub files: Vec<EntryInfo>,
}

/// A `saves` directory found in a backup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavesDir {
    /// Archive path up to and including `saves/`, e.g. `apps/com.noodlecake.blockheads/f/Library/Application Support/saves/`.
    pub prefix: String,
    /// Package name if the path is under `apps/<package>/`.
    pub package: Option<String>,
    /// Backup domain token (`f`, `r`, `ef`, ...) if under `apps/<package>/<domain>/`.
    pub domain: Option<String>,
    /// Worlds (directories that contain [`WORLD_MARKER`]), sorted by id.
    pub worlds: Vec<WorldEntry>,
    /// Archive path of the sibling `game_db/` directory (ends with `/`), if present. Global game data: left untouched.
    pub game_db_prefix: Option<String>,
    /// Entries below `game_db/`, paths relative to it.
    pub game_db: Vec<EntryInfo>,
}

/// `true` for a 32 character lowercase hex world id.
pub fn is_world_id(s: &str) -> bool {
    s.len() == 32 && s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Splits `<prefix>saves/<id>[/<rest>]` into `(prefix incl. "saves/", id, rest)`.
pub fn split_save_path(path: &str) -> Option<(&str, &str, &str)> {
    let mut from = 0;
    while let Some(pos) = path[from..].find("saves/") {
        let at = from + pos;
        from = at + 1;
        if at > 0 && path.as_bytes()[at - 1] != b'/' {
            continue;
        }
        let after = &path[at + 6..];
        let id = after.get(..32).unwrap_or("");
        if !is_world_id(id) {
            continue;
        }
        match after.as_bytes().get(32) {
            None => return Some((&path[..at + 6], id, "")),
            Some(b'/') => return Some((&path[..at + 6], id, &after[33..])),
            Some(_) => continue,
        }
    }
    None
}

/// Splits `apps/<package>/<domain>/...` into `(package, domain)`.
pub fn split_android_path(path: &str) -> Option<(&str, Option<&str>)> {
    let rest = path.strip_prefix("apps/")?;
    let mut it = rest.splitn(3, '/');
    let package = it.next().filter(|p| !p.is_empty())?;
    let domain = it.next().filter(|d| !d.is_empty() && it.next().is_some());
    Some((package, domain))
}

fn saves_parent_of_dir(path: &str) -> Option<&str> {
    if path == "saves" {
        return Some("");
    }
    path.strip_suffix("/saves").map(|p| &path[..p.len() + 1])
}

/// Groups entries into saves directories and worlds.
///
/// A saves directory is reported when it holds at least one world, or when it exists as a directory entry next to a
/// `game_db` directory (an empty saves folder of an installed game, so worlds can be added to a device that has none).
pub fn analyze(entries: &[EntryInfo]) -> Vec<SavesDir> {
    let mut found: BTreeMap<String, BTreeMap<String, Vec<EntryInfo>>> = BTreeMap::new();
    for e in entries {
        if let Some((prefix, id, rest)) = split_save_path(&e.path) {
            let world = found.entry(prefix.to_string()).or_default().entry(id.to_string()).or_default();
            if !rest.is_empty() {
                world.push(EntryInfo { path: rest.to_string(), ..e.clone() });
            }
        } else if let Some(parent) = saves_parent_of_dir(&e.path) {
            found.entry(format!("{parent}saves/")).or_default();
        }
    }
    let mut out = Vec::new();
    for (prefix, worlds) in found {
        let parent = &prefix[..prefix.len() - 6];
        let gdb = format!("{parent}game_db/");
        let game_db: Vec<EntryInfo> = entries
            .iter()
            .filter(|e| e.path.starts_with(&gdb))
            .map(|e| EntryInfo { path: e.path[gdb.len()..].to_string(), ..e.clone() })
            .collect();
        let has_game_db = !game_db.is_empty() || entries.iter().any(|e| e.path == gdb[..gdb.len() - 1]);
        let worlds: Vec<WorldEntry> = worlds
            .into_iter()
            .filter(|(_, files)| files.iter().any(|f| f.path == WORLD_MARKER && f.kind == EntryKind::File))
            .map(|(id, files)| WorldEntry { id, files })
            .collect();
        if worlds.is_empty() && !has_game_db {
            continue;
        }
        let (package, domain) = split_android_path(&prefix).unwrap_or(("", None));
        out.push(SavesDir {
            package: Some(package.to_string()).filter(|p| !p.is_empty()),
            domain: domain.map(str::to_string),
            game_db_prefix: has_game_db.then_some(gdb),
            game_db,
            worlds,
            prefix,
        });
    }
    out
}

/// Result of [`scan`] / [`read_saves`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavesContent {
    /// Backup header.
    pub header: BackupHeader,
    /// Every entry of the archive, in order.
    pub entries: Vec<EntryInfo>,
    /// Saves directories found.
    pub saves_dirs: Vec<SavesDir>,
    /// Data of files below any `saves/<id>/` or `game_db/` directory, by full archive path ([`read_saves`] only).
    pub files: BTreeMap<String, Vec<u8>>,
}

fn is_saves_data(path: &str) -> bool {
    split_save_path(path).is_some() || path.split('/').any(|c| c == "game_db")
}

fn read_impl(input: Box<dyn Read + '_>, password: Option<&str>, keep_data: bool) -> Result<SavesContent> {
    let (header, stream) = open_backup(input, password)?;
    let mut tar = TarReader::new(stream);
    let mut entries = Vec::new();
    let mut files = BTreeMap::new();
    while let Some(h) = tar.next_header()? {
        if keep_data && h.kind == EntryKind::File && is_saves_data(&h.path) {
            let mut data = Vec::new();
            tar.read_data(&mut data)?;
            files.insert(h.path.clone(), data);
        }
        entries.push(EntryInfo::from(&h));
    }
    let saves_dirs = analyze(&entries);
    Ok(SavesContent { header, entries, saves_dirs, files })
}

/// Lists a backup's entries and saves directories without keeping any file data (one streaming pass).
pub fn scan(input: Box<dyn Read + '_>, password: Option<&str>) -> Result<SavesContent> {
    read_impl(input, password, false)
}

/// Like [`scan`] but also keeps the data of all world and `game_db` files in memory (one streaming pass; other
/// entries such as the APK are skipped).
pub fn read_saves(input: Box<dyn Read + '_>, password: Option<&str>) -> Result<SavesContent> {
    read_impl(input, password, true)
}

/// A world to put into a backup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorldPut {
    /// 32 lowercase hex id.
    pub id: String,
    /// `(path relative to the world directory, contents)`.
    pub files: Vec<(String, Payload)>,
}

/// Anchors for inserting below `saves_prefix`: the saves dir and each ancestor up to `apps/<package>`.
fn anchors_for(saves_prefix: &str) -> Vec<String> {
    let mut v = Vec::new();
    let mut p = saves_prefix.trim_end_matches('/');
    let stop = split_android_path(p).map(|(pkg, _)| pkg.len() + 5).unwrap_or(0);
    loop {
        v.push(p.to_string());
        match p.rfind('/') {
            Some(i) if i >= stop && i > 0 => p = &p[..i],
            _ => break,
        }
    }
    v
}

/// Builds a [`TarEdit`] that removes the worlds `remove` and puts (adds or replaces) the worlds `put` below
/// `saves_prefix` (as found by [`analyze`]; must end with `saves/`). New entries get `mtime` and modes
/// [`FILE_MODE`]/[`DIR_MODE`]; directory entries are emitted for the world directory and every subdirectory.
pub fn world_edit(saves_prefix: &str, remove: &[&str], put: &[WorldPut], mtime: u64) -> Result<TarEdit> {
    if !(saves_prefix == "saves/" || saves_prefix.ends_with("/saves/")) {
        return Err(Error::Invalid(format!("saves prefix must end with saves/: {saves_prefix:?}")));
    }
    let mut edit = TarEdit { anchors: anchors_for(saves_prefix), ..TarEdit::default() };
    for id in remove.iter().copied().chain(put.iter().map(|w| w.id.as_str())) {
        if !is_world_id(id) {
            return Err(Error::Invalid(format!("bad world id {id:?}")));
        }
        edit.remove.push(format!("{saves_prefix}{id}"));
    }
    for w in put {
        let root = format!("{saves_prefix}{}", w.id);
        let mut dirs = std::collections::BTreeSet::new();
        let mut files = Vec::new();
        for (rel, data) in &w.files {
            if !is_safe_rel_path(rel) {
                return Err(Error::Invalid(format!("bad path in world {}: {rel:?}", w.id)));
            }
            for (i, _) in rel.match_indices('/') {
                dirs.insert(rel[..i].to_string());
            }
            files.push((rel, data));
        }
        files.sort_by(|a, b| a.0.cmp(b.0));
        let dir = |path: String| NewEntry {
            path,
            kind: EntryKind::Directory,
            mode: DIR_MODE,
            mtime,
            data: Payload::Bytes(vec![]),
        };
        edit.insert.push(dir(root.clone()));
        edit.insert.extend(dirs.into_iter().map(|d| dir(format!("{root}/{d}"))));
        for (rel, data) in files {
            edit.insert.push(NewEntry {
                path: format!("{root}/{rel}"),
                kind: EntryKind::File,
                mode: FILE_MODE,
                mtime,
                data: data.clone(),
            });
        }
    }
    Ok(edit)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::rewrite_backup;
    use crate::stream::tests::{list, make_backup};
    use crate::stream::BackupOptions;

    const A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const B: &str = "0123456789abcdef0123456789abcdef";

    fn info(path: &str, kind: EntryKind) -> EntryInfo {
        EntryInfo { path: path.into(), kind, size: 0, mode: 0, mtime: 0 }
    }

    #[test]
    fn ids_and_paths() {
        assert!(is_world_id(A));
        assert!(!is_world_id(&A.to_uppercase()));
        assert!(!is_world_id("abc"));
        assert!(!is_world_id(&format!("{}g", &A[..31])));
        assert_eq!(
            split_save_path(&format!("x/saves/{A}/world_db/data.mdb")),
            Some(("x/saves/", A, "world_db/data.mdb"))
        );
        assert_eq!(split_save_path(&format!("saves/{A}")), Some(("saves/", A, "")));
        assert_eq!(split_save_path(&format!("x/mysaves/{A}/a")), None);
        assert_eq!(split_save_path(&format!("x/saves/{A}x/a")), None);
        assert_eq!(split_save_path("x/saves/short"), None);
        assert_eq!(split_save_path(&format!("saves/saves/{A}/f")), Some(("saves/saves/", A, "f")));
        assert_eq!(split_android_path("apps/p/f/x"), Some(("p", Some("f"))));
        assert_eq!(split_android_path("apps/p/_manifest"), Some(("p", None)));
        assert_eq!(split_android_path("apps/p"), Some(("p", None)));
        assert_eq!(split_android_path("apps//x"), None);
        assert_eq!(split_android_path("shared/0/x"), None);
        assert_eq!(saves_parent_of_dir("saves"), Some(""));
        assert_eq!(saves_parent_of_dir("a/saves"), Some("a/"));
        assert_eq!(saves_parent_of_dir("a/saves2"), None);
        assert_eq!(anchors_for("apps/p/f/L/saves/"), ["apps/p/f/L/saves", "apps/p/f/L", "apps/p/f", "apps/p"]);
        assert_eq!(anchors_for("x/saves/"), ["x/saves", "x"]);
        assert_eq!(anchors_for("saves/"), ["saves"]);
        assert!(is_saves_data("a/game_db/x"));
        assert!(!is_saves_data("a/game_dbx"));
    }

    #[test]
    fn analyze_groups_worlds_and_game_db() {
        let p = "apps/com.noodlecake.blockheads/f/Library/Application Support/";
        let entries = vec![
            info(&format!("{p}saves"), EntryKind::Directory),
            info(&format!("{p}saves/{A}"), EntryKind::Directory),
            info(&format!("{p}saves/{A}/world_db/data.mdb"), EntryKind::File),
            info(&format!("{p}saves/{A}/map/default/1_2.png"), EntryKind::File),
            info(&format!("{p}saves/{B}/map/x"), EntryKind::File),
            info(&format!("{p}game_db/data.mdb"), EntryKind::File),
            // Saves dir without worlds and without game_db: ignored.
            info("apps/other/f/saves", EntryKind::Directory),
            // Empty saves dir next to an (empty) game_db dir: reported.
            info("apps/x/r/saves", EntryKind::Directory),
            info("apps/x/r/game_db", EntryKind::Directory),
            // Bare tar path outside apps/.
            info(&format!("saves/{B}/world_db/data.mdb"), EntryKind::File),
        ];
        let s = analyze(&entries);
        assert_eq!(s.len(), 3);
        let main = s.iter().find(|d| d.prefix == format!("{p}saves/")).unwrap();
        assert_eq!(main.package.as_deref(), Some(BLOCKHEADS_PACKAGE));
        assert_eq!(main.domain.as_deref(), Some("f"));
        assert_eq!(main.worlds.len(), 1);
        assert_eq!(main.worlds[0].id, A);
        assert_eq!(main.worlds[0].files.len(), 2);
        assert_eq!(main.game_db_prefix.as_deref(), Some(&*format!("{p}game_db/")));
        assert_eq!(main.game_db[0].path, "data.mdb");
        let x = s.iter().find(|d| d.prefix == "apps/x/r/saves/").unwrap();
        assert!(x.worlds.is_empty());
        assert!(x.game_db.is_empty());
        assert_eq!(x.game_db_prefix.as_deref(), Some("apps/x/r/game_db/"));
        let bare = s.iter().find(|d| d.prefix == "saves/").unwrap();
        assert_eq!((bare.package.clone(), bare.domain.clone(), bare.game_db_prefix.clone()), (None, None, None));
    }

    #[test]
    fn world_edit_validation_and_shape() {
        assert!(world_edit("apps/p/f/x/", &[], &[], 0).is_err());
        assert!(world_edit("apps/p/f/saves/", &["bad"], &[], 0).is_err());
        let put = |rel: &str| WorldPut { id: B.into(), files: vec![(rel.into(), Payload::Bytes(vec![1]))] };
        assert!(world_edit("saves/", &[], &[put("../x")], 0).is_err());
        let e = world_edit(
            "apps/p/f/saves/",
            &[A],
            &[WorldPut {
                id: B.into(),
                files: vec![
                    ("world_db/data.mdb".into(), Payload::Bytes(vec![1])),
                    ("map/default/0_0.png".into(), Payload::Bytes(vec![2])),
                ],
            }],
            7,
        )
        .unwrap();
        assert_eq!(e.remove, [format!("apps/p/f/saves/{A}"), format!("apps/p/f/saves/{B}")]);
        let paths: Vec<_> = e.insert.iter().map(|n| n.path.strip_prefix("apps/p/f/saves/").unwrap()).collect();
        let b = |s: &str| format!("{B}{s}");
        assert_eq!(
            paths,
            [b(""), b("/map"), b("/map/default"), b("/world_db"), b("/map/default/0_0.png"), b("/world_db/data.mdb")]
        );
        assert!(e.insert.iter().all(|n| n.mtime == 7));
    }

    #[test]
    fn scan_read_and_replace_round_trip() {
        let pfx = "apps/com.noodlecake.blockheads/f/Library/Application Support/";
        let a_data = format!("{pfx}saves/{A}/world_db/data.mdb");
        let a_map = format!("{pfx}saves/{A}/map/default/0_0.png");
        let gdb = format!("{pfx}game_db/data.mdb");
        let src = make_backup(
            &BackupOptions::default(),
            &[
                ("apps/com.noodlecake.blockheads/_manifest", Some(b"m")),
                ("apps/com.noodlecake.blockheads/a/base.apk", Some(b"APK")),
                (&format!("{pfx}saves"), None),
                (&format!("{pfx}saves/{A}"), None),
                (&a_data, Some(b"world A")),
                (&a_map, Some(b"png")),
                (&format!("{pfx}game_db"), None),
                (&gdb, Some(b"global")),
                ("apps/com.noodlecake.blockheads/sp/prefs.xml", Some(b"<map/>")),
            ],
        );
        let s = scan(Box::new(&src[..]), None).unwrap();
        assert!(s.files.is_empty());
        assert_eq!(s.entries.len(), 9);
        let r = read_saves(Box::new(&src[..]), None).unwrap();
        assert_eq!(r.files.len(), 3);
        assert_eq!(r.files[&a_data], b"world A");
        assert_eq!(r.files[&gdb], b"global");
        let dir = &r.saves_dirs[0];
        assert_eq!(dir.worlds[0].id, A);

        let edit = world_edit(
            &dir.prefix,
            &[A],
            &[WorldPut {
                id: B.into(),
                files: vec![("world_db/data.mdb".into(), Payload::Bytes(b"world B".to_vec()))],
            }],
            1,
        )
        .unwrap();
        let mut out = Vec::new();
        rewrite_backup(Box::new(&src[..]), None, Box::new(&mut out), None, &edit).unwrap();
        let after = read_saves(Box::new(&out[..]), None).unwrap();
        let ids: Vec<_> = after.saves_dirs[0].worlds.iter().map(|w| w.id.as_str()).collect();
        assert_eq!(ids, [B]);
        assert_eq!(after.files[&gdb], b"global");
        let all = list(&out, None).unwrap();
        let names: Vec<_> = all.iter().map(|(p, _)| p.strip_prefix(pfx).unwrap_or(p).to_string()).collect();
        assert_eq!(
            names,
            [
                "apps/com.noodlecake.blockheads/_manifest".to_string(),
                "apps/com.noodlecake.blockheads/a/base.apk".into(),
                "saves".into(),
                format!("saves/{B}"),
                format!("saves/{B}/world_db"),
                format!("saves/{B}/world_db/data.mdb"),
                "game_db".into(),
                "game_db/data.mdb".into(),
                "apps/com.noodlecake.blockheads/sp/prefs.xml".into(),
            ]
        );
    }
}
