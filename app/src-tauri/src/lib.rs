//! BH World Tool desktop shell: thin glue between the UI (`src/lib/api.ts`) and `bhw-core`, which holds all the
//! application logic. Long commands run on blocking threads and report progress through a `Channel`; `cancel`
//! goes to the core's cancel registry. Pickers use the dialog plugin; updates use the updater plugin (`update.rs`).

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bhw_core::{
    AppError, Core, CoreConfig, ErrorCode, ExportRequest, ImportRequest, Progress, ReadRequest, Settings, Source,
    TransferRequest,
};
use serde_json::{json, Map, Value};
use tauri::ipc::Channel;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_updater::UpdaterExt;

pub mod update;

type Res<T> = Result<T, AppError>;

struct AppState {
    core: Arc<Core>,
    open_files: Mutex<Vec<String>>,
}

/// `.bhworld` files and folders given on the command line (double-click in the file manager).
pub fn open_args(args: impl IntoIterator<Item = String>) -> Vec<String> {
    args.into_iter()
        .skip(1)
        .filter(|a| !a.starts_with('-') && (a.to_ascii_lowercase().ends_with(".bhworld") || Path::new(a).is_dir()))
        .collect()
}

/// Downloads over HTTPS for the core (Google's Platform-Tools, `docs/adb-download.md`).
struct HttpFetch(reqwest::blocking::Client);

impl HttpFetch {
    fn new() -> HttpFetch {
        let ua = concat!("BH-World-Tool/", env!("CARGO_PKG_VERSION"));
        HttpFetch(reqwest::blocking::Client::builder().user_agent(ua).timeout(None).build().unwrap_or_default())
    }
}

fn http_error(e: reqwest::Error) -> std::io::Error {
    std::io::Error::other(e)
}

impl bhw_core::adb_tools::Fetch for HttpFetch {
    fn size(&self, url: &str) -> std::io::Result<Option<u64>> {
        let r = self.0.head(url).send().and_then(|r| r.error_for_status()).map_err(http_error)?;
        Ok(r.content_length().filter(|&n| n > 0))
    }
    fn open(&self, url: &str) -> std::io::Result<(Box<dyn std::io::Read + Send>, Option<u64>)> {
        let r = self.0.get(url).send().and_then(|r| r.error_for_status()).map_err(http_error)?;
        let len = r.content_length();
        Ok((Box::new(r), len))
    }
}

/// The command that shows `path` in the system file manager.
pub fn reveal_command(os: &str, path: &Path) -> (String, Vec<String>) {
    let p = path.display().to_string();
    match os {
        "windows" => ("explorer".into(), vec![format!("/select,{p}")]),
        "macos" => ("open".into(), vec!["-R".into(), p]),
        _ => {
            let dir = if path.is_dir() { path } else { path.parent().unwrap_or(path) };
            ("xdg-open".into(), vec![dir.display().to_string()])
        }
    }
}

fn not_implemented<T>() -> Res<T> {
    Err(AppError::with(ErrorCode::NotImplemented, "not available in this version yet"))
}

/// Runs core work on a blocking thread.
async fn blocking<T: Send + 'static>(
    state: State<'_, AppState>,
    f: impl FnOnce(&Core) -> Res<T> + Send + 'static,
) -> Res<T> {
    let core = state.core.clone();
    tauri::async_runtime::spawn_blocking(move || f(&core)).await.map_err(AppError::io)?
}

/// Progress callback that forwards to the UI's channel.
fn sink(ch: Channel<Progress>) -> impl FnMut(Progress) {
    move |p| {
        let _ = ch.send(p);
    }
}

#[tauri::command]
fn app_info(app: AppHandle) -> Value {
    json!({ "version": app.package_info().version.to_string(), "os": std::env::consts::OS })
}

#[tauri::command]
fn get_settings(state: State<'_, AppState>) -> Settings {
    state.core.settings()
}

#[tauri::command]
fn set_settings(state: State<'_, AppState>, patch: Map<String, Value>) -> Res<Settings> {
    state.core.set_settings(&patch)
}

#[tauri::command]
fn take_open_files(state: State<'_, AppState>) -> Vec<String> {
    std::mem::take(&mut *state.open_files.lock().unwrap_or_else(|e| e.into_inner()))
}

