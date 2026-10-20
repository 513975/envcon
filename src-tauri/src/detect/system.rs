use std::path::{Path, PathBuf};
use std::time::Duration;

use tokio::process::Command;

use crate::types::{EnvType, ExternalEnv, ScanReport, ScanRunStatus, ScanStatus, ToolScanSummary};
use super::discovery::{self, Discovery};

/// 工具定义:(展示名, 可执行名, 版本参数, 版本命令超时)
const TOOLS: &[(&str, &str, &[&str], Duration)] = &[
    // 语言运行时
    ("Java", "java", &["-version"], Duration::from_secs(8)),
    ("Python", "python", &["--version"], Duration::from_secs(8)),
    ("Node.js", "node", &["--version"], Duration::from_secs(5)),
    ("Go", "go", &["version"], Duration::from_secs(8)),
    ("Rust", "rustc", &["--version"], Duration::from_secs(8)),
    ("PHP", "php", &["-v"], Duration::from_secs(5)),
    ("Zig", "zig", &["version"], Duration::from_secs(5)),
    ("LLVM", "clang", &["--version"], Duration::from_secs(5)),
    ("Deno", "deno", &["--version"], Duration::from_secs(5)),
    ("Bun", "bun", &["--version"], Duration::from_secs(5)),
    ("GCC", "gcc", &["--version"], Duration::from_secs(5)),
    ("CMake", "cmake", &["--version"], Duration::from_secs(5)),
    ("GitHub CLI", "gh", &["--version"], Duration::from_secs(5)),
    // 包管理器
    ("npm", "npm", &["--version"], Duration::from_secs(10)),
    ("pnpm", "pnpm", &["--version"], Duration::from_secs(10)),
    ("yarn", "yarn", &["--version"], Duration::from_secs(10)),
    ("pip", "pip", &["--version"], Duration::from_secs(10)),
    ("pipx", "pipx", &["--version"], Duration::from_secs(10)),
    ("uv", "uv", &["--version"], Duration::from_secs(5)),
    ("Poetry", "poetry", &["--version"], Duration::from_secs(10)),
    ("Cargo", "cargo", &["--version"], Duration::from_secs(8)),
    ("Composer", "composer", &["--version"], Duration::from_secs(8)),
    ("Conda", "conda", &["--version"], Duration::from_secs(10)),
    // 构建工具
    ("Maven", "mvn", &["--version"], Duration::from_secs(20)),
    ("Gradle", "gradle", &["--version"], Duration::from_secs(20)),
    // 其他常用
    ("Git", "git", &["--version"], Duration::from_secs(5)),
    (".NET", "dotnet", &["--version"], Duration::from_secs(10)),
    ("Docker", "docker", &["--version"], Duration::from_secs(8)),
    ("ADB", "adb", &["version"], Duration::from_secs(5)),
];

/// 工具名 → 可纳入管理的环境类型(结构不兼容的不支持)
fn integrable_type(tool: &str) -> Option<EnvType> {
    match tool {
        "Java" => Some(EnvType::Jdk),
        "Python" => Some(EnvType::Python),
        "Node.js" => Some(EnvType::Node),
        "Go" => Some(EnvType::Go),
        "Rust" => Some(EnvType::Rust),
        "PHP" => Some(EnvType::Php),
        "Zig" => Some(EnvType::Zig),
        "LLVM" => Some(EnvType::Llvm),
        "Deno" => Some(EnvType::Deno),
        "Bun" => Some(EnvType::Bun),
        "GitHub CLI" => Some(EnvType::Gh),
        "Maven" => Some(EnvType::Maven),
        "Gradle" => Some(EnvType::Gradle),
        "Git" => Some(EnvType::Git),
        "GCC" => Some(EnvType::Mingw),
        "ADB" => Some(EnvType::Adb),
        _ => None,
    }
}

/// 目录是否符合该类型环境的预期结构(防止链接到错误目录)
fn looks_like(env_type: EnvType, root: &Path) -> bool {
    let has = |p: PathBuf| p.is_file();
    match env_type {
        // 只有 java 的 JRE 不能作为 JDK 纳入管理。
        EnvType::Jdk => has(root.join("bin").join("java.exe")) && has(root.join("bin").join("javac.exe")),
        EnvType::Python => has(root.join("python.exe")),
        EnvType::Node => has(root.join("node.exe")),
        EnvType::Go => has(root.join("bin").join("go.exe")),
        EnvType::Maven => has(root.join("bin").join("mvn.cmd")) || has(root.join("bin").join("mvn.bat")),
        EnvType::Gradle => has(root.join("bin").join("gradle.bat")),
        EnvType::Php => has(root.join("php.exe")),
        EnvType::Llvm => has(root.join("bin").join("clang.exe")),
        EnvType::Zig => has(root.join("zig.exe")),
        EnvType::Deno => has(root.join("deno.exe")),
        EnvType::Bun => has(root.join("bun.exe")),
        EnvType::Git => has(root.join("cmd").join("git.exe")),
        EnvType::Gh => has(root.join("bin").join("gh.exe")),
        EnvType::Mingw => has(root.join("bin").join("gcc.exe")),
        EnvType::Rust => is_rust_toolchain(root),
        EnvType::Adb => is_adb_installation(root),
    }
}

/// 从 where.exe 定位到的可执行文件推导安装根目录
pub fn install_root_for(env_type: EnvType, exe: &Path) -> Option<PathBuf> {
    if env_type == EnvType::Adb {
        // Only the executable's own directory belongs to this environment, never its SDK parent.
        return exe.parent().filter(|root| is_adb_installation(root)).map(Path::to_path_buf);
    }
    if env_type == EnvType::Git {
        // Git for Windows includes wrappers and internal copies under the same root.
        for root in exe.ancestors().skip(2).take(4) {
            let relative_parent = exe.strip_prefix(root).ok()?.parent()?;
            if matches!(path_key(relative_parent).as_str(), "cmd" | "bin" | "mingw64\\bin" | "mingw32\\bin" | "mingw64\\libexec\\git-core" | "mingw32\\libexec\\git-core")
                && looks_like(env_type, root) { return Some(root.to_path_buf()); }
        }
        return None;
    }
    let dir = exe.parent()?;
    let dir_name = dir.file_name().map(|n| n.to_string_lossy().to_lowercase());
    let mut candidates: Vec<PathBuf> = Vec::new();
    match dir_name.as_deref() {
        // exe 位于 bin\ / cmd\ 下:安装根目录一般是其父级
        Some("bin") | Some("cmd") | Some("scripts") => {
            if let Some(p) = dir.parent() {
                candidates.push(p.to_path_buf());
            }
        }
        // shim 目录(scoop 等)不是真实安装目录
        Some("shims") => return None,
        _ => {}
    }
    candidates.push(dir.to_path_buf());
    candidates.into_iter().find(|c| looks_like(env_type, c))
}

/// A rustup proxy is not a toolchain. Only link a complete, concrete installation.
pub(crate) fn is_rust_toolchain(root: &Path) -> bool {
    root.join("bin/rustc.exe").is_file()
        && root.join("bin/cargo.exe").is_file()
        && root.join("lib/rustlib").is_dir()
}

