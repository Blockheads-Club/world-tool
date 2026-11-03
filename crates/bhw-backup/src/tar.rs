//! Minimal streaming tar reader/writer in the dialect Android full backups use.
//!
//! # What Android writes (`FullBackup.backupToTar` → native `write_tarfile`, `frameworks/base/libs/androidfw/BackupHelpers.cpp`)
//!
//! * POSIX ustar headers (`magic "ustar\0"`, version `"00"`), typeflag `'0'` for files and `'5'` for directories
//!   (directories have size 0). Symlinks are skipped by `FullBackupAgent.fullBackupFileTree`, never written.
//! * Paths are `apps/<package>/<domain token>/<path relative to the domain root>` (tokens from `FullBackup`: `a` apk,
//!   `obb`, `r` data root, `f` files, `db` databases, `sp` shared_prefs, `ef` external files, `d_*` device protected,
//!   `k` key/value) plus the per-package `apps/<package>/_manifest` (and optional `_meta`), which comes first.
//! * When a path does not fit the 100-byte name field (or is not 7-bit clean, or the size does not fit the octal size
//!   field) Android writes a **pax extended header** (`'x'`) *before* the ustar header. The pax header block is a copy
//!   of the ustar header (same mode and mtime) named `PaxHeader/<leaf>`, and its data always holds a `size` record
//!   followed by a `path` record. No GNU extensions (`'L'`, `'K'`) and no ustar prefix field.
//! * The archive ends with two zero blocks (`PerformAdbBackupTask.finalizeBackup`).
//!
//! # What Android reads (`TarBackupReader.readTarHeaders` / `readPaxExtendedHeader`, formerly in `FullRestoreEngine`)
//!
//! * `mode`, `mtime` and `size` are taken from the **first** header block; for a pax header that is the pax block
//!   itself, so its mode/mtime must equal the file's and its data must contain a `size` record, otherwise the restore
//!   reads the wrong length. Pax data larger than 32 KiB is rejected.
//! * Only typeflags `'0'` (file), `'5'` (directory, size forced to 0) and `'x'` are accepted; anything else (`'\0'`
//!   old-style files, symlinks, `'L'`, `'g'`) aborts the restore with "Unknown entity type". A zero typeflag means EOF.
//! * The ustar prefix field is honoured (`prefix + "/" + name`), header checksums are not verified, uid/gid/uname are
//!   ignored, and on restore the mode is masked with `0700`.
//!
//! [`TarWriter::append`] produces exactly that dialect; [`TarWriter::write_raw`] copies an entry read by
//! [`TarReader`] byte for byte (all its header blocks included), which is what backup rewriting uses for every entry it
//! keeps. The reader is more lenient than Android (old-style files, GNU long names, global pax headers) so that it can
//! also inspect tars produced by other tools.

use std::io::{self, Read, Write};

use crate::{Error, Result};

/// Tar block size.
pub const BLOCK: usize = 512;
/// Largest extension header payload accepted (pax / GNU long name).
pub const MAX_EXT_HEADER: u64 = 1 << 20;
/// Largest pax payload Android's restore accepts.
pub const ANDROID_MAX_PAX: usize = 32 * 1024;

/// Kind of a tar entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    /// Regular file (`'0'` or old-style `'\0'`).
    File,
    /// Directory (`'5'`).
    Directory,
    /// Anything else (symlink, device, ...), with its typeflag.
    Other(u8),
}

/// One entry's metadata plus its raw header blocks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TarHeader {
    /// Full path (extension headers and prefix applied, trailing `/` removed).
    pub path: String,
    /// Data size in bytes (0 for directories).
    pub size: u64,
    /// Permission bits.
    pub mode: u32,
    /// Modification time, seconds since the epoch.
    pub mtime: u64,
    /// Entry kind.
    pub kind: EntryKind,
    /// Every header block of the entry as read (extension headers with their data, then the main header).
    pub raw: Vec<u8>,
}

fn padding(size: u64) -> u64 {
    (BLOCK as u64 - size % BLOCK as u64) % BLOCK as u64
}

