use crate::error::{AppError, Result};
use crate::package_managers::{ItemResult, Package};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
    time::Duration,
};
use tokio::process::Command;

const INSPECT: &str = r#"
import sys, json, platform, site
from importlib import metadata
items = []
paths = [sys.argv[1]] if len(sys.argv) > 1 else list(sys.path)
if len(sys.argv) == 1 and sys.prefix == sys.base_prefix:
    user = site.getusersitepackages()
    paths.extend([user] if isinstance(user, str) else user)
distributions = metadata.distributions(path=list(dict.fromkeys(paths)))
for d in distributions:
    name = d.metadata.get('Name', '')
    direct = d.read_text('direct_url.json')
    items.append(dict(name=name, version=d.version or '', direct=bool(direct)))
print(json.dumps(dict(python=platform.python_version(), prefix=sys.prefix, packages=items)))
"#;

#[derive(Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub source: String,
    pub source_kind: String,
    pub python: String,
    pub destination: String,
    pub source_version: String,
    pub target_version: String,
    pub packages: Vec<Package>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub status: String,
    pub message: String,
    pub destination: String,
    pub report: String,
    pub items: Vec<ItemResult>,
    pub dependency_check: Option<String>,
}

static TASK: Mutex<Option<Snapshot>> = Mutex::new(None);
static CANCEL: AtomicBool = AtomicBool::new(false);

fn normalize(name: &str) -> String {
    name.to_ascii_lowercase()
        .split(['-', '_', '.'])
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

fn index_pin(name: &str, version: &str) -> bool {
    !name.is_empty()
        && name.len() <= 200
        && name.as_bytes()[0].is_ascii_alphanumeric()
        && name
            .as_bytes()
            .last()
            .is_some_and(u8::is_ascii_alphanumeric)
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
        && !version.is_empty()
        && version.len() <= 200
        && version.as_bytes()[0].is_ascii_digit()
        && version
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b".!+_-".contains(&b))
}

#[derive(Deserialize)]
struct RawPackage {
    name: String,
    version: String,
    direct: bool,
}
#[derive(Deserialize)]
struct Inspection {
    python: String,
    prefix: String,
    packages: Vec<RawPackage>,
}

fn packages(raw: Vec<RawPackage>) -> Vec<Package> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for item in raw {
        let key = normalize(&item.name);
        let reason = if !index_pin(&item.name, &item.version) {
            Some("包名或版本无法生成有效的索引安装要求")
        } else if item.direct {
            Some("本地、URL 或可编辑安装，需从原始源码/安装文件恢复")
        } else if matches!(key.as_str(), "pip" | "setuptools" | "wheel") {
            Some("构建工具由新环境维护，不复制旧版本")
        } else {
            None
        };
        if !seen.insert(key.clone()) {
            for row in out
                .iter_mut()
                .filter(|p: &&mut Package| normalize(&p.name) == key)
            {
                row.reason = Some("同名包有多份元数据，请先处理源环境冲突".into());
            }
            continue;
        }
        out.push(Package {
            name: item.name,
            version: item.version,
            reason: reason.map(str::to_owned),
        });
    }
    out.sort_by_key(|p| normalize(&p.name));
    out
}

pub(crate) fn disk_packages(source: &Path) -> Result<Vec<Package>> {
    let mut raw = Vec::new();
    let mut damaged = Vec::new();
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let lower = name.to_ascii_lowercase();
        let metadata = if lower.ends_with(".dist-info") {
            entry.path().join("METADATA")
        } else if lower.ends_with(".egg-info") {
            if entry.path().is_dir() {
                entry.path().join("PKG-INFO")
            } else {
                entry.path()
            }
        } else if lower.ends_with(".egg-link") {
            damaged.push(Package {
                name: name.trim_end_matches(".egg-link").into(),
                version: String::new(),
                reason: Some("可编辑安装链接，需要原始源码恢复".into()),
            });
            continue;
        } else {
            continue;
        };
        match std::fs::read_to_string(&metadata) {
            Ok(text) => {
                // Distribution identity lives in the RFC 822 header, not the description body.
                let mut fields = std::collections::HashMap::new();
                for line in text.lines().take_while(|line| !line.is_empty()) {
                    if let Some((key, value)) = line.split_once(':') {
                        fields
                            .entry(key.to_ascii_lowercase())
                            .or_insert_with(|| value.trim().to_owned());
                    }
                }
                raw.push(RawPackage {
                    name: fields.remove("name").unwrap_or(name),
                    version: fields.remove("version").unwrap_or_default(),
                    direct: entry.path().join("direct_url.json").exists(),
                });
            }
            Err(e) => damaged.push(Package {
                name,
                version: String::new(),
                reason: Some(format!("安装元数据无法读取: {e}")),
            }),
        }
    }
    let mut result = packages(raw);
    result.extend(damaged);
    result.sort_by_key(|p| normalize(&p.name));
    Ok(result)
}

