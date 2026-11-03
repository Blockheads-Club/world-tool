//! AES primitives used by encrypted iOS backups: RFC 3394 key unwrap and AES-256-CBC.

use aes::cipher::generic_array::GenericArray;
use aes::cipher::{BlockDecrypt, BlockEncrypt, KeyInit};
use aes::Aes256;

/// AES block size.
pub const BLOCK: usize = 16;
/// Default initial value of RFC 3394.
const KW_IV: [u8; 8] = [0xA6; 8];

fn cipher(key: &[u8; 32]) -> Aes256 {
    Aes256::new(GenericArray::from_slice(key))
}

/// RFC 3394 AES key unwrap with a 256-bit KEK. `None` if the input has a bad length or the integrity check fails
/// (with keybags: wrong password).
pub fn aes_unwrap(kek: &[u8; 32], wrapped: &[u8]) -> Option<Vec<u8>> {
    if wrapped.len() < 24 || wrapped.len() % 8 != 0 {
        return None;
    }
    let aes = cipher(kek);
    let n = wrapped.len() / 8 - 1;
    let mut a: [u8; 8] = wrapped[..8].try_into().unwrap();
    let mut r: Vec<[u8; 8]> = wrapped[8..].chunks(8).map(|c| c.try_into().unwrap()).collect();
    for j in (0..6u64).rev() {
        for i in (0..n).rev() {
            let t = (n as u64) * j + (i as u64) + 1;
            let mut b = [0u8; 16];
            b[..8].copy_from_slice(&(u64::from_be_bytes(a) ^ t).to_be_bytes());
            b[8..].copy_from_slice(&r[i]);
            let block = GenericArray::from_mut_slice(&mut b);
            aes.decrypt_block(block);
            a.copy_from_slice(&b[..8]);
            r[i].copy_from_slice(&b[8..]);
        }
    }
    (a == KW_IV).then(|| r.concat())
}

/// RFC 3394 AES key wrap with a 256-bit KEK (`plain` must be a multiple of 8 bytes, at least 16). Only needed to
/// build test fixtures; the tool never writes iOS backups.
pub fn aes_wrap(kek: &[u8; 32], plain: &[u8]) -> Vec<u8> {
    assert!(plain.len() >= 16 && plain.len() % 8 == 0, "bad key length");
    let aes = cipher(kek);
    let n = plain.len() / 8;
    let mut a = KW_IV;
    let mut r: Vec<[u8; 8]> = plain.chunks(8).map(|c| c.try_into().unwrap()).collect();
    for j in 0..6u64 {
        for (i, ri) in r.iter_mut().enumerate() {
            let mut b = [0u8; 16];
            b[..8].copy_from_slice(&a);
            b[8..].copy_from_slice(ri);
            aes.encrypt_block(GenericArray::from_mut_slice(&mut b));
            let t = (n as u64) * j + (i as u64) + 1;
            a = (u64::from_be_bytes(b[..8].try_into().unwrap()) ^ t).to_be_bytes();
            ri.copy_from_slice(&b[8..]);
        }
    }
    let mut out = a.to_vec();
    out.extend(r.concat());
    out
}

/// AES-256-CBC decryption without padding removal. `None` if the length is not a multiple of the block size.
pub fn cbc_decrypt(key: &[u8; 32], iv: &[u8; BLOCK], data: &[u8]) -> Option<Vec<u8>> {
    if data.len() % BLOCK != 0 {
        return None;
    }
    let aes = cipher(key);
    let mut prev = *iv;
    let mut out = Vec::with_capacity(data.len());
    for chunk in data.chunks(BLOCK) {
        let mut b: [u8; BLOCK] = chunk.try_into().unwrap();
        aes.decrypt_block(GenericArray::from_mut_slice(&mut b));
        for (x, p) in b.iter_mut().zip(prev) {
            *x ^= p;
        }
        out.extend_from_slice(&b);
        prev.copy_from_slice(chunk);
    }
    Some(out)
}

/// AES-256-CBC encryption without padding (`data` must be block aligned). Test fixtures only.
pub fn cbc_encrypt(key: &[u8; 32], iv: &[u8; BLOCK], data: &[u8]) -> Vec<u8> {
    assert!(data.len() % BLOCK == 0, "unaligned data");
    let aes = cipher(key);
    let mut prev = *iv;
    let mut out = Vec::with_capacity(data.len());
    for chunk in data.chunks(BLOCK) {
        let mut b: [u8; BLOCK] = chunk.try_into().unwrap();
        for (x, p) in b.iter_mut().zip(prev) {
            *x ^= p;
        }
        aes.encrypt_block(GenericArray::from_mut_slice(&mut b));
        out.extend_from_slice(&b);
        prev = b;
    }
    out
}

