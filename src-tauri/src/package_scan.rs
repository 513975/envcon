use crate::{
    error::{AppError, Result},
    package_managers::Package,
};
use serde::Serialize;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
    time::{Duration, Instant},
};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    pub tool: String,
    pub path: String,
    pub packages: usize,
    pub reinstallable: usize,
    pub current: bool,
    pub error: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub status: String,
    pub visited: usize,
    pub current_path: String,
    pub sources: Vec<Source>,
    pub warnings: Vec<String>,
    pub warning_count: usize,
    pub skipped_projects: usize,
    pub roots: Vec<String>,
}

static TASK: Mutex<Option<Snapshot>> = Mutex::new(None);
static CANCEL: AtomicBool = AtomicBool::new(false);

pub fn status() -> Option<Snapshot> {
    TASK.lock().unwrap().clone()
}
pub fn cancel() {
    CANCEL.store(true, Ordering::SeqCst);
}
fn publish(snapshot: &Snapshot) {
    *TASK.lock().unwrap() = Some(snapshot.clone());
}
fn warning(snapshot: &mut Snapshot, text: String) {
    snapshot.warning_count += 1;
    if snapshot.warnings.len() < 80 {
        snapshot.warnings.push(text);
    }
}

fn identity(path: &Path) -> String {
    crate::detect::system::path_key(path)
}
fn normalize(tool: &str, path: PathBuf) -> PathBuf {
    let mut path = path;
    if path
        .file_name()
        .is_some_and(|n| n.eq_ignore_ascii_case("node_modules"))
    {
        path.pop();
    }
    if tool == "pnpm"
        && path
            .file_name()
            .is_some_and(|n| n == "v11" || n.to_string_lossy().bytes().all(|b| b.is_ascii_digit()))
    {
        path.pop();
    }
    path
}

fn local_drives() -> Vec<PathBuf> {
    use windows_sys::Win32::Storage::FileSystem::{GetDriveTypeW, GetLogicalDrives};
    let mask = unsafe { GetLogicalDrives() };
    (0..26)
        .filter_map(|index| {
            if mask & (1 << index) == 0 {
                return None;
            }
            let path = format!("{}:\\", char::from(b'A' + index));
            let wide: Vec<u16> = path.encode_utf16().chain(Some(0)).collect();
            matches!(unsafe { GetDriveTypeW(wide.as_ptr()) }, 2 | 3).then(|| PathBuf::from(path))
        })
        .collect()
}

