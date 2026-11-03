//! Opening and validating the user's APK: a single `.apk`, an `.apks`/`.xapk` bundle (a zip of APKs) or a folder
//! holding base + split APKs.

use crate::axml::{parse_manifest, ManifestInfo};
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use zip::{CompressionMethod, ZipArchive};

/// Android package name of The Blockheads.
pub const PACKAGE: &str = "com.noodlecake.blockheads";
/// `versionName`s this tool supports.
pub const SUPPORTED_VERSIONS: [&str; 3] = ["1.7.4", "1.7.5", "1.7.6"];
/// Where the game keeps its resources inside the APK.
pub const ASSET_PREFIX: &str = "assets/GameResources/";
/// Game files (relative to [`ASSET_PREFIX`]) without which the editor cannot draw a world.
pub const REQUIRED_FILES: [&str; 4] =
    ["HDTex/TileMap.png", "HDTex/Items.png", "HDTex/TileDestruct.png", "HDTex/TileReflect.png"];

const MANIFEST: &str = "AndroidManifest.xml";

/// Returns the cache-relative path for an APK entry the editor needs, or `None` for anything else (audio, video,
/// shaders, code, HTML help, unsafe names). Kept: every `.png` and bitmap-font `.fnt` under `assets/GameResources/`
/// except the `instructions/` help pages.
pub fn game_file_rel(entry: &str) -> Option<&str> {
    let rel = entry.strip_prefix(ASSET_PREFIX)?;
    if rel.starts_with("instructions/") || rel.contains('\\') {
        return None;
    }
    if rel.split('/').any(|s| s.is_empty() || s == "." || s == "..") {
        return None;
    }
    let ext = rel.rsplit_once('.')?.1.to_ascii_lowercase();
    matches!(ext.as_str(), "png" | "fnt").then_some(rel)
}

/// What the user picked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceKind {
    /// A single `.apk` file.
    Apk,
    /// An `.apks` / `.xapk` zip of APKs.
    Bundle,
    /// A folder with base + split APKs.
    Folder,
}

/// A validated APK (set).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApkInfo {
    /// The path the user picked.
    pub source: PathBuf,
    /// Single APK, bundle or folder.
    pub kind: SourceKind,
    /// Package name (always [`PACKAGE`]).
    pub package: String,
    /// `versionName` (one of [`SUPPORTED_VERSIONS`]).
    pub version_name: String,
    /// `versionCode`, if present.
    pub version_code: Option<u32>,
    /// Lowercase hex SHA-256 of the picked file (for a folder: of the base APK).
    pub sha256: String,
    /// Names of the APKs in the set (the single APK's file name for [`SourceKind::Apk`]).
    pub parts: Vec<String>,
    /// Number of game files that would be extracted.
    pub files: usize,
    /// Their total uncompressed size.
    pub bytes: u64,
}

trait ReadSeek: Read + Seek + Send {}
impl<T: Read + Seek + Send> ReadSeek for T {}

/// A byte range of a file (a stored APK inside a bundle), read without copying.
struct Window {
    file: File,
    start: u64,
    len: u64,
    pos: u64,
}

impl Read for Window {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let left = self.len.saturating_sub(self.pos);
        let n = (buf.len() as u64).min(left) as usize;
        if n == 0 {
            return Ok(0);
        }
        self.file.seek(SeekFrom::Start(self.start + self.pos))?;
        let got = self.file.read(&mut buf[..n])?;
        self.pos += got as u64;
        Ok(got)
    }
}

impl Seek for Window {
    fn seek(&mut self, to: SeekFrom) -> io::Result<u64> {
        let pos = match to {
            SeekFrom::Start(p) => p as i128,
            SeekFrom::End(d) => self.len as i128 + d as i128,
            SeekFrom::Current(d) => self.pos as i128 + d as i128,
        };
        if pos < 0 {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "seek before start"));
        }
        self.pos = pos as u64;
        Ok(self.pos)
    }
}

struct Part {
    label: String,
    zip: ZipArchive<Box<dyn ReadSeek>>,
}

