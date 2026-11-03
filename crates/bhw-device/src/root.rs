//! Root access: copy the game's saves straight out of (and into) its private data directory with `tar` over
//! `adb exec-out` / `adb exec-in`.

use crate::adb::{sh_quote, Adb, RootStyle, PACKAGE};
use crate::error::{DeviceError, Result};
use crate::process::wait_until;
use crate::transfer::{CancelToken, IfRunning, Phase, Progress, ProgressFn, Watchdog};
use std::io::{self, Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

/// Where the game keeps its saves on this device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RootSaves {
    /// The directory holding `saves/` and the global `game_db`
    /// (normally `/data/data/com.noodlecake.blockheads/files/Library/Application Support`).
    pub support_dir: String,
    /// `<support_dir>/saves`.
    pub saves_dir: String,
    /// World ids (32 hex digits) that have a `world_db`.
    pub worlds: Vec<String>,
}

/// Whether `s` is a 32-hex-digit world id.
pub fn is_world_id(s: &str) -> bool {
    s.len() == 32 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Parses `find ... -name world_db` output (first saves dir found wins).
pub fn parse_world_dbs(text: &str) -> Option<RootSaves> {
    let mut found: Option<RootSaves> = None;
    for line in text.lines().map(str::trim) {
        let Some(world) = line.strip_suffix("/world_db") else { continue };
        let Some((saves, id)) = world.rsplit_once('/') else { continue };
        let Some(support) = saves.strip_suffix("/saves") else { continue };
        if !is_world_id(id) {
            continue;
        }
        let entry = found.get_or_insert_with(|| RootSaves {
            support_dir: support.to_string(),
            saves_dir: saves.to_string(),
            worlds: Vec::new(),
        });
        if entry.saves_dir == saves && !entry.worlds.iter().any(|w| w == id) {
            entry.worlds.push(id.to_string());
        }
    }
    if let Some(f) = &mut found {
        f.worlds.sort();
    }
    found
}

/// Counts bytes read and reports progress.
struct Counting<'a, R> {
    inner: R,
    bytes: Arc<AtomicU64>,
    progress: ProgressFn<'a>,
}

impl<R: Read> Read for Counting<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(buf)?;
        let total = self.bytes.fetch_add(n as u64, Ordering::SeqCst) + n as u64;
        (self.progress)(Progress { phase: Phase::Transferring, bytes: total, total: None });
        Ok(n)
    }
}

impl<W: Write> Write for Counting<'_, W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = self.inner.write(buf)?;
        let total = self.bytes.fetch_add(n as u64, Ordering::SeqCst) + n as u64;
        (self.progress)(Progress { phase: Phase::Transferring, bytes: total, total: None });
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

/// Default app data roots searched for saves.
const DATA_ROOTS: [&str; 2] = ["/data/data", "/data/user/0"];

impl Adb {
    /// Finds the saves directory by searching the app's data for `saves/<32 hex>/world_db` as root. When there
    /// are no worlds yet, falls back to `files/Library/Application Support/saves` if it exists.
    pub fn root_find_saves(&self, serial: &str, root: RootStyle) -> Result<RootSaves> {
        let dirs: Vec<String> = DATA_ROOTS.iter().map(|r| format!("{r}/{PACKAGE}")).collect();
        let cmd = format!(
            "for d in {}; do [ -d \"$d\" ] && find \"$d\" -type d -name world_db 2>/dev/null; done; true",
            dirs.join(" ")
        );
        let out = self.root_shell(serial, root, &cmd)?;
        if let Some(s) = parse_world_dbs(&out) {
            return Ok(s);
        }
        let support = format!("{}/files/Library/Application Support", dirs[0]);
        let saves = format!("{support}/saves");
        let out = self.root_shell(serial, root, &format!("[ -d {} ] && echo yes; true", sh_quote(&saves)))?;
        if out.trim() == "yes" {
            Ok(RootSaves { support_dir: support, saves_dir: saves, worlds: Vec::new() })
        } else {
            Err(DeviceError::SavesNotFound)
        }
    }

    /// Runs a root shell command; a failure to get root becomes [`DeviceError::RootUnavailable`].
    fn root_shell(&self, serial: &str, root: RootStyle, cmd: &str) -> Result<String> {
        match self.shell_timeout(serial, &root.wrap(cmd), self.timeouts.confirm) {
            Ok(out) if out.code == 0 => Ok(out.stdout),
            Ok(_) | Err(DeviceError::Timeout) => Err(DeviceError::RootUnavailable),
            Err(e) => Err(e),
        }
    }

