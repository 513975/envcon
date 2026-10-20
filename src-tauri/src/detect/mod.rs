pub mod probe;
pub mod system;
pub(crate) mod discovery;

use std::path::Path;
use std::os::windows::fs::MetadataExt;

use futures_util::{stream, StreamExt};

use crate::config::junction_target_name;
use crate::types::{CategoryOverview, EnvType, ManagedEnv, ALL_ENV_TYPES};

/// 扫描根目录,生成完整 Overview 的 categories 部分
pub async fn scan_root(root: &Path) -> Vec<CategoryOverview> {
    let scans: Vec<_> = ALL_ENV_TYPES.into_iter().map(|t| scan_category(root, t)).collect();
    stream::iter(scans).buffered(3).collect().await
}

/// The current-version junction of one category, read once so every entry is
/// judged against the same observation.
struct CurrentLink {
    name: Option<String>,
    read: Option<std::path::PathBuf>,
    target: Option<std::path::PathBuf>,
}

fn current_link(root: &Path, env_type: EnvType) -> (std::path::PathBuf, CurrentLink) {
    let junction_path = root.join("current").join(env_type.junction());
    let name = junction_target_name(&junction_path);
    let read = std::fs::read_link(&junction_path).ok();
    let target = read.clone()
        .map(|t| if t.is_absolute() { t } else { junction_path.parent().unwrap_or(root).join(t) })
        .and_then(|t| t.canonicalize().ok());
    (junction_path, CurrentLink { name, read, target })
}

/// Build one environment row. `None` means this entry is not an environment
/// (hidden, temporary, or neither directory nor link) — the skip rule the scan
/// has always applied.
async fn read_env_entry(
    root: &Path,
    env_type: EnvType,
    path: &Path,
    name: String,
    current: &CurrentLink,
) -> Option<ManagedEnv> {
    let is_junction = is_reparse_point(path);
    if name.starts_with('.') || name.starts_with(".tmp-") || (!path.is_dir() && !is_junction) {
        return None;
    }
    let accessible = path.is_dir();
    let identity = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let health = env_health(env_type, path, accessible).await;
    let is_current = current.target.as_ref().is_some_and(|target| {
        path.canonicalize().ok().as_ref() == Some(target)
    }) || (current.read.is_none() && current.name.as_deref() == Some(name.as_str()));
    // 启动扫描只读取结构和版本；递归统计大小会阻塞每个环境，改为按需统计。
    Some(ManagedEnv {
        name,
        env_type,
        path: path.to_string_lossy().to_string(),
        version: health.version,
        size_bytes: None,
        is_current,
        status: health.status.into(),
        status_detail: health.status_detail,
        size_complete: false,
        identity_path: identity.to_string_lossy().to_string(),
        is_link_entry: is_junction,
        is_external_link: is_junction && !system::path_within(root, &identity),
    })
}

/// Re-read exactly one environment directory.
///
/// Takes the same current-link observation and runs the same row builder as the
/// category scan, so a single re-detect cannot disagree with a full scan while
/// touching only that directory instead of every entry in the category.
pub async fn read_single_env(root: &Path, env_type: EnvType, name: &str) -> Option<ManagedEnv> {
    let (_, current) = current_link(root, env_type);
    let path = root.join("envs").join(env_type.folder()).join(name);
    read_env_entry(root, env_type, &path, name.to_string(), &current).await
}

async fn scan_category(root: &Path, env_type: EnvType) -> CategoryOverview {
    let cat_dir = root.join("envs").join(env_type.folder());
    let (junction_path, current) = current_link(root, env_type);

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
            if let Some(env) = read_env_entry(root, env_type, &path, name, &current).await {
                envs.push(env);
            }
        }
      }
      Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
      Err(e) => scan_warning = Some(format!("无法读取环境目录: {e}")),
    }
    envs.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    let current_error = if current.name.is_some() && !envs.iter().any(|env| env.is_current) {
        Some("current 链接目标不可访问，或未登记在此分类中".into())
    } else { None };
    CategoryOverview {
        env_type,
        envs,
        current: current.name,
        junction_path: junction_path.to_string_lossy().to_string(),
        scan_warning,
        current_error,
    }
}

fn is_reparse_point(path: &Path) -> bool {
    std::fs::symlink_metadata(path)
        .map(|meta| meta.file_attributes() & 0x400 != 0)
        .unwrap_or(false)
}