pub(crate) fn is_adb_installation(root: &Path) -> bool {
    ["adb.exe", "AdbWinApi.dll", "AdbWinUsbApi.dll"].iter().all(|name| root.join(name).is_file())
}

async fn rust_toolchain_executable(exe: &Path, discovery: &Discovery) -> Option<PathBuf> {
    if install_root_for(EnvType::Rust, exe).is_some() { return Some(exe.to_path_buf()); }
    let rustup = exe.parent()?.join("rustup.exe");
    if !rustup.is_file() { return None; }
    let mut command = Command::new(rustup);
    command.args(["which", "rustc"]).envs(&discovery.environment)
        .current_dir(std::env::temp_dir()).creation_flags(0x08000000);
    if let Some(home) = exe.parent().and_then(Path::parent)
        .filter(|home| home.file_name().is_some_and(|n| n == "cargo-home")) {
        command.env("CARGO_HOME", home);
        command.env("RUSTUP_HOME", home.parent()?.join("rustup-home"));
    }
    let output = crate::reinstall_process::output(command, Duration::from_secs(8)).await.ok()?;
    if !output.status.success() { return None; }
    let path = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());
    if !path.is_absolute() { return None; }
    install_root_for(EnvType::Rust, &path)?;
    Some(path)
}

fn scan_identity(tool: &str, path: &Path) -> PathBuf {
    let identity = integrable_type(tool)
        .and_then(|kind| install_root_for(kind, path))
        .unwrap_or_else(|| path.to_path_buf());
    identity.canonicalize().unwrap_or(identity)
}

/// 路径等价比较(不区分大小写与分隔符方向)
pub(crate) fn path_key(p: &Path) -> String {
        let value = p.to_string_lossy()
            .to_lowercase()
            .replace('/', "\\");
        let value = value.strip_prefix("\\\\?\\").unwrap_or(&value);
        value
            .trim_end_matches('\\')
            .to_string()
}

fn canonicalize_nearest_existing(path: &Path) -> PathBuf {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(path))
            .unwrap_or_else(|_| path.to_path_buf())
    };
    let mut candidate = absolute.clone();
    let mut suffix = Vec::new();
    loop {
        if let Ok(mut canonical) = candidate.canonicalize() {
            for component in suffix.iter().rev() {
                canonical.push(component);
            }
            return canonical;
        }
        let Some(name) = candidate.file_name() else {
            return absolute;
        };
        suffix.push(name.to_os_string());
        if !candidate.pop() {
            return absolute;
        }
    }
}

pub(crate) fn paths_eq(a: &Path, b: &Path) -> bool {
    path_key(&canonicalize_nearest_existing(a)) == path_key(&canonicalize_nearest_existing(b))
}

pub(crate) fn path_within(root: &Path, candidate: &Path) -> bool {
    let root = canonicalize_nearest_existing(root);
    let candidate = canonicalize_nearest_existing(candidate);
    let root_key = path_key(&root);
    let candidate_key = path_key(&candidate);
    candidate_key == root_key
        || candidate_key.strip_prefix(&root_key).is_some_and(|rest| rest.starts_with('\\'))
}

pub(crate) fn is_reparse_path(path: &Path) -> bool {
    let Ok(meta) = std::fs::symlink_metadata(path) else {
        return false;
    };
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        meta.file_type().is_symlink() || meta.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        meta.file_type().is_symlink()
    }
}

pub(crate) fn has_reparse_component(path: &Path) -> bool {
    path.ancestors().any(is_reparse_path)
}

/// Refresh persisted PATH, then discover installed and selected project tools.
pub async fn scan_system(managed_root: Option<&Path>, integrated: &[PathBuf], projects: &[PathBuf]) -> ScanReport {
    let root = managed_root.map(Path::to_path_buf);
    let projects = projects.to_vec();
    let discovery = match tokio::task::spawn_blocking(move || discovery::discover(root.as_deref(), &projects)).await {
        Ok(value) => value,
        Err(e) => return ScanReport {
            core_tools: unscanned_core_summaries(),
            tools: vec![],
            status: ScanRunStatus::Failed,
            warnings: vec![format!("扫描目录失败: {e}")],
        },
    };
    scan_discovered(managed_root, integrated, discovery).await
}

