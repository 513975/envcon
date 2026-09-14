use crate::{
    error::{AppError, Result},
    pkgtools::{self, Invocation},
};
use serde::Serialize;
use serde_json::Value;
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledPackage {
    name: String,
    version: Option<String>,
    path: Option<String>,
    detail: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageList {
    tool: String,
    executable: String,
    scope: String,
    source: Option<String>,
    packages: Vec<InstalledPackage>,
    warnings: Vec<String>,
}

fn string(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(str::to_owned)
}

fn node_rows(value: &Value) -> Result<Vec<InstalledPackage>> {
    let projects: Vec<&Value> = match value {
        Value::Array(rows) => rows.iter().collect(),
        Value::Object(_) => vec![value],
        _ => return Err(AppError::msg("包清单不是 JSON 对象或数组")),
    };
    let mut result = Vec::new();
    for project in projects {
        if !project.is_object()
            || project.get("error").is_some() && project.get("dependencies").is_none()
        {
            return Err(AppError::msg("包管理器未能读取依赖清单"));
        }
        for field in ["dependencies", "devDependencies", "optionalDependencies"] {
            if let Some(deps) = project.get(field) {
                let deps = deps
                    .as_object()
                    .ok_or_else(|| AppError::msg("依赖清单格式无效"))?;
                for (name, package) in deps {
                    let path = string(package, "path").or_else(|| {
                        string(project, "path").map(|root| {
                            Path::new(&root)
                                .join("node_modules")
                                .join(name)
                                .to_string_lossy()
                                .into()
                        })
                    });
                    let detail = if package.get("missing").and_then(Value::as_bool) == Some(true) {
                        Some("缺少安装文件".into())
                    } else if package
                        .get("invalid")
                        .is_some_and(|v| v != &Value::Bool(false))
                    {
                        Some("版本与声明不一致".into())
                    } else if package.get("link").and_then(Value::as_bool) == Some(true) {
                        Some("本地链接".into())
                    } else {
                        None
                    };
                    result.push(InstalledPackage {
                        name: name.clone(),
                        version: string(package, "version"),
                        path,
                        detail,
                    });
                }
            }
        }
    }
    result.sort_by(|a, b| (&a.name, &a.path).cmp(&(&b.name, &b.path)));
    result.dedup_by(|a, b| a.name == b.name && a.path == b.path);
    Ok(result)
}

fn pip_rows(value: &Value, inspect: bool) -> Result<Vec<InstalledPackage>> {
    let rows = if inspect {
        value.get("installed")
    } else {
        Some(value)
    }
    .and_then(Value::as_array)
    .ok_or_else(|| AppError::msg("pip 未返回有效包数组"))?;
    rows.iter()
        .map(|p| {
            let meta = if inspect {
                p.get("metadata")
                    .ok_or_else(|| AppError::msg("pip 元数据缺失"))?
            } else {
                p
            };
            Ok(InstalledPackage {
                name: string(meta, "name").ok_or_else(|| AppError::msg("包名缺失"))?,
                version: string(meta, "version"),
                path: string(p, "metadata_location")
                    .or_else(|| string(p, "editable_project_location")),
                detail: if p.get("direct_url").is_some() {
                    Some("本地或直接来源".into())
                } else {
                    None
                },
            })
        })
        .collect()
}

async fn query(
    inv: &Invocation,
    args: &[&str],
    cwd: Option<&Path>,
) -> Result<std::process::Output> {
    pkgtools::output(inv, args, cwd, 60)
        .await
        .map_err(AppError::msg)
}

fn json(output: &std::process::Output) -> Result<Value> {
    serde_json::from_slice(&output.stdout).map_err(|_| {
        AppError::msg(format!(
            "包管理器未返回有效 JSON（退出码 {:?}）：{}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
                .chars()
                .take(800)
                .collect::<String>()
        ))
    })
}

pub async fn list(root: Option<&Path>, tool: &str, directory: Option<&str>) -> Result<PackageList> {
    list_scope(root, tool, directory, false).await
}

pub async fn list_scope(
    root: Option<&Path>,
    tool: &str,
    directory: Option<&str>,
    historical: bool,
) -> Result<PackageList> {
    let definition = crate::package_managers::definition(tool)?;
    let guard = crate::migration::LOCK
        .try_lock()
        .map_err(|_| AppError::msg("正在迁移、重装或配置路径，请任务结束后刷新包清单"))?;
    let directory = directory.map(PathBuf::from);
    if directory
        .as_ref()
        .is_some_and(|p| !p.is_absolute() || !p.is_dir())
    {
        return Err(AppError::msg("请选择已存在的绝对目录"));
    }
    if !historical && directory.is_some() && !definition.project_query {
        return Err(AppError::msg("此管理器请使用全局工具或历史目录查询"));
    }
    if historical {
        let source = crate::fsutil::plain_path(
            directory
                .ok_or_else(|| AppError::msg("请选择历史全局目录"))?
                .canonicalize()?,
        );
        let scan = source.clone();
        let manager = tool.to_owned();
        let packages = tokio::task::spawn_blocking(move || global_rows(&manager, &scan))
            .await
            .map_err(|e| AppError::msg(e.to_string()))??;
        return Ok(PackageList {
            tool: tool.into(),
            executable: String::new(),
            scope: "historical".into(),
            source: Some(source.to_string_lossy().into()),
            packages,
            warnings: vec![],
        });
    }
    let mut inv = pkgtools::find_invocation(root, tool).await;
    if tool == "pip" {
        if let Some(dir) = &directory {
            let python = dir.join("Scripts/python.exe");
            if !python.is_file() {
                return Err(AppError::msg(
                    "所选目录不是 Windows Python 虚拟环境：缺少 Scripts/python.exe",
                ));
            }
            inv = Some(Invocation {
                program: python.to_string_lossy().into(),
                prefix: vec!["-m".into(), "pip".into()],
            });
        }
    }
    let inv = inv.ok_or_else(|| AppError::msg(format!("未找到 {tool}，请先安装或激活对应环境")))?;
    let mut result = PackageList {
        tool: tool.into(),
        executable: inv.program.clone(),
        scope: if directory.is_some() {
            "directory"
        } else {
            "global"
        }
        .into(),
        source: directory.as_ref().map(|p| p.to_string_lossy().into()),
        packages: vec![],
        warnings: vec![],
    };
    if crate::package_managers::isolated(tool) {
        let (configured, _, error) = pkgtools::values(&inv, tool).await;
        let source = directory
            .clone()
            .or_else(|| configured.map(PathBuf::from))
            .ok_or_else(|| AppError::msg(error.unwrap_or_else(|| "无法读取全局目录".into())))?;
        result.source = Some(source.to_string_lossy().into());
        match std::fs::metadata(&source) {
            Ok(meta) if meta.is_dir() => {
                result.packages = if tool == "bun" && directory.is_some() {
                    manifest_rows(&source, true)?
                } else {
                    global_rows(tool, &source)?
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
            _ => return Err(AppError::msg("全局包目录不是普通目录")),
        }
    } else if tool == "pip" {
        let inspection = query(
            &inv,
            &["inspect", "--disable-pip-version-check"],
            directory.as_deref(),
        )
        .await?;
        if inspection.status.success() {
            result.packages = pip_rows(&json(&inspection)?, true)?;
        } else {
            let fallback = query(
                &inv,
                &["list", "--format=json", "--disable-pip-version-check"],
                directory.as_deref(),
            )
            .await?;
            if !fallback.status.success() {
                return Err(AppError::msg(format!(
                    "pip 查询失败：{}",
                    String::from_utf8_lossy(&fallback.stderr)
                )));
            }
            result.packages = pip_rows(&json(&fallback)?, false)?;
            result
                .warnings
                .push("pip inspect 不可用，已回退到 pip list；部分安装路径不可获取".into());
        }
    } else if tool == "pnpm" && directory.is_none() {
        let output = query(&inv, &["root", "--global"], None).await?;
        let source = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        if !output.status.success() || !Path::new(&source).is_absolute() {
            return Err(AppError::msg(format!(
                "无法读取 pnpm 全局目录：{}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }
        result.source = Some(source.clone());
        result.packages = tokio::task::spawn_blocking(move || -> Result<Vec<InstalledPackage>> {
            let source = Path::new(&source);
            match std::fs::metadata(source) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
                Err(e) => return Err(e.into()),
                _ => {}
            }
            Ok(crate::package_managers::installed_packages("pnpm", source)?
                .into_iter()
                .map(|(p, path)| InstalledPackage {
                    name: p.name,
                    version: if p.version.is_empty() {
                        None
                    } else {
                        Some(p.version)
                    },
                    path: path.map(|p| p.to_string_lossy().into()),
                    detail: p.reason,
                })
                .collect())
        })
        .await
        .map_err(|e| AppError::msg(e.to_string()))??;
    } else if tool == "yarn" {
        let version = query(&inv, &["--version"], directory.as_deref()).await?;
        if !version.status.success()
            || !String::from_utf8_lossy(&version.stdout)
                .trim()
                .starts_with("1.")
        {
            return Err(AppError::msg(
                "当前仅支持 Yarn Classic 包清单；Yarn Berry/PnP 项目暂不支持",
            ));
        }
        let source = if let Some(dir) = &directory {
            dir.clone()
        } else {
            let output = query(&inv, &["global", "dir", "--silent"], None).await?;
            let path = String::from_utf8_lossy(&output.stdout).trim().to_owned();
            if !output.status.success() || !Path::new(&path).is_absolute() {
                return Err(AppError::msg(format!(
                    "无法读取 Yarn 全局目录：{}",
                    String::from_utf8_lossy(&output.stderr)
                )));
            }
            PathBuf::from(path)
        };
        result.source = Some(source.to_string_lossy().into());
        let project = directory.is_some();
        result.packages = tokio::task::spawn_blocking(move || manifest_rows(&source, project))
            .await
            .map_err(|e| AppError::msg(e.to_string()))??;
    } else {
        let args = if directory.is_some() {
            vec!["list", "--depth=0", "--json", "--long"]
        } else {
            vec!["list", "--global", "--depth=0", "--json", "--long"]
        };
        let output = query(&inv, &args, directory.as_deref()).await?;
        let value = json(&output)?;
        result.packages = node_rows(&value)?;
        if !output.status.success() {
            if result.packages.is_empty() {
                return Err(AppError::msg(format!(
                    "包查询失败：{}",
                    String::from_utf8_lossy(&output.stderr)
                        .chars()
                        .take(1000)
                        .collect::<String>()
                )));
            }
            result.warnings.push(format!(
                "包管理器报告依赖异常（退出码 {:?}），以下为可读取的部分清单：{}",
                output.status.code(),
                String::from_utf8_lossy(&output.stderr)
                    .chars()
                    .take(1000)
                    .collect::<String>()
            ));
        }
        if result.source.is_none() {
            result.source = string(
                value.as_array().and_then(|a| a.first()).unwrap_or(&value),
                "path",
            );
        }
    }
    result.packages.sort_by_key(|p| p.name.to_lowercase());
    drop(guard);
    Ok(result)
}

fn global_rows(tool: &str, source: &Path) -> Result<Vec<InstalledPackage>> {
    if std::fs::read_dir(source)?.next().is_none() {
        return Ok(vec![]);
    }
    if tool == "pip" {
        return Ok(crate::pip_reinstall::disk_packages(source)?
            .into_iter()
            .map(|p| InstalledPackage {
                name: p.name,
                version: if p.version.is_empty() {
                    None
                } else {
                    Some(p.version)
                },
                path: Some(source.to_string_lossy().into()),
                detail: p.reason,
            })
            .collect());
    }
    Ok(crate::package_managers::installed_packages(tool, source)?
        .into_iter()
        .map(|(p, path)| InstalledPackage {
            name: p.name,
            version: if p.version.is_empty() {
                None
            } else {
                Some(p.version)
            },
            path: path.map(|p| p.to_string_lossy().into()),
            detail: p.reason,
        })
        .collect())
}

pub async fn global_sources(root: Option<&Path>, tool: &str) -> Result<Vec<String>> {
    crate::package_managers::definition(tool)?;
    let mut candidates = Vec::new();
    if let Some(path) = crate::package_managers::default_source(tool) {
        candidates.push(path);
    }
    if let Some(root) = root {
        candidates.push(root.join("globals").join(format!("{tool}-global")));
        if tool == "npm" {
            match std::fs::read_dir(root.join("envs/nodes")) {
                Ok(entries) => {
                    for entry in entries {
                        candidates.push(entry?.path());
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
        }
    }
    if tool == "npm" {
        if let Some(inv) = pkgtools::find_invocation(root, tool).await {
            if let Some(parent) = Path::new(&inv.program).parent() {
                candidates.push(parent.to_path_buf());
            }
        }
        if let Some(appdata) = std::env::var_os("APPDATA") {
            candidates.push(PathBuf::from(appdata).join("npm"));
        }
    }
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        if tool == "pnpm" {
            candidates.push(PathBuf::from(local).join("pnpm/global"));
        } else if tool == "yarn" {
            candidates.push(PathBuf::from(local).join("Yarn/Data/global"));
        }
    }
    let mut paths: Vec<PathBuf> = Vec::new();
    for candidate in candidates {
        if !candidate.is_dir() {
            continue;
        }
        let path = candidate.canonicalize()?;
        if !paths
            .iter()
            .any(|p| crate::detect::system::paths_eq(p, &path))
        {
            paths.push(path);
        }
    }
    Ok(paths
        .into_iter()
        .map(|p| crate::fsutil::plain_path(p).to_string_lossy().into())
        .collect())
}

fn manifest_rows(source: &Path, project: bool) -> Result<Vec<InstalledPackage>> {
    let manifest = source.join("package.json");
    let bytes = match std::fs::read(manifest) {
        Ok(bytes) => bytes,
        Err(e) if !project && e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
        Err(e) => return Err(e.into()),
    };
    let value: Value = serde_json::from_slice(&bytes)?;
    if !value.is_object() {
        return Err(AppError::msg("package.json 格式无效"));
    }
    let mut packages = std::collections::BTreeSet::new();
    for field in ["dependencies", "devDependencies", "optionalDependencies"] {
        if let Some(deps) = value.get(field).and_then(Value::as_object) {
            packages.extend(deps.keys().cloned());
        }
    }
    packages
        .into_iter()
        .map(|name| {
            let rel = Path::new(&name);
            if rel.is_absolute()
                || rel
                    .components()
                    .any(|c| matches!(c, std::path::Component::ParentDir))
                || name.contains('\\')
            {
                return Err(AppError::msg("package.json 中存在无效包名"));
            }
            let path = source.join("node_modules").join(&name);
            let installed: Option<Value> = std::fs::read(path.join("package.json"))
                .ok()
                .and_then(|b| serde_json::from_slice(&b).ok());
            Ok(InstalledPackage {
                name,
                version: installed.as_ref().and_then(|p| string(p, "version")),
                path: Some(path.to_string_lossy().into()),
                detail: if installed.is_none() {
                    Some("已声明但未安装，或安装元数据损坏".into())
                } else {
                    None
                },
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn historical_global_reads_disk_without_a_launcher_or_project_manifest() {
        let d = crate::test_support::TestDir::new();
        let old = d.path().join("envs/nodes/node-old");
        std::fs::create_dir_all(old.join("node_modules/@scope/.update-remnant")).unwrap();
        std::fs::create_dir_all(old.join("node_modules/@scope/cli")).unwrap();
        std::fs::write(
            old.join("node_modules/@scope/cli/package.json"),
            r#"{"name":"@scope/cli","version":"1.0.0"}"#,
        )
        .unwrap();
        let result = list_scope(None, "npm", Some(&old.to_string_lossy()), true)
            .await
            .unwrap();
        assert_eq!(result.scope, "historical");
        assert!(result.executable.is_empty());
        assert_eq!(result.packages.len(), 1);
        assert_eq!(result.packages[0].name, "@scope/cli");
        assert!(result.packages[0]
            .path
            .as_ref()
            .unwrap()
            .ends_with(r"node_modules\@scope\cli"));
        assert!(list_scope(None, "pip", Some(&old.to_string_lossy()), true)
            .await
            .unwrap()
            .packages
            .is_empty());
        assert!(list_scope(None, "npm", None, true).await.is_err());
    }
    #[test]
    fn structured_lists_preserve_missing_packages_and_pip_locations() {
        let value = serde_json::json!([{ "path": "D:/project", "dependencies": { "@scope/tool": {"version":"1.0.0"}, "broken": {"missing":true} }, "devDependencies": {"dev": {"version":"2.0.0"}} }]);
        let rows = node_rows(&value).unwrap();
        assert_eq!(rows.len(), 3);
        assert!(rows
            .iter()
            .find(|p| p.name == "broken")
            .unwrap()
            .detail
            .is_some());
        assert!(node_rows(&serde_json::json!({"error":{"code":"FAIL"}})).is_err());
        assert!(node_rows(&serde_json::json!([])).unwrap().is_empty());
        assert!(pip_rows(&serde_json::json!({}), true).is_err());
        let rows = pip_rows(&serde_json::json!({"installed":[{"metadata":{"name":"requests","version":"2.0"},"metadata_location":"D:/venv/Lib/site-packages/requests.dist-info"}]}), true).unwrap();
        assert!(rows[0].path.as_ref().unwrap().contains("site-packages"));
    }

    #[tokio::test]
    #[ignore = "reads installed npm/pnpm/yarn/pip via ENVCON_TEST_ROOT without changing configuration"]
    async fn actual_managers_return_readable_lists() {
        let root = std::env::var("ENVCON_TEST_ROOT").expect("set ENVCON_TEST_ROOT");
        for tool in ["npm", "pnpm", "yarn", "pip"] {
            let result = list(Some(Path::new(&root)), tool, None)
                .await
                .unwrap_or_else(|e| panic!("{tool}: {e}"));
            assert_eq!(result.tool, tool);
            assert!(Path::new(&result.executable).is_absolute());
            println!("{tool}: {} packages", result.packages.len());
        }
        let project = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .to_string_lossy();
        let result = list(Some(Path::new(&root)), "npm", Some(&project))
            .await
            .unwrap();
        assert!(result.packages.iter().any(|p| p.name == "react"));
        let sources = global_sources(Some(Path::new(&root)), "npm").await.unwrap();
        for source in sources.iter().filter(|p| p.contains("node-24")) {
            let result = list_scope(Some(Path::new(&root)), "npm", Some(source), true)
                .await
                .unwrap();
            assert!(result.packages.iter().any(|p| p.name == "@openai/codex"));
            assert!(!result.packages.iter().any(|p| p.name.contains("/.")));
            println!(
                "historical {source}: {} packages: {}",
                result.packages.len(),
                result
                    .packages
                    .iter()
                    .map(|p| p.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
    }
}