/// Restarts the app, installing a downloaded update first (on Windows the installer exits and restarts the app).
#[tauri::command]
fn restart_app(app: AppHandle, updates: State<'_, Updates>) -> Res<()> {
    if let Some((u, bytes)) = updates.take_downloaded() {
        u.install(bytes).map_err(update_error)?;
    }
    app.restart()
}

#[tauri::command]
fn cancel(state: State<'_, AppState>, op: String) {
    state.core.cancel(&op)
}

#[tauri::command]
async fn list_devices(state: State<'_, AppState>) -> Res<Vec<bhw_core::Device>> {
    blocking(state, |c| Ok(c.list_devices())).await
}

#[tauri::command]
async fn wifi_discover(state: State<'_, AppState>) -> Res<Vec<bhw_core::WifiCandidate>> {
    blocking(state, |c| c.wifi_discover()).await
}

#[tauri::command]
async fn wifi_pair(state: State<'_, AppState>, address: String, code: String) -> Res<bhw_core::Device> {
    blocking(state, move |c| c.wifi_pair(&address, &code)).await
}

#[tauri::command]
async fn paired_devices(state: State<'_, AppState>) -> Res<Vec<bhw_core::PairedDevice>> {
    blocking(state, |c| c.paired_devices()).await
}

#[tauri::command]
async fn forget_paired(state: State<'_, AppState>, id: String) -> Res<()> {
    blocking(state, move |c| c.forget_paired(&id)).await
}

#[tauri::command]
async fn restart_adb(state: State<'_, AppState>) -> Res<()> {
    blocking(state, |c| c.restart_adb()).await
}

#[tauri::command]
fn adb_status(state: State<'_, AppState>) -> bhw_core::adb_tools::AdbStatus {
    state.core.adb_status()
}

#[tauri::command]
async fn adb_download_info(state: State<'_, AppState>) -> Res<bhw_core::adb_tools::AdbDownload> {
    blocking(state, |c| c.adb_download_info()).await
}

#[tauri::command]
async fn download_adb(
    state: State<'_, AppState>,
    op: String,
    on_progress: Channel<Progress>,
) -> Res<bhw_core::adb_tools::AdbStatus> {
    blocking(state, move |c| c.download_adb(&op, &mut sink(on_progress))).await
}

#[tauri::command]
fn open_device_manager() -> Res<()> {
    if !cfg!(windows) {
        return not_implemented();
    }
    std::process::Command::new("mmc").arg("devmgmt.msc").spawn().map(drop).map_err(AppError::from)
}

#[tauri::command]
async fn ios_backups(state: State<'_, AppState>, folder: Option<String>) -> Res<Vec<bhw_core::IosBackup>> {
    blocking(state, move |c| c.ios_backups(folder.as_deref())).await
}

#[tauri::command]
async fn ios_open(state: State<'_, AppState>, id: String, password: String) -> Res<bhw_core::Device> {
    blocking(state, move |c| c.ios_open(&id, &password)).await
}

#[tauri::command]
async fn diagnostics(state: State<'_, AppState>) -> Res<String> {
    blocking(state, |c| Ok(c.diagnostics())).await
}

#[tauri::command]
async fn read_worlds(
    state: State<'_, AppState>,
    op: String,
    req: ReadRequest,
    on_progress: Channel<Progress>,
) -> Res<bhw_core::ReadResult> {
    blocking(state, move |c| c.read(&op, &req, &mut sink(on_progress))).await
}

#[tauri::command]
async fn save_to_library(
    state: State<'_, AppState>,
    read_id: String,
    world_ids: Vec<String>,
) -> Res<Vec<bhw_core::World>> {
    blocking(state, move |c| c.save_to_library(&read_id, &world_ids)).await
}

#[tauri::command]
async fn export_worlds(
    state: State<'_, AppState>,
    op: String,
    req: ExportRequest,
    on_progress: Channel<Progress>,
) -> Res<bhw_core::ExportResult> {
    blocking(state, move |c| c.export_worlds(&op, &req, &mut sink(on_progress))).await
}

#[tauri::command]
async fn free_space(state: State<'_, AppState>, folder: String) -> Res<u64> {
    blocking(state, move |c| c.free_space(&folder)).await
}

