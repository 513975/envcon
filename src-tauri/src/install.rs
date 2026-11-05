use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::config::AppConfig;
use crate::download::{self, DownloadManager};
use crate::error::{AppError, Result};
use crate::sources::{self, InstallMethod};
use crate::types::EnvType;

const EVENT: &str = "install://update";
/// How long a finished install stays readable through `install_tasks`, so the
/// outcome survives a page switch instead of disappearing with the toast.
const FINISHED_TASK_TTL_SECS: u64 = 30 * 60;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase", tag = "status")]
pub enum TaskStatus {
    Downloading {
        downloaded: u64,
        total: Option<u64>,
        speed: u64,
    },
    Installing {
        message: String,
        cancelable: bool,
    },
    Done,
    Error {
        message: String,
    },
    Canceled,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskSnapshot {
    pub id: u64,
    pub env_type: EnvType,
    pub version: String,
    pub target_name: String,
    #[serde(flatten)]
    pub status: TaskStatus,
}

struct TaskInner {
    snapshot: TaskSnapshot,
    cancel: Arc<std::sync::atomic::AtomicBool>,
}

impl TaskInner {
    fn request_cancel(&self) -> Result<()> {
        match self.snapshot.status {
            TaskStatus::Downloading { .. } | TaskStatus::Installing { cancelable: true, .. } => {
                self.cancel.store(true, Ordering::SeqCst);
                Ok(())
            }
            _ => Err(AppError::msg("当前阶段无法取消")),
        }
    }

