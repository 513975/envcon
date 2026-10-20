use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use tokio::sync::Mutex;

use crate::error::{AppError, Result};
use crate::sources::{DownloadSpec, InstallMethod, VerificationInfo};
use sha2::{Digest, Sha256};

pub struct DownloadManager {
    next_id: AtomicU64,
    pub active: Mutex<Vec<(u64, Arc<AtomicBool>)>>,
}

impl Default for DownloadManager {
    fn default() -> Self {
        Self::new()
    }
}

impl DownloadManager {
    pub fn new() -> Self {
        Self {
            next_id: AtomicU64::new(1),
            active: Mutex::new(Vec::new()),
        }
    }

    fn alloc(&self) -> u64 {
        self.next_id.fetch_add(1, Ordering::SeqCst)
    }

    /// 流式下载到临时文件;通过 on_progress 回报 (downloaded, total, speed)
    pub async fn download(
        &self,
        spec: &DownloadSpec,
        dest: &Path,
        on_progress: impl Fn(u64, Option<u64>, u64),
    ) -> Result<PathBuf> {
        self.download_cancelable(spec, dest, Arc::new(AtomicBool::new(false)), on_progress)
            .await
    }

    pub async fn download_cancelable(
        &self,
        spec: &DownloadSpec,
        dest: &Path,
        cancel: Arc<AtomicBool>,
        on_progress: impl Fn(u64, Option<u64>, u64),
    ) -> Result<PathBuf> {
        let id = self.alloc();
        self.active.lock().await.push((id, cancel.clone()));

        let result = self.download_inner(spec, dest, &cancel, &on_progress).await;

        self.active.lock().await.retain(|(i, _)| *i != id);
        if result.is_err() {
            // The download future has dropped its file handle before cleanup.
            let _ = tokio::fs::remove_file(dest.with_extension("part")).await;
        }
        result
    }

    async fn download_inner(
        &self,
        spec: &DownloadSpec,
        dest: &Path,
        cancel: &AtomicBool,
        on_progress: &impl Fn(u64, Option<u64>, u64),
    ) -> Result<PathBuf> {
        check_canceled(cancel)?;
        if let Some(parent) = dest.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let client = reqwest::Client::builder()
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) EnvCon/0.1")
            .build()
            .map_err(AppError::Net)?;

        let request = client
            .get(&spec.url)
            .timeout(Duration::from_secs(300))
            .send();
        let resp = tokio::select! {
            biased;
            _ = wait_canceled(cancel) => return Err(canceled_error()),
            response = request => response?,
        }
        .error_for_status()
        .map_err(|e| AppError::msg(format!("下载失败({}): {e}", spec.url)))?;

        let total: Option<u64> = resp
            .headers()
            .get(reqwest::header::CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse().ok());

        let tmp_dest = dest.with_extension("part");
        let mut file = tokio::fs::File::create(&tmp_dest).await?;
        let mut stream = resp.bytes_stream();
        let mut digest = Sha256::new();

        let mut downloaded: u64 = 0;
        let mut last_report = Instant::now();
        let mut last_bytes: u64 = 0;
        let mut last_speed: u64 = 0;

        use tokio::io::AsyncWriteExt;
        loop {
            let chunk = tokio::select! {
                biased;
                _ = wait_canceled(cancel) => return Err(canceled_error()),
                chunk = stream.next() => chunk,
            };
            let Some(chunk) = chunk else { break };
            let chunk = chunk.map_err(AppError::Net)?;
            file.write_all(&chunk).await?;
            digest.update(&chunk);
            downloaded += chunk.len() as u64;

            // 每约 250ms 回报一次
            let now = Instant::now();
            if now.duration_since(last_report) >= Duration::from_millis(250) {
                let dt = now.duration_since(last_report).as_secs_f64().max(0.001);
                last_speed = ((downloaded - last_bytes) as f64 / dt) as u64;
                last_bytes = downloaded;
                last_report = now;
                on_progress(downloaded, total, last_speed);
            }
        }
        file.flush().await?;
        check_canceled(cancel)?;
        drop(file);
        validate_content_length(total, downloaded)?;
        validate_content_length(spec.expected_size, downloaded)?;
        validate_digest(&spec.verification, &digest)?;
        on_progress(downloaded, total, last_speed);

