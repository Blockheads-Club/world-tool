//! Copying worlds to a device. Android: a fresh full backup (kept as the automatic backup), rewritten with the new
//! worlds and an updated `game_db`, then `adb restore`. Blockheads PC: world folders and `game_db` written in place.

use std::collections::BTreeMap;
use std::io::BufReader;
use std::path::Path;
use std::sync::Arc;

use bhw_backup::saves::{world_edit, WorldPut, FILE_MODE};
use bhw_backup::{rewrite_backup, EntryKind, NewEntry, Payload};
use bhw_device::{AbMethod, AbOptions, IfRunning, Phase};
use bhw_save::GameDb;
use bhw_world::meta::new_world_id_excluding;
use bhw_world::{unique_name, WorldFiles};

use crate::activity::Entry;
use crate::error::{AppError, ErrorCode, Result};
use crate::read::{backup_game_db, open_ab, pick_saves, summarise, SUPPORT_PREFIX};
use crate::types::{
    ActivityKind, BackupReason, Choice, ConflictKind, Device, DeviceKind, DeviceWorld, PlanItem, PlanResult, Progress,
    Source, Stage, TransferPlan, TransferRequest, TransferResult,
};
use crate::util::{lock, unique_id, Cancel, TempPath, Throttle};
use crate::worlds::{register, set_identity};
use crate::Core;

/// A world offered by a source, with its world id.
#[derive(Debug, Clone)]
pub struct Offered {
    pub world: DeviceWorld,
    pub world_id: String,
}

/// A world already on the target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Existing {
    pub id: String,
    pub name: String,
    pub size: u64,
}

/// A planned transfer.
#[derive(Debug)]
pub struct PlanState {
    pub plan: TransferPlan,
    pub source: Source,
    pub target: Device,
    pub keys: Vec<String>,
}

/// What writing will change on the target.
#[derive(Debug, Default)]
pub struct Changes {
    pub puts: Vec<WorldFiles>,
    pub removes: Vec<String>,
    pub result: TransferResult,
}

/// Path of LMDB's lock file inside a world.
pub const LOCK: &str = "world_db/lock.mdb";

/// A world as the game needs it on disk: with a `world_db/lock.mdb`. The game (1.7.6, Android emulator) crashes
/// (SIGSEGV) loading a world whose lock file is missing. A new lock file is zero-filled (8 KiB), so LMDB then reads
/// meta page 0: `data.mdb` is re-encoded so that both meta pages hold the current tree (see `bhw_lmdb::writer`).
pub fn with_lock(mut w: WorldFiles) -> Result<WorldFiles> {
    if w.get(LOCK).is_some() {
        return Ok(w);
    }
    for f in w.files.iter_mut().filter(|f| f.path == "world_db/data.mdb") {
        f.data = Payload::Bytes(bhw_lmdb::convert(&f.data.read_all()?, bhw_lmdb::Layout::Bits32)?);
    }
    w.files.push(bhw_world::WorldFile { path: LOCK.into(), data: Payload::Bytes(vec![0; 8192]) });
    Ok(WorldFiles::new(w.id, w.files)?)
}

fn same_name(a: &str, b: &str) -> bool {
    a.trim().to_lowercase() == b.trim().to_lowercase()
}