/// An opened, validated APK set from which game files can be read.
pub struct ApkSet {
    info: ApkInfo,
    parts: Vec<Part>,
    /// cache-relative path → (part, entry index, size)
    assets: BTreeMap<String, (usize, usize, u64)>,
}

impl std::fmt::Debug for ApkSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ApkSet").field("info", &self.info).finish_non_exhaustive()
    }
}

fn damaged(what: &str, e: impl std::fmt::Display) -> Error {
    Error::Damaged(format!("{what}: {e}"))
}

/// Opens a top-level zip; a file that does not even look like a zip is [`Error::NotAnApk`].
fn open_zip(path: &Path) -> Result<ZipArchive<Box<dyn ReadSeek>>> {
    let mut file = File::open(path)?;
    let mut magic = [0u8; 2];
    let looks_zip = file.read_exact(&mut magic).is_ok() && &magic == b"PK";
    file.rewind()?;
    match ZipArchive::new(Box::new(file) as Box<dyn ReadSeek>) {
        Ok(z) => Ok(z),
        Err(e) if looks_zip => Err(damaged(&file_name(path), e)),
        Err(_) => Err(Error::NotAnApk),
    }
}

fn file_name(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.display().to_string())
}

fn is_apk_name(name: &str) -> bool {
    name.to_ascii_lowercase().ends_with(".apk")
}

/// Opens an APK stored inside a bundle: stored entries are read in place, compressed ones are inflated into an
/// anonymous temporary file.
fn open_nested(outer: &mut ZipArchive<Box<dyn ReadSeek>>, path: &Path, index: usize) -> Result<Part> {
    let mut entry = outer.by_index(index).map_err(|e| damaged(&file_name(path), e))?;
    let label = entry.name().to_owned();
    let reader: Box<dyn ReadSeek> = if entry.compression() == CompressionMethod::Stored {
        Box::new(Window { file: File::open(path)?, start: entry.data_start(), len: entry.size(), pos: 0 })
    } else {
        let mut tmp = tempfile::tempfile()?;
        copy(&mut entry, &mut tmp, &label)?;
        tmp.rewind()?;
        Box::new(tmp)
    };
    drop(entry);
    let zip = ZipArchive::new(reader).map_err(|e| damaged(&label, e))?;
    Ok(Part { label, zip })
}

/// Copies `r` into `w`; read failures (corrupt data, CRC mismatch) are [`Error::Damaged`], write failures I/O.
pub(crate) fn copy(r: &mut dyn Read, w: &mut dyn io::Write, what: &str) -> Result<u64> {
    let mut buf = vec![0u8; 64 * 1024];
    let mut total = 0u64;
    loop {
        let n = match r.read(&mut buf) {
            Ok(0) => return Ok(total),
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(damaged(what, e)),
        };
        w.write_all(&buf[..n])?;
        total += n as u64;
    }
}