/// Result of reading one environment directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvHealth {
    pub version: Option<String>,
    pub status: &'static str,
    pub status_detail: Option<String>,
}

/// Read one environment's structure and version.
///
/// Shared by the full scan and by the per-environment re-detect entry point, so
/// the two can never disagree about what "broken" means or about which reason
/// gets reported.
pub async fn env_health(env_type: EnvType, path: &Path, accessible: bool) -> EnvHealth {
    // Keep the reason, not just the verdict: the status text names the missing
    // entry so a user can tell a repairable directory from a wrong one.
    let structure = accessible
        .then(|| structure_problem(env_type, path))
        .flatten();
    let structural = structure.is_none() && accessible;
    let probe = probe::probe_version_with_reason(env_type, path).await;
    let version = probe.version;
    let status = if !accessible {
        "inaccessible"
    } else if structural && version.is_some() {
        "available"
    } else {
        "broken"
    };
    let status_detail = if !accessible {
        Some("链接目标不存在或目录不可访问".into())
    } else if structural && version.is_some() {
        None
    } else if !structural {
        Some(match structure {
            Some(reason) => format!("目录结构不完整：{reason}"),
            None => "目录结构不完整或缺少必要可执行文件".into(),
        })
    } else {
        // Say *why* the probe failed instead of a bare verdict, when the
        // failing path could report a reason.
        Some(match probe.reason {
            Some(reason) => format!("版本探测失败：{reason}"),
            None => "版本探测失败，无法确认环境可用性".into(),
        })
    };
    EnvHealth { version, status, status_detail }
}

pub(crate) fn looks_like(env_type: EnvType, root: &Path) -> bool {
    structure_problem(env_type, root).is_none()
}

