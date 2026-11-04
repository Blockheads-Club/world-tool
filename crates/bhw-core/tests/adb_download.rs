//! adb on first run: downloading Google's Platform-Tools through a fake server (no network).

mod common;

use std::io::Write;
use std::time::Duration;

use bhw_core::adb_tools::{self, entry_rel, managed_adb, unpack, url_for, Fetch, NoFetch};
use bhw_core::{Core, ErrorCode, Stage};
use bhw_device::fake::Reply;
use common::*;

#[test]
fn urls_and_paths() {
    assert_eq!(
        url_for("windows").unwrap(),
        "https://dl.google.com/android/repository/platform-tools-latest-windows.zip"
    );
    assert!(url_for("macos").unwrap().ends_with("-darwin.zip"));
    assert!(url_for("linux").unwrap().ends_with("-linux.zip"));
    assert_eq!(url_for("haiku"), None);
    assert_eq!(entry_rel("platform-tools/adb").unwrap(), std::path::PathBuf::from("adb"));
    assert_eq!(entry_rel("platform-tools/lib64/x.so").unwrap(), std::path::PathBuf::from("lib64/x.so"));
    for bad in ["platform-tools", "platform-tools/", "other/adb", "platform-tools/../x", "/platform-tools/adb", ""] {
        assert_eq!(entry_rel(bad), None, "{bad}");
    }
    assert!(managed_adb(std::path::Path::new("/d")).starts_with("/d/platform-tools"));
    assert!(NoFetch.size("u").is_err() && NoFetch.open("u").is_err());
    assert!(adb_tools::locate(std::path::Path::new("/nonexistent")).is_none() || std::env::var_os("PATH").is_some());
}

#[test]
fn status_with_adb_found() {
    let env = Env::new(&PhoneData::new(&[]));
    let s = env.core().adb_status();
    assert!(s.found && !s.managed);
    assert_eq!(s.path.as_deref(), Some("adb"));
    assert_eq!(s.url, url_for(std::env::consts::OS));
}

fn no_adb(env: &Env) -> Core {
    Core::new(env.config(false))
}

#[test]
fn downloads_unpacks_and_uses_adb() {
    let env = Env::new(&PhoneData::new(&[(A, "One")]));
    let core = no_adb(&env);
    assert!(core.list_devices().iter().all(|d| d.kind != bhw_core::DeviceKind::Android));
    assert!(!core.adb_status().found);
    assert_eq!(core.restart_adb().unwrap_err().code, ErrorCode::AdbMissing);
    // Offline: the size can't be asked, the download fails.
    assert_eq!(core.adb_download_info().unwrap_err().code, ErrorCode::DownloadFailed);
    assert_eq!(core.download_adb("op", &mut |_| {}).unwrap_err().code, ErrorCode::DownloadFailed);

    let zip = platform_tools_zip(true);
    env.fetch.serve(Some(zip.clone()), None);
    let info = core.adb_download_info().unwrap();
    assert_eq!((info.size, info.url.clone()), (Some(zip.len() as u64), url_for(std::env::consts::OS).unwrap()));
    assert!(info.folder.ends_with("platform-tools") && info.terms_url.starts_with("https://developer.android.com"));
    let (ev, mut on) = events();
    let s = core.download_adb("op", &mut on).unwrap();
    assert!(s.found && s.managed);
    let adb = managed_adb(&env.path("data"));
    assert_eq!(s.path.as_deref(), Some(adb.to_str().unwrap()));
    assert!(env.path("data/platform-tools/lib64/libc++.so").is_file());
    assert!(!env.path("data/evil").exists() && !env.path("data/platform-tools/other").exists());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(std::fs::metadata(&adb).unwrap().permissions().mode() & 0o777, 0o755);
        let notice = env.path("data/platform-tools/NOTICE.txt");
        assert_eq!(std::fs::metadata(notice).unwrap().permissions().mode() & 0o777, 0o644);
    }
    let stages: Vec<Stage> = ev.lock().unwrap().iter().map(|p| p.stage).collect();
    assert_eq!(stages.first(), Some(&Stage::Downloading));
    assert!(stages.contains(&Stage::Preparing) && stages.last() == Some(&Stage::Checking));
    // The new adb is used right away.
    assert!(core.list_devices().iter().any(|d| d.id == "S1"));
    assert!(core.diagnostics().contains("platform-tools"));
    // Downloading again replaces the folder.
    std::fs::write(env.path("data/platform-tools/stale"), b"x").unwrap();
    core.download_adb("op", &mut |_| {}).unwrap();
    assert!(!env.path("data/platform-tools/stale").exists());
    // A core started later finds the download first.
    assert_eq!(adb_tools::locate(&env.path("data")).unwrap().program(), adb.as_path());
}

