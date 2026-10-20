use crate::{
    error::{AppError, Result},
    package_managers::{ItemResult, Package},
};
use crate::{
    fsutil::plain_path,
    package_managers::{
        installed_packages,
        node::{inventory, modules_dir, valid_name, valid_version},
    },
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashSet},
    io::Write,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
    time::Duration,
};
use tokio::process::Command;

#[derive(Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    #[serde(default)]
    pub allow_unknown_registry: bool,
    pub tool: String,
    pub executable: String,
    pub tool_version: String,
    pub source: String,
    pub destination: String,
    pub bin_path: String,
    pub cache_path: String,
    pub packages: Vec<Package>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub tool: String,
    pub status: String,
    pub message: String,
    pub destination: String,
    pub bin_path: String,
    pub report: String,
    pub items: Vec<ItemResult>,
}

static TASK: Mutex<Option<Snapshot>> = Mutex::new(None);
static HISTORY: Mutex<BTreeMap<String, Snapshot>> = Mutex::new(BTreeMap::new());
static CANCEL: AtomicBool = AtomicBool::new(false);

fn launcher(value: &str) -> Result<PathBuf> {
    let p = Path::new(value);
    if !p.is_absolute()
        || !p.is_file()
        || !p.extension().is_some_and(|e| {
            ["exe", "cmd", "bat"]
                .iter()
                .any(|s| e.eq_ignore_ascii_case(s))
        })
    {
        return Err(AppError::msg(
            "请选择包管理器 .cmd/.exe/.bat 启动器的绝对路径",
        ));
    }
    Ok(plain_path(p.canonicalize()?))
}

