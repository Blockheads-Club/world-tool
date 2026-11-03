//! The backup keybag (`BackupKeyBag` in `Manifest.plist`): a TLV list of class keys wrapped with a key derived from
//! the backup password.

use std::collections::BTreeMap;

use pbkdf2::pbkdf2_hmac;
use sha1::Sha1;
use sha2::Sha256;

use crate::crypto::aes_unwrap;
use crate::{Error, Result};

/// `WRAP` flag: the class key is wrapped with the passcode (password) key.
const WRAP_PASSCODE: u32 = 2;

/// One class key entry as stored in the keybag.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct ClassEntry {
    class: Option<u32>,
    wrap: u32,
    wpky: Vec<u8>,
}

/// A parsed (still locked) keybag.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Keybag {
    /// `SALT` for PBKDF2-SHA1.
    pub salt: Vec<u8>,
    /// `ITER` for PBKDF2-SHA1.
    pub iter: u32,
    /// `DPSL` for the PBKDF2-SHA256 pre-step (iOS 10.2+); absent on older backups.
    pub dpsl: Option<Vec<u8>>,
    /// `DPIC` for the PBKDF2-SHA256 pre-step.
    pub dpic: u32,
    classes: Vec<ClassEntry>,
}

/// Unlocked class keys by protection class.
pub type ClassKeys = BTreeMap<u32, [u8; 32]>;

fn be_u32(v: &[u8]) -> Result<u32> {
    v.try_into().map(u32::from_be_bytes).map_err(|_| Error::Keybag("integer field is not 4 bytes".into()))
}

impl Keybag {
    /// Parses the TLV keybag (`tag[4] len[4 BE] value[len]`...).
    pub fn parse(data: &[u8]) -> Result<Keybag> {
        let mut kb = Keybag::default();
        let mut have_uuid = false;
        let mut cur: Option<ClassEntry> = None;
        let mut pos = 0;
        while pos < data.len() {
            if data.len() - pos < 8 {
                return Err(Error::Keybag("truncated tag header".into()));
            }
            let tag = &data[pos..pos + 4];
            let len = u32::from_be_bytes(data[pos + 4..pos + 8].try_into().unwrap()) as usize;
            pos += 8;
            if data.len() - pos < len {
                return Err(Error::Keybag("truncated value".into()));
            }
            let val = &data[pos..pos + len];
            pos += len;
            match tag {
                b"UUID" if !have_uuid => have_uuid = true,
                b"UUID" => kb.classes.extend(cur.replace(ClassEntry::default())),
                b"CLAS" if cur.is_some() => cur.as_mut().unwrap().class = Some(be_u32(val)?),
                b"WRAP" if cur.is_some() => cur.as_mut().unwrap().wrap = be_u32(val)?,
                b"WPKY" if cur.is_some() => cur.as_mut().unwrap().wpky = val.to_vec(),
                b"SALT" => kb.salt = val.to_vec(),
                b"ITER" => kb.iter = be_u32(val)?,
                b"DPSL" => kb.dpsl = Some(val.to_vec()),
                b"DPIC" => kb.dpic = be_u32(val)?,
                _ => {}
            }
        }
        kb.classes.extend(cur);
        if kb.salt.is_empty() || kb.iter == 0 {
            return Err(Error::Keybag("missing SALT/ITER".into()));
        }
        Ok(kb)
    }

    /// Derives the 32-byte passcode key: PBKDF2-HMAC-SHA256(password, DPSL, DPIC) then PBKDF2-HMAC-SHA1(.., SALT,
    /// ITER) on iOS 10.2+; only the SHA1 step on older keybags.
    pub fn passcode_key(&self, password: &str) -> [u8; 32] {
        let mut step1 = password.as_bytes().to_vec();
        if let Some(dpsl) = &self.dpsl {
            let mut k = [0u8; 32];
            pbkdf2_hmac::<Sha256>(&step1, dpsl, self.dpic, &mut k);
            step1 = k.to_vec();
        }
        let mut key = [0u8; 32];
        pbkdf2_hmac::<Sha1>(&step1, &self.salt, self.iter, &mut key);
        key
    }

    /// Unlocks the class keys with `password`. [`Error::WrongPassword`] if a passcode-wrapped key fails the RFC 3394
    /// integrity check.
    pub fn unlock(&self, password: &str) -> Result<ClassKeys> {
        self.unlock_with_key(&self.passcode_key(password))
    }

    /// Unlocks the class keys with an already derived passcode key.
    pub fn unlock_with_key(&self, key: &[u8; 32]) -> Result<ClassKeys> {
        let mut keys = ClassKeys::new();
        for c in &self.classes {
            let Some(class) = c.class else { continue };
            if c.wrap & WRAP_PASSCODE == 0 {
                continue;
            }
            let k = aes_unwrap(key, &c.wpky).ok_or(Error::WrongPassword)?;
            let k: [u8; 32] = k.try_into().map_err(|_| Error::Keybag(format!("class {class} key is not 32 bytes")))?;
            keys.insert(class, k);
        }
        if keys.is_empty() {
            return Err(Error::Keybag("no password-wrapped class keys".into()));
        }
        Ok(keys)
    }
}

