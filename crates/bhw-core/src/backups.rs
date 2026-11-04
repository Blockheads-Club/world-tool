//! The backups store: full device backups (`.ab`) in `<Worlds folder>/backups` with the library's sidecar plus our
//! own (`.ab.info.json`: device id, reason, world count). Blockheads PC backups are `.ab` files too, laid out like an
//! Android backup of the game, so they can be restored to either.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use bhw_backup::saves::{DIR_MODE, FILE_MODE};
use bhw_backup::{rewrite_backup, BackupOptions, BackupWriter, EntryKind, TarWriter};
use bhw_device::PcInstall;
use bhw_save::GameDb;
use bhw_world::WorldFiles;

use crate::activity::Entry;
use crate::error::Result;
use crate::read::{backup_game_db, open_ab, pick_saves, SUPPORT_PREFIX};
use crate::transfer::{backup_edit, Changes};
use crate::types::{ActivityKind, ActivityResult, Backup, BackupReason, Device, DeviceKind, Progress, Stage};
use crate::util::{child, file_name, mtime_ms, now_ms, unique_id, Cancel, TempPath};
use crate::verify::Expect;
use crate::worlds::register;
use crate::Core;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Info {
    device_id: String,
    reason: BackupReason,
    reason_count: Option<usize>,
    worlds: usize,
    time: u64,
    automatic: bool,
}

fn info_path(ab: &Path) -> PathBuf {
    let mut s = ab.as_os_str().to_owned();
    s.push(".info.json");
    PathBuf::from(s)
}

fn add_tree(t: &mut TarWriter<'_>, dir: &Path, prefix: &str) -> Result<()> {
    let mut entries: Vec<_> = fs::read_dir(dir)?.collect::<std::io::Result<_>>()?;
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let path = format!("{prefix}/{}", e.file_name().to_string_lossy());
        let ft = e.file_type()?;
        let mtime = mtime_ms(&e.path()) / 1000;
        if ft.is_dir() {
            t.append(&path, EntryKind::Directory, DIR_MODE, mtime, 0, &mut std::io::empty())?;
            add_tree(t, &e.path(), &path)?;
        } else if ft.is_file() && e.file_name() != "lock.mdb" {
            let len = e.metadata()?.len();
            let mut f = fs::File::open(e.path())?;
            t.append(&path, EntryKind::File, FILE_MODE, mtime, len, &mut f)?;
        }
    }
    Ok(())
}

/// Writes Blockheads PC's `saves/` and `game_db/` as an Android-style backup of the game.
pub fn write_pc_backup(pc: &PcInstall, out: &Path) -> Result<()> {
    let file = std::io::BufWriter::new(fs::File::create(out)?);
    let mut w = BackupWriter::create(Box::new(file), &BackupOptions::default())?;
    {
        let mut t = TarWriter::new(&mut w);
        let support = SUPPORT_PREFIX.trim_end_matches('/');
        for sub in ["saves", "game_db"] {
            let dir = pc.support_dir().join(sub);
            if dir.is_dir() {
                t.append(&format!("{support}/{sub}"), EntryKind::Directory, DIR_MODE, 0, 0, &mut std::io::empty())?;
                add_tree(&mut t, &dir, &format!("{support}/{sub}"))?;
            }
        }
        t.finish()?;
    }
    w.finish()?;
    Ok(())
}

impl Core {
    /// Stores a device backup file (copied) as an automatic backup and drops old ones of that device.
    pub(crate) fn store_backup(
        &self,
        src: &Path,
        device: &Device,
        reason: BackupReason,
        reason_count: Option<usize>,
        worlds: usize,
    ) -> Result<Backup> {
        let lib = self.lib()?;
        let tag = serde_json::to_value(reason)?.as_str().unwrap_or_default().to_string();
        let lb = lib.inner.add_backup(src, &device.name, &tag, false)?;
        let info = Info { device_id: device.id.clone(), reason, reason_count, worlds, time: now_ms(), automatic: true };
        fs::write(info_path(&lb.path), serde_json::to_vec_pretty(&info)?)?;
        let b = self.backup_of(&lb.path, Some(info), lb.meta.map_or(0, |m| m.size));
        let mut e = Entry::new(ActivityKind::Backup, &device.name);
        e.size = Some(b.size);
        self.activity.add(&e, ActivityResult::Done, None);
        self.prune(&device.id)?;
        Ok(b)
    }