pub async fn start(
    managed: Option<PathBuf>,
    mode: String,
    directory: Option<String>,
) -> Result<()> {
    if !matches!(mode.as_str(), "common" | "drives" | "directory") {
        return Err(AppError::msg("未知扫描范围"));
    }
    let selected = if mode == "directory" {
        let path = PathBuf::from(directory.ok_or_else(|| AppError::msg("请选择扫描目录"))?);
        if !path.is_absolute() || !path.is_dir() {
            return Err(AppError::msg("请选择已存在的绝对目录"));
        }
        Some(path)
    } else {
        None
    };
    let guard = crate::migration::LOCK
        .try_lock()
        .map_err(|_| AppError::msg("已有扫描、迁移或重装任务正在执行"))?;
    CANCEL.store(false, Ordering::SeqCst);
    let initial = Snapshot {
        status: "running".into(),
        visited: 0,
        current_path: "正在读取目录配置".into(),
        sources: vec![],
        warnings: vec![],
        warning_count: 0,
        skipped_projects: 0,
        roots: vec![],
    };
    publish(&initial);
    tauri::async_runtime::spawn(async move {
        let _guard = guard;
        let configs = {
            let configs = crate::pkgtools::get_tool_configs(managed.as_deref());
            tokio::pin!(configs);
            loop {
                tokio::select! {
                    configs = &mut configs => break configs,
                    _ = tokio::time::sleep(Duration::from_millis(100)) => {
                        if CANCEL.load(Ordering::SeqCst) {
                            let mut snapshot = initial;
                            snapshot.status = "canceled".into();
                            snapshot.current_path.clear();
                            publish(&snapshot);
                            return;
                        }
                    }
                }
            }
        };
        let result = tokio::task::spawn_blocking(move || {
            let mut snapshot = initial;
            let mut roots = Vec::new();
            let mut known: HashMap<String, String> = HashMap::new();
            for config in configs {
                if let Some(path) = config.global_path {
                    let path = normalize(&config.tool, PathBuf::from(path));
                    if let Ok(path) = path.canonicalize() {
                        known.insert(identity(&path), config.tool);
                        if mode == "common" {
                            roots.push(path);
                        }
                    }
                }
            }
            if let Some(selected) = selected {
                roots.push(selected);
            } else if mode == "drives" {
                roots.extend(local_drives());
            } else {
                let discovery = crate::detect::discovery::discover(managed.as_deref(), &[]);
                for text in discovery.warnings {
                    warning(&mut snapshot, text);
                }
                roots.extend(discovery.directories.into_iter().map(|(p, _)| p));
                roots.extend(discovery.path);
                if let Some(root) = managed {
                    roots.push(root.join("envs"));
                    roots.push(root.join("globals"));
                }
                if let Some(home) = dirs::home_dir() {
                    for child in [
                        ".bun/install/global",
                        ".cargo",
                        ".local/share",
                        ".virtualenvs",
                        ".conda/envs",
                        "scoop/apps",
                        ".dotnet/tools",
                    ] {
                        roots.push(home.join(child));
                    }
                }
                for (base, children) in [
                    (
                        dirs::data_dir(),
                        vec!["npm", "Python", "Composer", "uv", "pipx"],
                    ),
                    (
                        dirs::data_local_dir(),
                        vec![
                            "pnpm/global",
                            "Yarn/Data/global",
                            "Programs/Python",
                            "uv",
                            "pipx",
                        ],
                    ),
                ] {
                    if let Some(base) = base {
                        roots.extend(children.into_iter().map(|p| base.join(p)));
                    }
                }
                roots.retain(|p| p.is_dir());
            }
            if roots.is_empty() {
                warning(&mut snapshot, "未找到可访问的扫描根目录".into());
            }
            snapshot.roots = roots.iter().map(|p| p.to_string_lossy().into()).collect();
            walk(
                roots,
                &known,
                snapshot,
                &CANCEL,
                250_000,
                Duration::from_secs(300),
                publish,
            )
        })
        .await;
        match result {
            Ok(snapshot) => publish(&snapshot),
            Err(error) => {
                if let Some(mut snapshot) = status() {
                    snapshot.status = "error".into();
                    warning(&mut snapshot, format!("扫描线程异常: {error}"));
                    publish(&snapshot);
                }
            }
        }
    });
    Ok(())
}

fn detect(path: &Path, known: Option<&String>, snapshot: &mut Snapshot) -> Option<String> {
    if let Some(tool) = known {
        return Some(tool.clone());
    }
    if let Some(tool) = crate::package_managers::detect(path) {
        return Some(tool.into());
    }
    if path
        .file_name()
        .is_some_and(|n| n.eq_ignore_ascii_case("node_modules"))
    {
        return path.parent().and_then(|p| detect(p, None, snapshot));
    }
    if path.join("v11").is_dir()
        || (path.file_name().is_some_and(|n| n == "v11")
            && path.read_dir().ok().is_some_and(|mut entries| {
                entries.any(|e| e.is_ok_and(|e| e.file_type().is_ok_and(|t| t.is_symlink())))
            }))
    {
        return Some("pnpm".into());
    }
    if path
        .file_name()
        .is_some_and(|n| n.eq_ignore_ascii_case("site-packages"))
    {
        return Some("pip".into());
    }
    if !path.join("node_modules").is_dir() {
        return None;
    }
    let manifest = std::fs::read(path.join("package.json"))
        .ok()
        .and_then(|v| serde_json::from_slice::<serde_json::Value>(&v).ok());
    if !path.join("node.exe").is_file()
        && manifest
            .as_ref()
            .is_some_and(|v| v.get("name").is_some() || v.get("scripts").is_some())
    {
        snapshot.skipped_projects += 1;
        return None;
    }
    Some(
        if path.join("node_modules/.modules.yaml").is_file()
            || path.join("pnpm-lock.yaml").is_file()
        {
            "pnpm"
        } else if path.join("bun.lock").is_file() || path.join("bun.lockb").is_file() {
            "bun"
        } else if path.join("yarn.lock").is_file()
            || path.join("node_modules/.yarn-integrity").is_file()
        {
            "yarn"
        } else {
            "npm"
        }
        .into(),
    )
}

