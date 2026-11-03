//! One-pass editing of a backup's tar stream: drop some entries, add new ones, copy everything else byte for byte.
//!
//! # Why a rewrite and not a fresh backup
//!
//! Android's full restore (`FullRestoreEngine.restoreOneFile`) **clears the package's data**
//! (`clearApplicationDataSynchronous`) before it restores the first data file of a package. A backup that only
//! contains the new world would therefore wipe every other world, the game settings and purchases. So the tool always
//! starts from a fresh `adb backup` of the device and rewrites it:
//!
//! * every entry that is kept is copied unchanged (all header blocks, data, padding), so the order, the
//!   `apps/<pkg>/_manifest` entry at the start of the package (which restore needs first to decide on a restore policy:
//!   signature check, version code) and the header style stay exactly as Android wrote them;
//! * removed entries (e.g. a world being replaced) are skipped;
//! * new entries are written in Android's own dialect ([`crate::tar::android_header`]) and inserted **inside the same
//!   package section**, at the end of the innermost *anchor* directory that occurs in the backup (normally the
//!   `.../saves` directory itself). Restore needs a package's entries to be contiguous after its manifest: an entry for
//!   another package would end the current agent session and data after it without a manifest is skipped.
//!
//! Restore recreates missing parent directories of files itself (`FullBackup.restoreFile` calls `mkdirs()`), but new
//! directory entries are still written (parents first) to mirror what `fullBackupFileTree` produces.

use std::io::{self, Read, Write};
use std::path::PathBuf;

use crate::stream::{open_backup, BackupOptions, BackupWriter};
use crate::tar::{EntryKind, TarReader, TarWriter};
use crate::{Error, Result};

/// Contents of a file to write: in memory or read from disk when needed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Payload {
    /// Bytes in memory.
    Bytes(Vec<u8>),
    /// A file on disk, read when written.
    File(PathBuf),
}

impl Payload {
    /// Size in bytes.
    pub fn len(&self) -> io::Result<u64> {
        match self {
            Payload::Bytes(b) => Ok(b.len() as u64),
            Payload::File(p) => Ok(std::fs::metadata(p)?.len()),
        }
    }

    /// `true` when empty.
    pub fn is_empty(&self) -> io::Result<bool> {
        Ok(self.len()? == 0)
    }

    /// Opens the contents for reading.
    pub fn open(&self) -> io::Result<Box<dyn Read + '_>> {
        match self {
            Payload::Bytes(b) => Ok(Box::new(b.as_slice())),
            Payload::File(p) => Ok(Box::new(io::BufReader::new(std::fs::File::open(p)?))),
        }
    }

    /// Reads everything into memory.
    pub fn read_all(&self) -> io::Result<Vec<u8>> {
        let mut v = Vec::new();
        self.open()?.read_to_end(&mut v)?;
        Ok(v)
    }
}

/// `true` for a relative `/`-separated path without empty, `.` or `..` components, backslashes, NUL or drive letters.
pub fn is_safe_rel_path(p: &str) -> bool {
    !p.is_empty() && !p.contains(['\\', '\0', ':']) && p.split('/').all(|c| !c.is_empty() && c != "." && c != "..")
}

/// A new tar entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewEntry {
    /// Full tar path, e.g. `apps/com.noodlecake.blockheads/f/.../saves/<id>/world_db/data.mdb`.
    pub path: String,
    /// File or directory.
    pub kind: EntryKind,
    /// Permission bits (restore masks them with 0700).
    pub mode: u32,
    /// Modification time, seconds since the epoch.
    pub mtime: u64,
    /// Contents (ignored for directories).
    pub data: Payload,
}

/// What to change in a tar stream.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TarEdit {
    /// Paths to drop: an entry is dropped when its path equals one of these or lies below it (`p/...`).
    pub remove: Vec<String>,
    /// Entries to add, in order. Existing entries with the same paths are dropped.
    pub insert: Vec<NewEntry>,
    /// Directories after whose contents the new entries go, most specific first (e.g. `.../saves`, `.../f`,
    /// `apps/<pkg>`). Entries are inserted where the run of the first anchor seen ends. Must be nested.
    pub anchors: Vec<String>,
}

/// Counters of an edit.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EditStats {
    /// Entries copied unchanged.
    pub kept: usize,
    /// Entries dropped.
    pub removed: usize,
    /// Entries added.
    pub inserted: usize,
}

fn under(path: &str, dir: &str) -> bool {
    path == dir || (path.len() > dir.len() && path.starts_with(dir) && path.as_bytes()[dir.len()] == b'/')
}

fn insert_all(w: &mut TarWriter<'_>, edit: &TarEdit) -> Result<usize> {
    for e in &edit.insert {
        let size = if e.kind == EntryKind::Directory { 0 } else { e.data.len()? };
        let mut data: Box<dyn Read> = if size == 0 { Box::new(io::empty()) } else { e.data.open()? };
        w.append(&e.path, e.kind, e.mode, e.mtime, size, &mut data)?;
    }
    Ok(edit.insert.len())
}

