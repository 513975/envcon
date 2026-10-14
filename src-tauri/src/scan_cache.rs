use std::collections::{BTreeMap, HashMap};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{AppError, Result};
use crate::singleflight::SingleFlight;

const FORMAT: u32 = 1;
pub const RECHECK_MS: u64 = 30_000;

pub fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub value: Value,
    pub updated_at: u64,
    pub duration_ms: u64,
    pub fingerprint: String,
    pub generation: u64,
    #[serde(default)]
    pub warning: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct DiskCache {
    format: u32,
    app_version: String,
    entries: HashMap<String, Snapshot>,
}

#[derive(Default)]
struct CacheState {
    entries: HashMap<String, Snapshot>,
    generation: u64,
    mutations: usize,
}

pub struct ScanCache {
    state: Mutex<CacheState>,
    pending: SingleFlight<(String, u64), std::result::Result<Snapshot, String>>,
    path: PathBuf,
    /// Only one background scan group at a time; version probes retain their own bounded concurrency.
    work: tokio::sync::Semaphore,
}

impl ScanCache {
    pub fn new(data_dir: &Path) -> Arc<Self> {
        let path = data_dir.join("scan-cache-v1.json");
        let entries = std::fs::metadata(&path).ok().filter(|m| m.len() <= 8 * 1024 * 1024)
            .and_then(|_| std::fs::read(&path).ok())
            .and_then(|b| serde_json::from_slice::<DiskCache>(&b).ok())
            .filter(|d| d.format == FORMAT && d.app_version == env!("CARGO_PKG_VERSION") && d.entries.len() <= 32)
            .map(|d| d.entries).unwrap_or_default();
        Arc::new(Self { state: Mutex::new(CacheState { entries, ..Default::default() }), pending: Default::default(), path, work: tokio::sync::Semaphore::new(1) })
    }

    pub fn peek(&self, key: &str) -> Option<Snapshot> {
        let state = self.state.lock().unwrap();
        if state.mutations > 0 { return None; }
        state.entries.get(key).cloned()
    }

    pub async fn read<F, Fut>(&self, key: String, fingerprint: String, force: bool, read: F) -> Result<Snapshot>
    where F: FnOnce() -> Fut, Fut: std::future::Future<Output = Result<Value>> {
        let generation = {
            let state = self.state.lock().unwrap();
            if state.mutations > 0 { return Err(AppError::msg("环境正在变更，请稍后刷新扫描")); }
            if !force {
                if let Some(old) = state.entries.get(&key).filter(|s| s.fingerprint == fingerprint && s.updated_at <= now_ms() && now_ms().saturating_sub(s.updated_at) < RECHECK_MS) {
                    return Ok(old.clone());
                }
            }
            state.generation
        };
        self.pending.read((key.clone(), generation), false, || async {
            let _permit = self.work.acquire().await.map_err(|e| e.to_string())?;
            {
                let state = self.state.lock().unwrap();
                if state.generation != generation || state.mutations > 0 { return Err("扫描输入已改变，请刷新".into()); }
            }
            let start = std::time::Instant::now();
            let value = read().await.map_err(|e| e.to_string())?;
            let mut snapshot = Snapshot { value, updated_at: now_ms(), duration_ms: start.elapsed().as_millis() as u64, fingerprint, generation, warning: None };
            let mut state = self.state.lock().unwrap();
            if state.generation != generation || state.mutations > 0 { return Err("扫描期间环境发生变更，请刷新".into()); }
            if state.entries.len() >= 24 && !state.entries.contains_key(&key) {
                if let Some(oldest) = state.entries.iter().min_by_key(|(_, v)| v.updated_at).map(|(k, _)| k.clone()) { state.entries.remove(&oldest); }
            }
            state.entries.insert(key.clone(), snapshot.clone());
            if let Err(e) = self.persist(&state) {
                snapshot.warning = Some(format!("扫描结果可用，但无法保存启动缓存: {e}"));
                state.entries.insert(key, snapshot.clone());
            }
            Ok(snapshot)
        }).await.map_err(AppError::msg)
    }

