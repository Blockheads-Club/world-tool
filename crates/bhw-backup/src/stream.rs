//! Opening and creating `.ab` streams (header + encryption + compression around a tar stream).

use std::io::{self, BufReader, Read, Write};

use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use flate2::Compression;

use crate::crypto::{decrypt_master_key, new_aes_params, CbcDecryptReader, CbcEncryptWriter, PlainSink, Sink};
use crate::header::{read_header, write_header, BackupHeader, Encryption, MAX_VERSION};
use crate::{Error, Result};

/// Opens a backup: parses the header, unlocks it with `password` if it is encrypted and returns the header plus a
/// streaming reader of the plain tar data.
///
/// Errors: [`Error::NotABackup`], [`Error::UnsupportedVersion`], [`Error::BadHeader`],
/// [`Error::UnsupportedEncryption`], [`Error::PasswordRequired`] and [`Error::WrongPassword`] are reported here; a
/// corrupt payload surfaces later as an I/O error from the returned reader.
pub fn open_backup<'a>(
    input: Box<dyn Read + 'a>,
    password: Option<&str>,
) -> Result<(BackupHeader, Box<dyn Read + 'a>)> {
    let mut buf = BufReader::new(input);
    let header = read_header(&mut buf)?;
    let mut stream: Box<dyn Read + 'a> = Box::new(buf);
    if let Encryption::Aes256(p) = &header.encryption {
        let password = password.ok_or(Error::PasswordRequired)?;
        let mk = decrypt_master_key(p, password, header.version)?;
        stream = Box::new(CbcDecryptReader::new(stream, &mk));
    }
    if header.compressed {
        stream = Box::new(ZlibDecoder::new(stream));
    }
    Ok((header, stream))
}

/// Options for a new backup file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupOptions {
    /// Backup file version written in the header. Devices reject versions newer than their own, so when rewriting
    /// keep the version of the device's own backup. Default: 5 (Android 9+).
    pub version: u32,
    /// zlib-compress the tar stream (Android always does). Default: true.
    pub compressed: bool,
    /// Encrypt with this password (AES-256). Default: none.
    pub password: Option<String>,
    /// PBKDF2 rounds for encryption. Default: 10000 like Android.
    pub rounds: u32,
}

impl Default for BackupOptions {
    fn default() -> Self {
        Self { version: MAX_VERSION, compressed: true, password: None, rounds: crate::crypto::DEFAULT_ROUNDS }
    }
}

struct ZlibSink<'a>(ZlibEncoder<Box<dyn Sink + 'a>>);

impl Write for ZlibSink<'_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

impl Sink for ZlibSink<'_> {
    fn finish(self: Box<Self>) -> io::Result<()> {
        self.0.finish()?.finish()
    }
}

/// Writes a backup: create it, write the tar stream into it (e.g. with [`crate::TarWriter`]), then call
/// [`BackupWriter::finish`] (dropping it without `finish` leaves a truncated file).
pub struct BackupWriter<'a> {
    header: BackupHeader,
    sink: Box<dyn Sink + 'a>,
}

impl<'a> BackupWriter<'a> {
    /// Writes the header to `out` and sets up encryption/compression.
    pub fn create(mut out: Box<dyn Write + 'a>, opts: &BackupOptions) -> Result<Self> {
        if opts.version == 0 || opts.version > MAX_VERSION {
            return Err(Error::UnsupportedVersion(opts.version));
        }
        if opts.rounds == 0 || opts.rounds > crate::header::MAX_ROUNDS {
            return Err(Error::Invalid(format!("PBKDF2 rounds {} out of range", opts.rounds)));
        }
        let mut header =
            BackupHeader { version: opts.version, compressed: opts.compressed, encryption: Encryption::None };
        let mut key = None;
        if let Some(pw) = &opts.password {
            let (params, mk) = new_aes_params(pw, opts.rounds);
            header.encryption = Encryption::Aes256(params);
            key = Some(mk);
        }
        write_header(&mut out, &header)?;
        let mut sink: Box<dyn Sink + 'a> = Box::new(PlainSink(out));
        if let Some(mk) = key {
            sink = Box::new(CbcEncryptWriter::new(sink, &mk));
        }
        if opts.compressed {
            sink = Box::new(ZlibSink(ZlibEncoder::new(sink, Compression::default())));
        }
        Ok(Self { header, sink })
    }

