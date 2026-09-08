use std::path::{Path, PathBuf};
use std::time::Duration;

use tokio::process::Command;

use crate::types::{EnvType, ExternalEnv};

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
    ("uv", "uv", &["--version"], Duration::from_secs(5)),
    ("Poetry", "poetry", &["--version"], Duration::from_secs(10)),
    ("Cargo", "cargo", &["--version"], Duration::from_secs(8)),
    ("Composer", "composer", &["--version"], Duration::from_secs(8)),
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
    let has = |p: PathBuf| p.exists();
    match env_type {
        EnvType::Jdk => has(root.join("bin").join("java.exe")),
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
fn paths_eq(a: &Path, b: &Path) -> bool {
    let norm = |p: &Path| {
        p.to_string_lossy()
            .to_lowercase()
            .replace('/', "\\")
            .trim_end_matches('\\')
            .to_string()
    };
    norm(a) == norm(b)
}

/// 扫描系统散装环境:where.exe 定位 + 版本探测(并行执行,跳过管理器目录内与已纳入管理的)
pub async fn scan_system(managed_root: Option<&Path>, integrated: &[PathBuf]) -> Vec<ExternalEnv> {
    let mut out: Vec<ExternalEnv> = Vec::new();

    // 分批并行(每批 8 个),避免进程风暴
    for batch in TOOLS.chunks(8) {
        let futs = batch.iter().map(|&(tool, exe, ver_args, timeout)| {
            let exe = exe.to_string();
            let ver_args: Vec<String> = ver_args.iter().map(|s| s.to_string()).collect();
            async move {
                // where.exe 定位(优先级最高的第一个)
                let found = Command::new("where.exe")
                    .arg(&exe)
                    .creation_flags(0x08000000)
                    .output()
                    .await
                    .ok()
                    .filter(|o| o.status.success())
                    .map(|o| String::from_utf8_lossy(&o.stdout).to_string());
                let path = found.and_then(|out| {
                    out.lines()
                        .map(|l| l.trim())
                        .find(|l| !l.is_empty())
                        .map(|l| l.to_string())
                })?;
                let version = run_version(&exe, &ver_args, timeout).await;
                Some((tool.to_string(), version, path))
            }
        });
        let results = futures_util::future::join_all(futs).await;

        for (tool, version, path) in results.into_iter().flatten() {
            let p = PathBuf::from(&path);
            // 跳过管理器目录内的(受管理环境不重复列出)
            if let Some(root) = managed_root {
                if p.starts_with(root) {
                    continue;
                }
            }
            let env_type = integrable_type(&tool);
            // 已通过链接纳入 envs\ 管理的不再列出
            if let Some(t) = env_type {
                if let Some(install_root) = install_root_for(t, &p) {
                    if integrated.iter().any(|i| paths_eq(i, &install_root)) {
                        continue;
                    }
                }
            }
            out.push(ExternalEnv {
                tool,
                version,
                path: Some(path),
                source: "PATH".to_string(),
                env_type,
            });
        }
    }
    out
}

async fn run_version(exe: &str, args: &[String], timeout: Duration) -> Option<String> {
    let fut = Command::new(exe)
        .args(args)
        .creation_flags(0x08000000)
        .output();
    let out = tokio::time::timeout(timeout, fut).await.ok()?.ok()?;
    let s = if out.stdout.is_empty() {
        String::from_utf8_lossy(&out.stderr).to_string()
    } else {
        String::from_utf8_lossy(&out.stdout).to_string()
    };
    let first = s.lines().next()?.trim();
    Some(first.to_string())
}