    fn persist(&self, state: &CacheState) -> Result<()> {
        let bytes = serde_json::to_vec(&DiskCache { format: FORMAT, app_version: env!("CARGO_PKG_VERSION").into(), entries: state.entries.clone() })?;
        crate::config::atomic_write(&self.path, &bytes)
    }

    pub fn mutation(&self) -> Mutation<'_> {
        let mut state = self.state.lock().unwrap();
        state.generation += 1;
        state.mutations += 1;
        state.entries.clear();
        // A tombstone prevents the previous process snapshot from returning after restart.
        if self.persist(&state).is_err() { let _ = std::fs::remove_file(&self.path); }
        self.pending.invalidate();
        Mutation(self)
    }

    pub fn invalidate(&self) { drop(self.mutation()); }

    pub fn update_fingerprint(&self, key: &str, updated_at: u64, fingerprint: String) {
        let mut state = self.state.lock().unwrap();
        if let Some(snapshot) = state.entries.get_mut(key).filter(|s| s.updated_at == updated_at && s.fingerprint != fingerprint) {
            snapshot.fingerprint = fingerprint;
            let _ = self.persist(&state);
        }
    }
}

pub struct Mutation<'a>(&'a ScanCache);
impl Drop for Mutation<'_> {
    fn drop(&mut self) {
        let mut state = self.0.state.lock().unwrap();
        state.generation += 1;
        state.mutations -= 1;
        state.entries.clear();
        self.0.pending.invalidate();
    }
}

pub fn key(kind: &str, root: Option<&Path>, projects: &[PathBuf]) -> String {
    serde_json::to_string(&(kind, root, projects)).expect("scan key is serializable")
}