/// Applies the user's choices: `incoming` are `(key, client world, name)`. Worlds are renamed / given new ids for
/// "keep both" and registered in `game_db`; worlds replaced under another id are removed from it.
pub fn apply_choices(
    existing: &[Existing],
    incoming: Vec<(String, WorldFiles, String)>,
    choices: &BTreeMap<String, Choice>,
    game_db: &mut GameDb,
) -> Result<Changes> {
    let mut c = Changes::default();
    let mut names: Vec<String> = existing.iter().map(|e| e.name.clone()).collect();
    let mut ids: Vec<String> = existing.iter().map(|e| e.id.clone()).collect();
    for (key, mut w, name) in incoming {
        let id_clash = ids.contains(&w.id);
        let name_clash =
            existing.iter().find(|e| e.id != w.id && same_name(&e.name, &name) && !c.removes.contains(&e.id));
        if id_clash || name_clash.is_some() {
            match choices.get(&key).copied().unwrap_or_default() {
                Choice::Skip => continue,
                Choice::Replace => {
                    if let Some(e) = name_clash {
                        c.removes.push(e.id.clone());
                        game_db.unregister(&e.id);
                    }
                    c.result.replaced.push(name.clone());
                }
                Choice::KeepBoth => {
                    let taken: Vec<&str> = names.iter().map(String::as_str).collect();
                    let new_name = unique_name(&name, &taken);
                    let id_refs: Vec<&str> = ids.iter().map(String::as_str).collect();
                    let new_id = id_clash.then(|| new_world_id_excluding(&id_refs));
                    set_identity(&mut w, new_id.as_deref(), Some(&new_name))?;
                    c.result.kept_both.push(new_name.clone());
                    names.push(new_name);
                }
            }
        } else {
            c.result.added.push(name.clone());
            names.push(name);
        }
        register(game_db, &w)?;
        ids.push(w.id.clone());
        c.puts.retain(|p| p.id != w.id);
        c.puts.push(with_lock(w)?);
    }
    ids.retain(|i| !c.removes.contains(i));
    ids.sort();
    ids.dedup();
    c.result.total = ids.len();
    Ok(c)
}

/// The plan rows: clashes first, then the worlds that stay, then the new ones.
pub fn plan_items(existing: &[Existing], offered: &[Offered]) -> (Vec<PlanItem>, usize) {
    let mut conflicts = Vec::new();
    let mut added = Vec::new();
    let mut clashed: Vec<&str> = Vec::new();
    let mut taken: Vec<String> =
        existing.iter().map(|e| e.name.clone()).chain(offered.iter().map(|o| o.world.name.clone())).collect();
    for o in offered {
        let w = &o.world;
        let hit = existing
            .iter()
            .find(|e| e.id == o.world_id)
            .map(|e| (e, ConflictKind::SameId))
            .or_else(|| existing.iter().find(|e| same_name(&e.name, &w.name)).map(|e| (e, ConflictKind::SameName)));
        match hit {
            Some((e, kind)) => {
                clashed.push(&e.id);
                let refs: Vec<&str> = taken.iter().map(String::as_str).collect();
                let copy = unique_name(&w.name, &refs);
                taken.push(copy.clone());
                conflicts.push(PlanItem {
                    key: w.id.clone(),
                    name: w.name.clone(),
                    size: w.size,
                    result: PlanResult::Conflict,
                    conflict: Some(kind),
                    copy_name: Some(copy),
                });
            }
            None => added.push(PlanItem {
                key: w.id.clone(),
                name: w.name.clone(),
                size: w.size,
                result: PlanResult::Added,
                conflict: None,
                copy_name: None,
            }),
        }
    }
    let stays = existing.iter().filter(|e| !clashed.contains(&e.id.as_str())).map(|e| PlanItem {
        key: format!("stay:{}", e.id),
        name: e.name.clone(),
        size: e.size,
        result: PlanResult::Stays,
        conflict: None,
        copy_name: None,
    });
    let total_after = existing.len() + offered.len();
    (conflicts.into_iter().chain(stays).chain(added).collect(), total_after)
}

fn existing_of(worlds: &[crate::read::SessionWorld]) -> Vec<Existing> {
    worlds
        .iter()
        .map(|w| Existing { id: w.info.world_id.clone(), name: w.info.name.clone(), size: w.info.size })
        .collect()
}

