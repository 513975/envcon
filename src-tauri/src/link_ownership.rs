use std::fs::{self, File, OpenOptions};
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::AsRawHandle;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use windows_sys::Win32::Storage::FileSystem::{
    FileDispositionInfo, FileIdInfo, GetFileInformationByHandleEx, SetFileInformationByHandle,
    DELETE, FILE_DISPOSITION_INFO, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
    FILE_ID_INFO, FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
};

use crate::detect::system;
use crate::error::{AppError, Result};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Identity {
    volume: u64,
    file_id: [u8; 16],
    created: u128,
    target: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    schema: u32,
    entry: String,
    status: String,
    identity: Option<Identity>,
}

fn digest(bytes: &[u8]) -> String { format!("{:x}", Sha256::digest(bytes)) }

fn receipt_path(root: &Path, entry: &Path) -> Result<(PathBuf, String)> {
    let root = root.canonicalize()?;
    let parent = entry.parent().ok_or_else(|| AppError::msg("链接入口没有父目录"))?;
    if system::has_reparse_component(parent) {
        return Err(AppError::msg("链接入口的父目录被重定向，停止操作"));
    }
    let entry = parent.canonicalize()?.join(entry.file_name().ok_or_else(|| AppError::msg("链接入口无效"))?);
    let key = system::path_key(&entry);
    if !key.starts_with(&format!("{}\\", system::path_key(&root))) {
        return Err(AppError::msg("链接入口不在管理根目录内"));
    }
    let path = root.join(".envcon-ownership").join("links").join(format!("{}.json", digest(key.as_bytes())));
    if system::has_reparse_component(&path) {
        return Err(AppError::msg("链接来源记录被重定向，停止操作"));
    }
    Ok((path, key))
}

fn writer_lock(path: &Path) -> Result<File> {
    let parent = path.parent().ok_or_else(|| AppError::msg("链接来源目录无效"))?;
    fs::create_dir_all(parent)?;
    let lock = parent.join("writer.lock");
    if system::has_reparse_component(&lock) {
        return Err(AppError::msg("链接来源锁被重定向"));
    }
    Ok(OpenOptions::new().read(true).write(true).create(true).truncate(false).share_mode(0).open(lock)?)
}

fn read_receipt(path: &Path, key: &str) -> Result<Receipt> {
    let receipt: Receipt = serde_json::from_slice(&fs::read(path)?)?;
    if receipt.schema != 1 || receipt.entry != key {
        return Err(AppError::msg("链接来源记录版本或入口不匹配"));
    }
    Ok(receipt)
}

fn open_entry(entry: &Path, deleting: bool) -> Result<File> {
    // Open the link object itself, never the target directory.
    Ok(OpenOptions::new()
        .access_mode(FILE_READ_ATTRIBUTES | if deleting { DELETE } else { 0 })
        .share_mode(if deleting { FILE_SHARE_READ } else { FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE })
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS)
        .open(entry)?)
}

fn identity(file: &File, entry: &Path) -> Result<Identity> {
    if !system::is_reparse_path(entry) {
        return Err(AppError::msg("入口已不是链接，停止操作"));
    }
    let mut info = FILE_ID_INFO::default();
    let ok = unsafe {
        GetFileInformationByHandleEx(
            file.as_raw_handle(), FileIdInfo, &mut info as *mut _ as *mut _,
            std::mem::size_of::<FILE_ID_INFO>() as u32,
        )
    };
    if ok == 0 { return Err(std::io::Error::last_os_error().into()); }
    let target = fs::read_link(entry)?;
    let target = if target.is_absolute() { target } else { entry.parent().unwrap().join(target) };
    let created = file.metadata()?.created()?.duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| AppError::msg(error.to_string()))?.as_nanos();
    Ok(Identity {
        volume: info.VolumeSerialNumber, file_id: info.FileId.Identifier,
        created, target: system::path_key(&target),
    })
}

fn verified_identity(path: &Path, key: &str, file: &File, entry: &Path) -> Result<Identity> {
    let receipt = read_receipt(path, key)?;
    if receipt.status != "verified" {
        return Err(AppError::msg("链接创建来源未完成核验，保留该入口"));
    }
    let current = identity(file, entry)?;
    if receipt.identity.as_ref() != Some(&current) {
        return Err(AppError::msg("链接身份或目标已变化，保留该入口"));
    }
    Ok(current)
}