/// Explain *why* a directory does not look like the expected installation,
/// naming the entry that is missing. `None` means the structure check passed.
///
/// The scan reports this instead of a generic "incomplete": knowing *which*
/// executable is absent is what lets a user decide whether the directory is
/// repairable (reinstall into it) or simply the wrong one. `looks_like` is
/// defined in terms of this function so the two can never disagree.
pub(crate) fn structure_problem(env_type: EnvType, root: &Path) -> Option<String> {
    let file = |relative: &str| root.join(relative).is_file();
    let required = |relative: &str| {
        (!file(relative)).then(|| format!("缺少必要入口 {relative}"))
    };
    match env_type {
        EnvType::Jdk => required("bin/java.exe").or_else(|| required("bin/javac.exe")),
        EnvType::Python => required("python.exe"),
        EnvType::Node => required("node.exe"),
        EnvType::Go => required("bin/go.exe"),
        EnvType::Rust => {
            if file("cargo-home/bin/rustc.exe") || system::is_rust_toolchain(root) {
                None
            } else {
                Some("缺少 cargo-home/bin/rustc.exe，目录也不像 Rust 工具链".into())
            }
        }
        EnvType::Maven => {
            if file("bin/mvn.cmd") || file("bin/mvn.bat") {
                None
            } else {
                Some("缺少 bin/mvn.cmd 或 bin/mvn.bat".into())
            }
        }
        EnvType::Gradle => {
            if file("bin/gradle.bat") || file("bin/gradle.cmd") {
                None
            } else {
                Some("缺少 bin/gradle.bat 或 bin/gradle.cmd".into())
            }
        }
        EnvType::Php => required("php.exe"),
        EnvType::Llvm => required("bin/clang.exe"),
        EnvType::Zig => required("zig.exe"),
        EnvType::Deno => required("deno.exe"),
        EnvType::Bun => required("bun.exe"),
        EnvType::Git => required("cmd/git.exe"),
        EnvType::Gh => required("bin/gh.exe"),
        EnvType::Mingw => required("bin/gcc.exe"),
        EnvType::Adb => {
            if system::is_adb_installation(root) {
                None
            } else {
                Some("不是有效的 Android Platform Tools 安装".into())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestDir;

    /// A broken directory must say *what* is missing. A generic "incomplete"
    /// leaves the user unable to tell a repairable directory from a wrong one.
    #[test]
    fn structure_problem_names_the_missing_entry() {
        let dir = TestDir::new();
        let root = dir.path();

        // Empty directory: the first required entry is named.
        let problem = structure_problem(EnvType::Node, root).expect("empty dir is not a Node install");
        assert!(problem.contains("node.exe"), "unexpected: {problem}");

        // Present entry clears the problem, and looks_like agrees by definition.
        std::fs::write(root.join("node.exe"), b"").unwrap();
        assert_eq!(structure_problem(EnvType::Node, root), None);
        assert!(looks_like(EnvType::Node, root));

        // A JDK needs both javac and java; missing the second is reported by name.
        let jdk = TestDir::new();
        std::fs::create_dir_all(jdk.path().join("bin")).unwrap();
        std::fs::write(jdk.path().join("bin/java.exe"), b"").unwrap();
        let problem = structure_problem(EnvType::Jdk, jdk.path()).expect("javac is missing");
        assert!(problem.contains("bin/javac.exe"), "unexpected: {problem}");
        assert!(!looks_like(EnvType::Jdk, jdk.path()));
        std::fs::write(jdk.path().join("bin/javac.exe"), b"").unwrap();
        assert_eq!(structure_problem(EnvType::Jdk, jdk.path()), None);
        assert!(looks_like(EnvType::Jdk, jdk.path()));
    }

    /// `looks_like` must be exactly "no problem", for every environment type.
    #[test]
    fn looks_like_and_structure_problem_never_disagree() {
        for env_type in ALL_ENV_TYPES {
            let dir = TestDir::new();
            assert_eq!(
                looks_like(env_type, dir.path()),
                structure_problem(env_type, dir.path()).is_none(),
                "{env_type:?} disagreed on an empty directory"
            );
        }
    }

    /// A single-directory re-detect must answer for exactly that environment,
    /// pick up a repair without re-reading its siblings, and agree with the full
    /// category scan — that agreement is what lets the narrow read replace the
    /// wide one.
    #[tokio::test]
    async fn single_env_read_matches_the_full_scan_and_skips_non_environments() {
        let dir = TestDir::new();
        let root = dir.path().join("managed");
        let env = root.join("envs/nodes/node-22");
        std::fs::create_dir_all(&env).unwrap();
        std::fs::create_dir_all(root.join("current")).unwrap();

        // Missing entry: the re-read state names which file is absent.
        let entry = read_single_env(&root, EnvType::Node, "node-22")
            .await
            .expect("node-22 must be readable");
        assert_eq!(entry.status, "broken");
        let detail = entry.status_detail.clone().unwrap_or_default();
        assert!(detail.contains("node.exe"), "unexpected: {detail}");

        // Repairing the directory is picked up by the next re-read, and the
        // reason changes to the probe failure instead of staying structural.
        std::fs::write(env.join("node.exe"), b"fixture").unwrap();
        let entry = read_single_env(&root, EnvType::Node, "node-22")
            .await
            .expect("node-22 must be readable");
        let detail = entry.status_detail.clone().unwrap_or_default();
        assert!(!detail.contains("目录结构不完整"), "unexpected: {detail}");
        assert!(detail.contains("版本探测失败"), "unexpected: {detail}");

        // A single re-detect and a full category scan must agree, which is what
        // lets the narrow read replace the wide one. `current` is only set when
        // the junction exists, so this also covers the is_current branch.
        let category = scan_category(&root, EnvType::Node).await;
        let scanned = category
            .envs
            .iter()
            .find(|env| env.name == "node-22")
            .expect("node-22 must be listed");
        assert_eq!(scanned.status, entry.status);
        assert_eq!(scanned.status_detail, entry.status_detail);
        assert_eq!(scanned.version, entry.version);
        assert_eq!(scanned.is_current, entry.is_current);
        assert_eq!(scanned.path, entry.path);

        // The shared helper agrees with both.
        let health = env_health(EnvType::Node, &env, true).await;
        assert_eq!(health.status, entry.status);
        assert_eq!(health.status_detail, entry.status_detail);
        assert_eq!(health.version, entry.version);

        // A name that is not an environment is absent rather than invented.
        assert!(read_single_env(&root, EnvType::Node, "node-absent").await.is_none());
        // Hidden and temporary entries are skipped by the scan's own rule.
        std::fs::create_dir_all(root.join("envs/nodes/.tmp-staging")).unwrap();
        assert!(read_single_env(&root, EnvType::Node, ".tmp-staging").await.is_none());
    }
}