        // .part → 最终名
        let final_path = dest.to_path_buf();
        if final_path.exists() {
            tokio::fs::remove_file(&final_path).await?;
        }
        tokio::fs::rename(&tmp_dest, &final_path).await?;
        Ok(final_path)
    }
}

fn validate_content_length(expected: Option<u64>, downloaded: u64) -> Result<()> {
    if let Some(expected) = expected {
        if expected != downloaded {
            return Err(AppError::msg(format!(
                "下载长度校验失败：预期 {expected} 字节，实际 {downloaded} 字节"
            )));
        }
    }
    Ok(())
}

pub(crate) fn validate_digest(
    verification: &VerificationInfo,
    digest: &Sha256,
) -> Result<()> {
    match verification {
        VerificationInfo::TrustedSha256 { expected, .. } => {
            if !is_sha256(expected) {
                return Err(AppError::msg("下载校验资料无效：SHA-256 必须是 64 位十六进制值"));
            }
            let actual = format!("{:x}", digest.clone().finalize());
            if !actual.eq_ignore_ascii_case(expected) {
                return Err(AppError::msg(format!(
                    "下载 SHA-256 校验失败：预期 {expected}，实际 {actual}"
                )));
            }
        }
        VerificationInfo::ChecksumFile { .. } => {
            return Err(AppError::msg(
                "下载校验资料暂不支持：无法安全解析并核对来源校验文件",
            ));
        }
        VerificationInfo::Invalid { reason } => {
            return Err(AppError::msg(format!("下载校验资料无效：{reason}")));
        }
        VerificationInfo::Unverified { .. } => {}
    }
    Ok(())
}

pub(crate) fn validate_existing_file(
    path: &Path,
    verification: &VerificationInfo,
    expected_size: Option<u64>,
) -> Result<()> {
    let metadata = std::fs::metadata(path)?;
    validate_content_length(expected_size, metadata.len())?;
    if verification.expected_sha256().is_none() {
        return validate_digest(verification, &Sha256::new());
    }
    let mut file = std::fs::File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = std::io::Read::read(&mut file, &mut buffer)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    validate_digest(verification, &digest)
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit())
}

/// 解压 zip 到目标目录;strip_top 时剥掉顶层目录。进度通过 on_progress(0-1) 回报
pub async fn unzip(
    zip_path: &Path,
    target: &Path,
    strip_top: bool,
    on_progress: impl Fn(f64) + Send + 'static,
) -> Result<()> {
    unzip_cancelable(
        zip_path, target, strip_top, Arc::new(AtomicBool::new(false)), on_progress,
    ).await
}

pub async fn unzip_cancelable(
    zip_path: &Path,
    target: &Path,
    strip_top: bool,
    cancel: Arc<AtomicBool>,
    on_progress: impl Fn(f64) + Send + 'static,
) -> Result<()> {
    tokio::task::spawn_blocking({
        let zip_path = zip_path.to_path_buf();
        let target = target.to_path_buf();
        move || unzip_blocking(&zip_path, &target, strip_top, &cancel, &on_progress)
    })
    .await
    .map_err(|e| AppError::msg(format!("解压任务失败: {e}")))??;
    Ok(())
}

