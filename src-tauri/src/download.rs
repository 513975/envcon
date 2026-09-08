use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use tokio::sync::Mutex;

use crate::error::{AppError, Result};
use crate::sources::{DownloadSpec, InstallMethod};

pub struct DownloadManager {
    next_id: AtomicU64,
    pub active: Mutex<Vec<(u64, Arc<AtomicBool>)>>,
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
        let id = self.alloc();
        let cancel = Arc::new(AtomicBool::new(false));
        self.active.lock().await.push((id, cancel.clone()));

        let result = self
            .download_inner(spec, dest, &cancel, &on_progress)
            .await;

        self.active.lock().await.retain(|(i, _)| *i != id);
        result
    }

    async fn download_inner(
        &self,
        spec: &DownloadSpec,
        dest: &Path,
        cancel: &AtomicBool,
        on_progress: &impl Fn(u64, Option<u64>, u64),
    ) -> Result<PathBuf> {
        if let Some(parent) = dest.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let client = reqwest::Client::builder()
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) EnvCon/0.1")
            .build()
            .map_err(AppError::Net)?;

        let resp = client
            .get(&spec.url)
            .timeout(Duration::from_secs(300))
            .send()
            .await?
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

        let mut downloaded: u64 = 0;
        let mut last_report = Instant::now();
        let mut last_bytes: u64 = 0;
        let mut last_speed: u64 = 0;

        use tokio::io::AsyncWriteExt;
        while let Some(chunk) = stream.next().await {
            if cancel.load(Ordering::SeqCst) {
                let _ = tokio::fs::remove_file(&tmp_dest).await;
                return Err(AppError::msg("__CANCELED__".to_string()));
            }
            let chunk = chunk.map_err(AppError::Net)?;
            file.write_all(&chunk).await?;
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

/// 解压 zip 到目标目录;strip_top 时剥掉顶层目录。进度通过 on_progress(0-1) 回报
pub async fn unzip(
    zip_path: &Path,
    target: &Path,
    strip_top: bool,
    on_progress: impl Fn(f64) + Send + 'static,
) -> Result<()> {
    tokio::task::spawn_blocking({
        let zip_path = zip_path.to_path_buf();
        let target = target.to_path_buf();
        move || unzip_blocking(&zip_path, &target, strip_top, &on_progress)
    })
    .await
    .map_err(|e| AppError::msg(format!("解压任务失败: {e}")))??;
    Ok(())
}

fn unzip_blocking(
    zip_path: &Path,
    target: &Path,
    strip_top: bool,
    on_progress: &impl Fn(f64),
) -> Result<()> {
    let file = std::fs::File::open(zip_path)?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| AppError::msg(format!("读取压缩包失败: {e}")))?;

    let total = archive.len().max(1) as f64;
    let mut done: usize = 0;

    // 若 strip_top,先探测顶层目录名
    let top = if strip_top {
        let names: Vec<String> = (0..archive.len())
            .filter_map(|i| archive.by_index(i).ok().map(|f| f.name().to_string()))
            .collect();
        detect_top_dir(&names)
    } else {
        None
    };

    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| AppError::msg(format!("读取条目失败: {e}")))?;
        let name = entry.name().to_string();

        // 安全校验:拒绝 zip 路径穿越
        if name.contains("..") {
            continue;
        }

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
        if entry.is_dir() || rel.ends_with('/') {
            std::fs::create_dir_all(&out_path)?;
        } else {
            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut out = std::fs::File::create(&out_path)?;
            std::io::copy(&mut entry, &mut out)?;
            // 清除 zip 带来的只读位
            let _ = clear_readonly(&out_path);
        }
        done += 1;
        on_progress(done as f64 / total);
    }
    Ok(())
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
    use tokio::io::{AsyncBufReadExt, BufReader};

    let mut child = tokio::process::Command::new(exe)
        .args(args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .creation_flags(0x08000000) // CREATE_NO_WINDOW
        .spawn()
        .map_err(|e| AppError::msg(format!("启动安装器失败: {e}")))?;

    if let Some(stdout) = child.stdout.take() {
        let mut lines = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            on_line(line);
        }
    }
    let status = child
        .wait()
        .await
        .map_err(|e| AppError::msg(format!("等待安装器失败: {e}")))?;
    if status.success() {
        Ok(())
    } else {
        Err(AppError::msg(format!(
            "安装器退出码非零: {}",
            status.code().unwrap_or(-1)
        )))
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
    use tokio::io::{AsyncBufReadExt, BufReader};

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
    .stderr(std::process::Stdio::null())
    .creation_flags(0x08000000);
    if let Some(server) = dist_server {
        cmd.env("RUSTUP_DIST_SERVER", server)
            .env("RUSTUP_UPDATE_ROOT", format!("{server}/rustup"));
    }

    let mut child = cmd
        .spawn()
        .map_err(|e| AppError::msg(format!("启动 rustup-init 失败: {e}")))?;

    if let Some(stdout) = child.stdout.take() {
        let mut lines = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            on_line(line);
        }
    }

    let status = child
        .wait()
        .await
        .map_err(|e| AppError::msg(format!("等待 rustup-init 失败: {e}")))?;
    if status.success() {
        Ok(())
    } else {
        Err(AppError::msg(format!(
            "rustup-init 退出码非零: {}",
            status.code().unwrap_or(-1)
        )))
    }
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
