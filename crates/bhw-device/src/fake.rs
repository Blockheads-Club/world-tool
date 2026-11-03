//! A scripted [`Runner`] for tests (also usable by the app's tests): each spawn is matched against rules by
//! the space-joined argument line and answered with canned output.

use crate::process::{Child, Runner};
use std::io::{self, Read, Write};
use std::path::Path;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

/// Side effect run when a matching command starts (e.g. writing the file `adb backup -f` names).
pub type Hook = Arc<dyn Fn(&[String]) + Send + Sync>;

/// The canned behaviour of one process.
#[derive(Clone, Default)]
pub struct Reply {
    /// Exit code.
    pub code: i32,
    /// Stdout, delivered chunk by chunk.
    pub stdout: Vec<Vec<u8>>,
    /// Pause before each stdout chunk.
    pub chunk_delay: Duration,
    /// Stderr.
    pub stderr: Vec<u8>,
    /// After the stdout chunks, block (stdout and exit) until killed.
    pub hang: bool,
    /// Stdin accepts this many bytes, then blocks until killed (then fails).
    pub stdin_limit: Option<usize>,
    /// Run at spawn time.
    pub on_spawn: Option<Hook>,
    /// Spawn fails with this error kind.
    pub spawn_error: Option<io::ErrorKind>,
    /// Stdout fails once with this error kind after the chunks.
    pub read_error: Option<io::ErrorKind>,
}

impl Reply {
    /// Exit 0 with `stdout`.
    pub fn ok(stdout: impl AsRef<[u8]>) -> Reply {
        let out = stdout.as_ref();
        Reply { stdout: if out.is_empty() { Vec::new() } else { vec![out.to_vec()] }, ..Reply::default() }
    }
    /// Exit `code` with `stderr`.
    pub fn fail(code: i32, stderr: impl AsRef<[u8]>) -> Reply {
        Reply { code, stderr: stderr.as_ref().to_vec(), ..Reply::default() }
    }
    /// Never finishes on its own.
    pub fn hang() -> Reply {
        Reply { hang: true, ..Reply::default() }
    }
    /// Stdout in several chunks.
    pub fn chunks(chunks: Vec<Vec<u8>>, delay: Duration) -> Reply {
        Reply { stdout: chunks, chunk_delay: delay, ..Reply::default() }
    }
    /// Sets the exit code.
    pub fn code(mut self, code: i32) -> Reply {
        self.code = code;
        self
    }
    /// Sets stderr.
    pub fn stderr(mut self, s: impl AsRef<[u8]>) -> Reply {
        self.stderr = s.as_ref().to_vec();
        self
    }
    /// Blocks after the output until killed.
    pub fn then_hang(mut self) -> Reply {
        self.hang = true;
        self
    }
    /// Sets a spawn hook.
    pub fn on_spawn(mut self, f: impl Fn(&[String]) + Send + Sync + 'static) -> Reply {
        self.on_spawn = Some(Arc::new(f));
        self
    }
}

struct Rule {
    pattern: String,
    reply: Reply,
    once: bool,
}

/// One recorded spawn.
#[derive(Debug, Clone, Default)]
pub struct Call {
    pub program: String,
    pub args: Vec<String>,
    /// Bytes the caller wrote to stdin.
    pub stdin: Arc<Mutex<Vec<u8>>>,
}

impl Call {
    /// The space-joined arguments.
    pub fn line(&self) -> String {
        self.args.join(" ")
    }
    /// What was written to stdin so far.
    pub fn stdin_bytes(&self) -> Vec<u8> {
        self.stdin.lock().unwrap().clone()
    }
}

/// The scripted runner.
#[derive(Clone, Default)]
pub struct ScriptedRunner {
    rules: Arc<Mutex<Vec<Rule>>>,
    calls: Arc<Mutex<Vec<Call>>>,
}

impl ScriptedRunner {
    /// No rules yet.
    pub fn new() -> ScriptedRunner {
        ScriptedRunner::default()
    }
    /// Answers every command whose argument line contains `pattern` (earlier rules win).
    pub fn on(&self, pattern: &str, reply: Reply) -> &Self {
        self.rules.lock().unwrap().push(Rule { pattern: pattern.into(), reply, once: false });
        self
    }
    /// Like [`ScriptedRunner::on`] but used up by the first match.
    pub fn once(&self, pattern: &str, reply: Reply) -> &Self {
        self.rules.lock().unwrap().push(Rule { pattern: pattern.into(), reply, once: true });
        self
    }
    /// Every spawn so far.
    pub fn calls(&self) -> Vec<Call> {
        self.calls.lock().unwrap().clone()
    }
    /// Argument lines of every spawn so far.
    pub fn lines(&self) -> Vec<String> {
        self.calls().iter().map(Call::line).collect()
    }
}

struct State {
    killed: bool,
    stdout_done: bool,
}

struct Shared {
    state: Mutex<State>,
    cv: Condvar,
}

