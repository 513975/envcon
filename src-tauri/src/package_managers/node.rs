use super::Package;
use crate::{
    error::{AppError, Result},
    fsutil::plain_path,
};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

pub(crate) fn valid_name(name: &str) -> bool {
    let part = |s: &str| {
        !s.is_empty()
            && s.as_bytes()[0].is_ascii_alphanumeric()
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
    };
    if name.len() > 214 {
        return false;
    }
    if let Some(scoped) = name.strip_prefix('@') {
        scoped
            .split_once('/')
            .is_some_and(|(scope, name)| part(scope) && part(name))
    } else {
        part(name)
    }
}

pub(crate) fn valid_version(version: &str) -> bool {
    let core = version.split(['-', '+']).next().unwrap_or("");
    core.split('.').count() == 3
        && core
            .split('.')
            .all(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
        && version.len() < 200
        && version
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b".-+".contains(&b))
}

fn registry_spec(spec: &str) -> bool {
    !spec.is_empty()
        && spec
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b".^~*<>= |+-".contains(&b))
}

fn read_json(path: &Path) -> Result<Value> {
    Ok(serde_json::from_slice(&std::fs::read(path)?)?)
}

pub(crate) fn modules_dir(source: &Path) -> Result<PathBuf> {
    if source
        .file_name()
        .is_some_and(|n| n.eq_ignore_ascii_case("node_modules"))
    {
        return Ok(source.to_path_buf());
    }
    if source.join("node_modules").is_dir() {
        return Ok(source.join("node_modules"));
    }
    // pnpm global-dir contains a layout-version subdirectory, e.g. global/5.
    let mut candidates = Vec::new();
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        if entry
            .file_name()
            .to_string_lossy()
            .bytes()
            .all(|b| b.is_ascii_digit())
            && entry.path().join("node_modules").is_dir()
        {
            candidates.push(entry.path().join("node_modules"));
        }
    }
    if candidates.len() == 1 {
        return Ok(candidates.remove(0));
    }
    Err(AppError::msg(
        "未找到唯一的全局 node_modules；请选择实际全局目录或 node_modules 目录",
    ))
}

pub(crate) fn inventory(tool: &str, source: &Path) -> Result<Vec<Package>> {
    if tool == "pnpm" {
        let root = if source.join("v11").is_dir() {
            source.join("v11")
        } else {
            source.to_path_buf()
        };
        if root.file_name().is_some_and(|s| s == "v11") {
            let mut rows = Vec::new();
            // pnpm 11 exposes active groups through junctions; unlinked directories are stale installs.
            for entry in std::fs::read_dir(&root)? {
                let entry = entry?;
                if !std::fs::symlink_metadata(entry.path())?
                    .file_type()
                    .is_symlink()
                {
                    continue;
                }
                let target = entry.path().canonicalize()?;
                if !crate::detect::system::path_within(&root, &target) {
                    return Err(AppError::msg("pnpm 安装组指向全局目录外，无法确定来源"));
                }
                rows.extend(inventory_project(tool, &target)?);
            }
            let mut counts = BTreeMap::new();
            for p in &rows {
                *counts.entry(p.name.clone()).or_insert(0) += 1;
            }
            for p in &mut rows {
                if counts[&p.name] > 1 {
                    p.reason = Some("多个安装组包含同名包，请先处理冲突".into());
                }
            }
            rows.sort_by(|a, b| a.name.cmp(&b.name));
            rows.dedup_by(|a, b| a.name == b.name);
            return Ok(rows);
        }
    }
    inventory_project(tool, source)
}

pub(crate) fn installed_packages(
    tool: &str,
    source: &Path,
) -> Result<Vec<(Package, Option<PathBuf>)>> {
    let packages = inventory(tool, source)?;
    let root = if source.join("v11").is_dir() {
        source.join("v11")
    } else {
        source.to_path_buf()
    };
    let mut module_roots = Vec::new();
    if let Ok(modules) = modules_dir(source) {
        module_roots.push(modules);
    }
    if root.file_name().is_some_and(|n| n == "v11") {
        for entry in std::fs::read_dir(root)? {
            let entry = entry?;
            if entry.file_type()?.is_symlink() {
                module_roots.push(entry.path().join("node_modules"));
            }
        }
    }
    Ok(packages
        .into_iter()
        .map(|p| {
            let path = module_roots
                .iter()
                .map(|root| root.join(&p.name))
                .find(|path| path.join("package.json").is_file());
            (p, path.map(|p| plain_path(p.canonicalize().unwrap_or(p))))
        })
        .collect())
}

