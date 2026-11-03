//! Streaming transfers with progress, cancel and timeouts: `adb backup` / `adb restore` of the game.
//!
//! Every transfer runs the byte copy on the calling thread and a watchdog thread that kills the adb process when
//! the caller cancels, nobody confirms on the phone in time, or the stream stalls.

use crate::adb::{Adb, PACKAGE};
use crate::error::{classify_adb_message, local_write_error, DeviceError, Result};
use crate::process::{collect, wait_until, Child, TICK};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Shared cancel flag.
#[derive(Debug, Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    /// Not cancelled.
    pub fn new() -> CancelToken {
        CancelToken::default()
    }
    /// Cancels.
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }
    /// Whether cancelled.
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

/// Where a transfer is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// Copying the input to a local temporary file (restore via `adb restore`).
    Staging,
    /// Waiting for the user to confirm on the phone.
    WaitingForPhone,
    /// Bytes are moving.
    Transferring,
    /// Finished.
    Done,
}

/// A progress report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Progress {
    pub phase: Phase,
    /// Bytes moved so far in this phase.
    pub bytes: u64,
    /// Total bytes when known.
    pub total: Option<u64>,
}

/// Progress callback.
pub type ProgressFn<'a> = &'a mut dyn FnMut(Progress);

/// What to do when the game is running on the phone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IfRunning {
    /// Fail with [`DeviceError::GameRunning`].
    #[default]
    Fail,
    /// Stop it first: background + `am kill` before a backup, `am force-stop` before a restore.
    Stop,
    /// Go ahead anyway.
    Ignore,
}

/// How the `.ab` stream moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AbMethod {
    /// `adb backup -f <temp file>` (tailed while it grows) / `adb restore <temp file>`: the standard commands.
    #[default]
    AdbCommand,
    /// `adb exec-out bu backup` / `adb exec-in bu restore`: streams through stdout/stdin, no temporary file.
    /// Not verified on a phone yet.
    BuStream,
}

/// Options for [`Adb::backup`] / [`Adb::restore`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AbOptions {
    pub method: AbMethod,
    pub if_running: IfRunning,
}

/// Result of a backup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackupSummary {
    /// Bytes written to the output.
    pub bytes: u64,
    /// The phone encrypted it (a backup password is set there).
    pub encrypted: bool,
}

const REASON_NONE: u8 = 0;
const REASON_CANCEL: u8 = 1;
const REASON_CONFIRM: u8 = 2;
const REASON_STALL: u8 = 3;

/// Kills the child when cancelled or timed out; the reason is kept for the copying thread.
pub(crate) struct Watchdog {
    pub bytes: Arc<AtomicU64>,
    reason: Arc<AtomicU8>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Watchdog {
    /// Watches `child`: no byte within `confirm` → confirm timeout; no new byte for `stall` → stall.
    pub fn start(child: Arc<dyn Child>, cancel: CancelToken, confirm: Duration, stall: Duration) -> Watchdog {
        let bytes = Arc::new(AtomicU64::new(0));
        let reason = Arc::new(AtomicU8::new(REASON_NONE));
        let stop = Arc::new(AtomicBool::new(false));
        let (b, r, s) = (bytes.clone(), reason.clone(), stop.clone());
        let thread = std::thread::spawn(move || {
            let start = Instant::now();
            let (mut last, mut last_change) = (0, start);
            while !s.load(Ordering::SeqCst) {
                let now = Instant::now();
                let n = b.load(Ordering::SeqCst);
                if n != last {
                    (last, last_change) = (n, now);
                }
                let why = if cancel.is_cancelled() {
                    REASON_CANCEL
                } else if n == 0 && now - start > confirm {
                    REASON_CONFIRM
                } else if n > 0 && now - last_change > stall {
                    REASON_STALL
                } else {
                    REASON_NONE
                };
                if why != REASON_NONE {
                    r.store(why, Ordering::SeqCst);
                    child.kill();
                    return;
                }
                std::thread::sleep(TICK);
            }
        });
        Watchdog { bytes, reason, stop, thread: Some(thread) }
    }

    /// Stops watching and returns the error the watchdog caused, if it fired.
    pub fn finish(mut self) -> Option<DeviceError> {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
        match self.reason.load(Ordering::SeqCst) {
            REASON_CANCEL => Some(DeviceError::Cancelled),
            REASON_CONFIRM => Some(DeviceError::ConfirmTimeout),
            REASON_STALL => Some(DeviceError::Disconnected),
            _ => None,
        }
    }
}

/// Counts bytes read and reports progress.
pub(crate) struct Counting<'a, R> {
    pub inner: R,
    pub bytes: Arc<AtomicU64>,
    pub progress: ProgressFn<'a>,
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

/// Reads a file another process is still writing; EOF once `child` has exited and nothing more is there.
struct TailReader {
    path: PathBuf,
    file: Option<std::fs::File>,
    child: Arc<dyn Child>,
}

impl Read for TailReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        loop {
            if self.file.is_none() {
                self.file = std::fs::File::open(&self.path).ok();
            }
            let exited = self.child.try_wait()?.is_some();
            if let Some(f) = &mut self.file {
                let n = f.read(buf)?;
                if n > 0 {
                    return Ok(n);
                }
            }
            if exited {
                return Ok(0);
            }
            std::thread::sleep(TICK);
        }
    }
}

