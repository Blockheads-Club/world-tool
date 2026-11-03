//! Conversion between game client worlds and Blockheads Server worlds.
//!
//! Both programs share the `world_db` schema (`blocks`, `dw`, `main`), so the records are copied unchanged and
//! only the LMDB layout and map size change (client: 32-bit, 64 MiB; server: 64-bit, 1 GiB). What has no place in
//! the target format is put aside in a sub-folder that the target program ignores ([`CLIENT_EXTRAS`] inside a
//! server world, [`SERVER_EXTRAS`] inside a client world) and restored when converting back, so a round trip
//! loses nothing. Everything that the target cannot use is listed in the [`ConversionReport`].

use std::path::Path;

use bhw_lmdb::{Database, EnvContents, Layout};
use plist::{Dictionary, Value};

use crate::error::{io, Error, Result};
use crate::identity::rewrite_record;
use crate::plistio::{maybe_gunzip, parse_dict, to_gnustep_xml};
use crate::save::{
    env_bytes, get, map_size_for, put, read_dir_files, write_dir_files, ClientSave, FileMap, ServerSave,
    CLIENT_WORLD_MAP_SIZE, LIGHT_BLOCKS, PAGE_SIZE, SERVER_DB, SERVER_DB_MAP_SIZE, SERVER_WORLD_MAP_SIZE, WORLD_DB,
};
use crate::summary::blockheads_in;

/// Folder inside a server world that holds the files of the client world it was made from.
pub const CLIENT_EXTRAS: &str = "bhw-client-files";
/// Folder inside a client world that holds the files of the server world it was made from.
pub const SERVER_EXTRAS: &str = "bhw-server-files";

/// Keys of the server's `worldv2` file, taken from the world record.
const SERVER_FILE_KEYS: [&str; 9] = [
    "creationDate",
    "customRulesDict",
    "expertMode",
    "migrationComplete_1.7",
    "saveDate",
    "saveID",
    "saveVersion",
    "worldName",
    "worldWidthMacro",
];

/// World record keys only game clients write; Blockheads Server drops them when it saves the world.
pub const CLIENT_ONLY_RECORD_KEYS: [&str; 6] =
    ["previewImageData", "blockheadDatasv2", "tutorial", "cloudGame", "isOwner", "playerID"];

/// Options for both directions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConvertOptions {
    /// Keep files the target format has no place for in [`CLIENT_EXTRAS`] / [`SERVER_EXTRAS`] (default). When
    /// false they are left out and reported as lost (use this when writing to a device).
    pub keep_extras: bool,
    /// Server → client: give the single-player world the blockheads (and inventories) the server stored for this
    /// player id (`<id>_blockheads`), replacing the world's `blockheads` record. The per-player records stay too.
    pub adopt_player: Option<String>,
}

impl Default for ConvertOptions {
    fn default() -> ConvertOptions {
        ConvertOptions { keep_extras: true, adopt_player: None }
    }
}

/// How a note affects the data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum NoteKind {
    /// Informational (data kept and usable, or a behaviour of the target program).
    Info,
    /// Data kept aside in the extras folder: not used by the target program, restored by the reverse conversion.
    SetAside,
    /// Data not present in the result.
    Lost,
}

/// One remark of a conversion.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Note {
    /// Kind.
    pub kind: NoteKind,
    /// English description.
    pub message: String,
}

/// What a conversion did with data that does not map one-to-one.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ConversionReport {
    /// Remarks in the order they were made.
    pub notes: Vec<Note>,
}

impl ConversionReport {
    fn add(&mut self, kind: NoteKind, message: impl Into<String>) {
        self.notes.push(Note { kind, message: message.into() });
    }

    /// True if any data is missing from the result.
    pub fn has_losses(&self) -> bool {
        self.notes.iter().any(|n| n.kind == NoteKind::Lost)
    }
}

fn is_lock(path: &str) -> bool {
    path == "lock.mdb" || path.ends_with("/lock.mdb")
}

fn record_dict(record: Option<&[u8]>) -> Result<Dictionary> {
    parse_dict(record.ok_or_else(|| Error::Unsupported("the world has no main/worldv2 record".into()))?)
}

/// Splits `other` into files restored from `extras_dir` (prefix removed) and the rest.
fn take_extras(other: &FileMap, extras_dir: &str) -> (FileMap, FileMap) {
    let prefix = format!("{extras_dir}/");
    let mut restored = FileMap::new();
    let mut rest = FileMap::new();
    for (p, v) in other {
        if is_lock(p) {
            continue;
        }
        match p.strip_prefix(&prefix) {
            Some(inner) => restored.insert(inner.to_string(), v.clone()),
            None => rest.insert(p.clone(), v.clone()),
        };
    }
    (restored, rest)
}

