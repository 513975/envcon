use std::collections::HashMap;
use std::fs;
use std::path::Path;

use winreg::enums::{RegType, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WRITE};
use winreg::{RegKey, RegValue, HKEY};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG, WM_SETTINGCHANGE,
};

use crate::error::Result;
use crate::types::PathState;

const PATH_VALUE: &str = "Path";
const HKCU_ENV: &str = "Environment";
const HKLM_ENV: &str = r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment";

/// 读取用户级与系统级 PATH
pub fn get_path_state() -> Result<PathState> {
    let user = read_path(HKEY_CURRENT_USER, HKCU_ENV)?;
    let system = read_path(HKEY_LOCAL_MACHINE, HKLM_ENV)?;
    Ok(PathState { user, system })
}

fn read_path(hkey: HKEY, subkey: &str) -> Result<Vec<String>> {
    let key = RegKey::predef(hkey).open_subkey_with_flags(subkey, KEY_READ)?;
    let raw = match key.get_raw_value(PATH_VALUE) {
        Ok(value) => value,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.into()),
    };
    let s = decode_reg_string(&raw.bytes);
    Ok(split_path(&s))
}

pub fn split_path(s: &str) -> Vec<String> {
    s.split(';')
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .collect()
}

/// 保存用户级 PATH(自动备份 + 保留原值类型 + 广播生效)
pub fn save_user_path(entries: &[String], backups_dir: &Path) -> Result<()> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let env = hkcu.open_subkey_with_flags(HKCU_ENV, KEY_READ | KEY_WRITE)?;

    let old_raw = env.get_raw_value(PATH_VALUE).unwrap_or(RegValue {
        bytes: encode_reg_string(""),
        vtype: RegType::REG_EXPAND_SZ,
    });
    let old = decode_reg_string(&old_raw.bytes);
    backup_path(&old, backups_dir)?;

    write_path_value(&env, &entries.join(";"), old_raw.vtype)?;
    broadcast_change();
    Ok(())
}

/// 把缺失的条目追加进用户 PATH,返回新增条目列表
pub fn integrate_entries(new_entries: &[String], backups_dir: &Path) -> Result<Vec<String>> {
    let state = get_path_state()?;
    let existing: Vec<String> = state.user.iter().map(|p| p.to_lowercase()).collect();
    let to_add: Vec<String> = new_entries
        .iter()
        .filter(|e| !existing.contains(&e.to_lowercase()))
        .cloned()
        .collect();
    if to_add.is_empty() {
        return Ok(vec![]);
    }

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let env = hkcu.open_subkey_with_flags(HKCU_ENV, KEY_READ | KEY_WRITE)?;
    let old_raw = env.get_raw_value(PATH_VALUE).unwrap_or(RegValue {
        bytes: encode_reg_string(""),
        vtype: RegType::REG_EXPAND_SZ,
    });
    let old = decode_reg_string(&old_raw.bytes);
    backup_path(&old, backups_dir)?;

    let mut merged = state.user;
    merged.extend(to_add.iter().cloned());
    write_path_value(&env, &merged.join(";"), old_raw.vtype)?;
    broadcast_change();
    Ok(to_add)
}

/// 以指定类型写入 PATH(UTF-16LE + 终止符)
fn write_path_value(env: &RegKey, joined: &str, ty: RegType) -> Result<()> {
    let ty = if matches!(ty, RegType::REG_SZ | RegType::REG_EXPAND_SZ) {
        ty
    } else {
        RegType::REG_EXPAND_SZ
    };
    env.set_raw_value(
        PATH_VALUE,
        &RegValue {
            bytes: encode_reg_string(joined),
            vtype: ty,
        },
    )?;
    Ok(())
}

/// UTF-16LE 编码 + null 终止符
fn encode_reg_string(s: &str) -> Vec<u8> {
    let mut bytes: Vec<u8> = Vec::with_capacity((s.len() + 1) * 2);
    for u in s.encode_utf16().chain(std::iter::once(0)) {
        bytes.extend_from_slice(&u.to_le_bytes());
    }
    bytes
}

/// 解码 UTF-16LE 注册表字符串(去尾部 null)
fn decode_reg_string(bytes: &[u8]) -> String {
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    let s = String::from_utf16_lossy(&units);
    s.trim_end_matches('\0').to_string()
}

fn backup_path(old: &str, backups_dir: &Path) -> Result<()> {
    fs::create_dir_all(backups_dir)?;
    let ts = timestamp_utc();
    let file = backups_dir.join(format!("{}.json", ts));
    let json = serde_json::json!({ "userPath": split_path(old) });
    let tmp = file.with_extension("tmp");
    fs::write(&tmp, serde_json::to_string_pretty(&json)?)?;
    fs::rename(&tmp, &file)?;

    // 只保留最近 20 份
    let mut files: Vec<_> = fs::read_dir(backups_dir)?
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .map(|e| e.path())
        .collect();
    files.sort();
    while files.len() > 20 {
        let f = files.remove(0);
        let _ = fs::remove_file(f);
    }
    Ok(())
}

