//! Validation and extraction of the user's own The Blockheads APKs.
//!
//! Skipped unless `BHW_APK` points at an APK (or `.apks`/`.xapk`, or a folder of split APKs) and/or `BHW_APK_DIR`
//! points at a folder whose `*.apk` files are each checked (APKs of other apps or versions there must be rejected
//! with `WrongApp` / `Unsupported` and are skipped). Files are extracted into a temporary directory that is
//! deleted afterwards; nothing is written anywhere else.

use bhw_assets::{apk, CancelToken, Error, GameFiles, Status, PACKAGE, REQUIRED_FILES, SUPPORTED_VERSIONS};
use std::path::PathBuf;

/// `(path, must_be_supported)`.
fn sources() -> Vec<(PathBuf, bool)> {
    let mut out = Vec::new();
    if let Some(p) = std::env::var_os("BHW_APK") {
        out.push((PathBuf::from(p), true));
    }
    if let Some(d) = std::env::var_os("BHW_APK_DIR") {
        let mut v: Vec<PathBuf> = std::fs::read_dir(d)
            .expect("BHW_APK_DIR is not readable")
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("apk")))
            .collect();
        v.sort();
        out.extend(v.into_iter().map(|p| (p, false)));
    }
    out
}

#[test]
fn real_apks_validate_and_extract() {
    let sources = sources();
    if sources.is_empty() {
        eprintln!("BHW_APK / BHW_APK_DIR not set, skipping");
        return;
    }
    for (src, must) in sources {
        let info = match apk::inspect(&src) {
            Ok(i) => i,
            Err(e @ (Error::WrongApp(_) | Error::Unsupported(_))) if !must => {
                eprintln!("{}: rejected as expected: {e}", src.display());
                continue;
            }
            Err(e) => panic!("{}: {e}", src.display()),
        };
        assert_eq!(info.package, PACKAGE);
        assert!(SUPPORTED_VERSIONS.contains(&info.version_name.as_str()));
        eprintln!(
            "{}: {} {} code {:?} sha256 {} parts {:?}: {} files, {} bytes",
            src.display(),
            info.package,
            info.version_name,
            info.version_code,
            info.sha256,
            info.parts,
            info.files,
            info.bytes
        );
        let cache = tempfile::tempdir().unwrap();
        let gf = GameFiles::new(cache.path());
        let mut calls = 0;
        let m = gf.extract(&src, |_| calls += 1, &CancelToken::new()).unwrap();
        assert_eq!(calls, m.files.len() + 1);
        assert_eq!((m.files.len(), m.total_bytes), (info.files, info.bytes));
        for f in REQUIRED_FILES {
            let e = m.files.iter().find(|e| e.path == f).unwrap();
            eprintln!("  {f}: {}x{} {} bytes", e.width.unwrap(), e.height.unwrap(), e.size);
        }
        let tm = m.files.iter().find(|e| e.path == "HDTex/TileMap.png").unwrap();
        assert_eq!((tm.width, tm.height), (Some(2048), Some(2048)));
        assert!(m.files.iter().all(|f| !f.path.ends_with(".wav") && !f.path.ends_with(".mp4")));
        assert!(matches!(gf.status(), Status::Ready { .. }));
        gf.clear().unwrap();
    }
}