// PIP_TARGET/PREFIX/USER can silently redirect installation out of the new venv.
fn command(python: &Path, args: &[&str]) -> Command {
    let mut cmd = Command::new(python);
    cmd.args(args)
        .current_dir(std::env::temp_dir())
        .creation_flags(0x08000000)
        .kill_on_drop(true);
    for (key, _) in std::env::vars_os() {
        let key_str = key.to_string_lossy().to_ascii_uppercase();
        if key_str.starts_with("PIP_") || key_str.starts_with("PYTHON") {
            cmd.env_remove(key);
        }
    }
    cmd.env("PIP_CONFIG_FILE", "NUL")
        .env("PIP_DISABLE_PIP_VERSION_CHECK", "1");
    cmd
}

async fn run(python: &Path, args: &[&str], seconds: u64) -> Result<String> {
    let output =
        crate::reinstall_process::output(command(python, args), Duration::from_secs(seconds))
            .await?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::msg(format!(
            "退出码 {:?}: {}\n{}",
            output.status.code(),
            stderr
                .chars()
                .rev()
                .take(3000)
                .collect::<String>()
                .chars()
                .rev()
                .collect::<String>(),
            stdout
                .chars()
                .rev()
                .take(1000)
                .collect::<String>()
                .chars()
                .rev()
                .collect::<String>()
        )));
    }
    Ok(stdout.into_owned())
}

fn python_path(value: &str) -> Result<PathBuf> {
    let path = PathBuf::from(value);
    if !path.is_absolute()
        || !path.is_file()
        || !path
            .extension()
            .is_some_and(|s| s.eq_ignore_ascii_case("exe"))
    {
        return Err(AppError::msg("请选择 Python 解释器的绝对 EXE 路径"));
    }
    // Do not canonicalize the executable: venv interpreters may be symlinks to the base Python.
    Ok(command_path(
        path.parent()
            .unwrap()
            .canonicalize()?
            .join(path.file_name().unwrap()),
    ))
}

