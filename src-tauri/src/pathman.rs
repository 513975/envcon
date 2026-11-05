use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};
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

/// 保存指定作用域的 PATH(自动备份 + 保留原值类型 + 广播生效)
///
/// 用户级与系统级共用同一套 `path_ownership::apply` 校验链（备份、journal、
/// 写入前后复核），只有 hive 与 journal 位置不同。系统级需要管理员权限。
pub fn save_path(scope: EnvScope, entries: &[String], backups_dir: &Path) -> Result<()> {
    let mut store = RegistryPathStore::open(scope, backups_dir)?;
    crate::path_ownership::apply(
        &mut store,
        &journal_path(scope, backups_dir),
        crate::path_ownership::Change::Replace(entries),
    )?;
    broadcast_change();
    Ok(())
}

/// PATH 来源日志的位置。
///
/// 两个作用域各用一份 journal：混在一起会让「某条目由本应用写入」的记录无法判断
/// 属于哪个 hive，恢复时可能把用户级记录套用到系统级。
fn journal_path(scope: EnvScope, backups_dir: &Path) -> std::path::PathBuf {
    let dir = match scope {
        // 用户级沿用历史路径，避免既有 journal 读不到。
        EnvScope::User => "ownership",
        EnvScope::System => "ownership-system",
    };
    backups_dir.join(dir).join("journal.json")
}

/// 保存用户级 PATH(自动备份 + 保留原值类型 + 广播生效)
pub fn save_user_path(entries: &[String], backups_dir: &Path) -> Result<()> {
    save_path(EnvScope::User, entries, backups_dir)
}

pub fn preview_system_path_edit(
    entries: Option<&[String]>,
    expected_path: Option<&str>,
    backups: &Path,
) -> Result<crate::path_ownership::EditPlan> {
    use crate::path_ownership::PathStore;
    let current = RegistryPathStore::open(EnvScope::System, backups)?.read()?;
    if expected_path.is_some_and(|expected| expected != current) {
        return Err(crate::error::AppError::msg("系统 PATH 已变化，请重新读取后编辑"));
    }
    let original = if current.is_empty() { Vec::new() }
        else { current.split(';').map(str::to_owned).collect() };
    crate::path_ownership::preview_edit(
        &journal_path(EnvScope::System, backups), &current, entries.unwrap_or(&original),
    )
}

pub fn apply_system_path_edit(plan: &crate::path_ownership::EditPlan, backups: &Path) -> Result<()> {
    let mut store = RegistryPathStore::open(EnvScope::System, backups)?;
    crate::path_ownership::apply(
        &mut store, &journal_path(EnvScope::System, backups), crate::path_ownership::Change::Edit(plan),
    )?;
    broadcast_change();
    Ok(())
}

/// 把缺失的条目追加进用户 PATH,返回新增条目列表
pub fn integrate_entries(new_entries: &[String], backups_dir: &Path) -> Result<Vec<String>> {
    let mut store = RegistryPathStore::open(EnvScope::User, backups_dir)?;
    let added = crate::path_ownership::apply(
        &mut store,
        &backups_dir.join("ownership/journal.json"),
        crate::path_ownership::Change::Append(new_entries),
    )?;
    if !added.is_empty() {
        broadcast_change();
    }
    Ok(added)
}

/// 按预览计划把受管入口移到用户 PATH 最前；只调整顺序，不增删条目。
pub fn apply_path_reorder(
    plan: &crate::path_ownership::ReorderPlan,
    backups_dir: &Path,
) -> Result<crate::path_ownership::ReorderPlan> {
    let mut store = RegistryPathStore::open(EnvScope::User, backups_dir)?;
    crate::path_ownership::apply(
        &mut store,
        &backups_dir.join("ownership/journal.json"),
        crate::path_ownership::Change::Reorder(plan),
    )?;
    broadcast_change();
    Ok(plan.clone())
}

pub(crate) struct RegistryPathStore<'a> {
    env: RegKey,
    scope: EnvScope,
    backups: &'a Path,
}