fn tar_err(msg: impl Into<String>) -> Error {
    Error::Tar(msg.into())
}

fn field_str(f: &[u8]) -> &[u8] {
    let end = f.iter().position(|&b| b == 0).unwrap_or(f.len());
    &f[..end]
}

fn parse_octal(f: &[u8]) -> Result<u64> {
    if f[0] & 0x80 != 0 {
        // GNU base-256: big-endian, first byte's top bit set.
        let mut v: u64 = u64::from(f[0] & 0x7f);
        for &b in &f[1..] {
            v = v.checked_mul(256).ok_or_else(|| tar_err("numeric field overflow"))? | u64::from(b);
        }
        return Ok(v);
    }
    let s = std::str::from_utf8(field_str(f)).map_err(|_| tar_err("non-ASCII numeric field"))?;
    let s = s.trim_matches(' ');
    if s.is_empty() {
        return Ok(0);
    }
    u64::from_str_radix(s, 8).map_err(|_| tar_err(format!("bad octal field {s:?}")))
}

fn checksum(block: &[u8]) -> u64 {
    block.iter().enumerate().map(|(i, &b)| if (148..156).contains(&i) { 32 } else { u64::from(b) }).sum()
}

fn path_string(b: &[u8]) -> Result<String> {
    String::from_utf8(b.to_vec()).map_err(|_| tar_err("path is not UTF-8"))
}

/// Parses pax records, returning `(path, size)` overrides.
fn parse_pax(data: &[u8]) -> Result<(Option<String>, Option<u64>)> {
    let (mut path, mut size) = (None, None);
    let mut rest = data;
    while !rest.is_empty() {
        let sp = rest.iter().position(|&b| b == b' ').ok_or_else(|| tar_err("bad pax record"))?;
        let len: usize = std::str::from_utf8(&rest[..sp])
            .ok()
            .and_then(|s| s.parse().ok())
            .filter(|&l| l > sp + 1 && l <= rest.len())
            .ok_or_else(|| tar_err("bad pax record length"))?;
        let rec = &rest[sp + 1..len - 1];
        let eq = rec.iter().position(|&b| b == b'=').ok_or_else(|| tar_err("bad pax record"))?;
        let (key, value) = (&rec[..eq], &rec[eq + 1..]);
        match key {
            b"path" => path = Some(path_string(value)?),
            b"size" => {
                size = Some(
                    std::str::from_utf8(value)
                        .ok()
                        .and_then(|s| s.parse().ok())
                        .ok_or_else(|| tar_err("bad pax size"))?,
                )
            }
            _ => {}
        }
        rest = &rest[len..];
    }
    Ok((path, size))
}

/// Streaming tar reader. Call [`TarReader::next_header`], then optionally read the data with
/// [`TarReader::read_data`] / [`TarReader::data`]; unread data is skipped by the next `next_header` call.
pub struct TarReader<'a> {
    inner: Box<dyn Read + 'a>,
    data_left: u64,
    pad_left: u64,
    finished: bool,
}

impl<'a> TarReader<'a> {
    /// Reads a tar stream.
    pub fn new(inner: Box<dyn Read + 'a>) -> Self {
        Self { inner, data_left: 0, pad_left: 0, finished: false }
    }

    /// Reads one block; `false` at a clean end of stream.
    fn read_block(&mut self, block: &mut [u8; BLOCK]) -> Result<bool> {
        let mut got = 0;
        while got < BLOCK {
            let n = self.inner.read(&mut block[got..])?;
            if n == 0 {
                if got == 0 {
                    return Ok(false);
                }
                return Err(tar_err("truncated header block"));
            }
            got += n;
        }
        Ok(true)
    }

    fn skip(&mut self, mut n: u64) -> Result<()> {
        let mut buf = [0u8; 8192];
        while n > 0 {
            let want = n.min(buf.len() as u64) as usize;
            let got = self.inner.read(&mut buf[..want])?;
            if got == 0 {
                return Err(tar_err("truncated entry data"));
            }
            n -= got as u64;
        }
        Ok(())
    }

