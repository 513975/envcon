use crate::{
    error::{AppError, Result},
    package_managers::Package,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

pub fn global_env(tool: &str) -> Option<&'static str> {
    match tool {
        "bun" => Some("BUN_INSTALL_GLOBAL_DIR"),
        "uv" => Some("UV_TOOL_DIR"),
        "pipx" => Some("PIPX_HOME"),
        "cargo" => Some("CARGO_INSTALL_ROOT"),
        "composer" => Some("COMPOSER_HOME"),
        _ => None,
    }
}
pub fn cache_env(tool: &str) -> Option<&'static str> {
    match tool {
        "bun" => Some("BUN_INSTALL_CACHE_DIR"),
        "uv" => Some("UV_CACHE_DIR"),
        "pipx" => Some("PIP_CACHE_DIR"),
        "composer" => Some("COMPOSER_CACHE_DIR"),
        "dotnet" => Some("NUGET_PACKAGES"),
        _ => None,
    }
}
pub fn bin_env(tool: &str) -> Option<&'static str> {
    match tool {
        "bun" => Some("BUN_INSTALL_BIN"),
        "uv" => Some("UV_TOOL_BIN_DIR"),
        "pipx" => Some("PIPX_BIN_DIR"),
        "pnpm" => Some("PNPM_HOME"),
        _ => None,
    }
}
pub fn bin_path(tool: &str, root: &Path) -> PathBuf {
    match tool {
        "npm" | "dotnet" => root.to_path_buf(),
        "composer" => root.join("vendor/bin"),
        "yarn" => prefix_path(tool, root).join("bin"),
        "pnpm" => sibling_role(root, "bin"),
        // uv/pipx/Bun enumerate their tool roots; keep launchers as a sibling
        // directory so a launcher folder is never mistaken for an installed tool.
        "uv" => sibling_role(root, "bin"),
        "pipx" => sibling_role(root, "bin"),
        "bun" => sibling_role(root, "bin"),
        _ => root.join("bin"),
    }
}

pub fn prefix_path(tool: &str, root: &Path) -> PathBuf {
    if tool == "yarn" {
        sibling_role(root, "prefix")
    } else {
        root.to_path_buf()
    }
}

pub fn cache_path(tool: &str, root: &Path) -> PathBuf {
    match tool {
        "npm" | "cargo" | "composer" | "uv" | "pipx" | "bun" | "dotnet" => {
            sibling_role(root, "cache")
        }
        "pnpm" => sibling_role(root, "store"),
        "yarn" => sibling_role(root, "cache"),
        _ => root.join("cache"),
    }
}

