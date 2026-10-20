use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

use crate::app_update;
use crate::config::AppConfig;
use crate::detect::{self, system};
use crate::error::{AppError, Result};
use crate::install::TaskManager;
use crate::pathman;
use crate::sources;
use crate::switcher;
use crate::types::{
    CategoryOverview, EnvironmentManifest, EnvType, ManifestEntry, Overview, UninstallActionItem,
    UninstallMode, UninstallPreview, UninstallResult, UninstallResultStatus, UninstallScopeItem,
    ALL_ENV_TYPES,
};

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

/// One environment, re-read on demand.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentRedetect {
    pub env_type: EnvType,
    pub name: String,
    pub path: String,
    pub version: Option<String>,
    pub status: String,
    pub status_detail: Option<String>,
    pub is_current: bool,
}

/// Re-read a single environment.
///
/// Refreshing the overview re-reads every category, which is the wrong tool
/// after repairing one directory: the user wants to know whether *that* one is
/// healthy now. This reads only the named directory, but through the same row
/// builder and the same current-link observation the category scan uses, so a
/// single re-detect cannot report a state a full scan would contradict. It
/// writes nothing.
#[tauri::command]
pub async fn redetect_environment(
    state: State<'_, AppState>,
    env_type: EnvType,
    name: String,
) -> Result<EnvironmentRedetect> {
    switcher::validate_name(&name)?;
    let root = state
        .with_cfg(|c| c.resolve_root())
        .ok_or_else(|| AppError::msg("尚未设置管理根目录"))?;
    // Reads only the named directory. It shares the row builder with the
    // category scan, so this cannot disagree with a full scan while still
    // avoiding a re-read of every sibling entry.
    let env = detect::read_single_env(&root, env_type, &name)
        .await
        .ok_or_else(|| AppError::msg(format!("此分类下没有名为 {name} 的环境")))?;
    Ok(EnvironmentRedetect {
        env_type: env.env_type,
        name: env.name,
        path: env.path,
        version: env.version,
        status: env.status,
        status_detail: env.status_detail,
        is_current: env.is_current,
    })
}

/// Turn an overview into a manifest. Pure: no filesystem, no clock, so the
/// mapping can be tested without a temporary environment.
fn build_manifest(overview: &Overview, generated_at: String) -> EnvironmentManifest {
    let mut entries: Vec<ManifestEntry> = overview
        .categories
        .iter()
        .flat_map(|category| category.envs.iter())
        .map(|env| ManifestEntry {
            env_type: env.env_type,
            name: env.name.clone(),
            path: env.path.clone(),
            identity_path: env.identity_path.clone(),
            version: env.version.clone(),
            status: env.status.clone(),
            is_current: env.is_current,
            is_link_entry: env.is_link_entry,
            is_external_link: env.is_external_link,
        })
        .collect();
    entries.sort_by(|a, b| {
        let key = |entry: &ManifestEntry| {
            (format!("{:?}", entry.env_type), entry.name.to_lowercase())
        };
        key(a).cmp(&key(b))
    });

    // State the portability limits as facts, not as a soft disclaimer: a reader
    // must be able to tell what this file can and cannot be used for.
    let mut compatibility = vec![
        "路径均为本机绝对路径：换机器后通常无效，不能据此自动创建环境".into(),
        "不含任何文件内容、安装包或缓存：这是清单，不是可还原的备份".into(),
        "链接条目记录的是解析后的目标；目标不存在时身份路径可能仍指向旧位置".into(),
        "外部纳管条目只记录链接与目标，不表示目标受 EnvCon 管理".into(),
    ];
    if overview.portable {
        compatibility.push("当前为便携模式：data_dir 随程序目录移动，换机器后路径关系可能改变".into());
    }
    if !overview.root_exists {
        compatibility.push("当前没有配置管理根目录，清单中的条目为空不代表本机没有安装".into());
    }

    let notes = vec![
        "导入不会自动执行：本清单用于核对现状与人工重建，EnvCon 不提供“一键恢复环境”".into(),
        "版本字段来自版本探测；探测失败时为 null，不代表未安装".into(),
    ];

    EnvironmentManifest {
        schema_version: 1,
        generated_at,
        root: overview.root.clone(),
        portable: overview.portable,
        data_dir: overview.data_dir.clone(),
        entries,
        compatibility,
        notes,
    }
}

/// Reject manifest targets that would be ambiguous or unopenable. Kept pure so
/// the rule is covered by a fixture instead of only reachable through IPC.
fn validate_manifest_target(path: &Path) -> Result<()> {
    if !path.is_absolute() {
        return Err(AppError::msg("导出路径必须是绝对路径"));
    }
    if path.extension().is_none_or(|extension| extension != "json") {
        return Err(AppError::msg("导出文件必须使用 .json 扩展名"));
    }
    if path.parent().is_none() {
        return Err(AppError::msg("导出路径缺少父目录"));
    }
    Ok(())
}

/// Write a read-only manifest to a caller-chosen path. Nothing in the managed
/// root is read beyond what the overview already scanned, and no environment is
/// created, moved or restored.
#[tauri::command]
pub async fn export_environment_manifest(
    state: State<'_, AppState>,
    path: String,
) -> Result<String> {
    let target = PathBuf::from(&path);
    validate_manifest_target(&target)?;
    let overview = get_overview(state).await?;
    let manifest = build_manifest(&overview, crate::update_journal::now());
    let mut bytes = serde_json::to_vec_pretty(&manifest)?;
    bytes.push(b'\n');
    // atomic_write already writes a sibling temp file and replaces on success,
    // so a half-written manifest never appears at the target path.
    crate::config::atomic_write(&target, &bytes)?;
    Ok(target.to_string_lossy().into_owned())
}

/// What a recorded entry looks like compared with what this machine has now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ManifestComparisonKind {
    /// Same path, version and status.
    Unchanged,
    /// Present now, absent from the manifest.
    Added,
    /// In the manifest, absent now.
    Missing,
    /// Present in both, but an observed field differs.
    Changed,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestComparisonRow {
    pub env_type: EnvType,
    pub name: String,
    pub kind: ManifestComparisonKind,
    /// Manifest side. `None` for `Added`.
    pub recorded_version: Option<String>,
    pub recorded_path: Option<String>,
    pub recorded_status: Option<String>,
    /// This machine's side. `None` for `Missing`.
    pub current_version: Option<String>,
    pub current_path: Option<String>,
    pub current_status: Option<String>,
    /// Which fields differ, in a fixed order. Empty unless `Changed`.
    pub differences: Vec<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestComparisonSummary {
    pub unchanged: usize,
    pub added: usize,
    pub missing: usize,
    pub changed: usize,
}

/// Result of reading a manifest back and comparing it with the live state.
/// Read-only by construction: it carries no restore instruction, because a
/// path recorded on another machine cannot be turned into a local environment
/// without inventing where the files should come from.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestComparison {
    pub schema_version: u32,
    pub recorded_generated_at: String,
    pub recorded_portable: bool,
    pub recorded_root: Option<String>,
    pub current_root: Option<String>,
    pub same_root: bool,
    pub rows: Vec<ManifestComparisonRow>,
    pub summary: ManifestComparisonSummary,
    pub limitations: Vec<String>,
}

/// Compare a recorded manifest against the current one. Pure: both sides are
/// already-built manifests, so every rule below is reachable from a fixture.
fn compare_manifests(
    recorded: &EnvironmentManifest,
    current: &EnvironmentManifest,
) -> ManifestComparison {
    use std::collections::BTreeMap;

    let key = |entry: &ManifestEntry| {
        (
            format!("{:?}", entry.env_type),
            entry.name.to_lowercase(),
            entry.name.clone(),
        )
    };
    let recorded_by_key: BTreeMap<_, &ManifestEntry> =
        recorded.entries.iter().map(|entry| (key(entry), entry)).collect();
    let current_by_key: BTreeMap<_, &ManifestEntry> =
        current.entries.iter().map(|entry| (key(entry), entry)).collect();

    let mut all_keys: Vec<_> = recorded_by_key.keys().cloned().collect();
    for candidate in current_by_key.keys() {
        if !recorded_by_key.contains_key(candidate) {
            all_keys.push(candidate.clone());
        }
    }

    let mut rows = Vec::new();
    let mut summary = ManifestComparisonSummary::default();
    for entry_key in all_keys {
        let recorded_entry = recorded_by_key.get(&entry_key).copied();
        let current_entry = current_by_key.get(&entry_key).copied();
        let (Some(recorded_entry), Some(current_entry)) = (recorded_entry, current_entry) else {
            let entry = recorded_entry.or(current_entry).expect("one side exists");
            let kind = if current_entry.is_some() {
                ManifestComparisonKind::Added
            } else {
                ManifestComparisonKind::Missing
            };
            match kind {
                ManifestComparisonKind::Added => summary.added += 1,
                ManifestComparisonKind::Missing => summary.missing += 1,
                _ => {}
            }
            rows.push(ManifestComparisonRow {
                env_type: entry.env_type,
                name: entry.name.clone(),
                kind,
                recorded_version: recorded_entry.and_then(|e| e.version.clone()),
                recorded_path: recorded_entry.map(|e| e.path.clone()),
                recorded_status: recorded_entry.map(|e| e.status.clone()),
                current_version: current_entry.and_then(|e| e.version.clone()),
                current_path: current_entry.map(|e| e.path.clone()),
                current_status: current_entry.map(|e| e.status.clone()),
                differences: Vec::new(),
            });
            continue;
        };

        // Windows paths are case-insensitive and may carry either separator, so
        // comparing the raw strings would report phantom differences.
        let mut differences = Vec::new();
        if crate::detect::system::path_key(Path::new(&recorded_entry.path))
            != crate::detect::system::path_key(Path::new(&current_entry.path))
        {
            differences.push("路径".to_string());
        }
        // Only compare versions when the manifest actually recorded one: a null
        // version means "not probed then", which is not evidence of a change.
        if let (Some(recorded_version), Some(current_version)) =
            (&recorded_entry.version, &current_entry.version)
        {
            if recorded_version != current_version {
                differences.push("版本".to_string());
            }
        }
        if recorded_entry.status != current_entry.status {
            differences.push("状态".to_string());
        }

        let kind = if differences.is_empty() {
            summary.unchanged += 1;
            ManifestComparisonKind::Unchanged
        } else {
            summary.changed += 1;
            ManifestComparisonKind::Changed
        };
        rows.push(ManifestComparisonRow {
            env_type: recorded_entry.env_type,
            name: recorded_entry.name.clone(),
            kind,
            recorded_version: recorded_entry.version.clone(),
            recorded_path: Some(recorded_entry.path.clone()),
            recorded_status: Some(recorded_entry.status.clone()),
            current_version: current_entry.version.clone(),
            current_path: Some(current_entry.path.clone()),
            current_status: Some(current_entry.status.clone()),
            differences,
        });
    }

    let same_root = recorded.root.is_some() && recorded.root == current.root;
    let mut limitations = vec![
        "比对按（环境类型 + 名称）配对：同名但实际不同的两个目录会被当作同一条目".into(),
        "路径比较只在本机做大小写与分隔符归一化；清单来自另一台机器时路径不同属正常，不代表环境被改动".into(),
        "清单中版本为 null 表示当时未探测成功，这种条目不会因版本被判为已变更".into(),
        "本功能只读：不会创建、移动、切换或删除任何环境".into(),
    ];
    if !same_root {
        limitations.push(
            "清单记录的根目录与本机不同或缺失：这是跨机器比对，行结果只说明差异，不能据此恢复环境"
                .into(),
        );
    }

    ManifestComparison {
        schema_version: recorded.schema_version,
        recorded_generated_at: recorded.generated_at.clone(),
        recorded_portable: recorded.portable,
        recorded_root: recorded.root.clone(),
        current_root: current.root.clone(),
        same_root,
        rows,
        summary,
        limitations,
    }
}

/// Reject manifest sources that are not a readable JSON file.
fn validate_manifest_source(path: &Path) -> Result<()> {
    if !path.is_absolute() {
        return Err(AppError::msg("清单路径必须是绝对路径"));
    }
    if path.extension().is_none_or(|extension| extension != "json") {
        return Err(AppError::msg("清单文件必须使用 .json 扩展名"));
    }
    if !path.is_file() {
        return Err(AppError::msg(format!(
            "清单文件不存在: {}",
            path.display()
        )));
    }
    Ok(())
}

/// Read a previously exported manifest and compare it with what is here now.
/// Never restores anything: the caller gets a diff it can act on manually.
#[tauri::command]
pub async fn import_environment_manifest(
    state: State<'_, AppState>,
    path: String,
) -> Result<ManifestComparison> {
    let source = PathBuf::from(&path);
    validate_manifest_source(&source)?;
    let bytes = std::fs::read(&source)?;
    let recorded: EnvironmentManifest = serde_json::from_slice(&bytes)
        .map_err(|error| AppError::msg(format!("清单格式无法解析: {error}")))?;
    if recorded.schema_version != 1 {
        return Err(AppError::msg(format!(
            "不支持的清单版本 {}（本程序支持 1）",
            recorded.schema_version
        )));
    }
    let overview = get_overview(state).await?;
    let current = build_manifest(&overview, crate::update_journal::now());
    Ok(compare_manifests(&recorded, &current))
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageManagerUpdateResult {
    status: PackageManagerUpdateStatus,
    tool: String,
    executable_path: String,
    before_version: Option<String>,
    after_version: Option<String>,
    version_changed: Option<bool>,
    output: String,
    error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
enum PackageManagerUpdateStatus {
    CommandFailed,
    VerificationFailed,
    VersionChanged,
    VersionUnchanged,
    VersionUnknown,
}

/// A read-only description of what an update would do. Nothing here executes
/// the update or writes to disk, so the user can see the exact program,
/// arguments and installation that would be touched beforehand.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageManagerUpdatePreview {
    tool: String,
    /// Why this manager cannot be updated, when it cannot.
    blocked: Option<String>,
    executable_path: Option<String>,
    current_version: Option<String>,
    update_program: Option<String>,
    update_arguments: Vec<String>,
    /// `true` when the updater is the very file being replaced, `false` when
    /// the update runs through another program (for example the owning
    /// interpreter), `None` when no command could be derived.
    runs_same_file: Option<bool>,
    /// What this entry point can and cannot do, as structured facts rather than
    /// prose. Present even when the tool is blocked, because the limits are
    /// about the entry point, not about this particular attempt.
    capabilities: crate::pkgtools::UpdateCapabilities,
    notes: Vec<String>,
}

/// Every manager's preview plus the split the UI needs to offer a batch run
/// without re-deriving the decision from strings.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageManagerUpdateBatchPreview {
    items: Vec<PackageManagerUpdatePreview>,
    /// Tools whose preview produced no blocker.
    ready: Vec<String>,
    /// Tools that cannot be updated, kept with their reason in `items`.
    blocked: Vec<String>,
}

/// One entry of a batch run. `Skipped` is a decision made before execution
/// (no updater, unsupported manager, or excluded by the caller), which is not
/// a failure and must not be counted as one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
enum PackageManagerBatchOutcome {
    Updated,
    Unchanged,
    Unverified,
    Failed,
    Skipped,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageManagerBatchItem {
    tool: String,
    outcome: PackageManagerBatchOutcome,
    /// Present when the tool ran; absent for skipped entries.
    result: Option<PackageManagerUpdateResult>,
    /// Why the tool was skipped. Never set for tools that actually ran.
    skipped_reason: Option<String>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageManagerBatchResult {
    items: Vec<PackageManagerBatchItem>,
    /// Tools that failed and may be retried. Derived from `items` so the UI
    /// cannot drift from what actually happened.
    retryable: Vec<String>,
}

/// Decide the batch outcome from a single update result. Kept separate from
/// execution so the mapping is covered by a fixture.
fn batch_outcome_for(status: PackageManagerUpdateStatus) -> PackageManagerBatchOutcome {
    match status {
        PackageManagerUpdateStatus::VersionChanged => PackageManagerBatchOutcome::Updated,
        PackageManagerUpdateStatus::VersionUnchanged => PackageManagerBatchOutcome::Unchanged,
        PackageManagerUpdateStatus::VerificationFailed
        | PackageManagerUpdateStatus::VersionUnknown => PackageManagerBatchOutcome::Unverified,
        PackageManagerUpdateStatus::CommandFailed => PackageManagerBatchOutcome::Failed,
    }
}

/// Build the batch response from per-tool outcomes. Tools are never reordered,
/// so the caller can line the report up with the request it sent.
fn package_manager_batch_result(items: Vec<PackageManagerBatchItem>) -> PackageManagerBatchResult {
    let retryable = items
        .iter()
        .filter(|item| item.outcome == PackageManagerBatchOutcome::Failed)
        .map(|item| item.tool.clone())
        .collect();
    PackageManagerBatchResult { items, retryable }
}

/// Reasons that stop an update before any command runs. The preview and the
/// real update both call this, so the preview can never promise an update the
/// executor would refuse.
fn package_manager_update_blocker(tool: &str, version_output: &str) -> Option<String> {
    if tool == "yarn" && !version_output.starts_with("1.") {
        return Some("Yarn Berry 请在所属项目中使用 yarn set version stable 更新".into());
    }
    None
}

/// Build the preview from already-resolved parts. Kept free of process and
/// filesystem access so the decision rules can be tested in isolation.
fn package_manager_update_preview(
    tool: &str,
    executable_path: Option<String>,
    current_version: Option<String>,
    command: std::result::Result<(String, Vec<String>), String>,
    runs_same_file: Option<bool>,
) -> PackageManagerUpdatePreview {
    // The limits belong to the entry point, so they are reported even when this
    // particular attempt is blocked.
    let capabilities = crate::pkgtools::update_capabilities(tool);
    match command {
        Err(error) => PackageManagerUpdatePreview {
            tool: tool.into(),
            blocked: Some(error),
            executable_path,
            current_version,
            update_program: None,
            update_arguments: Vec::new(),
            runs_same_file: None,
            capabilities,
            notes: vec!["没有生成更新命令，也没有执行任何写入".into()],
        },
        Ok((program, arguments)) => {
            let mut notes = vec![
                "命令执行成功不代表版本一定变化；执行后仍会复查版本".into(),
                "无法在更新前得知目标版本：这些管理器不提供远程最新版查询".into(),
            ];
            if runs_same_file == Some(false) {
                notes.push("更新通过所属解释器执行，而不是直接替换命令文件".into());
            }
            if arguments.iter().any(|argument| argument == "--prefix") {
                notes.push("更新被固定到当前安装目录，不会写到其他前缀".into());
            }
            PackageManagerUpdatePreview {
                tool: tool.into(),
                blocked: None,
                executable_path,
                current_version,
                update_program: Some(program),
                update_arguments: arguments,
                runs_same_file,
                capabilities,
                notes,
            }
        }
    }
}

fn package_manager_update_result(
    tool: &str,
    executable_path: String,
    before: &str,
    command: std::result::Result<String, String>,
    verification: std::result::Result<String, String>,
) -> PackageManagerUpdateResult {
    let before_version = crate::pkgtools::display_version(before);
    match command {
        Err(error) => PackageManagerUpdateResult {
            status: PackageManagerUpdateStatus::CommandFailed,
            tool: tool.into(),
            executable_path,
            before_version,
            after_version: None,
            version_changed: None,
            output: String::new(),
            error: Some(error),
        },
        Ok(output) => match verification {
            Err(error) => PackageManagerUpdateResult {
                status: PackageManagerUpdateStatus::VerificationFailed,
                tool: tool.into(),
                executable_path,
                before_version,
                after_version: None,
                version_changed: None,
                output,
                error: Some(error),
            },
            Ok(after) => {
                let after_version = crate::pkgtools::display_version(&after);
                let version_changed = before_version
                    .as_ref()
                    .zip(after_version.as_ref())
                    .map(|(before, after)| before != after);
                let status = match version_changed {
                    Some(true) => PackageManagerUpdateStatus::VersionChanged,
                    Some(false) => PackageManagerUpdateStatus::VersionUnchanged,
                    None => PackageManagerUpdateStatus::VersionUnknown,
                };
                PackageManagerUpdateResult {
                    status,
                    tool: tool.into(),
                    executable_path,
                    before_version,
                    after_version,
                    version_changed,
                    output,
                    error: None,
                }
            }
        },
    }
}

#[tauri::command]
pub async fn preview_package_manager_update(
    state: State<'_, AppState>,
    tool: crate::package_managers::ManagerId,
) -> Result<PackageManagerUpdatePreview> {
    preview_package_manager_update_inner(&state, tool.as_ref()).await
}

/// Read-only preview for every catalog manager, so a batch can be reviewed
/// before anything runs. Managers that cannot be updated (missing command,
/// unsupported self-update, Yarn Berry) are returned with their reason instead
/// of being silently dropped from the list.
#[tauri::command]
pub async fn preview_package_manager_updates(
    state: State<'_, AppState>,
) -> Result<PackageManagerUpdateBatchPreview> {
    let mut items = Vec::new();
    for definition in crate::package_managers::definitions() {
        items.push(preview_package_manager_update_inner(&state, &definition.id).await?);
    }
    let ready = items
        .iter()
        .filter(|item| item.blocked.is_none())
        .map(|item| item.tool.clone())
        .collect();
    let blocked = items
        .iter()
        .filter(|item| item.blocked.is_some())
        .map(|item| item.tool.clone())
        .collect();
    Ok(PackageManagerUpdateBatchPreview {
        items,
        ready,
        blocked,
    })
}

async fn preview_package_manager_update_inner(
    state: &AppState,
    id: &str,
) -> Result<PackageManagerUpdatePreview> {
    let root = state.with_cfg(|c| c.resolve_root());
    let inv = match crate::pkgtools::find_invocation(root.as_deref(), id).await {
        Some(inv) => inv,
        None => {
            return Ok(package_manager_update_preview(
                id,
                None,
                None,
                Err(format!("未找到 {id} 可执行命令")),
                None,
            ))
        }
    };
    let executable_path = Some(inv.program.clone());
    // Reading the version is the one process the preview runs: it is
    // read-only and the result is needed to decide whether the update is
    // allowed at all (for example Yarn Berry).
    let version = crate::pkgtools::run(&inv, &["--version"], 15).await;
    let (current_version, version_output) = match &version {
        Ok(output) => (crate::pkgtools::display_version(output), output.clone()),
        Err(_) => (None, String::new()),
    };
    if let Some(blocked) = package_manager_update_blocker(id, &version_output) {
        return Ok(package_manager_update_preview(
            id,
            executable_path,
            current_version,
            Err(blocked),
            None,
        ));
    }
    if version.is_err() {
        return Ok(package_manager_update_preview(
            id,
            executable_path,
            current_version,
            Err("无法读取当前版本，未生成更新预览".into()),
            None,
        ));
    }
    let command = crate::pkgtools::self_update_command(id, &inv)
        .map(|(updater, arguments)| (updater.program, arguments))
        .map_err(|error| error.to_string());
    // `runs_same_file` must compare the exact updater target with the command
    // that was resolved, not the manager name: pip updates through its own
    // interpreter and must not be advertised as a self-replacement.
    let runs_same_file = command
        .as_ref()
        .ok()
        .map(|(program, _)| program.eq_ignore_ascii_case(&inv.program));
    Ok(package_manager_update_preview(
        id,
        executable_path,
        current_version,
        command,
        runs_same_file,
    ))
}

/// Read-only view of the most recent recorded update attempt for a tool.
/// Returns `None` when nothing was ever recorded, rather than inventing an
/// empty record.
#[tauri::command]
pub async fn package_manager_update_record(
    state: State<'_, AppState>,
    tool: crate::package_managers::ManagerId,
) -> Result<Option<crate::update_journal::UpdateRecord>> {
    let backups = state.with_cfg(|c| c.backups_dir());
    crate::update_journal::latest(&backups, tool.as_ref())
}

#[tauri::command]
pub async fn update_package_manager(
    state: State<'_, AppState>,
    tool: crate::package_managers::ManagerId,
) -> Result<PackageManagerUpdateResult> {
    // The guards must stay bound for the whole update: an unbound return value
    // would be dropped immediately and silently release every lock.
    let _guards = guard_update_exclusive(&state)?;
    update_package_manager_inner(&state, tool.as_ref()).await
}

/// The exclusive guards shared by the single and batch updaters. Taking them
/// once per batch is deliberate: each tool would otherwise contend for the same
/// lock and fail spuriously halfway through.
struct UpdateGuards<'a> {
    /// Invalidates the tool-config cache on drop, so the versions shown after
    /// the run cannot be a pre-update snapshot.
    _queries: crate::singleflight::Mutation<
        'a,
        Option<PathBuf>,
        Vec<crate::pkgtools::ToolConfig>,
    >,
    _scans: crate::scan_cache::Mutation<'a>,
    _lock: tokio::sync::MutexGuard<'static, ()>,
}

fn guard_update_exclusive(state: &AppState) -> Result<UpdateGuards<'_>> {
    let queries = state.tool_queries.mutation();
    let scans = state.scans.mutation();
    let lock = crate::migration::LOCK
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
    Ok(UpdateGuards {
        _queries: queries,
        _scans: scans,
        _lock: lock,
    })
}

