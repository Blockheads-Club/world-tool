//! The `.bhworld` exchange format (specification: `docs/bhworld-format.md`).
//!
//! A zip archive with `manifest.json` (first entry), an optional `preview.png` and the world's files below `world/`.
//! Opening validates everything before any data is used: entry paths (no traversal), the manifest, that the `world/`
//! entries and the manifest's file list match exactly, sizes, SHA-256 checksums and size/count/ratio [`Limits`]
//! (zip bomb protection; sizes are enforced on the decompressed stream, not only trusted from headers).

use std::collections::BTreeSet;
use std::fs;
use std::io::{self, Read, Seek, Write};
use std::path::Path;

use bhw_backup::saves::is_world_id;
use bhw_backup::{is_safe_rel_path, Payload};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::files::{WorldFile, WorldFiles};
use crate::time::now_rfc3339;
use crate::{Error, Result};

/// Value of `manifest.format`.
pub const FORMAT: &str = "bhworld";
/// Newest format version written and read.
pub const FORMAT_VERSION: u32 = 1;
/// Manifest entry name.
pub const MANIFEST: &str = "manifest.json";
/// Preview entry name.
pub const PREVIEW: &str = "preview.png";
/// Directory of the world files inside the zip.
pub const WORLD_DIR: &str = "world/";

/// Where a world came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SourceKind {
    /// `adb backup` of an Android device.
    AndroidBackup,
    /// Android device read with root.
    AndroidRoot,
    /// Blockheads PC saves directory.
    BlockheadsPc,
    /// Blockheads Server world folder.
    BlockheadsServer,
    /// iPhone/iPad backup.
    IosBackup,
    /// Another `.bhworld` file.
    Bhworld,
    /// Unknown (also used for kinds added by newer versions).
    #[serde(other)]
    Unknown,
}

/// `manifest.source`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    /// Source kind.
    pub kind: SourceKind,
    /// Device name (phone model, computer name), if known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_name: Option<String>,
}

/// `manifest.world`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestWorld {
    /// 32 lowercase hex world id.
    pub id: String,
    /// Display name, if known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Game version that last saved the world, if known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game_version: Option<String>,
}

/// One entry of `manifest.files` (and `manifest.preview`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileEntry {
    /// Path relative to `world/` (for the preview: `preview.png`).
    pub path: String,
    /// Size in bytes.
    pub size: u64,
    /// SHA-256 of the contents, lowercase hex.
    pub sha256: String,
}

/// `manifest.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    /// Always `"bhworld"`.
    pub format: String,
    /// Format version (1).
    pub format_version: u32,
    /// World identity.
    pub world: ManifestWorld,
    /// Origin.
    pub source: Source,
    /// World creation time (RFC 3339), if known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    /// Export time (RFC 3339).
    pub exported_at: String,
    /// Writer, e.g. `bhw-world 0.1.0`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generator: Option<String>,
    /// World files, sorted by path.
    pub files: Vec<FileEntry>,
    /// Preview image, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview: Option<FileEntry>,
}

/// What the caller knows about a world being exported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportInfo {
    /// Display name.
    pub name: Option<String>,
    /// Game version.
    pub game_version: Option<String>,
    /// Origin.
    pub source: Source,
    /// Creation time (RFC 3339).
    pub created_at: Option<String>,
}

/// Limits applied when opening a `.bhworld`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Max total uncompressed size of all world files.
    pub max_total: u64,
    /// Max size of one file.
    pub max_file: u64,
    /// Max number of world files.
    pub max_files: usize,
    /// Max uncompressed/compressed ratio for entries larger than [`Limits::ratio_floor`].
    pub max_ratio: u64,
    /// Entries up to this size are exempt from the ratio check.
    pub ratio_floor: u64,
    /// Max size of `manifest.json`.
    pub max_manifest: u64,
    /// Max size of `preview.png`.
    pub max_preview: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_total: 4 << 30,
            max_file: 4 << 30,
            max_files: 100_000,
            max_ratio: 1000,
            ratio_floor: 1 << 20,
            max_manifest: 16 << 20,
            max_preview: 8 << 20,
        }
    }
}