/// The first bytes kept to check the backup header.
const HEAD_KEEP: usize = 64 * 1024;

/// Copies `src` to `out`, counting into `bytes`; keeps the first [`HEAD_KEEP`] bytes. Read errors are returned as
/// `Err(Ok(e))`, write errors as `Err(Err(e))`.
fn pump(
    src: &mut dyn Read,
    out: &mut dyn Write,
    bytes: &AtomicU64,
    progress: ProgressFn<'_>,
) -> std::result::Result<Vec<u8>, std::result::Result<io::Error, DeviceError>> {
    let mut head = Vec::new();
    let mut buf = vec![0u8; 64 * 1024];
    let mut total = 0u64;
    loop {
        let n = match src.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(Ok(e)),
        };
        out.write_all(&buf[..n]).map_err(|e| Err(local_write_error(e)))?;
        if head.len() < HEAD_KEEP {
            head.extend_from_slice(&buf[..n.min(HEAD_KEEP - head.len())]);
        }
        total += n as u64;
        bytes.store(total, Ordering::SeqCst);
        progress(Progress { phase: Phase::Transferring, bytes: total, total: None });
    }
    out.flush().map_err(|e| Err(local_write_error(e)))?;
    Ok(head)
}

/// Checks the start of a backup stream: `Ok(encrypted)` for a backup with data, else why it is unusable.
/// `head` is the start of the stream (all of it when `total <= head.len()`).
pub fn check_backup(head: &[u8], total: u64) -> Result<bool> {
    if total == 0 {
        return Err(DeviceError::DeclinedOnPhone);
    }
    let mut lines = head.splitn(5, |&b| b == b'\n');
    let magic = lines.next().unwrap_or_default();
    if magic != b"ANDROID BACKUP" {
        return Err(if total < 15 && b"ANDROID BACKUP".starts_with(head) {
            DeviceError::EmptyBackup
        } else {
            DeviceError::NotABackup
        });
    }
    let (Some(_version), Some(compressed), Some(enc)) = (lines.next(), lines.next(), lines.next()) else {
        return Err(DeviceError::EmptyBackup);
    };
    let complete = total as usize <= head.len();
    let rest = lines.next().unwrap_or_default();
    if enc != b"none" {
        // Encrypted: user salt, checksum salt, rounds, IV, master key blob, then the payload. An empty tar
        // (1 KiB of zeros) deflates to ~20 bytes and pads to 32.
        let payload = rest.splitn(6, |&b| b == b'\n').nth(5).map_or(0, <[u8]>::len);
        return if complete && payload <= 48 { Err(DeviceError::EmptyBackup) } else { Ok(true) };
    }
    if !complete {
        return Ok(false);
    }
    let tar = if compressed == b"1" {
        let mut v = Vec::new();
        // A truncated stream still yields what was there; zero bytes then counts as empty.
        let _ = flate2::read::ZlibDecoder::new(rest).read_to_end(&mut v);
        v
    } else {
        rest.to_vec()
    };
    if tar.iter().all(|&b| b == 0) {
        Err(DeviceError::EmptyBackup)
    } else {
        Ok(false)
    }
}

