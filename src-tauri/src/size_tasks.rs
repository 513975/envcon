use std::collections::HashMap;
use std::os::windows::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};
use serde::Serialize;
use crate::error::{AppError, Result};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SizeSnapshot {
    pub id: u64,
    pub key: String,
    pub path: String,
    pub status: String,
    pub bytes: u64,
    pub files: u64,
    pub directories: u64,
    pub skipped_links: u64,
    pub issues: u64,
    pub detail: Option<String>,
    pub current_path: String,
    pub complete: bool,
    pub elapsed_ms: u64,
}

struct Task { snapshot: SizeSnapshot, cancel: Arc<AtomicBool> }
pub struct SizeTasks {
    tasks: Arc<Mutex<HashMap<u64, Task>>>,
    next: AtomicU64,
}
impl Default for SizeTasks {
    fn default() -> Self { Self { tasks: Default::default(), next: AtomicU64::new(1) } }
}

impl SizeTasks {
    pub fn invalidate(&self) {
        let mut tasks = self.tasks.lock().unwrap();
        for task in tasks.values() { task.cancel.store(true, Ordering::Relaxed); }
        tasks.retain(|_, task| task.snapshot.status == "running");
    }

    pub fn snapshots(&self) -> Vec<SizeSnapshot> {
        let mut result: Vec<_> = self.tasks.lock().unwrap().values().map(|t| t.snapshot.clone()).collect();
        result.sort_by_key(|s| s.id);
        result
    }

    pub fn cancel(&self, id: u64) -> Result<()> {
        let tasks = self.tasks.lock().unwrap();
        let task = tasks.get(&id).ok_or_else(|| AppError::msg("大小统计任务不存在"))?;
        task.cancel.store(true, Ordering::Relaxed);
        Ok(())
    }

    pub fn start(&self, key: String, path: PathBuf) -> Result<u64> {
        let identity = path.canonicalize()?;
        if !identity.is_dir() { return Err(AppError::msg("统计目标不是目录")); }
        let mut tasks = self.tasks.lock().unwrap();
        if let Some(existing) = tasks.values().find(|t| t.snapshot.status == "running") {
            if existing.snapshot.key == key && existing.snapshot.path == path.to_string_lossy() { return Ok(existing.snapshot.id); }
            return Err(AppError::msg("已有大小统计正在进行，请完成或取消后重试"));
        }
        if tasks.len() >= 32 {
            if let Some(oldest) = tasks.keys().min().copied() { tasks.remove(&oldest); }
        }
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let cancel = Arc::new(AtomicBool::new(false));
        let initial = SizeSnapshot { id, key, path: path.to_string_lossy().into(), status: "running".into(), bytes: 0, files: 0, directories: 0, skipped_links: 0, issues: 0, detail: None, current_path: String::new(), complete: false, elapsed_ms: 0 };
        tasks.insert(id, Task { snapshot: initial.clone(), cancel: cancel.clone() });
        let state = self.tasks.clone();
        tokio::task::spawn_blocking(move || {
            let mut result = measure(&identity, initial, &cancel, |snapshot| {
                if let Some(task) = state.lock().unwrap().get_mut(&id) { task.snapshot = snapshot.clone(); }
            });
            if path.canonicalize().ok().as_ref() != Some(&identity) {
                result.complete = false;
                result.status = "partial".into();
                result.detail = Some("统计期间目标目录或链接发生变化，请重新统计".into());
            }
            if let Some(task) = state.lock().unwrap().get_mut(&id) {
                if task.cancel.load(Ordering::Relaxed) {
                    result.complete = false;
                    result.status = "canceled".into();
                    result.detail = Some("已取消或环境已变更，请重新统计；当前仅为部分结果".into());
                }
                task.snapshot = result;
            }
        });
        Ok(id)
    }
}