    /// The header that was written.
    pub fn header(&self) -> &BackupHeader {
        &self.header
    }

    /// Flushes compression and encryption trailers.
    pub fn finish(self) -> Result<()> {
        self.sink.finish()?;
        Ok(())
    }
}

impl Write for BackupWriter<'_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.sink.write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.sink.flush()
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::tar::{EntryKind, TarReader, TarWriter};

    /// Builds a backup from `(path, data)` files (directories when data is `None`).
    pub(crate) fn make_backup(opts: &BackupOptions, entries: &[(&str, Option<&[u8]>)]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut w = BackupWriter::create(Box::new(&mut out), opts).unwrap();
        let mut t = TarWriter::new(&mut w);
        for (p, d) in entries {
            let (kind, data) = match d {
                Some(d) => (EntryKind::File, *d),
                None => (EntryKind::Directory, &b""[..]),
            };
            let mut r: &[u8] = data;
            t.append(p, kind, 0o600, 1_700_000_000, data.len() as u64, &mut r).unwrap();
        }
        t.finish().unwrap();
        w.flush().unwrap();
        w.finish().unwrap();
        out
    }

    pub(crate) fn list(ab: &[u8], pw: Option<&str>) -> Result<Vec<(String, Vec<u8>)>> {
        let (_, s) = open_backup(Box::new(ab), pw)?;
        let mut r = TarReader::new(s);
        let mut v = Vec::new();
        while let Some(h) = r.next_header()? {
            let mut d = Vec::new();
            r.read_data(&mut d)?;
            v.push((h.path, d));
        }
        Ok(v)
    }

    #[test]
    fn all_combinations_round_trip() {
        let big: Vec<u8> = (0..300_000u32).map(|i| (i % 253) as u8).collect();
        let files: [(&str, Option<&[u8]>); 3] =
            [("apps/p/_manifest", Some(b"1\np\n")), ("apps/p/f", None), ("apps/p/f/big", Some(&big))];
        for compressed in [false, true] {
            for password in [None, Some("secret".to_string())] {
                for version in [1, 2, 5] {
                    let opts = BackupOptions { version, compressed, password: password.clone(), rounds: 100 };
                    let ab = make_backup(&opts, &files);
                    let (h, _) = open_backup(Box::new(&ab[..]), password.as_deref()).unwrap();
                    assert_eq!((h.version, h.compressed), (version, compressed));
                    let got = list(&ab, password.as_deref()).unwrap();
                    assert_eq!(got.len(), 3);
                    assert_eq!(got[2].1, big);
                    if password.is_some() {
                        assert!(matches!(list(&ab, None), Err(Error::PasswordRequired)));
                        assert!(matches!(list(&ab, Some("wrong")), Err(Error::WrongPassword)));
                    }
                }
            }
        }
    }

    #[test]
    fn writer_rejects_bad_options_and_reports_header() {
        let mut out = Vec::new();
        for opts in [
            BackupOptions { version: 0, ..Default::default() },
            BackupOptions { version: 6, ..Default::default() },
            BackupOptions { rounds: 0, ..Default::default() },
        ] {
            assert!(BackupWriter::create(Box::new(&mut out), &opts).is_err());
        }
        let w = BackupWriter::create(Box::new(&mut out), &BackupOptions::default()).unwrap();
        assert_eq!(w.header().encryption, Encryption::None);
        w.finish().unwrap();
        assert!(out.starts_with(b"ANDROID BACKUP\n5\n1\nnone\n"));
    }

    #[test]
    fn corrupt_payload_is_an_io_error() {
        let ab = make_backup(&BackupOptions::default(), &[("a", Some(b"x"))]);
        let cut = ab[..ab.len() / 2].to_vec();
        assert!(matches!(list(&cut, None), Err(Error::Io(_)) | Err(Error::Tar(_))));
    }
}