#[tauri::command]
async fn source_worlds(state: State<'_, AppState>, source: Source) -> Res<Vec<bhw_core::DeviceWorld>> {
    blocking(state, move |c| c.source_worlds(&source)).await
}

#[tauri::command]
async fn transfer_plan(
    state: State<'_, AppState>,
    source: Source,
    world_ids: Vec<String>,
    target_id: String,
) -> Res<bhw_core::TransferPlan> {
    blocking(state, move |c| c.transfer_plan(&source, &world_ids, &target_id)).await
}

#[tauri::command]
async fn transfer(
    state: State<'_, AppState>,
    op: String,
    req: TransferRequest,
    on_progress: Channel<Progress>,
) -> Res<bhw_core::TransferResult> {
    blocking(state, move |c| c.transfer(&op, &req, &mut sink(on_progress))).await
}

#[tauri::command]
async fn list_worlds(state: State<'_, AppState>) -> Res<Vec<bhw_core::World>> {
    blocking(state, |c| c.lib()?.list()).await
}

#[tauri::command]
async fn rename_world(state: State<'_, AppState>, id: String, name: String) -> Res<bhw_core::World> {
    blocking(state, move |c| c.lib()?.rename(&id, &name)).await
}

#[tauri::command]
async fn duplicate_world(state: State<'_, AppState>, id: String) -> Res<bhw_core::World> {
    blocking(state, move |c| c.lib()?.duplicate(&id)).await
}

#[tauri::command]
async fn delete_world(state: State<'_, AppState>, id: String) -> Res<()> {
    blocking(state, move |c| c.lib()?.delete(&id)).await
}

#[tauri::command]
fn show_in_folder(path: String) -> Res<()> {
    let (prog, args) = reveal_command(std::env::consts::OS, Path::new(&path));
    std::process::Command::new(prog).args(args).spawn().map(drop).map_err(AppError::from)
}

#[tauri::command]
async fn inspect_import(state: State<'_, AppState>, paths: Vec<String>) -> Res<Vec<bhw_core::ImportItem>> {
    blocking(state, move |c| c.inspect_import(&paths)).await
}

#[tauri::command]
async fn import_worlds(state: State<'_, AppState>, req: ImportRequest) -> Res<Vec<bhw_core::World>> {
    blocking(state, move |c| c.import_worlds(&req)).await
}

#[tauri::command]
async fn list_backups(state: State<'_, AppState>) -> Res<Vec<bhw_core::Backup>> {
    blocking(state, |c| c.list_backups()).await
}

#[tauri::command]
async fn restore_backup(
    state: State<'_, AppState>,
    op: String,
    backup_id: String,
    device_id: String,
    on_progress: Channel<Progress>,
) -> Res<()> {
    blocking(state, move |c| c.restore_backup(&op, &backup_id, &device_id, &mut sink(on_progress))).await
}

#[tauri::command]
async fn delete_backup(state: State<'_, AppState>, id: String) -> Res<()> {
    blocking(state, move |c| c.delete_backup(&id)).await
}

#[tauri::command]
fn list_activity(state: State<'_, AppState>) -> Vec<bhw_core::Activity> {
    state.core.list_activity()
}

#[tauri::command]
fn clear_activity(state: State<'_, AppState>) -> Res<()> {
    state.core.clear_activity()
}

#[tauri::command]
async fn game_files(state: State<'_, AppState>) -> Res<bhw_core::GameFiles> {
    blocking(state, |c| Ok(c.game_files())).await
}

#[tauri::command]
async fn use_apk(
    state: State<'_, AppState>,
    op: String,
    path: String,
    on_progress: Channel<Progress>,
) -> Res<bhw_core::GameFiles> {
    blocking(state, move |c| c.use_apk(&op, &path, &mut sink(on_progress))).await
}

#[tauri::command]
async fn apk_from_device(
    state: State<'_, AppState>,
    op: String,
    device_id: String,
    on_progress: Channel<Progress>,
) -> Res<bhw_core::GameFiles> {
    blocking(state, move |c| c.apk_from_device(&op, &device_id, &mut sink(on_progress))).await
}