/// An opened `.bhworld` (all data in memory, validated).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BhWorld {
    /// The manifest.
    pub manifest: Manifest,
    /// Preview PNG bytes.
    pub preview: Option<Vec<u8>>,
    /// The world, files as [`Payload::Bytes`].
    pub world: WorldFiles,
}

/// `Read + Seek` trait object helper.
pub trait ReadSeek: Read + Seek {}
impl<T: Read + Seek> ReadSeek for T {}
/// `Write + Seek` trait object helper.
pub trait WriteSeek: Write + Seek {}
impl<T: Write + Seek> WriteSeek for T {}

fn sha256_hex(data: &[u8]) -> String {
    bhw_backup::hex::encode_lower(&Sha256::digest(data))
}

fn hash_payload(p: &Payload) -> Result<(u64, String)> {
    let mut r = p.open()?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    let mut size = 0u64;
    loop {
        let n = r.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
        size += n as u64;
    }
    Ok((size, bhw_backup::hex::encode_lower(&h.finalize())))
}

impl Manifest {
    /// A manifest for `world` (file list filled in by [`write_with_manifest`]).
    pub fn new(world_id: &str, info: &ExportInfo) -> Self {
        Self {
            format: FORMAT.into(),
            format_version: FORMAT_VERSION,
            world: ManifestWorld {
                id: world_id.into(),
                name: info.name.clone(),
                game_version: info.game_version.clone(),
            },
            source: info.source.clone(),
            created_at: info.created_at.clone(),
            exported_at: now_rfc3339(),
            generator: Some(format!("bhw-world {}", env!("CARGO_PKG_VERSION"))),
            files: Vec::new(),
            preview: None,
        }
    }
}

fn options_for(path: &str, size: u64) -> SimpleFileOptions {
    // PNGs are already compressed; LMDB files compress very well.
    let method = if path.ends_with(".png") { CompressionMethod::Stored } else { CompressionMethod::Deflated };
    SimpleFileOptions::default().compression_method(method).large_file(size >= u64::from(u32::MAX))
}

/// Writes `world` as a `.bhworld`, using `manifest` as template: its `files`/`preview` lists are recomputed and its
/// world id set to `world.id`. Returns the manifest written.
pub fn write_with_manifest(
    out: &mut dyn WriteSeek,
    world: &WorldFiles,
    mut manifest: Manifest,
    preview: Option<&[u8]>,
) -> Result<Manifest> {
    manifest.world.id = world.id.clone();
    manifest.files.clear();
    for f in &world.files {
        let (size, sha256) = hash_payload(&f.data)?;
        manifest.files.push(FileEntry { path: f.path.clone(), size, sha256 });
    }
    manifest.preview = preview.map(|p| FileEntry { path: PREVIEW.into(), size: p.len() as u64, sha256: sha256_hex(p) });
    let mut zip = ZipWriter::new(out);
    zip.start_file(MANIFEST, options_for(MANIFEST, 0))?;
    zip.write_all(&serde_json::to_vec_pretty(&manifest)?)?;
    if let Some(p) = preview {
        zip.start_file(PREVIEW, options_for(PREVIEW, 0))?;
        zip.write_all(p)?;
    }
    for (f, e) in world.files.iter().zip(&manifest.files) {
        zip.start_file(format!("{WORLD_DIR}{}", f.path), options_for(&f.path, e.size))?;
        let n = io::copy(&mut f.data.open()?, &mut zip)?;
        if n != e.size {
            return Err(Error::Format(format!("{} changed while exporting", f.path)));
        }
    }
    zip.finish()?;
    Ok(manifest)
}

/// Writes `world` as a new `.bhworld`.
pub fn write_bhworld(
    out: &mut dyn WriteSeek,
    world: &WorldFiles,
    info: &ExportInfo,
    preview: Option<&[u8]>,
) -> Result<Manifest> {
    write_with_manifest(out, world, Manifest::new(&world.id, info), preview)
}