async fn scan_discovered(managed_root: Option<&Path>, integrated: &[PathBuf], discovery: Discovery) -> ScanReport {
    let mut out = Vec::new();

    // 分批并行(每批 8 个),避免进程风暴
    for batch in TOOLS.chunks(8) {
        let futs = batch.iter().map(|&(tool, exe, ver_args, timeout)| {
            let exe = exe.to_string();
            let ver_args: Vec<String> = ver_args.iter().map(|s| s.to_string()).collect();
            let discovery = &discovery;
            async move {
                let mut paths: Vec<(PathBuf, String, bool)> = Vec::new();
                for dir in &discovery.path {
                    if let Some(path) = find_executable(dir, &exe, discovery) {
                        let preferred = paths.is_empty();
                        paths.push((path, "PATH".into(), preferred));
                    }
                }
                for (dir, source) in &discovery.directories {
                    if let Some(path) = find_executable(dir, &exe, discovery) {
                        paths.push((path, source.clone(), false));
                    }
                }
                let mut seen: Vec<PathBuf> = Vec::new();
                let mut rows: Vec<(String, Result<String, String>, PathBuf, String, bool)> = Vec::new();
                // Entrypoints that belong to an installation already selected but
                // report a different version. They are not separate installations,
                // yet silently dropping the discrepancy would misreport the install.
                let mut conflicts: std::collections::HashMap<usize, Vec<crate::types::VersionConflict>> =
                    std::collections::HashMap::new();
                for (path, source, preferred) in paths {
                    let path = if tool == "Rust" {
                        rust_toolchain_executable(&path, discovery).await.unwrap_or(path)
                    } else { path };
                    let resolved = scan_identity(tool, &path);
                    let git_result = if tool == "Git" { Some(probe(tool, &path, &ver_args, timeout, discovery).await) } else { None };
                    if let Some(existing) = seen.iter().position(|p| paths_eq(p, &resolved)) {
                        // Same installation, another launcher: probe it so a version
                        // mismatch is reported instead of guessed away.
                        let probe_result = match git_result {
                            Some(result) => result,
                            None => probe(tool, &path, &ver_args, timeout, discovery).await,
                        };
                        let selected = rows[existing].1.clone();
                        let differs = match (&selected, &probe_result) {
                            (Ok(a), Ok(b)) => a != b,
                            (Ok(_), Err(_)) | (Err(_), Ok(_)) => true,
                            (Err(_), Err(_)) => false,
                        };
                        if differs {
                            let entry = conflicts.entry(existing).or_default();
                            let path_text = path.to_string_lossy().into_owned();
                            if !entry.iter().any(|item| item.path == path_text) {
                                entry.push(crate::types::VersionConflict {
                                    path: path_text,
                                    version: probe_result.as_ref().ok().cloned(),
                                    error: probe_result.as_ref().err().cloned(),
                                });
                            }
                        }
                        // 同一文件从项目目录和 PATH 同时发现时，项目来源优先，
                        // 防止纳入管理资格依赖遍历顺序。
                        if source == "项目目录" {
                            if let Some(row) = rows.get_mut(existing) {
                                row.3 = source.clone();
                                row.4 = false;
                            }
                        }
                        // Git exposes several launchers under one install root. Keep one
                        // installation record and prefer a working explicit launcher.
                        if tool == "Git" && probe_result.is_ok() {
                            if let Some(row) = rows.get_mut(existing).filter(|row| row.1.is_err()) {
                                row.1 = probe_result;
                                row.2 = path;
                                row.3 = source;
                                row.4 = row.4 || preferred;
                            }
                        }
                        continue;
                    }
                    seen.push(resolved);
                    let result = match git_result { Some(result) => result, None => probe(tool, &path, &ver_args, timeout, discovery).await };
                    rows.push((tool.to_string(), result, path, source, preferred));
                }
                (rows, conflicts)
            }
        });
        let results = futures_util::future::join_all(futs).await;

        for (rows, conflicts) in results {
            for (row_index, (tool, result, p, source, preferred)) in
                rows.into_iter().enumerate()
            {
            let row_conflicts = conflicts.get(&row_index).cloned().unwrap_or_default();
            let healthy = result.is_ok();
            let install = integrable_type(&tool).and_then(|t| install_root_for(t, &p));
            let env_type = integrable_type(&tool).filter(|&_t| {
                if !healthy { return false; }
                install.as_ref().is_some_and(|install| {
                    !managed_root.is_some_and(|root| path_within(root, install))
                        && !integrated.iter().any(|i| paths_eq(i, &install.canonicalize().unwrap_or_else(|_| install.clone())))
                        && source != "项目目录"
                        && !install.join("pyvenv.cfg").exists()
                        && !install.join("conda-meta").is_dir()
                })
            });
            let scan_status = status_for_probe(&result);
            let (version, error) = match result { Ok(v) => (Some(v), None), Err(e) => (None, Some(e)) };
            let path = p.to_string_lossy().to_string();
            let identity_path = p.canonicalize().ok().map(|v| v.to_string_lossy().to_string());
            out.push(ExternalEnv {
                tool,
                version,
                command: format!("\"{path}\""),
                path: Some(path), source,
                is_preferred: preferred, error, scan_status,
                env_type,
                install_root: install.map(|v| v.to_string_lossy().to_string()),
                identity_path,
                version_conflicts: row_conflicts,
            });
            }
        }
    }
    // Module invocation belongs to this interpreter even when no pip launcher exists.
    let pythons: Vec<_> = out.iter().filter(|row| row.tool == "Python")
        .filter_map(|row| row.path.as_ref().map(|p| (PathBuf::from(p), row.source.clone()))).collect();
    for batch in pythons.chunks(8) {
        let rows = futures_util::future::join_all(batch.iter().map(|(python, source)| async {
            let args = vec!["-m".into(), "pip".into(), "--version".into()];
            let result = probe("pip (Python 模块)", python, &args, Duration::from_secs(10), &discovery).await;
            let scan_status = status_for_probe(&result);
            let (version, error) = match result { Ok(v) => (Some(v), None), Err(e) => (None, Some(e)) };
            let path = python.to_string_lossy().to_string();
            ExternalEnv { tool: "pip (Python 模块)".into(), version, path: Some(path.clone()),
                command: format!("\"{path}\" -m pip"), source: source.clone(),
                is_preferred: false, error, scan_status, env_type: None,
                install_root: python.parent().map(|p| p.to_string_lossy().to_string()),
                identity_path: python.canonicalize().ok().map(|p| p.to_string_lossy().to_string()),
                version_conflicts: Vec::new() }
        })).await;
        out.extend(rows);
    }

    let mut warnings = discovery.warnings;
    warnings.extend(uv_layout_warnings(managed_root, &discovery.environment));
    // A same-installation version mismatch is a data-integrity observation, not a
    // failure: report it so the scan is not presented as fully consistent.
    for row in out.iter().filter(|row| !row.version_conflicts.is_empty()) {
        let listed = row
            .version_conflicts
            .iter()
            .take(3)
            .map(|conflict| match (&conflict.version, &conflict.error) {
                (Some(version), _) => format!("{} 报告 {version}", conflict.path),
                (None, Some(error)) => format!("{} 无法读取版本（{error}）", conflict.path),
                (None, None) => format!("{} 版本未知", conflict.path),
            })
            .collect::<Vec<_>>()
            .join("；");
        warnings.push(format!(
            "{} 的同一安装存在版本不一致的入口：{}（共 {} 个）；已保留一条安装记录，未合并为不同版本",
            row.tool,
            listed,
            row.version_conflicts.len()
        ));
    }
    let status = if warnings.is_empty() && out.iter().all(|row| matches!(row.scan_status, ScanStatus::Available)) {
        ScanRunStatus::Complete
    } else {
        ScanRunStatus::Partial
    };
    ScanReport {
        core_tools: summarize_core_tools(&out),
        tools: out,
        status,
        warnings,
    }
}

fn uv_layout_warnings(
    managed_root: Option<&Path>,
    environment: &std::collections::HashMap<String, String>,
) -> Vec<String> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(value) = environment.get("UV_TOOL_DIR") {
        let path = PathBuf::from(value);
        if path.is_absolute() {
            candidates.push(path);
        }
    }
    if let Some(root) = managed_root {
        candidates.push(root.join("globals/uv-global"));
    }
    if let Some(data) = environment.get("APPDATA") {
        let path = PathBuf::from(data).join("uv/tools");
        if path.is_absolute() {
            candidates.push(path);
        }
    }
    let mut seen: Vec<PathBuf> = Vec::new();
    let mut warnings = Vec::new();
    for path in candidates {
        if seen
            .iter()
            .any(|entry| crate::detect::system::paths_eq(entry, &path))
        {
            continue;
        }
        seen.push(path.clone());
        if !path.is_dir() || crate::detect::system::is_reparse_path(&path) {
            continue;
        }
        match crate::package_managers::inspect_uv_layout(&path) {
            Ok(layout) if layout.status != "clean" => {
                let recoverable = layout
                    .tools
                    .iter()
                    .filter(|tool| tool.needs_recovery && tool.recoverable)
                    .count();
                let blocked = layout
                    .tools
                    .iter()
                    .filter(|tool| tool.needs_recovery && !tool.recoverable)
                    .count();
                warnings.push(format!(
                    "uv 工具目录 {} 检测到旧嵌套布局：{} 个工具可规划恢复，{} 个需人工核对。请在包管理器页面确认后恢复；本次扫描未把它当作正常布局。",
                    path.display(),
                    recoverable,
                    blocked
                ));
            }
            Ok(_) => {}
            Err(error) => warnings.push(format!(
                "无法检查 uv 旧布局 {}: {error}",
                path.display()
            )),
        }
    }
    warnings
}

const CORE_TOOLS: &[(&str, &str)] = &[
    ("Git", "Git"),
    ("GitHub CLI", "GitHub CLI"),
    ("Node.js", "Node.js"),
    ("Python", "Python"),
    ("Java", "JDK"),
    ("Go", "Go"),
    ("Rust", "Rust"),
];

