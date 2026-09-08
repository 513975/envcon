use std::path::Path;

use crate::error::{AppError, Result};
use crate::types::EnvType;

/// 切换 current/<junction> 指向指定环境
pub fn switch(root: &Path, env_type: EnvType, name: &str) -> Result<()> {
    let env_dir = root.join("envs").join(env_type.folder()).join(name);
    if !env_dir.exists() {
        return Err(AppError::msg(format!("环境目录不存在: {}", env_dir.display())));
    }

    let junction = root.join("current").join(env_type.junction());
    std::fs::create_dir_all(root.join("current"))?;

    remove_junction(&junction)?;
    junction::create(&env_dir, &junction)
        .map_err(|e| AppError::msg(format!("创建链接失败: {e}")))?;
    Ok(())
}

/// 移除指定类型的 current junction(不删除目标内容)
pub fn remove_junction_for(root: &Path, env_type: EnvType) -> Result<()> {
    let junction = root.join("current").join(env_type.junction());
    remove_junction(&junction)
}

/// 安全移除 junction:仅当它是链接时;带重试(进程占用场景)
fn remove_junction(junction: &Path) -> Result<()> {
    let meta = match std::fs::symlink_metadata(junction) {
        Ok(m) => m,
        Err(_) => return Ok(()), // 不存在,视为已移除
    };
    if !meta.file_type().is_symlink() {
        return Err(AppError::msg(format!(
            "{} 不是链接(可能是真实目录),为避免误删已跳过,请手动处理",
            junction.display()
        )));
    }
    let mut last_err = None;
    for i in 0..5 {
        match junction::delete(junction) {
            Ok(()) => return Ok(()),
            Err(e) => {
                last_err = Some(e);
                std::thread::sleep(std::time::Duration::from_millis(200 * (i + 1)));
            }
        }
    }
    Err(AppError::msg(format!(
        "移除链接失败(可能有进程占用,请关闭相关程序后重试): {:?}",
        last_err
    )))
}
