//! AES-256 backup encryption, as in AOSP `PerformAdbBackupTask.encryptAndWriteHeader`,
//! `PerformAdbRestoreTask.decodeAesHeaderAndInitialize` / `attemptMasterKeyDecryption` and `PasswordUtils`.
//!
//! * The *user key* is `PBKDF2-HMAC-SHA1(password, user_salt, rounds, 256 bit)`.
//! * A random 256-bit *master key* and 128-bit *data IV* encrypt the payload with AES-256-CBC/PKCS#5.
//! * The *master key blob* `[len(iv), iv, len(mk), mk, len(ck), ck]` is AES-256-CBC/PKCS#5 encrypted with the user key
//!   and `user_iv`. `ck` is `makeKeyChecksum(mk)`: the master key bytes are turned into a Java `char[]` with
//!   `(char) b` (a *signed* byte, so `0x80..=0xff` become `U+FF80..=U+FFFF`) and run through PBKDF2 with the checksum
//!   salt.
//! * How a `char[]` becomes PBKDF2 input bytes depends on the algorithm ([`KdfRegime`]): since Android 4.4 (backup
//!   version 2) `PBKDF2WithHmacSHA1` encodes the chars as UTF-8, so the checksum of a master key byte `>= 0x80` is fed
//!   as the three bytes `EF BE xx`/`EF BF xx`. Version 1 backups were made with the "8-bit" variant
//!   (`PBKDF2WithHmacSHA1And8bit`, low byte of each char); the restore code tries the UTF-8 regime first and falls back
//!   to the 8-bit one only for version 1 files (`pbkdf2Fallback = (version == 1)`). The same regime applies to the user
//!   password.
//!
//! A wrong password shows up as bad PKCS#5 padding of the blob, an implausible blob layout or a checksum mismatch;
//! all of these give [`Error::WrongPassword`].

use std::io::{self, Read, Write};

use aes::cipher::generic_array::GenericArray;
use aes::cipher::{BlockDecrypt, BlockEncrypt, KeyInit};
use aes::Aes256;
use sha1::Sha1;

use crate::header::AesParams;
use crate::{random_bytes, Error, Result};

/// AES block size.
pub const BLOCK: usize = 16;
/// Salt length Android uses (`PBKDF2_SALT_SIZE` = 512 bits).
pub const SALT_LEN: usize = 64;
/// PBKDF2 rounds Android uses (`PBKDF2_HASH_ROUNDS`).
pub const DEFAULT_ROUNDS: u32 = 10_000;

/// How a Java `char[]` password is turned into PBKDF2 input bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KdfRegime {
    /// `PBKDF2WithHmacSHA1`: UTF-8 (Android 4.4+, backup version >= 2).
    Utf8,
    /// `PBKDF2WithHmacSHA1And8bit`: low 8 bits of each UTF-16 unit (Android 4.0-4.3, version 1).
    EightBit,
}

/// Master key and data IV of an encrypted backup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MasterKey {
    /// AES-256 key of the payload.
    pub key: [u8; 32],
    /// CBC IV of the payload.
    pub iv: [u8; BLOCK],
}

/// Password bytes for PBKDF2 in the given regime.
pub fn password_bytes(password: &str, regime: KdfRegime) -> Vec<u8> {
    match regime {
        KdfRegime::Utf8 => password.as_bytes().to_vec(),
        KdfRegime::EightBit => password.encode_utf16().map(|c| c as u8).collect(),
    }
}

/// PBKDF2 input bytes of `makeKeyChecksum(masterKey)`: each byte is sign-extended to a Java char first.
pub fn checksum_password_bytes(master_key: &[u8], regime: KdfRegime) -> Vec<u8> {
    if regime == KdfRegime::EightBit {
        return master_key.to_vec();
    }
    let mut out = Vec::with_capacity(master_key.len() * 3);
    for &b in master_key {
        if b < 0x80 {
            out.push(b);
        } else {
            // (char)(byte)b == 0xFF00 | b, UTF-8: 1110_1111 10_11111x 10_xxxxxx
            let c = 0xFF00u16 | u16::from(b);
            out.extend_from_slice(&[0xEF, 0x80 | ((c >> 6) & 0x3F) as u8, 0x80 | (c & 0x3F) as u8]);
        }
    }
    out
}