fn read_packages(tool: &str, path: &Path) -> Result<Vec<Package>> {
    if tool == "pip" {
        crate::pip_reinstall::disk_packages(path)
    } else {
        Ok(crate::package_managers::installed_packages(tool, path)?
            .into_iter()
            .map(|(p, _)| p)
            .collect())
    }
}

fn walk(
    roots: Vec<PathBuf>,
    known: &HashMap<String, String>,
    mut snapshot: Snapshot,
    cancel: &AtomicBool,
    limit: usize,
    timeout: Duration,
    progress: impl Fn(&Snapshot),
) -> Snapshot {
    let start = Instant::now();
    let mut last = Instant::now();
    let mut pending: VecDeque<_> = roots.into_iter().map(|p| (p, 0)).collect();
    let mut visited = HashSet::new();
    let mut groups = HashSet::new();
    while let Some((path, depth)) = pending.pop_front() {
        if cancel.load(Ordering::SeqCst) {
            snapshot.status = "canceled".into();
            break;
        }
        if snapshot.visited >= limit || start.elapsed() >= timeout {
            warning(
                &mut snapshot,
                "达到目录数量或时间上限，请缩小范围继续扫描".into(),
            );
            break;
        }
        let path = match path.canonicalize() {
            Ok(path) => crate::fsutil::plain_path(path),
            Err(e) => {
                warning(&mut snapshot, format!("无法访问 {}: {e}", path.display()));
                continue;
            }
        };
        if !visited.insert(identity(&path)) {
            continue;
        }
        snapshot.visited += 1;
        snapshot.current_path = path.to_string_lossy().into();
        let mut tool = detect(&path, known.get(&identity(&path)), &mut snapshot);
        let entries = match std::fs::read_dir(&path) {
            Ok(entries) => entries,
            Err(e) => {
                warning(&mut snapshot, format!("无法读取 {}: {e}", path.display()));
                continue;
            }
        };
        let mut children = Vec::new();
        for entry in entries {
            if cancel.load(Ordering::SeqCst) {
                break;
            }
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    warning(
                        &mut snapshot,
                        format!("目录项读取失败 {}: {e}", path.display()),
                    );
                    continue;
                }
            };
            let name = entry.file_name().to_string_lossy().to_lowercase();
            if name.ends_with(".dist-info")
                || name.ends_with(".egg-info")
                || name.ends_with(".egg-link")
            {
                tool = Some("pip".into());
            }
            let meta = match std::fs::symlink_metadata(entry.path()) {
                Ok(meta) => meta,
                Err(e) => {
                    warning(
                        &mut snapshot,
                        format!("无法读取 {}: {e}", entry.path().display()),
                    );
                    continue;
                }
            };
            if meta.file_type().is_symlink() {
                warning(
                    &mut snapshot,
                    format!("已跳过链接目录，可单独选择扫描: {}", entry.path().display()),
                );
                continue;
            }
            if !meta.is_dir() {
                continue;
            }
            if matches!(
                name.as_str(),
                "node_modules"
                    | ".git"
                    | "cache"
                    | "caches"
                    | "store"
                    | "registry"
                    | "$recycle.bin"
                    | "system volume information"
                    | "windows"
                    | "target"
            ) {
                continue;
            }
            if depth >= 64 {
                warning(
                    &mut snapshot,
                    format!("超过扫描深度: {}", entry.path().display()),
                );
                continue;
            }
            children.push((entry.path(), depth + 1));
        }
        if let Some(tool) = tool {
            let source = normalize(&tool, path.clone());
            let key = format!("{tool}:{}", identity(&source));
            if groups.insert(key) {
                match read_packages(&tool, &source) {
                    Ok(packages) if !packages.is_empty() => snapshot.sources.push(Source {
                        current: known.get(&identity(&source)) == Some(&tool),
                        tool,
                        path: source.to_string_lossy().into(),
                        packages: packages.len(),
                        reinstallable: packages.iter().filter(|p| p.reason.is_none()).count(),
                        error: None,
                    }),
                    Ok(_) => {}
                    Err(e) => {
                        warning(
                            &mut snapshot,
                            format!("包清单读取失败 {}: {e}", source.display()),
                        );
                        snapshot.sources.push(Source {
                            current: known.get(&identity(&source)) == Some(&tool),
                            tool,
                            path: source.to_string_lossy().into(),
                            packages: 0,
                            reinstallable: 0,
                            error: Some(e.to_string()),
                        });
                    }
                }
            }
            children.clear();
        }
        pending.extend(children);
        if last.elapsed() >= Duration::from_millis(400) {
            progress(&snapshot);
            last = Instant::now();
        }
    }
    if snapshot.status != "canceled" {
        snapshot.status = if snapshot.warning_count > 0 {
            "partial"
        } else {
            "done"
        }
        .into();
    }
    snapshot.current_path.clear();
    snapshot
        .sources
        .sort_by(|a, b| (&a.tool, &a.path).cmp(&(&b.tool, &b.path)));
    snapshot
}