/// Turns adb's exit code and stderr into an error (`None` when fine).
fn finish_error(code: i32, stderr: &[u8], serial: &str, moved: u64) -> Option<DeviceError> {
    let text = String::from_utf8_lossy(stderr);
    if let Some(e) = classify_adb_message(&text, serial) {
        return Some(match e {
            DeviceError::DeviceNotFound { .. } | DeviceError::Offline { .. } if moved > 0 => DeviceError::Disconnected,
            e => e,
        });
    }
    if code == 0 {
        None
    } else if moved > 0 {
        Some(DeviceError::Disconnected)
    } else {
        Some(DeviceError::Command { code, message: text.trim().to_string() })
    }
}

impl Adb {
    /// Applies the running-game policy (stopping with [`Adb::stop_game`], which keeps the app backup-eligible).
    pub fn check_running(&self, serial: &str, policy: IfRunning) -> Result<()> {
        self.running_policy(serial, policy, false)
    }

    fn running_policy(&self, serial: &str, policy: IfRunning, force: bool) -> Result<()> {
        if policy == IfRunning::Ignore || !self.game_running(serial)? {
            return Ok(());
        }
        if policy == IfRunning::Stop {
            if force {
                self.force_stop_game(serial)
            } else {
                self.stop_game(serial)
            }
        } else {
            Err(DeviceError::GameRunning)
        }
    }

    /// Backs up the game (`adb backup -noapk com.noodlecake.blockheads`) into `out` as a raw `.ab` stream. The user
    /// must confirm on the phone (optionally entering a backup password there).
    pub fn backup(
        &self,
        serial: &str,
        out: &mut dyn Write,
        opts: AbOptions,
        progress: ProgressFn<'_>,
        cancel: &CancelToken,
    ) -> Result<BackupSummary> {
        if self.game_info(serial)?.stopped {
            return Err(DeviceError::GameStopped);
        }
        self.check_running(serial, opts.if_running)?;
        let tmp = tempfile::tempdir()?;
        let file = tmp.path().join("backup.ab");
        let args = match opts.method {
            AbMethod::AdbCommand => Adb::device_args(
                serial,
                &["backup", "-f", &file.to_string_lossy(), "-noapk", "-noobb", "-noshared", PACKAGE],
            ),
            AbMethod::BuStream => Adb::device_args(serial, &["exec-out", &format!("bu backup -noapk {PACKAGE}")]),
        };
        progress(Progress { phase: Phase::WaitingForPhone, bytes: 0, total: None });
        let child = self.runner().spawn(self.program(), &args, false).map_err(Adb::spawn_error)?;
        let stderr = collect(child.take_stderr());
        let stdout = child.take_stdout();
        let (mut src, stdout_sink): (Box<dyn Read + Send>, _) = match opts.method {
            AbMethod::AdbCommand => {
                (Box::new(TailReader { path: file, file: None, child: child.clone() }), Some(collect(stdout)))
            }
            AbMethod::BuStream => (stdout.unwrap_or_else(crate::process::empty_reader), None),
        };
        let dog = Watchdog::start(child.clone(), cancel.clone(), self.timeouts.confirm, self.timeouts.stall);
        let pumped = pump(&mut src, out, &dog.bytes, progress);
        let moved = dog.bytes.load(Ordering::SeqCst);
        if pumped.is_err() {
            child.kill();
        }
        let code = wait_until(&*child, Instant::now() + self.timeouts.command)?.unwrap_or(-1);
        let err = stderr.join().unwrap_or_default();
        drop(stdout_sink.map(|h| h.join()));
        if let Some(e) = dog.finish() {
            return Err(e);
        }
        let head = match pumped {
            Ok(h) => h,
            Err(Err(e)) => return Err(e),
            Err(Ok(_)) => return Err(DeviceError::Disconnected),
        };
        if let Some(e) = finish_error(code, &err, serial, moved) {
            return Err(e);
        }
        let encrypted = check_backup(&head, moved)?;
        progress(Progress { phase: Phase::Done, bytes: moved, total: Some(moved) });
        Ok(BackupSummary { bytes: moved, encrypted })
    }

    /// [`Adb::backup`] into a local file (removed again on failure).
    pub fn backup_to_file(
        &self,
        serial: &str,
        path: &Path,
        opts: AbOptions,
        progress: ProgressFn<'_>,
        cancel: &CancelToken,
    ) -> Result<BackupSummary> {
        let mut f = io::BufWriter::new(std::fs::File::create(path).map_err(local_write_error)?);
        let r = self.backup(serial, &mut f, opts, progress, cancel);
        drop(f);
        if r.is_err() {
            let _ = std::fs::remove_file(path);
        }
        r
    }

