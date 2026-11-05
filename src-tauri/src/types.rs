use serde::{Deserialize, Serialize};

/// 环境类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EnvType {
    Jdk,
    Python,
    Node,
    Go,
    Rust,
    Maven,
    Gradle,
    Php,
    Llvm,
    Zig,
    Deno,
    Bun,
    Git,
    Gh,
    Mingw,
    Adb,
    DotNet,
    PowerShell,
}

pub const ALL_ENV_TYPES: [EnvType; 18] = [
    EnvType::Jdk,
    EnvType::Python,
    EnvType::Node,
    EnvType::Go,
    EnvType::Rust,
    EnvType::Maven,
    EnvType::Gradle,
    EnvType::Php,
    EnvType::Llvm,
    EnvType::Zig,
    EnvType::Deno,
    EnvType::Bun,
    EnvType::Git,
    EnvType::Gh,
    EnvType::Mingw,
    EnvType::Adb,
    EnvType::DotNet,
    EnvType::PowerShell,
];

impl EnvType {
    /// envs/ 下的分类文件夹名
    pub fn folder(self) -> &'static str {
        match self {
            EnvType::Jdk => "jdks",
            EnvType::Python => "pythons",
            EnvType::Node => "nodes",
            EnvType::Go => "gos",
            EnvType::Rust => "rusts",
            EnvType::Maven => "mavens",
            EnvType::Gradle => "gradles",
            EnvType::Php => "phps",
            EnvType::Llvm => "llvms",
            EnvType::Zig => "zigs",
            EnvType::Deno => "denos",
            EnvType::Bun => "buns",
            EnvType::Git => "gits",
            EnvType::Gh => "ghs",
            EnvType::Mingw => "mingws",
            EnvType::Adb => "adbs",
            EnvType::DotNet => "dotnets",
            EnvType::PowerShell => "powershells",
        }
    }

    /// current/ 下的 junction 名
    pub fn junction(self) -> &'static str {
        match self {
            EnvType::Jdk => "jdk",
            EnvType::Python => "python",
            EnvType::Node => "node",
            EnvType::Go => "go",
            EnvType::Rust => "rust",
            EnvType::Maven => "maven",
            EnvType::Gradle => "gradle",
            EnvType::Php => "php",
            EnvType::Llvm => "llvm",
            EnvType::Zig => "zig",
            EnvType::Deno => "deno",
            EnvType::Bun => "bun",
            EnvType::Git => "git",
            EnvType::Gh => "gh",
            EnvType::Mingw => "mingw",
            EnvType::Adb => "adb",
            EnvType::DotNet => "dotnet",
            EnvType::PowerShell => "powershell",
        }
    }

    /// 该类型激活后需要加入 PATH 的子路径(相对根目录)
    pub fn path_entries(self, root: &std::path::Path) -> Vec<std::path::PathBuf> {
        let cur = root.join("current").join(self.junction());
        match self {
            EnvType::Jdk => vec![cur.join("bin")],
            EnvType::Python => vec![cur.clone(), cur.join("Scripts")],
            EnvType::Node => vec![cur],
            EnvType::Go => vec![cur.join("bin")],
            EnvType::Maven => vec![cur.join("bin")],
            EnvType::Gradle => vec![cur.join("bin")],
            // Rust 环境目录结构:rustup-home/ + cargo-home/(经由 current\rust 链接)
            // Include both stable paths so switching layouts never requires editing PATH again.
            EnvType::Rust => vec![cur.join("bin"), cur.join("cargo-home").join("bin")],
            EnvType::Php => vec![cur],
            EnvType::Llvm => vec![cur.join("bin")],
            EnvType::Zig => vec![cur],
            EnvType::Deno => vec![cur],
            EnvType::Bun => vec![cur],
            // PortableGit:git.exe 位于 cmd\;另含 bin\(内嵌工具)
            EnvType::Git => vec![cur.join("cmd")],
            EnvType::Gh => vec![cur.join("bin")],
            EnvType::Mingw => vec![cur.join("bin")],
            EnvType::Adb => vec![cur],
            // .NET SDK 的 dotnet.exe 直接位于 SDK 根目录，没有 bin 子目录。
            EnvType::DotNet => vec![cur],
            // PowerShell ZIP 包的 pwsh.exe 直接位于根目录。
            EnvType::PowerShell => vec![cur],
        }
    }
}