/// PBKDF2-HMAC-SHA1 with a 256-bit output.
pub fn pbkdf2_key(password: &[u8], salt: &[u8], rounds: u32) -> [u8; 32] {
    let mut key = [0u8; 32];
    pbkdf2::pbkdf2_hmac::<Sha1>(password, salt, rounds, &mut key);
    key
}

fn cipher(key: &[u8; 32]) -> Aes256 {
    Aes256::new(GenericArray::from_slice(key))
}

fn encrypt_block(c: &Aes256, prev: &mut [u8; BLOCK], block: &mut [u8]) {
    for (b, p) in block.iter_mut().zip(prev.iter()) {
        *b ^= p;
    }
    c.encrypt_block(GenericArray::from_mut_slice(block));
    prev.copy_from_slice(block);
}

fn decrypt_block(c: &Aes256, prev: &mut [u8; BLOCK], block: &mut [u8]) {
    let mut saved = [0u8; BLOCK];
    saved.copy_from_slice(block);
    c.decrypt_block(GenericArray::from_mut_slice(block));
    for (b, p) in block.iter_mut().zip(prev.iter()) {
        *b ^= p;
    }
    *prev = saved;
}

/// Removes PKCS#5 padding from a decrypted last block, returning the number of plaintext bytes in it.
fn unpad_len(block: &[u8]) -> Option<usize> {
    let n = usize::from(block[BLOCK - 1]);
    if n == 0 || n > BLOCK || block[BLOCK - n..].iter().any(|&b| usize::from(b) != n) {
        return None;
    }
    Some(BLOCK - n)
}

/// AES-256-CBC with PKCS#5 padding (whole buffer).
pub fn cbc_encrypt(key: &[u8; 32], iv: &[u8; BLOCK], data: &[u8]) -> Vec<u8> {
    let c = cipher(key);
    let pad = BLOCK - data.len() % BLOCK;
    let mut out = data.to_vec();
    out.resize(data.len() + pad, pad as u8);
    let mut prev = *iv;
    for block in out.chunks_mut(BLOCK) {
        encrypt_block(&c, &mut prev, block);
    }
    out
}

/// Inverse of [`cbc_encrypt`]; `None` if the length or padding is wrong.
pub fn cbc_decrypt(key: &[u8; 32], iv: &[u8; BLOCK], data: &[u8]) -> Option<Vec<u8>> {
    if data.is_empty() || data.len() % BLOCK != 0 {
        return None;
    }
    let c = cipher(key);
    let mut out = data.to_vec();
    let mut prev = *iv;
    for block in out.chunks_mut(BLOCK) {
        decrypt_block(&c, &mut prev, block);
    }
    let keep = unpad_len(&out[out.len() - BLOCK..])?;
    out.truncate(out.len() - BLOCK + keep);
    Some(out)
}

fn take_lv(b: &[u8]) -> Option<(&[u8], &[u8])> {
    let (&len, rest) = b.split_first()?;
    let len = usize::from(len);
    if rest.len() < len {
        return None;
    }
    Some(rest.split_at(len))
}

fn try_master_key(p: &AesParams, password: &str, regime: KdfRegime) -> Option<MasterKey> {
    let user_key = pbkdf2_key(&password_bytes(password, regime), &p.user_salt, p.rounds);
    let iv: [u8; BLOCK] = p.user_iv.as_slice().try_into().ok()?;
    let blob = cbc_decrypt(&user_key, &iv, &p.master_key_blob)?;
    let (data_iv, rest) = take_lv(&blob)?;
    let (mk, rest) = take_lv(rest)?;
    let (ck, _) = take_lv(rest)?;
    let key: [u8; 32] = mk.try_into().ok()?;
    let iv: [u8; BLOCK] = data_iv.try_into().ok()?;
    let expected = pbkdf2_key(&checksum_password_bytes(&key, regime), &p.checksum_salt, p.rounds);
    (ck == expected).then_some(MasterKey { key, iv })
}

