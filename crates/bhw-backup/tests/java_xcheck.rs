//! Cross-checks the AES-256 backup encryption against an independent Java implementation (JDK crypto, see
//! `tests/xcheck/AbXcheck.java`). Skips when `java` is not installed.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

use bhw_backup::{open_backup, BackupOptions, BackupWriter, EntryKind, Error, TarReader, TarWriter};

fn java() -> bool {
    Command::new("java").arg("-version").output().map(|o| o.status.success()).unwrap_or(false)
}

fn run_java(args: &[&str]) {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/xcheck/AbXcheck.java");
    let st = Command::new("java").arg(src).args(args).status().unwrap();
    assert!(st.success(), "java {args:?} failed");
}

fn sample_tar() -> Vec<u8> {
    let mut out = Vec::new();
    let mut w = TarWriter::new(&mut out);
    let data: Vec<u8> = (0..200_000u32).map(|i| (i * 7 % 251) as u8).collect();
    let long = format!(
        "apps/com.noodlecake.blockheads/f/Library/Application Support/saves/{}/world_db/data.mdb",
        "c".repeat(32)
    );
    for (p, d) in [("apps/com.noodlecake.blockheads/_manifest", &b"1\n"[..]), (long.as_str(), &data)] {
        let mut r: &[u8] = d;
        w.append(p, EntryKind::File, 0o600, 1_700_000_000, d.len() as u64, &mut r).unwrap();
    }
    w.finish().unwrap();
    out
}

fn tmp(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("bhw-xcheck-{}-{name}", std::process::id()))
}

#[test]
fn rust_and_java_agree() {
    if !java() {
        eprintln!("java not found, skipping");
        return;
    }
    let pw = "pässwörd 1";
    let tar = sample_tar();
    // Rust encrypts, Java decrypts (several times: each run draws a new master key, and keys with bytes >= 0x80
    // exercise the signed-char checksum quirk).
    for version in [2, 5] {
        let mut ab = Vec::new();
        let opts = BackupOptions { version, password: Some(pw.into()), ..Default::default() };
        let mut w = BackupWriter::create(Box::new(&mut ab), &opts).unwrap();
        std::io::Write::write_all(&mut w, &tar).unwrap();
        w.finish().unwrap();
        let (ab_path, tar_path) = (tmp("r.ab"), tmp("r.tar"));
        std::fs::write(&ab_path, &ab).unwrap();
        run_java(&["decrypt", pw, ab_path.to_str().unwrap(), tar_path.to_str().unwrap()]);
        assert_eq!(std::fs::read(&tar_path).unwrap(), tar);
        std::fs::remove_file(ab_path).unwrap();
        std::fs::remove_file(tar_path).unwrap();
    }
    // Java encrypts, Rust decrypts.
    let (tar_path, ab_path) = (tmp("j.tar"), tmp("j.ab"));
    std::fs::write(&tar_path, &tar).unwrap();
    run_java(&["encrypt", pw, "5", tar_path.to_str().unwrap(), ab_path.to_str().unwrap()]);
    let ab = std::fs::read(&ab_path).unwrap();
    std::fs::remove_file(ab_path).unwrap();
    std::fs::remove_file(tar_path).unwrap();
    let (_, mut s) = open_backup(Box::new(&ab[..]), Some(pw)).unwrap();
    let mut back = Vec::new();
    s.read_to_end(&mut back).unwrap();
    assert_eq!(back, tar);
    let mut r = TarReader::new(Box::new(&back[..]));
    assert!(r.next_header().unwrap().unwrap().path.ends_with("_manifest"));
    assert!(matches!(open_backup(Box::new(&ab[..]), Some("wrong")), Err(Error::WrongPassword)));
}
