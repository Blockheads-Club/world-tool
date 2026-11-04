//! Dynamic objects: the `dw` database. Key `"<macro x>_<macro y>/<object type>"`, value a property list
//! `{dynamicObjects: [ {…}, … ]}` with one dictionary per object (trees, plants, animals, workbenches, chests, signs,
//! doors, items lying on the ground…). Common keys of an object dictionary: `uniqueID`, `pos_x`, `pos_y` (tile),
//! `floatPos`.

use std::collections::BTreeMap;

use plist::{Dictionary, Value};

use crate::error::{Error, Result};
use crate::stored::{self, Encoding};

/// One object, with where it is stored.
#[derive(Debug, Clone, PartialEq)]
pub struct DynObject {
    /// Object type (the number after `/` in the key).
    pub kind: u32,
    /// Index in its record's `dynamicObjects` array.
    pub index: usize,
    /// Key of its record.
    pub key: String,
    /// The object's dictionary.
    pub dict: Dictionary,
}

impl DynObject {
    /// `uniqueID`, if present.
    pub fn unique_id(&self) -> Option<u64> {
        self.dict.get("uniqueID").and_then(Value::as_unsigned_integer)
    }

    /// Tile position (`pos_x`, `pos_y`), if present.
    pub fn pos(&self) -> Option<(i64, i64)> {
        Some((self.dict.get("pos_x")?.as_signed_integer()?, self.dict.get("pos_y")?.as_signed_integer()?))
    }
}

#[derive(Debug, Clone)]
struct Record {
    raw: Vec<u8>,
    parsed: Option<(Vec<Dictionary>, Encoding)>,
    dirty: bool,
}

/// Parses a `dw` key into (macro x, macro y, object type).
pub fn parse_key(key: &str) -> Option<(u32, u32, u32)> {
    let (pos, kind) = key.split_once('/')?;
    let (x, y) = bhw_save::blocks::parse_block_key(pos.as_bytes())?;
    Some((x, y, kind.parse().ok()?))
}

fn parse_record(raw: &[u8]) -> Result<(Vec<Dictionary>, Encoding)> {
    let (v, enc) = stored::decode(raw)?;
    let list = v
        .as_dictionary()
        .and_then(|d| d.get("dynamicObjects"))
        .and_then(Value::as_array)
        .ok_or_else(|| Error::Format("dw record without dynamicObjects".into()))?;
    let dicts = list.iter().map(|o| o.as_dictionary().cloned().unwrap_or_default()).collect();
    Ok((dicts, enc))
}

/// The dynamic objects of a world.
#[derive(Debug, Clone, Default)]
pub struct Objects {
    records: BTreeMap<String, Record>,
    /// Records whose key is not valid UTF-8; written back unchanged.
    foreign: Vec<(Vec<u8>, Vec<u8>)>,
}

impl Objects {
    /// Objects over the records of a `dw` database.
    pub fn new(entries: &[(Vec<u8>, Vec<u8>)]) -> Objects {
        let mut o = Objects::default();
        for (k, v) in entries {
            match String::from_utf8(k.clone()) {
                Ok(key) => {
                    o.records.insert(key, Record { raw: v.clone(), parsed: None, dirty: false });
                }
                Err(_) => o.foreign.push((k.clone(), v.clone())),
            }
        }
        o
    }

    fn parsed(&mut self, key: &str) -> Result<&mut (Vec<Dictionary>, Encoding)> {
        let r = self.records.get_mut(key).ok_or_else(|| Error::Invalid(format!("no dw record {key}")))?;
        if r.parsed.is_none() {
            r.parsed = Some(parse_record(&r.raw)?);
        }
        Ok(r.parsed.as_mut().expect("parsed"))
    }

    /// All objects of all readable records (damaged records are skipped and their keys returned).
    pub fn all(&mut self) -> (Vec<DynObject>, Vec<String>) {
        let keys: Vec<String> = self.records.keys().cloned().collect();
        let (mut out, mut bad) = (Vec::new(), Vec::new());
        for key in keys {
            let kind = parse_key(&key).map_or(u32::MAX, |k| k.2);
            match self.parsed(&key) {
                Ok((dicts, _)) => out.extend(dicts.iter().enumerate().map(|(index, d)| DynObject {
                    kind,
                    index,
                    key: key.clone(),
                    dict: d.clone(),
                })),
                Err(_) => bad.push(key),
            }
        }
        (out, bad)
    }

    /// Objects whose tile position is inside `x0..x1` × `y0..y1`.
    pub fn in_area(&mut self, x0: i64, y0: i64, x1: i64, y1: i64) -> Vec<DynObject> {
        self.all()
            .0
            .into_iter()
            .filter(|o| o.pos().is_some_and(|(x, y)| x >= x0 && x < x1 && y >= y0 && y < y1))
            .collect()
    }

    /// Replaces the dictionary of one object.
    pub fn set(&mut self, key: &str, index: usize, dict: Dictionary) -> Result<()> {
        let (dicts, _) = self.parsed(key)?;
        let slot = dicts.get_mut(index).ok_or_else(|| Error::Invalid(format!("no object {index} in {key}")))?;
        *slot = dict;
        self.records.get_mut(key).expect("present").dirty = true;
        Ok(())
    }

