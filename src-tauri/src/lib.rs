pub mod caches;
mod fsutil;
pub mod commands;
pub mod config;
pub mod detect;
pub mod download;
pub mod error;
pub mod install;
pub mod pathman;
pub mod pkgtools;
pub mod packages;
pub mod package_managers;
pub mod package_scan;
pub mod migration;
pub mod pip_reinstall;
pub mod global_reinstall;
pub mod old_packages;
mod reinstall_process;
pub mod sources;
pub mod switcher;
pub mod types;

#[cfg(test)]
mod test_support;

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
            commands::integrate_external_env,
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
            commands::list_installed_packages,
            commands::global_package_sources,
            commands::start_package_scan,
            commands::package_scan_status,
            commands::cancel_package_scan,
            commands::apply_tool_config,
            commands::preview_tool_migration,
            commands::migrate_tool_data,
            commands::preview_pip_reinstall,
            commands::start_pip_reinstall,
            commands::pip_reinstall_status,
            commands::cancel_pip_reinstall,
            commands::preview_global_reinstall,
            commands::start_global_reinstall,
            commands::global_reinstall_status,
            commands::cancel_global_reinstall,
            commands::preview_old_package_cleanup,
            commands::cleanup_old_packages,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