/// UTC 时间戳字符串(备份文件名用):20260908T054401Z
fn timestamp_utc() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();
    let (y, mo, d, h, mi, s) = unix_to_utc(secs);
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let sequence = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    format!("{y:04}{mo:02}{d:02}T{h:02}{mi:02}{s:02}Z-{:09}-{}-{sequence}", now.subsec_nanos(), std::process::id())
}

fn unix_to_utc(secs: u64) -> (u64, u64, u64, u64, u64, u64) {
    let days = secs / 86400;
    let rem = secs % 86400;
    let (h, mi, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    let z = days + 719468;
    let era = z / 146097;
    let doe = z % 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d, h, mi, s)
}

#[cfg(test)]
mod tests {
    #[test]
    fn rapid_path_changes_keep_distinct_backups() {
        let dir = crate::test_support::TestDir::new();
        super::backup_path("D:\\first", dir.path()).unwrap();
        super::backup_path("D:\\second", dir.path()).unwrap();
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 2);
    }
}

/// 广播环境变量变更,新开的终端立即生效
pub fn broadcast_change() {
    let env_str: Vec<u16> = "Environment\0".encode_utf16().collect();
    unsafe {
        SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            0,
            env_str.as_ptr() as isize,
            SMTO_ABORTIFHUNG,
            3000,
            std::ptr::null_mut(),
        );
    }
}

// ---------- 用户环境变量管理 ----------

/// 读取用户级全部环境变量(HKCU\Environment,不含 Path)
pub fn get_user_env_vars() -> Result<HashMap<String, String>> {
    let key = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(HKCU_ENV, KEY_READ)?;
    let mut map = HashMap::new();
    for (name, value) in key.enum_values().flatten() {
        if name.eq_ignore_ascii_case(PATH_VALUE) {
            continue;
        }
        if matches!(value.vtype, RegType::REG_SZ | RegType::REG_EXPAND_SZ) {
            map.insert(name, decode_reg_string(&value.bytes));
        }
    }
    Ok(map)
}

/// 写入单个用户环境变量(自动备份旧值 + 广播)
pub fn set_user_env_var(name: &str, value: &str, backups_dir: &Path) -> Result<()> {
    if name.is_empty() || name.eq_ignore_ascii_case(PATH_VALUE) {
        return Err(crate::error::AppError::msg("变量名无效"));
    }
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let env = hkcu.open_subkey_with_flags(HKCU_ENV, KEY_READ | KEY_WRITE)?;

    // 备份旧值(可能不存在)
    let old = env.get_raw_value(name).ok();
    backup_env_var(name, old.as_ref(), backups_dir)?;

    env.set_raw_value(
        name,
        &RegValue {
            bytes: encode_reg_string(value),
            vtype: RegType::REG_SZ,
        },
    )?;
    broadcast_change();
    Ok(())
}

/// 删除用户环境变量(自动备份旧值 + 广播)
pub fn delete_user_env_var(name: &str, backups_dir: &Path) -> Result<()> {
    if name.is_empty() || name.eq_ignore_ascii_case(PATH_VALUE) || name.contains(['\0', '=']) { return Err(crate::error::AppError::msg("变量名无效")); }
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let env = hkcu.open_subkey_with_flags(HKCU_ENV, KEY_READ | KEY_WRITE)?;

    let old = env.get_raw_value(name)?;
    backup_env_var(name, Some(&old), backups_dir)?;
    env.delete_value(name)?;
    broadcast_change();
    Ok(())
}

/// 备份环境变量旧值到 envvar_backups/{timestamp}.json
fn backup_env_var(name: &str, old: Option<&RegValue>, backups_dir: &Path) -> Result<()> {
    let dir = backups_dir.parent().unwrap_or(backups_dir).join("envvar_backups");
    fs::create_dir_all(&dir)?;
    let ts = timestamp_utc();
    let file = dir.join(format!("{ts}_{}.json", name.replace(|c: char| !c.is_ascii_alphanumeric(), "_")));
    let payload = serde_json::json!({
        "name": name,
        "old": old.map(|v| decode_reg_string(&v.bytes)),
        "oldType": old.map(|v| format!("{:?}", v.vtype)),
    });
    let tmp = file.with_extension("tmp");
    fs::write(&tmp, serde_json::to_string_pretty(&payload)?)?;
    fs::rename(&tmp, &file)?;

    // 备份目录总量控制(保留最近 60 份)
    let mut files: Vec<_> = fs::read_dir(&dir)?
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .map(|e| e.path())
        .collect();
    files.sort();
    while files.len() > 60 {
        let f = files.remove(0);
        let _ = fs::remove_file(f);
    }
    Ok(())
}