/// A tar edit that puts worlds into a backup's saves folder and replaces its `game_db`.
pub fn backup_edit(prefix: &str, changes: &Changes, game_db: &GameDb, mtime: u64) -> Result<bhw_backup::TarEdit> {
    let puts: Vec<WorldPut> = changes
        .puts
        .iter()
        .map(|w| WorldPut {
            id: w.id.clone(),
            files: w.files.iter().map(|f| (f.path.clone(), f.data.clone())).collect(),
        })
        .collect();
    let removes: Vec<&str> = changes.removes.iter().map(String::as_str).collect();
    let mut edit = world_edit(prefix, &removes, &puts, mtime)?;
    let support = prefix.strip_suffix("saves/").unwrap_or(prefix);
    edit.insert.push(NewEntry {
        path: format!("{support}game_db/data.mdb"),
        kind: EntryKind::File,
        mode: FILE_MODE,
        mtime,
        data: Payload::Bytes(game_db.to_bytes()?),
    });
    Ok(edit)
}

impl Core {
    /// Worlds a source offers (a phone must have been read first).
    pub(crate) fn offered(&self, source: &Source) -> Result<Vec<Offered>> {
        match source {
            Source::Library => Ok(self
                .lib()?
                .list()?
                .into_iter()
                .filter(|w| !w.world_id.is_empty())
                .map(|w| Offered { world_id: w.world_id.clone(), world: w.base })
                .collect()),
            Source::Files { paths } => Ok(paths
                .iter()
                .filter_map(|p| crate::files::load_path(Path::new(p), true).ok())
                .map(|l| Offered {
                    world: l.info.device_world(&l.path, l.game_version.as_deref().unwrap_or_default()),
                    world_id: l.world.id,
                })
                .collect()),
            Source::Device { device_id } => {
                let s = self.source_session(device_id)?;
                Ok(s.device_worlds().into_iter().map(|w| Offered { world_id: w.id.clone(), world: w }).collect())
            }
        }
    }

    /// The read session a device source uses: the last read of a phone, a fresh look at Blockheads PC.
    fn source_session(&self, device_id: &str) -> Result<Arc<crate::read::ReadSession>> {
        if device_id == crate::devices::PC_ID {
            let d = self.ready_device(device_id)?;
            return Ok(self.add_session(self.read_pc(&d, &mut |_| {})?));
        }
        self.latest_session(device_id).ok_or(AppError::not_found(device_id))
    }

    /// Worlds a source offers.
    pub fn source_worlds(&self, source: &Source) -> Result<Vec<DeviceWorld>> {
        Ok(self.offered(source)?.into_iter().map(|o| o.world).collect())
    }

    /// One world of a source as a client world, with its name.
    fn source_world(&self, source: &Source, key: &str) -> Result<(WorldFiles, String)> {
        match source {
            Source::Library => {
                let b = self.lib()?.open_world(key)?;
                let name = b.manifest.world.name.clone().unwrap_or("World".into());
                Ok((b.world, name))
            }
            Source::Files { .. } => {
                let l = crate::files::load_path(Path::new(key), false)?;
                Ok((l.world, l.info.name))
            }
            Source::Device { device_id } => {
                let s = self.latest_session(device_id).ok_or(AppError::not_found(device_id))?;
                let w = s.world(key)?;
                Ok((w.files.to_memory()?, w.info.name.clone()))
            }
        }
    }

    /// Worlds on a target as far as known (Android: its last read; Blockheads PC: now).
    fn target_worlds(&self, target: &Device) -> Result<(Vec<Existing>, u64)> {
        match target.kind {
            DeviceKind::Pc => {
                let s = self.read_pc(target, &mut |_| {})?;
                Ok((existing_of(&s.worlds), s.size()))
            }
            _ => Ok(self.latest_session(&target.id).map_or((Vec::new(), 0), |s| (existing_of(&s.worlds), s.size()))),
        }
    }

