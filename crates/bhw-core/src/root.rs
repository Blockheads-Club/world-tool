//! Rooted phones: the game's saves are read and written directly (`tar` over `adb exec-out` / `exec-in` as root),
//! without the "Back up my data" / "Restore my data" screens. Same safety as the backup method: the game is
//! stopped first, a copy of its `saves/` + `game_db/` is stored as the automatic backup before writing, and
//! `bhw_device` gives the files back to the app (owner, permissions, SELinux context) after writing. Root is
//! probed once per phone (a root manager may ask on the phone); when it is missing or fails before anything was
//! written, the backup method is used instead. Settings: `root_mode` `auto` (root when it works) or `never`.

use std::path::Path;
use std::sync::{Arc, OnceLock};

use bhw_device::root::RootSaves;
use bhw_device::{Adb, DeviceError, IfRunning, PcInstall, RootStyle};
use bhw_save::GameDb;

use crate::error::{AppError, ErrorCode, Result};
use crate::read::{read_install, ReadSession};
use crate::transfer::{apply_choices, existing_of, PlanState};
use crate::types::{BackupReason, Device, Progress, Stage, TransferRequest, TransferResult};
use crate::util::{lock, unique_id, Cancel, TempPath, Throttle};
use crate::Core;

/// A phone written through root: how to become root and where the game keeps its data.
#[derive(Debug, Clone)]
pub struct RootTarget {
    pub style: RootStyle,
    pub saves: RootSaves,
}

/// The parts of the game's data folder that are copied.
const PARTS: [&str; 2] = ["saves", "game_db"];

impl Core {
    fn root_probe(&self, serial: &str) -> Arc<OnceLock<Option<RootStyle>>> {
        lock(&self.root).entry(serial.to_string()).or_default().clone()
    }

    /// Whether root is known to work on this phone (and allowed by the settings); never waits.
    pub(crate) fn known_root(&self, serial: &str) -> bool {
        self.settings().root_mode != "never"
            && lock(&self.root).get(serial).is_some_and(|p| matches!(p.get(), Some(Some(_))))
    }

    /// Starts probing root on a newly seen phone in the background (when the settings allow root), and forgets
    /// phones that are gone (a reconnected phone is probed again).
    pub(crate) fn watch_root(&self, ready: &[String]) {
        lock(&self.root).retain(|serial, _| ready.contains(serial));
        if self.settings().root_mode == "never" {
            return;
        }
        let Ok(adb) = self.adb() else { return };
        for serial in ready {
            let fresh = !lock(&self.root).contains_key(serial);
            let probe = self.root_probe(serial);
            if fresh {
                let (adb, serial) = (adb.clone(), serial.clone());
                std::thread::spawn(move || {
                    probe.get_or_init(|| adb.probe_root(&serial).ok().flatten());
                });
            }
        }
    }

    /// How to become root on a phone (probing now if needed), `None` when root is off in the settings or missing.
    pub(crate) fn root_style(&self, serial: &str) -> Option<RootStyle> {
        if self.settings().root_mode == "never" {
            return None;
        }
        let adb = self.adb().ok()?;
        *self.root_probe(serial).get_or_init(|| adb.probe_root(serial).ok().flatten())
    }

    /// Remembers that root failed on a phone (it then uses the backup method until it reconnects).
    fn root_failed(&self, serial: &str) {
        let none = OnceLock::new();
        let _ = none.set(None);
        lock(&self.root).insert(serial.to_string(), Arc::new(none));
    }

    /// The root way to a phone's saves, or `None` for the backup method.
    pub(crate) fn root_target(&self, serial: &str) -> Option<RootTarget> {
        let style = self.root_style(serial)?;
        match self.adb().ok()?.root_find_saves(serial, style) {
            Ok(saves) => Some(RootTarget { style, saves }),
            Err(DeviceError::RootUnavailable) => {
                self.root_failed(serial);
                None
            }
            Err(_) => None,
        }
    }

    /// Copies the game's `saves/` and `game_db/` into `dir` (laid out like a Blockheads PC data folder).
    fn root_pull_all(
        &self,
        adb: &Adb,
        serial: &str,
        t: &RootTarget,
        dir: &Path,
        stage: Stage,
        progress: &mut dyn FnMut(Progress),
        cancel: &Cancel,
    ) -> Result<PcInstall> {
        let local = PcInstall::at(dir);
        let sup = local.support_dir();
        let mut th = Throttle::default();
        let mut on = |p: bhw_device::Progress| {
            if th.pass(p.bytes, false) {
                progress(Progress::bytes(stage, p.bytes, None));
            }
        };
        for part in PARTS {
            match adb.root_pull(serial, t.style, &t.saves.support_dir, part, &sup, &mut on, &cancel.device) {
                // An empty folder sends no files; a missing `game_db` (no world yet) sends nothing at all.
                Ok(_) | Err(DeviceError::SavesNotFound) => {}
                Err(DeviceError::RootUnavailable) if part == "game_db" => {}
                Err(e) => return Err(e.into()),
            }
        }
        std::fs::create_dir_all(local.saves_dir())?;
        Ok(local)
    }

    /// Stops the game (keeping it backup-eligible) and checks its version.
    fn root_prepare(&self, adb: &Adb, serial: &str) -> Result<()> {
        adb.game_info(serial)?.check_supported()?;
        adb.check_running(serial, IfRunning::Stop)?;
        Ok(())
    }

