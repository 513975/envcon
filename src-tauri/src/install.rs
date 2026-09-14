use std::collections::{HashMap, HashSet};
use std::path::Path;
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

pub struct TaskManager {
    next_id: AtomicU64,
    tasks: Arc<Mutex<HashMap<u64, TaskInner>>>,
    dl: Arc<DownloadManager>,
    targets: Arc<Mutex<HashSet<String>>>,
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

    /// 启动安装任务(spawn 到后台)
    pub fn start(
        &self,
        app: AppHandle,
        cfg: AppConfig,
        env_type: EnvType,
        version: String,
        target_name: String,
        source: String,
        url: Option<String>,
    ) -> Result<u64> {
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

        let spec = sources::download_spec(
            env_type,
            &version,
            &final_dir.to_string_lossy(),
            &source,
            url.as_deref(),
        )?;
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

        tauri::async_runtime::spawn(async move {
            let result =
                run_install(&dl, &cfg, &root, env_type, &spec, &target_name, id, &app2, &tasks)
                    .await;
            drop(reservation);
            match result {
                Ok(()) => {
                    // 任务完成,标记后延迟移除(前端可看到 Done 状态)
                    update_status(&tasks, id, &app2, TaskStatus::Done);
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
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
    // 下载
    let file_stem = format!(
        "{}-{}-{}-{id}",
        env_type.folder().trim_end_matches('s'), target_name, std::process::id()
    );
    let dest = cfg
        .downloads_dir(root)
        .join(format!("{}.{}", file_stem, spec.method.file_ext()));

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

    let result = async {
        let cancelable = matches!(spec.method, InstallMethod::Unzip { .. });
        begin_installing(tasks, id, app, "正在安装…", cancelable)?;
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
                    Ok(()) => match begin_installing(tasks, id, app, "正在完成安装…", false) {
                        Ok(()) => tokio::fs::rename(&tmp_dir, &final_dir)
                            .await
                            .map_err(|e| AppError::msg(format!("移动到最终目录失败: {e}"))),
                        Err(e) => Err(e),
                    },
                    Err(e) => Err(e),
                };
                // 失败清理
                if r.is_err() {
                    let _ = tokio::fs::remove_dir_all(&tmp_dir).await;
                }
                r
            }
            InstallMethod::Installer { args } => {
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
                recover_failed_install(&final_dir, result)
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
                recover_failed_install(&final_dir, result)
            }
        }
    }
    .await;

    // 清理安装包
    let _ = tokio::fs::remove_file(&dest_path).await;

    result
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;

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
