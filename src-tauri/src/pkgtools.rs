use crate::error::{AppError, Result};
use crate::package_managers as managers;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::process::Command;

#[cfg(test)]
pub(crate) static QUERY_COMMAND_ATTEMPTS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolConfig {
    pub tool: String,
    pub name: String,
    pub available: bool,
    /// Version returned by the exact executable selected for this manager.
    #[serde(default)]
    pub version: Option<String>,
    pub executable_path: Option<String>,
    pub error: Option<String>,
    pub global_path: Option<String>,
    pub cache_path: Option<String>,
    pub default_global: Option<String>,
    pub default_cache: Option<String>,
    /// Expected command directory for the selected global root.
    pub command_path: Option<String>,
    /// "queried" when the manager returned the entry, "layout" when derived from the declared layout.
    #[serde(default)]
    pub command_path_source: Option<String>,
    /// Read-only warning for a manager layout that must be recovered first.
    pub layout_warning: Option<String>,
    /// Runtime reason this installation cannot use its own self-update (for
    /// example a Corepack shim). The catalog says whether a self-update exists;
    /// only the resolved command can say whether it applies here.
    #[serde(default)]
    pub self_update_blocker: Option<String>,
}

#[derive(Clone)]
#[derive(Debug)]
pub(crate) struct Invocation {
    pub program: String,
    pub prefix: Vec<String>,
}

/// How a manager was installed, derived from evidence next to the resolved
/// command. The static catalog only says whether a self-update *exists*; it
/// cannot know whether the copy on this machine belongs to Corepack or to a
/// Python interpreter, and blaming the wrong updater would be worse than
/// refusing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UpdateChannel {
    /// Directly installed; the manager's own self-update is authoritative.
    Standalone,
    /// A Corepack shim. The active version is pinned by Corepack, so a
    /// global self-update would fight the pin.
    Corepack,
    /// Installed as a Python package into an interpreter's `Scripts`.
    PythonPackage,
}

/// A Corepack shim is a small launcher that names Corepack itself, so the file
/// content is the evidence. Only readable files are inspected: an unreadable
/// shim is not evidence of anything and falls back to `Standalone`.
fn is_corepack_shim(path: &Path) -> bool {
    std::fs::read_to_string(path)
        .map(|text| text.to_ascii_lowercase().contains("corepack"))
        .unwrap_or(false)
}

pub(crate) fn update_channel(tool: &str, inv: &Invocation) -> UpdateChannel {
    let path = Path::new(&inv.program);
    let Some(parent) = path.parent() else {
        return UpdateChannel::Standalone;
    };
    match tool {
        "pnpm" | "yarn" => {
            if is_corepack_shim(path) {
                UpdateChannel::Corepack
            } else {
                UpdateChannel::Standalone
            }
        }
        "uv" => {
            // A pip-installed uv lives in the interpreter's Scripts directory:
            // pip.exe sits beside it and python.exe one level up. Its own
            // `self update` would replace files pip believes it owns.
            let interpreter = [parent.join("python.exe"), parent.parent().unwrap_or(parent).join("python.exe")]
                .into_iter()
                .any(|candidate| candidate.is_file());
            if interpreter && parent.join("pip.exe").is_file() {
                UpdateChannel::PythonPackage
            } else {
                UpdateChannel::Standalone
            }
        }
        // `composer` and `bun` deliberately have no shape check. A composer
        // launcher only proves *that* it wraps composer.phar, never that some
        // other installer did not create it; every Windows bun flavour writes
        // the same static bun.exe, and scoop/winget add a shim outside the
        // install. A positive match here would be a guess, so these stay
        // `Standalone` rather than disabling an update that may well work.
        _ => UpdateChannel::Standalone,
    }
}

/// The runtime reason a manager cannot use its own self-update, or `None` when
/// the resolved command is authoritative. Kept in one place so the tool list,
/// the preview and the executor cannot disagree about the same installation.
pub(crate) fn self_update_channel_blocker(tool: &str, inv: &Invocation) -> Option<String> {
    match update_channel(tool, inv) {
        UpdateChannel::Standalone => None,
        UpdateChannel::Corepack => Some(format!(
            "{tool} 由 Corepack 管理（命令文件指向 Corepack），自更新会与 Corepack 的版本固定冲突；请在项目中使用 corepack prepare {tool}@latest --activate，或先关闭 Corepack 管理"
        )),
        UpdateChannel::PythonPackage => Some(
            "此 uv 由 pip 安装（与 python.exe、pip.exe 同目录），uv self update 会覆盖 pip 管理的文件；请改用 python -m pip install --upgrade uv".into(),
        ),
    }
}

/// What EnvCon's update entry point offers, which is narrower than what the
/// manager's own CLI can do. Every command built here targets "latest", so the
/// entry point has no version argument at all; describing that as "downgrade
/// unsupported" would wrongly read as a manager limitation. The capability
/// therefore states the shape of the entry point and, where the manager really
/// does document one, hands back a command the user can run themselves.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCapabilities {
    /// The entry point installs "latest" and accepts no version argument, so
    /// neither a pinned install nor a downgrade is reachable from here.
    pub latest_only: bool,
    /// A documented command for installing a specific version, for managers
    /// whose CLI has one. `None` means "no verified one-liner", never
    /// "impossible".
    pub manual_pinned_command: Option<String>,
}

pub(crate) fn update_capabilities(tool: &str) -> UpdateCapabilities {
    let manual_pinned_command = match tool {
        "npm" => Some("npm install --global npm@<version>"),
        "pnpm" => Some("pnpm add --global pnpm@<version>"),
        "pip" => Some("python -m pip install pip==<version>"),
        "composer" => Some("composer self-update <version>"),
        "uv" => Some("uv self update <version>"),
        // Yarn Classic has no stable global pin command, and Berry pins per
        // project; neither is a one-liner this module can stand behind.
        "yarn" => None,
        // bun documents no version-pinned self upgrade.
        "bun" => None,
        // These have no general self-update at all, so there is nothing to pin.
        _ => None,
    };
    UpdateCapabilities {
        latest_only: true,
        manual_pinned_command: manual_pinned_command.map(str::to_owned),
    }
}

pub(crate) fn self_update_command(
    tool: &str,
    inv: &Invocation,
) -> Result<(Invocation, Vec<String>)> {
    let mut updater = inv.clone();
    // Installation shape is checked before any command is built: a Corepack
    // shim or a pip-installed uv must not be updated by the manager's own
    // self-update, because that would fight whoever actually owns the files.
    if let Some(blocker) = self_update_channel_blocker(tool, inv) {
        return Err(AppError::msg(blocker));
    }
    let mut arguments = match tool {
        "npm" | "yarn" => vec!["install", "--global", if tool == "npm" { "npm@latest" } else { "yarn@latest" }],
        "pnpm" => vec!["self-update"],
        "bun" => vec!["upgrade"],
        "uv" => vec!["self", "update"],
        "pip" => vec!["install", "--upgrade", "pip"],
        "composer" => vec!["self-update"],
        "pipx" => return Err(AppError::msg("pipx 没有通用自更新命令，请通过安装它的 Python 执行 python -m pip install --upgrade pipx，或通过原安装管理器更新")),
        "cargo" => return Err(AppError::msg("Cargo 随 Rust 工具链发布，请运行 rustup update stable")),
        "dotnet" => return Err(AppError::msg(".NET CLI 由 .NET SDK 随附，请更新 SDK 后重启应用")),
        _ => return Err(AppError::msg("不支持的包管理器")),
    }.drain(..).map(str::to_owned).collect::<Vec<_>>();
    let parent = Path::new(&inv.program)
        .parent()
        .ok_or_else(|| AppError::msg("命令路径无效"))?;
    if matches!(tool, "npm" | "yarn") {
        let prefix = parent.canonicalize()?;
        if !prefix
            .join("node_modules")
            .join(tool)
            .join("package.json")
            .is_file()
            || !prefix.join("npm.cmd").is_file()
        {
            return Err(AppError::msg("无法确认当前命令对应的 npm 安装目录；Corepack 或其他安装方式请通过原安装管理器更新"));
        }
        updater.program = prefix.join("npm.cmd").to_string_lossy().into();
        updater.prefix.clear();
        arguments.extend(["--prefix".into(), prefix.to_string_lossy().into()]);
    } else if tool == "pip" && inv.prefix.is_empty() {
        let python = [
            parent.join("python.exe"),
            parent.parent().unwrap_or(parent).join("python.exe"),
        ]
        .into_iter()
        .find(|p| p.is_file())
        .ok_or_else(|| AppError::msg("未找到 pip 所属解释器，请通过原 Python 安装更新 pip"))?;
        updater.program = python.to_string_lossy().into();
        updater.prefix = vec!["-m".into(), "pip".into()];
    }
    Ok((updater, arguments))
}

fn effective_path(root: Option<&Path>) -> std::ffi::OsString {
    let mut entries = Vec::new();
    if let Some(root) = root {
        for kind in crate::types::ALL_ENV_TYPES {
            entries.extend(kind.path_entries(root));
        }
    }
    entries.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    std::env::join_paths(entries).unwrap_or_default()
}

async fn where_exe(root: Option<&Path>, name: &str) -> Option<String> {
    #[cfg(test)]
    QUERY_COMMAND_ATTEMPTS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let out = tokio::time::timeout(
        Duration::from_secs(5),
        Command::new("where.exe")
            .arg(name)
            .env("PATH", effective_path(root))
            .creation_flags(0x08000000)
            .kill_on_drop(true)
            .output(),
    )
    .await
    .ok()?
    .ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_owned)
}

pub(crate) async fn find_invocation(root: Option<&Path>, name: &str) -> Option<Invocation> {
    if let Some(root) = root {
        let direct = match name {
            "npm" => Some(root.join("current/node/npm.cmd")),
            "pnpm" => Some(root.join("current/node/pnpm.cmd")),
            "yarn" => Some(root.join("current/node/yarn.cmd")),
            "pip" => Some(root.join("current/python/Scripts/pip.exe")),
            "bun" => Some(root.join("current/bun/bun.exe")),
            "cargo" => Some(root.join("current/rust/bin/cargo.exe")),
            _ => None,
        };
        if let Some(path) = direct.filter(|path| path.is_file()) {
            return Some(Invocation {
                program: path.to_string_lossy().into(),
                prefix: vec![],
            });
        }
        if name == "pip" {
            let python = root.join("current/python/python.exe");
            if python.is_file() {
                return Some(Invocation {
                    program: python.to_string_lossy().into(),
                    prefix: vec!["-m".into(), "pip".into()],
                });
            }
        }
    }
    let command = if name == "pip" {
        "pip.exe".to_owned()
    } else {
        format!("{name}.cmd")
    };
    let program = match where_exe(root, &command).await {
        Some(p) => Some(p),
        None => match where_exe(root, &format!("{name}.exe")).await {
            Some(p) => Some(p),
            None => where_exe(root, name).await,
        },
    };
    if let Some(program) = program {
        return Some(Invocation {
            program,
            prefix: vec![],
        });
    }
    if name == "pip" {
        return Some(Invocation {
            program: where_exe(root, "python").await?,
            prefix: vec!["-m".into(), "pip".into()],
        });
    }
    None
}

