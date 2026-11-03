//! The text header of an `.ab` file.
//!
//! Written by AOSP `PerformAdbBackupTask.run()` (magic, `BACKUP_FILE_VERSION`, compression flag, encryption name, then
//! for AES-256 the output of `encryptAndWriteHeader`). Read back by `PerformAdbRestoreTask` /
//! `parseBackupFileHeaderAndReturnTarStream`, which accepts any version `<=` the device's own
//! `BACKUP_FILE_VERSION` (1: Android 4.0-4.3, 2: 4.4, 3: 5.x, 4: 7.x, 5: 9+). A device rejects backups whose version is
//! newer than its own, so a rewrite keeps the source version.

use std::io::{BufRead, Write};

use crate::{hex, Error, Result};

/// First line of every backup.
pub const MAGIC_LINE: &str = "ANDROID BACKUP";
/// Newest backup file version known (Android 9+ writes 5).
pub const MAX_VERSION: u32 = 5;
/// Encryption name for unencrypted backups.
pub const ENCRYPTION_NONE: &str = "none";
/// Encryption name for password protected backups (`PasswordUtils.ENCRYPTION_ALGORITHM_NAME`).
pub const ENCRYPTION_AES256: &str = "AES-256";
/// Highest PBKDF2 round count accepted when reading (Android uses 10000); guards against hostile files.
pub const MAX_ROUNDS: u32 = 1_000_000;
const MAX_LINE: usize = 4096;

/// Parsed `.ab` header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupHeader {
    /// Backup file format version (1..=5).
    pub version: u32,
    /// Payload is a zlib stream.
    pub compressed: bool,
    /// Encryption parameters.
    pub encryption: Encryption,
}

/// Encryption part of the header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Encryption {
    /// `none`.
    None,
    /// `AES-256`, see [`crate::crypto`].
    Aes256(AesParams),
}

/// The five extra header lines of an `AES-256` backup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AesParams {
    /// Salt for the PBKDF2 user key (64 bytes when written by Android).
    pub user_salt: Vec<u8>,
    /// Salt for the master key checksum (64 bytes).
    pub checksum_salt: Vec<u8>,
    /// PBKDF2 iteration count (10000).
    pub rounds: u32,
    /// IV of the master key blob encryption (16 bytes).
    pub user_iv: Vec<u8>,
    /// AES-256-CBC encrypted `[len, data IV, len, master key, len, checksum]` blob.
    pub master_key_blob: Vec<u8>,
}

/// Reads one `\n` terminated line (byte by byte so nothing past the header is consumed from `r`'s buffer contract).
fn read_line(r: &mut dyn BufRead) -> Result<String> {
    let mut buf = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        if r.read(&mut byte)? == 0 {
            return Err(Error::BadHeader("unexpected end of header".into()));
        }
        if byte[0] == b'\n' {
            break;
        }
        if buf.len() == MAX_LINE {
            return Err(Error::BadHeader("header line too long".into()));
        }
        buf.push(byte[0]);
    }
    String::from_utf8(buf).map_err(|_| Error::BadHeader("header line is not UTF-8".into()))
}

fn parse_u32(line: &str, what: &str) -> Result<u32> {
    line.trim().parse().map_err(|_| Error::BadHeader(format!("bad {what}: {line:?}")))
}

/// Reads the header, leaving `r` positioned at the first payload byte.
pub fn read_header(r: &mut dyn BufRead) -> Result<BackupHeader> {
    let magic = read_line(r).map_err(|_| Error::NotABackup)?;
    if magic != MAGIC_LINE {
        return Err(Error::NotABackup);
    }
    let version = parse_u32(&read_line(r)?, "version")?;
    if version == 0 || version > MAX_VERSION {
        return Err(Error::UnsupportedVersion(version));
    }
    let compressed = match read_line(r)?.as_str() {
        "0" => false,
        "1" => true,
        other => return Err(Error::BadHeader(format!("bad compression flag {other:?}"))),
    };
    let name = read_line(r)?;
    let encryption = match name.as_str() {
        ENCRYPTION_NONE => Encryption::None,
        ENCRYPTION_AES256 => Encryption::Aes256(read_aes_params(r)?),
        _ => return Err(Error::UnsupportedEncryption(name)),
    };
    Ok(BackupHeader { version, compressed, encryption })
}

