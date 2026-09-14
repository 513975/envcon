use std::path::PathBuf;
use crate::fsutil::directory_size as dir_size;

use serde::Serialize;

use crate::error::{AppError, Result};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheInfo {
    /// 工具标识:npm/pip/maven/gradle/cargo/go
    pub tool: String,
    pub name: String,
    pub path: String,
    pub exists: bool,
    pub size_bytes: Option<u64>,
    pub size_complete: bool,
    pub size_detail: Option<String>,
}

fn home() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
}

/// 检测全部缓存目录(不计算大小,大小由 compute_sizes 单独触发)
pub fn detect_caches(root: Option<&std::path::Path>, custom_downloads: Option<&std::path::Path>, environment: &HashMap<String, String>, configured: &HashMap<String, PathBuf>) -> Vec<CacheInfo> {
    let h = home();
    let localappdata = environment.get("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(|| h.join("AppData").join("Local"));

    let gopath = environment.get("GOPATH").cloned()
        .and_then(|v| v.split(';').map(str::trim).find(|p| !p.is_empty()).map(PathBuf::from))
        .unwrap_or_else(|| h.join("go"));

    let downloads = custom_downloads.map(PathBuf::from)
        .or_else(|| root.map(|r| r.join("downloads")))
        .unwrap_or_else(|| PathBuf::from(r"D:\DevEnv\downloads"));

    let npm_cache = configured.get("npm").cloned().or_else(|| environment.get("NPM_CONFIG_CACHE")
        .map(PathBuf::from)
        ).unwrap_or_else(|| localappdata.join("npm-cache"));
    let pip_cache = configured.get("pip").cloned().or_else(|| environment.get("PIP_CACHE_DIR").map(PathBuf::from))
        .unwrap_or_else(|| localappdata.join("pip").join("cache"));
    let maven_repo = environment.get("MAVEN_REPO_LOCAL").map(PathBuf::from)
        .unwrap_or_else(|| h.join(".m2").join("repository"));
    let gradle_home = environment.get("GRADLE_USER_HOME").map(PathBuf::from)
        .unwrap_or_else(|| h.join(".gradle"));
    let cargo_home = environment.get("CARGO_HOME").map(PathBuf::from)
        .unwrap_or_else(|| h.join(".cargo"));
    let gomodcache = environment.get("GOMODCACHE").map(PathBuf::from)
        .unwrap_or_else(|| gopath.join("pkg").join("mod"));

    let mut items: Vec<(&str, &str, PathBuf)> = vec![
        ("npm", "npm 缓存", npm_cache),
        ("pnpm", "pnpm 内容存储", configured.get("pnpm").cloned().unwrap_or_else(|| localappdata.join("pnpm/store"))),
        ("yarn", "Yarn 缓存", configured.get("yarn").cloned().unwrap_or_else(|| localappdata.join("Yarn/Cache"))),
        ("pip", "pip 缓存", pip_cache),
        ("maven", "Maven 仓库(.m2)", maven_repo),
        ("gradle", "Gradle 缓存", gradle_home.join("caches")),
        ("cargo", "Cargo 注册表缓存", cargo_home.join("registry")),
        ("go", "Go 模块缓存", gomodcache),
        ("downloads", "EnvCon 下载临时目录", downloads),
    ];
    for manager in crate::package_managers::definitions().iter().filter(|m| crate::package_managers::isolated(&m.id) && m.id != "cargo") {
        if let Some(path) = configured.get(&manager.id) { items.push((&manager.id, &manager.name, path.clone())); }
    }

    items
        .into_iter()
        .map(|(tool, name, path)| {
            let exists = path.exists();
            CacheInfo {
                tool: tool.into(),
                name: name.into(),
                path: path.to_string_lossy().to_string(),
                exists,
                size_bytes: None,
                size_complete: false,
                size_detail: None,
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
        match s {
            Some((size, complete)) => {
                c.size_bytes = Some(size);
                c.size_complete = complete;
                if !complete { c.size_detail = Some("部分目录无法读取，大小为下限估算".into()); }
            }
            None => { c.size_bytes = None; c.size_complete = false; c.size_detail = Some("无法读取缓存目录".into()); }
        }
    }
    caches
}


/// 清理指定缓存目录内容(保留目录本身)
pub fn clean_cache(tool: &str, root: Option<&std::path::Path>, custom_downloads: Option<&std::path::Path>, environment: &HashMap<String, String>, configured: &HashMap<String, PathBuf>) -> Result<u64> {
    let caches = detect_caches(root, custom_downloads, environment, configured);
    let Some(c) = caches.iter().find(|c| c.tool == tool && c.exists) else {
        return Err(AppError::msg(format!("缓存目录不存在或不可识别: {tool}")));
    };
    let dir = PathBuf::from(&c.path);
    validate_cleanup(&dir, root)?;
    let before = dir_size(&dir).map(|(size, _)| size).unwrap_or(0);
    let rd = std::fs::read_dir(&dir)?;
    let mut failures = Vec::new();
    for entry in rd {
        let entry = entry?;
        let p = entry.path();
        let meta = std::fs::symlink_metadata(&p)?;
        let r = if meta.file_type().is_symlink() {
            if meta.is_dir() { std::fs::remove_dir(&p) } else { std::fs::remove_file(&p) }
        } else if meta.is_dir() {
            std::fs::remove_dir_all(&p)
        } else {
            std::fs::remove_file(&p)
        };
        if let Err(e) = r {
            // 跳过被占用项,继续清理其余
            failures.push(format!("{}: {e}", p.display()));
        }
    }
    // 返回实际释放空间；被占用或无权限的条目会保留在目录中。
    let after = dir_size(&dir).map(|(size, _)| size).unwrap_or(0);
    if !failures.is_empty() { return Err(AppError::msg(format!("已释放 {} 字节；{} 项未清理：{}", before.saturating_sub(after), failures.len(), failures.into_iter().take(3).collect::<Vec<_>>().join("；")))); }
    Ok(before.saturating_sub(after))
}

fn validate_cleanup(dir: &std::path::Path, root: Option<&std::path::Path>) -> Result<()> {
    use crate::detect::system::path_within;
    if !dir.is_absolute() { return Err(AppError::msg("缓存路径必须为绝对路径")); }
    let dir = dir.canonicalize()?;
    let protected = [root.map(PathBuf::from), dirs::home_dir(), dirs::data_dir(), dirs::data_local_dir(),
        std::env::var_os("WINDIR").map(PathBuf::from), std::env::var_os("ProgramFiles").map(PathBuf::from)];
    if protected.into_iter().flatten().any(|p| path_within(&dir, &p))
        || root.is_some_and(|r| ["envs", "current"].iter().any(|p| path_within(&r.join(p), &dir)))
        || ["node.exe", "python.exe", "pyvenv.cfg", "package.json", ".git", ".crates.toml", "composer.json", "venvs", ".store", "uv-receipt.toml"].iter().any(|p| dir.join(p).exists())
        || crate::package_managers::detect(&dir).is_some() {
        return Err(AppError::msg("缓存配置指向受保护目录、项目或运行时，拒绝清理"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn rejects_root_project_runtime_and_link_aliases() {
        let dir = crate::test_support::TestDir::new();
        let root = dir.path().join("managed");
        std::fs::create_dir_all(root.join("envs/node")).unwrap();
        std::fs::create_dir_all(root.join("globals/cache")).unwrap();
        assert!(super::validate_cleanup(&root, Some(&root)).is_err());
        assert!(super::validate_cleanup(&root.join("envs/node"), Some(&root)).is_err());
        assert!(super::validate_cleanup(&root.join("globals/cache"), Some(&root)).is_ok());
        junction::create(&root, dir.path().join("alias")).unwrap();
        assert!(super::validate_cleanup(&dir.path().join("alias"), Some(&root)).is_err());
        std::fs::write(root.join("globals/cache/package.json"), "{}").unwrap();
        assert!(super::validate_cleanup(&root.join("globals/cache"), Some(&root)).is_err());
    }
}