    /// Removes one object (the record is removed when it becomes empty).
    pub fn remove(&mut self, key: &str, index: usize) -> Result<Dictionary> {
        let (dicts, _) = self.parsed(key)?;
        if index >= dicts.len() {
            return Err(Error::Invalid(format!("no object {index} in {key}")));
        }
        let d = dicts.remove(index);
        self.records.get_mut(key).expect("present").dirty = true;
        Ok(d)
    }

    /// Inserts an object into the record `key` (created if missing) at `index` (clamped).
    pub fn insert(&mut self, key: &str, index: usize, dict: Dictionary) -> Result<()> {
        if !self.records.contains_key(key) {
            self.records.insert(
                key.to_string(),
                Record { raw: Vec::new(), parsed: Some((Vec::new(), Encoding::CLIENT)), dirty: true },
            );
        }
        let (dicts, _) = self.parsed(key)?;
        let at = index.min(dicts.len());
        dicts.insert(at, dict);
        self.records.get_mut(key).expect("present").dirty = true;
        Ok(())
    }

    /// True if a record changed.
    pub fn is_dirty(&self) -> bool {
        self.records.values().any(|r| r.dirty)
    }

    /// All records as they are now (changed ones re-encoded, emptied ones left out).
    pub fn entries(&mut self) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
        let mut out = Vec::new();
        let mut gone = Vec::new();
        for (k, r) in self.records.iter_mut() {
            if r.dirty {
                let (dicts, enc) = r.parsed.as_ref().expect("dirty records are parsed");
                if dicts.is_empty() {
                    gone.push(k.clone());
                    continue;
                }
                let mut root = Dictionary::new();
                root.insert(
                    "dynamicObjects".into(),
                    Value::Array(dicts.iter().cloned().map(Value::Dictionary).collect()),
                );
                r.raw = stored::encode(&Value::Dictionary(root), *enc)?;
                r.dirty = false;
            }
            out.push((k.as_bytes().to_vec(), r.raw.clone()));
        }
        for k in gone {
            self.records.remove(&k);
        }
        out.extend(self.foreign.iter().cloned());
        Ok(out)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub fn obj(id: u64, x: i64, y: i64) -> Value {
        let mut d = Dictionary::new();
        d.insert("uniqueID".into(), Value::Integer(id.into()));
        d.insert("pos_x".into(), Value::Integer(x.into()));
        d.insert("pos_y".into(), Value::Integer(y.into()));
        Value::Dictionary(d)
    }

    pub fn record(objs: Vec<Value>) -> Vec<u8> {
        let mut root = Dictionary::new();
        root.insert("dynamicObjects".into(), Value::Array(objs));
        stored::encode(&Value::Dictionary(root), Encoding::CLIENT).unwrap()
    }

    #[test]
    fn objects() {
        assert_eq!(parse_key("3_16/45"), Some((3, 16, 45)));
        assert_eq!(parse_key("3_16"), None);
        assert_eq!(parse_key("3_16/x"), None);
        let entries = vec![
            (b"0_16/46".to_vec(), record(vec![obj(7, 5, 520), obj(8, 40, 530), Value::Integer(1.into())])),
            (b"1_16/4".to_vec(), b"garbage".to_vec()),
            (vec![0xff], b"x".to_vec()),
        ];
        let mut o = Objects::new(&entries);
        let (all, bad) = o.all();
        assert_eq!(all.len(), 3);
        assert_eq!(bad, vec!["1_16/4".to_string()]);
        assert_eq!(all[0].kind, 46);
        assert_eq!(all[0].unique_id(), Some(7));
        assert_eq!(all[1].pos(), Some((40, 530)));
        assert_eq!(all[2].pos(), None);
        assert_eq!(o.in_area(0, 500, 32, 600).len(), 1);
        assert!(!o.is_dirty());
        // unchanged records keep their bytes
        assert_eq!(o.entries().unwrap(), entries);

        let mut d = all[0].dict.clone();
        d.insert("pos_x".into(), Value::Integer(6.into()));
        o.set("0_16/46", 0, d).unwrap();
        assert!(o.set("0_16/46", 9, Dictionary::new()).is_err());
        assert!(o.set("nope", 0, Dictionary::new()).is_err());
        assert!(o.is_dirty());
        assert!(o.remove("0_16/46", 9).is_err());
        assert_eq!(o.remove("0_16/46", 2).unwrap(), Dictionary::new());
        o.insert("2_16/46", 5, obj(9, 70, 520).into_dictionary().unwrap()).unwrap();
        o.insert("2_16/46", 0, obj(10, 71, 520).into_dictionary().unwrap()).unwrap();
        let out = o.entries().unwrap();
        assert!(!o.is_dirty());
        assert_eq!(out.len(), 4);
        let mut o2 = Objects::new(&out);
        let (all2, _) = o2.all();
        assert_eq!(all2.iter().filter_map(DynObject::unique_id).collect::<Vec<_>>(), vec![7, 8, 10, 9]);
        assert_eq!(all2[0].pos(), Some((6, 520)));
        // emptying a record removes it
        o2.remove("2_16/46", 0).unwrap();
        o2.remove("2_16/46", 0).unwrap();
        assert_eq!(o2.entries().unwrap().len(), 3);
        assert!(parse_record(&stored::encode(&Value::Integer(1.into()), Encoding::CLIENT).unwrap()).is_err());
    }
}