/// Update one manager. The caller must already hold the exclusive guards;
/// keeping them out of here lets a batch hold them once for the whole run.
async fn update_package_manager_inner(
    state: &AppState,
    id: &str,
) -> Result<PackageManagerUpdateResult> {
    let inv = crate::pkgtools::find_invocation(state.with_cfg(|c| c.resolve_root()).as_deref(), id)
        .await
        .ok_or_else(|| AppError::msg(format!("未找到 {id} 可执行命令")))?;
    let before = crate::pkgtools::run(&inv, &["--version"], 15)
        .await
        .map_err(AppError::msg)?;
    if let Some(blocked) = package_manager_update_blocker(id, &before) {
        return Err(AppError::msg(blocked));
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
    // Record the plan *before* the updater runs. A crash or a hung process
    // still leaves the exact command and starting version on disk. If the
    // record cannot be written, the update must not run: this file is the only
    // durable evidence of what was about to change.
    let backups = state.with_cfg(|c| c.backups_dir());
    let attempt = crate::update_journal::now();
    crate::update_journal::record_planned(
        &backups,
        &attempt,
        id,
        &inv.program,
        crate::pkgtools::display_version(&before).as_deref(),
        &updater.program,
        &args.iter().map(|arg| (*arg).to_string()).collect::<Vec<_>>(),
    )
    .map_err(|error| AppError::msg(format!("无法保存更新清单，已取消更新：{error}")))?;

    let command = crate::pkgtools::mutate(&updater, &args, 600).await;
    let verification = if command.is_ok() {
        crate::pkgtools::run(&inv, &["--version"], 15).await
    } else {
        Err("未执行更新后版本复查".into())
    };
    let result = package_manager_update_result(
        id,
        inv.program,
        &before,
        command,
        verification,
    );
    // `unverified` is its own outcome: the command may have run but the new
    // version could not be read, which is neither success nor failure.
    let outcome = match result.status {
        PackageManagerUpdateStatus::VersionChanged
        | PackageManagerUpdateStatus::VersionUnchanged => "completed",
        PackageManagerUpdateStatus::VerificationFailed
        | PackageManagerUpdateStatus::VersionUnknown => "unverified",
        PackageManagerUpdateStatus::CommandFailed => "failed",
    };
    // A failure to settle the record must not hide the update result from the
    // caller, but it is reported so the audit trail is never silently stale.
    let mut result = result;
    if let Err(error) = crate::update_journal::record_outcome(
        &backups,
        &attempt,
        outcome,
        result.after_version.as_deref(),
    ) {
        if result.error.is_none() {
            result.error = Some(format!("更新清单未能写入结果：{error}"));
        }
    }
    Ok(result)
}

/// Run the update for several managers under one set of exclusive guards.
///
/// A refusal *before* any command runs (no executable, unsupported manager,
/// Yarn Berry, unverifiable pip interpreter) is reported as `skipped` with its
/// reason: it is a decision, not an outcome, and must not be counted as a
/// failure or offered for retry. Only `commandFailed` becomes `failed`, which
/// is what `retryable` is derived from.
#[tauri::command]
pub async fn update_package_managers(
    state: State<'_, AppState>,
    tools: Vec<crate::package_managers::ManagerId>,
    skip: Option<Vec<crate::package_managers::ManagerId>>,
) -> Result<PackageManagerBatchResult> {
    if tools.is_empty() {
        return Err(AppError::msg("没有选择要更新的包管理器"));
    }
    let _guards = guard_update_exclusive(&state)?;
    let skipped: std::collections::HashSet<String> = skip
        .unwrap_or_default()
        .into_iter()
        .map(|tool| tool.as_ref().to_string())
        .collect();
    let mut items = Vec::with_capacity(tools.len());
    for tool in &tools {
        let id = tool.as_ref();
        if skipped.contains(id) {
            items.push(PackageManagerBatchItem {
                tool: id.into(),
                outcome: PackageManagerBatchOutcome::Skipped,
                result: None,
                skipped_reason: Some("本次运行中由用户跳过".into()),
            });
            continue;
        }
        match update_package_manager_inner(&state, id).await {
            Ok(result) => {
                let outcome = batch_outcome_for(result.status);
                items.push(PackageManagerBatchItem {
                    tool: id.into(),
                    outcome,
                    result: Some(result),
                    skipped_reason: None,
                });
            }
            Err(error) => items.push(PackageManagerBatchItem {
                tool: id.into(),
                outcome: PackageManagerBatchOutcome::Skipped,
                result: None,
                skipped_reason: Some(error.to_string()),
            }),
        }
    }
    Ok(package_manager_batch_result(items))
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
    crate::link_ownership::create(&root, &link, &install_root)
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
) -> Result<SwitchDiagnosis> {
    let _query_guard = state.tool_queries.mutation();
    let _scan_guard = state.scans.mutation();
    let _guard = crate::migration::LOCK
        .try_lock()
        .map_err(|_| AppError::msg("包管理器任务正在执行，请结束后切换运行时"))?;
    reject_active_install(&state, env_type, &name)?;
    let root = require_root(&state)?;
    switcher::switch(&root, env_type, &name)?;
    Ok(diagnose_switch(&root, env_type, &name).await)
}

/// One command a new terminal should resolve after switching.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SwitchCommandDiagnosis {
    pub command: String,
    /// `managed`, `shadowed`, or `missing`.
    pub status: String,
    pub expected: String,
    pub resolved: Option<String>,
    pub detail: String,
}

/// Result of applying a repair, recomputed from the live PATH rather than the
/// intended plan.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PathRepairResult {
    pub applied: crate::path_ownership::ReorderPlan,
    /// True only when the live PATH matches the plan and no command is shadowed.
    pub verified: bool,
    pub user_path: String,
    pub commands: Vec<SwitchCommandDiagnosis>,
    /// Concrete mismatches; empty means the repair took effect.
    pub problems: Vec<String>,
    /// The PATH the plan was bound to, for an audit trail.
    pub requested_path: String,
    /// A running terminal keeps the old environment.
    pub requires_new_terminal: bool,
}

/// Read-only repair preview for a switch whose command is shadowed.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PathRepairPreview {
    pub env_type: String,
    /// Managed entry that must win.
    pub entry: String,
    /// Entries that currently shadow it; they move after it.
    pub shadowing: Vec<String>,
    /// `reorder` when only ordering changes, `manual` when the user must act.
    pub mode: String,
    /// Exact current user PATH the plan was built from.
    pub expected_path: String,
    pub resulting_path: String,
    /// One-line description of what will change.
    pub summary: String,
    pub notes: Vec<String>,
    /// Reasons the repair cannot be applied automatically.
    pub blocked: Vec<String>,
    /// Bound plan used to verify the write; opaque to the UI.
    pub plan: Option<crate::path_ownership::ReorderPlan>,
    /// Set when only manual cleanup is possible.
    pub manual_entries: Vec<String>,
}

/// Read-only result of a switch: what the new terminal should resolve, and
/// which environment effects still apply. It never executes an installed tool.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SwitchDiagnosis {
    pub env_type: String,
    pub name: String,
    pub current_target: Option<String>,
    pub version: Option<String>,
    pub commands: Vec<SwitchCommandDiagnosis>,
    /// `active`, `requires-new-terminal`, `not-in-path`, `shadowed`, or `unknown`.
    pub path_status: String,
    pub path_detail: String,
    pub retained: Vec<String>,
    pub notes: Vec<String>,
}

/// Commands a terminal would run for each environment type. Only names are
/// listed; resolution is read-only and never starts them.
fn switch_commands(env_type: EnvType) -> &'static [&'static str] {
    match env_type {
        EnvType::Jdk => &["java", "javac"],
        EnvType::Python => &["python", "pip"],
        EnvType::Node => &["node", "npm"],
        EnvType::Go => &["go"],
        EnvType::Rust => &["rustc", "cargo"],
        EnvType::Maven => &["mvn"],
        EnvType::Gradle => &["gradle"],
        EnvType::Php => &["php"],
        EnvType::Llvm => &["clang"],
        EnvType::Zig => &["zig"],
        EnvType::Deno => &["deno"],
        EnvType::Bun => &["bun"],
        EnvType::Git => &["git"],
        EnvType::Gh => &["gh"],
        EnvType::Mingw => &["gcc"],
        EnvType::Adb => &["adb"],
    }
}

/// Diagnose a completed switch without mutating anything. The version comes
/// from probing the current junction target, and PATH resolution reuses the
/// same read-only merge used by the package-manager preview.
async fn diagnose_switch(root: &Path, env_type: EnvType, name: &str) -> SwitchDiagnosis {
    let current = root.join("current").join(env_type.junction());
    let target = std::fs::read_link(&current)
        .ok()
        .map(|path| path.to_string_lossy().into_owned());
    let env_dir = root.join("envs").join(env_type.folder()).join(name);
    let version = crate::detect::probe::probe_version(env_type, &env_dir).await;

    let entries = crate::pkgtools::merged_path_entries_for_diagnostics();
    let expected_entries = env_type
        .path_entries(root)
        .into_iter()
        .map(|path| path.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    let extensions = crate::pkgtools::command_extensions_for_diagnostics();

    let mut commands = Vec::new();
    for command in switch_commands(env_type) {
        // Resolve against the entry directory that should provide this command.
        let expected = expected_entries
            .iter()
            .map(Path::new)
            .find(|entry| {
                crate::pkgtools::launcher_exists(entry, command, &extensions)
            })
            .or_else(|| expected_entries.first().map(Path::new));
        let Some(expected) = expected else {
            continue;
        };
        let resolution = crate::pkgtools::resolve_command_for_diagnostics(
            &entries,
            expected,
            command,
            &extensions,
        );
        commands.push(SwitchCommandDiagnosis {
            command: (*command).into(),
            status: resolution.status.into(),
            expected: expected.to_string_lossy().into_owned(),
            resolved: resolution.resolved,
            detail: resolution.detail,
        });
    }

    let joined = entries.join(";");
    let expected_entry = expected_entries.first().cloned().unwrap_or_default();
    let membership = crate::pkgtools::path_integration_status_from_entries(
        &joined,
        Path::new(&expected_entry),
        None,
    );
    let path_status = if expected_entry.is_empty() {
        "unknown".to_string()
    } else if !membership.new_terminal_contains {
        "not-in-path".to_string()
    } else if commands.iter().any(|item| item.status == "shadowed") {
        "shadowed".to_string()
    } else {
        "requires-new-terminal".to_string()
    };
    let path_detail = membership.detail;

    let mut retained = Vec::new();
    if commands.iter().any(|item| item.status == "shadowed") {
        retained.push(
            "已有终端与 PATH 靠前位置仍可能命中旧版本；请在新终端核对，必要时自行清理旧入口。"
                .to_string(),
        );
    }
    if commands.iter().any(|item| item.status == "missing") {
        retained.push(
            "当前 PATH 中未找到该命令；入口目录可能尚未写入用户/系统 PATH。".to_string(),
        );
    }
    retained.push("其他已安装版本不受影响；本诊断不修改 PATH、配置或文件。".to_string());

    let mut notes = vec![
        "切换只改变 current 链接；已打开终端继承的是旧环境，需要新终端才会读取新 PATH".to_string(),
        "诊断不执行被切换环境的命令，只读取链接目标和已解析路径".to_string(),
    ];
    if crate::detect::system::has_reparse_component(&root.join("current")) {
        notes.push("current 的祖先目录包含链接，诊断结果可能受重定向影响".to_string());
    }

    SwitchDiagnosis {
        env_type: format!("{env_type:?}"),
        name: name.into(),
        current_target: target,
        version,
        commands,
        path_status,
        path_detail,
        retained,
        notes,
    }
}

#[tauri::command]
pub fn preview_uninstall_env(
    state: State<'_, AppState>,
    env_type: EnvType,
    name: String,
    mode: Option<UninstallMode>,
) -> Result<UninstallPreview> {
    let mode = mode.unwrap_or_default();
    switcher::validate_name(&name)?;
    let _guard = crate::migration::LOCK
        .try_lock()
        .map_err(|_| AppError::msg("包管理器任务正在执行，请稍后预览卸载范围"))?;
    reject_active_install(&state, env_type, &name)?;
    let root = require_root(&state)?;
    let backups = state.with_cfg(|c| c.backups_dir());
    let mut store = pathman::RegistryPathStore::open(&backups);
    let (preview, _) = preview_uninstall_with_path_mode(
        &root, env_type, &name, mode, &backups.join("ownership/journal.json"), &mut store,
    )?;
    // An original uninstaller deletes exactly what Detach promises to keep, so
    // the preview must not advertise it. Saying so here keeps the refusal
    // visible before the user confirms, instead of surfacing at execution.
    if mode == UninstallMode::Detach {
        let mut preview = preview;
        if matches!(env_type, EnvType::Python | EnvType::Llvm) && !preview.is_link_entry {
            preview.unverified.push(UninstallScopeItem {
                resource: "原厂卸载器".into(),
                path: None,
                count: None,
                details: "停用模式不运行原厂卸载器：原厂卸载会删除安装内容，与“保留目录”矛盾。若要彻底卸载，请改用卸载模式。".into(),
            });
        }
        return Ok(preview);
    }
    if matches!(env_type, EnvType::Python | EnvType::Llvm) && !preview.is_link_entry {
        attach_installer_preview(&root, preview, crate::installer_uninstall::read_registry())
    } else { Ok(preview) }
}

#[tauri::command]
pub async fn uninstall_env(
    state: State<'_, AppState>,
    env_type: EnvType,
    name: String,
    expected_preview: UninstallPreview,
) -> Result<UninstallResult> {
    let _query_guard = state.tool_queries.mutation();
    let _scan_guard = state.scans.mutation();
    switcher::validate_name(&name)?;
    let _guard = crate::migration::LOCK
        .try_lock()
        .map_err(|_| AppError::msg("包管理器任务正在执行，请结束后卸载运行时"))?;
    reject_active_install(&state, env_type, &name)?;
    let root = require_root(&state)?;
    let backups = state.with_cfg(|c| c.backups_dir());
    let mut store = pathman::RegistryPathStore::open(&backups);
    // Re-derive with the requested mode. An Uninstall-shaped re-derivation
    // would never match a Detach submission and every Detach would look stale.
    let mode = expected_preview.mode;
    let (mut base_preview, _) = preview_uninstall_with_path_mode(
        &root, env_type, &name, mode, &backups.join("ownership/journal.json"), &mut store,
    )?;
    let checked = if mode == UninstallMode::Detach {
        // Detach never runs an original uninstaller, so there is no installer
        // plan to attach; the preview already states why.
        base_preview.clone()
    } else if matches!(env_type, EnvType::Python | EnvType::Llvm) && !base_preview.is_link_entry {
        attach_installer_preview(&root, base_preview.clone(), crate::installer_uninstall::read_registry())?
    } else { base_preview.clone() };
    validate_uninstall_execution_preview(&expected_preview, &checked)?;
    base_preview.confirmed_legacy_links = expected_preview.confirmed_legacy_links;
    base_preview.confirmed_legacy_path_entries = expected_preview.confirmed_legacy_path_entries;
    let installer_plan = checked.installer_plan.clone().filter(|plan| plan.state == "ready");
    // Detach promises that every byte stays on disk. An original uninstaller
    // does the opposite, so the two must never be combined silently: refuse and
    // let the user pick which promise they actually want.
    if installer_plan.is_some() && base_preview.mode == UninstallMode::Detach {
        return Err(AppError::msg(
            "停用模式不运行原厂卸载器：原厂卸载会删除安装内容，与“保留目录”矛盾。若要彻底卸载，请改用卸载模式",
        ));
    }
    let mut result = if let Some(plan) = installer_plan {
        let journal = backups.join("ownership/journal.json");
        execute_installer_uninstall_with_path(
            InstallerUninstallTarget { root, env_type, name, journal: &journal },
            base_preview, plan, crate::installer_uninstall::WindowsBackend, &mut store,
        ).await?
    } else {
        execute_uninstall_with_path(
            root, env_type, name, base_preview, &backups.join("ownership/journal.json"), &mut store,
        ).await?
    };
    if checked.installer_plan.is_some() {
        result.unverified.retain(|item| item.resource != "安装器登记与目录外副作用");
        result.unverified.extend(checked.unverified.into_iter().filter(|item|
            matches!(item.resource.as_str(), "卸载登记核对" | "卸载登记读取限制" | "安装器目录外副作用")
        ));
    }
    // A failed write/recheck can still have changed PATH; notify on attempted cleanup too.
    if result.removed.iter().chain(&result.failed).any(|item|
        matches!(item.resource.as_str(), "EnvCon 用户 PATH 条目" | "用户确认移除的历史 PATH 条目")
    ) {
        pathman::broadcast_change();
    }
    Ok(result)
}

const OWNED_PATH_RESOURCE: &str = "EnvCon 用户 PATH 条目";

/// `Detach` promises that every byte stays where it is. Directory and link
/// entries therefore move from `removable` to `retained`; only the current
/// junction and EnvCon-owned PATH entries stay removable, because leaving them
/// would also leave the environment active — which is not "stopped".
fn apply_detach_mode(preview: &mut UninstallPreview) {
    if preview.mode != UninstallMode::Detach {
        return;
    }
    let keeps_running = |resource: &str| {
        resource == "当前版本 Junction" || resource == OWNED_PATH_RESOURCE
    };
    let kept: Vec<_> = preview
        .removable
        .iter()
        .filter(|item| !keeps_running(&item.resource))
        .cloned()
        .collect();
    preview.removable.retain(|item| keeps_running(&item.resource));
    for mut item in kept {
        item.details = format!("停用模式：保留磁盘内容，不删除。原说明：{}", item.details);
        preview.retained.push(item);
    }
}

/// Set the mode and shape the preview accordingly. Used everywhere a preview is
/// produced or re-derived, so a Detach request is never compared against an
/// Uninstall-shaped observation.
fn shape_preview(preview: &mut UninstallPreview, mode: UninstallMode) {
    preview.mode = mode;
    apply_detach_mode(preview);
}

/// Read-only view of the Python per-user install location (`--user` /
/// `PYTHONUSERBASE`). It is never removed with an environment because another
/// interpreter may share it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct PythonUserScope {
    /// Absolute per-user site-packages directory, when it can be resolved.
    pub path: Option<String>,
    /// Number of distribution entries found; `None` when nothing was read.
    pub packages: Option<usize>,
    /// Why the directory is retained, including any read limitation.
    pub reason: String,
    /// True when more than one interpreter could use this directory.
    pub shared: bool,
}

impl PythonUserScope {
    fn package_count(&self) -> Option<u64> {
        self.packages.map(|count| count as u64)
    }

    fn retained_detail(&self) -> String {
        let base = "不属于所选受管目录，默认保留；本预览只读列举，不删除任何用户级包。";
        match self.packages {
            Some(count) if count > 0 => format!(
                "{base}发现 {count} 个用户级分发包{}；只列出数量，不推断其归属。",
                if self.shared {
                    "，且该位置可能被多个 Python 版本共用"
                } else {
                    ""
                }
            ),
            Some(_) => format!("{base}用户级目录存在但未发现分发包。"),
            None => format!("{base}{}", self.reason),
        }
    }
}

/// Resolve `PYTHONUSERBASE` or the Windows default. A relative or unreadable
/// value is reported instead of guessed.
fn python_user_base_from(
    configured: Option<String>,
    appdata: Option<std::ffi::OsString>,
) -> std::result::Result<PathBuf, String> {
    let configured = configured.filter(|value| !value.trim().is_empty());
    match configured {
        Some(value) => {
            let path = PathBuf::from(&value);
            if !path.is_absolute() {
                return Err(format!("PYTHONUSERBASE 不是绝对路径：{value}"));
            }
            Ok(path)
        }
        None => appdata
            .map(|appdata| PathBuf::from(appdata).join("Python"))
            .ok_or_else(|| "无法确定用户数据目录（APPDATA 不可用）".into()),
    }
}

fn python_user_base() -> std::result::Result<PathBuf, String> {
    let configured = std::env::var("PYTHONUSERBASE")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            crate::pathman::get_user_env_vars()
                .ok()
                .and_then(|vars| vars.get("PYTHONUSERBASE").cloned())
                .filter(|value| !value.trim().is_empty())
        });
    python_user_base_from(configured, std::env::var_os("APPDATA"))
}

/// Count distribution metadata entries without importing or executing Python.
fn count_user_distributions(site: &Path) -> std::io::Result<usize> {
    Ok(std::fs::read_dir(site)?
        .flatten()
        .filter(|entry| {
            let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
            name.ends_with(".dist-info") || name.ends_with(".egg-info")
        })
        .count())
}

pub(crate) fn python_user_package_scope() -> PythonUserScope {
    python_user_package_scope_at(python_user_base())
}

/// Same scan against an already resolved base, so fixtures can point at a
/// temporary directory without touching the caller's environment.
pub(crate) fn python_user_package_scope_at(
    base: std::result::Result<PathBuf, String>,
) -> PythonUserScope {
    let base = match base {
        Ok(base) => base,
        Err(reason) => {
            return PythonUserScope {
                path: None,
                packages: None,
                reason: format!("未读取用户级包：{reason}。"),
                shared: true,
            }
        }
    };
    // A user base holds one directory per Python version, each with site-packages.
    let mut versioned = std::fs::read_dir(&base)
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.path().join("site-packages"))
                .filter(|site| site.is_dir())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    versioned.sort();
    if versioned.is_empty() {
        return PythonUserScope {
            path: Some(base.to_string_lossy().into_owned()),
            packages: Some(0),
            reason: String::new(),
            shared: false,
        };
    }
    let mut total = 0usize;
    let mut unreadable = Vec::new();
    for site in &versioned {
        match count_user_distributions(site) {
            Ok(count) => total += count,
            Err(error) => unreadable.push(format!("{}: {error}", site.display())),
        }
    }
    PythonUserScope {
        path: Some(base.to_string_lossy().into_owned()),
        packages: if unreadable.is_empty() { Some(total) } else { None },
        reason: if unreadable.is_empty() {
            String::new()
        } else {
            format!("部分用户级目录无法读取：{}。", unreadable.join("；"))
        },
        shared: versioned.len() > 1,
    }
}

fn attach_installer_preview(
    root: &Path,
    mut preview: UninstallPreview,
    registry: crate::installer_uninstall::RegistrySnapshot,
) -> Result<UninstallPreview> {
    if preview.is_link_entry || !matches!(preview.env_type, EnvType::Python | EnvType::Llvm) {
        return Ok(preview);
    }
    let path = root.join("envs").join(preview.env_type.folder()).join(&preview.name);
    let plan = crate::installer_uninstall::preview(preview.env_type, &path, registry)?;
    preview.unverified.retain(|item| item.resource != "安装器登记与目录外副作用");
    if plan.state == "ready" {
        // The original uninstaller owns these files; listing them as a second
        // recursive-delete action would misstate what EnvCon will do.
        preview.removable.retain(|item|
            item.resource != "EnvCon 受管目录" && item.resource != "Python 环境内 site-packages"
        );
        if let Some(invocation) = &plan.invocation {
            preview.removable.push(UninstallScopeItem {
                resource: "原厂卸载范围（执行后复查）".into(),
                path: Some(invocation.executable.clone()),
                count: None,
                details: format!(
                    "登记：{}；固定参数：{}。执行前重新读取登记并核对 EXE 哈希；不经过 shell，也不回退到递归删除目录。",
                    invocation.registration_id, invocation.arguments.join(" ")
                ),
            });
        } else {
            preview.scope_complete = false;
            preview.blocked.push(UninstallScopeItem {
                resource: "原厂卸载登记".into(), path: Some(plan.install_path.clone()), count: None,
                details: "登记计划标记为就绪但没有可执行的精确入口，已阻止执行。".into(),
            });
        }
    } else if plan.state != "unmatched" {
        preview.scope_complete = false;
        preview.blocked.push(UninstallScopeItem {
            resource: "原厂卸载登记".into(), path: Some(plan.install_path.clone()), count: None,
            details: "登记读取不完整、路径/命令无法核验或存在冲突；保留目录和卸载入口，请先核对 Windows 应用设置。".into(),
        });
    }
    for assessment in &plan.assessments {
        if assessment.state == "matched" { continue; }
        preview.unverified.push(UninstallScopeItem {
            resource: "卸载登记核对".into(), path: Some(assessment.registration_id.clone()),
            count: None, details: format!("{}：{}", assessment.state, assessment.details),
        });
    }
    for error in &plan.errors {
        preview.unverified.push(UninstallScopeItem {
            resource: "卸载登记读取限制".into(), path: None, count: None, details: error.clone(),
        });
    }
    preview.unverified.push(UninstallScopeItem {
        resource: "安装器目录外副作用".into(), path: None, count: None,
        details: match plan.state.as_str() {
            "unmatched" => "本次读取范围未匹配到所选路径的原厂卸载入口，不代表绝对没有安装器登记；仅按目录移除，外部关联项仍未核验。".into(),
            "ready" => "执行仅依据精确匹配的本地卸载入口；目录外注册项、共享组件、用户级包和缓存不在可证明删除范围内。执行后只核验该登记与所选运行时目录，不宣称机器级完整清理。".into(),
            _ => "登记未就绪，未计划执行卸载器；目录外关联项仍未核验。".into(),
        },
    });
    preview.installer_plan = Some(plan);
    Ok(preview)
}

fn uninstall_path_plan(
    root: &Path,
    env_type: EnvType,
    name: &str,
    journal: &Path,
    current_path: &str,
    confirmed_legacy: &[String],
) -> Result<crate::path_ownership::CleanupPlan> {
    let shared = uninstall_path_has_dependents(root, env_type, name)?;
    // Match only the fixed current entry, never its resolved target or external aliases.
    let keys: Vec<_> = env_type.path_entries(root).iter().map(|path| system::path_key(path)).collect();
    let candidates: Vec<_> = current_path.split(';')
        .filter(|entry| keys.contains(&system::path_key(Path::new(entry.trim().trim_matches('"')))))
        .map(str::to_owned).collect();
    let dependencies = if shared { candidates.clone() } else { Vec::new() };
    crate::path_ownership::preview_cleanup_with_confirmations(
        journal, current_path, &candidates, &dependencies, confirmed_legacy,
    )
}

fn uninstall_path_has_dependents(root: &Path, env_type: EnvType, name: &str) -> Result<bool> {
    validate_environment_parent(root, env_type)?;
    let category = root.join("envs").join(env_type.folder());
    let selected = category.join(name);
    let mut shared = false;
    // Unknown or inaccessible siblings are dependencies, never evidence of absence.
    for entry in std::fs::read_dir(&category)? {
        let entry = entry?;
        if system::path_key(&entry.path()) != system::path_key(&selected) {
            shared = true;
        }
    }
    let current_dir = root.join("current");
    if system::has_reparse_component(&current_dir) {
        return Err(AppError::msg("current 的祖先目录被重定向，无法确认 PATH 依赖"));
    }
    let junction = current_dir.join(env_type.junction());
    if !path_is_absent(&junction)? && !current_junction_targets(&junction, &selected) {
        shared = true;
    }
    Ok(shared)
}

struct UninstallPathStore<'a, T> {
    backend: &'a mut T,
    root: &'a Path,
    env_type: EnvType,
    name: &'a str,
}

impl<T: crate::path_ownership::PathStore> crate::path_ownership::PathStore for UninstallPathStore<'_, T> {
    fn read(&mut self) -> Result<String> { self.backend.read() }
    fn write(&mut self, value: &str) -> Result<()> {
        // Recheck dependencies at the write boundary, after the journal's read check.
        if uninstall_path_has_dependents(self.root, self.env_type, self.name)? {
            return Err(AppError::msg("发现其他环境依赖，停止清理 PATH"));
        }
        self.backend.write(value)
    }
}

fn preview_uninstall_with_path(
    root: &Path,
    env_type: EnvType,
    name: &str,
    journal: &Path,
    store: &mut Result<impl crate::path_ownership::PathStore>,
) -> Result<(UninstallPreview, Option<crate::path_ownership::CleanupPlan>)> {
    preview_uninstall_with_path_mode(
        root,
        env_type,
        name,
        UninstallMode::Uninstall,
        journal,
        store,
    )
}