pub fn config_path(_tool: &str, root: &Path) -> PathBuf {
    sibling_role(root, "config")
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DirectoryRoles {
    pub tool_environment: PathBuf,
    pub command_entry: PathBuf,
    pub cache_storage: PathBuf,
    pub configuration: PathBuf,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandEntryPolicy {
    /// The manager uses its installation root as the command directory
    /// (for example the Windows npm prefix and the .NET tool path).
    ToolRoot,
    /// The manager hard-codes a subdirectory below its installation root.
    FixedSubdirectory(&'static str),
    /// The manager enumerates its installation root, so launchers must live
    /// in a sibling directory instead of a child directory.
    Sibling(&'static str),
}

pub fn command_entry_policy(tool: &str) -> Option<CommandEntryPolicy> {
    match tool {
        "npm" | "dotnet" => Some(CommandEntryPolicy::ToolRoot),
        "cargo" => Some(CommandEntryPolicy::FixedSubdirectory("bin")),
        "composer" => Some(CommandEntryPolicy::FixedSubdirectory("vendor/bin")),
        "yarn" => Some(CommandEntryPolicy::FixedSubdirectory("prefix/bin")),
        "uv" | "pipx" | "bun" | "pnpm" => Some(CommandEntryPolicy::Sibling("bin")),
        _ => None,
    }
}

pub fn directory_roles(
    tool: &str,
    tool_root: &Path,
    cache_override: Option<&Path>,
) -> DirectoryRoles {
    DirectoryRoles {
        tool_environment: tool_root.to_path_buf(),
        command_entry: bin_path(tool, tool_root),
        cache_storage: cache_override
            .map(Path::to_path_buf)
            .unwrap_or_else(|| cache_path(tool, tool_root)),
        configuration: config_path(tool, tool_root),
    }
}

pub fn validate_directory_roles(tool: &str, roles: &DirectoryRoles) -> Result<()> {
    validate_layout(
        tool,
        Some(&roles.tool_environment),
        Some(&roles.command_entry),
        Some(&roles.cache_storage),
        Some(&roles.configuration),
    )
}

fn sibling_role(root: &Path, role: &str) -> PathBuf {
    let Some(name) = root.file_name().and_then(|value| value.to_str()) else {
        return root.join(role);
    };
    root.parent()
        .unwrap_or_else(|| Path::new("."))
        .join(format!("{name}-{role}"))
}

fn overlaps(a: &Path, b: &Path) -> bool {
    crate::detect::system::paths_eq(a, b)
        || crate::detect::system::path_within(a, b)
        || crate::detect::system::path_within(b, a)
}

pub fn validate_layout(
    tool: &str,
    tool_root: Option<&Path>,
    command_root: Option<&Path>,
    cache_root: Option<&Path>,
    config_root: Option<&Path>,
) -> Result<()> {
    let roles = [
        ("工具环境目录", tool_root),
        ("命令入口目录", command_root),
        ("缓存/存储目录", cache_root),
        ("配置目录", config_root),
    ];
    for (name, path) in roles {
        if let Some(path) = path {
            if !path.is_absolute()
                || path
                    .components()
                    .any(|component| matches!(component, std::path::Component::ParentDir))
            {
                return Err(AppError::msg(format!("{name}必须是不含 .. 的绝对路径")));
            }
        }
    }
    if let (Some(tool_root), Some(command_root)) = (tool_root, command_root) {
        let expected = bin_path(tool, tool_root);
        if !crate::detect::system::paths_eq(&expected, command_root) {
            return Err(AppError::msg(format!(
                "命令入口与 {tool} 的固定布局不一致；期望 {}",
                expected.display()
            )));
        }
        if matches!(command_entry_policy(tool), Some(CommandEntryPolicy::Sibling(_)))
            && tool_root.file_name().is_none()
        {
            return Err(AppError::msg(
                "兄弟入口目录要求工具环境目录具有明确的目录名",
            ));
        }
    }
    if let (Some(command), Some(cache)) = (command_root, cache_root) {
        if overlaps(command, cache) {
            return Err(AppError::msg("命令入口目录与缓存/存储目录不能相同或嵌套"));
        }
    }
    if let Some(config) = config_root {
        for (role_name, role) in [
            ("工具环境目录", tool_root),
            ("命令入口目录", command_root),
            ("缓存/存储目录", cache_root),
        ] {
            if let Some(role) = role {
                if overlaps(config, role) {
                    return Err(AppError::msg(format!(
                        "配置目录不能与{role_name}相同或嵌套"
                    )));
                }
            }
        }
    }
    let Some(tool_root) = tool_root else {
        return Ok(());
    };
    let policy = command_entry_policy(tool);
    // A fixed child directory is the manager's real layout (Cargo, Composer,
    // Yarn). npm/.NET place launchers at the root. Everything else must keep
    // command and cache directories outside the enumerated tool root.
    let command_may_overlap_root = matches!(
        policy,
        Some(CommandEntryPolicy::ToolRoot)
            | Some(CommandEntryPolicy::FixedSubdirectory(_))
    );
    if !command_may_overlap_root {
        if let Some(command) = command_root {
            if overlaps(command, tool_root) {
                return Err(AppError::msg(
                    "命令入口目录不能与工具环境目录相同或嵌套",
                ));
            }
        }
    }
    if let Some(cache) = cache_root {
        if overlaps(cache, tool_root) {
            return Err(AppError::msg("缓存/存储目录不能与工具环境目录相同或嵌套"));
        }
    }
    if let Some(config) = config_root {
        if overlaps(config, tool_root) {
            return Err(AppError::msg("配置目录不能与工具环境目录相同或嵌套"));
        }
    }
    if tool == "yarn" {
        let prefix = prefix_path(tool, tool_root);
        for (role_name, role) in [
            ("工具环境目录", Some(tool_root)),
            ("缓存/存储目录", cache_root),
        ] {
            if let Some(role) = role {
                if overlaps(&prefix, role) {
                    return Err(AppError::msg(format!(
                        "Yarn prefix 目录不能与{role_name}相同或嵌套"
                    )));
                }
            }
        }
        if command_root.is_some_and(|command| {
            !crate::detect::system::path_within(&prefix, command)
        }) {
            return Err(AppError::msg("Yarn 命令入口必须位于 prefix/bin 中"));
        }
    }
    Ok(())
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UvEntrypointSnapshot {
    pub path: String,
    pub sha256: Option<String>,
    pub legacy: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UvToolLayout {
    pub name: String,
    pub version: Option<String>,
    pub directory: String,
    pub receipt_status: String,
    pub receipt_sha256: Option<String>,
    /// Exact pinned requirement reconstructed from the receipt, when safe.
    pub requirement: Option<String>,
    pub entrypoints: Vec<String>,
    pub entrypoint_snapshots: Vec<UvEntrypointSnapshot>,
    pub needs_recovery: bool,
    pub recoverable: bool,
    pub issues: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UvLegacyLayout {
    pub tool_root: String,
    pub legacy_bin: String,
    pub legacy_cache: String,
    pub nested_bin_exists: bool,
    pub nested_cache_exists: bool,
    pub proposed_bin: String,
    pub proposed_cache: String,
    /// `clean`, `legacy`, `legacy-partial` or `legacy-empty`.
    pub status: String,
    pub tools: Vec<UvToolLayout>,
    pub issues: Vec<String>,
}

fn path_present(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok()
}

fn normalized_package(name: &str) -> String {
    name.to_lowercase().replace(['_', '.'], "-")
}

fn file_sha256(path: &Path) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    Some(format!("{:x}", Sha256::digest(bytes)))
}

/// Read-only inspection of the pre-sibling uv layout. It never moves or
/// deletes anything; callers must show the findings before any recovery.
pub fn inspect_uv_layout(root: &Path) -> Result<UvLegacyLayout> {
    if !root.is_absolute()
        || root
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err(AppError::msg("uv 工具目录必须是不含 .. 的绝对路径"));
    }
    let metadata = std::fs::symlink_metadata(root)?;
    if !metadata.is_dir() || crate::detect::system::is_reparse_path(root) {
        return Err(AppError::msg("uv 工具目录必须是普通目录"));
    }
    let legacy_bin = root.join("bin");
    let legacy_cache = root.join("cache");
    let nested_bin_exists = path_present(&legacy_bin);
    let nested_cache_exists = path_present(&legacy_cache);
    let proposed_bin = bin_path("uv", root);
    let proposed_cache = cache_path("uv", root);
    let mut issues = Vec::new();
    if !proposed_bin.file_name().is_some_and(|_| root.file_name().is_some()) {
        issues.push("uv 工具目录没有可用于兄弟入口的目录名".into());
    }
    if path_present(&proposed_bin) {
        issues.push(format!(
            "建议的入口目录已存在，恢复前必须核对归属: {}",
            proposed_bin.display()
        ));
    }
    if path_present(&proposed_cache) {
        issues.push(format!(
            "建议的缓存目录已存在，恢复前必须核对归属: {}",
            proposed_cache.display()
        ));
    }
    if nested_bin_exists && crate::detect::system::is_reparse_path(&legacy_bin) {
        issues.push("旧入口目录是链接/重解析点，不能据此确认工具归属".into());
    }
    if nested_cache_exists && crate::detect::system::is_reparse_path(&legacy_cache) {
        issues.push("旧缓存目录是链接/重解析点，不能据此确认工具归属".into());
    }
    let mut tools = Vec::new();
    if nested_bin_exists || nested_cache_exists {
        let mut entries = std::fs::read_dir(root)?
            .collect::<std::io::Result<Vec<_>>>()?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let directory_name = entry.file_name().to_string_lossy().into_owned();
            let lower = directory_name.to_ascii_lowercase();
            if directory_name.starts_with('.') || matches!(lower.as_str(), "bin" | "cache") {
                continue;
            }
            let path = entry.path();
            let is_directory = std::fs::symlink_metadata(&path)
                .is_ok_and(|meta| meta.is_dir() && !crate::detect::system::is_reparse_path(&path));
            if !is_directory {
                continue;
            }
            tools.push(inspect_uv_tool(&path, &directory_name, &legacy_bin, &proposed_bin));
        }
    }
    let needs_recovery = tools.iter().any(|tool| tool.needs_recovery);
    let recoverable = tools
        .iter()
        .filter(|tool| tool.needs_recovery)
        .all(|tool| tool.recoverable);
    let has_tool_issues = tools
        .iter()
        .any(|tool| !tool.issues.is_empty() || (tool.needs_recovery && !tool.recoverable));
    let status = if !nested_bin_exists && !nested_cache_exists {
        "clean"
    } else if !needs_recovery {
        if has_tool_issues || !issues.is_empty() {
            "legacy-partial"
        } else {
            "legacy-empty"
        }
    } else if recoverable && issues.is_empty() && !has_tool_issues {
        "legacy"
    } else {
        "legacy-partial"
    };
    Ok(UvLegacyLayout {
        tool_root: root.to_string_lossy().into(),
        legacy_bin: legacy_bin.to_string_lossy().into(),
        legacy_cache: legacy_cache.to_string_lossy().into(),
        nested_bin_exists,
        nested_cache_exists,
        proposed_bin: proposed_bin.to_string_lossy().into(),
        proposed_cache: proposed_cache.to_string_lossy().into(),
        status: status.into(),
        tools,
        issues,
    })
}

pub(crate) fn inspect_uv_tool(
    directory: &Path,
    fallback_name: &str,
    legacy_bin: &Path,
    proposed_bin: &Path,
) -> UvToolLayout {
    let receipt_path = directory.join("uv-receipt.toml");
    let mut issues = Vec::new();
    let receipt_sha256 = file_sha256(&receipt_path);
    if path_present(legacy_bin) && crate::detect::system::is_reparse_path(legacy_bin) {
        issues.push("旧入口目录是链接/重解析点，不能据此确认工具归属".into());
    }
    let mut name = fallback_name.to_owned();
    let mut entrypoints = Vec::new();
    let mut entrypoint_snapshots = Vec::new();
    let mut recovery_requirement = None;
    let receipt_status = match std::fs::read_to_string(&receipt_path) {
        Ok(text) => match toml::from_str::<toml::Value>(&text) {
            Ok(receipt) => {
                let requirements = receipt
                    .get("tool")
                    .and_then(|tool| tool.get("requirements"))
                    .and_then(toml::Value::as_array);
                match requirements.map(Vec::as_slice) {
                    Some([requirement]) => {
                        if let Some(value) = requirement
                            .get("name")
                            .and_then(toml::Value::as_str)
                            .filter(|value| !value.is_empty())
                        {
                            name = value.to_owned();
                        } else {
                            issues.push("安装记录缺少包名".into());
                        }
                        let specifier = requirement
                            .get("specifier")
                            .and_then(toml::Value::as_str)
                            .unwrap_or("");
                        if requirement.as_table().is_some_and(|table| {
                            table.keys().any(|key| {
                                !matches!(key.as_str(), "name" | "specifier")
                            })
                        }) {
                            issues.push("uv 安装要求包含无法自动还原的选项或来源".into());
                        } else if !specifier.starts_with("==") {
                            issues.push("uv 安装要求不是精确版本，需人工恢复".into());
                        } else {
                            recovery_requirement = Some(format!("{name}{specifier}"));
                        }
                    }
                    Some([]) => issues.push("安装记录没有工具要求".into()),
                    Some(_) => {
                        issues.push("uv 工具包含多个安装要求或附加依赖，需人工恢复".into());
                    }
                    None => issues.push("安装记录缺少工具要求".into()),
                }
                if receipt
                    .get("tool")
                    .and_then(|tool| tool.get("options"))
                    .and_then(toml::Value::as_table)
                    .is_some_and(|options| !options.is_empty())
                {
                    issues.push("uv 安装记录包含额外安装选项，需人工恢复".into());
                }
                if let Some(entries) = receipt
                    .get("tool")
                    .and_then(|tool| tool.get("entrypoints"))
                    .and_then(toml::Value::as_array)
                {
                    for entry in entries {
                        match entry.get("install-path").and_then(toml::Value::as_str) {
                            Some(value) if Path::new(value).is_absolute() => {
                                let path = Path::new(value);
                                let legacy = crate::detect::system::path_within(legacy_bin, path);
                                entrypoint_snapshots.push(UvEntrypointSnapshot {
                                    path: value.to_owned(),
                                    sha256: file_sha256(path),
                                    legacy,
                                });
                                entrypoints.push(value.to_owned());
                            }
                            _ => issues.push("安装记录中的启动器路径无效".into()),
                        }
                    }
                }
                if entrypoints.is_empty() {
                    issues.push("安装记录缺少启动器路径".into());
                }
                "valid"
            }
            Err(_) => {
                issues.push("安装记录损坏".into());
                "invalid"
            }
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            issues.push("缺少 uv-receipt.toml".into());
            "missing"
        }
        Err(_) => {
            issues.push("安装记录无法读取".into());
            "unreadable"
        }
    };
    let wanted = normalized_package(&name);
    let found = crate::pip_reinstall::disk_packages(&directory.join("Lib/site-packages"))
        .ok()
        .and_then(|packages| {
            packages
                .into_iter()
                .find(|package| normalized_package(&package.name) == wanted)
        });
    if found.as_ref().is_some_and(|package| package.reason.is_some()) {
        issues.push("旧工具为直接来源或包含不可还原的安装要求".into());
    }
    let version = found.map(|package| package.version);
    if version.is_none() {
        issues.push("环境内未找到对应包的版本记录".into());
    }
    if let (Some(pin), Some(installed)) = (recovery_requirement.as_deref(), version.as_deref()) {
        if pin.rsplit_once("==").map(|(_, value)| value) != Some(installed) {
            issues.push("安装要求版本与环境内版本不一致，需人工恢复".into());
        }
    }
    let mut needs_recovery = false;
    for entrypoint in &entrypoints {
        let path = Path::new(entrypoint);
        if crate::detect::system::path_within(legacy_bin, path) {
            needs_recovery = true;
        } else if !crate::detect::system::path_within(proposed_bin, path) {
            issues.push(format!("启动器位于 uv 入口目录之外: {entrypoint}"));
        }
    }
    let recoverable = needs_recovery
        && receipt_status == "valid"
        && version.is_some()
        && !entrypoints.is_empty()
        && recovery_requirement.is_some()
        && issues.is_empty();
    UvToolLayout {
        name,
        version,
        directory: directory.to_string_lossy().into(),
        receipt_status: receipt_status.into(),
        receipt_sha256,
        requirement: recovery_requirement,
        entrypoints,
        entrypoint_snapshots,
        needs_recovery,
        recoverable,
        issues,
    }
}

/// Returns a reason when a directory must not be migrated as-is because it
/// still uses a layout that the manager itself interprets incorrectly.
pub fn legacy_layout_issue(tool: &str, root: &Path) -> Option<String> {
    if tool != "uv" {
        return None;
    }
    let nested: Vec<_> = ["bin", "cache"]
        .into_iter()
        .map(|name| root.join(name))
        .filter(|path| path_present(path))
        .collect();
    (!nested.is_empty()).then(|| {
        format!(
            "uv 旧布局包含 {}；请先按独立入口/缓存目录恢复工具，再迁移或清理",
            nested
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join("、")
        )
    })
}

pub fn configured_env_path(key: &str) -> Option<PathBuf> {
    crate::pathman::get_user_env_vars()
        .ok()
        .and_then(|vars| vars.get(key).cloned())
        .or_else(|| std::env::var(key).ok())
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
}

pub fn default_source(tool: &str) -> Option<PathBuf> {
    let user = crate::pathman::get_user_env_vars().unwrap_or_default();
    if let Some(key) = global_env(tool) {
        if let Some(value) = user.get(key).cloned().or_else(|| std::env::var(key).ok()) {
            if Path::new(&value).is_absolute() {
                return Some(value.into());
            }
        }
    }
    let home = dirs::home_dir()?;
    match tool {
        "bun" => {
            let config = configured_env_path("XDG_CONFIG_HOME")
                .map(|p| p.join(".bunfig.toml"))
                .filter(|p| p.is_file())
                .unwrap_or_else(|| home.join(".bunfig.toml"));
            let configured = std::fs::read_to_string(config)
                .ok()
                .and_then(|text| toml::from_str::<toml::Value>(&text).ok())
                .and_then(|value| {
                    value
                        .get("install")?
                        .get("globalDir")?
                        .as_str()
                        .map(PathBuf::from)
                })
                .filter(|path| path.is_absolute());
            configured.or_else(|| {
                Some(
                    configured_env_path("BUN_INSTALL")
                        .unwrap_or_else(|| home.join(".bun"))
                        .join("install/global"),
                )
            })
        }
        "uv" => dirs::data_dir().map(|p| p.join("uv/tools")),
        "pipx" => dirs::data_local_dir().map(|p| p.join("pipx/pipx")),
        "cargo" => Some(
            user.get("CARGO_HOME")
                .map(PathBuf::from)
                .or_else(|| std::env::var_os("CARGO_HOME").map(PathBuf::from))
                .unwrap_or_else(|| home.join(".cargo")),
        ),
        "composer" => dirs::data_dir().map(|p| p.join("Composer")),
        "dotnet" => Some(home.join(".dotnet/tools")),
        _ => None,
    }
}

fn json(path: &Path) -> Result<Value> {
    Ok(serde_json::from_slice(&std::fs::read(path)?)?)
}
fn valid_pin(name: &str, version: &str) -> bool {
    !name.is_empty()
        && name.as_bytes()[0].is_ascii_alphanumeric()
        && name
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"-_. /".contains(&c))
        && !name.contains(' ')
        && !name.contains("..")
        && !version.is_empty()
        && version.as_bytes()[0].is_ascii_digit()
        && version
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b".-+!_".contains(&c))
}
fn package(name: String, version: String, reason: Option<&str>) -> Package {
    let reason = if !valid_pin(&name, &version) {
        Some("包名或版本无法生成固定安装要求")
    } else {
        reason
    };
    Package {
        name,
        version,
        reason: reason.map(str::to_owned),
    }
}

pub fn inventory(tool: &str, root: &Path) -> Result<Vec<Package>> {
    let mut result = Vec::new();
    match tool {
        "cargo" => {
            let metadata = root.join(".crates.toml");
            let text = std::fs::read_to_string(&metadata)?;
            let value: toml::Value = toml::from_str(&text)
                .map_err(|e| AppError::msg(format!("Cargo 安装记录损坏: {e}")))?;
            let entries = value
                .get("v1")
                .and_then(toml::Value::as_table)
                .ok_or_else(|| AppError::msg("Cargo 缺少 v1 安装记录"))?;
            for key in entries.keys() {
                let parts: Vec<_> = key.splitn(3, ' ').collect();
                if parts.len() != 3 {
                    return Err(AppError::msg("Cargo 安装记录键格式无效"));
                }
                let registry = matches!(
                    parts[2],
                    "(registry+https://github.com/rust-lang/crates.io-index)"
                        | "(registry+https://index.crates.io/)"
                );
                result.push(package(
                    parts[0].into(),
                    parts[1].into(),
                    (!registry).then_some("Git、本地或自定义仓库包需要原始来源"),
                ));
            }
        }
        "uv" | "pipx" => {
            let base = if tool == "pipx" {
                root.join("venvs")
            } else {
                root.to_path_buf()
            };
            for entry in std::fs::read_dir(&base)? {
                let entry = entry?;
                if !entry.path().is_dir() || entry.file_name().to_string_lossy().starts_with('.') {
                    continue;
                }
                let meta = entry.path().join(if tool == "pipx" {
                    "pipx_metadata.json"
                } else {
                    "uv-receipt.toml"
                });
                if !meta.exists() {
                    continue;
                }
                let read = || -> Result<Package> {
                    let (name, pin, direct) = if tool == "pipx" {
                        let data = json(&meta)?;
                        let main = data
                            .get("main_package")
                            .ok_or_else(|| AppError::msg("pipx 缺少主包元数据"))?;
                        let name = main
                            .get("package")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_owned();
                        let pin = main
                            .get("package_version")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_owned();
                        let spec = main
                            .get("package_or_url")
                            .and_then(Value::as_str)
                            .unwrap_or("");
                        let direct = spec.contains([':', '/', '\\', '[', '@'])
                            || main.get("include_dependencies").and_then(Value::as_bool)
                                == Some(true)
                            || main
                                .get("pip_args")
                                .and_then(Value::as_array)
                                .is_some_and(|v| !v.is_empty())
                            || data
                                .get("injected_packages")
                                .and_then(Value::as_object)
                                .is_some_and(|p| !p.is_empty());
                        (name, pin, direct)
                    } else {
                        let value: toml::Value =
                            toml::from_str(&std::fs::read_to_string(&meta)?)
                                .map_err(|e| AppError::msg(format!("uv receipt 损坏: {e}")))?;
                        let reqs = value
                            .get("tool")
                            .and_then(|v| v.get("requirements"))
                            .and_then(toml::Value::as_array)
                            .ok_or_else(|| AppError::msg("uv 缺少工具安装要求"))?;
                        let req = reqs
                            .first()
                            .ok_or_else(|| AppError::msg("uv 安装要求为空"))?;
                        let name = req
                            .get("name")
                            .and_then(toml::Value::as_str)
                            .unwrap_or("")
                            .to_owned();
                        let installed = crate::pip_reinstall::disk_packages(
                            &entry.path().join("Lib/site-packages"),
                        )?;
                        let normalized = |s: &str| s.to_lowercase().replace(['_', '.'], "-");
                        let found = installed
                            .iter()
                            .find(|p| normalized(&p.name) == normalized(&name));
                        let direct = reqs.len() != 1
                            || req.as_table().is_none_or(|v| {
                                v.keys()
                                    .any(|key| !matches!(key.as_str(), "name" | "specifier"))
                            })
                            || found.is_some_and(|p| p.reason.is_some())
                            || value
                                .get("tool")
                                .and_then(|t| t.get("options"))
                                .and_then(toml::Value::as_table)
                                .is_some_and(|v| !v.is_empty());
                        (
                            name,
                            found.map(|p| p.version.clone()).unwrap_or_default(),
                            direct,
                        )
                    };
                    Ok(package(
                        name,
                        pin,
                        direct.then_some("包含直接来源、附加依赖或注入包，需按原始要求恢复"),
                    ))
                };
                result.push(read().unwrap_or_else(|e| Package {
                    name: entry.file_name().to_string_lossy().into(),
                    version: String::new(),
                    reason: Some(format!("安装记录损坏: {e}")),
                }));
            }
        }
        "composer" => {
            let manifest = json(&root.join("composer.json"))?;
            let installed = json(&root.join("vendor/composer/installed.json"))?;
            let packages = installed
                .get("packages")
                .unwrap_or(&installed)
                .as_array()
                .ok_or_else(|| AppError::msg("Composer installed.json 格式无效"))?;
            let custom = manifest.get("repositories").is_some();
            let names: std::collections::BTreeSet<_> = ["require", "require-dev"]
                .into_iter()
                .flat_map(|key| {
                    manifest
                        .get(key)
                        .and_then(Value::as_object)
                        .into_iter()
                        .flat_map(|values| values.keys())
                })
                .collect();
            for name in names {
                if !name.contains('/') {
                    continue;
                }
                let entry = packages
                    .iter()
                    .find(|p| p.get("name").and_then(Value::as_str) == Some(name));
                let version = entry
                    .and_then(|p| p.get("version"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .trim_start_matches('v')
                    .to_owned();
                result.push(package(
                    name.clone(),
                    version,
                    custom.then_some("使用自定义 Composer 仓库，需要原始仓库配置"),
                ));
            }
        }
        "dotnet" => {
            for entry in std::fs::read_dir(root.join(".store"))? {
                let entry = entry?;
                if !entry.path().is_dir() || entry.file_name().to_string_lossy().starts_with('.') {
                    continue;
                }
                let name = entry.file_name().to_string_lossy().into_owned();
                for version in std::fs::read_dir(entry.path())? {
                    let version = version?;
                    if !version.path().is_dir() {
                        continue;
                    }
                    let pin = version.file_name().to_string_lossy().into_owned();
                    let metadata = version
                        .path()
                        .join(&name)
                        .join(&pin)
                        .join(".nupkg.metadata");
                    let official = json(&metadata)
                        .ok()
                        .and_then(|v| v.get("source").and_then(Value::as_str).map(str::to_owned))
                        .is_some_and(|s| {
                            s.trim_end_matches('/')
                                .eq_ignore_ascii_case("https://api.nuget.org/v3/index.json")
                        });
                    result.push(package(
                        name.clone(),
                        pin,
                        (!official).then_some("无法确认 NuGet 官方仓库来源，需原始源配置"),
                    ));
                }
            }
        }
        _ => return Err(AppError::msg("未知管理器安装记录")),
    }
    let mut counts = BTreeMap::new();
    for item in &result {
        *counts.entry(item.name.to_lowercase()).or_insert(0) += 1;
    }
    for item in &mut result {
        if item.name == tool
            || (tool == "cargo" && item.name == "rustup")
            || item.name == "composer/composer"
        {
            item.reason = Some("管理器自身请通过环境管理功能更新".into());
        }
        if (tool != "composer" && item.name.contains('/'))
            || (tool == "composer" && item.name.split('/').count() != 2)
        {
            item.reason = Some("包名不符合管理器要求".into());
        }
        if counts[&item.name.to_lowercase()] > 1 {
            item.reason = Some("同名工具存在多个版本，请先核对旧目录".into());
        }
    }
    result.sort_by(|a, b| a.name.cmp(&b.name));
    result.dedup_by(|a, b| a.name.eq_ignore_ascii_case(&b.name));
    Ok(result)
}

pub fn installation_path(tool: &str, root: &Path, name: &str) -> Result<PathBuf> {
    if !valid_pin(name, "1.0.0") {
        return Err(AppError::msg("安装记录中的包名无效"));
    }
    let normalized = name.to_lowercase().replace(['_', '.'], "-");
    let path = match tool {
        "uv" => root.join(normalized),
        "pipx" => root.join("venvs").join(normalized),
        "composer" => root.join("vendor").join(name),
        "dotnet" => root.join(".store").join(name),
        "cargo" => root.join("bin"),
        _ => return Err(AppError::msg("未知安装位置类型")),
    };
    let path = path.canonicalize()?;
    if !crate::detect::system::path_within(root, &path) {
        return Err(AppError::msg("安装文件链接到来源目录以外"));
    }
    Ok(crate::fsutil::plain_path(path))
}

pub fn verify_cleanup_files(tool: &str, root: &Path, name: &str) -> Result<()> {
    if tool == "bun" {
        return Ok(());
    }
    let location = installation_path(tool, root, name)?;
    if tool == "uv" {
        let receipt: toml::Value =
            toml::from_str(&std::fs::read_to_string(location.join("uv-receipt.toml"))?)
                .map_err(|e| AppError::msg(e.to_string()))?;
        let entries = receipt
            .get("tool")
            .and_then(|v| v.get("entrypoints"))
            .and_then(toml::Value::as_array)
            .ok_or_else(|| AppError::msg("uv 启动器记录缺失，保留旧包"))?;
        for entry in entries {
            let path = entry
                .get("install-path")
                .and_then(toml::Value::as_str)
                .ok_or_else(|| AppError::msg("uv 启动器路径无法确认"))?;
            let path = Path::new(path);
            if !path.is_absolute() {
                return Err(AppError::msg("uv 启动器记录不是绝对路径"));
            }
            let entry_root = bin_path("uv", root);
            let legacy_entry_root = root.join("bin");
            if crate::detect::system::is_reparse_path(&entry_root)
                || crate::detect::system::is_reparse_path(&legacy_entry_root)
            {
                return Err(AppError::msg(
                    "uv 入口目录是链接/重解析点，不能确认入口归属",
                ));
            }
            // The current layout keeps launchers in a sibling directory. Old
            // installations used root/bin; accept that layout only for its own
            // recorded entrypoint, never an arbitrary external directory.
            if !crate::detect::system::path_within(&entry_root, path)
                && !crate::detect::system::path_within(&legacy_entry_root, path)
            {
                return Err(AppError::msg(
                    "旧 uv 启动器位于声明的入口目录之外，请通过原 uv 工具手动卸载",
                ));
            }
            if crate::detect::system::paths_eq(path, &entry_root)
                || crate::detect::system::paths_eq(path, &legacy_entry_root)
            {
                return Err(AppError::msg("uv 启动器记录指向入口目录本身"));
            }
        }
    }
    if tool == "cargo" {
        let receipt: toml::Value =
            toml::from_str(&std::fs::read_to_string(root.join(".crates.toml"))?)
                .map_err(|e| AppError::msg(e.to_string()))?;
        for (key, bins) in receipt
            .get("v1")
            .and_then(toml::Value::as_table)
            .into_iter()
            .flatten()
        {
            if key.split(' ').next() != Some(name) {
                continue;
            }
            let bins = bins
                .as_array()
                .ok_or_else(|| AppError::msg("Cargo 可执行文件清单损坏"))?;
            if bins.iter().any(|bin| {
                !bin.as_str().is_some_and(|s| {
                    !s.is_empty() && s != "." && s != ".." && !s.contains(['/', '\\', ':'])
                })
            }) {
                return Err(AppError::msg("Cargo 可执行文件记录存在越界路径"));
            }
        }
    }
    Ok(())
}

pub fn detect(path: &Path) -> Option<&'static str> {
    if path.join(".crates.toml").is_file() {
        Some("cargo")
    } else if path.join(".store").is_dir() {
        Some("dotnet")
    } else if path.join("composer.json").is_file()
        && path.join("vendor/composer/installed.json").is_file()
        && json(&path.join("composer.json"))
            .is_ok_and(|p| p.get("name").is_none() && p.get("scripts").is_none())
    {
        Some("composer")
    } else if path.join("venvs").is_dir()
        && path
            .join("venvs")
            .read_dir()
            .ok()
            .is_some_and(|mut entries| {
                entries.any(|e| e.is_ok_and(|e| e.path().join("pipx_metadata.json").is_file()))
            })
    {
        Some("pipx")
    } else if path.read_dir().ok().is_some_and(|mut entries| {
        entries.any(|e| e.is_ok_and(|e| e.path().join("uv-receipt.toml").is_file()))
    }) {
        Some("uv")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(root: &Path, relative: &str, content: &str) {
        let file = root.join(relative);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, content).unwrap();
    }

    #[test]
    fn composer_keeps_direct_requirements_and_blocks_custom_repositories() {
        let d = crate::test_support::TestDir::new();
        write(
            d.path(),
            "composer.json",
            r#"{"require":{"vendor/tool":"^1"},"require-dev":{"vendor/dev":"^2"}}"#,
        );
        write(
            d.path(),
            "vendor/composer/installed.json",
            r#"{"packages":[{"name":"vendor/tool","version":"v1.2.3"},{"name":"vendor/dev","version":"2.0.0"},{"name":"vendor/dependency","version":"3.0.0"}]}"#,
        );
        let rows = inventory("composer", d.path()).unwrap();
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|p| p.reason.is_none()));
        assert_eq!(rows[1].version, "1.2.3");
        write(
            d.path(),
            "composer.json",
            r#"{"require":{"vendor/tool":"^1"},"repositories":[{"type":"path","url":"../custom"}]}"#,
        );
        assert!(inventory("composer", d.path()).unwrap()[0].reason.is_some());
    }

    #[test]
    fn dotnet_requires_verified_source_and_rejects_duplicate_versions() {
        let d = crate::test_support::TestDir::new();
        write(
            d.path(),
            ".store/tool/1.0.0/tool/1.0.0/.nupkg.metadata",
            r#"{"source":"https://api.nuget.org/v3/index.json"}"#,
        );
        assert!(inventory("dotnet", d.path()).unwrap()[0].reason.is_none());
        write(
            d.path(),
            ".store/private/2.0.0/private/2.0.0/.nupkg.metadata",
            r#"{"source":"https://private.invalid/index.json"}"#,
        );
        assert!(inventory("dotnet", d.path())
            .unwrap()
            .iter()
            .find(|p| p.name == "private")
            .unwrap()
            .reason
            .is_some());
        write(
            d.path(),
            ".store/tool/1.1.0/tool/1.1.0/.nupkg.metadata",
            "{}",
        );
        assert!(inventory("dotnet", d.path())
            .unwrap()
            .iter()
            .find(|p| p.name == "tool")
            .unwrap()
            .reason
            .as_ref()
            .unwrap()
            .contains("多个版本"));
    }

    #[test]
    fn damaged_python_tool_receipts_stay_visible_and_custom_arguments_are_blocked() {
        let d = crate::test_support::TestDir::new();
        write(d.path(), "venvs/broken/pipx_metadata.json", "{");
        write(
            d.path(),
            "venvs/tool/pipx_metadata.json",
            r#"{"main_package":{"package":"tool","package_version":"1.0.0","package_or_url":"tool","pip_args":["--index-url","https://custom.invalid"]}}"#,
        );
        let rows = inventory("pipx", d.path()).unwrap();
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|p| p.reason.is_some()));
        write(d.path(), "uv/broken/uv-receipt.toml", "invalid = [");
        assert!(inventory("uv", &d.path().join("uv")).unwrap()[0]
            .reason
            .is_some());
    }

    #[test]
    fn cleanup_rejects_external_locations_and_cargo_launcher_traversal() {
        let d = crate::test_support::TestDir::new();
        std::fs::create_dir_all(d.path().join("cargo/bin")).unwrap();
        write(d.path(), "cargo/.crates.toml", "[v1]\n'cli 1.0.0 (registry+https://github.com/rust-lang/crates.io-index)' = ['../outside.exe']");
        assert!(verify_cleanup_files("cargo", &d.path().join("cargo"), "cli").is_err());
        std::fs::create_dir_all(d.path().join("external")).unwrap();
        std::fs::create_dir_all(d.path().join("pipx/venvs")).unwrap();
        junction::create(d.path().join("external"), d.path().join("pipx/venvs/tool")).unwrap();
        assert!(installation_path("pipx", &d.path().join("pipx"), "tool").is_err());
    }

    #[test]
    fn isolated_manager_roles_are_siblings_and_reject_nested_aliases() {
        let d = crate::test_support::TestDir::new();
        let root = d.path().join("globals/uv-global");
        std::fs::create_dir_all(&root).unwrap();
        let bin = d.path().join("globals/uv-global-bin");
        let cache = d.path().join("globals/uv-global-cache");
        assert_eq!(bin_path("uv", &root), bin);
        assert_eq!(cache_path("uv", &root), cache);
        assert!(validate_layout("uv", Some(&root), Some(&bin), Some(&cache), None).is_ok());

        let nested_bin = root.join("bin");
        let nested_cache = root.join("cache");
        assert!(validate_layout(
            "uv",
            Some(&root),
            Some(&nested_bin),
            Some(&nested_cache),
            None
        )
        .is_err());

        // Case and trailing separators must not bypass the containment check.
        let upper_root = PathBuf::from(format!("{}\\", root.to_string_lossy().to_uppercase()));
        let upper_nested = PathBuf::from(format!("{}\\BIN", root.to_string_lossy().to_uppercase()));
        assert!(validate_layout("uv", Some(&upper_root), Some(&upper_nested), None, None).is_err());
    }

    #[test]
    fn pnpm_and_yarn_classic_keep_package_store_and_command_prefixes_separate() {
        let d = crate::test_support::TestDir::new();
        let root = d.path().join("globals/node-global");
        assert_eq!(bin_path("pnpm", &root), d.path().join("globals/node-global-bin"));
        assert_eq!(cache_path("pnpm", &root), d.path().join("globals/node-global-store"));
        assert_eq!(prefix_path("yarn", &root), d.path().join("globals/node-global-prefix"));
        assert_eq!(
            bin_path("yarn", &root),
            d.path().join("globals/node-global-prefix/bin")
        );
        assert_eq!(
            cache_path("yarn", &root),
            d.path().join("globals/node-global-cache")
        );
        assert!(validate_layout(
            "yarn",
            Some(&root),
            Some(&bin_path("yarn", &root)),
            Some(&cache_path("yarn", &root)),
            None
        )
        .is_ok());
        assert!(validate_layout(
            "yarn",
            Some(&root),
            Some(&bin_path("yarn", &root)),
            Some(&prefix_path("yarn", &root).join("cache")),
            None
        )
        .is_err());
        let config = config_path("yarn", &root);
        assert!(validate_layout(
            "yarn",
            Some(&root),
            Some(&bin_path("yarn", &root)),
            Some(&cache_path("yarn", &root)),
            Some(&config)
        )
        .is_ok());
        assert!(validate_layout(
            "yarn",
            Some(&root),
            Some(&bin_path("yarn", &root)),
            Some(&cache_path("yarn", &root)),
            Some(&prefix_path("yarn", &root))
        )
        .is_err());
    }

    #[test]
    fn npm_reinstall_cache_and_config_must_not_overlap_prefix() {
        let d = crate::test_support::TestDir::new();
        let root = d.path().join("npm");
        std::fs::create_dir(&root).unwrap();
        let bin = bin_path("npm", &root);
        let cache = cache_path("npm", &root);
        let config = config_path("npm", &root);
        assert_eq!(cache, d.path().join("npm-cache"));
        assert!(validate_layout("npm", Some(&root), Some(&bin), Some(&cache), Some(&config)).is_ok());
        assert!(validate_layout("npm", Some(&root), Some(&bin), Some(&root.join("cache")), Some(&config)).is_err());
        assert!(validate_layout("npm", Some(&root), Some(&bin), Some(&cache), Some(&root)).is_err());
        let alias = d.path().join("cache-alias");
        junction::create(&root, &alias).unwrap();
        assert!(validate_layout("npm", Some(&root), Some(&bin), Some(&alias), Some(&config)).is_err());
    }

    #[test]
    fn isolated_manager_roles_resolve_junction_targets_before_validation() {
        let d = crate::test_support::TestDir::new();
        let root = d.path().join("globals/uv-global");
        let cache = d.path().join("globals/uv-global-cache");
        let linked_bin = d.path().join("links/uv-bin");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(linked_bin.parent().unwrap()).unwrap();
        junction::create(&root, &linked_bin).unwrap();
        assert!(validate_layout("uv", Some(&root), Some(&linked_bin), Some(&cache), None).is_err());
    }

    #[test]
    fn command_entry_policy_matches_real_manager_layouts() {
        assert_eq!(command_entry_policy("npm"), Some(CommandEntryPolicy::ToolRoot));
        assert_eq!(command_entry_policy("dotnet"), Some(CommandEntryPolicy::ToolRoot));
        assert_eq!(
            command_entry_policy("cargo"),
            Some(CommandEntryPolicy::FixedSubdirectory("bin"))
        );
        assert_eq!(
            command_entry_policy("composer"),
            Some(CommandEntryPolicy::FixedSubdirectory("vendor/bin"))
        );
        for tool in ["uv", "pipx", "bun", "pnpm"] {
            assert_eq!(
                command_entry_policy(tool),
                Some(CommandEntryPolicy::Sibling("bin")),
                "{tool}"
            );
        }
        assert_eq!(
            command_entry_policy("yarn"),
            Some(CommandEntryPolicy::FixedSubdirectory("prefix/bin"))
        );
        assert!(command_entry_policy("pip").is_none());

        let d = crate::test_support::TestDir::new();
        let root = d.path().join("globals/tool");
        std::fs::create_dir_all(&root).unwrap();
        for (tool, command, cache) in [
            ("npm", root.clone(), d.path().join("globals/npm-cache")),
            ("dotnet", root.clone(), d.path().join("globals/dotnet-cache")),
            ("cargo", root.join("bin"), d.path().join("globals/cargo-cache")),
            (
                "composer",
                root.join("vendor/bin"),
                d.path().join("globals/composer-cache"),
            ),
            ("uv", d.path().join("globals/tool-bin"), d.path().join("globals/tool-cache")),
        ] {
            assert!(
                validate_layout(tool, Some(&root), Some(&command), Some(&cache), None).is_ok(),
                "{tool}"
            );
        }
        // A manager with a fixed child entry still must not put its cache
        // inside the install root or accept a different command directory.
        assert!(validate_layout(
            "cargo",
            Some(&root),
            Some(&root.join("bin")),
            Some(&root.join("bin")),
            None
        )
        .is_err());
        assert!(validate_layout(
            "cargo",
            Some(&root),
            Some(&root.join("other")),
            None,
            None
        )
        .is_err());
        assert!(validate_layout(
            "npm",
            Some(&root),
            Some(&root),
            Some(&root.join("cache")),
            None
        )
        .is_err());
    }

    #[test]
    fn uv_legacy_layout_is_read_only_and_classifies_recovery() {
        let d = crate::test_support::TestDir::new();
        let root = d.path().join("globals/uv-legacy");
        std::fs::create_dir_all(root.join("bin")).unwrap();
        std::fs::create_dir_all(root.join("cache")).unwrap();
        let legacy_entry = root.join("bin/legacy-tool.cmd");
        std::fs::write(&legacy_entry, b"fixture").unwrap();
        let receipt = format!(
            "[tool]\nrequirements = [{{ name = \"legacy-tool\", specifier = \"==1.2.3\" }}]\n\
             [[tool.entrypoints]]\nname = \"legacy-tool\"\ninstall-path = \"{}\"\n",
            legacy_entry.to_string_lossy().replace('\\', "/")
        );
        write(&root, "legacy-tool/uv-receipt.toml", &receipt);
        write(
            &root,
            "legacy-tool/Lib/site-packages/legacy_tool-1.2.3.dist-info/METADATA",
            "Metadata-Version: 2.1\nName: legacy-tool\nVersion: 1.2.3\n",
        );
        let layout = inspect_uv_layout(&root).unwrap();
        assert_eq!(layout.status, "legacy");
        assert!(layout.nested_bin_exists && layout.nested_cache_exists);
        assert_eq!(layout.tools.len(), 1);
        assert_eq!(layout.tools[0].version.as_deref(), Some("1.2.3"));
        assert!(layout.tools[0].needs_recovery);
        assert!(layout.tools[0].recoverable);
        assert!(crate::detect::system::paths_eq(
            Path::new(&layout.proposed_bin),
            &d.path().join("globals/uv-legacy-bin")
        ));
        assert!(!Path::new(&layout.proposed_bin).exists());
        assert!(!Path::new(&layout.proposed_cache).exists());

        let custom_entry = root.join("bin/custom-tool.cmd");
        std::fs::write(&custom_entry, b"fixture").unwrap();
        let custom_receipt = format!(
            "[tool]\nrequirements = [{{ name = \"custom-tool\", specifier = \"==2.0.0\" }}]\n\
             [tool.options]\nindex-url = \"https://private.invalid/simple\"\n\
             [[tool.entrypoints]]\nname = \"custom-tool\"\ninstall-path = \"{}\"\n",
            custom_entry.to_string_lossy().replace('\\', "/")
        );
        write(&root, "custom-tool/uv-receipt.toml", &custom_receipt);
        write(
            &root,
            "custom-tool/Lib/site-packages/custom_tool-2.0.0.dist-info/METADATA",
            "Metadata-Version: 2.1\nName: custom-tool\nVersion: 2.0.0\n",
        );
        let layout = inspect_uv_layout(&root).unwrap();
        let custom = layout
            .tools
            .iter()
            .find(|tool| tool.name == "custom-tool")
            .unwrap();
        assert!(custom.needs_recovery);
        assert!(!custom.recoverable);
        assert!(custom.issues.iter().any(|issue| issue.contains("额外安装选项")));

        write(&root, "broken/uv-receipt.toml", "invalid = [");
        let layout = inspect_uv_layout(&root).unwrap();
        assert_eq!(layout.status, "legacy-partial");
        assert!(layout.tools.iter().any(|tool| tool.name == "broken"
            && tool.receipt_status == "invalid"
            && !tool.recoverable));
        assert!(!Path::new(&layout.proposed_bin).exists());

        let clean = d.path().join("globals/uv-clean");
        std::fs::create_dir_all(clean.join("tool/Lib/site-packages")).unwrap();
        let clean_layout = inspect_uv_layout(&clean).unwrap();
        assert_eq!(clean_layout.status, "clean");
        assert!(clean_layout.tools.is_empty());
    }

    #[test]
    fn uv_cleanup_accepts_declared_sibling_or_legacy_entry_only() {
        let d = crate::test_support::TestDir::new();
        let root = d.path().join("globals/uv-tools");
        let sibling = d.path().join("globals/uv-tools-bin");
        std::fs::create_dir_all(&sibling).unwrap();
        let entry = sibling.join("tool.exe");
        std::fs::write(&entry, b"fixture").unwrap();
        write(
            &root,
            "tool/Lib/site-packages/tool-1.0.0.dist-info/METADATA",
            "Metadata-Version: 2.1\nName: tool\nVersion: 1.0.0\n",
        );
        let receipt = format!(
            "[tool]\nrequirements = [{{ name = \"tool\", specifier = \"==1.0.0\" }}]\n\
             [[tool.entrypoints]]\nname = \"tool\"\ninstall-path = \"{}\"\n",
            entry.to_string_lossy().replace('\\', "/")
        );
        write(&root, "tool/uv-receipt.toml", &receipt);
        assert!(verify_cleanup_files("uv", &root, "tool").is_ok());

        std::fs::create_dir_all(root.join("bin")).unwrap();
        let legacy_entry = root.join("bin/legacy.exe");
        std::fs::write(&legacy_entry, b"fixture").unwrap();
        let legacy_receipt = format!(
            "[tool]\nrequirements = [{{ name = \"tool\", specifier = \"==1.0.0\" }}]\n\
             [[tool.entrypoints]]\nname = \"tool\"\ninstall-path = \"{}\"\n",
            legacy_entry.to_string_lossy().replace('\\', "/")
        );
        std::fs::write(root.join("tool/uv-receipt.toml"), legacy_receipt).unwrap();
        assert!(verify_cleanup_files("uv", &root, "tool").is_ok());

        let external = d.path().join("external-tool.exe");
        std::fs::write(&external, b"fixture").unwrap();
        let external_receipt = format!(
            "[tool]\nrequirements = [{{ name = \"tool\", specifier = \"==1.0.0\" }}]\n\
             [[tool.entrypoints]]\nname = \"tool\"\ninstall-path = \"{}\"\n",
            external.to_string_lossy().replace('\\', "/")
        );
        std::fs::write(root.join("tool/uv-receipt.toml"), external_receipt).unwrap();
        assert!(verify_cleanup_files("uv", &root, "tool").is_err());
    }
}