pub(crate) async fn output(
    inv: &Invocation,
    args: &[&str],
    cwd: Option<&Path>,
    timeout_secs: u64,
) -> std::result::Result<std::process::Output, String> {
    #[cfg(test)]
    QUERY_COMMAND_ATTEMPTS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let mut command = Command::new(&inv.program);
    if let Ok(environment) = crate::pathman::get_user_env_vars() {
        for (name, value) in environment {
            if managers::definitions().iter().any(|m| {
                [
                    managers::global_env(&m.id),
                    managers::bin_env(&m.id),
                    managers::cache_env(&m.id),
                ]
                .contains(&Some(name.as_str()))
            }) {
                command.env(name, value);
            }
        }
    }
    command
        .args(&inv.prefix)
        .args(args)
        .creation_flags(0x08000000);
    command.current_dir(
        cwd.unwrap_or_else(|| Path::new(&inv.program).parent().unwrap_or(Path::new("."))),
    );
    command
        .env("COREPACK_ENABLE_PROJECT_SPEC", "0")
        .env("COREPACK_ENABLE_NETWORK", "0")
        .env("NO_COLOR", "1");
    let mut paths = vec![Path::new(&inv.program)
        .parent()
        .unwrap_or(Path::new("."))
        .to_path_buf()];
    paths.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    if let Ok(path) = std::env::join_paths(paths) {
        command.env("PATH", path);
    }
    crate::reinstall_process::output(command, Duration::from_secs(timeout_secs))
        .await
        .map_err(|e| e.to_string())
}

/// Execute a mutating package-manager command and require a successful exit.
pub(crate) async fn mutate(
    inv: &Invocation,
    args: &[&str],
    timeout_secs: u64,
) -> std::result::Result<String, String> {
    let output = execute(inv, args, timeout_secs).await?;
    let text = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    let err = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    Ok(if text.is_empty() { err } else { text })
}

async fn execute(
    inv: &Invocation,
    args: &[&str],
    timeout_secs: u64,
) -> std::result::Result<std::process::Output, String> {
    let output = output(inv, args, None, timeout_secs).await?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let detail = if stderr.trim().is_empty() {
            stdout.trim()
        } else {
            stderr.trim()
        };
        return Err(format!(
            "退出码 {}{}",
            output.status.code().unwrap_or(-1),
            if detail.is_empty() {
                String::new()
            } else {
                format!(": {}", detail.chars().take(500).collect::<String>())
            }
        ));
    }
    Ok(output)
}

pub(crate) async fn run(
    inv: &Invocation,
    args: &[&str],
    timeout_secs: u64,
) -> std::result::Result<String, String> {
    let output = execute(inv, args, timeout_secs).await?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let value = stdout
        .lines()
        .map(str::trim)
        .find(|line| {
            !line.is_empty()
                && !matches!(line.to_lowercase().as_str(), "undefined" | "null" | "none")
        })
        .map(str::to_owned);
    value.ok_or_else(|| "命令没有返回有效内容".into())
}

pub(crate) fn display_version(output: &str) -> Option<String> {
    output.split_whitespace()
        .map(|word| word.trim_start_matches('v').trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '.' && c != '-'))
        .find(|word| word.starts_with(|c: char| c.is_ascii_digit()) && word.contains('.'))
        .map(str::to_string)
}

// Mutating commands may succeed silently; only queries require a returned value.
async fn set(inv: &Invocation, args: &[&str]) -> Result<()> {
    execute(inv, args, 30)
        .await
        .map(|_| ())
        .map_err(|e| AppError::msg(format!("配置命令失败: {e}")))
}

async fn optional_path(inv: &Invocation, args: &[&str]) -> Option<String> {
    run(inv, args, 25)
        .await
        .ok()
        .filter(|value| Path::new(value).is_absolute())
}

fn command_path_with_source(
    queried: Option<String>,
    fallback: Option<String>,
) -> (Option<String>, Option<String>) {
    match (queried, fallback) {
        (Some(command), _) => (Some(command), Some("queried".into())),
        (None, Some(command)) => (Some(command), Some("layout".into())),
        (None, None) => (None, None),
    }
}

pub(crate) async fn values(
    inv: &Invocation,
    tool: &str,
) -> (Option<String>, Option<String>, Option<String>, Option<String>) {
    let mut errors = Vec::new();
    let mut q = |r: std::result::Result<String, String>| match r {
        Ok(path) if Path::new(&path).is_absolute() => Some(path),
        Ok(_) => {
            errors.push("路径查询未返回绝对路径".to_string());
            None
        }
        Err(e) => {
            errors.push(e);
            None
        }
    };
    let (global, command, cache, error) = match tool {
        "npm" => {
            let global = q(run(inv, &["config", "get", "prefix"], 25).await);
            (
                global.clone(),
                global,
                q(run(inv, &["config", "get", "cache"], 25).await),
                None,
            )
        }
        "pnpm" => {
            let global = match run(inv, &["config", "get", "global-dir"], 25).await {
                Ok(v) => Some(v),
                Err(_) => q(run(inv, &["root", "--global"], 25).await),
            };
            (
                global,
                optional_path(inv, &["config", "get", "global-bin-dir"]).await,
                q(run(inv, &["store", "path"], 25).await),
                None,
            )
        }
        "yarn" => {
            let global = match run(inv, &["global", "dir", "--silent"], 25).await {
                Ok(v) => Some(v),
                Err(_) => q(run(inv, &["config", "get", "globalFolder"], 25).await),
            };
            let cache = match run(inv, &["cache", "dir", "--silent"], 25).await {
                Ok(v) => Some(v),
                Err(_) => q(run(inv, &["config", "get", "cacheFolder"], 25).await),
            };
            (
                global,
                optional_path(inv, &["global", "bin"]).await,
                cache,
                None,
            )
        }
        "pip" => {
            let cache = match run(inv, &["cache", "dir"], 25).await {
                Ok(v) => Some(v),
                Err(_) => q(run(inv, &["config", "get", "global.cache-dir"], 25).await),
            };
            (None, None, cache, None)
        }
        "bun" => {
            let global = managers::default_source(tool).map(|p| p.to_string_lossy().into());
            (
                global,
                optional_path(inv, &["pm", "bin", "-g"]).await,
                q(run(inv, &["pm", "cache"], 25).await),
                None,
            )
        }
        "uv" => {
            let global = q(run(inv, &["tool", "dir"], 25).await);
            (
                global,
                optional_path(inv, &["tool", "dir", "--bin"]).await,
                q(run(inv, &["cache", "dir"], 25).await),
                None,
            )
        }
        "pipx" => {
            let global = q(run(inv, &["environment", "--value", "PIPX_HOME"], 25).await);
            (
                global,
                optional_path(inv, &["environment", "--value", "PIPX_BIN_DIR"]).await,
                managers::configured_env_path("PIP_CACHE_DIR")
                    .or_else(|| dirs::cache_dir().map(|p| p.join("pip/Cache")))
                    .map(|p| p.to_string_lossy().into()),
                None,
            )
        }
        "cargo" => {
            let global = managers::default_source(tool).map(|p| p.to_string_lossy().into());
            (
                global,
                None,
                Some(
                    managers::configured_env_path("CARGO_HOME")
                        .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".cargo"))
                        .join("registry")
                        .to_string_lossy()
                        .into(),
                ),
                None,
            )
        }
        "composer" => {
            let global = q(run(inv, &["config", "--global", "home"], 25).await);
            (
                global,
                optional_path(inv, &["config", "--global", "bin-dir"]).await,
                q(run(inv, &["config", "--global", "cache-dir", "--absolute"], 25).await),
                None,
            )
        }
        "dotnet" => {
            let cache = run(inv, &["nuget", "locals", "global-packages", "--list"], 25)
                .await
                .and_then(|s| {
                    s.strip_prefix("global-packages:")
                        .map(|s| s.trim().to_owned())
                        .ok_or_else(|| "无法识别 NuGet 缓存目录".into())
                });
            (
                managers::default_source(tool).map(|p| p.to_string_lossy().into()),
                None,
                q(cache),
                None,
            )
        }
        _ => (None, None, None, Some("未知工具".into())),
    };
    (
        global,
        command,
        cache,
        error.or_else(|| {
            if errors.is_empty() {
                None
            } else {
                Some(format!("部分路径读取失败: {}", errors.join("；")))
            }
        }),
    )
}

pub async fn get_tool_configs(root: Option<&Path>) -> Vec<ToolConfig> {
    use futures_util::{stream, StreamExt};
    stream::iter(managers::definitions().to_vec())
        .map(|definition| async move {
            let tool = definition.id.as_str();
            let name = definition.name.as_str();
            let inv = find_invocation(root, tool).await;
            let (available, version, global, cache, error, command) = if let Some(inv) = &inv {
                match run(inv, &["--version"], 15).await {
                    Ok(version) => {
                        let (g, command, c, e) = values(inv, tool).await;
                        let version = display_version(&version);
                        let berry = tool == "yarn"
                            && version
                                .as_deref()
                                .and_then(|value| value.split('.').next())
                                .and_then(|value| value.parse::<u32>().ok())
                                .is_some_and(|major| major >= 2);
                        (
                            true,
                            version,
                            if berry { None } else { g },
                            c,
                            e,
                            if berry { None } else { command },
                        )
                    }
                    Err(e) => (
                        false,
                        None,
                        None,
                        None,
                        Some(format!("版本探测失败: {e}")),
                        None,
                    ),
                }
            } else {
                (
                    false,
                    None,
                    None,
                    None,
                    Some("未找到可执行命令".into()),
                    None,
                )
            };
            let yarn_classic = tool != "yarn"
                || version.as_deref().is_some_and(|value| value.starts_with("1."));
            let base = root.map(|r| r.join("globals"));
            let default_global = base
                .as_ref()
                .filter(|_| definition.global_config && yarn_classic)
                .map(|p| p.join(format!("{tool}-global")));
            let default_cache = base
                .as_ref()
                .filter(|_| definition.cache_config)
                .map(|p| {
                    if matches!(tool, "uv" | "pipx" | "bun" | "pnpm")
                        || (tool == "yarn" && yarn_classic)
                    {
                        default_global
                            .as_deref()
                            .map(|global| managers::cache_path(tool, global))
                            .unwrap_or_else(|| p.join(format!("{tool}-cache")))
                    } else {
                        p.join(format!("{tool}-cache"))
                    }
                });
            let selected_global = global
                .as_deref()
                .map(PathBuf::from)
                .or_else(|| default_global.clone());
            let fallback_command = if definition.global_config {
                selected_global.as_deref().map(|path| {
                    managers::bin_path(tool, path)
                        .to_string_lossy()
                        .into_owned()
                })
            } else {
                None
            };
            let (command_path, command_path_source) =
                command_path_with_source(command, fallback_command);
            let layout_warning = if tool == "uv" {
                selected_global
                    .as_deref()
                    .filter(|path| path.is_dir())
                    .and_then(|path| match managers::inspect_uv_layout(path) {
                        Ok(layout) if layout.status != "clean" => {
                            let recoverable = layout.tools.iter().filter(|tool| tool.recoverable).count();
                            let partial = layout.tools.iter()
                                .filter(|tool| tool.needs_recovery && !tool.recoverable)
                                .count();
                            Some(format!(
                                "检测到 uv 旧嵌套布局（{} 个工具可规划恢复，{} 个需人工核对）；建议入口 {}，缓存 {}。此检查只读，未移动或删除任何文件。",
                                recoverable,
                                partial,
                                layout.proposed_bin,
                                layout.proposed_cache,
                            ))
                        }
                        Ok(_) => None,
                        Err(error) => Some(format!("无法检查 uv 旧布局: {error}")),
                    })
            } else {
                None
            };
            ToolConfig {
                tool: tool.into(),
                name: name.into(),
                available,
                version,
                executable_path: inv.as_ref().map(|i| i.program.clone()),
                error,
                global_path: global,
                cache_path: cache,
                default_global: default_global.map(|p| p.to_string_lossy().into()),
                default_cache: default_cache.map(|p| p.to_string_lossy().into()),
                command_path,
                command_path_source,
                layout_warning,
                // The catalog decides whether a self-update exists; this decides
                // whether it is usable for the command that was actually found.
                self_update_blocker: inv
                    .as_ref()
                    .and_then(|invocation| self_update_channel_blocker(tool, invocation)),
            }
        })
        .buffered(4)
        .collect()
        .await
}