    /// Restores a raw `.ab` stream to the phone (`adb restore`). The user must confirm on the phone. Android does
    /// not report whether the restore was declined: read the device again afterwards to verify.
    pub fn restore(
        &self,
        serial: &str,
        input: &mut dyn Read,
        opts: AbOptions,
        progress: ProgressFn<'_>,
        cancel: &CancelToken,
    ) -> Result<u64> {
        // Force-stop is fine here: the restore wipes the app's data anyway.
        self.running_policy(serial, opts.if_running, true)?;
        let tmp = tempfile::tempdir()?;
        let (args, staged) = match opts.method {
            AbMethod::AdbCommand => {
                let file = tmp.path().join("restore.ab");
                let mut f = io::BufWriter::new(std::fs::File::create(&file).map_err(local_write_error)?);
                let n = copy_cancellable(input, &mut f, cancel, &mut |b| {
                    progress(Progress { phase: Phase::Staging, bytes: b, total: None })
                })?;
                drop(f);
                (Adb::device_args(serial, &["restore", &file.to_string_lossy()]), Some(n))
            }
            AbMethod::BuStream => (Adb::device_args(serial, &["exec-in", "bu restore"]), None),
        };
        progress(Progress { phase: Phase::WaitingForPhone, bytes: 0, total: staged });
        let child = self.runner().spawn(self.program(), &args, staged.is_none()).map_err(Adb::spawn_error)?;
        let stderr = collect(child.take_stderr());
        let stdout = collect(child.take_stdout());
        let (moved, copy_err) = match staged {
            Some(n) => {
                // adb restore blocks until the phone has read everything and shows no progress: the watchdog only
                // handles cancel, and twice the confirm timeout covers confirming plus the transfer.
                let dog = Watchdog::start(child.clone(), cancel.clone(), Duration::MAX, Duration::MAX);
                let code = wait_until(&*child, Instant::now() + self.timeouts.confirm * 2)?;
                if let Some(e) = dog.finish() {
                    return Err(e);
                }
                (n, code.is_none().then_some(DeviceError::ConfirmTimeout))
            }
            None => {
                let dog = Watchdog::start(child.clone(), cancel.clone(), self.timeouts.confirm, self.timeouts.stall);
                let stdin = child.take_stdin().unwrap_or_else(crate::process::null_writer);
                let mut stdin = Counting { inner: stdin, bytes: dog.bytes.clone(), progress: &mut *progress };
                let r = copy_cancellable(input, &mut stdin, &CancelToken::new(), &mut |_| {});
                drop(stdin);
                let moved = dog.bytes.load(Ordering::SeqCst);
                let code = wait_until(&*child, Instant::now() + self.timeouts.confirm)?;
                if let Some(e) = dog.finish() {
                    return Err(e);
                }
                (moved, r.err().or(code.is_none().then_some(DeviceError::ConfirmTimeout)))
            }
        };
        if let Some(e) = copy_err {
            child.kill();
            return Err(e);
        }
        let code = child.try_wait()?.unwrap_or(-1);
        let mut msg = stderr.join().unwrap_or_default();
        msg.extend(stdout.join().unwrap_or_default());
        if let Some(e) = finish_error(code, &msg, serial, moved) {
            return Err(e);
        }
        progress(Progress { phase: Phase::Done, bytes: moved, total: Some(moved) });
        Ok(moved)
    }
}

/// Copies with a cancel check between chunks; write errors to the local disk become [`DeviceError::DiskFull`]
/// where applicable, a broken pipe [`DeviceError::Disconnected`].
pub fn copy_cancellable(
    input: &mut dyn Read,
    out: &mut dyn Write,
    cancel: &CancelToken,
    on_bytes: &mut dyn FnMut(u64),
) -> Result<u64> {
    let mut buf = vec![0u8; 64 * 1024];
    let mut total = 0u64;
    loop {
        if cancel.is_cancelled() {
            return Err(DeviceError::Cancelled);
        }
        let n = match input.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e.into()),
        };
        out.write_all(&buf[..n]).map_err(pipe_or_local)?;
        total += n as u64;
        on_bytes(total);
    }
    out.flush().map_err(pipe_or_local)?;
    Ok(total)
}

pub(crate) fn pipe_or_local(e: io::Error) -> DeviceError {
    if e.kind() == io::ErrorKind::BrokenPipe {
        DeviceError::Disconnected
    } else {
        local_write_error(e)
    }
}
