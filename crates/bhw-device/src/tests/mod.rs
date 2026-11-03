//! Unit tests with scripted adb processes.
#![allow(clippy::bool_assert_comparison, clippy::type_complexity)]

mod adb_tests;
mod device_tests;
mod misc_tests;
mod pc_tests;
mod process_tests;
mod root_tests;
mod transfer_tests;
mod wifi_tests;

use crate::adb::{Adb, Timeouts};
use crate::fake::{Reply, ScriptedRunner};
use std::io::Write;
use std::sync::Arc;
use std::time::Duration;

/// An adb on a scripted runner with short timeouts.
pub fn adb(r: &ScriptedRunner) -> Adb {
    let mut a = Adb::new("adb", Arc::new(r.clone()));
    a.timeouts = Timeouts {
        command: Duration::from_secs(5),
        confirm: Duration::from_millis(300),
        stall: Duration::from_millis(300),
    };
    a
}

/// A device shell answer: stdout plus the exit-code marker.
pub fn sh(stdout: &str, code: i32) -> Reply {
    Reply::ok(format!("{stdout}__BHW_RC={code}\n"))
}

/// A tar stream with the given (path, content) files.
pub fn tar_of(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut b = tar::Builder::new(Vec::new());
    for (path, data) in files {
        let mut h = tar::Header::new_gnu();
        h.set_size(data.len() as u64);
        h.set_mode(0o644);
        let name = &mut h.as_old_mut().name;
        name[..path.len()].copy_from_slice(path.as_bytes());
        h.set_cksum();
        b.append(&h, *data).unwrap();
    }
    b.into_inner().unwrap()
}

/// An unencrypted, compressed `.ab` holding `tar`.
pub fn ab_of(tar: &[u8]) -> Vec<u8> {
    let mut out = b"ANDROID BACKUP\n5\n1\nnone\n".to_vec();
    let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    z.write_all(tar).unwrap();
    out.extend(z.finish().unwrap());
    out
}

/// A non-empty backup.
pub fn good_ab() -> Vec<u8> {
    ab_of(&tar_of(&[("apps/com.noodlecake.blockheads/f/x", b"hello")]))
}