/// 返回包管理器实际查询到的缓存目录，供缓存页与配置页使用同一来源。
pub async fn effective_cache_paths(
    root: Option<&Path>,
) -> std::collections::HashMap<String, std::path::PathBuf> {
    get_tool_configs(root)
        .await
        .into_iter()
        .filter_map(|tool| {
            tool.cache_path
                .map(|path| (tool.tool, std::path::PathBuf::from(path)))
        })
        .collect()
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PathIntegrationStatus {
    pub expected: String,
    pub user_path_contains: bool,
    pub new_terminal_contains: bool,
    pub current_process_contains: bool,
    /// `active`, `requires-new-terminal`, `current-process-only`,
    /// `duplicate`, or `missing`.
    pub status: String,
    pub detail: String,
    /// Command whose resolution was checked inside the entry directory.
    #[serde(default)]
    pub command: String,
    /// First matching launcher in the merged new-terminal PATH order.
    #[serde(default)]
    pub resolved_command: Option<String>,
    /// `managed`, `shadowed`, `missing`, or `unknown`.
    #[serde(default)]
    pub command_status: String,
    /// Explicit explanation of what would actually run, separate from PATH membership.
    #[serde(default)]
    pub command_detail: String,
    /// Launchers present in the entry directory, resolved against the merged PATH.
    #[serde(default)]
    pub launchers: Vec<LauncherStatus>,
}

/// Read-only result of resolving a launcher through an ordered PATH list.
/// This never executes the command and never writes PATH.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CommandResolution {
    pub resolved: Option<String>,
    /// `managed` when the first match is the expected entry, `shadowed` when
    /// another directory wins, `missing` when no launcher exists.
    pub status: &'static str,
    pub detail: String,
}

const DEFAULT_COMMAND_EXTENSIONS: [&str; 4] = [".COM", ".EXE", ".BAT", ".CMD"];

fn command_extensions() -> Vec<String> {
    std::env::var("PATHEXT")
        .ok()
        .map(|value| {
            value
                .split(';')
                .map(str::trim)
                .filter(|entry| !entry.is_empty())
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .filter(|entries| !entries.is_empty())
        .unwrap_or_else(|| DEFAULT_COMMAND_EXTENSIONS.iter().map(|e| (*e).into()).collect())
}

/// Expand `%VAR%` references that appear in registry PATH values.
/// Unknown names are kept verbatim instead of silently dropping the entry.
fn expand_env_references(entry: &str) -> String {
    let mut result = String::new();
    let mut rest = entry;
    while let Some(start) = rest.find('%') {
        result.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('%') {
            Some(end) if end > 0 => {
                let name = &after[..end];
                match std::env::var(name) {
                    Ok(value) => result.push_str(&value),
                    Err(_) => {
                        result.push('%');
                        result.push_str(name);
                        result.push('%');
                    }
                }
                rest = &after[end + 1..];
            }
            Some(_) => {
                result.push('%');
                rest = after;
            }
            None => {
                result.push_str(&rest[start..]);
                rest = "";
            }
        }
    }
    result.push_str(rest);
    result
}

/// Merge registry PATH values in the order a new process receives them:
/// machine entries first, then user entries.
fn merged_path_entries(system: &[String], user: &[String]) -> Vec<String> {
    system
        .iter()
        .chain(user.iter())
        .map(|entry| expand_env_references(entry))
        .map(|entry| entry.trim().trim_matches('"').to_string())
        .filter(|entry| !entry.is_empty())
        .collect()
}

fn resolve_command_in(
    entries: &[String],
    expected: &Path,
    command: &str,
    extensions: &[String],
    exists: impl Fn(&Path) -> bool,
) -> CommandResolution {
    if command.is_empty() || command.contains(['\\', '/']) {
        return CommandResolution {
            resolved: None,
            status: "missing",
            detail: "命令名无效，无法解析 PATH 优先级".into(),
        };
    }
    let literal = Path::new(command).extension().is_some();
    for raw in entries {
        let directory = Path::new(raw);
        if !directory.is_absolute() {
            // Windows resolves relative PATH entries against the process working
            // directory, which this app does not control; never claim a match.
            continue;
        }
        let candidates: Vec<std::ffi::OsString> = if literal {
            vec![command.into()]
        } else {
            extensions
                .iter()
                .map(|extension| format!("{command}{extension}").into())
                .collect()
        };
        for candidate in candidates {
            let path = directory.join(&candidate);
            if exists(&path) {
                let managed = crate::detect::system::paths_eq(directory, expected);
                return CommandResolution {
                    resolved: Some(path.to_string_lossy().into()),
                    status: if managed { "managed" } else { "shadowed" },
                    detail: if managed {
                        format!(
                            "PATH 解析命中受管入口：{}",
                            path.to_string_lossy()
                        )
                    } else {
                        format!(
                            "PATH 靠前位置命中了其他安装：{}；受管入口 {} 不会先执行，旧版本仍可能生效",
                            path.to_string_lossy(),
                            expected.display()
                        )
                    },
                };
            }
        }
    }
    CommandResolution {
        resolved: None,
        status: "missing",
        detail: format!(
            "在 PATH 中未找到 {command}；入口目录 {} 可能尚未写入，或命令名由启动器另行提供",
            expected.display()
        ),
    }
}

/// One launcher found in the managed entry directory and the outcome of
/// resolving its name through the merged PATH.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LauncherStatus {
    pub name: String,
    /// `managed`, `shadowed`, or `unknown`.
    pub status: String,
    pub resolved: Option<String>,
}

const MAX_REPORTED_LAUNCHERS: usize = 12;

fn entry_launcher_names(directory: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return Vec::new();
    };
    let mut names = entries
        .flatten()
        .filter(|entry| entry.path().is_file())
        .filter_map(|entry| {
            let path = entry.path();
            let extension = path.extension()?.to_string_lossy().to_ascii_lowercase();
            if !matches!(extension.as_str(), "exe" | "cmd" | "bat" | "com") {
                return None;
            }
            path.file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
        })
        .collect::<Vec<_>>();
    names.sort();
    names.dedup();
    names.truncate(MAX_REPORTED_LAUNCHERS);
    names
}

/// Resolve the launchers that already exist in `expected`. Only names are read;
/// nothing is executed. A launcher that a different directory shadows is
/// reported instead of being counted as effective.
pub(crate) fn launcher_statuses(
    expected: &Path,
    entries: &[String],
    extensions: &[String],
) -> Vec<LauncherStatus> {
    entry_launcher_names(expected)
        .into_iter()
        .map(|name| {
            let resolution = resolve_command_in(entries, expected, &name, extensions, |path| {
                path.is_file()
            });
            LauncherStatus {
                name,
                status: resolution.status.to_string(),
                resolved: resolution.resolved,
            }
        })
        .collect()
}

pub(crate) fn command_resolution(expected: &Path, command: &str) -> CommandResolution {
    let Ok(state) = crate::pathman::get_path_state() else {
        return CommandResolution {
            resolved: None,
            status: "unknown",
            detail: "无法读取用户/系统 PATH，未声称命令已经生效".into(),
        };
    };
    let entries = merged_path_entries(&state.system, &state.user);
    resolve_command_in(&entries, expected, command, &command_extensions(), |path| {
        path.is_file()
    })
}

/// Resolve a command against the supplied merged PATH entries. Shared with the
/// switch diagnosis so both paths use one resolution rule.
pub(crate) fn resolve_command_for_diagnostics(
    entries: &[String],
    expected: &Path,
    command: &str,
    extensions: &[String],
) -> CommandResolution {
    resolve_command_in(entries, expected, command, extensions, |path| path.is_file())
}

pub(crate) fn merged_path_entries_for_diagnostics() -> Vec<String> {
    match crate::pathman::get_path_state() {
        Ok(state) => merged_path_entries(&state.system, &state.user),
        Err(_) => Vec::new(),
    }
}

pub(crate) fn command_extensions_for_diagnostics() -> Vec<String> {
    command_extensions()
}

pub(crate) fn launcher_exists(directory: &Path, command: &str, extensions: &[String]) -> bool {
    if !directory.is_absolute() {
        return false;
    }
    let literal = Path::new(command).extension().is_some();
    let candidates: Vec<std::ffi::OsString> = if literal {
        vec![command.into()]
    } else {
        extensions
            .iter()
            .map(|extension| format!("{command}{extension}").into())
            .collect()
    };
    candidates
        .into_iter()
        .any(|candidate| directory.join(candidate).is_file())
}

/// Membership check over an already merged PATH string, without reading the
/// registry again.
pub(crate) fn path_integration_status_from_entries(
    joined: &str,
    expected: &Path,
    command: Option<&str>,
) -> PathIntegrationStatus {
    let entries = joined
        .split(';')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let user = entries.clone();
    path_integration_status_with(
        &user,
        &[],
        &entries,
        expected,
        command,
        |expected, command| command_resolution(expected, command),
        Vec::new(),
    )
}

#[cfg(test)]
pub(crate) fn path_integration_status_from(
    user: &[String],
    system: &[String],
    current: &[String],
    expected: &Path,
) -> PathIntegrationStatus {
    path_integration_status_with(
        user,
        system,
        current,
        expected,
        None,
        |_, _| CommandResolution {
            resolved: None,
            status: "unknown",
            detail: "未指定要核对的可执行命令".into(),
        },
        Vec::new(),
    )
}

/// Same membership check, plus a read-only resolution of `command` through the
/// merged new-terminal PATH. Resolution never executes anything, and a shadowed
/// command is reported instead of treating an appended directory as success.
#[allow(clippy::too_many_arguments)]
pub(crate) fn path_integration_status_with(
    user: &[String],
    system: &[String],
    current: &[String],
    expected: &Path,
    command: Option<&str>,
    resolve: impl Fn(&Path, &str) -> CommandResolution,
    launchers: Vec<LauncherStatus>,
) -> PathIntegrationStatus {
    let matches = |entry: &String| crate::detect::system::paths_eq(Path::new(entry), expected);
    let user_matches = user.iter().filter(|entry| matches(entry)).count();
    let system_matches = system.iter().filter(|entry| matches(entry)).count();
    let current_matches = current.iter().filter(|entry| matches(entry)).count();
    let in_user = user_matches > 0;
    let in_new = in_user || system_matches > 0;
    let in_current = current_matches > 0;
    let (status, detail) = if user_matches > 1 {
        (
            "duplicate",
            "用户 PATH 中存在多个等价入口，旧条目可能优先命中；请先核对再合并".to_string(),
        )
    } else if in_current && in_new {
        (
            "active",
            "入口目录已写入用户/系统 PATH，且当前应用进程的 PATH 也包含它".to_string(),
        )
    } else if in_current && !in_new {
        (
            "current-process-only",
            "当前进程 PATH 包含入口目录，但用户/系统注册表 PATH 不包含；新终端不会继承".to_string(),
        )
    } else if in_new && !in_current {
        (
            "requires-new-terminal",
            "入口目录已写入用户或系统 PATH，但当前应用进程尚未刷新；请在新终端核对命令".to_string(),
        )
    } else if in_new {
        (
            "requires-new-terminal",
            "入口目录已在用户或系统 PATH 中；请在新终端核对命令".to_string(),
        )
    } else {
        (
            "missing",
            "入口目录不在用户/系统 PATH 中；命令不会在新终端直接可用".to_string(),
        )
    };
    let (command, resolved_command, command_status, command_detail) = match command {
        Some(command) => {
            let resolution = resolve(expected, command);
            (
                command.to_string(),
                resolution.resolved,
                resolution.status.to_string(),
                resolution.detail,
            )
        }
        None => (
            String::new(),
            None,
            "unknown".to_string(),
            "未指定要核对的可执行命令".to_string(),
        ),
    };
    PathIntegrationStatus {
        expected: expected.to_string_lossy().into(),
        user_path_contains: in_user,
        new_terminal_contains: in_new,
        current_process_contains: in_current,
        status: status.into(),
        detail,
        command,
        resolved_command,
        command_status,
        command_detail,
        launchers,
    }
}

