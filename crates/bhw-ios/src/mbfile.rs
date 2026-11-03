//! The `file` column of `Manifest.db`: an `NSKeyedArchiver` binary plist of an `MBFile` object.

use std::io::Cursor;

use plist::{Dictionary, Value};

use crate::{Error, Result};

/// Fields of an `MBFile` this tool needs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MbFile {
    /// Plain file size in bytes (encrypted files are padded; the plain text is trimmed to this).
    pub size: u64,
    /// Modification time, Unix seconds.
    pub mtime: i64,
    /// Data protection class (selects the class key that wraps [`MbFile::wrapped_key`]).
    pub protection_class: u32,
    /// Wrapped per-file key (the `EncryptionKey` data without its 4-byte class prefix); encrypted backups only.
    pub wrapped_key: Option<Vec<u8>>,
}

fn bad(msg: &str) -> Error {
    Error::Corrupt(format!("MBFile: {msg}"))
}

/// Follows a `CF$UID` reference into `$objects`; other values are returned as they are.
fn resolve<'a>(objects: &'a [Value], v: &'a Value) -> Result<&'a Value> {
    match v {
        Value::Uid(uid) => objects.get(uid.get() as usize).ok_or_else(|| bad("dangling UID")),
        other => Ok(other),
    }
}

fn int(d: &Dictionary, key: &str) -> i64 {
    d.get(key).and_then(Value::as_signed_integer).unwrap_or(0)
}

impl MbFile {
    /// Decodes an archived `MBFile`.
    pub fn parse(blob: &[u8]) -> Result<MbFile> {
        let v = Value::from_reader(Cursor::new(blob)).map_err(|e| Error::Plist(format!("MBFile: {e}")))?;
        let top = v.as_dictionary().ok_or_else(|| bad("not a dictionary"))?;
        let objects = top.get("$objects").and_then(Value::as_array).ok_or_else(|| bad("no $objects"))?;
        let root_ref = top
            .get("$top")
            .and_then(Value::as_dictionary)
            .and_then(|t| t.get("root"))
            .ok_or_else(|| bad("no $top.root"))?;
        let root = resolve(objects, root_ref)?.as_dictionary().ok_or_else(|| bad("root is not a dictionary"))?;
        let wrapped_key = match root.get("EncryptionKey") {
            None => None,
            Some(k) => {
                let k = resolve(objects, k)?;
                let data = match k {
                    Value::Dictionary(d) => d.get("NS.data").and_then(Value::as_data),
                    other => other.as_data(),
                }
                .ok_or_else(|| bad("EncryptionKey has no data"))?;
                if data.len() < 4 {
                    return Err(bad("EncryptionKey too short"));
                }
                Some(data[4..].to_vec())
            }
        };
        Ok(MbFile {
            size: int(root, "Size").max(0) as u64,
            mtime: int(root, "LastModified"),
            protection_class: int(root, "ProtectionClass") as u32,
            wrapped_key,
        })
    }
}