    fn begin_installing(&mut self, message: String, cancelable: bool) -> Result<()> {
        download::check_canceled(&self.cancel)?;
        self.snapshot.status = TaskStatus::Installing { message, cancelable };
        Ok(())
    }
}

struct TargetReservation {
    targets: Arc<Mutex<HashSet<String>>>,
    key: String,
}

impl TargetReservation {
    fn acquire(targets: Arc<Mutex<HashSet<String>>>, path: &Path) -> Result<Self> {
        let parent = path.parent().ok_or_else(|| AppError::msg("安装路径无效"))?;
        std::fs::create_dir_all(parent)?;
        let name = path.file_name().ok_or_else(|| AppError::msg("安装名称无效"))?;
        let key = parent
            .canonicalize()?
            .join(name)
            .to_string_lossy()
            .trim_end_matches('.')
            .to_lowercase();
        let mut active = targets.lock().unwrap();
        if active.contains(&key) {
            return Err(AppError::msg("该目录已有安装任务，请等待完成或取消后重试"));
        }
        match std::fs::symlink_metadata(path) {
            Ok(_) => return Err(AppError::msg(format!("目标目录已存在: {}", path.display()))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        active.insert(key.clone());
        drop(active);
        Ok(Self { targets, key })
    }
}

impl Drop for TargetReservation {
    fn drop(&mut self) {
        self.targets.lock().unwrap().remove(&self.key);
    }
}

struct RustupDownloadDir {
    path: PathBuf,
}

impl RustupDownloadDir {
    fn create(downloads: &Path, task_name: &str) -> Result<Self> {
        std::fs::create_dir_all(downloads)?;
        let path = downloads.join(format!(".rustup-{task_name}"));
        // rustup dispatches by its executable name; isolate tasks by directory.
        std::fs::create_dir(&path)?;
        Ok(Self { path })
    }

    fn installer(&self) -> PathBuf {
        self.path.join("rustup-init.exe")
    }
}

impl Drop for RustupDownloadDir {
    fn drop(&mut self) {
        if crate::detect::system::has_reparse_component(&self.path) {
            return;
        }
        let _ = std::fs::remove_file(self.installer());
        // Never recursively delete leftovers or files created by another process.
        let _ = std::fs::remove_dir(&self.path);
    }
}

pub struct TaskManager {
    next_id: AtomicU64,
    tasks: Arc<Mutex<HashMap<u64, TaskInner>>>,
    dl: Arc<DownloadManager>,
    targets: Arc<Mutex<HashSet<String>>>,
}

impl Default for TaskManager {
    fn default() -> Self {
        Self::new()
    }
}

impl TaskManager {
    pub fn new() -> Self {
        Self {
            next_id: AtomicU64::new(1),
            tasks: Arc::new(Mutex::new(HashMap::new())),
            dl: Arc::new(DownloadManager::new()),
            targets: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    /// 当前任务快照(供前端初始化/恢复)
    pub fn snapshots(&self) -> Vec<TaskSnapshot> {
        let tasks = self.tasks.lock().unwrap();
        let mut v: Vec<TaskSnapshot> = tasks.values().map(|t| t.snapshot.clone()).collect();
        v.sort_by_key(|t| t.id);
        v
    }

    pub async fn cancel(&self, id: u64) -> Result<()> {
        let tasks = self.tasks.lock().unwrap();
        tasks.get(&id)
            .ok_or_else(|| AppError::msg(format!("任务 {id} 不存在")))?
            .request_cancel()
    }
}

/// 一次安装请求中由用户选择的字段。
///
/// `version`、`target_name` 与 `source` 都是 `String`，逐个传参时把版本号和
/// 目标目录名传反不会编译失败，只会在磁盘上建出一个以版本号命名的目录。
/// 收进结构体后调用点按字段名书写。
pub struct InstallRequest {
    pub env_type: EnvType,
    pub version: String,
    pub target_name: String,
    pub source: String,
    pub url: Option<String>,
}

impl TaskManager {
    /// 启动安装任务(spawn 到后台)
    pub fn start(
        &self,
        app: AppHandle,
        cfg: AppConfig,
        request: InstallRequest,
    ) -> Result<u64> {
        let InstallRequest { env_type, version, target_name, source, url } = request;
        // 预检
        crate::switcher::validate_name(&target_name)?;
        let root = cfg
            .resolve_root()
            .ok_or_else(|| AppError::msg("尚未设置管理根目录"))?;
        let final_dir = root
            .join("envs")
            .join(env_type.folder())
            .join(&target_name);
        let reservation = TargetReservation::acquire(self.targets.clone(), &final_dir)?;
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);

        {
            let mut tasks = self.tasks.lock().unwrap();
            tasks.insert(
                id,
                TaskInner {
                    snapshot: TaskSnapshot {
                        id,
                        env_type,
                        version: version.clone(),
                        target_name: target_name.clone(),
                        status: TaskStatus::Downloading {
                            downloaded: 0,
                            total: None,
                            speed: 0,
                        },
                    },
                    cancel: Arc::new(std::sync::atomic::AtomicBool::new(false)),
                },
            );
        }
        let _ = app.emit(
            EVENT,
            &TaskSnapshot {
                id,
                env_type,
                version: version.clone(),
                target_name: target_name.clone(),
                status: TaskStatus::Downloading {
                    downloaded: 0,
                    total: None,
                    speed: 0,
                },
            },
        );

        let tasks = self.tasks.clone();
        let dl = self.dl.clone();
        let app2 = app.clone();
        let final_dir_for_spec = final_dir.clone();
        let version_cache = cfg.data_dir.join("version_cache.json");

        tauri::async_runtime::spawn(async move {
            let result = async {
                let spec = sources::resolve_download_spec(
                    env_type,
                    &version,
                    &final_dir_for_spec.to_string_lossy(),
                    &source,
                    url.as_deref(),
                    &version_cache,
                )
                .await?;
                run_install(&dl, &cfg, &root, env_type, &spec, &target_name, id, &app2, &tasks)
                    .await
            }
            .await;
            drop(reservation);
            match result {
                Ok(()) => {
                    // Keep the finished task so a user who navigates away can
                    // still see the result. The entry is retired later instead of
                    // being dropped as soon as the current page shows a toast.
                    update_status(&tasks, id, &app2, TaskStatus::Done);
                    tokio::time::sleep(std::time::Duration::from_secs(FINISHED_TASK_TTL_SECS)).await;
                    tasks.lock().unwrap().remove(&id);
                }
                Err(e) => {
                    if e.to_string().contains("__CANCELED__") {
                        update_status(&tasks, id, &app2, TaskStatus::Canceled);
                    } else {
                        update_status(
                            &tasks,
                            id,
                            &app2,
                            TaskStatus::Error {
                                message: e.to_string(),
                            },
                        );
                    }
                }
            }
        });

        Ok(id)
    }
}

fn update_status(
    tasks: &Arc<Mutex<HashMap<u64, TaskInner>>>,
    id: u64,
    app: &AppHandle,
    status: TaskStatus,
) {
    let snapshot = {
        let mut t = tasks.lock().unwrap();
        match t.get_mut(&id) {
            Some(inner) => {
                inner.snapshot.status = status;
                inner.snapshot.clone()
            }
            None => return,
        }
    };
    let _ = app.emit(EVENT, &snapshot);
}

fn begin_installing(
    tasks: &Arc<Mutex<HashMap<u64, TaskInner>>>,
    id: u64,
    app: &AppHandle,
    message: &str,
    cancelable: bool,
) -> Result<()> {
    let snapshot = {
        let mut tasks = tasks.lock().unwrap();
        let task = tasks.get_mut(&id).ok_or_else(|| AppError::msg("安装任务不存在"))?;
        // Serialize cancellation with entering a phase that cannot be interrupted.
        task.begin_installing(message.into(), cancelable)?;
        task.snapshot.clone()
    };
    let _ = app.emit(EVENT, &snapshot);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn run_install(
    dl: &DownloadManager,
    cfg: &AppConfig,
    root: &std::path::Path,
    env_type: EnvType,
    spec: &sources::DownloadSpec,
    target_name: &str,
    id: u64,
    app: &AppHandle,
    tasks: &Arc<Mutex<HashMap<u64, TaskInner>>>,
) -> Result<()> {
    require_install_verification(env_type, &spec.verification)?;

    // 下载
    let file_stem = format!(
        "{}-{}-{}-{id}",
        env_type.folder().trim_end_matches('s'), target_name, std::process::id()
    );
    let downloads = cfg.downloads_dir(root);
    let rustup_download = if matches!(&spec.method, InstallMethod::RustupInit { .. }) {
        Some(RustupDownloadDir::create(&downloads, &file_stem)?)
    } else {
        None
    };
    let dest = match &rustup_download {
        Some(staging) => staging.installer(),
        None => downloads.join(format!("{}.{}", file_stem, spec.method.file_ext())),
    };

    let dl_app = app.clone();
    let cancel = tasks.lock().unwrap().get(&id).unwrap().cancel.clone();
    let dest_path = dl
        .download_cancelable(spec, &dest, cancel.clone(), move |downloaded, total, speed| {
            let snapshot = {
                let mut t = tasks.lock().unwrap();
                match t.get_mut(&id) {
                    Some(inner) => {
                        inner.snapshot.status = TaskStatus::Downloading {
                            downloaded,
                            total,
                            speed,
                        };
                        inner.snapshot.clone()
                    }
                    None => return,
                }
            };
            let _ = dl_app.emit(EVENT, &snapshot);
        })
        .await?;

    // Re-hash immediately before any archive extraction or installer execution.
    // This closes the gap where a downloaded file is replaced after the
    // download task has completed but before the install phase starts.
    download::validate_existing_file(&dest_path, &spec.verification, spec.expected_size)?;

    let result = async {
        let cancelable = matches!(spec.method, InstallMethod::Unzip { .. });
        let message = if spec.verification.is_trusted() {
            "下载 SHA-256 已校验，正在安装…"
        } else {
            "来源未提供可信校验资料；文件完整性未验证，正在安装…"
        };
        begin_installing(tasks, id, app, message, cancelable)?;
        let expected_version = tasks
            .lock()
            .unwrap()
            .get(&id)
            .ok_or_else(|| AppError::msg("安装任务不存在"))?
            .snapshot
            .version
            .clone();
        let cat_dir = root.join("envs").join(env_type.folder());
        let final_dir = cat_dir.join(target_name);
        tokio::fs::create_dir_all(&cat_dir).await?;

        match &spec.method {
            InstallMethod::Unzip { strip_top } => {
                let tmp_dir = cat_dir.join(format!(".tmp-{target_name}-{}-{id}", std::process::id()));
                tokio::fs::create_dir(&tmp_dir).await?;
                let app2 = app.clone();
                let tasks2 = tasks.clone();
                let unzip_result = download::unzip_cancelable(
                    &dest_path, &tmp_dir, *strip_top, cancel.clone(),
                    move |p| {
                        let msg = format!("解压中 {:.0}%", p * 100.0);
                        update_status(&tasks2, id, &app2, TaskStatus::Installing {
                            message: msg, cancelable: true,
                        });
                    },
                ).await;
                let r = match unzip_result {
                    Ok(()) => async {
                        validate_unpacked(env_type, &expected_version, &tmp_dir).await?;
                        if env_type == EnvType::PowerShell {
                            let actual = crate::detect::probe::probe_version(env_type, &tmp_dir)
                                .await
                                .ok_or_else(|| AppError::msg("PowerShell 7 压缩包已解压，但无法读取 pwsh.exe 版本"))?;
                            validate_powershell_version(&expected_version, Some(&actual))?;
                        }
                        begin_installing(tasks, id, app, "正在完成安装…", false)?;
                        tokio::fs::rename(&tmp_dir, &final_dir)
                            .await
                            .map_err(|e| AppError::msg(format!("移动到最终目录失败: {e}")))
                    }.await,
                    Err(e) => Err(e),
                };
                // 失败清理
                if r.is_err() {
                    let _ = tokio::fs::remove_dir_all(&tmp_dir).await;
                }
                r
            }
            InstallMethod::Installer {
                args,
                failure_recovery,
            } => {
                // Claim a fresh directory so recovery never moves a pre-existing installation.
                tokio::fs::create_dir(&final_dir).await?;
                let app2 = app.clone();
                let args = args.clone();
                let result = download::run_installer(&dest_path, &args, move |line| {
                    if !line.trim().is_empty() {
                        update_status(tasks, id, &app2, TaskStatus::Installing {
                            message: line, cancelable: false,
                        });
                    }
                })
                .await;
                let result = match result {
                    Ok(()) => validate_installer(env_type, &expected_version, &final_dir).await,
                    Err(e) => Err(e),
                };
                recover_installer_failure(*failure_recovery, &final_dir, result)
            }
            InstallMethod::RustupInit { channel, dist_server } => {
                tokio::fs::create_dir(&final_dir).await?;
                let rustup_home = final_dir.join("rustup-home");
                let cargo_home = final_dir.join("cargo-home");
                let app2 = app.clone();
                let result = download::run_rustup_init(
                    &dest_path,
                    channel,
                    &rustup_home,
                    &cargo_home,
                    dist_server.as_deref(),
                    move |line| {
                        if !line.trim().is_empty() {
                            update_status(tasks, id, &app2, TaskStatus::Installing {
                                message: line, cancelable: false,
                            });
                        }
                    },
                )
                .await;
                let result = match result {
                    Ok(()) => validate_rustup(&final_dir).await,
                    Err(e) => Err(e),
                };
                recover_failed_install(&final_dir, result)
            }
        }
    }
    .await;

    // 清理安装包
    let _ = tokio::fs::remove_file(&dest_path).await;

    result
}

fn require_install_verification(
    env_type: EnvType,
    verification: &sources::VerificationInfo,
) -> Result<()> {
    if env_type == EnvType::PowerShell {
        match verification {
            sources::VerificationInfo::TrustedSha256 { expected, source }
                if source == "GitHub Releases API: PowerShell/PowerShell"
                    && is_sha256(expected) =>
            {
                return Ok(());
            }
            sources::VerificationInfo::TrustedSha256 { .. } => {
                return Err(AppError::msg(
                    "PowerShell 7 安装所需的官方 SHA-256 校验资料无效",
                ));
            }
            _ => {
                return Err(AppError::msg(
                    "PowerShell 7 安装必须先取得官方 SHA-256 摘要，当前下载源未提供可验证校验资料",
                ));
            }
        }
    }
    Ok(())
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn validate_powershell_version(expected: &str, actual: Option<&str>) -> Result<()> {
    let Some(actual) = actual else {
        return Err(AppError::msg(
            "PowerShell 7 压缩包已解压，但无法读取 pwsh.exe 版本",
        ));
    };
    if actual != expected {
        return Err(AppError::msg(format!(
            "PowerShell 7 版本校验失败：预期 {expected}，实际 {actual}"
        )));
    }
    Ok(())
}

async fn validate_unpacked(env_type: EnvType, version: &str, dir: &Path) -> Result<()> {
    if !crate::detect::looks_like(env_type, dir) {
        return Err(AppError::msg(format!("{env_type:?} 压缩包缺少必要的可执行文件")));
    }
    if env_type == EnvType::Adb {
        if !dir.join("fastboot.exe").is_file() {
            return Err(AppError::msg("Platform Tools 安装包不完整，缺少 Fastboot"));
        }
        let properties = tokio::fs::read_to_string(dir.join("source.properties")).await
            .map_err(|e| AppError::msg(format!("Platform Tools 版本元数据缺失: {e}")))?;
        let actual = properties.lines().find_map(|line| {
            let (key, value) = line.split_once('=')?;
            (key.trim() == "Pkg.Revision").then(|| value.trim())
        });
        if actual != Some(version) {
            return Err(AppError::msg(format!(
                "Platform Tools 版本校验失败：预期 {version}，实际 {}",
                actual.unwrap_or("未知")
            )));
        }
        return Ok(());
    }
    // .NET SDK 的 zip 里 `sdk/<version>/` 的目录名就是 SDK 版本。读目录名而不是
    // 执行 dotnet.exe：上面的注释已说明，未校验的二进制不能执行。
    if env_type == EnvType::DotNet {
        let sdk_dir = dir.join("sdk");
        let found = std::fs::read_dir(&sdk_dir)
            .map(|reader| {
                reader
                    .flatten()
                    .any(|entry| entry.file_name().to_string_lossy() == version)
            })
            .unwrap_or(false);
        if !found {
            return Err(AppError::msg(format!(
                ".NET SDK 版本校验失败：sdk/ 下没有 {version} 目录"
            )));
        }
        return Ok(());
    }
    if env_type == EnvType::PowerShell {
        for required in ["pwsh.exe", "pwsh.dll", "pwsh.deps.json", "pwsh.runtimeconfig.json"] {
            if !dir.join(required).is_file() {
                return Err(AppError::msg(format!(
                    "PowerShell 7 压缩包结构不完整，缺少 {required}"
                )));
            }
        }
        return Ok(());
    }
    // Do not execute binaries from an archive until its publisher-provided
    // checksum/signature has been verified. Other archives currently receive
    // structural validation only; trusted integrity metadata is tracked by R2.
    Ok(())
}

async fn validate_installer(env_type: EnvType, expected: &str, dir: &Path) -> Result<()> {
    if !crate::detect::looks_like(env_type, dir) {
        return Err(AppError::msg(format!("{env_type:?} 安装器已退出，但目标目录缺少必要文件")));
    }
    let actual = crate::detect::probe::probe_version(env_type, dir).await;
    let Some(actual) = actual else {
        return Err(AppError::msg(format!("{env_type:?} 安装器退出成功，但无法探测版本")));
    };
    if !version_matches(env_type, expected, &actual) {
        return Err(AppError::msg(format!("{env_type:?} 版本校验失败：预期 {expected}，实际 {actual}")));
    }
    Ok(())
}

fn version_matches(env_type: EnvType, expected: &str, actual: &str) -> bool {
    if matches!(env_type, EnvType::Rust | EnvType::Mingw)
        || matches!(expected, "stable" | "beta" | "nightly")
    {
        return !actual.is_empty();
    }
    expected == actual
        || (env_type == EnvType::Jdk && expected.split('+').next() == actual.split('+').next())
}

async fn validate_rustup(dir: &Path) -> Result<()> {
    if !crate::detect::looks_like(EnvType::Rust, dir)
        || crate::detect::probe::probe_version(EnvType::Rust, dir).await.is_none()
    {
        return Err(AppError::msg("rustup 已退出，但目标目录没有可用的 Rust 工具链"));
    }
    Ok(())
}

fn recover_failed_install(final_dir: &Path, result: Result<()>) -> Result<()> {
    let Err(error) = result else { return Ok(()) };
    if !final_dir.try_exists()? {
        return Err(error);
    }
    let parent = final_dir.parent().ok_or_else(|| AppError::msg("安装路径无效"))?;
    let name = final_dir.file_name().unwrap().to_string_lossy();
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos();
    let backup = parent.join(format!(".failed-{name}-{stamp}"));
    match std::fs::rename(final_dir, &backup) {
        Ok(()) => Err(AppError::msg(format!(
            "{error}；可使用原名称重试，失败文件保留在 {}", backup.display()
        ))),
        Err(recovery) => Err(AppError::msg(format!(
            "{error}；无法移走失败目录 {}: {recovery}，请关闭占用进程后处理该目录", final_dir.display()
        ))),
    }
}

fn recover_installer_failure(
    recovery: sources::InstallerFailureRecovery,
    final_dir: &Path,
    result: Result<()>,
) -> Result<()> {
    match recovery {
        sources::InstallerFailureRecovery::RetainTarget => result.map_err(|error| {
            AppError::msg(format!(
                "{error}；安装器可能已写入系统记录，目标目录 {} 保留供原厂卸载或人工核对",
                final_dir.display()
            ))
        }),
        sources::InstallerFailureRecovery::MoveTargetAside => {
            recover_failed_install(final_dir, result)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;

    #[test]
    fn rustup_download_keeps_installer_name_and_isolates_concurrent_tasks() {
        let dir = crate::test_support::TestDir::new();
        let first = RustupDownloadDir::create(dir.path(), "rust-stable-123-1").unwrap();
        let second = RustupDownloadDir::create(dir.path(), "rust-stable-123-2").unwrap();
        assert_ne!(first.installer(), second.installer());
        for staging in [&first, &second] {
            assert_eq!(staging.installer().file_name().unwrap(), "rustup-init.exe");
            std::fs::write(staging.installer(), b"installer").unwrap();
        }
        let first_path = first.path.clone();
        drop(first);
        assert!(!first_path.exists());
        assert!(second.installer().is_file());
        let second_path = second.path.clone();
        drop(second);
        assert!(!second_path.exists());
    }

    #[test]
    fn rustup_download_refuses_existing_directory_without_touching_its_files() {
        let dir = crate::test_support::TestDir::new();
        let existing = dir.path().join(".rustup-task");
        std::fs::create_dir(&existing).unwrap();
        std::fs::write(existing.join("rustup-init.exe"), b"existing").unwrap();
        assert!(RustupDownloadDir::create(dir.path(), "task").is_err());
        assert_eq!(std::fs::read(existing.join("rustup-init.exe")).unwrap(), b"existing");
    }

    #[test]
    fn rustup_download_cleanup_keeps_unexpected_leftovers_and_sibling_data() {
        let dir = crate::test_support::TestDir::new();
        std::fs::write(dir.path().join("sibling"), b"keep").unwrap();
        let staging = RustupDownloadDir::create(dir.path(), "task").unwrap();
        let leftover = staging.path.join("unexpected.log");
        std::fs::write(&leftover, b"keep").unwrap();
        std::fs::write(staging.installer(), b"installer").unwrap();
        let installer = staging.installer();
        drop(staging);
        assert!(!installer.exists());
        assert_eq!(std::fs::read(leftover).unwrap(), b"keep");
        assert_eq!(std::fs::read(dir.path().join("sibling")).unwrap(), b"keep");
    }

    #[test]
    fn failed_install_preserves_files_and_allows_same_name_retry() {
        let dir = crate::test_support::TestDir::new();
        let target = dir.path().join("my-rust");
        std::fs::create_dir(&target).unwrap();
        std::fs::write(target.join("partial"), b"preserved").unwrap();
        let error = recover_failed_install(&target, Err(AppError::msg("installer failed"))).unwrap_err();
        assert!(error.to_string().contains("installer failed"));
        assert!(!target.exists());
        let backup = std::fs::read_dir(dir.path()).unwrap().next().unwrap().unwrap().path();
        assert_eq!(std::fs::read(backup.join("partial")).unwrap(), b"preserved");
        let targets = Arc::new(Mutex::new(HashSet::new()));
        let _reservation = TargetReservation::acquire(targets, &target).unwrap();
        std::fs::create_dir(&target).unwrap();
        recover_failed_install(&target, Ok(())).unwrap();
        assert!(target.is_dir());
    }

    #[test]
    fn failed_portable_git_install_quarantines_files_and_allows_same_name_retry() {
        let dir = crate::test_support::TestDir::new();
        let target = dir.path().join("git-2.55");
        std::fs::create_dir(&target).unwrap();
        std::fs::write(target.join("partial.bin"), b"preserved").unwrap();

        let error = recover_installer_failure(
            sources::InstallerFailureRecovery::MoveTargetAside,
            &target,
            Err(AppError::msg("portable extraction failed")),
        )
        .unwrap_err();

        assert!(error.to_string().contains("portable extraction failed"));
        assert!(!target.exists());
        let backup = std::fs::read_dir(dir.path()).unwrap().next().unwrap().unwrap().path();
        assert!(backup.file_name().unwrap().to_string_lossy().starts_with(".failed-git-2.55-"));
        assert_eq!(std::fs::read(backup.join("partial.bin")).unwrap(), b"preserved");

        let targets = Arc::new(Mutex::new(HashSet::new()));
        let reservation = TargetReservation::acquire(targets, &target).unwrap();
        std::fs::create_dir(&target).unwrap();
        drop(reservation);
        assert!(target.is_dir());
        assert_eq!(std::fs::read(backup.join("partial.bin")).unwrap(), b"preserved");
    }

    #[test]
    fn failed_registered_installer_keeps_its_original_target() {
        let dir = crate::test_support::TestDir::new();
        let target = dir.path().join("python");
        std::fs::create_dir(&target).unwrap();
        std::fs::write(target.join("python.exe"), b"installer-output").unwrap();

        let error = recover_installer_failure(
            sources::InstallerFailureRecovery::RetainTarget,
            &target,
            Err(AppError::msg("post-install validation failed")),
        )
        .unwrap_err();

        assert!(error.to_string().contains("可能已写入系统记录"));
        assert!(target.join("python.exe").is_file());
        assert_eq!(std::fs::read(target.join("python.exe")).unwrap(), b"installer-output");
    }

    fn task() -> TaskInner {
        TaskInner {
            snapshot: TaskSnapshot {
                id: 1, env_type: EnvType::Node, version: "22".into(),
                target_name: "node-22".into(),
                status: TaskStatus::Downloading { downloaded: 0, total: None, speed: 0 },
            },
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }

    #[tokio::test]
    async fn adb_archive_validation_rejects_incomplete_or_wrong_release() {
        let dir = crate::test_support::TestDir::new();
        for name in ["adb.exe", "AdbWinApi.dll", "AdbWinUsbApi.dll"] {
            std::fs::write(dir.path().join(name), b"fixture").unwrap();
        }
        std::fs::write(dir.path().join("source.properties"), "Pkg.Revision=36.0.2\n").unwrap();
        assert!(validate_unpacked(EnvType::Adb, "36.0.2", dir.path()).await.is_err());
        std::fs::write(dir.path().join("fastboot.exe"), b"fixture").unwrap();
        assert!(validate_unpacked(EnvType::Adb, "36.0.2", dir.path()).await.is_ok());
        assert!(validate_unpacked(EnvType::Adb, "36.0.1", dir.path()).await.is_err());
    }

    #[tokio::test]
    async fn unverified_archives_check_structure_without_executing_their_binaries() {
        let dir = crate::test_support::TestDir::new();
        assert!(validate_unpacked(EnvType::Node, "22.0.0", dir.path()).await.is_err());
        std::fs::write(dir.path().join("node.exe"), b"not an executable").unwrap();
        assert!(validate_unpacked(EnvType::Node, "22.0.0", dir.path()).await.is_ok());
    }

    /// .NET 的 zip 解压后 `sdk/<version>/` 的目录名就是 SDK 版本。校验读目录名而
    /// 不是执行 dotnet.exe——未校验的二进制不能执行。若不校验，下载 10.0.402 却
    /// 解压出别的版本会被当成成功。
    #[tokio::test]
    async fn dotnet_archive_validation_reads_the_sdk_directory_name_not_the_binary() {
        let dir = crate::test_support::TestDir::new();
        std::fs::write(dir.path().join("dotnet.exe"), b"not an executable").unwrap();
        // 只有 dotnet.exe 没有 sdk/：结构判定先拒绝。
        assert!(validate_unpacked(EnvType::DotNet, "10.0.402", dir.path()).await.is_err());

        std::fs::create_dir_all(dir.path().join("sdk/10.0.402")).unwrap();
        assert!(validate_unpacked(EnvType::DotNet, "10.0.402", dir.path()).await.is_ok());
        // 版本不符必须报错，而不是只看「有个 sdk 目录」。
        let error = validate_unpacked(EnvType::DotNet, "10.0.404", dir.path())
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains("10.0.404"), "unexpected: {error}");
    }

    #[tokio::test]
    async fn powershell_archive_validation_requires_the_portable_layout() {
        let dir = crate::test_support::TestDir::new();
        std::fs::write(dir.path().join("pwsh.exe"), b"fixture").unwrap();
        assert!(validate_unpacked(EnvType::PowerShell, "7.6.0", dir.path()).await.is_err());
        for name in ["pwsh.dll", "pwsh.deps.json", "pwsh.runtimeconfig.json"] {
            std::fs::write(dir.path().join(name), b"fixture").unwrap();
        }
        assert!(validate_unpacked(EnvType::PowerShell, "7.6.0", dir.path()).await.is_ok());
    }

    #[test]
    fn powershell_install_requires_a_trusted_official_sha256() {
        let digest = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let trusted = crate::sources::VerificationInfo::trusted_sha256(
            digest,
            "GitHub Releases API: PowerShell/PowerShell",
        );
        assert!(require_install_verification(EnvType::PowerShell, &trusted).is_ok());
        assert!(require_install_verification(EnvType::PowerShell, &crate::sources::VerificationInfo::unverified()).is_err());
        assert!(require_install_verification(
            EnvType::PowerShell,
            &crate::sources::VerificationInfo::Invalid { reason: "bad digest".into() }
        ).is_err());
        assert!(require_install_verification(
            EnvType::PowerShell,
            &crate::sources::VerificationInfo::trusted_sha256(digest, "untrusted fixture")
        ).is_err());
    }

    #[test]
    fn powershell_version_validation_is_exact_and_fail_closed() {
        assert!(validate_powershell_version("7.6.0", Some("7.6.0")).is_ok());
        let mismatch = validate_powershell_version("7.6.0", Some("7.5.0"))
            .unwrap_err()
            .to_string();
        assert!(mismatch.contains("预期 7.6.0") && mismatch.contains("实际 7.5.0"));
        assert!(validate_powershell_version("7.6.0", None).is_err());
    }

    #[test]
    fn cancellation_prevents_entering_installer_or_committing_extraction() {
        let mut downloading = task();
        downloading.request_cancel().unwrap();
        assert!(downloading.begin_installing("installer".into(), false).is_err());

        let mut extracting = task();
        extracting.begin_installing("unzip".into(), true).unwrap();
        extracting.request_cancel().unwrap();
        assert!(extracting.begin_installing("commit".into(), false).is_err());
    }

    #[test]
    fn external_installer_and_finished_tasks_reject_cancellation() {
        let mut task = task();
        task.begin_installing("installer".into(), false).unwrap();
        assert!(task.request_cancel().is_err());
        assert!(!task.cancel.load(Ordering::SeqCst));
        task.snapshot.status = TaskStatus::Done;
        assert!(task.request_cancel().is_err());
    }

    #[test]
    fn reservations_reject_aliases_and_release_after_failure() {
        let dir = crate::test_support::TestDir::new();
        let targets = Arc::new(Mutex::new(HashSet::new()));
        let path = dir.path().join("node-22");
        let reservation = TargetReservation::acquire(targets.clone(), &path).unwrap();
        assert!(TargetReservation::acquire(targets.clone(), &path).is_err());
        assert!(TargetReservation::acquire(targets.clone(), &dir.path().join("NODE-22")).is_err());
        assert!(TargetReservation::acquire(targets.clone(), &dir.path().join("node-22.")).is_err());
        let other = TargetReservation::acquire(targets.clone(), &dir.path().join("node-24")).unwrap();
        drop(other);
        drop(reservation);
        let retry = TargetReservation::acquire(targets.clone(), &path).unwrap();
        std::fs::create_dir(&path).unwrap();
        drop(retry);
        assert!(TargetReservation::acquire(targets, &path).is_err());
    }

    #[test]
    fn simultaneous_starts_reserve_only_one_target() {
        let dir = crate::test_support::TestDir::new();
        let path = dir.path().join("node-22");
        let targets = Arc::new(Mutex::new(HashSet::new()));
        let barrier = Arc::new(std::sync::Barrier::new(8));
        let winners = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..8).map(|_| {
                let targets = targets.clone();
                let barrier = barrier.clone();
                let path = path.clone();
                scope.spawn(move || {
                    barrier.wait();
                    let reservation = TargetReservation::acquire(targets, &path);
                    barrier.wait();
                    reservation.is_ok()
                })
            }).collect();
            handles.into_iter().map(|h| h.join().unwrap()).filter(|won| *won).count()
        });
        assert_eq!(winners, 1);
    }
}
