//! World helpers on top of `bhw-save`: summaries for the UI, map previews, client/server conversion and identity.

use base64::Engine;
use bhw_lmdb::Layout;
use bhw_save::convert::{server_to_client_files, ConvertOptions};
use bhw_save::save::detect_kind_files;
use bhw_save::world::{from_file_map, to_file_map, SaveMeta};
use bhw_save::{render_map, summarize_any, AnySave, ClientSave, GameDb, MapOptions, SaveKind};
use bhw_world::{Payload, WorldFiles, WorldMetaEditor};

use crate::error::{AppError, ErrorCode, Result};
use crate::types::{DeviceWorld, GameMode};

/// Widest map preview in pixels.
pub const PREVIEW_PX: u32 = 420;

/// What the UI needs to know about one world.
#[derive(Debug, Clone, PartialEq)]
pub struct WorldInfo {
    /// The world id (`saveID`).
    pub world_id: String,
    pub name: String,
    pub size: u64,
    /// Unix ms.
    pub last_played: Option<u64>,
    /// Unix ms (0 when unknown).
    pub created: u64,
    pub mode: GameMode,
    /// A client's cache of a multiplayer world (not shown in the game's world list).
    pub remote: bool,
    /// Our own map rendering (no game graphics).
    pub preview_png: Option<Vec<u8>>,
}

impl WorldInfo {
    /// As a [`DeviceWorld`] with id `id`.
    pub fn device_world(&self, id: &str, game_version: &str) -> DeviceWorld {
        DeviceWorld {
            id: id.to_string(),
            name: self.name.clone(),
            size: self.size,
            game_version: game_version.to_string(),
            last_played: self.last_played,
            created: self.created,
            mode: self.mode,
            preview: self.preview_png.as_deref().map(data_url),
        }
    }

    /// Creation time as RFC 3339 (for `.bhworld` manifests).
    pub fn created_rfc3339(&self) -> Option<String> {
        (self.created > 0).then(|| bhw_world::time::rfc3339(self.created / 1000))
    }
}

/// A PNG as a `data:` URL.
pub fn data_url(png: &[u8]) -> String {
    format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(png))
}

fn ms(secs: i64) -> u64 {
    secs.max(0) as u64 * 1000
}

/// Parses a world (either kind) and summarises it, rendering a map preview.
pub fn inspect(world: &WorldFiles, game_db: Option<&GameDb>) -> Result<WorldInfo> {
    let size = world.total_size()?;
    let save = AnySave::from_files(&world.id, to_file_map(world)?)?;
    let s = summarize_any(&save, game_db);
    let width = s.world_width_tiles().unwrap_or(0);
    let (img, _) = render_map(save.world_db(), MapOptions::fitting(width, PREVIEW_PX));
    let preview_png = (img.width > 0 && img.height > 0).then(|| img.to_png());
    Ok(WorldInfo {
        world_id: s.id,
        name: s.name.unwrap_or("World".into()),
        size,
        last_played: s.last_saved.map(ms),
        created: s.created.map(ms).unwrap_or(0),
        mode: if s.expert_mode { GameMode::Expert } else { GameMode::Normal },
        remote: s.remote_game,
        preview_png,
    })
}

/// A world in the game client's format: server worlds are converted (`keep_extras`: keep the server-only files
/// in the extras folder, for the library; leave them out when writing to a device).
/// 64-bit LMDB files (iOS 64-bit builds) become 32-bit ones like Android's and Blockheads PC's.
pub fn to_client(world: WorldFiles, keep_extras: bool) -> Result<WorldFiles> {
    let map = to_file_map(&world)?;
    if detect_kind_files(&map) != Some(SaveKind::Server) {
        return to_32bit(world);
    }
    let opts = ConvertOptions { keep_extras, ..ConvertOptions::default() };
    let (out, _) = server_to_client_files(&map, &opts)?;
    Ok(from_file_map(&world.id, out)?)
}

/// Rewrites every 64-bit `data.mdb` of a client world in the 32-bit layout.
pub fn to_32bit(mut world: WorldFiles) -> Result<WorldFiles> {
    for f in world.files.iter_mut().filter(|f| f.path.ends_with("data.mdb")) {
        let data = f.data.read_all()?;
        if Layout::detect(&data) == Some(Layout::Bits64) {
            f.data = Payload::Bytes(bhw_lmdb::convert(&data, Layout::Bits32)?);
        }
    }
    Ok(world)
}

/// Sets the stored id and/or name of a world (in its `world_db`) and the folder id.
pub fn set_identity(world: &mut WorldFiles, new_id: Option<&str>, new_name: Option<&str>) -> Result<()> {
    SaveMeta::default().set_identity(world, new_id, new_name)?;
    if let Some(id) = new_id {
        world.id = id.to_string();
    }
    Ok(())
}

/// Registers a client world in a `game_db` with its own world record.
pub fn register(game_db: &mut GameDb, world: &WorldFiles) -> Result<()> {
    let save = ClientSave::from_files(&world.id, to_file_map(world)?)?;
    let id = game_db.register_save(&save)?;
    if id != world.id {
        return Err(AppError::with(ErrorCode::WorldDamaged, format!("world {} has saveID {id}", world.id)));
    }
    Ok(())
}
