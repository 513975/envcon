use crate::{
    error::{AppError, Result},
    global_reinstall,
    package_managers::ItemResult,
    pip_reinstall,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    io::{Seek, Write},
    path::Path,
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    },
};

#[derive(Clone)]
pub enum SourcePlan {
    Global(global_reinstall::Plan),
    Pip(pip_reinstall::Plan),
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    pub name: String,
    pub version: String,
    pub path: Option<String>,
    pub reason: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    pub token: String,
    pub tool: String,
    pub source: String,
    pub destination: String,
    pub packages: Vec<Candidate>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupResult {
    pub report: String,
    pub tool: String,
    pub source: String,
    pub destination: String,
    pub items: Vec<ItemResult>,
}

static SOURCES: Mutex<BTreeMap<String, SourcePlan>> = Mutex::new(BTreeMap::new());
static PREVIEWS: Mutex<BTreeMap<String, Preview>> = Mutex::new(BTreeMap::new());
static SERIAL: AtomicU64 = AtomicU64::new(1);

pub(crate) fn verify_separate(source: &Path, destination: &Path) -> Result<()> {
    let source_real = source.canonicalize()?;
    let dest_real = destination.canonicalize()?;
    if !crate::detect::system::paths_eq(source, &source_real)
        || !crate::detect::system::paths_eq(destination, &dest_real)
        || crate::detect::system::path_within(source, destination)
        || crate::detect::system::path_within(destination, source)
    {
        return Err(AppError::msg("新旧目录重叠或路径已重定向，禁止清理"));
    }
    Ok(())
}

pub fn remember(tool: &str, source: SourcePlan) {
    SOURCES.lock().unwrap().insert(tool.into(), source);
    PREVIEWS.lock().unwrap().remove(tool);
}

fn context(tool: &str, expected_destination: &str) -> Result<(SourcePlan, Vec<ItemResult>)> {
    let plan = SOURCES
        .lock()
        .unwrap()
        .get(tool)
        .cloned()
        .ok_or_else(|| AppError::msg("没有本次运行内的重装来源记录，不能自动删除旧包"))?;
    let (status, destination, items) = if tool == "pip" {
        let task = pip_reinstall::status().ok_or_else(|| AppError::msg("重装任务不存在"))?;
        (task.status, task.destination, task.items)
    } else {
        let task = global_reinstall::status(tool).ok_or_else(|| AppError::msg("重装任务不存在"))?;
        (task.status, task.destination, task.items)
    };
    if status != "done"
        || destination != expected_destination
        || items.is_empty()
        || items.iter().any(|p| p.status != "installed")
    {
        return Err(AppError::msg(
            "仅可清理当前已全部成功的重装任务；任务已变化或仍有未完成项",
        ));
    }
    let planned = match &plan {
        SourcePlan::Global(p) => &p.destination,
        SourcePlan::Pip(p) => &p.destination,
    };
    if planned != &destination {
        return Err(AppError::msg("来源记录与重装目标不一致"));
    }
    Ok((plan, items))
}

async fn candidates(plan: &SourcePlan, items: &[ItemResult]) -> Result<Vec<Candidate>> {
    match plan {
        SourcePlan::Global(p) => global_reinstall::cleanup_candidates(p, items).await,
        SourcePlan::Pip(p) => pip_reinstall::cleanup_candidates(p, items).await,
    }
}

pub async fn preview(tool: String, destination: String) -> Result<Preview> {
    let _guard = crate::migration::LOCK
        .try_lock()
        .map_err(|_| AppError::msg("正在重装、迁移或清理，请稍后重试"))?;
    let (plan, items) = context(&tool, &destination)?;
    let packages = candidates(&plan, &items).await?;
    let source = match plan {
        SourcePlan::Global(p) => p.source,
        SourcePlan::Pip(p) => p.source,
    };
    let value = Preview {
        token: format!("{}-{}", std::process::id(), SERIAL.fetch_add(1, Ordering::SeqCst)), tool: tool.clone(), source, destination, packages,
        warnings: vec!["卸载勾选的旧包及管理器判定不再需要的依赖；保留运行时、其他顶层包、缓存和 PATH。依赖这些包的旧项目可能不再运行。".into(),
            if tool == "pnpm" || tool == "yarn" || crate::package_managers::isolated(&tool) { "请先验证新环境并切换命令路径。外置旧命令入口可能保留；无法确认范围时会拒绝清理。" } else { "请先验证新环境并切换所需命令路径；卸载不会放入回收站。" }.into()],
    };
    PREVIEWS.lock().unwrap().insert(tool, value.clone());
    Ok(value)
}

fn selected(preview: &Preview, names: &[String]) -> Result<Vec<Candidate>> {
    if names.is_empty() {
        return Err(AppError::msg("请至少选择一个旧包"));
    }
    let mut selected = BTreeMap::new();
    for name in names {
        let package = preview
            .packages
            .iter()
            .find(|p| &p.name == name && p.reason.is_none())
            .ok_or_else(|| AppError::msg("所选旧包不可清理，请重新预览"))?;
        selected.insert(name, package.clone());
    }
    Ok(selected.into_values().collect())
}

pub async fn execute(tool: String, token: String, names: Vec<String>) -> Result<CleanupResult> {
    let guard = crate::migration::LOCK
        .try_lock()
        .map_err(|_| AppError::msg("正在重装、迁移或清理，请稍后重试"))?;
    let preview = PREVIEWS
        .lock()
        .unwrap()
        .remove(&tool)
        .filter(|p| p.token == token)
        .ok_or_else(|| AppError::msg("清理预览已失效，请重新预览"))?;
    let selected = selected(&preview, &names)?;
    let (plan, items) = context(&tool, &preview.destination)?;
    let fresh = candidates(&plan, &items).await?;
    if fresh != preview.packages {
        return Err(AppError::msg(
            "新旧包或安装路径已变化，请重新预览；未删除任何包",
        ));
    }
    // The worker owns the lock and continues recording results if the webview closes.
    tauri::async_runtime::spawn(async move {
        let _guard = guard;
        let report =
            Path::new(&preview.destination).join(format!("envcon-cleanup-{}.json", preview.token));
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&report)?;
        let mut result = CleanupResult {
            report: report.to_string_lossy().into(),
            tool: preview.tool,
            source: preview.source,
            destination: preview.destination,
            items: selected
                .iter()
                .map(|p| ItemResult {
                    name: p.name.clone(),
                    version: p.version.clone(),
                    status: "pending".into(),
                    detail: p.path.clone(),
                })
                .collect(),
        };
        write_report(&mut file, &result)?;
        for (i, package) in selected.iter().enumerate() {
            // Check both installations again before each uninstall, including shared dependency changes.
            let check = candidates(&plan, &items).await;
            let outcome = match check {
                Ok(rows) if rows.iter().any(|p| p == package) => match &plan {
                    SourcePlan::Global(p) => global_reinstall::remove_old(p, package).await,
                    SourcePlan::Pip(p) => pip_reinstall::remove_old(p, package).await,
                },
                Ok(_) => Err(AppError::msg("包版本、位置或依赖条件已变化，已保留旧包")),
                Err(e) => Err(e),
            };
            result.items[i].status = if outcome.is_ok() { "removed" } else { "failed" }.into();
            result.items[i].detail = outcome.err().map(|e| e.to_string());
            write_report(&mut file, &result)?;
        }
        Ok(result)
    })
    .await
    .map_err(|e| AppError::msg(format!("清理任务异常: {e}")))?
}

