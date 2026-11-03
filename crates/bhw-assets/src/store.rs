//! The local game-files cache: `<cache>/game-files/<versionName>-<sha256 prefix>/` with the extracted files and a
//! `manifest.json`; `<cache>/game-files/current` names the active directory. Extraction writes into a temporary
//! directory that is renamed into place only when complete, so a crash or cancel never leaves a half-filled cache.

use crate::apk::{hex, ApkSet, SourceKind, SUPPORTED_VERSIONS};
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

const DIR: &str = "game-files";
const CURRENT: &str = "current";
const MANIFEST: &str = "manifest.json";
/// Version of the `manifest.json` layout.
pub const MANIFEST_FORMAT: u32 = 1;

/// Cooperative cancellation for [`GameFiles::extract`]; clones share the flag.
#[derive(Debug, Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    /// A fresh token.
    pub fn new() -> Self {
        Self::default()
    }
    /// Requests cancellation.
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }
    /// Whether cancellation was requested.
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

/// Extraction progress (reported before the first file and after each file).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Progress {
    pub files_done: usize,
    pub files_total: usize,
    pub bytes_done: u64,
    pub bytes_total: u64,
    /// The file just written (empty before the first one).
    pub current: String,
}

/// One extracted file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileEntry {
    /// Path relative to the cache directory (= relative to `assets/GameResources/` in the APK).
    pub path: String,
    pub size: u64,
    /// Lowercase hex SHA-256.
    pub sha256: String,
    /// PNG dimensions from the IHDR chunk.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
}

/// `manifest.json` of an extraction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    /// [`MANIFEST_FORMAT`].
    pub format: u32,
    /// The APK (set) the files came from.
    pub source: PathBuf,
    pub source_kind: SourceKind,
    /// SHA-256 of the source (see [`crate::ApkInfo::sha256`]).
    pub sha256: String,
    pub package: String,
    pub version_name: String,
    pub version_code: Option<u32>,
    /// Unix time in seconds.
    pub extracted_at: u64,
    pub total_bytes: u64,
    pub files: Vec<FileEntry>,
}

/// State of the cache, for the settings screen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum Status {
    /// Nothing extracted yet.
    NotSet,
    /// Files are present and match the manifest.
    Ready {
        version: String,
        version_code: Option<u32>,
        files: usize,
        bytes: u64,
        source: PathBuf,
        /// Whether the source APK still exists (re-extract possible).
        source_present: bool,
    },
    /// Extracted from a version this build no longer supports.
    Unsupported { version: String },
    /// Files are missing or the manifest is corrupt; re-extract from `source` if known and present.
    Damaged { reason: String, source: Option<PathBuf> },
    /// Files are missing and the source APK is gone: the user has to pick the APK again.
    MissingSource { version: String, source: PathBuf },
}

/// The game-files cache under an app cache directory.
#[derive(Debug, Clone)]
pub struct GameFiles {
    root: PathBuf,
}

fn png_size(head: &[u8]) -> (Option<u32>, Option<u32>) {
    if head.len() >= 24 && head.starts_with(b"\x89PNG\r\n\x1a\n") && &head[12..16] == b"IHDR" {
        let be = |o: usize| u32::from_be_bytes([head[o], head[o + 1], head[o + 2], head[o + 3]]);
        return (Some(be(16)), Some(be(20)));
    }
    (None, None)
}

/// Writes through to a file while hashing and keeping the first bytes (PNG header).
struct Sink<W: Write> {
    inner: W,
    hash: Sha256,
    head: Vec<u8>,
}

impl<W: Write> Write for Sink<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = self.inner.write(buf)?;
        self.hash.update(&buf[..n]);
        let want = 24usize.saturating_sub(self.head.len()).min(n);
        self.head.extend_from_slice(&buf[..want]);
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

