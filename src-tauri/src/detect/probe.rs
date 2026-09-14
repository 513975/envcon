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

/// 执行命令并保留 stdout/stderr，避免把启动提示误当版本。
/// `kill_on_drop` 确保超时后不会留下仍在运行的探测进程。
async fn run_capture_with_env(
    exe: &Path,
    args: &[&str],
    timeout: Duration,
    envs: &[(&str, &Path)],
) -> Result<String, String> {
    let mut command = Command::new(exe);
    command
        .args(args)
        .creation_flags(0x08000000)
        .kill_on_drop(true);
    for (name, value) in envs {
        command.env(name, value);
    }
    let out = tokio::time::timeout(timeout, command.output())
        .await
        .map_err(|_| format!("探测超时（{} 秒）", timeout.as_secs()))?
        .map_err(|e| format!("无法启动: {e}"))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let stdout = String::from_utf8_lossy(&out.stdout);
        let detail = if stderr.trim().is_empty() { stdout.trim() } else { stderr.trim() };
        return Err(format!("退出码 {}: {}", out.status.code().unwrap_or(-1),
            if detail.is_empty() { "命令未提供错误输出" } else { detail }));
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    Ok(format!("{stdout}\n{stderr}"))
}

async fn run_capture(exe: &Path, args: &[&str], timeout: Duration) -> Result<String, String> {
    run_capture_with_env(exe, args, timeout, &[]).await
}

/// 从命令输出中提取类似 3.14.0、20.1.0.windows.1 的版本 token。
fn version_token(raw: &str) -> Option<String> {
    for token in raw.split_whitespace() {
        let Some(start) = token.find(|c: char| c.is_ascii_digit()) else { continue };
        let candidate = token[start..]
            .trim_matches(|c: char| matches!(c, '"' | '\'' | '(' | ')' | '[' | ']' | ',' | ';'));
        let mut value = String::new();
        let mut dots = 0;
        for c in candidate.chars() {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+') {
                if c == '.' { dots += 1; }
                value.push(c);
            } else {
                break;
            }
        }
        if dots >= 1 && value.chars().next().is_some_and(|c| c.is_ascii_digit()) {
            return Some(value.trim_end_matches(['.', '-', '+']).to_string());
        }
    }
    None
}

async fn probe_cmd(exe: &Path, args: &[&str]) -> Option<String> {
    if !exe.exists() {
        return None;
    }
    let output = run_capture(exe, args, Duration::from_secs(3)).await.ok()?;
    let s = output.trim();
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
    version_token(s).or_else(|| s.lines().map(str::trim).find(|line| !line.is_empty()).map(str::to_string))
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
    if let Ok(release) = tokio::fs::read_to_string(dir.join("release")).await {
        for line in release.lines() {
            if let Some(v) = line.strip_prefix("JAVA_VERSION=") {
                let v = v.trim().trim_matches('"');
                if !v.is_empty() { return Some(v.to_string()); }
            }
        }
    }
    let java = dir.join("bin").join("java.exe");
    let output = run_capture(&java, &["-version"], Duration::from_secs(5)).await.ok()?;
    output.lines().find(|line| line.contains("version")).and_then(version_token)
}

/// Go:读 VERSION 文件(内容如 go1.24.0);失败执行 go.exe
async fn probe_go(dir: &Path) -> Option<String> {
    if let Ok(s) = tokio::fs::read_to_string(dir.join("VERSION")).await {
        if let Some(line) = s.lines().map(str::trim).find(|line| !line.is_empty()) {
            if let Some(v) = line.strip_prefix("go").and_then(version_token) {
                return Some(v);
            }
        }
    }
    let go_exe = dir.join("bin").join("go.exe");
    if go_exe.exists() {
        if let Ok(line) = run_capture(&go_exe, &["version"], Duration::from_secs(3)).await {
            // "go version go1.24.0 windows/amd64"
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 3 { return parts[2].trim_start_matches("go").parse::<String>().ok().and_then(|v| version_token(&v)); }
        }
    }
    None
}

