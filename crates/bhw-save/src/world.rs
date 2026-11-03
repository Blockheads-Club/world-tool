//! Implementations of the `bhw-world` plug-in traits ([`bhw_world::meta`]) on top of this crate.

use bhw_world::meta::ServerWorldConverter;
use bhw_world::{WorldFile, WorldFiles, WorldMeta, WorldMetaEditor, WorldMetaSource};

use crate::convert::{client_to_server_files, server_to_client_files, ConvertOptions};
use crate::save::{AnySave, FileMap};
use crate::summary::summarize_any;

/// The LMDB-backed world metadata reader, identity editor and server converter.
#[derive(Debug, Clone, Default)]
pub struct SaveMeta {
    /// Options used by the [`ServerWorldConverter`] methods.
    pub convert: ConvertOptions,
}

fn meta_err(e: impl std::fmt::Display) -> bhw_world::Error {
    bhw_world::Error::Meta(e.to_string())
}

/// Loads a [`WorldFiles`] into memory (lock files are skipped).
pub fn to_file_map(world: &WorldFiles) -> bhw_world::Result<FileMap> {
    let mut m = FileMap::new();
    for f in &world.files {
        if f.path == "lock.mdb" || f.path.ends_with("/lock.mdb") {
            continue;
        }
        m.insert(f.path.clone(), f.data.read_all()?);
    }
    Ok(m)
}

/// Builds a [`WorldFiles`] from memory (empty-directory entries are dropped).
pub fn from_file_map(id: &str, files: FileMap) -> bhw_world::Result<WorldFiles> {
    let files = files
        .into_iter()
        .filter(|(p, _)| !p.ends_with('/'))
        .map(|(path, data)| WorldFile { path, data: bhw_world::Payload::Bytes(data) })
        .collect();
    WorldFiles::new(id.to_string(), files)
}

impl WorldMetaSource for SaveMeta {
    fn read_meta(&self, world: &WorldFiles) -> bhw_world::Result<WorldMeta> {
        let save = AnySave::from_files(&world.id, to_file_map(world)?).map_err(meta_err)?;
        let s = summarize_any(&save, None);
        Ok(WorldMeta {
            name: s.name,
            // Saves record only the save format (`saveVersion`), not the game version.
            game_version: None,
            created_at: s.created.map(|t| bhw_world::time::rfc3339(t.max(0) as u64)),
        })
    }
}

impl WorldMetaEditor for SaveMeta {
    fn set_identity(
        &self,
        world: &mut WorldFiles,
        new_id: Option<&str>,
        new_name: Option<&str>,
    ) -> bhw_world::Result<()> {
        let mut m = to_file_map(world)?;
        crate::identity::set_identity(&mut m, new_id, new_name).map_err(meta_err)?;
        for f in &mut world.files {
            if let Some(v) = m.remove(&f.path) {
                f.data = bhw_world::Payload::Bytes(v);
            }
        }
        Ok(())
    }
}

impl ServerWorldConverter for SaveMeta {
    fn server_to_game(&self, world: &WorldFiles) -> bhw_world::Result<WorldFiles> {
        let (out, _) = server_to_client_files(&to_file_map(world)?, &self.convert).map_err(meta_err)?;
        from_file_map(&world.id, out)
    }

    fn game_to_server(&self, world: &WorldFiles) -> bhw_world::Result<WorldFiles> {
        let (out, _) = client_to_server_files(&to_file_map(world)?, &self.convert).map_err(meta_err)?;
        from_file_map(&world.id, out)
    }
}
