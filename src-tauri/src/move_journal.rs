//! 记录「受管环境是从哪个外部目录搬来的」。
//!
//! 搬入受管目录后，受管位置是真实目录，不再有 junction 指向外部旧目录，因此无法
//! 从文件系统反推旧路径。若不记录，用户一旦在搬移成功后取消删除，就再也找不到
//! 「删除旧目录」的入口——这是真实的功能缺口，不是界面疏忽。
//!
//! 记录只写路径字符串，不含任何可执行内容；读取失败或记录损坏时一律返回 `None`
//! 而不是猜测，因为猜错会导致删掉别的目录。

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::{AppError, Result};
use crate::types::EnvType;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MoveRecord {
    pub schema: u32,
    /// 受管环境名（`envs/<folder>/` 下的目录名）。
    pub name: String,
    /// 搬移前的来源目录，绝对路径。
    pub source: String,
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// 记录文件位置。
///
/// 与 `link_ownership` 的收据分开存放：那份记录证明「链接由本应用创建」，这份只
/// 说明「这个目录是从哪搬来的」，两者生命周期也不同（搬移成功后链接已不存在）。
fn record_path(root: &Path, env_type: EnvType, name: &str) -> Result<PathBuf> {
    if name.is_empty() || name.contains(['\\', '/']) {
        return Err(AppError::msg("环境名不能包含路径分隔符"));
    }
    let root = root.canonicalize()?;
    let dir = root.join(".envcon-ownership").join("moves");
    if crate::detect::system::has_reparse_component(&dir) {
        return Err(AppError::msg("搬移记录目录被重定向，停止操作"));
    }
    let key = format!("{}/{}", env_type.folder(), name.to_lowercase());
    Ok(dir.join(format!("{}.json", digest(key.as_bytes()))))
}

/// 写入一条搬移记录。覆盖已有记录是安全的：同一环境只能有一个来源。
pub fn record(root: &Path, env_type: EnvType, name: &str, source: &Path) -> Result<()> {
    let path = record_path(root, env_type, name)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let record = MoveRecord {
        schema: 1,
        name: name.to_string(),
        source: source.to_string_lossy().into(),
    };
    crate::config::atomic_write(&path, &serde_json::to_vec_pretty(&record)?)
}

/// 读取搬移记录。
///
/// 任何异常（不存在、解析失败、schema 不认识、名字对不上）都返回 `None`：调用方
/// 据此不显示删除入口。宁可少一个入口，也不能凭一条坏记录去删目录。
pub fn read(root: &Path, env_type: EnvType, name: &str) -> Option<String> {
    let path = record_path(root, env_type, name).ok()?;
    let bytes = fs::read(&path).ok()?;
    let record: MoveRecord = serde_json::from_slice(&bytes).ok()?;
    if record.schema != 1 || record.name != name || record.source.is_empty() {
        return None;
    }
    Some(record.source)
}

/// 删除搬移记录。外部旧目录被删除或用户不再需要之后调用。
pub fn forget(root: &Path, env_type: EnvType, name: &str) -> Result<()> {
    let path = record_path(root, env_type, name)?;
    match fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestDir;

    fn root(fixture: &TestDir) -> PathBuf {
        let root = fixture.path().join("managed");
        fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn a_recorded_source_survives_a_round_trip() {
        let fixture = TestDir::new();
        let root = root(&fixture);
        let source = fixture.path().join("external");
        fs::create_dir_all(&source).unwrap();

        // 未记录时不返回任何东西——不能凭猜测给出去重入口。
        assert_eq!(read(&root, EnvType::Node, "system-22"), None);

        record(&root, EnvType::Node, "system-22", &source).unwrap();
        let recorded = read(&root, EnvType::Node, "system-22").expect("record must be readable");
        assert_eq!(
            crate::detect::system::path_key(Path::new(&recorded)),
            crate::detect::system::path_key(&source)
        );

        // 记录按环境名隔离：同类型不同名、同名不同类型都不共享。
        assert_eq!(read(&root, EnvType::Node, "system-23"), None);
        assert_eq!(read(&root, EnvType::Python, "system-22"), None);

        forget(&root, EnvType::Node, "system-22").unwrap();
        assert_eq!(read(&root, EnvType::Node, "system-22"), None);
        // 重复 forget 是幂等的，不该报错。
        forget(&root, EnvType::Node, "system-22").unwrap();
    }

    /// 记录损坏、schema 不认识或名字对不上时必须当作「没有记录」。
    #[test]
    fn a_corrupt_or_mismatched_record_is_treated_as_absent() {
        let fixture = TestDir::new();
        let root = root(&fixture);
        let path = record_path(&root, EnvType::Node, "system-22").unwrap();
        fs::create_dir_all(path.parent().unwrap()).unwrap();

        for body in [
            "not json",
            r#"{"schema": 2, "name": "system-22", "source": "E:/x"}"#,
            r#"{"schema": 1, "name": "other", "source": "E:/x"}"#,
            r#"{"schema": 1, "name": "system-22", "source": ""}"#,
            // deny_unknown_fields：多出字段也视为不可信。
            r#"{"schema": 1, "name": "system-22", "source": "E:/x", "extra": 1}"#,
        ] {
            fs::write(&path, body).unwrap();
            assert_eq!(read(&root, EnvType::Node, "system-22"), None, "body: {body}");
        }
    }

    /// 环境名不能借道逃出记录目录。
    #[test]
    fn a_name_with_path_separators_is_rejected() {
        let fixture = TestDir::new();
        let root = root(&fixture);
        for name in ["../outside", "a/b", "a\\b"] {
            assert!(record(&root, EnvType::Node, name, Path::new("E:/x")).is_err(), "{name}");
            assert_eq!(read(&root, EnvType::Node, name), None);
        }
    }
}