/// Hash input values, executable metadata and configuration contents, never store environment values.
pub fn fingerprint(root: Option<&Path>, projects: &[PathBuf], config: &crate::config::Settings, previous: Option<&Value>) -> String {
    let environment: BTreeMap<_, _> = crate::detect::discovery::effective_environment().into_iter().collect();
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    environment.hash(&mut hasher);
    // Value sorts object keys, including Settings HashMaps, across process restarts.
    serde_json::to_vec(&serde_json::to_value(config).unwrap_or_default()).unwrap_or_default().hash(&mut hasher);
    let mut paths = Vec::new();
    if let Some(root) = root {
        paths.push(root.to_path_buf());
        for kind in crate::types::ALL_ENV_TYPES {
            paths.push(root.join("current").join(kind.junction()));
            paths.push(root.join("envs").join(kind.folder()));
        }
    }
    paths.extend(projects.iter().cloned());
    fn collect(value: &Value, paths: &mut Vec<PathBuf>) {
        match value {
            Value::Object(map) => for (key, value) in map {
                if matches!(key.as_str(), "path" | "executablePath" | "identityPath" | "installRoot") {
                    if let Some(path) = value.as_str().map(PathBuf::from).filter(|p| p.is_absolute()) { paths.push(path); }
                } else { collect(value, paths); }
            },
            Value::Array(items) => for item in items { collect(item, paths); },
            _ => {}
        }
    }
    if let Some(value) = previous { collect(value, &mut paths); }
    if let Some(home) = dirs::home_dir() {
        for name in [".npmrc", ".yarnrc", ".yarnrc.yml", "pip/pip.ini", ".config/pip/pip.conf", "AppData/Roaming/pip/pip.ini", ".cargo/config.toml", ".rustup/settings.toml"] { paths.push(home.join(name)); }
    }
    paths.sort(); paths.dedup();
    for path in paths {
        path.hash(&mut hasher);
        match std::fs::metadata(&path) {
            Ok(meta) => {
                meta.len().hash(&mut hasher);
                meta.modified().ok().hash(&mut hasher);
                path.canonicalize().ok().hash(&mut hasher);
                if meta.is_dir() {
                    // Probe files as well as parent timestamps. Unknown deeper installs are caught by the periodic full rescan.
                    for relative in ["node.exe", "python.exe", "bin/java.exe", "bin/rustc.exe", "bin/cargo.exe", "bin/go.exe", "cmd/git.exe", "adb.exe", "source.properties", "AdbWinApi.dll", "AdbWinUsbApi.dll", "release", "VERSION", "pyvenv.cfg", ".npmrc", "package.json"] {
                        let child = path.join(relative);
                        if let Ok(m) = std::fs::metadata(&child) { relative.hash(&mut hasher); m.len().hash(&mut hasher); m.modified().ok().hash(&mut hasher); }
                    }
                } else if meta.len() <= 1024 * 1024 && !path.extension().is_some_and(|e| e.eq_ignore_ascii_case("exe")) {
                    if let Ok(bytes) = std::fs::read(&path) { bytes.hash(&mut hasher); }
                }
            }
            Err(e) => e.kind().hash(&mut hasher),
        }
    }
    format!("{:016x}", hasher.finish())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    #[tokio::test]
    async fn persistent_cache_reuses_then_rechecks_changed_inputs() {
        let dir = crate::test_support::TestDir::new();
        let cache = ScanCache::new(dir.path());
        cache.read("a".into(), "v1".into(), false, || async { Ok(serde_json::json!(1)) }).await.unwrap();
        let restarted = ScanCache::new(dir.path());
        assert_eq!(restarted.peek("a").unwrap().value, 1);
        assert_eq!(restarted.read("a".into(), "v1".into(), false, || async { panic!("warm cache should reuse") }).await.unwrap().value, 1);
        assert_eq!(restarted.read("a".into(), "v2".into(), false, || async { Ok(serde_json::json!(2)) }).await.unwrap().value, 2);
        restarted.state.lock().unwrap().entries.get_mut("a").unwrap().updated_at = 0;
        assert_eq!(restarted.read("a".into(), "v2".into(), false, || async { Ok(serde_json::json!(3)) }).await.unwrap().value, 3);
        restarted.invalidate();
        assert!(ScanCache::new(dir.path()).peek("a").is_none());
        std::fs::write(&restarted.path, br#"{"format":99}"#).unwrap();
        assert!(ScanCache::new(dir.path()).peek("a").is_none());
    }

    #[tokio::test]
    async fn concurrent_force_shares_and_mutation_rejects_obsolete_result() {
        let dir = crate::test_support::TestDir::new();
        let cache = ScanCache::new(dir.path());
        let calls = std::sync::atomic::AtomicUsize::new(0);
        let read = || async { calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst); tokio::time::sleep(Duration::from_millis(10)).await; Ok(serde_json::json!(1)) };
        let (a, b) = tokio::join!(cache.read("a".into(), "x".into(), true, read), cache.read("a".into(), "x".into(), true, read));
        assert!(a.is_ok() && b.is_ok());
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
        let c = cache.clone();
        assert!(cache.read("a".into(), "x".into(), true, || async move { c.invalidate(); Ok(Value::Null) }).await.is_err());
        assert!(cache.peek("a").is_none());
    }

    #[test]
    fn child_binary_and_config_content_changes_invalidate_fingerprint() {
        let dir = crate::test_support::TestDir::new();
        let settings = crate::config::Settings::default();
        let value = serde_json::json!({"path":dir.path()});
        let before = fingerprint(None, &[], &settings, Some(&value));
        std::fs::write(dir.path().join("node.exe"), b"v1").unwrap();
        let after = fingerprint(None, &[], &settings, Some(&value));
        assert_ne!(before, after);
        std::fs::write(dir.path().join("node.exe"), b"v222").unwrap();
        assert_ne!(after, fingerprint(None, &[], &settings, Some(&value)));
        std::fs::remove_file(dir.path().join("node.exe")).unwrap();
        assert_ne!(after, fingerprint(None, &[], &settings, Some(&value)));
        let other = dir.path().join("project");
        assert_ne!(before, fingerprint(None, &[other], &settings, Some(&value)));
        let mut first = settings.clone();
        first.mirrors.insert("node".into(), "official".into());
        first.mirrors.insert("jdk".into(), "mirror".into());
        let mut second = settings.clone();
        second.mirrors.insert("jdk".into(), "mirror".into());
        second.mirrors.insert("node".into(), "official".into());
        assert_eq!(fingerprint(None, &[], &first, None), fingerprint(None, &[], &second, None));
        assert_ne!(fingerprint(None, &[], &settings, None), fingerprint(None, &[], &first, None));
    }
}