fn unscanned_core_summaries() -> Vec<ToolScanSummary> {
    CORE_TOOLS.iter()
        .map(|(_, label)| ToolScanSummary {
            tool: (*label).into(),
            status: ScanStatus::Unscanned,
            entry_count: 0,
            detail: Some("扫描未完成，未取得工具状态".into()),
        })
        .collect()
}

fn summarize_core_tools(rows: &[ExternalEnv]) -> Vec<ToolScanSummary> {
    CORE_TOOLS.iter().map(|(tool, label)| {
        let entries: Vec<_> = rows.iter().filter(|row| row.tool == *tool).collect();
        let status = if entries.is_empty() {
            ScanStatus::NotDiscovered
        } else if entries.iter().any(|row| row.scan_status == ScanStatus::Available) {
            ScanStatus::Available
        } else {
            [
                ScanStatus::PermissionDenied,
                ScanStatus::TimedOut,
                ScanStatus::StartFailed,
                ScanStatus::CommandFailed,
                ScanStatus::ParseFailed,
            ].into_iter().find(|status| entries.iter().any(|row| row.scan_status == *status))
                .unwrap_or(ScanStatus::NotDiscovered)
        };
        let detail = if status == ScanStatus::Available {
            None
        } else {
            let errors: Vec<_> = entries.iter().filter_map(|row| row.error.as_deref()).take(3).collect();
            if !errors.is_empty() {
                Some(errors.join("；"))
            } else if status == ScanStatus::NotDiscovered {
                Some(
                "在本次扫描范围（PATH、常见安装目录、受管目录及所选项目目录）内未发现；其他位置未作全盘扫描".into()
                )
            } else {
                None
            }
        };
        let entry_count = entries.iter().filter(|row| row.path.is_some()).count();
        ToolScanSummary { tool: (*label).into(), status, entry_count, detail }
    }).collect()
}

fn status_for_probe(result: &std::result::Result<String, String>) -> ScanStatus {
    match result {
        Ok(_) => ScanStatus::Available,
        Err(error) if error.contains("权限不足") || error.contains("拒绝访问") => ScanStatus::PermissionDenied,
        Err(error) if error.contains("超时") => ScanStatus::TimedOut,
        Err(error) if error.starts_with("无法启动") => ScanStatus::StartFailed,
        Err(error) if error.contains("退出码") => ScanStatus::CommandFailed,
        Err(_) => ScanStatus::ParseFailed,
    }
}

fn find_executable(dir: &Path, exe: &str, discovery: &Discovery) -> Option<PathBuf> {
    let extensions = discovery.environment.get("PATHEXT").map(String::as_str)
        .unwrap_or(".COM;.EXE;.BAT;.CMD");
    extensions.split(';').map(str::to_lowercase)
        .filter(|ext| matches!(ext.as_str(), ".exe" | ".com" | ".cmd" | ".bat"))
        .map(|ext| dir.join(format!("{exe}{ext}"))).find(|p| p.is_file())
}

async fn probe(tool: &str, exe: &Path, args: &[String], timeout: Duration, discovery: &Discovery) -> std::result::Result<String, String> {
    let mut command = Command::new(exe);
    command.args(args).envs(&discovery.environment)
        .current_dir(std::env::temp_dir())
        .env_remove("PYTHONHOME")
        .env_remove("PYTHONPATH")
        .env("COREPACK_ENABLE_NETWORK", "0")
        .env("PIP_DISABLE_PIP_VERSION_CHECK", "1")
        .kill_on_drop(true)
        .creation_flags(0x08000000)
        ;
    if let Some(dir) = exe.parent() {
        // Batch launchers can resolve their runtime relative to PATH.
        let mut paths = vec![dir.to_path_buf()];
        paths.extend(discovery.path.iter().cloned());
        if let Ok(path) = std::env::join_paths(paths) { command.env("PATH", path); }
        if dir.file_name().is_some_and(|n| n == "bin") {
            if let Some(home) = dir.parent().filter(|p| p.file_name().is_some_and(|n| n == "cargo-home")) {
                command.env("CARGO_HOME", home);
                if let Some(root) = home.parent() { command.env("RUSTUP_HOME", root.join("rustup-home")); }
            }
        }
    }
    let out = tokio::time::timeout(timeout, command.output()).await
        .map_err(|_| format!("版本探测超时（{} 秒）", timeout.as_secs()))?
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::PermissionDenied {
                format!("权限不足，无法启动: {e}")
            } else {
                format!("无法启动: {e}")
            }
        })?;
    if !out.status.success() {
        let message = String::from_utf8_lossy(&out.stderr);
        let fallback = String::from_utf8_lossy(&out.stdout);
        let detail = if message.trim().is_empty() { fallback.as_ref() } else { message.as_ref() };
        let detail = if detail.trim().is_empty() { "命令未提供错误输出" } else { detail };
        let code = out.status.code().unwrap_or(-1);
        if code == 5 || detail.to_lowercase().contains("access is denied") || detail.to_lowercase().contains("permission denied") {
            return Err(format!("权限不足（退出码 {code}）: {}", detail.chars().take(800).collect::<String>().trim()));
        }
        return Err(format!("命令退出码 {code}: {}", detail.chars().take(800).collect::<String>().trim()));
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    parse_version(tool, &format!("{stdout}\n{stderr}"))
        .ok_or_else(|| "版本命令输出无法解析".to_string())
}

fn parse_version(tool: &str, output: &str) -> Option<String> {
    if tool == "ADB" { return super::probe::adb_output_version(output); }
    let marker = match tool {
        "Java" => Some("version"),
        "Go" => Some("go version"),
        "Maven" => Some("Apache Maven"),
        "Gradle" => Some("Gradle "),
        "PHP" => Some("PHP "),
        "LLVM" => Some("clang"),
        "Git" => Some("git version"),
        "GitHub CLI" => Some("gh version"),
        "CMake" => Some("cmake version"),
        "Docker" => Some("Docker version"),
        _ => None,
    };
    if let Some(marker) = marker {
        if let Some(line) = output.lines().map(str::trim).find(|line| line.contains(marker)) {
            if let Some(version) = version_token(line) { return Some(version); }
        }
    }
    output.lines().map(str::trim).filter(|line| !line.is_empty()).find_map(version_token)
}