pub(crate) fn fingerprint(root: &Path, entry: &Path) -> Result<String> {
    let (path, key) = receipt_path(root, entry)?;
    let file = open_entry(entry, false)?;
    let identity = verified_identity(&path, &key, &file, entry)?;
    Ok(digest(&serde_json::to_vec(&identity)?))
}

pub(crate) enum Source {
    Owned(String),
    Legacy(String),
}

pub(crate) fn assess(root: &Path, entry: &Path) -> Result<Source> {
    let (path, _) = receipt_path(root, entry)?;
    match fs::symlink_metadata(&path) {
        Ok(_) => fingerprint(root, entry).map(Source::Owned),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let file = open_entry(entry, false)?;
            let observed = identity(&file, entry)?;
            Ok(Source::Legacy(digest(&serde_json::to_vec(&observed)?)))
        }
        Err(error) => Err(error.into()),
    }
}

pub(crate) fn create(root: &Path, entry: &Path, target: &Path) -> Result<()> {
    match fs::symlink_metadata(entry) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
        Ok(_) => return Err(AppError::msg("链接入口已存在，不覆盖或认领")),
    }
    record_creation(root, entry, target, || {
        junction::create(target, entry).map_err(Into::into)
    })
}

pub(crate) fn record_creation(
    root: &Path, entry: &Path, target: &Path, create: impl FnOnce() -> Result<()>,
) -> Result<()> {
    let (path, key) = receipt_path(root, entry)?;
    let _lock = writer_lock(&path)?;
    match fs::symlink_metadata(&path) {
        Ok(_) => { read_receipt(&path, &key)?; }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let mut receipt = Receipt { schema: 1, entry: key, status: "pending".into(), identity: None };
    crate::config::atomic_write(&path, &serde_json::to_vec_pretty(&receipt)?)?;
    create()?;
    let file = open_entry(entry, true)?;
    let observed = identity(&file, entry)?;
    if observed.target != system::path_key(target) {
        return Err(AppError::msg("链接创建后目标不匹配，来源记录保持待核验"));
    }
    receipt.identity = Some(observed);
    receipt.status = "verified".into();
    crate::config::atomic_write(&path, &serde_json::to_vec_pretty(&receipt)?)
        .map_err(|error| AppError::msg(format!("链接可能已创建，但来源记录提交失败：{error}")))?;
    Ok(())
}

pub(crate) fn remove(root: &Path, entry: &Path, expected: Option<&str>) -> Result<()> {
    remove_inner(root, entry, expected, false)
}

pub(crate) fn remove_confirmed_legacy(root: &Path, entry: &Path, expected: &str) -> Result<()> {
    remove_inner(root, entry, Some(expected), true)
}

fn remove_inner(root: &Path, entry: &Path, expected: Option<&str>, confirmed_legacy: bool) -> Result<()> {
    let (path, key) = receipt_path(root, entry)?;
    let _lock = writer_lock(&path)?;
    let file = open_entry(entry, true)?;
    let observed = if confirmed_legacy {
        match fs::symlink_metadata(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => identity(&file, entry)?,
            Err(error) => return Err(error.into()),
            Ok(_) => return Err(AppError::msg("链接来源记录已变化，请重新预览；不能以旧链接确认绕过记录核验")),
        }
    } else {
        verified_identity(&path, &key, &file, entry)?
    };
    let fingerprint = digest(&serde_json::to_vec(&observed)?);
    if expected.is_some_and(|expected| expected != fingerprint) {
        return Err(AppError::msg("链接预览身份已变化，停止移除"));
    }
    let mut receipt = if confirmed_legacy {
        Receipt { schema: 1, entry: key, status: "confirmed-removing".into(), identity: Some(observed) }
    } else {
        let mut receipt = read_receipt(&path, &key)?;
        receipt.status = "removing".into();
        receipt
    };
    crate::config::atomic_write(&path, &serde_json::to_vec_pretty(&receipt)?)?;
    // Mark the verified open object for deletion; do not re-resolve a path for deletion.
    let disposition = FILE_DISPOSITION_INFO { DeleteFile: true };
    let ok = unsafe {
        SetFileInformationByHandle(
            file.as_raw_handle(), FileDispositionInfo,
            &disposition as *const _ as *const _, std::mem::size_of::<FILE_DISPOSITION_INFO>() as u32,
        )
    };
    if ok == 0 { return Err(std::io::Error::last_os_error().into()); }
    drop(file);
    match fs::symlink_metadata(entry) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
        Ok(_) => return Err(AppError::msg("链接已请求删除，但入口尚未消失；可能仍有句柄占用，不能确认成功")),
    }
    receipt.status = if confirmed_legacy { "confirmed-removed" } else { "removed" }.into();
    crate::config::atomic_write(&path, &serde_json::to_vec_pretty(&receipt)?)
        .map_err(|error| AppError::msg(format!("链接已移除，但来源记录提交失败：{error}")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (crate::test_support::TestDir, PathBuf, PathBuf, PathBuf) {
        let dir = crate::test_support::TestDir::new();
        let root = dir.path().join("managed");
        let target = dir.path().join("external");
        fs::create_dir_all(root.join("current")).unwrap();
        fs::create_dir(&target).unwrap();
        fs::write(target.join("keep"), b"keep").unwrap();
        let entry = root.join("current/node");
        (dir, root, target, entry)
    }

    #[test]
    fn legacy_confirmation_removes_only_the_previewed_object_without_claiming_creation() {
        let (_dir, root, target, entry) = fixture();
        junction::create(&target, &entry).unwrap();
        let Source::Legacy(preview) = assess(&root, &entry).unwrap() else { panic!("unexpected source"); };
        assert!(fingerprint(&root, &entry).is_err());
        assert!(remove(&root, &entry, Some(&preview)).is_err());
        remove_confirmed_legacy(&root, &entry, &preview).unwrap();
        assert!(fs::symlink_metadata(&entry).is_err());
        assert!(target.join("keep").exists());
        let (path, key) = receipt_path(&root, &entry).unwrap();
        assert_eq!(read_receipt(&path, &key).unwrap().status, "confirmed-removed");
    }

    #[test]
    fn legacy_confirmation_cannot_override_new_corrupt_or_pending_receipts_or_replacement() {
        for change in ["corrupt", "pending", "replacement"] {
            let (_dir, root, target, entry) = fixture();
            junction::create(&target, &entry).unwrap();
            let Source::Legacy(preview) = assess(&root, &entry).unwrap() else { panic!("unexpected source"); };
            let (path, key) = receipt_path(&root, &entry).unwrap();
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            match change {
                "corrupt" => fs::write(&path, b"broken").unwrap(),
                "pending" => fs::write(&path, serde_json::to_vec(&Receipt {
                    schema: 1, entry: key, status: "pending".into(), identity: None,
                }).unwrap()).unwrap(),
                _ => {
                    fs::rename(&entry, root.join("current/old")).unwrap();
                    junction::create(&target, &entry).unwrap();
                }
            }
            assert!(remove_confirmed_legacy(&root, &entry, &preview).is_err(), "{change}");
            assert!(entry.exists());
            assert!(target.join("keep").exists());
            if change != "replacement" { assert!(assess(&root, &entry).is_err()); }
        }
    }

    #[test]
    fn owned_link_removal_verifies_object_and_preserves_target() {
        let (_dir, root, target, entry) = fixture();
        create(&root, &entry, &target).unwrap();
        let preview = fingerprint(&root, &entry).unwrap();
        remove(&root, &entry, Some(&preview)).unwrap();
        assert!(!entry.exists());
        assert_eq!(fs::read(target.join("keep")).unwrap(), b"keep");
    }

    #[test]
    fn missing_pending_corrupt_or_redirected_receipts_never_authorize_removal() {
        for mode in ["missing", "pending", "corrupt", "redirected"] {
            let (dir, root, target, entry) = fixture();
            create(&root, &entry, &target).unwrap();
            let (path, _) = receipt_path(&root, &entry).unwrap();
            match mode {
                "missing" => fs::remove_file(path).unwrap(),
                "pending" => {
                    let mut value: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
                    value["status"] = "pending".into();
                    fs::write(path, serde_json::to_vec(&value).unwrap()).unwrap();
                }
                "corrupt" => fs::write(path, b"damaged").unwrap(),
                _ => {
                    let source = root.join(".envcon-ownership");
                    let moved = dir.path().join("moved-records");
                    fs::rename(&source, &moved).unwrap();
                    junction::create(&moved, &source).unwrap();
                }
            }
            assert!(fingerprint(&root, &entry).is_err(), "{mode}");
            assert!(remove(&root, &entry, None).is_err(), "{mode}");
            assert!(entry.exists());
            assert!(target.join("keep").is_file());
        }
    }

    #[test]
    fn replacement_with_same_target_and_retargeted_links_are_not_owned() {
        for same_target in [false, true] {
            let (dir, root, target, entry) = fixture();
            create(&root, &entry, &target).unwrap();
            let preview = fingerprint(&root, &entry).unwrap();
            // Keep the old object alive at a different name so file-ID reuse is impossible.
            fs::rename(&entry, root.join("current/old")).unwrap();
            let other = dir.path().join("other");
            fs::create_dir(&other).unwrap();
            junction::create(if same_target { &target } else { &other }, &entry).unwrap();
            assert!(fingerprint(&root, &entry).is_err());
            assert!(remove(&root, &entry, Some(&preview)).is_err());
            assert!(entry.exists());
            assert!(target.join("keep").exists());
        }
    }

    #[test]
    fn creation_failure_never_claims_a_link_and_preexisting_links_are_preserved() {
        let (_dir, root, target, entry) = fixture();
        assert!(record_creation(&root, &entry, &target, || Err(AppError::msg("fixture create failed"))).is_err());
        junction::create(&target, &entry).unwrap();
        assert!(fingerprint(&root, &entry).is_err());
        assert!(create(&root, &entry, &target).is_err());
        assert!(remove(&root, &entry, None).is_err());
        assert!(entry.exists());
    }

    #[test]
    fn normalized_entry_case_matches_but_redirected_parent_is_rejected() {
        let (dir, root, target, entry) = fixture();
        create(&root, &entry, &target).unwrap();
        let upper = PathBuf::from(entry.to_string_lossy().to_uppercase());
        assert_eq!(fingerprint(&root, &entry).unwrap(), fingerprint(&root, &upper).unwrap());
        let moved = dir.path().join("moved-current");
        fs::rename(root.join("current"), &moved).unwrap();
        junction::create(&moved, root.join("current")).unwrap();
        assert!(fingerprint(&root, &entry).is_err());
        assert!(remove(&root, &entry, None).is_err());
        assert!(moved.join("node").exists());
        assert!(target.join("keep").exists());
    }

    #[test]
    fn locked_link_and_writer_lock_prevent_mutation() {
        let (_dir, root, target, entry) = fixture();
        create(&root, &entry, &target).unwrap();
        let before = fingerprint(&root, &entry).unwrap();
        let handle = OpenOptions::new().read(true).share_mode(FILE_SHARE_READ)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS).open(&entry).unwrap();
        assert!(remove(&root, &entry, Some(&before)).is_err());
        assert_eq!(fingerprint(&root, &entry).unwrap(), before);
        drop(handle);
        let (path, _) = receipt_path(&root, &entry).unwrap();
        let lock = writer_lock(&path).unwrap();
        assert!(remove(&root, &entry, Some(&before)).is_err());
        assert!(entry.exists());
        drop(lock);
        remove(&root, &entry, Some(&before)).unwrap();
        assert!(target.join("keep").exists());
    }

    #[test]
    fn pending_delete_is_not_reported_as_verified_removal() {
        let (_dir, root, target, entry) = fixture();
        create(&root, &entry, &target).unwrap();
        let before = fingerprint(&root, &entry).unwrap();
        let metadata_handle = OpenOptions::new().access_mode(FILE_READ_ATTRIBUTES).share_mode(FILE_SHARE_READ)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS).open(&entry).unwrap();
        let error = remove(&root, &entry, Some(&before)).unwrap_err();
        assert!(error.to_string().contains("不能确认成功"));
        let (path, key) = receipt_path(&root, &entry).unwrap();
        assert_eq!(read_receipt(&path, &key).unwrap().status, "removing");
        assert!(fingerprint(&root, &entry).is_err());
        assert!(target.join("keep").exists());
        drop(metadata_handle);
        assert!(fs::symlink_metadata(&entry).is_err());
        assert!(target.join("keep").exists());
    }

    #[test]
    fn final_receipt_failure_leaves_link_unverified() {
        let (_dir, root, target, entry) = fixture();
        let (path, _) = receipt_path(&root, &entry).unwrap();
        assert!(record_creation(&root, &entry, &target, || {
            junction::create(&target, &entry)?;
            fs::remove_file(&path)?;
            fs::create_dir(&path)?;
            Ok(())
        }).is_err());
        assert!(entry.exists());
        assert!(fingerprint(&root, &entry).is_err());
        assert!(remove(&root, &entry, None).is_err());
        assert!(target.join("keep").exists());
    }
}