/// Appends PKCS#7 padding (test fixtures).
pub fn pad(data: &[u8]) -> Vec<u8> {
    let n = BLOCK - data.len() % BLOCK;
    let mut out = data.to_vec();
    out.resize(data.len() + n, n as u8);
    out
}

/// Length of `data` without valid PKCS#7 padding, or `None` if the padding is not valid.
pub fn unpadded_len(data: &[u8]) -> Option<usize> {
    let n = *data.last()? as usize;
    (1..=BLOCK.min(data.len())).contains(&n).then_some(())?;
    data[data.len() - n..].iter().all(|&b| b as usize == n).then(|| data.len() - n)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(s: &str) -> Vec<u8> {
        let s: String = s.chars().filter(|c| !c.is_whitespace()).collect();
        (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
    }

    fn kek() -> [u8; 32] {
        hex("000102030405060708090A0B0C0D0E0F101112131415161718191A1B1C1D1E1F").try_into().unwrap()
    }

    #[test]
    fn rfc3394_4_3_and_4_6() {
        let k = kek();
        let p128 = hex("00112233445566778899AABBCCDDEEFF");
        let c128 = hex("64E8C3F9CE0F5BA2 63E9777905818A2A 93C8191E7D6E8AE7");
        assert_eq!(aes_wrap(&k, &p128), c128);
        assert_eq!(aes_unwrap(&k, &c128).unwrap(), p128);
        let p256 = hex("00112233445566778899AABBCCDDEEFF000102030405060708090A0B0C0D0E0F");
        let c256 = hex("28C9F404C4B810F4 CBCCB35CFB87F826 3F5786E2D80ED326 CBC7F0E71A99F43B FB988B9B7A02DD21");
        assert_eq!(aes_wrap(&k, &p256), c256);
        assert_eq!(aes_unwrap(&k, &c256).unwrap(), p256);
    }

    #[test]
    fn unwrap_rejects_bad_input() {
        let k = kek();
        assert!(aes_unwrap(&k, &[0; 16]).is_none());
        assert!(aes_unwrap(&k, &[0; 25]).is_none());
        let mut c = aes_wrap(&k, &[7; 32]);
        c[3] ^= 1;
        assert!(aes_unwrap(&k, &c).is_none());
    }

    #[test]
    #[should_panic(expected = "bad key length")]
    fn wrap_rejects_short_key() {
        aes_wrap(&kek(), &[0; 8]);
    }

    #[test]
    fn nist_sp800_38a_cbc_aes256() {
        let key: [u8; 32] = hex("603deb1015ca71be2b73aef0857d77811f352c073b6108d72d9810a30914dff4").try_into().unwrap();
        let iv: [u8; 16] = hex("000102030405060708090a0b0c0d0e0f").try_into().unwrap();
        let pt = hex("6bc1bee22e409f96e93d7e117393172a ae2d8a571e03ac9c9eb76fac45af8e51
             30c81c46a35ce411e5fbc1191a0a52ef f69f2445df4f9b17ad2b417be66c3710");
        let ct = hex("f58c4c04d6e5f1ba779eabfb5f7bfbd6 9cfc4e967edb808d679f777bc6702c7d
             39f23369a9d9bacfa530e26304231461 b2eb05e2c39be9fcda6c19078c6a9d1b");
        assert_eq!(cbc_encrypt(&key, &iv, &pt), ct);
        assert_eq!(cbc_decrypt(&key, &iv, &ct).unwrap(), pt);
        assert!(cbc_decrypt(&key, &iv, &ct[..15]).is_none());
    }

    #[test]
    #[should_panic(expected = "unaligned data")]
    fn encrypt_rejects_unaligned() {
        cbc_encrypt(&[0; 32], &[0; 16], &[0; 3]);
    }

    #[test]
    fn padding() {
        assert_eq!(pad(b"abc").len(), 16);
        assert_eq!(pad(&[0; 16]).len(), 32);
        assert_eq!(unpadded_len(&pad(b"abc")), Some(3));
        assert_eq!(unpadded_len(&pad(&[0; 16])), Some(16));
        assert_eq!(unpadded_len(&[]), None);
        assert_eq!(unpadded_len(&[0]), None);
        assert_eq!(unpadded_len(&[3, 3]), None);
        assert_eq!(unpadded_len(&[17; 32]), None);
        assert_eq!(unpadded_len(&[1, 2, 2, 3]), None);
    }
}