/// Same as [`preview_uninstall_with_path`] but for an explicit mode. The
/// historical entry point above stays as the `Uninstall` delegate so the many
/// fixtures that never touch modes keep reading as before.
fn preview_uninstall_with_path_mode(
    root: &Path,
    env_type: EnvType,
    name: &str,
    mode: UninstallMode,
    journal: &Path,
    store: &mut Result<impl crate::path_ownership::PathStore>,
) -> Result<(UninstallPreview, Option<crate::path_ownership::CleanupPlan>)> {
    let mut preview = preview_uninstall_scope(root, env_type, name)?;
    shape_preview(&mut preview, mode);
    preview.unverified.retain(|item| item.resource != "PATH 与环境变量");
    preview.unverified.push(UninstallScopeItem {
        resource: "系统 PATH 与其他环境变量".into(), path: None, count: None,
        details: "来源不明的固定用户 PATH 入口只会列为单独候选；系统 PATH、HOME 等变量及外部别名保留。用户确认仅授权移除精确条目，不证明其由 EnvCon 创建。".into(),
    });
    let plan = (|| {
        let backend = store.as_mut().map_err(|error| AppError::msg(error.to_string()))?;
        let current = crate::path_ownership::PathStore::read(backend)?;
        uninstall_path_plan(root, env_type, name, journal, &current, &[])
    })();
    match plan {
        Ok(plan) => {
            preview.path_snapshot_hash = Some(snapshot_hash(&serde_json::to_string(&plan)?));
            for entry in &plan.removed {
                preview.removable.push(UninstallScopeItem {
                    resource: OWNED_PATH_RESOURCE.into(), path: Some(entry.clone()), count: None,
                    details: "来源日志与当前 PATH 一致且未发现其他受管版本依赖；受管范围移除成功后复查并清理此入口。".into(),
                });
            }
            preview.legacy_path_entries = plan.legacy_candidates.iter().map(|entry| UninstallScopeItem {
                resource: "来源未知的用户 PATH 条目".into(), path: Some(entry.clone()), count: None,
                details: "该固定 current 入口没有可用的 EnvCon 写入来源证据；默认保留。逐项确认仅授权移除此精确条目，不代表 EnvCon 创建。".into(),
            }).collect();
            for (entry, reason) in &plan.retained {
                preview.retained.push(UninstallScopeItem {
                    resource: OWNED_PATH_RESOURCE.into(), path: Some(entry.clone()), count: None,
                    details: format!("保留：{reason}。"),
                });
            }
            Ok((preview, Some(plan)))
        }
        Err(error) => {
            preview.unverified.push(UninstallScopeItem {
                resource: OWNED_PATH_RESOURCE.into(), path: None, count: None,
                details: format!("无法核验来源或共享依赖，保留用户 PATH：{error}"),
            });
            Ok((preview, None))
        }
    }
}

async fn execute_uninstall_with_path(
    root: PathBuf,
    env_type: EnvType,
    name: String,
    expected: UninstallPreview,
    journal: &Path,
    store: &mut Result<impl crate::path_ownership::PathStore>,
) -> Result<UninstallResult> {
    reject_installer_ready_directory_removal(&expected)?;
    let mode = expected.mode;
    let (mut preview, plan) =
        preview_uninstall_with_path_mode(&root, env_type, &name, mode, journal, store)?;
    validate_uninstall_execution_preview(&expected, &preview)?;
    let plan = plan.map(|plan| plan.with_confirmations(&expected.confirmed_legacy_path_entries)).transpose()?;
    preview.confirmed_legacy_links = expected.confirmed_legacy_links.clone();
    let mut basic = preview_uninstall_scope(&root, env_type, &name)?;
    shape_preview(&mut basic, mode);
    basic.confirmed_legacy_links = expected.confirmed_legacy_links.clone();
    // Bind the filesystem check to the same enriched preview before any mutation.
    let mut checked = basic.clone();
    checked.path_snapshot_hash = preview.path_snapshot_hash.clone();
    checked.legacy_path_entries = preview.legacy_path_entries.clone();
    checked.removable.extend(preview.removable.iter().filter(|item| item.resource == OWNED_PATH_RESOURCE).cloned());
    checked.retained.extend(preview.retained.iter().filter(|item| item.resource == OWNED_PATH_RESOURCE).cloned());
    checked.unverified.retain(|item| item.resource != "PATH 与环境变量");
    checked.unverified.extend(preview.unverified.iter().filter(|item|
        item.resource == OWNED_PATH_RESOURCE || item.resource == "系统 PATH 与其他环境变量"
    ).cloned());
    validate_uninstall_execution_preview(&preview, &checked)?;
    let mut result = execute_uninstall_scope(root.clone(), env_type, name.clone(), basic).await?;
    apply_path_scope_and_cleanup(
        &mut result,
        UninstallPathScope { preview: &preview, expected: &expected, plan },
        UninstallTarget { root: &root, env_type, name: &name, journal },
        store,
    );
    if !result.failed.is_empty() {
        result.status = UninstallResultStatus::Partial;
    }
    Ok(result)
}

/// 原厂卸载所需的身份与定位信息。
///
/// `root` 与 `journal` 同为 `&Path`，`env_type` 与 `name` 一起标识目标；
/// 逐个传参时把两个路径传反不会编译失败，只会让日志写错位置。
struct InstallerUninstallTarget<'a> {
    root: PathBuf,
    env_type: EnvType,
    name: String,
    journal: &'a Path,
}

async fn execute_installer_uninstall_with_path<B>(
    target: InstallerUninstallTarget<'_>,
    expected: UninstallPreview,
    plan: crate::installer_uninstall::Plan,
    backend: B,
    store: &mut Result<impl crate::path_ownership::PathStore>,
) -> Result<UninstallResult>
where
    B: crate::installer_uninstall::Backend + Send + 'static,
{
    let InstallerUninstallTarget { root, env_type, name, journal } = target;
    let (mut preview, cleanup_plan) = preview_uninstall_with_path(&root, env_type, &name, journal, store)?;
    validate_uninstall_execution_preview(&expected, &preview)?;
    preview.confirmed_legacy_links = expected.confirmed_legacy_links.clone();
    let cleanup_plan = cleanup_plan
        .map(|plan| plan.with_confirmations(&expected.confirmed_legacy_path_entries))
        .transpose()?;
    let mut current = preview_uninstall_scope(&root, env_type, &name)?;
    current.confirmed_legacy_links = expected.confirmed_legacy_links.clone();
    current.path_snapshot_hash = preview.path_snapshot_hash.clone();
    current.legacy_path_entries = preview.legacy_path_entries.clone();
    current.removable.extend(preview.removable.iter().filter(|item| item.resource == OWNED_PATH_RESOURCE).cloned());
    current.retained.extend(preview.retained.iter().filter(|item| item.resource == OWNED_PATH_RESOURCE).cloned());
    current.unverified.retain(|item| item.resource != "PATH 与环境变量");
    current.unverified.extend(preview.unverified.iter().filter(|item|
        item.resource == OWNED_PATH_RESOURCE || item.resource == "系统 PATH 与其他环境变量"
    ).cloned());
    validate_uninstall_execution_preview(&preview, &current)?;

    let registration_id = plan.invocation.as_ref()
        .map(|invocation| invocation.registration_id.clone())
        .ok_or_else(|| AppError::msg("原厂卸载计划缺少登记标识"))?;
    let install_path = plan.install_path.clone();
    let (mut removed, mut failed) = detach_current_link_for_uninstall(&root, env_type, &current)?;
    if failed.is_empty() {
        if plan.state != "ready" {
            failed.push(UninstallActionItem {
                resource: "原厂卸载范围".into(),
                path: Some(install_path.clone()),
                details: "原厂卸载计划不是就绪状态，未执行命令。".into(),
            });
        } else {
            let execution = tokio::task::spawn_blocking(move || {
                let mut backend = backend;
                crate::installer_uninstall::execute(&plan, &mut backend)
            }).await;
            match execution {
                Ok(Ok(execution)) => {
                    if execution.command_exit_code != Some(0) {
                        let exit = execution.command_exit_code
                            .map(|code| code.to_string())
                            .unwrap_or_else(|| "未知".into());
                        failed.push(UninstallActionItem {
                            resource: "原厂卸载命令".into(),
                            path: Some(install_path.clone()),
                            details: format!(
                                "退出码 {exit}；退出码本身不代表成功。{}",
                                execution.details.join("；")
                            ),
                        });
                    }
                    match execution.registration_removed {
                        Some(true) => removed.push(UninstallActionItem {
                            resource: "原厂卸载登记".into(),
                            path: Some(registration_id.clone()),
                            details: "执行后重新读取登记，未再发现匹配该安装路径的原厂卸载项。".into(),
                        }),
                        Some(false) => failed.push(UninstallActionItem {
                            resource: "原厂卸载登记".into(),
                            path: Some(registration_id.clone()),
                            details: "命令结束后仍发现匹配该安装路径的登记；保留残留内容，请重新预览或使用原厂入口。".into(),
                        }),
                        None => {
                            failed.push(UninstallActionItem {
                                resource: "原厂卸载登记".into(),
                                path: Some(registration_id.clone()),
                                details: "无法复查登记是否移除，不能确认卸载完成。".into(),
                            });
                        }
                    }
                    match execution.runtime_removed {
                        Some(true) => removed.push(UninstallActionItem {
                            resource: "原厂卸载运行时目录".into(),
                            path: Some(install_path.clone()),
                            details: "执行后复查所选安装路径不存在。".into(),
                        }),
                        Some(false) => {
                            failed.push(UninstallActionItem {
                                resource: "原厂卸载运行时目录".into(),
                                path: Some(install_path.clone()),
                                details: "命令结束后安装路径仍存在；未递归删除残留内容。".into(),
                            });
                            current.retained.push(UninstallScopeItem {
                                resource: "原厂卸载后的残留内容".into(),
                                path: Some(install_path.clone()),
                                count: None,
                                details: "原厂卸载未使所选路径消失；可能是未安装完的登记、占用文件或安装器范围外数据，保留供用户核对。".into(),
                            });
                        }
                        None => failed.push(UninstallActionItem {
                            resource: "原厂卸载运行时目录".into(),
                            path: Some(install_path.clone()),
                            details: "无法复查安装路径是否移除，不能确认卸载完成。".into(),
                        }),
                    }
                    if execution.status != "verifiedInstallerScopeRemoved" {
                        current.unverified.push(UninstallScopeItem {
                            resource: "原厂卸载结果复核".into(),
                            path: Some(install_path.clone()),
                            count: None,
                            details: execution.details.join("；"),
                        });
                    }
                }
                Ok(Err(error)) => failed.push(UninstallActionItem {
                    resource: "原厂卸载预检/执行".into(),
                    path: Some(install_path.clone()),
                    details: format!("命令未通过执行前核验或未能启动；未递归删除目录：{error}"),
                }),
                Err(error) => failed.push(UninstallActionItem {
                    resource: "原厂卸载任务".into(),
                    path: Some(install_path.clone()),
                    details: format!("执行任务异常结束；未递归删除目录：{error}"),
                }),
            }
        }
    }

    let mut result = UninstallResult {
        name,
        status: if failed.is_empty() {
            UninstallResultStatus::VerifiedScopeRemoved
        } else {
            UninstallResultStatus::Partial
        },
        removed,
        retained: current.retained.clone(),
        unverified: current.unverified.clone(),
        failed,
    };
    let result_name = result.name.clone();
    apply_path_scope_and_cleanup(
        &mut result,
        UninstallPathScope { preview: &preview, expected: &expected, plan: cleanup_plan },
        UninstallTarget { root: &root, env_type, name: &result_name, journal },
        store,
    );
    if !result.failed.is_empty() {
        result.status = UninstallResultStatus::Partial;
    }
    Ok(result)
}

/// 卸载预览与据此得出的 PATH 清理计划。
///
/// `preview` 与 `expected` 类型完全相同，逐个传参时传反不会编译失败，
/// 却会让「用户确认的内容」与「当前观察到的内容」互换——两者恰恰是
/// 用来互相校验的。收进结构体后调用点按字段名书写，传错会变成字段名写错。
struct UninstallPathScope<'a> {
    preview: &'a UninstallPreview,
    expected: &'a UninstallPreview,
    plan: Option<crate::path_ownership::CleanupPlan>,
}

/// 本次卸载的目标身份。`root` 与 `journal` 同为 `&Path`，同样有传反风险。
struct UninstallTarget<'a> {
    root: &'a Path,
    env_type: EnvType,
    name: &'a str,
    journal: &'a Path,
}

fn apply_path_scope_and_cleanup(
    result: &mut UninstallResult,
    scope: UninstallPathScope<'_>,
    target: UninstallTarget<'_>,
    store: &mut Result<impl crate::path_ownership::PathStore>,
) {
    let UninstallPathScope { preview, expected, plan } = scope;
    let UninstallTarget { root, env_type, name, journal } = target;
    result.retained.extend(preview.retained.iter().filter(|item| item.resource == OWNED_PATH_RESOURCE).cloned());
    result.retained.extend(preview.legacy_path_entries.iter()
        .filter(|item| !item.path.as_ref().is_some_and(|entry| expected.confirmed_legacy_path_entries.contains(entry)))
        .cloned()
        .map(|mut item| {
            item.details = "未对此条目作本次移除确认，保持原样保留。".into();
            item
        }));
    result.unverified.retain(|item| item.resource != "PATH 与环境变量");
    result.unverified.extend(preview.unverified.iter().filter(|item|
        item.resource == OWNED_PATH_RESOURCE || item.resource == "系统 PATH 与其他环境变量"
    ).cloned());
    if let Some(plan) = plan.filter(|plan| !plan.removed.is_empty()) {
        if result.failed.is_empty() {
            let cleanup = (|| {
                let backend = store.as_mut().map_err(|error| AppError::msg(error.to_string()))?;
                let current = crate::path_ownership::PathStore::read(backend)?;
                let fresh = uninstall_path_plan(
                    root, env_type, name, journal, &current, &plan.confirmed_legacy,
                )?;
                if fresh != plan {
                    return Err(AppError::msg("PATH、来源记录或其他版本依赖已变化，未执行清理，请重新检查"));
                }
                let mut guarded = UninstallPathStore { backend, root, env_type, name };
                crate::path_ownership::apply(&mut guarded, journal, crate::path_ownership::Change::Remove(&fresh, &[]))?;
                Ok(())
            })();
            for entry in &plan.removed {
                let legacy = plan.confirmed_legacy.contains(entry);
                match &cleanup {
                    Ok(()) => result.removed.push(UninstallActionItem {
                        resource: if legacy { "用户确认移除的历史 PATH 条目" } else { OWNED_PATH_RESOURCE }.into(),
                        path: Some(entry.clone()),
                        details: if legacy {
                            "按本次逐项确认清理并重新读取 PATH；不记录为 EnvCon 创建来源。".into()
                        } else {
                            "已清理并重新读取用户 PATH，确认结果与预览一致。".into()
                        },
                    }),
                    Err(error) => result.failed.push(UninstallActionItem {
                        resource: if legacy { "用户确认移除的历史 PATH 条目" } else { OWNED_PATH_RESOURCE }.into(),
                        path: Some(entry.clone()),
                        details: if legacy {
                            format!("逐项确认后的 PATH 清理未能核验，可能已执行：{error}")
                        } else {
                            format!("PATH 清理未能核验，可能已执行：{error}")
                        },
                    }),
                }
            }
        } else {
            result.retained.extend(plan.removed.into_iter().map(|entry| UninstallScopeItem {
                resource: if plan.confirmed_legacy.contains(&entry) {
                    "来源未知的用户 PATH 条目"
                } else { OWNED_PATH_RESOURCE }.into(), path: Some(entry), count: None,
                details: "受管范围移除未完全成功，未尝试清理 PATH。".into(),
            }));
        }
    }
}

async fn execute_uninstall_scope(
    root: PathBuf,
    env_type: EnvType,
    name: String,
    expected_preview: UninstallPreview,
) -> Result<UninstallResult> {
    reject_installer_ready_directory_removal(&expected_preview)?;
    // The regenerated preview must carry the same mode, otherwise the equality
    // check below would reject a legitimate Detach request as "changed".
    let mode = expected_preview.mode;
    let mut current_preview = preview_uninstall_scope(&root, env_type, &name)?;
    shape_preview(&mut current_preview, mode);
    validate_uninstall_execution_preview(&expected_preview, &current_preview)?;
    let env_dir = root.join("envs").join(env_type.folder()).join(&name);
    validate_environment_parent(&root, env_type)?;
    if std::fs::symlink_metadata(&env_dir).is_err() {
        return Err(AppError::msg(format!("环境不存在: {}", env_dir.display())));
    }
    let (mut removed, mut failed) = detach_current_link_for_uninstall(&root, env_type, &current_preview)?;
    let legacy_entry = current_preview.legacy_links.iter().any(|item| item.resource == "来源未知的管理链接");
    if failed.is_empty() {
        let mut after_detach_expected = current_preview.clone();
        after_detach_expected.confirmed_legacy_links = expected_preview.confirmed_legacy_links.clone();
        if current_preview.is_current {
            after_detach_expected.is_current = false;
            after_detach_expected.current_link_snapshot_hash = None;
            after_detach_expected
                .removable
                .retain(|item| item.resource != "当前版本 Junction");
            after_detach_expected.legacy_links.retain(|item| item.resource != "来源未知的 current 链接");
            after_detach_expected.confirmed_legacy_links.retain(|path|
                after_detach_expected.legacy_links.iter().any(|item| item.path.as_ref() == Some(path))
            );
        }
        let after_detach = preview_uninstall_scope(&root, env_type, &name)
            .map(|mut actual| {
                // Re-shape with the same mode: an Uninstall-shaped observation
                // would otherwise never match a Detach expectation.
                shape_preview(&mut actual, mode);
                actual
            });
        let scope_still_matches = after_detach
            .as_ref()
            .is_ok_and(|actual| validate_uninstall_execution_preview(&after_detach_expected, actual).is_ok());
        if !scope_still_matches {
            failed.push(UninstallActionItem {
                resource: if current_preview.is_link_entry {
                    if current_preview.is_external_link {
                        "EnvCon 外部纳管链接".into()
                    } else {
                        "EnvCon 受管目录链接".into()
                    }
                } else {
                    "EnvCon 受管目录".into()
                },
                path: Some(env_dir.to_string_lossy().into_owned()),
                details: "目录状态在执行过程中发生变化或无法重新核验；为避免扩大删除范围，保留该目录，请重新预览。".into(),
            });
        } else if mode == UninstallMode::Detach {
            // 停用模式：目录内容必须原地保留，所以这里不做任何删除。
            // 结果中的 `retained` 已由 `apply_detach_mode` 填入保留项，
            // `removed` 只会包含上方已解除的当前版本 Junction。
        } else {
            let verify_env_dir = env_dir.clone();
            let is_link_entry = current_preview.is_link_entry;
            let link_fingerprint = current_preview.snapshot_hash.clone();
            let remove_root = root.clone();
            let resource = if legacy_entry {
                "用户确认移除的管理链接"
            } else if current_preview.is_link_entry {
                if current_preview.is_external_link {
                    "EnvCon 外部纳管链接"
                } else {
                    "EnvCon 受管目录链接"
                }
            } else {
                "EnvCon 受管目录"
            };
            match tokio::task::spawn_blocking(move || {
                if is_link_entry {
                    if legacy_entry {
                        crate::link_ownership::remove_confirmed_legacy(
                            &remove_root, &env_dir,
                            link_fingerprint.as_deref().ok_or_else(|| AppError::msg("旧链接身份快照缺失"))?,
                        )
                    } else {
                        crate::link_ownership::remove(&remove_root, &env_dir, link_fingerprint.as_deref())
                    }
                } else {
                    force_remove_dir_all(&env_dir)
                }
            })
            .await
            {
                Ok(Ok(())) => match path_is_absent(&verify_env_dir) {
                    Ok(true) => removed.push(UninstallActionItem {
                        resource: resource.into(),
                        path: Some(verify_env_dir.to_string_lossy().into_owned()),
                        details: if legacy_entry {
                            "按用户对此旧链接的单独确认移除，并核验入口不存在；目标保留，不据此认领原创建来源。".into()
                        } else if current_preview.is_link_entry {
                            if current_preview.is_external_link {
                                "已移除外部管理链接并确认链接入口不存在；未删除原安装目标。".into()
                            } else {
                                "已移除受管目录内的链接入口；未删除其指向的受管目标目录。".into()
                            }
                        } else {
                            "已删除并确认受管目录不存在。".into()
                        },
                    }),
                    Ok(false) => failed.push(UninstallActionItem {
                        resource: resource.into(),
                        path: Some(verify_env_dir.to_string_lossy().into_owned()),
                        details: "删除返回后仍发现管理入口，未能验证移除。".into(),
                    }),
                    Err(error) => failed.push(UninstallActionItem {
                        resource: resource.into(),
                        path: Some(verify_env_dir.to_string_lossy().into_owned()),
                        details: format!("无法核验管理入口是否移除：{error}"),
                    }),
                },
                Ok(Err(error)) => failed.push(UninstallActionItem {
                    resource: resource.into(),
                    path: Some(verify_env_dir.to_string_lossy().into_owned()),
                    details: format!("删除受管内容失败，可能已部分删除：{error}"),
                }),
                Err(error) => failed.push(UninstallActionItem {
                    resource: resource.into(),
                    path: Some(verify_env_dir.to_string_lossy().into_owned()),
                    details: format!("删除任务失败：{error}"),
                }),
            }
        }
    }
    let external_target = current_preview
        .retained
        .iter()
        .find(|item| item.resource == "外部原安装目标")
        .and_then(|item| item.path.clone());
    let mut retained = current_preview.retained;
    let mut unverified = current_preview.unverified;
    if current_preview.is_external_link && current_preview.is_link_entry {
        if let Some(target) = external_target {
            match std::fs::symlink_metadata(&target) {
                Ok(_) => {}
                Err(error) => {
                    unverified.push(UninstallScopeItem {
                        resource: "外部原安装目标".into(),
                        path: Some(target),
                        details: format!("无法证明原目标仍存在或可访问；本操作未尝试删除目标：{error}"),
                        count: None,
                    });
                    retained.retain(|item| item.resource != "外部原安装目标");
                }
            }
        }
    }
    Ok(UninstallResult {
        name,
        status: if failed.is_empty() {
            UninstallResultStatus::VerifiedScopeRemoved
        } else {
            UninstallResultStatus::Partial
        },
        removed,
        retained,
        unverified,
        failed,
    })
}

fn reject_installer_ready_directory_removal(preview: &UninstallPreview) -> Result<()> {
    if preview.installer_plan.as_ref().is_some_and(|plan| plan.state == "ready") {
        return Err(AppError::msg("该环境已精确匹配原厂卸载入口，拒绝回退到递归删除目录"));
    }
    Ok(())
}

fn detach_current_link_for_uninstall(
    root: &Path,
    env_type: EnvType,
    preview: &UninstallPreview,
) -> Result<(Vec<UninstallActionItem>, Vec<UninstallActionItem>)> {
    if !preview.is_current {
        return Ok((Vec::new(), Vec::new()));
    }
    let junction = root.join("current").join(env_type.junction());
    let legacy_current = preview.legacy_links.iter()
        .any(|item| item.resource == "来源未知的 current 链接");
    let detach = if legacy_current {
        crate::link_ownership::remove_confirmed_legacy(
            root, &junction,
            preview.current_link_snapshot_hash.as_deref()
                .ok_or_else(|| AppError::msg("旧链接身份快照缺失"))?,
        )
    } else {
        switcher::remove_junction_for_checked(root, env_type, preview.current_link_snapshot_hash.as_deref())
    };
    if let Err(error) = detach {
        return Ok((Vec::new(), vec![UninstallActionItem {
            resource: "当前版本 Junction".into(),
            path: Some(junction.to_string_lossy().into_owned()),
            details: error.to_string(),
        }]));
    }
    Ok(match path_is_absent(&junction) {
        Ok(true) => (vec![UninstallActionItem {
            resource: "当前版本 Junction".into(),
            path: Some(junction.to_string_lossy().into_owned()),
            details: if legacy_current {
                "按用户对此旧链接的单独确认移除，并确认入口不存在；不据此认领原创建来源。".into()
            } else { "已移除并确认链接不存在。".into() },
        }], Vec::new()),
        Ok(false) => (Vec::new(), vec![UninstallActionItem {
            resource: "当前版本 Junction".into(),
            path: Some(junction.to_string_lossy().into_owned()),
            details: "命令返回后仍发现链接，未能验证移除。".into(),
        }]),
        Err(error) => (Vec::new(), vec![UninstallActionItem {
            resource: "当前版本 Junction".into(),
            path: Some(junction.to_string_lossy().into_owned()),
            details: format!("无法核验链接是否移除：{error}"),
        }]),
    })
}

#[derive(Default)]
struct TreeInventory {
    files: u64,
    directories: u64,
    reparse_points: u64,
    signature: String,
    /// Relative paths whose deletion is currently blocked by another handle.
    locked: Vec<String>,
    /// Directories another process currently holds open. Reported separately
    /// because "a program has this folder open" is a different fix for the user
    /// than "a file is locked".
    locked_dirs: Vec<String>,
}

/// How many files the preview probes for an open handle. Probing every file in
/// a large tree would cost one open/close per entry for no extra certainty, so
/// the preview samples and says so instead of claiming a complete scan.
const LOCK_PROBE_LIMIT: u64 = 200;

/// `DELETE` access bit: requesting it answers "would deleting this file fail
/// right now" without deleting anything.
const DELETE_ACCESS: u32 = 0x0001_0000;
/// Opening a *directory* with `DELETE` access requires this flag; without it
/// Windows answers access-denied for every directory, which would be
/// indistinguishable from a permissions problem and is therefore not evidence.
const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
/// Any handle that closed off deletion. This is the only code that means
/// "in use"; access-denied means something else entirely.
const ERROR_SHARING_VIOLATION: i32 = 32;

/// Resource name used for the lock observation. Named once so the preview and
/// the preview-equality check cannot drift apart.
const LOCK_RESOURCE: &str = "文件占用";

/// Resource name for a directory another process holds open. Separate from
/// `LOCK_RESOURCE` because the remedy differs: a locked file is usually closed
/// from the file itself, while an open folder is usually someone's working
/// directory.
const LOCKED_DIR_RESOURCE: &str = "目录占用";

/// Whether another handle currently prevents deleting this directory.
///
/// Same rule as [`file_blocks_deletion`]: only a sharing violation counts.
/// Access-denied is a permission or privilege-probe outcome and must never be
/// reported as "in use". A directory that merely sits on someone's PATH or is
/// someone's working directory may still open successfully for `DELETE`, so a
/// `false` here means "nothing observed", not "safe to delete".
fn directory_blocks_deletion(path: &Path) -> bool {
    use std::os::windows::fs::OpenOptionsExt;
    match std::fs::OpenOptions::new()
        .access_mode(DELETE_ACCESS)
        .share_mode(7)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)
    {
        Ok(handle) => {
            drop(handle);
            false
        }
        Err(error) => error.raw_os_error() == Some(ERROR_SHARING_VIOLATION),
    }
}

/// Whether another handle currently blocks deleting this file.
///
/// Opens with `DELETE` access and closes immediately, so nothing is modified.
/// Success means no handle is in the way. A sharing violation means a real
/// lock, while access-denied is a permissions problem and must never be
/// reported as "in use".
fn file_blocks_deletion(path: &Path) -> bool {
    use std::os::windows::fs::OpenOptionsExt;
    match std::fs::OpenOptions::new()
        .access_mode(DELETE_ACCESS)
        // Allow every access ourselves so the decision rests only on whether
        // an existing handle permits deletion.
        .share_mode(7)
        .open(path)
    {
        Ok(handle) => {
            drop(handle);
            false
        }
        Err(error) => error.raw_os_error() == Some(ERROR_SHARING_VIOLATION),
    }
}

