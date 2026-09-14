use crate::error::{AppError, Result};
use crate::package_managers as managers;
use serde::Serialize;
use std::path::Path;
use std::time::Duration;
use tokio::process::Command;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolConfig {
    pub tool: String,
    pub name: String,
    pub available: bool,
    pub executable_path: Option<String>,
    pub error: Option<String>,
    pub global_path: Option<String>,
    pub cache_path: Option<String>,
    pub default_global: Option<String>,
    pub default_cache: Option<String>,
}

#[derive(Clone)]
pub(crate) struct Invocation {
    pub program: String,
    pub prefix: Vec<String>,
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

async fn run(
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

// Mutating commands may succeed silently; only queries require a returned value.
async fn set(inv: &Invocation, args: &[&str]) -> Result<()> {
    execute(inv, args, 30)
        .await
        .map(|_| ())
        .map_err(|e| AppError::msg(format!("配置命令失败: {e}")))
}

pub(crate) async fn values(
    inv: &Invocation,
    tool: &str,
) -> (Option<String>, Option<String>, Option<String>) {
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
    let (global, cache, error) = match tool {
        "npm" => (
            q(run(inv, &["config", "get", "prefix"], 25).await),
            q(run(inv, &["config", "get", "cache"], 25).await),
            None,
        ),
        "pnpm" => {
            let global = match run(inv, &["config", "get", "global-dir"], 25).await {
                Ok(v) => Some(v),
                Err(_) => q(run(inv, &["root", "--global"], 25).await),
            };
            let cache = q(run(inv, &["store", "path"], 25).await);
            (global, cache, None)
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
            (global, cache, None)
        }
        "pip" => {
            let cache = match run(inv, &["cache", "dir"], 25).await {
                Ok(v) => Some(v),
                Err(_) => q(run(inv, &["config", "get", "global.cache-dir"], 25).await),
            };
            (None, cache, None)
        }
        "bun" => (
            managers::default_source(tool).map(|p| p.to_string_lossy().into()),
            q(run(inv, &["pm", "cache"], 25).await),
            None,
        ),
        "uv" => (
            q(run(inv, &["tool", "dir"], 25).await),
            q(run(inv, &["cache", "dir"], 25).await),
            None,
        ),
        "pipx" => (
            q(run(inv, &["environment", "--value", "PIPX_HOME"], 25).await),
            managers::configured_env_path("PIP_CACHE_DIR")
                .or_else(|| dirs::cache_dir().map(|p| p.join("pip/Cache")))
                .map(|p| p.to_string_lossy().into()),
            None,
        ),
        "cargo" => (
            managers::default_source(tool).map(|p| p.to_string_lossy().into()),
            Some(
                managers::configured_env_path("CARGO_HOME")
                    .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".cargo"))
                    .join("registry")
                    .to_string_lossy()
                    .into(),
            ),
            None,
        ),
        "composer" => (
            q(run(inv, &["config", "--global", "home"], 25).await),
            q(run(inv, &["config", "--global", "cache-dir", "--absolute"], 25).await),
            None,
        ),
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
                q(cache),
                None,
            )
        }
        _ => (None, None, Some("未知工具".into())),
    };
    (
        global,
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
    let mut out = Vec::new();
    for definition in managers::definitions() {
        let tool = definition.id.as_str();
        let name = definition.name.as_str();
        let inv = find_invocation(root, tool).await;
        let (available, global, cache, error) = if let Some(inv) = &inv {
            match run(inv, &["--version"], 15).await {
                Ok(_) => {
                    let (g, c, e) = values(inv, tool).await;
                    (true, g, c, e)
                }
                Err(e) => (false, None, None, Some(format!("版本探测失败: {e}"))),
            }
        } else {
            (false, None, None, Some("未找到可执行命令".into()))
        };
        let base = root.map(|r| r.join("globals"));
        out.push(ToolConfig {
            tool: tool.into(),
            name: name.into(),
            available,
            executable_path: inv.as_ref().map(|i| i.program.clone()),
            error,
            global_path: global,
            cache_path: cache,
            default_global: base
                .as_ref()
                .filter(|_| definition.global_config)
                .map(|p| p.join(format!("{tool}-global")).to_string_lossy().into()),
            default_cache: base
                .filter(|_| definition.cache_config)
                .map(|p| p.join(format!("{tool}-cache")).to_string_lossy().into()),
        });
    }
    out
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
    let inv = find_invocation(root, tool)
        .await
        .ok_or_else(|| AppError::msg(format!("未找到 {tool}，请先安装或激活对应环境")))?;
    run(&inv, &["--version"], 15)
        .await
        .map_err(|e| AppError::msg(format!("{tool} 不可用: {e}")))?;
    for p in [global_path, cache_path].into_iter().flatten() {
        if !Path::new(p).is_absolute()
            || Path::new(p)
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return Err(AppError::msg("配置目录必须是不含 .. 的绝对路径"));
        }
        std::fs::create_dir_all(p)?;
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
    let berry = tool == "yarn"
        && run(&inv, &["--version"], 15)
            .await
            .ok()
            .and_then(|v| v.split('.').next()?.parse::<u32>().ok())
            .is_some_and(|v| v >= 2);
    let (global_key, cache_key) = match tool {
        "npm" => (Some("prefix"), "cache"),
        "pnpm" => (Some("global-dir"), "store-dir"),
        "yarn" if berry => (Some("globalFolder"), "cacheFolder"),
        "yarn" => (Some("global-folder"), "cache-folder"),
        "pip" => (None, "global.cache-dir"),
        _ => return Err(AppError::msg(format!("未知工具: {tool}"))),
    };
    let mut applied = Vec::new();
    if let (Some(p), Some(k)) = (global_path, global_key) {
        set(&inv, &["config", "set", k, p]).await?;
        applied.push(format!("全局安装目录 → {p}"));
    }
    if let Some(p) = cache_path {
        set(&inv, &["config", "set", cache_key, p])
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
        applied.push(format!("缓存/存储目录 → {p}"));
    }
    Ok(applied)
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