fn read_aes_params(r: &mut dyn BufRead) -> Result<AesParams> {
    let user_salt = hex::decode(&read_line(r)?)?;
    let checksum_salt = hex::decode(&read_line(r)?)?;
    let rounds = parse_u32(&read_line(r)?, "PBKDF2 rounds")?;
    if rounds == 0 || rounds > MAX_ROUNDS {
        return Err(Error::BadHeader(format!("PBKDF2 rounds {rounds} out of range")));
    }
    let user_iv = hex::decode(&read_line(r)?)?;
    if user_iv.len() != 16 {
        return Err(Error::BadHeader("user key IV must be 16 bytes".into()));
    }
    let master_key_blob = hex::decode(&read_line(r)?)?;
    if master_key_blob.is_empty() || master_key_blob.len() % 16 != 0 {
        return Err(Error::BadHeader("master key blob must be a non-empty multiple of 16 bytes".into()));
    }
    Ok(AesParams { user_salt, checksum_salt, rounds, user_iv, master_key_blob })
}

/// Writes the header exactly as Android does (upper-case hex, `\n` line ends).
pub fn write_header(w: &mut dyn Write, h: &BackupHeader) -> std::io::Result<()> {
    write!(w, "{MAGIC_LINE}\n{}\n{}\n", h.version, u8::from(h.compressed))?;
    match &h.encryption {
        Encryption::None => writeln!(w, "{ENCRYPTION_NONE}"),
        Encryption::Aes256(p) => write!(
            w,
            "{ENCRYPTION_AES256}\n{}\n{}\n{}\n{}\n{}\n",
            hex::encode_upper(&p.user_salt),
            hex::encode_upper(&p.checksum_salt),
            p.rounds,
            hex::encode_upper(&p.user_iv),
            hex::encode_upper(&p.master_key_blob)
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &[u8]) -> Result<BackupHeader> {
        let mut r: &[u8] = s;
        read_header(&mut r)
    }

    fn aes() -> AesParams {
        AesParams {
            user_salt: vec![1; 64],
            checksum_salt: vec![2; 64],
            rounds: 10000,
            user_iv: vec![3; 16],
            master_key_blob: vec![4; 96],
        }
    }

    #[test]
    fn round_trip_plain_and_aes() {
        for h in [
            BackupHeader { version: 5, compressed: true, encryption: Encryption::None },
            BackupHeader { version: 1, compressed: false, encryption: Encryption::Aes256(aes()) },
        ] {
            let mut out = Vec::new();
            write_header(&mut out, &h).unwrap();
            out.extend_from_slice(b"PAYLOAD");
            let mut r: &[u8] = &out;
            assert_eq!(read_header(&mut r).unwrap(), h);
            assert_eq!(r, b"PAYLOAD");
        }
        let mut out = Vec::new();
        write_header(&mut out, &BackupHeader { version: 5, compressed: true, encryption: Encryption::None }).unwrap();
        assert_eq!(out, b"ANDROID BACKUP\n5\n1\nnone\n");
    }

    #[test]
    fn bad_headers() {
        assert!(matches!(parse(b"PK\x03\x04"), Err(Error::NotABackup)));
        assert!(matches!(parse(b"HELLO\n"), Err(Error::NotABackup)));
        assert!(matches!(parse(&[b'A'; 5000]), Err(Error::NotABackup)));
        assert!(matches!(parse(b"ANDROID BACKUP\n\xff\n"), Err(Error::BadHeader(_))));
        assert!(matches!(parse(b"ANDROID BACKUP\nx\n"), Err(Error::BadHeader(_))));
        assert!(matches!(parse(b"ANDROID BACKUP\n0\n"), Err(Error::UnsupportedVersion(0))));
        assert!(matches!(parse(b"ANDROID BACKUP\n6\n"), Err(Error::UnsupportedVersion(6))));
        assert!(matches!(parse(b"ANDROID BACKUP\n5\n2\n"), Err(Error::BadHeader(_))));
        assert!(matches!(parse(b"ANDROID BACKUP\n5\n1\nDES\n"), Err(Error::UnsupportedEncryption(n)) if n == "DES"));
        assert!(matches!(parse(b"ANDROID BACKUP\n5\n1\nnone"), Err(Error::BadHeader(_))));
        let pre = b"ANDROID BACKUP\n5\n1\nAES-256\n00\n00\n".to_vec();
        for (tail, ok) in [
            (&b"0\n"[..], false),
            (b"2000000\n", false),
            (b"10\n00\n", false),
            (b"10\n00000000000000000000000000000000\n\n", false),
            (b"10\n00000000000000000000000000000000\n0000\n", false),
            (b"10\n00000000000000000000000000000000\n00000000000000000000000000000000\n", true),
        ] {
            let mut h = pre.clone();
            h.extend_from_slice(tail);
            assert_eq!(parse(&h).is_ok(), ok, "{}", String::from_utf8_lossy(tail));
        }
    }
}
