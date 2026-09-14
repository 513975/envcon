use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};

use winreg::{enums::*, RegKey, HKEY};

pub struct Discovery {
    pub environment: HashMap<String, String>,
    pub path: Vec<PathBuf>,
    pub directories: Vec<(PathBuf, String)>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug)]
struct RegistryVar {
    value: String,
    expandable: bool,
}

const PERSISTED_ROOT_VARS: &[&str] = &[
    "JAVA_HOME", "GOROOT", "MAVEN_HOME", "GRADLE_HOME", "CARGO_HOME", "RUSTUP_HOME",
    "VIRTUAL_ENV", "CONDA_PREFIX", "PYTHON_HOME", "PNPM_HOME", "NVM_HOME", "NVM_SYMLINK",
    "SCOOP", "PHP_HOME", "ZIG_HOME", "LLVM_HOME",
];

/// 获取注册表合并后的当前有效环境，用于缓存和工具配置读取。
pub fn effective_environment() -> HashMap<String, String> {
    discover(None, &[]).environment
}

pub fn expand(value: &str, vars: &HashMap<String, String>) -> String {
    let mut current = value.to_owned();
    for _ in 0..16 {
        let mut result = String::new();
        let mut rest = current.as_str();
        while let Some(start) = rest.find('%') {
            result.push_str(&rest[..start]);
            let after = &rest[start + 1..];
            let Some(end) = after.find('%') else { result.push_str(&rest[start..]); rest = ""; break };
            let key = after[..end].to_uppercase();
            match vars.get(&key) {
                Some(value) => result.push_str(value),
                None => result.push_str(&rest[start..start + end + 2]),
            }
            rest = &after[end + 1..];
        }
        result.push_str(rest);
        if result == current { return result; }
        current = result;
    }
    current
}

fn registry_vars(hive: HKEY, key: &str) -> std::io::Result<HashMap<String, RegistryVar>> {
    let key = RegKey::predef(hive).open_subkey_with_flags(key, KEY_READ)?;
    let mut vars = HashMap::new();
    for entry in key.enum_values() {
        let (name, value) = entry?;
        if !matches!(value.vtype, REG_SZ | REG_EXPAND_SZ) { continue; }
        let units: Vec<u16> = value.bytes.chunks_exact(2)
            .map(|b| u16::from_le_bytes([b[0], b[1]])).collect();
        vars.insert(name.to_uppercase(), RegistryVar {
            value: String::from_utf16_lossy(&units).trim_end_matches('\0').into(),
            expandable: value.vtype == REG_EXPAND_SZ,
        });
    }
    Ok(vars)
}

fn apply_registry_layer(environment: &mut HashMap<String, String>, vars: &HashMap<String, RegistryVar>) {
    for (name, var) in vars.iter().filter(|(name, _)| name.as_str() != "PATH") {
        environment.insert(name.clone(), var.value.clone());
    }
    let context = environment.clone();
    for (name, var) in vars.iter().filter(|(name, var)| name.as_str() != "PATH" && var.expandable) {
        environment.insert(name.clone(), expand(&var.value, &context));
    }
}

fn registry_path(vars: &HashMap<String, RegistryVar>, environment: &HashMap<String, String>) -> String {
    vars.get("PATH").map(|var| {
        if var.expandable { expand(&var.value, environment) } else { var.value.clone() }
    }).unwrap_or_default()
}