/// Builds a keybag (test fixtures): `classes` are `(class, key)` wrapped with the passcode key of `password`.
#[cfg(any(test, feature = "test-util"))]
pub fn build(password: &str, salt: &[u8], iter: u32, dp: Option<(&[u8], u32)>, classes: &[(u32, [u8; 32])]) -> Vec<u8> {
    fn tlv(out: &mut Vec<u8>, tag: &[u8; 4], v: &[u8]) {
        out.extend_from_slice(tag);
        out.extend_from_slice(&(v.len() as u32).to_be_bytes());
        out.extend_from_slice(v);
    }
    let kb = Keybag {
        salt: salt.to_vec(),
        iter,
        dpsl: dp.map(|d| d.0.to_vec()),
        dpic: dp.map_or(0, |d| d.1),
        classes: vec![],
    };
    let pk = kb.passcode_key(password);
    let mut out = Vec::new();
    tlv(&mut out, b"VERS", &4u32.to_be_bytes());
    tlv(&mut out, b"TYPE", &1u32.to_be_bytes());
    tlv(&mut out, b"UUID", &[0x11; 16]);
    tlv(&mut out, b"WRAP", &0u32.to_be_bytes());
    tlv(&mut out, b"SALT", salt);
    tlv(&mut out, b"ITER", &iter.to_be_bytes());
    if let Some((dpsl, dpic)) = dp {
        tlv(&mut out, b"DPWT", &1u32.to_be_bytes());
        tlv(&mut out, b"DPIC", &dpic.to_be_bytes());
        tlv(&mut out, b"DPSL", dpsl);
    }
    for (class, key) in classes {
        tlv(&mut out, b"UUID", &[*class as u8; 16]);
        tlv(&mut out, b"CLAS", &class.to_be_bytes());
        tlv(&mut out, b"WRAP", &2u32.to_be_bytes());
        tlv(&mut out, b"KTYP", &0u32.to_be_bytes());
        tlv(&mut out, b"WPKY", &crate::crypto::aes_wrap(&pk, key));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tlv(tag: &[u8; 4], v: &[u8]) -> Vec<u8> {
        let mut out = tag.to_vec();
        out.extend_from_slice(&(v.len() as u32).to_be_bytes());
        out.extend_from_slice(v);
        out
    }

    #[test]
    fn round_trip_new_and_old_derivation() {
        let keys = [(1, [1u8; 32]), (3, [3u8; 32])];
        for dp in [Some((&b"dpsl-salt"[..], 3)), None] {
            let kb = Keybag::parse(&build("pw", b"salt", 2, dp, &keys)).unwrap();
            assert_eq!(kb.dpsl.is_some(), dp.is_some());
            let got = kb.unlock("pw").unwrap();
            assert_eq!(got.into_iter().collect::<Vec<_>>(), keys.to_vec());
            assert!(matches!(kb.unlock("nope"), Err(Error::WrongPassword)));
        }
    }

    #[test]
    fn derivation_matches_reference_steps() {
        let kb = Keybag { salt: b"s".to_vec(), iter: 1, dpsl: Some(b"d".to_vec()), dpic: 1, classes: vec![] };
        let mut a = [0u8; 32];
        pbkdf2_hmac::<Sha256>(b"pw", b"d", 1, &mut a);
        let mut b = [0u8; 32];
        pbkdf2_hmac::<Sha1>(&a, b"s", 1, &mut b);
        assert_eq!(kb.passcode_key("pw"), b);
    }

    #[test]
    fn parse_errors() {
        assert!(matches!(Keybag::parse(b"SAL"), Err(Error::Keybag(_))));
        assert!(matches!(Keybag::parse(&tlv(b"SALT", b"x")[..8]), Err(Error::Keybag(_))));
        assert!(matches!(Keybag::parse(&tlv(b"ITER", b"x")), Err(Error::Keybag(_))));
        assert!(matches!(Keybag::parse(&tlv(b"SALT", b"x")), Err(Error::Keybag(_))));
    }

    #[test]
    fn unlock_skips_and_rejects() {
        let mut data = [tlv(b"SALT", b"s"), tlv(b"ITER", &1u32.to_be_bytes()), tlv(b"UUID", &[0; 16])].concat();
        // Class entries before any keybag UUID are ignored; an entry with no class and a non-passcode one are skipped.
        data = [tlv(b"CLAS", &1u32.to_be_bytes()), data].concat();
        data.extend(tlv(b"UUID", &[1; 16]));
        data.extend(tlv(b"WRAP", &2u32.to_be_bytes()));
        data.extend(tlv(b"UUID", &[2; 16]));
        data.extend(tlv(b"CLAS", &2u32.to_be_bytes()));
        data.extend(tlv(b"WRAP", &1u32.to_be_bytes()));
        let kb = Keybag::parse(&data).unwrap();
        assert!(matches!(kb.unlock("x"), Err(Error::Keybag(_))));

        // A passcode-wrapped key of the wrong size.
        let pk = kb.passcode_key("x");
        data.extend(tlv(b"UUID", &[3; 16]));
        data.extend(tlv(b"CLAS", &3u32.to_be_bytes()));
        data.extend(tlv(b"WRAP", &2u32.to_be_bytes()));
        data.extend(tlv(b"WPKY", &crate::crypto::aes_wrap(&pk, &[9; 16])));
        let kb = Keybag::parse(&data).unwrap();
        assert!(matches!(kb.unlock("x"), Err(Error::Keybag(m)) if m.contains("class 3")));
    }
}