fn sha256_file(path: &Path) -> Result<String> {
    let mut h = Sha256::new();
    copy(&mut File::open(path)?, &mut h, &file_name(path))?;
    Ok(hex(&h.finalize()))
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn read_manifest(part: &mut Part) -> Result<ManifestInfo> {
    let mut entry = part.zip.by_name(MANIFEST).map_err(|e| damaged(&format!("{}: {MANIFEST}", part.label), e))?;
    let mut bytes = Vec::new();
    copy(&mut entry, &mut bytes, &part.label)?;
    parse_manifest(&bytes).map_err(|e| damaged(&part.label, e))
}

impl ApkSet {
    /// Opens and validates `path` (an `.apk`, `.apks`/`.xapk` or a folder of APKs).
    pub fn open(path: &Path) -> Result<Self> {
        let (kind, mut parts, mut part_paths) = if fs::metadata(path)?.is_dir() {
            let mut files: Vec<PathBuf> = fs::read_dir(path)?
                .map(|e| e.map(|e| e.path()))
                .collect::<io::Result<Vec<_>>>()?
                .into_iter()
                .filter(|p| p.is_file() && is_apk_name(&file_name(p)))
                .collect();
            files.sort();
            let mut parts = Vec::new();
            for f in &files {
                let zip = open_zip(f).map_err(|e| match e {
                    Error::NotAnApk => damaged(&file_name(f), "not a zip file"),
                    e => e,
                })?;
                parts.push(Part { label: file_name(f), zip });
            }
            (SourceKind::Folder, parts, files)
        } else {
            let mut zip = open_zip(path)?;
            if zip.index_for_name(MANIFEST).is_some() {
                (SourceKind::Apk, vec![Part { label: file_name(path), zip }], vec![path.to_owned()])
            } else {
                let mut idx: Vec<(String, usize)> =
                    (0..zip.len()).filter_map(|i| Some((zip.name_for_index(i)?.to_owned(), i))).collect();
                idx.retain(|(n, _)| is_apk_name(n));
                idx.sort();
                let mut parts = Vec::new();
                for (_, i) in idx {
                    parts.push(open_nested(&mut zip, path, i)?);
                }
                let n = parts.len();
                (SourceKind::Bundle, parts, vec![path.to_owned(); n])
            }
        };
        if parts.is_empty() {
            return Err(Error::NotAnApk);
        }
        let mut base = None;
        for (i, part) in parts.iter_mut().enumerate() {
            let m = read_manifest(part)?;
            if m.split.is_none() && base.is_none() {
                base = Some((i, m));
            }
        }
        let Some((base_idx, m)) = base else { return Err(Error::Incomplete(vec!["base APK".into()])) };
        if m.package != PACKAGE {
            return Err(Error::WrongApp(m.package));
        }
        let Some(version_name) = m.version_name else { return Err(damaged(&parts[base_idx].label, "no versionName")) };
        if !SUPPORTED_VERSIONS.contains(&version_name.as_str()) {
            return Err(Error::Unsupported(version_name));
        }
        let mut assets = BTreeMap::new();
        for (pi, part) in parts.iter_mut().enumerate() {
            for i in 0..part.zip.len() {
                let entry = part.zip.by_index_raw(i).map_err(|e| damaged(&part.label, e))?;
                if let Some(rel) = game_file_rel(entry.name()) {
                    assets.entry(rel.to_owned()).or_insert((pi, i, entry.size()));
                }
            }
        }
        let missing: Vec<String> =
            REQUIRED_FILES.iter().filter(|f| !assets.contains_key(**f)).map(|f| format!("{ASSET_PREFIX}{f}")).collect();
        if !missing.is_empty() {
            return Err(Error::Incomplete(missing));
        }
        let hash_path = part_paths.swap_remove(base_idx);
        let info = ApkInfo {
            source: path.to_owned(),
            kind,
            package: m.package,
            version_name,
            version_code: m.version_code,
            sha256: sha256_file(&hash_path)?,
            parts: parts.iter().map(|p| p.label.clone()).collect(),
            files: assets.len(),
            bytes: assets.values().map(|a| a.2).sum(),
        };
        Ok(Self { info, parts, assets })
    }

    /// What was validated.
    pub fn info(&self) -> &ApkInfo {
        &self.info
    }

    /// The game files to extract: `(cache-relative path, size)`, sorted by path.
    pub fn files(&self) -> Vec<(String, u64)> {
        self.assets.iter().map(|(k, v)| (k.clone(), v.2)).collect()
    }

    /// Copies one game file (a path from [`Self::files`]) into `w`; returns the number of bytes.
    pub fn copy_file(&mut self, rel: &str, w: &mut dyn io::Write) -> Result<u64> {
        let Some(&(pi, i, _)) = self.assets.get(rel) else {
            return Err(Error::Io(io::Error::new(io::ErrorKind::NotFound, rel.to_owned())));
        };
        let part = &mut self.parts[pi];
        let what = format!("{}: {ASSET_PREFIX}{rel}", part.label);
        let mut entry = part.zip.by_index(i).map_err(|e| damaged(&what, e))?;
        copy(&mut entry, w, &what)
    }
}

/// Validates `path` without extracting anything.
pub fn inspect(path: &Path) -> Result<ApkInfo> {
    Ok(ApkSet::open(path)?.info)
}

#[cfg(test)]
pub(crate) mod testutil {
    //! Synthesized APKs for tests.

    use crate::axml::build;
    use std::io::{Cursor, Write};
    use std::path::Path;
    use zip::write::SimpleFileOptions;
    use zip::{CompressionMethod, ZipWriter};

    /// A zip with `(name, bytes)` entries, deflated or stored.
    pub fn zip(entries: &[(&str, Vec<u8>)], stored: bool) -> Vec<u8> {
        let method = if stored { CompressionMethod::Stored } else { CompressionMethod::Deflated };
        let opts = SimpleFileOptions::default().compression_method(method);
        let mut w = ZipWriter::new(Cursor::new(Vec::new()));
        for (n, b) in entries {
            w.start_file(*n, opts).unwrap();
            w.write_all(b).unwrap();
        }
        w.finish().unwrap().into_inner()
    }

    /// A minimal PNG header (signature + IHDR) followed by `extra` bytes.
    pub fn png(w: u32, h: u32, extra: usize) -> Vec<u8> {
        let mut v = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
        v.extend_from_slice(&w.to_be_bytes());
        v.extend_from_slice(&h.to_be_bytes());
        v.extend_from_slice(&[8, 6, 0, 0, 0]);
        v.resize(v.len() + extra, 0x55);
        v
    }

    /// Game-file entries: the required atlases plus a few others and some files that must be skipped.
    pub fn game_entries() -> Vec<(&'static str, Vec<u8>)> {
        vec![
            ("assets/GameResources/HDTex/TileMap.png", png(2048, 2048, 100)),
            ("assets/GameResources/HDTex/Items.png", png(2048, 1024, 50)),
            ("assets/GameResources/HDTex/TileDestruct.png", png(2048, 2048, 10)),
            ("assets/GameResources/HDTex/TileReflect.png", png(2048, 2048, 10)),
            ("assets/GameResources/HDTex/skins/male_face_0.png", png(256, 128, 0)),
            ("assets/GameResources/TileMap.png", png(512, 512, 5)),
            ("assets/GameResources/Fonts/Blockheads_64.fnt", b"info face=x".to_vec()),
            ("assets/GameResources/blockheadsSong.wav", vec![1; 20]),
            ("assets/GameResources/largo.mp4", vec![2; 20]),
            ("assets/GameResources/instructions/icon200.png", png(200, 200, 0)),
            ("assets/GameResources/Shaders/tile.fsh", b"void main(){}".to_vec()),
            ("classes.dex", vec![3; 20]),
        ]
    }

    /// A base APK of `package`/`version`.
    pub fn apk(package: &'static str, version: &'static str, with_assets: bool) -> Vec<u8> {
        let mut e = vec![("AndroidManifest.xml", build::manifest(package, version, 1_548_230_154, None))];
        if with_assets {
            e.extend(game_entries());
        }
        zip(&e, false)
    }

    /// A split APK (no game files).
    pub fn split(name: &'static str) -> Vec<u8> {
        zip(
            &[
                ("AndroidManifest.xml", build::manifest(super::PACKAGE, "1.7.6", 1, Some(name))),
                ("lib/armeabi-v7a/libx.so", vec![0; 8]),
            ],
            true,
        )
    }

    /// Rewrites the compression method of entry `name` (local + central header) to `method`.
    pub fn patch_method(zip: &mut [u8], name: &str, method: u16) {
        patch(zip, name, b"PK\x03\x04", 26, 30, 8, &method.to_le_bytes());
        patch(zip, name, b"PK\x01\x02", 28, 46, 10, &method.to_le_bytes());
    }

    /// Breaks the local header signature of entry `name`.
    pub fn break_local_header(zip: &mut [u8], name: &str) {
        patch(zip, name, b"PK\x03\x04", 26, 30, 0, b"XX");
    }

    fn patch(zip: &mut [u8], name: &str, sig: &[u8], len_at: usize, name_at: usize, at: usize, val: &[u8]) {
        let pos = (0..zip.len() - name_at)
            .find(|&i| {
                let n = u16::from_le_bytes([zip[i + len_at], zip[i + len_at + 1]]) as usize;
                zip[i..].starts_with(sig) && zip.get(i + name_at..i + name_at + n) == Some(name.as_bytes())
            })
            .unwrap();
        zip[pos + at..pos + at + val.len()].copy_from_slice(val);
    }

    pub fn write(dir: &Path, name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, bytes).unwrap();
        p
    }
}

