//! Finding backups and reading their descriptive plists (`Info.plist`, `Manifest.plist`, `Status.plist`).

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use plist::{Dictionary, Value};

use crate::{Error, Result};

/// Summary of one backup, for listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupInfo {
    /// Backup directory (`.../Backup/<udid>`).
    pub path: PathBuf,
    /// Directory name (device UDID, possibly with a date suffix for archived backups).
    pub id: String,
    /// Device name chosen by the user (e.g. "Anna's iPhone").
    pub device_name: Option<String>,
    /// Model identifier (e.g. `iPhone14,2`).
    pub product_type: Option<String>,
    /// iOS version (e.g. `17.4.1`).
    pub ios_version: Option<String>,
    /// Backup date, Unix seconds.
    pub date: Option<i64>,
    /// `true` if the backup is encrypted (a password is needed to read it).
    pub encrypted: bool,
}

pub(crate) fn read_plist(path: &Path) -> Result<Dictionary> {
    let v = Value::from_file(path).map_err(|e| Error::Plist(format!("{}: {e}", path.display())))?;
    v.into_dictionary().ok_or_else(|| Error::Plist(format!("{}: not a dictionary", path.display())))
}

fn string(d: &Dictionary, key: &str) -> Option<String> {
    d.get(key)?.as_string().map(str::to_string)
}

fn date(d: &Dictionary, key: &str) -> Option<i64> {
    let t: SystemTime = d.get(key)?.as_date()?.into();
    Some(match t.duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_secs() as i64,
        Err(e) => -(e.duration().as_secs() as i64),
    })
}

/// Reads the summary of the backup in `dir`. `Manifest.plist` is required; `Info.plist` and `Status.plist` are
/// optional (fields fall back to `Manifest.plist`'s `Lockdown` dictionary).
pub fn read_backup_info(dir: &Path) -> Result<BackupInfo> {
    let manifest_path = dir.join("Manifest.plist");
    if !manifest_path.is_file() {
        return Err(Error::NotABackup(dir.to_path_buf()));
    }
    let manifest = read_plist(&manifest_path)?;
    let info = read_plist(&dir.join("Info.plist")).unwrap_or_default();
    let status = read_plist(&dir.join("Status.plist")).unwrap_or_default();
    let lockdown = manifest.get("Lockdown").and_then(Value::as_dictionary).cloned().unwrap_or_default();
    let pick = |info_key: &str, lockdown_key: &str| string(&info, info_key).or_else(|| string(&lockdown, lockdown_key));
    Ok(BackupInfo {
        path: dir.to_path_buf(),
        id: dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        device_name: pick("Device Name", "DeviceName"),
        product_type: pick("Product Type", "ProductType"),
        ios_version: pick("Product Version", "ProductVersion"),
        date: date(&info, "Last Backup Date").or_else(|| date(&status, "Date")).or_else(|| date(&manifest, "Date")),
        encrypted: manifest.get("IsEncrypted").and_then(Value::as_boolean).unwrap_or(false),
    })
}

/// Lists the backups in a backup root (`.../MobileSync/Backup`), newest first. Subdirectories that are not readable
/// backups are skipped. A directory that is itself a backup is returned as the only entry.
pub fn list_backups(root: &Path) -> Result<Vec<BackupInfo>> {
    if root.join("Manifest.plist").is_file() {
        return Ok(vec![read_backup_info(root)?]);
    }
    let mut out = Vec::new();
    for entry in fs::read_dir(root)? {
        if let Ok(info) = read_backup_info(&entry?.path()) {
            out.push(info);
        }
    }
    out.sort_by(|a, b| b.date.cmp(&a.date).then_with(|| a.id.cmp(&b.id)));
    Ok(out)
}

/// Candidate backup roots for an OS (`std::env::consts::OS` values) given an environment lookup. Linux and other
/// systems have no default (the user chooses a folder, e.g. one copied from a Mac or Windows PC).
pub fn candidate_backup_dirs(os: &str, env: impl Fn(&str) -> Option<OsString>) -> Vec<PathBuf> {
    let under = |var: &str, rel: &[&str]| env(var).map(|base| rel.iter().fold(PathBuf::from(base), |p, c| p.join(c)));
    match os {
        "windows" => [
            under("APPDATA", &["Apple Computer", "MobileSync", "Backup"]),
            under("USERPROFILE", &["Apple", "MobileSync", "Backup"]),
        ]
        .into_iter()
        .flatten()
        .collect(),
        "macos" => under("HOME", &["Library", "Application Support", "MobileSync", "Backup"]).into_iter().collect(),
        _ => Vec::new(),
    }
}