pub(crate) fn path_integration_status(
    expected: &Path,
    command: Option<&str>,
) -> PathIntegrationStatus {
    let current = std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
        .map(|path| path.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    let resolve = |expected: &Path, command: &str| command_resolution(expected, command);
    match crate::pathman::get_path_state() {
        Ok(state) => {
            let entries = merged_path_entries(&state.system, &state.user);
            let launchers = launcher_statuses(expected, &entries, &command_extensions());
            path_integration_status_with(
                &state.user,
                &state.system,
                &current,
                expected,
                command,
                resolve,
                launchers,
            )
        }
        Err(error) => PathIntegrationStatus {
            expected: expected.to_string_lossy().into(),
            user_path_contains: false,
            new_terminal_contains: false,
            current_process_contains: current
                .iter()
                .any(|entry| crate::detect::system::paths_eq(Path::new(entry), expected)),
            status: "unknown".into(),
            detail: format!("无法读取注册表 PATH，不能确认新终端是否生效: {error}"),
            command: command.unwrap_or_default().into(),
            resolved_command: None,
            command_status: "unknown".into(),
            command_detail: "无法读取用户/系统 PATH，未声称命令已经生效".into(),
            launchers: Vec::new(),
        },
    }
}

fn tool_config_settings(
    tool: &str,
    global_path: Option<&str>,
    cache_path: Option<&str>,
    yarn_berry: bool,
) -> Result<Vec<(String, String)>> {
    if tool == "yarn" && yarn_berry && global_path.is_some() {
        return Err(AppError::msg(
            "Yarn Berry 不支持配置 Classic 全局安装目录；请仅配置适用的缓存目录",
        ));
    }

    let mut settings = Vec::new();
    if let Some(path) = global_path {
        match tool {
            "npm" => settings.push(("prefix".into(), path.into())),
            "pnpm" => {
                settings.push(("global-dir".into(), path.into()));
                settings.push((
                    "global-bin-dir".into(),
                    managers::bin_path(tool, Path::new(path))
                        .to_string_lossy()
                        .into(),
                ));
            }
            "yarn" => {
                settings.push(("global-folder".into(), path.into()));
                settings.push((
                    "prefix".into(),
                    managers::prefix_path(tool, Path::new(path))
                        .to_string_lossy()
                        .into(),
                ));
            }
            "pip" => {}
            _ => return Err(AppError::msg(format!("未知工具: {tool}"))),
        }
    }
    if let Some(path) = cache_path {
        let key = match tool {
            "npm" => "cache",
            "pnpm" => "store-dir",
            "yarn" if yarn_berry => "cacheFolder",
            "yarn" => "cache-folder",
            "pip" => "global.cache-dir",
            _ => return Err(AppError::msg(format!("未知工具: {tool}"))),
        };
        settings.push((key.into(), path.into()));
    }
    Ok(settings)
}

fn validate_tool_config_layout(
    tool: &str,
    global_path: Option<&str>,
    cache_path: Option<&str>,
) -> Result<()> {
    for path in [global_path, cache_path].into_iter().flatten() {
        let path_text = Path::new(path);
        if !path_text.is_absolute()
            || path_text
                .components()
                .any(|component| matches!(component, std::path::Component::ParentDir))
        {
            return Err(AppError::msg("配置目录必须是不含 .. 的绝对路径"));
        }
        // The value is passed as a structured argument, but it is also written
        // into manager configuration files. Characters that terminate or quote
        // a config value would change the meaning of that file, so refuse them
        // instead of silently escaping.
        if path.contains([';', '"', '\0', '\r', '\n']) {
            return Err(AppError::msg(
                "配置目录不能包含分号、引号、空字符或换行，否则会改变配置文件含义",
            ));
        }
    }
    if let Some(global) = global_path {
        let roles = managers::directory_roles(
            tool,
            Path::new(global),
            cache_path.map(Path::new),
        );
        managers::validate_directory_roles(tool, &roles)
    } else {
        managers::validate_layout(tool, None, None, cache_path.map(Path::new), None)
    }
}

/// One manager request for a batch preview; paths are validated later by the
/// same layout checks used for a single configuration.
#[derive(Clone, Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolConfigRequest {
    pub tool: String,
    pub global_path: Option<String>,
    pub cache_path: Option<String>,
}

/// One `key → value` change with its previous value, so a user can compare
/// before anything is written. Preview never creates directories or runs the
/// manager's `config set`.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ToolConfigChange {
    pub key: String,
    /// `global`, `command`, `cache`, or `configuration`.
    pub role: String,
    pub current: Option<String>,
    pub next: String,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ToolConfigPreview {
    pub tool: String,
    pub available: bool,
    /// Reasons the current request would be refused, reusing the write checks.
    pub blocked: Vec<String>,
    /// Non-fatal observations, such as an existing non-empty target.
    pub warnings: Vec<String>,
    pub changes: Vec<ToolConfigChange>,
    pub command_entry: String,
    pub command_entry_changed: bool,
    /// Read-only resolution of the target entry through the merged PATH.
    pub entry_in_path: bool,
    pub command_status: String,
    pub command_detail: String,
    /// Always false: paths are reconfigured, existing global packages stay put.
    pub moves_existing_packages: bool,
    pub notes: Vec<String>,
    /// Number of change rows whose value actually differs from the current one.
    #[serde(default)]
    pub changed_count: usize,
    /// Existing global packages and whether a command-entry change affects them.
    #[serde(default)]
    pub affected_packages: AffectedPackages,
}

/// Batch preview for "configure everything". Each requested tool is checked
/// independently so one blocked manager does not hide the others.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ToolConfigBatchPreview {
    pub items: Vec<ToolConfigPreview>,
    /// Tools that could be written in this batch.
    pub ready: Vec<String>,
    /// Tools with at least one blocking reason; they are skipped, not forced.
    pub blocked: Vec<String>,
    pub total_changes: usize,
    /// Always false: a batch reconfigures paths, it does not move packages.
    pub moves_existing_packages: bool,
}

/// One global package that exists in the current global root, used to show what
/// a path change would affect. Read-only: only names, versions and reasons.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AffectedPackage {
    pub name: String,
    pub version: Option<String>,
    /// `kept` when the package stays in the same global root, `unresolved`
    /// when the command entry change means its launcher may no longer resolve.
    pub status: String,
    /// Read-only reason, reusing the inventory's own diagnostics.
    pub detail: Option<String>,
}

/// Result of reading the current global root without executing the manager.
#[derive(Clone, Debug, Serialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct AffectedPackages {
    pub packages: Vec<AffectedPackage>,
    /// Number of existing packages whose launcher may stop resolving.
    pub unresolved_count: usize,
    /// Explain why the list is incomplete or empty; never claim "no packages".
    pub note: String,
}

/// Classify existing global packages against a path change. `entry_changed`
/// comes from the preview; when the command entry moves, every launcher in the
/// old entry directory may stop resolving even though package files are kept.
fn classify_affected_packages(
    packages: Vec<(String, Option<String>, Option<String>)>,
    entry_changed: bool,
) -> AffectedPackages {
    let total = packages.len();
    let items = packages
        .into_iter()
        .map(|(name, version, detail)| AffectedPackage {
            name,
            version,
            status: if entry_changed { "unresolved" } else { "kept" }.into(),
            detail,
        })
        .collect::<Vec<_>>();
    let unresolved_count = if entry_changed { total } else { 0 };
    let note = if total == 0 {
        "当前全局安装目录未发现可列出的包；这不代表该目录为空或未安装任何工具".into()
    } else if entry_changed {
        format!(
            "当前全局目录中的 {total} 个包仍保留在原位置，但命令入口将变化，旧入口可能不再解析到它们"
        )
    } else {
        format!("当前全局目录中的 {total} 个包保留在原位置，命令入口不变")
    };
    AffectedPackages {
        packages: items,
        unresolved_count,
        note,
    }
}

const MAX_AFFECTED_PACKAGES: usize = 50;

/// Read the current global root before changing paths. Only the existing global
/// directory is inspected; nothing is executed and nothing is written.
fn read_affected_packages(
    tool: &str,
    current_global: Option<&Path>,
    entry_changed: bool,
) -> AffectedPackages {
    let Some(current) = current_global.filter(|path| path.is_dir()) else {
        return AffectedPackages {
            packages: Vec::new(),
            unresolved_count: 0,
            note: "未找到当前全局安装目录，未列出受影响包".into(),
        };
    };
    let rows = match crate::packages::global_package_rows(tool, current) {
        Ok(rows) => rows,
        Err(error) => {
            return AffectedPackages {
                packages: Vec::new(),
                unresolved_count: 0,
                note: format!("无法读取当前全局安装目录：{error}"),
            }
        }
    };
    let mut rows = rows;
    rows.truncate(MAX_AFFECTED_PACKAGES);
    classify_affected_packages(rows, entry_changed)
}

fn role_for_key(tool: &str, key: &str) -> &'static str {    if managers::global_env(tool) == Some(key)
        || matches!((tool, key), ("npm", "prefix") | ("pnpm", "global-dir") | ("yarn", "global-folder"))
    {
        "global"
    } else if managers::bin_env(tool) == Some(key)
        || matches!((tool, key), ("pnpm", "global-bin-dir") | ("yarn", "prefix"))
    {
        "command"
    } else if managers::cache_env(tool) == Some(key)
        || matches!(
            (tool, key),
            ("npm", "cache")
                | ("pnpm", "store-dir")
                | ("yarn", "cache-folder")
                | ("yarn", "cacheFolder")
                | ("pip", "global.cache-dir")
        )
    {
        "cache"
    } else {
        "configuration"
    }
}

/// Pure diff builder so fixtures can compare arbitrary old/new manager values
/// without touching the registry or a real manager.
fn build_config_changes(
    tool: &str,
    current: &[(String, String)],
    next: &[(String, String)],
) -> Vec<ToolConfigChange> {
    next.iter()
        .map(|(key, value)| {
            let previous = current
                .iter()
                .find(|(name, _)| name.eq_ignore_ascii_case(key))
                .map(|(_, value)| value.clone());
            ToolConfigChange {
                key: key.clone(),
                role: role_for_key(tool, key).into(),
                // An unchanged value is still listed so the user sees the target.
                current: previous,
                next: value.clone(),
            }
        })
        .collect()
}

fn config_target_exists(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok()
}

fn config_target_non_empty(path: &Path) -> bool {
    std::fs::read_dir(path)
        .map(|mut entries| entries.next().is_some())
        .unwrap_or(false)
}

