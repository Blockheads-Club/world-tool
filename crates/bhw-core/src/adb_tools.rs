//! Google's adb on first run. The app does not ship adb (the SDK Platform-Tools license limits redistribution):
//! when none is found (Android SDK, `PATH`, or an earlier download), the UI offers to download the official
//! Platform-Tools zip from Google and the core unpacks it into the app's data folder (`<data>/platform-tools/`).
//! Networking goes through the [`Fetch`] trait so tests use a fake (no network in tests).

use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{AppError, ErrorCode, Result};
use crate::types::{Progress, Stage};
use crate::util::{unique_id, TempPath, Throttle};
use crate::Core;

/// Google's stable "latest Platform-Tools" download, `{os}` = `windows`, `darwin` or `linux`.
pub const URL_BASE: &str = "https://dl.google.com/android/repository/platform-tools-latest-";
/// Platform-Tools release notes and terms (shown with the download offer).
pub const TERMS_URL: &str = "https://developer.android.com/tools/releases/platform-tools";
/// Refuse downloads larger than this (the zip is about 7–15 MB).
pub const MAX_DOWNLOAD: u64 = 200 << 20;
/// Refuse zips that unpack to more than this.
pub const MAX_UNPACKED: u64 = 500 << 20;
/// The folder inside the zip and inside the app's data folder.
pub const DIR: &str = "platform-tools";

/// Downloads over HTTPS (the app uses a real client; tests a fake).
pub trait Fetch: Send + Sync {
    /// The size the server announces for `url` (`Content-Length` of a HEAD request), if any.
    fn size(&self, url: &str) -> std::io::Result<Option<u64>>;
    /// Starts a GET of `url`: the body and its announced length.
    fn open(&self, url: &str) -> std::io::Result<(Box<dyn Read + Send>, Option<u64>)>;
}

/// A fetcher for builds without networking: every request fails.
#[derive(Debug, Default)]
pub struct NoFetch;

impl Fetch for NoFetch {
    fn size(&self, _url: &str) -> std::io::Result<Option<u64>> {
        Err(std::io::Error::other("downloads are not available"))
    }
    fn open(&self, _url: &str) -> std::io::Result<(Box<dyn Read + Send>, Option<u64>)> {
        Err(std::io::Error::other("downloads are not available"))
    }
}

/// The download URL for an OS (`std::env::consts::OS` names).
pub fn url_for(os: &str) -> Option<String> {
    let name = match os {
        "windows" => "windows",
        "macos" => "darwin",
        "linux" => "linux",
        _ => return None,
    };
    Some(format!("{URL_BASE}{name}.zip"))
}

/// Where a downloaded adb lives: `<data_dir>/platform-tools/adb[.exe]`.
pub fn managed_adb(data_dir: &Path) -> PathBuf {
    data_dir.join(DIR).join(bhw_device::adb::adb_file_name())
}

/// Finds adb as the app does at start: an earlier download first, then the Android SDK, then `PATH`.
pub fn locate(data_dir: &Path) -> Option<bhw_device::Adb> {
    bhw_device::Adb::locate(Some(&managed_adb(data_dir))).ok()
}

/// Whether adb is usable and where it came from (`AdbStatus` in `types.ts`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdbStatus {
    pub found: bool,
    pub path: Option<String>,
    /// adb was downloaded by this app.
    pub managed: bool,
    /// Where a download would come from (`None` on an OS without Platform-Tools).
    pub url: Option<String>,
    pub terms_url: String,
}

/// What the download offer shows before the user confirms.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdbDownload {
    pub url: String,
    pub size: Option<u64>,
    pub terms_url: String,
    /// Destination folder.
    pub folder: String,
}

fn download_failed(e: impl ToString) -> AppError {
    AppError::with(ErrorCode::DownloadFailed, e)
}

/// The path below `platform-tools/` of a zip entry name, if it is a safe relative path there.
pub fn entry_rel(name: &str) -> Option<PathBuf> {
    let p = Path::new(name);
    let mut parts = p.components();
    if parts.next() != Some(Component::Normal(DIR.as_ref())) {
        return None;
    }
    let rest: PathBuf = parts.clone().collect();
    let safe = parts.all(|c| matches!(c, Component::Normal(_)));
    (safe && !rest.as_os_str().is_empty()).then_some(rest)
}