    /// Next entry header, or `None` at the end of the archive (two zero blocks or end of stream).
    pub fn next_header(&mut self) -> Result<Option<TarHeader>> {
        let rest = self.data_left + self.pad_left;
        self.skip(rest)?;
        self.data_left = 0;
        self.pad_left = 0;
        let mut raw = Vec::new();
        let (mut long_path, mut pax_path, mut pax_size) = (None, None, None);
        loop {
            let mut block = [0u8; BLOCK];
            if self.finished || !self.read_block(&mut block)? {
                self.finished = true;
                if raw.is_empty() {
                    return Ok(None);
                }
                return Err(tar_err("extension header without an entry"));
            }
            if block.iter().all(|&b| b == 0) {
                self.finished = true;
                if raw.is_empty() {
                    return Ok(None);
                }
                return Err(tar_err("extension header without an entry"));
            }
            let stored = parse_octal(&block[148..156])?;
            if stored != checksum(&block) {
                return Err(tar_err("header checksum mismatch"));
            }
            raw.extend_from_slice(&block);
            let size = parse_octal(&block[124..136])?;
            let typeflag = block[156];
            if matches!(typeflag, b'x' | b'g' | b'L' | b'K') {
                if size > MAX_EXT_HEADER {
                    return Err(tar_err("extension header too large"));
                }
                let mut data = vec![0u8; (size + padding(size)) as usize];
                self.inner.read_exact(&mut data).map_err(|_| tar_err("truncated extension header"))?;
                raw.extend_from_slice(&data);
                data.truncate(size as usize);
                match typeflag {
                    b'x' => (pax_path, pax_size) = parse_pax(&data)?,
                    b'L' => long_path = Some(path_string(field_str(&data))?),
                    _ => {}
                }
                continue;
            }
            let name = field_str(&block[0..100]);
            let mut path = if &block[257..262] == b"ustar" && block[345] != 0 {
                let mut p = field_str(&block[345..500]).to_vec();
                p.push(b'/');
                p.extend_from_slice(name);
                path_string(&p)?
            } else {
                path_string(name)?
            };
            if let Some(p) = pax_path.take().or(long_path.take()) {
                path = p;
            }
            let kind = match typeflag {
                b'0' | 0 => EntryKind::File,
                b'5' => EntryKind::Directory,
                t => EntryKind::Other(t),
            };
            while path.len() > 1 && path.ends_with('/') {
                path.pop();
            }
            let size = if kind == EntryKind::Directory { 0 } else { pax_size.unwrap_or(size) };
            self.data_left = size;
            self.pad_left = padding(size);
            let mode = parse_octal(&block[100..108])? as u32;
            let mtime = parse_octal(&block[136..148])?;
            return Ok(Some(TarHeader { path, size, mode, mtime, kind, raw }));
        }
    }

    /// Reader over the current entry's remaining data.
    pub fn data(&mut self) -> EntryData<'_, 'a> {
        EntryData { reader: self }
    }

    /// Copies the current entry's remaining data to `out`, returning the number of bytes.
    pub fn read_data(&mut self, out: &mut dyn Write) -> Result<u64> {
        Ok(io::copy(&mut self.data(), out)?)
    }
}

/// `Read` over the data of the current [`TarReader`] entry.
pub struct EntryData<'r, 'a> {
    reader: &'r mut TarReader<'a>,
}

impl Read for EntryData<'_, '_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let r = &mut *self.reader;
        if r.data_left == 0 {
            return Ok(0);
        }
        let want = (buf.len() as u64).min(r.data_left) as usize;
        let n = r.inner.read(&mut buf[..want])?;
        if n == 0 {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "truncated tar entry data"));
        }
        r.data_left -= n as u64;
        Ok(n)
    }
}

fn put_octal(block: &mut [u8], at: usize, width: usize, v: u64) {
    let s = format!("{:0w$o}", v, w = width - 1);
    block[at..at + width - 1].copy_from_slice(s.as_bytes());
    block[at + width - 1] = 0;
}

