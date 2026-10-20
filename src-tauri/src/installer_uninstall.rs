use std::fs::{self, File};
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_32KEY, KEY_WOW64_64KEY};
use winreg::RegKey;

use crate::detect::system;
use crate::error::{AppError, Result};
use crate::types::EnvType;

const UNINSTALL_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Registration {
    pub id: String,
    pub key_name: String,
    pub display_name: String,
    pub publisher: String,
    pub install_location: Option<String>,
    pub uninstall_command: Option<String>,
    pub quiet_command: Option<String>,
    pub bundle_provider: Option<String>,
    /// Read from the same hive and view as the matching CPython provider.
    pub python_install_path: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RegistrySnapshot {
    pub records: Vec<Registration>,
    pub errors: Vec<String>,
}

fn optional_string(key: &RegKey, name: &str) -> Result<Option<String>> {
    match key.get_value::<String, _>(name) {
        Ok(value) if value.contains('\0') => Err(AppError::msg("登记字符串含嵌入空字符")),
        Ok(value) if value.trim().is_empty() => Ok(None),
        Ok(value) => Ok(Some(value)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn python_slot(provider: &str) -> Option<&str> {
    let slot = provider.strip_prefix("CPython-")?;
    let version = slot.strip_suffix("-64").unwrap_or(slot);
    let minor = version.strip_prefix("3.")?;
    if minor.is_empty() || !minor.bytes().all(|c| c.is_ascii_digit()) { return None; }
    Some(slot)
}

fn python_bundle_name(name: &str) -> bool {
    let Some(version) = name.strip_prefix("Python ").and_then(|name| name.strip_suffix(" (64-bit)")) else { return false; };
    let parts: Vec<_> = version.split('.').collect();
    parts.len() == 3 && parts.iter().all(|part| !part.is_empty() && part.bytes().all(|c| c.is_ascii_digit()))
}

fn relevant(kind: EnvType, record: &Registration) -> bool {
    match kind {
        EnvType::Python => python_bundle_name(&record.display_name)
            || record.bundle_provider.as_deref().is_some_and(|provider| provider.starts_with("CPython-")),
        EnvType::Llvm => record.display_name == "LLVM" || record.display_name.starts_with("LLVM "),
        _ => false,
    }
}

/// Read only. Tests supply RegistrySnapshot directly and never call this host adapter.
pub(crate) fn read_registry() -> RegistrySnapshot {
    let mut snapshot = RegistrySnapshot::default();
    for (hive_name, hive) in [("HKCU", HKEY_CURRENT_USER), ("HKLM", HKEY_LOCAL_MACHINE)] {
        for (view_name, view) in [("64", KEY_WOW64_64KEY), ("32", KEY_WOW64_32KEY)] {
            // HKCU\Software is shared under WOW64; reading both views invents duplicates.
            if hive == HKEY_CURRENT_USER && view == KEY_WOW64_32KEY { continue; }
            let root = RegKey::predef(hive);
            let parent = match root.open_subkey_with_flags(UNINSTALL_KEY, KEY_READ | view) {
                Ok(key) => key,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => {
                    snapshot.errors.push(format!("{hive_name}/{view_name} 卸载登记无法读取：{error}"));
                    continue;
                }
            };
            for name in parent.enum_keys() {
                let name = match name {
                    Ok(name) => name,
                    Err(error) => { snapshot.errors.push(format!("{hive_name}/{view_name} 枚举失败：{error}")); continue; }
                };
                let id = format!("{hive_name}/{view_name}/{name}");
                let record = (|| -> Result<Option<Registration>> {
                    let key = parent.open_subkey_with_flags(&name, KEY_READ | view)?;
                    let display_name = optional_string(&key, "DisplayName")?.unwrap_or_default();
                    if !display_name.starts_with("Python ") && !display_name.starts_with("LLVM") {
                        return Ok(None);
                    }
                    let provider = optional_string(&key, "BundleProviderKey")?;
                    let python_install_path = match provider.as_deref().and_then(python_slot) {
                        Some(slot) => {
                            let python = format!(r"Software\Python\PythonCore\{slot}\InstallPath");
                            match root.open_subkey_with_flags(python, KEY_READ | view) {
                                Ok(key) => optional_string(&key, "")?,
                                Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                                Err(error) => return Err(error.into()),
                            }
                        }
                        None => None,
                    };
                    Ok(Some(Registration {
                        id: id.clone(), key_name: name.clone(), display_name,
                        publisher: optional_string(&key, "Publisher")?.unwrap_or_default(),
                        install_location: optional_string(&key, "InstallLocation")?,
                        uninstall_command: optional_string(&key, "UninstallString")?,
                        quiet_command: optional_string(&key, "QuietUninstallString")?,
                        bundle_provider: provider, python_install_path,
                    }))
                })();
                match record {
                    Ok(Some(record)) => snapshot.records.push(record),
                    Ok(None) => {}
                    Err(error) => snapshot.errors.push(format!("{id} 登记读取不完整：{error}")),
                }
            }
        }
    }
    snapshot.records.sort_by(|a, b| a.id.cmp(&b.id));
    snapshot.errors.sort();
    snapshot
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Invocation {
    pub registration_id: String,
    pub executable: String,
    pub arguments: Vec<String>,
    /// Detects changes between preview and execution, not publisher authenticity.
    pub file_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Assessment {
    pub registration_id: String,
    pub state: String,
    pub details: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub env_type: EnvType,
    pub install_path: String,
    pub state: String,
    pub invocation: Option<Invocation>,
    pub assessments: Vec<Assessment>,
    pub errors: Vec<String>,
    pub registry_fingerprint: String,
}

fn digest(bytes: &[u8]) -> String { format!("{:x}", Sha256::digest(bytes)) }

fn checked_path(value: &str) -> Result<PathBuf> {
    let path = PathBuf::from(value.trim().trim_end_matches(['\\', '/']));
    if !path.is_absolute() || value.contains(['"', ';', '%', '\0'])
        || path.components().any(|part| matches!(part, Component::ParentDir | Component::CurDir))
        || value.split(['\\', '/']).any(|part| part == "." || part == "..")
        || system::has_reparse_component(&path)
    {
        return Err(AppError::msg("登记路径不是可核验的绝对普通路径"));
    }
    Ok(path)
}

fn exact_location(value: &str, selected: &Path) -> Result<bool> {
    let path = checked_path(value)?;
    // Never collapse a Junction alias into the selected installation.
    Ok(system::path_key(&path) == system::path_key(selected))
}

/// Accept a quoted absolute EXE or an unquoted path without whitespace.
/// The small allowlist below validates every argument; no shell interpretation.
fn parse_command(value: &str) -> Result<(PathBuf, Vec<String>)> {
    let value = value.trim();
    let (exe, rest) = if let Some(quoted) = value.strip_prefix('"') {
        let end = quoted.find('"').ok_or_else(|| AppError::msg("卸载命令引号未闭合"))?;
        let rest = &quoted[end + 1..];
        if !rest.is_empty() && !rest.starts_with(char::is_whitespace) {
            return Err(AppError::msg("卸载命令路径和参数未分隔"));
        }
        (&quoted[..end], rest)
    } else {
        value.split_once(char::is_whitespace).unwrap_or((value, ""))
    };
    if rest.contains(['"', '\'', '&', '|', '<', '>', '^', '%', '\0', '\r', '\n']) {
        return Err(AppError::msg("卸载参数包含不支持的语法"));
    }
    let path = checked_path(exe)?;
    if !path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("exe")) {
        return Err(AppError::msg("仅接受精确 EXE 入口，不执行 shell、脚本或 msiexec 猜测"));
    }
    Ok((path, rest.split_whitespace().map(str::to_owned).collect()))
}

fn valid_guid(value: &str) -> bool {
    let Some(value) = value.strip_prefix('{').and_then(|v| v.strip_suffix('}')) else { return false; };
    let parts: Vec<_> = value.split('-').collect();
    parts.len() == 5 && parts.iter().zip([8, 4, 4, 4, 12])
        .all(|(part, length)| part.len() == length && part.bytes().all(|c| c.is_ascii_hexdigit()))
}

fn file_sha256(path: &Path) -> Result<String> {
    let meta = fs::symlink_metadata(path)?;
    if !meta.is_file() || system::has_reparse_component(path) {
        return Err(AppError::msg("卸载器缺失、不是普通文件或被重定向"));
    }
    let mut file = File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 { break; }
        hash.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

fn validate_invocation(kind: EnvType, selected: &Path, record: &Registration) -> Result<Invocation> {
    let command = record.quiet_command.as_ref().or(record.uninstall_command.as_ref())
        .ok_or_else(|| AppError::msg("缺少卸载命令"))?;
    let (exe, args) = parse_command(command)?;
    let args = match kind {
        EnvType::Python => {
            let slot = record.bundle_provider.as_deref().and_then(python_slot)
                .ok_or_else(|| AppError::msg("缺少可识别的 CPython x64 Bundle provider"))?;
            let version = record.display_name.strip_prefix("Python ")
                .and_then(|name| name.strip_suffix(" (64-bit)"))
                .ok_or_else(|| AppError::msg("不是支持的 Python x64 Bundle 登记"))?;
            let version_parts: Vec<_> = version.split('.').collect();
            if version_parts.len() != 3 || version_parts.iter().any(|v| v.is_empty() || !v.bytes().all(|c| c.is_ascii_digit()))
                || !version.starts_with(&format!("{}.", slot.trim_end_matches("-64")))
                || record.publisher != "Python Software Foundation"
            {
                return Err(AppError::msg("Python 版本、发行方声明或 provider 不一致"));
            }
            let filename = format!("python-{version}-amd64.exe");
            let parent = exe.parent().ok_or_else(|| AppError::msg("卸载器路径缺少父目录"))?;
            if !exe.file_name().is_some_and(|name| name.eq_ignore_ascii_case(filename.as_str()))
                || !valid_guid(&record.key_name)
                || !parent.file_name().is_some_and(|name| name.eq_ignore_ascii_case(record.key_name.as_str()))
                || !parent.parent().and_then(Path::file_name).is_some_and(|name| name.eq_ignore_ascii_case("Package Cache"))
                || !args.iter().any(|arg| arg.eq_ignore_ascii_case("/uninstall"))
                || args.iter().any(|arg| !["/uninstall", "/quiet", "/passive", "/norestart"].iter().any(|allowed| arg.eq_ignore_ascii_case(allowed)))
            {
                return Err(AppError::msg("Python 缓存卸载器路径、Bundle 标识或参数不匹配"));
            }
            vec!["/uninstall".into(), "/quiet".into(), "/norestart".into()]
        }
        EnvType::Llvm => {
            if !exe.file_name().is_some_and(|name| name.eq_ignore_ascii_case("Uninstall.exe"))
                || exe.parent().is_none_or(|parent| system::path_key(parent) != system::path_key(selected))
                || args.iter().any(|arg| arg != "/S")
            {
                return Err(AppError::msg("LLVM 卸载入口必须是所选安装目录中的 Uninstall.exe，且只能携带 /S"));
            }
            vec!["/S".into()]
        }
        _ => return Err(AppError::msg("此类型不使用注册式卸载适配器")),
    };
    Ok(Invocation {
        registration_id: record.id.clone(), executable: exe.to_string_lossy().into_owned(),
        arguments: args, file_sha256: file_sha256(&exe)?,
    })
}

pub(crate) fn preview(kind: EnvType, selected: &Path, snapshot: RegistrySnapshot) -> Result<Plan> {
    let selected = checked_path(&selected.to_string_lossy())?;
    let selected = selected.canonicalize()?;
    let mut snapshot = snapshot;
    snapshot.records.retain(|record| relevant(kind, record));
    snapshot.records.sort_by(|a, b| a.id.cmp(&b.id));
    snapshot.errors.sort();
    let mut plan = Plan {
        env_type: kind, install_path: selected.to_string_lossy().into_owned(), state: "unmatched".into(),
        invocation: None, assessments: Vec::new(), errors: snapshot.errors.clone(),
        registry_fingerprint: digest(&serde_json::to_vec(&snapshot)?),
    };
    if !matches!(kind, EnvType::Python | EnvType::Llvm) {
        plan.state = "notApplicable".into();
        return Ok(plan);
    }
    let mut matched = 0;
    let mut eligible = Vec::new();
    for record in snapshot.records {
        let locations: Vec<_> = record.install_location.iter()
            .chain(if kind == EnvType::Python { record.python_install_path.iter() } else { None.iter() })
            .collect();
        let location = (|| -> Result<bool> {
            if locations.is_empty() {
                if kind == EnvType::Llvm {
                    // NSIS may omit InstallLocation; require the exact local uninstaller parent.
                    let command = record.quiet_command.as_ref().or(record.uninstall_command.as_ref())
                        .ok_or_else(|| AppError::msg("没有安装位置或卸载命令"))?;
                    let (exe, _) = parse_command(command)?;
                    return Ok(exe.file_name().is_some_and(|n| n.eq_ignore_ascii_case("Uninstall.exe"))
                        && exe.parent().is_some_and(|p| system::path_key(p) == system::path_key(&selected)));
                }
                return Err(AppError::msg("缺少精确安装路径；不按显示名称或版本猜测"));
            }
            let mut matches = Vec::new();
            for value in &locations { matches.push(exact_location(value, &selected)?); }
            if matches.iter().any(|yes| *yes) && matches.iter().any(|yes| !*yes) {
                return Err(AppError::msg("安装位置证据冲突"));
            }
            Ok(matches.iter().all(|yes| *yes))
        })();
        let (state, details) = match location {
            Ok(false) => ("otherInstallation", "登记指向其他安装，保留且不执行。".into()),
            Err(error) => ("unverified", error.to_string()),
            Ok(true) => {
                matched += 1;
                match validate_invocation(kind, &selected, &record) {
                    Ok(invocation) => {
                        eligible.push(invocation);
                        ("matched", "安装路径与入口精确匹配；文件哈希仅用于执行前变化检测，不证明发布者身份。".into())
                    }
                    Err(error) => ("unsupported", error.to_string()),
                }
            }
        };
        plan.assessments.push(Assessment { registration_id: record.id, state: state.into(), details });
    }
    if !plan.errors.is_empty() || plan.assessments.iter().any(|item| item.state == "unverified" || item.state == "unsupported") {
        plan.state = "unverified".into();
    } else if matched > 1 {
        plan.state = "ambiguous".into();
    } else if matched == 1 && eligible.len() == 1 {
        plan.state = "ready".into();
        plan.invocation = eligible.pop();
    }
    Ok(plan)
}

pub(crate) trait Backend {
    fn registry(&mut self) -> Result<RegistrySnapshot>;
    fn run(&mut self, invocation: &Invocation) -> Result<i32>;
    fn runtime_present(&mut self, kind: EnvType, root: &Path) -> Result<bool>;
}

/// Production adapter. Every destructive action is constrained to the exact
/// executable and argument vector produced by `preview`; no shell is involved.
pub(crate) struct WindowsBackend;

impl Backend for WindowsBackend {
    fn registry(&mut self) -> Result<RegistrySnapshot> {
        Ok(read_registry())
    }

    fn run(&mut self, invocation: &Invocation) -> Result<i32> {
        let executable = Path::new(&invocation.executable);
        if file_sha256(executable)? != invocation.file_sha256 {
            return Err(AppError::msg("卸载器文件哈希在启动前已变化，未执行命令"));
        }
        let mut command = std::process::Command::new(executable);
        command.args(&invocation.arguments);
        if let Some(parent) = executable.parent() {
            command.current_dir(parent);
        }
        let output = crate::reinstall_process::output_blocking(
            command,
            Duration::from_secs(15 * 60),
        )?;
        output.status.code().ok_or_else(|| AppError::msg("卸载命令异常结束，没有可用退出码"))
    }

    fn runtime_present(&mut self, _: EnvType, root: &Path) -> Result<bool> {
        match fs::symlink_metadata(root) {
            Ok(_) => Ok(true),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(error.into()),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Execution {
    pub status: String,
    pub command_exit_code: Option<i32>,
    pub registration_removed: Option<bool>,
    pub runtime_removed: Option<bool>,
    pub details: Vec<String>,
}

fn registration_absence(plan: &Plan, snapshot: RegistrySnapshot) -> Result<bool> {
    if !snapshot.errors.is_empty() {
        return Err(AppError::msg(snapshot.errors.join("\n")));
    }
    let selected = Path::new(&plan.install_path);
    for record in snapshot.records.iter().filter(|record| relevant(plan.env_type, record)) {
        if plan.invocation.as_ref().is_some_and(|call| call.registration_id == record.id) {
            return Ok(false);
        }
        let mut locations: Vec<_> = record.install_location.iter().collect();
        if plan.env_type == EnvType::Python { locations.extend(record.python_install_path.iter()); }
        if locations.is_empty() && plan.env_type == EnvType::Llvm {
            let command = record.quiet_command.as_ref().or(record.uninstall_command.as_ref())
                .ok_or_else(|| AppError::msg("卸载后发现无法定位的 LLVM 登记"))?;
            let (exe, _) = parse_command(command)?;
            if exe.parent().is_some_and(|parent| system::path_key(parent) == system::path_key(selected)) {
                return Ok(false);
            }
        } else if locations.is_empty() {
            return Err(AppError::msg("卸载后发现无法定位的 Python Bundle 登记"));
        } else {
            for location in locations {
                if exact_location(location, selected)? { return Ok(false); }
            }
        }
    }
    Ok(true)
}

/// Pure orchestration. Never removes leftover directories or registry keys itself.
pub(crate) fn execute(plan: &Plan, backend: &mut impl Backend) -> Result<Execution> {
    let root = Path::new(&plan.install_path);
    let fresh = preview(plan.env_type, root, backend.registry()?)?;
    if plan.state != "ready" || &fresh != plan {
        return Err(AppError::msg("原厂卸载预览已变化、存在冲突或未准备好；未执行命令"));
    }
    let invocation = fresh.invocation.as_ref().ok_or_else(|| AppError::msg("缺少核验后的卸载入口"))?;
    let command = backend.run(invocation);
    let mut result = Execution {
        status: "partial".into(), command_exit_code: command.as_ref().ok().copied(),
        registration_removed: None, runtime_removed: None, details: Vec::new(),
    };
    match command {
        Ok(code) if code != 0 => result.details.push(format!("卸载命令退出码 {code}；仍需复查，不能据此确认完成。")),
        Err(error) => result.details.push(format!("卸载命令失败，可能已部分执行：{error}")),
        _ => {}
    }
    match backend.registry() {
        Ok(snapshot) => match registration_absence(plan, snapshot) {
            Ok(absent) => result.registration_removed = Some(absent),
            Err(error) => result.details.push(format!("卸载登记复查不完整：{error}")),
        },
        Err(error) => result.details.push(format!("无法复查卸载登记：{error}")),
    }
    match backend.runtime_present(plan.env_type, root) {
        Ok(present) => result.runtime_removed = Some(!present),
        Err(error) => result.details.push(format!("无法复查运行时文件：{error}")),
    }
    if result.command_exit_code == Some(0)
        && result.registration_removed == Some(true) && result.runtime_removed == Some(true)
    {
        result.status = "verifiedInstallerScopeRemoved".into();
    } else {
        result.details.push("原厂卸载未能完整核验；保留剩余目录和共享数据，不继续递归删除或清理 PATH。".into());
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(kind: EnvType) -> (crate::test_support::TestDir, PathBuf, RegistrySnapshot) {
        let dir = crate::test_support::TestDir::new();
        let selected = dir.path().join("managed/selected");
        fs::create_dir_all(&selected).unwrap();
        let guid = "{01234567-89ab-cdef-0123-456789abcdef}";
        let exe = if kind == EnvType::Python {
            dir.path().join("Package Cache").join(guid).join("python-3.14.0-amd64.exe")
        } else { selected.join("Uninstall.exe") };
        fs::create_dir_all(exe.parent().unwrap()).unwrap();
        fs::write(&exe, b"fixture executable bytes; never executed").unwrap();
        let record = Registration {
            id: format!("HKCU/64/{guid}"), key_name: guid.into(),
            display_name: if kind == EnvType::Python { "Python 3.14.0 (64-bit)" } else { "LLVM" }.into(),
            publisher: if kind == EnvType::Python { "Python Software Foundation" } else { "LLVM Project" }.into(),
            install_location: Some(selected.to_string_lossy().into_owned()),
            uninstall_command: Some(format!("\"{}\"{}", exe.display(), if kind == EnvType::Python { " /uninstall" } else { "" })),
            quiet_command: None,
            bundle_provider: if kind == EnvType::Python { Some("CPython-3.14".into()) } else { None },
            python_install_path: if kind == EnvType::Python { Some(selected.to_string_lossy().into_owned()) } else { None },
        };
        (dir, selected, RegistrySnapshot { records: vec![record], errors: vec![] })
    }

    #[test]
    fn exact_path_match_preserves_other_versions_and_normalizes_case() {
        for kind in [EnvType::Python, EnvType::Llvm] {
            let (_dir, root, mut snapshot) = fixture(kind);
            snapshot.records[0].install_location = Some(format!("{}\\", root.to_string_lossy().to_uppercase()));
            let mut other = snapshot.records[0].clone();
            other.id = "HKCU/64/other".into();
            other.install_location = Some(root.with_file_name("selected-other").to_string_lossy().into_owned());
            other.python_install_path = other.install_location.clone();
            snapshot.records.push(other);
            let plan = preview(kind, &root, snapshot).unwrap();
            assert_eq!(plan.state, "ready");
            assert_eq!(plan.assessments.iter().filter(|a| a.state == "matched").count(), 1);
            assert_eq!(plan.assessments.iter().filter(|a| a.state == "otherInstallation").count(), 1);
        }
    }

    #[test]
    fn python_requires_provider_version_path_consistency_and_exact_bundle_cache() {
        for mode in ["conflict", "provider", "version", "publisher", "guid", "missing", "arguments"] {
            let (_dir, root, mut snapshot) = fixture(EnvType::Python);
            let record = &mut snapshot.records[0];
            match mode {
                "conflict" => record.python_install_path = Some(root.with_file_name("other").to_string_lossy().into_owned()),
                "provider" => record.bundle_provider = Some("CPython-3.14-32".into()),
                "version" => record.display_name = "Python 3.13.0 (64-bit)".into(),
                "publisher" => record.publisher = "unrelated".into(),
                "guid" => record.key_name = "{aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee}".into(),
                "missing" => { record.install_location = None; record.python_install_path = None; }
                _ => record.uninstall_command.as_mut().unwrap().push_str(" InstallAllUsers=1"),
            }
            assert_ne!(preview(EnvType::Python, &root, snapshot).unwrap().state, "ready", "{mode}");
        }
        let (_dir, root, mut snapshot) = fixture(EnvType::Python);
        snapshot.records[0].install_location = None;
        assert_eq!(preview(EnvType::Python, &root, snapshot).unwrap().state, "ready");
    }

    #[test]
    fn llvm_parent_fallback_is_exact_and_does_not_accept_shell_commands() {
        let (_dir, root, mut snapshot) = fixture(EnvType::Llvm);
        snapshot.records[0].install_location = None;
        assert_eq!(preview(EnvType::Llvm, &root, snapshot.clone()).unwrap().state, "ready");
        snapshot.records[0].uninstall_command = Some(format!("\"{}\" /S & echo bad", root.join("Uninstall.exe").display()));
        assert_ne!(preview(EnvType::Llvm, &root, snapshot).unwrap().state, "ready");
        for command in [r"cmd.exe /c uninstall", r"msiexec.exe /x {guid}", r#""C:\x.exe"/S"#, r#""C:\x.exe" "_?=C:\elsewhere""#] {
            assert!(parse_command(command).is_err(), "{command}");
        }
    }

    #[test]
    fn unrelated_msi_components_are_not_bundle_candidates_but_conflicting_bundles_block() {
        let (_dir, root, mut snapshot) = fixture(EnvType::Python);
        let mut component = snapshot.records[0].clone();
        component.id = "HKCU/64/msi-component".into();
        component.display_name = "Python 3.14.0 Core Interpreter (64-bit)".into();
        component.bundle_provider = None;
        component.install_location = None;
        component.python_install_path = None;
        component.uninstall_command = Some("MsiExec.exe /I{guid}".into());
        snapshot.records.push(component);
        assert_eq!(preview(EnvType::Python, &root, snapshot.clone()).unwrap().state, "ready");
        let mut conflict = snapshot.records[0].clone();
        conflict.id = "HKLM/64/conflicting-bundle".into();
        conflict.python_install_path = Some(root.with_file_name("different").to_string_lossy().into_owned());
        snapshot.records.push(conflict);
        let plan = preview(EnvType::Python, &root, snapshot).unwrap();
        assert_eq!(plan.state, "unverified");
        assert!(plan.invocation.is_none());
    }

    #[test]
    fn duplicate_registration_or_incomplete_enumeration_never_selects_an_arbitrary_uninstaller() {
        let (_dir, root, snapshot) = fixture(EnvType::Llvm);
        let mut duplicate = snapshot.clone();
        duplicate.records.push(duplicate.records[0].clone());
        duplicate.records[1].id = "HKLM/32/same".into();
        let plan = preview(EnvType::Llvm, &root, duplicate).unwrap();
        assert_eq!(plan.state, "ambiguous");
        assert!(plan.invocation.is_none());
        let mut inaccessible = snapshot;
        inaccessible.errors.push("fixture HKLM access denied".into());
        let plan = preview(EnvType::Llvm, &root, inaccessible).unwrap();
        assert_eq!(plan.state, "unverified");
        assert!(plan.invocation.is_none());
    }

    #[test]
    fn redirected_locations_and_executables_are_not_matched() {
        let (dir, root, mut snapshot) = fixture(EnvType::Python);
        let alias = dir.path().join("alias");
        junction::create(&root, &alias).unwrap();
        snapshot.records[0].install_location = Some(alias.to_string_lossy().into_owned());
        assert_ne!(preview(EnvType::Python, &root, snapshot.clone()).unwrap().state, "ready");
        assert!(preview(EnvType::Python, &alias, snapshot.clone()).is_err());
        snapshot.records[0].install_location = Some(root.to_string_lossy().into_owned());
        let cache = dir.path().join("Package Cache");
        let moved = dir.path().join("cache-target");
        fs::rename(&cache, &moved).unwrap();
        junction::create(&moved, &cache).unwrap();
        assert_ne!(preview(EnvType::Python, &root, snapshot).unwrap().state, "ready");
    }

    struct FixtureBackend {
        before: RegistrySnapshot,
        after: RegistrySnapshot,
        reads: usize,
        runs: usize,
        exit: i32,
        runtime: bool,
        fail_run: bool,
        fail_registry: bool,
        fail_files: bool,
    }
    impl Backend for FixtureBackend {
        fn registry(&mut self) -> Result<RegistrySnapshot> {
            self.reads += 1;
            if self.reads == 1 { return Ok(self.before.clone()); }
            if self.fail_registry { return Err(AppError::msg("fixture post-read denied")); }
            Ok(self.after.clone())
        }
        fn run(&mut self, _: &Invocation) -> Result<i32> {
            self.runs += 1;
            if self.fail_run { Err(AppError::msg("fixture start/interruption failure")) } else { Ok(self.exit) }
        }
        fn runtime_present(&mut self, _: EnvType, _: &Path) -> Result<bool> {
            if self.fail_files { Err(AppError::msg("fixture filesystem denied")) } else { Ok(self.runtime) }
        }
    }
    fn backend(before: RegistrySnapshot) -> FixtureBackend {
        FixtureBackend { before, after: RegistrySnapshot::default(), reads: 0, runs: 0,
            exit: 0, runtime: false, fail_run: false, fail_registry: false, fail_files: false }
    }

    #[test]
    fn execution_rechecks_plan_and_binary_before_calling_any_runner() {
        for change in ["registry", "binary", "permissions"] {
            let (_dir, root, snapshot) = fixture(EnvType::Llvm);
            let plan = preview(EnvType::Llvm, &root, snapshot.clone()).unwrap();
            let mut backend = backend(snapshot);
            match change {
                "registry" => backend.before.records[0].uninstall_command.as_mut().unwrap().push_str(" /S"),
                "binary" => fs::write(root.join("Uninstall.exe"), b"changed").unwrap(),
                _ => backend.before.errors.push("fixture denied".into()),
            }
            assert!(execute(&plan, &mut backend).is_err(), "{change}");
            assert_eq!(backend.runs, 0);
            assert!(root.join("Uninstall.exe").exists());
        }
    }

    #[test]
    fn postcheck_detects_registration_reappearing_under_a_different_identity() {
        let (_dir, root, snapshot) = fixture(EnvType::Llvm);
        let plan = preview(EnvType::Llvm, &root, snapshot.clone()).unwrap();
        let mut backend = backend(snapshot.clone());
        backend.after = snapshot;
        backend.after.records[0].id = "HKLM/64/replacement".into();
        let result = execute(&plan, &mut backend).unwrap();
        assert_eq!(result.status, "partial");
        assert_eq!(result.registration_removed, Some(false));
        assert_eq!(result.runtime_removed, Some(true));
    }

    #[test]
    fn successful_exit_is_insufficient_without_both_registration_and_runtime_removal() {
        for mode in ["success", "registration", "runtime", "registry-failure", "files-failure", "command-failure", "restart", "start-failure"] {
            let (_dir, root, snapshot) = fixture(EnvType::Python);
            let plan = preview(EnvType::Python, &root, snapshot.clone()).unwrap();
            let mut backend = backend(snapshot.clone());
            match mode {
                "registration" => backend.after = snapshot,
                "runtime" => backend.runtime = true,
                "registry-failure" => backend.fail_registry = true,
                "files-failure" => backend.fail_files = true,
                "command-failure" => backend.exit = 1,
                "restart" => backend.exit = 3010,
                "start-failure" => backend.fail_run = true,
                _ => {}
            }
            let result = execute(&plan, &mut backend).unwrap();
            assert_eq!(backend.runs, 1);
            assert_eq!(result.status == "verifiedInstallerScopeRemoved", mode == "success", "{mode}");
            assert!(root.exists()); // The orchestrator never recursively removes leftovers.
        }
    }
}