/// Writes a `.bhworld` file atomically (temporary file next to `path`, then rename).
pub fn save_with_manifest(
    path: &Path,
    world: &WorldFiles,
    manifest: Manifest,
    preview: Option<&[u8]>,
) -> Result<Manifest> {
    let tmp = path.with_extension("bhworld.tmp");
    let r = (|| {
        let mut f = fs::File::create(&tmp)?;
        let m = write_with_manifest(&mut f, world, manifest, preview)?;
        f.sync_all()?;
        Ok(m)
    })();
    if r.is_err() {
        let _ = fs::remove_file(&tmp);
        return r;
    }
    fs::rename(&tmp, path)?;
    r
}

fn limit(what: String) -> Error {
    Error::LimitExceeded(what)
}

fn check_manifest(m: &Manifest, limits: &Limits) -> Result<()> {
    if m.format != FORMAT {
        return Err(Error::Format(format!("format is {:?}, expected {FORMAT:?}", m.format)));
    }
    if m.format_version == 0 {
        return Err(Error::Format("format version 0".into()));
    }
    if m.format_version > FORMAT_VERSION {
        return Err(Error::UnsupportedFormatVersion(m.format_version));
    }
    if !is_world_id(&m.world.id) {
        return Err(Error::InvalidId(m.world.id.clone()));
    }
    if m.files.len() > limits.max_files {
        return Err(limit(format!("{} files (max {})", m.files.len(), limits.max_files)));
    }
    let mut seen = BTreeSet::new();
    let mut total = 0u64;
    for f in &m.files {
        if !is_safe_rel_path(&f.path) || !seen.insert(f.path.as_str()) {
            return Err(Error::InvalidPath(f.path.clone()));
        }
        if f.sha256.len() != 64 || !f.sha256.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) {
            return Err(Error::Format(format!("bad sha256 for {}", f.path)));
        }
        if f.size > limits.max_file {
            return Err(limit(format!("{} is {} bytes", f.path, f.size)));
        }
        total = total.saturating_add(f.size);
    }
    if total > limits.max_total {
        return Err(limit(format!("world is {total} bytes (max {})", limits.max_total)));
    }
    if let Some(p) = &m.preview {
        if p.path != PREVIEW || p.size > limits.max_preview {
            return Err(Error::Format("bad preview entry".into()));
        }
    }
    Ok(())
}

/// Reads one entry fully, enforcing `size` (from the manifest) on the decompressed stream, the ratio limit and the
/// checksum.
fn read_entry(
    zip: &mut ZipArchive<&mut dyn ReadSeek>,
    name: &str,
    entry: &FileEntry,
    limits: &Limits,
) -> Result<Vec<u8>> {
    let f = zip.by_name(name)?;
    if f.size() != entry.size {
        return Err(Error::Format(format!("{name}: size {} differs from manifest {}", f.size(), entry.size)));
    }
    if entry.size > limits.ratio_floor && entry.size / f.compressed_size().max(1) > limits.max_ratio {
        return Err(limit(format!("{name}: compression ratio too high")));
    }
    let mut data = Vec::with_capacity(entry.size.min(64 << 20) as usize);
    f.take(entry.size + 1).read_to_end(&mut data)?;
    if data.len() as u64 != entry.size || sha256_hex(&data) != entry.sha256 {
        return Err(Error::ChecksumMismatch(name.to_string()));
    }
    Ok(data)
}

fn read_manifest_zip(zip: &mut ZipArchive<&mut dyn ReadSeek>, limits: &Limits) -> Result<Manifest> {
    let f = zip.by_name(MANIFEST).map_err(|_| Error::Format(format!("missing {MANIFEST}")))?;
    let mut raw = Vec::new();
    f.take(limits.max_manifest + 1).read_to_end(&mut raw)?;
    if raw.len() as u64 > limits.max_manifest {
        return Err(limit(format!("{MANIFEST} too large")));
    }
    let m: Manifest = serde_json::from_slice(&raw)?;
    check_manifest(&m, limits)?;
    Ok(m)
}