/// Applies `edit` while copying `reader` to `writer` (the end-of-archive marker is written too).
pub fn apply_tar_edit(reader: &mut TarReader<'_>, writer: &mut TarWriter<'_>, edit: &TarEdit) -> Result<EditStats> {
    let mut stats = EditStats::default();
    let mut seen = vec![false; edit.anchors.len()];
    let mut done = edit.insert.is_empty();
    while let Some(h) = reader.next_header()? {
        let inside: Vec<bool> = edit.anchors.iter().map(|a| under(&h.path, a)).collect();
        if !done && seen.iter().zip(&inside).any(|(&s, &i)| s && !i) {
            stats.inserted = insert_all(writer, edit)?;
            done = true;
        }
        for (s, i) in seen.iter_mut().zip(&inside) {
            *s |= *i;
        }
        let dropped = edit.remove.iter().any(|r| under(&h.path, r)) || edit.insert.iter().any(|e| e.path == h.path);
        if dropped {
            stats.removed += 1;
            continue;
        }
        writer.write_raw(&h, &mut reader.data())?;
        stats.kept += 1;
    }
    if !done {
        if !seen.contains(&true) {
            return Err(Error::AnchorNotFound(edit.anchors.clone()));
        }
        stats.inserted = insert_all(writer, edit)?;
    }
    Ok(stats)
}