impl Shared {
    fn wait_killed(&self) {
        let mut s = self.state.lock().unwrap();
        while !s.killed {
            s = self.cv.wait(s).unwrap();
        }
    }
    fn killed(&self) -> bool {
        self.state.lock().unwrap().killed
    }
}

struct FakeChild {
    reply: Reply,
    shared: Arc<Shared>,
    stdin_log: Arc<Mutex<Vec<u8>>>,
    taken: Mutex<[bool; 3]>,
}

struct FakeStdout {
    chunks: std::collections::VecDeque<Vec<u8>>,
    delay: Duration,
    hang: bool,
    read_error: Option<io::ErrorKind>,
    shared: Arc<Shared>,
}

impl Read for FakeStdout {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.shared.killed() {
            return Ok(0);
        }
        if let Some(mut c) = self.chunks.pop_front() {
            std::thread::sleep(self.delay);
            let n = c.len().min(buf.len());
            buf[..n].copy_from_slice(&c[..n]);
            if n < c.len() {
                self.chunks.push_front(c.split_off(n));
            }
            return Ok(n);
        }
        if let Some(kind) = self.read_error.take() {
            return Err(kind.into());
        }
        if self.hang {
            self.shared.wait_killed();
        }
        self.shared.state.lock().unwrap().stdout_done = true;
        Ok(0)
    }
}

struct FakeStdin {
    log: Arc<Mutex<Vec<u8>>>,
    limit: Option<usize>,
    shared: Arc<Shared>,
}

impl Write for FakeStdin {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut log = self.log.lock().unwrap();
        let room = self.limit.map_or(buf.len(), |l| l.saturating_sub(log.len()).min(buf.len()));
        if room == 0 || self.shared.killed() {
            drop(log);
            self.shared.wait_killed();
            return Err(io::Error::new(io::ErrorKind::BrokenPipe, "broken pipe"));
        }
        log.extend_from_slice(&buf[..room]);
        Ok(room)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl FakeChild {
    fn first_take(&self, i: usize) -> bool {
        !std::mem::replace(&mut self.taken.lock().unwrap()[i], true)
    }
}

impl Child for FakeChild {
    fn take_stdin(&self) -> Option<Box<dyn Write + Send>> {
        self.first_take(0).then(|| {
            Box::new(FakeStdin {
                log: self.stdin_log.clone(),
                limit: self.reply.stdin_limit,
                shared: self.shared.clone(),
            }) as Box<dyn Write + Send>
        })
    }
    fn take_stdout(&self) -> Option<Box<dyn Read + Send>> {
        self.first_take(1).then(|| {
            Box::new(FakeStdout {
                chunks: self.reply.stdout.iter().cloned().collect(),
                delay: self.reply.chunk_delay,
                hang: self.reply.hang,
                read_error: self.reply.read_error,
                shared: self.shared.clone(),
            }) as Box<dyn Read + Send>
        })
    }
    fn take_stderr(&self) -> Option<Box<dyn Read + Send>> {
        self.first_take(2).then(|| Box::new(io::Cursor::new(self.reply.stderr.clone())) as Box<dyn Read + Send>)
    }
    fn try_wait(&self) -> io::Result<Option<i32>> {
        let s = self.shared.state.lock().unwrap();
        Ok(if s.killed {
            Some(-1)
        } else if self.reply.hang || !s.stdout_done {
            None
        } else {
            Some(self.reply.code)
        })
    }
    fn kill(&self) {
        self.shared.state.lock().unwrap().killed = true;
        self.shared.cv.notify_all();
    }
}

impl Runner for ScriptedRunner {
    fn spawn(&self, program: &Path, args: &[String], _stdin: bool) -> io::Result<Arc<dyn Child>> {
        let line = args.join(" ");
        let reply = {
            let mut rules = self.rules.lock().unwrap();
            let i = rules
                .iter()
                .position(|r| line.contains(&r.pattern))
                .ok_or_else(|| io::Error::other(format!("unscripted command: {line}")))?;
            if rules[i].once {
                rules.remove(i).reply
            } else {
                rules[i].reply.clone()
            }
        };
        let call = Call { program: program.display().to_string(), args: args.to_vec(), stdin: Arc::default() };
        self.calls.lock().unwrap().push(call.clone());
        if let Some(kind) = reply.spawn_error {
            return Err(io::Error::new(kind, "spawn failed"));
        }
        if let Some(hook) = &reply.on_spawn {
            hook(args);
        }
        // A process whose stdout is never read still exits: stdout counts as done when there is none.
        let stdout_done = reply.stdout.is_empty();
        Ok(Arc::new(FakeChild {
            shared: Arc::new(Shared { state: Mutex::new(State { killed: false, stdout_done }), cv: Condvar::new() }),
            stdin_log: call.stdin,
            reply,
            taken: Mutex::new([false; 3]),
        }))
    }
}