    /// Copies `<support_dir>/<rel>` (e.g. `saves`, `saves/<id>`, `game_db`) from the device into `dest`
    /// (keeping the `rel` path below it). Returns the number of files written.
    #[allow(clippy::too_many_arguments)]
    pub fn root_pull(
        &self,
        serial: &str,
        root: RootStyle,
        support_dir: &str,
        rel: &str,
        dest: &Path,
        progress: ProgressFn<'_>,
        cancel: &CancelToken,
    ) -> Result<usize> {
        std::fs::create_dir_all(dest).map_err(crate::error::local_write_error)?;
        let cmd = root.wrap(&format!("tar -C {} -cf - {} 2>/dev/null", sh_quote(support_dir), sh_quote(rel)));
        let child = self
            .runner()
            .spawn(self.program(), &Adb::device_args(serial, &["exec-out", &cmd]), false)
            .map_err(Adb::spawn_error)?;
        let stderr = crate::process::collect(child.take_stderr());
        let stdout = child.take_stdout().unwrap_or_else(|| Box::new(io::empty()));
        let dog = Watchdog::start(child.clone(), cancel.clone(), self.timeouts.confirm, self.timeouts.stall);
        let reader = Counting { inner: stdout, bytes: dog.bytes.clone(), progress };
        let unpacked = unpack(reader, dest);
        let moved = dog.bytes.load(Ordering::SeqCst);
        child.kill();
        wait_until(&*child, Instant::now() + self.timeouts.command)?;
        let err = stderr.join().unwrap_or_default();
        if let Some(e) = dog.finish() {
            return Err(e);
        }
        if let Some(e) = crate::error::classify_adb_message(&String::from_utf8_lossy(&err), serial) {
            return Err(e);
        }
        match unpacked {
            Ok(0) if moved == 0 => Err(DeviceError::RootUnavailable),
            Ok(0) => Err(DeviceError::SavesNotFound),
            Ok(n) => Ok(n),
            Err(e @ DeviceError::DiskFull) => Err(e),
            Err(_) => Err(DeviceError::Disconnected),
        }
    }

    /// Replaces `<support_dir>/<rel>` on the device with the contents of the local directory `src`, then gives
    /// everything below `support_dir` back to the app (owner, SELinux context). Stops or refuses to touch a running
    /// game first. Returns the bytes sent.
    #[allow(clippy::too_many_arguments)]
    pub fn root_push(
        &self,
        serial: &str,
        root: RootStyle,
        src: &Path,
        support_dir: &str,
        rel: &str,
        if_running: IfRunning,
        progress: ProgressFn<'_>,
        cancel: &CancelToken,
    ) -> Result<u64> {
        if !src.is_dir() {
            return Err(DeviceError::PathNotFound(src.to_path_buf()));
        }
        self.check_running(serial, if_running)?;
        let support = sh_quote(support_dir);
        let owner = self.root_shell(serial, root, &format!("stat -c %u:%g {support}"))?.trim().to_string();
        let tmp = sh_quote(&format!("{support_dir}/.bhw-incoming"));
        let cmd = root.wrap(&format!("rm -rf {tmp} && mkdir -p {tmp} && tar -C {tmp} -xf - 2>/dev/null"));
        let child = self
            .runner()
            .spawn(self.program(), &Adb::device_args(serial, &["exec-in", &cmd]), true)
            .map_err(Adb::spawn_error)?;
        let stderr = crate::process::collect(child.take_stderr());
        let stdout = crate::process::collect(child.take_stdout());
        let dog = Watchdog::start(child.clone(), cancel.clone(), self.timeouts.confirm, self.timeouts.stall);
        let stdin = child.take_stdin().unwrap_or_else(|| Box::new(io::sink()));
        let sent = pack(Counting { inner: stdin, bytes: dog.bytes.clone(), progress }, src);
        let moved = dog.bytes.load(Ordering::SeqCst);
        let code = wait_until(&*child, Instant::now() + self.timeouts.confirm)?;
        let err = stderr.join().unwrap_or_default();
        drop(stdout.join());
        if let Some(e) = dog.finish() {
            return Err(e);
        }
        if let Some(e) = crate::error::classify_adb_message(&String::from_utf8_lossy(&err), serial) {
            return Err(e);
        }
        match (sent, code) {
            (Ok(()), Some(_)) => {}
            (Err(e @ DeviceError::Io(_)), _) if moved == 0 => return Err(e),
            _ => return Err(DeviceError::Disconnected),
        }
        let dest = sh_quote(&format!("{support_dir}/{rel}"));
        let finish = format!(
            "[ -d {tmp}/x ] && rm -rf {dest} && mkdir -p \"$(dirname {dest})\" && mv {tmp}/x {dest} && rm -rf {tmp} \
             && chown -R {owner} {support} && chmod -R u+rwX {dest} \
             && {{ restorecon -R {support} 2>/dev/null; \
             ctx=$(ls -Zd {support} | cut -d' ' -f1); [ -n \"$ctx\" ] && chcon -R \"$ctx\" {support} 2>/dev/null; true; }}"
        );
        match self.shell_timeout(serial, &root.wrap(&finish), self.timeouts.confirm)? {
            out if out.code == 0 => Ok(moved),
            out => Err(DeviceError::Command { code: out.code, message: out.stdout.trim().to_string() }),
        }
    }
}

/// Unpacks a tar stream into `dest`; returns the number of regular files written.
fn unpack(reader: impl Read, dest: &Path) -> Result<usize> {
    let mut archive = tar::Archive::new(reader);
    let mut files = 0;
    for entry in archive.entries()? {
        let mut entry = entry?;
        let is_file = entry.header().entry_type().is_file();
        if entry.unpack_in(dest).map_err(crate::error::local_write_error)? && is_file {
            files += 1;
        }
    }
    Ok(files)
}

/// Writes `src` as a tar stream (entries below `x/`).
fn pack(out: impl Write, src: &Path) -> Result<()> {
    let mut builder = tar::Builder::new(out);
    builder.follow_symlinks(false);
    builder.append_dir_all("x", src)?;
    builder.into_inner()?.flush()?;
    Ok(())
}