fn inventory_managed_tree(
    path: &Path,
    root: &Path,
    relative: &Path,
    counts: &mut TreeInventory,
) -> std::io::Result<()> {
    let meta = std::fs::symlink_metadata(path)?;
    if is_reparse(&meta) {
        counts.reparse_points += 1;
        counts.signature.push_str(&format!(
            "L|{}|{}|{}|",
            relative.to_string_lossy().to_lowercase().replace('/', "\\"),
            meta.len(),
            std::fs::read_link(path)?.to_string_lossy().replace('/', "\\")
        ));
        return Ok(());
    }
    let actual = path.canonicalize()?;
    if !system::path_within(root, &actual) {
        return Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied, "目录项解析到受管环境目录之外"));
    }
    if meta.is_dir() {
        counts.directories += 1;
        // Probe directories too: a folder another process holds open blocks the
        // delete just as a locked file does, only the remedy differs. Sampled
        // for the same reason as files.
        if counts.directories <= LOCK_PROBE_LIMIT && directory_blocks_deletion(path) {
            counts
                .locked_dirs
                .push(relative.to_string_lossy().replace('/', "\\"));
        }
        counts.signature.push_str(&format!(
            "D|{}|",
            relative.to_string_lossy().to_lowercase().replace('/', "\\")
        ));
        let mut entries = std::fs::read_dir(path)?
            .map(|entry| entry.map(|entry| entry.path()))
            .collect::<std::io::Result<Vec<_>>>()?;
        entries.sort_by(|a, b| {
            a.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_lowercase()
                .cmp(
                    &b.file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_lowercase(),
                ).then_with(|| a.file_name().cmp(&b.file_name()))
        });
        for child in entries {
            let name = child.file_name().unwrap_or_default();
            inventory_managed_tree(&child, root, &relative.join(name), counts)?;
        }
    } else {
        counts.files += 1;
        // Probe only the first files up to the limit: this is a sample, and the
        // preview text must say so rather than implying a complete scan.
        if counts.files <= LOCK_PROBE_LIMIT && file_blocks_deletion(path) {
            counts
                .locked
                .push(relative.to_string_lossy().replace('/', "\\"));
        }
        let modified = meta
            .modified()
            .ok()
            .and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|value| value.as_nanos())
            .unwrap_or_default();
        counts.signature.push_str(&format!(
            "F|{}|{}|{}|",
            relative.to_string_lossy().to_lowercase().replace('/', "\\"),
            meta.len(),
            modified
        ));
    }
    Ok(())
}

fn snapshot_hash(signature: &str) -> String {
    use sha2::Digest;
    let mut hasher = sha2::Sha256::new();
    hasher.update(signature.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn preview_uninstall_scope(root: &Path, env_type: EnvType, name: &str) -> Result<UninstallPreview> {
    switcher::validate_name(name)?;
    let root = root.canonicalize()?;
    validate_environment_parent(&root, env_type)?;
    let env_dir = root.join("envs").join(env_type.folder()).join(name);
    let category_root = root.join("envs").join(env_type.folder());
    let metadata = std::fs::symlink_metadata(&env_dir)
        .map_err(|e| AppError::msg(format!("无法读取环境路径 {}: {e}", env_dir.display())))?;
    let is_link_entry = is_reparse(&metadata);
    // 「链接目标已不存在」与「链接对象本身读不了」是两件事。前者仍能安全
    // 移除链接对象（不触发任何目标），只是无法再判断它指向受管目录还是外部
    // 安装；后者连对象身份都无法确认，必须继续阻断。
    let mut unresolved_target: Option<String> = None;
    let (is_external_link, link_target, link_error) = if is_link_entry {
        match link_target_path(&env_dir) {
            Ok(target) => match target.canonicalize() {
                Ok(target) => (
                    !system::path_within(&root, &target),
                    Some(target),
                    None,
                ),
                Err(error) => {
                    unresolved_target = Some(error.to_string());
                    (false, None, None)
                }
            },
            Err(error) => (false, None, Some(error)),
        }
    } else {
        (false, None, None)
    };
    let junction = root.join("current").join(env_type.junction());
    let is_current = current_junction_targets(&junction, &env_dir);
    let mut preview = UninstallPreview {
        name: name.to_owned(),
        env_type,
        is_current,
        is_link_entry,
        is_external_link,
        snapshot_hash: None,
        path_snapshot_hash: None,
        current_link_snapshot_hash: None,
        legacy_links: Vec::new(),
        confirmed_legacy_links: Vec::new(),
        legacy_path_entries: Vec::new(),
        confirmed_legacy_path_entries: Vec::new(),
        installer_plan: None,
        removable: Vec::new(),
        retained: Vec::new(),
        unverified: Vec::new(),
        blocked: Vec::new(),
        scope_complete: link_error.is_none(),
        mode: UninstallMode::Uninstall,
    };

    if is_current {
        match crate::link_ownership::assess(&root, &junction) {
            Ok(crate::link_ownership::Source::Owned(fingerprint)) => {
                preview.current_link_snapshot_hash = Some(fingerprint);
                preview.removable.push(UninstallScopeItem {
                    resource: "当前版本 Junction".into(),
                    path: Some(junction.to_string_lossy().into_owned()),
                    details: "创建来源与链接对象身份一致；仅移除此 current 链接，不删除链接目标。".into(),
                    count: None,
                });
            }
            Ok(crate::link_ownership::Source::Legacy(fingerprint)) => {
                preview.current_link_snapshot_hash = Some(fingerprint);
                preview.legacy_links.push(UninstallScopeItem {
                    resource: "来源未知的 current 链接".into(),
                    path: Some(junction.to_string_lossy().into_owned()), count: None,
                    details: "没有创建来源记录，默认不移除。需单独确认移除此链接入口；所选环境目录的删除范围另见预览。确认不代表它由 EnvCon 创建。".into(),
                });
            }
            Err(error) => {
                preview.scope_complete = false;
                preview.blocked.push(UninstallScopeItem {
                    resource: "当前版本 Junction 来源".into(),
                    path: Some(junction.to_string_lossy().into_owned()), count: None,
                    details: format!("不能确认是 EnvCon 创建的同一链接，保留入口并停止本次移除：{error}。旧链接需另行确认归属。"),
                });
            }
        }
    }
    if is_link_entry {
        match crate::link_ownership::assess(&root, &env_dir) {
            Ok(crate::link_ownership::Source::Owned(fingerprint)) => preview.snapshot_hash = Some(fingerprint),
            Ok(crate::link_ownership::Source::Legacy(fingerprint)) => {
                preview.snapshot_hash = Some(fingerprint);
                preview.legacy_links.push(UninstallScopeItem {
                    resource: "来源未知的管理链接".into(),
                    path: Some(env_dir.to_string_lossy().into_owned()), count: None,
                    details: "没有创建来源记录，默认保留。单独确认后仅移除这个管理入口，保留目标目录；不追溯认领原创建来源。".into(),
                });
            }
            Err(error) => {
                preview.scope_complete = false;
                preview.blocked.push(UninstallScopeItem {
                    resource: "管理链接来源".into(),
                    path: Some(env_dir.to_string_lossy().into_owned()), count: None,
                    details: format!("来源记录缺失或链接身份已变化，保留该入口：{error}。旧链接需另行确认归属。"),
                });
            }
        }
    }

    if let Some(error) = link_error {
        preview.blocked.push(UninstallScopeItem {
            resource: "管理链接目标".into(),
            path: Some(env_dir.to_string_lossy().into_owned()),
            details: format!("无法读取链接目标，不能确认其是否位于受管目录内：{error}"),
            count: None,
        });
    } else if let Some(error) = unresolved_target {
        // The link object itself is readable and removable, but its target is
        // gone. Removing just the link cannot touch any target, so this is the
        // one repair path for a dangling managed entry.
        preview.removable.push(UninstallScopeItem {
            resource: "失效链接入口".into(),
            path: Some(env_dir.to_string_lossy().into_owned()),
            details: format!(
                "链接目标已无法解析（{error}）。只移除该链接对象本身，不会跟随或删除任何目标；这是清理失效条目的唯一安全方式。"
            ),
            count: None,
        });
    } else if is_external_link {
        preview.removable.push(UninstallScopeItem {
            resource: "EnvCon 外部纳管链接".into(),
            path: Some(env_dir.to_string_lossy().into_owned()),
            details: "只移除管理目录中的链接本身，不遍历或删除原安装。".into(),
            count: None,
        });
        let target = link_target.expect("external link target was resolved");
        preview.retained.push(UninstallScopeItem {
            resource: "外部原安装目标".into(),
            path: Some(target.to_string_lossy().into_owned()),
            details: "目标由链接元数据确定；当前操作只移除 EnvCon 链接，原目标保留。".into(),
            count: None,
        });
    } else if is_link_entry {
        preview.removable.push(UninstallScopeItem {
            resource: "EnvCon 受管目录链接".into(),
            path: Some(env_dir.to_string_lossy().into_owned()),
            details: "链接目标位于 EnvCon 受管环境目录内；只移除链接入口，不删除目标目录。".into(),
            count: None,
        });
        if let Some(target) = link_target {
            preview.retained.push(UninstallScopeItem {
                resource: "受管目标目录".into(),
                path: Some(target.to_string_lossy().into_owned()),
                details: "链接目标仍由其他受管条目拥有；本操作不删除目标目录。".into(),
                count: None,
            });
        }
    } else {
        let canonical_env = match env_dir.canonicalize() {
            Ok(path) if !system::paths_eq(&path, &category_root) && system::path_within(&category_root, &path) => path,
            Ok(_) => {
                preview.scope_complete = false;
                preview.blocked.push(UninstallScopeItem {
                    resource: "受管环境目录".into(),
                    path: Some(env_dir.to_string_lossy().into_owned()),
                    details: "实际路径不在所选类型的受管目录内，阻止预览继续。".into(),
                    count: None,
                });
                return Ok(preview);
            }
            Err(error) => {
                preview.scope_complete = false;
                preview.blocked.push(UninstallScopeItem {
                    resource: "受管环境目录".into(),
                    path: Some(env_dir.to_string_lossy().into_owned()),
                    details: format!("无法规范化实际路径，阻止删除范围预览：{error}"),
                    count: None,
                });
                return Ok(preview);
            }
        };
        let mut inventory = TreeInventory::default();
        match inventory_managed_tree(&env_dir, &canonical_env, Path::new(""), &mut inventory) {
            Ok(()) => {
                preview.snapshot_hash = Some(snapshot_hash(&inventory.signature));
                let probed = inventory.files.min(LOCK_PROBE_LIMIT);
                // Sampling is stated as sampling: claiming a complete lock scan
                // of a large tree would be a promise this check cannot keep.
                let probe_note = if inventory.files == 0 {
                    "目录内没有普通文件，未做占用检查。".to_string()
                } else if probed < inventory.files {
                    format!(
                        "已抽样检查前 {probed} 个文件（共 {} 个）是否被占用。",
                        inventory.files
                    )
                } else {
                    format!("已检查全部 {probed} 个文件是否被占用。")
                };
                preview.removable.push(UninstallScopeItem {
                    resource: "EnvCon 受管目录".into(),
                    path: Some(env_dir.to_string_lossy().into_owned()),
                    details: format!(
                        "仅统计不跟随重解析点的目录树；{} 个文件、{} 个目录、{} 个重解析点。{probe_note}",
                        inventory.files, inventory.directories, inventory.reparse_points
                    ),
                    count: Some(inventory.files + inventory.directories + inventory.reparse_points),
                });
                if !inventory.locked.is_empty() {
                    // Report the locks before deletion runs, not only after it
                    // fails. Listing the paths lets the user close the owning
                    // program instead of guessing which file blocked the delete.
                    let shown = inventory
                        .locked
                        .iter()
                        .take(10)
                        .cloned()
                        .collect::<Vec<_>>()
                        .join("、");
                    let more = inventory.locked.len().saturating_sub(10);
                    preview.unverified.push(UninstallScopeItem {
                        resource: "文件占用".into(),
                        path: Some(env_dir.to_string_lossy().into_owned()),
                        details: format!(
                            "{} 个文件当前被其他进程占用，删除会失败：{}{}。请先关闭占用这些文件的程序再执行删除。",
                            inventory.locked.len(),
                            shown,
                            if more > 0 {
                                format!("，另有 {more} 个未列出")
                            } else {
                                String::new()
                            }
                        ),
                        count: Some(inventory.locked.len() as u64),
                    });
                }
                if !inventory.locked_dirs.is_empty() {
                    // A folder someone has open blocks deletion just as a
                    // locked file does, but the remedy is different: usually it
                    // is another program's working directory.
                    let shown = inventory
                        .locked_dirs
                        .iter()
                        .take(10)
                        .cloned()
                        .collect::<Vec<_>>()
                        .join("、");
                    let more = inventory.locked_dirs.len().saturating_sub(10);
                    preview.unverified.push(UninstallScopeItem {
                        resource: LOCKED_DIR_RESOURCE.into(),
                        path: Some(env_dir.to_string_lossy().into_owned()),
                        details: format!(
                            "{} 个目录当前被其他进程打开，删除会失败：{}{}。请先退出占用这些目录的程序（或被设为工作目录的终端）再执行删除。",
                            inventory.locked_dirs.len(),
                            shown,
                            if more > 0 {
                                format!("，另有 {more} 个未列出")
                            } else {
                                String::new()
                            }
                        ),
                        count: Some(inventory.locked_dirs.len() as u64),
                    });
                }
                if inventory.reparse_points > 0 {
                    preview.unverified.push(UninstallScopeItem {
                        resource: "目录内重解析点目标".into(),
                        path: Some(env_dir.to_string_lossy().into_owned()),
                        details: format!(
                            "发现 {} 个链接对象；预览不跟随链接核查目标。删除会作用于受管目录树中的链接对象，不据此扩大到链接目标。",
                            inventory.reparse_points
                        ),
                        count: Some(inventory.reparse_points),
                    });
                }
            }
            Err(error) => {
                preview.scope_complete = false;
                // The traversal stopped, but the reason is often already known:
                // a directory another process holds open can block `read_dir`
                // itself. Naming it beats a bare "could not read".
                if !inventory.locked_dirs.is_empty() {
                    let shown = inventory
                        .locked_dirs
                        .iter()
                        .take(10)
                        .cloned()
                        .collect::<Vec<_>>()
                        .join("、");
                    preview.blocked.push(UninstallScopeItem {
                        resource: LOCKED_DIR_RESOURCE.into(),
                        path: Some(env_dir.to_string_lossy().into_owned()),
                        details: format!(
                            "{} 个目录当前被其他进程打开，连目录内容都无法读取：{shown}。请先退出占用这些目录的程序（或被设为工作目录的终端）再重试。",
                            inventory.locked_dirs.len()
                        ),
                        count: Some(inventory.locked_dirs.len() as u64),
                    });
                }
                preview.blocked.push(UninstallScopeItem {
                    resource: "受管目录树".into(),
                    path: Some(env_dir.to_string_lossy().into_owned()),
                    details: format!("目录树未能完整读取，不能确认影响范围：{error}"),
                    count: None,
                });
            }
        }

        if env_type == EnvType::Python && !system::is_reparse_path(&env_dir.join("Lib")) {
            let site_packages = env_dir.join("Lib").join("site-packages");
            match std::fs::symlink_metadata(&site_packages) {
                Ok(meta) if meta.is_dir() && !is_reparse(&meta) => match std::fs::read_dir(&site_packages) {
                    Ok(mut entries) => {
                        let count_result = entries.try_fold(0u64, |count, entry| {
                            entry.map(|_| count + 1)
                        });
                        match count_result {
                            Ok(package_entries) => preview.removable.push(UninstallScopeItem {
                                resource: "Python 环境内 site-packages".into(),
                                path: Some(site_packages.to_string_lossy().into_owned()),
                                details: "位于所选解释器目录内，随该目录移除；此项只标识环境内包，不代表逐包 pip uninstall。".into(),
                                count: Some(package_entries),
                            }),
                            Err(error) => {
                                preview.scope_complete = false;
                                preview.blocked.push(UninstallScopeItem {
                                    resource: "Python 环境内 site-packages".into(),
                                    path: Some(site_packages.to_string_lossy().into_owned()),
                                    details: format!("包目录有条目无法读取：{error}"),
                                    count: None,
                                });
                            }
                        }
                    }
                    Err(error) => {
                        preview.scope_complete = false;
                        preview.blocked.push(UninstallScopeItem {
                            resource: "Python 环境内 site-packages".into(),
                            path: Some(site_packages.to_string_lossy().into_owned()),
                            details: format!("无法列出环境内包目录：{error}"),
                            count: None,
                        });
                    }
                },
                Ok(meta) if is_reparse(&meta) => preview.unverified.push(UninstallScopeItem {
                    resource: "Python site-packages 链接目标".into(),
                    path: std::fs::read_link(&site_packages).ok().map(|p| p.to_string_lossy().into_owned()),
                    details: "site-packages 是重解析点；只按受管目录中的链接对象处理，不跟随或清理其目标。".into(),
                    count: None,
                }),
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    preview.scope_complete = false;
                    preview.blocked.push(UninstallScopeItem {
                        resource: "Python 环境内 site-packages".into(),
                        path: Some(site_packages.to_string_lossy().into_owned()),
                        details: format!("无法检查环境内包目录：{error}"),
                        count: None,
                    });
                }
            }
        } else if env_type == EnvType::Python {
            preview.unverified.push(UninstallScopeItem {
                resource: "Python Lib 链接目标".into(),
                path: Some(env_dir.join("Lib").to_string_lossy().into_owned()),
                details: "Lib 是重解析点；不遍历其中的 site-packages，不将外部包列为可删除内容。".into(),
                count: None,
            });
        }
    }

    preview.retained.push(if env_type == EnvType::Python {
        let user_scope = python_user_package_scope();
        UninstallScopeItem {
            resource: "Python 用户级包、外部 venv 与共享缓存".into(),
            path: user_scope.path.clone(),
            details: user_scope.retained_detail(),
            count: user_scope.package_count(),
        }
    } else {
        UninstallScopeItem {
            resource: "环境目录外的共享缓存与用户数据".into(),
            path: None,
            details: "未将此环境类型的目录外共享资源纳入删除范围；需要单独识别和确认归属。".into(),
            count: None,
        }
    });
    preview.unverified.extend([
        UninstallScopeItem {
            resource: "PATH 与环境变量".into(),
            path: None,
            details: "没有可证明来源的逐项所有权记录；本预览不读取或修改系统/用户配置。".into(),
            count: None,
        },
        UninstallScopeItem {
            resource: "安装器登记与目录外副作用".into(),
            path: None,
            details: "未查询 Windows 卸载登记，也不会按相似目录名推断原厂卸载器或删除外部资源。".into(),
            count: None,
        },
    ]);
    if is_link_entry && preview.snapshot_hash.is_none() {
        preview.removable.retain(|item| item.path.as_deref() != Some(env_dir.to_string_lossy().as_ref()));
        preview.retained.push(UninstallScopeItem {
            resource: "来源未核验的管理链接入口".into(),
            path: Some(env_dir.to_string_lossy().into_owned()), count: None,
            details: "不认领或删除用户自建、历史无来源记录或已被替换的链接；原目标也保留。".into(),
        });
    }
    preview.removable.retain(|item| !preview.legacy_links.iter().any(|legacy| legacy.path == item.path));
    Ok(preview)
}

fn path_is_absent(path: &Path) -> std::io::Result<bool> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => Ok(false),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(true),
        Err(error) => Err(error),
    }
}

fn validate_uninstall_execution_preview(
    expected: &UninstallPreview,
    current: &UninstallPreview,
) -> Result<()> {
    if !current.scope_complete || !current.blocked.is_empty() {
        return Err(AppError::msg(
            "卸载范围不完整，已阻止执行，请先处理预览中的阻止项",
        ));
    }
    let required: Vec<_> = current.legacy_links.iter().filter_map(|item| item.path.as_ref()).collect();
    let confirmed = &expected.confirmed_legacy_links;
    if confirmed.len() != required.len()
        || required.iter().any(|path| confirmed.iter().filter(|entry| *entry == *path).count() != 1)
    {
        return Err(AppError::msg("来源未知的链接需要逐项单独确认，默认保留；确认不能扩大到预览外的链接"));
    }
    let path_candidates: Vec<_> = current.legacy_path_entries.iter()
        .filter_map(|item| item.path.as_ref()).collect();
    let path_confirmed = &expected.confirmed_legacy_path_entries;
    if path_confirmed.iter().collect::<std::collections::HashSet<_>>().len() != path_confirmed.len()
        || path_confirmed.iter().any(|path| !path_candidates.contains(&path)) {
        return Err(AppError::msg("历史 PATH 确认包含重复或候选外条目"));
    }
    let mut expected_unconfirmed = expected.clone();
    expected_unconfirmed.confirmed_legacy_links.clear();
    expected_unconfirmed.confirmed_legacy_path_entries.clear();
    let mut current_unconfirmed = current.clone();
    current_unconfirmed.confirmed_legacy_links.clear();
    current_unconfirmed.confirmed_legacy_path_entries.clear();
    // Lock state is an observation about the environment, not part of the
    // deletion scope: a program may open a file between preview and execution.
    // Comparing it would reject a perfectly valid preview, so both sides drop
    // it here while the preview keeps reporting it to the user.
    let drop_lock_items = |preview: &mut UninstallPreview| {
        preview.retained.retain(|item| item.resource != LOCK_RESOURCE && item.resource != LOCKED_DIR_RESOURCE);
        preview.unverified.retain(|item| item.resource != LOCK_RESOURCE && item.resource != LOCKED_DIR_RESOURCE);
        preview.removable.retain(|item| item.resource != LOCK_RESOURCE && item.resource != LOCKED_DIR_RESOURCE);
    };
    drop_lock_items(&mut expected_unconfirmed);
    drop_lock_items(&mut current_unconfirmed);
    if expected_unconfirmed != current_unconfirmed {
        return Err(AppError::msg(
            "卸载预览已变化，请重新生成预览后再执行",
        ));
    }
    Ok(())
}

fn current_junction_targets(junction: &Path, env_dir: &Path) -> bool {
    std::fs::read_link(junction).ok().is_some_and(|target| {
        let target = if target.is_absolute() {
            target
        } else {
            junction.parent().unwrap_or_else(|| Path::new(".")).join(target)
        };
        if system::is_reparse_path(env_dir) {
            // Resolve ancestors, but preserve the selected link's own identity.
            let entry_identity = |path: &Path| {
                path.parent()?.canonicalize().ok()
                    .map(|parent| parent.join(path.file_name().unwrap_or_default()))
            };
            entry_identity(&target).zip(entry_identity(env_dir))
                .is_some_and(|(a, b)| system::path_key(&a) == system::path_key(&b))
        } else {
            system::paths_eq(&target, env_dir)
        }
    })
}