fn version_token(line: &str) -> Option<String> {
    for token in line.split_whitespace() {
        let Some(start) = token.find(|c: char| c.is_ascii_digit()) else { continue };
        let mut value = String::new();
        let mut dots = 0;
        for c in token[start..].chars() {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+') {
                if c == '.' { dots += 1; }
                value.push(c);
            } else { break; }
        }
        if dots >= 1 && !value.is_empty() {
            return Some(value.trim_end_matches(['.', '-', '+']).to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Discovery {
        Discovery { environment: std::env::vars().map(|(k,v)| (k.to_uppercase(),v)).collect(),
            path: std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()).collect(),
            directories: vec![], warnings: vec![] }
    }

    #[tokio::test]
    async fn adb_external_platform_tools_lifecycle_preserves_sdk() {
        let d = crate::test_support::TestDir::new();
        let sdk = d.path().join("Android SDK");
        let platform = sdk.join("platform-tools");
        std::fs::create_dir_all(&platform).unwrap();
        std::fs::write(sdk.join("keep-sdk.txt"), b"other components").unwrap();
        let exe = platform.join("adb.exe");
        std::fs::write(&exe, b"fixture").unwrap();
        assert!(install_root_for(EnvType::Adb, &exe).is_none());
        for name in ["AdbWinApi.dll", "AdbWinUsbApi.dll"] { std::fs::write(platform.join(name), b"fixture").unwrap(); }
        std::fs::write(platform.join("source.properties"), "Pkg.Revision=36.0.2\n").unwrap();
        assert_eq!(install_root_for(EnvType::Adb, &exe), Some(platform.clone()));
        assert!(!looks_like(EnvType::Adb, &sdk));
        let root = d.path().join("managed");
        let category = root.join("envs/adbs");
        std::fs::create_dir_all(&category).unwrap();
        let link = category.join("external");
        junction::create(&platform, &link).unwrap();
        crate::switcher::switch(&root, EnvType::Adb, "external").unwrap();
        assert_eq!(EnvType::Adb.path_entries(&root), vec![root.join("current/adb")]);
        let overview = crate::detect::scan_root(&root).await;
        let env = &overview.iter().find(|c| c.env_type == EnvType::Adb).unwrap().envs[0];
        assert_eq!(env.version.as_deref(), Some("36.0.2"));
        assert!(env.is_current && env.is_external_link);
        crate::switcher::remove_junction_for(&root, EnvType::Adb).unwrap();
        std::fs::remove_dir(&link).unwrap();
        assert!(exe.is_file() && sdk.join("keep-sdk.txt").is_file());
    }

    #[test]
    fn finds_windows_launcher_in_pathext_order() {
        let dir = crate::test_support::TestDir::new();
        std::fs::write(dir.path().join("npm"), b"unix").unwrap();
        std::fs::write(dir.path().join("npm.cmd"), b"cmd").unwrap();
        std::fs::write(dir.path().join("npm.exe"), b"exe").unwrap();
        let mut discovery = fixture();
        discovery.environment.insert("PATHEXT".into(), ".CMD;.EXE".into());
        assert_eq!(find_executable(dir.path(), "npm", &discovery), Some(dir.path().join("npm.cmd")));
    }

    #[test]
    fn rust_proxies_are_rejected_but_full_toolchains_can_be_integrated() {
        let d = crate::test_support::TestDir::new();
        std::fs::create_dir_all(d.path().join("bin")).unwrap();
        for exe in ["rustc.exe", "cargo.exe"] { std::fs::write(d.path().join("bin").join(exe), b"").unwrap(); }
        assert!(install_root_for(EnvType::Rust, &d.path().join("bin/rustc.exe")).is_none());
        std::fs::create_dir_all(d.path().join("lib/rustlib")).unwrap();
        assert!(install_root_for(EnvType::Rust, &d.path().join("bin/rustc.exe")).is_some());
        assert_eq!(integrable_type("Rust"), Some(EnvType::Rust));
    }

    #[tokio::test]
    async fn git_entrypoints_merge_distinct_installs_and_prefer_a_working_launcher() {
        let d = crate::test_support::TestDir::new();
        let first = d.path().join("git-one");
        let second = d.path().join("git-two");
        for root in [&first, &second] {
            for folder in ["cmd", "bin", "mingw64/bin", "mingw64/libexec/git-core"] {
                std::fs::create_dir_all(root.join(folder)).unwrap();
                std::fs::write(root.join(folder).join("git.cmd"), "@echo off\r\necho git version 2.50.0\r\n").unwrap();
            }
            std::fs::write(root.join("cmd/git.exe"), b"structural fixture").unwrap();
        }
        let mut discovery = fixture();
        discovery.environment.insert("PATHEXT".into(), ".CMD;.EXE".into());
        discovery.path = vec![first.join("cmd"), first.join("bin"), first.join("mingw64/bin"), first.join("mingw64/libexec/git-core"), second.join("cmd")];
        discovery.directories = vec![(first.join("cmd"), "常见安装".into())];
        let report = scan_discovered(None, &[], discovery).await;
        let git: Vec<_> = report.tools.iter().filter(|r| r.tool == "Git").collect();
        assert_eq!(git.len(), 2);
        assert!(git[0].is_preferred);
        for folder in ["cmd", "bin"] {
            std::fs::write(first.join(folder).join("git.cmd"), "@echo off\r\necho unavailable 1>&2\r\nexit /b 3\r\n").unwrap();
        }
        let mut discovery = fixture();
        discovery.environment.insert("PATHEXT".into(), ".CMD;.EXE".into());
        discovery.path = vec![first.join("cmd"), first.join("bin")];
        let report = scan_discovered(None, &[], discovery).await;
        let git: Vec<_> = report.tools.iter().filter(|r| r.tool == "Git").collect();
        assert_eq!(git.len(), 1);
        assert!(git[0].error.is_some());
        assert_eq!(git[0].scan_status, ScanStatus::CommandFailed);
    }

    #[tokio::test]
    async fn core_summary_marks_only_scanned_range_without_claiming_global_absence() {
        let report = scan_discovered(None, &[], Discovery {
            environment: std::collections::HashMap::new(),
            path: vec![],
            directories: vec![],
            warnings: vec![],
        }).await;
        assert!(report.tools.is_empty());
        assert!(matches!(report.status, ScanRunStatus::Complete));
        let git = report.core_tools.iter().find(|row| row.tool == "Git").unwrap();
        assert!(matches!(git.status, ScanStatus::NotDiscovered));
        assert_eq!(git.entry_count, 0);
        assert!(git.detail.as_deref().unwrap().contains("扫描范围"));
    }

    #[tokio::test]
    async fn scan_warns_about_legacy_uv_layouts_seen_in_the_effective_environment() {
        let dir = crate::test_support::TestDir::new();
        let root = dir.path().join("uv-tools");
        std::fs::create_dir_all(root.join("bin")).unwrap();
        std::fs::create_dir_all(root.join("cache")).unwrap();
        let environment = std::collections::HashMap::from([
            ("UV_TOOL_DIR".into(), root.to_string_lossy().into_owned()),
            ("PATHEXT".into(), ".CMD;.EXE".into()),
        ]);
        let report = scan_discovered(None, &[], Discovery {
            environment,
            path: vec![],
            directories: vec![],
            warnings: vec![],
        }).await;
        assert!(matches!(report.status, ScanRunStatus::Partial));
        assert!(report
            .warnings
            .iter()
            .any(|warning| warning.contains("uv 工具目录") && warning.contains("旧嵌套布局")));

        let clean = dir.path().join("uv-clean");
        std::fs::create_dir_all(clean.join("tool/Lib/site-packages")).unwrap();
        let environment = std::collections::HashMap::from([
            ("UV_TOOL_DIR".into(), clean.to_string_lossy().into_owned()),
            ("PATHEXT".into(), ".CMD;.EXE".into()),
        ]);
        let report = scan_discovered(None, &[], Discovery {
            environment,
            path: vec![],
            directories: vec![],
            warnings: vec![],
        }).await;
        assert!(!report
            .warnings
            .iter()
            .any(|warning| warning.contains(&clean.to_string_lossy().to_string())));
    }

    #[tokio::test]
    async fn same_installation_entrypoints_with_different_versions_are_reported() {
        let dir = crate::test_support::TestDir::new();
        let root = dir.path().join("shared-node");
        std::fs::create_dir_all(root.join("bin")).unwrap();
        // Two launchers under one install root report different versions.
        std::fs::write(root.join("node.exe"), b"fixture marker").unwrap();
        std::fs::write(root.join("node.cmd"), "@echo off\r\necho v22.1.0\r\n").unwrap();
        std::fs::write(root.join("bin/node.cmd"), "@echo off\r\necho v20.0.0\r\n").unwrap();
        let mut discovery = Discovery {
            environment: std::collections::HashMap::from([("PATHEXT".into(), ".CMD;.EXE".into())]),
            path: vec![root.clone()],
            directories: vec![(root.join("bin"), "常见安装".into())],
            warnings: vec![],
        };
        discovery.environment.insert("PATH".into(), String::new());
        let report = scan_discovered(None, &[], discovery).await;

        let nodes: Vec<_> = report.tools.iter().filter(|row| row.tool == "Node.js").collect();
        // One installation record, as before.
        assert_eq!(nodes.len(), 1, "same install must stay a single record");
        let conflicts = &nodes[0].version_conflicts;
        assert_eq!(conflicts.len(), 1, "the other launcher differs: {conflicts:?}");
        assert!(conflicts[0].path.ends_with("bin\\node.cmd") || conflicts[0].path.ends_with("bin/node.cmd"));
        assert_eq!(conflicts[0].version.as_deref(), Some("20.0.0"));
        // The scan must not present itself as fully consistent.
        assert!(matches!(report.status, ScanRunStatus::Partial));
        assert!(report
            .warnings
            .iter()
            .any(|warning| warning.contains("版本不一致") && warning.contains("Node.js")));
    }

    #[tokio::test]
    async fn identical_versions_across_entrypoints_produce_no_conflict() {
        let dir = crate::test_support::TestDir::new();
        let root = dir.path().join("shared-node");
        std::fs::create_dir_all(root.join("bin")).unwrap();
        std::fs::write(root.join("node.exe"), b"fixture marker").unwrap();
        std::fs::write(root.join("node.cmd"), "@echo off\r\necho v22.1.0\r\n").unwrap();
        std::fs::write(root.join("bin/node.cmd"), "@echo off\r\necho v22.1.0\r\n").unwrap();
        let mut discovery = Discovery {
            environment: std::collections::HashMap::from([("PATHEXT".into(), ".CMD;.EXE".into())]),
            path: vec![root.clone()],
            directories: vec![(root.join("bin"), "常见安装".into())],
            warnings: vec![],
        };
        discovery.environment.insert("PATH".into(), String::new());
        let report = scan_discovered(None, &[], discovery).await;
        let node = report.tools.iter().find(|row| row.tool == "Node.js").unwrap();
        assert!(node.version_conflicts.is_empty(), "{:?}", node.version_conflicts);
        assert!(!report.warnings.iter().any(|warning| warning.contains("版本不一致")));
    }

    #[tokio::test]
    async fn core_tool_aliases_dedupe_by_installation_but_keep_distinct_roots() {
        let dir = crate::test_support::TestDir::new();
        let shared = dir.path().join("shared-node");
        let separate = dir.path().join("separate-node");
        for root in [&shared, &separate] {
            std::fs::create_dir_all(root.join("bin")).unwrap();
            std::fs::write(root.join("node.exe"), b"fixture marker").unwrap();
            std::fs::write(root.join("node.cmd"), "@echo off\r\necho v22.1.0\r\n").unwrap();
        }
        std::fs::write(shared.join("bin/node.cmd"), "@echo off\r\necho v22.1.0\r\n").unwrap();
        let mut discovery = Discovery {
            environment: std::collections::HashMap::from([("PATHEXT".into(), ".CMD;.EXE".into())]),
            path: vec![shared.clone()],
            directories: vec![
                (shared.join("bin"), "常见安装".into()),
                (separate.clone(), "另一安装".into()),
            ],
            warnings: vec![],
        };
        discovery.environment.insert("PATH".into(), String::new());
        let report = scan_discovered(None, &[], discovery).await;
        let nodes: Vec<_> = report.tools.iter().filter(|row| row.tool == "Node.js").collect();
        assert_eq!(nodes.len(), 2);
        assert!(nodes.iter().all(|row| row.scan_status == ScanStatus::Available));
        let summary = report.core_tools.iter().find(|row| row.tool == "Node.js").unwrap();
        assert!(matches!(summary.status, ScanStatus::Available));
        assert_eq!(summary.entry_count, 2);
        assert!(nodes.iter().any(|row| row.install_root.as_deref() == Some(shared.to_string_lossy().as_ref())));
        assert!(nodes.iter().any(|row| row.install_root.as_deref() == Some(separate.to_string_lossy().as_ref())));
    }

    #[test]
    fn core_tool_install_identity_uses_canonical_install_roots() {
        let dir = crate::test_support::TestDir::new();
        let cases = [
            ("Git", "gits", "cmd/git.exe", &["cmd/git.exe", "bin/git.exe"][..]),
            ("GitHub CLI", "gh", "bin/gh.exe", &["bin/gh.exe"][..]),
            ("Node.js", "node", "node.exe", &["node.exe", "bin/node.exe"][..]),
            ("Python", "python", "python.exe", &["python.exe"][..]),
            ("Java", "jdk", "bin/java.exe", &["bin/java.exe"][..]),
            ("Go", "go", "bin/go.exe", &["bin/go.exe"][..]),
            ("Rust", "rust", "bin/rustc.exe", &["bin/rustc.exe"][..]),
        ];
        for (tool, name, marker, aliases) in cases {
            let root = dir.path().join(name);
            std::fs::create_dir_all(root.join("bin")).unwrap();
            if tool == "Git" {
                std::fs::create_dir_all(root.join("cmd")).unwrap();
                std::fs::write(root.join(marker), b"").unwrap();
            } else if tool == "GitHub CLI" {
                std::fs::create_dir_all(root.join("bin")).unwrap();
                std::fs::write(root.join(marker), b"").unwrap();
            } else if tool == "Python" || tool == "Node.js" {
                std::fs::write(root.join(marker), b"").unwrap();
            } else if tool == "Java" {
                std::fs::write(root.join("bin/java.exe"), b"").unwrap();
                std::fs::write(root.join("bin/javac.exe"), b"").unwrap();
            } else if tool == "Go" {
                std::fs::write(root.join("bin/go.exe"), b"").unwrap();
            } else {
                std::fs::write(root.join("bin/rustc.exe"), b"").unwrap();
                std::fs::write(root.join("bin/cargo.exe"), b"").unwrap();
                std::fs::create_dir_all(root.join("lib/rustlib")).unwrap();
            }
            let expected = root.canonicalize().unwrap();
            let identities: Vec<_> = aliases.iter().map(|alias| scan_identity(tool, &root.join(alias))).collect();
            assert!(identities.iter().all(|identity| paths_eq(identity, &expected)), "{tool}: {identities:?}");
        }
    }

    #[test]
    fn path_comparisons_normalize_case_trailing_separators_and_junctions() {
        let dir = crate::test_support::TestDir::new();
        let root = dir.path().join("Globals/uv-global");
        let target = dir.path().join("Globals/uv-global-bin");
        let link = dir.path().join("Links/UV-BIN");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&target).unwrap();
        std::fs::create_dir_all(link.parent().unwrap()).unwrap();
        junction::create(&target, &link).unwrap();

        let upper_root = PathBuf::from(format!("{}\\", root.to_string_lossy().to_uppercase()));
        let upper_root_again = PathBuf::from(format!("{}\\\\", root.to_string_lossy().to_uppercase()));
        assert!(paths_eq(&root, &upper_root));
        assert!(paths_eq(&upper_root, &upper_root_again));
        assert!(paths_eq(&target, &link));
        assert!(path_within(&root, &root.join("missing/child")));
        assert!(!path_within(&root, &target));
    }

    /// Path edge cases that real Windows layouts actually produce.
    ///
    /// The existing test above covers case, trailing separators and junctions.
    /// These three are the ones that silently break path comparisons in
    /// practice, and none of them were covered:
    ///
    /// - **Space** in a directory name: `C:\Program Files\EnvCon`. A space must
    ///   not change identity, and must not be trimmed by the key function.
    /// - **Unicode**: Chinese user directories (`C:\Users\张三`) are common on
    ///   this product's target platform. `to_string_lossy` in `path_key` could
    ///   mangle them, so this asserts the round trip stays exact.
    /// - **Long path** (> 260 chars): without the `\\?\` prefix Windows rejects
    ///   these, and `path_key` strips that prefix. Creating one proves the
    ///   comparison still works when canonicalize returns the extended form.
    ///
    /// Each case is checked for both directions of `path_within`, because a
    /// false positive there would let a cleanup delete a sibling directory.
    #[test]
    fn path_comparisons_survive_spaces_unicode_and_long_paths() {
        let dir = crate::test_support::TestDir::new();

        // --- Space in the name ---------------------------------------------
        let spaced = dir.path().join("Program Files").join("EnvCon Home");
        std::fs::create_dir_all(&spaced).unwrap();
        let spaced_child = spaced.join("bin");
        std::fs::create_dir_all(&spaced_child).unwrap();

        assert!(paths_eq(&spaced, &dir.path().join("Program Files").join("EnvCon Home")));
        assert!(path_within(&spaced, &spaced_child));
        assert!(!path_within(&spaced_child, &spaced));
        // Windows trims trailing spaces at the filesystem API level, so an
        // unescaped trailing space resolves to the same directory. This is
        // measured behaviour, not a guess: creating "EnvCon Home " yields a
        // directory whose canonical name has no trailing space. The comparison
        // must therefore treat them as equal rather than inventing a distinct
        // directory that the OS itself would never create.
        let trailing_space = dir.path().join("Program Files").join("EnvCon Home ");
        assert!(
            paths_eq(&spaced, &trailing_space),
            "Windows trims trailing spaces, so both names resolve to one directory"
        );

        // --- Unicode --------------------------------------------------------
        let unicode_root = dir.path().join("用户").join("开发环境");
        let unicode_child = unicode_root.join("工具");
        std::fs::create_dir_all(&unicode_child).unwrap();

        assert!(paths_eq(&unicode_root, &dir.path().join("用户").join("开发环境")));
        assert!(path_within(&unicode_root, &unicode_child));
        assert!(!path_within(&unicode_child, &unicode_root));
        // Case folding must not corrupt non-ASCII bytes.
        assert!(path_key(&unicode_root).contains("用户"));
        assert!(!path_key(&unicode_root).contains('\u{FFFD}'), "no replacement characters");

        // --- Long path ------------------------------------------------------
        // Build a path past 260 chars. Each segment is short so no single
        // component exceeds MAX_PATH on its own.
        let mut deep = dir.path().to_path_buf();
        for i in 0..20 {
            deep.push(format!("segment-{i:02}-padding-padding"));
        }
        std::fs::create_dir_all(&deep).unwrap();
        assert!(
            deep.as_os_str().len() > 260,
            "fixture must exceed MAX_PATH, got {}",
            deep.as_os_str().len()
        );
        let deep_child = deep.join("leaf");
        std::fs::create_dir_all(&deep_child).unwrap();

        assert!(paths_eq(&deep, &deep.clone()));
        assert!(path_within(&deep, &deep_child));
        assert!(!path_within(&deep_child, &deep));
        // The `\\?\` prefix canonicalize adds must not leak into the key.
        assert!(
            !path_key(&deep).starts_with("\\\\?\\"),
            "extended-length prefix must be stripped from the comparison key"
        );

        // A sibling whose name merely shares the prefix is NOT within the root.
        // This is the false positive that would let a cleanup escape its scope.
        let lookalike = dir.path().join("segment-00-padding-padding");
        std::fs::create_dir_all(&lookalike).unwrap();
        assert!(
            !path_within(&deep, &lookalike),
            "a name sharing a text prefix is not a descendant"
        );
    }

    #[test]
    fn probe_failures_map_to_structured_scan_statuses() {
        assert!(matches!(status_for_probe(&Ok("1.2.3".into())), ScanStatus::Available));
        assert!(matches!(status_for_probe(&Err("权限不足（退出码 5）".into())), ScanStatus::PermissionDenied));
        assert!(matches!(status_for_probe(&Err("无法启动: missing".into())), ScanStatus::StartFailed));
        assert!(matches!(status_for_probe(&Err("命令退出码 1: bad".into())), ScanStatus::CommandFailed));
        assert!(matches!(status_for_probe(&Err("版本命令输出无法解析".into())), ScanStatus::ParseFailed));
        assert!(matches!(status_for_probe(&Err("版本探测超时（5 秒）".into())), ScanStatus::TimedOut));
    }

    #[tokio::test]
    async fn probes_exact_batch_path_and_rejects_error_output() {
        let dir = crate::test_support::TestDir::new();
        let first = dir.path().join("first.cmd");
        let second = dir.path().join("second.cmd");
        std::fs::write(&first, "@echo off\r\necho 10.1.0\r\n").unwrap();
        std::fs::write(&second, "@echo off\r\necho not installed 1>&2\r\nexit /b 1\r\n").unwrap();
        assert_eq!(probe("test", &first, &["--version".into()], Duration::from_secs(5), &fixture()).await.unwrap(), "10.1.0");
        let error = probe("test", &second, &[], Duration::from_secs(5), &fixture()).await.unwrap_err();
        assert!(error.contains("not installed"));
        assert!(error.contains("退出码 1"));
    }

    #[tokio::test]
    async fn reports_start_failure_timeout_and_empty_output() {
        let dir = crate::test_support::TestDir::new();
        let discovery = fixture();
        assert!(probe("test", &dir.path().join("missing.exe"), &[], Duration::from_secs(2), &discovery)
            .await.unwrap_err().contains("无法启动"));
        let empty = dir.path().join("empty.cmd");
        std::fs::write(&empty, "@echo off\r\nexit /b 0\r\n").unwrap();
        assert!(probe("test", &empty, &[], Duration::from_secs(2), &discovery)
            .await.unwrap_err().contains("无法解析"));
        let shell = PathBuf::from(std::env::var("SystemRoot").unwrap()).join("System32/WindowsPowerShell/v1.0/powershell.exe");
        assert!(probe("test", &shell, &["-NoProfile".into(), "-Command".into(), "Start-Sleep -Seconds 10".into()], Duration::from_millis(200), &discovery)
            .await.unwrap_err().contains("超时"));
    }

    #[tokio::test]
    async fn python_module_probe_passes_exact_arguments_without_pip_launcher() {
        let dir = crate::test_support::TestDir::new();
        let python = dir.path().join("python.cmd");
        std::fs::write(&python, "@echo off\r\nif not \"%1\"==\"-m\" exit /b 2\r\nif not \"%2\"==\"pip\" exit /b 3\r\nif not \"%3\"==\"--version\" exit /b 4\r\necho pip 26.0 from isolated-env\r\n").unwrap();
        let version = probe("pip (Python 模块)", &python, &["-m".into(), "pip".into(), "--version".into()], Duration::from_secs(5), &fixture()).await.unwrap();
        assert_eq!(version, "26.0");
    }

    #[test]
    fn parses_warning_prefixed_java_output() {
        let output = "Picked up JAVA_TOOL_OPTIONS: -Dfile.encoding=UTF-8\nopenjdk version \"21.0.4\" 2024-07-16";
        assert_eq!(parse_version("Java", output).as_deref(), Some("21.0.4"));
    }

    #[test]
    fn parses_gradle_banner_and_prefers_marked_line() {
        let output = "------------------------------------------------------------\nGradle 8.10.2\n------------------------------------------------------------";
        assert_eq!(parse_version("Gradle", output).as_deref(), Some("8.10.2"));
    }

    #[test]
    fn parses_pip_and_rejects_unparseable_output() {
        assert_eq!(parse_version("pip", "pip 26.0 from C:\\env\\Lib\\site-packages").as_deref(), Some("26.0"));
        assert!(parse_version("pip", "pip is unavailable").is_none());
    }

    #[tokio::test]
    async fn scan_uses_supplied_path_order_and_finds_pip_only_as_python_module() {
        let dir = crate::test_support::TestDir::new();
        let first = dir.path().join("first");
        let second = dir.path().join("project/.venv/Scripts");
        for root in [&first, &second] {
            std::fs::create_dir_all(root).unwrap();
            std::fs::write(root.join("python.cmd"), "@echo off\r\nif \"%1\"==\"-m\" (echo pip 26.0) else (echo Python 3.14)\r\n").unwrap();
        }
        let mut discovery = fixture();
        discovery.environment.insert("PATHEXT".into(), ".CMD;.EXE".into());
        discovery.path = vec![first.clone()];
        discovery.directories = vec![(second.clone(), "项目目录".into()), (first.clone(), "常见安装".into())];
        let report = scan_discovered(None, &[], discovery).await;
        let pythons: Vec<_> = report.tools.iter().filter(|r| r.tool == "Python").collect();
        assert_eq!(pythons.len(), 2);
        assert!(pythons[0].is_preferred);
        assert!(!pythons[1].is_preferred);
        assert_eq!(pythons[1].source, "项目目录");
        let modules: Vec<_> = report.tools.iter().filter(|r| r.tool == "pip (Python 模块)").collect();
        assert_eq!(modules.len(), 2);
        assert!(modules.iter().all(|r| r.version.as_deref() == Some("26.0") && !r.is_preferred));
        let mut refreshed = fixture();
        refreshed.environment.insert("PATHEXT".into(), ".CMD;.EXE".into());
        refreshed.path = vec![second.clone(), first.clone()];
        let report = scan_discovered(None, &[], refreshed).await;
        let preferred = report.tools.iter().find(|r| r.tool == "Python" && r.is_preferred).unwrap();
        assert_eq!(preferred.path.as_deref(), Some(second.join("python.cmd").to_string_lossy().as_ref()));
    }

    #[test]
    fn discovers_packages_without_path_and_skips_failed_installs() {
        let dir = crate::test_support::TestDir::new();
        for folder in ["envs/nodes/node-22", "envs/nodes/.failed-node-22", "envs/pythons/python-3/Scripts"] {
            std::fs::create_dir_all(dir.path().join(folder)).unwrap();
        }
        let npm = dir.path().join("envs/nodes/node-22/npm.cmd");
        let pip = dir.path().join("envs/pythons/python-3/Scripts/pip.exe");
        std::fs::write(&npm, b"").unwrap();
        std::fs::write(&pip, b"").unwrap();
        std::fs::write(dir.path().join("envs/nodes/.failed-node-22/npm.cmd"), b"").unwrap();
        let mut discovery = fixture();
        discovery.walk(&dir.path().join("envs"), 5, "EnvCon", true);
        let npm_found: Vec<_> = discovery.directories.iter().filter_map(|(p,_)| find_executable(p, "npm", &discovery)).collect();
        let pip_found: Vec<_> = discovery.directories.iter().filter_map(|(p,_)| find_executable(p, "pip", &discovery)).collect();
        assert_eq!(npm_found, vec![npm]);
        assert_eq!(pip_found, vec![pip]);
    }

    #[tokio::test]
    #[ignore = "scans tools installed on the host"]
    async fn host_scan() {
        let results = scan_system(None, &[], &[]).await;
        println!("warnings: {:?}", results.warnings);
        for row in results.tools.iter().filter(|row| matches!(row.tool.as_str(), "Rust" | "Git" | "Cargo")) {
            println!("{} | {:?} | {:?} | preferred={} | error={:?}", row.tool, row.version, row.path, row.is_preferred, row.error);
            println!("integrable={:?} root={:?}", row.env_type, row.install_root);
        }
        if let Some(rust) = results.tools.iter().find(|row| row.env_type == Some(EnvType::Rust)) {
            let dir = crate::test_support::TestDir::new();
            let link = dir.path().join("envs/rusts/external");
            std::fs::create_dir_all(link.parent().unwrap()).unwrap();
            junction::create(rust.install_root.as_ref().unwrap(), &link).unwrap();
            crate::switcher::switch(dir.path(), EnvType::Rust, "external").unwrap();
            let overview = crate::detect::scan_root(dir.path()).await;
            let row = &overview.iter().find(|c| c.env_type == EnvType::Rust).unwrap().envs[0];
            assert_eq!(row.status, "available");
            assert_eq!(row.version, rust.version);
            assert!(row.is_external_link && row.is_current);
            crate::switcher::remove_junction_for(dir.path(), EnvType::Rust).unwrap();
            std::fs::remove_dir(link).unwrap();
            assert!(Path::new(rust.install_root.as_ref().unwrap()).join("bin/rustc.exe").is_file());
            println!("host Rust temporary link / overview / switch / unlink verified");
        }
    }
}
