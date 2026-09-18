use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::windows::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::error::{AppError, Result};

const SCHEMA_VERSION: u32 = 1;
static NEXT_FILE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Missing in 0.5.2 and earlier; migrated on the next successful save.
    pub schema_version: u32,
    /// 管理根目录(如 D:\DevEnv)
    pub root: Option<String>,
    /// 下载临时目录
    pub downloads_dir: Option<String>,
    /// 各类型镜像源偏好
    pub mirrors: std::collections::HashMap<String, String>,
    #[serde(flatten)]
    pub extra: std::collections::BTreeMap<String, serde_json::Value>,
}

/// 应用配置:数据目录定位(便携优先)+ 设置持久化
#[derive(Debug, Clone)]
pub struct AppConfig {
    pub data_dir: PathBuf,
    pub portable: bool,
    pub settings: Settings,
    config_path: PathBuf,
    pub warning: Option<String>,
    pub write_blocked: bool,
    loaded_bytes: Option<Vec<u8>>,
}

impl AppConfig {
    /// 定位数据目录:exe 旁 data/ 可写 → 便携模式;否则 %APPDATA%\envcon
    pub fn load() -> Self {
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()));

        let mut portable = false;
        let mut data_dir: Option<PathBuf> = None;

        if let Some(dir) = exe_dir {
            let candidate = dir.join("data");
            if fs::create_dir_all(&candidate).is_ok() {
                let probe = candidate.join(".write-probe");
                if fs::write(&probe, b"ok").is_ok() {
                    let _ = fs::remove_file(&probe);
                    portable = true;
                    data_dir = Some(candidate);
                }
            }
        }

        let data_dir = data_dir.unwrap_or_else(|| {
            dirs::data_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("envcon")
        });
        let _ = fs::create_dir_all(&data_dir);

        Self::load_from(data_dir, portable)
    }

    fn load_from(data_dir: PathBuf, portable: bool) -> Self {
        let config_path = data_dir.join("config.json");
        let mut cfg = Self {
            data_dir,
            portable,
            settings: Settings::default(),
            config_path,
            warning: None,
            write_blocked: false,
            loaded_bytes: None,
        };
        let result = match fs::read(&cfg.config_path) {
            Ok(bytes) => {
                let settings = parse_settings(&bytes);
                cfg.loaded_bytes = Some(bytes);
                settings
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound && !cfg.backup_path().exists() => Ok(Settings::default()),
            Err(e) => Err(AppError::msg(format!("读取配置失败: {e}"))),
        };
        match result {
            Ok(settings) => cfg.settings = settings,
            Err(e) => {
                cfg.warning = Some(format!("{e}。原文件未修改，已暂停使用管理根目录和保存配置。请在设置中恢复备份，或修复文件后重新加载。"));
                cfg.write_blocked = true;
            }
        }
        cfg
    }

    pub fn reload(&mut self) {
        *self = Self::load_from(self.data_dir.clone(), self.portable);
    }

    pub fn backup_path(&self) -> PathBuf { self.data_dir.join("config.previous.json") }

    pub fn has_valid_backup(&self) -> bool {
        fs::read(self.backup_path()).ok().is_some_and(|b| parse_settings(&b).is_ok())
    }

    /// Serialize writers across application instances. This lock file contains no settings.
    fn lock_writer(&self) -> Result<fs::File> {
        OpenOptions::new().read(true).write(true).create(true).truncate(false).share_mode(0)
            .open(self.data_dir.join("config.lock"))
            .map_err(|e| AppError::msg(format!("无法锁定配置，请关闭其他 EnvCon 实例后重试: {e}")))
    }

    fn ensure_unchanged(&self) -> Result<()> {
        let current = match fs::read(&self.config_path) {
            Ok(bytes) => Some(bytes),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(e.into()),
        };
        if current != self.loaded_bytes {
            return Err(AppError::msg("配置文件已被其他程序修改；请先在设置中重新加载配置，再重试"));
        }
        Ok(())
    }

    pub fn ensure_writable(&self) -> Result<()> {
        if self.write_blocked {
            return Err(AppError::msg(self.warning.as_deref().unwrap_or("配置不可写")));
        }
        Ok(())
    }

    /// Keep the last validated configuration, flush a unique sibling file, then replace.
    pub fn save(&mut self) -> Result<()> {
        self.ensure_writable()?;
        let _lock = self.lock_writer()?;
        self.ensure_unchanged()?;
        let mut next = self.settings.clone();
        next.schema_version = SCHEMA_VERSION;
        let json = serde_json::to_string_pretty(&next)?;
        if let Some(previous) = &self.loaded_bytes {
            parse_settings(previous)?;
            atomic_write(&self.backup_path(), previous)?;
        }
        atomic_write(&self.config_path, json.as_bytes())?;
        self.settings = next;
        self.loaded_bytes = Some(json.into_bytes());
        Ok(())
    }

    /// Explicit user action. Preserve the displaced file even when it cannot be parsed.
    pub fn restore_backup(&mut self) -> Result<()> {
        let _lock = self.lock_writer()?;
        self.ensure_unchanged()?;
        let bytes = fs::read(self.backup_path())?;
        let settings = parse_settings(&bytes)?;
        if let Some(current) = &self.loaded_bytes {
            let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos();
            let preserved = self.data_dir.join(format!("config.before-recovery-{stamp}-{}.json", std::process::id()));
            write_new(&preserved, current)?;
        }
        atomic_write(&self.config_path, &bytes)?;
        self.settings = settings;
        self.loaded_bytes = Some(bytes);
        self.write_blocked = false;
        self.warning = None;
        Ok(())
    }

    /// 解析管理根目录:设置优先 → 默认目录存在则用(新名优先,兼容旧名) → None
    pub fn resolve_root(&self) -> Option<PathBuf> {
        if self.write_blocked { return None; }
        if let Some(r) = &self.settings.root {
            let p = PathBuf::from(r);
            return if p.is_absolute() && p.is_dir() { Some(p) } else { None };
        }
        for d in [r"D:\DevEnv", r"D:\DevEnvManager"] {
            let p = PathBuf::from(d);
            if p.is_dir() {
                return Some(p);
            }
        }
        None
    }

    /// 设置根目录并初始化 envs/current/downloads 结构
    pub fn set_root(&mut self, root: &str) -> Result<PathBuf> {
        self.ensure_writable()?;
        self.ensure_unchanged()?;
        let p = PathBuf::from(root);
        if !p.is_absolute() { return Err(crate::error::AppError::msg("管理根目录必须是绝对路径")); }
        if !p.exists() {
            fs::create_dir_all(&p)?;
        }
        for sub in ["envs", "current", "downloads"] {
            fs::create_dir_all(p.join(sub))?;
        }
        let old = self.settings.root.replace(root.to_string());
        if let Err(e) = self.save() { self.settings.root = old; return Err(e); }
        Ok(p)
    }

    /// 下载临时目录(默认 根目录/downloads)
    pub fn downloads_dir(&self, root: &Path) -> PathBuf {
        self.settings
            .downloads_dir
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(|| root.join("downloads"))
    }

    pub fn backups_dir(&self) -> PathBuf {
        self.data_dir.join("path_backups")
    }
}