fn write_report(file: &mut std::fs::File, result: &CleanupResult) -> Result<()> {
    let json = serde_json::to_vec_pretty(result)?;
    file.rewind()?;
    file.write_all(&json)?;
    file.set_len(json.len() as u64)?;
    file.sync_all()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cleanup_requires_current_successful_task_and_allowlisted_names() {
        assert!(context("npm", "D:/untrusted").is_err());
        let p = Preview {
            tool: "npm".into(),
            token: "x".into(),
            source: "D:/old".into(),
            destination: "D:/new".into(),
            warnings: vec![],
            packages: vec![
                Candidate {
                    name: "ok".into(),
                    version: "1.0.0".into(),
                    path: None,
                    reason: None,
                },
                Candidate {
                    name: "blocked".into(),
                    version: "1.0.0".into(),
                    path: None,
                    reason: Some("版本变化".into()),
                },
            ],
        };
        assert!(selected(&p, &[]).is_err());
        assert!(selected(&p, &["--prefix".into()]).is_err());
        assert!(selected(&p, &["blocked".into()]).is_err());
        assert_eq!(selected(&p, &["ok".into(), "ok".into()]).unwrap().len(), 1);
    }

    #[test]
    fn rejects_overlapping_and_redirected_cleanup_roots() {
        let dir = crate::test_support::TestDir::new();
        let old = dir.path().join("old");
        let new = dir.path().join("new");
        std::fs::create_dir(&old).unwrap();
        std::fs::create_dir(&new).unwrap();
        assert!(verify_separate(&old, &new).is_ok());
        assert!(verify_separate(&old, &old).is_err());
        let link = dir.path().join("redirected");
        junction::create(&old, &link).unwrap();
        assert!(verify_separate(&link, &new).is_err());
        std::fs::remove_dir(link).unwrap();
    }
}