#[test]
fn bad_downloads_change_nothing() {
    let env = Env::with(
        &PhoneData::new(&[]),
        |r| {
            r.once("version", Reply { spawn_error: Some(std::io::ErrorKind::NotFound), ..Reply::default() });
        },
        |_| {},
    );
    let core = no_adb(&env);
    let zip = platform_tools_zip(true);
    let fail = |body: Option<Vec<u8>>, len: Option<Option<u64>>, why: &str| {
        env.fetch.serve(body, len);
        let e = core.download_adb("op", &mut |_| {}).unwrap_err();
        assert_eq!(e.code, ErrorCode::DownloadFailed, "{why}: {e:?}");
        assert!(!core.adb_status().found, "{why}");
        assert!(!env.path("data/platform-tools").exists(), "{why}");
        e
    };
    let e = fail(Some(zip.clone()), None, "adb that does not run");
    assert!(e.detail.unwrap().contains("does not run"));
    fail(Some(zip.clone()), Some(Some(adb_tools::MAX_DOWNLOAD + 1)), "too large");
    fail(Some(zip[..zip.len() / 2].to_vec()), Some(Some(zip.len() as u64)), "cut off");
    fail(Some(b"<html>not a zip</html>".to_vec()), None, "not a zip");
    fail(Some(platform_tools_zip(false)), None, "no adb inside");
    // A stored file whose bytes were changed fails its checksum.
    let mut bad = stored_zip(&[("platform-tools/adb", b"ADBADBADB".to_vec())]);
    let at = bad.windows(9).position(|w| w == b"ADBADBADB").unwrap();
    bad[at] = b'X';
    fail(Some(bad), None, "checksum");
    // Too much data once unpacked.
    fail(Some(big_zip()), None, "unpacks to too much");
    // Not unpacked anywhere either.
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("z.zip"), platform_tools_zip(false)).unwrap();
    assert_eq!(unpack(&dir.path().join("z.zip"), &dir.path().join("out")).unwrap_err().code, ErrorCode::DownloadFailed);
}

fn stored_zip(files: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let mut z = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let o = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (name, data) in files {
        z.start_file(*name, o).unwrap();
        z.write_all(data).unwrap();
    }
    z.finish().unwrap().into_inner()
}

fn big_zip() -> Vec<u8> {
    let mut z = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let o = zip::write::SimpleFileOptions::default().large_file(true);
    z.start_file("platform-tools/huge", o).unwrap();
    let chunk = vec![0u8; 1 << 20];
    for _ in 0..(adb_tools::MAX_UNPACKED >> 20) + 1 {
        z.write_all(&chunk).unwrap();
    }
    z.finish().unwrap().into_inner()
}

/// A body that hands out one byte at a time, slowly.
struct Slow(usize);

impl std::io::Read for Slow {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        std::thread::sleep(Duration::from_millis(5));
        if self.0 == 0 || buf.is_empty() {
            return Ok(0);
        }
        self.0 -= 1;
        buf[0] = 0;
        Ok(1)
    }
}

struct SlowFetch;

impl Fetch for SlowFetch {
    fn size(&self, _url: &str) -> std::io::Result<Option<u64>> {
        Ok(None)
    }
    fn open(&self, _url: &str) -> std::io::Result<(Box<dyn std::io::Read + Send>, Option<u64>)> {
        Ok((Box::new(Slow(10_000)), None))
    }
}

#[test]
fn a_download_can_be_cancelled() {
    let env = Env::new(&PhoneData::new(&[]));
    let core = Core::new(bhw_core::CoreConfig { fetch: std::sync::Arc::new(SlowFetch), ..env.config(false) });
    assert_eq!(core.adb_download_info().unwrap().size, None);
    let e = std::thread::scope(|s| {
        s.spawn(|| {
            std::thread::sleep(Duration::from_millis(100));
            core.cancel("dl");
        });
        core.download_adb("dl", &mut |_| {}).unwrap_err()
    });
    assert_eq!(e.code, ErrorCode::Cancelled);
    assert!(!core.adb_status().found);
}
