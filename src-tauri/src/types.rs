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
}

pub const ALL_ENV_TYPES: [EnvType; 15] = [
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
            EnvType::Rust => vec![cur.join("cargo-home").join("bin")],
            EnvType::Php => vec![cur],
            EnvType::Llvm => vec![cur.join("bin")],
            EnvType::Zig => vec![cur],
            EnvType::Deno => vec![cur],
            EnvType::Bun => vec![cur],
            // PortableGit:git.exe 位于 cmd\;另含 bin\(内嵌工具)
            EnvType::Git => vec![cur.join("cmd")],
            EnvType::Gh => vec![cur.join("bin")],
            EnvType::Mingw => vec![cur.join("bin")],
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
    pub is_external_link: bool,
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
    /// 可纳入管理的环境类型(结构不兼容时为 None)
    pub env_type: Option<EnvType>,
    pub install_root: Option<String>,
    pub identity_path: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanReport {
    pub tools: Vec<ExternalEnv>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PathState {
    pub user: Vec<String>,
    pub system: Vec<String>,
}