    fn backup_of(&self, path: &Path, info: Option<Info>, size: u64) -> Backup {
        let meta = fs::read(path.with_extension("ab.json"))
            .ok()
            .and_then(|b| serde_json::from_slice::<bhw_world::BackupMeta>(&b).ok());
        let info = info.unwrap_or(Info {
            device_id: String::new(),
            reason: BackupReason::BeforeWrite,
            reason_count: None,
            worlds: 0,
            time: mtime_ms(path),
            automatic: false,
        });
        Backup {
            id: file_name(path),
            device_id: info.device_id,
            device: meta.map_or_else(String::new, |m| m.device),
            reason: info.reason,
            reason_count: info.reason_count,
            time: info.time,
            worlds: info.worlds,
            size,
            automatic: info.automatic,
        }
    }

    /// Stored backups, newest first.
    pub fn list_backups(&self) -> Result<Vec<Backup>> {
        let mut out: Vec<Backup> = self
            .lib()?
            .inner
            .list_backups()?
            .into_iter()
            .map(|b| {
                let info = fs::read(info_path(&b.path)).ok().and_then(|x| serde_json::from_slice(&x).ok());
                let size = fs::metadata(&b.path).map_or(0, |m| m.len());
                self.backup_of(&b.path, info, size)
            })
            .collect();
        // A single closure, so function coverage does not depend on two backups sharing a timestamp.
        out.sort_by(|a, b| (&b.time, &b.id).cmp(&(&a.time, &a.id)));
        Ok(out)
    }

    /// Keeps the newest `backups_to_keep` automatic backups of a device.
    fn prune(&self, device_id: &str) -> Result<()> {
        let keep = self.settings().backups_to_keep.max(1) as usize;
        let mine = self.list_backups()?.into_iter().filter(|b| b.automatic && b.device_id == device_id);
        for b in mine.skip(keep) {
            self.delete_backup(&b.id)?;
        }
        Ok(())
    }

    /// Deletes a stored backup.
    pub fn delete_backup(&self, id: &str) -> Result<()> {
        let lib = self.lib()?;
        let path = child(&lib.inner.backups_dir(), id)?;
        lib.inner.delete_backup(&path)?;
        let _ = fs::remove_file(info_path(&path));
        Ok(())
    }

    /// Backs up Blockheads PC into the store.
    pub(crate) fn backup_pc(
        &self,
        pc: &PcInstall,
        device: &Device,
        reason: BackupReason,
        count: Option<usize>,
    ) -> Result<Backup> {
        let tmp = TempPath(self.tmp_dir()?.join(format!("{}-pc.ab", unique_id("b"))));
        write_pc_backup(pc, &tmp.0)?;
        self.store_backup(&tmp.0, device, reason, count, pc.list_worlds()?.len())
    }

    /// Makes Blockheads PC's saves and world list equal to a backup's.
    pub(crate) fn restore_pc(&self, pc: &PcInstall, file: &Path, progress: &mut dyn FnMut(Progress)) -> Result<()> {
        let saves = pick_saves(open_ab(file, None)?);
        let (worlds, game_db) = match &saves {
            Some(s) => (s.worlds.clone(), backup_game_db(s)),
            None => (Vec::new(), None),
        };
        let mut game_db = match game_db {
            Some(g) => g,
            None => {
                let mut g = GameDb::new();
                for w in &worlds {
                    register(&mut g, w)?;
                }
                g
            }
        };
        let dir = pc.saves_dir();
        for old in pc.list_worlds()? {
            if !worlds.iter().any(|w| w.id == old.id) {
                fs::remove_dir_all(&old.path)?;
                game_db.unregister(&old.id);
            }
        }
        for (i, w) in worlds.iter().enumerate() {
            progress(Progress::item(Stage::Writing, &w.id, i + 1, worlds.len()));
            crate::transfer::with_lock(w.clone())?.save_to_saves_dir(&dir, true)?;
        }
        game_db.save(&pc.game_db())?;
        Ok(())
    }

