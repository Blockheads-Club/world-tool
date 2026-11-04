//! How a property-list record is stored: plain or gzip, Apple or GNUstep XML (or binary). Decoding remembers the
//! encoding so a changed record is written back the same way.

use std::io::{Read, Write};

use bhw_save::plistio::{self, Dialect};
use plist::Value;

use crate::error::{Error, Result};

/// The encoding of one stored property list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Encoding {
    /// The bytes are a gzip stream.
    pub gzip: bool,
    /// The plist was binary (`bplist00`) rather than XML.
    pub binary: bool,
    /// XML dialect (ignored for binary plists).
    pub dialect: Dialect,
}

impl Encoding {
    /// Apple XML, not compressed: what game clients write.
    pub const CLIENT: Encoding = Encoding { gzip: false, binary: false, dialect: Dialect::Apple };
}

/// True if `bytes` start with the gzip magic.
pub fn is_gzip(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0x1f, 0x8b])
}

/// Inflates a gzip stream.
pub fn gunzip(bytes: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    flate2::read::GzDecoder::new(bytes).read_to_end(&mut out).map_err(|e| Error::Format(format!("gzip: {e}")))?;
    Ok(out)
}

/// Compresses with gzip.
pub fn gzip(bytes: &[u8]) -> Vec<u8> {
    let mut gz = flate2::GzBuilder::new().operating_system(3).write(Vec::new(), flate2::Compression::default());
    gz.write_all(bytes).expect("writing to a Vec cannot fail");
    gz.finish().expect("writing to a Vec cannot fail")
}

/// Parses a stored property list and returns it with its encoding.
pub fn decode(bytes: &[u8]) -> Result<(Value, Encoding)> {
    let gz = is_gzip(bytes);
    let body = if gz { gunzip(bytes)? } else { bytes.to_vec() };
    let binary = body.starts_with(b"bplist");
    let value = plistio::parse(&body)?;
    Ok((value, Encoding { gzip: gz, binary, dialect: Dialect::detect(&body) }))
}

/// Serializes a property list with an encoding.
pub fn encode(value: &Value, enc: Encoding) -> Result<Vec<u8>> {
    let body = if enc.binary {
        let mut out = Vec::new();
        value.to_writer_binary(&mut out)?;
        out
    } else {
        plistio::to_xml(value, enc.dialect)?
    };
    Ok(if enc.gzip { gzip(&body) } else { body })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let v = Value::Array(vec![Value::Integer(5.into()), Value::String("a".into())]);
        for gz in [false, true] {
            for binary in [false, true] {
                for dialect in [Dialect::Apple, Dialect::GnuStep] {
                    let enc = Encoding { gzip: gz, binary, dialect };
                    let bytes = encode(&v, enc).unwrap();
                    let (back, e2) = decode(&bytes).unwrap();
                    assert_eq!(back, v);
                    assert_eq!(e2.gzip, gz);
                    assert_eq!(e2.binary, binary);
                    if !binary {
                        assert_eq!(e2.dialect, dialect);
                    }
                }
            }
        }
        assert!(decode(&[0x1f, 0x8b, 1, 2]).is_err());
        assert!(decode(b"nonsense <").is_err());
        assert_eq!(Encoding::CLIENT.dialect, Dialect::Apple);
    }
}