fn unzip_blocking(
    zip_path: &Path,
    target: &Path,
    strip_top: bool,
    cancel: &AtomicBool,
    on_progress: &impl Fn(f64),
) -> Result<()> {
    check_canceled(cancel)?;
    let file = std::fs::File::open(zip_path)?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| AppError::msg(format!("读取压缩包失败: {e}")))?;

    let total = archive.len().max(1) as f64;
    let mut done: usize = 0;

    // Validate every original entry before stripping a directory or writing files.
    let mut names = Vec::with_capacity(archive.len());
    for i in 0..archive.len() {
        check_canceled(cancel)?;
        let entry = archive.by_index(i)
            .map_err(|e| AppError::msg(format!("读取条目失败: {e}")))?;
        if entry.enclosed_name().is_none() {
            return Err(AppError::msg(format!("不安全的 ZIP 路径: {}", entry.name())));
        }
        names.push(safe_zip_name(entry.name())?);
    }
    let top = if strip_top {
        detect_top_dir(&names)
    } else {
        None
    };

    for i in 0..archive.len() {
        check_canceled(cancel)?;
        let mut entry = archive
            .by_index(i)
            .map_err(|e| AppError::msg(format!("读取条目失败: {e}")))?;
        let name = &names[i];

        let rel = match &top {
            Some(t) if name.starts_with(&format!("{t}/")) || name.as_str() == t.as_str() => {
                name.strip_prefix(&format!("{t}/")).unwrap_or("").to_string()
            }
            _ => name.clone(),
        };
        if rel.is_empty() {
            continue;
        }

        let out_path = target.join(&rel);
        reject_link_ancestors(target, &out_path)?;
        if entry.is_dir() || rel.ends_with('/') {
            std::fs::create_dir_all(&out_path)?;
        } else {
            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut out = std::fs::File::create(&out_path)?;
            use std::io::{Read, Write};
            let mut buffer = [0u8; 64 * 1024];
            loop {
                check_canceled(cancel)?;
                let n = entry.read(&mut buffer)?;
                if n == 0 {
                    break;
                }
                out.write_all(&buffer[..n])?;
            }
            // 清除 zip 带来的只读位
            let _ = clear_readonly(&out_path);
        }
        done += 1;
        on_progress(done as f64 / total);
    }
    check_canceled(cancel)
}

fn safe_zip_name(name: &str) -> Result<String> {
    let normalized = name.replace('\\', "/");
    let invalid = normalized.starts_with('/')
        || normalized.contains([':', '\0'])
        || normalized.split('/').any(|part| {
            part == "." || part == ".." || part.ends_with(['.', ' '])
                || matches!(part.split('.').next().unwrap_or("").to_ascii_uppercase().as_str(),
                    "CON" | "PRN" | "AUX" | "NUL" | "COM1" | "COM2" | "COM3" | "COM4" |
                    "COM5" | "COM6" | "COM7" | "COM8" | "COM9" | "LPT1" | "LPT2" |
                    "LPT3" | "LPT4" | "LPT5" | "LPT6" | "LPT7" | "LPT8" | "LPT9")
        });
    if invalid || normalized.is_empty() {
        return Err(AppError::msg(format!("不安全的 ZIP 路径: {name}")));
    }
    Ok(normalized)
}

fn reject_link_ancestors(target: &Path, output: &Path) -> Result<()> {
    for path in output.ancestors() {
        match std::fs::symlink_metadata(path) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err(AppError::msg(format!("解压路径包含链接: {}", path.display())));
            }
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
            _ => {}
        }
        if path == target {
            break;
        }
    }
    Ok(())
}

pub(crate) fn canceled_error() -> AppError {
    AppError::msg("__CANCELED__")
}

pub(crate) fn check_canceled(cancel: &AtomicBool) -> Result<()> {
    if cancel.load(Ordering::SeqCst) {
        Err(canceled_error())
    } else {
        Ok(())
    }
}