/// Rust:rustup 结构 → cargo.exe --version;否则文件夹名
async fn probe_rust(dir: &Path) -> Option<String> {
    let cargo_home = dir.join("cargo-home");
    let rustup_home = dir.join("rustup-home");
    let envs = [("CARGO_HOME", cargo_home.as_path()), ("RUSTUP_HOME", rustup_home.as_path())];
    let rustc = cargo_home.join("bin").join("rustc.exe");
    if rustc.exists() {
        if let Ok(output) = run_capture_with_env(&rustc, &["--version"], Duration::from_secs(8), &envs).await {
            if let Some(v) = version_token(&output) { return Some(v); }
        }
    }
    let cargo = cargo_home.join("bin").join("cargo.exe");
    if cargo.exists() {
        if let Ok(output) = run_capture_with_env(&cargo, &["--version"], Duration::from_secs(8), &envs).await {
            if let Some(v) = version_token(&output) { return Some(v); }
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
    let mvn = dir.join("bin").join("mvn.cmd");
    run_capture(&mvn, &["--version"], Duration::from_secs(8)).await.ok().and_then(|s| version_token(&s))
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
    let gradle = dir.join("bin").join("gradle.bat");
    run_capture(&gradle, &["--version"], Duration::from_secs(8)).await.ok().and_then(|s| version_token(&s))
}

/// PHP:php.exe -v → "PHP 8.3.16 (cli)..."
async fn probe_php(dir: &Path) -> Option<String> {
    let php = dir.join("php.exe");
    if !php.exists() {
        return None;
    }
    let output = run_capture(&php, &["-v"], Duration::from_secs(3)).await.ok()?;
    version_token(&output)
}

/// LLVM:clang --version → "clang version 20.1.0"
async fn probe_llvm(dir: &Path) -> Option<String> {
    let clang = dir.join("bin").join("clang.exe");
    if !clang.exists() {
        return None;
    }
    let output = run_capture(&clang, &["--version"], Duration::from_secs(3)).await.ok()?;
    version_token(&output)
}

/// Git(PortableGit):cmd\git.exe --version → "git version 2.55.0.windows.5"
async fn probe_git(dir: &Path) -> Option<String> {
    let git = dir.join("cmd").join("git.exe");
    if !git.exists() {
        return None;
    }
    let output = run_capture(&git, &["--version"], Duration::from_secs(5)).await.ok()?;
    version_token(&output)
}

/// GitHub CLI:bin\gh.exe --version → "gh version 2.100.0 (...)"
async fn probe_gh(dir: &Path) -> Option<String> {
    let gh = dir.join("bin").join("gh.exe");
    if !gh.exists() {
        return None;
    }
    let output = run_capture(&gh, &["--version"], Duration::from_secs(5)).await.ok()?;
    version_token(&output)
}

/// MinGW(WinLibs):bin\gcc.exe --version → "gcc.exe (...) 16.2.0"(取末词版本号)
async fn probe_mingw(dir: &Path) -> Option<String> {
    let gcc = dir.join("bin").join("gcc.exe");
    if !gcc.exists() {
        return None;
    }
    let output = run_capture(&gcc, &["--version"], Duration::from_secs(5)).await.ok()?;
    version_token(&output)
}

#[cfg(test)]
mod tests {
    use super::version_token;

    #[test]
    fn extracts_versions_without_accepting_non_versions() {
        assert_eq!(version_token("openjdk version \"21.0.4\""), Some("21.0.4".into()));
        assert_eq!(version_token("clang version 20.1.0 (vendor build)"), Some("20.1.0".into()));
        assert_eq!(version_token("gcc.exe (WinLibs) 14.2.0"), Some("14.2.0".into()));
        assert_eq!(version_token("installation unavailable"), None);
    }

    #[test]
    fn handles_go_version_file_with_metadata_lines() {
        let version_file = "go1.24.0\ntime 2025-02-11T00:00:00Z\n";
        let version = version_file.lines().map(str::trim).find(|line| !line.is_empty())
            .and_then(|line| line.strip_prefix("go")).and_then(version_token);
        assert_eq!(version.as_deref(), Some("1.24.0"));
    }
}