fn finish_checksum(block: &mut [u8]) {
    let sum = checksum(block);
    block[148..154].copy_from_slice(format!("{sum:06o}").as_bytes());
    block[154] = 0;
    block[155] = b' ';
}

/// A pax record `"<len> key=value\n"` where `len` counts the whole record including its own digits.
fn pax_record(key: &str, value: &[u8]) -> Vec<u8> {
    let body = key.len() + value.len() + 3; // ' ', '=', '\n'
    let mut len = body + 1;
    while len != body + len.to_string().len() {
        len = body + len.to_string().len();
    }
    let mut r = format!("{len} {key}=").into_bytes();
    r.extend_from_slice(value);
    r.push(b'\n');
    r
}

/// Size limit of the 12-byte octal size field.
const MAX_OCTAL_SIZE: u64 = 0o77777777777;

/// Header blocks for one entry in Android's `write_tarfile` style (pax header first when needed).
pub fn android_header(path: &str, kind: EntryKind, mode: u32, mtime: u64, size: u64) -> Result<Vec<u8>> {
    let (typeflag, size) = match kind {
        EntryKind::File => (b'0', size),
        EntryKind::Directory => (b'5', 0),
        EntryKind::Other(_) => return Err(Error::Invalid("only files and directories can be written".into())),
    };
    if path.is_empty() || path.contains('\0') {
        return Err(Error::Invalid(format!("bad tar path {path:?}")));
    }
    let bytes = path.as_bytes();
    let mut h = [0u8; BLOCK];
    h[..bytes.len().min(100)].copy_from_slice(&bytes[..bytes.len().min(100)]);
    put_octal(&mut h, 100, 8, u64::from(mode & 0o7777));
    put_octal(&mut h, 108, 8, 0);
    put_octal(&mut h, 116, 8, 0);
    put_octal(&mut h, 124, 12, size.min(MAX_OCTAL_SIZE));
    put_octal(&mut h, 136, 12, mtime.min(MAX_OCTAL_SIZE));
    h[156] = typeflag;
    h[257..263].copy_from_slice(b"ustar\0");
    h[263..265].copy_from_slice(b"00");
    let need_pax = bytes.len() > 99 || !path.is_ascii() || size > MAX_OCTAL_SIZE;
    let mut out = Vec::with_capacity(3 * BLOCK);
    if need_pax {
        let mut data = pax_record("size", size.to_string().as_bytes());
        data.extend_from_slice(&pax_record("path", bytes));
        if data.len() > ANDROID_MAX_PAX {
            return Err(Error::Invalid(format!("path too long for an Android backup: {path:?}")));
        }
        let mut p = h;
        p[..100].fill(0);
        let leaf = path.rsplit('/').next().unwrap_or(path);
        let name = format!("PaxHeader/{leaf}");
        let mut cut = name.len().min(99);
        while !name.is_char_boundary(cut) {
            cut -= 1;
        }
        p[..cut].copy_from_slice(&name.as_bytes()[..cut]);
        p[156] = b'x';
        put_octal(&mut p, 124, 12, data.len() as u64);
        finish_checksum(&mut p);
        out.extend_from_slice(&p);
        let pad = padding(data.len() as u64) as usize;
        out.extend_from_slice(&data);
        out.resize(out.len() + pad, 0);
    }
    finish_checksum(&mut h);
    out.extend_from_slice(&h);
    Ok(out)
}

/// Streaming tar writer.
pub struct TarWriter<'w> {
    out: &'w mut dyn Write,
}

impl<'w> TarWriter<'w> {
    /// Writes to `out`.
    pub fn new(out: &'w mut dyn Write) -> Self {
        Self { out }
    }

    fn data(&mut self, size: u64, data: &mut dyn Read) -> Result<()> {
        let copied = io::copy(&mut data.take(size), self.out)?;
        if copied != size {
            return Err(Error::Invalid(format!("entry data is {copied} bytes, header says {size}")));
        }
        self.out.write_all(&[0u8; BLOCK][..padding(size) as usize])?;
        Ok(())
    }