/// Recovers the master key, trying the 8-bit regime as well for version 1 backups like Android does.
pub fn decrypt_master_key(p: &AesParams, password: &str, version: u32) -> Result<MasterKey> {
    let regimes: &[KdfRegime] = if version == 1 { &[KdfRegime::Utf8, KdfRegime::EightBit] } else { &[KdfRegime::Utf8] };
    for &regime in regimes {
        if let Some(mk) = try_master_key(p, password, regime) {
            return Ok(mk);
        }
    }
    Err(Error::WrongPassword)
}

/// Builds the blob plaintext `[len, iv, len, key, len, checksum]`.
pub fn master_key_blob_plain(mk: &MasterKey, checksum: &[u8]) -> Vec<u8> {
    let mut blob = Vec::with_capacity(3 + BLOCK + 32 + checksum.len());
    blob.push(BLOCK as u8);
    blob.extend_from_slice(&mk.iv);
    blob.push(32);
    blob.extend_from_slice(&mk.key);
    blob.push(checksum.len() as u8);
    blob.extend_from_slice(checksum);
    blob
}

/// Fresh random salts/keys for a new encrypted backup (UTF-8 regime, valid for every backup version on Android 4.4+).
pub fn new_aes_params(password: &str, rounds: u32) -> (AesParams, MasterKey) {
    let mut user_salt = vec![0u8; SALT_LEN];
    let mut checksum_salt = vec![0u8; SALT_LEN];
    let mut user_iv = [0u8; BLOCK];
    let mut mk = MasterKey { key: [0; 32], iv: [0; BLOCK] };
    random_bytes(&mut user_salt);
    random_bytes(&mut checksum_salt);
    random_bytes(&mut user_iv);
    random_bytes(&mut mk.key);
    random_bytes(&mut mk.iv);
    let params = seal_master_key(&mk, password, user_salt, checksum_salt, user_iv, rounds);
    (params, mk)
}

/// Deterministic part of [`new_aes_params`] (exposed for known-answer tests).
pub fn seal_master_key(
    mk: &MasterKey,
    password: &str,
    user_salt: Vec<u8>,
    checksum_salt: Vec<u8>,
    user_iv: [u8; BLOCK],
    rounds: u32,
) -> AesParams {
    let user_key = pbkdf2_key(&password_bytes(password, KdfRegime::Utf8), &user_salt, rounds);
    let checksum = pbkdf2_key(&checksum_password_bytes(&mk.key, KdfRegime::Utf8), &checksum_salt, rounds);
    let master_key_blob = cbc_encrypt(&user_key, &user_iv, &master_key_blob_plain(mk, &checksum));
    AesParams { user_salt, checksum_salt, rounds, user_iv: user_iv.to_vec(), master_key_blob }
}

const CHUNK: usize = 64 * 1024;

/// Streaming AES-256-CBC/PKCS#5 decryption. The last decrypted block is held back until end of input so the padding
/// can be checked and removed; bad padding or a length that is not a multiple of 16 is an `InvalidData` error.
pub struct CbcDecryptReader<'a> {
    inner: Box<dyn Read + 'a>,
    cipher: Aes256,
    prev: [u8; BLOCK],
    cipher_buf: Vec<u8>,
    held: Option<[u8; BLOCK]>,
    out: Vec<u8>,
    pos: usize,
    done: bool,
}

impl<'a> CbcDecryptReader<'a> {
    /// Wraps `inner` (ciphertext).
    pub fn new(inner: Box<dyn Read + 'a>, mk: &MasterKey) -> Self {
        Self {
            inner,
            cipher: cipher(&mk.key),
            prev: mk.iv,
            cipher_buf: Vec::with_capacity(CHUNK + BLOCK),
            held: None,
            out: Vec::with_capacity(CHUNK + BLOCK),
            pos: 0,
            done: false,
        }
    }