/// Rewrites a backup in one pass: decrypts/decompresses `input`, applies `edit`, and writes a new backup to `output`
/// with the **same version and compression** as the input, encrypted with `output_password` if given (fresh salts,
/// keys and IVs; Android asks for this password on restore).
///
/// `input` is read only once and nothing is buffered beyond one entry's copy buffer, so this works for backups of
/// any size. On error the output is incomplete and must be discarded.
pub fn rewrite_backup<'a>(
    input: Box<dyn Read + 'a>,
    input_password: Option<&str>,
    output: Box<dyn Write + 'a>,
    output_password: Option<&str>,
    edit: &TarEdit,
) -> Result<EditStats> {
    let (header, stream) = open_backup(input, input_password)?;
    let opts = BackupOptions {
        version: header.version,
        compressed: header.compressed,
        password: output_password.map(str::to_string),
        ..BackupOptions::default()
    };
    let mut out = BackupWriter::create(output, &opts)?;
    let mut reader = TarReader::new(stream);
    let mut writer = TarWriter::new(&mut out);
    let stats = apply_tar_edit(&mut reader, &mut writer, edit)?;
    writer.finish()?;
    out.finish()?;
    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stream::tests::{list, make_backup};

    fn file(path: &str, data: &[u8]) -> NewEntry {
        NewEntry {
            path: path.into(),
            kind: EntryKind::File,
            mode: 0o600,
            mtime: 1,
            data: Payload::Bytes(data.to_vec()),
        }
    }

    fn dir(path: &str) -> NewEntry {
        NewEntry { path: path.into(), kind: EntryKind::Directory, mode: 0o771, mtime: 1, data: Payload::Bytes(vec![]) }
    }

    const SRC: [(&str, Option<&[u8]>); 9] = [
        ("apps/p/_manifest", Some(b"manifest")),
        ("apps/p/f", None),
        ("apps/p/f/saves", None),
        ("apps/p/f/saves/w1", None),
        ("apps/p/f/saves/w1/data", Some(b"old1")),
        ("apps/p/f/saves/w2/data", Some(b"old2")),
        ("apps/p/f/other", Some(b"other")),
        ("apps/q/_manifest", Some(b"q")),
        ("apps/q/f/x", Some(b"x")),
    ];

    fn paths(v: &[(String, Vec<u8>)]) -> Vec<&str> {
        v.iter().map(|(p, _)| p.as_str()).collect()
    }

    type Listing = Vec<(String, Vec<u8>)>;

    fn rewrite(src: &[u8], edit: &TarEdit) -> Result<(EditStats, Listing)> {
        let mut out = Vec::new();
        let stats = rewrite_backup(Box::new(src), Some("in"), Box::new(&mut out), None, edit)?;
        Ok((stats, list(&out, None).unwrap()))
    }

    fn src() -> Vec<u8> {
        make_backup(&BackupOptions { version: 3, password: Some("in".into()), rounds: 10, ..Default::default() }, &SRC)
    }

    #[test]
    fn replace_world_inserts_at_end_of_saves_run() {
        let edit = TarEdit {
            remove: vec!["apps/p/f/saves/w1".into()],
            insert: vec![
                dir("apps/p/f/saves/w1"),
                file("apps/p/f/saves/w1/data", b"new1"),
                file("apps/p/f/saves/w2/data", b"new2"),
            ],
            anchors: vec!["apps/p/f/saves".into(), "apps/p/f".into(), "apps/p".into()],
        };
        let (stats, got) = rewrite(&src(), &edit).unwrap();
        assert_eq!(stats, EditStats { kept: 6, removed: 3, inserted: 3 });
        assert_eq!(
            paths(&got),
            [
                "apps/p/_manifest",
                "apps/p/f",
                "apps/p/f/saves",
                "apps/p/f/saves/w1",
                "apps/p/f/saves/w1/data",
                "apps/p/f/saves/w2/data",
                "apps/p/f/other",
                "apps/q/_manifest",
                "apps/q/f/x"
            ]
        );
        assert_eq!(got[4].1, b"new1");
        assert_eq!(got[5].1, b"new2");
        assert_eq!(got[6].1, b"other");
    }

    #[test]
    fn falls_back_to_outer_anchor_and_eof() {
        // Innermost anchor missing: insert at the end of apps/p/f.
        let edit = TarEdit {
            remove: vec![],
            insert: vec![file("apps/p/f/new/a", b"a")],
            anchors: vec!["apps/p/f/new".into(), "apps/p/f".into()],
        };
        let (_, got) = rewrite(&src(), &edit).unwrap();
        assert_eq!(paths(&got)[7], "apps/p/f/new/a");
        // Anchor run reaching the end of the archive.
        let edit = TarEdit { anchors: vec!["apps/q".into()], ..edit };
        let (_, got) = rewrite(&src(), &edit).unwrap();
        assert_eq!(paths(&got).last(), Some(&"apps/p/f/new/a"));
        // No anchor at all.
        let edit = TarEdit { anchors: vec!["apps/zzz".into()], ..edit };
        assert!(matches!(rewrite(&src(), &edit), Err(Error::AnchorNotFound(a)) if a == ["apps/zzz"]));
        // Removal only needs no anchor; prefix match is per path component.
        let edit = TarEdit { remove: vec!["apps/p/f/saves/w".into(), "apps/q".into()], ..TarEdit::default() };
        let (stats, got) = rewrite(&src(), &edit).unwrap();
        assert_eq!((stats.removed, got.len()), (2, 7));
    }

    #[test]
    fn kept_entries_are_byte_identical() {
        let plain = make_backup(&BackupOptions { compressed: false, ..Default::default() }, &SRC);
        let mut out = Vec::new();
        rewrite_backup(Box::new(&plain[..]), None, Box::new(&mut out), None, &TarEdit::default()).unwrap();
        assert_eq!(out, plain);
    }

    #[test]
    fn payload_from_file_and_errors() {
        let dir_path = std::env::temp_dir().join(format!("bhw-backup-payload-{}", std::process::id()));
        std::fs::create_dir_all(&dir_path).unwrap();
        let f = dir_path.join("p.bin");
        std::fs::write(&f, b"from disk").unwrap();
        let p = Payload::File(f.clone());
        assert_eq!(p.len().unwrap(), 9);
        assert!(!p.is_empty().unwrap());
        assert_eq!(p.read_all().unwrap(), b"from disk");
        let edit = TarEdit {
            insert: vec![NewEntry { data: p, ..file("apps/p/f/saves/z", b"") }],
            anchors: vec!["apps/p/f/saves".into()],
            ..Default::default()
        };
        let (_, got) = rewrite(&src(), &edit).unwrap();
        assert!(got.iter().any(|(p, d)| p == "apps/p/f/saves/z" && d == b"from disk"));
        let missing = Payload::File(dir_path.join("missing"));
        assert!(missing.len().is_err());
        assert!(missing.open().is_err());
        assert!(Payload::Bytes(vec![]).is_empty().unwrap());
        std::fs::remove_dir_all(&dir_path).unwrap();
        // Bad entry path propagates.
        let edit = TarEdit { insert: vec![file("", b"x")], anchors: vec!["apps/p".into()], ..Default::default() };
        assert!(matches!(rewrite(&src(), &edit), Err(Error::Invalid(_))));
        // Wrong input password.
        let mut out = Vec::new();
        let r = rewrite_backup(Box::new(&src()[..]), Some("x"), Box::new(&mut out), None, &TarEdit::default());
        assert!(matches!(r, Err(Error::WrongPassword)));
    }

    #[test]
    fn encrypted_output() {
        let mut out = Vec::new();
        rewrite_backup(Box::new(&src()[..]), Some("in"), Box::new(&mut out), Some("out"), &TarEdit::default()).unwrap();
        assert_eq!(list(&out, Some("out")).unwrap().len(), SRC.len());
        assert!(out.starts_with(b"ANDROID BACKUP\n3\n1\nAES-256\n"));
    }

    #[test]
    fn safe_paths() {
        for ok in ["a", "a/b.c", "world_db/data.mdb", "..a/b"] {
            assert!(is_safe_rel_path(ok), "{ok}");
        }
        for bad in ["", "/a", "a/", "a//b", "./a", "a/../b", "..", "a\\b", "C:x", "a\0"] {
            assert!(!is_safe_rel_path(bad), "{bad}");
        }
    }
}