fn unique() -> String {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or_default();
    format!("{}-{nanos}", std::process::id())
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let tmp = path.with_extension(format!("tmp-{}", unique()));
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

impl GameFiles {
    /// The cache inside `cache_dir` (the app's cache directory).
    pub fn new(cache_dir: &Path) -> Self {
        Self { root: cache_dir.join(DIR) }
    }

    /// `<cache>/game-files`.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The active extraction directory and its manifest, if any.
    pub fn current(&self) -> Result<Option<(PathBuf, Manifest)>> {
        let name = match fs::read_to_string(self.root.join(CURRENT)) {
            Ok(n) => n.trim().to_owned(),
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        if name.is_empty() || name.starts_with('.') || name.contains(['/', '\\']) {
            return Err(Error::Io(io::Error::new(io::ErrorKind::InvalidData, format!("bad pointer {name:?}"))));
        }
        let dir = self.root.join(name);
        let manifest: Manifest = serde_json::from_slice(&fs::read(dir.join(MANIFEST))?)?;
        Ok(Some((dir, manifest)))
    }

    /// Checks the cache (every file exists with its recorded size).
    pub fn status(&self) -> Status {
        let (dir, m) = match self.current() {
            Ok(None) => return Status::NotSet,
            Ok(Some(c)) => c,
            Err(e) => return Status::Damaged { reason: e.to_string(), source: None },
        };
        if !SUPPORTED_VERSIONS.contains(&m.version_name.as_str()) {
            return Status::Unsupported { version: m.version_name };
        }
        let source_present = m.source.exists();
        let broken =
            m.files.iter().find(|f| fs::metadata(dir.join(&f.path)).map(|md| md.len() != f.size).unwrap_or(true));
        match broken {
            None => Status::Ready {
                version: m.version_name,
                version_code: m.version_code,
                files: m.files.len(),
                bytes: m.total_bytes,
                source: m.source,
                source_present,
            },
            Some(_) if !source_present => Status::MissingSource { version: m.version_name, source: m.source },
            Some(f) => Status::Damaged { reason: format!("{} is missing or changed", f.path), source: Some(m.source) },
        }
    }

    /// Path of a cached file (`rel` as in [`FileEntry::path`]) if it is part of the active extraction.
    pub fn file(&self, rel: &str) -> Result<Option<PathBuf>> {
        Ok(self.current()?.and_then(|(dir, m)| m.files.iter().any(|f| f.path == rel).then(|| dir.join(rel))))
    }

    /// Validates `source` and extracts its game files, replacing the active extraction when complete.
    pub fn extract(
        &self,
        source: &Path,
        progress: &mut dyn FnMut(&Progress),
        cancel: &CancelToken,
    ) -> Result<Manifest> {
        let mut set = ApkSet::open(source)?;
        let info = set.info().clone();
        fs::create_dir_all(&self.root)?;
        let name = format!("{}-{}", info.version_name, &info.sha256[..12]);
        let tmp = self.root.join(format!(".tmp-{name}-{}", unique()));
        let files = match Self::fill(&mut set, &tmp, progress, cancel) {
            Ok(f) => f,
            Err(e) => {
                let _ = fs::remove_dir_all(&tmp);
                return Err(e);
            }
        };
        let manifest = Manifest {
            format: MANIFEST_FORMAT,
            source: info.source,
            source_kind: info.kind,
            sha256: info.sha256,
            package: info.package,
            version_name: info.version_name,
            version_code: info.version_code,
            extracted_at: SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or_default(),
            total_bytes: files.iter().map(|f| f.size).sum(),
            files,
        };
        fs::write(tmp.join(MANIFEST), serde_json::to_vec_pretty(&manifest)?)?;
        let fin = self.root.join(&name);
        if fin.exists() {
            let old = self.root.join(format!(".old-{name}-{}", unique()));
            fs::rename(&fin, &old)?;
        }
        fs::rename(&tmp, &fin)?;
        write_atomic(&self.root.join(CURRENT), name.as_bytes())?;
        // Drop older versions and leftovers of interrupted runs.
        for e in fs::read_dir(&self.root)?.flatten() {
            let n = e.file_name();
            if n != *name && n != CURRENT {
                let p = e.path();
                let _ = if p.is_dir() { fs::remove_dir_all(&p) } else { fs::remove_file(&p) };
            }
        }
        Ok(manifest)
    }

    fn fill(
        set: &mut ApkSet,
        tmp: &Path,
        progress: &mut dyn FnMut(&Progress),
        cancel: &CancelToken,
    ) -> Result<Vec<FileEntry>> {
        fs::create_dir_all(tmp)?;
        let list = set.files();
        let mut p = Progress {
            files_done: 0,
            files_total: list.len(),
            bytes_done: 0,
            bytes_total: list.iter().map(|f| f.1).sum(),
            current: String::new(),
        };
        progress(&p);
        let mut out = Vec::with_capacity(list.len());
        for (rel, _) in list {
            if cancel.is_cancelled() {
                return Err(Error::Cancelled);
            }
            let path = tmp.join(&rel);
            fs::create_dir_all(path.parent().unwrap_or(tmp))?;
            let mut sink = Sink { inner: BufWriter::new(File::create(&path)?), hash: Sha256::new(), head: Vec::new() };
            let n = set.copy_file(&rel, &mut sink)?;
            sink.flush()?;
            let (width, height) = png_size(&sink.head);
            out.push(FileEntry { path: rel.clone(), size: n, sha256: hex(&sink.hash.finalize()), width, height });
            p.files_done += 1;
            p.bytes_done += n;
            p.current = rel;
            progress(&p);
        }
        Ok(out)
    }

    /// Extracts again from the active extraction's source APK.
    pub fn reextract(&self, progress: &mut dyn FnMut(&Progress), cancel: &CancelToken) -> Result<Manifest> {
        let Some((_, m)) = self.current()? else { return Err(Error::NotSet) };
        if !m.source.exists() {
            return Err(Error::MissingSource(m.source));
        }
        self.extract(&m.source, progress, cancel)
    }

    /// Deletes every extracted file.
    pub fn clear(&self) -> Result<()> {
        match fs::remove_dir_all(&self.root) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e.into()),
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apk::testutil::*;
    use crate::apk::PACKAGE;

    fn noop(_: &Progress) {}

    fn setup() -> (tempfile::TempDir, PathBuf, GameFiles) {
        let d = tempfile::tempdir().unwrap();
        let apk_path = write(d.path(), "game.apk", &apk(PACKAGE, "1.7.6", true));
        let gf = GameFiles::new(&d.path().join("cache"));
        (d, apk_path, gf)
    }

    #[test]
    fn extract_status_reextract_clear() {
        let (d, apk_path, gf) = setup();
        assert_eq!(gf.status(), Status::NotSet);
        assert!(matches!(gf.reextract(&mut noop, &CancelToken::new()), Err(Error::NotSet)));
        assert_eq!(gf.file("TileMap.png").unwrap(), None);
        let mut seen = Vec::new();
        let m = gf.extract(&apk_path, &mut |p: &Progress| seen.push(p.clone()), &CancelToken::new()).unwrap();
        assert_eq!(m.files.len(), 7);
        assert_eq!(m.version_name, "1.7.6");
        assert_eq!(seen.len(), 8);
        assert_eq!(seen[0].files_done, 0);
        let last = seen.last().unwrap();
        assert_eq!((last.files_done, last.bytes_done), (7, m.total_bytes));
        assert_eq!(last.bytes_total, m.total_bytes);
        let tm = m.files.iter().find(|f| f.path == "HDTex/TileMap.png").unwrap();
        assert_eq!((tm.width, tm.height), (Some(2048), Some(2048)));
        let fnt = m.files.iter().find(|f| f.path.ends_with(".fnt")).unwrap();
        assert_eq!((fnt.width, fnt.height), (None, None));
        let dir_name = format!("1.7.6-{}", &m.sha256[..12]);
        let p = gf.file("HDTex/TileMap.png").unwrap().unwrap();
        assert_eq!(p, gf.root().join(&dir_name).join("HDTex/TileMap.png"));
        assert_eq!(fs::read(&p).unwrap(), png(2048, 2048, 100));
        assert_eq!(gf.file("nope.png").unwrap(), None);
        assert!(!gf.root().join(&dir_name).join("largo.mp4").exists());
        let Status::Ready { version, files, bytes, source_present, .. } = gf.status() else { unreachable!() };
        assert_eq!((version.as_str(), files, bytes, source_present), ("1.7.6", 7, m.total_bytes, true));
        // Re-extract the same APK: same directory, leftovers cleaned.
        fs::create_dir_all(gf.root().join(".tmp-stale")).unwrap();
        fs::write(gf.root().join("stray"), b"x").unwrap();
        let m2 = gf.reextract(&mut noop, &CancelToken::new()).unwrap();
        assert_eq!(m2.files, m.files);
        let mut names: Vec<_> = fs::read_dir(gf.root()).unwrap().map(|e| e.unwrap().file_name()).collect();
        names.sort();
        assert_eq!(names, [std::ffi::OsString::from(&dir_name), CURRENT.into()]);
        // Another version replaces the old one.
        let other = write(d.path(), "old.apk", &apk(PACKAGE, "1.7.4", true));
        gf.extract(&other, &mut noop, &CancelToken::new()).unwrap();
        assert!(!gf.root().join(&dir_name).exists());
        assert!(matches!(gf.status(), Status::Ready { version, .. } if version == "1.7.4"));
        // Damage: a file disappears; the source is present → Damaged, gone → MissingSource.
        fs::remove_file(gf.file("TileMap.png").unwrap().unwrap()).unwrap();
        assert!(matches!(gf.status(), Status::Damaged { source: Some(_), .. }));
        fs::remove_file(&other).unwrap();
        assert!(matches!(gf.status(), Status::MissingSource { version, .. } if version == "1.7.4"));
        assert!(matches!(gf.reextract(&mut noop, &CancelToken::new()), Err(Error::MissingSource(_))));
        gf.clear().unwrap();
        gf.clear().unwrap();
        assert_eq!(gf.status(), Status::NotSet);
    }

    #[test]
    fn ready_without_source_and_unsupported_manifest() {
        let (_d, apk_path, gf) = setup();
        gf.extract(&apk_path, &mut noop, &CancelToken::new()).unwrap();
        fs::remove_file(&apk_path).unwrap();
        assert!(matches!(gf.status(), Status::Ready { source_present: false, .. }));
        let (dir, mut m) = gf.current().unwrap().unwrap();
        m.version_name = "1.8.0".into();
        fs::write(dir.join(MANIFEST), serde_json::to_vec(&m).unwrap()).unwrap();
        assert_eq!(gf.status(), Status::Unsupported { version: "1.8.0".into() });
    }

    #[test]
    fn cancel_leaves_nothing_behind() {
        let (_d, apk_path, gf) = setup();
        let c = CancelToken::new();
        let c2 = c.clone();
        let r = gf.extract(
            &apk_path,
            &mut move |p: &Progress| {
                if p.files_done == 2 {
                    c2.cancel()
                }
            },
            &c,
        );
        assert!(matches!(r, Err(Error::Cancelled)));
        assert!(c.is_cancelled());
        assert_eq!(fs::read_dir(gf.root()).unwrap().count(), 0);
        assert_eq!(gf.status(), Status::NotSet);
    }

    #[test]
    fn invalid_sources_and_pointers() {
        let (d, _apk, gf) = setup();
        let bad = write(d.path(), "x.apk", b"nope");
        assert!(matches!(gf.extract(&bad, &mut noop, &CancelToken::new()), Err(Error::NotAnApk)));
        fs::create_dir_all(gf.root()).unwrap();
        for ptr in ["", ".tmp-x", "a/b", "a\\b"] {
            fs::write(gf.root().join(CURRENT), ptr).unwrap();
            assert!(matches!(gf.status(), Status::Damaged { source: None, .. }), "{ptr:?}");
            assert!(gf.file("x").is_err());
        }
        fs::write(gf.root().join(CURRENT), "1.7.6-abc").unwrap();
        assert!(matches!(gf.current(), Err(Error::Io(_))));
        fs::create_dir_all(gf.root().join("1.7.6-abc")).unwrap();
        fs::write(gf.root().join("1.7.6-abc").join(MANIFEST), b"{").unwrap();
        assert!(matches!(gf.current(), Err(Error::Json(_))));
        // `current` that is a directory: read error other than NotFound.
        fs::remove_file(gf.root().join(CURRENT)).unwrap();
        fs::create_dir(gf.root().join(CURRENT)).unwrap();
        assert!(matches!(gf.current(), Err(Error::Io(_))));
        // The cache root is a file: extraction and clear fail with I/O errors.
        let blocked = GameFiles::new(&bad);
        assert!(matches!(blocked.clear(), Err(Error::Io(_))));
        let apk_path = d.path().join("game.apk");
        assert!(matches!(blocked.extract(&apk_path, &mut noop, &CancelToken::new()), Err(Error::Io(_))));
    }

    #[test]
    fn truncated_entry_is_damaged_and_cleaned() {
        // A stored entry whose recorded size is larger than its data cannot be produced by the zip writer, so
        // corrupt the CRC of a stored game file instead: reading it fails at the end.
        let d = tempfile::tempdir().unwrap();
        let mut e = vec![("AndroidManifest.xml", crate::axml::build::manifest(PACKAGE, "1.7.6", 1, None))];
        e.extend(game_entries());
        let mut bytes = zip(&e, true);
        let pos = bytes.windows(4).rposition(|w| w == [0x55; 4]).unwrap();
        bytes[pos] = 0x56;
        let p = write(d.path(), "c.apk", &bytes);
        let gf = GameFiles::new(&d.path().join("cache"));
        assert!(matches!(gf.extract(&p, &mut noop, &CancelToken::new()), Err(Error::Damaged(_))));
        assert_eq!(fs::read_dir(gf.root()).unwrap().count(), 0);
    }

    #[test]
    fn helpers() {
        assert_eq!(png_size(b"short"), (None, None));
        assert_eq!(png_size(&png(3, 4, 0)), (Some(3), Some(4)));
        let mut s = Sink { inner: Vec::new(), hash: Sha256::new(), head: Vec::new() };
        s.write_all(&[1; 30]).unwrap();
        s.flush().unwrap();
        assert_eq!(s.head.len(), 24);
        let st = Status::Ready {
            version: "1.7.6".into(),
            version_code: Some(1),
            files: 1,
            bytes: 2,
            source: "a".into(),
            source_present: true,
        };
        assert!(serde_json::to_string(&st).unwrap().contains("\"state\":\"ready\""));
        assert!(Error::MissingSource("/x".into()).to_string().contains("/x"));
        assert!(Error::Incomplete(vec!["a".into(), "b".into()]).to_string().ends_with("a, b"));
    }
}
