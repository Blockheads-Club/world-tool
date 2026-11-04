//! In-place updates through `tauri-plugin-updater`. The endpoint is the `latest.json` of the newest GitHub release
//! (written by `.github/workflows/release.yml`, see `scripts/updater-manifest.ts`). `check_update` keeps the offered
//! update, `download_update` downloads and verifies it, and `restart_app` installs it before restarting (on Windows
//! the installer takes over and restarts the app itself).
//!
//! Everything that does not need the network lives in plain functions here so it is unit tested; the commands in
//! `lib.rs` only call the plugin and these.

use std::sync::Mutex;

use bhw_core::{Progress, Stage};
use serde::Serialize;
use serde_json::Value;

/// What the update popup shows (`UpdateInfo` in `src/lib/types.ts`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UpdateInfo {
    pub version: String,
    /// Publish date, milliseconds since the Unix epoch (0 when the manifest has none).
    pub date: i64,
    /// Download size in bytes (0 when the manifest does not say).
    pub size: u64,
    pub notes: Vec<String>,
}

/// Most notes shown in the popup.
const MAX_NOTES: usize = 8;

/// Builds the popup data from what the updater read out of `latest.json`. `target` is the manifest platform the
/// updater picked (`Update::target`, e.g. `linux-x86_64`); its entry may carry a `size` (our manifest adds one).
pub fn info(version: &str, date_ms: Option<i64>, body: Option<&str>, raw: &Value, target: &str) -> UpdateInfo {
    UpdateInfo {
        version: version.to_string(),
        date: date_ms.unwrap_or(0),
        size: raw["platforms"][target]["size"].as_u64().unwrap_or(0),
        notes: notes(body.unwrap_or("")),
    }
}

/// The bullet points of release notes as semantic-release writes them (`* **scope:** text ([abc1234](url))`),
/// without Markdown, links and commit hashes. Plain text without bullets gives its non-empty lines.
pub fn notes(body: &str) -> Vec<String> {
    let lines: Vec<&str> = body.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    let bullets: Vec<&str> =
        lines.iter().filter_map(|l| l.strip_prefix("* ").or_else(|| l.strip_prefix("- "))).collect();
    let picked = if bullets.is_empty() { lines.into_iter().filter(|l| !l.starts_with('#')).collect() } else { bullets };
    picked.into_iter().map(clean).filter(|l| !l.is_empty()).take(MAX_NOTES).collect()
}

/// `**scope:** text ([abc1234](https://…)), closes [#1](…)` → `scope: text`.
fn clean(line: &str) -> String {
    let mut s = line.to_string();
    // Trailing commit link and anything after it.
    if let Some(i) = s.find(" ([") {
        s.truncate(i);
    }
    // Remaining links: keep the text.
    while let (Some(a), Some(b)) = (s.find('['), s.find("](")) {
        let Some(c) = s[b..].find(')').map(|c| b + c) else { break };
        if a > b {
            break;
        }
        s = format!("{}{}{}", &s[..a], &s[a + 1..b], &s[c + 1..]);
    }
    s.replace("**", "").replace('`', "").trim().to_string()
}

/// Turns the updater's download callbacks (chunk length, total) into the UI's progress events.
#[derive(Debug, Default)]
pub struct Counter {
    done: u64,
}

impl Counter {
    pub fn chunk(&mut self, len: usize, total: Option<u64>) -> Progress {
        self.done += len as u64;
        Progress::bytes(Stage::Downloading, self.done, total)
    }
}

/// How often a running download looks at its cancel flag.
const CANCEL_POLL: std::time::Duration = std::time::Duration::from_millis(200);

/// Runs `fut` until it finishes (`Some`) or `cancelled()` turns true (`None`; `fut` is dropped, which stops the
/// download).
pub async fn unless_cancelled<T>(fut: impl std::future::Future<Output = T>, cancelled: impl Fn() -> bool) -> Option<T> {
    let watch = async {
        while !cancelled() {
            tokio::time::sleep(CANCEL_POLL).await;
        }
    };
    tokio::select! {
        r = fut => Some(r),
        () = watch => None,
    }
}

/// The update between the commands: offered by `check_update`, downloaded by `download_update`, installed by
/// `restart_app`. Generic so the bookkeeping is testable without a real `tauri_plugin_updater::Update`.
pub struct Slot<U> {
    offered: Mutex<Option<U>>,
    downloaded: Mutex<Option<(U, Vec<u8>)>>,
}

impl<U> Default for Slot<U> {
    fn default() -> Self {
        Slot { offered: Mutex::new(None), downloaded: Mutex::new(None) }
    }
}