    /// Copies an entry read by [`TarReader`] unchanged (header blocks byte for byte, then `header.size` data bytes).
    pub fn write_raw(&mut self, header: &TarHeader, data: &mut dyn Read) -> Result<()> {
        self.out.write_all(&header.raw)?;
        self.data(header.size, data)
    }

    /// Appends a new entry in Android's dialect ([`android_header`]); `data` must yield exactly `size` bytes.
    pub fn append(
        &mut self,
        path: &str,
        kind: EntryKind,
        mode: u32,
        mtime: u64,
        size: u64,
        data: &mut dyn Read,
    ) -> Result<()> {
        let header = android_header(path, kind, mode, mtime, size)?;
        self.out.write_all(&header)?;
        let size = if kind == EntryKind::Directory { 0 } else { size };
        self.data(size, data)
    }

    /// Writes the end-of-archive marker (two zero blocks).
    pub fn finish(self) -> Result<()> {
        self.out.write_all(&[0u8; 2 * BLOCK])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build(entries: &[(&str, EntryKind, &[u8])]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut w = TarWriter::new(&mut out);
        for (p, k, d) in entries {
            let mut r: &[u8] = d;
            w.append(p, *k, 0o600, 1_700_000_000, d.len() as u64, &mut r).unwrap();
        }
        w.finish().unwrap();
        out
    }

    fn read_all(data: Vec<u8>) -> Result<Vec<(TarHeader, Vec<u8>)>> {
        let mut r = TarReader::new(Box::new(io::Cursor::new(data)));
        let mut v = Vec::new();
        while let Some(h) = r.next_header()? {
            let mut d = Vec::new();
            r.read_data(&mut d)?;
            v.push((h, d));
        }
        assert!(r.next_header()?.is_none());
        Ok(v)
    }

    #[test]
    fn round_trip_short_long_and_unicode_paths() {
        let long = format!(
            "apps/com.noodlecake.blockheads/f/Library/Application Support/saves/{}/world_db/data.mdb",
            "a".repeat(32)
        );
        let tar = build(&[
            ("apps/x/_manifest", EntryKind::File, b"1\nx\n"),
            ("apps/x/f/dir", EntryKind::Directory, b""),
            (&long, EntryKind::File, &[7u8; 1000]),
            ("apps/x/f/çok.txt", EntryKind::File, b""),
        ]);
        let e = read_all(tar).unwrap();
        assert_eq!(e.len(), 4);
        assert_eq!(e[0].0.path, "apps/x/_manifest");
        assert_eq!(e[0].1, b"1\nx\n");
        assert_eq!((e[1].0.kind, e[1].0.size), (EntryKind::Directory, 0));
        assert_eq!(e[2].0.path, long);
        assert_eq!(e[2].1, vec![7u8; 1000]);
        assert_eq!(e[2].0.raw.len(), 3 * BLOCK);
        assert_eq!(e[2].0.raw[156], b'x');
        assert_eq!((e[2].0.mode, e[2].0.mtime), (0o600, 1_700_000_000));
        // Android reads mode/mtime from the pax block, so it must carry the file's values.
        assert_eq!(&e[2].0.raw[100..108], &e[2].0.raw[BLOCK * 2 + 100..BLOCK * 2 + 108]);
        assert!(String::from_utf8_lossy(&e[2].0.raw[BLOCK..2 * BLOCK]).starts_with("13 size=1000\n"));
        assert_eq!(e[3].0.path, "apps/x/f/çok.txt");
        assert_eq!(e[3].0.raw.len(), 3 * BLOCK);
    }

    #[test]
    fn header_bytes_match_android_style() {
        let h = android_header("apps/p/f/a", EntryKind::File, 0o100660, 0o1234, 5).unwrap();
        assert_eq!(h.len(), BLOCK);
        assert_eq!(&h[100..108], b"0000660\0");
        assert_eq!(&h[124..136], b"00000000005\0");
        assert_eq!(&h[136..148], b"00000001234\0");
        assert_eq!(h[156], b'0');
        assert_eq!(&h[257..265], b"ustar\x0000");
        assert_eq!(h[154..156], [0, b' ']);
        assert_eq!(parse_octal(&h[148..156]).unwrap(), checksum(&h));
        let d = android_header("apps/p/f/d", EntryKind::Directory, 0o771, 0, 99).unwrap();
        assert_eq!((d[156], &d[124..136]), (b'5', &b"00000000000\0"[..]));
        assert!(android_header("x", EntryKind::Other(b'2'), 0, 0, 0).is_err());
        assert!(android_header("", EntryKind::File, 0, 0, 0).is_err());
        assert!(android_header("a\0b", EntryKind::File, 0, 0, 0).is_err());
        assert!(android_header(&"a".repeat(40_000), EntryKind::File, 0, 0, 0).is_err());
        // Pax header name keeps char boundaries.
        let p = android_header(&format!("d/{}", "é".repeat(60)), EntryKind::File, 0, 0, 0).unwrap();
        assert!(std::str::from_utf8(field_str(&p[..100])).is_ok());
        // Huge sizes go to pax.
        let big = android_header("a", EntryKind::File, 0, 0, MAX_OCTAL_SIZE + 1).unwrap();
        assert_eq!(big.len(), 3 * BLOCK);
    }

    #[test]
    fn pax_record_lengths() {
        assert_eq!(pax_record("size", b"1000"), b"13 size=1000\n");
        let r = pax_record("path", &[b'a'; 91]);
        assert_eq!(r.len(), 101);
        assert!(r.starts_with(b"101 path="));
        let r = pax_record("path", &[b'a'; 90]);
        assert_eq!(r.len(), 99);
    }

    fn raw_header(name: &[u8], typeflag: u8, size: u64, prefix: &[u8], ustar: bool) -> [u8; BLOCK] {
        let mut h = [0u8; BLOCK];
        h[..name.len()].copy_from_slice(name);
        put_octal(&mut h, 100, 8, 0o644);
        put_octal(&mut h, 124, 12, size);
        put_octal(&mut h, 136, 12, 0);
        h[156] = typeflag;
        if ustar {
            h[257..263].copy_from_slice(b"ustar\0");
        }
        h[345..345 + prefix.len()].copy_from_slice(prefix);
        finish_checksum(&mut h);
        h
    }

    fn with_data(mut h: Vec<u8>, data: &[u8]) -> Vec<u8> {
        h.extend_from_slice(data);
        h.resize(h.len() + padding(data.len() as u64) as usize, 0);
        h
    }

    #[test]
    fn reads_foreign_dialects() {
        // Base-256 size.
        let mut b = raw_header(b"b256", b'0', 0, b"", true);
        b[124..136].copy_from_slice(&[0x80, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 3]);
        finish_checksum(&mut b);

        let mut t = Vec::new();
        t.extend(with_data(raw_header(b"old.txt", 0, 2, b"", false).to_vec(), b"hi"));
        t.extend(raw_header(b"name", b'0', 0, b"pre/fix", true));
        t.extend(with_data(raw_header(b"././@LongLink", b'L', 8, b"", false).to_vec(), b"gnu/lng\0"));
        t.extend(raw_header(b"trunc", b'0', 0, b"", false));
        t.extend(with_data(raw_header(b"g", b'g', 8, b"", true).to_vec(), b"8 a=bc\n"));
        t.extend(with_data(raw_header(b"x", b'x', 11, b"", true).to_vec(), b"11 mtime=1\n"));
        t.extend(raw_header(b"dir/", b'5', 0, b"", true));
        t.extend(raw_header(b"link", b'2', 0, b"", true));
        t.extend(raw_header(b"/", b'5', 0, b"", true));
        t.extend(with_data(b.to_vec(), b"abc"));
        let e = read_all(t).unwrap();
        let paths: Vec<_> = e.iter().map(|(h, _)| (h.path.as_str(), h.kind)).collect();
        assert_eq!(
            paths,
            [
                ("old.txt", EntryKind::File),
                ("pre/fix/name", EntryKind::File),
                ("gnu/lng", EntryKind::File),
                ("dir", EntryKind::Directory),
                ("link", EntryKind::Other(b'2')),
                ("/", EntryKind::Directory),
                ("b256", EntryKind::File),
            ]
        );
        assert_eq!(e[0].1, b"hi");
        assert_eq!(e[6].1, b"abc");
    }

    #[test]
    fn rejects_broken_archives() {
        let ok = build(&[("a", EntryKind::File, b"hello")]);
        // Truncated header block.
        assert!(read_all(ok[..100].to_vec()).is_err());
        // Truncated data while skipping (data not read by the caller).
        let mut r = TarReader::new(Box::new(io::Cursor::new(ok[..BLOCK + 2].to_vec())));
        r.next_header().unwrap().unwrap();
        assert!(r.next_header().is_err());
        // Truncated data while reading.
        let mut r = TarReader::new(Box::new(io::Cursor::new(ok[..BLOCK + 2].to_vec())));
        r.next_header().unwrap().unwrap();
        assert!(r.read_data(&mut Vec::new()).is_err());
        // Checksum mismatch.
        let mut bad = ok.clone();
        bad[0] = b'b';
        assert!(read_all(bad).is_err());
        // Missing end marker is fine.
        assert_eq!(read_all(ok[..2 * BLOCK].to_vec()).unwrap().len(), 1);
        // Extension header followed by end of stream / zero block.
        let x = with_data(raw_header(b"x", b'x', 11, b"", true).to_vec(), b"11 mtime=1\n");
        assert!(read_all(x.clone()).is_err());
        let mut xz = x.clone();
        xz.extend_from_slice(&[0; BLOCK]);
        assert!(read_all(xz).is_err());
        // Truncated / oversized extension data.
        assert!(read_all(raw_header(b"x", b'x', 11, b"", true).to_vec()).is_err());
        assert!(read_all(raw_header(b"x", b'x', MAX_EXT_HEADER + 1, b"", true).to_vec()).is_err());
        // Bad octal, non-ASCII numeric, non-UTF-8 path.
        let mut h = raw_header(b"a", b'0', 0, b"", true);
        h[124] = b'9';
        finish_checksum(&mut h);
        assert!(read_all(h.to_vec()).is_err());
        let mut h = raw_header(b"a", b'0', 0, b"", true);
        h[124] = 0xC3;
        h[125] = 0x28;
        finish_checksum(&mut h);
        assert!(read_all(h.to_vec()).is_err());
        assert!(read_all(raw_header(b"\xff", b'0', 0, b"", true).to_vec()).is_err());
        // Base-256 overflow.
        let mut h = raw_header(b"a", b'0', 0, b"", true);
        h[124..136].copy_from_slice(&[0xff; 12]);
        finish_checksum(&mut h);
        assert!(read_all(h.to_vec()).is_err());
        // Bad pax records.
        for rec in [&b"nospace"[..], b"99 path=a\n", b"x path=a\n", b"9 pathx\n\n", b"10 size=z\n", b"11 path=\xff\n"] {
            let t = with_data(raw_header(b"x", b'x', rec.len() as u64, b"", true).to_vec(), rec);
            assert!(read_all(t).is_err(), "{}", String::from_utf8_lossy(rec));
        }
    }

    #[test]
    fn writer_checks_sizes_and_raw_copy() {
        let mut out = Vec::new();
        let mut w = TarWriter::new(&mut out);
        let mut short: &[u8] = b"abc";
        assert!(w.append("a", EntryKind::File, 0, 0, 5, &mut short).is_err());
        let tar = build(&[("a", EntryKind::File, b"hello"), ("d", EntryKind::Directory, b"")]);
        let mut r = TarReader::new(Box::new(io::Cursor::new(tar.clone())));
        let mut out = Vec::new();
        let mut w = TarWriter::new(&mut out);
        while let Some(h) = r.next_header().unwrap() {
            w.write_raw(&h, &mut r.data()).unwrap();
        }
        w.finish().unwrap();
        assert_eq!(out, tar);
    }
}
