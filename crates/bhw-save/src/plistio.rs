//! Property list helpers: parsing, and writing in the two XML dialects found in saves.
//!
//! Game clients write Apple-style XML plists (`-//Apple//DTD PLIST 1.0//EN`, tab indentation). Blockheads Server
//! writes GNUstep-style XML (`-//GNUstep//DTD plist 0.9//EN`, 4-space indentation with every 8 spaces turned into
//! a tab, no trailing newline). Both programs read either dialect.

use std::io::Cursor;

use base64::Engine;
use plist::{Dictionary, Value};

use crate::error::{Error, Result};

/// The XML flavour of a stored plist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dialect {
    /// Apple CoreFoundation style (written by game clients).
    Apple,
    /// GNUstep style (written by Blockheads Server).
    GnuStep,
}

impl Dialect {
    /// Guesses the dialect of serialized plist bytes (GNUstep if its DOCTYPE says so, Apple otherwise).
    pub fn detect(bytes: &[u8]) -> Dialect {
        let head = &bytes[..bytes.len().min(256)];
        if head.windows(7).any(|w| w == b"GNUstep") {
            Dialect::GnuStep
        } else {
            Dialect::Apple
        }
    }
}

/// Parses a plist in any format (XML, binary, ASCII).
pub fn parse(bytes: &[u8]) -> Result<Value> {
    Ok(Value::from_reader(Cursor::new(bytes))?)
}

/// Parses a plist whose root must be a dictionary.
pub fn parse_dict(bytes: &[u8]) -> Result<Dictionary> {
    match parse(bytes)? {
        Value::Dictionary(d) => Ok(d),
        _ => Err(Error::Format("property list root is not a dictionary".into())),
    }
}

/// Serializes `value` as XML in the given dialect.
pub fn to_xml(value: &Value, dialect: Dialect) -> Result<Vec<u8>> {
    match dialect {
        Dialect::Apple => {
            let mut out = Vec::new();
            value.to_writer_xml(&mut out)?;
            Ok(out)
        }
        Dialect::GnuStep => to_gnustep_xml(value),
    }
}

/// Serializes `value` as GNUstep XML (the format of Blockheads Server's `worldv2` file and database values).
pub fn to_gnustep_xml(value: &Value) -> Result<Vec<u8>> {
    let mut s = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE plist PUBLIC \"-//GNUstep//DTD plist 0.9//EN\" \"http://www.gnustep.org/plist-0_9.xml\">\n\
         <plist version=\"0.9\">\n",
    );
    gs_value(&mut s, value, 0)?;
    s.push_str("</plist>");
    Ok(s.into_bytes())
}

fn gs_indent(s: &mut String, level: usize) {
    let spaces = level * 4;
    for _ in 0..spaces / 8 {
        s.push('\t');
    }
    for _ in 0..spaces % 8 {
        s.push(' ');
    }
}

fn gs_escape(s: &mut String, text: &str) {
    for c in text.chars() {
        match c {
            '&' => s.push_str("&amp;"),
            '<' => s.push_str("&lt;"),
            '>' => s.push_str("&gt;"),
            c => s.push(c),
        }
    }
}

fn gs_real(v: f64) -> String {
    if v.is_nan() {
        "nan".into()
    } else if v.is_infinite() {
        if v > 0.0 { "+infinity" } else { "-infinity" }.into()
    } else {
        format!("{v}")
    }
}

fn gs_value(s: &mut String, value: &Value, level: usize) -> Result<()> {
    gs_indent(s, level);
    match value {
        Value::Dictionary(d) => {
            s.push_str("<dict>\n");
            let mut keys: Vec<&String> = d.keys().collect();
            keys.sort();
            for k in keys {
                gs_indent(s, level + 1);
                s.push_str("<key>");
                gs_escape(s, k);
                s.push_str("</key>\n");
                gs_value(s, &d[k], level + 1)?;
            }
            gs_indent(s, level);
            s.push_str("</dict>\n");
        }
        Value::Array(a) => {
            s.push_str("<array>\n");
            for v in a {
                gs_value(s, v, level + 1)?;
            }
            gs_indent(s, level);
            s.push_str("</array>\n");
        }
        Value::Boolean(b) => s.push_str(if *b { "<true/>\n" } else { "<false/>\n" }),
        Value::Data(d) => {
            s.push_str("<data>\n");
            s.push_str(&base64::engine::general_purpose::STANDARD.encode(d));
            s.push_str("</data>\n");
        }
        Value::Date(d) => {
            s.push_str("<date>");
            s.push_str(&d.to_xml_format());
            s.push_str("</date>\n");
        }
        Value::Integer(i) => {
            s.push_str("<integer>");
            s.push_str(&i.to_string());
            s.push_str("</integer>\n");
        }
        Value::Real(r) => {
            s.push_str("<real>");
            s.push_str(&gs_real(*r));
            s.push_str("</real>\n");
        }
        Value::String(t) => {
            s.push_str("<string>");
            gs_escape(s, t);
            s.push_str("</string>\n");
        }
        other => return Err(Error::Format(format!("cannot write {other:?} in an XML property list"))),
    }
    Ok(())
}

