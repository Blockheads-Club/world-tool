//! Hex helpers (Android writes upper-case hex in backup headers).

use crate::{Error, Result};

/// Upper-case hex, like AOSP `byteArrayToHex`.
pub fn encode_upper(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789ABCDEF";
    let mut s = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        s.push(DIGITS[usize::from(b >> 4)] as char);
        s.push(DIGITS[usize::from(b & 15)] as char);
    }
    s
}

/// Lower-case hex.
pub fn encode_lower(bytes: &[u8]) -> String {
    encode_upper(bytes).to_ascii_lowercase()
}

fn nibble(c: u8) -> Result<u8> {
    match c {
        b'0'..=b'9' => Ok(c - b'0'),
        b'a'..=b'f' => Ok(c - b'a' + 10),
        b'A'..=b'F' => Ok(c - b'A' + 10),
        _ => Err(Error::BadHeader(format!("invalid hex digit {:?}", c as char))),
    }
}

/// Decodes hex in either case.
pub fn decode(s: &str) -> Result<Vec<u8>> {
    let b = s.as_bytes();
    if b.len() % 2 != 0 {
        return Err(Error::BadHeader("odd-length hex string".into()));
    }
    let mut out = Vec::with_capacity(b.len() / 2);
    for pair in b.chunks(2) {
        out.push(nibble(pair[0])? << 4 | nibble(pair[1])?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let data = [0u8, 1, 0x7f, 0x80, 0xab, 0xff];
        assert_eq!(encode_upper(&data), "00017F80ABFF");
        assert_eq!(encode_lower(&data), "00017f80abff");
        assert_eq!(decode("00017F80abff").unwrap(), data);
    }

    #[test]
    fn errors() {
        assert!(matches!(decode("abc"), Err(Error::BadHeader(_))));
        assert!(matches!(decode("zz"), Err(Error::BadHeader(_))));
        assert!(matches!(decode("0g"), Err(Error::BadHeader(_))));
    }
}