#[cfg(test)]
mod tests {
    use super::testutil::*;
    use super::*;
    use crate::axml::build;

    #[test]
    fn selection_rule() {
        assert_eq!(game_file_rel("assets/GameResources/HDTex/TileMap.png"), Some("HDTex/TileMap.png"));
        assert_eq!(game_file_rel("assets/GameResources/Fonts/a.FNT"), Some("Fonts/a.FNT"));
        for skip in [
            "assets/GameResources/a.wav",
            "assets/GameResources/a.mp4",
            "assets/GameResources/instructions/x.png",
            "assets/GameResources/../x.png",
            "assets/GameResources/a//x.png",
            "assets/GameResources/./x.png",
            "assets/GameResources/a\\x.png",
            "assets/GameResources/noext",
            "assets/Default.png",
            "res/drawable/icon.png",
        ] {
            assert_eq!(game_file_rel(skip), None, "{skip}");
        }
    }

    #[test]
    fn single_apk() {
        let d = tempfile::tempdir().unwrap();
        let bytes = apk(PACKAGE, "1.7.6", true);
        let p = write(d.path(), "game.apk", &bytes);
        let mut set = ApkSet::open(&p).unwrap();
        let info = set.info().clone();
        assert_eq!(info.kind, SourceKind::Apk);
        assert_eq!(info.version_name, "1.7.6");
        assert_eq!(info.version_code, Some(1_548_230_154));
        assert_eq!(info.sha256, hex(&Sha256::digest(&bytes)));
        assert_eq!(info.parts, vec!["game.apk"]);
        assert_eq!(info.files, 7);
        let files = set.files();
        assert_eq!(files.len(), 7);
        assert_eq!(info.bytes, files.iter().map(|f| f.1).sum::<u64>());
        assert!(files.iter().any(|f| f.0 == "Fonts/Blockheads_64.fnt"));
        let mut out = Vec::new();
        assert_eq!(set.copy_file("TileMap.png", &mut out).unwrap(), 34);
        assert_eq!(out, png(512, 512, 5));
        assert!(matches!(set.copy_file("nope.png", &mut out), Err(Error::Io(_))));
        assert!(format!("{set:?}").contains("ApkSet"));
        assert_eq!(inspect(&p).unwrap(), info);
    }

