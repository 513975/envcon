pub mod caches;
pub mod commands;
pub mod config;
pub mod detect;
pub mod download;
pub mod error;
pub mod install;
pub mod pathman;
pub mod pkgtools;
pub mod sources;
pub mod switcher;
pub mod types;

use commands::AppState;
use std::sync::Mutex;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState {
            cfg: Mutex::new(config::AppConfig::load()),
            tasks: install::TaskManager::new(),
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_overview,
            commands::set_root,
            commands::scan_system,
            commands::switch_env,
            commands::uninstall_env,
            commands::open_path,
            commands::get_path_state,
            commands::save_user_path,
            commands::integrate_current_to_path,
            commands::get_sources,
            commands::list_versions,
            commands::start_install,
            commands::cancel_install,
            commands::install_tasks,
            commands::get_caches,
            commands::clean_cache,
            commands::get_settings,
            commands::set_downloads_dir,
            commands::list_path_backups,
            commands::restore_path_backup,
            commands::get_env_vars,
            commands::save_env_var,
            commands::get_tool_configs,
            commands::apply_tool_config,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
