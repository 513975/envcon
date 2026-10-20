pub mod app_update;
pub mod caches;
mod fsutil;
mod singleflight;
mod scan_cache;
mod size_tasks;
pub mod commands;
pub mod config;
pub mod detect;
pub mod download;
pub mod error;
pub mod install;
pub mod pathman;
mod path_ownership;
mod link_ownership;
mod installer_uninstall;
pub mod pkgtools;
pub mod packages;
pub mod package_managers;
pub mod package_scan;
pub mod migration;
pub mod pip_reinstall;
pub mod global_reinstall;
pub mod uv_recovery;
pub mod old_packages;
pub mod update_journal;
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
    let cfg = config::AppConfig::load();
    let scans = scan_cache::ScanCache::new(&cfg.data_dir);
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState {
            cfg: Mutex::new(cfg),
            tasks: install::TaskManager::new(),
            tool_queries: Default::default(),
            scans,
            sizes: Default::default(),
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_overview,
            commands::redetect_environment,
            commands::export_environment_manifest,
            commands::import_environment_manifest,
            commands::query_scan,
            commands::invalidate_scans,
            commands::start_size_scan,
            commands::size_scan_status,
            commands::cancel_size_scan,
            commands::update_package_manager,
            commands::update_package_managers,
            commands::preview_package_manager_update,
            commands::preview_package_manager_updates,
            commands::package_manager_update_record,
            commands::set_root,
            commands::scan_system,
            commands::integrate_external_env,
            commands::switch_env,
            commands::preview_uninstall_env,
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
            commands::check_app_update,
            commands::reload_settings,
            commands::restore_settings_backup,
            commands::set_downloads_dir,
            commands::list_path_backups,
            commands::restore_path_backup,
            commands::get_env_vars,
            commands::save_env_var,
            commands::get_tool_configs,
            commands::inspect_uv_layout,
            commands::recover_uv_layout,
            commands::list_installed_packages,
            commands::global_package_sources,
            commands::start_package_scan,
            commands::package_scan_status,
            commands::cancel_package_scan,
            commands::apply_tool_config,
            commands::preview_tool_config,
            commands::preview_tool_configs,
            commands::preview_path_repair,
            commands::apply_path_repair,
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
            commands::preview_scanned_package_cleanup,
            commands::cleanup_old_packages,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