fn command_path(path: PathBuf) -> PathBuf {
    let text = path.to_string_lossy();
    if let Some(unc) = text.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{unc}"));
    }
    PathBuf::from(text.strip_prefix(r"\\?\").unwrap_or(&text))
}

fn destination_path(value: &str) -> Result<PathBuf> {
    let path = PathBuf::from(value);
    if !path.is_absolute()
        || path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
        || path.file_name().is_none()
        || std::fs::symlink_metadata(&path).is_ok()
    {
        return Err(AppError::msg(
            "目标必须是尚不存在的新虚拟环境目录，不覆盖已有环境",
        ));
    }
    let parent = path
        .parent()
        .unwrap()
        .canonicalize()
        .map_err(|_| AppError::msg("目标父目录不存在，请先选择已有父目录"))?;
    Ok(command_path(parent.join(path.file_name().unwrap())))
}

pub async fn preview(
    source: String,
    source_kind: String,
    python: String,
    destination: String,
) -> Result<Plan> {
    let python = python_path(&python)?;
    let destination = destination_path(&destination)?;
    let target: Inspection =
        serde_json::from_str(&run(&python, &["-I", "-c", INSPECT], 30).await?)?;
    let (source, inspection) = match source_kind.as_str() {
        "python" => {
            let source = python_path(&source)?;
            let data: Inspection =
                serde_json::from_str(&run(&source, &["-I", "-c", INSPECT], 30).await?)?;
            (source, data)
        }
        "directory" => {
            let source = PathBuf::from(&source);
            if !source.is_absolute() || !source.is_dir() {
                return Err(AppError::msg("请选择旧 site-packages 目录"));
            }
            let source = source.canonicalize()?;
            let mut data: Inspection = serde_json::from_str(
                &run(
                    &python,
                    &["-I", "-c", INSPECT, &source.to_string_lossy()],
                    30,
                )
                .await?,
            )?;
            data.python = "目录元数据（原 Python 版本未知）".into();
            data.prefix = source.to_string_lossy().into();
            (source, data)
        }
        _ => return Err(AppError::msg("未知源类型")),
    };
    if crate::detect::system::path_within(Path::new(&inspection.prefix), &destination) {
        return Err(AppError::msg("新环境不能创建在旧环境内部"));
    }
    Ok(Plan {
        source: source.to_string_lossy().into(),
        source_kind,
        python: python.to_string_lossy().into(),
        destination: destination.to_string_lossy().into(),
        source_version: inspection.python,
        target_version: target.python,
        packages: packages(inspection.packages),
    })
}

fn selected(plan: &Plan, names: &[String]) -> Result<Vec<Package>> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for name in names {
        let key = normalize(name);
        let package = plan
            .packages
            .iter()
            .find(|p| normalize(&p.name) == key && p.reason.is_none())
            .ok_or_else(|| AppError::msg("所选包不在可安装清单中，请重新预览"))?;
        if seen.insert(key) {
            out.push(package.clone());
        }
    }
    if out.is_empty() {
        return Err(AppError::msg("请至少选择一个可重装的包"));
    }
    Ok(out)
}

pub fn status() -> Option<Snapshot> {
    TASK.lock().unwrap().clone()
}
pub fn cancel() {
    CANCEL.store(true, Ordering::SeqCst);
}

fn update(f: impl FnOnce(&mut Snapshot)) -> Result<()> {
    let mut guard = TASK.lock().unwrap();
    if let Some(task) = guard.as_mut() {
        f(task);
        std::fs::write(&task.report, serde_json::to_vec_pretty(task)?)?;
    }
    Ok(())
}

pub async fn start(plan: Plan, names: Vec<String>) -> Result<()> {
    let guard = crate::migration::LOCK
        .try_lock()
        .map_err(|_| AppError::msg("已有迁移、重装或路径配置任务正在运行"))?;
    let fresh = preview(
        plan.source.clone(),
        plan.source_kind.clone(),
        plan.python.clone(),
        plan.destination.clone(),
    )
    .await?;
    if fresh != plan {
        return Err(AppError::msg("源包或解释器已变化，请重新预览"));
    }
    let packages = selected(&fresh, &names)?;
    let destination = PathBuf::from(&fresh.destination);
    std::fs::create_dir(&destination)?;
    let report = destination.join("envcon-reinstall.json");
    let task = Snapshot {
        status: "running".into(),
        message: "正在创建虚拟环境".into(),
        destination: fresh.destination.clone(),
        report: report.to_string_lossy().into(),
        items: packages
            .iter()
            .map(|p| ItemResult {
                name: p.name.clone(),
                version: p.version.clone(),
                status: "pending".into(),
                detail: None,
            })
            .collect(),
        dependency_check: None,
    };
    std::fs::write(&report, serde_json::to_vec_pretty(&task)?)?;
    std::fs::write(
        destination.join("envcon-source.json"),
        serde_json::to_vec_pretty(&fresh)?,
    )?;
    *TASK.lock().unwrap() = Some(task);
    crate::old_packages::remember("pip", crate::old_packages::SourcePlan::Pip(fresh.clone()));
    CANCEL.store(false, Ordering::SeqCst);
    tauri::async_runtime::spawn(async move {
        let _guard = guard;
        if let Err(error) = install(&fresh, &packages).await {
            let message = error.to_string();
            let _ = update(|t| {
                t.status = "error".into();
                t.message = message;
            });
        }
    });
    Ok(())
}

