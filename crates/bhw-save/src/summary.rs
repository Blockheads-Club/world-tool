//! World summaries: what a world list shows without loading the world.

use std::path::Path;

use bhw_lmdb::EnvContents;
use plist::{Dictionary, Value};

use crate::blocks::{parse_block_key, MACRO_SIZE};
use crate::error::Result;
use crate::gamedb::GameDb;
use crate::plistio::{maybe_gunzip, parse, parse_dict, unix_seconds};
use crate::save::{AnySave, ClientSave, SaveKind, ServerSave};

/// One blockhead found in a save.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct BlockheadInfo {
    /// The player the blockhead belongs to: `None` for the single-player (local) blockheads, otherwise the key
    /// prefix (a player id, or `local` in a client's cache of a multiplayer world).
    pub owner: Option<String>,
    /// Its name.
    pub name: String,
    /// Its unique object id.
    pub unique_id: i64,
}

/// A summary of one world.
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct WorldSummary {
    /// Client or server world.
    pub kind: Option<SaveKind>,
    /// World id (`saveID`; falls back to the folder name).
    pub id: String,
    /// Folder name.
    pub dir_name: String,
    /// Display name (`worldName`).
    pub name: Option<String>,
    /// Creation time (Unix seconds, `creationDate`).
    pub created: Option<i64>,
    /// Last save time (Unix seconds, `saveDate`); the game's "last played".
    pub last_saved: Option<i64>,
    /// Last connection time of a client's cached multiplayer world (`connectionDate`).
    pub last_connected: Option<i64>,
    /// Expert mode.
    pub expert_mode: bool,
    /// Created with custom rules (`customRulesDict` present).
    pub custom_rules: bool,
    /// A client's local cache of a multiplayer world (`remoteGame`): not a playable single-player world.
    pub remote_game: bool,
    /// Server host and port of a cached multiplayer world.
    pub remote_host: Option<String>,
    /// Stored in the cloud (`cloudGame`).
    pub cloud_game: bool,
    /// World width in macro blocks (`worldWidthMacro`: 32 = 1/16, 128 = 1/4, 512 = 1×, 2048 = 4×, 8192 = 16×).
    pub world_width_macro: Option<u32>,
    /// Generation seed (`randomSeed`).
    pub seed: Option<i64>,
    /// Save format version (`saveVersion`, 1100 for game 1.7).
    pub save_version: Option<i64>,
    /// In-game time (`worldTime`).
    pub world_time: Option<f64>,
    /// Spawn portal tile position.
    pub start_portal: Option<(i64, i64)>,
    /// The game's own thumbnail (`previewImageData`) is stored.
    pub has_preview_image: bool,
    /// Blockheads found in `main` (single-player and per-player records).
    pub blockheads: Vec<BlockheadInfo>,
    /// Number of players in a server's `server_db` (players that ever connected); `None` for client worlds.
    pub player_count: Option<usize>,
    /// Number of stored macro blocks.
    pub block_count: usize,
    /// Number of stored dynamic object lists (`dw` records).
    pub dynamic_record_count: usize,
    /// Total size of the folder in bytes.
    pub size_on_disk: u64,
    /// Whether the world is listed in the given `game_db` (`None` when no `game_db` was given or for server
    /// worlds).
    pub registered_in_game_db: Option<bool>,
    /// Problems noticed while reading (missing records, undecodable values).
    pub problems: Vec<String>,
}

impl WorldSummary {
    /// World width in tiles.
    pub fn world_width_tiles(&self) -> Option<u32> {
        self.world_width_macro.map(|w| w * MACRO_SIZE)
    }
}

/// Reads a summary of any world folder. `game_db` (the client's global database) is used for client worlds to
/// report registration and as a fallback source of the world record.
pub fn summarize(dir: &Path, game_db: Option<&GameDb>) -> Result<WorldSummary> {
    Ok(summarize_any(&AnySave::open(dir)?, game_db))
}

/// Summary of an opened world of either kind.
pub fn summarize_any(save: &AnySave, game_db: Option<&GameDb>) -> WorldSummary {
    match save {
        AnySave::Client(c) => summarize_client(c, game_db),
        AnySave::Server(s) => summarize_server(s),
    }
}

/// Summary of an opened client world.
pub fn summarize_client(save: &ClientSave, game_db: Option<&GameDb>) -> WorldSummary {
    let mut s = base(&save.dir_name, save.size, &save.world_db, SaveKind::Client);
    let mut record = save.worldv2().map(<[u8]>::to_vec);
    if record.is_none() {
        s.problems.push("no main/worldv2 record".into());
        record = game_db.and_then(|g| g.entry(&s.id)).map(<[u8]>::to_vec);
    }
    if let Some(r) = record {
        apply_worldv2(&mut s, &r);
    }
    if let Some(g) = game_db {
        s.registered_in_game_db = Some(g.entry(&s.id).is_some());
    }
    s
}

