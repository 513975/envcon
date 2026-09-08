use std::path::Path;
use std::time::Duration;

use serde::Serialize;
use tokio::process::Command;

use crate::error::{AppError, Result};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolConfig {
    /// 工具标识:npm/pnpm/yarn/pip
    pub tool: String,
    pub name: String,
    pub available: bool,
    pub global_path: Option<String>,
    pub cache_path: Option<String>,
    /// 建议默认值(基于管理根目录推导)
    pub default_global: Option<String>,
    pub default_cache: Option<String>,
}

/// 工具定义:(id, 显示名, 是否有全局安装路径概念)
const TOOLS: &[(&str, &str, bool)] = &[
    ("npm", "npm", true),
    ("pnpm", "pnpm", true),
    ("yarn", "Yarn", true),
    ("pip", "pip", false),
];

/// 定位工具可执行文件:优先受管理环境,其次 PATH
async fn find_tool_exe(root: Option<&Path>, name: &str) -> Option<String> {
    // npm 优先用受管理 node 自带的 npm.cmd
    if name == "npm" {
        if let Some(r) = root {
            let p = r.join("current").join("node").join("npm.cmd");
            if p.exists() {
                return Some(p.to_string_lossy().to_string());
            }
        }
    }
    let out = Command::new("where.exe")
        .arg(name)
        .creation_flags(0x08000000)
        .output()
        .await
        .ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|l| l.trim())
        .find(|l| !l.is_empty())
        .map(|l| l.to_string())
}

/// 执行工具命令,取首个非空输出行
async fn run_tool(exe: &str, args: &[&str]) -> Option<String> {
    let fut = Command::new(exe)
        .args(args)
        .creation_flags(0x08000000)
        .output();
    let out = tokio::time::timeout(Duration::from_secs(25), fut)
        .await
        .ok()?
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout);
    s.lines()
        .map(|l| l.trim())
        .find(|l| !l.is_empty())
        .map(|l| l.to_string())
}

async fn run_set(exe: &str, args: &[&str]) -> Result<()> {
    let fut = Command::new(exe)
        .args(args)
        .creation_flags(0x08000000)
        .output();
    let out = tokio::time::timeout(Duration::from_secs(30), fut)
        .await
        .map_err(|_| AppError::msg("命令执行超时"))?
        .map_err(|e| AppError::msg(format!("启动失败: {e}")))?;
    if out.status.success() {
        Ok(())
    } else {
        let err = String::from_utf8_lossy(&out.stderr);
        Err(AppError::msg(format!(
            "配置命令失败: {}{}",
            args.join(" "),
            if err.trim().is_empty() {
                String::new()
            } else {
                format!(" — {}", err.trim())
            }
        )))
    }
}

async fn get_values(exe: &str, tool: &str) -> (Option<String>, Option<String>) {
    match tool {
        "npm" => (
            run_tool(exe, &["config", "get", "prefix"]).await,
            run_tool(exe, &["config", "get", "cache"]).await,
        ),
        "pnpm" => (
            run_tool(exe, &["config", "get", "global-dir"]).await,
            run_tool(exe, &["config", "get", "cache-dir"]).await,
        ),
        "yarn" => (
            run_tool(exe, &["config", "get", "global-folder"]).await,
            run_tool(exe, &["config", "get", "cache-folder"]).await,
        ),
        "pip" => (
            None,
            run_tool(exe, &["config", "get", "global.cache-dir"]).await,
        ),
        _ => (None, None),
    }
}

/// 读取各包管理器当前的全局/缓存路径配置
pub async fn get_tool_configs(root: Option<&Path>) -> Vec<ToolConfig> {
    let mut out = Vec::new();
    for &(tool, name, has_global) in TOOLS {
        let exe = find_tool_exe(root, tool).await;
        let (global_path, cache_path) = match &exe {
            Some(e) => get_values(e, tool).await,
            None => (None, None),
        };
        let (default_global, default_cache) = match root {
            Some(r) => {
                let globals = r.join("globals");
                (
                    if has_global {
                        Some(globals.join(format!("{tool}-global")).to_string_lossy().to_string())
                    } else {
                        None
                    },
                    Some(globals.join(format!("{tool}-cache")).to_string_lossy().to_string()),
                )
            }
            None => (None, None),
        };
        out.push(ToolConfig {
            tool: tool.to_string(),
            name: name.to_string(),
            available: exe.is_some(),
            global_path,
            cache_path,
            default_global,
            default_cache,
        });
    }
    out
}

/// 应用单个工具的全局/缓存路径配置,返回已应用项的描述
pub async fn apply_tool_config(
    root: Option<&Path>,
    tool: &str,
    global_path: Option<&str>,
    cache_path: Option<&str>,
) -> Result<Vec<String>> {
    let exe = find_tool_exe(root, tool)
        .await
        .ok_or_else(|| AppError::msg(format!("未找到 {tool},请先安装或激活对应环境")))?;

    // 预创建目录
    for p in [global_path, cache_path].into_iter().flatten() {
        std::fs::create_dir_all(p)?;
    }

    let mut applied: Vec<String> = Vec::new();
    match tool {
        "npm" => {
            if let Some(g) = global_path {
                run_set(&exe, &["config", "set", "prefix", g]).await?;
                applied.push(format!("全局安装目录 → {g}"));
            }
            if let Some(c) = cache_path {
                run_set(&exe, &["config", "set", "cache", c]).await?;
                applied.push(format!("缓存目录 → {c}"));
            }
        }
        "pnpm" => {
            if let Some(g) = global_path {
                run_set(&exe, &["config", "set", "global-dir", g]).await?;
                applied.push(format!("全局安装目录 → {g}"));
            }
            if let Some(c) = cache_path {
                run_set(&exe, &["config", "set", "cache-dir", c]).await?;
                applied.push(format!("缓存目录 → {c}"));
            }
        }
        "yarn" => {
            if let Some(g) = global_path {
                run_set(&exe, &["config", "set", "global-folder", g]).await?;
                applied.push(format!("全局安装目录 → {g}"));
            }
            if let Some(c) = cache_path {
                run_set(&exe, &["config", "set", "cache-folder", c]).await?;
                applied.push(format!("缓存目录 → {c}"));
            }
        }
        "pip" => {
            if let Some(c) = cache_path {
                run_set(&exe, &["config", "set", "global.cache-dir", c]).await?;
                applied.push(format!("缓存目录 → {c}"));
            }
        }
        _ => return Err(AppError::msg(format!("未知工具: {tool}"))),
    }
    Ok(applied)
}
