//! BH World Tool desktop shell. The commands mirror the `Backend` interface of `src/lib/api.ts`. Phase 1 UI work:
//! settings, app info and OS integration are real; everything that needs the world / backup crates (`crates/*`) is a
//! stub that rejects with `{ code: "not_implemented" }` until those crates are wired in.

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::ipc::Channel;
#[cfg(target_os = "macos")]
use tauri::Emitter;
use tauri::{AppHandle, Manager, State};

/// Rejection payload; matches `AppError` in `src/lib/types.ts`.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AppError {
    pub code: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl AppError {
    pub fn new(code: &'static str) -> Self {
        Self { code, detail: None }
    }

    pub fn io(e: impl std::fmt::Display) -> Self {
        Self { code: "io", detail: Some(e.to_string()) }
    }
}

type Res<T> = Result<T, AppError>;

fn not_implemented<T>() -> Res<T> {
    Err(AppError::new("not_implemented"))
}

/// Settings as the UI sees them (`Settings` in types.ts).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Settings {
    pub first_run_done: bool,
    pub worlds_folder: String,
    pub backup_before_write: bool,
    pub backups_to_keep: u32,
    pub root_mode: String,
    pub theme: String,
    pub lang: Option<String>,
    pub reduce_motion: bool,
    pub auto_update: bool,
    pub pc_folder: Option<String>,
    pub skipped_version: Option<String>,
    pub last_update_check: Option<u64>,
}

impl Settings {
    pub fn defaults(worlds_folder: &Path) -> Self {
        Self {
            first_run_done: false,
            worlds_folder: worlds_folder.to_string_lossy().into_owned(),
            backup_before_write: true,
            backups_to_keep: 5,
            root_mode: "auto".into(),
            theme: "system".into(),
            lang: None,
            reduce_motion: false,
            auto_update: true,
            pc_folder: None,
            skipped_version: None,
            last_update_check: None,
        }
    }

    /// Apply a partial update (`Partial<Settings>` from the UI); unknown keys are ignored, bad types rejected.
    pub fn merge(&self, patch: &Map<String, Value>) -> Res<Self> {
        let mut v = serde_json::to_value(self).map_err(AppError::io)?;
        let obj = v.as_object_mut().ok_or_else(|| AppError::io("settings"))?;
        for (k, val) in patch {
            if obj.contains_key(k) {
                obj.insert(k.clone(), val.clone());
            }
        }
        serde_json::from_value(v).map_err(AppError::io)
    }
}

/// Load saved settings, falling back to the defaults for a missing or broken file.
pub fn load_settings(file: &Path, defaults: Settings) -> Settings {
    std::fs::read(file)
        .ok()
        .and_then(|b| serde_json::from_slice::<Map<String, Value>>(&b).ok())
        .and_then(|m| defaults.merge(&m).ok())
        .unwrap_or(defaults)
}

/// `.bhworld` files and folders given on the command line (double-click in the file manager).
pub fn open_args(args: impl IntoIterator<Item = String>) -> Vec<String> {
    args.into_iter()
        .skip(1)
        .filter(|a| !a.starts_with('-') && (a.to_ascii_lowercase().ends_with(".bhworld") || Path::new(a).is_dir()))
        .collect()
}

struct AppState {
    settings: Mutex<Settings>,
    file: PathBuf,
    open_files: Mutex<Vec<String>>,
}

#[tauri::command]
fn app_info(app: AppHandle) -> Value {
    json!({ "version": app.package_info().version.to_string(), "os": std::env::consts::OS })
}

