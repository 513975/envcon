use std::path::PathBuf;

use serde::Serialize;

use crate::error::{AppError, Result};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheInfo {
    /// 工具标识:npm/pip/maven/gradle/cargo/go
    pub tool: &'static str,
    pub name: &'static str,
    pub path: String,
    pub exists: bool,
    pub size_bytes: Option<u64>,
}

fn home() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
}

/// 检测全部缓存目录(不计算大小,大小由 compute_sizes 单独触发)
pub fn detect_caches(root: Option<&std::path::Path>) -> Vec<CacheInfo> {
    let h = home();
    let localappdata = std::env::var("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(|_| h.join("AppData").join("Local"));

    let gopath = std::env::var("GOPATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| h.join("go"));

    let downloads = root
        .map(|r| r.join("downloads"))
        .unwrap_or_else(|| PathBuf::from(r"D:\DevEnv\downloads"));

    let items: Vec<(&'static str, &'static str, PathBuf)> = vec![
        ("npm", "npm 缓存", localappdata.join("npm-cache")),
        ("pip", "pip 缓存", localappdata.join("pip").join("cache")),
        ("maven", "Maven 仓库(.m2)", h.join(".m2").join("repository")),
        ("gradle", "Gradle 缓存", h.join(".gradle").join("caches")),
        ("cargo", "Cargo 注册表缓存", h.join(".cargo").join("registry")),
        ("go", "Go 模块缓存", gopath.join("pkg").join("mod")),
        ("downloads", "EnvCon 下载临时目录", downloads),
    ];

    items
        .into_iter()
        .map(|(tool, name, path)| {
            let exists = path.exists();
            CacheInfo {
                tool,
                name,
                path: path.to_string_lossy().to_string(),
                exists,
                size_bytes: None,
            }
        })
        .collect()
}

/// 并行统计全部缓存大小(填入 size_bytes)
pub async fn compute_sizes(mut caches: Vec<CacheInfo>) -> Vec<CacheInfo> {
    let futs: Vec<_> = caches
        .iter()
        .map(|c| {
            let p = PathBuf::from(&c.path);
            async move {
                if !p.exists() {
                    return None;
                }
                tokio::task::spawn_blocking(move || dir_size(&p))
                    .await
                    .ok()
                    .flatten()
            }
        })
        .collect();
    let sizes = futures_util::future::join_all(futs).await;
    for (c, s) in caches.iter_mut().zip(sizes) {
        c.size_bytes = s;
    }
    caches
}

fn dir_size(path: &std::path::Path) -> Option<u64> {
    let mut total: u64 = 0;
    let mut stack = vec![path.to_path_buf()];
    while let Some(p) = stack.pop() {
        let rd = match std::fs::read_dir(&p) {
            Ok(r) => r,
            Err(_) => continue,
        };
        for entry in rd.flatten() {
            if let Ok(meta) = entry.metadata() {
                if meta.is_dir() {
                    stack.push(entry.path());
                } else {
                    total += meta.len();
                }
            }
        }
    }
    Some(total)
}

/// 清理指定缓存目录内容(保留目录本身)
pub fn clean_cache(tool: &str, root: Option<&std::path::Path>) -> Result<u64> {
    let caches = detect_caches(root);
    let Some(c) = caches.iter().find(|c| c.tool == tool && c.exists) else {
        return Err(AppError::msg(format!("缓存目录不存在或不可识别: {tool}")));
    };
    let dir = PathBuf::from(&c.path);
    let size = dir_size(&dir).unwrap_or(0);
    let rd = std::fs::read_dir(&dir)?;
    for entry in rd.flatten() {
        let p = entry.path();
        let r = if p.is_dir() {
            std::fs::remove_dir_all(&p)
        } else {
            std::fs::remove_file(&p)
        };
        if let Err(e) = r {
            // 跳过被占用项,继续清理其余
            let _ = e;
        }
    }
    Ok(size)
}