/// Reads and validates only the manifest (cheap; used for library listings).
pub fn read_manifest(input: &mut dyn ReadSeek, limits: &Limits) -> Result<Manifest> {
    let mut zip = ZipArchive::new(input)?;
    read_manifest_zip(&mut zip, limits)
}

/// Opens and fully validates a `.bhworld`.
pub fn read_bhworld(input: &mut dyn ReadSeek, limits: &Limits) -> Result<BhWorld> {
    let mut zip = ZipArchive::new(input)?;
    if zip.len() > limits.max_files + 2 {
        return Err(limit(format!("{} zip entries", zip.len())));
    }
    let manifest = read_manifest_zip(&mut zip, limits)?;
    let mut in_zip = BTreeSet::new();
    for name in zip.file_names() {
        let trimmed = name.strip_suffix('/').unwrap_or(name);
        if !is_safe_rel_path(trimmed) {
            return Err(Error::InvalidPath(name.to_string()));
        }
        if let Some(rel) = name.strip_prefix(WORLD_DIR).filter(|r| !r.is_empty() && !r.ends_with('/')) {
            in_zip.insert(rel.to_string());
        }
    }
    let listed: BTreeSet<String> = manifest.files.iter().map(|f| f.path.clone()).collect();
    if in_zip != listed {
        return Err(Error::Format("world/ entries do not match the manifest".into()));
    }
    let mut files = Vec::with_capacity(manifest.files.len());
    for e in &manifest.files {
        let data = read_entry(&mut zip, &format!("{WORLD_DIR}{}", e.path), e, limits)?;
        files.push(WorldFile { path: e.path.clone(), data: Payload::Bytes(data) });
    }
    let mut preview = None;
    if let Some(p) = &manifest.preview {
        preview = Some(read_entry(&mut zip, PREVIEW, p, limits)?);
    }
    let world = WorldFiles::new(manifest.world.id.clone(), files)?;
    Ok(BhWorld { manifest, preview, world })
}

/// [`read_bhworld`] on a file.
pub fn open_bhworld(path: &Path, limits: &Limits) -> Result<BhWorld> {
    read_bhworld(&mut io::BufReader::new(fs::File::open(path)?), limits)
}

