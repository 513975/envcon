use crate::{
    error::{AppError, Result},
    package_managers::Package,
};
use std::{collections::BTreeMap, path::Path};

pub struct Invocation {
    pub arguments: Vec<String>,
    pub environment: BTreeMap<String, String>,
}

pub enum Action<'a> {
    Install(&'a Package),
    Remove(&'a str),
}

pub fn environment(tool: &str, scope: &Path, workspace: &Path) -> BTreeMap<String, String> {
    let mut values = BTreeMap::new();
    let mut put = |key: &str, path: std::path::PathBuf| {
        values.insert(key.into(), path.to_string_lossy().into());
    };
    if let Some(key) = super::global_env(tool) {
        put(key, scope.to_path_buf());
    }
    if let Some(key) = super::bin_env(tool) {
        put(key, workspace.join("bin"));
    }
    if let Some(key) = super::cache_env(tool) {
        put(key, workspace.join("cache"));
    }
    match tool {
        "pipx" => {
            put("UV_CACHE_DIR", workspace.join("cache"));
            put("PIPX_MAN_DIR", workspace.join("man"));
            put("PIPX_LOG_DIR", workspace.join("logs"));
            put("PIPX_TRASH_DIR", workspace.join("trash"));
        }
        "cargo" => {
            put("CARGO_HOME", workspace.join(".cargo"));
        }
        "dotnet" => {
            put("DOTNET_CLI_HOME", workspace.join(".dotnet"));
        }
        _ => {}
    }
    values.extend([
        ("PIP_CONFIG_FILE".into(), "NUL".into()),
        ("PIP_DISABLE_PIP_VERSION_CHECK".into(), "1".into()),
        ("UV_NO_CONFIG".into(), "1".into()),
        ("UV_PYTHON_DOWNLOADS".into(), "never".into()),
        ("DOTNET_NOLOGO".into(), "1".into()),
        ("DOTNET_CLI_TELEMETRY_OPTOUT".into(), "1".into()),
        ("DOTNET_SKIP_FIRST_TIME_EXPERIENCE".into(), "1".into()),
    ]);
    values
}

pub fn invocation(
    tool: &str,
    scope: &Path,
    workspace: &Path,
    action: Action<'_>,
) -> Result<Invocation> {
    let path = scope.to_string_lossy();
    let arguments: Vec<String> = match action {
        Action::Install(package) => {
            if package.reason.is_some() {
                return Err(AppError::msg("此包来源不可自动重装"));
            }
            let name = package.name.as_str();
            let version = package.version.as_str();
            match tool {
                "uv" => vec![
                    "tool".into(),
                    "install".into(),
                    format!("{name}=={version}"),
                ],
                "pipx" => vec!["install".into(), format!("{name}=={version}")],
                "cargo" => vec![
                    "install".into(),
                    "--root".into(),
                    path.into(),
                    "--locked".into(),
                    "--version".into(),
                    format!("={version}"),
                    "--".into(),
                    name.into(),
                ],
                "composer" => vec![
                    "global".into(),
                    "require".into(),
                    "--no-interaction".into(),
                    "--".into(),
                    format!("{name}:{version}"),
                ],
                "dotnet" => vec![
                    "tool".into(),
                    "install".into(),
                    name.into(),
                    "--tool-path".into(),
                    path.into(),
                    "--version".into(),
                    version.into(),
                    "--configfile".into(),
                    workspace
                        .join(".envcon-nuget.config")
                        .to_string_lossy()
                        .into(),
                ],
                _ => return Err(AppError::msg("缺少安装命令适配器")),
            }
        }
        Action::Remove(name) => match tool {
            "bun" => vec![
                "remove".into(),
                "--global".into(),
                "--ignore-scripts".into(),
                "--".into(),
                name.into(),
            ],
            "uv" => vec!["tool".into(), "uninstall".into(), name.into()],
            "pipx" => vec!["uninstall".into(), name.into()],
            "cargo" => vec![
                "uninstall".into(),
                "--root".into(),
                path.into(),
                "--".into(),
                name.into(),
            ],
            "composer" => vec![
                "global".into(),
                "remove".into(),
                "--no-interaction".into(),
                "--no-scripts".into(),
                "--no-plugins".into(),
                "--".into(),
                name.into(),
            ],
            "dotnet" => vec![
                "tool".into(),
                "uninstall".into(),
                name.into(),
                "--tool-path".into(),
                path.into(),
            ],
            _ => return Err(AppError::msg("缺少卸载命令适配器")),
        },
    };
    Ok(Invocation {
        arguments,
        environment: environment(tool, scope, workspace),
    })
}

pub fn prepare(tool: &str, workspace: &Path) -> Result<()> {
    if tool == "bun" {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(workspace.join("package.json"))?;
        file.write_all(br#"{"dependencies":{}}"#)?;
    }
    if tool == "dotnet" {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(workspace.join(".envcon-nuget.config"))?;
        file.write_all(br#"<?xml version="1.0" encoding="utf-8"?><configuration><packageSources><clear/><add key="nuget.org" value="https://api.nuget.org/v3/index.json"/></packageSources></configuration>"#)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_commands_bind_install_and_uninstall_to_explicit_roots() {
        let root = Path::new("D:/new tools");
        let scratch = Path::new("D:/scratch");
        for tool in ["uv", "pipx", "cargo", "composer", "dotnet"] {
            let p = Package {
                name: if tool == "composer" {
                    "vendor/tool"
                } else {
                    "tool"
                }
                .into(),
                version: "1.2.3".into(),
                reason: None,
            };
            let install = invocation(tool, root, root, Action::Install(&p)).unwrap();
            let remove = invocation(tool, root, scratch, Action::Remove(&p.name)).unwrap();
            if let Some(key) = super::super::global_env(tool) {
                assert_eq!(remove.environment[key], "D:/new tools");
            } else {
                assert!(remove.arguments.contains(&"D:/new tools".into()));
            }
            assert!(install.arguments.iter().any(|a| a.contains("1.2.3")));
        }
    }
}