/// 管理器内的已装环境
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedEnv {
    pub name: String,
    pub env_type: EnvType,
    pub path: String,
    pub version: Option<String>,
    pub size_bytes: Option<u64>,
    pub is_current: bool,
    /// 可用、损坏或不可访问；旧数据兼容时默认为可用。
    pub status: String,
    pub status_detail: Option<String>,
    pub size_complete: bool,
    pub identity_path: String,
    pub is_link_entry: bool,
    pub is_external_link: bool,
    /// 搬移来源：该环境是从哪个外部目录搬进受管目录的。
    ///
    /// 搬移后受管位置是真实目录，不再有链接指向外部旧目录，因此只能靠这条记录
    /// 回答「旧目录在哪」。为 `None` 表示没有搬移记录（或记录不可信），此时不提供
    /// 删除入口——宁可少一个入口，也不能凭猜测去删目录。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub moved_from: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UninstallScopeItem {
    pub resource: String,
    pub path: Option<String>,
    pub details: String,
    pub count: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[derive(Default)]
pub enum UninstallMode {
    /// Remove the environment content as well as its EnvCon records. This is
    /// the historical behaviour and stays the default.
    #[default]
    Uninstall,
    /// Keep every file on disk and only drop EnvCon's management records: the
    /// current-version junction and any PATH entry this app can prove it wrote.
    /// Nothing is moved or deleted, so a managed directory keeps its bytes and
    /// an external link keeps its target.
    Detach,
}


#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UninstallPreview {
    pub name: String,
    pub env_type: EnvType,
    pub is_current: bool,
    pub is_link_entry: bool,
    pub is_external_link: bool,
    /// Directory metadata fingerprint; not a file-content integrity proof.
    #[serde(default)]
    pub snapshot_hash: Option<String>,
    /// Binds the observed PATH and ownership plan without exposing the whole PATH.
    #[serde(default)]
    pub path_snapshot_hash: Option<String>,
    #[serde(default)]
    pub current_link_snapshot_hash: Option<String>,
    #[serde(default)]
    pub legacy_links: Vec<UninstallScopeItem>,
    /// Explicit per-entry consent submitted by the UI, never a creation-ownership claim.
    #[serde(default)]
    pub confirmed_legacy_links: Vec<String>,
    /// Per-entry user consent for PATH strings without EnvCon ownership evidence.
    #[serde(default)]
    pub legacy_path_entries: Vec<UninstallScopeItem>,
    #[serde(default)]
    pub confirmed_legacy_path_entries: Vec<String>,
    #[serde(default)]
    pub installer_plan: Option<crate::installer_uninstall::Plan>,
    pub removable: Vec<UninstallScopeItem>,
    pub retained: Vec<UninstallScopeItem>,
    pub unverified: Vec<UninstallScopeItem>,
    pub blocked: Vec<UninstallScopeItem>,
    pub scope_complete: bool,
    /// Which operation the preview describes. The executor compares the whole
    /// preview, so previewing one mode and executing another is rejected
    /// instead of silently changing what gets deleted.
    #[serde(default)]
    pub mode: UninstallMode,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UninstallActionItem {
    pub resource: String,
    pub path: Option<String>,
    pub details: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum UninstallResultStatus {
    VerifiedScopeRemoved,
    Partial,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UninstallResult {
    pub name: String,
    pub status: UninstallResultStatus,
    pub removed: Vec<UninstallActionItem>,
    pub retained: Vec<UninstallScopeItem>,
    pub unverified: Vec<UninstallScopeItem>,
    pub failed: Vec<UninstallActionItem>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryOverview {
    pub env_type: EnvType,
    pub envs: Vec<ManagedEnv>,
    /// current junction 指向的环境名
    pub current: Option<String>,
    pub junction_path: String,
    pub scan_warning: Option<String>,
    pub current_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Overview {
    pub root: Option<String>,
    pub root_exists: bool,
    pub portable: bool,
    pub data_dir: String,
    pub categories: Vec<CategoryOverview>,
}

/// One environment as recorded in an export. Deliberately carries no file
/// content and no restore instruction: paths are machine-bound, so the manifest
/// documents what exists here rather than promising a portable copy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestEntry {
    pub env_type: EnvType,
    pub name: String,
    /// Absolute path of the managed entry on this machine.
    pub path: String,
    /// Resolved identity, so a link and its target are both visible.
    pub identity_path: String,
    pub version: Option<String>,
    pub status: String,
    pub is_current: bool,
    pub is_link_entry: bool,
    pub is_external_link: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentManifest {
    pub schema_version: u32,
    pub generated_at: String,
    pub root: Option<String>,
    pub portable: bool,
    pub data_dir: String,
    pub entries: Vec<ManifestEntry>,
    /// Facts the reader needs before treating this as a restorable backup.
    pub compatibility: Vec<String>,
    pub notes: Vec<String>,
}

/// 系统散装环境
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ScanStatus {
    Unscanned,
    Available,
    NotDiscovered,
    PermissionDenied,
    StartFailed,
    CommandFailed,
    ParseFailed,
    TimedOut,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ScanRunStatus {
    Complete,
    Partial,
    Failed,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolScanSummary {
    pub tool: String,
    pub status: ScanStatus,
    pub entry_count: usize,
    pub detail: Option<String>,
}

/// 系统散装环境
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalEnv {
    pub tool: String,
    pub version: Option<String>,
    pub path: Option<String>,
    pub source: String,
    pub command: String,
    pub is_preferred: bool,
    pub error: Option<String>,
    pub scan_status: ScanStatus,
    /// 可纳入管理的环境类型(结构不兼容时为 None)
    pub env_type: Option<EnvType>,
    pub install_root: Option<String>,
    pub identity_path: Option<String>,
    /// Other entrypoints of the same installation whose reported version
    /// differs from the selected one. Empty when all launchers agree.
    #[serde(default)]
    pub version_conflicts: Vec<VersionConflict>,
}

/// A launcher under the same installation that reported a different version.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionConflict {
    pub path: String,
    pub version: Option<String>,
    /// Why the probe produced no version, when it failed.
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanReport {
    pub tools: Vec<ExternalEnv>,
    pub core_tools: Vec<ToolScanSummary>,
    pub status: ScanRunStatus,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PathState {
    pub user: Vec<String>,
    pub system: Vec<String>,
}