    fn fill(&mut self) -> io::Result<()> {
        self.out.clear();
        self.pos = 0;
        let start = self.cipher_buf.len();
        self.cipher_buf.resize(start + CHUNK, 0);
        let n = self.inner.read(&mut self.cipher_buf[start..])?;
        self.cipher_buf.truncate(start + n);
        if n == 0 {
            self.done = true;
            let bad = |m: &str| io::Error::new(io::ErrorKind::InvalidData, m.to_string());
            if !self.cipher_buf.is_empty() {
                return Err(bad("encrypted backup payload is truncated"));
            }
            let last = self.held.take().ok_or_else(|| bad("encrypted backup payload is empty"))?;
            let keep = unpad_len(&last).ok_or_else(|| bad("bad padding in encrypted backup payload"))?;
            self.out.extend_from_slice(&last[..keep]);
            return Ok(());
        }
        let whole = self.cipher_buf.len() / BLOCK * BLOCK;
        for i in (0..whole).step_by(BLOCK) {
            let mut block = [0u8; BLOCK];
            block.copy_from_slice(&self.cipher_buf[i..i + BLOCK]);
            decrypt_block(&self.cipher, &mut self.prev, &mut block);
            if let Some(h) = self.held.replace(block) {
                self.out.extend_from_slice(&h);
            }
        }
        self.cipher_buf.drain(..whole);
        Ok(())
    }
}

impl Read for CbcDecryptReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        while self.pos == self.out.len() {
            if self.done {
                return Ok(0);
            }
            self.fill()?;
        }
        let n = buf.len().min(self.out.len() - self.pos);
        buf[..n].copy_from_slice(&self.out[self.pos..self.pos + n]);
        self.pos += n;
        Ok(n)
    }
}

/// A writer that can be finished (flushes trailers such as padding or the zlib footer).
pub trait Sink: Write {
    /// Writes any trailer and finishes the inner sinks.
    fn finish(self: Box<Self>) -> io::Result<()>;
}

/// The innermost sink: the output file.
pub struct PlainSink<'a>(pub Box<dyn Write + 'a>);

impl Write for PlainSink<'_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

impl Sink for PlainSink<'_> {
    fn finish(mut self: Box<Self>) -> io::Result<()> {
        self.0.flush()
    }
}

/// Streaming AES-256-CBC/PKCS#5 encryption; [`Sink::finish`] writes the padded last block.
pub struct CbcEncryptWriter<'a> {
    inner: Box<dyn Sink + 'a>,
    cipher: Aes256,
    prev: [u8; BLOCK],
    buf: Vec<u8>,
}

impl<'a> CbcEncryptWriter<'a> {
    /// Encrypts into `inner`.
    pub fn new(inner: Box<dyn Sink + 'a>, mk: &MasterKey) -> Self {
        Self { inner, cipher: cipher(&mk.key), prev: mk.iv, buf: Vec::with_capacity(CHUNK + BLOCK) }
    }
}

