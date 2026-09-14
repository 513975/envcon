use std::path::{Path, PathBuf};
use std::time::Duration;

use tokio::process::Command;

use crate::types::{EnvType, ExternalEnv, ScanReport};
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
];

/// 工具名 → 可纳入管理的环境类型(结构不兼容的不支持)
fn integrable_type(tool: &str) -> Option<EnvType> {
    match tool {
        "Java" => Some(EnvType::Jdk),
        "Python" => Some(EnvType::Python),
        "Node.js" => Some(EnvType::Node),
        "Go" => Some(EnvType::Go),
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
        // rustup 代理结构(cargo\bin\rustc.exe)与受管结构不兼容
        EnvType::Rust => false,
    }
}

/// 从 where.exe 定位到的可执行文件推导安装根目录
pub fn install_root_for(env_type: EnvType, exe: &Path) -> Option<PathBuf> {
    let dir = exe.parent()?;
    let dir_name = dir.file_name().map(|n| n.to_string_lossy().to_lowercase());
    let mut candidates: Vec<PathBuf> = Vec::new();
    match dir_name.as_deref() {
        // exe 位于 bin\ / cmd\ 下:安装根目录一般是其父级
        Some("bin") | Some("cmd") => {
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

pub(crate) fn paths_eq(a: &Path, b: &Path) -> bool {
    path_key(a) == path_key(b)
}

pub(crate) fn path_within(root: &Path, candidate: &Path) -> bool {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let candidate = candidate.canonicalize().unwrap_or_else(|_| candidate.to_path_buf());
    let root_key = path_key(&root);
    let candidate_key = path_key(&candidate);
    candidate_key == root_key
        || candidate_key.strip_prefix(&root_key).is_some_and(|rest| rest.starts_with('\\'))
}

/// Refresh persisted PATH, then discover installed and selected project tools.
pub async fn scan_system(managed_root: Option<&Path>, integrated: &[PathBuf], projects: &[PathBuf]) -> ScanReport {
    let root = managed_root.map(Path::to_path_buf);
    let projects = projects.to_vec();
    let discovery = match tokio::task::spawn_blocking(move || discovery::discover(root.as_deref(), &projects)).await {
        Ok(value) => value,
        Err(e) => return ScanReport { tools: vec![], warnings: vec![format!("扫描目录失败: {e}")] },
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
                for (path, source, preferred) in paths {
                    let resolved = if tool == "Python" {
                        path.parent().and_then(|p| p.canonicalize().ok())
                            .map(|p| p.join(path.file_name().unwrap())).unwrap_or_else(|| path.clone())
                    } else { path.canonicalize().unwrap_or_else(|_| path.clone()) };
                    if let Some(existing) = seen.iter().position(|p| paths_eq(p, &resolved)) {
                        // 同一文件从项目目录和 PATH 同时发现时，项目来源优先，
                        // 防止纳入管理资格依赖遍历顺序。
                        if source == "项目目录" {
                            if let Some(row) = rows.get_mut(existing) {
                                row.3 = source.clone();
                                row.4 = false;
                            }
                        }
                        continue;
                    }
                    seen.push(resolved);
                    let result = probe(tool, &path, &ver_args, timeout, discovery).await;
                    rows.push((tool.to_string(), result, path, source, preferred));
                }
                rows
            }
        });
        let results = futures_util::future::join_all(futs).await;

        for (tool, result, p, source, preferred) in results.into_iter().flatten() {
            let healthy = result.is_ok();
            let install = integrable_type(&tool).and_then(|t| install_root_for(t, &p));
            let env_type = integrable_type(&tool).filter(|&_t| {
                if !healthy { return false; }
                install.as_ref().is_some_and(|install| {
                    !managed_root.is_some_and(|root| path_within(root, install))
                        && !integrated.iter().any(|i| paths_eq(i, &install))
                        && source != "项目目录"
                        && !install.join("pyvenv.cfg").exists()
                        && !install.join("conda-meta").is_dir()
                })
            });
            let (version, error) = match result { Ok(v) => (Some(v), None), Err(e) => (None, Some(e)) };
            let path = p.to_string_lossy().to_string();
            let identity_path = p.canonicalize().ok().map(|v| v.to_string_lossy().to_string());
            out.push(ExternalEnv {
                tool,
                version,
                command: format!("\"{path}\""),
                path: Some(path), source,
                is_preferred: preferred, error,
                env_type,
                install_root: install.map(|v| v.to_string_lossy().to_string()),
                identity_path,
            });
        }
    }
    // Module invocation belongs to this interpreter even when no pip launcher exists.
    let pythons: Vec<_> = out.iter().filter(|row| row.tool == "Python")
        .filter_map(|row| row.path.as_ref().map(|p| (PathBuf::from(p), row.source.clone()))).collect();
    for batch in pythons.chunks(8) {
        let rows = futures_util::future::join_all(batch.iter().map(|(python, source)| async {
            let args = vec!["-m".into(), "pip".into(), "--version".into()];
            let result = probe("pip (Python 模块)", python, &args, Duration::from_secs(10), &discovery).await;
            let (version, error) = match result { Ok(v) => (Some(v), None), Err(e) => (None, Some(e)) };
            let path = python.to_string_lossy().to_string();
            ExternalEnv { tool: "pip (Python 模块)".into(), version, path: Some(path.clone()),
                command: format!("\"{path}\" -m pip"), source: source.clone(),
                is_preferred: false, error, env_type: None,
                install_root: python.parent().map(|p| p.to_string_lossy().to_string()),
                identity_path: python.canonicalize().ok().map(|p| p.to_string_lossy().to_string()) }
        })).await;
        out.extend(rows);
    }
    ScanReport { tools: out, warnings: discovery.warnings }
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
        .map_err(|e| format!("无法启动: {e}"))?;
    if !out.status.success() {
        let message = String::from_utf8_lossy(&out.stderr);
        let fallback = String::from_utf8_lossy(&out.stdout);
        let detail = if message.trim().is_empty() { fallback.as_ref() } else { message.as_ref() };
        let detail = if detail.trim().is_empty() { "命令未提供错误输出" } else { detail };
        return Err(format!("命令退出码 {}: {}", out.status.code().unwrap_or(-1), detail.chars().take(800).collect::<String>().trim()));
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    parse_version(tool, &format!("{stdout}\n{stderr}"))
        .ok_or_else(|| "版本命令输出无法解析".to_string())
}

fn parse_version(tool: &str, output: &str) -> Option<String> {
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
        for row in results.tools.iter().filter(|row| matches!(row.tool.as_str(), "npm" | "pip" | "pip (Python 模块)" | "Conda")) {
            println!("{} | {:?} | {:?} | preferred={} | error={:?}", row.tool, row.version, row.path, row.is_preferred, row.error);
        }
    }
}
