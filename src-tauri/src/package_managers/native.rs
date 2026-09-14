use crate::{
    error::{AppError, Result},
    package_managers::Package,
};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

pub fn global_env(tool: &str) -> Option<&'static str> {
    match tool {
        "bun" => Some("BUN_INSTALL_GLOBAL_DIR"),
        "uv" => Some("UV_TOOL_DIR"),
        "pipx" => Some("PIPX_HOME"),
        "cargo" => Some("CARGO_INSTALL_ROOT"),
        "composer" => Some("COMPOSER_HOME"),
        _ => None,
    }
}
pub fn cache_env(tool: &str) -> Option<&'static str> {
    match tool {
        "bun" => Some("BUN_INSTALL_CACHE_DIR"),
        "uv" => Some("UV_CACHE_DIR"),
        "pipx" => Some("PIP_CACHE_DIR"),
        "composer" => Some("COMPOSER_CACHE_DIR"),
        "dotnet" => Some("NUGET_PACKAGES"),
        _ => None,
    }
}
pub fn bin_env(tool: &str) -> Option<&'static str> {
    match tool {
        "bun" => Some("BUN_INSTALL_BIN"),
        "uv" => Some("UV_TOOL_BIN_DIR"),
        "pipx" => Some("PIPX_BIN_DIR"),
        _ => None,
    }
}
pub fn bin_path(tool: &str, root: &Path) -> PathBuf {
    match tool {
        "npm" | "dotnet" => root.to_path_buf(),
        "composer" => root.join("vendor/bin"),
        _ => root.join("bin"),
    }
}

pub fn configured_env_path(key: &str) -> Option<PathBuf> {
    crate::pathman::get_user_env_vars()
        .ok()
        .and_then(|vars| vars.get(key).cloned())
        .or_else(|| std::env::var(key).ok())
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
}

pub fn default_source(tool: &str) -> Option<PathBuf> {
    let user = crate::pathman::get_user_env_vars().unwrap_or_default();
    if let Some(key) = global_env(tool) {
        if let Some(value) = user.get(key).cloned().or_else(|| std::env::var(key).ok()) {
            if Path::new(&value).is_absolute() {
                return Some(value.into());
            }
        }
    }
    let home = dirs::home_dir()?;
    match tool {
        "bun" => {
            let config = configured_env_path("XDG_CONFIG_HOME")
                .map(|p| p.join(".bunfig.toml"))
                .filter(|p| p.is_file())
                .unwrap_or_else(|| home.join(".bunfig.toml"));
            let configured = std::fs::read_to_string(config)
                .ok()
                .and_then(|text| toml::from_str::<toml::Value>(&text).ok())
                .and_then(|value| {
                    value
                        .get("install")?
                        .get("globalDir")?
                        .as_str()
                        .map(PathBuf::from)
                })
                .filter(|path| path.is_absolute());
            configured.or_else(|| {
                Some(
                    configured_env_path("BUN_INSTALL")
                        .unwrap_or_else(|| home.join(".bun"))
                        .join("install/global"),
                )
            })
        }
        "uv" => dirs::data_dir().map(|p| p.join("uv/tools")),
        "pipx" => dirs::data_local_dir().map(|p| p.join("pipx/pipx")),
        "cargo" => Some(
            user.get("CARGO_HOME")
                .map(PathBuf::from)
                .or_else(|| std::env::var_os("CARGO_HOME").map(PathBuf::from))
                .unwrap_or_else(|| home.join(".cargo")),
        ),
        "composer" => dirs::data_dir().map(|p| p.join("Composer")),
        "dotnet" => Some(home.join(".dotnet/tools")),
        _ => None,
    }
}