fn link_target_path(path: &Path) -> std::io::Result<PathBuf> {
    let target = std::fs::read_link(path)?;
    Ok(if target.is_absolute() {
        target
    } else {
        path.parent().unwrap_or_else(|| Path::new(".")).join(target)
    })
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

#[tauri::command]
pub fn preview_path_repair(
    state: State<'_, AppState>,
    env_type: EnvType,
) -> Result<PathRepairPreview> {
    let root = require_root(&state)?;
    preview_path_repair_with(Some(root.as_path()), env_type, &pathman::get_path_state()?)
}

/// Build the read-only repair preview from an already read PATH state, so
/// fixtures can exercise every branch without touching the registry.
fn preview_path_repair_with(
    root: Option<&Path>,
    env_type: EnvType,
    state: &crate::types::PathState,
) -> Result<PathRepairPreview> {
    let root = root.ok_or_else(|| AppError::msg("尚未设置管理根目录，无法核对受管入口"))?;
    let user_path = state.user.join(";");
    let extensions = crate::pkgtools::command_extensions_for_diagnostics();
    let merged = state
        .system
        .iter()
        .chain(state.user.iter())
        .map(|entry| entry.trim().trim_matches('"').to_string())
        .filter(|entry| !entry.is_empty())
        .collect::<Vec<_>>();

    let mut blocked = Vec::new();
    let mut manual_entries = Vec::new();
    let mut shadowing = Vec::new();
    let mut entry = String::new();
    for command in switch_commands(env_type) {
        let expected = env_type
            .path_entries(root)
            .into_iter()
            .find(|candidate| {
                crate::pkgtools::launcher_exists(candidate, command, &extensions)
            });
        let Some(expected) = expected else {
            continue;
        };
        let resolution = crate::pkgtools::resolve_command_for_diagnostics(
            &merged,
            &expected,
            command,
            &extensions,
        );
        if resolution.status != "shadowed" {
            continue;
        }
        let expected_text = expected.to_string_lossy().into_owned();
        if !crate::detect::system::paths_eq(
            &expected,
            &root.join("current").join(env_type.junction()),
        ) && !env_type
            .path_entries(root)
            .iter()
            .any(|candidate| crate::detect::system::paths_eq(candidate, &expected))
        {
            blocked.push("受管入口不在声明的 PATH 入口内，停止自动修复".into());
            continue;
        }
        let in_user = state
            .user
            .iter()
            .filter(|item| crate::detect::system::path_key(Path::new(item)) == crate::detect::system::path_key(&expected))
            .count();
        let shadow_entry = resolution
            .resolved
            .as_deref()
            .and_then(|path| Path::new(path).parent())
            .map(|path| path.to_string_lossy().into_owned());
        if !in_user_single(in_user) {
            // Not in the user PATH (or duplicated): only manual guidance.
            blocked.push(format!(
                "受管入口 {expected_text} 在用户 PATH 中缺失或重复出现，无法只调整顺序"
            ));
            if let Some(shadow) = &shadow_entry {
                manual_entries.push(shadow.clone());
            }
            continue;
        }
        let Some(shadow_entry) = shadow_entry else {
            blocked.push(format!("{command} 命中的安装目录无法确定，需手动核对"));
            continue;
        };
        if entry.is_empty() {
            entry = expected_text;
        } else if !crate::detect::system::paths_eq(Path::new(&entry), &expected) {
            blocked.push("多个受管入口同时被覆盖，请分别处理".into());
            continue;
        }
        if !shadowing.iter().any(|item: &String| {
            crate::detect::system::path_key(Path::new(item))
                == crate::detect::system::path_key(Path::new(&shadow_entry))
        }) {
            shadowing.push(shadow_entry);
        }
    }

    if entry.is_empty() {
        return Ok(PathRepairPreview {
            env_type: format!("{env_type:?}"),
            entry: String::new(),
            shadowing: Vec::new(),
            mode: "none".into(),
            expected_path: user_path,
            resulting_path: String::new(),
            summary: "当前没有检测到被覆盖的受管命令".into(),
            notes: vec!["修复预览只在切换诊断发现 shadowed 命令时才有意义".into()],
            blocked,
            plan: None,
            manual_entries,
        });
    }
    if shadowing.is_empty() {
        blocked.push("未能确定覆盖受管入口的 PATH 目录，不能自动调整顺序".into());
    }
    if !blocked.is_empty() {
        return Ok(PathRepairPreview {
            env_type: format!("{env_type:?}"),
            entry,
            shadowing,
            mode: "manual".into(),
            expected_path: user_path.clone(),
            resulting_path: String::new(),
            summary: "无法只调整顺序，需要手动处理".into(),
            notes: vec![
                "检测到需要人工处理的冲突；本预览不会修改用户 PATH".into(),
                "手动方式：在系统环境变量中把受管入口移到冲突目录之前，或用“保存 PATH”提交核对后的值".into(),
            ],
            blocked,
            plan: None,
            manual_entries,
        });
    }

    let plan = crate::path_ownership::preview_reorder(&user_path, &entry, &shadowing)?;
    Ok(PathRepairPreview {
        env_type: format!("{env_type:?}"),
        entry,
        shadowing,
        mode: "reorder".into(),
        expected_path: plan.expected_path.clone(),
        resulting_path: plan.resulting_path.clone(),
        summary: format!(
            "把受管入口移到用户 PATH 最前；{} 项冲突目录随后保留",
            plan.shadowing.len()
        ),
        notes: vec![
            "只调整顺序：不新增、不删除、不改写任何条目".into(),
            "需要新终端才能看到效果；本预览本身不写入 PATH".into(),
            "共享系统 PATH 与用户自定义条目保持原样".into(),
        ],
        blocked,
        plan: Some(plan),
        manual_entries,
    })
}

fn in_user_single(count: usize) -> bool {
    count == 1
}

#[tauri::command]
pub fn apply_path_repair(
    state: State<'_, AppState>,
    plan: crate::path_ownership::ReorderPlan,
) -> Result<PathRepairResult> {
    let _query_guard = state.tool_queries.mutation();
    let _scan_guard = state.scans.mutation();
    let backups = state.with_cfg(|c| c.backups_dir());
    let root = require_root(&state)?;
    let applied = pathman::apply_path_reorder(&plan, &backups)?;
    // Re-read the registry after the write so the reported outcome is what a new
    // terminal will actually see, not the plan we intended to apply.
    let after = pathman::get_path_state()?;
    Ok(verify_path_repair(&root, &plan, &applied, &after))
}

/// Post-write verification shared with fixtures. It never writes anything; it
/// only recomputes the resolution from the live PATH.
fn verify_path_repair(
    root: &Path,
    requested: &crate::path_ownership::ReorderPlan,
    applied: &crate::path_ownership::ReorderPlan,
    after: &crate::types::PathState,
) -> PathRepairResult {
    let merged = after
        .system
        .iter()
        .chain(after.user.iter())
        .map(|entry| entry.trim().trim_matches('"').to_string())
        .filter(|entry| !entry.is_empty())
        .collect::<Vec<_>>();
    verify_path_repair_at(root, requested, applied, after, &merged)
}

/// Same verification against an explicit merged PATH order, so fixtures never
/// depend on the host registry.
fn verify_path_repair_at(
    root: &Path,
    requested: &crate::path_ownership::ReorderPlan,
    applied: &crate::path_ownership::ReorderPlan,
    after: &crate::types::PathState,
    merged: &[String],
) -> PathRepairResult {
    let mut problems = Vec::new();
    let live_user_path = after.user.join(";");
    if live_user_path != applied.resulting_path {
        problems.push(format!(
            "用户 PATH 与预期不一致：实际 {live_user_path}"
        ));
    }
    // The managed entry must now be first in the user PATH.
    let first_matches = after
        .user
        .first()
        .is_some_and(|entry| {
            crate::detect::system::path_key(Path::new(entry))
                == crate::detect::system::path_key(Path::new(&applied.entry))
        });
    if !first_matches {
        problems.push("受管入口未位于用户 PATH 首位".into());
    }

    let extensions = crate::pkgtools::command_extensions_for_diagnostics();
    let mut commands = Vec::new();
    for env_type in crate::types::ALL_ENV_TYPES {
        let entry = env_type
            .path_entries(root)
            .into_iter()
            .find(|candidate| {
                crate::detect::system::paths_eq(
                    candidate,
                    Path::new(&applied.entry),
                )
            });
        let Some(entry) = entry else {
            continue;
        };
        for command in switch_commands(env_type) {
            if !crate::pkgtools::launcher_exists(&entry, command, &extensions) {
                continue;
            }
            let resolution = crate::pkgtools::resolve_command_for_diagnostics(
                merged,
                &entry,
                command,
                &extensions,
            );
            commands.push(SwitchCommandDiagnosis {
                command: (*command).into(),
                status: resolution.status.into(),
                expected: entry.to_string_lossy().into_owned(),
                resolved: resolution.resolved,
                detail: resolution.detail,
            });
        }
    }
    let still_shadowed = commands
        .iter()
        .filter(|item| item.status == "shadowed")
        .map(|item| item.command.clone())
        .collect::<Vec<_>>();
    if !still_shadowed.is_empty() {
        problems.push(format!(
            "以下命令仍被覆盖，可能需要新终端或处理系统 PATH：{}",
            still_shadowed.join("、")
        ));
    }

    PathRepairResult {
        applied: applied.clone(),
        verified: problems.is_empty(),
        user_path: live_user_path,
        commands,
        problems,
        requested_path: requested.expected_path.clone(),
        requires_new_terminal: true,
    }
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
                    // 同 clear_readonly：只需清除只读位，不重写整个属性字。
                    #[allow(clippy::permissions_set_readonly_false)]
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
    if system::has_reparse_component(&expected) {
        return Err(AppError::msg("环境分类目录是符号链接或重解析点，禁止递归删除"));
    }
    let current = canonical_root.join("current");
    if system::has_reparse_component(&current) {
        return Err(AppError::msg("current 父目录或其上级是重定向点，禁止修改外部链接"));
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
pub async fn start_install(
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
    state.tasks.start(
        app,
        cfg,
        crate::install::InstallRequest {
            env_type,
            version,
            target_name,
            source,
            url,
        },
    )
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
pub async fn check_app_update() -> app_update::AppUpdateStatus {
    app_update::check_latest().await
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
pub fn inspect_uv_layout(path: String) -> Result<crate::package_managers::UvLegacyLayout> {
    crate::package_managers::inspect_uv_layout(Path::new(&path))
}

#[tauri::command]
pub async fn recover_uv_layout(
    state: State<'_, AppState>,
    expected: crate::package_managers::UvLegacyLayout,
    executable: String,
) -> Result<crate::uv_recovery::UvRecoveryReport> {
    let _query_guard = state.tool_queries.mutation();
    let _scan_guard = state.scans.mutation();
    let _guard = crate::migration::LOCK
        .try_lock()
        .map_err(|_| AppError::msg("正在迁移、重装或修改包管理器路径，请稍后重试"))?;
    crate::uv_recovery::recover_uv_layout(expected, executable).await
}

#[tauri::command]
pub async fn preview_tool_configs(
    state: State<'_, AppState>,
    tools: Vec<crate::pkgtools::ToolConfigRequest>,
) -> Result<crate::pkgtools::ToolConfigBatchPreview> {
    // Validate every identifier before running any manager query.
    for request in &tools {
        crate::package_managers::definition(&request.tool)?;
    }
    let root = state.with_cfg(|c| c.resolve_root());
    let requests = tools
        .into_iter()
        .map(|request| (request.tool, request.global_path, request.cache_path))
        .collect::<Vec<_>>();
    Ok(crate::pkgtools::preview_tool_configs(root.as_deref(), &requests).await)
}

#[tauri::command]
pub async fn preview_tool_config(
    state: State<'_, AppState>,
    tool: String,
    global_path: Option<String>,
    cache_path: Option<String>,
) -> Result<crate::pkgtools::ToolConfigPreview> {
    let root = state.with_cfg(|c| c.resolve_root());
    crate::pkgtools::preview_tool_config(
        root.as_deref(),
        &tool,
        global_path.as_deref(),
        cache_path.as_deref(),
    )
    .await
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
    if matches!(tool.as_str(), "npm" | "pnpm" | "yarn")
        || crate::package_managers::isolated(&tool)
    {
        if let Some(g) = &global_path {
            let g = crate::package_managers::bin_path(&tool, Path::new(g))
                .to_string_lossy()
                .into_owned();
            let backups = state.with_cfg(|c| c.backups_dir());
            let added = pathman::integrate_entries(std::slice::from_ref(&g), &backups)?;
            if !added.is_empty() {
                applied.push(format!("已将 {g} 加入用户 PATH"));
            }
            // Report what a new terminal would actually resolve, so appending a
            // directory is never presented as the command being effective.
            let status =
                crate::pkgtools::path_integration_status(Path::new(&g), Some(&tool));
            applied.push(format!(
                "PATH 生效检查（目录）：{}（{}）",
                status.status, status.detail
            ));
            applied.push(format!(
                "命令解析：{}",
                if status.command_status == "managed" {
                    format!(
                        "{} 会命中受管入口 {}",
                        status.command,
                        status.resolved_command.as_deref().unwrap_or(&g)
                    )
                } else {
                    status.command_detail.clone()
                }
            ));
            let shadowed = status
                .launchers
                .iter()
                .filter(|launcher| launcher.status == "shadowed")
                .take(3)
                .map(|launcher| {
                    format!(
                        "{} → {}",
                        launcher.name,
                        launcher.resolved.as_deref().unwrap_or("未知")
                    )
                })
                .collect::<Vec<_>>();
            if !shadowed.is_empty() {
                applied.push(format!(
                    "受管入口内的命令被 PATH 靠前位置覆盖：{}",
                    shadowed.join("；")
                ));
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
    let version = tool_version(&state, &tool).await;
    let capabilities = crate::migration::Capabilities::for_tool(&tool, version.as_deref());
    tokio::task::spawn_blocking(move || {
        crate::migration::preview(&root, &tool, &kind, Path::new(&source), capabilities)
    })
    .await
    .map_err(|e| AppError::msg(format!("迁移预览失败: {e}")))?
}

/// Resolve the version of the manager that will actually run, so version-dependent
/// semantics (Yarn Classic vs Berry) are not guessed from the static catalog.
async fn tool_version(state: &AppState, tool: &str) -> Option<String> {
    let root = state.with_cfg(|c| c.resolve_root());
    let inv = crate::pkgtools::find_invocation(root.as_deref(), tool).await?;
    let output = crate::pkgtools::run(&inv, &["--version"], 15).await.ok()?;
    crate::pkgtools::display_version(&output)
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
    let version = tool_version(&state, &plan.tool).await;
    let capabilities =
        crate::migration::Capabilities::for_tool(&plan.tool, version.as_deref());
    tokio::task::spawn_blocking(move || {
        // Hold the lock in the worker even if the invoking webview is closed.
        let _guard = guard;
        crate::migration::execute(&root, &plan, capabilities)
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
    use crate::types::ManagedEnv;

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
    fn path_repair_result_requires_the_live_path_to_match_and_clear_shadowing() {
        use crate::test_support::TestDir;
        let dir = TestDir::new();
        let root = dir.path().join("managed");
        let entry = root.join("current").join("node");
        std::fs::create_dir_all(&entry).unwrap();
        std::fs::write(entry.join("node.exe"), b"fixture").unwrap();
        let old = dir.path().join("old-nodejs");
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("node.exe"), b"fixture").unwrap();

        let plan = crate::path_ownership::preview_reorder(
            &format!("{};{}", old.display(), entry.display()),
            &entry.to_string_lossy(),
            &[old.to_string_lossy().into_owned()],
        )
        .unwrap();
        // Live PATH still has the old order: the repair did not take effect.
        let stale = crate::types::PathState {
            user: vec![
                old.to_string_lossy().into_owned(),
                entry.to_string_lossy().into_owned(),
            ],
            system: Vec::new(),
        };
        let stale_merged = vec![
            old.to_string_lossy().into_owned(),
            entry.to_string_lossy().into_owned(),
        ];
        let failed =
            super::verify_path_repair_at(&root, &plan, &plan, &stale, &stale_merged);
        assert!(!failed.verified);
        assert!(failed
            .problems
            .iter()
            .any(|problem| problem.contains("未位于用户 PATH 首位")));
        assert!(failed.requires_new_terminal);

        // Live PATH matches the plan and nothing shadows the command any more.
        let applied = crate::types::PathState {
            user: vec![
                entry.to_string_lossy().into_owned(),
                old.to_string_lossy().into_owned(),
            ],
            system: Vec::new(),
        };
        let ok = super::verify_path_repair_at(
            &root,
            &plan,
            &plan,
            &applied,
            &[
                entry.to_string_lossy().into_owned(),
                old.to_string_lossy().into_owned(),
            ],
        );
        assert!(ok.verified, "unexpected problems: {:?}", ok.problems);
        assert!(ok.problems.is_empty());
        assert_eq!(ok.user_path, applied.user.join(";"));
        assert_eq!(ok.requested_path, plan.expected_path);
        // The reorder result is what a user would compare against.
        assert_eq!(ok.applied.resulting_path, plan.resulting_path);
    }

    #[test]
    fn path_repair_preview_requires_the_managed_entry_in_the_user_path() {
        let dir = crate::test_support::TestDir::new();
        let root = dir.path().join("managed");
        let entry = root.join("current").join("node").to_string_lossy().into_owned();
        let old = dir.path().join("old-nodejs").to_string_lossy().into_owned();
        // The preview only inspects PATH text, but it does need a launcher to
        // know which declared entry provides `node`.
        std::fs::create_dir_all(root.join("current").join("node")).unwrap();
        std::fs::write(root.join("current").join("node").join("node.exe"), b"fixture").unwrap();
        // The earlier installation must really provide node.exe, otherwise the
        // resolver reports `missing` instead of `shadowed`.
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(Path::new(&old).join("node.exe"), b"fixture").unwrap();
        let state = crate::types::PathState {
            user: vec![old.clone(), entry.clone()],
            system: Vec::new(),
        };
        let preview = super::preview_path_repair_with(Some(&root), EnvType::Node, &state).unwrap();
        assert_eq!(preview.entry, entry);
        assert_eq!(preview.shadowing, vec![old]);
        assert_eq!(preview.mode, "reorder");
        // The plan is bound to the exact PATH text it was built from.
        assert_eq!(preview.expected_path, state.user.join(";"));
        assert!(preview.plan.is_some());
        assert!(preview.blocked.is_empty());

        // A managed entry that is not in the user PATH cannot be reordered:
        // the preview must not invent an ordering or claim success.
        let absent = crate::types::PathState {
            user: vec![dir.path().join("old-nodejs").to_string_lossy().into_owned()],
            system: Vec::new(),
        };
        let manual =
            super::preview_path_repair_with(Some(&root), EnvType::Node, &absent).unwrap();
        assert_eq!(manual.mode, "none");
        assert!(manual.plan.is_none());
        assert!(manual.resulting_path.is_empty());
    }

    #[test]
    fn path_repair_preview_reports_nothing_to_fix_without_a_shadowed_command() {
        let dir = crate::test_support::TestDir::new();
        let root = dir.path().join("managed");
        let state = crate::types::PathState {
            user: vec![dir.path().join("unrelated").to_string_lossy().into_owned()],
            system: Vec::new(),
        };
        let preview = super::preview_path_repair_with(Some(&root), EnvType::Node, &state).unwrap();
        assert_eq!(preview.mode, "none");
        assert!(preview.plan.is_none());
        assert!(preview.blocked.is_empty());
        assert!(preview.notes.iter().any(|note| note.contains("shadowed")));
        // Nothing was written or created.
        assert!(!root.exists());
    }

    #[test]
    fn switch_diagnosis_lists_commands_for_every_environment_type() {
        for env_type in crate::types::ALL_ENV_TYPES {
            let commands = super::switch_commands(env_type);
            assert!(!commands.is_empty(), "{env_type:?} has no diagnostic command");
            assert!(
                commands.iter().all(|command| !command.is_empty()
                    && !command.contains(['\\', '/', ' '])),
                "{env_type:?} has an invalid command name"
            );
            assert!(!env_type.path_entries(std::path::Path::new("D:/root")).is_empty());
        }
    }

    #[test]
    fn switch_diagnosis_resolution_distinguishes_shadowed_and_missing_commands() {
        use crate::test_support::TestDir;
        let dir = TestDir::new();
        let expected = dir.path().join("managed/current/node");
        let older = dir.path().join("older/nodejs");
        std::fs::create_dir_all(&expected).unwrap();
        std::fs::create_dir_all(&older).unwrap();
        // Both installations provide node.exe; only the earlier one wins.
        std::fs::write(expected.join("node.exe"), b"fixture").unwrap();
        std::fs::write(older.join("node.exe"), b"fixture").unwrap();
        let extensions = vec![".EXE".into(), ".CMD".into()];
        let entries = vec![
            older.to_string_lossy().into_owned(),
            expected.to_string_lossy().into_owned(),
        ];

        let shadowed = super::super::pkgtools::resolve_command_for_diagnostics(
            &entries,
            &expected,
            "node",
            &extensions,
        );
        assert_eq!(shadowed.status, "shadowed");
        assert!(shadowed
            .resolved
            .as_deref()
            .is_some_and(|path| path.to_ascii_lowercase().contains("older")));

        // Put the managed entry first: the same files now resolve as managed.
        let reordered = vec![
            expected.to_string_lossy().into_owned(),
            older.to_string_lossy().into_owned(),
        ];
        let managed = super::super::pkgtools::resolve_command_for_diagnostics(
            &reordered,
            &expected,
            "node",
            &extensions,
        );
        assert_eq!(managed.status, "managed");

        // An absent command must not be reported as resolved.
        let missing = super::super::pkgtools::resolve_command_for_diagnostics(
            &reordered,
            &expected,
            "npm",
            &extensions,
        );
        assert_eq!(missing.status, "missing");
        assert!(missing.resolved.is_none());
        // Resolution is read-only: both launchers still exist untouched.
        assert!(expected.join("node.exe").is_file());
        assert!(older.join("node.exe").is_file());
    }

    #[test]
    fn python_user_scope_counts_distributions_without_claiming_ownership() {        let dir = crate::test_support::TestDir::new();
        let base = dir.path().join("userbase");
        let site_314 = base.join("Python314/site-packages");
        let site_313 = base.join("Python313/site-packages");
        std::fs::create_dir_all(site_314.join("requests-2.32.0.dist-info")).unwrap();
        std::fs::create_dir_all(site_314.join("legacy-1.0.egg-info")).unwrap();
        std::fs::create_dir_all(site_314.join("requests")).unwrap();
        std::fs::create_dir_all(&site_313).unwrap();

        let scope = super::python_user_package_scope_at(Ok(base.clone()));
        assert_eq!(scope.packages, Some(2), "two distribution entries");
        assert!(scope.shared, "two interpreter directories share one base");
        assert_eq!(scope.package_count(), Some(2));
        assert_eq!(scope.path.as_deref(), Some(base.to_string_lossy().as_ref()));
        let detail = scope.retained_detail();
        assert!(detail.contains("发现 2 个用户级分发包"));
        assert!(detail.contains("多个 Python 版本共用"));
        assert!(detail.contains("不删除任何用户级包"));
        // Reading must leave the packages untouched.
        assert!(site_314.join("requests").is_dir());
    }

    #[test]
    fn python_user_scope_reports_unresolvable_or_relative_bases_instead_of_guessing() {
        let relative = super::python_user_base_from(Some("relative/Python".into()), None);
        assert!(relative.is_err());
        let scope = super::python_user_package_scope_at(relative);
        assert!(scope.path.is_none());
        assert_eq!(scope.packages, None);
        assert!(scope.reason.contains("不是绝对路径"));
        assert!(scope.retained_detail().contains("未读取用户级包"));

        let missing = super::python_user_base_from(None, None);
        assert!(missing.is_err());
        assert!(super::python_user_package_scope_at(missing)
            .reason
            .contains("APPDATA"));

        // An absolute PYTHONUSERBASE takes precedence over APPDATA.
        let resolved = super::python_user_base_from(
            Some(r"D:\custom-userbase".into()),
            Some(std::ffi::OsString::from(r"C:\Users\me\AppData\Roaming")),
        )
        .unwrap();
        assert_eq!(resolved, PathBuf::from(r"D:\custom-userbase"));

        let fallback = super::python_user_base_from(
            None,
            Some(std::ffi::OsString::from(r"C:\Users\me\AppData\Roaming")),
        )
        .unwrap();
        assert!(fallback.ends_with("Python"));
    }

    #[test]
    fn python_user_scope_does_not_report_an_empty_read_as_zero_when_unreadable() {
        let dir = crate::test_support::TestDir::new();
        // A file where a directory is expected makes the per-version read fail.
        let base = dir.path().join("userbase");
        std::fs::create_dir_all(&base).unwrap();
        std::fs::write(base.join("Python314"), b"not a directory").unwrap();

        let scope = super::python_user_package_scope_at(Ok(base));
        // No versioned site-packages was found, so the base is reported as empty
        // rather than inventing packages.
        assert_eq!(scope.packages, Some(0));
        assert!(scope.retained_detail().contains("未发现分发包"));
    }

    #[tokio::test]
    async fn uninstall_preview_separates_owned_python_packages_from_unverified_external_data() {        let dir = crate::test_support::TestDir::new();
        let root = dir.path().join("managed");
        let python = root.join("envs/pythons/python-3.14");
        let other_python = root.join("envs/pythons/python-3.13");
        let site_packages = python.join("Lib/site-packages");
        std::fs::create_dir_all(&site_packages).unwrap();
        std::fs::write(python.join("python.exe"), b"fixture").unwrap();
        std::fs::create_dir_all(&other_python).unwrap();
        std::fs::write(other_python.join("python.exe"), b"other fixture").unwrap();
        std::fs::create_dir_all(site_packages.join("demo-1.0.dist-info")).unwrap();
        std::fs::write(site_packages.join("demo.py"), b"fixture").unwrap();
        let user_site = dir.path().join("userbase/Python/site-packages");
        let external_venv = dir.path().join("project/.venv");
        let shared_cache = dir.path().join("pip-cache");
        for path in [&user_site, &external_venv, &shared_cache] {
            std::fs::create_dir_all(path).unwrap();
            std::fs::write(path.join("keep.txt"), b"keep").unwrap();
        }
        std::fs::create_dir_all(root.join("current")).unwrap();
        switcher::switch(&root, EnvType::Python, "python-3.14").unwrap();

        let preview = preview_uninstall_scope(&root, EnvType::Python, "python-3.14").unwrap();
        assert!(preview.scope_complete);
        assert!(preview.is_current);
        assert!(preview.blocked.is_empty());
        assert_eq!(preview.name, "python-3.14");
        assert!(preview.removable.iter().any(|item| item.resource == "EnvCon 受管目录"));
        assert_eq!(
            preview.removable.iter().find(|item| item.resource == "Python 环境内 site-packages").and_then(|item| item.count),
            Some(2)
        );
        assert_eq!(preview.retained.len(), 1);
        assert!(preview.retained[0].resource.contains("用户级包、外部 venv"));
        assert!(preview.unverified.iter().any(|item| item.resource == "PATH 与环境变量"));
        assert!(preview.unverified.iter().any(|item| item.resource == "安装器登记与目录外副作用"));
        assert!(site_packages.join("demo.py").is_file());
        assert!(user_site.join("keep.txt").is_file());
        assert!(external_venv.join("keep.txt").is_file());
        assert!(shared_cache.join("keep.txt").is_file());

        let non_current = preview_uninstall_scope(&root, EnvType::Python, "python-3.13").unwrap();
        assert!(!non_current.is_current);
        assert_eq!(non_current.name, "python-3.13");
        assert!(non_current.removable.iter().any(|item| {
            item.resource == "EnvCon 受管目录"
                && item.path.as_deref().is_some_and(|path| system::paths_eq(Path::new(path), &other_python))
        }));
        assert!(python.join("python.exe").is_file());

        let result = execute_uninstall_scope(
            root.clone(),
            EnvType::Python,
            "python-3.14".into(),
            preview,
        )
        .await
        .unwrap();
        assert_eq!(result.status, UninstallResultStatus::VerifiedScopeRemoved);
        assert!(std::fs::symlink_metadata(&python).is_err());
        assert!(other_python.join("python.exe").is_file());
        assert!(user_site.join("keep.txt").is_file());
        assert!(external_venv.join("keep.txt").is_file());
        assert!(shared_cache.join("keep.txt").is_file());
    }

    #[tokio::test]
    async fn uninstall_python_lib_link_preserves_external_packages() {
        let dir = crate::test_support::TestDir::new();
        let root = dir.path().join("managed");
        let env = root.join("envs/pythons/python-fixture");
        let library = dir.path().join("external-lib");
        std::fs::create_dir_all(&env).unwrap();
        std::fs::create_dir_all(library.join("site-packages")).unwrap();
        std::fs::write(library.join("site-packages/keep.py"), b"keep").unwrap();
        junction::create(&library, env.join("Lib")).unwrap();

        let preview = preview_uninstall_scope(&root, EnvType::Python, "python-fixture").unwrap();
        assert!(preview.scope_complete);
        assert!(!preview.removable.iter().any(|item| item.resource == "Python 环境内 site-packages"));
        assert!(preview.unverified.iter().any(|item| item.resource == "Python Lib 链接目标"));
        let result = execute_uninstall_scope(root, EnvType::Python, "python-fixture".into(), preview)
            .await.unwrap();
        assert_eq!(result.status, UninstallResultStatus::VerifiedScopeRemoved);
        assert!(!env.exists());
        assert_eq!(std::fs::read(library.join("site-packages/keep.py")).unwrap(), b"keep");
    }

    #[test]
    fn uninstall_snapshot_is_stable_and_detects_same_count_changes() {
        let dir = crate::test_support::TestDir::new();
        let env = dir.path().join("envs/nodes/node-fixture");
        std::fs::create_dir_all(env.join("nested")).unwrap();
        for name in ["z.txt", "a.txt", "nested/second.txt", "nested/first.txt"] {
            std::fs::write(env.join(name), b"fixture").unwrap();
        }
        let preview = || preview_uninstall_scope(dir.path(), EnvType::Node, "node-fixture").unwrap();
        let before = preview();
        assert_eq!(before, preview());
        std::fs::write(env.join("a.txt"), b"changed size").unwrap();
        let after = preview();
        assert_eq!(before.removable, after.removable);
        assert_ne!(before.snapshot_hash, after.snapshot_hash);
        assert!(validate_uninstall_execution_preview(&before, &after).is_err());
    }

    struct UninstallPathFixture {
        value: String,
        writes: usize,
        reads: usize,
        fail_write: bool,
        fail_read_at: Option<usize>,
        create_peer_at: Option<(usize, PathBuf)>,
    }

    impl crate::path_ownership::PathStore for UninstallPathFixture {
        fn read(&mut self) -> Result<String> {
            self.reads += 1;
            if let Some((at, path)) = &self.create_peer_at {
                if self.reads == *at { std::fs::create_dir_all(path)?; }
            }
            if self.fail_read_at == Some(self.reads) {
                return Err(AppError::msg("fixture PATH read denied"));
            }
            Ok(self.value.clone())
        }
        fn write(&mut self, value: &str) -> Result<()> {
            self.writes += 1;
            if self.fail_write { return Err(AppError::msg("fixture PATH write denied")); }
            self.value = value.into();
            Ok(())
        }
    }

    fn uninstall_path_fixture(
        temp: &Path, kind: EnvType, owned: bool,
    ) -> (PathBuf, PathBuf, PathBuf, Result<UninstallPathFixture>) {
        let root = temp.join("managed");
        let env = root.join("envs").join(kind.folder()).join("fixture");
        std::fs::create_dir_all(&env).unwrap();
        std::fs::create_dir_all(root.join("current")).unwrap();
        std::fs::write(env.join("payload"), b"fixture").unwrap();
        switcher::switch(&root, kind, "fixture").unwrap();
        let journal = temp.join("ownership/journal.json");
        let entries: Vec<_> = kind.path_entries(&root).iter().map(|p| p.to_string_lossy().into_owned()).collect();
        let mut backend = UninstallPathFixture {
            value: r"C:\user;;C:\keep;".into(), writes: 0, reads: 0,
            fail_write: false, fail_read_at: None, create_peer_at: None,
        };
        if owned {
            crate::path_ownership::apply(&mut backend, &journal, crate::path_ownership::Change::Append(&entries)).unwrap();
        } else {
            backend.value.push_str(&entries.join(";"));
        }
        backend.reads = 0;
        backend.writes = 0;
        (root, env, journal, Ok(backend))
    }

    #[tokio::test]
    async fn uninstall_path_all_environment_types_remove_only_verified_entries() {
        for kind in ALL_ENV_TYPES {
            let dir = crate::test_support::TestDir::new();
            let (root, env, journal, mut store) = uninstall_path_fixture(dir.path(), kind, true);
            let (preview, _) = preview_uninstall_with_path(&root, kind, "fixture", &journal, &mut store).unwrap();
            assert_eq!(preview.removable.iter().filter(|item| item.resource == OWNED_PATH_RESOURCE).count(), kind.path_entries(&root).len());
            assert!(preview.path_snapshot_hash.is_some());
            let result = execute_uninstall_with_path(root, kind, "fixture".into(), preview, &journal, &mut store).await.unwrap();
            assert_eq!(result.status, UninstallResultStatus::VerifiedScopeRemoved);
            assert!(!env.exists());
            assert_eq!(store.as_ref().unwrap().value, r"C:\user;;C:\keep");
            assert_eq!(store.as_ref().unwrap().writes, 1);
            assert!(result.removed.iter().any(|item| item.resource == OWNED_PATH_RESOURCE));
        }
    }

    #[tokio::test]
    async fn uninstall_path_keeps_user_entries_shared_versions_and_external_current() {
        for scenario in ["user", "sibling", "external-current"] {
            let dir = crate::test_support::TestDir::new();
            let (root, env, journal, mut store) = uninstall_path_fixture(dir.path(), EnvType::Node, scenario != "user");
            if scenario == "sibling" {
                std::fs::create_dir_all(env.parent().unwrap().join("other")).unwrap();
            } else if scenario == "external-current" {
                let external = dir.path().join("external");
                std::fs::create_dir(&external).unwrap();
                std::fs::remove_dir(root.join("current/node")).unwrap();
                junction::create(&external, root.join("current/node")).unwrap();
            }
            let before = store.as_ref().unwrap().value.clone();
            let (preview, _) = preview_uninstall_with_path(&root, EnvType::Node, "fixture", &journal, &mut store).unwrap();
            assert!(if scenario == "user" { !preview.legacy_path_entries.is_empty() } else {
                preview.retained.iter().any(|item| item.resource == OWNED_PATH_RESOURCE)
            });
            assert!(!preview.removable.iter().any(|item| item.resource == OWNED_PATH_RESOURCE));
            let result = execute_uninstall_with_path(root, EnvType::Node, "fixture".into(), preview, &journal, &mut store).await.unwrap();
            assert_eq!(result.status, UninstallResultStatus::VerifiedScopeRemoved);
            assert_eq!(store.as_ref().unwrap().value, before);
            assert_eq!(store.as_ref().unwrap().writes, 0);
        }
    }

    #[tokio::test]
    async fn uninstall_legacy_path_requires_exact_confirmation_and_records_user_consent() {
        let dir = crate::test_support::TestDir::new();
        let (root, env, journal, mut store) = uninstall_path_fixture(dir.path(), EnvType::Node, false);
        let (preview, _) = preview_uninstall_with_path(&root, EnvType::Node, "fixture", &journal, &mut store).unwrap();
        let legacy: Vec<_> = preview.legacy_path_entries.iter()
            .filter_map(|item| item.path.clone()).collect();
        assert!(!legacy.is_empty());
        assert_eq!(store.as_ref().unwrap().writes, 0);
        assert!(!preview.removable.iter().any(|item| item.resource == OWNED_PATH_RESOURCE));
        assert!(preview.confirmed_legacy_path_entries.is_empty());

        let mut forged = preview.clone();
        forged.confirmed_legacy_path_entries = vec![r"C:\outside".into()];
        assert!(execute_uninstall_with_path(
            root.clone(), EnvType::Node, "fixture".into(), forged, &journal, &mut store,
        ).await.is_err());
        assert!(env.exists());
        assert_eq!(store.as_ref().unwrap().writes, 0);

        let mut confirmed = preview;
        confirmed.confirmed_legacy_path_entries = legacy.clone();
        let result = execute_uninstall_with_path(
            root, EnvType::Node, "fixture".into(), confirmed, &journal, &mut store,
        ).await.unwrap();
        assert_eq!(result.status, UninstallResultStatus::VerifiedScopeRemoved);
        assert!(result.removed.iter().any(|item|
            item.resource == "用户确认移除的历史 PATH 条目" && legacy.contains(&item.path.clone().unwrap())
        ));
        assert!(!store.as_ref().unwrap().value.split(';').any(|entry| legacy.contains(&entry.to_owned())));
        let audit: serde_json::Value = serde_json::from_slice(&std::fs::read(&journal).unwrap()).unwrap();
        let record = audit["records"].as_array().unwrap().last().unwrap();
        assert_eq!(record["action"], "confirmed-legacy-cleanup");
        assert_eq!(record["confirmationStatus"], "confirmed-removed");
        assert_eq!(record["confirmedLegacy"], serde_json::json!(legacy));
        assert_eq!(record["owned"], serde_json::json!([]));
    }

    #[tokio::test]
    async fn uninstall_legacy_path_shared_or_changed_after_preview_is_preserved() {
        let dir = crate::test_support::TestDir::new();
        let (root, env, journal, mut store) = uninstall_path_fixture(dir.path(), EnvType::Node, false);
        let shared = env.parent().unwrap().join("other");
        std::fs::create_dir_all(&shared).unwrap();
        let (preview, _) = preview_uninstall_with_path(&root, EnvType::Node, "fixture", &journal, &mut store).unwrap();
        assert!(preview.legacy_path_entries.is_empty());
        assert!(preview.retained.iter().any(|item|
            item.resource == OWNED_PATH_RESOURCE && item.details.contains("其他环境依赖")
        ));
        let before = store.as_ref().unwrap().value.clone();
        let result = execute_uninstall_with_path(
            root, EnvType::Node, "fixture".into(), preview, &journal, &mut store,
        ).await.unwrap();
        assert_eq!(result.status, UninstallResultStatus::VerifiedScopeRemoved);
        assert_eq!(store.as_ref().unwrap().value, before);
        assert_eq!(store.as_ref().unwrap().writes, 0);
        assert!(!env.exists());

        let dir = crate::test_support::TestDir::new();
        let (root, env, journal, mut store) = uninstall_path_fixture(dir.path(), EnvType::Node, false);
        let (mut preview, _) = preview_uninstall_with_path(&root, EnvType::Node, "fixture", &journal, &mut store).unwrap();
        preview.confirmed_legacy_path_entries = preview.legacy_path_entries.iter()
            .filter_map(|item| item.path.clone()).collect();
        store.as_mut().unwrap().value.push_str(r";C:\external");
        assert!(execute_uninstall_with_path(
            root, EnvType::Node, "fixture".into(), preview, &journal, &mut store,
        ).await.is_err());
        assert!(env.exists());
        assert_eq!(store.as_ref().unwrap().writes, 0);
    }

    #[tokio::test]
    async fn uninstall_legacy_path_write_or_recheck_failure_is_audited_as_partial() {
        for mode in ["write", "recheck", "late-peer"] {
        let dir = crate::test_support::TestDir::new();
        let (root, env, journal, mut store) = uninstall_path_fixture(dir.path(), EnvType::Node, false);
        let (mut preview, _) = preview_uninstall_with_path(&root, EnvType::Node, "fixture", &journal, &mut store).unwrap();
        preview.confirmed_legacy_path_entries = preview.legacy_path_entries.iter()
            .filter_map(|item| item.path.clone()).collect();
        let backend = store.as_mut().unwrap();
        let before = backend.value.clone();
        backend.reads = 0;
        backend.fail_write = mode == "write";
        // Execution preview, post-removal plan, apply before, prewrite, postwrite.
        if mode == "recheck" { backend.fail_read_at = Some(5); }
        if mode == "late-peer" { backend.create_peer_at = Some((4, env.parent().unwrap().join("new"))); }
        let result = execute_uninstall_with_path(
            root, EnvType::Node, "fixture".into(), preview, &journal, &mut store,
        ).await.unwrap();
        assert_eq!(result.status, UninstallResultStatus::Partial);
        assert!(result.failed.iter().any(|item|
            item.resource == "用户确认移除的历史 PATH 条目"
        ));
        let audit: serde_json::Value = serde_json::from_slice(&std::fs::read(&journal).unwrap()).unwrap();
        let record = audit["records"].as_array().unwrap().last().unwrap();
        assert_eq!(record["confirmationStatus"], "unverified");
        assert_eq!(record["status"], "unverified");
        assert_eq!(record["owned"], serde_json::json!([]));
        assert!(!env.exists());
        if mode == "recheck" {
            assert_eq!(store.as_ref().unwrap().value, r"C:\user;;C:\keep");
        } else {
            assert_eq!(store.as_ref().unwrap().value, before);
        }
        if mode == "late-peer" { assert_eq!(store.as_ref().unwrap().writes, 0); }
        assert!(!result.removed.iter().any(|item| item.resource.contains("PATH")));
        }
    }

    #[tokio::test]
    async fn uninstall_legacy_path_all_types_keep_unselected_and_remove_only_confirmed_strings() {
        for kind in ALL_ENV_TYPES {
            for select_one in [false, true] {
                let dir = crate::test_support::TestDir::new();
                let (root, env, journal, mut store) = uninstall_path_fixture(dir.path(), kind, false);
                let (mut preview, _) = preview_uninstall_with_path(&root, kind, "fixture", &journal, &mut store).unwrap();
                let candidates: Vec<_> = preview.legacy_path_entries.iter().map(|item| item.path.clone().unwrap()).collect();
                assert_eq!(candidates.len(), kind.path_entries(&root).len());
                if select_one { preview.confirmed_legacy_path_entries = vec![candidates[0].clone()]; }
                let before = store.as_ref().unwrap().value.clone();
                let expected_path = before.split(';').filter(|entry| !select_one || *entry != candidates[0])
                    .collect::<Vec<_>>().join(";");
                let result = execute_uninstall_with_path(root, kind, "fixture".into(), preview, &journal, &mut store).await.unwrap();
                assert_eq!(result.status, UninstallResultStatus::VerifiedScopeRemoved);
                assert!(!env.exists());
                assert_eq!(store.as_ref().unwrap().value, expected_path);
                assert_eq!(store.as_ref().unwrap().writes, usize::from(select_one));
                for (index, entry) in candidates.iter().enumerate() {
                    if select_one && index == 0 {
                        assert!(result.removed.iter().any(|item| item.resource == "用户确认移除的历史 PATH 条目" && item.path.as_ref() == Some(entry)));
                        assert!(!result.retained.iter().any(|item| item.path.as_ref() == Some(entry)));
                    } else {
                        assert!(result.retained.iter().any(|item| item.resource == "来源未知的用户 PATH 条目" && item.path.as_ref() == Some(entry)));
                        assert!(!result.removed.iter().any(|item|
                            item.resource == "用户确认移除的历史 PATH 条目"
                                && item.path.as_ref() == Some(entry)
                        ));
                    }
                }
            }
        }
    }

    #[tokio::test]
    async fn uninstall_legacy_path_rejects_duplicate_foreign_and_rewritten_confirmations() {
        let dir = crate::test_support::TestDir::new();
        let (root, env, journal, mut store) = uninstall_path_fixture(dir.path(), EnvType::Python, false);
        let (preview, _) = preview_uninstall_with_path(&root, EnvType::Python, "fixture", &journal, &mut store).unwrap();
        let candidate = preview.legacy_path_entries[0].path.clone().unwrap();
        for invalid in [vec![candidate.clone(), candidate.clone()], vec![r"C:\outside".into()], vec![format!("{candidate}\\")]] {
            let mut forged = preview.clone();
            forged.confirmed_legacy_path_entries = invalid;
            assert!(execute_uninstall_with_path(root.clone(), EnvType::Python, "fixture".into(), forged, &journal, &mut store).await.is_err());
            assert!(env.join("payload").exists());
            assert!(root.join("current/python").exists());
            assert_eq!(store.as_ref().unwrap().writes, 0);
        }
    }

    #[tokio::test]
    async fn uninstall_legacy_path_rechecks_source_history_even_when_path_value_is_unchanged() {
        for change in ["manual", "observe", "pending", "damaged", "peer"] {
            let dir = crate::test_support::TestDir::new();
            let (root, env, journal, mut store) = uninstall_path_fixture(dir.path(), EnvType::Node, false);
            let (mut preview, _) = preview_uninstall_with_path(&root, EnvType::Node, "fixture", &journal, &mut store).unwrap();
            preview.confirmed_legacy_path_entries = preview.legacy_path_entries.iter().map(|item| item.path.clone().unwrap()).collect();
            let before = store.as_ref().unwrap().value.clone();
            match change {
                "manual" | "observe" => {
                    let entries: Vec<_> = before.split(';').map(str::to_owned).collect();
                    crate::path_ownership::apply(store.as_mut().unwrap(), &journal, crate::path_ownership::Change::Replace(&entries)).unwrap();
                    if change == "observe" {
                        crate::path_ownership::apply(store.as_mut().unwrap(), &journal, crate::path_ownership::Change::Append(&[])).unwrap();
                    }
                }
                "peer" => std::fs::create_dir_all(env.parent().unwrap().join("new")).unwrap(),
                _ => {
                    std::fs::create_dir_all(journal.parent().unwrap()).unwrap();
                    let bytes = if change == "pending" {
                        serde_json::to_vec(&serde_json::json!({ "schema": 1, "records": [{
                            "action": "integrate", "before": "", "after": before, "owned": [], "status": "pending"
                        }]})).unwrap()
                    } else { b"damaged".to_vec() };
                    std::fs::write(&journal, bytes).unwrap();
                }
            }
            store.as_mut().unwrap().writes = 0;
            assert!(execute_uninstall_with_path(root.clone(), EnvType::Node, "fixture".into(), preview, &journal, &mut store).await.is_err());
            assert!(env.join("payload").exists(), "{change}");
            assert!(root.join("current/node").exists());
            assert_eq!(store.as_ref().unwrap().value, before);
            assert_eq!(store.as_ref().unwrap().writes, 0);
        }
    }

    #[tokio::test]
    async fn uninstall_legacy_path_duplicate_aliases_cannot_be_confirmed_or_removed() {
        let dir = crate::test_support::TestDir::new();
        let (root, env, journal, mut store) = uninstall_path_fixture(dir.path(), EnvType::Node, false);
        let entry = root.join("current/node").to_string_lossy().into_owned();
        store.as_mut().unwrap().value.push_str(&format!(";{}\\", entry.to_uppercase()));
        let (preview, _) = preview_uninstall_with_path(&root, EnvType::Node, "fixture", &journal, &mut store).unwrap();
        assert!(preview.legacy_path_entries.is_empty());
        assert!(preview.retained.iter().any(|item| item.details.contains("重复")));
        let mut forged = preview.clone();
        forged.confirmed_legacy_path_entries = vec![entry];
        assert!(execute_uninstall_with_path(root.clone(), EnvType::Node, "fixture".into(), forged, &journal, &mut store).await.is_err());
        assert!(env.exists());
        let before = store.as_ref().unwrap().value.clone();
        let result = execute_uninstall_with_path(root, EnvType::Node, "fixture".into(), preview, &journal, &mut store).await.unwrap();
        assert_eq!(result.status, UninstallResultStatus::VerifiedScopeRemoved);
        assert_eq!(store.as_ref().unwrap().value, before);
        assert_eq!(store.as_ref().unwrap().writes, 0);
    }

    #[tokio::test]
    async fn uninstall_path_stale_preview_prevents_directory_removal() {
        for change in ["path", "peer", "journal"] {
            let dir = crate::test_support::TestDir::new();
            let (root, env, journal, mut store) = uninstall_path_fixture(dir.path(), EnvType::Node, true);
            let (preview, _) = preview_uninstall_with_path(&root, EnvType::Node, "fixture", &journal, &mut store).unwrap();
            match change {
                "path" => store.as_mut().unwrap().value.push_str(r";C:\new"),
                "peer" => std::fs::create_dir_all(env.parent().unwrap().join("new")).unwrap(),
                _ => std::fs::write(&journal, b"damaged").unwrap(),
            }
            assert!(execute_uninstall_with_path(root.clone(), EnvType::Node, "fixture".into(), preview, &journal, &mut store).await.is_err());
            assert!(env.join("payload").exists());
            assert!(root.join("current/node").exists());
            assert_eq!(store.as_ref().unwrap().writes, 0);
        }
    }

    #[tokio::test]
    async fn uninstall_path_failed_write_or_recheck_reports_partial() {
        for mode in ["write", "recheck", "late-peer"] {
            let dir = crate::test_support::TestDir::new();
            let (root, env, journal, mut store) = uninstall_path_fixture(dir.path(), EnvType::Node, true);
            let (preview, _) = preview_uninstall_with_path(&root, EnvType::Node, "fixture", &journal, &mut store).unwrap();
            let backend = store.as_mut().unwrap();
            backend.reads = 0;
            backend.fail_write = mode == "write";
            // Reads: execution preview, post-removal plan, apply before, prewrite, postwrite.
            if mode == "recheck" { backend.fail_read_at = Some(5); }
            if mode == "late-peer" {
                backend.create_peer_at = Some((4, env.parent().unwrap().join("new")));
            }
            let result = execute_uninstall_with_path(root, EnvType::Node, "fixture".into(), preview, &journal, &mut store).await.unwrap();
            assert!(!env.exists());
            assert_eq!(result.status, UninstallResultStatus::Partial);
            assert!(result.failed.iter().any(|item| item.resource == OWNED_PATH_RESOURCE));
            assert!(!result.removed.iter().any(|item| item.resource == OWNED_PATH_RESOURCE));
            if mode == "late-peer" { assert_eq!(store.as_ref().unwrap().writes, 0); }
            let value = &store.as_ref().unwrap().value;
            let candidates = vec![value.split(';').last().unwrap().to_owned()];
            assert!(crate::path_ownership::preview_cleanup(&journal, value, &candidates, &[]).unwrap().removed.is_empty());
        }
    }

    #[tokio::test]
    async fn uninstall_path_unknown_source_is_unverified_and_never_written() {
        for source in ["damaged", "unreadable"] {
            let dir = crate::test_support::TestDir::new();
            let (root, env, journal, mut store) = uninstall_path_fixture(dir.path(), EnvType::Node, true);
            if source == "damaged" { std::fs::write(&journal, b"broken").unwrap(); }
            else { store = Err(AppError::msg("fixture registry unavailable")); }
            let (preview, _) = preview_uninstall_with_path(&root, EnvType::Node, "fixture", &journal, &mut store).unwrap();
            assert!(preview.unverified.iter().any(|item| item.resource == OWNED_PATH_RESOURCE));
            assert!(preview.path_snapshot_hash.is_none());
            let result = execute_uninstall_with_path(root, EnvType::Node, "fixture".into(), preview, &journal, &mut store).await.unwrap();
            assert_eq!(result.status, UninstallResultStatus::VerifiedScopeRemoved);
            assert!(!env.exists());
            assert!(result.unverified.iter().any(|item| item.resource == OWNED_PATH_RESOURCE));
            if let Ok(store) = store { assert_eq!(store.writes, 0); }
        }
    }

    #[tokio::test]
    async fn uninstall_path_directory_failure_keeps_path_without_attempt() {
        use std::os::windows::fs::OpenOptionsExt;
        let dir = crate::test_support::TestDir::new();
        let (root, env, journal, mut store) = uninstall_path_fixture(dir.path(), EnvType::Node, true);
        let lock = std::fs::OpenOptions::new().read(true).share_mode(1).open(env.join("payload")).unwrap();
        let (preview, _) = preview_uninstall_with_path(&root, EnvType::Node, "fixture", &journal, &mut store).unwrap();
        let before = store.as_ref().unwrap().value.clone();
        let result = execute_uninstall_with_path(root, EnvType::Node, "fixture".into(), preview, &journal, &mut store).await.unwrap();
        assert_eq!(result.status, UninstallResultStatus::Partial);
        assert!(result.retained.iter().any(|item| item.resource == OWNED_PATH_RESOURCE));
        assert_eq!(store.as_ref().unwrap().writes, 0);
        assert_eq!(store.as_ref().unwrap().value, before);
        drop(lock);
    }

    #[tokio::test]
    async fn uninstall_preview_must_match_again_before_any_removal() {
        let dir = crate::test_support::TestDir::new();
        let root = dir.path().join("managed");
        let env = root.join("envs/nodes/node-22");
        std::fs::create_dir_all(&env).unwrap();
        std::fs::write(env.join("node.exe"), b"fixture").unwrap();
        let expected = preview_uninstall_scope(&root, EnvType::Node, "node-22").unwrap();
        std::fs::rename(env.join("node.exe"), env.join("node-renamed.exe")).unwrap();
        let current = preview_uninstall_scope(&root, EnvType::Node, "node-22").unwrap();

        assert_eq!(
            expected.removable.iter().find(|item| item.resource == "EnvCon 受管目录").and_then(|item| item.count),
            current.removable.iter().find(|item| item.resource == "EnvCon 受管目录").and_then(|item| item.count)
        );
        assert_ne!(expected.snapshot_hash, current.snapshot_hash);
        assert!(validate_uninstall_execution_preview(&expected, &current).is_err());
        let result =
            execute_uninstall_scope(root, EnvType::Node, "node-22".into(), expected).await;
        assert!(result.is_err());
        assert!(!env.join("node.exe").exists());
        assert!(env.join("node-renamed.exe").is_file());
    }

    #[tokio::test]
    async fn uninstall_nested_reparse_point_never_removes_its_external_target() {
        let dir = crate::test_support::TestDir::new();
        let root = dir.path().join("managed");
        let env = root.join("envs/nodes/node-22");
        let external = dir.path().join("external-data");
        std::fs::create_dir_all(&env).unwrap();
        std::fs::create_dir_all(&external).unwrap();
        std::fs::write(env.join("node.exe"), b"fixture").unwrap();
        std::fs::write(external.join("keep.txt"), b"keep").unwrap();
        junction::create(&external, env.join("shared-data")).unwrap();
        let preview = preview_uninstall_scope(&root, EnvType::Node, "node-22").unwrap();
        assert!(preview.unverified.iter().any(|item| item.resource == "目录内重解析点目标"));

        let result =
            execute_uninstall_scope(root, EnvType::Node, "node-22".into(), preview)
                .await
                .unwrap();

        assert_eq!(result.status, UninstallResultStatus::VerifiedScopeRemoved);
        assert!(!env.exists());
        assert!(external.join("keep.txt").is_file());
    }

    #[tokio::test]
    async fn uninstall_verifies_current_link_and_managed_directory_removal() {
        let dir = crate::test_support::TestDir::new();
        let root = dir.path().join("managed");
        let env = root.join("envs/nodes/node-22");
        std::fs::create_dir_all(&env).unwrap();
        std::fs::write(env.join("node.exe"), b"fixture").unwrap();
        std::fs::create_dir_all(root.join("current")).unwrap();
        switcher::switch(&root, EnvType::Node, "node-22").unwrap();
        let preview = preview_uninstall_scope(&root, EnvType::Node, "node-22").unwrap();

        let result = execute_uninstall_scope(root.clone(), EnvType::Node, "node-22".into(), preview)
            .await
            .unwrap();

        assert_eq!(result.status, UninstallResultStatus::VerifiedScopeRemoved);
        assert!(result.failed.is_empty());
        assert!(result.removed.iter().any(|item| item.resource == "当前版本 Junction"));
        assert!(result.removed.iter().any(|item| item.resource == "EnvCon 受管目录"));
        assert!(std::fs::symlink_metadata(&env).is_err());
        assert!(std::fs::symlink_metadata(root.join("current/node")).is_err());
    }

    #[tokio::test]
    async fn uninstall_reports_partial_failure_when_a_managed_file_is_locked() {
        use std::os::windows::fs::OpenOptionsExt;

        let dir = crate::test_support::TestDir::new();
        let root = dir.path().join("managed");
        let env = root.join("envs/nodes/node-22");
        std::fs::create_dir_all(&env).unwrap();
        let locked_file = env.join("locked.bin");
        std::fs::write(&locked_file, b"fixture").unwrap();
        std::fs::create_dir_all(root.join("current")).unwrap();
        switcher::switch(&root, EnvType::Node, "node-22").unwrap();
        let preview = preview_uninstall_scope(&root, EnvType::Node, "node-22").unwrap();
        let lock = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&locked_file)
            .unwrap();

        let result = execute_uninstall_scope(root.clone(), EnvType::Node, "node-22".into(), preview)
            .await
            .unwrap();
        drop(lock);

        assert_eq!(result.status, UninstallResultStatus::Partial);
        assert!(result.removed.iter().any(|item| item.resource == "当前版本 Junction"));
        assert!(result.failed.iter().any(|item| item.resource == "EnvCon 受管目录"));
        assert!(locked_file.is_file());
        assert!(std::fs::symlink_metadata(root.join("current/node")).is_err());
    }

    #[tokio::test]
    async fn uninstall_preview_reports_a_locked_file_before_any_deletion_runs() {
        use std::os::windows::fs::OpenOptionsExt;

        let dir = crate::test_support::TestDir::new();
        let root = dir.path().join("managed");
        let env = root.join("envs/nodes/node-22");
        std::fs::create_dir_all(&env).unwrap();
        let locked_file = env.join("locked.bin");
        std::fs::write(&locked_file, b"fixture").unwrap();
        std::fs::write(env.join("free.txt"), b"free").unwrap();
        std::fs::create_dir_all(root.join("current")).unwrap();
        switcher::switch(&root, EnvType::Node, "node-22").unwrap();

        // Nothing holds the file yet: the preview must not invent a lock.
        let clean = preview_uninstall_scope(&root, EnvType::Node, "node-22").unwrap();
        assert!(
            !clean.unverified.iter().any(|item| item.resource == "文件占用"),
            "a clean tree must not report a lock: {:?}",
            clean.unverified
        );
        assert!(clean
            .removable
            .iter()
            .any(|item| item.details.contains("已检查全部") && item.details.contains("占用")));

        // Hold the file with no sharing, exactly as a running program would.
        let lock = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&locked_file)
            .unwrap();

        let preview = preview_uninstall_scope(&root, EnvType::Node, "node-22").unwrap();
        let held = preview
            .unverified
            .iter()
            .find(|item| item.resource == "文件占用")
            .expect("the preview must surface the lock before deletion");
        // The exact file is named, so the user can close the owning program.
        assert!(
            held.details.contains("locked.bin"),
            "unexpected details: {}",
            held.details
        );
        assert!(held.details.contains("请先关闭占用这些文件的程序"));
        assert_eq!(held.count, Some(1));
        // The unlocked sibling must not be blamed.
        assert!(!held.details.contains("free.txt"));
        // A lock is reported, not used to block the whole preview: the scope is
        // still complete so the user can decide after seeing it.
        assert!(preview.scope_complete);

        drop(lock);
        let released = preview_uninstall_scope(&root, EnvType::Node, "node-22").unwrap();
        assert!(
            !released
                .unverified
                .iter()
                .any(|item| item.resource == "文件占用"),
            "a released lock must disappear from the preview"
        );
    }

    #[tokio::test]
    async fn uninstall_preview_reports_an_open_directory_separately_from_a_locked_file() {
        use std::os::windows::fs::OpenOptionsExt;

        let dir = crate::test_support::TestDir::new();
        let root = dir.path().join("managed");
        let env = root.join("envs/nodes/node-22");
        let cachedir = env.join("cache");
        std::fs::create_dir_all(&cachedir).unwrap();
        std::fs::write(env.join("payload.bin"), b"fixture").unwrap();
        std::fs::create_dir_all(root.join("current")).unwrap();
        switcher::switch(&root, EnvType::Node, "node-22").unwrap();

        // Nothing holds the folder: no directory lock may be invented.
        let clean = preview_uninstall_scope(&root, EnvType::Node, "node-22").unwrap();
        assert!(
            !clean
                .unverified
                .iter()
                .any(|item| item.resource == LOCKED_DIR_RESOURCE),
            "a clean tree must not report a directory lock: {:?}",
            clean.unverified
        );

        // Hold the directory the way a program with it as its working directory
        // would: reads and writes still allowed, deletion refused. A fully
        // exclusive handle (share_mode 0) is stronger than that and blocks the
        // traversal itself, which is a different situation and is covered by the
        // blocked branch, not here.
        let held_dir = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(3)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
            .open(&cachedir)
            .unwrap();
        // The probe must see this as a real block, not as a permissions problem.
        assert!(
            directory_blocks_deletion(&cachedir),
            "an open working directory must block deletion"
        );

        let preview = preview_uninstall_scope(&root, EnvType::Node, "node-22").unwrap();
        let held = preview
            .unverified
            .iter()
            .find(|item| item.resource == LOCKED_DIR_RESOURCE)
            .expect("an open directory must be surfaced before deletion");
        // The specific folder is named so the user knows what to close.
        assert!(held.details.contains("cache"), "unexpected: {}", held.details);
        // The remedy differs from a locked file and must say so.
        assert!(held.details.contains("工作目录"), "unexpected: {}", held.details);
        assert_eq!(held.count, Some(1));
        // A directory lock is reported, not used to block the preview.
        assert!(preview.scope_complete);
        // Directory and file locks are distinct resources: this one must not be
        // reported as the file-lock entry.
        assert!(!preview
            .unverified
            .iter()
            .any(|item| item.resource == LOCK_RESOURCE));

        drop(held_dir);
        let released = preview_uninstall_scope(&root, EnvType::Node, "node-22").unwrap();
        assert!(
            !released
                .unverified
                .iter()
                .any(|item| item.resource == LOCKED_DIR_RESOURCE),
            "a released directory handle must disappear from the preview"
        );
    }

    /// A handle that permits no sharing at all blocks `read_dir` itself, so the
    /// walk fails before the success-branch reporting can run. The preview must
    /// still name the directory: a bare "could not read" is exactly the message
    /// this round exists to replace.
    #[tokio::test]
    async fn an_exclusively_held_directory_is_named_even_when_the_walk_fails() {
        use std::os::windows::fs::OpenOptionsExt;

        let dir = crate::test_support::TestDir::new();
        let root = dir.path().join("managed");
        let env = root.join("envs/nodes/node-22");
        let cachedir = env.join("cache");
        std::fs::create_dir_all(&cachedir).unwrap();
        std::fs::write(env.join("payload.bin"), b"fixture").unwrap();
        std::fs::create_dir_all(root.join("current")).unwrap();
        switcher::switch(&root, EnvType::Node, "node-22").unwrap();

        // Fully exclusive: even reading the directory contents is impossible.
        let exclusive = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
            .open(&cachedir)
            .unwrap();

        let preview = preview_uninstall_scope(&root, EnvType::Node, "node-22").unwrap();
        // The traversal really did stop, which is what makes this the error path.
        assert!(
            !preview.scope_complete,
            "an unreadable subtree must not be reported as a complete scope"
        );
        assert!(preview
            .blocked
            .iter()
            .any(|item| item.resource == "受管目录树"));

        // And the cause is named rather than left to guesswork.
        let named = preview
            .blocked
            .iter()
            .find(|item| item.resource == LOCKED_DIR_RESOURCE)
            .expect("the blocking directory must be named in the blocked list");
        assert!(named.details.contains("cache"), "unexpected: {}", named.details);
        assert_eq!(named.count, Some(1));

        drop(exclusive);
        let recovered = preview_uninstall_scope(&root, EnvType::Node, "node-22").unwrap();
        assert!(
            recovered.scope_complete,
            "releasing the handle must let the scope be read again"
        );
    }

    #[tokio::test]
    async fn detach_mode_keeps_every_file_and_only_drops_management_records() {
        let dir = crate::test_support::TestDir::new();
        let root = dir.path().join("managed");
        let env = root.join("envs/nodes/node-22");
        std::fs::create_dir_all(&env).unwrap();
        std::fs::write(env.join("payload.bin"), b"keep").unwrap();
        std::fs::create_dir_all(root.join("current")).unwrap();
        switcher::switch(&root, EnvType::Node, "node-22").unwrap();

        // Uninstall keeps planning to delete the directory.
        let uninstall = preview_uninstall_scope(&root, EnvType::Node, "node-22").unwrap();
        assert_eq!(uninstall.mode, UninstallMode::Uninstall);
        assert!(uninstall
            .removable
            .iter()
            .any(|item| item.resource == "EnvCon 受管目录"));

        // Detach moves that plan to `retained` and says why in the details.
        let mut detach = preview_uninstall_scope(&root, EnvType::Node, "node-22").unwrap();
        shape_preview(&mut detach, UninstallMode::Detach);
        assert_eq!(detach.mode, UninstallMode::Detach);
        assert!(
            !detach
                .removable
                .iter()
                .any(|item| item.resource == "EnvCon 受管目录"),
            "Detach must not plan to delete the directory"
        );
        let kept = detach
            .retained
            .iter()
            .find(|item| item.resource == "EnvCon 受管目录")
            .expect("the directory must be listed as retained");
        assert!(kept.details.contains("停用模式：保留磁盘内容"));
        // "Stopped" must also mean inactive, so the current junction still goes.
        assert!(detach
            .removable
            .iter()
            .any(|item| item.resource == "当前版本 Junction"));

        let result =
            execute_uninstall_scope(root.clone(), EnvType::Node, "node-22".into(), detach)
                .await
                .unwrap();
        assert_eq!(result.status, UninstallResultStatus::VerifiedScopeRemoved);
        assert!(result.failed.is_empty());
        // The whole promise of this mode: the bytes are still there.
        assert_eq!(std::fs::read(env.join("payload.bin")).unwrap(), b"keep");
        // ... while the environment is no longer active.
        assert!(std::fs::symlink_metadata(root.join("current/node")).is_err());
    }

    #[tokio::test]
    async fn a_preview_whose_mode_disagrees_with_its_scope_is_rejected() {
        let dir = crate::test_support::TestDir::new();
        let root = dir.path().join("managed");
        let env = root.join("envs/nodes/node-22");
        std::fs::create_dir_all(&env).unwrap();
        std::fs::write(env.join("payload.bin"), b"keep").unwrap();

        // A Detach-shaped scope whose mode claims Uninstall: the directory is
        // listed as retained, so the executor's own re-derivation (which would
        // plan a deletion) can never match it. This is exactly what a stale or
        // tampered submission looks like, and it must be refused.
        let mut forged = preview_uninstall_scope(&root, EnvType::Node, "node-22").unwrap();
        shape_preview(&mut forged, UninstallMode::Detach);
        forged.mode = UninstallMode::Uninstall;

        let error =
            execute_uninstall_scope(root.clone(), EnvType::Node, "node-22".into(), forged)
                .await
                .unwrap_err()
                .to_string();
        assert!(
            error.contains("卸载预览已变化"),
            "unexpected error: {error}"
        );
        // Refused before any mutation: the bytes are untouched.
        assert_eq!(std::fs::read(env.join("payload.bin")).unwrap(), b"keep");
    }

    #[tokio::test]
    async fn uninstall_unowned_links_block_removal_without_claiming_legacy_ownership() {
        for current in [false, true] {
            let dir = crate::test_support::TestDir::new();
            let root = dir.path().join("managed");
            let target = dir.path().join("external");
            let env = root.join("envs/nodes/fixture");
            std::fs::create_dir_all(&target).unwrap();
            std::fs::write(target.join("keep"), b"keep").unwrap();
            std::fs::create_dir_all(env.parent().unwrap()).unwrap();
            if current {
                std::fs::create_dir(&env).unwrap();
                std::fs::write(env.join("keep"), b"keep").unwrap();
                std::fs::create_dir_all(root.join("current")).unwrap();
                junction::create(&env, root.join("current/node")).unwrap();
            } else {
                junction::create(&target, &env).unwrap();
            }
            let preview = preview_uninstall_scope(&root, EnvType::Node, "fixture").unwrap();
            assert!(preview.scope_complete);
            assert!(preview.blocked.is_empty());
            assert_eq!(preview.legacy_links.len(), 1);
            assert!(execute_uninstall_scope(root.clone(), EnvType::Node, "fixture".into(), preview).await.is_err());
            assert!(env.join("keep").is_file());
            assert!(target.join("keep").is_file());
            assert!(!root.join(".envcon-ownership").exists());
        }
    }

    struct InstallerCommandFixture {
        _dir: crate::test_support::TestDir,
        root: PathBuf,
        env: PathBuf,
        journal: PathBuf,
        snapshot: crate::installer_uninstall::RegistrySnapshot,
        base: UninstallPreview,
        plan: crate::installer_uninstall::Plan,
        store: Result<UninstallPathFixture>,
    }

    struct FixtureInstallerBackend {
        before: crate::installer_uninstall::RegistrySnapshot,
        after: crate::installer_uninstall::RegistrySnapshot,
        runs: std::sync::Arc<std::sync::atomic::AtomicUsize>,
        reads: usize,
        runtime_root: PathBuf,
        remove_runtime: bool,
        exit: i32,
    }

    impl crate::installer_uninstall::Backend for FixtureInstallerBackend {
        fn registry(&mut self) -> Result<crate::installer_uninstall::RegistrySnapshot> {
            self.reads += 1;
            if self.reads == 1 {
                Ok(self.before.clone())
            } else {
                Ok(self.after.clone())
            }
        }
        fn run(&mut self, _: &crate::installer_uninstall::Invocation) -> Result<i32> {
            use std::sync::atomic::Ordering;
            self.runs.fetch_add(1, Ordering::SeqCst);
            if self.remove_runtime {
                std::fs::remove_dir_all(&self.runtime_root)?;
            }
            Ok(self.exit)
        }
        fn runtime_present(&mut self, _: EnvType, root: &Path) -> Result<bool> {
            match std::fs::symlink_metadata(root) {
                Ok(_) => Ok(true),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
                Err(error) => Err(error.into()),
            }
        }
    }

    fn installer_command_fixture(owned_path: bool) -> InstallerCommandFixture {
        let dir = crate::test_support::TestDir::new();
        let root = dir.path().join("managed");
        let env = root.join("envs").join(EnvType::Llvm.folder()).join("fixture");
        std::fs::create_dir_all(&env).unwrap();
        std::fs::write(env.join("Uninstall.exe"), b"fixture never executed").unwrap();
        std::fs::create_dir_all(root.join("current")).unwrap();
        switcher::switch(&root, EnvType::Llvm, "fixture").unwrap();
        let journal = dir.path().join("path/journal.json");
        let entries: Vec<_> = EnvType::Llvm.path_entries(&root).iter()
            .map(|path| path.to_string_lossy().into_owned()).collect();
        let mut store = Ok(UninstallPathFixture {
            value: r"C:\user".into(), writes: 0, reads: 0, fail_write: false,
            fail_read_at: None, create_peer_at: None,
        });
        if owned_path {
            crate::path_ownership::apply(
                store.as_mut().unwrap(), &journal, crate::path_ownership::Change::Append(&entries),
            ).unwrap();
            let fixture = store.as_mut().unwrap();
            fixture.writes = 0;
            fixture.reads = 0;
        }
        let registration = crate::installer_uninstall::Registration {
            id: "HKCU/64/LLVM".into(), key_name: "LLVM".into(), display_name: "LLVM".into(),
            publisher: "LLVM Project".into(), install_location: Some(env.to_string_lossy().into_owned()),
            uninstall_command: Some(format!("\"{}\"", env.join("Uninstall.exe").display())),
            quiet_command: None, bundle_provider: None, python_install_path: None,
        };
        let snapshot = crate::installer_uninstall::RegistrySnapshot {
            records: vec![registration], errors: vec![],
        };
        let (base, _) = preview_uninstall_with_path(&root, EnvType::Llvm, "fixture", &journal, &mut store).unwrap();
        let attached = attach_installer_preview(&root, base.clone(), snapshot.clone()).unwrap();
        assert!(attached.scope_complete);
        assert!(attached.blocked.is_empty());
        assert!(attached.removable.iter().any(|item| item.resource == "原厂卸载范围（执行后复查）"));
        assert!(!attached.removable.iter().any(|item| item.resource == "EnvCon 受管目录"));
        assert!(attached.unverified.iter().any(|item| item.resource == "安装器目录外副作用"));
        let plan = attached.installer_plan.clone().expect("fixture installer plan");
        InstallerCommandFixture { _dir: dir, root, env, journal, snapshot, base, plan, store }
    }

    fn installer_backend(
        fixture: &InstallerCommandFixture, after: crate::installer_uninstall::RegistrySnapshot,
        remove_runtime: bool, exit: i32,
    ) -> (FixtureInstallerBackend, std::sync::Arc<std::sync::atomic::AtomicUsize>) {
        let runs = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        (FixtureInstallerBackend {
            before: fixture.snapshot.clone(), after, runs: runs.clone(), reads: 0,
            runtime_root: fixture.env.clone(), remove_runtime, exit,
        }, runs)
    }

    #[tokio::test]
    async fn installer_ready_preview_uses_original_entry_and_rejects_directory_fallback() {
        let fixture = installer_command_fixture(false);
        let preview = fixture.base.clone();
        let mut attached = attach_installer_preview(
            &fixture.root, preview.clone(), fixture.snapshot.clone(),
        ).unwrap();
        attached.confirmed_legacy_links = vec![];
        assert!(execute_uninstall_scope(
            fixture.root.clone(), EnvType::Llvm, "fixture".into(), attached,
        ).await.is_err());
        assert!(fixture.env.join("Uninstall.exe").exists());

        let empty = attach_installer_preview(
            &fixture.root, preview, crate::installer_uninstall::RegistrySnapshot::default(),
        ).unwrap();
        assert_eq!(empty.installer_plan.as_ref().unwrap().state, "unmatched");
        assert!(empty.scope_complete);
        assert!(empty.unverified.iter().any(|item| item.details.contains("不代表绝对没有")));
    }

    #[tokio::test]
    async fn installer_success_removes_registration_runtime_junction_and_owned_path() {
        let mut fixture = installer_command_fixture(true);
        let (backend, runs) = installer_backend(
            &fixture, crate::installer_uninstall::RegistrySnapshot::default(), true, 0,
        );
        let result = execute_installer_uninstall_with_path(
            InstallerUninstallTarget { root: fixture.root.clone(), env_type: EnvType::Llvm, name: "fixture".into(), journal: &fixture.journal },
            fixture.base.clone(),
            fixture.plan.clone(), backend, &mut fixture.store,
        ).await.unwrap();

        assert_eq!(result.status, UninstallResultStatus::VerifiedScopeRemoved);
        assert!(result.failed.is_empty());
        assert_eq!(runs.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert!(!fixture.env.exists());
        assert!(std::fs::symlink_metadata(fixture.root.join("current/llvm")).is_err());
        assert!(result.removed.iter().any(|item| item.resource == "原厂卸载登记"));
        assert!(result.removed.iter().any(|item| item.resource == "原厂卸载运行时目录"));
        assert!(result.removed.iter().any(|item| item.resource == OWNED_PATH_RESOURCE));
        assert_eq!(fixture.store.as_ref().unwrap().value, r"C:\user");
    }

    #[tokio::test]
    async fn installer_command_failure_keeps_runtime_path_and_never_recursively_deletes() {
        let mut fixture = installer_command_fixture(true);
        let (backend, runs) = installer_backend(&fixture, fixture.snapshot.clone(), false, 1);
        let result = execute_installer_uninstall_with_path(
            InstallerUninstallTarget { root: fixture.root.clone(), env_type: EnvType::Llvm, name: "fixture".into(), journal: &fixture.journal },
            fixture.base.clone(),
            fixture.plan.clone(), backend, &mut fixture.store,
        ).await.unwrap();

        assert_eq!(result.status, UninstallResultStatus::Partial);
        assert_eq!(runs.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert!(fixture.env.join("Uninstall.exe").exists());
        assert!(result.failed.iter().any(|item| item.resource == "原厂卸载命令"));
        assert!(result.retained.iter().any(|item| item.resource == "原厂卸载后的残留内容"));
        assert!(result.retained.iter().any(|item| item.resource == OWNED_PATH_RESOURCE));
        assert_eq!(fixture.store.as_ref().unwrap().writes, 0);
        assert!(fixture.store.as_ref().unwrap().value.contains("current\\llvm\\bin"));
    }

    #[tokio::test]
    async fn installer_success_exit_still_requires_registration_and_runtime_absence() {
        for mode in ["registration", "runtime"] {
            let mut fixture = installer_command_fixture(false);
            let (after, remove_runtime) = if mode == "registration" {
                (fixture.snapshot.clone(), true)
            } else {
                (crate::installer_uninstall::RegistrySnapshot::default(), false)
            };
            let (backend, runs) = installer_backend(&fixture, after, remove_runtime, 0);
            let result = execute_installer_uninstall_with_path(
                InstallerUninstallTarget { root: fixture.root.clone(), env_type: EnvType::Llvm, name: "fixture".into(), journal: &fixture.journal },
                fixture.base.clone(),
                fixture.plan.clone(), backend, &mut fixture.store,
            ).await.unwrap();
            assert_eq!(result.status, UninstallResultStatus::Partial, "{mode}");
            assert_eq!(runs.load(std::sync::atomic::Ordering::SeqCst), 1);
            if mode == "registration" {
                assert!(result.failed.iter().any(|item| item.resource == "原厂卸载登记"));
                assert!(!fixture.env.exists());
            } else {
                assert!(result.failed.iter().any(|item| item.resource == "原厂卸载运行时目录"));
                assert!(result.retained.iter().any(|item| item.resource == "原厂卸载后的残留内容"));
                assert!(fixture.env.exists());
            }
        }
    }

    #[tokio::test]
    async fn installer_preflight_change_blocks_command_and_keeps_directory() {
        let mut fixture = installer_command_fixture(false);
        let (mut backend, runs) = installer_backend(
            &fixture, crate::installer_uninstall::RegistrySnapshot::default(), true, 0,
        );
        backend.before.records[0].uninstall_command.as_mut().unwrap().push_str(" /S");
        let result = execute_installer_uninstall_with_path(
            InstallerUninstallTarget { root: fixture.root.clone(), env_type: EnvType::Llvm, name: "fixture".into(), journal: &fixture.journal },
            fixture.base.clone(),
            fixture.plan.clone(), backend, &mut fixture.store,
        ).await.unwrap();
        assert_eq!(result.status, UninstallResultStatus::Partial);
        assert_eq!(runs.load(std::sync::atomic::Ordering::SeqCst), 0);
        assert!(fixture.env.join("Uninstall.exe").exists());
        assert!(result.failed.iter().any(|item| item.resource == "原厂卸载预检/执行"));
    }

    #[tokio::test]
    async fn installer_path_cleanup_failure_is_partial_after_verified_original_uninstall() {
        let mut fixture = installer_command_fixture(true);
        fixture.store.as_mut().unwrap().fail_write = true;
        let (backend, _) = installer_backend(
            &fixture, crate::installer_uninstall::RegistrySnapshot::default(), true, 0,
        );
        let result = execute_installer_uninstall_with_path(
            InstallerUninstallTarget { root: fixture.root.clone(), env_type: EnvType::Llvm, name: "fixture".into(), journal: &fixture.journal },
            fixture.base.clone(),
            fixture.plan.clone(), backend, &mut fixture.store,
        ).await.unwrap();
        assert_eq!(result.status, UninstallResultStatus::Partial);
        assert!(result.removed.iter().any(|item| item.resource == "原厂卸载运行时目录"));
        assert!(result.failed.iter().any(|item| item.resource == OWNED_PATH_RESOURCE));
        assert!(!fixture.env.exists());
        assert!(fixture.store.as_ref().unwrap().value.contains("current\\llvm\\bin"));
    }

    #[test]
    fn installer_current_junction_removal_failure_is_reported_before_command() {
        let fixture = installer_command_fixture(false);
        let mut preview = fixture.base.clone();
        preview.current_link_snapshot_hash = Some("fixture-invalid-identity".into());
        let (removed, failed) =
            detach_current_link_for_uninstall(&fixture.root, EnvType::Llvm, &preview).unwrap();
        assert!(removed.is_empty());
        assert!(failed.iter().any(|item| item.resource == "当前版本 Junction"));
        assert!(std::fs::symlink_metadata(fixture.root.join("current/llvm")).is_ok());
        assert!(fixture.env.join("Uninstall.exe").exists());
    }

    #[tokio::test]
    async fn uninstall_legacy_links_require_each_confirmation_and_keep_external_data() {
        let dir = crate::test_support::TestDir::new();
        let root = dir.path().join("managed");
        let target = dir.path().join("external");
        let env = root.join("envs/nodes/fixture");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("keep"), b"keep").unwrap();
        std::fs::create_dir_all(env.parent().unwrap()).unwrap();
        std::fs::create_dir_all(root.join("current")).unwrap();
        junction::create(&target, &env).unwrap();
        junction::create(&env, root.join("current/node")).unwrap();
        let preview = preview_uninstall_scope(&root, EnvType::Node, "fixture").unwrap();
        assert_eq!(preview.legacy_links.len(), 2);
        let paths: Vec<_> = preview.legacy_links.iter().map(|item| item.path.clone().unwrap()).collect();
        for confirmations in [vec![], vec![paths[0].clone()], vec![paths[0].clone(), paths[0].clone()], vec![paths[0].clone(), target.to_string_lossy().into_owned()]] {
            let mut submitted = preview.clone();
            submitted.confirmed_legacy_links = confirmations;
            assert!(execute_uninstall_scope(root.clone(), EnvType::Node, "fixture".into(), submitted).await.is_err());
            assert!(root.join("current/node/keep").exists());
            assert!(env.join("keep").exists());
        }
        let mut confirmed = preview;
        confirmed.confirmed_legacy_links = paths;
        let result = execute_uninstall_scope(root.clone(), EnvType::Node, "fixture".into(), confirmed).await.unwrap();
        assert_eq!(result.status, UninstallResultStatus::VerifiedScopeRemoved);
        assert!(result.removed.iter().any(|item| item.resource == "用户确认移除的管理链接"));
        assert!(result.removed.iter().all(|item| item.details.contains("确认")));
        assert!(std::fs::symlink_metadata(root.join("current/node")).is_err());
        assert!(std::fs::symlink_metadata(&env).is_err());
        assert_eq!(std::fs::read(target.join("keep")).unwrap(), b"keep");
    }

    #[tokio::test]
    async fn uninstall_legacy_confirmation_is_bound_to_object_and_path_preview() {
        let dir = crate::test_support::TestDir::new();
        let root = dir.path().join("managed");
        let env = root.join("envs/nodes/fixture");
        let journal = dir.path().join("path/journal.json");
        std::fs::create_dir_all(&env).unwrap();
        std::fs::create_dir_all(root.join("current")).unwrap();
        std::fs::write(env.join("keep"), b"keep").unwrap();
        junction::create(&env, root.join("current/node")).unwrap();
        let entries = vec![root.join("current/node").to_string_lossy().into_owned()];
        let mut store = Ok(UninstallPathFixture {
            value: r"C:\user".into(), writes: 0, reads: 0, fail_write: false,
            fail_read_at: None, create_peer_at: None,
        });
        crate::path_ownership::apply(store.as_mut().unwrap(), &journal, crate::path_ownership::Change::Append(&entries)).unwrap();
        let (mut preview, _) = preview_uninstall_with_path(&root, EnvType::Node, "fixture", &journal, &mut store).unwrap();
        preview.confirmed_legacy_links = preview.legacy_links.iter().map(|item| item.path.clone().unwrap()).collect();
        std::fs::rename(root.join("current/node"), root.join("current/old-object")).unwrap();
        junction::create(&env, root.join("current/node")).unwrap();
        assert!(execute_uninstall_with_path(root.clone(), EnvType::Node, "fixture".into(), preview, &journal, &mut store).await.is_err());
        assert!(env.join("keep").exists());
        let (mut fresh, _) = preview_uninstall_with_path(&root, EnvType::Node, "fixture", &journal, &mut store).unwrap();
        fresh.confirmed_legacy_links = fresh.legacy_links.iter().map(|item| item.path.clone().unwrap()).collect();
        let result = execute_uninstall_with_path(root.clone(), EnvType::Node, "fixture".into(), fresh, &journal, &mut store).await.unwrap();
        assert_eq!(result.status, UninstallResultStatus::VerifiedScopeRemoved);
        assert!(result.removed.iter().any(|item| item.resource == OWNED_PATH_RESOURCE));
        assert_eq!(store.as_ref().unwrap().value, r"C:\user");
        assert!(!env.exists());
        // The unrelated renamed alias was not selected for deletion.
        assert!(std::fs::symlink_metadata(root.join("current/old-object")).is_ok());
    }

    #[tokio::test]
    async fn uninstall_replaced_link_or_switched_current_invalidates_preview() {
        for current in [false, true] {
            let dir = crate::test_support::TestDir::new();
            let root = dir.path().join("managed");
            let target = dir.path().join("external");
            let env = root.join("envs/nodes/fixture");
            std::fs::create_dir_all(&target).unwrap();
            std::fs::write(target.join("keep"), b"keep").unwrap();
            std::fs::create_dir_all(env.parent().unwrap()).unwrap();
            crate::link_ownership::create(&root, &env, &target).unwrap();
            switcher::switch(&root, EnvType::Node, "fixture").unwrap();
            let preview = preview_uninstall_scope(&root, EnvType::Node, "fixture").unwrap();
            assert!(preview.scope_complete);
            if current {
                switcher::switch(&root, EnvType::Node, "fixture").unwrap();
            } else {
                std::fs::rename(&env, env.parent().unwrap().join("old-object")).unwrap();
                junction::create(&target, &env).unwrap();
            }
            assert!(execute_uninstall_scope(root.clone(), EnvType::Node, "fixture".into(), preview).await.is_err());
            assert!(env.join("keep").is_file());
            assert!(root.join("current/node/keep").is_file());
            assert!(target.join("keep").is_file());
        }
    }

    #[tokio::test]
    async fn uninstall_external_link_removes_link_but_verifies_target_retained() {
        let dir = crate::test_support::TestDir::new();
        let root = dir.path().join("managed");
        let target = dir.path().join("external-tool");
        let link = root.join("envs/jdks/system-jdk");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("keep.txt"), b"keep").unwrap();
        std::fs::create_dir_all(link.parent().unwrap()).unwrap();
        crate::link_ownership::create(&root, &link, &target).unwrap();
        let preview = preview_uninstall_scope(&root, EnvType::Jdk, "system-jdk").unwrap();

        let result = execute_uninstall_scope(root.clone(), EnvType::Jdk, "system-jdk".into(), preview)
            .await
            .unwrap();

        assert_eq!(result.status, UninstallResultStatus::VerifiedScopeRemoved);
        assert!(result.removed.iter().any(|item| item.resource == "EnvCon 外部纳管链接"));
        assert!(result.retained.iter().any(|item| {
            item.resource == "外部原安装目标"
                && item.path.as_deref().is_some_and(|path| system::paths_eq(Path::new(path), &target))
        }));
        assert!(std::fs::symlink_metadata(&link).is_err());
        assert!(target.join("keep.txt").is_file());
    }

    #[tokio::test]
    async fn uninstall_inactive_alias_preserves_current_entry_to_same_target() {
        let dir = crate::test_support::TestDir::new();
        let root = dir.path().join("managed");
        let target = dir.path().join("external-jdk");
        let category = root.join("envs/jdks");
        std::fs::create_dir_all(&category).unwrap();
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("keep.txt"), b"keep").unwrap();
        for name in ["jdk-active", "jdk-inactive"] {
            crate::link_ownership::create(&root, &category.join(name), &target).unwrap();
        }
        switcher::switch(&root, EnvType::Jdk, "jdk-active").unwrap();
        let preview = preview_uninstall_scope(&root, EnvType::Jdk, "jdk-inactive").unwrap();
        assert!(!preview.is_current);
        execute_uninstall_scope(root.clone(), EnvType::Jdk, "jdk-inactive".into(), preview)
            .await.unwrap();
        assert!(root.join("current/jdk/keep.txt").is_file());
        assert!(category.join("jdk-active/keep.txt").is_file());
        assert!(std::fs::symlink_metadata(category.join("jdk-inactive")).is_err());
        let active = preview_uninstall_scope(&root, EnvType::Jdk, "jdk-active").unwrap();
        assert!(active.is_current);
        execute_uninstall_scope(root.clone(), EnvType::Jdk, "jdk-active".into(), active)
            .await.unwrap();
        assert!(std::fs::symlink_metadata(root.join("current/jdk")).is_err());
        assert_eq!(std::fs::read(target.join("keep.txt")).unwrap(), b"keep");
    }

    #[tokio::test]
    async fn uninstall_internal_link_is_not_reported_as_external_and_preserves_target() {
        let dir = crate::test_support::TestDir::new();
        let root = dir.path().join("managed");
        let target = root.join("envs/jdks/jdk-21");
        let link = root.join("envs/jdks/jdk-alias");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("keep.txt"), b"keep").unwrap();
        crate::link_ownership::create(&root, &link, &target).unwrap();

        let preview = preview_uninstall_scope(&root, EnvType::Jdk, "jdk-alias").unwrap();
        assert!(preview.scope_complete);
        assert!(preview.is_link_entry);
        assert!(!preview.is_external_link);
        assert!(preview
            .removable
            .iter()
            .any(|item| item.resource == "EnvCon 受管目录链接"));
        assert!(preview
            .retained
            .iter()
            .any(|item| item.resource == "受管目标目录"));

        let result =
            execute_uninstall_scope(root, EnvType::Jdk, "jdk-alias".into(), preview)
                .await
                .unwrap();
        assert_eq!(result.status, UninstallResultStatus::VerifiedScopeRemoved);
        assert!(result
            .removed
            .iter()
            .any(|item| item.resource == "EnvCon 受管目录链接"));
        assert!(std::fs::symlink_metadata(&link).is_err());
        assert!(target.join("keep.txt").is_file());
    }

    #[test]
    fn uninstall_preview_only_lists_external_link_for_removal_and_keeps_target() {
        let dir = crate::test_support::TestDir::new();
        let root = dir.path().join("managed");
        let target = dir.path().join("external-jdk");
        let link = root.join("envs/jdks/system-jdk");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("keep.txt"), b"keep").unwrap();
        std::fs::create_dir_all(link.parent().unwrap()).unwrap();
        crate::link_ownership::create(&root, &link, &target).unwrap();

        let preview = preview_uninstall_scope(&root, EnvType::Jdk, "system-jdk").unwrap();
        assert!(preview.scope_complete);
        assert!(preview.is_external_link);
        assert!(preview.removable.iter().any(|item| item.resource == "EnvCon 外部纳管链接"));
        assert!(preview.retained.iter().any(|item| {
            item.resource == "外部原安装目标"
                && item
                    .path
                    .as_deref()
                    .is_some_and(|path| system::paths_eq(Path::new(path), &target))
        }));
        assert!(!preview.removable.iter().any(|item| {
            item.path
                .as_deref()
                .is_some_and(|path| system::path_key(Path::new(path)) == system::path_key(&target))
        }));
        assert!(link.exists());
        assert!(target.join("keep.txt").is_file());
    }

    #[test]
    fn a_dangling_managed_link_can_be_cleaned_up_instead_of_blocking_forever() {
        let dir = crate::test_support::TestDir::new();
        let root = dir.path().join("managed");
        let target = dir.path().join("managed/envs/jdks/jdk-old");
        let link = root.join("envs/jdks/jdk-alias");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("payload.txt"), b"keep").unwrap();
        crate::link_ownership::create(&root, &link, &target).unwrap();

        // Removing the target leaves a link whose object is intact but whose
        // destination is gone: exactly the "damaged entry" this item is about.
        std::fs::remove_dir_all(&target).unwrap();
        assert!(std::fs::symlink_metadata(&link).is_ok());

        let preview = preview_uninstall_scope(&root, EnvType::Jdk, "jdk-alias").unwrap();
        // The link object is readable, so the scope is complete: a dangling
        // target must not veto cleaning up the entry itself.
        assert!(
            preview.scope_complete,
            "a dangling target must not make the scope incomplete: {:?}",
            preview.blocked
        );
        assert!(
            !preview
                .blocked
                .iter()
                .any(|item| item.resource == "管理链接目标"),
            "the target being gone is not an unreadable link object"
        );
        assert!(preview.removable.iter().any(|item| {
            item.resource == "失效链接入口"
                && item.details.contains("不会跟随或删除任何目标")
        }));
        // It must not claim to remove a managed target that no longer exists.
        assert!(!preview
            .retained
            .iter()
            .any(|item| item.resource == "受管目标目录"));
    }

    #[tokio::test]
    async fn a_dangling_link_is_removed_without_touching_anything_else() {
        let dir = crate::test_support::TestDir::new();
        let root = dir.path().join("managed");
        let target = dir.path().join("managed/envs/jdks/jdk-old");
        let link = root.join("envs/jdks/jdk-alias");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("payload.txt"), b"keep").unwrap();
        crate::link_ownership::create(&root, &link, &target).unwrap();
        std::fs::remove_dir_all(&target).unwrap();

        let preview = preview_uninstall_scope(&root, EnvType::Jdk, "jdk-alias").unwrap();
        let result =
            execute_uninstall_scope(root.clone(), EnvType::Jdk, "jdk-alias".into(), preview)
                .await
                .unwrap();

        assert_eq!(result.status, UninstallResultStatus::VerifiedScopeRemoved);
        assert!(result.failed.is_empty(), "unexpected failures: {:?}", result.failed);
        // The entry is gone...
        assert!(std::fs::symlink_metadata(&link).is_err());
        // ...and nothing recreated the target in its place.
        assert!(!target.exists());
    }

    #[test]
    fn uninstall_preview_rejects_redirected_environment_category_without_touching_target() {
        let dir = crate::test_support::TestDir::new();
        let root = dir.path().join("managed");
        let redirected = dir.path().join("redirected-jdks");
        std::fs::create_dir_all(&redirected).unwrap();
        std::fs::write(redirected.join("keep.txt"), b"keep").unwrap();
        std::fs::create_dir_all(root.join("envs")).unwrap();
        junction::create(&redirected, root.join("envs/jdks")).unwrap();

        assert!(preview_uninstall_scope(&root, EnvType::Jdk, "jdk-21").is_err());
        assert!(redirected.join("keep.txt").is_file());
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

    #[test]
    fn manifest_records_every_entry_and_states_its_portability_limits() {
        let env = |name: &str, kind: EnvType, current: bool, link: bool| ManagedEnv {
            name: name.into(),
            env_type: kind,
            path: format!("D:/managed/envs/{name}"),
            version: Some("1.0.0".into()),
            size_bytes: None,
            is_current: current,
            status: "available".into(),
            status_detail: None,
            size_complete: false,
            identity_path: format!("D:/managed/envs/{name}"),
            is_link_entry: link,
            is_external_link: false,
        };
        let overview = Overview {
            root: Some("D:/managed".into()),
            root_exists: true,
            portable: false,
            data_dir: "D:/data".into(),
            categories: vec![
                CategoryOverview {
                    env_type: EnvType::Node,
                    envs: vec![env("node-22", EnvType::Node, true, false)],
                    current: Some("node-22".into()),
                    junction_path: "D:/managed/current/node".into(),
                    scan_warning: None,
                    current_error: None,
                },
                CategoryOverview {
                    env_type: EnvType::Jdk,
                    envs: vec![
                        env("jdk-21", EnvType::Jdk, false, false),
                        env("jdk-alias", EnvType::Jdk, true, true),
                    ],
                    current: Some("jdk-alias".into()),
                    junction_path: "D:/managed/current/jdk".into(),
                    scan_warning: None,
                    current_error: None,
                },
            ],
        };

        let manifest = build_manifest(&overview, "fixture-token".into());

        assert_eq!(manifest.schema_version, 1);
        assert_eq!(manifest.generated_at, "fixture-token");
        // Entries from every category are kept, and none are dropped silently.
        assert_eq!(manifest.entries.len(), 3);
        assert!(manifest.entries.iter().any(|entry| {
            entry.name == "jdk-alias" && entry.is_link_entry && entry.is_current
        }));
        // Sorted deterministically, so two exports of the same state diff cleanly.
        let names: Vec<_> = manifest.entries.iter().map(|e| e.name.as_str()).collect();
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(names, sorted);

        // The limits are stated as facts a reader can act on.
        let joined = manifest.compatibility.join("\n");
        assert!(joined.contains("本机绝对路径"));
        assert!(joined.contains("不是可还原的备份"));
        assert!(!joined.contains("便携模式"));
        assert!(manifest
            .notes
            .iter()
            .any(|note| note.contains("不提供“一键恢复环境”")));
    }

    #[test]
    fn manifest_flags_portable_mode_and_a_missing_root() {
        let overview = Overview {
            root: None,
            root_exists: false,
            portable: true,
            data_dir: "E:/portable/data".into(),
            categories: Vec::new(),
        };
        let manifest = build_manifest(&overview, "token".into());
        assert!(manifest.entries.is_empty());
        let joined = manifest.compatibility.join("\n");
        assert!(joined.contains("便携模式"));
        // An empty list with no root must not read as "nothing installed".
        assert!(joined.contains("不代表本机没有安装"));
    }

    #[test]
    fn manifest_target_must_be_an_absolute_json_path() {
        assert!(validate_manifest_target(Path::new("relative/manifest.json")).is_err());
        assert!(validate_manifest_target(Path::new("D:/data/manifest.txt")).is_err());
        assert!(validate_manifest_target(Path::new("D:/data/manifest")).is_err());
        assert!(validate_manifest_target(Path::new("D:/data/manifest.json")).is_ok());
    }

    /// The comparison must classify every entry into exactly one bucket, and it
    /// must not invent differences from a null recorded version.
    #[test]
    fn manifest_comparison_classifies_unchanged_added_missing_and_changed() {
        let entry = |name: &str, version: Option<&str>, status: &str, path: &str| ManifestEntry {
            env_type: EnvType::Node,
            name: name.into(),
            path: path.into(),
            identity_path: path.into(),
            version: version.map(Into::into),
            status: status.into(),
            is_current: false,
            is_link_entry: false,
            is_external_link: false,
        };
        let manifest = |generated_at: &str, root: Option<&str>, entries: Vec<ManifestEntry>| {
            EnvironmentManifest {
                schema_version: 1,
                generated_at: generated_at.into(),
                root: root.map(Into::into),
                portable: false,
                data_dir: "D:/data".into(),
                entries,
                compatibility: Vec::new(),
                notes: Vec::new(),
            }
        };

        let recorded = manifest(
            "then",
            Some("D:/managed"),
            vec![
                // Unchanged, but with a different case and separator on disk.
                entry("node-22", Some("22.0.0"), "available", "D:/managed/envs/node-22"),
                entry("node-20", Some("20.0.0"), "available", "D:/managed/envs/node-20"),
                entry("node-18", Some("18.0.0"), "available", "D:/managed/envs/node-18"),
                entry("node-16", None, "available", "D:/managed/envs/node-16"),
            ],
        );
        let current = manifest(
            "now",
            Some("D:/managed"),
            vec![
                entry("node-22", Some("22.0.0"), "available", "d:\\MANAGED\\envs\\node-22\\"),
                // Version changed.
                entry("node-20", Some("20.1.0"), "available", "D:/managed/envs/node-20"),
                // Status changed.
                entry("node-18", Some("18.0.0"), "broken", "D:/managed/envs/node-18"),
                // Recorded version was null: a probe failure then is not a change.
                entry("node-16", Some("16.0.0"), "available", "D:/managed/envs/node-16"),
                // Present now only.
                entry("node-24", Some("24.0.0"), "available", "D:/managed/envs/node-24"),
            ],
        );

        let comparison = compare_manifests(&recorded, &current);
        assert!(comparison.same_root);
        assert_eq!(comparison.recorded_generated_at, "then");
        assert_eq!(comparison.summary.unchanged, 2);
        assert_eq!(comparison.summary.changed, 2);
        assert_eq!(comparison.summary.added, 1);
        assert_eq!(comparison.summary.missing, 0);

        let row = |name: &str| {
            comparison
                .rows
                .iter()
                .find(|row| row.name == name)
                .unwrap_or_else(|| panic!("missing row {name}"))
        };
        // Case/separator/trailing-slash differences are not reported.
        assert_eq!(row("node-22").kind, ManifestComparisonKind::Unchanged);
        assert!(row("node-22").differences.is_empty());
        assert_eq!(row("node-20").differences, vec!["版本".to_string()]);
        assert_eq!(row("node-18").differences, vec!["状态".to_string()]);
        assert_eq!(row("node-16").kind, ManifestComparisonKind::Unchanged);
        // An added row has no recorded side; the fixture proves it stays empty
        // rather than echoing the current values into the recorded fields.
        assert_eq!(row("node-24").kind, ManifestComparisonKind::Added);
        assert_eq!(row("node-24").recorded_version, None);
        assert_eq!(row("node-24").recorded_path, None);
        // Every row is classified, none dropped.
        assert_eq!(comparison.rows.len(), current.entries.len() + 0);

        // A manifest recorded elsewhere is called out as a cross-machine diff.
        let foreign = manifest("then", Some("E:/other"), vec![entry("node-22", Some("22.0.0"), "available", "E:/other/envs/node-22")]);
        let cross = compare_manifests(&foreign, &current);
        assert!(!cross.same_root);
        assert!(cross.limitations.iter().any(|line| line.contains("跨机器比对")));
        // node-22 exists on both sides, so a different path is a change, not a
        // disappearance; the remaining four local entries are new here.
        assert_eq!(cross.summary.changed, 1);
        assert_eq!(cross.summary.added, 4);
        assert_eq!(cross.summary.missing, 0);
        assert_eq!(
            cross
                .rows
                .iter()
                .find(|row| row.name == "node-22")
                .unwrap()
                .differences,
            vec!["路径".to_string()]
        );
        // The local-only manifest does not carry the cross-machine caveat.
        assert!(!comparison
            .limitations
            .iter()
            .any(|line| line.contains("跨机器比对")));
    }

    /// A missing entry must be reported from the recorded side, not silently
    /// dropped, and the source guard must reject unusable paths before parsing.
    #[test]
    fn manifest_source_must_be_an_existing_absolute_json_file() {
        let dir = crate::test_support::TestDir::new();
        let json = dir.path().join("manifest.json");
        std::fs::write(&json, b"{}").unwrap();
        assert!(validate_manifest_source(&json).is_ok());

        assert!(validate_manifest_source(Path::new("relative/manifest.json")).is_err());
        assert!(validate_manifest_source(&dir.path().join("manifest.txt")).is_err());
        assert!(validate_manifest_source(&dir.path().join("absent.json")).is_err());
        // A directory is not a readable manifest even with the right suffix.
        assert!(validate_manifest_source(dir.path()).is_err());

        let recorded = EnvironmentManifest {
            schema_version: 1,
            generated_at: "then".into(),
            root: Some("D:/managed".into()),
            portable: false,
            data_dir: "D:/data".into(),
            entries: vec![ManifestEntry {
                env_type: EnvType::Jdk,
                name: "jdk-21".into(),
                path: "D:/managed/envs/jdk-21".into(),
                identity_path: "D:/managed/envs/jdk-21".into(),
                version: Some("21.0.1".into()),
                status: "available".into(),
                is_current: true,
                is_link_entry: false,
                is_external_link: false,
            }],
            compatibility: Vec::new(),
            notes: Vec::new(),
        };
        let current = EnvironmentManifest {
            entries: Vec::new(),
            root: Some("D:/managed".into()),
            ..recorded.clone()
        };
        let comparison = compare_manifests(&recorded, &current);
        assert_eq!(comparison.summary.missing, 1);
        let missing = &comparison.rows[0];
        assert_eq!(missing.kind, ManifestComparisonKind::Missing);
        assert_eq!(missing.recorded_version.as_deref(), Some("21.0.1"));
        // No current side: the fixture proves absence is not filled with blanks
        // that would read as an empty version.
        assert_eq!(missing.current_version, None);
        assert_eq!(missing.current_path, None);
    }

    #[test]
    fn package_manager_update_failures_are_structured_for_all_adapters() {
        let definitions = crate::package_managers::definitions();
        assert_eq!(definitions.len(), 10);
        for manager in definitions {
            let command_failed = package_manager_update_result(
                &manager.id,
                "fixture-manager.exe".into(),
                "tool 1.0.0",
                Err("exit 1".into()),
                Err("not queried".into()),
            );
            assert_eq!(command_failed.status, PackageManagerUpdateStatus::CommandFailed);
            assert_eq!(command_failed.error.as_deref(), Some("exit 1"));

            let verification_failed = package_manager_update_result(
                &manager.id,
                "fixture-manager.exe".into(),
                "tool 1.0.0",
                Ok("updated".into()),
                Err("version command failed".into()),
            );
            assert_eq!(
                verification_failed.status,
                PackageManagerUpdateStatus::VerificationFailed,
                "{}",
                manager.id
            );
            assert_eq!(verification_failed.output, "updated");
            assert_eq!(
                verification_failed.error.as_deref(),
                Some("version command failed")
            );

            let changed = package_manager_update_result(
                &manager.id,
                "fixture-manager.exe".into(),
                "tool 1.0.0",
                Ok("updated".into()),
                Ok("tool 1.1.0".into()),
            );
            assert_eq!(changed.status, PackageManagerUpdateStatus::VersionChanged);
            assert_eq!(changed.before_version.as_deref(), Some("1.0.0"));
            assert_eq!(changed.after_version.as_deref(), Some("1.1.0"));
        }
    }

    #[test]
    fn successful_update_with_unparseable_recheck_is_not_reported_as_command_failure() {
        let result = package_manager_update_result(
            "npm",
            "fixture-npm.cmd".into(),
            "npm 10.0.0",
            Ok("update completed".into()),
            Ok("version unknown".into()),
        );
        assert_eq!(result.status, PackageManagerUpdateStatus::VersionUnknown);
        assert_eq!(result.version_changed, None);
        assert_eq!(result.output, "update completed");
        assert_eq!(result.error, None);
    }

    #[test]
    fn update_preview_describes_the_command_without_claiming_a_target_version() {
        let preview = package_manager_update_preview(
            "npm",
            Some("D:/node/npm.cmd".into()),
            Some("10.0.0".into()),
            Ok((
                "D:/node/npm.cmd".into(),
                vec!["install".into(), "--global".into(), "npm@latest".into()],
            )),
            Some(true),
        );
        assert_eq!(preview.tool, "npm");
        assert_eq!(preview.blocked, None);
        assert_eq!(preview.current_version.as_deref(), Some("10.0.0"));
        assert_eq!(preview.update_program.as_deref(), Some("D:/node/npm.cmd"));
        assert_eq!(preview.update_arguments.len(), 3);
        assert_eq!(preview.runs_same_file, Some(true));
        // The preview must never invent a target version: these managers offer
        // no remote "latest" query, so a number here would be a guess.
        assert!(preview
            .notes
            .iter()
            .any(|note| note.contains("无法在更新前得知目标版本")));

        // A blocked manager reports why and carries no command at all.
        let blocked = package_manager_update_preview(
            "pipx",
            Some("D:/Python/Scripts/pipx.exe".into()),
            Some("1.7.1".into()),
            Err("pipx 没有通用自更新命令".into()),
            None,
        );
        assert_eq!(blocked.blocked.as_deref(), Some("pipx 没有通用自更新命令"));
        assert_eq!(blocked.update_program, None);
        assert!(blocked.update_arguments.is_empty());
        assert_eq!(blocked.runs_same_file, None);
        assert!(blocked
            .notes
            .iter()
            .any(|note| note.contains("没有执行任何写入")));

        // `runs_same_file` distinguishes a self-replacement from an update that
        // runs through another program such as the owning interpreter.
        let via_interpreter = package_manager_update_preview(
            "pip",
            Some("D:/Python/Scripts/pip.exe".into()),
            Some("25.0".into()),
            Ok((
                "D:/Python/python.exe".into(),
                vec!["-m".into(), "pip".into(), "install".into()],
            )),
            Some(false),
        );
        assert_eq!(via_interpreter.runs_same_file, Some(false));
        assert!(via_interpreter
            .notes
            .iter()
            .any(|note| note.contains("所属解释器")));
    }

    #[test]
    fn batch_outcomes_separate_skips_from_failures_and_drive_retry() {
        // Every status maps to exactly one batch outcome.
        assert_eq!(
            batch_outcome_for(PackageManagerUpdateStatus::VersionChanged),
            PackageManagerBatchOutcome::Updated
        );
        assert_eq!(
            batch_outcome_for(PackageManagerUpdateStatus::VersionUnchanged),
            PackageManagerBatchOutcome::Unchanged
        );
        assert_eq!(
            batch_outcome_for(PackageManagerUpdateStatus::VerificationFailed),
            PackageManagerBatchOutcome::Unverified
        );
        assert_eq!(
            batch_outcome_for(PackageManagerUpdateStatus::VersionUnknown),
            PackageManagerBatchOutcome::Unverified
        );
        assert_eq!(
            batch_outcome_for(PackageManagerUpdateStatus::CommandFailed),
            PackageManagerBatchOutcome::Failed
        );

        // A skip is a pre-execution decision: it carries its reason, no result,
        // and must never land in `retryable`.
        let skipped = PackageManagerBatchItem {
            tool: "pipx".into(),
            outcome: PackageManagerBatchOutcome::Skipped,
            result: None,
            skipped_reason: Some("pipx 没有通用自更新命令".into()),
        };
        let failed = PackageManagerBatchItem {
            tool: "npm".into(),
            outcome: PackageManagerBatchOutcome::Failed,
            result: Some(package_manager_update_result(
                "npm",
                "fixture-npm.cmd".into(),
                "npm 10.0.0",
                Err("exit 1".into()),
                Err("not queried".into()),
            )),
            skipped_reason: None,
        };
        let updated = PackageManagerBatchItem {
            tool: "uv".into(),
            outcome: PackageManagerBatchOutcome::Updated,
            result: Some(package_manager_update_result(
                "uv",
                "fixture-uv.exe".into(),
                "uv 0.5.0",
                Ok("updated".into()),
                Ok("uv 0.6.0".into()),
            )),
            skipped_reason: None,
        };
        let batch = package_manager_batch_result(vec![skipped, failed, updated]);

        // Order is preserved so the caller can line the report up with its list.
        assert_eq!(
            batch
                .items
                .iter()
                .map(|item| item.tool.as_str())
                .collect::<Vec<_>>(),
            vec!["pipx", "npm", "uv"]
        );
        // Only a real command failure is retryable; a skip is not, even though
        // both end with no version change.
        assert_eq!(batch.retryable, vec!["npm".to_string()]);
        assert!(batch.items[0].result.is_none());
        assert!(batch.items[1].result.is_some());
        assert!(batch.items[0].skipped_reason.is_some());
        assert!(batch.items[1].skipped_reason.is_none());
    }

    #[test]
    fn batch_preview_splits_ready_and_blocked_without_dropping_reasons() {
        let ready = package_manager_update_preview(
            "npm",
            Some("D:/node/npm.cmd".into()),
            Some("10.0.0".into()),
            Ok((
                "D:/node/npm.cmd".into(),
                vec!["install".into(), "--global".into(), "npm@latest".into()],
            )),
            Some(true),
        );
        let blocked = package_manager_update_preview(
            "pipx",
            Some("D:/Python/Scripts/pipx.exe".into()),
            Some("1.7.1".into()),
            Err("pipx 没有通用自更新命令".into()),
            None,
        );
        let items = vec![ready, blocked];
        let split: (Vec<_>, Vec<_>) = items
            .iter()
            .partition::<Vec<_>, _>(|item| item.blocked.is_none());
        assert_eq!(split.0.len(), 1);
        assert_eq!(split.1.len(), 1);
        // The blocked entry keeps its reason for display instead of vanishing.
        assert!(split.1[0].blocked.is_some());
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn update_blocker_matches_the_executor_for_berry_yarn_only() {
        // Classic Yarn and every other manager are allowed.
        assert_eq!(package_manager_update_blocker("yarn", "1.22.22"), None);
        assert_eq!(package_manager_update_blocker("npm", "10.0.0"), None);
        // Berry is refused by the same rule the executor uses, so the preview
        // cannot promise an update the executor would reject.
        assert!(package_manager_update_blocker("yarn", "4.1.0")
            .unwrap()
            .contains("Yarn Berry"));
        // An unparseable version is not silently treated as Classic.
        assert!(package_manager_update_blocker("yarn", "").is_some());
    }
}
