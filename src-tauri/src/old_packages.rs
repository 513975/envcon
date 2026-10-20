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
static SCANNED: Mutex<BTreeMap<String, (Preview, SourcePlan)>> = Mutex::new(BTreeMap::new());
static SERIAL: AtomicU64 = AtomicU64::new(1);

pub(crate) fn verify_separate(source: &Path, destination: &Path) -> Result<()> {
    if crate::detect::system::has_reparse_component(source)
        || crate::detect::system::has_reparse_component(destination)
    {
        return Err(AppError::msg("新旧目录不能是符号链接或 Junction"));
    }
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

async fn scanned_candidates(plan: &SourcePlan) -> Result<Vec<Candidate>> {
    match plan {
        SourcePlan::Global(p) => global_reinstall::scanned_cleanup_candidates(p).await,
        SourcePlan::Pip(p) => pip_reinstall::scanned_cleanup_candidates(p).await,
    }
}

pub async fn preview_scanned(
    tool: String,
    source: String,
    executable: String,
    reports: String,
) -> Result<Preview> {
    let _guard = crate::migration::LOCK
        .try_lock()
        .map_err(|_| AppError::msg("正在扫描或修改包，请稍后重试"))?;
    crate::package_managers::definition(&tool)?;
    let scan = crate::package_scan::status().ok_or_else(|| AppError::msg("请先扫描旧包"))?;
    if scan.status == "running"
        || !scan
            .sources
            .iter()
            .any(|s| s.tool == tool && s.path == source && s.error.is_none())
    {
        return Err(AppError::msg("来源不在已完成的扫描结果中，请重新扫描"));
    }
    std::fs::create_dir_all(&reports)?;
    let plan = if tool == "pip" {
        SourcePlan::Pip(pip_reinstall::Plan {
            source: source.clone(),
            source_kind: "directory".into(),
            python: String::new(),
            destination: reports.clone(),
            source_version: String::new(),
            target_version: String::new(),
            packages: vec![],
        })
    } else {
        SourcePlan::Global(
            global_reinstall::scanned_cleanup_plan(
                tool.clone(),
                executable,
                source.clone(),
                reports.clone(),
            )
            .await?,
        )
    };
    let packages = scanned_candidates(&plan).await?;
    let preview = Preview {
        token: format!("scan-{}-{}", std::process::id(), SERIAL.fetch_add(1, Ordering::SeqCst)),
        tool: tool.clone(), source, destination: reports, packages,
        warnings: vec!["直接卸载所选来源中的包，无需先重装。此操作不可撤销，不会放入回收站，依赖这些包的命令或项目可能失效。".into(),
            "包管理器可能同时移除不再需要的依赖；运行时和其他顶层包保留，外置命令入口可能残留。".into()],
    };
    SCANNED
        .lock()
        .unwrap()
        .insert(tool, (preview.clone(), plan));
    Ok(preview)
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
    let scanned = if token.starts_with("scan-") {
        SCANNED.lock().unwrap().remove(&tool)
    } else {
        None
    };
    let (preview, plan, items, direct) = if let Some((preview, plan)) = scanned {
        if preview.token != token {
            return Err(AppError::msg("删除预览已失效，请重新预览"));
        }
        (preview, plan, vec![], true)
    } else {
        let preview = PREVIEWS
            .lock()
            .unwrap()
            .remove(&tool)
            .filter(|p| p.token == token)
            .ok_or_else(|| AppError::msg("清理预览已失效，请重新预览"))?;
        let (plan, items) = context(&tool, &preview.destination)?;
        (preview, plan, items, false)
    };
    let selected = selected(&preview, &names)?;
    let fresh = if direct {
        scanned_candidates(&plan).await?
    } else {
        candidates(&plan, &items).await?
    };
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
            let check = if direct {
                scanned_candidates(&plan).await
            } else {
                candidates(&plan, &items).await
            };
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

/// How many cleanup reports to keep. Older ones are removed after a new report
/// is written; this bounds a directory that would otherwise grow forever.
pub const RETAIN_REPORTS: usize = 20;

/// Delete the oldest cleanup reports so at most `retain` remain.
///
/// Only files this module created (`envcon-cleanup-*.json`) are considered:
/// anything else in the directory, including a user's own notes, is left
/// untouched. Returns how many files were actually removed.
pub fn prune_reports(dir: &Path, retain: usize) -> Result<usize> {
    if !dir.is_dir() {
        return Ok(0);
    }
    // A redirected directory could make this delete files outside the reports
    // folder, so refuse rather than guess.
    if crate::detect::system::has_reparse_component(dir) {
        return Err(AppError::msg("清理报告目录被重定向，已跳过保留期限清理"));
    }
    let mut reports: Vec<(std::time::SystemTime, std::path::PathBuf)> = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => continue,
        };
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !name.starts_with("envcon-cleanup-") || !name.ends_with(".json") {
            continue;
        }
        let meta = match entry.metadata() {
            Ok(meta) => meta,
            Err(_) => continue,
        };
        if !meta.is_file() {
            continue;
        }
        // An unreadable timestamp sorts before every known time, so such a file
        // is pruned first rather than being kept forever.
        reports.push((
            meta.modified().unwrap_or(std::time::UNIX_EPOCH),
            path,
        ));
    }
    if reports.len() <= retain {
        return Ok(0);
    }
    reports.sort();
    let stale = reports.len() - retain;
    let mut removed = 0;
    for (_, path) in reports.into_iter().take(stale) {
        if std::fs::remove_file(&path).is_ok() {
            removed += 1;
        }
    }
    Ok(removed)
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

    #[test]
    fn rejects_cleanup_roots_with_redirected_ancestors() {
        let dir = crate::test_support::TestDir::new();
        let redirected_parent = dir.path().join("redirected-parent");
        let outside = dir.path().join("outside");
        std::fs::create_dir_all(redirected_parent.join("source")).unwrap();
        std::fs::create_dir_all(outside.join("destination")).unwrap();
        let source_link = dir.path().join("source-parent");
        junction::create(&redirected_parent, &source_link).unwrap();

        assert!(verify_separate(
            &source_link.join("source"),
            &outside.join("destination")
        )
        .is_err());
    }

    #[test]
    fn cleanup_report_pruning_keeps_the_newest_and_leaves_foreign_files_alone() {
        let dir = crate::test_support::TestDir::new();
        let reports = dir.path().join("cleanup-reports");
        std::fs::create_dir_all(&reports).unwrap();

        // Ten reports created with deliberately increasing mtimes so the
        // ordering is deterministic rather than depending on filesystem tick.
        for index in 0..10 {
            let path = reports.join(format!("envcon-cleanup-{index}.json"));
            std::fs::write(&path, "{}").unwrap();
            set_mtime(&path, 1_700_000_000 + index);
        }
        // Files this module did not create must survive pruning.
        std::fs::write(reports.join("notes.txt"), "keep me").unwrap();
        std::fs::write(reports.join("envcon-cleanup-notjson.md"), "keep me").unwrap();
        std::fs::write(reports.join("other-report.json"), "keep me").unwrap();

        assert_eq!(prune_reports(&reports, 4).unwrap(), 6);
        let mut remaining: Vec<String> = std::fs::read_dir(&reports)
            .unwrap()
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        remaining.sort();
        // The four newest reports survive; the six oldest are gone.
        assert_eq!(
            remaining
                .iter()
                .filter(|name| name.starts_with("envcon-cleanup-") && name.ends_with(".json"))
                .count(),
            4
        );
        assert!(remaining.contains(&"envcon-cleanup-9.json".to_string()));
        assert!(remaining.contains(&"envcon-cleanup-6.json".to_string()));
        assert!(!remaining.contains(&"envcon-cleanup-5.json".to_string()));
        assert!(remaining.contains(&"notes.txt".to_string()));
        assert!(remaining.contains(&"envcon-cleanup-notjson.md".to_string()));
        assert!(remaining.contains(&"other-report.json".to_string()));

        // The bound is inclusive: exactly `retain` files are a no-op.
        assert_eq!(prune_reports(&reports, 4).unwrap(), 0);
        // A missing directory is not an error and removes nothing.
        assert_eq!(prune_reports(&dir.path().join("absent"), 4).unwrap(), 0);
    }

    #[test]
    fn pruning_refuses_a_redirected_report_directory() {
        let dir = crate::test_support::TestDir::new();
        let outside = dir.path().join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        for index in 0..3 {
            std::fs::write(outside.join(format!("envcon-cleanup-{index}.json")), "{}").unwrap();
        }
        let redirected = dir.path().join("cleanup-reports");
        junction::create(&outside, &redirected).unwrap();

        // Deleting through a junction would remove files outside the reports
        // folder, so the whole prune is refused instead.
        assert!(prune_reports(&redirected, 1).is_err());
        assert_eq!(std::fs::read_dir(&outside).unwrap().count(), 3);
    }
}

/// Force a file's modification time so ordering fixtures do not depend on
/// filesystem timestamp granularity.
#[cfg(test)]
fn set_mtime(path: &Path, secs: u64) {
    let file = std::fs::OpenOptions::new().write(true).open(path).unwrap();
    let time = std::time::UNIX_EPOCH + std::time::Duration::from_secs(secs);
    file.set_times(std::fs::FileTimes::new().set_modified(time))
        .unwrap();
}
