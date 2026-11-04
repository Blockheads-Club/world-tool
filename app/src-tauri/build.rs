//! The app commands go through the access control list like plugin commands: only windows whose capability grants
//! them (`permissions/app-commands.toml`) can call them.

/// Every command of `generate_handler!` in `src/lib.rs` and of `COMMANDS` in `src/lib/api.ts` (kept in step by tests).
const COMMANDS: &[&str] = &[
    "app_info",
    "get_settings",
    "set_settings",
    "list_devices",
    "wifi_discover",
    "wifi_pair",
    "paired_devices",
    "forget_paired",
    "restart_adb",
    "adb_status",
    "adb_download_info",
    "download_adb",
    "open_device_manager",
    "ios_backups",
    "ios_open",
    "diagnostics",
    "read_worlds",
    "save_to_library",
    "export_worlds",
    "free_space",
    "cancel",
    "source_worlds",
    "transfer_plan",
    "transfer",
    "list_worlds",
    "rename_world",
    "duplicate_world",
    "delete_world",
    "show_in_folder",
    "inspect_import",
    "import_worlds",
    "list_backups",
    "restore_backup",
    "delete_backup",
    "list_activity",
    "clear_activity",
    "game_files",
    "use_apk",
    "apk_from_device",
    "reextract",
    "clear_game_files",
    "pick_folder",
    "pick_import",
    "pick_apk",
    "check_update",
    "download_update",
    "restart_app",
    "take_open_files",
];

fn main() {
    let manifest = tauri_build::AppManifest::new().commands(COMMANDS);
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(manifest)).expect("failed to run tauri-build");
}