/// Read-only configuration diff. It applies the same validation and the same
/// target computation as `apply_tool_config`, but performs no write: no
/// directory is created and no `config set`/environment variable is executed.
pub async fn preview_tool_config(
    root: Option<&Path>,
    tool: &str,
    global_path: Option<&str>,
    cache_path: Option<&str>,
) -> Result<ToolConfigPreview> {
    let definition = managers::definition(tool)?;
    let mut blocked = Vec::new();
    let mut warnings = Vec::new();
    let mut notes = Vec::new();

    if global_path.is_some() && !definition.global_config
        || cache_path.is_some() && !definition.cache_config
    {
        return Err(AppError::msg("此管理器不支持对应路径设置"));
    }
    let inv = find_invocation(root, tool).await;
    let version_output = match &inv {
        Some(inv) => run(inv, &["--version"], 15).await.ok(),
        None => None,
    };
    let berry = tool == "yarn"
        && version_output
            .as_deref()
            .and_then(|value| display_version(value))
            .and_then(|value| value.split('.').next().map(str::to_owned))
            .and_then(|value| value.parse::<u32>().ok())
            .is_some_and(|major| major >= 2);

    if let Err(error) = validate_tool_config_layout(tool, global_path, cache_path) {
        blocked.push(error.to_string());
    }
    if inv.is_none() {
        blocked.push(format!("未找到 {tool}，请先安装或激活对应环境"));
    } else if version_output.is_none() {
        blocked.push(format!("{tool} 不可用，无法确认版本与当前配置"));
    }

    let (current_pairs, next_pairs) = if managers::isolated(tool) {
        let current = crate::pathman::get_user_env_vars().unwrap_or_default();
        let keys = [
            (managers::global_env(tool), "global"),
            (managers::bin_env(tool), "command"),
            (managers::cache_env(tool), "cache"),
        ];
        let current = keys
            .iter()
            .filter_map(|(key, _)| {
                let key = (*key)?;
                Some((key.to_string(), current.get(key).cloned().unwrap_or_default()))
            })
            .collect::<Vec<_>>();
        let next = managers::configuration(tool, global_path, cache_path)
            .map_err(|error| {
                blocked.push(error.to_string());
                error
            })
            .unwrap_or_default();
        (current, next)
    } else {
        let current = match &inv {
            Some(inv) => {
                let (global, command, cache, error) = values(inv, tool).await;
                let mut pairs = Vec::new();
                if let Some(value) = global {
                    pairs.push((
                        managers::global_env(tool).unwrap_or("prefix").to_string(),
                        value,
                    ));
                }
                if let Some(value) = command {
                    pairs.push(("command-entry".to_string(), value));
                }
                if let Some(value) = cache {
                    let key = match tool {
                        "pnpm" => "store-dir",
                        "yarn" => "cache-folder",
                        "pip" => "global.cache-dir",
                        _ => "cache",
                    };
                    pairs.push((key.to_string(), value));
                }
                if let Some(error) = error {
                    warnings.push(format!("当前配置读取不完整：{error}"));
                }
                pairs
            }
            None => Vec::new(),
        };
        let next = tool_config_settings(tool, global_path, cache_path, berry).map_err(|error| {
            blocked.push(error.to_string());
            error
        });
        (
            current,
            next.map(|settings| {
                let mut pairs = settings;
                if let Some(global) = global_path {
                    pairs.push((
                        "command-entry".into(),
                        managers::bin_path(tool, Path::new(global))
                            .to_string_lossy()
                            .into(),
                    ));
                }
                pairs
            })
            .unwrap_or_default(),
        )
    };

    let changes = build_config_changes(tool, &current_pairs, &next_pairs);

    let command_entry = global_path
        .map(|path| managers::bin_path(tool, Path::new(path)))
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_default();
    let current_entry = current_pairs
        .iter()
        .find(|(key, _)| role_for_key(tool, key) == "command")
        .map(|(_, value)| value.clone());
    let has_global_target = global_path.is_some();
    let command_entry_changed = match (&current_entry, has_global_target) {
        (Some(current), true) => {
            !crate::detect::system::paths_eq(Path::new(current), Path::new(&command_entry))
        }
        (None, true) => true,
        _ => false,
    };

    let (entry_in_path, command_status, command_detail) = if command_entry.is_empty() {
        (
            false,
            "unknown".to_string(),
            "未指定全局安装目录，不核对命令入口".to_string(),
        )
    } else {
        let status = path_integration_status(Path::new(&command_entry), Some(tool));
        (
            status.new_terminal_contains,
            status.command_status,
            status.command_detail,
        )
    };

    for path in [global_path, cache_path].into_iter().flatten() {
        let path = Path::new(path);
        if config_target_exists(path) && config_target_non_empty(path) {
            warnings.push(format!(
                "目标目录已存在且非空：{}；现有内容不会被合并、搬移或删除",
                path.display()
            ));
        }
    }
    if command_status == "shadowed" {
        warnings.push(command_detail.clone());
    }
    if tool == "yarn" && berry && global_path.is_some() {
        blocked.push(
            "Yarn 2+ 不支持配置 Classic 全局安装目录；请仅配置适用的缓存目录".into(),
        );
    }

    notes.push("仅修改包管理器配置；已安装的全局包保留在原位置，不会搬移".into());
    notes.push("写入前会备份现有配置/环境变量，并保留最近若干份记录".into());
    if command_entry_changed && !command_entry.is_empty() {
        notes.push(format!(
            "命令入口将变化，原有入口可能需要手动清理；新入口 {} 需在新终端核对命令",
            command_entry
        ));
    }

    let changed_count = changes
        .iter()
        .filter(|change| {
            change
                .current
                .as_deref()
                .is_none_or(|current| current != change.next)
        })
        .count();
    // Read the existing global root before the change so the user can see which
    // packages keep their files but may lose their launcher.
    let current_global = current_pairs
        .iter()
        .find(|(key, _)| role_for_key(tool, key) == "global")
        .map(|(_, value)| PathBuf::from(value));
    let affected_packages = read_affected_packages(
        tool,
        current_global.as_deref(),
        command_entry_changed,
    );
    // A blocked tool is not written at all, so it must not advertise planned
    // changes or inflate a batch total.
    let (changes, changed_count) = if blocked.is_empty() {
        (changes, changed_count)
    } else {
        (Vec::new(), 0)
    };

    Ok(ToolConfigPreview {
        tool: tool.into(),
        available: inv.is_some(),
        blocked,
        warnings,
        changed_count,
        changes,
        command_entry,
        command_entry_changed,
        entry_in_path,
        command_status,
        command_detail,
        moves_existing_packages: false,
        notes,
        affected_packages,
    })
}

/// Read-only batch preview. Every tool is evaluated with the same single-tool
/// checks; a blocked tool is listed and excluded from `ready` instead of being
/// silently written later.
pub async fn preview_tool_configs(
    root: Option<&Path>,
    requests: &[(String, Option<String>, Option<String>)],
) -> ToolConfigBatchPreview {
    let mut items = Vec::new();
    let mut ready = Vec::new();
    let mut blocked = Vec::new();
    let mut total_changes = 0;
    for (tool, global, cache) in requests {
        match preview_tool_config(root, tool, global.as_deref(), cache.as_deref()).await {
            Ok(preview) => {
                if preview.blocked.is_empty() {
                    ready.push(tool.clone());
                } else {
                    blocked.push(tool.clone());
                }
                total_changes += preview.changed_count;
                items.push(preview);
            }
            Err(error) => {
                blocked.push(tool.clone());
                items.push(ToolConfigPreview {
                    tool: tool.clone(),
                    available: false,
                    blocked: vec![error.to_string()],
                    warnings: Vec::new(),
                    changes: Vec::new(),
                    command_entry: String::new(),
                    command_entry_changed: false,
                    entry_in_path: false,
                    command_status: "unknown".into(),
                    command_detail: "预览失败，未核对命令入口".into(),
                    moves_existing_packages: false,
                    notes: Vec::new(),
                    changed_count: 0,
                    affected_packages: AffectedPackages {
                        packages: Vec::new(),
                        unresolved_count: 0,
                        note: "预览失败，未读取当前全局包".into(),
                    },
                });
            }
        }
    }
    ToolConfigBatchPreview {
        items,
        ready,
        blocked,
        total_changes,
        moves_existing_packages: false,
    }
}