/// Archives an `MBFile` the way iOS does (test fixtures).
#[cfg(any(test, feature = "test-util"))]
pub fn build(file: &MbFile, rel_path: &str) -> Vec<u8> {
    use plist::Uid;
    let mut root = Dictionary::new();
    root.insert("$class".into(), Value::Uid(Uid::new(3)));
    root.insert("RelativePath".into(), Value::Uid(Uid::new(2)));
    root.insert("Size".into(), Value::Integer(file.size.into()));
    root.insert("LastModified".into(), Value::Integer(file.mtime.into()));
    root.insert("ProtectionClass".into(), Value::Integer(file.protection_class.into()));
    root.insert("Mode".into(), Value::Integer(0o100644.into()));
    let mut objects = vec![Value::String("$null".into()), Value::Dictionary(root), Value::String(rel_path.into())];
    objects.push(Value::Dictionary([("$classname".to_string(), Value::String("MBFile".into()))].into_iter().collect()));
    if let Some(k) = &file.wrapped_key {
        let mut data = file.protection_class.to_le_bytes().to_vec();
        data.extend_from_slice(k);
        let mut kd = Dictionary::new();
        kd.insert("NS.data".into(), Value::Data(data));
        objects.push(Value::Dictionary(kd));
        if let Value::Dictionary(r) = &mut objects[1] {
            r.insert("EncryptionKey".into(), Value::Uid(Uid::new(4)));
        }
    }
    let mut top = Dictionary::new();
    top.insert("$archiver".into(), Value::String("NSKeyedArchiver".into()));
    top.insert("$version".into(), Value::Integer(100000.into()));
    top.insert("$top".into(), Value::Dictionary([("root".to_string(), Value::Uid(Uid::new(1)))].into_iter().collect()));
    top.insert("$objects".into(), Value::Array(objects));
    let mut out = Vec::new();
    Value::Dictionary(top).to_writer_binary(&mut out).unwrap();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use plist::Uid;

    fn archive(objects: Vec<Value>, top: Option<Value>) -> Vec<u8> {
        let mut d = Dictionary::new();
        d.insert("$objects".into(), Value::Array(objects));
        if let Some(t) = top {
            d.insert("$top".into(), Value::Dictionary([("root".to_string(), t)].into_iter().collect()));
        }
        let mut out = Vec::new();
        Value::Dictionary(d).to_writer_binary(&mut out).unwrap();
        out
    }

    fn root(items: Vec<(&str, Value)>) -> Value {
        Value::Dictionary(items.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
    }

    #[test]
    fn round_trip() {
        let f = MbFile { size: 1234, mtime: 1_700_000_000, protection_class: 3, wrapped_key: Some(vec![7; 40]) };
        assert_eq!(MbFile::parse(&build(&f, "a/b")).unwrap(), f);
        let plain = MbFile { size: 5, mtime: 1, protection_class: 0, wrapped_key: None };
        assert_eq!(MbFile::parse(&build(&plain, "a")).unwrap(), plain);
    }

    #[test]
    fn inline_key_data_and_defaults() {
        let mut key = 2u32.to_le_bytes().to_vec();
        key.extend([1, 2]);
        let blob = archive(
            vec![root(vec![("EncryptionKey", Value::Data(key)), ("Size", Value::Integer((-1).into()))])],
            Some(Value::Uid(Uid::new(0))),
        );
        let f = MbFile::parse(&blob).unwrap();
        assert_eq!(f.wrapped_key, Some(vec![1, 2]));
        assert_eq!(f.size, 0);
        // Root given inline (no UID).
        assert_eq!(MbFile::parse(&archive(vec![], Some(root(vec![])))).unwrap(), MbFile::default());
    }

    #[test]
    fn errors() {
        let corrupt = |blob: &[u8]| matches!(MbFile::parse(blob), Err(Error::Corrupt(_)));
        assert!(matches!(MbFile::parse(b"nope"), Err(Error::Plist(_))));
        let mut arr = Vec::new();
        Value::Array(vec![]).to_writer_binary(&mut arr).unwrap();
        assert!(corrupt(&arr));
        let mut no_objects = Vec::new();
        Value::Dictionary(Dictionary::new()).to_writer_binary(&mut no_objects).unwrap();
        assert!(corrupt(&no_objects));
        assert!(corrupt(&archive(vec![], None)));
        assert!(corrupt(&archive(vec![], Some(Value::Uid(Uid::new(5))))));
        assert!(corrupt(&archive(vec![Value::Integer(1.into())], Some(Value::Uid(Uid::new(0))))));
        let with_key = |k: Value| archive(vec![root(vec![("EncryptionKey", k)])], Some(Value::Uid(Uid::new(0))));
        assert!(corrupt(&with_key(Value::Integer(1.into()))));
        assert!(corrupt(&with_key(root(vec![]))));
        assert!(corrupt(&with_key(Value::Data(vec![1, 2]))));
    }
}