    #[test]
    fn rejections() {
        let d = tempfile::tempdir().unwrap();
        let p = write(d.path(), "a.apk", &apk("com.example", "1.7.6", true));
        assert!(matches!(inspect(&p), Err(Error::WrongApp(p)) if p == "com.example"));
        let p = write(d.path(), "b.apk", &apk(PACKAGE, "1.7.3", true));
        assert!(matches!(inspect(&p), Err(Error::Unsupported(v)) if v == "1.7.3"));
        let p = write(d.path(), "c.apk", &apk(PACKAGE, "1.7.6", false));
        assert!(matches!(inspect(&p), Err(Error::Incomplete(m)) if m.len() == 4));
        let p = write(d.path(), "d.txt", b"hello world");
        assert!(matches!(inspect(&p), Err(Error::NotAnApk)));
        let p = write(d.path(), "e.apk", b"PK\x03\x04 truncated");
        assert!(matches!(inspect(&p), Err(Error::Damaged(_))));
        let p = write(d.path(), "empty.zip", &zip(&[("readme.txt", vec![1])], false));
        assert!(matches!(inspect(&p), Err(Error::NotAnApk)));
        let p = write(d.path(), "f.apk", &zip(&[("AndroidManifest.xml", vec![1, 2, 3])], false));
        assert!(matches!(inspect(&p), Err(Error::Damaged(m)) if m.contains("AndroidManifest")));
        // No versionName.
        let doc = build::document("manifest", &[("package", None, build::Val::Str(PACKAGE))], false);
        let p = write(d.path(), "g.apk", &zip(&[("AndroidManifest.xml", doc)], false));
        assert!(matches!(inspect(&p), Err(Error::Damaged(m)) if m.contains("versionName")));
        assert!(matches!(inspect(&d.path().join("missing.apk")), Err(Error::Io(_))));
    }