    /// Puts a stored backup back on a device (after backing the device up first).
    pub fn restore_backup(
        &self,
        op: &str,
        backup_id: &str,
        device_id: &str,
        progress: &mut dyn FnMut(Progress),
    ) -> Result<()> {
        let guard = self.ops.start(op);
        let mut entry = Entry::new(ActivityKind::Restore, device_id);
        let r = (|| {
            let path = child(&self.lib()?.inner.backups_dir(), backup_id)?;
            let d = self.writable_device(device_id)?;
            entry.device = d.name.clone();
            entry.size = Some(fs::metadata(&path)?.len());
            match d.kind {
                DeviceKind::Pc => {
                    let pc = self.pc().ok_or(crate::AppError::not_found("Blockheads PC"))?;
                    self.check_pc_stopped()?;
                    progress(Progress::stage(Stage::BackingUp));
                    self.backup_pc(&pc, &d, BackupReason::BeforeRestore, None)?;
                    self.restore_pc(&pc, &path, progress)?;
                }
                _ => {
                    let r = match self.root_target(&d.id) {
                        Some(t) => self.restore_root(&d, &t, &path, progress, &guard.cancel),
                        None => self.restore_android(&d, &path, progress, &guard.cancel),
                    };
                    r.map_err(|e| e.device(&d.name))?
                }
            }
            self.drop_sessions(&d.id);
            Ok(())
        })();
        self.activity.record(&entry, r)
    }

    /// `adb restore` of a stored backup, then a check backup. Backups without the app's `_manifest` (made from
    /// Blockheads PC or through root) would be ignored by Android's restore, so their saves are put into the
    /// fresh backup of the phone instead (like a copy that replaces every world).
    fn restore_android(
        &self,
        d: &Device,
        path: &Path,
        progress: &mut dyn FnMut(Progress),
        cancel: &Cancel,
    ) -> Result<()> {
        let before = TempPath(self.tmp_dir()?.join(format!("{}-before.ab", unique_id("b"))));
        self.backup_phone(&d.id, &before.0, Stage::PhoneBackup, Stage::BackingUp, None, progress, cancel)?;
        let password = self.latest_session(&d.id).and_then(|s| s.password.clone());
        let now = pick_saves(open_ab(&before.0, password.as_deref())?);
        let now_ids: Vec<String> = now.as_ref().map_or(Vec::new(), |s| s.worlds.iter().map(|w| w.id.clone()).collect());
        self.store_backup(&before.0, d, BackupReason::BeforeRestore, None, now_ids.len())?;
        let stored = pick_saves(open_ab(path, password.as_deref())?);
        let worlds: Vec<WorldFiles> = stored.as_ref().map_or(Vec::new(), |s| s.worlds.clone());
        let mut expect = Expect {
            absent: now_ids.iter().filter(|id| !worlds.iter().any(|w| &w.id == *id)).cloned().collect(),
            ..Expect::default()
        };
        let rebuilt;
        let file = if has_manifest(path, password.as_deref())? {
            expect.present = worlds;
            path
        } else {
            progress(Progress::stage(Stage::Preparing));
            let mut game_db = stored.as_ref().and_then(backup_game_db).unwrap_or_default();
            for w in &worlds {
                register(&mut game_db, w)?;
            }
            let changes = Changes {
                puts: worlds.into_iter().map(crate::transfer::with_lock).collect::<Result<_>>()?,
                removes: expect.absent.clone(),
                ..Changes::default()
            };
            let prefix = now.map_or(format!("{SUPPORT_PREFIX}saves/"), |s| s.prefix);
            let edit = backup_edit(&prefix, &changes, &game_db, bhw_world::time::now_unix())?;
            rebuilt = TempPath(self.tmp_dir()?.join(format!("{}-restore.ab", unique_id("b"))));
            let input = std::io::BufReader::new(fs::File::open(&before.0)?);
            let output = std::io::BufWriter::new(fs::File::create(&rebuilt.0)?);
            rewrite_backup(Box::new(input), password.as_deref(), Box::new(output), password.as_deref(), &edit)?;
            expect.registered = changes.puts.iter().map(|w| w.id.clone()).collect();
            expect.present = changes.puts;
            &rebuilt.0
        };
        self.restore_phone(&d.id, file, progress, cancel)?;
        self.verify_restore(&d.id, &expect, password.as_deref(), progress, cancel)?;
        Ok(())
    }
}

/// The app's manifest entry in an Android backup (`apps/<package>/_manifest`).
pub const MANIFEST: &str = "apps/com.noodlecake.blockheads/_manifest";

/// Whether a backup file holds the game's `_manifest` (made by Android, not synthesized).
pub fn has_manifest(path: &Path, password: Option<&str>) -> Result<bool> {
    let f = std::io::BufReader::new(fs::File::open(path)?);
    Ok(bhw_backup::saves::scan(Box::new(f), password)?.entries.iter().any(|e| e.path == MANIFEST))
}