async fn wait_canceled(cancel: &AtomicBool) {
    while !cancel.load(Ordering::SeqCst) {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

fn detect_top_dir(names: &[String]) -> Option<String> {
    let first = names.first()?;
    let top = first.split('/').next()?;
    // 所有条目都在同一顶层目录下
    if names.iter().all(|n| n == top || n.starts_with(&format!("{top}/"))) {
        Some(top.to_string())
    } else {
        None
    }
}

fn clear_readonly(path: &Path) -> std::io::Result<()> {
    let meta = std::fs::metadata(path)?;
    let mut perms = meta.permissions();
    if perms.readonly() {
        perms.set_readonly(false);
        std::fs::set_permissions(path, perms)?;
    }
    Ok(())
}

/// 解压 tar.gz(Rust rustup 场景备用)
#[allow(dead_code)]
pub async fn untargz(gz_path: &Path, target: &Path) -> Result<()> {
    tokio::task::spawn_blocking({
        let gz_path = gz_path.to_path_buf();
        let target = target.to_path_buf();
        move || {
            let file = std::fs::File::open(&gz_path)?;
            let gz = flate2::read::GzDecoder::new(file);
            let mut archive = tar::Archive::new(gz);
            archive.unpack(&target)?;
            Ok(())
        }
    })
    .await
    .map_err(|e| AppError::msg(format!("解压任务失败: {e}")))?
}

/// 执行 exe 安装器(静默),实时读取输出行
pub async fn run_installer(
    exe: &Path,
    args: &[String],
    on_line: impl Fn(String),
) -> Result<()> {
    let child = tokio::process::Command::new(exe)
        .args(args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .creation_flags(0x08000000) // CREATE_NO_WINDOW
        .spawn()
        .map_err(|e| AppError::msg(format!("启动安装器失败: {e}")))?;

    monitor_installer(child, on_line).await
}

async fn drain_output(
    mut reader: impl tokio::io::AsyncRead + Unpin,
    on_line: &impl Fn(String),
) -> std::io::Result<String> {
    use tokio::io::AsyncReadExt;
    let mut buffer = [0u8; 4096];
    let mut tail = Vec::new();
    loop {
        let n = reader.read(&mut buffer).await?;
        if n == 0 { break; }
        // Bounded reads also handle non-UTF8 output and progress without newlines.
        on_line(String::from_utf8_lossy(&buffer[..n]).into_owned());
        tail.extend_from_slice(&buffer[..n]);
        if tail.len() > 8192 { tail.drain(..tail.len() - 8192); }
    }
    Ok(String::from_utf8_lossy(&tail).into_owned())
}

async fn monitor_installer(mut child: tokio::process::Child, on_line: impl Fn(String)) -> Result<()> {
    let stdout = child.stdout.take().ok_or_else(|| AppError::msg("安装器输出管道缺失"))?;
    let stderr = child.stderr.take().ok_or_else(|| AppError::msg("安装器错误管道缺失"))?;
    let (output, errors, status) = tokio::join!(
        drain_output(stdout, &on_line),
        drain_output(stderr, &on_line),
        child.wait(),
    );
    let output = output?;
    let errors = errors?;
    let status = status?;
    if status.success() || matches!(status.code(), Some(3010)) {
        Ok(())
    } else {
        Err(AppError::msg(format!("安装器退出码 {}: {}\n{}",
            status.code().unwrap_or(-1), errors.trim(), output.trim())))
    }
}

/// rustup-init 引导安装:隔离 RUSTUP_HOME/CARGO_HOME 到 rusts/<name>/;
/// dist_server 为 Some 时走镜像(如 rsproxy.cn),None 时用官方
pub async fn run_rustup_init(
    exe: &Path,
    channel: &str,
    rustup_home: &Path,
    cargo_home: &Path,
    dist_server: Option<&str>,
    on_line: impl Fn(String),
) -> Result<()> {
    std::fs::create_dir_all(rustup_home)?;
    std::fs::create_dir_all(cargo_home)?;

    let mut cmd = tokio::process::Command::new(exe);
    cmd.args([
        "-y",
        "--no-modify-path",
        "--default-toolchain",
        channel,
        "--profile",
        "minimal",
    ])
    .env("RUSTUP_HOME", rustup_home)
    .env("CARGO_HOME", cargo_home)
    .stdout(std::process::Stdio::piped())
    .stderr(std::process::Stdio::piped())
    .kill_on_drop(true)
    .creation_flags(0x08000000);
    if let Some(server) = dist_server {
        cmd.env("RUSTUP_DIST_SERVER", server)
            .env("RUSTUP_UPDATE_ROOT", format!("{server}/rustup"));
    }

    let child = cmd
        .spawn()
        .map_err(|e| AppError::msg(format!("启动 rustup-init 失败: {e}")))?;

    monitor_installer(child, on_line).await
}

impl InstallMethod {
    /// 下载的文件扩展名
    pub fn file_ext(&self) -> &'static str {
        match self {
            InstallMethod::Unzip { .. } => "zip",
            InstallMethod::Installer { .. } | InstallMethod::RustupInit { .. } => "exe",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    #[test]
    fn content_length_verification_rejects_mismatch_but_allows_unknown_length() {
        assert!(validate_content_length(Some(4), 3).is_err());
        assert!(validate_content_length(Some(4), 4).is_ok());
        assert!(validate_content_length(None, 3).is_ok());
    }

    #[tokio::test]
    async fn installer_drains_large_stderr_and_reports_failure() {
        let exe = PathBuf::from(std::env::var("SystemRoot").unwrap())
            .join("System32/WindowsPowerShell/v1.0/powershell.exe");
        let args = vec!["-NoProfile".into(), "-NonInteractive".into(), "-Command".into(),
            "[Console]::Error.Write(('x' * 200000)); [Console]::Error.Write('diagnostic-marker'); [Console]::Out.Write('stdout-marker'); exit 7".into()];
        let output = std::sync::Mutex::new(String::new());
        let result = tokio::time::timeout(Duration::from_secs(20), run_installer(&exe, &args, |line| {
            output.lock().unwrap().push_str(&line);
        })).await.expect("installer must not deadlock");
        let error = result.unwrap_err().to_string();
        assert!(error.contains("diagnostic-marker"));
        assert!(error.contains('7'));
        assert!(output.lock().unwrap().contains("stdout-marker"));
    }

    #[tokio::test]
    async fn go_and_bun_archives_match_expected_executable_paths() {
        use crate::types::EnvType;
        for (kind, entry, executable) in [
            (EnvType::Go, "go/bin/go.exe", "bin/go.exe"),
            (EnvType::Bun, "bun-windows-x64/bun.exe", "bun.exe"),
        ] {
            let dir = crate::test_support::TestDir::new();
            let zip = dir.path().join("input.zip");
            let target = dir.path().join("output");
            archive(&zip, &[(entry, b"binary")]);
            let spec = crate::sources::download_spec(kind, "1.0.0", "custom-name", "official", Some("https://example.test/package.zip")).unwrap();
            let InstallMethod::Unzip { strip_top } = spec.method else { panic!("expected ZIP") };
            unzip(&zip, &target, strip_top, |_| {}).await.unwrap();
            assert_eq!(std::fs::read(target.join(executable)).unwrap(), b"binary");
        }
    }

    fn archive(path: &Path, entries: &[(&str, &[u8])]) {
        let file = std::fs::File::create(path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        for (name, contents) in entries {
            zip.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
            zip.write_all(contents).unwrap();
        }
        zip.finish().unwrap();
    }

    #[test]
    fn zip_rejects_unsafe_paths_before_writing_even_with_strip_top() {
        let dir = crate::test_support::TestDir::new();
        let zip = dir.path().join("input.zip");
        let target = dir.path().join("output");
        for name in ["../escaped", "..\\escaped", "/escaped", "C:/escaped", "C:escaped",
            "\\\\server\\share\\escaped", "folder/../../escaped", "file:stream", "folder/.. /escaped", "NUL.txt"] {
            for strip_top in [false, true] {
                archive(&zip, &[("safe.txt", b"safe"), (name, b"bad")]);
                assert!(unzip_blocking(&zip, &target, strip_top, &AtomicBool::new(false), &|_| {}).is_err(), "{name}");
                assert!(!target.exists(), "must validate all paths before extracting");
            }
        }
    }

    #[test]
    fn zip_extracts_valid_paths_and_preserves_double_dots_in_names() {
        let dir = crate::test_support::TestDir::new();
        let zip = dir.path().join("input.zip");
        archive(&zip, &[("tool/bin/run.exe", b"binary"), ("tool/lib/a..b", b"data")]);
        let target = dir.path().join("output");
        unzip_blocking(&zip, &target, true, &AtomicBool::new(false), &|_| {}).unwrap();
        assert_eq!(std::fs::read(target.join("bin/run.exe")).unwrap(), b"binary");
        assert_eq!(std::fs::read(target.join("lib/a..b")).unwrap(), b"data");
    }

    #[test]
    fn zip_refuses_to_write_through_existing_junction() {
        let dir = crate::test_support::TestDir::new();
        let zip = dir.path().join("input.zip");
        let target = dir.path().join("output");
        let outside = dir.path().join("outside");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        junction::create(&outside, target.join("linked")).unwrap();
        archive(&zip, &[("linked/escaped", b"bad")]);
        assert!(unzip_blocking(&zip, &target, false, &AtomicBool::new(false), &|_| {}).is_err());
        assert!(!outside.join("escaped").exists());
    }

    #[tokio::test]
    async fn unzip_observes_cancellation_between_entries() {
        let dir = crate::test_support::TestDir::new();
        let zip = dir.path().join("input.zip");
        let target = dir.path().join("output");
        archive(&zip, &[("first", b"first"), ("second", b"second")]);
        let cancel = Arc::new(AtomicBool::new(false));
        let signal = cancel.clone();
        let result = unzip_cancelable(&zip, &target, false, cancel, move |_| {
            signal.store(true, Ordering::SeqCst);
        }).await;
        assert!(result.unwrap_err().to_string().contains("__CANCELED__"));
        assert!(!target.join("second").exists());
        std::fs::remove_dir_all(&target).unwrap();
    }

    #[tokio::test]
    async fn download_cancels_while_waiting_for_headers_or_more_data() {
        for headers in [false, true] {
            let dir = crate::test_support::TestDir::new();
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
            let (stop_tx, stop_rx) = std::sync::mpsc::channel();
            let server = std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
                let mut buf = [0; 4096];
                stream.read(&mut buf).unwrap();
                if headers {
                    stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 1000000\r\n\r\ndata").unwrap();
                    stream.flush().unwrap();
                }
                let _ = ready_tx.send(());
                let _ = stop_rx.recv_timeout(Duration::from_secs(5));
            });
            let spec = DownloadSpec {
                url: format!("http://{address}/archive"),
                method: InstallMethod::Unzip { strip_top: false },
                verification: VerificationInfo::default(),
                expected_size: None,
            };
            let dest = dir.path().join("download.zip");
            let cancel = Arc::new(AtomicBool::new(false));
            let manager = DownloadManager::new();
            let download = manager.download_cancelable(&spec, &dest, cancel.clone(), |_, _, _| {});
            let trigger = async {
                tokio::time::timeout(Duration::from_secs(5), ready_rx).await.unwrap().unwrap();
                tokio::time::sleep(Duration::from_millis(100)).await;
                cancel.store(true, Ordering::SeqCst);
            };
            let (result, ()) = tokio::join!(tokio::time::timeout(Duration::from_secs(5), download), trigger);
            let _ = stop_tx.send(());
            server.join().unwrap();
            assert!(result.unwrap().unwrap_err().to_string().contains("__CANCELED__"));
            assert!(!dest.exists());
            assert!(!dest.with_extension("part").exists());
            assert!(manager.active.lock().await.is_empty());
        }
    }

    #[test]
    fn checksum_states_are_distinguished_and_never_misreported_as_verified() {
        let digest = Sha256::new();
        let expected = format!("{:x}", Sha256::digest(b"fixture"));
        assert!(validate_digest(
            &VerificationInfo::trusted_sha256(&expected, "test fixture"),
            &Sha256::new()
        )
        .is_err());
        assert!(validate_digest(
            &VerificationInfo::trusted_sha256("not-a-sha256", "test fixture"),
            &digest
        )
        .unwrap_err()
        .to_string()
        .contains("无效"));
        assert!(validate_digest(
            &VerificationInfo::ChecksumFile {
                url: "https://example.test/SHA256SUMS".into(),
                filename: "archive.zip".into(),
                source: "fixture".into(),
            },
            &digest,
        )
        .unwrap_err()
        .to_string()
        .contains("暂不支持"));
        assert!(validate_digest(
            &VerificationInfo::Invalid {
                reason: "malformed digest".into(),
            },
            &digest,
        )
        .unwrap_err()
        .to_string()
        .contains("无效"));
        assert!(validate_digest(&VerificationInfo::unverified(), &digest).is_ok());
    }

    #[test]
    fn pre_install_recheck_detects_file_tampering_and_size_changes() {
        let dir = crate::test_support::TestDir::new();
        let file = dir.path().join("asset.bin");
        std::fs::write(&file, b"trusted bytes").unwrap();
        let verification = VerificationInfo::trusted_sha256(
            format!("{:x}", Sha256::digest(b"trusted bytes")),
            "fixture publisher metadata",
        );
        assert!(validate_existing_file(&file, &verification, Some(13)).is_ok());

        std::fs::write(&file, b"tampered data").unwrap();
        assert!(validate_existing_file(&file, &verification, Some(13))
            .unwrap_err()
            .to_string()
            .contains("SHA-256"));
        assert!(validate_existing_file(&file, &verification, Some(12))
            .unwrap_err()
            .to_string()
            .contains("长度"));
        assert!(validate_existing_file(&file, &VerificationInfo::unverified(), Some(12)).is_err());
        assert!(validate_existing_file(&file, &VerificationInfo::unverified(), None).is_ok());
    }

    #[tokio::test]
    async fn download_length_and_digest_failures_never_commit_partial_file() {
        for (body, content_length, expected_size, verification) in [
            (
                b"short".as_slice(),
                Some(10usize),
                None,
                VerificationInfo::unverified(),
            ),
            (
                b"tampered".as_slice(),
                Some(8usize),
                None,
                VerificationInfo::trusted_sha256(
                    format!("{:x}", Sha256::digest(b"expected")),
                    "fixture publisher metadata",
                ),
            ),
            (
                b"exact".as_slice(),
                Some(5usize),
                Some(6u64),
                VerificationInfo::unverified(),
            ),
            (
                b"asset".as_slice(),
                Some(5usize),
                None,
                VerificationInfo::ChecksumFile {
                    url: "https://example.test/SHA256SUMS".into(),
                    filename: "asset.zip".into(),
                    source: "fixture publisher".into(),
                },
            ),
            (
                b"asset".as_slice(),
                Some(5usize),
                None,
                VerificationInfo::Invalid {
                    reason: "malformed publisher digest".into(),
                },
            ),
        ] {
            let dir = crate::test_support::TestDir::new();
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let server = std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = [0; 4096];
                stream.read(&mut request).unwrap();
                let content_length = content_length.unwrap();
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Length: {content_length}\r\nConnection: close\r\n\r\n"
                )
                .unwrap();
                stream.write_all(body).unwrap();
                stream.flush().unwrap();
            });
            let spec = DownloadSpec {
                url: format!("http://{address}/asset"),
                method: InstallMethod::Unzip { strip_top: false },
                verification,
                expected_size,
            };
            let destination = dir.path().join("asset.zip");
            let result = DownloadManager::new()
                .download(&spec, &destination, |_, _, _| {})
                .await;
            server.join().unwrap();
            assert!(result.is_err());
            assert!(!destination.exists());
            assert!(!destination.with_extension("part").exists());
        }
    }

    #[tokio::test]
    async fn unverified_mirror_asset_is_labeled_as_unverified_but_can_download() {
        let dir = crate::test_support::TestDir::new();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0; 4096];
            stream.read(&mut request).unwrap();
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: 6\r\nConnection: close\r\n\r\nmirror"
            )
            .unwrap();
            stream.flush().unwrap();
        });
        let spec = DownloadSpec {
            url: format!("http://{address}/mirror.zip"),
            method: InstallMethod::Unzip { strip_top: false },
            verification: VerificationInfo::unverified(),
            expected_size: Some(6),
        };
        let destination = dir.path().join("mirror.zip");
        assert!(DownloadManager::new()
            .download(&spec, &destination, |_, _, _| {})
            .await
            .is_ok());
        server.join().unwrap();
        assert_eq!(std::fs::read(destination).unwrap(), b"mirror");
    }
}
