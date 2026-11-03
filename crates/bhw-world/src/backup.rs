//! Worlds inside Android backups (glue between [`bhw_backup`] and [`WorldFiles`]).

use std::io::{Read, Write};

use bhw_backup::saves::{read_saves, world_edit, WorldPut};
use bhw_backup::{rewrite_backup, BackupHeader, EditStats, EntryKind, Payload, SavesContent};

use crate::files::{WorldFile, WorldFiles};
use crate::time::now_unix;
use crate::{Error, Result};

/// The worlds of one `saves/` directory of a backup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupSaves {
    /// Archive prefix up to and including `saves/`; pass it back to [`put_worlds`].
    pub prefix: String,
    /// Package (`com.noodlecake.blockheads`) if known.
    pub package: Option<String>,
    /// Backup domain token (`f`, `r`, `ef`, ...) if known.
    pub domain: Option<String>,
    /// Worlds, files in memory.
    pub worlds: Vec<WorldFiles>,
    /// Global `game_db` files (paths relative to `game_db/`), exposed read-only; rewrites never touch them.
    pub game_db: Vec<WorldFile>,
}

fn data(content: &SavesContent, path: &str) -> Result<Payload> {
    let d = content.files.get(path).ok_or_else(|| Error::Format(format!("no data for {path} (use read_saves)")))?;
    Ok(Payload::Bytes(d.clone()))
}

/// Builds [`WorldFiles`] from the result of [`bhw_backup::saves::read_saves`].
pub fn worlds_from_backup(content: &SavesContent) -> Result<Vec<BackupSaves>> {
    let mut out = Vec::new();
    for dir in &content.saves_dirs {
        let mut worlds = Vec::new();
        for w in &dir.worlds {
            let mut files = Vec::new();
            for f in w.files.iter().filter(|f| f.kind == EntryKind::File) {
                files.push(WorldFile {
                    path: f.path.clone(),
                    data: data(content, &format!("{}{}/{}", dir.prefix, w.id, f.path))?,
                });
            }
            worlds.push(WorldFiles::new(w.id.clone(), files)?);
        }
        let mut game_db = Vec::new();
        let gdb = dir.game_db_prefix.clone().unwrap_or_default();
        for f in dir.game_db.iter().filter(|f| f.kind == EntryKind::File) {
            game_db.push(WorldFile { path: f.path.clone(), data: data(content, &format!("{gdb}{}", f.path))? });
        }
        out.push(BackupSaves {
            prefix: dir.prefix.clone(),
            package: dir.package.clone(),
            domain: dir.domain.clone(),
            worlds,
            game_db,
        });
    }
    Ok(out)
}

/// Reads a backup in one pass and returns its header and worlds.
pub fn load_backup(input: Box<dyn Read + '_>, password: Option<&str>) -> Result<(BackupHeader, Vec<BackupSaves>)> {
    let content = read_saves(input, password)?;
    let saves = worlds_from_backup(&content)?;
    Ok((content.header, saves))
}

/// Rewrites a device backup with worlds added/replaced (`put`) and removed (`remove`, ids) below `saves_prefix`,
/// keeping every other entry unchanged (see [`bhw_backup::edit`] for why the whole backup is rewritten).
pub fn put_worlds<'a>(
    input: Box<dyn Read + 'a>,
    input_password: Option<&str>,
    output: Box<dyn Write + 'a>,
    output_password: Option<&str>,
    saves_prefix: &str,
    put: &[WorldFiles],
    remove: &[&str],
) -> Result<EditStats> {
    let puts: Vec<WorldPut> = put
        .iter()
        .map(|w| WorldPut {
            id: w.id.clone(),
            files: w.files.iter().map(|f| (f.path.clone(), f.data.clone())).collect(),
        })
        .collect();
    let edit = world_edit(saves_prefix, remove, &puts, now_unix())?;
    Ok(rewrite_backup(input, input_password, output, output_password, &edit)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::files::tests::{mem, sample, ID, ID2};
    use bhw_backup::saves::scan;
    use bhw_backup::{BackupOptions, BackupWriter, TarWriter};

    const PFX: &str = "apps/com.noodlecake.blockheads/f/Library/Application Support/";

    fn device_backup(pw: Option<&str>) -> Vec<u8> {
        let mut out = Vec::new();
        let opts = BackupOptions { version: 4, password: pw.map(str::to_string), rounds: 10, ..Default::default() };
        let mut w = BackupWriter::create(Box::new(&mut out), &opts).unwrap();
        let mut t = TarWriter::new(&mut w);
        let entries: Vec<(String, Option<&[u8]>)> = vec![
            ("apps/com.noodlecake.blockheads/_manifest".into(), Some(b"m")),
            (format!("{PFX}saves"), None),
            (format!("{PFX}saves/{ID}"), None),
            (format!("{PFX}saves/{ID}/world_db"), None),
            (format!("{PFX}saves/{ID}/world_db/data.mdb"), Some(b"old")),
            (format!("{PFX}game_db"), None),
            (format!("{PFX}game_db/data.mdb"), Some(b"global")),
        ];
        for (p, d) in entries {
            let (kind, d) = match d {
                Some(d) => (EntryKind::File, d),
                None => (EntryKind::Directory, &b""[..]),
            };
            let mut r: &[u8] = d;
            t.append(&p, kind, 0o600, 0, d.len() as u64, &mut r).unwrap();
        }
        t.finish().unwrap();
        w.finish().unwrap();
        out
    }

    #[test]
    fn load_put_and_reload() {
        let src = device_backup(Some("pw"));
        let (h, saves) = load_backup(Box::new(&src[..]), Some("pw")).unwrap();
        assert_eq!(h.version, 4);
        assert_eq!(saves.len(), 1);
        let s = &saves[0];
        assert_eq!(s.prefix, format!("{PFX}saves/"));
        assert_eq!(s.domain.as_deref(), Some("f"));
        assert_eq!(s.package.as_deref(), Some("com.noodlecake.blockheads"));
        assert_eq!(s.worlds[0].files, vec![mem("world_db/data.mdb", b"old")]);
        assert_eq!(s.game_db, vec![mem("data.mdb", b"global")]);

        let mut out = Vec::new();
        let stats =
            put_worlds(Box::new(&src[..]), Some("pw"), Box::new(&mut out), None, &s.prefix, &[sample(ID2)], &[ID])
                .unwrap();
        assert_eq!(stats.removed, 3);
        let (_, after) = load_backup(Box::new(&out[..]), None).unwrap();
        assert_eq!(after[0].worlds, vec![sample(ID2)]);
        assert_eq!(after[0].game_db, s.game_db);
        assert!(put_worlds(Box::new(&src[..]), Some("pw"), Box::new(&mut Vec::new()), None, "x/", &[], &[]).is_err());
    }

    #[test]
    fn scan_result_has_no_data() {
        let src = device_backup(None);
        let c = scan(Box::new(&src[..]), None).unwrap();
        assert!(matches!(worlds_from_backup(&c), Err(Error::Format(_))));
        // game_db data missing too.
        let mut c2 = c.clone();
        c2.saves_dirs[0].worlds.clear();
        assert!(matches!(worlds_from_backup(&c2), Err(Error::Format(_))));
    }
}