#[tauri::command]
fn get_settings(state: State<'_, AppState>) -> Settings {
    state.settings.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

#[tauri::command]
fn set_settings(state: State<'_, AppState>, patch: Map<String, Value>) -> Res<Settings> {
    let mut s = state.settings.lock().unwrap_or_else(|e| e.into_inner());
    let next = s.merge(&patch)?;
    if let Some(dir) = state.file.parent() {
        std::fs::create_dir_all(dir).map_err(AppError::io)?;
    }
    std::fs::write(&state.file, serde_json::to_vec_pretty(&next).map_err(AppError::io)?).map_err(AppError::io)?;
    *s = next.clone();
    Ok(next)
}

#[tauri::command]
fn take_open_files(state: State<'_, AppState>) -> Vec<String> {
    std::mem::take(&mut *state.open_files.lock().unwrap_or_else(|e| e.into_inner()))
}

#[tauri::command]
fn restart_app(app: AppHandle) {
    app.restart()
}

/// Nothing runs yet, so there is nothing to cancel.
#[tauri::command]
fn cancel(op: String) {
    let _ = op;
}

/// Lists that are simply empty until the crates are wired (the UI then shows its empty states).
#[tauri::command]
fn list_devices() -> Vec<Value> {
    Vec::new()
}
#[tauri::command]
fn list_worlds() -> Vec<Value> {
    Vec::new()
}
#[tauri::command]
fn list_backups() -> Vec<Value> {
    Vec::new()
}
#[tauri::command]
fn list_activity() -> Vec<Value> {
    Vec::new()
}
#[tauri::command]
fn paired_devices() -> Vec<Value> {
    Vec::new()
}
#[tauri::command]
fn ios_backups(folder: Option<String>) -> Vec<Value> {
    let _ = folder;
    Vec::new()
}
#[tauri::command]
fn game_files() -> Value {
    json!({ "state": "not_set", "apk_path": null, "apk_name": null, "version": null, "images": null, "size": null })
}

/// Stubs: same names and arguments as the real commands will have.
macro_rules! stub {
    ($( $name:ident ( $( $arg:ident : $ty:ty ),* ) ),* $(,)?) => {
        $(
            #[tauri::command]
            #[allow(unused_variables)]
            fn $name($( $arg: $ty ),*) -> Res<Value> {
                not_implemented()
            }
        )*
    };
}

type Progress = Channel<Value>;

stub! {
    wifi_discover(),
    wifi_pair(address: String, code: String),
    forget_paired(id: String),
    restart_adb(),
    open_device_manager(),
    ios_open(id: String, password: String),
    diagnostics(),
    read_worlds(op: String, req: Value, on_progress: Progress),
    save_to_library(read_id: String, world_ids: Vec<String>),
    export_worlds(op: String, req: Value, on_progress: Progress),
    free_space(folder: String),
    source_worlds(source: Value),
    transfer_plan(source: Value, world_ids: Vec<String>, target_id: String),
    transfer(op: String, req: Value, on_progress: Progress),
    rename_world(id: String, name: String),
    duplicate_world(id: String),
    delete_world(id: String),
    show_in_folder(path: String),
    inspect_import(paths: Vec<String>),
    import_worlds(req: Value),
    restore_backup(op: String, backup_id: String, device_id: String, on_progress: Progress),
    delete_backup(id: String),
    clear_activity(),
    use_apk(op: String, path: String, on_progress: Progress),
    apk_from_device(op: String, device_id: String, on_progress: Progress),
    reextract(op: String, on_progress: Progress),
    clear_game_files(),
    pick_folder(start: Option<String>),
    pick_import(folders: bool),
    pick_apk(),
    check_update(),
    download_update(op: String, on_progress: Progress),
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let config = app.path().app_config_dir()?;
            let docs = app.path().document_dir().unwrap_or_else(|_| config.clone());
            let file = config.join("settings.json");
            let settings = load_settings(&file, Settings::defaults(&docs.join("BH World Tool")));
            app.manage(AppState {
                settings: Mutex::new(settings),
                file,
                open_files: Mutex::new(open_args(std::env::args())),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_info,
            get_settings,
            set_settings,
            take_open_files,
            restart_app,
            cancel,
            list_devices,
            list_worlds,
            list_backups,
            list_activity,
            paired_devices,
            ios_backups,
            game_files,
            wifi_discover,
            wifi_pair,
            forget_paired,
            restart_adb,
            open_device_manager,
            ios_open,
            diagnostics,
            read_worlds,
            save_to_library,
            export_worlds,
            free_space,
            source_worlds,
            transfer_plan,
            transfer,
            rename_world,
            duplicate_world,
            delete_world,
            show_in_folder,
            inspect_import,
            import_worlds,
            restore_backup,
            delete_backup,
            clear_activity,
            use_apk,
            apk_from_device,
            reextract,
            clear_game_files,
            pick_folder,
            pick_import,
            pick_apk,
            check_update,
            download_update,
        ])
        .build(tauri::generate_context!())
        .expect("error while building BH World Tool")
        .run(|app, event| {
            // macOS hands double-clicked files over as an event, not as arguments.
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Opened { urls } = event {
                let paths: Vec<String> = urls
                    .iter()
                    .filter_map(|u| u.to_file_path().ok())
                    .map(|p| p.to_string_lossy().into_owned())
                    .collect();
                let _ = app.emit("open-files", paths);
            }
            #[cfg(not(target_os = "macos"))]
            let _ = (app, event);
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_applies_known_keys_only() {
        let s = Settings::defaults(Path::new("/w"));
        let mut patch = Map::new();
        patch.insert("theme".into(), json!("dark"));
        patch.insert("nope".into(), json!(1));
        let n = s.merge(&patch).unwrap();
        assert_eq!(n.theme, "dark");
        assert_eq!(n.worlds_folder, "/w");
        patch.insert("backups_to_keep".into(), json!("x"));
        assert_eq!(s.merge(&patch).unwrap_err().code, "io");
    }

    #[test]
    fn load_falls_back_to_defaults() {
        let dir = std::env::temp_dir().join(format!("bhwt-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("s.json");
        let d = Settings::defaults(Path::new("/w"));
        assert_eq!(load_settings(&f, d.clone()), d);
        std::fs::write(&f, br#"{"first_run_done":true}"#).unwrap();
        assert!(load_settings(&f, d.clone()).first_run_done);
        std::fs::write(&f, b"broken").unwrap();
        assert_eq!(load_settings(&f, d.clone()), d);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn open_args_keeps_worlds() {
        let a = open_args(["app", "--flag", "/x/A.BHWORLD", "/x/b.txt"].map(String::from));
        assert_eq!(a, vec!["/x/A.BHWORLD".to_string()]);
    }

    #[test]
    fn commands_match_the_ui() {
        let api = include_str!("../../src/lib/api.ts");
        let build = include_str!("../build.rs");
        let start = api.find("export const COMMANDS").unwrap();
        let list = &api[start..api[start..].find("] as const").unwrap() + start];
        for name in list.split('\'').skip(1).step_by(2) {
            assert!(build.contains(&format!("\"{name}\"")), "{name} missing in build.rs");
        }
        assert_eq!(not_implemented::<()>().unwrap_err(), AppError::new("not_implemented"));
    }
}
