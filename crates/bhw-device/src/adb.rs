//! The `adb` client binary: locating it, running commands, device shell with exit codes.

use crate::error::{classify_adb_message, DeviceError, Result};
use crate::process::{run_collect, Output, Runner, SystemRunner};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

/// The game's Android package.
pub const PACKAGE: &str = "com.noodlecake.blockheads";

/// Timeouts used by [`Adb`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timeouts {
    /// Plain commands (`devices`, `shell getprop`, ...).
    pub command: Duration,
    /// Waiting for the user to confirm a backup/restore on the phone, or a root grant dialog.
    pub confirm: Duration,
    /// No new bytes for this long during a transfer means the device is gone.
    pub stall: Duration,
}

impl Default for Timeouts {
    fn default() -> Self {
        Timeouts { command: Duration::from_secs(30), confirm: Duration::from_secs(180), stall: Duration::from_secs(60) }
    }
}

/// An `adb` client binary plus the runner used to start it.
#[derive(Clone)]
pub struct Adb {
    program: PathBuf,
    runner: Arc<dyn Runner>,
    /// Timeouts for every operation.
    pub timeouts: Timeouts,
}

impl std::fmt::Debug for Adb {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Adb").field("program", &self.program).field("timeouts", &self.timeouts).finish()
    }
}

/// The adb executable's file name on this OS.
pub fn adb_file_name() -> &'static str {
    if cfg!(windows) {
        "adb.exe"
    } else {
        "adb"
    }
}

/// Finds adb: the bundled sidecar when it exists, then `$ANDROID_HOME`/`$ANDROID_SDK_ROOT` `platform-tools`,
/// then every `PATH` entry. `env` reads environment variables, `exists` checks files (both injectable for tests).
pub fn find_adb(
    sidecar: Option<&Path>,
    env: &dyn Fn(&str) -> Option<OsString>,
    exists: &dyn Fn(&Path) -> bool,
) -> Option<PathBuf> {
    let name = adb_file_name();
    let mut candidates: Vec<PathBuf> = sidecar.map(Path::to_path_buf).into_iter().collect();
    for var in ["ANDROID_HOME", "ANDROID_SDK_ROOT"] {
        if let Some(dir) = env(var).filter(|d| !d.is_empty()) {
            candidates.push(PathBuf::from(dir).join("platform-tools").join(name));
        }
    }
    if let Some(path) = env("PATH") {
        candidates.extend(std::env::split_paths(&path).map(|d| d.join(name)));
    }
    candidates.into_iter().find(|p| exists(p))
}

/// Quotes `s` for the device's `sh` (single quotes).
pub fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// How to run a command as root on the device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RootStyle {
    /// `adb root` is active: the shell already is uid 0.
    AdbRoot,
    /// Magisk / SuperSU / KernelSU: `su -c 'cmd'`.
    SuDashC,
    /// AOSP debug `su`: `su 0 sh -c 'cmd'`.
    SuUid,
}

impl RootStyle {
    /// Wraps a shell command to run as root.
    pub fn wrap(self, cmd: &str) -> String {
        match self {
            RootStyle::AdbRoot => cmd.to_string(),
            RootStyle::SuDashC => format!("su -c {}", sh_quote(cmd)),
            RootStyle::SuUid => format!("su 0 sh -c {}", sh_quote(cmd)),
        }
    }
}

/// A device shell command's result (exit code recovered even on adb without the shell v2 protocol).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellOutput {
    pub code: i32,
    pub stdout: String,
}

const RC_MARK: &str = "__BHW_RC=";

impl Adb {
    /// An adb at `program` started through `runner`.
    pub fn new(program: impl Into<PathBuf>, runner: Arc<dyn Runner>) -> Adb {
        Adb { program: program.into(), runner, timeouts: Timeouts::default() }
    }

    /// Finds adb ([`find_adb`] with the process environment) and uses real processes.
    pub fn locate(sidecar: Option<&Path>) -> Result<Adb> {
        find_adb(sidecar, &|k| std::env::var_os(k), &|p| p.is_file())
            .map(|p| Adb::new(p, Arc::new(SystemRunner)))
            .ok_or(DeviceError::AdbNotFound)
    }

    /// The adb binary.
    pub fn program(&self) -> &Path {
        &self.program
    }

    /// The process runner.
    pub fn runner(&self) -> &Arc<dyn Runner> {
        &self.runner
    }

    /// Maps a spawn error ("file not found" means adb is gone).
    pub(crate) fn spawn_error(e: std::io::Error) -> DeviceError {
        if e.kind() == std::io::ErrorKind::NotFound {
            DeviceError::AdbNotFound
        } else {
            DeviceError::Io(e)
        }
    }

    /// Runs `adb args...` with a timeout.
    pub fn run_timeout(&self, args: &[String], timeout: Duration) -> Result<Output> {
        run_collect(&*self.runner, &self.program, args, timeout).map_err(Adb::spawn_error)?.ok_or(DeviceError::Timeout)
    }

    /// Runs `adb args...` with the command timeout.
    pub fn run(&self, args: &[&str]) -> Result<Output> {
        let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        self.run_timeout(&args, self.timeouts.command)
    }

    /// `["-s", serial, rest...]`.
    pub fn device_args(serial: &str, rest: &[&str]) -> Vec<String> {
        let mut v = vec!["-s".to_string(), serial.to_string()];
        v.extend(rest.iter().map(|s| s.to_string()));
        v
    }

    /// Runs `cmd` in the device shell; returns its exit code and stdout. adb's own errors (unauthorized,
    /// offline, gone) become typed errors.
    pub fn shell_timeout(&self, serial: &str, cmd: &str, timeout: Duration) -> Result<ShellOutput> {
        let full = format!("({cmd}); echo \"{RC_MARK}$?\"");
        let out = self.run_timeout(&Adb::device_args(serial, &["shell", &full]), timeout)?;
        let stdout = out.stdout.replace("\r\n", "\n");
        let Some(pos) = stdout.rfind(RC_MARK) else {
            let msg = format!("{}\n{}", out.stderr, out.stdout);
            return Err(classify_adb_message(&msg, serial)
                .unwrap_or(DeviceError::Command { code: out.code, message: msg.trim().to_string() }));
        };
        let code = stdout[pos + RC_MARK.len()..].trim().parse().unwrap_or(-1);
        Ok(ShellOutput { code, stdout: stdout[..pos].to_string() })
    }

    /// [`Adb::shell_timeout`] with the command timeout.
    pub fn shell(&self, serial: &str, cmd: &str) -> Result<ShellOutput> {
        self.shell_timeout(serial, cmd, self.timeouts.command)
    }

    /// Like [`Adb::shell`] but a non-zero exit code is an error.
    pub fn shell_ok(&self, serial: &str, cmd: &str) -> Result<String> {
        let out = self.shell(serial, cmd)?;
        if out.code == 0 {
            Ok(out.stdout)
        } else {
            Err(DeviceError::Command { code: out.code, message: out.stdout.trim().to_string() })
        }
    }

    /// `adb version`'s first line.
    pub fn version(&self) -> Result<String> {
        let out = self.run(&["version"])?;
        Ok(out.stdout.lines().next().unwrap_or_default().to_string())
    }

    /// `adb start-server`.
    pub fn start_server(&self) -> Result<()> {
        self.run(&["start-server"]).map(drop)
    }

    /// `adb kill-server` (needed after forgetting a paired device).
    pub fn kill_server(&self) -> Result<()> {
        self.run(&["kill-server"]).map(drop)
    }
}