fn destination(value: &str) -> Result<PathBuf> {
    let p = Path::new(value);
    if !p.is_absolute()
        || p.file_name().is_none()
        || p.components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(AppError::msg("目标必须是绝对路径，且不能包含上级目录"));
    }
    match std::fs::symlink_metadata(p) {
        Ok(meta) => {
            if !meta.is_dir() || meta.file_type().is_symlink() {
                return Err(AppError::msg("目标必须是普通目录，不能是文件或目录链接"));
            }
            let entries = std::fs::read_dir(p)?
                .take(5)
                .map(|e| e.map(|e| e.file_name().to_string_lossy().into_owned()))
                .collect::<std::io::Result<Vec<_>>>()?;
            if !entries.is_empty() {
                return Err(AppError::msg(format!(
                    "目标目录非空，存在冲突内容：{}。请选择空目录，避免覆盖已有包",
                    entries.join("、")
                )));
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    Ok(plain_path(
        p.parent()
            .unwrap()
            .canonicalize()
            .map_err(|_| AppError::msg("请选择已存在的目标父目录"))?
            .join(p.file_name().unwrap()),
    ))
}

fn command(
    executable: &Path,
    working: &Path,
    tool: &str,
    dest: Option<&Path>,
    args: &[String],
) -> Command {
    let mut cmd = Command::new(executable);
    cmd.args(args)
        .current_dir(working)
        .creation_flags(0x08000000)
        .kill_on_drop(true);
    for (key, _) in std::env::vars_os() {
        let name = key.to_string_lossy().to_ascii_uppercase();
        if [
            "NPM_CONFIG_",
            "PNPM_",
            "YARN_",
            "BUN_",
            "UV_",
            "PIP_",
            "PIPX_",
            "PYTHON",
            "COMPOSER",
            "CARGO_",
            "NUGET_",
            "DOTNET_",
        ]
        .iter()
        .any(|prefix| name.starts_with(prefix))
            || name == "NODE_OPTIONS"
        {
            cmd.env_remove(key);
        }
    }
    let mut path = vec![executable.parent().unwrap().to_path_buf()];
    if let Some(dest) = dest {
        path.push(crate::package_managers::bin_path(tool, dest));
    }
    path.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    if let Ok(path) = std::env::join_paths(path) {
        cmd.env("PATH", path);
    }
    cmd.env("COREPACK_ENABLE_NETWORK", "0")
        .env("COREPACK_ENABLE_PROJECT_SPEC", "0");
    if let Some(dest) = dest {
        if crate::package_managers::isolated(tool) {
            cmd.envs(crate::package_managers::commands::environment(
                tool, dest, dest,
            ));
        }
        cmd.env(
            "NPM_CONFIG_CACHE",
            crate::package_managers::cache_path(tool, dest),
        );
        let config = crate::package_managers::config_path(tool, dest);
        cmd.env("NPM_CONFIG_USERCONFIG", config.join(".envcon-user.npmrc"));
        cmd.env(
            "NPM_CONFIG_GLOBALCONFIG",
            config.join(".envcon-global.npmrc"),
        );
        if tool == "pnpm" {
            cmd.env("PNPM_HOME", crate::package_managers::bin_path(tool, dest));
        }
    }
    cmd
}

async fn run(
    exe: &Path,
    working: &Path,
    tool: &str,
    dest: Option<&Path>,
    args: &[String],
    seconds: u64,
) -> Result<String> {
    let output = crate::reinstall_process::output(
        command(exe, working, tool, dest, args),
        Duration::from_secs(seconds),
    )
    .await?;
    if !output.status.success() {
        let text = format!(
            "{}\n{}",
            String::from_utf8_lossy(&output.stderr),
            String::from_utf8_lossy(&output.stdout)
        );
        return Err(AppError::msg(format!(
            "退出码 {:?}: {}",
            output.status.code(),
            text.chars()
                .rev()
                .take(4000)
                .collect::<String>()
                .chars()
                .rev()
                .collect::<String>()
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|s| (*s).into()).collect()
}

fn create_destination_layout(tool: &str, dest: &Path) -> Result<Vec<PathBuf>> {
    let bin = bin_path(tool, dest);
    let cache = crate::package_managers::cache_path(tool, dest);
    let config = crate::package_managers::config_path(tool, dest);
    crate::package_managers::validate_layout(
        tool,
        Some(dest),
        Some(&bin),
        Some(&cache),
        Some(&config),
    )?;
    let mut roles = vec![
        crate::package_managers::prefix_path(tool, dest),
        bin,
        cache,
        config,
    ];
    if tool == "composer" {
        roles.push(dest.join("vendor"));
    }
    roles.sort_by_key(|path| path.components().count());
    roles.dedup_by(|a, b| crate::detect::system::paths_eq(a, b));
    let mut created = Vec::new();
    for role in roles {
        if crate::detect::system::paths_eq(dest, &role) {
            continue;
        }
        match std::fs::create_dir(&role) {
            Ok(()) => created.push(role),
            Err(error) => {
                for path in created.iter().rev() {
                    let _ = std::fs::remove_dir(path);
                }
                return Err(AppError::msg(format!(
                    "角色目录已存在或无法独占创建 {}：{error}",
                    role.display()
                )));
            }
        }
    }
    Ok(created)
}

fn remove_empty_roles(paths: &[PathBuf]) {
    for path in paths.iter().rev() {
        let _ = std::fs::remove_dir(path);
    }
}

fn prepare_configs(tool: &str, dest: &Path) -> Result<()> {
    let mut created = Vec::new();
    let config = crate::package_managers::config_path(tool, dest);
    for name in [".envcon-user.npmrc", ".envcon-global.npmrc"] {
        let path = config.join(name);
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(_) => created.push(path),
            Err(error) => {
                for path in created.iter().rev() {
                    let _ = std::fs::remove_file(path);
                }
                return Err(error.into());
            }
        }
    }
    Ok(())
}

fn initialize_destination(tool: &str, value: &str) -> Result<PathBuf> {
    let dest = destination(value)?;
    let root_created = !dest.exists();
    if root_created {
        std::fs::create_dir(&dest)?;
    }
    destination(&dest.to_string_lossy())?;
    let roles = match create_destination_layout(tool, &dest) {
        Ok(roles) => roles,
        Err(error) => {
            if root_created {
                let _ = std::fs::remove_dir(&dest);
            }
            return Err(error);
        }
    };
    if let Err(error) = prepare_configs(tool, &dest) {
        remove_empty_roles(&roles);
        if root_created {
            let _ = std::fs::remove_dir(&dest);
        }
        return Err(error);
    }
    Ok(dest)
}

fn bin_path(tool: &str, dest: &Path) -> PathBuf {
    crate::package_managers::bin_path(tool, dest)
}

fn manager_version(tool: &str, output: &str) -> String {
    if crate::package_managers::isolated(tool) {
        output
            .split_whitespace()
            .find(|s| valid_version(s))
            .unwrap_or("")
            .into()
    } else {
        output.into()
    }
}

struct OwnedPreviewDirectory {
    path: PathBuf,
    marker: PathBuf,
    token: String,
}

impl OwnedPreviewDirectory {
    fn create(path: PathBuf) -> Result<Self> {
        std::fs::create_dir(&path)?;
        let marker = path.join(".envcon-preview-owner");
        let token = format!(
            "{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        );
        let mut file = match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&marker)
        {
            Ok(file) => file,
            Err(error) => {
                let _ = std::fs::remove_dir(&path);
                return Err(error.into());
            }
        };
        if let Err(error) = file.write_all(token.as_bytes()) {
            drop(file);
            let _ = std::fs::remove_file(&marker);
            let _ = std::fs::remove_dir(&path);
            return Err(error.into());
        }
        Ok(Self {
            path,
            marker,
            token,
        })
    }

    fn cleanup(&self) {
        let Ok(metadata) = std::fs::symlink_metadata(&self.path) else {
            return;
        };
        if !metadata.is_dir() || crate::detect::system::is_reparse_path(&self.path) {
            return;
        }
        let marker_is_owned = std::fs::symlink_metadata(&self.marker)
            .is_ok_and(|metadata| metadata.is_file())
            && !crate::detect::system::is_reparse_path(&self.marker)
            && std::fs::read_to_string(&self.marker).is_ok_and(|token| token == self.token);
        if marker_is_owned {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}

struct PreviewDir {
    root: OwnedPreviewDirectory,
    owned_roles: Vec<OwnedPreviewDirectory>,
}

impl PreviewDir {
    fn create(tool: &str, path: PathBuf) -> Result<Self> {
        crate::package_managers::validate_layout(
            tool,
            Some(&path),
            Some(&bin_path(tool, &path)),
            Some(&crate::package_managers::cache_path(tool, &path)),
            Some(&crate::package_managers::config_path(tool, &path)),
        )?;
        let root = OwnedPreviewDirectory::create(path.clone())?;
        let mut owned_roles: Vec<OwnedPreviewDirectory> = Vec::new();
        let mut candidates = vec![
            crate::package_managers::prefix_path(tool, &path),
            bin_path(tool, &path),
            crate::package_managers::cache_path(tool, &path),
            crate::package_managers::config_path(tool, &path),
        ];
        candidates.dedup_by(|a, b| crate::detect::system::paths_eq(a, b));
        for role in candidates {
            if crate::detect::system::path_within(&path, &role)
                || owned_roles
                    .iter()
                    .any(|existing| crate::detect::system::paths_eq(&existing.path, &role))
            {
                continue;
            }
            // Only claim directories created atomically by this preview. Reusing an
            // existing sibling here would make Drop capable of deleting user data.
            match OwnedPreviewDirectory::create(role) {
                Ok(owned) => owned_roles.push(owned),
                Err(error) => {
                    for existing in owned_roles.iter().rev() {
                        existing.cleanup();
                    }
                    root.cleanup();
                    return Err(error);
                }
            }
        }
        let scratch = Self { root, owned_roles };
        if let Err(error) = prepare_configs(tool, &scratch.root.path) {
            drop(scratch);
            return Err(error);
        }
        Ok(scratch)
    }

    fn path(&self) -> &Path {
        &self.root.path
    }
}

impl Drop for PreviewDir {
    fn drop(&mut self) {
        for role in self.owned_roles.iter().rev() {
            role.cleanup();
        }
        self.root.cleanup();
    }
}

pub async fn preview(
    tool: String,
    executable: String,
    source: String,
    dest: String,
) -> Result<Plan> {
    if crate::package_managers::definition(&tool)?.reinstall
        != crate::package_managers::ReinstallKind::Global
    {
        return Err(AppError::msg("此管理器使用独立虚拟环境重装入口"));
    }
    let executable = launcher(&executable)?;
    let destination = destination(&dest)?;
    let source = PathBuf::from(&source);
    if !source.is_absolute() || !source.is_dir() {
        return Err(AppError::msg("请选择旧全局包目录"));
    }
    let source = plain_path(source.canonicalize()?);
    if crate::detect::system::path_within(&source, &destination)
        || crate::detect::system::path_within(&destination, &source)
        || crate::detect::system::path_within(&destination, &executable)
    {
        return Err(AppError::msg(
            "目标不能与旧包目录重叠，也不能包含包管理器启动器",
        ));
    }
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let scratch =
        std::env::temp_dir().join(format!("envcon-preview-{}-{stamp}", std::process::id()));
    let scratch = PreviewDir::create(&tool, scratch)?;
    let version_args = if tool == "yarn" {
        args(&["--no-default-rc", "--version"])
    } else {
        args(&["--version"])
    };
    let version = manager_version(
        &tool,
        &run(
            &executable,
            &scratch.path(),
            &tool,
            Some(&scratch.path()),
            &version_args,
            30,
        )
        .await?,
    );
    if !valid_version(&version) {
        return Err(AppError::msg("无法识别包管理器版本"));
    }
    if tool == "yarn" && !version.starts_with("1.") {
        return Err(AppError::msg(
            "Yarn Berry 不支持 global add；请选择 Yarn Classic 1.x，或使用其他管理器重装旧包",
        ));
    }
    let scan_tool = tool.clone();
    let scan_source = source.clone();
    let packages = tokio::task::spawn_blocking(move || {
        installed_packages(&scan_tool, &scan_source)
            .map(|rows| rows.into_iter().map(|(p, _)| p).collect())
    })
    .await
    .map_err(|e| AppError::msg(e.to_string()))??;
    Ok(Plan {
        allow_unknown_registry: false,
        tool: tool.clone(),
        executable: executable.to_string_lossy().into(),
        tool_version: version,
        source: source.to_string_lossy().into(),
        bin_path: bin_path(&tool, &destination).to_string_lossy().into(),
        cache_path: if tool == "cargo" {
            destination.join(".cargo/registry")
        } else {
            crate::package_managers::cache_path(&tool, &destination)
        }
        .to_string_lossy()
        .into(),
        destination: destination.to_string_lossy().into(),
        packages,
    })
}

pub(crate) fn allow_registry_recovery(plan: &mut Plan, allow: bool) {
    plan.allow_unknown_registry = allow && plan.tool == "npm";
    if plan.allow_unknown_registry {
        for package in &mut plan.packages {
            if package.reason.as_deref() == Some(crate::package_managers::node::UNKNOWN_ORIGIN) {
                package.reason = None;
            }
        }
    }
}

fn install_args(tool: &str, dest: &Path, pin: &str) -> Result<Vec<String>> {
    let bin = crate::package_managers::bin_path(tool, dest)
        .to_string_lossy()
        .into_owned();
    let prefix = crate::package_managers::prefix_path(tool, dest)
        .to_string_lossy()
        .into_owned();
    let cache = crate::package_managers::cache_path(tool, dest)
        .to_string_lossy()
        .into_owned();
    Ok(match tool {
        "npm" => args(&[
            "install",
            "--global",
            "--prefix",
            &dest.to_string_lossy(),
            "--no-audit",
            "--no-fund",
            "--registry=https://registry.npmjs.org/",
            "--",
            pin,
        ]),
        "pnpm" => args(&[
            "add",
            "--global",
            "--save-exact",
            "--global-dir",
            &dest.to_string_lossy(),
            "--global-bin-dir",
            &bin,
            "--store-dir",
            &cache,
            "--reporter",
            "append-only",
            "--",
            pin,
        ]),
        "bun" => args(&["add", "--global", "--exact", "--no-progress", "--", pin]),
        "yarn" => args(&[
            "global",
            "add",
            "--exact",
            "--global-folder",
            &dest.to_string_lossy(),
            "--prefix",
            &prefix,
            "--cache-folder",
            &cache,
            "--no-default-rc",
            "--non-interactive",
            "--",
            pin,
        ]),
        _ => return Err(AppError::msg("没有对应的 Node 安装命令适配器")),
    })
}

pub fn status(tool: &str) -> Option<Snapshot> {
    let current = TASK.lock().unwrap().clone().filter(|t| t.tool == tool);
    current.or_else(|| HISTORY.lock().unwrap().get(tool).cloned())
}
pub fn cancel(tool: &str) {
    if TASK
        .lock()
        .unwrap()
        .as_ref()
        .is_some_and(|t| t.tool == tool && t.status == "running")
    {
        CANCEL.store(true, Ordering::SeqCst);
    }
}
fn update(f: impl FnOnce(&mut Snapshot)) -> Result<()> {
    if let Some(task) = TASK.lock().unwrap().as_mut() {
        f(task);
        std::fs::write(&task.report, serde_json::to_vec_pretty(task)?)?;
    }
    Ok(())
}

pub async fn start(plan: Plan, names: Vec<String>) -> Result<()> {
    let guard = crate::migration::LOCK
        .try_lock()
        .map_err(|_| AppError::msg("已有重装、迁移或配置任务正在执行"))?;
    let mut fresh = preview(
        plan.tool.clone(),
        plan.executable.clone(),
        plan.source.clone(),
        plan.destination.clone(),
    )
    .await?;
    allow_registry_recovery(&mut fresh, plan.allow_unknown_registry);
    if fresh != plan {
        return Err(AppError::msg("源清单或工具版本变化，请重新预览"));
    }
    let mut selected = Vec::new();
    let mut seen = HashSet::new();
    for name in names {
        let p = fresh
            .packages
            .iter()
            .find(|p| p.name == name && p.reason.is_none())
            .ok_or_else(|| AppError::msg("包不在可重装清单中"))?;
        if seen.insert(name) {
            selected.push(p.clone());
        }
    }
    if selected.is_empty() {
        return Err(AppError::msg("请至少选择一个包"));
    }
    let dest = initialize_destination(&fresh.tool, &fresh.destination)?;
    crate::package_managers::commands::prepare(&fresh.tool, &dest)?;
    std::fs::write(
        dest.join("envcon-source.json"),
        serde_json::to_vec_pretty(&fresh)?,
    )?;
    let task = Snapshot {
        tool: fresh.tool.clone(),
        status: "running".into(),
        message: "准备重装".into(),
        destination: fresh.destination.clone(),
        bin_path: fresh.bin_path.clone(),
        report: dest.join("envcon-reinstall.json").to_string_lossy().into(),
        items: selected
            .iter()
            .map(|p| ItemResult {
                name: p.name.clone(),
                version: p.version.clone(),
                status: "pending".into(),
                detail: None,
            })
            .collect(),
    };
    std::fs::write(&task.report, serde_json::to_vec_pretty(&task)?)?;
    if let Some(previous) = TASK.lock().unwrap().replace(task) {
        HISTORY
            .lock()
            .unwrap()
            .insert(previous.tool.clone(), previous);
    }
    CANCEL.store(false, Ordering::SeqCst);
    crate::old_packages::remember(
        &fresh.tool,
        crate::old_packages::SourcePlan::Global(fresh.clone()),
    );
    tauri::async_runtime::spawn(async move {
        let _guard = guard;
        if let Err(e) = install(&fresh, &selected).await {
            let _ = update(|t| {
                t.status = "error".into();
                t.message = e.to_string();
            });
        }
    });
    Ok(())
}

async fn install(plan: &Plan, packages: &[Package]) -> Result<()> {
    let dest = Path::new(&plan.destination);
    for (i, package) in packages.iter().enumerate() {
        if CANCEL.load(Ordering::SeqCst) {
            break;
        }
        update(|t| {
            t.message = format!("正在重装 {}@{}", package.name, package.version);
            t.items[i].status = "installing".into();
        })?;
        let arguments = if crate::package_managers::node(&plan.tool) {
            install_args(
                &plan.tool,
                dest,
                &format!("{}@{}", package.name, package.version),
            )?
        } else {
            crate::package_managers::commands::invocation(
                &plan.tool,
                dest,
                dest,
                crate::package_managers::commands::Action::Install(package),
            )?
            .arguments
        };
        let result = run(
            Path::new(&plan.executable),
            dest,
            &plan.tool,
            Some(dest),
            &arguments,
            600,
        )
        .await;
        update(|t| match result {
            Ok(_) => t.items[i].status = "installed".into(),
            Err(e) => {
                t.items[i].status = "failed".into();
                t.items[i].detail = Some(e.to_string());
            }
        })?;
    }
    let verified = installed_packages(&plan.tool, dest).map(|rows| {
        let mut verified = plan.clone();
        verified.packages = rows.into_iter().map(|(p, _)| p).collect();
        allow_registry_recovery(&mut verified, plan.allow_unknown_registry);
        verified.packages
    });
    update(|t| finish(t, &verified, CANCEL.load(Ordering::SeqCst)))
}

fn cleanup_anchor(tool: &str, source: &Path) -> Result<PathBuf> {
    if !crate::package_managers::node(tool) {
        return Ok(source.canonicalize()?);
    }
    if tool == "pnpm" {
        if source.join("v11").is_dir() {
            return Ok(source.join("v11").canonicalize()?);
        }
        if source.file_name().is_some_and(|n| n == "v11") {
            return Ok(source.canonicalize()?);
        }
    }
    Ok(modules_dir(source)?.canonicalize()?)
}

fn cleanup_scope(tool: &str, source: &Path) -> Result<PathBuf> {
    let mut scope = source.to_path_buf();
    if scope
        .file_name()
        .is_some_and(|n| n.eq_ignore_ascii_case("node_modules"))
    {
        scope.pop();
    }
    if tool == "pnpm"
        && scope
            .file_name()
            .is_some_and(|n| n == "v11" || n.to_string_lossy().bytes().all(|b| b.is_ascii_digit()))
    {
        scope.pop();
    }
    if scope.parent().is_none() {
        return Err(AppError::msg("无法确定旧全局目录"));
    }
    Ok(scope)
}

fn cleanup_args(tool: &str, scope: &Path, scratch: &Path, name: Option<&str>) -> Vec<String> {
    let scope = scope.to_string_lossy();
    let bin = crate::package_managers::bin_path(tool, scratch)
        .to_string_lossy()
        .into_owned();
    let cache = crate::package_managers::cache_path(tool, scratch)
        .to_string_lossy()
        .into_owned();
    let prefix = crate::package_managers::prefix_path(tool, scratch)
        .to_string_lossy()
        .into_owned();
    let mut result = match (tool, name) {
        ("npm", None) => args(&["root", "--global", "--prefix", &scope]),
        ("npm", Some(_)) => args(&[
            "uninstall",
            "--global",
            "--prefix",
            &scope,
            "--ignore-scripts",
            "--no-audit",
            "--no-fund",
        ]),
        ("pnpm", None) => args(&[
            "root",
            "--global",
            &format!("--config.global-dir={scope}"),
            &format!("--config.global-bin-dir={bin}"),
        ]),
        ("pnpm", Some(_)) => args(&[
            "remove",
            "--global",
            &format!("--config.global-dir={scope}"),
            &format!("--config.global-bin-dir={bin}"),
            "--config.ignore-scripts=true",
            "--reporter",
            "append-only",
        ]),
        (_, None) => args(&[
            "global",
            "dir",
            "--global-folder",
            &scope,
            "--prefix",
            &prefix,
            "--no-default-rc",
            "--silent",
        ]),
        (_, Some(_)) => args(&[
            "global",
            "remove",
            "--global-folder",
            &scope,
            "--prefix",
            &prefix,
            "--cache-folder",
            &cache,
            "--no-default-rc",
            "--non-interactive",
            "--ignore-scripts",
            "--offline",
        ]),
    };
    if let Some(name) = name {
        result.extend(args(&["--", name]));
    }
    result
}

fn cleanup_scratch(tool: &str, destination: &Path) -> Result<PreviewDir> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let scratch = destination.join(format!(".envcon-cleanup-{}-{stamp}", std::process::id()));
    PreviewDir::create(tool, scratch)
}

async fn verify_cleanup_scope(plan: &Plan, scratch: &Path) -> Result<PathBuf> {
    let source = Path::new(&plan.source);
    crate::old_packages::verify_separate(source, Path::new(&plan.destination))?;
    if let Some(issue) = crate::package_managers::legacy_layout_issue(&plan.tool, source) {
        return Err(AppError::msg(format!(
            "{issue}；旧入口归属未确认前不允许通过清理命令删除"
        )));
    }
    let anchor = cleanup_anchor(&plan.tool, source)?;
    if !crate::detect::system::path_within(source, &anchor) {
        return Err(AppError::msg("旧包目录指向来源以外，禁止清理"));
    }
    let exe = launcher(&plan.executable)?;
    let version = manager_version(
        &plan.tool,
        &run(
            &exe,
            scratch,
            &plan.tool,
            Some(scratch),
            &args(&["--version"]),
            30,
        )
        .await?,
    );
    if version != plan.tool_version {
        return Err(AppError::msg("包管理器版本已变化，请保留旧包并重新核对"));
    }
    let scope = cleanup_scope(&plan.tool, source)?;
    if crate::package_managers::isolated(&plan.tool) {
        return Ok(scope);
    }
    let queried = run(
        &exe,
        scratch,
        &plan.tool,
        Some(scratch),
        &cleanup_args(&plan.tool, &scope, scratch, None),
        30,
    )
    .await?;
    let queried = Path::new(&queried);
    let actual = if plan.tool == "yarn" {
        cleanup_anchor("yarn", queried)?
    } else {
        queried.canonicalize()?
    };
    if !crate::detect::system::paths_eq(&actual, &anchor) {
        return Err(AppError::msg(
            "包管理器解析出的卸载目录与原清单不一致，禁止清理",
        ));
    }
    Ok(scope)
}

pub(crate) async fn scanned_cleanup_plan(
    tool: String,
    executable: String,
    source: String,
    reports: String,
) -> Result<Plan> {
    crate::package_managers::definition(&tool)?;
    let source_path = Path::new(&source);
    if !source_path.is_absolute() || !source_path.is_dir() {
        return Err(AppError::msg("请选择已存在的旧包目录"));
    }
    crate::old_packages::verify_separate(source_path, Path::new(&reports))?;
    let executable = launcher(&executable)?;
    let scratch = cleanup_scratch(&tool, Path::new(&reports))?;
    let version = manager_version(
        &tool,
        &run(
            &executable,
            &scratch.path(),
            &tool,
            Some(&scratch.path()),
            &args(&["--version"]),
            30,
        )
        .await?,
    );
    if !valid_version(&version) || (tool == "yarn" && !version.starts_with("1.")) {
        return Err(AppError::msg("无法确认兼容的包管理器版本"));
    }
    let plan = Plan {
        allow_unknown_registry: false,
        tool,
        executable: executable.to_string_lossy().into(),
        tool_version: version,
        source,
        destination: reports,
        bin_path: String::new(),
        cache_path: String::new(),
        packages: vec![],
    };
    verify_cleanup_scope(&plan, &scratch.path()).await?;
    Ok(plan)
}

pub(crate) async fn scanned_cleanup_candidates(
    plan: &Plan,
) -> Result<Vec<crate::old_packages::Candidate>> {
    let scratch = cleanup_scratch(&plan.tool, Path::new(&plan.destination))?;
    verify_cleanup_scope(plan, &scratch.path()).await?;
    let tool = plan.tool.clone();
    let source = PathBuf::from(&plan.source);
    tokio::task::spawn_blocking(move || {
        installed_packages(&tool, &source)?
            .into_iter()
            .map(|(p, path)| {
                let mut reason = p.reason;
                if reason.as_deref() == Some(crate::package_managers::node::UNKNOWN_ORIGIN) {
                    reason = None;
                }
                if matches!(
                    p.name.as_str(),
                    "npm"
                        | "pnpm"
                        | "yarn"
                        | "corepack"
                        | "bun"
                        | "uv"
                        | "pipx"
                        | "cargo"
                        | "composer"
                        | "dotnet"
                ) {
                    reason = Some("包管理器自身不能通过旧包删除入口卸载".into());
                }
                if path
                    .as_ref()
                    .is_some_and(|p| !crate::detect::system::path_within(&source, p))
                {
                    reason = Some("包文件位于来源目录之外".into());
                }
                if crate::package_managers::isolated(&tool) {
                    if let Err(e) =
                        crate::package_managers::verify_cleanup_files(&tool, &source, &p.name)
                    {
                        reason = Some(e.to_string());
                    }
                }
                Ok(crate::old_packages::Candidate {
                    name: p.name,
                    version: p.version,
                    path: path.map(|p| p.to_string_lossy().into()),
                    reason,
                })
            })
            .collect()
    })
    .await
    .map_err(|e| AppError::msg(e.to_string()))?
}

pub(crate) async fn cleanup_candidates(
    plan: &Plan,
    items: &[ItemResult],
) -> Result<Vec<crate::old_packages::Candidate>> {
    crate::old_packages::verify_separate(Path::new(&plan.source), Path::new(&plan.destination))?;
    let scratch = cleanup_scratch(&plan.tool, Path::new(&plan.destination))?;
    verify_cleanup_scope(plan, &scratch.path()).await?;
    let old = installed_packages(&plan.tool, Path::new(&plan.source))?;
    let dest = Path::new(&plan.destination);
    let target_root = dest.to_path_buf();
    if !crate::detect::system::path_within(dest, &cleanup_anchor(&plan.tool, &target_root)?) {
        return Err(AppError::msg("新包目录指向目标以外，禁止清理"));
    }
    let target: Vec<_> = installed_packages(&plan.tool, &target_root)?
        .into_iter()
        .map(|(p, _)| p)
        .collect();
    Ok(items
        .iter()
        .map(|item| {
            let old = old.iter().find(|(p, _)| p.name == item.name);
            let original = plan.packages.iter().find(|p| p.name == item.name);
            let cleanup_error = if crate::package_managers::isolated(&plan.tool) {
                crate::package_managers::verify_cleanup_files(
                    &plan.tool,
                    Path::new(&plan.source),
                    &item.name,
                )
                .err()
                .map(|e| e.to_string())
            } else {
                None
            };
            let reason = if !original
                .is_some_and(|p| p.reason.is_none() && p.version == item.version)
            {
                Some("不在原可重装清单中".into())
            } else if !target.iter().any(|p| {
                p.name == item.name
                    && p.version == item.version
                    && (p.reason.is_none()
                        || (plan.tool == "npm"
                            && plan.allow_unknown_registry
                            && p.reason.as_deref()
                                == Some(crate::package_managers::node::UNKNOWN_ORIGIN)))
            }) {
                Some("新目录中要求的包版本不存在或来源异常".into())
            } else if cleanup_error.is_some() {
                cleanup_error
            } else {
                match old {
                    None => Some("旧包已不存在".into()),
                    Some((p, _)) if p.version != item.version => Some("旧包版本已变化".into()),
                    Some((_, Some(path)))
                        if !crate::detect::system::path_within(Path::new(&plan.source), path) =>
                    {
                        Some("旧包文件指向来源目录以外".into())
                    }
                    Some((p, _))
                        if plan.allow_unknown_registry
                            && p.reason.as_deref()
                                == Some(crate::package_managers::node::UNKNOWN_ORIGIN) =>
                    {
                        None
                    }
                    Some((p, _)) => p.reason.clone(),
                }
            };
            crate::old_packages::Candidate {
                name: item.name.clone(),
                version: item.version.clone(),
                path: old
                    .and_then(|(_, p)| p.as_ref())
                    .map(|p| p.to_string_lossy().into()),
                reason,
            }
        })
        .collect())
}

pub(crate) async fn remove_old(
    plan: &Plan,
    package: &crate::old_packages::Candidate,
) -> Result<()> {
    let name_valid = if plan.tool == "composer" {
        package
            .name
            .split_once('/')
            .is_some_and(|(vendor, name)| valid_name(vendor) && valid_name(name))
    } else {
        valid_name(&package.name)
    };
    if !name_valid {
        return Err(AppError::msg("无效包名"));
    }
    let scratch = cleanup_scratch(&plan.tool, Path::new(&plan.destination))?;
    let scope = verify_cleanup_scope(plan, &scratch.path()).await?;
    if crate::package_managers::isolated(&plan.tool) {
        crate::package_managers::verify_cleanup_files(&plan.tool, &scope, &package.name)?;
        let request = crate::package_managers::commands::invocation(
            &plan.tool,
            &scope,
            &scratch.path(),
            crate::package_managers::commands::Action::Remove(&package.name),
        )?;
        let mut cmd = command(
            Path::new(&plan.executable),
            &scratch.path(),
            &plan.tool,
            Some(&scratch.path()),
            &request.arguments,
        );
        cmd.envs(request.environment);
        let output = crate::reinstall_process::output(cmd, Duration::from_secs(180)).await?;
        if !output.status.success() {
            return Err(AppError::msg(format!(
                "卸载失败 {:?}: {}",
                output.status.code(),
                String::from_utf8_lossy(&output.stderr)
                    .chars()
                    .take(2000)
                    .collect::<String>()
            )));
        }
        if installed_packages(&plan.tool, Path::new(&plan.source))?
            .iter()
            .any(|(p, _)| p.name == package.name)
        {
            return Err(AppError::msg("卸载结束，但原包仍存在"));
        }
        return Ok(());
    }
    run(
        Path::new(&plan.executable),
        &scratch.path(),
        &plan.tool,
        Some(&scratch.path()),
        &cleanup_args(&plan.tool, &scope, &scratch.path(), Some(&package.name)),
        180,
    )
    .await?;
    let remaining = inventory(&plan.tool, Path::new(&plan.source))?;
    if remaining.iter().any(|p| p.name == package.name) {
        return Err(AppError::msg("卸载命令结束，但旧包仍在原目录中"));
    }
    Ok(())
}

fn finish(t: &mut Snapshot, verified: &Result<Vec<Package>>, canceled: bool) {
    for item in t.items.iter_mut().filter(|p| p.status == "installed") {
        if !verified.as_ref().is_ok_and(|rows| {
            rows.iter()
                .any(|p| p.name == item.name && p.version == item.version && p.reason.is_none())
        }) {
            item.status = "failed".into();
            item.detail = Some("最终目标中未找到要求版本的安装元数据".into());
        }
    }
    t.status = if canceled {
        "canceled"
    } else if t.items.iter().all(|p| p.status == "installed") {
        "done"
    } else {
        "partial"
    }
    .into();
    for item in t.items.iter_mut().filter(|p| p.status == "pending") {
        item.status = "canceled".into();
    }
    t.message = "重装结束；已核对包版本，请验证命令后再配置新路径".into();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_dir_refuses_preexisting_sibling_roles_without_deleting_them() {
        let d = crate::test_support::TestDir::new();
        let scratch = d.path().join("preview-uv");
        let existing_bin = d.path().join("preview-uv-bin");
        std::fs::create_dir(&existing_bin).unwrap();
        std::fs::write(existing_bin.join("keep.txt"), "keep").unwrap();

        assert!(PreviewDir::create("uv", scratch.clone()).is_err());
        assert!(!scratch.exists());
        assert_eq!(
            std::fs::read_to_string(existing_bin.join("keep.txt")).unwrap(),
            "keep"
        );
    }

    #[test]
    fn preview_dir_refuses_preexisting_junction_roles_without_touching_targets() {
        let d = crate::test_support::TestDir::new();
        let scratch = d.path().join("preview-uv");
        let target = d.path().join("existing-bin-target");
        let existing_bin = d.path().join("preview-uv-bin");
        std::fs::create_dir(&target).unwrap();
        std::fs::write(target.join("keep.txt"), "keep").unwrap();
        junction::create(&target, &existing_bin).unwrap();

        assert!(PreviewDir::create("uv", scratch.clone()).is_err());
        assert!(!scratch.exists());
        assert!(junction::exists(&existing_bin).unwrap());
        assert_eq!(
            std::fs::read_to_string(target.join("keep.txt")).unwrap(),
            "keep"
        );
    }

    #[test]
    fn preview_dir_cleanup_removes_only_owned_directories() {
        let d = crate::test_support::TestDir::new();
        let scratch = d.path().join("preview-uv");
        let bin = d.path().join("preview-uv-bin");
        let cache = d.path().join("preview-uv-cache");

        {
            let preview = PreviewDir::create("uv", scratch.clone()).unwrap();
            assert!(preview.path().join(".envcon-preview-owner").is_file());
            assert!(bin.join(".envcon-preview-owner").is_file());
            assert!(cache.join(".envcon-preview-owner").is_file());
        }

        assert!(!scratch.exists());
        assert!(!bin.exists());
        assert!(!cache.exists());
    }

    #[test]
    fn preview_dir_does_not_remove_a_replacement_directory_without_its_marker() {
        let d = crate::test_support::TestDir::new();
        let scratch = d.path().join("preview-uv");
        let moved = d.path().join("preview-uv-moved");
        {
            let preview = PreviewDir::create("uv", scratch.clone()).unwrap();
            std::fs::rename(&scratch, &moved).unwrap();
            std::fs::create_dir(&scratch).unwrap();
            std::fs::write(scratch.join("keep.txt"), "keep").unwrap();
            drop(preview);
        }

        assert_eq!(
            std::fs::read_to_string(scratch.join("keep.txt")).unwrap(),
            "keep"
        );
    }

    #[test]
    fn preview_dir_does_not_remove_a_replacement_role_without_its_marker() {
        let d = crate::test_support::TestDir::new();
        let scratch = d.path().join("preview-uv");
        let bin = d.path().join("preview-uv-bin");
        let moved = d.path().join("preview-uv-bin-moved");
        {
            let preview = PreviewDir::create("uv", scratch).unwrap();
            std::fs::rename(&bin, &moved).unwrap();
            std::fs::create_dir(&bin).unwrap();
            std::fs::write(bin.join("keep.txt"), "keep").unwrap();
            drop(preview);
        }

        assert_eq!(
            std::fs::read_to_string(bin.join("keep.txt")).unwrap(),
            "keep"
        );
    }

    #[test]
    fn preview_dir_refuses_an_existing_root_without_changing_it() {
        let d = crate::test_support::TestDir::new();
        let scratch = d.path().join("preview-uv");
        std::fs::create_dir(&scratch).unwrap();
        std::fs::write(scratch.join("keep.txt"), "keep").unwrap();

        assert!(PreviewDir::create("uv", scratch.clone()).is_err());
        assert_eq!(
            std::fs::read_to_string(scratch.join("keep.txt")).unwrap(),
            "keep"
        );
    }
    #[tokio::test]
    #[ignore = "requires ENVCON_TEST_UV/PIPX/CARGO/PYTHON; uses only temporary fixtures"]
    async fn actual_native_tools_install_and_uninstall_in_isolated_roots() {
        let d = crate::test_support::TestDir::new();
        let wheel = d.path().join("envcon_fixture-1.0.0-py3-none-any.whl");
        crate::test_support::fixture_wheel(&wheel);
        let crate_dir = d.path().join("crate");
        std::fs::create_dir_all(crate_dir.join("src")).unwrap();
        std::fs::write(
            crate_dir.join("Cargo.toml"),
            "[package]\nname=\"envcon-fixture\"\nversion=\"1.0.0\"\nedition=\"2021\"\n",
        )
        .unwrap();
        std::fs::write(
            crate_dir.join("src/main.rs"),
            "fn main(){println!(\"envcon-fixture-ok\");}\n",
        )
        .unwrap();
        let python = std::env::var("ENVCON_TEST_PYTHON").unwrap();
        for tool in ["uv", "pipx", "cargo"] {
            let exe =
                launcher(&std::env::var(format!("ENVCON_TEST_{}", tool.to_uppercase())).unwrap())
                    .unwrap();
            let old = d.path().join(format!("{tool}-old"));
            let new = d.path().join(format!("{tool}-new"));
            for root in [&old, &new] {
                initialize_destination(tool, &root.to_string_lossy()).unwrap();
                let arguments = match tool {
                    "uv" => args(&[
                        "tool",
                        "install",
                        "--offline",
                        "--python",
                        &python,
                        &wheel.to_string_lossy(),
                    ]),
                    "pipx" => args(&[
                        "install",
                        "--backend",
                        "pip",
                        "--skip-maintenance",
                        "--pip-args=--no-index",
                        &wheel.to_string_lossy(),
                    ]),
                    _ => args(&[
                        "install",
                        "--offline",
                        "--path",
                        &crate_dir.to_string_lossy(),
                        "--root",
                        &root.to_string_lossy(),
                    ]),
                };
                run(&exe, root, tool, Some(root), &arguments, 120)
                    .await
                    .unwrap_or_else(|e| panic!("{tool}: {e}"));
                let rows = installed_packages(tool, root).unwrap();
                assert!(
                    rows.iter()
                        .any(|(p, _)| p.name == "envcon-fixture" && p.version == "1.0.0"),
                    "{tool}: {:?}",
                    rows
                );
            }
            let version = manager_version(
                tool,
                &run(&exe, &old, tool, Some(&old), &args(&["--version"]), 30)
                    .await
                    .unwrap(),
            );
            let plan = Plan {
                allow_unknown_registry: false,
                tool: tool.into(),
                executable: exe.to_string_lossy().into(),
                tool_version: version,
                source: old.to_string_lossy().into(),
                destination: new.to_string_lossy().into(),
                bin_path: bin_path(tool, &new).to_string_lossy().into(),
                cache_path: new.join("cache").to_string_lossy().into(),
                packages: vec![],
            };
            std::fs::write(old.join("keep.txt"), "keep").unwrap();
            remove_old(
                &plan,
                &crate::old_packages::Candidate {
                    name: "envcon-fixture".into(),
                    version: "1.0.0".into(),
                    path: None,
                    reason: None,
                },
            )
            .await
            .unwrap_or_else(|e| panic!("{tool} uninstall: {e}"));
            assert!(installed_packages(tool, &old)
                .unwrap()
                .iter()
                .all(|(p, _)| p.name != "envcon-fixture"));
            assert!(installed_packages(tool, &new)
                .unwrap()
                .iter()
                .any(|(p, _)| p.name == "envcon-fixture"));
            assert!(old.join("keep.txt").exists());
            println!(
                "{tool}: fixture installed, old copy removed, new copy and unrelated file retained"
            );
        }
    }

    #[test]
    fn empty_targets_are_rechecked_and_links_are_rejected() {
        let d = crate::test_support::TestDir::new();
        let target = d.path().join("configured-global");
        std::fs::create_dir(&target).unwrap();
        assert!(destination(&target.to_string_lossy()).is_ok());
        std::fs::write(target.join("existing.txt"), "keep").unwrap();
        let error = initialize_destination("npm", &target.to_string_lossy())
            .unwrap_err()
            .to_string();
        assert!(error.contains("existing.txt"));
        assert_eq!(
            std::fs::read_to_string(target.join("existing.txt")).unwrap(),
            "keep"
        );
        assert!(!target.join("bin").exists());
        let empty = d.path().join("empty");
        std::fs::create_dir(&empty).unwrap();
        let link = d.path().join("redirect");
        junction::create(&empty, &link).unwrap();
        assert!(destination(&link.to_string_lossy()).is_err());
        initialize_destination("npm", &empty.to_string_lossy()).unwrap();
        assert!(crate::package_managers::config_path("npm", &empty)
            .join(".envcon-user.npmrc")
            .is_file());
        assert!(initialize_destination("npm", &empty.to_string_lossy()).is_err());
    }

    #[test]
    fn destination_layout_initializes_all_global_adapters_and_isolates_configs() {
        let d = crate::test_support::TestDir::new();
        for tool in ["npm", "pnpm", "yarn", "bun", "uv", "pipx", "cargo", "composer", "dotnet"] {
            let root = d.path().join(tool);
            initialize_destination(tool, &root.to_string_lossy()).unwrap_or_else(|e| panic!("{tool}: {e}"));
            assert!(bin_path(tool, &root).is_dir(), "{tool}");
            assert!(crate::package_managers::cache_path(tool, &root).is_dir(), "{tool}");
            let config = crate::package_managers::config_path(tool, &root);
            assert!(config.join(".envcon-user.npmrc").is_file(), "{tool}");
            assert!(config.join(".envcon-global.npmrc").is_file(), "{tool}");
            assert!(!root.join(".envcon-user.npmrc").exists(), "{tool}");
            assert!(initialize_destination(tool, &root.to_string_lossy()).is_err(), "{tool}");
        }
    }

    #[test]
    fn destination_layout_preserves_existing_config_and_rolls_back_empty_roles() {
        let d = crate::test_support::TestDir::new();
        let root = d.path().join("npm");
        let config = crate::package_managers::config_path("npm", &root);
        std::fs::create_dir(&config).unwrap();
        std::fs::write(config.join(".envcon-user.npmrc"), b"keep").unwrap();
        assert!(initialize_destination("npm", &root.to_string_lossy()).is_err());
        assert!(!root.exists());
        assert!(!crate::package_managers::cache_path("npm", &root).exists());
        assert_eq!(std::fs::read(config.join(".envcon-user.npmrc")).unwrap(), b"keep");
    }

    #[test]
    fn destination_layout_refuses_existing_sibling_without_reusing_it() {
        let d = crate::test_support::TestDir::new();
        let root = d.path().join("uv-root");
        let bin = d.path().join("uv-root-bin");
        std::fs::create_dir(&bin).unwrap();
        std::fs::write(bin.join("keep.txt"), "keep").unwrap();

        assert!(initialize_destination("uv", &root.to_string_lossy()).is_err());
        assert!(!root.exists());
        assert_eq!(
            std::fs::read_to_string(bin.join("keep.txt")).unwrap(),
            "keep"
        );
    }

    #[test]
    fn destination_layout_rolls_back_created_roles_when_cache_is_a_junction() {
        let d = crate::test_support::TestDir::new();
        let root = d.path().join("uv-root");
        let bin = d.path().join("uv-root-bin");
        let cache = d.path().join("uv-root-cache");
        let target = d.path().join("shared-cache");
        std::fs::create_dir(&target).unwrap();
        std::fs::write(target.join("keep.txt"), "keep").unwrap();
        junction::create(&target, &cache).unwrap();

        assert!(initialize_destination("uv", &root.to_string_lossy()).is_err());
        assert!(!root.exists());
        assert!(!bin.exists());
        assert!(junction::exists(&cache).unwrap());
        assert_eq!(
            std::fs::read_to_string(target.join("keep.txt")).unwrap(),
            "keep"
        );
    }

    #[test]
    fn npm_ignores_hidden_scope_update_remnants() {
        let d = crate::test_support::TestDir::new();
        for name in [
            "@openai/.codex-remnant",
            "@openai/codex",
            ".temporary",
            "broken",
        ] {
            std::fs::create_dir_all(d.path().join("node_modules").join(name)).unwrap();
        }
        std::fs::write(d.path().join("node_modules/@openai/codex/package.json"), r#"{"name":"@openai/codex","version":"0.144.4","_resolved":"https://registry.npmjs.org/@openai/codex/-/codex-0.144.4.tgz"}"#).unwrap();
        let rows = installed_packages("npm", d.path()).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].0.name, "@openai/codex");
        assert!(rows[0].0.reason.is_none());
        assert!(rows[0]
            .1
            .as_ref()
            .unwrap()
            .ends_with("node_modules/@openai/codex"));
        assert_eq!(rows[1].0.name, "broken");
        assert!(rows[1].0.reason.is_some());
    }
    #[test]
    fn verifies_versions_even_when_canceled_and_retains_failures() {
        let mut task = Snapshot {
            tool: "npm".into(),
            status: "running".into(),
            message: String::new(),
            destination: String::new(),
            bin_path: String::new(),
            report: String::new(),
            items: ["installed", "installed", "pending", "failed"]
                .iter()
                .enumerate()
                .map(|(i, s)| ItemResult {
                    name: format!("pkg{i}"),
                    version: "1.0.0".into(),
                    status: (*s).into(),
                    detail: None,
                })
                .collect(),
        };
        let verified = Ok(vec![
            Package {
                name: "pkg0".into(),
                version: "1.0.0".into(),
                reason: None,
            },
            Package {
                name: "pkg1".into(),
                version: "2.0.0".into(),
                reason: None,
            },
        ]);
        finish(&mut task, &verified, true);
        assert_eq!(task.status, "canceled");
        assert_eq!(
            task.items
                .iter()
                .map(|p| p.status.as_str())
                .collect::<Vec<_>>(),
            ["installed", "failed", "canceled", "failed"]
        );
        finish(&mut task, &verified, false);
        assert_eq!(task.status, "partial");
        task.items.truncate(1);
        finish(&mut task, &verified, false);
        assert_eq!(task.status, "done");
    }
    #[test]
    fn validates_scoped_names_versions_and_local_sources() {
        assert!(valid_name("@scope/tool"));
        assert!(!valid_name("--registry"));
        assert!(!valid_name("a/../../b"));
        assert!(valid_version("1.2.3-beta.1+build"));
        assert!(!valid_version("1.2.3 & whoami"));
        assert!(install_args("unknown", Path::new("D:/target"), "tool@1.0.0").is_err());
        assert!(install_args("npm", Path::new("D:/target"), "tool@1.0.0")
            .unwrap()
            .contains(&"--registry=https://registry.npmjs.org/".to_owned()));
    }
    #[test]
    fn only_reads_direct_pnpm_dependencies_and_handles_scopes() {
        let d = crate::test_support::TestDir::new();
        std::fs::create_dir_all(d.path().join("node_modules/@scope/tool")).unwrap();
        std::fs::write(
            d.path().join("package.json"),
            r#"{"dependencies":{"@scope/tool":"^1.0.0","missing":"1.0.0"}}"#,
        )
        .unwrap();
        std::fs::write(
            d.path().join("node_modules/@scope/tool/package.json"),
            r#"{"name":"@scope/tool","version":"1.2.0"}"#,
        )
        .unwrap();
        let rows = inventory("pnpm", d.path()).unwrap();
        assert_eq!(rows.len(), 2);
        assert!(rows[0].reason.is_none());
        assert!(rows[1].reason.is_some());
        assert!(destination(&d.path().to_string_lossy()).is_err());
        for tool in ["npm", "pnpm", "yarn"] {
            assert_eq!(
                install_args(tool, d.path(), "@scope/tool@1.2.0")
                    .unwrap()
                    .last()
                    .unwrap(),
                "@scope/tool@1.2.0"
            );
        }
    }

    #[test]
    fn npm_hidden_lock_and_unknown_origins() {
        let d = crate::test_support::TestDir::new();
        for name in ["registry", "url", "unknown"] {
            let p = d.path().join("node_modules").join(name);
            std::fs::create_dir_all(&p).unwrap();
            std::fs::write(
                p.join("package.json"),
                format!(r#"{{"name":"{name}","version":"1.0.0"}}"#),
            )
            .unwrap();
        }
        std::fs::write(d.path().join("node_modules/.package-lock.json"), r#"{"packages":{"node_modules/registry":{"resolved":"https://registry.npmjs.org/registry/-/registry-1.0.0.tgz"},"node_modules/url":{"resolved":"https://example.com/custom.tgz"}}}"#).unwrap();
        let p = inventory("npm", d.path()).unwrap();
        assert!(p
            .iter()
            .find(|p| p.name == "registry")
            .unwrap()
            .reason
            .is_none());
        assert!(p
            .iter()
            .filter(|p| p.name != "registry")
            .all(|p| p.reason.is_some()));
        let mut plan = Plan {
            tool: "npm".into(),
            executable: String::new(),
            tool_version: String::new(),
            source: String::new(),
            destination: String::new(),
            bin_path: String::new(),
            cache_path: String::new(),
            packages: p,
            allow_unknown_registry: false,
        };
        allow_registry_recovery(&mut plan, true);
        assert!(plan
            .packages
            .iter()
            .find(|p| p.name == "unknown")
            .unwrap()
            .reason
            .is_none());
        assert!(plan
            .packages
            .iter()
            .find(|p| p.name == "url")
            .unwrap()
            .reason
            .is_some());
        std::fs::rename(
            d.path().join("node_modules/.package-lock.json"),
            d.path().join("package-lock.json"),
        )
        .unwrap();
        assert!(inventory("npm", d.path())
            .unwrap()
            .iter()
            .find(|p| p.name == "registry")
            .unwrap()
            .reason
            .is_none());
    }

    #[test]
    fn pnpm_v11_active_groups_ignore_orphans() {
        let d = crate::test_support::TestDir::new();
        let root = d.path().join("v11");
        let group = root.join("install-1");
        std::fs::create_dir_all(group.join("node_modules/tool")).unwrap();
        std::fs::create_dir_all(root.join("orphan")).unwrap();
        std::fs::write(
            group.join("package.json"),
            r#"{"dependencies":{"tool":"1.2.3"}}"#,
        )
        .unwrap();
        std::fs::write(
            group.join("node_modules/tool/package.json"),
            r#"{"name":"tool","version":"1.2.3"}"#,
        )
        .unwrap();
        junction::create(&group, root.join("active-hash")).unwrap();
        let rows = inventory("pnpm", d.path()).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name, "tool");
        assert!(rows[0].reason.is_none());
    }

    #[tokio::test]
    async fn cleanup_blocks_changed_versions_and_missing_targets() {
        let d = crate::test_support::TestDir::new();
        let old = d.path().join("old");
        let new = d.path().join("new");
        for root in [&old, &new] {
            std::fs::create_dir_all(root.join("node_modules/fixture")).unwrap();
            std::fs::write(root.join("node_modules/fixture/package.json"), r#"{"name":"fixture","version":"1.0.0","_resolved":"https://registry.npmjs.org/fixture/-/fixture-1.0.0.tgz"}"#).unwrap();
        }
        let exe = d.path().join("npm.cmd");
        std::fs::write(
            &exe,
            format!(
                "@echo off\r\nif \"%1\"==\"--version\" (echo 1.0.0 & exit /b 0)\r\necho {}\r\n",
                old.join("node_modules").display()
            ),
        )
        .unwrap();
        let mut plan = Plan {
            allow_unknown_registry: false,
            tool: "npm".into(),
            executable: exe.to_string_lossy().into(),
            tool_version: "1.0.0".into(),
            source: old.to_string_lossy().into(),
            destination: new.to_string_lossy().into(),
            bin_path: new.to_string_lossy().into(),
            cache_path: new.join("cache").to_string_lossy().into(),
            packages: inventory("npm", &old).unwrap(),
        };
        let items = vec![ItemResult {
            name: "fixture".into(),
            version: "1.0.0".into(),
            status: "installed".into(),
            detail: None,
        }];
        assert!(cleanup_candidates(&plan, &items).await.unwrap()[0]
            .reason
            .is_none());
        for root in [&old, &new] {
            std::fs::write(
                root.join("node_modules/fixture/package.json"),
                r#"{"name":"fixture","version":"1.0.0"}"#,
            )
            .unwrap();
        }
        assert!(cleanup_candidates(&plan, &items).await.unwrap()[0]
            .reason
            .is_some());
        allow_registry_recovery(&mut plan, true);
        assert!(cleanup_candidates(&plan, &items).await.unwrap()[0]
            .reason
            .is_none());
        let direct = scanned_cleanup_plan(
            "npm".into(),
            plan.executable.clone(),
            plan.source.clone(),
            plan.destination.clone(),
        )
        .await
        .unwrap();
        assert!(scanned_cleanup_candidates(&direct).await.unwrap()[0]
            .reason
            .is_none());
        std::fs::write(
            old.join("node_modules/fixture/package.json"),
            r#"{"name":"fixture","version":"2.0.0"}"#,
        )
        .unwrap();
        assert!(cleanup_candidates(&plan, &items).await.unwrap()[0]
            .reason
            .as_ref()
            .unwrap()
            .contains("旧包版本"));
        std::fs::remove_file(new.join("node_modules/fixture/package.json")).unwrap();
        assert!(cleanup_candidates(&plan, &items).await.unwrap()[0]
            .reason
            .as_ref()
            .unwrap()
            .contains("新目录"));
        assert!(crate::old_packages::verify_separate(&old, &old).is_err());
        assert!(crate::old_packages::verify_separate(&old, &old.join("node_modules")).is_err());
    }

    #[tokio::test]
    #[ignore = "requires ENVCON_TEST_NPM/PNPM/YARN; only installs a local fixture into temporary directories"]
    async fn actual_managers_install_offline_into_isolated_directories() {
        let d = crate::test_support::TestDir::new();
        let parent_manifest = r#"{"name":"parent-project","private":true}"#;
        std::fs::write(d.path().join("package.json"), parent_manifest).unwrap();
        let archive = d.path().join("fixture.tgz");
        let retained = d.path().join("retained.tgz");
        for (archive, package_name) in [
            (&archive, "envcon-offline-fixture"),
            (&retained, "envcon-retained-fixture"),
        ] {
            let file = std::fs::File::create(archive).unwrap();
            let zip = flate2::write::GzEncoder::new(file, flate2::Compression::default());
            let mut tar = tar::Builder::new(zip);
            let manifest = serde_json::json!({"name":package_name,"version":"1.0.0","bin":{package_name:"cli.js"}}).to_string();
            for (name, content) in [
                ("package/package.json", manifest.as_str()),
                (
                    "package/cli.js",
                    "#!/usr/bin/env node\nconsole.log('envcon-fixture-ok');\n",
                ),
            ] {
                let mut header = tar::Header::new_gnu();
                header.set_size(content.len() as u64);
                header.set_mode(0o755);
                header.set_cksum();
                tar.append_data(&mut header, name, content.as_bytes())
                    .unwrap();
            }
            tar.into_inner().unwrap().finish().unwrap();
        }
        for tool in ["npm", "pnpm", "yarn", "bun"] {
            let exe = launcher(
                &std::env::var(format!("ENVCON_TEST_{}", tool.to_uppercase()))
                    .expect("set launcher paths"),
            )
            .unwrap();
            let dest = d.path().join(tool);
            std::fs::create_dir_all(dest.join("bin")).unwrap();
            std::fs::write(dest.join(".envcon-user.npmrc"), "").unwrap();
            std::fs::write(dest.join(".envcon-global.npmrc"), "").unwrap();
            crate::package_managers::commands::prepare(tool, &dest).unwrap();
            let version = run(&exe, &dest, tool, Some(&dest), &args(&["--version"]), 30)
                .await
                .unwrap();
            assert!(valid_version(&version), "{tool}: {version}");
            let mut install = install_args(tool, &dest, &archive.to_string_lossy()).unwrap();
            if tool != "bun" {
                install.insert(1, "--offline".into());
            }
            run(&exe, &dest, tool, Some(&dest), &install, 90)
                .await
                .unwrap_or_else(|e| panic!("{tool}: {e}"));
            let mut install_retained =
                install_args(tool, &dest, &retained.to_string_lossy()).unwrap();
            if tool != "bun" {
                install_retained.insert(1, "--offline".into());
            }
            run(&exe, &dest, tool, Some(&dest), &install_retained, 90)
                .await
                .unwrap();
            let root = dest.clone();
            let rows = inventory(tool, &root).unwrap_or_else(|e| panic!("{tool}: {e}"));
            assert!(
                rows.iter()
                    .any(|p| p.name == "envcon-offline-fixture" && p.version == "1.0.0"),
                "{tool}"
            );
            let bin = if tool == "npm" {
                dest.clone()
            } else {
                dest.join("bin")
            };
            let output = run(
                &bin.join(if tool == "bun" {
                    "envcon-offline-fixture.exe"
                } else {
                    "envcon-offline-fixture.cmd"
                }),
                &dest,
                tool,
                Some(&dest),
                &[],
                30,
            )
            .await
            .unwrap();
            assert_eq!(output, "envcon-fixture-ok", "{tool}");
            let next = d.path().join(format!("{tool}-restored"));
            std::fs::create_dir(&next).unwrap();
            let plan = preview(
                tool.into(),
                exe.to_string_lossy().into(),
                root.to_string_lossy().into(),
                next.to_string_lossy().into(),
            )
            .await
            .unwrap();
            assert!(crate::detect::system::paths_eq(
                Path::new(&plan.destination),
                &next
            ));
            assert!(crate::detect::system::paths_eq(
                Path::new(&plan.bin_path),
                &bin_path(tool, &next)
            ));
            let mut changed = plan.clone();
            changed.tool_version = "0.0.0".into();
            assert!(start(changed, vec!["envcon-offline-fixture".into()])
                .await
                .is_err());
            assert!(start(plan.clone(), vec!["--unknown-package".into()])
                .await
                .is_err());
            assert_eq!(std::fs::read_dir(&next).unwrap().count(), 0);
            initialize_destination(tool, &next.to_string_lossy()).unwrap();
            crate::package_managers::commands::prepare(tool, &next).unwrap();
            let mut install_new = install_args(tool, &next, &archive.to_string_lossy()).unwrap();
            if tool != "bun" {
                install_new.insert(1, "--offline".into());
            }
            run(&exe, &next, tool, Some(&next), &install_new, 90)
                .await
                .unwrap();
            std::fs::write(root.join("keep-runtime.txt"), "retained").unwrap();
            let candidate = crate::old_packages::Candidate {
                name: "envcon-offline-fixture".into(),
                version: "1.0.0".into(),
                path: None,
                reason: None,
            };
            remove_old(&plan, &candidate)
                .await
                .unwrap_or_else(|e| panic!("{tool} cleanup: {e}"));
            assert!(root.join("keep-runtime.txt").is_file());
            assert!(inventory(tool, &root)
                .unwrap()
                .iter()
                .any(|p| p.name == "envcon-retained-fixture"));
            let new_root = next.clone();
            assert!(inventory(tool, &new_root)
                .unwrap()
                .iter()
                .any(|p| p.name == "envcon-offline-fixture"));
            assert!(
                !next.join("global").exists(),
                "must not add another global layer: {tool}"
            );
            println!("{tool}: old fixture removed, new install and unrelated files retained");
            assert_eq!(
                std::fs::read_to_string(d.path().join("package.json")).unwrap(),
                parent_manifest
            );
            assert!(!d.path().join("bun.lock").exists());
        }
    }
}