pub async fn apply_tool_config(
    root: Option<&Path>,
    tool: &str,
    global_path: Option<&str>,
    cache_path: Option<&str>,
    backups: &Path,
) -> Result<Vec<String>> {
    let definition = managers::definition(tool)?;
    if global_path.is_some() && !definition.global_config
        || cache_path.is_some() && !definition.cache_config
    {
        return Err(AppError::msg("此管理器不支持对应路径设置"));
    }
    validate_tool_config_layout(tool, global_path, cache_path)?;
    let inv = find_invocation(root, tool)
        .await
        .ok_or_else(|| AppError::msg(format!("未找到 {tool}，请先安装或激活对应环境")))?;
    let version_output = run(&inv, &["--version"], 15)
        .await
        .map_err(|e| AppError::msg(format!("{tool} 不可用: {e}")))?;
    let berry = tool == "yarn"
        && version_output
            .split('.')
            .next()
            .and_then(|v| v.parse::<u32>().ok())
            .is_some_and(|v| v >= 2);
    let settings = if managers::isolated(tool) {
        Vec::new()
    } else {
        tool_config_settings(tool, global_path, cache_path, berry)?
    };
    let tool_root = global_path.map(Path::new);
    let command_root = global_path.map(|path| managers::bin_path(tool, Path::new(path)));
    for path in [tool_root, command_root.as_deref(), cache_path.map(Path::new)]
        .into_iter()
        .flatten()
    {
        std::fs::create_dir_all(path)?;
    }
    if managers::isolated(tool) {
        let changes = managers::configuration(tool, global_path, cache_path)?;
        let mut applied = Vec::new();
        for (name, value) in changes {
            crate::pathman::set_user_env_var(&name, &value, backups)
                .map_err(|e| AppError::msg(format!("{e}；已完成: {}", applied.join("；"))))?;
            applied.push(format!("{name} → {value}"));
        }
        return Ok(applied);
    }
    let mut applied = Vec::new();
    for (key, value) in settings {
        set(&inv, &["config", "set", &key, &value])
            .await
            .map_err(|e| {
                AppError::msg(format!(
                    "{e}；已完成: {}",
                    if applied.is_empty() {
                        "无".into()
                    } else {
                        applied.join("；")
                    }
                ))
            })?;
        if Some(key.as_str())
            == match tool {
                "npm" => Some("cache"),
                "pnpm" => Some("store-dir"),
                "yarn" if berry => Some("cacheFolder"),
                "yarn" => Some("cache-folder"),
                "pip" => Some("global.cache-dir"),
                _ => None,
            }
        {
            applied.push(format!("缓存/存储目录 → {value}"));
        } else {
            applied.push(format!("{key} → {value}"));
        }
    }
    Ok(applied)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_display_uses_cli_version_not_install_path_or_date() {
        assert_eq!(display_version("pip 26.0 from C:\\Python\\Lib (python 3.15)").as_deref(), Some("26.0"));
        assert_eq!(display_version("Composer version 2.8.5 2025-02-01").as_deref(), Some("2.8.5"));
        assert_eq!(display_version("v12.1.0").as_deref(), Some("12.1.0"));
        assert_eq!(display_version("unknown version"), None);
    }

    #[test]
    fn self_update_targets_original_installation() {
        let dir = crate::test_support::TestDir::new();
        let inv = Invocation {
            program: dir.path().join("npm.cmd").to_string_lossy().into(),
            prefix: vec![],
        };
        assert!(self_update_command("npm", &inv).is_err());
        std::fs::create_dir_all(dir.path().join("node_modules/npm")).unwrap();
        std::fs::write(dir.path().join("node_modules/npm/package.json"), "{}").unwrap();
        std::fs::write(&inv.program, "").unwrap();
        let (_, args) = self_update_command("npm", &inv).unwrap();
        assert_eq!(args[3], "--prefix");
        assert!(crate::detect::system::paths_eq(
            Path::new(&args[4]),
            dir.path()
        ));
        std::fs::create_dir(dir.path().join("Scripts")).unwrap();
        std::fs::write(dir.path().join("python.exe"), "").unwrap();
        let pip = Invocation {
            program: dir.path().join("Scripts/pip.exe").to_string_lossy().into(),
            prefix: vec![],
        };
        let (updater, _) = self_update_command("pip", &pip).unwrap();
        assert_eq!(updater.prefix, ["-m", "pip"]);
        assert_eq!(Path::new(&updater.program), dir.path().join("python.exe"));
        assert!(self_update_command("pipx", &inv).is_err());
    }

    #[test]
    fn corepack_and_pip_installed_managers_refuse_self_update_with_a_specific_reason() {
        let dir = crate::test_support::TestDir::new();

        // A Corepack shim names corepack in its body; its version is pinned by
        // Corepack, so the manager's own self-update must not run.
        let shim = dir.path().join("pnpm.cmd");
        std::fs::write(&shim, "@echo off\r\ncorepack pnpm %*\r\n").unwrap();
        let corepack = Invocation {
            program: shim.to_string_lossy().into(),
            prefix: vec![],
        };
        assert_eq!(update_channel("pnpm", &corepack), UpdateChannel::Corepack);
        let error = self_update_command("pnpm", &corepack).unwrap_err().to_string();
        assert!(error.contains("Corepack"), "unexpected error: {error}");

        // A real pnpm bundle is not a shim, so the normal self-update applies.
        let standalone = dir.path().join("standalone/pnpm.cmd");
        std::fs::create_dir_all(standalone.parent().unwrap()).unwrap();
        std::fs::write(&standalone, "@echo off\r\nnode \"%~dp0pnpm.cjs\" %*\r\n").unwrap();
        let standalone = Invocation {
            program: standalone.to_string_lossy().into(),
            prefix: vec![],
        };
        assert_eq!(update_channel("pnpm", &standalone), UpdateChannel::Standalone);
        assert_eq!(
            self_update_command("pnpm", &standalone).unwrap().1,
            vec!["self-update".to_string()]
        );

        // An unreadable shim is not evidence of Corepack: refuse to guess.
        assert_eq!(
            update_channel(
                "pnpm",
                &Invocation {
                    program: dir.path().join("missing/pnpm.cmd").to_string_lossy().into(),
                    prefix: vec![],
                }
            ),
            UpdateChannel::Standalone
        );

        // A pip-installed uv sits beside python.exe and pip.exe.
        let scripts = dir.path().join("Python/Scripts");
        std::fs::create_dir_all(&scripts).unwrap();
        std::fs::write(dir.path().join("Python/python.exe"), "").unwrap();
        std::fs::write(scripts.join("pip.exe"), "").unwrap();
        let uv_exe = scripts.join("uv.exe");
        std::fs::write(&uv_exe, "").unwrap();
        let pip_uv = Invocation {
            program: uv_exe.to_string_lossy().into(),
            prefix: vec![],
        };
        assert_eq!(update_channel("uv", &pip_uv), UpdateChannel::PythonPackage);
        let error = self_update_command("uv", &pip_uv).unwrap_err().to_string();
        assert!(
            error.contains("pip install --upgrade uv"),
            "unexpected error: {error}"
        );

        // A standalone uv (no sibling interpreter) keeps its own self update.
        let lone = dir.path().join("uv-bin/uv.exe");
        std::fs::create_dir_all(lone.parent().unwrap()).unwrap();
        std::fs::write(&lone, "").unwrap();
        let standalone_uv = Invocation {
            program: lone.to_string_lossy().into(),
            prefix: vec![],
        };
        assert_eq!(update_channel("uv", &standalone_uv), UpdateChannel::Standalone);
        assert_eq!(
            self_update_command("uv", &standalone_uv).unwrap().1,
            vec!["self".to_string(), "update".to_string()]
        );
    }

    /// The tool list, the preview and the executor must never disagree about
    /// the same installation: all three read this one function, so the reason a
    /// button is disabled is byte-identical to the reason an update would fail.
    #[test]
    fn list_preview_and_executor_share_one_self_update_blocker() {
        let dir = crate::test_support::TestDir::new();

        let shim = dir.path().join("pnpm.cmd");
        std::fs::write(&shim, "@echo off\r\ncorepack pnpm %*\r\n").unwrap();
        let corepack = Invocation {
            program: shim.to_string_lossy().into(),
            prefix: vec![],
        };
        let listed = self_update_channel_blocker("pnpm", &corepack).expect("shim must be blocked");
        let executed = self_update_command("pnpm", &corepack).unwrap_err().to_string();
        assert_eq!(
            listed, executed,
            "the list reason must match the executor reason exactly"
        );

        let scripts = dir.path().join("Python/Scripts");
        std::fs::create_dir_all(&scripts).unwrap();
        std::fs::write(dir.path().join("Python/python.exe"), "").unwrap();
        std::fs::write(scripts.join("pip.exe"), "").unwrap();
        let uv_exe = scripts.join("uv.exe");
        std::fs::write(&uv_exe, "").unwrap();
        let pip_uv = Invocation {
            program: uv_exe.to_string_lossy().into(),
            prefix: vec![],
        };
        let listed = self_update_channel_blocker("uv", &pip_uv).expect("pip uv must be blocked");
        let executed = self_update_command("uv", &pip_uv).unwrap_err().to_string();
        assert_eq!(listed, executed);

        // A standalone installation has no blocker, so the list must not invent
        // one and the button stays enabled.
        let lone = dir.path().join("uv-bin/uv.exe");
        std::fs::create_dir_all(lone.parent().unwrap()).unwrap();
        std::fs::write(&lone, "").unwrap();
        assert_eq!(
            self_update_channel_blocker(
                "uv",
                &Invocation {
                    program: lone.to_string_lossy().into(),
                    prefix: vec![],
                }
            ),
            None
        );
    }

    /// Guards a deliberate *absence* of logic: composer and bun have no
    /// installation-shape check because no reliable evidence exists, and this
    /// fixture exists so a later "helpful" heuristic cannot be added silently.
    #[test]
    fn composer_and_bun_stay_standalone_because_no_reliable_evidence_exists() {
        let dir = crate::test_support::TestDir::new();

        // A composer launcher that wraps composer.phar proves it *is* a phar
        // install; it never proves another installer did not produce it.
        let composer = dir.path().join("composer/composer.bat");
        std::fs::create_dir_all(composer.parent().unwrap()).unwrap();
        std::fs::write(&composer, "@php \"%~dp0composer.phar\" %*\r\n").unwrap();
        let composer_inv = Invocation {
            program: composer.to_string_lossy().into(),
            prefix: vec![],
        };
        assert_eq!(
            update_channel("composer", &composer_inv),
            UpdateChannel::Standalone
        );
        assert_eq!(self_update_channel_blocker("composer", &composer_inv), None);
        assert_eq!(
            self_update_command("composer", &composer_inv).unwrap().1,
            vec!["self-update".to_string()]
        );

        // Every Windows bun flavour ships the same static bun.exe, so a
        // side-by-side scoop or winget shim outside the install directory is
        // invisible here. Guessing would disable a working update.
        let bun_dir = dir.path().join("bun");
        std::fs::create_dir_all(&bun_dir).unwrap();
        // Even with interpreter-looking files nearby, bun must not be judged.
        std::fs::write(bun_dir.join("python.exe"), "").unwrap();
        std::fs::write(bun_dir.join("pip.exe"), "").unwrap();
        let bun = bun_dir.join("bun.exe");
        std::fs::write(&bun, "").unwrap();
        let bun_inv = Invocation {
            program: bun.to_string_lossy().into(),
            prefix: vec![],
        };
        assert_eq!(update_channel("bun", &bun_inv), UpdateChannel::Standalone);
        assert_eq!(self_update_channel_blocker("bun", &bun_inv), None);
        assert_eq!(
            self_update_command("bun", &bun_inv).unwrap().1,
            vec!["upgrade".to_string()]
        );
    }

    /// The limits must be structured facts, not prose: the entry point installs
    /// `latest` for every manager, but what the manager itself can pin differs.
    /// A `None` here means "no verified one-liner", never "impossible".
    #[test]
    fn update_capabilities_state_the_entry_point_limit_and_real_pin_commands() {
        // Every manager shares the same entry-point limit.
        for definition in crate::package_managers::definitions() {
            assert!(
                update_capabilities(&definition.id).latest_only,
                "{} must report the latest-only entry point",
                definition.id
            );
        }

        // Managers with a documented version-pinned command expose one, and the
        // text is a template the user can edit rather than a guessed value.
        for (tool, expected) in [
            ("npm", "npm install --global npm@<version>"),
            ("pnpm", "pnpm add --global pnpm@<version>"),
            ("pip", "python -m pip install pip==<version>"),
            ("composer", "composer self-update <version>"),
            ("uv", "uv self update <version>"),
        ] {
            assert_eq!(
                update_capabilities(tool).manual_pinned_command.as_deref(),
                Some(expected),
                "{tool} pinned command"
            );
        }

        // Managers without a verified pin command report none rather than
        // inventing one: yarn pins per project, bun documents none, and the
        // remaining three have no general self-update at all.
        for tool in ["yarn", "bun", "pipx", "cargo", "dotnet"] {
            assert_eq!(
                update_capabilities(tool).manual_pinned_command,
                None,
                "{tool} must not invent a pin command"
            );
        }
    }

    #[test]
    fn tool_config_exposes_the_runtime_blocker_over_ipc() {
        // The field must reach the UI under its camelCase name, or the button
        // would silently keep looking only at the static catalog.
        let config = ToolConfig {
            tool: "pnpm".into(),
            name: "pnpm".into(),
            available: true,
            version: Some("9.0.0".into()),
            executable_path: Some("D:/node/pnpm.cmd".into()),
            error: None,
            global_path: None,
            cache_path: None,
            default_global: None,
            default_cache: None,
            command_path: None,
            command_path_source: None,
            layout_warning: None,
            self_update_blocker: Some("由 Corepack 管理".into()),
        };
        let json = serde_json::to_value(&config).unwrap();
        assert_eq!(
            json["selfUpdateBlocker"].as_str(),
            Some("由 Corepack 管理")
        );
        // Absent in older snapshots: the field defaults rather than failing.
        let without: ToolConfig = serde_json::from_value(serde_json::json!({
            "tool": "npm", "name": "npm", "available": true,
            "executablePath": null, "error": null, "globalPath": null,
            "cachePath": null, "defaultGlobal": null, "defaultCache": null,
            "commandPath": null, "layoutWarning": null
        }))
        .unwrap();
        assert_eq!(without.self_update_blocker, None);
    }

    #[test]
    fn global_directory_settings_keep_manager_roles_explicit() {
        assert_eq!(
            tool_config_settings(
                "npm",
                Some("D:/npm-global"),
                Some("D:/npm-cache"),
                false
            )
            .unwrap(),
            vec![
                ("prefix".into(), "D:/npm-global".into()),
                ("cache".into(), "D:/npm-cache".into())
            ]
        );
        assert_eq!(
            tool_config_settings(
                "pnpm",
                Some("D:/pnpm-global"),
                Some("D:/pnpm-store"),
                false
            )
            .unwrap(),
            vec![
                ("global-dir".into(), "D:/pnpm-global".into()),
                (
                    "global-bin-dir".into(),
                    managers::bin_path("pnpm", Path::new("D:/pnpm-global"))
                        .to_string_lossy()
                        .into()
                ),
                ("store-dir".into(), "D:/pnpm-store".into())
            ]
        );
        assert_eq!(
            tool_config_settings(
                "yarn",
                Some("D:/yarn-global"),
                Some("D:/yarn-cache"),
                false
            )
            .unwrap(),
            vec![
                ("global-folder".into(), "D:/yarn-global".into()),
                (
                    "prefix".into(),
                    managers::prefix_path("yarn", Path::new("D:/yarn-global"))
                        .to_string_lossy()
                        .into()
                ),
                ("cache-folder".into(), "D:/yarn-cache".into())
            ]
        );
        assert_eq!(
            tool_config_settings("yarn", None, Some("D:/berry-cache"), true).unwrap(),
            vec![("cacheFolder".into(), "D:/berry-cache".into())]
        );
        assert!(tool_config_settings("yarn", Some("D:/global"), None, true).is_err());
    }

    #[test]
    fn configuration_write_layout_rejects_config_cache_overlap_before_side_effects() {
        let dir = crate::test_support::TestDir::new();
        let root = dir.path().join("npm-global");
        let cache = dir.path().join("npm-cache");
        let config = crate::package_managers::config_path("npm", &root);
        std::fs::create_dir_all(&root).unwrap();

        assert!(validate_tool_config_layout(
            "npm",
            Some(&root.to_string_lossy()),
            Some(&config.to_string_lossy()),
        )
        .is_err());
        assert!(!config.exists());
        assert!(validate_tool_config_layout(
            "npm",
            Some(&root.to_string_lossy()),
            Some(&cache.to_string_lossy()),
        )
        .is_ok());
    }

    #[test]
    fn cache_only_configuration_checks_absolute_paths() {
        let dir = crate::test_support::TestDir::new();
        let cache = dir.path().join("cache");
        assert!(validate_tool_config_layout("pip", None, Some(&cache.to_string_lossy())).is_ok());
        assert!(validate_tool_config_layout("pip", None, Some("relative/cache")).is_err());
    }

    #[tokio::test]
    async fn silent_config_writes_succeed_but_queries_still_require_values() {
        let dir = crate::test_support::TestDir::new();
        let script = dir.path().join("manager.cmd");
        std::fs::write(&script, "@echo off\r\nif \"%1\"==\"fail\" (\r\n echo fixture error 1>&2\r\n exit /b 7\r\n)\r\nif \"%1\"==\"get\" echo D:\\fixture\\global\r\nif \"%1\"==\"unset\" echo undefined\r\nexit /b 0\r\n").unwrap();
        let inv = Invocation {
            program: script.to_string_lossy().into(),
            prefix: vec![],
        };
        set(
            &inv,
            &["config", "set", "global-dir", "D:\\fixture\\global"],
        )
        .await
        .unwrap();
        set(&inv, &["config", "set", "cache-dir", "D:\\fixture\\cache"])
            .await
            .unwrap();
        assert!(run(&inv, &["silent"], 5).await.is_err());
        assert!(run(&inv, &["unset"], 5).await.is_err());
        assert_eq!(run(&inv, &["get"], 5).await.unwrap(), r"D:\fixture\global");
        let error = set(&inv, &["fail"]).await.unwrap_err().to_string();
        assert!(
            error.contains("7") && error.contains("fixture error"),
            "{error}"
        );
    }

    #[tokio::test]
    async fn values_include_actual_command_entries_for_pnpm_yarn_and_uv() {
        let dir = crate::test_support::TestDir::new();
        let script = dir.path().join("manager.cmd");
        std::fs::write(
            &script,
            "@echo off\r\n\
             if \"%1\"==\"config\" if \"%2\"==\"get\" if \"%3\"==\"global-dir\" echo D:\\pnpm-global\r\n\
             if \"%1\"==\"config\" if \"%2\"==\"get\" if \"%3\"==\"global-bin-dir\" echo D:\\pnpm-bin\r\n\
             if \"%1\"==\"store\" if \"%2\"==\"path\" echo D:\\pnpm-store\r\n\
             if \"%1\"==\"global\" if \"%2\"==\"dir\" echo D:\\yarn-global\r\n\
             if \"%1\"==\"global\" if \"%2\"==\"bin\" echo D:\\yarn-bin\r\n\
             if \"%1\"==\"cache\" if \"%2\"==\"dir\" echo D:\\yarn-cache\r\n\
             if \"%1\"==\"tool\" if \"%2\"==\"dir\" if \"%3\"==\"--bin\" echo D:\\uv-bin\r\n\
             if \"%1\"==\"tool\" if \"%2\"==\"dir\" echo D:\\uv-tools\r\n\
             if \"%1\"==\"cache\" if \"%2\"==\"dir\" echo D:\\uv-cache\r\n\
             exit /b 0\r\n",
        )
        .unwrap();
        let inv = Invocation {
            program: script.to_string_lossy().into(),
            prefix: vec![],
        };
        let (global, command, cache, error) = values(&inv, "pnpm").await;
        assert_eq!(global.as_deref(), Some(r"D:\pnpm-global"));
        assert_eq!(command.as_deref(), Some(r"D:\pnpm-bin"));
        assert_eq!(cache.as_deref(), Some(r"D:\pnpm-store"));
        assert!(error.is_none());

        let (_, command, _, _) = values(&inv, "yarn").await;
        assert_eq!(command.as_deref(), Some(r"D:\yarn-bin"));
        let (_, command, _, _) = values(&inv, "uv").await;
        assert_eq!(command.as_deref(), Some(r"D:\uv-bin"));
    }

    #[test]
    fn command_entry_source_distinguishes_queried_and_layout_fallback() {
        let (command, source) =
            command_path_with_source(Some(r"D:\queried-bin".into()), Some(r"D:\layout-bin".into()));
        assert_eq!(command.as_deref(), Some(r"D:\queried-bin"));
        assert_eq!(source.as_deref(), Some("queried"));

        let (command, source) =
            command_path_with_source(None, Some(r"D:\layout-bin".into()));
        assert_eq!(command.as_deref(), Some(r"D:\layout-bin"));
        assert_eq!(source.as_deref(), Some("layout"));

        let (command, source) = command_path_with_source(None, None);
        assert!(command.is_none());
        assert!(source.is_none());
    }

    #[test]
    fn path_integration_distinguishes_registry_and_current_process() {
        let expected = Path::new(r"D:\managed\uv-bin");
        let user = vec![r"D:\managed\uv-bin\".to_string()];
        let missing = path_integration_status_from(&[], &[], &[], expected);
        assert_eq!(missing.status, "missing");
        assert!(!missing.new_terminal_contains);

        let new_terminal = path_integration_status_from(&user, &[], &[], expected);
        assert_eq!(new_terminal.status, "requires-new-terminal");
        assert!(new_terminal.user_path_contains);

        let active = path_integration_status_from(&user, &[], &user, expected);
        assert_eq!(active.status, "active");

        let current_only =
            path_integration_status_from(&[], &[], &[r"d:/managed/UV-BIN".to_string()], expected);
        assert_eq!(current_only.status, "current-process-only");
        assert!(current_only.current_process_contains);

        let duplicate = path_integration_status_from(
            &[r"D:\managed\uv-bin".into(), r"D:/managed/uv-bin/".into()],
            &[],
            &[],
            expected,
        );
        assert_eq!(duplicate.status, "duplicate");

        let system = path_integration_status_from(&[], &user, &[], expected);
        assert_eq!(system.status, "requires-new-terminal");
        assert!(system.new_terminal_contains);
    }

    #[test]
    fn command_resolution_detects_shadowed_managed_entry_and_missing_command() {
        use std::collections::HashSet;
        let existing: HashSet<String> = [
            r"c:\users\me\appdata\roaming\npm\pnpm.cmd",
            r"d:\managed\pnpm-bin\pnpm.cmd",
            r"d:\managed\pnpm-bin\pnpx.cmd",
        ]
        .iter()
        .map(|path| path.to_ascii_lowercase())
        .collect();
        // Windows file names compare case-insensitively; PATHEXT is upper-case here.
        let exists = |path: &Path| existing.contains(&path.to_string_lossy().to_ascii_lowercase());
        let extensions = vec![".EXE".into(), ".CMD".into()];
        let expected = Path::new(r"D:\managed\pnpm-bin");

        // A user-level entry earlier in PATH wins even though the managed
        // directory is also present: appending is not the same as effective.
        let entries = vec![
            r"C:\Users\me\AppData\Roaming\npm".to_string(),
            r"D:\managed\pnpm-bin".to_string(),
        ];
        let shadowed =
            resolve_command_in(&entries, expected, "pnpm", &extensions, exists);
        assert_eq!(shadowed.status, "shadowed");
        assert_eq!(
            shadowed
                .resolved
                .as_deref()
                .map(str::to_ascii_lowercase)
                .as_deref(),
            Some(r"c:\users\me\appdata\roaming\npm\pnpm.cmd")
        );
        assert!(shadowed.detail.contains("旧版本仍可能生效"));

        // Once the managed directory comes first, resolution reports managed.
        let entries = vec![
            r"D:\managed\pnpm-bin".to_string(),
            r"C:\Users\me\AppData\Roaming\npm".to_string(),
        ];
        let managed = resolve_command_in(&entries, expected, "pnpm", &extensions, exists);
        assert_eq!(managed.status, "managed");

        let missing = resolve_command_in(&entries, expected, "yarn", &extensions, exists);
        assert_eq!(missing.status, "missing");
        assert!(missing.resolved.is_none());

        // Literal file names must not gain PATHEXT suffixes.
        let literal = resolve_command_in(&entries, expected, "pnpm.cmd", &extensions, exists);
        assert_eq!(literal.status, "managed");
    }

    #[test]
    fn launcher_statuses_report_each_entry_and_skip_shadowed_ones() {
        let dir = crate::test_support::TestDir::new();
        let managed = dir.path().join("managed-bin");
        let older = dir.path().join("older-bin");
        std::fs::create_dir_all(&managed).unwrap();
        std::fs::create_dir_all(&older).unwrap();
        std::fs::write(managed.join("pnpm.cmd"), b"@echo off").unwrap();
        std::fs::write(managed.join("pnpx.cmd"), b"@echo off").unwrap();
        std::fs::write(managed.join("readme.txt"), b"not a launcher").unwrap();
        std::fs::write(older.join("pnpm.cmd"), b"@echo off").unwrap();
        let extensions = vec![".EXE".into(), ".CMD".into()];

        let entries = vec![
            older.to_string_lossy().into_owned(),
            managed.to_string_lossy().into_owned(),
        ];
        let statuses = launcher_statuses(&managed, &entries, &extensions);
        let names: Vec<_> = statuses.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["pnpm", "pnpx"], "only launchers are listed");
        let pnpm = statuses.iter().find(|s| s.name == "pnpm").unwrap();
        assert_eq!(pnpm.status, "shadowed");
        let pnpx = statuses.iter().find(|s| s.name == "pnpx").unwrap();
        assert_eq!(pnpx.status, "managed");
    }

    #[test]
    fn registry_path_order_and_variable_expansion_match_a_new_terminal() {        let system = vec![r"%SystemRoot%\system32".to_string()];
        let user = vec![r"D:\managed\pnpm-bin\".to_string()];
        let merged = merged_path_entries(&system, &user);
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[1], r"D:\managed\pnpm-bin\", "user entries keep their text");
        assert!(
            !merged[0].contains('%'),
            "known variables are expanded: {}",
            merged[0]
        );

        let unknown = merged_path_entries(&[r"%ENVCON_UNSET_VAR%\bin".into()], &[]);
        assert_eq!(unknown[0], r"%ENVCON_UNSET_VAR%\bin");

        let quoted = merged_path_entries(&[], &[r#""D:\quoted dir""#.into()]);
        assert_eq!(quoted, vec![r"D:\quoted dir"]);
    }

    #[test]
    fn config_preview_lists_old_and_new_values_without_writing() {
        let current = vec![
            ("UV_TOOL_DIR".to_string(), r"D:\old\uv-tools".to_string()),
            ("UV_CACHE_DIR".to_string(), r"D:\old\uv-cache".to_string()),
        ];
        let next = vec![
            ("UV_TOOL_DIR".to_string(), r"D:\new\uv-tools".to_string()),
            ("UV_TOOL_BIN_DIR".to_string(), r"D:\new\uv-bin".to_string()),
            ("UV_CACHE_DIR".to_string(), r"D:\old\uv-cache".to_string()),
        ];
        let changes = build_config_changes("uv", &current, &next);
        assert_eq!(changes.len(), 3);
        assert_eq!(changes[0].role, "global");
        assert_eq!(changes[0].current.as_deref(), Some(r"D:\old\uv-tools"));
        assert_eq!(changes[0].next, r"D:\new\uv-tools");
        assert_eq!(changes[1].role, "command");
        assert!(changes[1].current.is_none(), "a new entry has no old value");
        // An unchanged value is still shown so the target stays visible.
        assert_eq!(changes[2].role, "cache");
        assert_eq!(changes[2].current.as_deref(), Some(r"D:\old\uv-cache"));
        assert_eq!(changes[2].next, r"D:\old\uv-cache");
    }

    #[test]
    fn config_preview_classifies_every_manager_key() {
        assert_eq!(role_for_key("npm", "prefix"), "global");
        assert_eq!(role_for_key("npm", "cache"), "cache");
        assert_eq!(role_for_key("pnpm", "global-dir"), "global");
        assert_eq!(role_for_key("pnpm", "global-bin-dir"), "command");
        assert_eq!(role_for_key("pnpm", "store-dir"), "cache");
        assert_eq!(role_for_key("yarn", "global-folder"), "global");
        assert_eq!(role_for_key("yarn", "prefix"), "command");
        assert_eq!(role_for_key("yarn", "cache-folder"), "cache");
        assert_eq!(role_for_key("yarn", "cacheFolder"), "cache");
        assert_eq!(role_for_key("pip", "global.cache-dir"), "cache");
        assert_eq!(role_for_key("uv", "UV_TOOL_DIR"), "global");
        assert_eq!(role_for_key("uv", "UV_TOOL_BIN_DIR"), "command");
        assert_eq!(role_for_key("uv", "UV_CACHE_DIR"), "cache");
        assert_eq!(role_for_key("pipx", "PIPX_HOME"), "global");
        assert_eq!(role_for_key("pipx", "PIPX_BIN_DIR"), "command");
        assert_eq!(role_for_key("bun", "BUN_INSTALL_GLOBAL_DIR"), "global");
        assert_eq!(role_for_key("bun", "BUN_INSTALL_BIN"), "command");
        assert_eq!(role_for_key("cargo", "CARGO_INSTALL_ROOT"), "global");
        assert_eq!(role_for_key("composer", "COMPOSER_HOME"), "global");
        assert_eq!(role_for_key("unexpected", "whatever"), "configuration");
    }

    #[test]
    fn config_preview_refuses_invalid_layout_without_creating_directories() {
        let dir = crate::test_support::TestDir::new();
        let tool_root = dir.path().join("managed/uv-global");
        let nested_bin = tool_root.join("bin");
        // uv must not place its command entry inside the tool environment root;
        // the declared layout puts launchers in a sibling directory instead.
        let roles = managers::directory_roles("uv", &tool_root, None);
        assert_eq!(roles.command_entry, dir.path().join("managed/uv-global-bin"));
        assert!(!crate::detect::system::path_within(&tool_root, &roles.command_entry));
        assert!(validate_tool_config_layout(
            "uv",
            Some(&tool_root.to_string_lossy()),
            None
        )
        .is_ok());
        // Validation is read-only: no target directory is created.
        assert!(!tool_root.exists());
        assert!(!nested_bin.exists());
        assert!(!roles.command_entry.exists());

        // The write path's own role check still rejects a nested command entry.
        let nested = managers::DirectoryRoles {
            tool_environment: tool_root.clone(),
            command_entry: nested_bin.clone(),
            cache_storage: roles.cache_storage.clone(),
            configuration: roles.configuration.clone(),
        };
        assert!(managers::validate_directory_roles("uv", &nested).is_err());
        assert!(!nested_bin.exists());
    }

    #[test]
    fn config_preview_counts_only_existing_non_empty_targets() {
        let dir = crate::test_support::TestDir::new();
        let empty = dir.path().join("empty-target");
        let filled = dir.path().join("filled-target");
        std::fs::create_dir_all(&empty).unwrap();
        std::fs::create_dir_all(&filled).unwrap();
        std::fs::write(filled.join("keep.txt"), b"keep").unwrap();
        let missing = dir.path().join("missing-target");

        assert!(config_target_exists(&empty));
        assert!(!config_target_non_empty(&empty));
        assert!(config_target_non_empty(&filled));
        assert!(!config_target_exists(&missing));
        // Preview inspection must never create the missing target.
        assert!(!missing.exists());
        assert_eq!(std::fs::read(filled.join("keep.txt")).unwrap(), b"keep");
    }

    #[test]
    fn config_paths_cannot_break_out_of_manager_config_files() {
        let base = r"D:\managed\globals";
        // A normal absolute path still passes.
        assert!(validate_tool_config_layout("npm", Some(base), None).is_ok());
        for injected in [
            r"D:\managed\globals;prefix=evil",
            "D:\\managed\\globals\"evil",
            "D:\\managed\\globals\nevil",
            "D:\\managed\\globals\revil",
            "D:\\managed\\globals\0evil",
        ] {
            assert!(
                validate_tool_config_layout("npm", Some(injected), None).is_err(),
                "accepted injected path: {injected:?}"
            );
        }
        // Relative and parent-relative paths stay rejected as before.
        assert!(validate_tool_config_layout("npm", Some("relative/path"), None).is_err());
        assert!(validate_tool_config_layout("npm", Some(r"D:\managed\..\globals"), None).is_err());
    }

    #[test]
    fn config_preview_counts_only_rows_that_actually_change() {
        let current = vec![
            ("UV_TOOL_DIR".to_string(), r"D:\old\uv-tools".to_string()),
            ("UV_CACHE_DIR".to_string(), r"D:\same\uv-cache".to_string()),
        ];
        let next = vec![
            ("UV_TOOL_DIR".to_string(), r"D:\new\uv-tools".to_string()),
            ("UV_CACHE_DIR".to_string(), r"D:\same\uv-cache".to_string()),
            ("UV_TOOL_BIN_DIR".to_string(), r"D:\new\uv-bin".to_string()),
        ];
        let changes = build_config_changes("uv", &current, &next);
        let changed = changes
            .iter()
            .filter(|change| {
                change
                    .current
                    .as_deref()
                    .is_none_or(|value| value != change.next)
            })
            .count();
        // Unchanged rows are still listed, but they must not inflate the count.
        assert_eq!(changes.len(), 3);
        assert_eq!(changed, 2);
    }

    #[tokio::test]
    async fn batch_preview_separates_ready_and_blocked_tools_without_writing() {
        let dir = crate::test_support::TestDir::new();
        let root = dir.path().join("managed");
        std::fs::create_dir_all(&root).unwrap();
        // A directory that cannot be a valid target for any manager: the layout
        // check must block it before any write.
        let invalid = root.join("blocked-target");
        std::fs::create_dir_all(invalid.join("nested")).unwrap();
        let requests = vec![
            (
                "uv".to_string(),
                Some(root.join("globals").join("uv-global").to_string_lossy().into_owned()),
                Some(root.join("globals").join("uv-cache").to_string_lossy().into_owned()),
            ),
            (
                // Relative paths are rejected by the shared layout validation.
                "npm".to_string(),
                Some("relative/global".to_string()),
                None,
            ),
        ];
        let preview = preview_tool_configs(None, &requests).await;
        assert_eq!(preview.items.len(), 2);
        assert!(!preview.moves_existing_packages);

        let npm = preview.items.iter().find(|item| item.tool == "npm").unwrap();
        assert!(
            npm.blocked.iter().any(|reason| reason.contains("绝对路径")),
            "unexpected blocked reasons: {:?}",
            npm.blocked
        );
        assert!(preview.blocked.contains(&"npm".to_string()));
        assert!(preview.ready.contains(&"uv".to_string()) || preview.blocked.contains(&"uv".to_string()));

        // A blocked tool never contributes change rows to the batch total.
        assert_eq!(npm.changed_count, 0);
        assert_eq!(npm.changes.len(), 0);
        assert!(preview.items.iter().all(|item| item
            .changes
            .iter()
            .all(|change| !change.next.is_empty())));
        // Preview is read-only: the invalid target is neither created nor filled.
        assert_eq!(std::fs::read_dir(&invalid).unwrap().count(), 1);
    }

    #[test]
    fn batch_preview_leaves_target_directories_untouched() {        let dir = crate::test_support::TestDir::new();
        let target = dir.path().join("managed/globals/uv-global");
        let cache = dir.path().join("managed/globals/uv-cache");
        // Both targets are absent before a preview; the batch helper itself must
        // never create them.
        assert!(!target.exists());
        assert!(!cache.exists());
        assert!(!config_target_exists(&target));
        assert!(!config_target_non_empty(&target));
        assert!(!target.exists());
        assert!(!cache.exists());
    }

    #[test]
    fn affected_packages_mark_launchers_only_when_the_entry_moves() {
        let rows = vec![
            ("eslint".to_string(), Some("9.1.0".to_string()), None),
            ("typescript".to_string(), Some("5.6.0".to_string()), None),
        ];
        let kept = classify_affected_packages(rows.clone(), false);
        assert_eq!(kept.packages.len(), 2);
        assert_eq!(kept.unresolved_count, 0);
        assert!(kept.packages.iter().all(|package| package.status == "kept"));
        assert!(kept.note.contains("命令入口不变"));

        let moved = classify_affected_packages(rows, true);
        assert_eq!(moved.packages.len(), 2);
        assert_eq!(moved.unresolved_count, 2);
        assert!(moved
            .packages
            .iter()
            .all(|package| package.status == "unresolved"));
        assert!(moved.note.contains("仍保留在原位置"));
        assert!(moved.note.contains("旧入口可能不再解析"));
    }

    #[test]
    fn affected_packages_never_claim_absence_from_an_empty_read() {
        let empty = classify_affected_packages(Vec::new(), true);
        assert!(empty.packages.is_empty());
        assert_eq!(empty.unresolved_count, 0);
        // An empty read must not be described as "nothing is installed".
        assert!(empty.note.contains("不代表该目录为空"));

        let missing = read_affected_packages("npm", None, true);
        assert!(missing.packages.is_empty());
        assert!(missing.note.contains("未找到当前全局安装目录"));
    }

    #[test]
    fn affected_packages_read_the_global_root_without_executing_anything() {
        use crate::test_support::TestDir;
        let dir = TestDir::new();
        let global = dir.path().join("npm-global");
        std::fs::create_dir_all(global.join("node_modules").join("@scope").join("tool")).unwrap();
        std::fs::write(
            global.join("node_modules").join("@scope").join("tool").join("package.json"),
            br#"{"name":"@scope/tool","version":"1.2.3"}"#,
        )
        .unwrap();
        std::fs::create_dir_all(global.join("node_modules").join("plain")).unwrap();
        std::fs::write(
            global.join("node_modules").join("plain").join("package.json"),
            br#"{"name":"plain","version":"2.0.0"}"#,
        )
        .unwrap();

        let affected = read_affected_packages("npm", Some(&global), true);
        let names = affected
            .packages
            .iter()
            .map(|package| package.name.as_str())
            .collect::<Vec<_>>();
        assert!(names.contains(&"plain"));
        assert!(names.contains(&"@scope/tool"));
        let plain = affected
            .packages
            .iter()
            .find(|package| package.name == "plain")
            .unwrap();
        assert_eq!(plain.version.as_deref(), Some("2.0.0"));
        assert_eq!(plain.status, "unresolved");
        // Reading never deletes or rewrites package files.
        assert!(global.join("node_modules").join("plain").is_dir());
    }
}