#[tauri::command]
async fn reextract(state: State<'_, AppState>, op: String, on_progress: Channel<Progress>) -> Res<bhw_core::GameFiles> {
    blocking(state, move |c| c.reextract(&op, &mut sink(on_progress))).await
}

#[tauri::command]
async fn clear_game_files(state: State<'_, AppState>) -> Res<bhw_core::GameFiles> {
    blocking(state, |c| c.clear_game_files()).await
}

fn path_string(p: tauri_plugin_dialog::FilePath) -> Option<String> {
    p.into_path().ok().map(|p| p.display().to_string())
}

#[tauri::command]
async fn pick_folder(app: AppHandle, start: Option<String>) -> Option<String> {
    let mut d = app.dialog().file();
    if let Some(s) = start {
        d = d.set_directory(s);
    }
    tauri::async_runtime::spawn_blocking(move || d.blocking_pick_folder().and_then(path_string)).await.ok().flatten()
}

#[tauri::command]
async fn pick_import(app: AppHandle, folders: bool) -> Vec<String> {
    let d = app.dialog().file();
    let picked = tauri::async_runtime::spawn_blocking(move || {
        if folders {
            d.blocking_pick_folders()
        } else {
            d.add_filter("BH World", &["bhworld"]).blocking_pick_files()
        }
    });
    picked.await.ok().flatten().unwrap_or_default().into_iter().filter_map(path_string).collect()
}

#[tauri::command]
async fn pick_apk(app: AppHandle) -> Option<String> {
    let d = app.dialog().file().add_filter("Android app", &["apk", "apks", "xapk"]);
    tauri::async_runtime::spawn_blocking(move || d.blocking_pick_file().and_then(path_string)).await.ok().flatten()
}

type Updates = update::Slot<tauri_plugin_updater::Update>;

fn update_error(e: tauri_plugin_updater::Error) -> AppError {
    AppError::with(ErrorCode::Offline, e)
}

#[tauri::command]
async fn check_update(
    app: AppHandle,
    state: State<'_, AppState>,
    updates: State<'_, Updates>,
) -> Res<Option<update::UpdateInfo>> {
    let found = app.updater().map_err(update_error)?.check().await.map_err(update_error)?;
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    let mut patch = Map::new();
    patch.insert("last_update_check".into(), json!(now.as_millis() as u64));
    state.core.set_settings(&patch)?;
    let info = found.as_ref().map(|u| {
        let date = u.date.map(|d| (d.unix_timestamp_nanos() / 1_000_000) as i64);
        update::info(&u.version, date, u.body.as_deref(), &u.raw_json, &u.target)
    });
    updates.offer(found);
    Ok(info)
}

#[tauri::command]
async fn download_update(updates: State<'_, Updates>, op: String, on_progress: Channel<Progress>) -> Res<()> {
    let _ = op;
    let u = updates.offered().ok_or_else(|| AppError::new(ErrorCode::NotFound))?;
    let mut counter = update::Counter::default();
    let bytes = u
        .download(
            |len, total| {
                let _ = on_progress.send(counter.chunk(len, total));
            },
            || {},
        )
        .await
        .map_err(update_error)?;
    updates.set_downloaded(u, bytes);
    Ok(())
}

// ---- world editor (Phase 2) ----

#[tauri::command]
async fn editor_open(state: State<'_, AppState>, id: String) -> Res<bhw_core::editor::EditorWorld> {
    blocking(state, move |c| c.editor_open(&id)).await
}

#[tauri::command]
fn editor_close(state: State<'_, AppState>, session: String) {
    state.core.editor_close(&session)
}

#[tauri::command]
async fn editor_region(state: State<'_, AppState>, session: String, rect: [u32; 4]) -> Res<tauri::ipc::Response> {
    let [x0, y0, x1, y1] = rect;
    blocking(state, move |c| c.editor_region(&session, x0, y0, x1, y1)).await.map(tauri::ipc::Response::new)
}

#[tauri::command]
async fn editor_overview(state: State<'_, AppState>, session: String, scale: u32) -> Res<tauri::ipc::Response> {
    blocking(state, move |c| c.editor_overview(&session, scale)).await.map(tauri::ipc::Response::new)
}

#[tauri::command]
async fn editor_tile(state: State<'_, AppState>, session: String, x: u32, y: u32) -> Res<bhw_core::editor::EditorTile> {
    blocking(state, move |c| c.editor_tile(&session, x, y)).await
}

