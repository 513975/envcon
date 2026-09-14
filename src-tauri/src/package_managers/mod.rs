//! Shared manager capabilities; adapter-specific layouts and commands live in child modules.
mod native;
pub(crate) mod node;
mod types;
pub use types::{ItemResult, Package};
pub mod commands;
pub use native::{
    bin_env, bin_path, cache_env, configured_env_path, default_source, detect, global_env,
    installation_path, inventory, verify_cleanup_files,
};

use crate::error::{AppError, Result};
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Family {
    Node,
    Python,
    Native,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ReinstallKind {
    Global,
    Venv,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum AdapterKind {
    NodeCli,
    PythonPip,
    Isolated,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Definition {
    pub id: String,
    pub name: String,
    pub family: Family,
    pub adapter: AdapterKind,
    pub reinstall: ReinstallKind,
    pub global_config: bool,
    pub cache_config: bool,
    pub migrate_global: bool,
    pub project_query: bool,
}

pub fn definitions() -> &'static [Definition] {
    static CATALOG: OnceLock<Vec<Definition>> = OnceLock::new();
    CATALOG.get_or_init(|| {
        serde_json::from_str(include_str!("../../../src/lib/managerCatalog.json"))
            .expect("invalid built-in manager catalog")
    })
}

pub fn definition(id: &str) -> Result<&'static Definition> {
    definitions()
        .iter()
        .find(|entry| entry.id == id)
        .ok_or_else(|| AppError::msg(format!("未知包管理器: {id}")))
}

/// IPC values are validated once before entering a manager operation.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(try_from = "String", into = "String")]
pub struct ManagerId(String);
impl TryFrom<String> for ManagerId {
    type Error = AppError;
    fn try_from(value: String) -> Result<Self> {
        definition(&value)?;
        Ok(Self(value))
    }
}
impl From<ManagerId> for String {
    fn from(value: ManagerId) -> Self {
        value.0
    }
}
impl AsRef<str> for ManagerId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

pub fn isolated(tool: &str) -> bool {
    definition(tool).is_ok_and(|entry| entry.adapter == AdapterKind::Isolated)
}
pub fn node(tool: &str) -> bool {
    definition(tool).is_ok_and(|entry| entry.family == Family::Node)
}

pub fn installed_packages(
    tool: &str,
    source: &std::path::Path,
) -> Result<Vec<(Package, Option<std::path::PathBuf>)>> {
    definition(tool)?;
    if node(tool) {
        return node::installed_packages(tool, source);
    }
    Ok(inventory(tool, source)?
        .into_iter()
        .map(|mut package| {
            let path = installation_path(tool, source, &package.name);
            if let Err(error) = &path {
                package.reason = Some(error.to_string());
            }
            (package, path.ok())
        })
        .collect())
}

pub fn configuration(
    tool: &str,
    global: Option<&str>,
    cache: Option<&str>,
) -> Result<Vec<(String, String)>> {
    let definition = definition(tool)?;
    if global.is_some() && !definition.global_config || cache.is_some() && !definition.cache_config
    {
        return Err(AppError::msg("管理器不支持此路径设置"));
    }
    let mut changes = Vec::new();
    if let Some(path) = global {
        changes.push((
            global_env(tool)
                .ok_or_else(|| AppError::msg("没有全局路径适配器"))?
                .into(),
            path.into(),
        ));
        if let Some(key) = bin_env(tool) {
            changes.push((
                key.into(),
                bin_path(tool, std::path::Path::new(path))
                    .to_string_lossy()
                    .into(),
            ));
        }
    }
    if let Some(path) = cache {
        changes.push((
            cache_env(tool)
                .ok_or_else(|| AppError::msg("没有缓存路径适配器"))?
                .into(),
            path.into(),
        ));
    }
    Ok(changes)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn catalog_is_unique_and_identifiers_are_validated() {
        let mut seen = std::collections::HashSet::new();
        for item in definitions() {
            assert!(seen.insert(&item.id));
            assert!(item.id.bytes().all(|c| c.is_ascii_lowercase()));
            assert!(!item.name.is_empty());
            assert!(ManagerId::try_from(item.id.clone()).is_ok());
        }
        assert!(ManagerId::try_from(String::from("../untrusted")).is_err());
    }

    #[test]
    fn capabilities_keep_install_roots_separate_from_runtime_homes() {
        assert!(configuration("cargo", None, Some("D:/cache")).is_err());
        assert!(configuration("dotnet", Some("D:/tools"), None).is_err());
        assert!(configuration("pip", Some("D:/tools"), None).is_err());
        assert_eq!(
            configuration("cargo", Some("D:/tools"), None).unwrap(),
            vec![("CARGO_INSTALL_ROOT".into(), "D:/tools".into())]
        );
        for manager in definitions()
            .iter()
            .filter(|m| m.adapter == AdapterKind::Isolated)
        {
            if manager.global_config {
                assert!(global_env(&manager.id).is_some());
            }
            if manager.cache_config {
                assert!(cache_env(&manager.id).is_some());
            }
        }
    }
}