impl<'a> RegistryPathStore<'a> {
    pub(crate) fn open(scope: EnvScope, backups: &'a Path) -> Result<Self> {
        let (hive, subkey) = scope.hive();
        let env = RegKey::predef(hive).open_subkey_with_flags(subkey, KEY_READ)?;
        Ok(Self { env, scope, backups })
    }

    fn raw(&self) -> Result<RegValue> {
        let value = match self.env.get_raw_value(PATH_VALUE) {
            Ok(value) => value,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => RegValue {
                bytes: encode_reg_string(""), vtype: RegType::REG_EXPAND_SZ,
            },
            Err(error) => return Err(error.into()),
        };
        if !matches!(value.vtype, RegType::REG_SZ | RegType::REG_EXPAND_SZ) {
            return Err(crate::error::AppError::msg("PATH 注册表类型不是字符串，已停止写入"));
        }
        checked_path_string(&value.bytes)?;
        Ok(value)
    }
}

impl crate::path_ownership::PathStore for RegistryPathStore<'_> {
    fn read(&mut self) -> Result<String> {
        Ok(decode_reg_string(&self.raw()?.bytes))
    }

    fn write(&mut self, value: &str) -> Result<()> {
        let (hive, subkey) = self.scope.hive();
        let writable = RegKey::predef(hive)
            .open_subkey_with_flags(subkey, KEY_READ | KEY_WRITE)
            .map_err(|error| match self.scope {
                EnvScope::System if !is_elevated() => crate::error::AppError::msg(
                    "写入系统 PATH 需要管理员权限；请通过应用内的提权写入流程操作",
                ),
                _ => crate::error::AppError::msg(format!(
                    "无法写入{} PATH: {error}",
                    self.scope.label()
                )),
            })?;
        let old = self.raw()?;
        backup_path(&decode_reg_string(&old.bytes), self.backups)?;
        write_path_value(&writable, value, old.vtype)
    }
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
        .as_chunks::<2>().0.iter()
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    let s = String::from_utf16_lossy(&units);
    s.trim_end_matches('\0').to_string()
}

