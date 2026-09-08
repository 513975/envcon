use std::path::Path;
use std::time::Duration;

use tokio::process::Command;

use crate::types::EnvType;

/// 探测环境目录的版本号。优先免进程方式(读文件),否则带超时执行命令。
pub async fn probe_version(env_type: EnvType, dir: &Path) -> Option<String> {
    match env_type {
        EnvType::Jdk => probe_jdk(dir).await,
        EnvType::Python => probe_cmd(&dir.join("python.exe"), &["--version"]).await,
        EnvType::Node => probe_cmd(&dir.join("node.exe"), &["--version"]).await,
        EnvType::Go => probe_go(dir).await,
        EnvType::Rust => probe_rust(dir).await,
        EnvType::Maven => probe_maven(dir).await,
        EnvType::Gradle => probe_gradle(dir).await,
        EnvType::Php => probe_php(dir).await,
        EnvType::Llvm => probe_llvm(dir).await,
        EnvType::Zig => probe_cmd(&dir.join("zig.exe"), &["version"]).await,
        EnvType::Deno => probe_cmd(&dir.join("deno.exe"), &["--version"]).await,
        EnvType::Bun => probe_cmd(&dir.join("bun.exe"), &["--version"]).await,
        EnvType::Git => probe_git(dir).await,
        EnvType::Gh => probe_gh(dir).await,
        EnvType::Mingw => probe_mingw(dir).await,
    }
}

/// 执行命令取第一行输出(java 输出在 stderr 也要兼容)
async fn run_capture(exe: &Path, args: &[&str], timeout: Duration) -> Option<String> {
    let fut = Command::new(exe)
        .args(args)
        .creation_flags(0x08000000) // CREATE_NO_WINDOW
        .output();
    let out = tokio::time::timeout(timeout, fut).await.ok()?.ok()?;
    let s = if out.stdout.is_empty() {
        String::from_utf8_lossy(&out.stderr).to_string()
    } else {
        String::from_utf8_lossy(&out.stdout).to_string()
    };
    s.lines().next().map(|l| l.trim().to_string())
}

async fn probe_cmd(exe: &Path, args: &[&str]) -> Option<String> {
    if !exe.exists() {
        return None;
    }
    let line = run_capture(exe, args, Duration::from_secs(3)).await?;
    let s = line.trim();
    // 兼容 "v24.17.0" / "Python 3.14.0" / "deno 2.1.0 (...)" / "bun 1.1.x"
    let s = s
        .strip_prefix("Python ")
        .or_else(|| strip_word_prefix(s, "deno"))
        .or_else(|| strip_word_prefix(s, "bun"))
        .unwrap_or(s);
    let s = if s.starts_with('v') && s.len() > 1 && s.as_bytes()[1].is_ascii_digit() {
        &s[1..]
    } else {
        s
    };
    // 取首词(去掉 deno "(stable, ...)" 之类的尾巴)
    Some(s.split_whitespace().next().unwrap_or(s).to_string())
}

/// 去掉 "word " 前缀(仅当后跟版本号数字)
fn strip_word_prefix<'a>(s: &'a str, word: &str) -> Option<&'a str> {
    let rest = s.strip_prefix(word)?.strip_prefix(' ')?;
    if rest.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        Some(rest)
    } else {
        None
    }
}

/// JDK:读 release 文件 JAVA_VERSION="17.0.12"
async fn probe_jdk(dir: &Path) -> Option<String> {
    let release = tokio::fs::read_to_string(dir.join("release")).await.ok()?;
    for line in release.lines() {
        if let Some(v) = line.strip_prefix("JAVA_VERSION=") {
            let v = v.trim().trim_matches('"');
            if !v.is_empty() {
                return Some(v.to_string());
            }
        }
    }
    None
}

/// Go:读 VERSION 文件(内容如 go1.24.0);失败执行 go.exe
async fn probe_go(dir: &Path) -> Option<String> {
    if let Ok(s) = tokio::fs::read_to_string(dir.join("VERSION")).await {
        if let Some(v) = s.trim().strip_prefix("go") {
            return Some(v.to_string());
        }
    }
    let go_exe = dir.join("bin").join("go.exe");
    if go_exe.exists() {
        if let Some(line) = run_capture(&go_exe, &["version"], Duration::from_secs(3)).await {
            // "go version go1.24.0 windows/amd64"
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 3 {
                return Some(parts[2].trim_start_matches("go").to_string());
            }
        }
    }
    None
}

