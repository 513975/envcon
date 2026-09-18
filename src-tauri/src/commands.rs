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
    pub tool_queries: crate::singleflight::SingleFlight<Option<PathBuf>, Vec<crate::pkgtools::ToolConfig>>,
    pub scans: std::sync::Arc<crate::scan_cache::ScanCache>,
    pub sizes: crate::size_tasks::SizeTasks,
}

impl AppState {
    fn with_cfg<T>(&self, f: impl FnOnce(&AppConfig) -> T) -> T {
        let guard = self.cfg.lock().unwrap();
        f(&guard)
    }
    fn with_cfg_mut<T>(&self, f: impl FnOnce(&mut AppConfig) -> T) -> T {
        let _query_guard = self.tool_queries.mutation();
        let _scan_guard = self.scans.mutation();
        let mut guard = self.cfg.lock().unwrap();
        f(&mut guard)
    }

    async fn tool_configs(&self, root: Option<PathBuf>, force: bool) -> Vec<crate::pkgtools::ToolConfig> {
        self.tool_queries.read(root.clone(), force, || async {
            crate::pkgtools::get_tool_configs(root.as_deref()).await
        }).await
    }
}

#[tauri::command]
pub fn invalidate_scans(state: State<'_, AppState>) {
    state.scans.invalidate();
    state.sizes.invalidate();
}

#[tauri::command]
pub async fn start_size_scan(state: State<'_, AppState>, env_type: Option<EnvType>, name: Option<String>, cache_tool: Option<String>) -> Result<u64> {
    if let (Some(kind), Some(name), None) = (env_type, name.as_deref(), cache_tool.as_ref()) {
        switcher::validate_name(name)?;
        let root = require_root(&state)?;
        validate_environment_parent(&root, kind)?;
        reject_active_install(&state, kind, name)?;
        let path = root.join("envs").join(kind.folder()).join(name);
        return state.sizes.start(path.to_string_lossy().into(), path);
    }
    if let (None, None, Some(tool)) = (env_type, name, cache_tool) {
        let cfg = state.with_cfg(Clone::clone);
        let root = cfg.resolve_root();
        let environment = crate::detect::discovery::effective_environment();
        let configured = crate::pkgtools::effective_cache_paths(root.as_deref()).await;
        let cache = crate::caches::detect_caches(root.as_deref(), cfg.settings.downloads_dir.as_deref().map(Path::new), &environment, &configured)
            .into_iter().find(|c| c.tool == tool).ok_or_else(|| AppError::msg("未知缓存目录"))?;
        return state.sizes.start(format!("cache:{tool}"), PathBuf::from(cache.path));
    }
    Err(AppError::msg("请选择一个环境或缓存目录"))
}

