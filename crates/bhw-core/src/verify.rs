//! Checking a phone after `adb restore`. Android reports nothing when "Restore my data" is declined (or the screen
//! times out), so after every restore a fresh backup is taken and compared with what was written: each written
//! world must be there byte for byte (its `world_db/data.mdb`) and registered in `game_db`, and removed worlds must
//! be gone. A phone that already matched counts as restored.

use std::path::Path;

use bhw_world::backup::BackupSaves;
use bhw_world::WorldFiles;

use crate::error::{AppError, ErrorCode, Result};
use crate::read::{backup_game_db, open_ab, pick_saves};
use crate::types::{Progress, Stage};
use crate::util::{unique_id, Cancel, TempPath};
use crate::Core;

const DATA: &str = "world_db/data.mdb";

/// What the phone must hold after a restore.
#[derive(Debug, Default)]
pub struct Expect {
    /// Worlds that must be there (compared by `world_db/data.mdb`).
    pub present: Vec<WorldFiles>,
    /// World ids that must be gone.
    pub absent: Vec<String>,
    /// World ids that must be registered in `game_db`.
    pub registered: Vec<String>,
}

fn data(w: &WorldFiles) -> Option<Vec<u8>> {
    w.get(DATA)?.data.read_all().ok()
}

/// What is wrong with `found` (the phone's saves after the restore), `None` when it matches.
pub fn mismatch(expect: &Expect, found: Option<&BackupSaves>) -> Option<String> {
    let worlds = found.map_or(&[][..], |s| &s.worlds[..]);
    let registered = found.and_then(backup_game_db).map(|g| g.world_ids()).unwrap_or_default();
    for w in &expect.present {
        match worlds.iter().find(|x| x.id == w.id) {
            None => return Some(format!("world {} is missing", w.id)),
            Some(x) if data(x) != data(w) => return Some(format!("world {} was not replaced", w.id)),
            Some(_) => {}
        }
    }
    if let Some(id) = expect.absent.iter().find(|id| worlds.iter().any(|x| &x.id == *id)) {
        return Some(format!("world {id} is still there"));
    }
    if let Some(id) = expect.registered.iter().find(|id| !registered.contains(id)) {
        return Some(format!("world {id} is not in the world list"));
    }
    None
}

impl Core {
    /// Takes a fresh backup of the phone and checks it against `expect`. Returns the backup (the phone's state now)
    /// or `restore_declined`.
    pub(crate) fn verify_restore(
        &self,
        serial: &str,
        expect: &Expect,
        password: Option<&str>,
        progress: &mut dyn FnMut(Progress),
        cancel: &Cancel,
    ) -> Result<TempPath> {
        progress(Progress::stage(Stage::Verifying));
        let file = TempPath(self.tmp_dir()?.join(format!("{}-check.ab", unique_id("v"))));
        self.backup_phone(serial, &file.0, Stage::PhoneCheck, Stage::Verifying, None, progress, cancel)?;
        check_file(&file.0, expect, password)?;
        Ok(file)
    }
}

/// [`mismatch`] for a backup file; a mismatch is `restore_declined`.
pub fn check_file(file: &Path, expect: &Expect, password: Option<&str>) -> Result<()> {
    let saves = pick_saves(open_ab(file, password)?);
    match mismatch(expect, saves.as_ref()) {
        Some(why) => Err(AppError::with(ErrorCode::RestoreDeclined, why)),
        None => Ok(()),
    }
}