/// Default backup roots of this computer that exist.
pub fn default_backup_dirs() -> Vec<PathBuf> {
    candidate_backup_dirs(std::env::consts::OS, |v| std::env::var_os(v)).into_iter().filter(|p| p.is_dir()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn write(path: &Path, d: Dictionary, binary: bool) {
        let v = Value::Dictionary(d);
        if binary {
            v.to_file_binary(path).unwrap()
        } else {
            v.to_file_xml(path).unwrap()
        }
    }

    fn dict(items: Vec<(&str, Value)>) -> Dictionary {
        items.into_iter().map(|(k, v)| (k.to_string(), v)).collect()
    }

    fn at(secs: u64) -> Value {
        Value::Date((UNIX_EPOCH + Duration::from_secs(secs)).into())
    }

    #[test]
    fn info_from_all_plists_and_fallbacks() {
        let root = tempfile::tempdir().unwrap();
        let a = root.path().join("aaaa");
        fs::create_dir(&a).unwrap();
        write(&a.join("Manifest.plist"), dict(vec![("IsEncrypted", Value::Boolean(true)), ("Date", at(5))]), true);
        write(
            &a.join("Info.plist"),
            dict(vec![
                ("Device Name", "Anna's iPhone".into()),
                ("Product Type", "iPhone14,2".into()),
                ("Product Version", "17.4.1".into()),
                ("Last Backup Date", at(300)),
            ]),
            false,
        );
        let b = root.path().join("bbbb");
        fs::create_dir(&b).unwrap();
        let lockdown = dict(vec![
            ("DeviceName", "iPad".into()),
            ("ProductType", "iPad6,11".into()),
            ("ProductVersion", "15.0".into()),
        ]);
        write(&b.join("Manifest.plist"), dict(vec![("Lockdown", Value::Dictionary(lockdown)), ("Date", at(7))]), false);
        write(&b.join("Status.plist"), dict(vec![("Date", at(100))]), true);
        let c = root.path().join("cccc");
        fs::create_dir(&c).unwrap();
        write(&c.join("Manifest.plist"), dict(vec![("Date", at(50))]), true);
        fs::write(c.join("Info.plist"), b"garbage").unwrap();
        // Not backups: no manifest, broken manifest, a file.
        fs::create_dir(root.path().join("empty")).unwrap();
        fs::create_dir(root.path().join("broken")).unwrap();
        fs::write(root.path().join("broken/Manifest.plist"), b"x").unwrap();
        fs::write(root.path().join("file"), b"x").unwrap();

        let list = list_backups(root.path()).unwrap();
        assert_eq!(list.iter().map(|i| i.id.as_str()).collect::<Vec<_>>(), ["aaaa", "bbbb", "cccc"]);
        assert_eq!(
            list[0],
            BackupInfo {
                path: a.clone(),
                id: "aaaa".into(),
                device_name: Some("Anna's iPhone".into()),
                product_type: Some("iPhone14,2".into()),
                ios_version: Some("17.4.1".into()),
                date: Some(300),
                encrypted: true,
            }
        );
        assert_eq!(list[1].device_name.as_deref(), Some("iPad"));
        assert_eq!(list[1].ios_version.as_deref(), Some("15.0"));
        assert_eq!(list[1].date, Some(100));
        assert!(!list[1].encrypted);
        assert_eq!(list[2].date, Some(50));
        assert_eq!(list[2].device_name, None);

        // A backup dir given directly.
        assert_eq!(list_backups(&a).unwrap()[0].id, "aaaa");
        assert!(matches!(read_backup_info(&root.path().join("empty")), Err(Error::NotABackup(_))));
        assert!(matches!(read_backup_info(&root.path().join("broken")), Err(Error::Plist(_))));
        assert!(matches!(list_backups(&root.path().join("missing")), Err(Error::Io(_))));
    }

    #[test]
    fn plist_must_be_dict_and_dates_before_epoch() {
        let dir = tempfile::tempdir().unwrap();
        Value::Array(vec![]).to_file_xml(dir.path().join("Manifest.plist")).unwrap();
        assert!(matches!(read_backup_info(dir.path()), Err(Error::Plist(m)) if m.contains("not a dictionary")));
        let d = dict(vec![("d", Value::Date((UNIX_EPOCH - Duration::from_secs(10)).into()))]);
        assert_eq!(date(&d, "d"), Some(-10));
        assert_eq!(date(&d, "x"), None);
    }

    #[test]
    fn candidate_dirs() {
        let env = |v: &str| Some(OsString::from(format!("/{v}")));
        assert_eq!(
            candidate_backup_dirs("windows", env),
            [
                PathBuf::from("/APPDATA/Apple Computer/MobileSync/Backup"),
                PathBuf::from("/USERPROFILE/Apple/MobileSync/Backup")
            ]
        );
        assert_eq!(
            candidate_backup_dirs("macos", env),
            [PathBuf::from("/HOME/Library/Application Support/MobileSync/Backup")]
        );
        assert!(candidate_backup_dirs("linux", env).is_empty());
        assert!(candidate_backup_dirs("windows", |_| None).is_empty());
        for p in default_backup_dirs() {
            assert!(p.is_dir());
        }
    }
}
