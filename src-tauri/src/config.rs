use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use crate::error::Result;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// 管理根目录(如 D:\DevEnv)
    pub root: Option<String>,
    /// 下载临时目录
    pub downloads_dir: Option<String>,
    /// 各类型镜像源偏好
    pub mirrors: std::collections::HashMap<String, String>,
}

/// 应用配置:数据目录定位(便携优先)+ 设置持久化
#[derive(Debug, Clone)]
pub struct AppConfig {
    pub data_dir: PathBuf,
    pub portable: bool,
    pub settings: Settings,
    config_path: PathBuf,
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

        let config_path = data_dir.join("config.json");
        let settings = fs::read_to_string(&config_path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();

        Self {
            data_dir,
            portable,
            settings,
            config_path,
        }
    }

    /// 原子写入:先写 .tmp 再 rename
    pub fn save(&self) -> Result<()> {
        let json = serde_json::to_string_pretty(&self.settings)?;
        let tmp = self.config_path.with_extension("json.tmp");
        fs::write(&tmp, json.as_bytes())?;
        fs::rename(&tmp, &self.config_path)?;
        Ok(())
    }

    /// 解析管理根目录:设置优先 → 默认目录存在则用(新名优先,兼容旧名) → None
    pub fn resolve_root(&self) -> Option<PathBuf> {
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missing_explicit_root_never_falls_back_and_failed_save_restores_settings() {
        let dir = crate::test_support::TestDir::new();
        let missing = dir.path().join("missing").to_string_lossy().into_owned();
        let mut cfg = AppConfig { data_dir: dir.path().into(), portable: false, settings: Settings { root: Some(missing.clone()), ..Default::default() }, config_path: dir.path().join("absent/config.json") };
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