    #[test]
    fn corrupt_entry_data_is_damaged() {
        let d = tempfile::tempdir().unwrap();
        let mut bytes = apk(PACKAGE, "1.7.6", true);
        // Corrupt the deflated payload of the manifest (first local file entry, header 30 + name 19 bytes).
        for b in &mut bytes[49 + 4..49 + 40] {
            *b ^= 0xa5;
        }
        let p = write(d.path(), "x.apk", &bytes);
        assert!(matches!(inspect(&p), Err(Error::Damaged(_))));
    }

    #[test]
    fn bundles_stored_and_deflated() {
        let d = tempfile::tempdir().unwrap();
        for stored in [true, false] {
            let b = zip(
                &[
                    ("manifest.json", b"{}".to_vec()),
                    ("config.armeabi_v7a.apk", split("config.armeabi_v7a")),
                    ("com.noodlecake.blockheads.apk", apk(PACKAGE, "1.7.5", true)),
                ],
                stored,
            );
            let p = write(d.path(), "game.xapk", &b);
            let mut set = ApkSet::open(&p).unwrap();
            assert_eq!(set.info().kind, SourceKind::Bundle);
            assert_eq!(set.info().version_name, "1.7.5");
            assert_eq!(set.info().parts.len(), 2);
            assert_eq!(set.info().sha256, hex(&Sha256::digest(&b)));
            let mut out = Vec::new();
            set.copy_file("HDTex/Items.png", &mut out).unwrap();
            assert_eq!(out, png(2048, 1024, 50));
        }
        // Only splits: incomplete.
        let p = write(d.path(), "s.apks", &zip(&[("splits/config.en.apk", split("config.en"))], true));
        assert!(matches!(inspect(&p), Err(Error::Incomplete(m)) if m == ["base APK"]));
        // Base without assets + asset split providing them.
        let assets_split = {
            let mut e = vec![("AndroidManifest.xml", build::manifest(PACKAGE, "1.7.4", 1, Some("assets")))];
            e.extend(game_entries());
            zip(&e, false)
        };
        let p = write(
            d.path(),
            "t.apks",
            &zip(&[("splits/base-master.apk", apk(PACKAGE, "1.7.4", false)), ("splits/z.apk", assets_split)], true),
        );
        assert_eq!(inspect(&p).unwrap().files, 7);
        // A nested APK without a manifest.
        let p = write(d.path(), "v.apks", &zip(&[("base.apk", zip(&[("a", vec![1])], true))], true));
        assert!(matches!(inspect(&p), Err(Error::Damaged(m)) if m.contains("AndroidManifest.xml")));
        // A nested entry that is not a zip.
        let p = write(d.path(), "u.apks", &zip(&[("base.apk", b"garbage".to_vec())], true));
        assert!(matches!(inspect(&p), Err(Error::Damaged(m)) if m.contains("base.apk")));
    }