pub fn discover(managed: Option<&Path>, projects: &[PathBuf]) -> Discovery {
    let mut environment: HashMap<String, String> = std::env::vars()
        .map(|(key, value)| (key.to_uppercase(), value)).collect();
    let mut warnings = Vec::new();
    let system = registry_vars(HKEY_LOCAL_MACHINE, r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment");
    let user = registry_vars(HKEY_CURRENT_USER, "Environment");
    let path = match (system, user) {
        (Ok(system), Ok(user)) => {
            apply_registry_layer(&mut environment, &system);
            let system_path = registry_path(&system, &environment);
            apply_registry_layer(&mut environment, &user);
            // 环境变量来自启动进程快照；删除注册表变量后，不能继续把旧值当成有效安装根。
            for &name in PERSISTED_ROOT_VARS {
                if !system.contains_key(name) && !user.contains_key(name) { environment.remove(name); }
            }
            environment.insert("PATH".into(), system_path.clone());
            let user_path = registry_path(&user, &environment);
            [system_path, user_path].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(";")
        }
        _ => {
            warnings.push("无法读取注册表 PATH，已回退到应用启动时的 PATH，首选标记可能过期".into());
            environment.get("PATH").cloned().unwrap_or_default()
        }
    };
    environment.insert("PATH".into(), path.clone());
    let mut result = Discovery { environment, path: Vec::new(), directories: Vec::new(), warnings };
    // Retain discoverability of tools inherited from the launcher without calling them PATH defaults.
    for dir in std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()) {
        if dir.is_absolute() && dir.is_dir() { result.directories.push((dir, "进程 PATH".into())); }
    }
    for entry in path.split(';').map(|p| p.trim().trim_matches('"')).filter(|p| !p.is_empty()) {
        let p = PathBuf::from(entry);
        if !p.is_absolute() || entry.contains('%') {
            result.warnings.push(format!("跳过无法解析的 PATH 条目: {entry}"));
            continue;
        }
        if !result.path.iter().any(|old| key(old) == key(&p)) { result.path.push(p); }
    }

    let home = dirs::home_dir().unwrap_or_default();
    let mut roots = Vec::new();
    for (var, children) in [
        ("ProgramFiles", vec!["nodejs", "Python", "Java", "Eclipse Adoptium", "Go", "Git", "LLVM", "CMake", "dotnet"]),
        ("ProgramFiles(x86)", vec!["Python", "Java", "Git"]),
        ("LOCALAPPDATA", vec!["Programs/Python", "Programs/nodejs", "pnpm", "uv"]),
        ("APPDATA", vec!["npm", "Python", "Composer", "pypoetry"]),
        ("ProgramData", vec!["chocolatey/bin", "ComposerSetup"]),
    ] {
        if let Some(base) = result.environment.get(&var.to_uppercase()) {
            roots.extend(children.into_iter().map(|child| (PathBuf::from(base).join(child), 3)));
        }
    }
    for child in [".cargo/bin", ".local/bin", ".bun/bin", ".deno/bin", ".volta/bin", "scoop/shims", "scoop/apps", ".virtualenvs", ".conda/envs", "miniconda3", "anaconda3", "miniforge3", "mambaforge"] {
        roots.push((home.join(child), if child == "scoop/apps" { 4 } else { 3 }));
    }
    for var in ["JAVA_HOME", "GOROOT", "MAVEN_HOME", "GRADLE_HOME", "CARGO_HOME", "RUSTUP_HOME", "VIRTUAL_ENV", "CONDA_PREFIX", "PYTHON_HOME", "PNPM_HOME", "NVM_HOME", "NVM_SYMLINK", "SCOOP", "PHP_HOME", "ZIG_HOME", "LLVM_HOME"] {
        if let Some(value) = result.environment.get(var) { roots.push((PathBuf::from(value), 3)); }
    }
    // Conda records all known environments, including custom locations.
    match std::fs::read_to_string(home.join(".conda/environments.txt")) {
        Ok(contents) => roots.extend(contents.lines().map(str::trim).filter(|s| !s.is_empty()).map(|s| (PathBuf::from(s), 2))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => result.warnings.push(format!("读取 Conda 环境记录失败: {e}")),
    }
    roots.extend(python_registry_roots(&mut result.warnings).into_iter().map(|p| (p, 2)));
    let mut seen_roots = HashSet::new();
    for (root, depth) in roots {
        if seen_roots.insert(key(&root)) { result.walk(&root, depth, "常见安装", false); }
    }
    if let Some(root) = managed {
        // Follow explicit managed environment roots, including integrated junctions.
        for kind in crate::types::ALL_ENV_TYPES {
            result.walk(&root.join("current").join(kind.junction()), 3, "EnvCon", false);
            if let Ok(entries) = std::fs::read_dir(root.join("envs").join(kind.folder())) {
                for entry in entries.flatten().filter(|e| !e.file_name().to_string_lossy().starts_with('.')) {
                    result.walk(&entry.path(), 3, "EnvCon", false);
                }
            }
        }
    }
    for project in projects { result.walk(project, 6, "项目目录", true); }
    result
}

pub fn key(path: &Path) -> String {
    path.to_string_lossy().replace('/', "\\").trim_end_matches('\\').to_lowercase()
}

fn python_registry_roots(warnings: &mut Vec<String>) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    for hive in [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE] {
        for view in [KEY_WOW64_64KEY, KEY_WOW64_32KEY] {
            let base = match RegKey::predef(hive).open_subkey_with_flags(r"Software\Python", KEY_READ | view) {
                Ok(base) => base,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => { warnings.push(format!("读取 Python 注册信息失败: {e}")); continue; }
            };
            for company in base.enum_keys().flatten() {
                let Ok(company) = base.open_subkey(company) else { continue };
                for tag in company.enum_keys().flatten() {
                    let Ok(install) = company.open_subkey(format!(r"{tag}\InstallPath")) else { continue };
                    if let Ok(path) = install.get_value::<String, _>("") { roots.push(path.into()); }
                }
            }
        }
    }
    roots
}

