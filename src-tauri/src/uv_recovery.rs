use crate::{
    error::{AppError, Result},
    package_managers::{inspect_uv_layout, inspect_uv_tool, UvLegacyLayout, UvToolLayout},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    future::Future,
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::process::Command;

/// uv 恢复条目的终态。
///
/// 用枚举而不是 `String`：取值集合是封闭的，编译器应当阻止出现第四种。
/// `retained` 刻意不在这里——保留项从未被尝试（预览阶段就判定为不可自动
/// 恢复），记在报告顶层的 `retained` 里；把它混进条目会让「压根没试」与
/// 「试了但失败」在界面上看起来一样。
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum UvRecoveryStatus {
    /// 入口已重建，旧入口已清理。
    Recovered,
    /// 入口已重建，但旧入口未能全部清理；`detail` 说明残留。
    RecoveredWithResidue,
    /// 恢复失败，`detail` 说明原因。
    Failed,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UvRecoveryItem {
    pub name: String,
    pub version: String,
    pub status: UvRecoveryStatus,
    pub detail: Option<String>,
    pub removed_entrypoints: Vec<String>,
    pub new_entrypoints: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UvRecoveryReport {
    /// `complete`, `partial` or `failed`.
    pub status: String,
    pub items: Vec<UvRecoveryItem>,
    pub retained: Vec<String>,
    pub layout: UvLegacyLayout,
}

fn launcher(value: &str) -> Result<PathBuf> {
    let path = Path::new(value);
    if !path.is_absolute()
        || !path.is_file()
        || !path
            .file_stem()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.eq_ignore_ascii_case("uv"))
        || !path.extension().is_some_and(|extension| {
            ["exe", "cmd", "bat"]
                .iter()
                .any(|value| extension.eq_ignore_ascii_case(value))
        })
    {
        return Err(AppError::msg(
            "请选择名为 uv 的 .exe/.cmd/.bat 启动器绝对路径",
        ));
    }
    Ok(crate::fsutil::plain_path(path.canonicalize()?))
}

fn path_present(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok()
}

fn file_sha256(path: &Path) -> Option<String> {
    use sha2::{Digest, Sha256};
    let bytes = std::fs::read(path).ok()?;
    Some(format!("{:x}", Sha256::digest(bytes)))
}

fn require_real_directory(path: &Path, label: &str) -> Result<()> {
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_dir() || crate::detect::system::is_reparse_path(path) {
        return Err(AppError::msg(format!(
            "{label}必须是普通目录，不能是链接/重解析点: {}",
            path.display()
        )));
    }
    Ok(())
}

fn directory_empty(path: &Path) -> Result<bool> {
    Ok(std::fs::read_dir(path)?.next().is_none())
}

async fn run_uv_command(
    executable: PathBuf,
    arguments: Vec<String>,
    environment: BTreeMap<String, String>,
) -> Result<String> {
    let mut command = Command::new(&executable);
    command
        .args(&arguments)
        .current_dir(std::env::temp_dir())
        .creation_flags(0x08000000)
        .kill_on_drop(true);
    for (key, _) in std::env::vars_os() {
        let name = key.to_string_lossy().to_ascii_uppercase();
        if name.starts_with("UV_")
            || name.starts_with("PIP_")
            || name.starts_with("PYTHON")
            || name == "VIRTUAL_ENV"
        {
            command.env_remove(key);
        }
    }
    command.envs(environment).env("NO_COLOR", "1");
    let mut paths = vec![executable
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf()];
    paths.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    if let Ok(path) = std::env::join_paths(paths) {
        command.env("PATH", path);
    }
    let output = crate::reinstall_process::output(command, Duration::from_secs(600)).await?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let detail = if stderr.trim().is_empty() {
            stdout.trim()
        } else {
            stderr.trim()
        };
        return Err(AppError::msg(format!(
            "uv 命令退出码 {}: {}",
            output.status.code().unwrap_or(-1),
            detail.chars().take(2000).collect::<String>()
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

/// 恢复单个工具时用到的固定路径集合。
///
/// 这几个参数类型完全相同（都是 `&Path`），逐个传参时把 legacy 与 proposed
/// 传反不会编译失败，只会在运行时动错目录。收进结构体后调用点按字段名书写，
/// 传错会变成字段名写错，读代码时也能直接看出用的是哪一组。
struct RecoveryPaths<'a> {
    root: &'a Path,
    legacy_bin: &'a Path,
    legacy_cache: &'a Path,
    proposed_bin: &'a Path,
    proposed_cache: &'a Path,
    executable: &'a Path,
}

async fn recover_tool<F, Fut>(
    tool: &UvToolLayout,
    paths: &RecoveryPaths<'_>,
    run: &F,
) -> Result<UvRecoveryItem>
where
    F: Fn(PathBuf, Vec<String>, BTreeMap<String, String>) -> Fut,
    Fut: Future<Output = Result<String>>,
{
    let RecoveryPaths {
        root,
        legacy_bin,
        legacy_cache,
        proposed_bin,
        proposed_cache,
        executable,
    } = *paths;
    let directory = PathBuf::from(&tool.directory);
    let before = inspect_uv_tool(&directory, &tool.name, legacy_bin, proposed_bin);
    if &before != tool {
        return Err(AppError::msg("工具目录、receipt 或旧入口在预览后发生变化"));
    }
    let requirement = tool
        .requirement
        .clone()
        .ok_or_else(|| AppError::msg("安装记录没有可还原的精确版本要求"))?;
    for snapshot in tool
        .entrypoint_snapshots
        .iter()
        .filter(|entry| entry.legacy)
    {
        let path = Path::new(&snapshot.path);
        if !crate::detect::system::path_within(legacy_bin, path)
            || crate::detect::system::paths_eq(path, legacy_bin)
            || crate::detect::system::is_reparse_path(path)
        {
            return Err(AppError::msg(format!(
                "旧入口路径无法确认归属: {}",
                snapshot.path
            )));
        }
        if !path.is_file() {
            return Err(AppError::msg(format!(
                "旧入口已不存在: {}",
                snapshot.path
            )));
        }
        if file_sha256(path) != snapshot.sha256 {
            return Err(AppError::msg(format!(
                "旧入口内容在预览后发生变化: {}",
                snapshot.path
            )));
        }
    }

    let arguments = vec![
        "tool".into(),
        "install".into(),
        requirement.clone(),
        "--force".into(),
    ];
    let mut environment = BTreeMap::new();
    environment.insert(
        "UV_TOOL_DIR".into(),
        root.to_string_lossy().into_owned(),
    );
    environment.insert(
        "UV_TOOL_BIN_DIR".into(),
        proposed_bin.to_string_lossy().into_owned(),
    );
    environment.insert(
        "UV_CACHE_DIR".into(),
        if path_present(legacy_cache) {
            legacy_cache.to_string_lossy().into_owned()
        } else {
            proposed_cache.to_string_lossy().into_owned()
        },
    );
    environment.insert("UV_NO_CONFIG".into(), "1".into());
    run(executable.to_path_buf(), arguments, environment).await?;

    let after = inspect_uv_tool(&directory, &tool.name, legacy_bin, proposed_bin);
    if after.receipt_status != "valid" {
        return Err(AppError::msg(format!(
            "恢复后安装记录无效: {}",
            after.receipt_status
        )));
    }
    if after.needs_recovery {
        return Err(AppError::msg("恢复后登录器仍指向旧入口目录"));
    }
    if after.version != tool.version {
        return Err(AppError::msg(format!(
            "恢复后版本不一致: 预期 {:?}，实际 {:?}",
            tool.version, after.version
        )));
    }
    if after.entrypoints.is_empty() {
        return Err(AppError::msg("恢复后没有登录器记录"));
    }
    for entry in &after.entrypoint_snapshots {
        let path = Path::new(&entry.path);
        if !crate::detect::system::path_within(proposed_bin, path)
            || crate::detect::system::paths_eq(path, proposed_bin)
            || crate::detect::system::is_reparse_path(path)
            || !path.is_file()
        {
            return Err(AppError::msg(format!(
                "恢复后登录器不在声明的入口目录或无法执行: {}",
                entry.path
            )));
        }
    }

    let mut removed = Vec::new();
    let mut residue = Vec::new();
    for snapshot in tool
        .entrypoint_snapshots
        .iter()
        .filter(|entry| entry.legacy)
    {
        let path = Path::new(&snapshot.path);
        if file_sha256(path) != snapshot.sha256 {
            residue.push(format!("{}（内容已变化）", snapshot.path));
            continue;
        }
        match std::fs::remove_file(path) {
            Ok(()) => removed.push(snapshot.path.clone()),
            Err(error) => residue.push(format!("{}（{error}）", snapshot.path)),
        }
    }
    Ok(UvRecoveryItem {
        name: tool.name.clone(),
        version: tool.version.clone().unwrap_or_default(),
        status: if residue.is_empty() {
            UvRecoveryStatus::Recovered
        } else {
            UvRecoveryStatus::RecoveredWithResidue
        },
        detail: (!residue.is_empty()).then(|| format!("旧入口未能清理: {}", residue.join("；"))),
        removed_entrypoints: removed,
        new_entrypoints: after.entrypoints,
    })
}

/// Recover uv's pre-sibling layout using an injected command runner. The
/// caller must pass the exact read-only preview that the user confirmed.
pub async fn recover_uv_layout_with<F, Fut>(
    expected: UvLegacyLayout,
    executable: String,
    run: F,
) -> Result<UvRecoveryReport>
where
    F: Fn(PathBuf, Vec<String>, BTreeMap<String, String>) -> Fut,
    Fut: Future<Output = Result<String>>,
{
    let executable = launcher(&executable)?;
    let root = PathBuf::from(&expected.tool_root);
    let fresh = inspect_uv_layout(&root)?;
    if fresh != expected {
        return Err(AppError::msg(
            "uv 布局、receipt 或旧入口在预览后发生变化，请重新预览",
        ));
    }
    if fresh.status == "clean" {
        return Err(AppError::msg("当前 uv 布局无需恢复"));
    }
    if !fresh.issues.is_empty() {
        return Err(AppError::msg(format!(
            "当前布局存在必须先处理的冲突: {}",
            fresh.issues.join("；")
        )));
    }
    let legacy_bin = PathBuf::from(&fresh.legacy_bin);
    let legacy_cache = PathBuf::from(&fresh.legacy_cache);
    let proposed_bin = PathBuf::from(&fresh.proposed_bin);
    let proposed_cache = PathBuf::from(&fresh.proposed_cache);
    if path_present(&proposed_bin) || path_present(&proposed_cache) {
        return Err(AppError::msg(
            "建议的入口或缓存目录已存在，恢复前必须核对归属",
        ));
    }
    if path_present(&legacy_bin) {
        require_real_directory(&legacy_bin, "旧入口目录")?;
    }
    if path_present(&legacy_cache) {
        require_real_directory(&legacy_cache, "旧缓存目录")?;
    }

    let recoverable: Vec<_> = fresh
        .tools
        .iter()
        .filter(|tool| tool.needs_recovery && tool.recoverable)
        .cloned()
        .collect();
    if recoverable.is_empty() {
        return Err(AppError::msg(
            "没有可自动恢复的 uv 工具；请按预览中的问题人工处理",
        ));
    }

    let paths = RecoveryPaths {
        root: &root,
        legacy_bin: &legacy_bin,
        legacy_cache: &legacy_cache,
        proposed_bin: &proposed_bin,
        proposed_cache: &proposed_cache,
        executable: &executable,
    };
    let mut items = Vec::new();
    for tool in &recoverable {
        match recover_tool(tool, &paths, &run).await {
            Ok(item) => items.push(item),
            Err(error) => items.push(UvRecoveryItem {
                name: tool.name.clone(),
                version: tool.version.clone().unwrap_or_default(),
                status: UvRecoveryStatus::Failed,
                detail: Some(error.to_string()),
                removed_entrypoints: vec![],
                new_entrypoints: vec![],
            }),
        }
    }

    let mut retained = Vec::new();
    for tool in fresh
        .tools
        .iter()
        .filter(|tool| tool.needs_recovery && !tool.recoverable)
    {
        retained.push(format!(
            "{}：{}",
            tool.name,
            if tool.issues.is_empty() {
                "需人工核对安装记录".into()
            } else {
                tool.issues.join("；")
            }
        ));
    }
    let recovered_all = items
        .iter()
        .all(|item| {
            matches!(
                item.status,
                UvRecoveryStatus::Recovered | UvRecoveryStatus::RecoveredWithResidue
            )
        })
        && retained.is_empty();
    let pending_legacy = inspect_uv_layout(&root)?
        .tools
        .iter()
        .any(|tool| tool.needs_recovery);

    // Only move or remove the shared legacy cache after every recovery that
    // required it succeeded and no blocked tool still depends on the old root.
    if recovered_all && !pending_legacy {
        if path_present(&legacy_cache) {
            if directory_empty(&legacy_cache)? {
                if let Err(error) = std::fs::remove_dir(&legacy_cache) {
                    retained.push(format!(
                        "旧缓存目录无法删除 {}: {error}",
                        legacy_cache.display()
                    ));
                }
            } else if path_present(&proposed_cache) {
                retained.push(format!(
                    "旧缓存保留在 {}：建议缓存目录已存在",
                    legacy_cache.display()
                ));
            } else {
                match std::fs::rename(&legacy_cache, &proposed_cache) {
                    Ok(()) => {}
                    Err(error) => retained.push(format!(
                        "旧缓存保留在 {}：迁移失败 {error}",
                        legacy_cache.display()
                    )),
                }
            }
        }
        if path_present(&legacy_bin) && directory_empty(&legacy_bin)? {
            if let Err(error) = std::fs::remove_dir(&legacy_bin) {
                retained.push(format!(
                    "空旧入口目录无法删除 {}: {error}",
                    legacy_bin.display()
                ));
            }
        } else if path_present(&legacy_bin) {
            retained.push(format!(
                "旧入口目录仍含未处理文件: {}",
                legacy_bin.display()
            ));
        }
    } else {
        if path_present(&legacy_cache) {
            retained.push(format!(
                "旧缓存目录保留，等待所有工具恢复后再迁移: {}",
                legacy_cache.display()
            ));
        }
        if path_present(&legacy_bin) {
            retained.push(format!(
                "旧入口目录保留，供未恢复工具继续使用: {}",
                legacy_bin.display()
            ));
        }
    }

    let layout = inspect_uv_layout(&root)?;
    let failed = items
        .iter()
        .any(|item| matches!(item.status, UvRecoveryStatus::Failed));
    let status = if layout.status == "clean" && retained.is_empty() && !failed {
        "complete"
    } else if items.iter().any(|item| {
        matches!(
            item.status,
            UvRecoveryStatus::Recovered | UvRecoveryStatus::RecoveredWithResidue
        )
    }) {
        "partial"
    } else {
        "failed"
    };
    Ok(UvRecoveryReport {
        status: status.into(),
        items,
        retained,
        layout,
    })
}

pub async fn recover_uv_layout(
    expected: UvLegacyLayout,
    executable: String,
) -> Result<UvRecoveryReport> {
    recover_uv_layout_with(expected, executable, run_uv_command).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    fn write(path: &Path, content: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }

    fn legacy_tool(root: &Path, name: &str, version: &str) -> (PathBuf, PathBuf) {
        let entry = root.join("bin").join(format!("{name}.cmd"));
        write(&entry, "@echo off\r\nexit /b 0\r\n");
        let receipt = format!(
            "[tool]\nrequirements = [{{ name = \"{name}\", specifier = \"=={version}\" }}]\n\
             [[tool.entrypoints]]\nname = \"{name}\"\ninstall-path = \"{}\"\n",
            entry.to_string_lossy().replace('\\', "/")
        );
        write(&root.join(name).join("uv-receipt.toml"), &receipt);
        write(
            &root.join(name).join("Lib/site-packages").join(format!(
                "{}-{version}.dist-info/METADATA",
                name.replace('-', "_")
            )),
            &format!("Metadata-Version: 2.1\nName: {name}\nVersion: {version}\n"),
        );
        (root.join(name), entry)
    }

    fn fake_launcher(root: &Path) -> String {
        let path = root.join("uv.cmd");
        write(&path, "@echo off\r\nexit /b 0\r\n");
        path.to_string_lossy().into_owned()
    }

    #[test]
    fn recovery_accepts_only_a_uv_launcher_name() {
        let directory = crate::test_support::TestDir::new();
        let evil = directory.path().join("other.exe");
        write(&evil, "");
        assert!(launcher(&evil.to_string_lossy()).is_err());
        let uv = directory.path().join("uv.exe");
        write(&uv, "");
        assert!(launcher(&uv.to_string_lossy()).is_ok());
    }

    #[tokio::test]
    async fn recovers_entrypoints_from_exact_preview_and_moves_cache() {
        let directory = crate::test_support::TestDir::new();
        let root = directory.path().join("globals/uv-tools");
        let (_tool_dir, legacy_entry) = legacy_tool(&root, "alpha", "1.2.3");
        write(&root.join("cache/blob"), "cache fixture");
        let expected = inspect_uv_layout(&root).unwrap();
        assert_eq!(expected.status, "legacy");
        let executable = fake_launcher(directory.path());
        let proposed_bin = PathBuf::from(&expected.proposed_bin);
        let proposed_cache = PathBuf::from(&expected.proposed_cache);
        let calls = Arc::new(AtomicUsize::new(0));
        let counter = calls.clone();
        let bin = proposed_bin.clone();
        let tool_root = root.clone();
        let report = recover_uv_layout_with(
            expected.clone(),
            executable,
            move |_exe, arguments, environment| {
                let bin = bin.clone();
                let tool_root = tool_root.clone();
                let counter = counter.clone();
                async move {
                    counter.fetch_add(1, Ordering::SeqCst);
                    assert_eq!(
                        arguments,
                        ["tool", "install", "alpha==1.2.3", "--force"]
                            .map(str::to_owned)
                            .to_vec()
                    );
                    assert_eq!(
                        environment.get("UV_TOOL_DIR").unwrap(),
                        &tool_root.to_string_lossy().to_string()
                    );
                    assert_eq!(
                        environment.get("UV_TOOL_BIN_DIR").unwrap(),
                        &bin.to_string_lossy().to_string()
                    );
                    let entry = bin.join("alpha.exe");
                    write(&entry, "new launcher");
                    let receipt = format!(
                        "[tool]\nrequirements = [{{ name = \"alpha\", specifier = \"==1.2.3\" }}]\n\
                         [[tool.entrypoints]]\nname = \"alpha\"\ninstall-path = \"{}\"\n",
                        entry.to_string_lossy().replace('\\', "/")
                    );
                    write(&tool_root.join("alpha/uv-receipt.toml"), &receipt);
                    Ok(String::new())
                }
            },
        )
        .await
        .unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(report.status, "complete", "{report:?}");
        assert_eq!(report.items[0].status, UvRecoveryStatus::Recovered);
        assert!(!legacy_entry.exists());
        assert!(proposed_bin.join("alpha.exe").is_file());
        assert!(proposed_cache.join("blob").is_file());
        assert!(!root.join("bin").exists());
        assert!(!root.join("cache").exists());
        assert_eq!(report.layout.status, "clean");
    }

    #[tokio::test]
    async fn fails_before_mutation_when_receipt_changed_after_preview() {
        let directory = crate::test_support::TestDir::new();
        let root = directory.path().join("globals/uv-tools");
        let (tool_dir, legacy_entry) = legacy_tool(&root, "alpha", "1.2.3");
        let expected = inspect_uv_layout(&root).unwrap();
        let executable = fake_launcher(directory.path());
        std::fs::write(
            tool_dir.join("uv-receipt.toml"),
            "[tool]\nrequirements = [{ name = \"alpha\", specifier = \"==9.9.9\" }]\n",
        )
        .unwrap();
        let result = recover_uv_layout_with(
            expected,
            executable,
            |_exe, _arguments, _environment| async {
                panic!("runner must not be called for a stale preview")
            },
        )
        .await;
        assert!(result.is_err());
        assert!(legacy_entry.is_file());
        assert!(!root.join("alpha/.recovered").exists());
    }

    #[tokio::test]
    async fn retains_old_entrypoint_when_recovery_command_fails() {
        let directory = crate::test_support::TestDir::new();
        let root = directory.path().join("globals/uv-tools");
        let (_tool_dir, legacy_entry) = legacy_tool(&root, "alpha", "1.2.3");
        let expected = inspect_uv_layout(&root).unwrap();
        let executable = fake_launcher(directory.path());
        let report = recover_uv_layout_with(
            expected,
            executable,
            |_exe, _arguments, _environment| async {
                Err(AppError::msg("network unavailable"))
            },
        )
        .await
        .unwrap();
        assert_eq!(report.status, "failed");
        assert_eq!(report.items[0].status, UvRecoveryStatus::Failed);
        assert!(legacy_entry.is_file());
        assert!(!root.join("bin").read_dir().unwrap().next().is_none());
    }

    #[tokio::test]
    async fn partial_recovery_keeps_blocked_tool_and_shared_cache() {
        let directory = crate::test_support::TestDir::new();
        let root = directory.path().join("globals/uv-tools");
        let (_good_dir, good_entry) = legacy_tool(&root, "alpha", "1.2.3");
        let (_bad_dir, bad_entry) = legacy_tool(&root, "beta", "2.0.0");
        // Remove the pinned specifier so beta is visible but not automatically recoverable.
        write(
            &root.join("beta/uv-receipt.toml"),
            &format!(
                "[tool]\nrequirements = [{{ name = \"beta\" }}]\n\
                 [[tool.entrypoints]]\nname = \"beta\"\ninstall-path = \"{}\"\n",
                bad_entry.to_string_lossy().replace('\\', "/")
            ),
        );
        write(&root.join("cache/blob"), "shared cache");
        let expected = inspect_uv_layout(&root).unwrap();
        assert_eq!(expected.status, "legacy-partial");
        let executable = fake_launcher(directory.path());
        let proposed_bin = PathBuf::from(&expected.proposed_bin);
        let tool_root = root.clone();
        let report = recover_uv_layout_with(
            expected,
            executable,
            move |_exe, arguments, _environment| {
                let bin = proposed_bin.clone();
                let tool_root = tool_root.clone();
                async move {
                    assert_eq!(arguments[2], "alpha==1.2.3");
                    let entry = bin.join("alpha.exe");
                    write(&entry, "new launcher");
                    write(
                        &tool_root.join("alpha/uv-receipt.toml"),
                        &format!(
                            "[tool]\nrequirements = [{{ name = \"alpha\", specifier = \"==1.2.3\" }}]\n\
                             [[tool.entrypoints]]\nname = \"alpha\"\ninstall-path = \"{}\"\n",
                            entry.to_string_lossy().replace('\\', "/")
                        ),
                    );
                    Ok(String::new())
                }
            },
        )
        .await
        .unwrap();
        assert_eq!(report.status, "partial");
        assert_eq!(report.items[0].status, UvRecoveryStatus::Recovered);
        assert!(!good_entry.exists());
        assert!(bad_entry.is_file());
        assert!(root.join("cache/blob").is_file());
        assert!(report.retained.iter().any(|value| value.contains("beta")));
        assert!(report
            .retained
            .iter()
            .any(|value| value.contains("旧缓存目录保留")));

        // `retained` 不是条目状态：beta 在预览阶段就判定为不可自动恢复，
        // 从未被尝试，因此它只能出现在 `retained` 里，不能混进 `items`——
        // 否则「尝试过并失败」与「压根没尝试」在 UI 上会看起来一样。
        assert!(
            report.items.iter().all(|item| item.name != "beta"),
            "未被尝试的工具不能出现在条目里: {:?}",
            report.items
        );
        // 枚举化后取值集合由类型系统保证；这里改为锁定**序列化字符串**，
        // 那才是前端真正依赖的契约（kebab-case：`recovered` /
        // `recovered-with-residue` / `failed`）。
        for (status, expected) in [
            (UvRecoveryStatus::Recovered, "recovered"),
            (
                UvRecoveryStatus::RecoveredWithResidue,
                "recovered-with-residue",
            ),
            (UvRecoveryStatus::Failed, "failed"),
        ] {
            assert_eq!(
                serde_json::to_value(status).unwrap(),
                serde_json::Value::String(expected.into()),
                "{status:?} 的序列化字符串变化会直接破坏前端渲染"
            );
        }
    }

}
