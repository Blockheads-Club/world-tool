//! World identity: id (`saveID`) and display name (`worldName`).
//!
//! Where they are stored:
//! * the folder name `saves/<id>/` (both kinds);
//! * `world_db` → `main` → `worldv2`: `saveID` and `worldName` (both kinds);
//! * Blockheads Server only: the `worldv2` file (`saveID`, `worldName`);
//! * game clients only: `game_db` → `main` → `<id>_worldv2`, a copy of the world record (see [`crate::gamedb`]).
//!
//! No other record contains the id (`blocks`, `dw`, `server_db`, `lightBlocks` keys use macro positions and
//! player ids).

use bhw_lmdb::EnvContents;
use plist::Value;

use crate::error::{Error, Result};
use crate::plistio::{parse_dict, to_xml, Dialect};
use crate::save::{env_bytes, env_from, get, is_world_id, put, FileMap, WORLD_DB};

/// Rewrites `saveID` and/or `worldName` in a serialized world record.
///
/// The record is edited in place as text: only the two `<string>` values change and every other byte stays as the
/// game wrote it. Re-serializing the whole property list is not safe: the game (1.7.6 on Android) crashes loading a
/// world whose record was rewritten by a different XML writer (seen with unwrapped `<data>` lines). Records where
/// a key is missing or not a plain `<string>` fall back to parsing and re-serializing in the record's dialect.
pub fn rewrite_record(record: &[u8], new_id: Option<&str>, new_name: Option<&str>) -> Result<Vec<u8>> {
    let mut out = Some(record.to_vec());
    for (key, value) in [("saveID", new_id), ("worldName", new_name)] {
        if let Some(v) = value {
            out = out.and_then(|r| replace_string(&r, key, v));
        }
    }
    let wanted = |d: &plist::Dictionary| {
        [("saveID", new_id), ("worldName", new_name)]
            .iter()
            .all(|(k, v)| v.is_none_or(|v| d.get(k).and_then(Value::as_string) == Some(v)))
    };
    if let Some(r) = out {
        if parse_dict(&r).is_ok_and(|d| wanted(&d)) {
            return Ok(r);
        }
    }
    let mut d = parse_dict(record)?;
    if let Some(id) = new_id {
        d.insert("saveID".into(), Value::String(id.into()));
    }
    if let Some(name) = new_name {
        d.insert("worldName".into(), Value::String(name.into()));
    }
    to_xml(&Value::Dictionary(d), Dialect::detect(record))
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

/// Replaces the text of the `<string>` that follows `<key>key</key>` (first occurrence).
fn replace_string(xml: &[u8], key: &str, value: &str) -> Option<Vec<u8>> {
    let k = format!("<key>{key}</key>");
    let after_key = find(xml, k.as_bytes())? + k.len();
    let ws = xml[after_key..].iter().take_while(|b| b.is_ascii_whitespace()).count();
    let start = after_key + ws + b"<string>".len();
    if !xml[after_key + ws..].starts_with(b"<string>") {
        return None;
    }
    let end = start + find(&xml[start..], b"</string>")?;
    let escaped = value.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    let mut out = xml[..start].to_vec();
    out.extend_from_slice(escaped.as_bytes());
    out.extend_from_slice(&xml[end..]);
    Some(out)
}

/// Rewrites `main/worldv2` of a `world_db`. False if there is no such record.
pub fn set_identity_env(world_db: &mut EnvContents, new_id: Option<&str>, new_name: Option<&str>) -> Result<bool> {
    let Some(old) = get(world_db, "main", "worldv2") else { return Ok(false) };
    let new = rewrite_record(old, new_id, new_name)?;
    put(world_db, "main", b"worldv2", new);
    Ok(true)
}

/// Sets the id and/or name of a world held in memory (either kind): `main/worldv2` in `world_db/data.mdb`
/// (re-encoded in its original layout) and, if present, the server's `worldv2` file. The folder name and a
/// client's `game_db` entry are the caller's business ([`crate::gamedb::GameDb::register`]).
pub fn set_identity(files: &mut FileMap, new_id: Option<&str>, new_name: Option<&str>) -> Result<()> {
    if let Some(id) = new_id {
        if !is_world_id(id) {
            return Err(Error::Format(format!("invalid world id {id:?}")));
        }
    }
    let (mut env, layout) = env_from(files, WORLD_DB)?.ok_or_else(|| Error::Format(format!("no {WORLD_DB}")))?;
    let in_db = set_identity_env(&mut env, new_id, new_name)?;
    let file = match files.get("worldv2") {
        Some(f) => Some(rewrite_record(f, new_id, new_name)?),
        None => None,
    };
    if !in_db && file.is_none() {
        return Err(Error::Format("the world has no worldv2 record".into()));
    }
    if in_db {
        files.insert(WORLD_DB.into(), env_bytes(&env, layout)?);
    }
    if let Some(f) = file {
        files.insert("worldv2".into(), f);
    }
    Ok(())
}
