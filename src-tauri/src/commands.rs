use std::path::{Path, PathBuf};
use std::sync::Mutex;

use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

use crate::config::AppConfig;
use crate::detect::{self, system};
use crate::error::{AppError, Result};
use crate::install::TaskManager;
use crate::pathman;
use crate::sources;
use crate::switcher;
use crate::types::{CategoryOverview, EnvType, Overview, ALL_ENV_TYPES};

pub struct AppState {
    pub cfg: Mutex<AppConfig>,
    pub tasks: TaskManager,
}

impl AppState {
    fn with_cfg<T>(&self, f: impl FnOnce(&AppConfig) -> T) -> T {
        let guard = self.cfg.lock().unwrap();
        f(&guard)
    }
    fn with_cfg_mut<T>(&self, f: impl FnOnce(&mut AppConfig) -> T) -> T {
        let mut guard = self.cfg.lock().unwrap();
        f(&mut guard)
    }
}

#[tauri::command]
pub async fn get_overview(state: State<'_, AppState>) -> Result<Overview> {
    let cfg = state.with_cfg(|c| c.clone());
    let root = cfg.resolve_root();
    let categories = match &root {
        Some(r) => detect::scan_root(r).await,
        None => ALL_ENV_TYPES
            .iter()
            .map(|&t| CategoryOverview {
                env_type: t,
                envs: vec![],
                current: None,
                junction_path: String::new(),
                scan_warning: None,
                current_error: None,
            })
            .collect(),
    };
    Ok(Overview {
        root: root.as_ref().map(|p| p.to_string_lossy().to_string()),
        root_exists: root.is_some(),
        portable: cfg.portable,
        data_dir: cfg.data_dir.to_string_lossy().to_string(),
        categories,
    })
}

#[tauri::command]
pub async fn set_root(state: State<'_, AppState>, path: String) -> Result<String> {
    if path.trim().is_empty() {
        return Err(AppError::msg("路径不能为空"));
    }
    let root = state.with_cfg_mut(|c| c.set_root(path.trim()))?;
    Ok(root.to_string_lossy().to_string())
}

#[tauri::command]
pub async fn scan_system(state: State<'_, AppState>, project_dirs: Option<Vec<String>>) -> Result<crate::types::ScanReport> {
    let root = state.with_cfg(|c| c.resolve_root());
    let integrated = integrated_targets(root.as_deref());
    let projects: Vec<PathBuf> = project_dirs.unwrap_or_default().into_iter().map(PathBuf::from).collect();
    if projects.len() > 8 || projects.iter().any(|p| !p.is_absolute()) {
        return Err(AppError::msg("最多选择 8 个项目目录，且必须使用绝对路径"));
    }
    Ok(system::scan_system(root.as_deref(), &integrated, &projects).await)
}

/// 收集 envs\ 下所有 junction 链接的目标(即已纳入管理的散装环境)
fn integrated_targets(root: Option<&Path>) -> Vec<PathBuf> {
    let Some(root) = root else {
        return vec![];
    };
    let mut out = Vec::new();
    let Ok(cats) = std::fs::read_dir(root.join("envs")) else {
        return out;
    };
    for cat in cats.flatten() {
        let Ok(entries) = std::fs::read_dir(cat.path()) else {
            continue;
        };
        for e in entries.flatten() {
            let p = e.path();
            if junction::exists(&p).unwrap_or(false) {
                if let Ok(t) = junction::get_target(&p) {
                    out.push(t.canonicalize().unwrap_or(t));
                }
            }
        }
    }
    out
}

