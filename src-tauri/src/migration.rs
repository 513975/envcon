use std::fs;
use std::io::{Read, Write};
use std::os::windows::fs::MetadataExt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::detect::system::path_within;
use crate::error::{AppError, Result};

pub static LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MigrationPlan {
    pub tool: String,
    pub kind: String,
    pub source: String,
    pub target: String,
    pub files: u64,
    pub bytes: u64,
    pub links: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationResult {
    pub target: String,
    pub backup: String,
    pub files: u64,
    pub bytes: u64,
    pub journal: String,
}

#[derive(Debug, PartialEq)]
struct Entry {
    relative: PathBuf,
    size: u64,
    modified: Option<std::time::SystemTime>,
    link: Option<PathBuf>,
    directory: bool,
}

fn inventory(root: &Path) -> Result<Vec<Entry>> {
    let mut entries = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            let meta = fs::symlink_metadata(&path)?;
            let reparse = meta.file_attributes() & 0x400 != 0;
            let link = if reparse {
                // Preserve junction references without copying their targets or traversing cycles.
                if meta.file_attributes() & 0x10 == 0 {
                    return Err(AppError::msg(format!(
                        "暂不迁移文件符号链接: {}",
                        path.display()
                    )));
                }
                Some(fs::read_link(&path).map_err(|_| {
                    AppError::msg(format!("不支持迁移此链接或重解析点: {}", path.display()))
                })?)
            } else {
                None
            };
            if meta.is_dir() && !reparse {
                pending.push(path.clone());
            }
            entries.push(Entry {
                relative: path.strip_prefix(root).unwrap().to_path_buf(),
                size: if meta.is_file() && !reparse {
                    meta.len()
                } else {
                    0
                },
                modified: meta.modified().ok(),
                link,
                directory: meta.is_dir() || reparse,
            });
        }
    }
    entries.sort_by(|a, b| a.relative.cmp(&b.relative));
    Ok(entries)
}

fn absolute_identity(path: &Path) -> Result<PathBuf> {
    if !path.is_absolute()
        || path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(AppError::msg("必须使用不含 .. 的绝对路径"));
    }
    if fs::symlink_metadata(path).is_ok() {
        return Ok(path.canonicalize()?);
    }
    let parent = path.parent().ok_or_else(|| AppError::msg("无效目标路径"))?;
    Ok(absolute_identity(parent)?.join(
        path.file_name()
            .ok_or_else(|| AppError::msg("无效目录名称"))?,
    ))
}

pub fn preview(root: &Path, tool: &str, kind: &str, source: &Path) -> Result<MigrationPlan> {
    let definition = crate::package_managers::definition(tool)?;
    if !matches!(kind, "global" | "cache") || (kind == "global" && !definition.migrate_global) {
        return Err(AppError::msg(
            "不支持此迁移类型；解释器内的 Python 包请使用重装功能",
        ));
    }
    let metadata = fs::symlink_metadata(source)?;
    if metadata.file_attributes() & 0x400 != 0 || !metadata.is_dir() {
        return Err(AppError::msg(
            "源目录必须是普通目录；已迁移的链接无需再次迁移",
        ));
    }
    let source = absolute_identity(source)?;
    let target_path = root.join("globals").join(format!("{tool}-{kind}"));
    if fs::symlink_metadata(&target_path).is_ok_and(|meta| meta.file_attributes() & 0x400 != 0) {
        return Err(AppError::msg("目标目录不能是链接"));
    }
    let target = absolute_identity(&target_path)?;
    if path_within(&source, &target) || path_within(&target, &source) {
        return Err(AppError::msg(
            "源目录和目标目录不能相同、互为父子或通过链接指向同一位置",
        ));
    }
    for protected in [
        Some(root.to_path_buf()),
        dirs::home_dir(),
        dirs::data_dir(),
        dirs::data_local_dir(),
        std::env::var_os("WINDIR").map(PathBuf::from),
        std::env::var_os("ProgramFiles").map(PathBuf::from),
    ]
    .into_iter()
    .flatten()
    {
        if path_within(&source, &protected) {
            return Err(AppError::msg(
                "不能迁移管理根目录、用户目录、系统目录或它们的父目录",
            ));
        }
    }
    if source.parent().is_none()
        || [
            "node.exe",
            "python.exe",
            "pyvenv.cfg",
            "bin/rustup.exe",
            "rustup.exe",
            "bun.exe",
            "dotnet.exe",
        ]
        .iter()
        .any(|p| source.join(p).exists())
    {
        return Err(AppError::msg(
            "源目录包含运行时或虚拟环境，不能整体迁移；请使用独立的全局包目录",
        ));
    }
    if let Ok(meta) = fs::symlink_metadata(&target) {
        if meta.file_attributes() & 0x400 != 0
            || !meta.is_dir()
            || fs::read_dir(&target)?.next().is_some()
        {
            return Err(AppError::msg(
                "目标目录非空或是链接，本工具不会覆盖或合并已有数据",
            ));
        }
    }
    let entries = inventory(&source)?;
    Ok(MigrationPlan {
        tool: tool.into(),
        kind: kind.into(),
        source: source.to_string_lossy().into(),
        target: target.to_string_lossy().into(),
        files: entries.iter().filter(|e| !e.directory).count() as u64,
        bytes: entries.iter().map(|e| e.size).sum(),
        links: entries.iter().filter(|e| e.link.is_some()).count() as u64,
    })
}

