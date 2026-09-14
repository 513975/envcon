pub mod probe;
pub mod system;
pub(crate) mod discovery;

use std::path::{Path, PathBuf};
use std::os::windows::fs::MetadataExt;

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
    let junction_read = std::fs::read_link(&junction_path).ok();
    let current_target = junction_read.clone()
        .map(|target| if target.is_absolute() { target } else { junction_path.parent().unwrap_or(root).join(target) })
        .and_then(|target| target.canonicalize().ok());

    let mut envs: Vec<ManagedEnv> = Vec::new();
    let mut scan_warning = None;
    match tokio::fs::read_dir(&cat_dir).await {
      Ok(mut rd) => {
        loop {
            let entry = match rd.next_entry().await {
                Ok(Some(entry)) => entry,
                Ok(None) => break,
                Err(e) => { scan_warning = Some(format!("部分环境目录未能读取: {e}")); break; }
            };
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            // 跳过临时目录与隐藏目录
            let is_junction = is_reparse_point(&path);
            if name.starts_with('.') || name.starts_with(".tmp-") || (!path.is_dir() && !is_junction) {
                continue;
            }
            let accessible = path.is_dir();
            let identity = path.canonicalize().unwrap_or_else(|_| path.clone());
            let structural = accessible && looks_like(env_type, &path);
            let is_current = current_target.as_ref().is_some_and(|target| {
                path.canonicalize().ok().as_ref() == Some(target)
            }) || (junction_read.is_none() && current.as_deref() == Some(name.as_str()));
            let version = probe::probe_version(env_type, &path).await;
            let (size_bytes, size_complete) = dir_size_blocking(path.clone()).await;
            let status = if !accessible { "inaccessible" } else if structural && version.is_some() { "available" } else { "broken" };
            let status_detail = if !accessible { Some("链接目标不存在或目录不可访问".into()) }
            else if structural && version.is_some() { None } else if !structural {
                Some("目录结构不完整或缺少必要可执行文件".into())
            } else { Some("版本探测失败，无法确认环境可用性".into()) };
            envs.push(ManagedEnv {
                name,
                env_type,
                path: path.to_string_lossy().to_string(),
                version,
                size_bytes,
                is_current,
                status: status.into(),
                status_detail,
                size_complete,
                identity_path: identity.to_string_lossy().to_string(),
                is_external_link: is_junction && !system::path_within(&cat_dir, &identity),
            });
        }
      }
      Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
      Err(e) => scan_warning = Some(format!("无法读取环境目录: {e}")),
    }
    envs.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    let current_error = if current.is_some() && !envs.iter().any(|env| env.is_current) {
        Some("current 链接目标不可访问，或未登记在此分类中".into())
    } else { None };
    CategoryOverview {
        env_type,
        envs,
        current,
        junction_path: junction_path.to_string_lossy().to_string(),
        scan_warning,
        current_error,
    }
}

/// 后台线程递归计算目录大小
async fn dir_size_blocking(path: PathBuf) -> (Option<u64>, bool) {
    tokio::task::spawn_blocking(move || dir_size(&path))
        .await
        .unwrap_or((None, false))
}

fn dir_size(path: &Path) -> (Option<u64>, bool) {
    match crate::fsutil::directory_size(path) {
        Some((bytes, complete)) => (Some(bytes), complete),
        None => (None, false),
    }
}

fn is_reparse_point(path: &Path) -> bool {
    std::fs::symlink_metadata(path)
        .map(|meta| meta.file_attributes() & 0x400 != 0)
        .unwrap_or(false)
}

fn looks_like(env_type: EnvType, root: &Path) -> bool {
    let file = |relative: &str| root.join(relative).is_file();
    match env_type {
        EnvType::Jdk => file("bin/java.exe") && file("bin/javac.exe"),
        EnvType::Python => file("python.exe"),
        EnvType::Node => file("node.exe"),
        EnvType::Go => file("bin/go.exe"),
        EnvType::Rust => file("cargo-home/bin/rustc.exe") || file("cargo-home/bin/cargo.exe"),
        EnvType::Maven => file("bin/mvn.cmd") || file("bin/mvn.bat"),
        EnvType::Gradle => file("bin/gradle.bat") || file("bin/gradle.cmd"),
        EnvType::Php => file("php.exe"),
        EnvType::Llvm => file("bin/clang.exe"),
        EnvType::Zig => file("zig.exe"),
        EnvType::Deno => file("deno.exe"),
        EnvType::Bun => file("bun.exe"),
        EnvType::Git => file("cmd/git.exe"),
        EnvType::Gh => file("bin/gh.exe"),
        EnvType::Mingw => file("bin/gcc.exe"),
    }
}