/// 将散装环境以 junction 链接方式纳入管理(不移动/复制文件),返回环境名
#[tauri::command]
pub async fn integrate_external_env(
    state: State<'_, AppState>,
    env_type: EnvType,
    path: String,
    version: Option<String>,
) -> Result<String> {
    let root = require_root(&state)?;
    let exe = PathBuf::from(&path);
    if !exe.exists() {
        return Err(AppError::msg(format!("路径不存在: {path}")));
    }
    let install_root = system::install_root_for(env_type, &exe)
        .ok_or_else(|| AppError::msg("无法识别该工具的安装目录结构,不支持纳入管理"))?;
    let install_identity = install_root.canonicalize().unwrap_or_else(|_| install_root.clone());
    let root_identity = root.canonicalize().unwrap_or_else(|_| root.clone());
    if system::path_within(&root_identity, &install_identity) {
        return Err(AppError::msg("该环境已位于管理根目录内"));
    }
    // 名称:system-{版本},冲突时追加序号
    let base = match version.as_deref().and_then(version_token) {
        Some(v) => format!("system-{v}"),
        None => format!("system-{}", env_type.junction()),
    };
    let cat_dir = root.join("envs").join(env_type.folder());
    std::fs::create_dir_all(&cat_dir)?;
    let mut name = base.clone();
    let mut n = 2;
    while cat_dir.join(&name).exists() {
        name = format!("{base}-{n}");
        n += 1;
    }
    let link = cat_dir.join(&name);
    junction::create(&install_root, &link)
        .map_err(|e| AppError::msg(format!("创建链接失败: {e}")))?;
    Ok(name)
}

/// 从版本输出行提取纯版本号(如 `openjdk version "17.0.12" ...` → `17.0.12`)
fn version_token(raw: &str) -> Option<String> {
    let b = raw.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i].is_ascii_digit() {
            let start = i;
            let mut j = i;
            let mut dots = 0;
            while j < b.len() && (b[j].is_ascii_digit() || b[j] == b'.') {
                if b[j] == b'.' {
                    // 点后必须紧跟数字,否则结束
                    if j + 1 < b.len() && b[j + 1].is_ascii_digit() {
                        dots += 1;
                        j += 1;
                    } else {
                        break;
                    }
                } else {
                    j += 1;
                }
            }
            if dots >= 1 {
                return Some(raw[start..j].to_string());
            }
            i = j.max(i + 1);
        } else {
            i += 1;
        }
    }
    None
}

#[tauri::command]
pub async fn switch_env(
    state: State<'_, AppState>,
    env_type: EnvType,
    name: String,
) -> Result<()> {
    let _guard = crate::migration::LOCK.try_lock().map_err(|_| AppError::msg("包管理器任务正在执行，请结束后切换运行时"))?;
    reject_active_install(&state, env_type, &name)?;
    let root = require_root(&state)?;
    switcher::switch(&root, env_type, &name)
}

#[tauri::command]
pub async fn uninstall_env(
    state: State<'_, AppState>,
    env_type: EnvType,
    name: String,
) -> Result<()> {
    switcher::validate_name(&name)?;
    let _guard = crate::migration::LOCK.try_lock().map_err(|_| AppError::msg("包管理器任务正在执行，请结束后卸载运行时"))?;
    reject_active_install(&state, env_type, &name)?;
    let root = require_root(&state)?;
    let env_dir = root.join("envs").join(env_type.folder()).join(&name);
    if std::fs::symlink_metadata(&env_dir).is_err() {
        return Err(AppError::msg(format!("环境不存在: {}", env_dir.display())));
    }
    // 若是当前版本,先移除 junction
    let junction = root.join("current").join(env_type.junction());
    if crate::config::junction_target_name(&junction).as_deref() == Some(name.as_str()) {
        switcher::remove_junction_for(&root, env_type)?;
    }
    // 链接环境(纳入管理的散装环境):只删链接本身,不动原安装目录
    let is_link = std::fs::symlink_metadata(&env_dir)
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false);
    if is_link {
        std::fs::remove_dir(&env_dir)
            .map_err(|e| AppError::msg(format!("移除链接失败: {e}")))?;
        return Ok(());
    }
    tokio::task::spawn_blocking(move || force_remove_dir_all(&env_dir))
        .await
        .map_err(|e| AppError::msg(format!("删除任务失败: {e}")))??;
    Ok(())
}

#[tauri::command]
pub fn open_path(app: AppHandle, path: String) -> Result<()> {
    let p = PathBuf::from(&path);
    if !p.exists() {
        return Err(AppError::msg(format!("路径不存在: {path}")));
    }
    app.opener()
        .open_path(path, None::<&str>)
        .map_err(|e| AppError::msg(format!("打开失败: {e}")))?;
    Ok(())
}

#[tauri::command]
pub fn get_path_state() -> Result<crate::types::PathState> {
    pathman::get_path_state()
}