fn inventory_project(tool: &str, source: &Path) -> Result<Vec<Package>> {
    let modules = modules_dir(source)?;
    let manifest = modules.parent().unwrap().join("package.json");
    let lock_path = modules.join(".package-lock.json");
    let lock = if tool == "npm" && lock_path.exists() {
        Some(read_json(&lock_path)?)
    } else {
        None
    };
    let mut names: BTreeMap<String, Option<String>> = BTreeMap::new();
    if tool != "npm" {
        let json = read_json(&manifest)
            .map_err(|e| AppError::msg(format!("全局清单读取失败 {}: {e}", manifest.display())))?;
        if let Some(deps) = json.get("dependencies").and_then(Value::as_object) {
            for (name, spec) in deps {
                names.insert(name.clone(), Some(spec.as_str().unwrap_or("").into()));
            }
        }
    } else {
        for entry in std::fs::read_dir(&modules)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            if name.starts_with('@') {
                for scoped in std::fs::read_dir(entry.path())? {
                    let scoped = scoped?;
                    if scoped.file_name().to_string_lossy().starts_with('.') {
                        continue;
                    }
                    if !scoped.path().is_dir() && !scoped.file_type()?.is_symlink() {
                        continue;
                    }
                    names.insert(
                        format!("{name}/{}", scoped.file_name().to_string_lossy()),
                        None,
                    );
                }
            } else if entry.path().is_dir() || entry.file_type()?.is_symlink() {
                names.insert(name, None);
            }
        }
    }
    let mut out = Vec::new();
    for (name, declared) in names {
        if !valid_name(&name) {
            return Err(AppError::msg(format!("无效包名: {name}")));
        }
        let path = modules.join(&name);
        let json = match read_json(&path.join("package.json")) {
            Ok(json) => json,
            Err(e) => {
                out.push(Package {
                    name,
                    version: String::new(),
                    reason: Some(format!("安装元数据缺失或损坏: {e}")),
                });
                continue;
            }
        };
        let version = json
            .get("version")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned();
        let resolved = json.get("_resolved").and_then(Value::as_str).unwrap_or("");
        let from = json.get("_from").and_then(Value::as_str).unwrap_or("");
        let locked = lock
            .as_ref()
            .and_then(|l| l.get("packages"))
            .and_then(|p| p.get(format!("node_modules/{name}")))
            .and_then(|p| p.get("resolved"))
            .and_then(Value::as_str)
            .unwrap_or("");
        let registry_url = |value: &str| {
            reqwest::Url::parse(value).ok().is_some_and(|url| {
                matches!(url.scheme(), "http" | "https")
                    && matches!(
                        url.host_str(),
                        Some("registry.npmjs.org" | "registry.npmmirror.com")
                    )
                    && url.path().ends_with(&format!(
                        "/{}/-/{}-{version}.tgz",
                        name,
                        name.rsplit('/').next().unwrap()
                    ))
            })
        };
        let registry_origin = registry_url(if locked.is_empty() { resolved } else { locked });
        let external_link = path
            .canonicalize()
            .ok()
            .is_some_and(|real| !crate::detect::system::path_within(&modules, &real));
        let reason = if json.get("name").and_then(Value::as_str) != Some(&name) {
            Some("包别名与真实名称不同，需按原始安装来源恢复")
        } else if !valid_version(&version) {
            Some("版本不是固定 semver，不能自动生成安装要求")
        } else if matches!(name.as_str(), "npm" | "pnpm" | "yarn" | "corepack" | "bun") {
            Some("包管理器自身请使用环境安装功能更新")
        } else if external_link
            || declared.as_ref().is_some_and(|s| !registry_spec(s))
            || resolved.starts_with("file:")
            || resolved.starts_with("git")
            || from.contains("file:")
            || from.contains("git+")
            || from.contains("http:")
            || from.contains("https:")
        {
            Some("本地、Git、URL、链接或别名来源需要原始安装材料")
        } else if tool == "npm" && !registry_origin {
            Some("缺少可确认的仓库来源元数据，不能仅凭包名替换为仓库包")
        } else {
            None
        };
        out.push(Package {
            name,
            version,
            reason: reason.map(str::to_owned),
        });
    }
    Ok(out)
}