#[tauri::command]
pub fn size_scan_status(state: State<'_, AppState>) -> Vec<crate::size_tasks::SizeSnapshot> { state.sizes.snapshots() }

#[tauri::command]
pub fn cancel_size_scan(state: State<'_, AppState>, id: u64) -> Result<()> { state.sizes.cancel(id) }

#[tauri::command]
pub async fn query_scan(
    state: State<'_, AppState>,
    kind: String,
    project_dirs: Option<Vec<String>>,
    force: Option<bool>,
    cached_only: Option<bool>,
) -> Result<Option<crate::scan_cache::Snapshot>> {
    if !matches!(kind.as_str(), "overview" | "system" | "tools" | "caches") { return Err(AppError::msg("未知扫描类型")); }
    let projects: Vec<PathBuf> = project_dirs.unwrap_or_default().into_iter().map(PathBuf::from).collect();
    if projects.len() > 8 || projects.iter().any(|p| !p.is_absolute()) { return Err(AppError::msg("最多选择 8 个绝对路径项目目录")); }
    let cfg = state.with_cfg(Clone::clone);
    let root = cfg.resolve_root();
    let key = crate::scan_cache::key(&kind, root.as_deref(), &projects);
    if cached_only.unwrap_or(false) { return Ok(state.scans.peek(&key)); }
    let previous = state.scans.peek(&key).map(|s| s.value);
    let fp_root = root.clone();
    let fp_projects = projects.clone();
    let settings = cfg.settings.clone();
    let task_state = serde_json::to_string(&state.tasks.snapshots())?;
    let fingerprint = tokio::task::spawn_blocking(move || format!("{}:{task_state}", crate::scan_cache::fingerprint(fp_root.as_deref(), &fp_projects, &settings, previous.as_ref())))
        .await.map_err(|e| AppError::msg(e.to_string()))?;
    let snapshot = state.scans.read(key.clone(), fingerprint, force.unwrap_or(false), || async {
        let value = match kind.as_str() {
            "overview" => serde_json::to_value(get_overview(state.clone()).await?)?,
            "system" => serde_json::to_value(scan_system(state.clone(), Some(projects.iter().map(|p| p.to_string_lossy().into()).collect())).await?)?,
            "tools" => serde_json::to_value(state.tool_configs(root.clone(), force.unwrap_or(false)).await)?,
            "caches" => {
                let environment = crate::detect::discovery::effective_environment();
                // Configuration queries are shared with the tool page; recursive sizing is an explicit action.
                let tool_key = crate::scan_cache::key("tools", root.as_deref(), &[]);
                let cached = state.scans.peek(&tool_key).filter(|s| crate::scan_cache::now_ms().saturating_sub(s.updated_at) < crate::scan_cache::RECHECK_MS);
                let configs: Vec<crate::pkgtools::ToolConfig> = match cached.and_then(|s| serde_json::from_value(s.value).ok()) {
                    Some(configs) if !force.unwrap_or(false) => configs,
                    _ => state.tool_configs(root.clone(), false).await,
                };
                let configured = configs.into_iter().filter_map(|t| t.cache_path.map(|p| (t.tool, PathBuf::from(p)))).collect();
                serde_json::to_value(crate::caches::detect_caches(root.as_deref(), cfg.settings.downloads_dir.as_deref().map(Path::new), &environment, &configured))?
            }
            _ => unreachable!(),
        };
        Ok(value)
    }).await?;
    // Include paths first discovered by this scan so the first warm read is not a false miss.
    let stamp_root = root.clone();
    let stamp_projects = projects.clone();
    let stamp_settings = cfg.settings.clone();
    let value = snapshot.value.clone();
    let task_state = serde_json::to_string(&state.tasks.snapshots())?;
    let stamp = tokio::task::spawn_blocking(move || format!("{}:{task_state}", crate::scan_cache::fingerprint(stamp_root.as_deref(), &stamp_projects, &stamp_settings, Some(&value))))
        .await.map_err(|e| AppError::msg(e.to_string()))?;
    state.scans.update_fingerprint(&key, snapshot.updated_at, stamp);
    Ok(Some(snapshot))
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
pub async fn update_package_manager(
    state: State<'_, AppState>,
    tool: crate::package_managers::ManagerId,
) -> Result<String> {
    let _query_guard = state.tool_queries.mutation();
    let _scan_guard = state.scans.mutation();
    let _guard = crate::migration::LOCK
        .try_lock()
        .map_err(|_| AppError::msg("扫描、安装或包管理器任务正在执行，请稍后更新"))?;
    if state.tasks.snapshots().iter().any(|t| {
        matches!(
            t.status,
            crate::install::TaskStatus::Downloading { .. }
                | crate::install::TaskStatus::Installing { .. }
        )
    }) {
        return Err(AppError::msg("环境正在安装，请完成后更新包管理器"));
    }
    let id = tool.as_ref();
    let inv = crate::pkgtools::find_invocation(state.with_cfg(|c| c.resolve_root()).as_deref(), id)
        .await
        .ok_or_else(|| AppError::msg(format!("未找到 {id} 可执行命令")))?;
    let before = crate::pkgtools::run(&inv, &["--version"], 15)
        .await
        .map_err(AppError::msg)?;
    if id == "yarn" && !before.starts_with("1.") {
        return Err(AppError::msg(
            "Yarn Berry 请在所属项目中使用 yarn set version stable 更新",
        ));
    }
    let (updater, args) = crate::pkgtools::self_update_command(id, &inv)?;
    if id == "pip"
        && crate::pkgtools::run(&updater, &["--version"], 15)
            .await
            .map_err(AppError::msg)?
            != before
    {
        return Err(AppError::msg(
            "无法确认 pip 所属解释器，请使用原解释器运行 python -m pip install --upgrade pip",
        ));
    }
    let args: Vec<_> = args.iter().map(String::as_str).collect();
    let output = crate::pkgtools::mutate(&updater, &args, 600)
        .await
        .map_err(|e| AppError::msg(format!("{id} 更新失败: {e}")))?;
    let after = crate::pkgtools::run(&inv, &["--version"], 15)
        .await
        .map_err(AppError::msg)?;
    Ok(format!(
        "{id}: {before} -> {after}\n{output}{}",
        if before == after {
            "\n当前命令版本未变化，可能已是最新版；也请核对原安装方式与命令路径。"
        } else {
            ""
        }
    ))
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
pub async fn scan_system(
    state: State<'_, AppState>,
    project_dirs: Option<Vec<String>>,
) -> Result<crate::types::ScanReport> {
    let root = state.with_cfg(|c| c.resolve_root());
    let integrated = integrated_targets(root.as_deref());
    let projects: Vec<PathBuf> = project_dirs
        .unwrap_or_default()
        .into_iter()
        .map(PathBuf::from)
        .collect();
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
    let _scan_guard = state.scans.mutation();
    let _guard = crate::migration::LOCK.try_lock()
        .map_err(|_| AppError::msg("正在执行包管理器任务，请稍后纳入管理"))?;
    let root = require_root(&state)?;
    let exe = PathBuf::from(&path);
    if !exe.exists() {
        return Err(AppError::msg(format!("路径不存在: {path}")));
    }
    let install_root = system::install_root_for(env_type, &exe)
        .ok_or_else(|| AppError::msg("无法识别该工具的安装目录结构,不支持纳入管理"))?;
    let install_identity = install_root
        .canonicalize()
        .unwrap_or_else(|_| install_root.clone());
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
    validate_environment_parent(&root, env_type)?;
    if integrated_targets(Some(&root)).iter().any(|p| system::paths_eq(p, &install_identity)) {
        return Err(AppError::msg("此安装目录已经纳入管理，请刷新环境列表"));
    }
    let mut name = base.clone();
    let mut n = 2;
    while std::fs::symlink_metadata(cat_dir.join(&name)).is_ok() {
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
pub async fn switch_env(state: State<'_, AppState>, env_type: EnvType, name: String) -> Result<()> {
    let _query_guard = state.tool_queries.mutation();
    let _scan_guard = state.scans.mutation();
    let _guard = crate::migration::LOCK
        .try_lock()
        .map_err(|_| AppError::msg("包管理器任务正在执行，请结束后切换运行时"))?;
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
    let _query_guard = state.tool_queries.mutation();
    let _scan_guard = state.scans.mutation();
    switcher::validate_name(&name)?;
    let _guard = crate::migration::LOCK
        .try_lock()
        .map_err(|_| AppError::msg("包管理器任务正在执行，请结束后卸载运行时"))?;
    reject_active_install(&state, env_type, &name)?;
    let root = require_root(&state)?;
    let env_dir = root.join("envs").join(env_type.folder()).join(&name);
    validate_environment_parent(&root, env_type)?;
    if std::fs::symlink_metadata(&env_dir).is_err() {
        return Err(AppError::msg(format!("环境不存在: {}", env_dir.display())));
    }
    // 若是当前版本,先移除 junction
    let junction = root.join("current").join(env_type.junction());
    let current_matches = std::fs::read_link(&junction).ok().is_some_and(|target| {
        let target = if target.is_absolute() {
            target
        } else {
            junction.parent().unwrap().join(target)
        };
        system::paths_eq(&target, &env_dir)
            || target
                .canonicalize()
                .ok()
                .zip(env_dir.canonicalize().ok())
                .is_some_and(|(a, b)| system::paths_eq(&a, &b))
    });
    if current_matches {
        switcher::remove_junction_for(&root, env_type)?;
    }
    // 链接环境(纳入管理的散装环境):只删链接本身,不动原安装目录
    let is_link = is_reparse(&std::fs::symlink_metadata(&env_dir)?);
    if is_link {
        std::fs::remove_dir(&env_dir).map_err(|e| AppError::msg(format!("移除链接失败: {e}")))?;
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
    let _query_guard = state.tool_queries.mutation();
    let _scan_guard = state.scans.mutation();
    let backups = state.with_cfg(|c| c.backups_dir());
    pathman::save_user_path(&entries, &backups)
}

/// 一键集成:为已安装的类型补全 current\* PATH 条目,返回新增条目
#[tauri::command]
pub fn integrate_current_to_path(state: State<'_, AppState>) -> Result<Vec<String>> {
    let _query_guard = state.tool_queries.mutation();
    let _scan_guard = state.scans.mutation();
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
            EnvType::Rust => root.join("current").join("rust").join("cargo-home").join("bin").is_dir()
                || root.join("current").join("rust").join("bin").is_dir(),
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
            if !is_reparse(&meta) {
                let mut perms = meta.permissions();
                if perms.readonly() {
                    perms.set_readonly(false);
                    let _ = std::fs::set_permissions(p, perms);
                }
            }
        }
    }
    fn remove(p: &Path) -> Result<()> {
        let meta = std::fs::symlink_metadata(p)?;
        clear_attrs(p);
        if is_reparse(&meta) {
            return std::fs::remove_file(p)
                .or_else(|_| std::fs::remove_dir(p))
                .map_err(Into::into);
        }
        if meta.is_dir() {
            for entry in std::fs::read_dir(p)? {
                remove(&entry?.path())?;
            }
            std::fs::remove_dir(p)?;
        } else {
            std::fs::remove_file(p)?;
        }
        Ok(())
    }
    remove(dir)
}

fn is_reparse(meta: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    meta.file_attributes() & 0x400 != 0
}

fn validate_environment_parent(root: &Path, env_type: EnvType) -> Result<()> {
    let canonical_root = root.canonicalize()?;
    let expected = canonical_root.join("envs").join(env_type.folder());
    if !system::paths_eq(&expected, &expected.canonicalize()?) {
        return Err(AppError::msg("环境分类目录被重定向，禁止递归删除"));
    }
    let current = canonical_root.join("current");
    match std::fs::symlink_metadata(&current) {
        Ok(_) if !system::paths_eq(&current, &current.canonicalize()?) => {
            return Err(AppError::msg("current 父目录被重定向，禁止修改外部链接"));
        }
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
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
    let _guard = crate::migration::LOCK
        .try_lock()
        .map_err(|_| AppError::msg("迁移、重装、配置或清理任务正在执行，请稍后安装"))?;
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
    if state.tasks.snapshots().iter().any(|t| {
        t.env_type == env_type
            && t.target_name.eq_ignore_ascii_case(name)
            && matches!(
                t.status,
                crate::install::TaskStatus::Downloading { .. }
                    | crate::install::TaskStatus::Installing { .. }
            )
    }) {
        return Err(AppError::msg("该环境正在安装，暂不能切换或卸载"));
    }
    Ok(())
}

#[tauri::command]
pub async fn get_caches(state: State<'_, AppState>, force: Option<bool>) -> Result<Vec<crate::caches::CacheInfo>> {
    let (root, downloads) =
        state.with_cfg(|c| (c.resolve_root(), c.settings.downloads_dir.clone()));
    let downloads = downloads.map(PathBuf::from);
    let environment = crate::detect::discovery::effective_environment();
    let configured = state.tool_configs(root.clone(), force.unwrap_or(false)).await
        .into_iter().filter_map(|tool| tool.cache_path.map(|p| (tool.tool, PathBuf::from(p)))).collect();
    let caches = crate::caches::detect_caches(
        root.as_deref(),
        downloads.as_deref(),
        &environment,
        &configured,
    );
    Ok(crate::caches::compute_sizes(caches).await)
}

#[tauri::command]
pub async fn clean_cache(
    state: State<'_, AppState>,
    tool: String,
    expected_path: String,
) -> Result<u64> {
    let guard = crate::migration::LOCK
        .try_lock()
        .map_err(|_| AppError::msg("正在迁移或配置路径，请稍后再清理缓存"))?;
    let (root, downloads) =
        state.with_cfg(|c| (c.resolve_root(), c.settings.downloads_dir.clone()));
    let downloads = downloads.map(PathBuf::from);
    let environment = crate::detect::discovery::effective_environment();
    let configured = crate::pkgtools::effective_cache_paths(root.as_deref()).await;
    if tool == "downloads"
        && state.tasks.snapshots().iter().any(|t| {
            matches!(
                t.status,
                crate::install::TaskStatus::Downloading { .. }
                    | crate::install::TaskStatus::Installing { .. }
            )
        })
    {
        return Err(AppError::msg(
            "存在进行中的下载或安装，请任务结束后再清理下载目录",
        ));
    }
    let caches = crate::caches::detect_caches(
        root.as_deref(),
        downloads.as_deref(),
        &environment,
        &configured,
    );
    let current = caches
        .iter()
        .find(|c| c.tool == tool)
        .ok_or_else(|| AppError::msg("未知缓存"))?;
    if !system::paths_eq(Path::new(&current.path), Path::new(&expected_path)) {
        return Err(AppError::msg("缓存路径已改变，请刷新后重新确认"));
    }
    tokio::task::spawn_blocking(move || {
        let _guard = guard;
        crate::caches::clean_cache(
            &tool,
            root.as_deref(),
            downloads.as_deref(),
            &environment,
            &configured,
        )
    })
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
    pub config_warning: Option<String>,
    pub write_blocked: bool,
    pub has_config_backup: bool,
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> SettingsView {
    state.with_cfg(|c| SettingsView {
        root: c.settings.root.clone(),
        downloads_dir: c.settings.downloads_dir.clone(),
        portable: c.portable,
        data_dir: c.data_dir.to_string_lossy().to_string(),
        config_warning: c.warning.clone(),
        write_blocked: c.write_blocked,
        has_config_backup: c.has_valid_backup(),
    })
}

#[tauri::command]
pub fn reload_settings(state: State<'_, AppState>) {
    state.with_cfg_mut(|c| c.reload());
}

#[tauri::command]
pub fn restore_settings_backup(state: State<'_, AppState>) -> Result<()> {
    state.with_cfg_mut(|c| c.restore_backup())
}

#[tauri::command]
pub fn set_downloads_dir(state: State<'_, AppState>, dir: String) -> Result<()> {
    if !Path::new(&dir).is_absolute() {
        return Err(AppError::msg("下载目录必须是绝对路径"));
    }
    state.with_cfg_mut(|c| {
        let old = c.settings.downloads_dir.clone();
        c.settings.downloads_dir = Some(dir);
        if let Err(e) = c.save() {
            c.settings.downloads_dir = old;
            return Err(e);
        }
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
            let name = path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
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
    if name.len() < 16
        || !name.is_ascii()
        || name.as_bytes()[8] != b'T'
        || !name[..8].bytes().all(|b| b.is_ascii_digit())
        || !name[9..15].bytes().all(|b| b.is_ascii_digit())
    {
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
    let _query_guard = state.tool_queries.mutation();
    let _scan_guard = state.scans.mutation();
    let path = PathBuf::from(&file);
    let backups_dir = state.with_cfg(|c| c.backups_dir());
    if !system::path_within(&backups_dir, &path) || !path.extension().is_some_and(|e| e == "json") {
        return Err(AppError::msg("请选择应用备份目录中的 JSON 文件"));
    }
    let content = std::fs::read_to_string(&path)?;
    let entries = parse_path_backup(&content)?;
    pathman::save_user_path(&entries, &backups_dir)
}

fn parse_path_backup(content: &str) -> Result<Vec<String>> {
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Backup {
        user_path: Vec<String>,
    }
    Ok(serde_json::from_str::<Backup>(content)
        .map_err(|e| AppError::msg(format!("PATH 备份格式无效，未修改 PATH: {e}")))?
        .user_path)
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
        (EnvType::Rust, "CARGO_HOME" | "RUSTUP_HOME")
            if system::is_rust_toolchain(&cur) => return None,
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
                (None, Some(r)) if name == "GRADLE_USER_HOME" => Some(
                    r.join("envs")
                        .join("gradle-user-home")
                        .to_string_lossy()
                        .to_string(),
                ),
                _ => None,
            };
            EnvVarInfo {
                name: name.into(),
                label: label.into(),
                value,
                suggested,
            }
        })
        .collect())
}

#[tauri::command]
pub fn save_env_var(state: State<'_, AppState>, name: String, value: String) -> Result<()> {
    let _query_guard = state.tool_queries.mutation();
    let _scan_guard = state.scans.mutation();
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
pub async fn start_package_scan(
    state: State<'_, AppState>,
    mode: String,
    directory: Option<String>,
) -> Result<()> {
    crate::package_scan::start(state.with_cfg(|c| c.resolve_root()), mode, directory).await
}

#[tauri::command]
pub fn package_scan_status() -> Option<crate::package_scan::Snapshot> {
    crate::package_scan::status()
}

#[tauri::command]
pub fn cancel_package_scan() {
    crate::package_scan::cancel();
}

#[tauri::command]
pub async fn list_installed_packages(
    state: State<'_, AppState>,
    tool: crate::package_managers::ManagerId,
    directory: Option<String>,
    historical: Option<bool>,
) -> Result<crate::packages::PackageList> {
    let root = state.with_cfg(|c| c.resolve_root());
    crate::packages::list_scope(
        root.as_deref(),
        tool.as_ref(),
        directory.as_deref(),
        historical.unwrap_or(false),
    )
    .await
}

#[tauri::command]
pub async fn global_package_sources(
    state: State<'_, AppState>,
    tool: crate::package_managers::ManagerId,
) -> Result<Vec<String>> {
    let root = state.with_cfg(|c| c.resolve_root());
    crate::packages::global_sources(root.as_deref(), tool.as_ref()).await
}

#[tauri::command]
pub async fn get_tool_configs(
    state: State<'_, AppState>,
    force: Option<bool>,
) -> Result<Vec<crate::pkgtools::ToolConfig>> {
    let root = state.with_cfg(|c| c.resolve_root());
    Ok(state.tool_configs(root, force.unwrap_or(false)).await)
}

#[tauri::command]
pub async fn apply_tool_config(
    state: State<'_, AppState>,
    tool: String,
    global_path: Option<String>,
    cache_path: Option<String>,
) -> Result<Vec<String>> {
    let _query_guard = state.tool_queries.mutation();
    let _scan_guard = state.scans.mutation();
    let _guard = crate::migration::LOCK
        .try_lock()
        .map_err(|_| AppError::msg("正在迁移或修改包管理器路径，请稍后重试"))?;
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
            let g = crate::package_managers::bin_path(&tool, Path::new(g))
                .to_string_lossy()
                .into_owned();
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
    state: State<'_, AppState>,
    tool: String,
    kind: String,
    source: String,
) -> Result<crate::migration::MigrationPlan> {
    let root = require_root(&state)?;
    tokio::task::spawn_blocking(move || {
        crate::migration::preview(&root, &tool, &kind, Path::new(&source))
    })
    .await
    .map_err(|e| AppError::msg(format!("迁移预览失败: {e}")))?
}

#[tauri::command]
pub async fn migrate_tool_data(
    state: State<'_, AppState>,
    plan: crate::migration::MigrationPlan,
) -> Result<crate::migration::MigrationResult> {
    let _query_guard = state.tool_queries.mutation();
    let _scan_guard = state.scans.mutation();
    let guard = crate::migration::LOCK
        .try_lock()
        .map_err(|_| AppError::msg("正在迁移或修改包管理器路径，请稍后重试"))?;
    let root = require_root(&state)?;
    tokio::task::spawn_blocking(move || {
        // Hold the lock in the worker even if the invoking webview is closed.
        let _guard = guard;
        crate::migration::execute(&root, &plan)
    })
    .await
    .map_err(|e| AppError::msg(format!("迁移任务失败: {e}")))?
}

#[tauri::command]
pub async fn preview_pip_reinstall(
    source: String,
    source_kind: String,
    python: String,
    destination: String,
) -> Result<crate::pip_reinstall::Plan> {
    crate::pip_reinstall::preview(source, source_kind, python, destination).await
}

#[tauri::command]
pub async fn start_pip_reinstall(
    plan: crate::pip_reinstall::Plan,
    names: Vec<String>,
) -> Result<()> {
    crate::pip_reinstall::start(plan, names).await
}

#[tauri::command]
pub fn pip_reinstall_status() -> Option<crate::pip_reinstall::Snapshot> {
    crate::pip_reinstall::status()
}

#[tauri::command]
pub fn cancel_pip_reinstall() {
    crate::pip_reinstall::cancel();
}

#[tauri::command]
pub async fn preview_global_reinstall(
    tool: String,
    executable: String,
    source: String,
    destination: String,
    allow_unknown_registry: Option<bool>,
) -> Result<crate::global_reinstall::Plan> {
    let mut plan = crate::global_reinstall::preview(tool, executable, source, destination).await?;
    crate::global_reinstall::allow_registry_recovery(
        &mut plan,
        allow_unknown_registry.unwrap_or(false),
    );
    Ok(plan)
}
#[tauri::command]
pub async fn start_global_reinstall(
    plan: crate::global_reinstall::Plan,
    names: Vec<String>,
) -> Result<()> {
    crate::global_reinstall::start(plan, names).await
}
#[tauri::command]
pub fn global_reinstall_status(tool: String) -> Option<crate::global_reinstall::Snapshot> {
    crate::global_reinstall::status(&tool)
}
#[tauri::command]
pub fn cancel_global_reinstall(tool: String) {
    crate::global_reinstall::cancel(&tool);
}

#[tauri::command]
pub async fn preview_old_package_cleanup(
    tool: String,
    destination: String,
) -> Result<crate::old_packages::Preview> {
    crate::old_packages::preview(tool, destination).await
}
#[tauri::command]
pub async fn preview_scanned_package_cleanup(
    state: State<'_, AppState>,
    tool: crate::package_managers::ManagerId,
    source: String,
) -> Result<crate::old_packages::Preview> {
    let id = tool.as_ref();
    let (root, reports) =
        state.with_cfg(|c| (c.resolve_root(), c.data_dir.join("cleanup-reports")));
    let executable = if id == "pip" {
        String::new()
    } else {
        crate::pkgtools::find_invocation(root.as_deref(), id)
            .await
            .ok_or_else(|| AppError::msg("未找到对应包管理器，无法卸载旧包"))?
            .program
    };
    crate::old_packages::preview_scanned(
        id.into(),
        source,
        executable,
        reports.to_string_lossy().into(),
    )
    .await
}
#[tauri::command]
pub async fn cleanup_old_packages(
    tool: String,
    token: String,
    names: Vec<String>,
) -> Result<crate::old_packages::CleanupResult> {
    crate::old_packages::execute(tool, token, names).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "Read-only host benchmark; invokes installed managers, run explicitly and serially"]
    async fn host_query_benchmark() {
        use std::sync::atomic::Ordering;
        use std::time::Instant;
        let mut records = Vec::new();
        for sample in 1..=3 {
            for shared in [false, true] {
                crate::pkgtools::QUERY_COMMAND_ATTEMPTS.store(0, Ordering::Relaxed);
                let start = Instant::now();
                let (a, b) = if shared {
                    let flight = crate::singleflight::SingleFlight::default();
                    tokio::join!(
                        flight.read((), false, || crate::pkgtools::get_tool_configs(None)),
                        flight.read((), false, || crate::pkgtools::get_tool_configs(None)),
                    )
                } else {
                    tokio::join!(crate::pkgtools::get_tool_configs(None), crate::pkgtools::get_tool_configs(None))
                };
                assert_eq!(serde_json::to_value(&a).unwrap(), serde_json::to_value(&b).unwrap());
                assert_eq!(a.len(), crate::package_managers::definitions().len());
                records.push(serde_json::json!({"sample":sample,"shared":shared,"elapsedMs":start.elapsed().as_millis(),"commandAttempts":crate::pkgtools::QUERY_COMMAND_ATTEMPTS.load(Ordering::Relaxed),"managers":a.len()}));
            }
        }
        let start = Instant::now();
        for _ in 0..3 { crate::detect::discovery::discover(None, &[]); }
        let full = start.elapsed().as_micros();
        let start = Instant::now();
        for _ in 0..3 { crate::detect::discovery::effective_environment(); }
        println!("BENCHMARK {}", serde_json::json!({"configurationQueries":records,"environmentRead3xMicros":{"previousFullDiscovery":full,"snapshotOnly":start.elapsed().as_micros()},"scope":"root=None; no project directories; command attempts include discovery and probes; not a cold-start or process/memory/IO benchmark"}));
    }

    #[test]
    fn linked_rust_toolchain_preserves_external_files_and_does_not_invent_homes() {
        let d = crate::test_support::TestDir::new();
        let root = d.path().join("managed");
        let toolchain = d.path().join("toolchain");
        std::fs::create_dir_all(toolchain.join("bin")).unwrap();
        std::fs::create_dir_all(toolchain.join("lib/rustlib")).unwrap();
        for exe in ["rustc.exe", "cargo.exe"] { std::fs::write(toolchain.join("bin").join(exe), b"fixture").unwrap(); }
        let link = root.join("envs/rusts/system-stable");
        std::fs::create_dir_all(link.parent().unwrap()).unwrap();
        junction::create(&toolchain, &link).unwrap();
        switcher::switch(&root, EnvType::Rust, "system-stable").unwrap();
        assert!(system::is_rust_toolchain(&root.join("current/rust")));
        assert!(suggested_env_path(&root, EnvType::Rust, "CARGO_HOME").is_none());
        assert!(suggested_env_path(&root, EnvType::Rust, "RUSTUP_HOME").is_none());
        assert!(current_path_entries(&root).iter().any(|p| system::paths_eq(Path::new(p), &root.join("current/rust/bin"))));
        switcher::remove_junction_for(&root, EnvType::Rust).unwrap();
        force_remove_dir_all(&link).unwrap();
        assert!(toolchain.join("bin/rustc.exe").is_file());
    }

    #[test]
    fn environment_delete_preserves_external_junction_targets() {
        let dir = crate::test_support::TestDir::new();
        let external = dir.path().join("external");
        let owned = dir.path().join("owned");
        std::fs::create_dir_all(&external).unwrap();
        std::fs::create_dir_all(&owned).unwrap();
        std::fs::write(external.join("keep.txt"), "keep").unwrap();
        junction::create(&external, owned.join("link")).unwrap();
        force_remove_dir_all(&owned).unwrap();
        assert!(external.join("keep.txt").is_file());
        assert!(!owned.exists());
        std::fs::create_dir_all(dir.path().join("envs")).unwrap();
        junction::create(
            &external,
            dir.path().join("envs").join(EnvType::Node.folder()),
        )
        .unwrap();
        assert!(validate_environment_parent(dir.path(), EnvType::Node).is_err());
        std::fs::remove_dir(dir.path().join("envs").join(EnvType::Node.folder())).unwrap();
        std::fs::create_dir(dir.path().join("envs").join(EnvType::Node.folder())).unwrap();
        assert!(validate_environment_parent(dir.path(), EnvType::Node).is_ok());
        junction::create(&external, dir.path().join("current")).unwrap();
        assert!(validate_environment_parent(dir.path(), EnvType::Node).is_err());
        assert!(external.join("keep.txt").is_file());
    }

    #[test]
    fn invalid_backups_do_not_become_empty_path() {
        for value in [
            "{}",
            r#"{"userPath":null}"#,
            r#"{"userPath":"D:/x"}"#,
            "bad",
        ] {
            assert!(parse_path_backup(value).is_err());
        }
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
            vec![current.join("bin").to_string_lossy().to_string(), current
                .join("cargo-home")
                .join("bin")
                .to_string_lossy()
                .to_string()]
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