fn measure(path: &Path, mut snapshot: SizeSnapshot, cancel: &AtomicBool, mut progress: impl FnMut(&SizeSnapshot)) -> SizeSnapshot {
    let start = Instant::now();
    let mut last = Instant::now();
    let mut pending = vec![path.to_path_buf()];
    let mut canceled = false;
    while let Some(dir) = pending.pop() {
        if cancel.load(Ordering::Relaxed) { canceled = true; break; }
        snapshot.current_path = dir.to_string_lossy().into();
        // Do not follow child reparse points, including junction loops and cloud placeholders.
        match std::fs::symlink_metadata(&dir) {
            Ok(m) if m.file_attributes() & 0x400 != 0 => { snapshot.skipped_links += 1; continue; }
            Err(e) => { record_issue(&mut snapshot, &dir, e); continue; }
            _ => {}
        }
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(e) => { record_issue(&mut snapshot, &dir, e); continue; }
        };
        snapshot.directories += 1;
        for entry in entries {
            if cancel.load(Ordering::Relaxed) { canceled = true; break; }
            match entry.and_then(|e| Ok((e.path(), std::fs::symlink_metadata(e.path())?))) {
                Ok((_, meta)) if meta.file_attributes() & 0x400 != 0 => snapshot.skipped_links += 1,
                Ok((path, meta)) if meta.is_dir() => pending.push(path),
                Ok((_, meta)) => { snapshot.bytes = snapshot.bytes.saturating_add(meta.len()); snapshot.files += 1; }
                Err(e) => record_issue(&mut snapshot, &dir, e),
            }
            if last.elapsed() >= Duration::from_millis(100) {
                snapshot.elapsed_ms = start.elapsed().as_millis() as u64;
                progress(&snapshot);
                // Yield disk time; background sizing must not monopolize the storage device.
                std::thread::sleep(Duration::from_millis(2));
                last = Instant::now();
            }
        }
        if canceled { break; }
    }
    snapshot.elapsed_ms = start.elapsed().as_millis() as u64;
    snapshot.complete = !canceled && snapshot.issues == 0 && snapshot.skipped_links == 0;
    snapshot.status = if canceled { "canceled" } else if snapshot.complete { "done" } else { "partial" }.into();
    if canceled { snapshot.detail = Some("已取消，显示已读取部分的大小".into()); }
    else if snapshot.skipped_links > 0 && snapshot.detail.is_none() { snapshot.detail = Some("未跟随子目录链接或其他重解析点，大小不含这些目标".into()); }
    snapshot.current_path.clear();
    snapshot
}

fn record_issue(snapshot: &mut SizeSnapshot, path: &Path, error: std::io::Error) {
    snapshot.issues += 1;
    if snapshot.detail.is_none() { snapshot.detail = Some(format!("部分目录无法读取：{}: {error}", path.display())); }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn initial() -> SizeSnapshot { SizeSnapshot { id: 0, key: "fixture".into(), path: String::new(), status: "running".into(), bytes: 0, files: 0, directories: 0, skipped_links: 0, issues: 0, detail: None, current_path: String::new(), complete: false, elapsed_ms: 0 } }

    #[test]
    fn sizes_files_reports_missing_and_skips_link_cycles() {
        let dir = crate::test_support::TestDir::new();
        std::fs::write(dir.path().join("data"), b"12345").unwrap();
        let cancel = AtomicBool::new(false);
        let complete = measure(dir.path(), initial(), &cancel, |_| {});
        assert!(complete.complete); assert_eq!(complete.bytes, 5);
        junction::create(dir.path(), dir.path().join("loop")).unwrap();
        let partial = measure(dir.path(), initial(), &cancel, |_| {});
        assert_eq!(partial.bytes, 5); assert_eq!(partial.skipped_links, 1); assert!(!partial.complete);
        let missing = measure(&dir.path().join("missing"), initial(), &cancel, |_| {});
        assert_eq!(missing.status, "partial"); assert_eq!(missing.issues, 1);
    }

    #[test]
    fn cancellation_is_not_a_successful_zero_size() {
        let dir = crate::test_support::TestDir::new();
        let result = measure(dir.path(), initial(), &AtomicBool::new(true), |_| {});
        assert_eq!(result.status, "canceled"); assert!(!result.complete); assert!(result.detail.is_some());
    }

    #[tokio::test]
    async fn running_tasks_coalesce_limit_work_and_invalidate() {
        let dir = crate::test_support::TestDir::new();
        let tasks = SizeTasks::default();
        let mut snapshot = initial();
        snapshot.path = dir.path().to_string_lossy().into();
        let cancel = Arc::new(AtomicBool::new(false));
        tasks.tasks.lock().unwrap().insert(0, Task { snapshot, cancel: cancel.clone() });
        assert_eq!(tasks.start("fixture".into(), dir.path().into()).unwrap(), 0);
        assert!(tasks.start("other".into(), dir.path().into()).is_err());
        tasks.cancel(0).unwrap();
        assert!(cancel.load(Ordering::Relaxed));
        tasks.invalidate();
        assert!(tasks.start("other".into(), dir.path().into()).is_err());
        tasks.tasks.lock().unwrap().get_mut(&0).unwrap().snapshot.status = "canceled".into();
        tasks.invalidate();
        assert!(tasks.snapshots().is_empty());
    }

    #[tokio::test]
    async fn external_root_can_be_measured_without_modifying_it() {
        let dir = crate::test_support::TestDir::new();
        std::fs::create_dir(dir.path().join("external")).unwrap();
        std::fs::write(dir.path().join("external/keep"), b"123").unwrap();
        junction::create(dir.path().join("external"), dir.path().join("link")).unwrap();
        let tasks = SizeTasks::default();
        let id = tasks.start("fixture".into(), dir.path().join("link")).unwrap();
        tokio::time::timeout(Duration::from_secs(2), async {
            loop { if tasks.snapshots()[0].status != "running" { break; } tokio::time::sleep(Duration::from_millis(5)).await; }
        }).await.unwrap();
        let result = tasks.snapshots().pop().unwrap();
        assert_eq!(result.id, id); assert_eq!(result.bytes, 3); assert!(result.complete);
        assert!(dir.path().join("external/keep").is_file());
    }
}