fn parse_settings(bytes: &[u8]) -> Result<Settings> {
    let settings: Settings = serde_json::from_slice(bytes)
        .map_err(|e| AppError::msg(format!("配置文件格式无效: {e}")))?;
    if settings.schema_version > SCHEMA_VERSION {
        return Err(AppError::msg(format!("配置版本 {} 高于当前支持的 {}，请使用较新版本的 EnvCon", settings.schema_version, SCHEMA_VERSION)));
    }
    Ok(settings)
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

pub(crate) fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH};
    let temp = path.with_extension(format!("tmp-{}-{}", std::process::id(), NEXT_FILE.fetch_add(1, Ordering::Relaxed)));
    let mut file = OpenOptions::new().write(true).create_new(true).open(&temp)?;
    let result = (|| {
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        let from: Vec<u16> = temp.as_os_str().encode_wide().chain(Some(0)).collect();
        let to: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let ok = unsafe { MoveFileExW(from.as_ptr(), to.as_ptr(), MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH) };
        if ok == 0 { return Err(std::io::Error::last_os_error().into()); }
        Ok(())
    })();
    if result.is_err() { let _ = fs::remove_file(&temp); }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_settings_migrate_without_losing_unknown_fields_and_keep_backup() {
        let dir = crate::test_support::TestDir::new();
        let original = br#"{"root":null,"mirrors":{"node":"official"},"futureOption":{"enabled":true}}"#;
        fs::write(dir.path().join("config.json"), original).unwrap();
        let mut cfg = AppConfig::load_from(dir.path().into(), false);
        cfg.settings.downloads_dir = Some("D:/downloads".into());
        cfg.save().unwrap();
        assert_eq!(fs::read(cfg.backup_path()).unwrap(), original);
        let saved: serde_json::Value = serde_json::from_slice(&fs::read(&cfg.config_path).unwrap()).unwrap();
        assert_eq!(saved["schema_version"], 1);
        assert_eq!(saved["futureOption"]["enabled"], true);
        assert_eq!(saved["mirrors"]["node"], "official");
        cfg.restore_backup().unwrap();
        assert_eq!(fs::read(&cfg.config_path).unwrap(), original);
        assert!(cfg.settings.downloads_dir.is_none());
    }

    #[test]
    fn invalid_or_future_config_is_never_silently_overwritten() {
        for invalid in ["{broken", "null", r#"{"schema_version":99}"#, r#"{"root":3}"#] {
            let dir = crate::test_support::TestDir::new();
            let path = dir.path().join("config.json");
            fs::write(&path, invalid).unwrap();
            let mut cfg = AppConfig::load_from(dir.path().into(), false);
            assert!(cfg.write_blocked);
            assert!(cfg.warning.is_some());
            assert!(cfg.resolve_root().is_none());
            assert!(cfg.save().is_err());
            assert!(cfg.set_root(&dir.path().join("new").to_string_lossy()).is_err());
            assert!(!dir.path().join("new").exists());
            assert_eq!(fs::read_to_string(path).unwrap(), invalid);
        }
    }

    #[test]
    fn recovery_preserves_damaged_file_and_rejects_invalid_backup() {
        let dir = crate::test_support::TestDir::new();
        fs::write(dir.path().join("config.json"), "broken-original").unwrap();
        let mut cfg = AppConfig::load_from(dir.path().into(), false);
        fs::write(cfg.backup_path(), "bad-backup").unwrap();
        assert!(!cfg.has_valid_backup());
        assert!(cfg.restore_backup().is_err());
        assert_eq!(fs::read_to_string(&cfg.config_path).unwrap(), "broken-original");
        fs::write(cfg.backup_path(), r#"{"downloads_dir":"D:/restored"}"#).unwrap();
        assert!(cfg.has_valid_backup());
        cfg.restore_backup().unwrap();
        assert!(!cfg.write_blocked);
        assert_eq!(cfg.settings.downloads_dir.as_deref(), Some("D:/restored"));
        let preserved = fs::read_dir(dir.path()).unwrap().flatten().find(|e| e.file_name().to_string_lossy().starts_with("config.before-recovery-")).unwrap();
        assert_eq!(fs::read_to_string(preserved.path()).unwrap(), "broken-original");
    }

    #[test]
    fn stale_instance_and_locked_target_preserve_disk_and_memory() {
        let dir = crate::test_support::TestDir::new();
        let mut first = AppConfig::load_from(dir.path().into(), false);
        first.save().unwrap();
        let mut stale = AppConfig::load_from(dir.path().into(), false);
        first.settings.downloads_dir = Some("D:/one".into());
        first.save().unwrap();
        stale.settings.downloads_dir = Some("D:/two".into());
        assert!(stale.save().unwrap_err().to_string().contains("重新加载"));
        let before = fs::read(&first.config_path).unwrap();
        let _held = OpenOptions::new().read(true).share_mode(1).open(&first.config_path).unwrap();
        assert!(first.set_root(&dir.path().join("新 根目录").to_string_lossy()).is_err());
        assert!(first.settings.root.is_none());
        assert_eq!(fs::read(&first.config_path).unwrap(), before);
        assert_eq!(fs::read(first.backup_path()).unwrap(), before);
        assert!(!fs::read_dir(dir.path()).unwrap().flatten().any(|e| e.file_name().to_string_lossy().contains(".tmp-")));
    }

    #[test]
    fn missing_main_with_backup_requires_recovery_and_writer_lock_excludes_other_instances() {
        let dir = crate::test_support::TestDir::new();
        let mut cfg = AppConfig::load_from(dir.path().into(), false);
        assert!(!cfg.write_blocked);
        let held = cfg.lock_writer().unwrap();
        assert!(cfg.save().is_err());
        drop(held);
        cfg.save().unwrap();
        cfg.save().unwrap();
        fs::remove_file(&cfg.config_path).unwrap();
        cfg.reload();
        assert!(cfg.write_blocked);
        cfg.restore_backup().unwrap();
        assert!(cfg.config_path.is_file());
    }
    #[test]
    fn missing_explicit_root_never_falls_back_and_failed_save_restores_settings() {
        let dir = crate::test_support::TestDir::new();
        let missing = dir.path().join("missing").to_string_lossy().into_owned();
        let mut cfg = AppConfig::load_from(dir.path().into(), false);
        cfg.settings.root = Some(missing.clone());
        cfg.config_path = dir.path().join("absent/config.json");
        assert!(cfg.resolve_root().is_none());
        assert!(cfg.set_root(&dir.path().join("new").to_string_lossy()).is_err());
        assert_eq!(cfg.settings.root.as_deref(), Some(missing.as_str()));
    }
}

/// 读取 junction 指向的目标文件夹名;非链接或不存在返回 None
pub fn junction_target_name(junction_path: &Path) -> Option<String> {
    let meta = fs::symlink_metadata(junction_path).ok()?;
    // junction/符号链接在 symlink_metadata 下 is_symlink() 为 true
    if !meta.file_type().is_symlink() {
        return None;
    }
    let target = fs::read_link(junction_path).ok()?;
    target
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
}