/// [`read_manifest`] on a file.
pub fn open_manifest(path: &Path, limits: &Limits) -> Result<Manifest> {
    read_manifest(&mut io::BufReader::new(fs::File::open(path)?), limits)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::files::tests::{mem, sample, ID};
    use std::io::Cursor;

    pub(crate) fn info(name: &str) -> ExportInfo {
        ExportInfo {
            name: Some(name.into()),
            game_version: Some("1.7.6".into()),
            source: Source { kind: SourceKind::BlockheadsPc, device_name: Some("pc".into()) },
            created_at: None,
        }
    }

    fn pack(world: &WorldFiles, preview: Option<&[u8]>) -> Vec<u8> {
        let mut c = Cursor::new(Vec::new());
        write_bhworld(&mut c, world, &info("Home"), preview).unwrap();
        c.into_inner()
    }

    fn open(bytes: &[u8], limits: &Limits) -> Result<BhWorld> {
        read_bhworld(&mut Cursor::new(bytes.to_vec()), limits)
    }

    /// Builds a zip from raw `(name, data)` entries.
    fn raw_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut z = ZipWriter::new(Cursor::new(Vec::new()));
        for (n, d) in entries {
            if n.ends_with('/') {
                z.add_directory(*n, SimpleFileOptions::default()).unwrap();
            } else {
                z.start_file(*n, SimpleFileOptions::default()).unwrap();
                z.write_all(d).unwrap();
            }
        }
        z.finish().unwrap().into_inner()
    }

    fn manifest_json(m: &Manifest) -> Vec<u8> {
        serde_json::to_vec(m).unwrap()
    }

    #[test]
    fn round_trip_with_preview() {
        let w = sample(ID);
        let bytes = pack(&w, Some(b"\x89PNG"));
        let b = open(&bytes, &Limits::default()).unwrap();
        assert_eq!(b.world, w);
        assert_eq!(b.preview.as_deref(), Some(&b"\x89PNG"[..]));
        assert_eq!(b.manifest.world.name.as_deref(), Some("Home"));
        assert_eq!(b.manifest.files.len(), 3);
        assert_eq!(b.manifest.files[1].sha256, sha256_hex(b"DATA"));
        let m = read_manifest(&mut Cursor::new(bytes.clone()), &Limits::default()).unwrap();
        assert_eq!(m, b.manifest);
        // JSON shape.
        let v: serde_json::Value = serde_json::to_value(&m).unwrap();
        assert_eq!(v["format"], "bhworld");
        assert_eq!(v["formatVersion"], 1);
        assert_eq!(v["source"]["kind"], "blockheads-pc");
        assert_eq!(v["world"]["gameVersion"], "1.7.6");
        // Without preview.
        assert_eq!(open(&pack(&w, None), &Limits::default()).unwrap().preview, None);
    }

    #[test]
    fn unknown_source_kind_and_fields_are_accepted() {
        let w = WorldFiles::new(ID.into(), vec![]).unwrap();
        let mut m = Manifest::new(ID, &info("x"));
        m.files.clear();
        let mut v = serde_json::to_value(&m).unwrap();
        v["source"]["kind"] = "nintendo-switch".into();
        v["futureField"] = 1.into();
        let z = raw_zip(&[(MANIFEST, &serde_json::to_vec(&v).unwrap())]);
        let b = open(&z, &Limits::default()).unwrap();
        assert_eq!(b.manifest.source.kind, SourceKind::Unknown);
        assert_eq!(b.world, w);
    }

    #[test]
    fn file_helpers() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("w.bhworld");
        let m = save_with_manifest(&p, &sample(ID), Manifest::new(ID, &info("A")), None).unwrap();
        assert_eq!(open_manifest(&p, &Limits::default()).unwrap(), m);
        assert_eq!(open_bhworld(&p, &Limits::default()).unwrap().world, sample(ID));
        let bad = WorldFiles::new(
            ID.into(),
            vec![WorldFile { path: "a".into(), data: Payload::File(t.path().join("nope")) }],
        )
        .unwrap();
        assert!(save_with_manifest(&t.path().join("x.bhworld"), &bad, Manifest::new(ID, &info("A")), None).is_err());
        assert!(!t.path().join("x.bhworld.tmp").exists());
        assert!(!t.path().join("x.bhworld").exists());
    }

    #[test]
    fn rejects_tampering() {
        let w = sample(ID);
        let good = pack(&w, Some(b"img"));
        let b = open(&good, &Limits::default()).unwrap();
        let m = b.manifest.clone();
        let files: Vec<(String, Vec<u8>)> =
            w.files.iter().map(|f| (format!("world/{}", f.path), f.data.read_all().unwrap())).collect();
        let build = |m: &Manifest, extra: &[(&str, &[u8])], skip: Option<&str>| {
            let mj = manifest_json(m);
            let mut e: Vec<(&str, &[u8])> = vec![(MANIFEST, &mj)];
            e.push((PREVIEW, b"img"));
            for (n, d) in &files {
                if Some(n.as_str()) != skip {
                    e.push((n, d));
                }
            }
            e.extend_from_slice(extra);
            raw_zip(&e)
        };
        let lim = Limits::default();
        assert!(open(&build(&m, &[], None), &lim).is_ok());
        assert!(open(&build(&m, &[("world/", b"")], None), &lim).is_ok());
        let err = |z: Vec<u8>| open(&z, &lim).unwrap_err();
        assert!(matches!(err(build(&m, &[("world/extra", b"x")], None)), Error::Format(_)));
        assert!(matches!(err(build(&m, &[], Some("world/world_db/lock.mdb"))), Error::Format(_)));
        assert!(matches!(err(build(&m, &[("../evil", b"x")], None)), Error::InvalidPath(_)));
        assert!(matches!(err(raw_zip(&[("world/a", b"")])), Error::Format(_)));
        assert!(matches!(err(b"not a zip".to_vec()), Error::Zip(_)));
        assert!(matches!(err(raw_zip(&[(MANIFEST, b"{")])), Error::Json(_)));
        let mut c = m.clone();
        c.files[0].sha256 = "0".repeat(64);
        assert!(matches!(err(build(&c, &[], None)), Error::ChecksumMismatch(_)));
        let mut c = m.clone();
        c.files[0].size += 1;
        assert!(matches!(err(build(&c, &[], None)), Error::Format(_)));
        let mut c = m.clone();
        c.preview.as_mut().unwrap().sha256 = "1".repeat(64);
        assert!(matches!(err(build(&c, &[], None)), Error::ChecksumMismatch(_)));
        // Manifest checks.
        type Case = (fn(&mut Manifest), fn(&Error) -> bool);
        let cases: Vec<Case> = vec![
            (|m| m.format = "zip".into(), |e| matches!(e, Error::Format(_))),
            (|m| m.format_version = 0, |e| matches!(e, Error::Format(_))),
            (|m| m.format_version = 2, |e| matches!(e, Error::UnsupportedFormatVersion(2))),
            (|m| m.world.id = "X".into(), |e| matches!(e, Error::InvalidId(_))),
            (|m| m.files[0].path = "/abs".into(), |e| matches!(e, Error::InvalidPath(_))),
            (|m| m.files[1].path = m.files[0].path.clone(), |e| matches!(e, Error::InvalidPath(_))),
            (|m| m.files[0].sha256 = "XYZ".into(), |e| matches!(e, Error::Format(_))),
            (|m| m.files[0].sha256 = "G".repeat(64), |e| matches!(e, Error::Format(_))),
            (|m| m.preview.as_mut().unwrap().path = "a.png".into(), |e| matches!(e, Error::Format(_))),
        ];
        for (f, check) in cases {
            let mut c = m.clone();
            f(&mut c);
            let e = err(build(&c, &[], None));
            assert!(check(&e), "{e}");
        }
    }

    #[test]
    fn limits() {
        let w = sample(ID);
        let good = pack(&w, Some(b"img"));
        let ok = Limits::default();
        let e = |l: Limits| open(&good, &l).unwrap_err();
        assert!(matches!(e(Limits { max_files: 2, ..ok }), Error::LimitExceeded(_)));
        let no_preview = pack(&w, None);
        assert!(matches!(open(&no_preview, &Limits { max_files: 2, ..ok }), Err(Error::LimitExceeded(_))));
        assert!(matches!(e(Limits { max_file: 3, ..ok }), Error::LimitExceeded(_)));
        assert!(matches!(e(Limits { max_total: 10, ..ok }), Error::LimitExceeded(_)));
        assert!(matches!(e(Limits { max_manifest: 10, ..ok }), Error::LimitExceeded(_)));
        assert!(matches!(e(Limits { max_preview: 2, ..ok }), Error::Format(_)));
        // Highly compressible entry above the ratio floor.
        let zeros = WorldFiles::new(ID.into(), vec![mem("world_db/data.mdb", &vec![0u8; 100_000])]).unwrap();
        let z = pack(&zeros, None);
        assert!(matches!(open(&z, &Limits { ratio_floor: 1000, max_ratio: 10, ..ok }), Err(Error::LimitExceeded(_))));
        assert!(open(&z, &ok).is_ok());
    }

    #[test]
    fn manifest_size_enforced_on_stream() {
        // Header claims a small size but the stream is longer: take() stops and the length check fires.
        let w = sample(ID);
        let good = pack(&w, None);
        let m = read_manifest(&mut Cursor::new(good), &Limits::default()).unwrap();
        let big = serde_json::to_vec_pretty(&m).unwrap();
        let lim = Limits { max_manifest: big.len() as u64 - 1, ..Limits::default() };
        let z = raw_zip(&[(MANIFEST, &big)]);
        assert!(matches!(read_manifest(&mut Cursor::new(z), &lim), Err(Error::LimitExceeded(_))));
    }
}