impl Write for CbcEncryptWriter<'_> {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        self.buf.extend_from_slice(data);
        let whole = self.buf.len() / BLOCK * BLOCK;
        if whole >= CHUNK {
            for block in self.buf[..whole].chunks_mut(BLOCK) {
                encrypt_block(&self.cipher, &mut self.prev, block);
            }
            self.inner.write_all(&self.buf[..whole])?;
            self.buf.drain(..whole);
        }
        Ok(data.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

impl Sink for CbcEncryptWriter<'_> {
    fn finish(mut self: Box<Self>) -> io::Result<()> {
        let pad = BLOCK - self.buf.len() % BLOCK;
        let len = self.buf.len() + pad;
        self.buf.resize(len, pad as u8);
        for block in self.buf.chunks_mut(BLOCK) {
            encrypt_block(&self.cipher, &mut self.prev, block);
        }
        self.inner.write_all(&self.buf)?;
        self.inner.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk() -> MasterKey {
        MasterKey { key: core::array::from_fn(|i| (i * 9) as u8 ^ 0x85), iv: [7; BLOCK] }
    }

    #[test]
    fn checksum_bytes_follow_java_char_conversion() {
        assert_eq!(
            checksum_password_bytes(&[0x41, 0x80, 0xBF, 0xC0, 0xFF], KdfRegime::Utf8),
            [0x41, 0xEF, 0xBE, 0x80, 0xEF, 0xBE, 0xBF, 0xEF, 0xBF, 0x80, 0xEF, 0xBF, 0xBF]
        );
        // Same as Rust's own UTF-8 encoding of the sign-extended chars.
        let s: String = [0x80u8, 0xFF].iter().map(|&b| char::from_u32(0xFF00 | u32::from(b)).unwrap()).collect();
        assert_eq!(checksum_password_bytes(&[0x80, 0xFF], KdfRegime::Utf8), s.as_bytes());
        assert_eq!(checksum_password_bytes(&[0x80, 1], KdfRegime::EightBit), [0x80, 1]);
        assert_eq!(password_bytes("aĀ", KdfRegime::EightBit), [b'a', 0]);
        assert_eq!(password_bytes("aĀ", KdfRegime::Utf8), "aĀ".as_bytes());
    }

    #[test]
    fn pbkdf2_known_answer() {
        // RFC 6070 test vector 2 (PBKDF2-HMAC-SHA1, "password"/"salt", 2 rounds), first 20 bytes.
        let k = pbkdf2_key(b"password", b"salt", 2);
        assert_eq!(crate::hex::encode_lower(&k[..20]), "ea6c014dc72d6f8ccd1ed92ace1d41f0d8de8957");
    }

    #[test]
    fn aes_cbc_known_answer() {
        // NIST SP 800-38A F.2.5 CBC-AES256.Encrypt, first block (our PKCS#5 adds one padding block).
        let key: [u8; 32] = crate::hex::decode("603deb1015ca71be2b73aef0857d77811f352c073b6108d72d9810a30914dff4")
            .unwrap()
            .try_into()
            .unwrap();
        let iv: [u8; 16] = crate::hex::decode("000102030405060708090a0b0c0d0e0f").unwrap().try_into().unwrap();
        let pt = crate::hex::decode("6bc1bee22e409f96e93d7e117393172a").unwrap();
        let ct = cbc_encrypt(&key, &iv, &pt);
        assert_eq!(crate::hex::encode_lower(&ct[..16]), "f58c4c04d6e5f1ba779eabfb5f7bfbd6");
        assert_eq!(cbc_decrypt(&key, &iv, &ct).unwrap(), pt);
    }

    #[test]
    fn cbc_decrypt_rejects_bad_input() {
        let k = mk();
        assert!(cbc_decrypt(&k.key, &k.iv, &[]).is_none());
        assert!(cbc_decrypt(&k.key, &k.iv, &[0; 15]).is_none());
        let mut ct = cbc_encrypt(&k.key, &k.iv, b"hello");
        let last = ct.len() - 1;
        ct[last] ^= 1;
        assert!(cbc_decrypt(&k.key, &k.iv, &ct).is_none());
        assert_eq!(unpad_len(&[0; 16]), None);
        assert_eq!(unpad_len(&[17; 16]), None);
        assert_eq!(unpad_len(&[16; 16]), Some(0));
    }

    #[test]
    fn master_key_round_trip_and_wrong_password() {
        let (p, key) = new_aes_params("pässword", 50);
        assert_eq!(p.user_salt.len(), SALT_LEN);
        assert_eq!(p.master_key_blob.len(), 96);
        assert_eq!(decrypt_master_key(&p, "pässword", 5).unwrap(), key);
        assert!(matches!(decrypt_master_key(&p, "nope", 5), Err(Error::WrongPassword)));
        assert!(matches!(decrypt_master_key(&p, "nope", 1), Err(Error::WrongPassword)));
    }

    fn params_with_blob(plain: &[u8], regime: KdfRegime) -> AesParams {
        let user_key = pbkdf2_key(&password_bytes("pw", regime), b"salt", 3);
        AesParams {
            user_salt: b"salt".to_vec(),
            checksum_salt: b"cks".to_vec(),
            rounds: 3,
            user_iv: vec![1; 16],
            master_key_blob: cbc_encrypt(&user_key, &[1; 16], plain),
        }
    }

    #[test]
    fn eight_bit_regime_only_for_version_1() {
        let k = mk();
        let ck = pbkdf2_key(&checksum_password_bytes(&k.key, KdfRegime::EightBit), b"cks", 3);
        let p = params_with_blob(&master_key_blob_plain(&k, &ck), KdfRegime::EightBit);
        assert_eq!(decrypt_master_key(&p, "pw", 1).unwrap(), k);
        assert!(matches!(decrypt_master_key(&p, "pw", 2), Err(Error::WrongPassword)));
    }

    #[test]
    fn implausible_blobs_are_wrong_password() {
        let k = mk();
        let ck = pbkdf2_key(&checksum_password_bytes(&k.key, KdfRegime::Utf8), b"cks", 3);
        let good = master_key_blob_plain(&k, &ck);
        assert_eq!(decrypt_master_key(&params_with_blob(&good, KdfRegime::Utf8), "pw", 5).unwrap(), k);
        let mut wrong_ck = good.clone();
        wrong_ck[60] ^= 1;
        let mut short_iv = good.clone();
        short_iv[0] = 15;
        let mut long_len = good.clone();
        long_len[0] = 200;
        let mut short_key = good.clone();
        short_key[17] = 31;
        for blob in [wrong_ck, short_iv, long_len, short_key, vec![], vec![0], vec![0, 0]] {
            let p = params_with_blob(&blob, KdfRegime::Utf8);
            assert!(matches!(decrypt_master_key(&p, "pw", 5), Err(Error::WrongPassword)), "{blob:?}");
        }
        let mut bad_iv = params_with_blob(&good, KdfRegime::Utf8);
        bad_iv.user_iv = vec![1; 3];
        assert!(matches!(decrypt_master_key(&bad_iv, "pw", 5), Err(Error::WrongPassword)));
    }

    struct Collect(std::rc::Rc<std::cell::RefCell<Vec<u8>>>);
    impl Write for Collect {
        fn write(&mut self, b: &[u8]) -> io::Result<usize> {
            self.0.borrow_mut().extend_from_slice(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    fn stream_encrypt(data: &[u8], piece: usize) -> Vec<u8> {
        let out = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut w = Box::new(CbcEncryptWriter::new(Box::new(PlainSink(Box::new(Collect(out.clone())))), &mk()));
        for c in data.chunks(piece.max(1)) {
            w.write_all(c).unwrap();
        }
        w.flush().unwrap();
        w.finish().unwrap();
        let v = out.borrow().clone();
        v
    }

    /// Reader returning at most `n` bytes per call, to exercise partial blocks.
    struct Trickle<'a>(&'a [u8], usize);
    impl Read for Trickle<'_> {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            let n = buf.len().min(self.1).min(self.0.len());
            buf[..n].copy_from_slice(&self.0[..n]);
            self.0 = &self.0[n..];
            Ok(n)
        }
    }

    #[test]
    fn streaming_matches_whole_buffer() {
        let k = mk();
        for len in [0usize, 1, 15, 16, 17, 100_000, 200_003] {
            let data: Vec<u8> = (0..len).map(|i| (i * 31 % 251) as u8).collect();
            let whole = cbc_encrypt(&k.key, &k.iv, &data);
            assert_eq!(stream_encrypt(&data, 7000), whole);
            for trickle in [5usize, 1 << 20] {
                let mut r = CbcDecryptReader::new(Box::new(Trickle(&whole, trickle)), &k);
                let mut back = Vec::new();
                r.read_to_end(&mut back).unwrap();
                assert_eq!(back, data);
                assert_eq!(r.read(&mut [0; 4]).unwrap(), 0);
            }
        }
    }

    #[test]
    fn streaming_decrypt_errors() {
        let k = mk();
        let read = |d: &[u8]| {
            let mut r = CbcDecryptReader::new(Box::new(Trickle(d, 1000)), &k);
            let mut v = Vec::new();
            r.read_to_end(&mut v).unwrap_err().kind()
        };
        assert_eq!(read(&[]), io::ErrorKind::InvalidData);
        assert_eq!(read(&[0; 20]), io::ErrorKind::InvalidData);
        let mut ct = cbc_encrypt(&k.key, &k.iv, b"abc");
        ct[15] ^= 0x55;
        assert_eq!(read(&ct), io::ErrorKind::InvalidData);
    }
}