impl Discovery {
    pub fn walk(&mut self, root: &Path, max_depth: usize, source: &str, required: bool) {
        if !root.is_dir() {
            if required { self.warnings.push(format!("扫描目录不存在或不可访问: {}", root.display())); }
            return;
        }
        let mut pending = VecDeque::from([(root.to_path_buf(), 0)]);
        let mut visited = HashSet::new();
        let mut count = 0;
        while let Some((dir, depth)) = pending.pop_front() {
            let resolved = dir.canonicalize().unwrap_or_else(|_| dir.clone());
            if !visited.insert(key(&resolved)) { continue; }
            count += 1;
            if count > 2000 { self.warnings.push(format!("目录扫描达到 2000 项上限: {}", root.display())); break; }
            if !self.directories.iter().any(|(p, _)| key(p) == key(&dir)) {
                self.directories.push((dir.clone(), source.into()));
            }
            let entries = match std::fs::read_dir(&dir) {
                Ok(entries) => entries,
                Err(e) => { self.warnings.push(format!("无法读取 {}: {e}", dir.display())); continue; }
            };
            let mut depth_limited = false;
            for entry in entries {
                let entry = match entry { Ok(e) => e, Err(e) => { self.warnings.push(format!("读取目录项失败: {e}")); continue; } };
                let name = entry.file_name().to_string_lossy().to_lowercase();
                if matches!(name.as_str(), "node_modules" | ".git" | "target" | "dist" | "lib" | "libs" | "include" | "pkgs" | "cache" | "caches" | "share" | "docs" | "store" | "registry" | "buckets" | "etc" | "conf" | "shared")
                    || name.starts_with(".failed-") || name.starts_with(".tmp-") { continue; }
                let Ok(meta) = std::fs::symlink_metadata(entry.path()) else { continue };
                if meta.file_type().is_symlink() {
                    if required && entry.path().is_dir() {
                        self.warnings.push(format!("已跳过链接目录，可单独选择扫描: {}", entry.path().display()));
                    }
                    continue;
                }
                if meta.is_dir() {
                    if depth < max_depth { pending.push_back((entry.path(), depth + 1)); }
                    else { depth_limited = true; }
                }
            }
            if depth_limited {
                self.warnings.push(format!("已达到扫描深度 {max_depth}: {}", dir.display()));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty() -> Discovery {
        Discovery { environment: HashMap::new(), path: vec![], directories: vec![], warnings: vec![] }
    }

    #[test]
    fn expands_fresh_nested_variables_case_insensitively_without_looping() {
        let mut vars = HashMap::from([
            ("TOOLS".into(), r"D:\new-tools".into()),
            ("NPM_HOME".into(), r"%tools%\npm".into()),
        ]);
        assert_eq!(expand("%npm_home%;%UNKNOWN%", &vars), r"D:\new-tools\npm;%UNKNOWN%");
        vars.insert("LOOP".into(), "%LOOP%".into());
        assert_eq!(expand("%LOOP%", &vars), "%LOOP%");
    }

    #[test]
    fn expands_only_registry_expand_strings() {
        let vars = HashMap::from([
            ("BASE".into(), RegistryVar { value: r"D:\tools".into(), expandable: false }),
            ("LITERAL".into(), RegistryVar { value: r"%BASE%\literal".into(), expandable: false }),
            ("EXPANDED".into(), RegistryVar { value: r"%BASE%\expanded".into(), expandable: true }),
        ]);
        let mut environment = HashMap::new();
        apply_registry_layer(&mut environment, &vars);
        assert_eq!(environment["LITERAL"], r"%BASE%\literal");
        assert_eq!(environment["EXPANDED"], r"D:\tools\expanded");
    }

    #[test]
    fn project_walk_discovers_venv_and_conda_without_entering_dependencies_or_links() {
        let dir = crate::test_support::TestDir::new();
        for sub in ["project/.venv/Scripts", "project/conda-env/conda-meta", "project/conda-env/Scripts", "project/node_modules/dependency"] {
            std::fs::create_dir_all(dir.path().join(sub)).unwrap();
        }
        let project = dir.path().join("project");
        junction::create(&project, project.join("loop")).unwrap();
        let mut scan = empty();
        scan.walk(&project, 6, "项目目录", true);
        assert!(scan.directories.iter().any(|(p,_)| p.ends_with(".venv/Scripts")));
        assert!(scan.directories.iter().any(|(p,_)| p.ends_with("conda-env/Scripts")));
        assert!(!scan.directories.iter().any(|(p,_)| p.ends_with("dependency")));
        assert!(scan.warnings.iter().any(|s| s.contains("链接目录")));
        let mut shallow = empty();
        shallow.walk(&project, 0, "项目目录", true);
        assert!(shallow.warnings.iter().any(|s| s.contains("扫描深度")));
    }

    #[test]
    fn missing_selected_root_is_reported() {
        let dir = crate::test_support::TestDir::new();
        let mut scan = empty();
        scan.walk(&dir.path().join("missing"), 6, "项目目录", true);
        assert_eq!(scan.warnings.len(), 1);
    }
}
