use crate::error::{AppError, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::os::windows::fs::OpenOptionsExt;
use std::path::Path;

pub(crate) trait PathStore {
    fn read(&mut self) -> Result<String>;
    fn write(&mut self, value: &str) -> Result<()>;
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Record {
    action: String,
    before: String,
    after: String,
    owned: Vec<String>,
    status: String,
    #[serde(default)]
    confirmed_legacy: Vec<String>,
    #[serde(default)]
    confirmation_status: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Journal {
    schema: u32,
    records: Vec<Record>,
}

impl Default for Journal {
    fn default() -> Self {
        Self { schema: 1, records: Vec::new() }
    }
}

fn load(path: &Path) -> Result<Journal> {
    if crate::detect::system::has_reparse_component(path) {
        return Err(AppError::msg("PATH 来源记录路径被重定向，已停止写入"));
    }
    let journal: Journal = match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Journal::default(),
        Err(error) => return Err(error.into()),
    };
    if journal.schema != 1 {
        return Err(AppError::msg("PATH 来源记录版本不受支持"));
    }
    Ok(journal)
}

fn key(value: &str) -> String {
    crate::detect::system::path_key(Path::new(value.trim().trim_matches('"')))
}

fn owned_at(journal: &Journal, current: &str) -> Vec<String> {
    journal.records.last()
        .filter(|record| record.status == "verified" && record.after == current)
        .map(|record| record.owned.clone())
        .unwrap_or_default()
}

pub(crate) enum Change<'a> {
    Append(&'a [String]),
    Replace(&'a [String]),
    Edit(&'a EditPlan),
    Remove(&'a CleanupPlan, &'a [String]),
    /// Move already-present managed entries ahead of the entries that shadow
    /// them. Nothing is added, removed or rewritten.
    Reorder(&'a ReorderPlan),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EditPlan {
    pub expected_path: String,
    pub journal_snapshot: String,
    pub entries: Vec<String>,
    pub resulting_path: String,
    pub sources: Vec<String>,
}

/// Preserve untouched legacy segments exactly; validate only new spellings.
pub(crate) fn preview_edit(journal_path: &Path, current: &str, entries: &[String]) -> Result<EditPlan> {
    let journal = load(journal_path)?;
    let original = current.split(';').collect::<Vec<_>>();
    let owned = owned_at(&journal, current);
    for entry in entries {
        if entry.contains([';', '\0', '\r', '\n']) {
            return Err(AppError::msg("PATH 单项不能包含分号、空字符或换行"));
        }
        if original.contains(&entry.as_str()) { continue; }
        let expanded_root = entry.strip_prefix('%')
            .and_then(|tail| tail.split_once('%'))
            .is_some_and(|(name, tail)| !name.is_empty() && tail.starts_with(['\\', '/'])
                && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'));
        if entry.trim() != entry || entry.contains('"')
            || !(Path::new(entry).is_absolute() || expanded_root)
        {
            return Err(AppError::msg("新增或修改的 PATH 项必须是绝对路径（可用 %变量%），且不能带引号或首尾空白"));
        }
        if entries.iter().filter(|other| key(other) == key(entry)).count() > 1 {
            return Err(AppError::msg("不能新增重复的 PATH 项"));
        }
    }
    let resulting_path = entries.join(";");
    if resulting_path.encode_utf16().count() >= 32767 {
        return Err(AppError::msg("PATH 超过 Windows 环境变量长度限制"));
    }
    Ok(EditPlan {
        expected_path: current.into(),
        journal_snapshot: format!("{:x}", Sha256::digest(serde_json::to_vec(&journal)?)),
        entries: entries.to_vec(),
        resulting_path,
        sources: entries.iter().map(|entry| {
            if owned.contains(entry) { "EnvCon 已记录；手工保存后不再自动认领" }
            else if original.contains(&entry.as_str()) { "共享 / 来源未确认" }
            else { "本次手工新增 / 修改；不自动认领" }.into()
        }).collect(),
    })
}

/// Read-only plan for fixing a shadowed command by ordering, not by deleting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReorderPlan {
    /// Exact user PATH string the plan was produced from.
    pub expected_path: String,
    /// Managed entry to move to the front, spelled exactly as it appears.
    pub entry: String,
    /// Entries that currently shadow it and therefore move after it.
    pub shadowing: Vec<String>,
    /// PATH after applying the move; bound at preview time.
    pub resulting_path: String,
}

/// Build a reorder plan from the current user PATH. The entry must already be
/// present exactly once, and only entries that are genuinely ahead of it move.
pub(crate) fn preview_reorder(
    current: &str,
    entry: &str,
    shadowing: &[String],
) -> Result<ReorderPlan> {
    if entry.trim() != entry || !Path::new(entry).is_absolute() {
        return Err(AppError::msg("受管入口必须是绝对路径且不能带首尾空白"));
    }
    let segments = current.split(';').collect::<Vec<_>>();
    if segments.iter().filter(|segment| key(segment) == key(entry)).count() != 1 {
        return Err(AppError::msg("受管入口在用户 PATH 中不是唯一出现，无法自动调整顺序"));
    }
    let mut shadow_keys = Vec::new();
    for candidate in shadowing {
        if candidate.trim() != candidate || !Path::new(candidate).is_absolute() {
            return Err(AppError::msg("需调整的 PATH 条目必须是绝对路径"));
        }
        shadow_keys.push(key(candidate));
    }
    let target = segments
        .iter()
        .position(|segment| key(segment) == key(entry))
        .ok_or_else(|| AppError::msg("用户 PATH 中未找到受管入口"))?;
    // Every confirmed conflict must actually sit ahead of the entry; unrelated
    // leading entries move down with the rest, so nothing is left behind.
    for shadow in shadow_keys.iter().filter(|key| !key.is_empty()) {
        let position = segments.iter().position(|segment| key(segment) == *shadow);
        if let Some(position) = position {
            if position > target {
                return Err(AppError::msg("冲突目录当前不在受管入口之前，PATH 已变化"));
            }
        }
    }
    let mut reordered = Vec::with_capacity(segments.len());
    reordered.push(entry);
    reordered.extend(
        segments
            .iter()
            .filter(|segment| key(segment) != key(entry))
            .copied(),
    );
    let resulting_path = reordered.join(";");
    Ok(ReorderPlan {
        expected_path: current.into(),
        entry: entry.into(),
        shadowing: shadowing.to_vec(),
        resulting_path,
    })
}


#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CleanupPlan {
    pub expected_path: String,
    pub journal_snapshot: String,
    pub resulting_path: String,
    pub removed: Vec<String>,
    pub legacy_candidates: Vec<String>,
    pub confirmed_legacy: Vec<String>,
    pub retained: Vec<(String, String)>,
}

impl CleanupPlan {
    pub(crate) fn with_confirmations(&self, confirmations: &[String]) -> Result<Self> {
        if confirmations.iter().collect::<std::collections::HashSet<_>>().len() != confirmations.len()
            || confirmations.iter().any(|entry| !self.legacy_candidates.contains(entry))
        {
            return Err(AppError::msg("历史 PATH 确认包含重复、候选外或不可清理的条目"));
        }
        let mut plan = self.clone();
        plan.removed.retain(|entry| !self.confirmed_legacy.contains(entry));
        plan.confirmed_legacy = self.legacy_candidates.iter()
            .filter(|entry| confirmations.contains(entry)).cloned().collect();
        plan.removed.extend(plan.confirmed_legacy.iter().cloned());
        // Keep every unselected segment, including its spelling, empty entries and order.
        plan.resulting_path = plan.expected_path.split(';')
            .filter(|entry| !plan.removed.iter().any(|removed| removed == entry))
            .collect::<Vec<_>>().join(";");
        Ok(plan)
    }
}

/// Convenience wrapper with no per-entry confirmations. Only tests use this
/// shape; production always goes through
/// [`preview_cleanup_with_confirmations`] so legacy consent is explicit.
#[cfg(test)]
pub(crate) fn preview_cleanup(
    journal_path: &Path,
    current: &str,
    candidates: &[String],
    shared: &[String],
) -> Result<CleanupPlan> {
    preview_cleanup_with_confirmations(journal_path, current, candidates, shared, &[])
}

pub(crate) fn preview_cleanup_with_confirmations(
    journal_path: &Path,
    current: &str,
    candidates: &[String],
    shared: &[String],
    confirmed_legacy: &[String],
) -> Result<CleanupPlan> {
    let journal = load(journal_path)?;
    let journal_snapshot = format!("{:x}", Sha256::digest(serde_json::to_vec(&journal)?));
    let owned = owned_at(&journal, current);
    let history_blocked = journal.records.last()
        .is_some_and(|record| record.status != "verified");
    let mut removed = Vec::new();
    let mut legacy_candidates = Vec::new();
    let mut retained = Vec::new();
    for candidate in candidates {
        if removed.contains(candidate)
            || legacy_candidates.contains(candidate)
            || retained.iter().any(|(entry, _)| entry == candidate)
        {
            continue;
        }
        let occurrences = current.split(';').filter(|entry| key(entry) == key(candidate)).count();
        let reason = if shared.iter().any(|entry| key(entry) == key(candidate)) {
            Some("仍有其他环境依赖")
        } else if occurrences != 1 || !current.split(';').any(|entry| entry == candidate) {
            Some("条目被改写、重复或已不存在")
        } else {
            None
        };
        if let Some(reason) = reason {
            retained.push((candidate.clone(), reason.into()));
        } else if owned.contains(candidate) {
            removed.push(candidate.clone());
        } else if history_blocked {
            retained.push((candidate.clone(), "存在未完成或未验证的来源记录".into()));
        } else {
            legacy_candidates.push(candidate.clone());
        }
    }
    CleanupPlan {
        expected_path: current.into(), journal_snapshot, resulting_path: String::new(),
        removed, legacy_candidates, confirmed_legacy: Vec::new(), retained,
    }.with_confirmations(confirmed_legacy)
}

pub(crate) fn apply(
    store: &mut impl PathStore,
    journal_path: &Path,
    change: Change<'_>,
) -> Result<Vec<String>> {
    let parent = journal_path.parent().ok_or_else(|| AppError::msg("PATH 来源目录无效"))?;
    if crate::detect::system::has_reparse_component(parent) {
        return Err(AppError::msg("PATH 来源目录被重定向"));
    }
    fs::create_dir_all(parent)?;
    let lock_path = parent.join("writer.lock");
    if crate::detect::system::is_reparse_path(&lock_path) {
        return Err(AppError::msg("PATH 来源锁文件被重定向"));
    }
    let _lock = OpenOptions::new().read(true).write(true).create(true)
        .truncate(false).share_mode(0).open(lock_path)?;
    let mut journal = load(journal_path)?;
    let before = store.read()?;
    let mut owned = owned_at(&journal, &before);
    let mut added = Vec::new();
    let mut confirmed_legacy = Vec::new();
    let (action, after) = match change {
        Change::Append(entries) => {
            if journal.records.last().is_some_and(|record| record.status != "verified") {
                return Err(AppError::msg("PATH 有未完成或未验证的写入记录，请先核对实际值并手工保存后再集成"));
            }
            let mut keys: Vec<_> = before.split(';').map(key).collect();
            for entry in entries {
                if !Path::new(entry).is_absolute() || entry.contains([';', '\0', '"'])
                    || entry.trim() != entry
                {
                    return Err(AppError::msg("PATH 新增项必须是无分号/引号的绝对路径"));
                }
                if !keys.contains(&key(entry)) {
                    keys.push(key(entry));
                    added.push(entry.clone());
                }
            }
            if added.is_empty() {
                if !journal.records.is_empty() {
                    journal.records.push(Record {
                        action: "observe".into(), before: before.clone(), after: before,
                        owned, status: "verified".into(), confirmed_legacy: Vec::new(),
                        confirmation_status: None,
                    });
                    crate::config::atomic_write(journal_path, &serde_json::to_vec_pretty(&journal)?)?;
                }
                return Ok(added);
            }
            owned.extend(added.iter().cloned());
            let separator = if before.is_empty() || before.ends_with(';') { "" } else { ";" };
            ("integrate", format!("{before}{separator}{}", added.join(";")))
        }
        Change::Replace(entries) => {
            if entries.iter().any(|entry| entry.contains([';', '\0'])) {
                return Err(AppError::msg("PATH 条目不能包含分号或空字符"));
            }
            // An explicit manual save/restore is the user-confirmed recovery path
            // for a pending record; it establishes a new baseline without
            // converting any pre-existing entry into automatic ownership.
            owned.clear();
            ("manual", entries.join(";"))
        }
        Change::Edit(plan) => {
            if plan.expected_path != before
                || preview_edit(journal_path, &before, &plan.entries)? != *plan
            {
                return Err(AppError::msg("PATH 或来源记录已变化，请重新读取并预览"));
            }
            owned.clear();
            ("manual", plan.resulting_path.clone())
        }
        Change::Reorder(plan) => {
            // Recompute from the live value so a stale preview cannot reorder a
            // different PATH. Only ordering may change; the set must be equal.
            let checked = preview_reorder(&before, &plan.entry, &plan.shadowing)?;
            if checked != *plan {
                return Err(AppError::msg("用户 PATH 已变化，请重新预览修复"));
            }
            let mut before_set = before.split(';').map(key).collect::<Vec<_>>();
            let mut after_set = checked.resulting_path.split(';').map(key).collect::<Vec<_>>();
            before_set.sort();
            after_set.sort();
            if before_set != after_set || before_set.len() != after_set.len() {
                return Err(AppError::msg("修复只允许调整顺序，不允许增删条目"));
            }
            owned = owned_at(&journal, &before);
            ("reorder", checked.resulting_path)
        }
        Change::Remove(plan, shared) => {            if plan.expected_path != before {
                return Err(AppError::msg("PATH 已变化，请重新预览"));
            }
            let checked = preview_cleanup_with_confirmations(
                journal_path, &before, &plan.removed, shared, &plan.confirmed_legacy,
            )?;
            if checked.journal_snapshot != plan.journal_snapshot
                || checked.removed != plan.removed || checked.resulting_path != plan.resulting_path {
                return Err(AppError::msg("PATH 清理范围缺少来源证据或已变化"));
            }
            confirmed_legacy = checked.confirmed_legacy;
            owned.retain(|entry| !plan.removed.contains(entry));
            (
                if confirmed_legacy.is_empty() { "cleanup" } else { "confirmed-legacy-cleanup" },
                checked.resulting_path,
            )
        }
    };
    journal.records.push(Record {
        action: action.into(), before: before.clone(), after: after.clone(),
        owned, status: "pending".into(),
        confirmation_status: if confirmed_legacy.is_empty() {
            None
        } else {
            Some("confirmed-removing".into())
        },
        confirmed_legacy,
    });
    crate::config::atomic_write(journal_path, &serde_json::to_vec_pretty(&journal)?)?;
    let result = (|| {
        if store.read()? != before {
            return Err(AppError::msg("PATH 在写入前发生变化，已停止"));
        }
        store.write(&after)?;
        if store.read()? != after {
            return Err(AppError::msg("PATH 写入后复查不一致"));
        }
        Ok(())
    })();
    let last = journal.records.last_mut().unwrap();
    last.status = if result.is_ok() { "verified" } else { "unverified" }.into();
    if !last.confirmed_legacy.is_empty() {
        last.confirmation_status = Some(if result.is_ok() { "confirmed-removed" } else { "unverified" }.into());
    }
    // A failed final journal write leaves the persisted pending record untrusted.
    crate::config::atomic_write(journal_path, &serde_json::to_vec_pretty(&journal)?)
        .map_err(|error| AppError::msg(format!("PATH 可能已写入，但来源记录提交失败：{error}")))?;
    result.map_err(|error| AppError::msg(format!("PATH 操作未能验证；可能已执行，请重新读取：{error}")))?;
    Ok(added)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FixtureStore { value: String, writes: usize, fail_write: bool, corrupt_write: bool }
    impl PathStore for FixtureStore {
        fn read(&mut self) -> Result<String> { Ok(self.value.clone()) }
        fn write(&mut self, value: &str) -> Result<()> {
            self.writes += 1;
            if self.fail_write { return Err(AppError::msg("fixture write failure")); }
            self.value = if self.corrupt_write { "unexpected".into() } else { value.into() };
            Ok(())
        }
    }
    fn store(value: &str) -> FixtureStore {
        FixtureStore { value: value.into(), writes: 0, fail_write: false, corrupt_write: false }
    }
    fn entries(values: &[&str]) -> Vec<String> { values.iter().map(|v| (*v).into()).collect() }

    #[test]
    fn manual_edit_preserves_legacy_segments_and_rejects_stale_or_tampered_plans() {
        let dir = crate::test_support::TestDir::new();
        let journal = dir.path().join("system/journal.json");
        let original = r"%SystemRoot%\system32;; C:\legacy ;C:\Windows;";
        let requested = entries(&[r"%SystemRoot%\system32", "", " C:\\legacy ", r"C:\Windows", "", r"D:\tools"]);
        let plan = preview_edit(&journal, original, &requested).unwrap();
        assert_eq!(plan.resulting_path, format!("{original};D:\\tools"));
        assert_eq!(plan.sources[0], "共享 / 来源未确认");
        let mut backend = store(original);
        let mut tampered = plan.clone();
        tampered.resulting_path = r"C:\overwritten".into();
        assert!(apply(&mut backend, &journal, Change::Edit(&tampered)).is_err());
        assert_eq!(backend.writes, 0);
        backend.value.push_str(r";D:\external");
        assert!(apply(&mut backend, &journal, Change::Edit(&plan)).is_err());
        assert_eq!(backend.writes, 0);
        backend.value = original.into();
        apply(&mut backend, &journal, Change::Edit(&plan)).unwrap();
        assert_eq!(backend.value, plan.resulting_path);
        let last = load(&journal).unwrap().records.pop().unwrap();
        assert_eq!(last.status, "verified");
        assert!(last.owned.is_empty());
        // The same PATH with a different journal is still a stale preview.
        backend.value = original.into();
        assert!(apply(&mut backend, &journal, Change::Edit(&plan)).is_err());
        assert_eq!(backend.writes, 1);
    }

    #[test]
    fn manual_edit_validates_new_entries_and_does_not_claim_existing_ones() {
        let dir = crate::test_support::TestDir::new();
        let journal = dir.path().join("journal.json");
        for bad in ["relative", "", "C:\\one;C:\\two", "C:\\bad\0", "C:\\bad\n", "\"C:\\quoted\"", " C:\\space", "%ROOT%relative"] {
            assert!(preview_edit(&journal, r"C:\Windows", &entries(&[bad])).is_err(), "{bad:?}");
        }
        assert!(preview_edit(&journal, "", &entries(&[r"D:\one", r"d:/one"])).is_err());
        assert!(preview_edit(&journal, "", &entries(&[&format!("C:\\{}", "x".repeat(32767))])).is_err());
        let mut backend = store(r"C:\Windows");
        let owned = entries(&[r"D:\managed"]);
        apply(&mut backend, &journal, Change::Append(&owned)).unwrap();
        let plan = preview_edit(&journal, &backend.value, &entries(&[r"C:\Windows", r"D:\managed", r"%TOOLS%\bin"])).unwrap();
        assert!(plan.sources[1].starts_with("EnvCon"));
        apply(&mut backend, &journal, Change::Edit(&plan)).unwrap();
        assert!(preview_cleanup(&journal, &backend.value, &owned, &[]).unwrap().removed.is_empty());
    }

    #[test]
    fn manual_edit_failure_is_unverified_and_scopes_keep_independent_journals() {
        let dir = crate::test_support::TestDir::new();
        let user = dir.path().join("ownership/journal.json");
        let system = dir.path().join("ownership-system/journal.json");
        let mut backend = store(r"C:\Windows");
        apply(&mut backend, &user, Change::Append(&entries(&[r"D:\managed"]))).unwrap();
        let user_bytes = fs::read(&user).unwrap();
        let plan = preview_edit(&system, &backend.value, &entries(&[r"D:\new"])).unwrap();
        backend.corrupt_write = true;
        assert!(apply(&mut backend, &system, Change::Edit(&plan)).is_err());
        assert_eq!(fs::read(&user).unwrap(), user_bytes);
        assert_eq!(load(&system).unwrap().records.last().unwrap().status, "unverified");
    }

    #[test]
    fn reorder_moves_only_the_managed_entry_and_keeps_every_other_segment() {
        let current = r"D:\old-node;D:\user;D:\managed\current\node;D:\other";
        let plan = preview_reorder(
            current,
            r"D:\managed\current\node",
            &[r"D:\old-node".to_string()],
        )
        .unwrap();
        assert_eq!(plan.expected_path, current);
        // Only ordering changes: the same segments survive, spelled as before.
        assert_eq!(
            plan.resulting_path,
            r"D:\managed\current\node;D:\old-node;D:\user;D:\other"
        );
        let mut before = current.split(';').map(key).collect::<Vec<_>>();
        let mut after = plan.resulting_path.split(';').map(key).collect::<Vec<_>>();
        before.sort();
        after.sort();
        assert_eq!(before, after);
    }

    #[test]
    fn reorder_rejects_conflicts_that_are_not_ahead_or_not_identified() {
        // A conflict that is already after the entry means PATH moved.
        assert!(preview_reorder(
            r"D:\managed\current\node;D:\old-node",
            r"D:\managed\current\node",
            &[r"D:\old-node".to_string()],
        )
        .is_err());
        // The managed entry must be unique in PATH.
        assert!(preview_reorder(
            r"D:\managed\current\node;D:\user;D:\managed\current\node",
            r"D:\managed\current\node",
            &[r"D:\user".to_string()],
        )
        .is_err());
        // Non-absolute or padded inputs are refused instead of normalised.
        assert!(preview_reorder(
            r"D:\managed\current\node",
            "relative\\node",
            &[],
        )
        .is_err());
        assert!(preview_reorder(
            r"D:\managed\current\node",
            " D:\\managed\\current\\node",
            &[],
        )
        .is_err());
        assert!(preview_reorder(
            r"D:\managed\current\node",
            r"D:\managed\current\node",
            &["D:\\padded ".to_string()],
        )
        .is_err());
    }

    #[test]
    fn reorder_apply_rechecks_the_live_path_and_never_adds_or_removes() {
        let dir = crate::test_support::TestDir::new();
        let journal = dir.path().join("journal.json");
        let entry = r"D:\managed\current\node";
        let mut backend = store(r"D:\old-node;D:\managed\current\node");
        let plan = preview_reorder(&backend.value, entry, &[r"D:\old-node".to_string()]).unwrap();
        let applied = apply(&mut backend, &journal, Change::Reorder(&plan)).unwrap();
        assert!(applied.is_empty(), "reorder adds no entries");
        assert_eq!(backend.value, r"D:\managed\current\node;D:\old-node");
        let record = load(&journal).unwrap();
        assert_eq!(record.records.last().unwrap().action, "reorder");
        assert_eq!(record.records.last().unwrap().status, "verified");

        // A plan built from an older PATH must be refused.
        let mut stale_backend = store(r"D:\managed\current\node;D:\old-node");
        assert!(apply(&mut stale_backend, &journal, Change::Reorder(&plan)).is_err());
        assert_eq!(stale_backend.value, r"D:\managed\current\node;D:\old-node");
        assert_eq!(stale_backend.writes, 0);
    }

    #[test]
    fn reorder_write_is_verified_before_being_recorded() {
        let dir = crate::test_support::TestDir::new();
        let journal = dir.path().join("journal.json");
        let plan = preview_reorder(
            r"D:\old-node;D:\managed\current\node",
            r"D:\managed\current\node",
            &[r"D:\old-node".to_string()],
        )
        .unwrap();
        let mut backend = store(&plan.expected_path);
        backend.corrupt_write = true;
        assert!(apply(&mut backend, &journal, Change::Reorder(&plan)).is_err());
        let record = load(&journal).unwrap();
        assert_eq!(record.records.last().unwrap().status, "unverified");
        // The failed write never claims ownership of the entry.
        assert!(record.records.last().unwrap().owned.is_empty());
    }

    #[test]
    fn tracked_cleanup_preserves_unowned_segments_and_shared_entries() {        let dir = crate::test_support::TestDir::new();
        let journal = dir.path().join("journal.json");
        let mut backend = store(r"D:\user;;D:\other;");
        let new = entries(&[r"D:\managed\current\node", r"D:\managed\current\python"]);
        assert_eq!(apply(&mut backend, &journal, Change::Append(&new)).unwrap(), new);
        let candidates = entries(&[r"D:\user", &new[0], &new[1]]);
        let plan = preview_cleanup(&journal, &backend.value, &candidates, &[new[1].clone()]).unwrap();
        assert_eq!(plan.removed, vec![new[0].clone()]);
        assert_eq!(plan.retained.len(), 1);
        assert_eq!(plan.legacy_candidates, vec![r"D:\user".to_owned()]);
        apply(&mut backend, &journal, Change::Remove(&plan, &[new[1].clone()])).unwrap();
        assert_eq!(backend.value, format!(r"D:\user;;D:\other;{}", new[1]));
        assert_eq!(load(&journal).unwrap().records.len(), 2);
    }

    #[test]
    fn legacy_cleanup_requires_exact_unique_non_shared_confirmation() {
        let dir = crate::test_support::TestDir::new();
        let journal = dir.path().join("journal.json");
        let current = r"D:\managed\current\node;D:\user";
        let candidate = entries(&[r"D:\managed\current\node"]);
        let plan = preview_cleanup(&journal, current, &candidate, &[]).unwrap();
        assert!(plan.removed.is_empty());
        assert_eq!(plan.legacy_candidates, candidate);

        assert!(preview_cleanup_with_confirmations(
            &journal, current, &candidate, &[], &[r"D:\managed\current\node".into(), r"D:\managed\current\node".into()]
        ).is_err());
        assert!(preview_cleanup_with_confirmations(
            &journal, current, &candidate, &[], &[r"D:\other".into()]
        ).is_err());
        assert!(preview_cleanup_with_confirmations(
            &journal, current, &candidate, &[candidate[0].clone()], &[candidate[0].clone()]
        ).is_err());

        let duplicate_path = format!("{current};{}", candidate[0]);
        let duplicate = preview_cleanup(&journal, &duplicate_path, &candidate, &[]).unwrap();
        assert!(duplicate.legacy_candidates.is_empty());
        assert!(duplicate.removed.is_empty());
        assert_eq!(duplicate.retained[0].1, "条目被改写、重复或已不存在");

        let confirmed = preview_cleanup_with_confirmations(
            &journal, current, &candidate, &[], &candidate
        ).unwrap();
        assert_eq!(confirmed.removed, candidate);
        assert_eq!(confirmed.confirmed_legacy, candidate);
        assert_eq!(confirmed.resulting_path, r"D:\user");
    }

    #[test]
    fn pending_or_unverified_journal_blocks_legacy_confirmation() {
        for status in ["pending", "unverified"] {
            let dir = crate::test_support::TestDir::new();
            let journal = dir.path().join("journal.json");
            let candidate = entries(&[r"D:\managed\current\node"]);
            fs::write(&journal, serde_json::to_vec(&Journal {
                schema: 1,
                records: vec![Record {
                    action: "integrate".into(),
                    before: "".into(),
                    after: candidate[0].clone(),
                    owned: candidate.clone(),
                    status: status.into(),
                    confirmed_legacy: Vec::new(),
                    confirmation_status: None,
                }],
            }).unwrap()).unwrap();
            let plan = preview_cleanup(&journal, &candidate[0], &candidate, &[]).unwrap();
            assert!(plan.legacy_candidates.is_empty());
            assert!(plan.removed.is_empty());
            assert!(plan.retained[0].1.contains("未完成或未验证"));
            assert!(preview_cleanup_with_confirmations(
                &journal, &candidate[0], &candidate, &[], &candidate,
            ).is_err());
            let mut backend = store(&candidate[0]);
            assert!(apply(&mut backend, &journal, Change::Append(&candidate)).is_err());
            assert!(apply(&mut backend, &journal, Change::Append(&entries(&[r"D:\new"]))).is_err());
            assert_eq!(backend.writes, 0);
            assert_eq!(load(&journal).unwrap().records.len(), 1);
        }
    }

    #[test]
    fn legacy_cleanup_binds_journal_snapshot_and_does_not_claim_readded_entries() {
        let dir = crate::test_support::TestDir::new();
        let journal = dir.path().join("journal.json");
        let candidate = entries(&[r"D:\managed\current\node"]);
        let mut backend = store(&format!(r"D:\other;;{};D:\end;", candidate[0]));
        let plan = preview_cleanup_with_confirmations(&journal, &backend.value, &candidate, &[], &candidate).unwrap();
        let all: Vec<_> = backend.value.split(';').map(str::to_owned).collect();
        apply(&mut backend, &journal, Change::Replace(&all)).unwrap();
        backend.writes = 0;
        assert!(apply(&mut backend, &journal, Change::Remove(&plan, &[])).is_err());
        assert_eq!(backend.writes, 0);
        let fresh = preview_cleanup_with_confirmations(&journal, &backend.value, &candidate, &[], &candidate).unwrap();
        apply(&mut backend, &journal, Change::Remove(&fresh, &[])).unwrap();
        assert_eq!(backend.value, r"D:\other;;D:\end;");
        let record = load(&journal).unwrap().records.pop().unwrap();
        assert_eq!(record.action, "confirmed-legacy-cleanup");
        assert_eq!(record.confirmation_status.as_deref(), Some("confirmed-removed"));
        assert!(record.owned.is_empty());
        backend.value.push_str(&candidate[0]);
        let readded = preview_cleanup(&journal, &backend.value, &candidate, &[]).unwrap();
        assert!(readded.removed.is_empty());
        assert_eq!(readded.legacy_candidates, candidate);
    }

    #[test]
    fn legacy_cleanup_interrupted_commit_keeps_confirmed_removing_not_ownership() {
        struct CommitFailureStore {
            value: String,
            journal: std::path::PathBuf,
            held: Option<fs::File>,
        }
        impl PathStore for CommitFailureStore {
            fn read(&mut self) -> Result<String> { Ok(self.value.clone()) }
            fn write(&mut self, value: &str) -> Result<()> {
                self.value = value.into();
                self.held = Some(OpenOptions::new().read(true).share_mode(1).open(&self.journal)?);
                Ok(())
            }
        }
        let dir = crate::test_support::TestDir::new();
        let journal = dir.path().join("journal.json");
        let candidate = entries(&[r"D:\managed\current\node"]);
        let plan = preview_cleanup_with_confirmations(&journal, &candidate[0], &candidate, &[], &candidate).unwrap();
        let mut backend = CommitFailureStore { value: candidate[0].clone(), journal: journal.clone(), held: None };
        assert!(apply(&mut backend, &journal, Change::Remove(&plan, &[])).is_err());
        backend.held.take();
        assert_eq!(backend.value, "");
        let record = load(&journal).unwrap().records.pop().unwrap();
        assert_eq!(record.status, "pending");
        assert_eq!(record.confirmation_status.as_deref(), Some("confirmed-removing"));
        assert_eq!(record.confirmed_legacy, candidate);
        assert!(record.owned.is_empty());
        backend.value = candidate[0].clone();
        let checked = preview_cleanup(&journal, &backend.value, &candidate, &[]).unwrap();
        assert!(checked.removed.is_empty());
        assert!(checked.legacy_candidates.is_empty());
    }

    #[test]
    fn preexisting_aliases_are_not_claimed_and_duplicate_additions_are_collapsed() {
        let dir = crate::test_support::TestDir::new();
        let journal = dir.path().join("journal.json");
        let mut backend = store(r"D:\User\");
        let requested = entries(&[r"d:/user", r"D:\new", r"d:/NEW/"]);
        assert_eq!(apply(&mut backend, &journal, Change::Append(&requested)).unwrap(), entries(&[r"D:\new"]));
        let plan = preview_cleanup(&journal, &backend.value, &entries(&[r"D:\User\", r"D:\new"]), &[]).unwrap();
        assert_eq!(plan.removed, entries(&[r"D:\new"]));
        assert!(plan.retained.is_empty());
        assert_eq!(plan.legacy_candidates, entries(&[r"D:\User\"]));
    }

    #[test]
    fn external_changes_manual_edits_and_backup_restores_revoke_claims() {
        let dir = crate::test_support::TestDir::new();
        let journal = dir.path().join("journal.json");
        let mut backend = store("");
        let new = entries(&[r"D:\managed"]);
        apply(&mut backend, &journal, Change::Append(&new)).unwrap();
        let plan = preview_cleanup(&journal, &backend.value, &new, &[]).unwrap();
        backend.value.push_str(r";D:\external");
        assert!(apply(&mut backend, &journal, Change::Remove(&plan, &[])).is_err());
        assert!(preview_cleanup(&journal, &backend.value, &new, &[]).unwrap().removed.is_empty());
        apply(&mut backend, &journal, Change::Replace(&new)).unwrap();
        assert!(preview_cleanup(&journal, &backend.value, &new, &[]).unwrap().removed.is_empty());
    }

    #[test]
    fn write_or_recheck_failure_never_creates_verified_ownership() {
        for corrupt in [false, true] {
            let dir = crate::test_support::TestDir::new();
            let journal = dir.path().join("journal.json");
            let mut backend = store("");
            backend.fail_write = !corrupt;
            backend.corrupt_write = corrupt;
            let new = entries(&[r"D:\managed"]);
            assert!(apply(&mut backend, &journal, Change::Append(&new)).is_err());
            assert_eq!(load(&journal).unwrap().records[0].status, "unverified");
            assert!(preview_cleanup(&journal, &backend.value, &new, &[]).unwrap().removed.is_empty());
        }
    }

    #[test]
    fn explicit_manual_save_reconciles_pending_record_without_claiming_entries() {
        let dir = crate::test_support::TestDir::new();
        let journal = dir.path().join("journal.json");
        let existing = r"D:\managed\current\node";
        fs::write(&journal, serde_json::to_vec(&Journal {
            schema: 1,
            records: vec![Record {
                action: "integrate".into(), before: "".into(), after: existing.into(),
                owned: vec![existing.into()], status: "pending".into(),
                confirmed_legacy: Vec::new(), confirmation_status: None,
            }],
        }).unwrap()).unwrap();
        let mut backend = store(existing);
        apply(&mut backend, &journal, Change::Replace(&entries(&[existing]))).unwrap();
        let record = load(&journal).unwrap().records.pop().unwrap();
        assert_eq!(record.action, "manual");
        assert_eq!(record.status, "verified");
        assert!(record.owned.is_empty());

        let added = entries(&[r"D:\new"]);
        assert_eq!(apply(&mut backend, &journal, Change::Append(&added)).unwrap(), added);
        let plan = preview_cleanup(&journal, &backend.value, &entries(&[existing]), &[]).unwrap();
        assert!(plan.removed.is_empty());
        assert_eq!(plan.legacy_candidates, entries(&[existing]));
    }

    #[test]
    fn cleanup_rechecks_new_dependencies_and_noop_observation_revokes_stale_claims() {
        let dir = crate::test_support::TestDir::new();
        let journal = dir.path().join("journal.json");
        let new = entries(&[r"D:\managed"]);
        let mut backend = store("");
        apply(&mut backend, &journal, Change::Append(&new)).unwrap();
        let plan = preview_cleanup(&journal, &backend.value, &new, &[]).unwrap();
        assert!(apply(&mut backend, &journal, Change::Remove(&plan, &new)).is_err());
        assert_eq!(backend.writes, 1);
        backend.value.push_str(r";D:\external");
        assert!(apply(&mut backend, &journal, Change::Append(&new)).unwrap().is_empty());
        backend.value = new[0].clone();
        assert!(preview_cleanup(&journal, &backend.value, &new, &[]).unwrap().removed.is_empty());
    }

    #[test]
    fn interrupted_journal_commit_and_writer_lock_preserve_unverified_state() {
        struct CommitFailureStore {
            value: String,
            journal: std::path::PathBuf,
            held: Option<fs::File>,
        }
        impl PathStore for CommitFailureStore {
            fn read(&mut self) -> Result<String> { Ok(self.value.clone()) }
            fn write(&mut self, value: &str) -> Result<()> {
                self.value = value.into();
                self.held = Some(OpenOptions::new().read(true).share_mode(1).open(&self.journal)?);
                Ok(())
            }
        }
        let dir = crate::test_support::TestDir::new();
        let journal = dir.path().join("journal.json");
        let new = entries(&[r"D:\managed"]);
        let mut backend = CommitFailureStore { value: "".into(), journal: journal.clone(), held: None };
        assert!(apply(&mut backend, &journal, Change::Append(&new)).is_err());
        backend.held.take();
        assert_eq!(backend.value, new[0]);
        assert_eq!(load(&journal).unwrap().records[0].status, "pending");
        assert!(preview_cleanup(&journal, &backend.value, &new, &[]).unwrap().removed.is_empty());
        let lock = OpenOptions::new().read(true).write(true).share_mode(0)
            .open(dir.path().join("writer.lock")).unwrap();
        let mut another = store("");
        assert!(apply(&mut another, &journal, Change::Append(&new)).is_err());
        assert_eq!(another.writes, 0);
        drop(lock);
    }

    #[test]
    fn path_read_failure_or_concurrent_change_prevents_writes() {
        struct ChangingStore { reads: usize, fail: bool }
        impl PathStore for ChangingStore {
            fn read(&mut self) -> Result<String> {
                self.reads += 1;
                if self.fail { return Err(AppError::msg("fixture access denied")); }
                Ok(if self.reads == 1 { "" } else { r"D:\external" }.into())
            }
            fn write(&mut self, _: &str) -> Result<()> { panic!("must not write") }
        }
        for fail in [true, false] {
            let dir = crate::test_support::TestDir::new();
            let journal = dir.path().join("journal.json");
            let mut backend = ChangingStore { reads: 0, fail };
            assert!(apply(&mut backend, &journal, Change::Append(&entries(&[r"D:\new"]))).is_err());
        }
    }

    #[test]
    fn missing_damaged_pending_or_redirected_journal_never_authorizes_removal() {
        let dir = crate::test_support::TestDir::new();
        let journal = dir.path().join("journal.json");
        let new = entries(&[r"D:\managed"]);
        assert!(preview_cleanup(&journal, &new[0], &new, &[]).unwrap().removed.is_empty());
        fs::write(&journal, b"broken").unwrap();
        let mut backend = store("");
        assert!(apply(&mut backend, &journal, Change::Append(&new)).is_err());
        assert_eq!(backend.writes, 0);
        fs::write(&journal, serde_json::to_vec(&Journal { schema: 1, records: vec![Record {
            action: "integrate".into(), before: "".into(), after: new[0].clone(),
            owned: new.clone(), status: "pending".into(), confirmed_legacy: Vec::new(),
            confirmation_status: None,
        }] }).unwrap()).unwrap();
        assert!(preview_cleanup(&journal, &new[0], &new, &[]).unwrap().removed.is_empty());
        let alias = dir.path().join("alias");
        junction::create(dir.path(), &alias).unwrap();
        assert!(apply(&mut backend, &alias.join("journal.json"), Change::Append(&new)).is_err());
        assert_eq!(backend.writes, 0);
    }
}