async fn install(plan: &Plan, packages: &[Package]) -> Result<()> {
    let destination = PathBuf::from(&plan.destination);
    run(
        Path::new(&plan.python),
        &["-I", "-m", "venv", &plan.destination],
        180,
    )
    .await?;
    let python = destination.join("Scripts/python.exe");
    let requirements = destination.join("envcon-requirements.txt");
    std::fs::write(
        &requirements,
        packages
            .iter()
            .map(|p| format!("{}=={}\n", p.name, p.version))
            .collect::<String>(),
    )?;
    for (i, package) in packages.iter().enumerate() {
        if CANCEL.load(Ordering::SeqCst) {
            break;
        }
        update(|t| {
            t.message = format!("正在安装 {}=={}", package.name, package.version);
            t.items[i].status = "installing".into();
        })?;
        let pin = format!("{}=={}", package.name, package.version);
        let result = run(
            &python,
            &[
                "-I",
                "-m",
                "pip",
                "install",
                "--no-input",
                "--progress-bar",
                "off",
                "--retries",
                "1",
                "--timeout",
                "30",
                "--constraint",
                &requirements.to_string_lossy(),
                &pin,
            ],
            600,
        )
        .await;
        update(|t| match result {
            Ok(_) => t.items[i].status = "installed".into(),
            Err(e) => {
                t.items[i].status = "failed".into();
                t.items[i].detail = Some(e.to_string());
            }
        })?;
    }
    let installed: Inspection =
        serde_json::from_str(&run(&python, &["-I", "-c", INSPECT], 30).await?)?;
    update(|t| {
        for item in t.items.iter_mut().filter(|p| p.status == "installed") {
            if !installed
                .packages
                .iter()
                .any(|p| normalize(&p.name) == normalize(&item.name) && p.version == item.version)
            {
                item.status = "failed".into();
                item.detail = Some("最终环境未检测到要求的包版本".into());
            }
        }
    })?;
    let check = run(&python, &["-I", "-m", "pip", "check"], 60).await;
    let checked = check.is_ok();
    update(|t| {
        for item in t.items.iter_mut().filter(|p| p.status == "pending") {
            item.status = "canceled".into();
        }
        t.dependency_check = Some(match check {
            Ok(text) => text,
            Err(e) => e.to_string(),
        });
        t.status = if CANCEL.load(Ordering::SeqCst) {
            "canceled"
        } else if checked && t.items.iter().all(|p| p.status == "installed") {
            "done"
        } else {
            "partial"
        }
        .into();
        t.message = "重装结束；原环境保持不变，详细结果已写入报告".into();
    })
}

const CLEANUP_INSPECT: &str = r#"
import csv, io, json, sys, sysconfig, site
from pathlib import Path
from importlib import metadata
roots = {Path(sysconfig.get_path(k)).resolve() for k in ('purelib', 'platlib')}
scripts = Path(sysconfig.get_path('scripts')).resolve()
paths = list(roots)
if sys.prefix == sys.base_prefix:
    user = site.getusersitepackages()
    paths.extend(Path(p).resolve() for p in ([user] if isinstance(user, str) else user))
distributions = list(metadata.distributions(path=list(map(str, dict.fromkeys(paths)))))
records = []
for d in distributions:
    try:
        record = d.read_text('RECORD')
        if record is None:
            raise ValueError('missing RECORD')
        rows = [row for row in csv.reader(io.StringIO(record)) if row]
        if not rows or any(len(row) != 3 or not row[0] for row in rows):
            raise ValueError('invalid RECORD')
        records.append(([Path(d.locate_file(row[0])).resolve() for row in rows], None))
    except (ValueError, OSError, csv.Error):
        records.append(([], '缺少或损坏卸载文件清单 RECORD，无法安全自动卸载'))
owners = {}
for i, (files, _) in enumerate(records):
    for p in files:
        owners.setdefault(str(p).lower(), set()).add(i)
