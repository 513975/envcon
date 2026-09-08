pub mod probe;
pub mod system;

use std::path::{Path, PathBuf};

use futures_util::future::join_all;

use crate::config::junction_target_name;
use crate::types::{CategoryOverview, EnvType, ManagedEnv, ALL_ENV_TYPES};

/// 扫描根目录,生成完整 Overview 的 categories 部分
pub async fn scan_root(root: &Path) -> Vec<CategoryOverview> {
    join_all(ALL_ENV_TYPES.iter().map(|&t| scan_category(root, t))).await
}

async fn scan_category(root: &Path, env_type: EnvType) -> CategoryOverview {
    let cat_dir = root.join("envs").join(env_type.folder());
    let junction_path = root.join("current").join(env_type.junction());
    let current = junction_target_name(&junction_path);

    let mut envs: Vec<ManagedEnv> = Vec::new();
    if let Ok(mut rd) = tokio::fs::read_dir(&cat_dir).await {
        while let Ok(Some(entry)) = rd.next_entry().await {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            // 跳过临时目录与隐藏目录
            if name.starts_with('.') || name.starts_with(".tmp-") || !path.is_dir() {
                continue;
            }
            let is_current = current.as_deref() == Some(name.as_str());
            let version = probe::probe_version(env_type, &path).await;
            let size_bytes = dir_size_blocking(path.clone()).await;
            envs.push(ManagedEnv {
                name,
                env_type,
                path: path.to_string_lossy().to_string(),
                version,
                size_bytes,
                is_current,
            });
        }
    }
    envs.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    CategoryOverview {
        env_type,
        envs,
        current,
        junction_path: junction_path.to_string_lossy().to_string(),
    }
}

/// 后台线程递归计算目录大小
async fn dir_size_blocking(path: PathBuf) -> Option<u64> {
    tokio::task::spawn_blocking(move || dir_size(&path))
        .await
        .ok()
        .flatten()
}

fn dir_size(path: &Path) -> Option<u64> {
    let mut total: u64 = 0;
    let mut stack = vec![path.to_path_buf()];
    while let Some(p) = stack.pop() {
        let rd = std::fs::read_dir(&p).ok()?;
        for entry in rd.flatten() {
            let Ok(meta) = entry.metadata() else { continue };
            if meta.is_dir() {
                stack.push(entry.path());
            } else {
                total += meta.len();
            }
        }
    }
    Some(total)
}