/// Puts `rest` into the extras folder (or reports it lost), with one note per top-level entry.
fn stash(out: &mut FileMap, rest: FileMap, extras_dir: &str, opts: &ConvertOptions, report: &mut ConversionReport) {
    let mut tops: Vec<String> = Vec::new();
    for (p, v) in rest {
        let top = p.split('/').next().expect("split yields one item").to_string();
        if !tops.contains(&top) {
            tops.push(top);
        }
        if opts.keep_extras {
            out.insert(format!("{extras_dir}/{p}"), v);
        }
    }
    for top in tops {
        if opts.keep_extras {
            report.add(NoteKind::SetAside, format!("{top} kept in {extras_dir}/ (not used by the target program)"));
        } else {
            report.add(NoteKind::Lost, format!("{top} left out (the target format has no place for it)"));
        }
    }
}

fn empty_env(map_size: u64) -> EnvContents {
    let mut e = EnvContents::new(PAGE_SIZE, map_size);
    e.named.insert(b"main".to_vec(), Database::new(0));
    e
}

fn sized(env: &EnvContents, min: u64) -> EnvContents {
    let mut e = env.clone();
    let data: usize = e.named.values().flat_map(|d| d.entries.iter()).map(|(k, v)| k.len() + v.len()).sum();
    e.map_size = map_size_for(data, min);
    e
}

/// The `worldv2` file of a server world made from a world record.
pub fn server_world_file(record: &Dictionary) -> Result<Vec<u8>> {
    let mut d = Dictionary::new();
    for k in SERVER_FILE_KEYS {
        if let Some(v) = record.get(k) {
            d.insert(k.into(), v.clone());
        }
    }
    d.insert("migrationComplete_1.7".into(), Value::Boolean(true));
    to_gnustep_xml(&Value::Dictionary(d))
}

/// Client world → Blockheads Server world, in memory.
pub fn client_to_server_files(files: &FileMap, opts: &ConvertOptions) -> Result<(FileMap, ConversionReport)> {
    let save = ClientSave::from_files("", files.clone())?;
    let record = record_dict(save.worldv2())?;
    if record.get("remoteGame").and_then(Value::as_boolean) == Some(true) {
        return Err(Error::Unsupported(
            "this is the game's cache of a multiplayer world, not a single-player world; it holds only what the \
             player saw, and the server keeps the real world"
                .into(),
        ));
    }
    let mut report = ConversionReport::default();
    let mut out = FileMap::new();
    out.insert(WORLD_DB.into(), env_bytes(&sized(&save.world_db, SERVER_WORLD_MAP_SIZE), Layout::Bits64)?);

    let (restored, rest) = take_extras(&save.other_files, SERVER_EXTRAS);
    if !restored.is_empty() {
        report.add(NoteKind::Info, format!("restored the server files kept in {SERVER_EXTRAS}/"));
    }
    out.extend(restored);
    stash(&mut out, rest, CLIENT_EXTRAS, opts, &mut report);

    let id = record.get("saveID").and_then(Value::as_string).map(str::to_string);
    let name = record.get("worldName").and_then(Value::as_string).map(str::to_string);
    match out.get("worldv2") {
        Some(f) => {
            let fd = parse_dict(f)?;
            let differs = |k: &str, v: &Option<String>| fd.get(k).and_then(Value::as_string) != v.as_deref();
            if differs("saveID", &id) || differs("worldName", &name) {
                let new = rewrite_record(f, id.as_deref(), name.as_deref())?;
                out.insert("worldv2".into(), new);
            }
        }
        None => {
            out.insert("worldv2".into(), server_world_file(&record)?);
        }
    }
    if !out.contains_key(SERVER_DB) {
        out.insert(SERVER_DB.into(), env_bytes(&empty_env(SERVER_DB_MAP_SIZE), Layout::Bits64)?);
    }
    if !out.contains_key(LIGHT_BLOCKS) {
        out.insert(LIGHT_BLOCKS.into(), env_bytes(&empty_env(SERVER_WORLD_MAP_SIZE), Layout::Bits64)?);
    }

    let local = get(&save.world_db, "main", "blockheads").map(blockheads_in).transpose()?.unwrap_or_default();
    if !local.is_empty() {
        report.add(
            NoteKind::Info,
            format!(
                "the {} single-player blockhead(s) stay in the `blockheads` record; the server loads them as \
                 blockheads without a player, and players who join get their own",
                local.len()
            ),
        );
    }
    let client_keys: Vec<&str> = CLIENT_ONLY_RECORD_KEYS.into_iter().filter(|k| record.contains_key(k)).collect();
    if !client_keys.is_empty() {
        report.add(
            NoteKind::Info,
            format!(
                "world record keys only the game uses ({}) are copied, but the server drops them when it saves",
                client_keys.join(", ")
            ),
        );
    }
    Ok((out, report))
}