fn checked_path_string(bytes: &[u8]) -> Result<String> {
    if !bytes.len().is_multiple_of(2) {
        return Err(crate::error::AppError::msg("PATH 注册表字符串字节长度无效"));
    }
    let units: Vec<u16> = bytes.as_chunks::<2>().0.iter()
        .map(|bytes| u16::from_le_bytes([bytes[0], bytes[1]])).collect();
    let value = String::from_utf16(&units)
        .map_err(|_| crate::error::AppError::msg("PATH 包含无效 UTF-16，已停止写入"))?;
    let value = value.trim_end_matches('\0');
    if value.contains('\0') {
        return Err(crate::error::AppError::msg("PATH 包含嵌入的空字符，已停止写入"));
    }
    Ok(value.into())
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
    fn malformed_path_values_are_not_replaced_with_empty_or_lossy_text() {
        assert!(super::checked_path_string(&[0]).is_err());
        assert!(super::checked_path_string(&[0, 0xd8]).is_err());
        assert!(super::checked_path_string(&super::encode_reg_string("first\0second")).is_err());
        assert_eq!(
            super::checked_path_string(&super::encode_reg_string(r"D:\user;;D:\tools")).unwrap(),
            r"D:\user;;D:\tools"
        );
    }

    #[test]
    fn rapid_path_changes_keep_distinct_backups() {
        let dir = crate::test_support::TestDir::new();
        super::backup_path("D:\\first", dir.path()).unwrap();
        super::backup_path("D:\\second", dir.path()).unwrap();
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 2);
    }

    /// 两个作用域必须映射到不同的注册表位置：用户级写 HKCU、系统级写 HKLM。
    /// 写错 hive 会让「改用户变量」意外影响所有用户，是最严重的一类错误。
    #[test]
    fn scopes_map_to_distinct_hives_and_only_system_needs_elevation() {
        let (user_hive, user_subkey) = super::EnvScope::User.hive();
        let (system_hive, system_subkey) = super::EnvScope::System.hive();
        assert_ne!(user_hive, system_hive);
        assert_ne!(user_subkey, system_subkey);
        assert!(user_subkey.eq_ignore_ascii_case("Environment"));
        // 系统级必须在 Session Manager 下，否则改的不是环境变量。
        assert!(system_subkey.contains("Session Manager"));
        assert!(system_subkey.contains("Environment"));

        assert!(!super::EnvScope::User.needs_elevation());
        assert!(super::EnvScope::System.needs_elevation());
        assert_eq!(super::EnvScope::User.label(), "用户");
        assert_eq!(super::EnvScope::System.label(), "系统");
    }

    /// 提权请求承载结构化数据，不承载可执行命令。
    #[test]
    fn elevation_request_serializes_only_names_and_values() {
        let ops = vec![
            super::EnvWriteOp::Set {
                name: "JAVA_HOME".into(),
                value: r"D:\managed\current\jdk".into(),
            },
            super::EnvWriteOp::Delete {
                name: "OLD_VAR".into(),
            },
            super::EnvWriteOp::Path {
                plan: crate::path_ownership::EditPlan {
                    expected_path: r"C:\Windows".into(), journal_snapshot: "fixture".into(),
                    entries: vec![r"C:\Windows".into(), r"D:\tools".into()],
                    resulting_path: r"C:\Windows;D:\tools".into(),
                    sources: vec!["shared".into(), "manual".into()],
                },
            },
        ];
        let json = serde_json::to_string(&ops).unwrap();
        assert!(json.contains("\"op\":\"set\""));
        assert!(json.contains("\"op\":\"delete\""));
        assert!(json.contains("\"op\":\"path\""));
        assert!(json.contains("JAVA_HOME"));
        assert!(json.contains("OLD_VAR"));
        // 回环：子进程读回的内容必须与主进程发出的一致。
        let parsed: Vec<super::EnvWriteOp> = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, ops);
    }

    /// 非法变量名必须在进入注册表之前被拒绝，而不是让系统 API 报错。
    #[test]
    fn invalid_names_are_rejected_before_touching_the_registry() {
        let dir = crate::test_support::TestDir::new();
        let backups = dir.path().join("path_backups");
        for name in ["", "Path", "path", "A=B", "A\0B"] {
            assert!(
                super::set_env_var(super::EnvScope::User, name, "x", &backups).is_err(),
                "name: {name:?}"
            );
            assert!(
                super::delete_env_var(super::EnvScope::User, name, &backups).is_err(),
                "delete name: {name:?}"
            );
        }
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

// ---------- 环境变量管理（用户级 / 系统级） ----------

/// 环境变量作用域。
///
/// 系统级位于 HKLM，写入需要管理员权限；用户级位于 HKCU，当前用户即可写。
/// 两者用同一套读写逻辑，只在 hive 与权限要求上不同。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EnvScope {
    User,
    System,
}

impl EnvScope {
    fn hive(self) -> (HKEY, &'static str) {
        match self {
            EnvScope::User => (HKEY_CURRENT_USER, HKCU_ENV),
            EnvScope::System => (HKEY_LOCAL_MACHINE, HKLM_ENV),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            EnvScope::User => "用户",
            EnvScope::System => "系统",
        }
    }

    /// 该作用域是否需要管理员权限才能写。
    pub fn needs_elevation(self) -> bool {
        self == EnvScope::System
    }
}

/// 提权子进程执行的一条环境变量写入操作。
///
/// 序列化后写入临时文件，由提权子进程读取并执行；只包含变量名与值，不含任何
/// 可执行内容或路径，避免提权进程被利用去执行别的东西。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "lowercase")]
pub enum EnvWriteOp {
    Set { name: String, value: String },
    Delete { name: String },
    Path { plan: crate::path_ownership::EditPlan },
}

/// 当前进程是否以管理员身份运行。
///
/// 用注册表写探测代替 token 查询：能打开 HKLM 环境键的可写句柄，就说明有权限。
/// 探测完立即关闭，不修改任何值。
pub fn is_elevated() -> bool {
    RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey_with_flags(HKLM_ENV, KEY_READ | KEY_WRITE)
        .is_ok()
}