    /// What copying `world_ids` of `source` to `target_id` would do.
    pub fn transfer_plan(&self, source: &Source, world_ids: &[String], target_id: &str) -> Result<TransferPlan> {
        let target = self.writable_device(target_id)?;
        let offered: Vec<Offered> =
            self.offered(source)?.into_iter().filter(|o| world_ids.contains(&o.world.id)).collect();
        let (existing, backup_size) = self.target_worlds(&target)?;
        let (items, total_after) = plan_items(&existing, &offered);
        let plan = TransferPlan {
            plan_id: unique_id("p"),
            target: target.name.clone(),
            total_after,
            size: offered.iter().map(|o| o.world.size).sum(),
            items,
            backup_name: target.name.clone(),
            backup_size,
            needs_phone: target.kind == DeviceKind::Android,
        };
        let keys = offered.into_iter().map(|o| o.world.id).collect();
        let state = PlanState { plan: plan.clone(), source: source.clone(), target, keys };
        lock(&self.plans).insert(plan.plan_id.clone(), Arc::new(state));
        Ok(plan)
    }

    fn incoming(&self, p: &PlanState) -> Result<Vec<(String, WorldFiles, String)>> {
        p.keys
            .iter()
            .map(|k| {
                let (w, name) = self.source_world(&p.source, k)?;
                Ok((k.clone(), w, name))
            })
            .collect()
    }

    /// Runs a plan.
    pub fn transfer(
        &self,
        op: &str,
        req: &TransferRequest,
        progress: &mut dyn FnMut(Progress),
    ) -> Result<TransferResult> {
        let guard = self.ops.start(op);
        let p = lock(&self.plans).get(&req.plan_id).cloned().ok_or(AppError::not_found(&req.plan_id))?;
        let mut entry = Entry::new(ActivityKind::Copy, &p.target.name);
        entry.worlds = p.plan.items.iter().filter(|i| i.result != PlanResult::Stays).map(|i| i.name.clone()).collect();
        entry.size = Some(p.plan.size);
        let mut backup = None;
        let r = match p.target.kind {
            DeviceKind::Pc => self.transfer_pc(&p, req, progress, &mut backup),
            _ => self.transfer_android(&p, req, progress, &guard.cancel, &mut backup),
        };
        let r = r.map_err(|e| e.device(&p.target.name).backup(backup.as_ref().map(|b: &crate::Backup| b.id.as_str())));
        let r = r.map(|mut res| {
            res.target = p.target.name.clone();
            res.backup_id = backup.as_ref().map(|b| b.id.clone());
            res.backup_time = backup.as_ref().map(|b| b.time);
            res
        });
        self.activity.record(&entry, r)
    }

    fn transfer_android(
        &self,
        p: &PlanState,
        req: &TransferRequest,
        progress: &mut dyn FnMut(Progress),
        cancel: &Cancel,
        backup: &mut Option<crate::Backup>,
    ) -> Result<TransferResult> {
        let serial = &p.target.id;
        let before = TempPath(self.tmp_dir()?.join(format!("{}-before.ab", unique_id("t"))));
        self.backup_phone(serial, &before.0, Stage::BackingUp, Some(p.plan.backup_size), progress, cancel)?;
        let password = self.latest_session(serial).and_then(|s| s.password.clone());
        let saves = pick_saves(open_ab(&before.0, password.as_deref())?);
        let (prefix, existing, mut game_db) = match &saves {
            Some(s) => (
                s.prefix.clone(),
                existing_of(&summarise(s.worlds.clone(), None)),
                backup_game_db(s).unwrap_or_default(),
            ),
            None => (format!("{SUPPORT_PREFIX}saves/"), Vec::new(), GameDb::new()),
        };
        if self.settings().backup_before_write {
            let (count, worlds) = (Some(p.keys.len()), existing.len());
            *backup = Some(self.store_backup(&before.0, &p.target, BackupReason::BeforeWrite, count, worlds)?);
        }
        progress(Progress::stage(Stage::Preparing));
        let changes = apply_choices(&existing, self.incoming(p)?, &req.choices, &mut game_db)?;
        let edit = backup_edit(&prefix, &changes, &game_db, bhw_world::time::now_unix())?;
        let after = TempPath(self.tmp_dir()?.join(format!("{}-after.ab", unique_id("t"))));
        {
            let input = BufReader::new(std::fs::File::open(&before.0)?);
            let output = std::io::BufWriter::new(std::fs::File::create(&after.0)?);
            rewrite_backup(Box::new(input), password.as_deref(), Box::new(output), password.as_deref(), &edit)?;
        }
        cancel.check()?;
        self.restore_phone(serial, &after.0, progress, cancel)?;
        progress(Progress::stage(Stage::Checking));
        self.drop_sessions(serial);
        let version = p.target.game_version.clone().unwrap_or_default();
        let s = self.session_from_backup(&p.target, after, password, &version)?;
        self.add_session(s);
        Ok(changes.result)
    }

