//! Pre-update inventory for package-manager self-updates.
//!
//! These managers offer no supported way to return to an exact previous
//! version, so this file is an audit trail, not a rollback plan. The record is
//! written *before* the updater runs: if the process dies mid-update, the
//! planned command and the version it started from are still on disk.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{AppError, Result};

const SCHEMA: u32 = 1;
/// Bound on retained records so the journal cannot grow without limit.
const MAX_RECORDS: usize = 50;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateRecord {
    /// `planned` before the updater reports back, then `completed`, `failed`
    /// or `unverified` when the outcome could not be confirmed.
    pub status: String,
    pub tool: String,
    pub executable_path: String,
    pub before_version: Option<String>,
    pub after_version: Option<String>,
    pub program: String,
    pub arguments: Vec<String>,
    /// Unique token for this attempt, used to attach the outcome to the exact
    /// record that was written before the command ran.
    pub attempt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Journal {
    schema: u32,
    records: Vec<UpdateRecord>,
}

/// Unique token for one update attempt.
pub fn now() -> String {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    format!(
        "{}.{:09}-{}-{}",
        now.as_secs(),
        now.subsec_nanos(),
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    )
}

pub fn path(backups_dir: &Path) -> PathBuf {
    backups_dir.join("updates").join("journal.json")
}

fn load(file: &Path) -> Result<Journal> {
    // A redirected journal path would let another program observe or replace
    // the record, so refuse instead of writing through it.
    if crate::detect::system::has_reparse_component(file) {
        return Err(AppError::msg("更新清单路径被重定向，已停止写入"));
    }
    let journal: Journal = match fs::read(file) {
        Ok(bytes) => serde_json::from_slice(&bytes)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Journal {
            schema: SCHEMA,
            records: Vec::new(),
        },
        Err(error) => return Err(error.into()),
    };
    if journal.schema != SCHEMA {
        return Err(AppError::msg("更新清单版本不受支持"));
    }
    Ok(journal)
}

fn store(file: &Path, journal: &Journal) -> Result<()> {
    let parent = file
        .parent()
        .ok_or_else(|| AppError::msg("更新清单路径无效"))?;
    fs::create_dir_all(parent)?;
    // Write-then-rename so a crash cannot leave a half-written journal.
    let tmp = file.with_extension("tmp");
    fs::write(&tmp, serde_json::to_vec_pretty(journal)?)?;
    fs::rename(&tmp, file)?;
    Ok(())
}

/// Record the update that is about to run, returning the attempt token.
///
/// A caller that cannot write this record must not run the updater: the
/// record is the only durable evidence of what was about to change.
pub fn record_planned(
    backups_dir: &Path,
    attempt: &str,
    tool: &str,
    executable_path: &str,
    before_version: Option<&str>,
    program: &str,
    arguments: &[String],
) -> Result<()> {
    let file = path(backups_dir);
    let mut journal = load(&file)?;
    journal.records.push(UpdateRecord {
        status: "planned".into(),
        tool: tool.into(),
        executable_path: executable_path.into(),
        before_version: before_version.map(str::to_string),
        after_version: None,
        program: program.into(),
        arguments: arguments.to_vec(),
        attempt: attempt.into(),
    });
    while journal.records.len() > MAX_RECORDS {
        journal.records.remove(0);
    }
    store(&file, &journal)
}

pub fn record_outcome(
    backups_dir: &Path,
    attempt: &str,
    status: &str,
    after_version: Option<&str>,
) -> Result<()> {
    let file = path(backups_dir);
    let mut journal = load(&file)?;
    let record = journal
        .records
        .iter_mut()
        .rev()
        .find(|record| record.attempt == attempt && record.status == "planned")
        .ok_or_else(|| AppError::msg("未找到对应的更新清单记录，可能已被其他操作替换"))?;
    record.status = status.into();
    record.after_version = after_version.map(str::to_string);
    store(&file, &journal)
}

/// Most recent record for a tool, for read-only display.
pub fn latest(backups_dir: &Path, tool: &str) -> Result<Option<UpdateRecord>> {
    let journal = load(&path(backups_dir))?;
    Ok(journal
        .records
        .into_iter()
        .rev()
        .find(|record| record.tool == tool))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestDir;

    fn plan(dir: &Path, attempt: &str, tool: &str) {
        record_planned(
            dir,
            attempt,
            tool,
            "D:/node/npm.cmd",
            Some("10.0.0"),
            "D:/node/npm.cmd",
            &["install".into(), "--global".into(), "npm@latest".into()],
        )
        .unwrap();
    }

    #[test]
    fn planned_record_survives_a_process_that_never_reports_back() {
        let fixture = TestDir::new();
        let dir = fixture.path();
        plan(dir, "a1", "npm");

        // This is the crash case: the record stays `planned`, so a later reader
        // knows an update started from 10.0.0 with that exact command.
        let record = latest(dir, "npm").unwrap().unwrap();
        assert_eq!(record.status, "planned");
        assert_eq!(record.before_version.as_deref(), Some("10.0.0"));
        assert_eq!(record.after_version, None);
        assert_eq!(record.arguments.len(), 3);
        assert_eq!(record.attempt, "a1");
        // A different tool has no record rather than someone else's.
        assert_eq!(latest(dir, "pnpm").unwrap(), None);
    }

    #[test]
    fn outcome_attaches_to_the_exact_attempt_only() {
        let fixture = TestDir::new();
        let dir = fixture.path();
        plan(dir, "a1", "npm");
        plan(dir, "a2", "npm");

        // Settling the older attempt must not rewrite the newer one.
        record_outcome(dir, "a1", "completed", Some("10.0.1")).unwrap();
        let journal = load(&path(dir)).unwrap();
        assert_eq!(journal.records.len(), 2);
        assert_eq!(journal.records[0].status, "completed");
        assert_eq!(journal.records[0].after_version.as_deref(), Some("10.0.1"));
        assert_eq!(journal.records[1].status, "planned");
        assert_eq!(journal.records[1].after_version, None);

        // An unknown token is an error, never a silent no-op.
        assert!(record_outcome(dir, "missing", "failed", None).is_err());
        // Settling twice cannot resurrect or overwrite a finished record.
        assert!(record_outcome(dir, "a1", "failed", None).is_err());
    }

    #[test]
    fn unverified_outcome_stays_distinct_from_success_and_failure() {
        let fixture = TestDir::new();
        let dir = fixture.path();
        plan(dir, "a1", "npm");
        // The command ran and exited 0, but the version could not be read.
        record_outcome(dir, "a1", "unverified", None).unwrap();
        let record = latest(dir, "npm").unwrap().unwrap();
        assert_eq!(record.status, "unverified");
        assert_eq!(record.before_version.as_deref(), Some("10.0.0"));
        assert_eq!(record.after_version, None);
    }

    #[test]
    fn records_are_bounded_and_schema_mismatch_is_refused() {
        let fixture = TestDir::new();
        let dir = fixture.path();
        for index in 0..MAX_RECORDS + 5 {
            plan(dir, &format!("a{index}"), "npm");
        }
        let journal = load(&path(dir)).unwrap();
        assert_eq!(journal.records.len(), MAX_RECORDS);
        // The newest record is kept; the oldest were dropped.
        assert_eq!(
            journal.records.last().unwrap().attempt,
            format!("a{}", MAX_RECORDS + 4)
        );

        // A future schema must not be silently reinterpreted.
        let file = path(dir);
        let mut value: serde_json::Value =
            serde_json::from_slice(&fs::read(&file).unwrap()).unwrap();
        value["schema"] = serde_json::json!(99);
        fs::write(&file, serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(load(&file).is_err());
        assert!(record_planned(
            dir,
            "later",
            "npm",
            "D:/node/npm.cmd",
            None,
            "D:/node/npm.cmd",
            &[]
        )
        .is_err());
    }

    #[test]
    fn redirected_journal_is_refused_before_any_write() {
        let fixture = TestDir::new();
        let dir = fixture.path();
        let elsewhere = fixture.path().join("elsewhere");
        fs::create_dir_all(&elsewhere).unwrap();
        fs::create_dir_all(path(dir).parent().unwrap()).unwrap();
        // Turn the journal file into a junction pointing outside the backups
        // directory: the record must not be written through it.
        junction::create(&elsewhere, &path(dir)).unwrap();

        let error = record_planned(
            dir,
            "a1",
            "npm",
            "D:/node/npm.cmd",
            Some("10.0.0"),
            "D:/node/npm.cmd",
            &[],
        )
        .unwrap_err();
        assert!(error.to_string().contains("重定向"), "{error}");
        assert_eq!(fs::read_dir(&elsewhere).unwrap().count(), 0);
    }
}
