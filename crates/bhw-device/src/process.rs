//! Spawning processes behind a trait, so tests can script `adb`'s answers.

use std::io::{self, Read, Write};
use std::path::Path;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// How often loops waiting for a process look again.
pub const TICK: Duration = Duration::from_millis(20);

/// A started process. All methods take `&self` so one child can be shared with a watchdog thread.
pub trait Child: Send + Sync {
    /// The process's stdin (only when spawned with `stdin: true`); `None` after the first call.
    fn take_stdin(&self) -> Option<Box<dyn Write + Send>>;
    /// The process's stdout; `None` after the first call.
    fn take_stdout(&self) -> Option<Box<dyn Read + Send>>;
    /// The process's stderr; `None` after the first call.
    fn take_stderr(&self) -> Option<Box<dyn Read + Send>>;
    /// The exit code once it has exited (`-1` when killed by a signal).
    fn try_wait(&self) -> io::Result<Option<i32>>;
    /// Kills the process (no error when it has already exited).
    fn kill(&self);
}

/// Starts processes.
pub trait Runner: Send + Sync {
    /// Starts `program args...` with piped stdout/stderr and, when `stdin` is true, a piped stdin (else null).
    fn spawn(&self, program: &Path, args: &[String], stdin: bool) -> io::Result<Arc<dyn Child>>;
}

/// The real thing: `std::process::Command`.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemRunner;

struct SystemChild {
    child: Mutex<std::process::Child>,
}

fn take<T>(slot: &mut Option<T>) -> Option<T> {
    slot.take()
}

impl Child for SystemChild {
    fn take_stdin(&self) -> Option<Box<dyn Write + Send>> {
        take(&mut self.child.lock().unwrap().stdin).map(|s| Box::new(s) as Box<dyn Write + Send>)
    }
    fn take_stdout(&self) -> Option<Box<dyn Read + Send>> {
        take(&mut self.child.lock().unwrap().stdout).map(|s| Box::new(s) as Box<dyn Read + Send>)
    }
    fn take_stderr(&self) -> Option<Box<dyn Read + Send>> {
        take(&mut self.child.lock().unwrap().stderr).map(|s| Box::new(s) as Box<dyn Read + Send>)
    }
    fn try_wait(&self) -> io::Result<Option<i32>> {
        Ok(self.child.lock().unwrap().try_wait()?.map(|s| s.code().unwrap_or(-1)))
    }
    fn kill(&self) {
        let _ = self.child.lock().unwrap().kill();
    }
}

impl Runner for SystemRunner {
    fn spawn(&self, program: &Path, args: &[String], stdin: bool) -> io::Result<Arc<dyn Child>> {
        let mut cmd = std::process::Command::new(program);
        cmd.args(args)
            .stdin(if stdin { Stdio::piped() } else { Stdio::null() })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            // CREATE_NO_WINDOW: no console window flashing up from a GUI app.
            cmd.creation_flags(0x0800_0000);
        }
        Ok(Arc::new(SystemChild { child: Mutex::new(cmd.spawn()?) }))
    }
}

/// What a finished command printed.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Output {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

/// Stands in for a stream that was already taken.
pub(crate) fn empty_reader() -> Box<dyn Read + Send> {
    Box::new(io::empty())
}

/// Stands in for a stdin that was already taken.
pub(crate) fn null_writer() -> Box<dyn Write + Send> {
    Box::new(io::sink())
}

/// Reads a stream to the end on a thread.
pub(crate) fn collect(r: Option<Box<dyn Read + Send>>) -> std::thread::JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(mut r) = r {
            let _ = r.read_to_end(&mut buf);
        }
        buf
    })
}

/// Polls until the child exits or `deadline` passes (then kills it and returns `None`).
pub(crate) fn wait_until(child: &dyn Child, deadline: Instant) -> io::Result<Option<i32>> {
    loop {
        if let Some(code) = child.try_wait()? {
            return Ok(Some(code));
        }
        if Instant::now() >= deadline {
            child.kill();
            return Ok(None);
        }
        std::thread::sleep(TICK);
    }
}

/// Runs a command to completion, collecting its output; `None` when it timed out (and was killed).
pub fn run_collect(
    runner: &dyn Runner,
    program: &Path,
    args: &[String],
    timeout: Duration,
) -> io::Result<Option<Output>> {
    let child = runner.spawn(program, args, false)?;
    let out = collect(child.take_stdout());
    let err = collect(child.take_stderr());
    let code = wait_until(&*child, Instant::now() + timeout)?;
    let stdout = String::from_utf8_lossy(&out.join().unwrap_or_default()).into_owned();
    let stderr = String::from_utf8_lossy(&err.join().unwrap_or_default()).into_owned();
    Ok(code.map(|code| Output { code, stdout, stderr }))
}