fn json(path: &Path) -> Result<Value> {
    Ok(serde_json::from_slice(&std::fs::read(path)?)?)
}
fn valid_pin(name: &str, version: &str) -> bool {
    !name.is_empty()
        && name.as_bytes()[0].is_ascii_alphanumeric()
        && name
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"-_. /".contains(&c))
        && !name.contains(' ')
        && !name.contains("..")
        && !version.is_empty()
        && version.as_bytes()[0].is_ascii_digit()
        && version
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b".-+!_".contains(&c))
}
fn package(name: String, version: String, reason: Option<&str>) -> Package {
    let reason = if !valid_pin(&name, &version) {
        Some("包名或版本无法生成固定安装要求")
    } else {
        reason
    };
    Package {
        name,
        version,
        reason: reason.map(str::to_owned),
    }
}

pub fn inventory(tool: &str, root: &Path) -> Result<Vec<Package>> {
    let mut result = Vec::new();
    match tool {
        "cargo" => {
            let metadata = root.join(".crates.toml");
            let text = std::fs::read_to_string(&metadata)?;
            let value: toml::Value = toml::from_str(&text)
                .map_err(|e| AppError::msg(format!("Cargo 安装记录损坏: {e}")))?;
            let entries = value
                .get("v1")
                .and_then(toml::Value::as_table)
                .ok_or_else(|| AppError::msg("Cargo 缺少 v1 安装记录"))?;
            for key in entries.keys() {
                let parts: Vec<_> = key.splitn(3, ' ').collect();
                if parts.len() != 3 {
                    return Err(AppError::msg("Cargo 安装记录键格式无效"));
                }
                let registry = matches!(
                    parts[2],
                    "(registry+https://github.com/rust-lang/crates.io-index)"
                        | "(registry+https://index.crates.io/)"
                );
                result.push(package(
                    parts[0].into(),
                    parts[1].into(),
                    (!registry).then_some("Git、本地或自定义仓库包需要原始来源"),
                ));
            }
        }
        "uv" | "pipx" => {
            let base = if tool == "pipx" {
                root.join("venvs")
            } else {
                root.to_path_buf()
            };
            for entry in std::fs::read_dir(&base)? {
                let entry = entry?;
                if !entry.path().is_dir() || entry.file_name().to_string_lossy().starts_with('.') {
                    continue;
                }
                let meta = entry.path().join(if tool == "pipx" {
                    "pipx_metadata.json"
                } else {
                    "uv-receipt.toml"
                });
                if !meta.exists() {
                    continue;
                }
                let read = || -> Result<Package> {
                    let (name, pin, direct) = if tool == "pipx" {
                        let data = json(&meta)?;
                        let main = data
                            .get("main_package")
                            .ok_or_else(|| AppError::msg("pipx 缺少主包元数据"))?;
                        let name = main
                            .get("package")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_owned();
                        let pin = main
                            .get("package_version")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_owned();
                        let spec = main
                            .get("package_or_url")
                            .and_then(Value::as_str)
                            .unwrap_or("");
                        let direct = spec.contains([':', '/', '\\', '[', '@'])
                            || main.get("include_dependencies").and_then(Value::as_bool)
                                == Some(true)
                            || main
                                .get("pip_args")
                                .and_then(Value::as_array)
                                .is_some_and(|v| !v.is_empty())
                            || data
                                .get("injected_packages")
                                .and_then(Value::as_object)
                                .is_some_and(|p| !p.is_empty());
                        (name, pin, direct)
                    } else {
                        let value: toml::Value =
                            toml::from_str(&std::fs::read_to_string(&meta)?)
                                .map_err(|e| AppError::msg(format!("uv receipt 损坏: {e}")))?;
                        let reqs = value
                            .get("tool")
                            .and_then(|v| v.get("requirements"))
                            .and_then(toml::Value::as_array)
                            .ok_or_else(|| AppError::msg("uv 缺少工具安装要求"))?;
                        let req = reqs
                            .first()
                            .ok_or_else(|| AppError::msg("uv 安装要求为空"))?;
                        let name = req
                            .get("name")
                            .and_then(toml::Value::as_str)
                            .unwrap_or("")
                            .to_owned();
                        let installed = crate::pip_reinstall::disk_packages(
                            &entry.path().join("Lib/site-packages"),
                        )?;
                        let normalized = |s: &str| s.to_lowercase().replace(['_', '.'], "-");
                        let found = installed
                            .iter()
                            .find(|p| normalized(&p.name) == normalized(&name));
                        let direct = reqs.len() != 1
                            || req.as_table().is_none_or(|v| {
                                v.keys()
                                    .any(|key| !matches!(key.as_str(), "name" | "specifier"))
                            })
                            || found.is_some_and(|p| p.reason.is_some())
                            || value
                                .get("tool")
                                .and_then(|t| t.get("options"))
                                .and_then(toml::Value::as_table)
                                .is_some_and(|v| !v.is_empty());
                        (
                            name,
                            found.map(|p| p.version.clone()).unwrap_or_default(),
                            direct,
                        )
                    };
                    Ok(package(
                        name,
                        pin,
                        direct.then_some("包含直接来源、附加依赖或注入包，需按原始要求恢复"),
                    ))
                };
                result.push(read().unwrap_or_else(|e| Package {
                    name: entry.file_name().to_string_lossy().into(),
                    version: String::new(),
                    reason: Some(format!("安装记录损坏: {e}")),
                }));
            }
        }
        "composer" => {
            let manifest = json(&root.join("composer.json"))?;
            let installed = json(&root.join("vendor/composer/installed.json"))?;
            let packages = installed
                .get("packages")
                .unwrap_or(&installed)
                .as_array()
                .ok_or_else(|| AppError::msg("Composer installed.json 格式无效"))?;
            let custom = manifest.get("repositories").is_some();
            let names: std::collections::BTreeSet<_> = ["require", "require-dev"]
                .into_iter()
                .flat_map(|key| {
                    manifest
                        .get(key)
                        .and_then(Value::as_object)
                        .into_iter()
                        .flat_map(|values| values.keys())
                })
                .collect();
            for name in names {
                if !name.contains('/') {
                    continue;
                }
                let entry = packages
                    .iter()
                    .find(|p| p.get("name").and_then(Value::as_str) == Some(name));
                let version = entry
                    .and_then(|p| p.get("version"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .trim_start_matches('v')
                    .to_owned();
                result.push(package(
                    name.clone(),
                    version,
                    custom.then_some("使用自定义 Composer 仓库，需要原始仓库配置"),
                ));
            }
        }
        "dotnet" => {
            for entry in std::fs::read_dir(root.join(".store"))? {
                let entry = entry?;
                if !entry.path().is_dir() || entry.file_name().to_string_lossy().starts_with('.') {
                    continue;
                }
                let name = entry.file_name().to_string_lossy().into_owned();
                for version in std::fs::read_dir(entry.path())? {
                    let version = version?;
                    if !version.path().is_dir() {
                        continue;
                    }
                    let pin = version.file_name().to_string_lossy().into_owned();
                    let metadata = version
                        .path()
                        .join(&name)
                        .join(&pin)
                        .join(".nupkg.metadata");
                    let official = json(&metadata)
                        .ok()
                        .and_then(|v| v.get("source").and_then(Value::as_str).map(str::to_owned))
                        .is_some_and(|s| {
                            s.trim_end_matches('/')
                                .eq_ignore_ascii_case("https://api.nuget.org/v3/index.json")
                        });
                    result.push(package(
                        name.clone(),
                        pin,
                        (!official).then_some("无法确认 NuGet 官方仓库来源，需原始源配置"),
                    ));
                }
            }
        }
        _ => return Err(AppError::msg("未知管理器安装记录")),
    }
    let mut counts = BTreeMap::new();
    for item in &result {
        *counts.entry(item.name.to_lowercase()).or_insert(0) += 1;
    }
    for item in &mut result {
        if item.name == tool
            || (tool == "cargo" && item.name == "rustup")
            || item.name == "composer/composer"
        {
            item.reason = Some("管理器自身请通过环境管理功能更新".into());
        }
        if (tool != "composer" && item.name.contains('/'))
            || (tool == "composer" && item.name.split('/').count() != 2)
        {
            item.reason = Some("包名不符合管理器要求".into());
        }
        if counts[&item.name.to_lowercase()] > 1 {
            item.reason = Some("同名工具存在多个版本，请先核对旧目录".into());
        }
    }
    result.sort_by(|a, b| a.name.cmp(&b.name));
    result.dedup_by(|a, b| a.name.eq_ignore_ascii_case(&b.name));
    Ok(result)
}

pub fn installation_path(tool: &str, root: &Path, name: &str) -> Result<PathBuf> {
    if !valid_pin(name, "1.0.0") {
        return Err(AppError::msg("安装记录中的包名无效"));
    }
    let normalized = name.to_lowercase().replace(['_', '.'], "-");
    let path = match tool {
        "uv" => root.join(normalized),
        "pipx" => root.join("venvs").join(normalized),
        "composer" => root.join("vendor").join(name),
        "dotnet" => root.join(".store").join(name),
        "cargo" => root.join("bin"),
        _ => return Err(AppError::msg("未知安装位置类型")),
    };
    let path = path.canonicalize()?;
    if !crate::detect::system::path_within(root, &path) {
        return Err(AppError::msg("安装文件链接到来源目录以外"));
    }
    Ok(crate::fsutil::plain_path(path))
}

pub fn verify_cleanup_files(tool: &str, root: &Path, name: &str) -> Result<()> {
    if tool == "bun" {
        return Ok(());
    }
    let location = installation_path(tool, root, name)?;
    if tool == "uv" {
        let receipt: toml::Value =
            toml::from_str(&std::fs::read_to_string(location.join("uv-receipt.toml"))?)
                .map_err(|e| AppError::msg(e.to_string()))?;
        let entries = receipt
            .get("tool")
            .and_then(|v| v.get("entrypoints"))
            .and_then(toml::Value::as_array)
            .ok_or_else(|| AppError::msg("uv 启动器记录缺失，保留旧包"))?;
        for entry in entries {
            let path = entry
                .get("install-path")
                .and_then(toml::Value::as_str)
                .ok_or_else(|| AppError::msg("uv 启动器路径无法确认"))?;
            if !crate::detect::system::path_within(root, Path::new(path)) {
                return Err(AppError::msg(
                    "旧 uv 启动器位于来源目录外，请通过原 uv 工具手动卸载",
                ));
            }
        }
    }
    if tool == "cargo" {
        let receipt: toml::Value =
            toml::from_str(&std::fs::read_to_string(root.join(".crates.toml"))?)
                .map_err(|e| AppError::msg(e.to_string()))?;
        for (key, bins) in receipt
            .get("v1")
            .and_then(toml::Value::as_table)
            .into_iter()
            .flatten()
        {
            if key.split(' ').next() != Some(name) {
                continue;
            }
            let bins = bins
                .as_array()
                .ok_or_else(|| AppError::msg("Cargo 可执行文件清单损坏"))?;
            if bins.iter().any(|bin| {
                !bin.as_str().is_some_and(|s| {
                    !s.is_empty() && s != "." && s != ".." && !s.contains(['/', '\\', ':'])
                })
            }) {
                return Err(AppError::msg("Cargo 可执行文件记录存在越界路径"));
            }
        }
    }
    Ok(())
}

pub fn detect(path: &Path) -> Option<&'static str> {
    if path.join(".crates.toml").is_file() {
        Some("cargo")
    } else if path.join(".store").is_dir() {
        Some("dotnet")
    } else if path.join("composer.json").is_file()
        && path.join("vendor/composer/installed.json").is_file()
        && json(&path.join("composer.json"))
            .is_ok_and(|p| p.get("name").is_none() && p.get("scripts").is_none())
    {
        Some("composer")
    } else if path.join("venvs").is_dir()
        && path
            .join("venvs")
            .read_dir()
            .ok()
            .is_some_and(|mut entries| {
                entries.any(|e| e.is_ok_and(|e| e.path().join("pipx_metadata.json").is_file()))
            })
    {
        Some("pipx")
    } else if path.read_dir().ok().is_some_and(|mut entries| {
        entries.any(|e| e.is_ok_and(|e| e.path().join("uv-receipt.toml").is_file()))
    }) {
        Some("uv")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(root: &Path, relative: &str, content: &str) {
        let file = root.join(relative);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, content).unwrap();
    }

    #[test]
    fn composer_keeps_direct_requirements_and_blocks_custom_repositories() {
        let d = crate::test_support::TestDir::new();
        write(
            d.path(),
            "composer.json",
            r#"{"require":{"vendor/tool":"^1"},"require-dev":{"vendor/dev":"^2"}}"#,
        );
        write(
            d.path(),
            "vendor/composer/installed.json",
            r#"{"packages":[{"name":"vendor/tool","version":"v1.2.3"},{"name":"vendor/dev","version":"2.0.0"},{"name":"vendor/dependency","version":"3.0.0"}]}"#,
        );
        let rows = inventory("composer", d.path()).unwrap();
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|p| p.reason.is_none()));
        assert_eq!(rows[1].version, "1.2.3");
        write(
            d.path(),
            "composer.json",
            r#"{"require":{"vendor/tool":"^1"},"repositories":[{"type":"path","url":"../custom"}]}"#,
        );
        assert!(inventory("composer", d.path()).unwrap()[0].reason.is_some());
    }

    #[test]
    fn dotnet_requires_verified_source_and_rejects_duplicate_versions() {
        let d = crate::test_support::TestDir::new();
        write(
            d.path(),
            ".store/tool/1.0.0/tool/1.0.0/.nupkg.metadata",
            r#"{"source":"https://api.nuget.org/v3/index.json"}"#,
        );
        assert!(inventory("dotnet", d.path()).unwrap()[0].reason.is_none());
        write(
            d.path(),
            ".store/private/2.0.0/private/2.0.0/.nupkg.metadata",
            r#"{"source":"https://private.invalid/index.json"}"#,
        );
        assert!(inventory("dotnet", d.path())
            .unwrap()
            .iter()
            .find(|p| p.name == "private")
            .unwrap()
            .reason
            .is_some());
        write(
            d.path(),
            ".store/tool/1.1.0/tool/1.1.0/.nupkg.metadata",
            "{}",
        );
        assert!(inventory("dotnet", d.path())
            .unwrap()
            .iter()
            .find(|p| p.name == "tool")
            .unwrap()
            .reason
            .as_ref()
            .unwrap()
            .contains("多个版本"));
    }

    #[test]
    fn damaged_python_tool_receipts_stay_visible_and_custom_arguments_are_blocked() {
        let d = crate::test_support::TestDir::new();
        write(d.path(), "venvs/broken/pipx_metadata.json", "{");
        write(
            d.path(),
            "venvs/tool/pipx_metadata.json",
            r#"{"main_package":{"package":"tool","package_version":"1.0.0","package_or_url":"tool","pip_args":["--index-url","https://custom.invalid"]}}"#,
        );
        let rows = inventory("pipx", d.path()).unwrap();
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|p| p.reason.is_some()));
        write(d.path(), "uv/broken/uv-receipt.toml", "invalid = [");
        assert!(inventory("uv", &d.path().join("uv")).unwrap()[0]
            .reason
            .is_some());
    }

    #[test]
    fn cleanup_rejects_external_locations_and_cargo_launcher_traversal() {
        let d = crate::test_support::TestDir::new();
        std::fs::create_dir_all(d.path().join("cargo/bin")).unwrap();
        write(d.path(), "cargo/.crates.toml", "[v1]\n'cli 1.0.0 (registry+https://github.com/rust-lang/crates.io-index)' = ['../outside.exe']");
        assert!(verify_cleanup_files("cargo", &d.path().join("cargo"), "cli").is_err());
        std::fs::create_dir_all(d.path().join("external")).unwrap();
        std::fs::create_dir_all(d.path().join("pipx/venvs")).unwrap();
        junction::create(d.path().join("external"), d.path().join("pipx/venvs/tool")).unwrap();
        assert!(installation_path("pipx", &d.path().join("pipx"), "tool").is_err());
    }
}
