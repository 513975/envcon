use std::collections::HashMap;
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

pub struct TaskManager {
    next_id: AtomicU64,
    tasks: Arc<Mutex<HashMap<u64, TaskInner>>>,
    dl: Arc<DownloadManager>,
}

impl TaskManager {
    pub fn new() -> Self {
        Self {
            next_id: AtomicU64::new(1),
            tasks: Arc::new(Mutex::new(HashMap::new())),
            dl: Arc::new(DownloadManager::new()),
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
        let flag = {
            let tasks = self.tasks.lock().unwrap();
            tasks.get(&id).map(|t| t.cancel.clone())
        };
        match flag {
            Some(f) => {
                f.store(true, Ordering::SeqCst);
                Ok(())
            }
            None => Err(AppError::msg(format!("任务 {id} 不存在"))),
        }
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
        let root = cfg
            .resolve_root()
            .ok_or_else(|| AppError::msg("尚未设置管理根目录"))?;
        let final_dir = root
            .join("envs")
            .join(env_type.folder())
            .join(&target_name);
        if final_dir.exists() {
            return Err(AppError::msg(format!(
                "目标目录已存在: {}(请换一个名称或先卸载)",
                final_dir.display()
            )));
        }

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
    let file_stem = format!("{}-{}", env_type.folder().trim_end_matches('s'), target_name);
    let dest = cfg
        .downloads_dir(root)
        .join(format!("{}.{}", file_stem, spec.method.file_ext()));

    let dl_app = app.clone();
    let dest_path = dl
        .download(spec, &dest, move |downloaded, total, speed| {
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

    // 安装
    update_status(
        tasks,
        id,
        app,
        TaskStatus::Installing {
            message: "正在安装…".into(),
        },
    );

    let cat_dir = root.join("envs").join(env_type.folder());
    let final_dir = cat_dir.join(target_name);
    tokio::fs::create_dir_all(&cat_dir).await?;

    let result = match &spec.method {
        InstallMethod::Unzip { strip_top } => {
            let tmp_dir = cat_dir.join(format!(".tmp-{target_name}"));
            let _ = tokio::fs::remove_dir_all(&tmp_dir).await;
            let app2 = app.clone();
            let tasks2 = tasks.clone();
            let unzip_result =
                download::unzip(&dest_path, &tmp_dir, *strip_top, move |p| {
                    let msg = format!("解压中 {:.0}%", p * 100.0);
                    update_status(&tasks2, id, &app2, TaskStatus::Installing { message: msg });
                })
                .await;
            let r = match unzip_result {
                Ok(()) => tokio::fs::rename(&tmp_dir, &final_dir)
                    .await
                    .map_err(|e| AppError::msg(format!("移动到最终目录失败: {e}"))),
                Err(e) => Err(e),
            };
            // 失败清理
            if r.is_err() {
                let _ = tokio::fs::remove_dir_all(&tmp_dir).await;
            }
            r
        }
        InstallMethod::Installer { args } => {
            let app2 = app.clone();
            let args = args.clone();
            download::run_installer(&dest_path, &args, move |line| {
                if !line.trim().is_empty() {
                    update_status(tasks, id, &app2, TaskStatus::Installing { message: line });
                }
            })
            .await
        }
        InstallMethod::RustupInit { dist_server } => {
            let rustup_home = final_dir.join("rustup-home");
            let cargo_home = final_dir.join("cargo-home");
            let app2 = app.clone();
            download::run_rustup_init(
                &dest_path,
                &spec_version(target_name),
                &rustup_home,
                &cargo_home,
                dist_server.as_deref(),
                move |line| {
                    if !line.trim().is_empty() {
                        update_status(tasks, id, &app2, TaskStatus::Installing { message: line });
                    }
                },
            )
            .await
        }
    };

    // 清理安装包
    let _ = tokio::fs::remove_file(&dest_path).await;

    result
}

/// target_name 可能是 rust-stable 之类;rustup channel 从 version 提取
fn spec_version(target_name: &str) -> String {
    let lowered = target_name.to_lowercase();
    for ch in ["stable", "beta", "nightly"] {
        if lowered.contains(ch) {
            return ch.to_string();
        }
    }
    "stable".to_string()
}