    #[test]
    fn folder_of_apks() {
        let d = tempfile::tempdir().unwrap();
        let base = apk(PACKAGE, "1.7.6", true);
        write(d.path(), "base.apk", &base);
        write(d.path(), "split_config.armeabi_v7a.apk", &split("config.armeabi_v7a"));
        write(d.path(), "notes.txt", b"x");
        std::fs::create_dir(d.path().join("sub.apk")).unwrap();
        let info = inspect(d.path()).unwrap();
        assert_eq!(info.kind, SourceKind::Folder);
        assert_eq!(info.parts, vec!["base.apk", "split_config.armeabi_v7a.apk"]);
        assert_eq!(info.sha256, hex(&Sha256::digest(&base)));
        // A non-zip .apk in the folder is damaged, an empty folder is not an APK.
        write(d.path(), "zz.apk", b"nope");
        assert!(matches!(inspect(d.path()), Err(Error::Damaged(m)) if m.contains("zz.apk")));
        let e = tempfile::tempdir().unwrap();
        assert!(matches!(inspect(e.path()), Err(Error::NotAnApk)));
        // A truncated zip in the folder.
        write(e.path(), "a.apk", b"PK\x03\x04");
        assert!(matches!(inspect(e.path()), Err(Error::Damaged(_))));
    }

    #[test]
    fn unreadable_entries_are_damaged() {
        let d = tempfile::tempdir().unwrap();
        let mut e = vec![("AndroidManifest.xml", build::manifest(PACKAGE, "1.7.6", 1, None))];
        e.extend(game_entries());
        let ok = zip(&e, true);
        // Unsupported compression method (bzip2) on a game file: validation passes, reading it fails.
        let mut b = ok.clone();
        patch_method(&mut b, "assets/GameResources/TileMap.png", 12);
        let mut set = ApkSet::open(&write(d.path(), "a.apk", &b)).unwrap();
        assert!(
            matches!(set.copy_file("TileMap.png", &mut Vec::new()), Err(Error::Damaged(m)) if m.contains("TileMap"))
        );
        // Broken local header of a game file.
        let mut b = ok.clone();
        break_local_header(&mut b, "assets/GameResources/TileMap.png");
        assert!(matches!(inspect(&write(d.path(), "b.apk", &b)), Err(Error::Damaged(_))));
        // Bundle whose nested APK uses an unsupported method.
        let mut b = zip(&[("base.apk", ok)], true);
        patch_method(&mut b, "base.apk", 12);
        assert!(matches!(inspect(&write(d.path(), "c.apks", &b)), Err(Error::Damaged(m)) if m.contains("c.apks")));
    }

    #[test]
    fn window_reader_seeks() {
        let d = tempfile::tempdir().unwrap();
        let p = write(d.path(), "w", b"0123456789");
        let mut w = Window { file: File::open(&p).unwrap(), start: 2, len: 5, pos: 0 };
        let mut s = String::new();
        w.read_to_string(&mut s).unwrap();
        assert_eq!(s, "23456");
        assert_eq!(w.seek(SeekFrom::End(-2)).unwrap(), 3);
        assert_eq!(w.seek(SeekFrom::Current(1)).unwrap(), 4);
        assert!(w.seek(SeekFrom::Current(-9)).is_err());
        assert_eq!(w.seek(SeekFrom::Start(0)).unwrap(), 0);
        assert_eq!(file_name(Path::new("/")), "/");
    }

    #[test]
    fn copy_errors() {
        struct Flaky(u8);
        impl Read for Flaky {
            fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
                self.0 += 1;
                match self.0 {
                    1 => Err(io::ErrorKind::Interrupted.into()),
                    2 => {
                        buf[0] = 7;
                        Ok(1)
                    }
                    _ => Err(io::Error::other("crc")),
                }
            }
        }
        let mut out = Vec::new();
        assert!(matches!(copy(&mut Flaky(0), &mut out, "x"), Err(Error::Damaged(m)) if m == "x: crc"));
        assert_eq!(out, [7]);
        struct Full;
        impl io::Write for Full {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                Err(io::Error::other("disk full"))
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        assert!(matches!(copy(&mut &b"abc"[..], &mut Full, "x"), Err(Error::Io(_))));
        let mut f = Full;
        io::Write::flush(&mut f).unwrap();
    }
}