#[tauri::command]
pub fn save_user_path(state: State<'_, AppState>, entries: Vec<String>) -> Result<()> {
    let backups = state.with_cfg(|c| c.backups_dir());
    pathman::save_user_path(&entries, &backups)
}

/// 一键集成:为已安装的类型补全 current\* PATH 条目,返回新增条目
#[tauri::command]
pub fn integrate_current_to_path(state: State<'_, AppState>) -> Result<Vec<String>> {
    let root = state.with_cfg(|c| c.resolve_root());
    let Some(root) = root else {
        return Err(AppError::msg("尚未设置管理根目录"));
    };
    let entries = current_path_entries(&root);
    let backups = state.with_cfg(|c| c.backups_dir());
    pathman::integrate_entries(&entries, &backups)
}

fn current_path_entries(root: &Path) -> Vec<String> {
    let mut entries = Vec::new();
    for &t in ALL_ENV_TYPES.iter() {
        let installed = match t {
            EnvType::Rust => root
                .join("current")
                .join("rust")
                .join("cargo-home")
                .join("bin")
                .is_dir(),
            _ => root.join("current").join(t.junction()).exists(),
        };
        if installed {
            for p in t.path_entries(root) {
                entries.push(p.to_string_lossy().to_string());
            }
        }
    }
    entries
}

fn require_root(state: &State<'_, AppState>) -> Result<PathBuf> {
    state
        .with_cfg(|c| c.resolve_root())
        .ok_or_else(|| AppError::msg("尚未设置管理根目录,请先在设置中选择"))
}

