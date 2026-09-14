use std::path::Path;
use std::time::Duration;

use crate::error::{AppError, Result};
use crate::types::EnvType;
static SWITCH_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// 切换 current/<junction> 指向指定环境
pub fn switch(root: &Path, env_type: EnvType, name: &str) -> Result<()> {
    let _guard = SWITCH_LOCK.lock().unwrap();
    validate_name(name)?;
    let env_dir = root.join("envs").join(env_type.folder()).join(name);
    if !env_dir.is_dir() {
        return Err(AppError::msg(format!("环境目录不存在: {}", env_dir.display())));
    }

    let junction = root.join("current").join(env_type.junction());
    std::fs::create_dir_all(root.join("current"))?;

    let previous = std::fs::read_link(&junction).ok();
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos();
    let prepared = root.join("current").join(format!(".{}-next-{stamp}", env_type.junction()));
    junction::create(&env_dir, &prepared).map_err(|e| AppError::msg(format!("创建新链接失败，当前环境保留: {e}")))?;
    if let Err(e) = remove_junction(&junction) { let _ = std::fs::remove_dir(&prepared); return Err(e); }
    if let Err(e) = std::fs::rename(&prepared, &junction) {
        let restored = previous.map(|target| junction::create(target, &junction)).transpose();
        let _ = std::fs::remove_dir(&prepared);
        return Err(AppError::msg(format!("切换失败: {e}；旧链接恢复结果: {restored:?}")));
    }
    Ok(())
}

pub(crate) fn validate_name(name: &str) -> Result<()> {
    let reserved = name.split('.').next().unwrap_or("").to_ascii_uppercase();
    if name.is_empty() || name == "." || name == ".." || name.ends_with(['.', ' '])
        || name.chars().any(|c| c.is_control() || "\\/:*?\"<>|".contains(c))
        || matches!(reserved.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ["COM", "LPT"].iter().any(|p| reserved.strip_prefix(p).is_some_and(|n| matches!(n, "1"|"2"|"3"|"4"|"5"|"6"|"7"|"8"|"9"))) {
        return Err(AppError::msg("环境名称必须是单个有效目录名，不能包含路径或 Windows 保留名称"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn environment_names_cannot_escape_managed_directory() {
        for name in ["../outside", "..", "D:\\outside", "a/b", "a\\b", "NUL", "COM1.zip", "name."] { assert!(super::validate_name(name).is_err(), "{name}"); }
        for name in ["node-22.1", "jdk 21", "自定义环境"] { assert!(super::validate_name(name).is_ok(), "{name}"); }
    }
}

/// 移除指定类型的 current junction(不删除目标内容)
pub fn remove_junction_for(root: &Path, env_type: EnvType) -> Result<()> {
    let _guard = SWITCH_LOCK.lock().unwrap();
    let junction = root.join("current").join(env_type.junction());
    remove_junction(&junction)
}

/// 判定路径是否为链接(junction 或符号链接)
fn is_link(path: &Path) -> bool {
    junction::exists(path).unwrap_or(false)
        || std::fs::symlink_metadata(path)
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false)
}

/// 目录是否为空(读取失败按非空处理,保守起见)
fn is_empty_dir(path: &Path) -> bool {
    std::fs::read_dir(path).is_ok_and(|mut rd| rd.next().is_none())
}

/// 带重试执行删除操作(进程占用场景)
fn retry_remove(label: &str, mut f: impl FnMut() -> std::io::Result<()>) -> Result<()> {
    let mut last_err: Option<std::io::Error> = None;
    for i in 0..5 {
        match f() {
            Ok(()) => return Ok(()),
            // 期间被并发移除,视为成功
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => {
                last_err = Some(e);
                std::thread::sleep(Duration::from_millis(200 * (i + 1)));
            }
        }
    }
    let msg = last_err.map(|e| e.to_string()).unwrap_or_default();
    Err(AppError::msg(format!(
        "{label}失败(可能有进程占用,请关闭相关程序后重试): {msg}"
    )))
}

/// 把路径改名挪开为 <名称>.old-<时间戳>(非空真实目录,避免误删数据)
fn rename_aside(path: &Path) -> Result<()> {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".into());
    let parent = path.parent().ok_or_else(|| AppError::msg("路径异常"))?;
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let mut bak = parent.join(format!("{name}.old-{ts}"));
    let mut n = 1u64;
    while bak.exists() {
        n += 1;
        bak = parent.join(format!("{name}.old-{ts}-{n}"));
    }
    std::fs::rename(path, &bak).map_err(|e| {
        AppError::msg(format!(
            "{} 为真实目录且无法自动挪开({e}),请手动处理后重试",
            path.display()
        ))
    })
}

/// 安全清除 current/<junction> 占位,为重新创建链接做准备:
/// - 链接 → 只删链接本身(RemoveDirectoryW 对 junction 不会触碰目标内容;
///   不能用 junction::delete,它只清除链接属性,残留的空目录会挡住后续重建)
/// - 空的真实目录(历史损坏残留) → 直接删除
/// - 非空真实目录/文件 → 改名挪开为 <名称>.old-<时间戳>,不误删数据
fn remove_junction(junction: &Path) -> Result<()> {
    let Ok(meta) = std::fs::symlink_metadata(junction) else {
        return Ok(()); // 不存在,视为已移除
    };

    if is_link(junction) {
        return retry_remove("移除链接", || std::fs::remove_dir(junction));
    }
    if meta.is_dir() && is_empty_dir(junction) {
        return retry_remove("清理残留目录", || std::fs::remove_dir(junction));
    }
    rename_aside(junction)
}