items = []
for i, d in enumerate(distributions):
    location = Path(d.locate_file('')).resolve()
    files, reason = records[i]
    if location not in roots:
        reason = '用户级包不在此解释器的隔离卸载范围，已保留'
    elif reason is None:
        for p in files:
            inside = any(p.is_relative_to(root) and p != root for root in roots)
            script = p.parent == scripts and not (p.name.lower().startswith(('python', 'pip')) and p.suffix.lower() == '.exe')
            if not (inside or script) or len(owners[str(p).lower()]) > 1:
                reason = '卸载清单含越界路径或共享文件，已保留'
                break
    items.append(dict(name=d.metadata.get('Name',''), version=d.version or '', path=str(location), reason=reason))
print(json.dumps(dict(prefix=str(Path(sys.prefix).resolve()), roots=list(map(str, roots)), packages=items)))
"#;

#[derive(Deserialize)]
struct CleanupInspection {
    prefix: String,
    roots: Vec<String>,
    packages: Vec<crate::old_packages::Candidate>,
}

fn cleanup_python(plan: &Plan) -> Result<PathBuf> {
    if plan.source_kind == "python" {
        return python_path(&plan.source);
    }
    let source = Path::new(&plan.source);
    if !source
        .file_name()
        .is_some_and(|n| n.eq_ignore_ascii_case("site-packages"))
    {
        return Err(AppError::msg(
            "旧目录没有可确认的对应解释器，不能自动卸载；请保留旧文件或手动处理",
        ));
    }
    let root = source
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| AppError::msg("无法定位旧解释器"))?;
    for candidate in [root.join("Scripts/python.exe"), root.join("python.exe")] {
        if candidate.is_file() {
            return python_path(&candidate.to_string_lossy());
        }
    }
    Err(AppError::msg(
        "仅有旧 site-packages，没有对应的旧 python.exe；无法安全卸载并清理脚本，未删除任何文件",
    ))
}

async fn inspect_cleanup_source(plan: &Plan) -> Result<(PathBuf, CleanupInspection)> {
    crate::old_packages::verify_separate(Path::new(&plan.source), Path::new(&plan.destination))?;
    let python = cleanup_python(plan)?;
    let inspection: CleanupInspection =
        serde_json::from_str(&run(&python, &["-I", "-c", CLEANUP_INSPECT], 30).await?)?;
    crate::old_packages::verify_separate(
        Path::new(&inspection.prefix),
        Path::new(&plan.destination),
    )?;
    if plan.source_kind == "directory"
        && !inspection
            .roots
            .iter()
            .any(|root| crate::detect::system::paths_eq(Path::new(root), Path::new(&plan.source)))
    {
        return Err(AppError::msg("旧解释器的包目录与重装来源不一致，禁止清理"));
    }
    Ok((python, inspection))
}

pub(crate) async fn cleanup_candidates(
    plan: &Plan,
    items: &[ItemResult],
) -> Result<Vec<crate::old_packages::Candidate>> {
    let (_, old) = inspect_cleanup_source(plan).await?;
    let target_python = Path::new(&plan.destination).join("Scripts/python.exe");
    let target: Inspection =
        serde_json::from_str(&run(&target_python, &["-I", "-c", INSPECT], 30).await?)?;
    if !crate::detect::system::paths_eq(Path::new(&target.prefix), Path::new(&plan.destination)) {
        return Err(AppError::msg("新虚拟环境指向其他解释器环境，禁止清理"));
    }
    run(&target_python, &["-I", "-m", "pip", "check"], 60).await?;
    Ok(items
        .iter()
        .map(|item| {
            let key = normalize(&item.name);
            let matches: Vec<_> = old
                .packages
                .iter()
                .filter(|p| normalize(&p.name) == key)
                .collect();
            let original = plan.packages.iter().find(|p| normalize(&p.name) == key);
            let reason =
                if !original.is_some_and(|p| p.reason.is_none() && p.version == item.version) {
                    Some("不在原可重装清单内".into())
                } else if !target
                    .packages
                    .iter()
                    .any(|p| normalize(&p.name) == key && p.version == item.version)
                {
                    Some("新环境中要求的包版本不存在".into())
                } else if matches.len() > 1 {
                    Some("旧环境存在多份同名包，禁止自动卸载".into())
                } else {
                    match matches.first() {
                        None => Some("旧包已不存在".into()),
                        Some(p) if p.version != item.version => Some("旧包版本已变化".into()),
                        Some(p) => p.reason.clone(),
                    }
                };
            crate::old_packages::Candidate {
                name: item.name.clone(),
                version: item.version.clone(),
                path: matches.first().and_then(|p| p.path.clone()),
                reason,
            }
        })
        .collect())
}