/// Seconds since the Unix epoch of a plist date.
pub fn unix_seconds(d: &plist::Date) -> i64 {
    let t: std::time::SystemTime = (*d).into();
    match t.duration_since(std::time::UNIX_EPOCH) {
        Ok(after) => after.as_secs() as i64,
        Err(before) => -(before.duration().as_secs_f64().ceil() as i64),
    }
}

/// Inflates `bytes` if they are a gzip stream, otherwise returns them unchanged.
pub fn maybe_gunzip(bytes: &[u8]) -> Result<Vec<u8>> {
    if bytes.starts_with(&[0x1f, 0x8b]) {
        let mut out = Vec::new();
        std::io::Read::read_to_end(&mut flate2::read::GzDecoder::new(bytes), &mut out)
            .map_err(|e| Error::Format(format!("gzip: {e}")))?;
        Ok(out)
    } else {
        Ok(bytes.to_vec())
    }
}

/// Gzip-compresses like the server's `-[NSData gzipDeflate]` (default level, no name, mtime 0).
pub fn gzip(bytes: &[u8]) -> Vec<u8> {
    let mut gz = flate2::GzBuilder::new().operating_system(3).write(Vec::new(), flate2::Compression::default());
    std::io::Write::write_all(&mut gz, bytes).expect("writing to a Vec cannot fail");
    gz.finish().expect("writing to a Vec cannot fail")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Value {
        let mut inner = Dictionary::new();
        inner.insert("b".into(), Value::Real(2700.0));
        inner.insert("a".into(), Value::Real(0.25));
        let mut d = Dictionary::new();
        d.insert("worldName".into(), Value::String("A & <B>".into()));
        d.insert("expertMode".into(), Value::Boolean(false));
        d.insert("on".into(), Value::Boolean(true));
        d.insert("n".into(), Value::Integer(1100.into()));
        d.insert("data".into(), Value::Data(vec![1, 2, 3]));
        d.insert("date".into(), Value::Date(plist::Date::from_xml_format("2026-10-17T19:03:40Z").unwrap()));
        d.insert("arr".into(), Value::Array(vec![Value::Dictionary(inner), Value::Array(vec![])]));
        Value::Dictionary(d)
    }

    #[test]
    fn gnustep_layout() {
        let xml = String::from_utf8(to_gnustep_xml(&sample()).unwrap()).unwrap();
        let expected = [
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
            "<!DOCTYPE plist PUBLIC \"-//GNUstep//DTD plist 0.9//EN\" \"http://www.gnustep.org/plist-0_9.xml\">",
            "<plist version=\"0.9\">",
            "<dict>",
            "    <key>arr</key>",
            "    <array>",
            "\t<dict>",
            "\t    <key>a</key>",
            "\t    <real>0.25</real>",
            "\t    <key>b</key>",
            "\t    <real>2700</real>",
            "\t</dict>",
            "\t<array>",
            "\t</array>",
            "    </array>",
            "    <key>data</key>",
            "    <data>",
            "AQID</data>",
            "    <key>date</key>",
            "    <date>2026-10-17T19:03:40Z</date>",
            "    <key>expertMode</key>",
            "    <false/>",
            "    <key>n</key>",
            "    <integer>1100</integer>",
            "    <key>on</key>",
            "    <true/>",
            "    <key>worldName</key>",
            "    <string>A &amp; &lt;B&gt;</string>",
            "</dict>",
            "</plist>",
        ]
        .join("\n");
        assert_eq!(xml, expected);
        // both dialects parse back to the same value
        assert_eq!(parse(xml.as_bytes()).unwrap(), sample());
        let apple = to_xml(&sample(), Dialect::Apple).unwrap();
        assert_eq!(parse(&apple).unwrap(), sample());
        assert_eq!(Dialect::detect(&apple), Dialect::Apple);
        assert_eq!(Dialect::detect(xml.as_bytes()), Dialect::GnuStep);
        assert_eq!(to_xml(&sample(), Dialect::GnuStep).unwrap(), xml.as_bytes());
    }

    #[test]
    fn reals_and_errors() {
        assert_eq!(gs_real(f64::NAN), "nan");
        assert_eq!(gs_real(f64::INFINITY), "+infinity");
        assert_eq!(gs_real(f64::NEG_INFINITY), "-infinity");
        assert_eq!(gs_real(18726.25390625), "18726.25390625");
        assert!(to_gnustep_xml(&Value::Uid(plist::Uid::new(1))).is_err());
        assert!(parse_dict(b"<plist><array/></plist>").is_err());
        assert!(parse(b"garbage").is_err());
        assert_eq!(parse_dict(&to_gnustep_xml(&sample()).unwrap()).unwrap().len(), 7);
    }

    #[test]
    fn dates_and_gzip() {
        let d = plist::Date::from_xml_format("1970-01-01T00:00:10Z").unwrap();
        assert_eq!(unix_seconds(&d), 10);
        let d = plist::Date::from_xml_format("1969-12-31T23:59:50Z").unwrap();
        assert_eq!(unix_seconds(&d), -10);
        let z = gzip(b"hello");
        assert_eq!(z[9], 3);
        assert_eq!(maybe_gunzip(&z).unwrap(), b"hello");
        assert_eq!(maybe_gunzip(b"plain").unwrap(), b"plain");
        assert!(maybe_gunzip(&[0x1f, 0x8b, 0]).is_err());
    }
}
