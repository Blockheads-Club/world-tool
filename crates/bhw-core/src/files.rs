//! Worlds as files on this computer: importing `.bhworld` files and world folders into the library, exporting
//! library worlds as `.bhworld` files or Blockheads Server world folders.

use std::path::{Path, PathBuf};

use bhw_save::convert::{client_to_server_files, ConvertOptions};
use bhw_save::save::{detect_kind_files, write_dir_files};
use bhw_save::world::to_file_map;
use bhw_save::SaveKind;
use bhw_world::bhworld::{open_bhworld, Limits};
use bhw_world::library::slug;
use bhw_world::meta::new_world_id_excluding;
use bhw_world::{unique_name, SourceKind, WorldFiles};

use crate::activity::Entry;
use crate::error::{AppError, ErrorCode, Result};
use crate::library::{origin_of, AddMeta};
use crate::types::{
    ActivityKind, Choice, ConflictKind, ExportFormat, ExportRequest, ExportResult, ExportedFile, ImportItem,
    ImportKind, ImportRequest, ImportStatus, Origin, Progress, Stage, World,
};
use crate::util::file_name;
use crate::worlds::{inspect, set_identity, to_client, WorldInfo};
use crate::Core;

/// A world loaded from a file or folder, in client format.
#[derive(Debug, Clone)]
pub struct Loaded {
    pub path: String,
    pub world: WorldFiles,
    pub info: WorldInfo,
    pub kind: ImportKind,
    pub game_version: Option<String>,
    pub origin: Origin,
    pub source_kind: SourceKind,
}

const PLACEHOLDER_ID: &str = "00000000000000000000000000000000";

/// Loads a `.bhworld` or a world folder (Blockheads Server or game); server worlds are converted.
pub fn load_path(path: &Path, keep_extras: bool) -> Result<Loaded> {
    let shown = path.display().to_string();
    if path.is_dir() {
        let world = WorldFiles::load_dir(path, PLACEHOLDER_ID.into())?;
        let server = detect_kind_files(&to_file_map(&world)?) == Some(SaveKind::Server);
        let mut world = to_client(world, keep_extras)?;
        let info = inspect(&world, None)?;
        world.id = if bhw_save::save::is_world_id(&info.world_id) {
            info.world_id.clone()
        } else {
            new_world_id_excluding(&[])
        };
        let info = WorldInfo { world_id: world.id.clone(), ..info };
        return Ok(Loaded {
            path: shown,
            world,
            info,
            kind: ImportKind::Server,
            game_version: None,
            origin: Origin::File,
            source_kind: if server { SourceKind::BlockheadsServer } else { SourceKind::Unknown },
        });
    }
    if !path.extension().is_some_and(|e| e.eq_ignore_ascii_case("bhworld")) {
        return Err(
            AppError::with(ErrorCode::WorldDamaged, "not a .bhworld file or world folder").name(file_name(path))
        );
    }
    let b = open_bhworld(path, &Limits::default()).map_err(|e| AppError::from(e).name(file_name(path)))?;
    let mut info = inspect(&b.world, None)?;
    info.name = b.manifest.world.name.clone().unwrap_or(info.name);
    Ok(Loaded {
        path: shown,
        world: b.world,
        info,
        kind: ImportKind::Bhworld,
        game_version: b.manifest.world.game_version,
        origin: origin_of(b.manifest.source.kind),
        source_kind: b.manifest.source.kind,
    })
}

/// `dir/<stem><ext>`, or `dir/<stem> (2)<ext>`, ... when taken.
fn free_path(dir: &Path, stem: &str, ext: &str) -> PathBuf {
    let mut p = dir.join(format!("{stem}{ext}"));
    let mut i = 2;
    while p.exists() {
        p = dir.join(format!("{stem} ({i}){ext}"));
        i += 1;
    }
    p
}

fn dir_size(dir: &Path) -> u64 {
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| if e.path().is_dir() { dir_size(&e.path()) } else { e.metadata().map_or(0, |m| m.len()) })
        .sum()
}

/// The library world a loaded world clashes with: same world id, else same name.
fn clash<'a>(lib: &'a [World], l: &Loaded) -> Option<(&'a World, ConflictKind)> {
    lib.iter().find(|w| w.world_id == l.world.id).map(|w| (w, ConflictKind::SameId)).or_else(|| {
        lib.iter()
            .find(|w| w.base.name.trim().to_lowercase() == l.info.name.trim().to_lowercase())
            .map(|w| (w, ConflictKind::SameName))
    })
}

impl Core {
    /// What dropped or picked files would import.
    pub fn inspect_import(&self, paths: &[String]) -> Result<Vec<ImportItem>> {
        let lib = self.lib()?.list()?;
        Ok(paths
            .iter()
            .map(|p| match load_path(Path::new(p), true) {
                Ok(l) => {
                    let c = clash(&lib, &l);
                    ImportItem {
                        path: p.clone(),
                        name: file_name(Path::new(p)),
                        kind: l.kind,
                        size: l.info.size,
                        game_version: l.game_version,
                        status: if c.is_some() { ImportStatus::Conflict } else { ImportStatus::New },
                        conflict: c.map(|x| x.1),
                        existing: c.map(|x| x.0.base.name.clone()),
                    }
                }
                Err(_) => ImportItem {
                    path: p.clone(),
                    name: file_name(Path::new(p)),
                    kind: ImportKind::Invalid,
                    size: 0,
                    game_version: None,
                    status: ImportStatus::Invalid,
                    conflict: None,
                    existing: None,
                },
            })
            .collect())
    }

