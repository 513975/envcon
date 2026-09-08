use std::path::{Path, PathBuf};
use std::time::Duration;

use futures_util::StreamExt;
use tokio::process::Command;

use crate::types::ExternalEnv;

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

/// 扫描系统散装环境:where.exe 定位 + 版本探测(并行执行,跳过管理器目录内的)
pub async fn scan_system(managed_root: Option<&Path>) -> Vec<ExternalEnv> {
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
            out.push(ExternalEnv {
                tool,
                version,
                path: Some(path),
                source: "PATH".to_string(),
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