#[tauri::command]
fn editor_catalog(state: State<'_, AppState>) -> bhw_editor::atlas::Catalog {
    state.core.editor_catalog()
}

/// The atlas image, or an empty body when the game files are not set up.
#[tauri::command]
async fn editor_atlas(state: State<'_, AppState>, which: String) -> Res<tauri::ipc::Response> {
    blocking(state, move |c| c.editor_atlas(&which)).await.map(|b| tauri::ipc::Response::new(b.unwrap_or_default()))
}

#[tauri::command]
fn editor_state(state: State<'_, AppState>, session: String) -> Res<bhw_core::editor::EditorState> {
    state.core.editor_state(&session)
}

#[tauri::command]
fn editor_blockheads(state: State<'_, AppState>, session: String) -> Res<Vec<bhw_core::editor::EditorBlockhead>> {
    state.core.editor_blockheads(&session)
}

#[tauri::command]
async fn editor_edit(
    state: State<'_, AppState>,
    session: String,
    req: bhw_core::editor::EditRequest,
) -> Res<bhw_core::editor::EditResult> {
    blocking(state, move |c| c.editor_edit(&session, req)).await
}

#[tauri::command]
async fn editor_find(
    state: State<'_, AppState>,
    session: String,
    target: bhw_editor::Match,
    area: Option<bhw_editor::Rect>,
    limit: usize,
) -> Res<(Vec<[u32; 2]>, u64)> {
    blocking(state, move |c| c.editor_find(&session, target, area, limit)).await
}

#[tauri::command]
async fn editor_history(
    state: State<'_, AppState>,
    session: String,
    to: usize,
) -> Res<bhw_core::editor::HistoryResult> {
    blocking(state, move |c| c.editor_history(&session, to)).await
}

#[tauri::command]
fn editor_changes(state: State<'_, AppState>, session: String) -> Res<Vec<[u32; 2]>> {
    state.core.editor_changes(&session)
}

#[tauri::command]
async fn editor_save(state: State<'_, AppState>, session: String, as_new: bool) -> Res<bhw_core::World> {
    blocking(state, move |c| c.editor_save(&session, as_new)).await
}

#[tauri::command]
async fn editor_search(
    state: State<'_, AppState>,
    session: String,
    kind: bhw_core::editor::SearchKind,
    text: String,
) -> Res<Vec<bhw_core::editor::EditorObject>> {
    blocking(state, move |c| c.editor_search(&session, kind, &text)).await
}

#[tauri::command]
async fn editor_schematics(state: State<'_, AppState>) -> Res<Vec<bhw_core::editor::SchematicInfo>> {
    blocking(state, |c| c.editor_schematics()).await
}

#[tauri::command]
async fn editor_save_schematic(
    state: State<'_, AppState>,
    session: String,
    rect: Option<bhw_editor::Rect>,
    name: String,
) -> Res<bhw_core::editor::SchematicInfo> {
    blocking(state, move |c| c.editor_save_schematic(&session, rect, &name)).await
}

#[tauri::command]
async fn editor_use_schematic(
    state: State<'_, AppState>,
    session: String,
    file: String,
) -> Res<bhw_core::editor::EditorState> {
    blocking(state, move |c| c.editor_use_schematic(&session, &file)).await
}

#[tauri::command]
async fn editor_import_schematic(state: State<'_, AppState>, path: String) -> Res<bhw_core::editor::SchematicInfo> {
    blocking(state, move |c| c.editor_import_schematic(&path)).await
}

#[tauri::command]
async fn editor_export_schematic(state: State<'_, AppState>, file: String, dest: String) -> Res<()> {
    blocking(state, move |c| c.editor_export_schematic(&file, &dest)).await
}

#[tauri::command]
async fn editor_delete_schematic(state: State<'_, AppState>, file: String) -> Res<()> {
    blocking(state, move |c| c.editor_delete_schematic(&file)).await
}