/// 强制删除目录:先清除只读属性再删(JDK 等包含只读文件)
fn force_remove_dir_all(dir: &Path) -> Result<()> {
    fn clear_attrs(p: &Path) {
        if let Ok(meta) = std::fs::symlink_metadata(p) {
            if !meta.file_type().is_symlink() {
                let mut perms = meta.permissions();
                if perms.readonly() {
                    perms.set_readonly(false);
                    let _ = std::fs::set_permissions(p, perms);
                }
            }
        }
    }
    let mut stack = vec![dir.to_path_buf()];
    let mut dirs: Vec<PathBuf> = Vec::new();
    while let Some(p) = stack.pop() {
        clear_attrs(&p);
        if p.is_dir() {
            dirs.push(p.clone());
            if let Ok(rd) = std::fs::read_dir(&p) {
                for e in rd.flatten() {
                    let child = e.path();
                    clear_attrs(&child);
                    if child.is_dir() {
                        stack.push(child);
                    }
                }
            }
        }
    }
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

// ---------- 下载中心 ----------

/// 某类型可用的下载源列表
#[tauri::command]
pub fn get_sources(env_type: EnvType) -> Vec<sources::SourceInfo> {
    sources::available_sources(env_type)
}

#[tauri::command]
pub async fn list_versions(
    state: State<'_, AppState>,
    env_type: EnvType,
    source: Option<String>,
) -> Result<Vec<sources::VersionInfo>> {
    let source = source.unwrap_or_else(|| sources::SOURCE_MIRROR.into());
    let cache_path = state.with_cfg(|c| c.data_dir.join("version_cache.json"));
    sources::list_versions(env_type, &source, &cache_path).await
}

#[tauri::command]
pub fn start_install(
    app: AppHandle,
    state: State<'_, AppState>,
    env_type: EnvType,
    version: String,
    target_name: String,
    source: Option<String>,
    url: Option<String>,
) -> Result<u64> {
    let _guard = crate::migration::LOCK.try_lock().map_err(|_| AppError::msg("迁移、重装、配置或清理任务正在执行，请稍后安装"))?;
    switcher::validate_name(&target_name)?;
    // 校验名称:仅字母数字 . _ -
    let valid = !target_name.is_empty()
        && target_name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
            && !target_name.starts_with('.');
    if !valid {
        return Err(AppError::msg("名称仅可包含字母、数字与 . _ -"));
    }
    let source = source.unwrap_or_else(|| sources::SOURCE_MIRROR.into());
    let cfg = state.with_cfg(|c| c.clone());
    state
        .tasks
        .start(app, cfg, env_type, version, target_name, source, url)
}

#[tauri::command]
pub async fn cancel_install(state: State<'_, AppState>, id: u64) -> Result<()> {
    state.tasks.cancel(id).await
}

#[tauri::command]
pub fn install_tasks(state: State<'_, AppState>) -> Vec<crate::install::TaskSnapshot> {
    state.tasks.snapshots()
}

// ---------- 缓存与设置 ----------

fn reject_active_install(state: &AppState, env_type: EnvType, name: &str) -> Result<()> {
    if state.tasks.snapshots().iter().any(|t| t.env_type == env_type && t.target_name.eq_ignore_ascii_case(name)
        && matches!(t.status, crate::install::TaskStatus::Downloading { .. } | crate::install::TaskStatus::Installing { .. })) {
        return Err(AppError::msg("该环境正在安装，暂不能切换或卸载"));
    }
    Ok(())
}

#[tauri::command]
pub async fn get_caches(state: State<'_, AppState>) -> Result<Vec<crate::caches::CacheInfo>> {
    let (root, downloads) = state.with_cfg(|c| (c.resolve_root(), c.settings.downloads_dir.clone()));
    let downloads = downloads.map(PathBuf::from);
    let environment = crate::detect::discovery::effective_environment();
    let configured = crate::pkgtools::effective_cache_paths(root.as_deref()).await;
    let caches = crate::caches::detect_caches(root.as_deref(), downloads.as_deref(), &environment, &configured);
    Ok(crate::caches::compute_sizes(caches).await)
}

#[tauri::command]
pub async fn clean_cache(state: State<'_, AppState>, tool: String, expected_path: String) -> Result<u64> {
    let guard = crate::migration::LOCK.try_lock().map_err(|_| AppError::msg("正在迁移或配置路径，请稍后再清理缓存"))?;
    let (root, downloads) = state.with_cfg(|c| (c.resolve_root(), c.settings.downloads_dir.clone()));
    let downloads = downloads.map(PathBuf::from);
    let environment = crate::detect::discovery::effective_environment();
    let configured = crate::pkgtools::effective_cache_paths(root.as_deref()).await;
    if tool == "downloads" && state.tasks.snapshots().iter().any(|t| matches!(t.status, crate::install::TaskStatus::Downloading { .. } | crate::install::TaskStatus::Installing { .. })) {
        return Err(AppError::msg("存在进行中的下载或安装，请任务结束后再清理下载目录"));
    }
    let caches = crate::caches::detect_caches(root.as_deref(), downloads.as_deref(), &environment, &configured);
    let current = caches.iter().find(|c| c.tool == tool).ok_or_else(|| AppError::msg("未知缓存"))?;
    if !system::paths_eq(Path::new(&current.path), Path::new(&expected_path)) { return Err(AppError::msg("缓存路径已改变，请刷新后重新确认")); }
    tokio::task::spawn_blocking(move || { let _guard = guard; crate::caches::clean_cache(&tool, root.as_deref(), downloads.as_deref(), &environment, &configured) })
        .await
        .map_err(|e| AppError::msg(format!("清理任务失败: {e}")))?
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsView {
    pub root: Option<String>,
    pub downloads_dir: Option<String>,
    pub portable: bool,
    pub data_dir: String,
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> SettingsView {
    state.with_cfg(|c| SettingsView {
        root: c.settings.root.clone(),
        downloads_dir: c.settings.downloads_dir.clone(),
        portable: c.portable,
        data_dir: c.data_dir.to_string_lossy().to_string(),
    })
}

#[tauri::command]
pub fn set_downloads_dir(state: State<'_, AppState>, dir: String) -> Result<()> {
    if !Path::new(&dir).is_absolute() { return Err(AppError::msg("下载目录必须是绝对路径")); }
    state.with_cfg_mut(|c| {
        let old = c.settings.downloads_dir.clone();
        c.settings.downloads_dir = Some(dir);
        if let Err(e) = c.save() { c.settings.downloads_dir = old; return Err(e); }
        Ok(())
    })
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PathBackupInfo {
    pub file: String,
    pub time: String,
}

#[tauri::command]
pub fn list_path_backups(state: State<'_, AppState>) -> Result<Vec<PathBackupInfo>> {
    let dir = state.with_cfg(|c| c.backups_dir());
    let mut out = Vec::new();
    if !dir.exists() {
        return Ok(out);
    }
    for entry in std::fs::read_dir(&dir)?.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|x| x == "json") {
            let name = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            // 20260908T054401Z → 可读时间
            let time = parse_backup_name(&name);
            out.push(PathBackupInfo {
                file: path.to_string_lossy().to_string(),
                time,
            });
        }
    }
    out.sort_by(|a, b| b.file.cmp(&a.file));
    Ok(out)
}

fn parse_backup_name(name: &str) -> String {
    if name.len() < 16 || !name.is_ascii() || name.as_bytes()[8] != b'T'
        || !name[..8].bytes().all(|b| b.is_ascii_digit()) || !name[9..15].bytes().all(|b| b.is_ascii_digit()) {
        return name.to_owned();
    }
    // 20260908T054401Z
    let y = &name[0..4];
    let mo = &name[4..6];
    let d = &name[6..8];
    let h = &name[9..11];
    let mi = &name[11..13];
    let s = &name[13..15];
    format!("{y}-{mo}-{d} {h}:{mi}:{s} UTC")
}

#[tauri::command]
pub fn restore_path_backup(state: State<'_, AppState>, file: String) -> Result<()> {
    let path = PathBuf::from(&file);
    let backups_dir = state.with_cfg(|c| c.backups_dir());
    if !system::path_within(&backups_dir, &path) || !path.extension().is_some_and(|e| e == "json") { return Err(AppError::msg("请选择应用备份目录中的 JSON 文件")); }
    let content = std::fs::read_to_string(&path)?;
    let entries = parse_path_backup(&content)?;
    pathman::save_user_path(&entries, &backups_dir)
}

fn parse_path_backup(content: &str) -> Result<Vec<String>> {
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Backup { user_path: Vec<String> }
    Ok(serde_json::from_str::<Backup>(content).map_err(|e| AppError::msg(format!("PATH 备份格式无效，未修改 PATH: {e}")))?.user_path)
}

// ---------- 环境变量管理 ----------

/// 常见开发环境变量与说明
const COMMON_ENV_VARS: &[(&str, &str, Option<EnvType>)] = &[
    ("JAVA_HOME", "JDK 根目录", Some(EnvType::Jdk)),
    ("GOPATH", "Go 工作区(默认 %USERPROFILE%\\go)", None),
    ("GOROOT", "Go 安装根目录", Some(EnvType::Go)),
    ("MAVEN_HOME", "Maven 安装目录", Some(EnvType::Maven)),
    ("GRADLE_HOME", "Gradle 安装目录", Some(EnvType::Gradle)),
    ("RUSTUP_HOME", "Rust 工具链存储目录", Some(EnvType::Rust)),
    ("CARGO_HOME", "Cargo 注册表与 bin 目录", Some(EnvType::Rust)),
    ("PHP_HOME", "PHP 安装目录", Some(EnvType::Php)),
    ("ZIG_HOME", "Zig 安装目录", Some(EnvType::Zig)),
    ("LLVM_HOME", "LLVM 安装目录", Some(EnvType::Llvm)),
];

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvVarInfo {
    pub name: String,
    pub label: String,
    /// 当前用户级值(未设置为 None)
    pub value: Option<String>,
    /// 根据当前激活环境推导的建议值
    pub suggested: Option<String>,
}

fn suggested_env_path(root: &Path, env_type: EnvType, name: &str) -> Option<String> {
    let cur = root.join("current").join(env_type.junction());
    if !cur.is_dir() {
        return None;
    }
    let path = match (env_type, name) {
        (EnvType::Rust, "CARGO_HOME") => cur.join("cargo-home"),
        (EnvType::Rust, "RUSTUP_HOME") => cur.join("rustup-home"),
        _ => cur,
    };
    Some(path.to_string_lossy().to_string())
}

#[tauri::command]
pub fn get_env_vars(state: State<'_, AppState>) -> Result<Vec<EnvVarInfo>> {
    let user_vars = pathman::get_user_env_vars()?;
    let root = state.with_cfg(|c| c.resolve_root());

    Ok(COMMON_ENV_VARS
        .iter()
        .map(|&(name, label, env_type)| {
            let value = user_vars
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(name))
                .map(|(_, v)| v.clone());
            // 根据当前激活环境推导建议值
            let suggested = match (env_type, &root) {
                (Some(t), Some(r)) => suggested_env_path(r, t, name),
                // Gradle 用户目录:挂到根目录下,与激活版本无关
                (None, Some(r)) if name == "GRADLE_USER_HOME" => {
                    Some(r.join("envs").join("gradle-user-home").to_string_lossy().to_string())
                }
                _ => None,
            };
            EnvVarInfo { name: name.into(), label: label.into(), value, suggested }
        })
        .collect())
}

#[tauri::command]
pub fn save_env_var(state: State<'_, AppState>, name: String, value: String) -> Result<()> {
    let backups = state.with_cfg(|c| c.backups_dir());
    if value.trim().is_empty() {
        pathman::delete_user_env_var(&name, &backups)
            .map_err(|e| AppError::msg(format!("删除失败(可能本就未设置): {e}")))
    } else {
        pathman::set_user_env_var(&name, value.trim(), &backups)
    }
}

// ---------- 包管理器路径配置 ----------

#[tauri::command]
pub async fn start_package_scan(state: State<'_, AppState>, mode: String, directory: Option<String>) -> Result<()> {
    crate::package_scan::start(state.with_cfg(|c| c.resolve_root()), mode, directory).await
}

#[tauri::command]
pub fn package_scan_status() -> Option<crate::package_scan::Snapshot> { crate::package_scan::status() }

#[tauri::command]
pub fn cancel_package_scan() { crate::package_scan::cancel(); }

#[tauri::command]
pub async fn list_installed_packages(state: State<'_, AppState>, tool: crate::package_managers::ManagerId, directory: Option<String>, historical: Option<bool>) -> Result<crate::packages::PackageList> {
    let root = state.with_cfg(|c| c.resolve_root());
    crate::packages::list_scope(root.as_deref(), tool.as_ref(), directory.as_deref(), historical.unwrap_or(false)).await
}

#[tauri::command]
pub async fn global_package_sources(state: State<'_, AppState>, tool: crate::package_managers::ManagerId) -> Result<Vec<String>> {
    let root = state.with_cfg(|c| c.resolve_root());
    crate::packages::global_sources(root.as_deref(), tool.as_ref()).await
}

#[tauri::command]
pub async fn get_tool_configs(
    state: State<'_, AppState>,
) -> Result<Vec<crate::pkgtools::ToolConfig>> {
    let root = state.with_cfg(|c| c.resolve_root());
    Ok(crate::pkgtools::get_tool_configs(root.as_deref()).await)
}

#[tauri::command]
pub async fn apply_tool_config(
    state: State<'_, AppState>,
    tool: String,
    global_path: Option<String>,
    cache_path: Option<String>,
) -> Result<Vec<String>> {
    let _guard = crate::migration::LOCK.try_lock().map_err(|_| AppError::msg("正在迁移或修改包管理器路径，请稍后重试"))?;
    let root = state.with_cfg(|c| c.resolve_root());
    let mut applied = crate::pkgtools::apply_tool_config(
        root.as_deref(),
        &tool,
        global_path.as_deref(),
        cache_path.as_deref(),
        &state.with_cfg(|c| c.backups_dir()),
    )
    .await?;

    // npm:全局 bin 目录需要进 PATH,全局安装的命令才能直接使用
    if tool == "npm" || crate::package_managers::isolated(&tool) {
        if let Some(g) = &global_path {
            let g = crate::package_managers::bin_path(&tool, Path::new(g)).to_string_lossy().into_owned();
            let backups = state.with_cfg(|c| c.backups_dir());
            let added = pathman::integrate_entries(&[g.clone()], &backups)?;
            if !added.is_empty() {
                applied.push(format!("已将 {g} 加入用户 PATH"));
            }
        }
    }
    Ok(applied)
}

#[tauri::command]
pub async fn preview_tool_migration(
    state: State<'_, AppState>, tool: String, kind: String, source: String,
) -> Result<crate::migration::MigrationPlan> {
    let root = require_root(&state)?;
    tokio::task::spawn_blocking(move || crate::migration::preview(&root, &tool, &kind, Path::new(&source)))
        .await.map_err(|e| AppError::msg(format!("迁移预览失败: {e}")))?
}

#[tauri::command]
pub async fn migrate_tool_data(
    state: State<'_, AppState>, plan: crate::migration::MigrationPlan,
) -> Result<crate::migration::MigrationResult> {
    let guard = crate::migration::LOCK.try_lock().map_err(|_| AppError::msg("正在迁移或修改包管理器路径，请稍后重试"))?;
    let root = require_root(&state)?;
    tokio::task::spawn_blocking(move || {
        // Hold the lock in the worker even if the invoking webview is closed.
        let _guard = guard;
        crate::migration::execute(&root, &plan)
    }).await.map_err(|e| AppError::msg(format!("迁移任务失败: {e}")))?
}

#[tauri::command]
pub async fn preview_pip_reinstall(source: String, source_kind: String, python: String, destination: String) -> Result<crate::pip_reinstall::Plan> {
    crate::pip_reinstall::preview(source, source_kind, python, destination).await
}

#[tauri::command]
pub async fn start_pip_reinstall(plan: crate::pip_reinstall::Plan, names: Vec<String>) -> Result<()> {
    crate::pip_reinstall::start(plan, names).await
}

#[tauri::command]
pub fn pip_reinstall_status() -> Option<crate::pip_reinstall::Snapshot> { crate::pip_reinstall::status() }

#[tauri::command]
pub fn cancel_pip_reinstall() { crate::pip_reinstall::cancel(); }

#[tauri::command]
pub async fn preview_global_reinstall(tool: String, executable: String, source: String, destination: String) -> Result<crate::global_reinstall::Plan> {
    crate::global_reinstall::preview(tool, executable, source, destination).await
}
#[tauri::command]
pub async fn start_global_reinstall(plan: crate::global_reinstall::Plan, names: Vec<String>) -> Result<()> {
    crate::global_reinstall::start(plan, names).await
}
#[tauri::command]
pub fn global_reinstall_status(tool: String) -> Option<crate::global_reinstall::Snapshot> { crate::global_reinstall::status(&tool) }
#[tauri::command]
pub fn cancel_global_reinstall(tool: String) { crate::global_reinstall::cancel(&tool); }

#[tauri::command]
pub async fn preview_old_package_cleanup(tool: String, destination: String) -> Result<crate::old_packages::Preview> {
    crate::old_packages::preview(tool, destination).await
}
#[tauri::command]
pub async fn cleanup_old_packages(tool: String, token: String, names: Vec<String>) -> Result<crate::old_packages::CleanupResult> {
    crate::old_packages::execute(tool, token, names).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_backups_do_not_become_empty_path() {
        for value in ["{}", r#"{"userPath":null}"#, r#"{"userPath":"D:/x"}"#, "bad"] { assert!(parse_path_backup(value).is_err()); }
        assert!(parse_path_backup(r#"{"userPath":[]}"#).unwrap().is_empty());
        assert_eq!(parse_backup_name("x"), "x");
        assert_eq!(parse_backup_name("用户自定义"), "用户自定义");
        assert!(parse_backup_name("20260912T123456Z-123").contains("12:34:56"));
    }

    #[test]
    fn rust_paths_follow_current_junction_and_keep_homes_separate() {
        let dir = crate::test_support::TestDir::new();
        let root = dir.path();
        let installed = root.join("envs/rusts/rust-stable");
        std::fs::create_dir_all(installed.join("cargo-home/bin")).unwrap();
        std::fs::create_dir_all(installed.join("rustup-home")).unwrap();
        assert!(current_path_entries(root).is_empty());
        assert!(suggested_env_path(root, EnvType::Rust, "CARGO_HOME").is_none());
        switcher::switch(root, EnvType::Rust, "rust-stable").unwrap();
        let current = root.join("current").join("rust");
        assert_eq!(
            current_path_entries(root),
            vec![current.join("cargo-home").join("bin").to_string_lossy().to_string()]
        );
        assert_eq!(
            suggested_env_path(root, EnvType::Rust, "CARGO_HOME"),
            Some(current.join("cargo-home").to_string_lossy().to_string())
        );
        assert_eq!(
            suggested_env_path(root, EnvType::Rust, "RUSTUP_HOME"),
            Some(current.join("rustup-home").to_string_lossy().to_string())
        );
    }
}