/// Blockheads Server world → client world, in memory.
pub fn server_to_client_files(files: &FileMap, opts: &ConvertOptions) -> Result<(FileMap, ConversionReport)> {
    let save = ServerSave::from_files("", files.clone())?;
    let record = record_dict(get(&save.world_db, "main", "worldv2")).map_err(|_| {
        Error::Unsupported("the server world was never saved by the server (no main/worldv2); run it once".into())
    })?;
    let mut report = ConversionReport::default();
    let mut world_db = save.world_db.clone();

    if let Some(player) = &opts.adopt_player {
        adopt(&mut world_db, player, &mut report)?;
    }
    let mut out = FileMap::new();
    out.insert(WORLD_DB.into(), env_bytes(&sized(&world_db, CLIENT_WORLD_MAP_SIZE), Layout::Bits32)?);

    let (restored, rest) = take_extras(&save.other_files, CLIENT_EXTRAS);
    if !restored.is_empty() {
        report.add(NoteKind::Info, format!("restored the client files kept in {CLIENT_EXTRAS}/"));
    }
    out.extend(restored);
    if !out.keys().any(|k| k.starts_with("map/")) {
        out.insert("map/default/".into(), Vec::new());
    }
    if let Some(sdb) = &save.server_db {
        let players =
            sdb.named.get(&b"main"[..]).map_or(0, |d| d.entries.iter().filter(|(k, _)| k.ends_with(b"_info")).count());
        if players > 0 {
            report.add(NoteKind::Info, format!("{players} player record(s) in server_db are not used by the game"));
        }
    }
    stash(&mut out, rest, SERVER_EXTRAS, opts, &mut report);

    let per_player = save
        .world_db
        .named
        .get(&b"main"[..])
        .map_or(0, |d| d.entries.iter().filter(|(k, _)| k.ends_with(b"_blockheads")).count());
    if per_player > 0 {
        report.add(
            NoteKind::Info,
            format!(
                "blockheads of {per_player} player(s) stay in their per-player records, which single-player games \
                 ignore (see the adopt_player option)"
            ),
        );
    }
    if !record.contains_key("previewImageData") {
        report.add(NoteKind::Info, "no thumbnail is stored; the game makes one when it next saves the world");
    }
    report.add(NoteKind::Info, "register the world in the game's game_db, or the game will not list it");
    Ok((out, report))
}

/// Copies `<player>_blockheads` and its inventories to the single-player records.
fn adopt(world_db: &mut EnvContents, player: &str, report: &mut ConversionReport) -> Result<()> {
    let key = format!("{player}_blockheads");
    let stored = get(world_db, "main", &key)
        .ok_or_else(|| Error::Format(format!("no blockheads stored for player {player:?}")))?
        .to_vec();
    let plain = maybe_gunzip(&stored)?;
    let list = blockheads_in(&plain)?;
    put(world_db, "main", b"blockheads", plain);
    let mut missing = 0;
    for (_, uid) in &list {
        match get(world_db, "main", &format!("{player}_blockhead_{uid}_inventory")) {
            Some(inv) => {
                let inv = maybe_gunzip(inv)?;
                put(world_db, "main", format!("blockhead_{uid}_inventory").as_bytes(), inv);
            }
            None => missing += 1,
        }
    }
    report.add(
        NoteKind::Info,
        format!("the {} blockhead(s) of player {player} are now the single-player blockheads", list.len()),
    );
    if missing > 0 {
        report
            .add(NoteKind::Lost, format!("{missing} of them had no stored inventory; the game skips such blockheads"));
    }
    Ok(())
}

/// Writes a converted world to `dst` (must not exist) through a temporary sibling folder.
fn write_new_dir(dst: &Path, files: &FileMap) -> Result<()> {
    if dst.exists() {
        return Err(Error::Io { path: dst.into(), source: std::io::ErrorKind::AlreadyExists.into() });
    }
    let mut tmp = dst.as_os_str().to_owned();
    tmp.push(".bhw-tmp");
    let tmp = std::path::PathBuf::from(tmp);
    if tmp.exists() {
        std::fs::remove_dir_all(&tmp).map_err(io(&tmp))?;
    }
    write_dir_files(&tmp, files)?;
    std::fs::rename(&tmp, dst).map_err(io(dst))
}

/// Converts the client world folder `src` into a new server world folder `dst`.
pub fn client_to_server(src: &Path, dst: &Path, opts: &ConvertOptions) -> Result<ConversionReport> {
    let (files, report) = client_to_server_files(&read_dir_files(src)?, opts)?;
    write_new_dir(dst, &files)?;
    Ok(report)
}

/// Converts the server world folder `src` into a new client world folder `dst`.
pub fn server_to_client(src: &Path, dst: &Path, opts: &ConvertOptions) -> Result<ConversionReport> {
    let (files, report) = server_to_client_files(&read_dir_files(src)?, opts)?;
    write_new_dir(dst, &files)?;
    Ok(report)
}