fn copy_verified(source: &Path, target: &Path) -> Result<()> {
    let mut input = fs::File::open(source)?;
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(target)?;
    std::io::copy(&mut input, &mut output)?;
    output.flush()?;
    output.sync_all()?;
    drop(output);
    verify_file(source, target)
}

fn verify_file(source: &Path, target: &Path) -> Result<()> {
    let mut a = fs::File::open(source)?;
    let mut b = fs::File::open(target)?;
    let mut left = [0u8; 65536];
    let mut right = [0u8; 65536];
    loop {
        let count = a.read(&mut left)?;
        if count == 0 {
            if b.read(&mut right[..1])? == 0 {
                return Ok(());
            }
            break;
        }
        if b.read_exact(&mut right[..count]).is_err() || left[..count] != right[..count] {
            break;
        }
    }
    Err(AppError::msg(format!("文件校验失败: {}", source.display())))
}

pub fn execute(root: &Path, requested: &MigrationPlan) -> Result<MigrationResult> {
    let plan = preview(
        root,
        &requested.tool,
        &requested.kind,
        Path::new(&requested.source),
    )?;
    if &plan != requested {
        return Err(AppError::msg("目录或配置已变化，请重新预览迁移"));
    }
    let source = PathBuf::from(&plan.source);
    let target = PathBuf::from(&plan.target);
    let entries = inventory(&source)?;
    fs::create_dir_all(target.parent().unwrap())?;
    if target.exists() {
        fs::remove_dir(&target)?;
    }
    // Exclusive creation ensures another operation cannot turn this into a merge.
    fs::create_dir(&target)?;
    let operation = || -> Result<(PathBuf, PathBuf)> {
        for entry in &entries {
            let destination = target.join(&entry.relative);
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent)?;
            }
            if let Some(link) = &entry.link {
                let link = if link.is_absolute() {
                    link.clone()
                } else {
                    source.join(&entry.relative).parent().unwrap().join(link)
                };
                junction::create(link, &destination)?;
            } else if entry.directory {
                fs::create_dir_all(&destination)?;
            } else {
                copy_verified(&source.join(&entry.relative), &destination)?;
            }
        }
        if inventory(&source)? != entries {
            return Err(AppError::msg(
                "迁移期间源目录发生变化，请关闭包管理器后重试",
            ));
        }
        let backup = source.with_file_name(format!(
            "{}.envcon-backup-{}",
            source.file_name().unwrap().to_string_lossy(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let journal = target.parent().unwrap().join(format!(
            "{}.json",
            backup.file_name().unwrap().to_string_lossy()
        ));
        let mut record = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&journal)?;
        serde_json::to_writer_pretty(
            &mut record,
            &serde_json::json!({
                "plan": plan, "backup": backup, "recovery": "Inspect source junction and backup before restoring; never merge or overwrite either directory."
            }),
        )?;
        record.flush()?;
        record.sync_all()?;
        fs::rename(&source, &backup)?;
        // Validate once more after renaming so open writers are detected before switching paths.
        let finish = || -> Result<()> {
            if inventory(&backup)? != entries {
                return Err(AppError::msg("源数据在迁移期间变化"));
            }
            for entry in entries.iter().filter(|e| !e.directory) {
                verify_file(&backup.join(&entry.relative), &target.join(&entry.relative))?;
            }
            junction::create(&target, &source)?;
            Ok(())
        };
        if let Err(error) = finish() {
            if let Err(restore) = fs::rename(&backup, &source) {
                return Err(AppError::msg(format!(
                    "{error}；恢复失败: {restore}；原数据备份: {}",
                    backup.display()
                )));
            }
            return Err(error);
        }
        Ok((backup, journal))
    };
    let (backup, journal) = operation().map_err(|error| {
        AppError::msg(format!(
            "{error}；目标副本保留在 {}，未覆盖原数据。重试前请检查并移走该副本",
            target.display()
        ))
    })?;
    Ok(MigrationResult {
        target: plan.target,
        backup: backup.to_string_lossy().into(),
        files: plan.files,
        bytes: plan.bytes,
        journal: journal.to_string_lossy().into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestDir;

    #[test]
    fn migrates_files_keeps_backup_and_old_command_paths() {
        let fixture = TestDir::new();
        let root = fixture.path().join("managed");
        let source = fixture.path().join("npm");
        fs::create_dir_all(source.join("node_modules/tool")).unwrap();
        fs::create_dir_all(&root).unwrap();
        fs::write(
            source.join("node_modules/tool/index.js"),
            "console.log('ok')",
        )
        .unwrap();
        fs::write(
            source.join("tool.cmd"),
            "@node %~dp0\\node_modules\\tool\\index.js",
        )
        .unwrap();
        junction::create(source.join("node_modules/tool"), source.join("reference")).unwrap();
        let plan = preview(&root, "npm", "global", &source).unwrap();
        assert_eq!(plan.files, 2);
        assert_eq!(plan.links, 1);
        let result = execute(&root, &plan).unwrap();
        assert_eq!(
            fs::read(source.join("tool.cmd")).unwrap(),
            fs::read(Path::new(&result.backup).join("tool.cmd")).unwrap()
        );
        assert!(source.join("reference/index.js").is_file());
        assert!(preview(&root, "npm", "global", &source).is_err());
    }

    #[test]
    fn refuses_existing_targets_runtime_roots_and_changed_preview() {
        let fixture = TestDir::new();
        let root = fixture.path().join("managed");
        let source = fixture.path().join("source");
        fs::create_dir_all(&source).unwrap();
        fs::create_dir_all(&root).unwrap();
        let plan = preview(&root, "pip", "cache", &source).unwrap();
        fs::write(source.join("new"), b"changed").unwrap();
        assert!(execute(&root, &plan).is_err());
        fs::create_dir_all(root.join("globals/pip-cache")).unwrap();
        fs::write(root.join("globals/pip-cache/existing"), b"keep").unwrap();
        assert!(preview(&root, "pip", "cache", &source).is_err());
        fs::write(source.join("node.exe"), b"runtime").unwrap();
        assert!(preview(&root, "npm", "global", &source).is_err());
        assert!(preview(&root, "npm", "global", fixture.path()).is_err());
        assert!(source.join("new").is_file());
    }

    #[test]
    fn rejects_tampered_target_and_detects_changed_contents() {
        let fixture = TestDir::new();
        let root = fixture.path().join("managed");
        let source = fixture.path().join("source");
        fs::create_dir_all(&source).unwrap();
        fs::create_dir_all(&root).unwrap();
        let mut plan = preview(&root, "npm", "cache", &source).unwrap();
        plan.target = fixture.path().join("elsewhere").to_string_lossy().into();
        assert!(execute(&root, &plan).is_err());
        fs::write(source.join("a"), b"aaa").unwrap();
        fs::write(source.join("b"), b"bbb").unwrap();
        assert!(verify_file(&source.join("a"), &source.join("b")).is_err());
    }

    #[test]
    fn supports_empty_target_created_by_configuration() {
        let fixture = TestDir::new();
        let root = fixture.path().join("managed");
        let source = fixture.path().join("cache");
        fs::create_dir_all(&source).unwrap();
        fs::create_dir_all(root.join("globals/pip-cache")).unwrap();
        fs::write(source.join("entry"), b"cache").unwrap();
        let plan = preview(&root, "pip", "cache", &source).unwrap();
        let result = execute(&root, &plan).unwrap();
        assert!(Path::new(&result.journal).is_file());
        assert_eq!(fs::read(source.join("entry")).unwrap(), b"cache");
    }

    #[test]
    fn locked_file_failure_preserves_source_and_reports_partial_target() {
        use std::os::windows::fs::OpenOptionsExt;
        let fixture = TestDir::new();
        let root = fixture.path().join("managed");
        let source = fixture.path().join("source");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("locked"), b"keep").unwrap();
        let plan = preview(&root, "npm", "cache", &source).unwrap();
        let lock = fs::OpenOptions::new()
            .write(true)
            .share_mode(0)
            .open(source.join("locked"))
            .unwrap();
        assert!(execute(&root, &plan).is_err());
        drop(lock);
        assert!(!junction::exists(&source).unwrap_or(false));
        assert_eq!(fs::read(source.join("locked")).unwrap(), b"keep");
    }
}