    /// Reads a rooted phone's worlds into a local copy the session owns.
    pub(crate) fn read_root(
        &self,
        d: &Device,
        t: &RootTarget,
        progress: &mut dyn FnMut(Progress),
        cancel: &Cancel,
    ) -> Result<ReadSession> {
        let adb = self.adb()?;
        self.root_prepare(&adb, &d.id)?;
        let dir = TempPath(self.reads_dir()?.join(unique_id("root-")));
        let local = self.root_pull_all(&adb, &d.id, t, &dir.0, Stage::Reading, progress, cancel)?;
        let mut s = read_install(d, &local, progress)?;
        s.dir = Some(dir);
        Ok(s)
    }

    /// Stores the pulled copy as an automatic backup (an Android-style `.ab` of `saves/` + `game_db/`).
    fn root_backup(
        &self,
        local: &PcInstall,
        d: &Device,
        reason: BackupReason,
        count: Option<usize>,
    ) -> Result<crate::Backup> {
        let tmp = TempPath(self.tmp_dir()?.join(format!("{}-root.ab", unique_id("b"))));
        crate::backups::write_pc_backup(local, &tmp.0)?;
        self.store_backup(&tmp.0, d, reason, count, local.list_worlds()?.len())
    }

    /// Sends the local `saves/` and `game_db/` back to the phone and checks the world ids are there.
    fn root_push_all(
        &self,
        adb: &Adb,
        serial: &str,
        t: &RootTarget,
        local: &PcInstall,
        progress: &mut dyn FnMut(Progress),
        cancel: &Cancel,
    ) -> Result<()> {
        let mut th = Throttle::default();
        let mut on = |p: bhw_device::Progress| {
            if th.pass(p.bytes, false) {
                progress(Progress::bytes(Stage::Writing, p.bytes, None));
            }
        };
        for part in PARTS {
            let src = local.support_dir().join(part);
            if src.is_dir() {
                adb.root_push(
                    serial,
                    t.style,
                    &src,
                    &t.saves.support_dir,
                    part,
                    IfRunning::Stop,
                    &mut on,
                    &cancel.device,
                )?;
            }
        }
        progress(Progress::stage(Stage::Verifying));
        let mut found = adb.root_find_saves(serial, t.style).map(|s| s.worlds).unwrap_or_default();
        found.sort();
        let mut want: Vec<String> = local.list_worlds()?.into_iter().map(|w| w.id).collect();
        want.sort();
        if found != want {
            return Err(AppError::with(
                ErrorCode::Io,
                format!("the phone has worlds {found:?} after writing {want:?}"),
            ));
        }
        Ok(())
    }

    /// Copies worlds to a rooted phone.
    pub(crate) fn transfer_root(
        &self,
        p: &PlanState,
        t: &RootTarget,
        req: &TransferRequest,
        progress: &mut dyn FnMut(Progress),
        cancel: &Cancel,
        backup: &mut Option<crate::Backup>,
    ) -> Result<TransferResult> {
        let (adb, serial) = (self.adb()?, &p.target.id);
        self.root_prepare(&adb, serial)?;
        progress(Progress::stage(Stage::BackingUp));
        let work = TempPath(self.reads_dir()?.join(unique_id("root-")));
        let local = self.root_pull_all(&adb, serial, t, &work.0, Stage::BackingUp, progress, cancel)?;
        let before = read_install(&p.target, &local, &mut |_| {})?;
        let existing = existing_of(&before.worlds);
        drop(before);
        if self.settings().backup_before_write {
            *backup = Some(self.root_backup(&local, &p.target, BackupReason::BeforeWrite, Some(p.keys.len()))?);
        }
        progress(Progress::stage(Stage::Preparing));
        let mut game_db = GameDb::open(&local.game_db()).unwrap_or_default();
        let changes = apply_choices(&existing, self.incoming(p)?, &req.choices, &mut game_db)?;
        for w in &changes.puts {
            w.save_to_saves_dir(&local.saves_dir(), true)?;
        }
        for id in &changes.removes {
            std::fs::remove_dir_all(local.saves_dir().join(id))?;
        }
        game_db.save(&local.game_db())?;
        cancel.check()?;
        self.root_push_all(&adb, serial, t, &local, progress, cancel)?;
        progress(Progress::stage(Stage::Checking));
        self.drop_sessions(serial);
        let mut s = read_install(&p.target, &local, &mut |_| {})?;
        s.dir = Some(work);
        self.add_session(s);
        Ok(changes.result)
    }

    /// Puts a stored backup on a rooted phone (after storing a copy of what is there now).
    pub(crate) fn restore_root(
        &self,
        d: &Device,
        t: &RootTarget,
        file: &Path,
        progress: &mut dyn FnMut(Progress),
        cancel: &Cancel,
    ) -> Result<()> {
        let adb = self.adb()?;
        self.root_prepare(&adb, &d.id)?;
        progress(Progress::stage(Stage::BackingUp));
        let work = TempPath(self.tmp_dir()?.join(unique_id("root-")));
        let local = self.root_pull_all(&adb, &d.id, t, &work.0, Stage::BackingUp, progress, cancel)?;
        self.root_backup(&local, d, BackupReason::BeforeRestore, None)?;
        self.restore_pc(&local, file, progress)?;
        cancel.check()?;
        self.root_push_all(&adb, &d.id, t, &local, progress, cancel)
    }
}