/// 读取指定作用域的全部环境变量(不含 Path)
pub fn get_env_vars(scope: EnvScope) -> Result<HashMap<String, String>> {
    let (hive, subkey) = scope.hive();
    let key = RegKey::predef(hive).open_subkey_with_flags(subkey, KEY_READ)?;
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

/// 读取用户级全部环境变量(HKCU\Environment,不含 Path)
pub fn get_user_env_vars() -> Result<HashMap<String, String>> {
    get_env_vars(EnvScope::User)
}

/// 写入单个环境变量(自动备份旧值 + 广播)。
///
/// 系统级写入要求当前进程已是管理员，否则返回明确错误——不在后台偷偷提权。
pub fn set_env_var(scope: EnvScope, name: &str, value: &str, backups_dir: &Path) -> Result<()> {
    if name.is_empty() || name.eq_ignore_ascii_case(PATH_VALUE) {
        return Err(crate::error::AppError::msg("变量名无效"));
    }
    if name.contains(['\0', '=']) {
        return Err(crate::error::AppError::msg("变量名不能包含 = 或空字符"));
    }
    let (hive, subkey) = scope.hive();
    let env = RegKey::predef(hive)
        .open_subkey_with_flags(subkey, KEY_READ | KEY_WRITE)
        .map_err(|error| match scope {
            EnvScope::System if !is_elevated() => crate::error::AppError::msg(
                "写入系统环境变量需要管理员权限；请通过应用内的提权写入流程操作",
            ),
            _ => crate::error::AppError::msg(format!(
                "无法写入{}环境变量: {error}",
                scope.label()
            )),
        })?;

    // 备份旧值(可能不存在)
    let old = env.get_raw_value(name).ok();
    backup_env_var(scope, name, old.as_ref(), backups_dir)?;

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

/// 写入用户级环境变量
pub fn set_user_env_var(name: &str, value: &str, backups_dir: &Path) -> Result<()> {
    set_env_var(EnvScope::User, name, value, backups_dir)
}

/// 删除环境变量(自动备份旧值 + 广播)
pub fn delete_env_var(scope: EnvScope, name: &str, backups_dir: &Path) -> Result<()> {
    if name.is_empty() || name.eq_ignore_ascii_case(PATH_VALUE) || name.contains(['\0', '=']) {
        return Err(crate::error::AppError::msg("变量名无效"));
    }
    let (hive, subkey) = scope.hive();
    let env = RegKey::predef(hive)
        .open_subkey_with_flags(subkey, KEY_READ | KEY_WRITE)
        .map_err(|error| match scope {
            EnvScope::System if !is_elevated() => crate::error::AppError::msg(
                "删除系统环境变量需要管理员权限；请通过应用内的提权写入流程操作",
            ),
            _ => crate::error::AppError::msg(format!(
                "无法写入{}环境变量: {error}",
                scope.label()
            )),
        })?;

    let old = env.get_raw_value(name)?;
    backup_env_var(scope, name, Some(&old), backups_dir)?;
    env.delete_value(name)?;
    broadcast_change();
    Ok(())
}

/// 删除用户级环境变量
pub fn delete_user_env_var(name: &str, backups_dir: &Path) -> Result<()> {
    delete_env_var(EnvScope::User, name, backups_dir)
}

/// 备份环境变量旧值到 envvar_backups/{timestamp}.json
///
/// 文件名带作用域前缀：同名的用户级与系统级变量是两条独立记录，混在一起会让
/// 「恢复某个值」时无法判断该写回哪个 hive。
fn backup_env_var(
    scope: EnvScope,
    name: &str,
    old: Option<&RegValue>,
    backups_dir: &Path,
) -> Result<()> {
    let dir = backups_dir.parent().unwrap_or(backups_dir).join("envvar_backups");
    fs::create_dir_all(&dir)?;
    let ts = timestamp_utc();
    let file = dir.join(format!(
        "{ts}_{}_{}.json",
        scope.label(),
        name.replace(|c: char| !c.is_ascii_alphanumeric(), "_")
    ));
    let payload = serde_json::json!({
        "scope": scope,
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