/// Rust:rustup 结构 → cargo.exe --version;否则文件夹名
async fn probe_rust(dir: &Path) -> Option<String> {
    let cargo = dir.join("cargo-home").join("bin").join("cargo.exe");
    if cargo.exists() {
        if let Some(line) = run_capture(&cargo, &["--version"], Duration::from_secs(5)).await {
            // "cargo 1.75.0 (1d84205a9 2023-11-20)"
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                return Some(parts[1].to_string());
            }
        }
    }
    None
}

/// Maven:解析 lib/maven-core-{v}.jar 文件名
async fn probe_maven(dir: &Path) -> Option<String> {
    let lib = dir.join("lib");
    let mut rd = tokio::fs::read_dir(&lib).await.ok()?;
    while let Ok(Some(e)) = rd.next_entry().await {
        let name = e.file_name().to_string_lossy().to_string();
        if let Some(v) = name
            .strip_prefix("maven-core-")
            .and_then(|s| s.strip_suffix(".jar"))
        {
            return Some(v.to_string());
        }
    }
    None
}

/// Gradle:解析 lib/gradle-base-services-{v}.jar 文件名
async fn probe_gradle(dir: &Path) -> Option<String> {
    let lib = dir.join("lib");
    let mut rd = tokio::fs::read_dir(&lib).await.ok()?;
    while let Ok(Some(e)) = rd.next_entry().await {
        let name = e.file_name().to_string_lossy().to_string();
        if let Some(v) = name
            .strip_prefix("gradle-base-services-")
            .and_then(|s| s.strip_suffix(".jar"))
        {
            return Some(v.to_string());
        }
    }
    None
}

/// PHP:php.exe -v → "PHP 8.3.16 (cli)..."
async fn probe_php(dir: &Path) -> Option<String> {
    let php = dir.join("php.exe");
    if !php.exists() {
        return None;
    }
    let line = run_capture(&php, &["-v"], Duration::from_secs(3)).await?;
    let s = line.trim();
    s.strip_prefix("PHP ").map(|r| r.split_whitespace().next().unwrap_or(r).to_string())
}

/// LLVM:clang --version → "clang version 20.1.0"
async fn probe_llvm(dir: &Path) -> Option<String> {
    let clang = dir.join("bin").join("clang.exe");
    if !clang.exists() {
        return None;
    }
    let line = run_capture(&clang, &["--version"], Duration::from_secs(3)).await?;
    let s = line.trim();
    s.strip_prefix("clang version ")
        .map(|r| r.split_whitespace().next().unwrap_or(r).to_string())
}

/// Git(PortableGit):cmd\git.exe --version → "git version 2.55.0.windows.5"
async fn probe_git(dir: &Path) -> Option<String> {
    let git = dir.join("cmd").join("git.exe");
    if !git.exists() {
        return None;
    }
    let line = run_capture(&git, &["--version"], Duration::from_secs(5)).await?;
    let s = line.trim();
    s.strip_prefix("git version ").map(|r| r.to_string())
}

/// GitHub CLI:bin\gh.exe --version → "gh version 2.100.0 (...)"
async fn probe_gh(dir: &Path) -> Option<String> {
    let gh = dir.join("bin").join("gh.exe");
    if !gh.exists() {
        return None;
    }
    let line = run_capture(&gh, &["--version"], Duration::from_secs(5)).await?;
    let s = line.trim();
    s.strip_prefix("gh version ").map(|r| r.split_whitespace().next().unwrap_or(r).to_string())
}

/// MinGW(WinLibs):bin\gcc.exe --version → "gcc.exe (...) 16.2.0"(取末词版本号)
async fn probe_mingw(dir: &Path) -> Option<String> {
    let gcc = dir.join("bin").join("gcc.exe");
    if !gcc.exists() {
        return None;
    }
    let line = run_capture(&gcc, &["--version"], Duration::from_secs(5)).await?;
    line.split_whitespace().last().map(|v| v.to_string())
}