/// Picks a `.bhwschem` file to import, or (`save`) where to export one (named `name`).
#[tauri::command]
async fn pick_schematic(app: AppHandle, save: bool, name: Option<String>) -> Option<String> {
    let d = app.dialog().file().add_filter("Schematic", &["bhwschem"]);
    tauri::async_runtime::spawn_blocking(move || {
        if save {
            d.set_file_name(name.unwrap_or_default()).blocking_save_file().and_then(path_string)
        } else {
            d.blocking_pick_file().and_then(path_string)
        }
    })
    .await
    .ok()
    .flatten()
}

/// Sends `devices-changed` whenever the device list changes (polled every few seconds).
fn watch_devices(app: AppHandle, core: Arc<Core>) {
    std::thread::spawn(move || {
        let mut last = Value::Null;
        loop {
            let now = serde_json::to_value(core.list_devices()).unwrap_or_default();
            if now != last {
                let _ = app.emit("devices-changed", &now);
                last = now;
            }
            std::thread::sleep(Duration::from_secs(3));
        }
    });
}

/// Everything the core needs, from Tauri's folders and the process environment.
fn core_config(app: &tauri::App) -> tauri::Result<CoreConfig> {
    let config = app.path().app_config_dir()?;
    let cache = app.path().app_cache_dir()?;
    let docs = app.path().document_dir().unwrap_or_else(|_| config.clone());
    let data = app.path().app_data_dir()?;
    Ok(CoreConfig {
        config_dir: config,
        cache_dir: cache,
        default_worlds_folder: docs.join("BH World Tool"),
        adb: bhw_core::adb_tools::locate(&data),
        data_dir: data,
        make_adb: Arc::new(|p| bhw_device::Adb::new(p, Arc::new(bhw_device::process::SystemRunner))),
        fetch: Arc::new(HttpFetch::new()),
        runner: Arc::new(bhw_device::process::SystemRunner),
        pc_locator: Arc::new(|| bhw_device::PcInstall::locate().map(|p| p.data_dir)),
        ios_locator: Arc::new(bhw_ios::default_backup_dirs),
        android_dir: bhw_device::wifi::android_user_dir(&|k| std::env::var_os(k), app.path().home_dir().ok()),
        app_version: app.package_info().version.to_string(),
    })
}

/// Payload of `AppError` as the UI sees it (for tests).
pub fn error_json(e: &AppError) -> Value {
    serde_json::to_value(e).unwrap_or_default()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(Updates::default())
        .setup(|app| {
            let core = Arc::new(Core::new(core_config(app)?));
            watch_devices(app.handle().clone(), core.clone());
            app.manage(AppState { core, open_files: Mutex::new(open_args(std::env::args())) });
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
            adb_status,
            adb_download_info,
            download_adb,
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
            editor_open,
            editor_close,
            editor_region,
            editor_overview,
            editor_tile,
            editor_catalog,
            editor_atlas,
            editor_state,
            editor_blockheads,
            editor_edit,
            editor_find,
            editor_history,
            editor_changes,
            editor_save,
            editor_search,
            editor_schematics,
            editor_save_schematic,
            editor_use_schematic,
            editor_import_schematic,
            editor_export_schematic,
            editor_delete_schematic,
            pick_schematic,
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
    fn open_args_keeps_worlds() {
        let a = open_args(["app", "--flag", "/x/A.BHWORLD", "/x/b.txt"].map(String::from));
        assert_eq!(a, vec!["/x/A.BHWORLD".to_string()]);
    }

    #[test]
    fn reveal_per_os() {
        let f = Path::new("/w/a.bhworld");
        assert_eq!(reveal_command("windows", f), ("explorer".into(), vec!["/select,/w/a.bhworld".into()]));
        assert_eq!(reveal_command("macos", f), ("open".into(), vec!["-R".into(), "/w/a.bhworld".into()]));
        assert_eq!(reveal_command("linux", f), ("xdg-open".into(), vec!["/w".into()]));
        let d = std::env::temp_dir();
        assert_eq!(reveal_command("linux", &d).1, vec![d.display().to_string()]);
    }

    #[test]
    fn errors_serialize_like_the_ui_expects() {
        let e = error_json(&not_implemented::<()>().unwrap_err());
        assert_eq!(e["code"], "not_implemented");
        let e = error_json(&AppError::new(ErrorCode::GameStopped).device("Pixel"));
        assert_eq!(e, json!({ "code": "game_stopped", "device": "Pixel" }));
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
    }
}