/// Unpacks the `platform-tools/` folder of a Platform-Tools zip into `dest` (keeping Unix modes) and checks that
/// it holds adb. Returns the number of files written.
pub fn unpack(zip_file: &Path, dest: &Path) -> Result<usize> {
    let mut z = zip::ZipArchive::new(std::fs::File::open(zip_file)?).map_err(download_failed)?;
    let mut files = 0;
    let mut total = 0u64;
    for i in 0..z.len() {
        let mut e = z.by_index(i).map_err(download_failed)?;
        let Some(rel) = entry_rel(e.name()) else { continue };
        let out = dest.join(rel);
        if e.is_dir() {
            std::fs::create_dir_all(&out)?;
            continue;
        }
        total += e.size();
        if total > MAX_UNPACKED {
            return Err(download_failed("the download unpacks to too much data"));
        }
        std::fs::create_dir_all(out.parent().unwrap_or(dest))?;
        let mut f = std::fs::File::create(&out)?;
        std::io::copy(&mut e, &mut f).map_err(|x| match x.kind() {
            std::io::ErrorKind::InvalidData => download_failed(x),
            _ => AppError::from(x),
        })?;
        set_mode(&out, e.unix_mode())?;
        files += 1;
    }
    let adb = dest.join(bhw_device::adb::adb_file_name());
    if !adb.is_file() {
        return Err(download_failed("the download has no adb"));
    }
    set_mode(&adb, Some(0o755))?;
    Ok(files)
}

#[cfg(unix)]
fn set_mode(path: &Path, mode: Option<u32>) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let exec = mode.unwrap_or(0o644) & 0o111 != 0;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(if exec { 0o755 } else { 0o644 }))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_mode(_path: &Path, _mode: Option<u32>) -> Result<()> {
    Ok(())
}

impl Core {
    fn adb_url(&self) -> Result<String> {
        let os = std::env::consts::OS;
        url_for(os).ok_or_else(|| AppError::with(ErrorCode::NotSupportedYet, "no Platform-Tools for this OS"))
    }

    /// Whether adb was found and where it is.
    pub fn adb_status(&self) -> AdbStatus {
        let adb = self.adb().ok();
        let path = adb.as_ref().map(|a| a.program().to_path_buf());
        AdbStatus {
            found: adb.is_some(),
            managed: path.as_deref() == Some(managed_adb(&self.cfg.data_dir).as_path()),
            path: path.map(|p| p.display().to_string()),
            url: url_for(std::env::consts::OS),
            terms_url: TERMS_URL.into(),
        }
    }

    /// What downloading adb would fetch (asks the server for the size).
    pub fn adb_download_info(&self) -> Result<AdbDownload> {
        let url = self.adb_url()?;
        let size = self.cfg.fetch.size(&url).map_err(download_failed)?;
        Ok(AdbDownload {
            url,
            size,
            terms_url: TERMS_URL.into(),
            folder: self.cfg.data_dir.join(DIR).display().to_string(),
        })
    }

    /// Downloads Google's Platform-Tools, unpacks them into the app's data folder (replacing an earlier download
    /// only once the new one is complete), checks that adb runs and starts using it.
    pub fn download_adb(&self, op: &str, progress: &mut dyn FnMut(Progress)) -> Result<AdbStatus> {
        let guard = self.ops.start(op);
        let cancel = &guard.cancel;
        let url = self.adb_url()?;
        progress(Progress::bytes(Stage::Downloading, 0, None));
        let (mut body, len) = self.cfg.fetch.open(&url).map_err(download_failed)?;
        if len.is_some_and(|l| l > MAX_DOWNLOAD) {
            return Err(download_failed("the download is too large"));
        }
        let zip_file = TempPath(self.tmp_dir()?.join(format!("{}.zip", unique_id("adb"))));
        {
            let mut out = std::io::BufWriter::new(std::fs::File::create(&zip_file.0)?);
            let (mut buf, mut done, mut t) = (vec![0u8; 64 << 10], 0u64, Throttle::default());
            loop {
                cancel.check()?;
                let n = body.read(&mut buf).map_err(download_failed)?;
                if n == 0 {
                    break;
                }
                done += n as u64;
                if done > MAX_DOWNLOAD {
                    return Err(download_failed("the download is too large"));
                }
                out.write_all(&buf[..n])?;
                if t.pass(done, false) {
                    progress(Progress::bytes(Stage::Downloading, done, len));
                }
            }
            out.flush()?;
            if len.is_some_and(|l| l != done) {
                return Err(download_failed(format!("the download stopped after {done} bytes")));
            }
            progress(Progress::bytes(Stage::Downloading, done, Some(done)));
        }
        progress(Progress::stage(Stage::Preparing));
        std::fs::create_dir_all(&self.cfg.data_dir)?;
        let staged = TempPath(self.cfg.data_dir.join(format!(".{DIR}-{}", unique_id(""))));
        unpack(&zip_file.0, &staged.0)?;
        cancel.check()?;
        progress(Progress::stage(Stage::Checking));
        let probe = (self.cfg.make_adb)(&staged.0.join(bhw_device::adb::adb_file_name()));
        probe.version().map_err(|e| download_failed(format!("the downloaded adb does not run: {e}")))?;
        let dir = self.cfg.data_dir.join(DIR);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::rename(&staged.0, &dir)?;
        *crate::util::lock(&self.adb) = Some((self.cfg.make_adb)(&managed_adb(&self.cfg.data_dir)));
        Ok(self.adb_status())
    }
}