    /// `adb restore` of a backup file, reporting `phone_restore` while waiting and `writing` after.
    pub(crate) fn restore_phone(
        &self,
        serial: &str,
        file: &Path,
        progress: &mut dyn FnMut(Progress),
        cancel: &Cancel,
    ) -> Result<()> {
        let adb = self.adb()?;
        let total = std::fs::metadata(file)?.len();
        let mut input = BufReader::new(std::fs::File::open(file)?);
        let opts = AbOptions { method: AbMethod::AdbCommand, if_running: IfRunning::Stop };
        let mut t = Throttle::default();
        let mut on = |x: bhw_device::Progress| match x.phase {
            Phase::WaitingForPhone => progress(Progress { elapsed: Some(0), ..Progress::stage(Stage::PhoneRestore) }),
            Phase::Staging => {
                if t.pass(x.bytes, false) {
                    progress(Progress::bytes(Stage::Preparing, x.bytes, Some(total)));
                }
            }
            _ => progress(Progress::bytes(Stage::Writing, x.bytes, Some(total))),
        };
        adb.restore(serial, &mut input, opts, &mut on, &cancel.device)?;
        Ok(())
    }

    fn transfer_pc(
        &self,
        p: &PlanState,
        req: &TransferRequest,
        progress: &mut dyn FnMut(Progress),
        backup: &mut Option<crate::Backup>,
    ) -> Result<TransferResult> {
        let pc = self.pc().ok_or(AppError::not_found("Blockheads PC"))?;
        self.check_pc_stopped()?;
        let (existing, _) = self.target_worlds(&p.target)?;
        if self.settings().backup_before_write {
            progress(Progress::stage(Stage::BackingUp));
            *backup = Some(self.backup_pc(&pc, &p.target, BackupReason::BeforeWrite, Some(p.keys.len()))?);
        }
        progress(Progress::stage(Stage::Preparing));
        let mut game_db = GameDb::open(&pc.game_db()).unwrap_or_default();
        let changes = apply_choices(&existing, self.incoming(p)?, &req.choices, &mut game_db)?;
        let saves = pc.saves_dir();
        for (i, w) in changes.puts.iter().enumerate() {
            progress(Progress::item(Stage::Writing, &w.id, i + 1, changes.puts.len()));
            w.save_to_saves_dir(&saves, true)?;
        }
        for id in &changes.removes {
            std::fs::remove_dir_all(saves.join(id))?;
        }
        game_db.save(&pc.game_db())?;
        progress(Progress::stage(Stage::Checking));
        self.drop_sessions(&p.target.id);
        Ok(changes.result)
    }

    /// Fails with `game_running` while Blockheads PC runs.
    pub(crate) fn check_pc_stopped(&self) -> Result<()> {
        if bhw_device::pc::game_running(&*self.cfg.runner, cfg!(windows))? {
            return Err(AppError::with(ErrorCode::GameRunning, "Blockheads PC is running"));
        }
        Ok(())
    }
}