impl<U: Clone> Slot<U> {
    /// Keeps the update the last check offered (`None`: up to date, forget an older offer).
    pub fn offer(&self, u: Option<U>) {
        *self.offered.lock().unwrap_or_else(|e| e.into_inner()) = u;
    }

    /// The offered update, for downloading.
    pub fn offered(&self) -> Option<U> {
        self.offered.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// The verified package, ready to install on restart.
    pub fn set_downloaded(&self, u: U, bytes: Vec<u8>) {
        *self.downloaded.lock().unwrap_or_else(|e| e.into_inner()) = Some((u, bytes));
    }

    /// Takes the downloaded package (once).
    pub fn take_downloaded(&self) -> Option<(U, Vec<u8>)> {
        self.downloaded.lock().unwrap_or_else(|e| e.into_inner()).take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn info_from_manifest() {
        let raw = json!({ "platforms": { "linux-x86_64": { "url": "u", "signature": "s", "size": 1234 } } });
        let i = info("0.2.0", Some(1_700_000_000_000), Some("* a\n* b"), &raw, "linux-x86_64");
        assert_eq!(
            i,
            UpdateInfo {
                version: "0.2.0".into(),
                date: 1_700_000_000_000,
                size: 1234,
                notes: vec!["a".into(), "b".into()]
            }
        );
        let i = info("0.2.0", None, None, &raw, "windows-x86_64");
        assert_eq!((i.date, i.size, i.notes.len()), (0, 0, 0));
        assert_eq!(serde_json::to_value(&i).unwrap()["notes"], json!([]));
    }

    #[test]
    fn notes_from_semantic_release() {
        let body = "## [0.2.0](https://github.com/o/r/compare/v0.1.0...v0.2.0) (2026-11-04)\n\n\n### Features\n\n\
            * **app:** copy worlds over Wi-Fi ([abc1234](https://github.com/o/r/commit/abc1234))\n\
            * `.bhworld` files open with a [double click](https://x.test/doc) ([def5678](https://x))\n\n\
            ### Bug Fixes\n\n- fix crash, see [here](https://x.test\n";
        assert_eq!(
            notes(body),
            vec![
                "app: copy worlds over Wi-Fi",
                ".bhworld files open with a double click",
                "fix crash, see [here](https://x.test"
            ]
        );
    }

    #[test]
    fn notes_plain_and_limited() {
        assert_eq!(notes("# Title\n\nFirst line\n  second  \n"), vec!["First line", "second"]);
        assert_eq!(notes("*  ([abc](x))\n* real"), vec!["real"]);
        assert_eq!(notes("* a ] then [b](c)"), vec!["a ] then b"]);
        assert_eq!(notes("* x](y) [z"), vec!["x](y) [z"]);
        let many: String = (0..20).map(|i| format!("* n{i}\n")).collect();
        assert_eq!(notes(&many).len(), MAX_NOTES);
        assert!(notes("").is_empty());
    }

    #[test]
    fn counter_adds_chunks() {
        let mut c = Counter::default();
        assert_eq!(c.chunk(10, Some(30)), Progress::bytes(Stage::Downloading, 10, Some(30)));
        assert_eq!(c.chunk(5, None), Progress::bytes(Stage::Downloading, 15, None));
    }

    #[test]
    fn downloads_stop_when_cancelled() {
        let rt = tokio::runtime::Builder::new_current_thread().enable_time().build().unwrap();
        assert_eq!(rt.block_on(unless_cancelled(async { 7 }, || false)), Some(7));
        let never = std::future::pending::<u8>();
        assert_eq!(rt.block_on(unless_cancelled(never, || true)), None);
        let flag = std::sync::atomic::AtomicBool::new(false);
        let later = async {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            flag.store(true, std::sync::atomic::Ordering::SeqCst);
            std::future::pending::<u8>().await
        };
        assert_eq!(rt.block_on(unless_cancelled(later, || flag.load(std::sync::atomic::Ordering::SeqCst))), None);
    }

    #[test]
    fn slot_keeps_offer_and_download() {
        let s = Slot::<u8>::default();
        assert_eq!(s.offered(), None);
        s.offer(Some(1));
        assert_eq!(s.offered(), Some(1));
        s.offer(None);
        assert_eq!(s.offered(), None);
        assert_eq!(s.take_downloaded(), None);
        s.set_downloaded(2, vec![9]);
        assert_eq!(s.take_downloaded(), Some((2, vec![9])));
        assert_eq!(s.take_downloaded(), None);
    }
}