/// Summary of an opened server world.
pub fn summarize_server(save: &ServerSave) -> WorldSummary {
    let mut s = base(&save.dir_name, save.size, &save.world_db, SaveKind::Server);
    match save.worldv2() {
        Some(r) => apply_worldv2(&mut s, r),
        None => s.problems.push("no worldv2 record or file".into()),
    }
    s.player_count = Some(
        save.server_db
            .as_ref()
            .and_then(|e| e.named.get(&b"main"[..]))
            .map_or(0, |d| d.entries.iter().filter(|(k, _)| k.ends_with(b"_info")).count()),
    );
    s
}

fn base(dir_name: &str, size: u64, world_db: &EnvContents, kind: SaveKind) -> WorldSummary {
    let mut s = WorldSummary {
        kind: Some(kind),
        id: dir_name.to_string(),
        dir_name: dir_name.to_string(),
        size_on_disk: size,
        ..Default::default()
    };
    if let Some(b) = world_db.named.get(&b"blocks"[..]) {
        s.block_count = b.entries.iter().filter(|(k, _)| parse_block_key(k).is_some()).count();
    }
    s.dynamic_record_count = world_db.named.get(&b"dw"[..]).map_or(0, |d| d.entries.len());
    if let Some(main) = world_db.named.get(&b"main"[..]) {
        for (k, v) in &main.entries {
            let owner = if k == b"blockheads" {
                None
            } else if let Some(p) = k.strip_suffix(b"_blockheads") {
                Some(String::from_utf8_lossy(p).into_owned())
            } else {
                continue;
            };
            match blockheads_in(v) {
                Ok(list) => s.blockheads.extend(list.into_iter().map(|(name, unique_id)| BlockheadInfo {
                    owner: owner.clone(),
                    name,
                    unique_id,
                })),
                Err(e) => s.problems.push(format!("main/{}: {e}", String::from_utf8_lossy(k))),
            }
        }
    }
    s
}

/// `(name, uniqueID)` of every object in a `{dynamicObjects: [...]}` record (plain or gzip).
pub fn blockheads_in(value: &[u8]) -> Result<Vec<(String, i64)>> {
    let v = parse(&maybe_gunzip(value)?)?;
    let objs = v.as_dictionary().and_then(|d| d.get("dynamicObjects")).and_then(Value::as_array);
    Ok(objs
        .into_iter()
        .flatten()
        .filter_map(Value::as_dictionary)
        .map(|o| {
            let name = o.get("name").and_then(Value::as_string).unwrap_or_default().to_string();
            let id = o.get("uniqueID").and_then(Value::as_signed_integer).unwrap_or_default();
            (name, id)
        })
        .collect())
}

fn apply_worldv2(s: &mut WorldSummary, record: &[u8]) {
    let d = match parse_dict(record) {
        Ok(d) => d,
        Err(e) => {
            s.problems.push(format!("worldv2: {e}"));
            return;
        }
    };
    if let Some(id) = str_of(&d, "saveID") {
        s.id = id;
    }
    s.name = str_of(&d, "worldName");
    s.created = date_of(&d, "creationDate");
    s.last_saved = date_of(&d, "saveDate");
    s.last_connected = date_of(&d, "connectionDate");
    s.expert_mode = bool_of(&d, "expertMode");
    s.custom_rules = d.contains_key("customRulesDict");
    s.remote_game = bool_of(&d, "remoteGame");
    s.cloud_game = bool_of(&d, "cloudGame");
    if s.remote_game {
        s.remote_host = str_of(&d, "host").map(|h| match str_of(&d, "port") {
            Some(p) => format!("{h}:{p}"),
            None => h,
        });
    }
    s.world_width_macro = int_of(&d, "worldWidthMacro").and_then(|w| u32::try_from(w).ok());
    s.seed = int_of(&d, "randomSeed");
    s.save_version = int_of(&d, "saveVersion");
    s.world_time = d.get("worldTime").and_then(Value::as_real);
    s.start_portal = int_of(&d, "startPortalPos.x").zip(int_of(&d, "startPortalPos.y"));
    s.has_preview_image = d.get("previewImageData").and_then(Value::as_data).is_some_and(|p| !p.is_empty());
}

fn str_of(d: &Dictionary, k: &str) -> Option<String> {
    d.get(k).and_then(Value::as_string).map(str::to_string)
}

fn date_of(d: &Dictionary, k: &str) -> Option<i64> {
    d.get(k).and_then(Value::as_date).map(|t| unix_seconds(&t))
}

fn bool_of(d: &Dictionary, k: &str) -> bool {
    d.get(k).and_then(Value::as_boolean).unwrap_or(false)
}

fn int_of(d: &Dictionary, k: &str) -> Option<i64> {
    match d.get(k)? {
        Value::Integer(i) => i.as_signed(),
        Value::Real(r) => Some(*r as i64),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}