pub(crate) async fn remove_old(
    plan: &Plan,
    package: &crate::old_packages::Candidate,
) -> Result<()> {
    if !index_pin(&package.name, &package.version)
        || matches!(
            normalize(&package.name).as_str(),
            "pip" | "setuptools" | "wheel"
        )
    {
        return Err(AppError::msg("不能卸载此包"));
    }
    let (python, _) = inspect_cleanup_source(plan).await?;
    run(
        &python,
        &["-I", "-m", "pip", "uninstall", "--yes", "--", &package.name],
        180,
    )
    .await?;
    let (_, remaining) = inspect_cleanup_source(plan).await?;
    if remaining
        .packages
        .iter()
        .any(|p| normalize(&p.name) == normalize(&package.name))
    {
        return Err(AppError::msg("卸载命令结束，但旧包仍存在"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_options_paths_and_marks_direct_sources() {
        assert!(!index_pin("--index-url", "1"));
        assert!(!index_pin("foo", "1\n--extra-index-url evil"));
        assert!(index_pin("my_package", "1!2.0+cpu"));
        let p = packages(vec![
            RawPackage {
                name: "local".into(),
                version: "1.0".into(),
                direct: true,
            },
            RawPackage {
                name: "pip".into(),
                version: "25.0".into(),
                direct: false,
            },
        ]);
        assert!(p.iter().all(|p| p.reason.is_some()));
    }
    #[test]
    fn rejects_duplicate_distribution_metadata_and_existing_destination() {
        let p = packages(vec![
            RawPackage {
                name: "a_b".into(),
                version: "1".into(),
                direct: false,
            },
            RawPackage {
                name: "a-b".into(),
                version: "2".into(),
                direct: false,
            },
        ]);
        assert_eq!(p.len(), 1);
        assert!(p[0].reason.is_some());
        let dir = crate::test_support::TestDir::new();
        assert!(destination_path(&dir.path().to_string_lossy()).is_err());
        assert!(destination_path(&dir.path().join("new").to_string_lossy()).is_ok());
    }

    #[tokio::test]
    #[ignore = "requires ENVCON_TEST_PYTHON; creates only temporary venvs and installs an offline fixture wheel"]
    async fn offline_python_reads_old_metadata_and_installs_into_new_venv() {
        let python =
            PathBuf::from(std::env::var("ENVCON_TEST_PYTHON").expect("set ENVCON_TEST_PYTHON"));
        let dir = crate::test_support::TestDir::new();
        let old = dir.path().join("site-packages");
        std::fs::create_dir_all(old.join("envcon_fixture-1.0.dist-info")).unwrap();
        std::fs::write(
            old.join("envcon_fixture-1.0.dist-info/METADATA"),
            "Metadata-Version: 2.1\nName: envcon-fixture\nVersion: 1.0\n",
        )
        .unwrap();
        let destination = dir.path().join("new-venv");
        let plan = preview(
            old.to_string_lossy().into(),
            "directory".into(),
            python.to_string_lossy().into(),
            destination.to_string_lossy().into(),
        )
        .await
        .unwrap();
        assert_eq!(
            plan.packages,
            vec![Package {
                name: "envcon-fixture".into(),
                version: "1.0".into(),
                reason: None
            }]
        );
        assert!(selected(&plan, &["--bad".into()]).is_err());
        run(&python, &["-I", "-m", "venv", &plan.destination], 180)
            .await
            .unwrap();
        let wheel = dir.path().join("envcon_fixture-1.0-py3-none-any.whl");
        let build_wheel = r#"
import zipfile, sys
with zipfile.ZipFile(sys.argv[1], 'w') as z:
    z.writestr('envcon_fixture.py', 'VALUE = 42\n')
    z.writestr('envcon_fixture-1.0.dist-info/METADATA', 'Metadata-Version: 2.1\nName: envcon-fixture\nVersion: 1.0\n')
    z.writestr('envcon_fixture-1.0.dist-info/WHEEL', 'Wheel-Version: 1.0\nGenerator: envcon-test\nRoot-Is-Purelib: true\nTag: py3-none-any\n')
    names = z.namelist() + ['envcon_fixture-1.0.dist-info/RECORD']
    z.writestr('envcon_fixture-1.0.dist-info/RECORD', ''.join(name + ',,\n' for name in names))
"#;
        run(
            &python,
            &["-I", "-c", build_wheel, &wheel.to_string_lossy()],
            30,
        )
        .await
        .unwrap();
        let target = destination.join("Scripts/python.exe");
        run(
            &target,
            &[
                "-I",
                "-m",
                "pip",
                "install",
                "--no-index",
                "--no-deps",
                &wheel.to_string_lossy(),
            ],
            60,
        )
        .await
        .unwrap();
        let installed: Inspection =
            serde_json::from_str(&run(&target, &["-I", "-c", INSPECT], 30).await.unwrap()).unwrap();
        assert!(installed
            .packages
            .iter()
            .any(|p| p.name == "envcon-fixture" && p.version == "1.0"));
        run(
            &target,
            &[
                "-I",
                "-c",
                "import envcon_fixture; assert envcon_fixture.VALUE == 42",
            ],
            30,
        )
        .await
        .unwrap();
        run(&target, &["-I", "-m", "pip", "check"], 30)
            .await
            .unwrap();
        assert!(old.join("envcon_fixture-1.0.dist-info/METADATA").is_file());
        assert!(cleanup_python(&plan).is_err());
        let source_venv = dir.path().join("source-venv");
        run(
            &python,
            &["-I", "-m", "venv", &source_venv.to_string_lossy()],
            180,
        )
        .await
        .unwrap();
        let source_python = source_venv.join("Scripts/python.exe");
        run(
            &source_python,
            &[
                "-I",
                "-m",
                "pip",
                "install",
                "--no-index",
                "--no-deps",
                &wheel.to_string_lossy(),
            ],
            60,
        )
        .await
        .unwrap();
        let source_plan = Plan {
            source: source_python.to_string_lossy().into(),
            source_kind: "python".into(),
            ..plan.clone()
        };
        let items = vec![ItemResult {
            name: "envcon-fixture".into(),
            version: "1.0".into(),
            status: "installed".into(),
            detail: None,
        }];
        let candidates = cleanup_candidates(&source_plan, &items).await.unwrap();
        assert!(candidates[0].reason.is_none(), "{:?}", candidates);
        let record = source_venv.join("Lib/site-packages/envcon_fixture-1.0.dist-info/RECORD");
        let saved_record = std::fs::read_to_string(&record).unwrap();
        std::fs::write(
            &record,
            format!("{saved_record}\n../../../../outside.txt,,\n"),
        )
        .unwrap();
        assert!(cleanup_candidates(&source_plan, &items).await.unwrap()[0]
            .reason
            .as_ref()
            .unwrap()
            .contains("越界"));
        std::fs::write(&record, saved_record).unwrap();
        remove_old(&source_plan, &candidates[0]).await.unwrap();
        run(
            &source_python,
            &[
                "-I",
                "-c",
                "import importlib.util; assert importlib.util.find_spec('envcon_fixture') is None",
            ],
            30,
        )
        .await
        .unwrap();
        assert!(source_python.is_file());
        run(&source_python, &["-I", "-m", "pip", "--version"], 30)
            .await
            .unwrap();
        run(
            &target,
            &[
                "-I",
                "-c",
                "import envcon_fixture; assert envcon_fixture.VALUE == 42",
            ],
            30,
        )
        .await
        .unwrap();
        assert!(cleanup_candidates(&source_plan, &items).await.unwrap()[0]
            .reason
            .is_some());
        println!("pip: old fixture removed, interpreter/pip/new fixture retained; detached directory rejected");
    }
}