    /// Imports files into the library with the user's choices for clashes (default: keep both).
    pub fn import_worlds(&self, req: &ImportRequest) -> Result<Vec<World>> {
        let lib = self.lib()?;
        let mut entry = Entry::new(ActivityKind::Import, "Worlds");
        let r = (|| {
            let mut out = Vec::new();
            for p in &req.paths {
                let Ok(mut l) = load_path(Path::new(p), true) else { continue };
                let current = lib.list()?;
                if let Some((w, kind)) = clash(&current, &l) {
                    match req.choices.get(p).copied().unwrap_or_default() {
                        Choice::Skip => continue,
                        Choice::Replace => lib.delete(&w.base.id)?,
                        Choice::KeepBoth => {
                            let names: Vec<&str> = current.iter().map(|w| w.base.name.as_str()).collect();
                            let name = unique_name(&l.info.name, &names);
                            let ids: Vec<&str> = current.iter().map(|w| w.world_id.as_str()).collect();
                            let id = (kind == ConflictKind::SameId).then(|| new_world_id_excluding(&ids));
                            set_identity(&mut l.world, id.as_deref(), Some(&name))?;
                            l.info.name = name;
                        }
                    }
                }
                let meta = AddMeta {
                    origin: l.origin,
                    source: file_name(Path::new(p)),
                    source_detail: (l.source_kind == SourceKind::BlockheadsServer).then(|| "server".to_string()),
                    game_version: l.game_version.clone(),
                    kind: l.source_kind,
                    device_name: None,
                };
                let w = lib.add(&l.world, &l.info, &meta)?;
                entry.worlds.push(w.base.name.clone());
                entry.size = Some(entry.size.unwrap_or(0) + w.base.size);
                out.push(w);
            }
            Ok(out)
        })();
        self.activity.record(&entry, r)
    }

    /// Exports library worlds (or worlds of a read, saved to the library on the way) into a folder.
    pub fn export_worlds(
        &self,
        op: &str,
        req: &ExportRequest,
        progress: &mut dyn FnMut(Progress),
    ) -> Result<ExportResult> {
        let guard = self.ops.start(op);
        let mut entry = Entry::new(ActivityKind::Export, "Worlds");
        let r = (|| {
            let saved = match &req.read_id {
                Some(id) => self.save_to_library(id, &req.world_ids)?,
                None => Vec::new(),
            };
            let ids: Vec<String> = match &req.read_id {
                Some(_) => saved.iter().map(|w| w.base.id.clone()).collect(),
                None => req.world_ids.clone(),
            };
            let lib = self.lib()?;
            let folder = PathBuf::from(&req.folder);
            std::fs::create_dir_all(&folder)?;
            let mut files = Vec::new();
            for (i, id) in ids.iter().enumerate() {
                guard.cancel.check()?;
                let b = lib.open_world(id)?;
                let name = b.manifest.world.name.clone().unwrap_or("World".into());
                progress(Progress::item(Stage::Exporting, &name, i + 1, ids.len()));
                let out = match req.format {
                    ExportFormat::Bhworld => {
                        let dest = free_path(&folder, &slug(&name), ".bhworld");
                        let size = std::fs::copy(lib.path(id)?, &dest)?;
                        ExportedFile { name: file_name(&dest), size }
                    }
                    ExportFormat::Server => {
                        let dest = folder.join(&b.world.id);
                        if dest.exists() {
                            return Err(AppError::io(format!("{} already exists", dest.display())));
                        }
                        let (map, _) = client_to_server_files(&to_file_map(&b.world)?, &ConvertOptions::default())?;
                        write_dir_files(&dest, &map)?;
                        ExportedFile { name: file_name(&dest), size: dir_size(&dest) }
                    }
                };
                entry.worlds.push(name);
                files.push(out);
            }
            entry.size = Some(files.iter().map(|f| f.size).sum());
            Ok(ExportResult { folder: req.folder.clone(), files, saved })
        })();
        self.activity.record(&entry, r)
    }

    /// Free bytes on the disk holding `folder` (or its nearest existing parent).
    pub fn free_space(&self, folder: &str) -> Result<u64> {
        let mut p = PathBuf::from(folder);
        while !p.exists() {
            p = p.parent().map(Path::to_path_buf).ok_or(AppError::not_found(folder))?;
        }
        let p = std::fs::canonicalize(&p)?;
        let disks = sysinfo::Disks::new_with_refreshed_list();
        disks
            .list()
            .iter()
            .filter(|d| p.starts_with(d.mount_point()))
            .max_by_key(|d| d.mount_point().as_os_str().len())
            .map(|d| d.available_space())
            .ok_or(AppError::not_found(folder))
    }
}