#[cfg(test)]
mod tests {
    use super::*;
    fn empty() -> Snapshot {
        Snapshot {
            status: "running".into(),
            visited: 0,
            current_path: String::new(),
            sources: vec![],
            warnings: vec![],
            warning_count: 0,
            skipped_projects: 0,
            roots: vec![],
        }
    }

    #[test]
    #[ignore = "read-only scan of the explicitly selected ENVCON_TEST_SCAN_ROOT"]
    fn actual_historical_source_scan() {
        let root = PathBuf::from(std::env::var("ENVCON_TEST_SCAN_ROOT").unwrap());
        let result = walk(
            vec![root],
            &HashMap::new(),
            empty(),
            &AtomicBool::new(false),
            10000,
            Duration::from_secs(60),
            |_| {},
        );
        assert!(result
            .sources
            .iter()
            .any(|s| s.tool == "npm" && s.path.contains("node-24") && s.packages >= 13));
        println!("{}", serde_json::to_string_pretty(&result).unwrap());
    }

    #[test]
    fn scans_distinct_sources_without_entering_projects_or_link_cycles() {
        let d = crate::test_support::TestDir::new();
        for dir in ["old-a", "old-b", "project"] {
            let root = d.path().join(dir);
            std::fs::create_dir_all(root.join("node_modules/cli")).unwrap();
            std::fs::write(
                root.join("node_modules/cli/package.json"),
                r#"{"name":"cli","version":"1.0.0"}"#,
            )
            .unwrap();
        }
        std::fs::write(
            d.path().join("project/package.json"),
            r#"{"name":"application","dependencies":{"cli":"1.0.0"}}"#,
        )
        .unwrap();
        let python = d.path().join("venv/Lib/site-packages");
        std::fs::create_dir_all(python.join("cli-1.0.dist-info")).unwrap();
        std::fs::write(
            python.join("cli-1.0.dist-info/METADATA"),
            "Name: cli\nVersion: 1.0\n",
        )
        .unwrap();
        junction::create(d.path(), d.path().join("loop")).unwrap();
        let known = HashMap::from([(
            identity(&d.path().join("old-a").canonicalize().unwrap()),
            "npm".into(),
        )]);
        let result = walk(
            vec![d.path().into(), d.path().join("old-a")],
            &known,
            empty(),
            &AtomicBool::new(false),
            100,
            Duration::from_secs(10),
            |_| {},
        );
        assert_eq!(result.sources.len(), 3);
        assert_eq!(result.sources.iter().filter(|s| s.tool == "npm").count(), 2);
        assert_eq!(result.sources.iter().filter(|s| s.current).count(), 1);
        assert_eq!(result.skipped_projects, 1);
        assert!(result.visited < 20);
        assert_eq!(result.status, "partial");
        assert!(result.warnings.iter().any(|s| s.contains("链接目录")));
    }

    #[test]
    fn cancellation_and_limits_never_claim_a_complete_scan() {
        let d = crate::test_support::TestDir::new();
        let canceled = walk(
            vec![d.path().into()],
            &HashMap::new(),
            empty(),
            &AtomicBool::new(true),
            100,
            Duration::from_secs(10),
            |_| {},
        );
        assert_eq!(canceled.status, "canceled");
        let limited = walk(
            vec![d.path().into()],
            &HashMap::new(),
            empty(),
            &AtomicBool::new(false),
            0,
            Duration::from_secs(10),
            |_| {},
        );
        assert_eq!(limited.status, "partial");
        assert_eq!(limited.warning_count, 1);
        let missing = walk(
            vec![d.path().join("missing")],
            &HashMap::new(),
            empty(),
            &AtomicBool::new(false),
            100,
            Duration::from_secs(10),
            |_| {},
        );
        assert_eq!(missing.status, "partial");
    }
}
