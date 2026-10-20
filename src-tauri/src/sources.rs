use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use futures_util::{stream, StreamExt};

use crate::error::{AppError, Result};
use crate::types::EnvType;

mod android;

pub const SOURCE_MIRROR: &str = "mirror";
pub const SOURCE_OFFICIAL: &str = "official";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceInfo {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionInfo {
    pub version: String,
    pub date: Option<String>,
    pub size_bytes: Option<u64>,
    pub note: Option<String>,
    /// 直接下载链接(无法从版本号推导的源使用,如 GitHub/Adoptium API)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// 发布方提供的可信校验依据；缺少时必须明确显示为未验证。
    #[serde(default)]
    pub verification: VerificationInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", tag = "status")]
pub enum VerificationInfo {
    TrustedSha256 { expected: String, source: String },
    ChecksumFile {
        url: String,
        filename: String,
        source: String,
    },
    Invalid { reason: String },
    Unverified { reason: String },
}

impl Default for VerificationInfo {
    fn default() -> Self {
        Self::unverified()
    }
}

impl VerificationInfo {
    /// The shared "nothing resolved yet" reason. A branch returning this value
    /// has not decided anything, so the caller's metadata replaces it.
    pub const DEFAULT_UNVERIFIED_REASON: &'static str =
        "来源未提供可核对的官方 SHA-256、签名或校验文件";

    pub fn trusted_sha256(expected: impl Into<String>, source: impl Into<String>) -> Self {
        Self::TrustedSha256 {
            expected: expected.into().to_ascii_lowercase(),
            source: source.into(),
        }
    }

    pub fn unverified() -> Self {
        Self::Unverified {
            reason: Self::DEFAULT_UNVERIFIED_REASON.into(),
        }
    }

    pub fn expected_sha256(&self) -> Option<&str> {
        match self {
            Self::TrustedSha256 { expected, .. } => Some(expected),
            Self::ChecksumFile { .. } | Self::Invalid { .. } | Self::Unverified { .. } => None,
        }
    }

    pub fn is_trusted(&self) -> bool {
        matches!(self, Self::TrustedSha256 { .. })
    }
}

/// 下载方式
#[derive(Debug, Clone)]
pub enum InstallMethod {
    /// 解压 zip;strip_top = zip 内是否有一层顶层目录需要剥掉
    Unzip { strip_top: bool },
    /// Windows EXE installer or portable self-extracting archive.
    Installer {
        args: Vec<String>,
        failure_recovery: InstallerFailureRecovery,
    },
    /// rustup-init 引导安装;dist_server 为 None 时用官方
    RustupInit { channel: String, dist_server: Option<String> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallerFailureRecovery {
    /// Keep installer target intact because the installer may have registered it externally.
    RetainTarget,
    /// Preserve failed files elsewhere so a fresh attempt can reserve the original name.
    MoveTargetAside,
}

#[derive(Debug, Clone)]
pub struct DownloadSpec {
    pub url: String,
    pub method: InstallMethod,
    pub verification: VerificationInfo,
    pub expected_size: Option<u64>,
}

fn http() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent(concat!("EnvCon/", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(AppError::Net)
}

/// 每个类型可用的下载源
pub fn available_sources(env_type: EnvType) -> Vec<SourceInfo> {
    let mirror = |label: &str| SourceInfo { id: SOURCE_MIRROR.into(), label: label.into() };
    let official = |label: &str| SourceInfo { id: SOURCE_OFFICIAL.into(), label: label.into() };
    match env_type {
        EnvType::Jdk => vec![mirror("清华 TUNA"), official("Adoptium 官方")],
        EnvType::Python => vec![mirror("华为云镜像"), official("python.org")],
        EnvType::Node => vec![mirror("npmmirror"), official("nodejs.org")],
        EnvType::Go => vec![mirror("golang.google.cn"), official("go.dev")],
        EnvType::Rust => vec![mirror("rsproxy.cn"), official("rust-lang.org")],
        EnvType::Maven => vec![mirror("华为云镜像"), official("apache.org")],
        EnvType::Gradle => vec![mirror("腾讯云镜像"), official("gradle.org")],
        EnvType::Php => vec![official("windows.php.net")],
        EnvType::Llvm => vec![mirror("ghfast.top(GitHub)"), official("GitHub")],
        EnvType::Zig => vec![official("ziglang.org")],
        EnvType::Deno => vec![mirror("ghfast.top(GitHub)"), official("GitHub")],
        EnvType::Bun => vec![mirror("ghfast.top(GitHub)"), official("GitHub")],
        EnvType::Git => vec![mirror("ghfast.top(GitHub)"), official("GitHub")],
        EnvType::Gh => vec![mirror("ghfast.top(GitHub)"), official("GitHub")],
        EnvType::Mingw => vec![mirror("ghfast.top(GitHub)"), official("GitHub")],
        EnvType::Adb => vec![official("Google Android SDK")],
    }
}

/// 拉取指定类型+源的版本列表(带本地缓存兜底)
pub async fn list_versions(
    env_type: EnvType,
    source: &str,
    cache_path: &PathBuf,
) -> Result<Vec<VersionInfo>> {
    let result = fetch_versions(env_type, source).await;

    let result = result.and_then(|v| if v.is_empty() {
        Err(AppError::msg("在线版本列表为空，请检查源或切换官方源"))
    } else { Ok(v) });
    match result {
        Ok(mut versions) => {
            if env_type != EnvType::Rust {
                versions.sort_by(|a, b| prerelease_last(&a.version, &b.version));
            }
            versions.dedup_by(|a, b| a.version == b.version);
            save_cache(env_type, source, &versions, cache_path);
            Ok(versions)
        }
        Err(e) => match load_cache(env_type, source, cache_path) {
            Some(mut v) => {
                if env_type == EnvType::Python {
                    // Pre-API caches were generated from FTP directory names. They
                    // include source-only releases and must not be offered offline.
                    v.retain(|row| python_download_url(&row.version, source).as_deref() == row.url.as_deref());
                    if v.is_empty() { return Err(e); }
                }
                for version in &mut v {
                    downgrade_cached_version(version);
                }
                Ok(v)
            },
            None => Err(e),
        },
    }
}

async fn fetch_versions(env_type: EnvType, source: &str) -> Result<Vec<VersionInfo>> {
    if !available_sources(env_type).iter().any(|item| item.id == source) {
        return Err(AppError::msg("下载源与该工具支持的来源不匹配"));
    }
    let result = match (env_type, source) {
        (EnvType::Node, SOURCE_MIRROR) => node_index("https://npmmirror.com/mirrors/node/index.json").await,
        (EnvType::Node, _) => node_index("https://nodejs.org/dist/index.json").await,

        (EnvType::Go, SOURCE_MIRROR) => go_index("https://golang.google.cn/dl/?mode=json&include=all").await,
        (EnvType::Go, _) => go_index("https://go.dev/dl/?mode=json&include=all").await,

        (EnvType::Python, source) => python_releases(source == SOURCE_MIRROR).await,

        (EnvType::Jdk, SOURCE_MIRROR) => jdk_tuna().await,
        (EnvType::Jdk, _) => jdk_adoptium().await,

        (EnvType::Maven, SOURCE_MIRROR) => {
            maven_dir("https://mirrors.huaweicloud.com/apache/maven/maven-3/").await
        }
        (EnvType::Maven, _) => maven_dir("https://dlcdn.apache.org/maven/maven-3/").await,

        (EnvType::Gradle, SOURCE_MIRROR) => {
            gradle_dir("https://mirrors.cloud.tencent.com/gradle/").await
        }
        (EnvType::Gradle, _) => gradle_dir("https://services.gradle.org/distributions/").await,

        (EnvType::Rust, _) => Ok(rust_channels()),

        (EnvType::Php, _) => php_releases().await,
        (EnvType::Llvm, _) => llvm_releases().await,
        (EnvType::Zig, _) => zig_releases().await,
        (EnvType::Deno, _) => deno_releases().await,
        (EnvType::Bun, _) => bun_releases().await,
        (EnvType::Git, _) => git_releases().await,
        (EnvType::Gh, _) => gh_releases().await,
        (EnvType::Mingw, _) => mingw_releases().await,
        (EnvType::Adb, _) => android::releases().await,
    };
    let mut versions = result?;
    if versions.is_empty() {
        return Err(AppError::msg("在线版本列表为空，请检查源或切换官方源"));
    }
    if env_type != EnvType::Rust {
        versions.sort_by(|a, b| prerelease_last(&a.version, &b.version));
    }
    versions.dedup_by(|a, b| a.version == b.version);
    Ok(versions)
}

pub async fn resolve_download_spec(
    env_type: EnvType,
    version: &str,
    target_dir: &str,
    source: &str,
    selected_url: Option<&str>,
    cache_path: &PathBuf,
) -> Result<DownloadSpec> {
    let versions = list_versions(env_type, source, cache_path).await?;
    let row = select_version(&versions, version, selected_url)?;
    let verification = resolve_verification(&row.verification).await?;
    download_spec_with_verification(
        env_type,
        version,
        target_dir,
        source,
        row.url.as_deref(),
        Some(&verification),
        row.size_bytes,
    )
}

async fn resolve_verification(verification: &VerificationInfo) -> Result<VerificationInfo> {
    match verification {
        VerificationInfo::ChecksumFile {
            url,
            filename,
            source,
        } => {
            if !url.starts_with("https://") {
                return Err(AppError::msg("校验文件必须来自 HTTPS 来源"));
            }
            let body = http()?
                .get(url)
                .send()
                .await?
                .error_for_status()?
                .text()
                .await?;
            Ok(VerificationInfo::trusted_sha256(
                parse_sha256_checksum_file(&body, filename)?,
                source,
            ))
        }
        other => Ok(other.clone()),
    }
}

fn parse_sha256_checksum_file(body: &str, filename: &str) -> Result<String> {
    if filename.is_empty()
        || filename.contains(['/', '\\'])
        || matches!(filename, "." | "..")
    {
        return Err(AppError::msg("校验文件目标名称无效"));
    }
    let mut found: Option<String> = None;
    let lines: Vec<_> = body.lines().map(str::trim).filter(|line| !line.is_empty()).collect();
    if lines.len() == 1 {
        if let Some(hash) = parse_sha256(lines[0]) {
            return Ok(hash);
        }
    }
    // Track whether the file described *some* asset. A file that lists other
    // assets is a different situation from an empty or unparseable one, and the
    // message must let a user tell them apart.
    let mut listed_assets = 0usize;
    for line in lines {
        let mut fields = line.split_whitespace();
        let Some(hash) = fields.next() else { continue };
        let Some(name) = fields.next() else { continue };
        listed_assets += 1;
        if name.trim_start_matches('*').trim_matches('"') != filename {
            continue;
        }
        let Some(hash) = parse_sha256(hash) else {
            return Err(AppError::msg(format!(
                "校验文件中 {filename} 对应的 SHA-256 格式无效"
            )));
        };
        if found.as_ref().is_some_and(|previous| previous != &hash) {
            return Err(AppError::msg(format!(
                "校验文件对 {filename} 提供了冲突的 SHA-256"
            )));
        }
        found = Some(hash);
    }
    match found {
        Some(hash) => Ok(hash),
        None if listed_assets > 0 => Err(AppError::msg(format!(
            "校验文件未列出目标资产 {filename}（该文件列出了 {listed_assets} 个其他条目，可能版本或资产名已变化）"
        ))),
        None => Err(AppError::msg(format!("校验文件缺少目标资产 {filename}"))),
    }
}

/// 生成下载规格;url_hint 为版本列表提供的直链(GitHub/Adoptium 等源)
pub fn download_spec(
    env_type: EnvType,
    version: &str,
    target_dir: &str,
    source: &str,
    url_hint: Option<&str>,
) -> Result<DownloadSpec> {
    download_spec_with_verification(env_type, version, target_dir, source, url_hint, None, None)
}

pub fn select_version<'a>(
    versions: &'a [VersionInfo],
    version: &str,
    selected_url: Option<&str>,
) -> Result<&'a VersionInfo> {
    let row = versions
        .iter()
        .find(|row| row.version == version)
        .ok_or_else(|| AppError::msg("所选版本不在后端当前版本目录中，请刷新后重试"))?;
    if row.url.as_deref() != selected_url {
        return Err(AppError::msg("下载链接与后端版本目录不匹配，请刷新后重试"));
    }
    Ok(row)
}

pub fn cached_verification(
    env_type: EnvType,
    source: &str,
    version: &str,
    selected_url: Option<&str>,
    cache_path: &PathBuf,
) -> Result<VerificationInfo> {
    let versions = load_cache(env_type, source, cache_path)
        .ok_or_else(|| AppError::msg("版本目录缓存不存在，请刷新下载列表后重试"))?;
    Ok(select_version(&versions, version, selected_url)?.verification.clone())
}

/// Build a download specification after the caller has selected a version row.
/// The URL/source shape is still checked here; the optional verification metadata
/// is copied only after that check so a stale or mismatched URL cannot be used
/// with a trusted digest from another row.
pub fn download_spec_with_verification(
    env_type: EnvType,
    version: &str,
    target_dir: &str,
    source: &str,
    url_hint: Option<&str>,
    verification: Option<&VerificationInfo>,
    expected_size: Option<u64>,
) -> Result<DownloadSpec> {
    let gh = |url: &str| -> String {
        if source == SOURCE_MIRROR {
            format!("https://ghfast.top/{url}")
        } else {
            url.to_string()
        }
    };
    let verification = verification.cloned().unwrap_or_default();
    let mut spec = match env_type {
        EnvType::Adb => android::download_spec(version, url_hint),
        EnvType::Node => {
            let base = if source == SOURCE_MIRROR {
                "https://npmmirror.com/mirrors/node"
            } else {
                "https://nodejs.org/dist"
            };
            Ok(DownloadSpec {
                url: format!("{base}/v{v}/node-v{v}-win-x64.zip", v = version),
                method: InstallMethod::Unzip { strip_top: true },
                verification: VerificationInfo::default(),
                expected_size: None,
            })
        }
        EnvType::Go => {
            let base = if source == SOURCE_MIRROR {
                "https://golang.google.cn/dl"
            } else {
                "https://go.dev/dl"
            };
            Ok(DownloadSpec {
                url: format!("{base}/go{v}.windows-amd64.zip", v = version),
                method: InstallMethod::Unzip { strip_top: true },
                verification: VerificationInfo::default(),
                expected_size: None,
            })
        }
        EnvType::Python => {
            let expected = python_download_url(version, source)
                .ok_or_else(|| AppError::msg("无效的 Python 版本"))?;
            if url_hint != Some(expected.as_str()) {
                return Err(AppError::msg("无法确认此 Python 版本有 Windows x64 安装器，请刷新版本列表后重试"));
            }
            Ok(DownloadSpec {
                url: expected,
                method: InstallMethod::Installer {
                    args: vec![
                        "/quiet".into(),
                        "InstallAllUsers=0".into(),
                        format!("TargetDir={}", target_dir.replace('/', "\\")),
                        "PrependPath=0".into(),
                        "Shortcuts=0".into(),
                        "Include_launcher=0".into(),
                    ],
                    failure_recovery: InstallerFailureRecovery::RetainTarget,
                },
                verification: VerificationInfo::default(),
                expected_size: None,
            })
        }
        EnvType::Jdk => {
            if source == SOURCE_MIRROR {
                // TUNA:版本号可推导文件名
                let (ver, build) = version
                    .split_once('+')
                    .ok_or_else(|| AppError::msg(format!("无法解析 JDK 版本号: {version}")))?;
                let major = ver.split('.').next().unwrap_or("17").to_string();
                let file_part = if major == "8" {
                    let update = ver.split('.').nth(2).unwrap_or("0");
                    let b: u32 = build.parse().unwrap_or(0);
                    format!("8u{update}b{b:02}")
                } else {
                    format!("{ver}_{build}")
                };
                Ok(DownloadSpec {
                    url: format!(
                        "https://mirrors.tuna.tsinghua.edu.cn/Adoptium/{major}/jdk/x64/windows/OpenJDK{major}U-jdk_x64_windows_hotspot_{file_part}.zip",
                    ),
                    method: InstallMethod::Unzip { strip_top: true },
                    verification: VerificationInfo::default(),
                    expected_size: None,
                })
            } else {
                // Adoptium API 直链
                let url = url_hint.ok_or_else(|| {
                    AppError::msg("官方源 JDK 下载链接缺失,请刷新版本列表后重试")
                })?;
                Ok(DownloadSpec {
                    url: url.to_string(),
                    method: InstallMethod::Unzip { strip_top: true },
                    verification: VerificationInfo::default(),
                    expected_size: None,
                })
            }
        }
        EnvType::Maven => {
            let base = if source == SOURCE_MIRROR {
                "https://mirrors.huaweicloud.com/apache/maven/maven-3"
            } else {
                "https://dlcdn.apache.org/maven/maven-3"
            };
            Ok(DownloadSpec {
                url: format!("{base}/{v}/binaries/apache-maven-{v}-bin.zip", v = version),
                method: InstallMethod::Unzip { strip_top: true },
                verification: VerificationInfo::default(),
                expected_size: None,
            })
        }
        EnvType::Gradle => {
            let base = if source == SOURCE_MIRROR {
                "https://mirrors.cloud.tencent.com/gradle"
            } else {
                "https://services.gradle.org/distributions"
            };
            Ok(DownloadSpec {
                url: format!("{base}/gradle-{v}-bin.zip", v = version),
                method: InstallMethod::Unzip { strip_top: true },
                verification: VerificationInfo::default(),
                expected_size: None,
            })
        }
        EnvType::Rust => {
            if source == SOURCE_MIRROR {
                Ok(DownloadSpec {
                    url: "https://rsproxy.cn/rustup/dist/x86_64-pc-windows-msvc/rustup-init.exe".into(),
                    method: InstallMethod::RustupInit {
                        channel: version.to_string(),
                        dist_server: Some("https://rsproxy.cn".into()),
                    },
                    // A mirror copy has not been compared with the publisher file,
                    // so a digest resolved for the official URL must not be reused.
                    verification: VerificationInfo::Unverified {
                        reason: "镜像地址与官方 rustup-init 未经逐字节核对".into(),
                    },
                    expected_size: None,
                })
            } else {
                Ok(DownloadSpec {
                    url: "https://static.rust-lang.org/rustup/dist/x86_64-pc-windows-msvc/rustup-init.exe".into(),
                    method: InstallMethod::RustupInit { channel: version.to_string(), dist_server: None },
                    // rust-lang.org publishes rustup-init.exe.sha256 next to the
                    // binary; keep whatever the caller resolved instead of
                    // dropping it on the floor.
                    verification: verification.clone(),
                    expected_size,
                })
            }
        }
        EnvType::Php => {
            let url = url_hint.ok_or_else(|| {
                AppError::msg("PHP 下载链接缺失,请刷新版本列表后重试")
            })?;
            Ok(DownloadSpec {
                url: url.to_string(),
                method: InstallMethod::Unzip { strip_top: false },
                verification: VerificationInfo::default(),
                expected_size: None,
            })
        }
        EnvType::Llvm => {
            let url = url_hint.ok_or_else(|| {
                AppError::msg("LLVM 下载链接缺失,请刷新版本列表后重试")
            })?;
            let url = gh(url);
            // LLVM's Windows MSI package needs a separately verified property
            // contract before it can safely be installed into an EnvCon target.
            // Keep the known NSIS EXE path until that contract is documented.
            if !url.to_ascii_lowercase().ends_with(".exe") {
                return Err(AppError::msg(
                    "此 LLVM 版本只有未验证的 Windows 安装器格式，暂不提供安装",
                ));
            }
            Ok(DownloadSpec {
                url,
                method: InstallMethod::Installer {
                    args: vec!["/S".into(), format!("/D={}", target_dir.replace('/', "\\"))],
                    failure_recovery: InstallerFailureRecovery::RetainTarget,
                },
                verification: VerificationInfo::default(),
                expected_size: None,
            })
        }
        EnvType::Zig => {
            let url = url_hint.ok_or_else(|| {
                AppError::msg("Zig 版本清单未提供 Windows x64 压缩包，请刷新版本列表后重试")
            })?;
            Ok(DownloadSpec {
                url: url.to_string(),
                method: InstallMethod::Unzip { strip_top: true },
                verification: VerificationInfo::default(),
                expected_size: None,
            })
        }
        EnvType::Deno => {
            let url = url_hint.ok_or_else(|| {
                AppError::msg("Deno 下载链接缺失,请刷新版本列表后重试")
            })?;
            Ok(DownloadSpec {
                url: gh(url),
                method: InstallMethod::Unzip { strip_top: false },
                verification: VerificationInfo::default(),
                expected_size: None,
            })
        }
        EnvType::Bun => {
            let url = url_hint.ok_or_else(|| {
                AppError::msg("Bun 下载链接缺失,请刷新版本列表后重试")
            })?;
            Ok(DownloadSpec {
                url: gh(url),
                method: InstallMethod::Unzip { strip_top: true },
                verification: VerificationInfo::default(),
                expected_size: None,
            })
        }
        EnvType::Git => {
            let url = url_hint.ok_or_else(|| {
                AppError::msg("Git 下载链接缺失,请刷新版本列表后重试")
            })?;
            // PortableGit 自解压包(7z SFX):-o<目录> -y 静默解压到指定目录,免管理员、不改 PATH
            Ok(DownloadSpec {
                url: gh(url),
                method: InstallMethod::Installer {
                    args: vec![
                        format!("-o{}", target_dir.replace('/', "\\")),
                        "-y".into(),
                    ],
                    failure_recovery: InstallerFailureRecovery::MoveTargetAside,
                },
                verification: VerificationInfo::default(),
                expected_size: None,
            })
        }
        EnvType::Gh => {
            let url = url_hint.ok_or_else(|| {
                AppError::msg("GitHub CLI 下载链接缺失,请刷新版本列表后重试")
            })?;
            Ok(DownloadSpec {
                url: gh(url),
                method: InstallMethod::Unzip { strip_top: true },
                verification: VerificationInfo::default(),
                expected_size: None,
            })
        }
        EnvType::Mingw => {
            let url = url_hint.ok_or_else(|| {
                AppError::msg("C/C++ 下载链接缺失,请刷新版本列表后重试")
            })?;
            Ok(DownloadSpec {
                url: gh(url),
                method: InstallMethod::Unzip { strip_top: true },
                verification: VerificationInfo::default(),
                expected_size: None,
            })
        }
    }?;
    // Caller-supplied metadata fills the branch default. A branch that made its
    // own decision (a checksum file, or a deliberate unverified reason such as a
    // mirror URL that was not proven identical) keeps it. `VerificationInfo::default()`
    // is the shared "nothing resolved yet" value and is replaced.
    let branch_decided = match &spec.verification {
        VerificationInfo::TrustedSha256 { .. } | VerificationInfo::ChecksumFile { .. } => true,
        VerificationInfo::Invalid { .. } => true,
        VerificationInfo::Unverified { reason } => reason != VerificationInfo::DEFAULT_UNVERIFIED_REASON,
    };
    if !branch_decided {
        spec.verification = verification;
    }
    spec.expected_size = spec.expected_size.or(expected_size);
    Ok(spec)
}

// ---------- 各源解析 ----------

/// Node 系 index.json(npmmirror / nodejs.org 结构相同)
async fn node_index(base: &str) -> Result<Vec<VersionInfo>> {
    let json = http()?.get(base).send().await?.error_for_status()?.text().await?;
    parse_node_index_with_checksum(&json, base.starts_with("https://nodejs.org/"))
}

#[cfg(test)]
fn parse_node_index(json: &str) -> Result<Vec<VersionInfo>> {
    parse_node_index_with_checksum(json, false)
}

fn parse_node_index_with_checksum(json: &str, official: bool) -> Result<Vec<VersionInfo>> {
    #[derive(Deserialize)]
    struct Entry {
        version: String,
        date: Option<String>,
        /// LTS 版本为代号字符串(如 "Jod"),非 LTS 为 false
        lts: Option<serde_json::Value>,
        security: Option<bool>,
        #[serde(default)]
        files: Vec<String>,
    }
    let list: Vec<Entry> = serde_json::from_str(json)?;
    let mut versions: Vec<_> = list
        .into_iter()
        .filter(|e| e.files.iter().any(|f| f == "win-x64-zip"))
        .map(|e| {
            let verification = if official {
                VerificationInfo::ChecksumFile {
                    url: format!("https://nodejs.org/dist/{}/SHASUMS256.txt", e.version),
                    filename: format!("node-{}-win-x64.zip", e.version),
                    source: "Node.js 官方 SHASUMS256.txt".into(),
                }
            } else {
                VerificationInfo::unverified()
            };
            VersionInfo {
            version: e.version.trim_start_matches('v').to_string(),
            date: e.date,
            size_bytes: None,
            note: match &e.lts {
                Some(serde_json::Value::String(_)) => Some("LTS".into()),
                _ if e.security == Some(true) => Some("安全更新".into()),
                _ => None,
            },
            url: None,
            verification,
        }})
        .collect();
    versions.sort_by(|a, b| version_cmp(&b.version, &a.version));
    versions.dedup_by(|a,b| a.version == b.version);
    if versions.is_empty() { return Err(AppError::msg("源未返回可安装的 Node Windows x64 ZIP 版本")); }
    Ok(versions)
}

/// Go 官方 JSON(golang.google.cn / go.dev 结构相同)
async fn go_index(base: &str) -> Result<Vec<VersionInfo>> {
    #[derive(Deserialize)]
    struct File {
        size: i64,
        os: Option<String>,
        arch: Option<String>,
        kind: Option<String>,
        #[serde(default)]
        sha256: Option<String>,
    }
    #[derive(Deserialize)]
    struct Release {
        version: String,
        #[serde(default)]
        files: Vec<File>,
    }
    let list: Vec<Release> = http()?
        .get(base)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(list
        .into_iter()
        .filter_map(|r| {
            let win = r.files.iter().find(|f| {
                f.os.as_deref() == Some("windows")
                    && f.arch.as_deref() == Some("amd64")
                    && f.kind.as_deref() == Some("archive")
            })?;
            Some(VersionInfo {
                version: r.version.trim_start_matches("go").to_string(),
                date: None,
                size_bytes: u64::try_from(win.size).ok(),
                note: None,
                url: None,
                verification: go_asset_verification(base, win.sha256.as_deref()),
            })
        })
        .collect())
}

#[derive(Deserialize)]
struct PythonRelease {
    name: String,
    release_date: Option<String>,
    is_published: bool,
    pre_release: bool,
    resource_uri: String,
}

#[derive(Deserialize)]
struct PythonReleaseFile {
    name: String,
    url: String,
    filesize: Option<u64>,
    release: String,
    #[serde(default)]
    sha256_sum: Option<String>,
}

fn python_download_url(version: &str, source: &str) -> Option<String> {
    if !version.starts_with("3.")
        || version.split('.').count() != 3
        || !version.split('.').all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
    {
        return None;
    }
    let base = if source == SOURCE_MIRROR {
        "https://mirrors.huaweicloud.com/python"
    } else {
        "https://www.python.org/ftp/python"
    };
    Some(format!("{base}/{version}/python-{version}-amd64.exe"))
}

/// Python download list. Official data uses the release API for dates and
/// installer metadata; mirror data is deliberately directory-only so the
/// mirror can still work when python.org is unavailable.
async fn python_releases(mirror: bool) -> Result<Vec<VersionInfo>> {
    if mirror {
        return python_mirror_releases().await;
    }
    let client = http()?;
    let releases: Vec<PythonRelease> = client
        .get("https://www.python.org/api/v2/downloads/release/")
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let files: Vec<PythonReleaseFile> = client
        .get("https://www.python.org/api/v2/downloads/release_file/?os=1")
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    parse_python_assets(releases, files, mirror)
}

async fn python_mirror_releases() -> Result<Vec<VersionInfo>> {
    python_mirror_releases_from(
        "https://mirrors.huaweicloud.com/python/",
        "https://www.python.org/api/v2/downloads/release/",
    )
    .await
}

async fn python_mirror_releases_from(base: &str, release_api: &str) -> Result<Vec<VersionInfo>> {
    let client = http()?;
    let html = client
        .get(base)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    let versions = parse_python_mirror_directories(&html);
    if versions.is_empty() {
        return Err(AppError::msg("华为云镜像没有可解析的 Python 版本目录"));
    }
    // Dates are enrichment only: a python.org outage must not prevent the
    // Huawei directory and its actual installer listing from being used.
    let release_dates = tokio::time::timeout(
        std::time::Duration::from_secs(4),
        python_official_release_dates(&client, release_api),
    )
    .await
    .ok()
    .and_then(Result::ok)
    .unwrap_or_default();

    let rows = stream::iter(versions.into_iter().map(|version| {
        let client = client.clone();
        let release_dates = release_dates.clone();
        async move {
            let directory = format!("{base}{version}/");
            let html = client
                .get(&directory)
                .send()
                .await
                .ok()?
                .error_for_status()
                .ok()?
                .text()
                .await
                .ok()?;
            let filename = format!("python-{version}-amd64.exe");
            if !parse_python_mirror_asset(&html, &filename) {
                return None;
            }
            Some(VersionInfo {
                date: release_dates.get(&version).cloned(),
                version: version.clone(),
                size_bytes: None,
                note: Some(if release_dates.contains_key(&version) {
                    "镜像目录确认 Windows x64 安装器；发布日期来自 Python 官方发布 API".into()
                } else {
                    "镜像目录确认 Windows x64 安装器；正式发布日期未知".into()
                }),
                url: Some(format!("{directory}{filename}")),
                verification: VerificationInfo::default(),
            })
        }
    }))
    .buffer_unordered(8)
    .filter_map(|row| async move { row })
    .collect::<Vec<_>>()
    .await;

    if rows.is_empty() {
        return Err(AppError::msg("华为云镜像没有可用的 Windows x64 安装器"));
    }
    Ok(rows)
}

async fn python_official_release_dates(
    client: &reqwest::Client,
    release_api: &str,
) -> Result<HashMap<String, String>> {
    let releases: Vec<PythonRelease> = client
        .get(release_api)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(releases
        .into_iter()
        .filter(|release| {
            release.is_published
                && release.name.starts_with("Python 3.")
                && !release.pre_release
        })
        .filter_map(|release| {
            let version = release.name.strip_prefix("Python ")?.to_string();
            if !is_python_version(&version) {
                return None;
            }
            let date = release.release_date?.get(..10)?.to_string();
            Some((version, date))
        })
        .collect())
}

fn parse_python_mirror_directories(html: &str) -> Vec<String> {
    let mut versions: Vec<_> = extract_all_between(html, "href=\"", "/\"")
        .into_iter()
        .filter(|version| is_python_version(version))
        .collect();
    versions.sort_by(|a, b| version_cmp(b, a));
    versions.dedup();
    versions
}

fn parse_python_mirror_asset(html: &str, filename: &str) -> bool {
    extract_all_between(html, "href=\"", "\"")
        .into_iter()
        .any(|name| name == filename)
}

fn is_python_version(version: &str) -> bool {
    version.starts_with("3.")
        && version.split('.').count() == 3
        && version
            .split('.')
            .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
}

fn parse_python_assets(
    releases: Vec<PythonRelease>,
    files: Vec<PythonReleaseFile>,
    mirror: bool,
) -> Result<Vec<VersionInfo>> {
    let releases: HashMap<String, PythonRelease> = releases
        .into_iter()
        .filter(|release| {
            release.is_published
                && release.name.starts_with("Python 3.")
                && !release.pre_release
        })
        .filter_map(|release| {
            let id = release.resource_uri.trim_end_matches('/').rsplit('/').next()?.to_string();
            Some((id, release))
        })
        .collect();
    let mut out = Vec::new();
    for file in files {
        if !matches!(file.name.as_str(), "Windows installer (64-bit)" | "Windows x86-64 executable installer") { continue; }
        let Some(id) = file.release.trim_end_matches('/').rsplit('/').next() else { continue };
        let Some(release) = releases.get(id) else { continue };
        let Some(version) = release.name.strip_prefix("Python ") else { continue };
        let source = if mirror { SOURCE_MIRROR } else { SOURCE_OFFICIAL };
        let Some(expected) = python_download_url(version, source) else { continue };
        let Some((_, rest)) = file.url.split_once("/ftp/python/") else { continue };
        let mut parts = rest.split('/');
        let directory = parts.next().unwrap_or_default();
        let filename = parts.next().unwrap_or_default();
        if directory != version || filename != format!("python-{version}-amd64.exe") || parts.next().is_some() { continue; }
        if file.url != python_download_url(version, SOURCE_OFFICIAL).unwrap_or_default() { continue; }
        out.push(VersionInfo {
            version: version.to_string(),
            date: release.release_date.as_deref().and_then(|date| date.get(..10)).map(str::to_owned),
            size_bytes: file.filesize,
            note: Some("Windows x64 安装器".into()),
            url: Some(expected),
            verification: publisher_sha256(
                file.sha256_sum.as_deref(),
                "Python 官方 release_file API",
            ),
        });
    }
    out.sort_by(|a, b| version_cmp(&b.version, &a.version));
    out.dedup_by(|a, b| a.version == b.version);
    if out.is_empty() {
        return Err(AppError::msg("Python 官方清单没有可用的 Windows x64 安装器"));
    }
    Ok(out)
}

fn extract_date_from_row(row: &str) -> Option<String> {
    // Apache listings use either `YYYY-MM-DD HH:MM` or `DD-Mon-YYYY HH:MM`.
    // Do not start at the first `20` in the latter form: that drops the
    // month/day and makes JavaScript display every row as January 1.
    let bytes = row.as_bytes();
    for start in 0..bytes.len() {
        if start + 10 <= bytes.len()
            && is_digits(&bytes[start..start + 4])
            && bytes[start + 4] == b'-'
            && is_digits(&bytes[start + 5..start + 7])
            && bytes[start + 7] == b'-'
            && is_digits(&bytes[start + 8..start + 10])
        {
            let date = &row[start..start + 10];
            return Some(append_time(date.to_string(), &row[start + 10..]));
        }
        if start + 11 <= bytes.len()
            && is_digits(&bytes[start..start + 2])
            && bytes[start + 2] == b'-'
            && bytes[start + 6] == b'-'
            && is_digits(&bytes[start + 7..start + 11])
        {
            let month = match &row[start + 3..start + 6].to_ascii_lowercase()[..] {
                "jan" => "01", "feb" => "02", "mar" => "03", "apr" => "04",
                "may" => "05", "jun" => "06", "jul" => "07", "aug" => "08",
                "sep" => "09", "oct" => "10", "nov" => "11", "dec" => "12",
                _ => continue,
            };
            let date = format!("{}-{month}-{}", &row[start + 7..start + 11], &row[start..start + 2]);
            return Some(append_time(date, &row[start + 11..]));
        }
    }
    None
}

fn is_digits(bytes: &[u8]) -> bool {
    !bytes.is_empty() && bytes.iter().all(u8::is_ascii_digit)
}

fn append_time(date: String, tail: &str) -> String {
    let bytes = tail.as_bytes();
    for start in 0..bytes.len().saturating_sub(4) {
        if is_digits(&bytes[start..start + 2])
            && bytes[start + 2] == b':'
            && is_digits(&bytes[start + 3..start + 5])
        {
            return format!("{date} {}", &tail[start..start + 5]);
        }
    }
    date
}

/// JDK 镜像:清华 TUNA Adoptium 各 major 目录(取 8/11/17/21/25 最新)
async fn jdk_tuna() -> Result<Vec<VersionInfo>> {
    let mut out: Vec<VersionInfo> = Vec::new();
    for major in [25u32, 21, 17, 11, 8] {
        let url = format!("https://mirrors.tuna.tsinghua.edu.cn/Adoptium/{major}/jdk/x64/windows/");
        let html = match http()?.get(&url).send().await?.error_for_status() {
            Ok(r) => r.text().await?,
            Err(_) => continue,
        };
        let mut best: Option<(String, String)> = None;
        for name in extract_all_between(&html, "href=\"OpenJDK", "\"") {
            if !name.ends_with(".zip") {
                continue;
            }
            if let Some(v) = jdk_version_from_filename(&name, major) {
                let better = match &best {
                    Some((bv, _)) => version_cmp(&v, bv) == std::cmp::Ordering::Greater,
                    None => true,
                };
                if better {
                    best = Some((v, format!("OpenJDK{name}")));
                }
            }
        }
        if let Some((v, file)) = best {
            let date = html
                .rfind(&file)
                .and_then(|i| extract_date_from_row(&html[i..]));
            out.push(VersionInfo {
                version: v,
                date,
                size_bytes: None,
                note: Some(format!("Adoptium Temurin {major}")),
                url: Some(format!("{url}{file}")),
                verification: VerificationInfo::default(),
            });
        }
    }
    if out.is_empty() {
        return Err(AppError::msg("未能从镜像解析出 JDK 版本列表"));
    }
    Ok(out)
}

/// JDK 官方:Adoptium API(各 major 最新版)
async fn jdk_adoptium() -> Result<Vec<VersionInfo>> {
    #[derive(Deserialize)]
    struct Pkg {
        link: String,
        size: Option<u64>,
        checksum: Option<String>,
    }
    #[derive(Deserialize)]
    struct Binary {
        package: Pkg,
    }
    #[derive(Deserialize)]
    struct Asset {
        release_name: String,
        binary: Binary,
    }

    let mut out: Vec<VersionInfo> = Vec::new();
    #[derive(Deserialize)]
    struct Releases { available_releases: Vec<u32> }
    let releases: Releases = http()?.get("https://api.adoptium.net/v3/info/available_releases")
        .send().await?.error_for_status()?.json().await?;
    let client = http()?;
    let results = stream::iter(releases.available_releases.into_iter().filter(|v| *v >= 8))
        .map(|major| {
            let client = client.clone();
            async move {
        let url = format!(
            "https://api.adoptium.net/v3/assets/latest/{major}/hotspot?architecture=x64&image_type=jdk&os=windows"
        );
        let result = client
            .get(&url)
            .send()
            .await
            .and_then(|r| r.error_for_status());
        let assets = match result { Ok(r) => r.json::<Vec<Asset>>().await, Err(e) => Err(e) };
        (major, assets)
            }
        }).buffer_unordered(4).collect::<Vec<_>>().await;
    let mut failures = Vec::new();
    for (major, assets) in results {
        let assets = match assets { Ok(v) => v, Err(_) => { failures.push(major); continue; } };
        let Some(a) = assets.iter().find(|a| a.binary.package.link.ends_with(".zip")) else { failures.push(major); continue; };
        // release_name: "jdk-21.0.5+11" / "jdk8u402-b06"
        let version = adoptium_version(&a.release_name, major);
        out.push(VersionInfo {
            version,
            date: None,
            size_bytes: a.binary.package.size,
            note: Some(format!("Adoptium Temurin {major}")),
            url: Some(a.binary.package.link.clone()),
            verification: publisher_sha256(
                a.binary.package.checksum.as_deref(),
                "Eclipse Adoptium API",
            ),
        });
    }
    if out.is_empty() {
        return Err(AppError::msg("未能从 Adoptium API 获取版本列表"));
    }
    if !failures.is_empty() {
        for item in &mut out {
            item.note = Some(format!("{}；部分主版本查询失败：{failures:?}", item.note.take().unwrap_or_default()));
        }
    }
    Ok(out)
}

/// "jdk-21.0.5+11" → "21.0.5+11";"jdk8u402-b06" → "8.0.402+6"
fn adoptium_version(release_name: &str, major: u32) -> String {
    if major == 8 {
        // jdk8u402-b06
        let rest = release_name.trim_start_matches("jdk8u");
        let (update, build) = rest.split_once('-').unwrap_or((rest, "b0"));
        let b = build.trim_start_matches('b');
        let bnum: String = b.chars().take_while(|c| c.is_ascii_digit()).collect();
        format!("8.0.{update}+{bnum}")
    } else {
        release_name
            .trim_start_matches("jdk-")
            .replace('-', "+")
    }
}

fn jdk_version_from_filename(name: &str, major: u32) -> Option<String> {
    // 输入形如:17U-jdk_x64_windows_hotspot_17.0.20.1_1 或 8U-jdk_x64_windows_hotspot_8u504b01
    let re_part = name
        .split("windows_hotspot_")
        .nth(1)?
        .trim_end_matches(".zip")
        .trim_end_matches('"');
    if major == 8 {
        let rest = re_part.trim_start_matches("8u");
        let bnum: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        let bld = rest.trim_start_matches(&bnum).trim_start_matches('b');
        if bnum.is_empty() {
            return None;
        }
        let b: u32 = bld.parse().unwrap_or(0);
        return Some(format!("8.0.{bnum}+{b}"));
    }
    let (ver, build) = re_part.split_once('_')?;
    if ver.is_empty() || build.is_empty() {
        return None;
    }
    Some(format!("{ver}+{build}"))
}

/// Maven 目录(华为云 / apache.org)
async fn maven_dir(base: &str) -> Result<Vec<VersionInfo>> {
    let html = http()?.get(base).send().await?.error_for_status()?.text().await?;
    let mut versions: Vec<String> = extract_all_between(&html, "href=\"", "/\"")
        .into_iter()
        .filter(|v| {
            v.split('.').count() == 3 && v.chars().next().is_some_and(|c| c.is_ascii_digit())
        })
        .collect();
    versions.sort_by(|a, b| prerelease_last(a, b));
    if versions.is_empty() {
        return Err(AppError::msg("未能解析 Maven 版本列表"));
    }
    Ok(versions
        .into_iter()
        .map(|v| VersionInfo {
            version: v,
            date: None,
            size_bytes: None,
            note: None,
            url: None,
            // Apache publishes apache-maven-<v>-bin.zip.sha512 next to the
            // archive, but the download verifier only accepts SHA-256. Say so
            // instead of implying the source offers nothing.
            verification: VerificationInfo::Unverified {
                reason: "Apache 仅提供 SHA-512 校验文件；当前只核对 SHA-256，故标为未验证".into(),
            },
        })
        .collect())
}

/// Gradle 目录(腾讯云 / gradle.org,均为 *-bin.zip 文件列表;href 可能为相对或绝对路径)
async fn gradle_dir(base: &str) -> Result<Vec<VersionInfo>> {
    let official = base.starts_with("https://services.gradle.org/");
    let html = http()?.get(base).send().await?.error_for_status()?.text().await?;
    let mut versions: Vec<String> = extract_all_between(&html, "href=\"", "\"")
        .into_iter()
        .filter_map(|href| {
            // 取路径最后一段,形如 gradle-9.7.1-bin.zip
            let base = href.rsplit('/').next()?.to_string();
            base.strip_prefix("gradle-")?
                .strip_suffix("-bin.zip")
                .map(|s| s.to_string())
        })
        .filter(|v| v.chars().next().is_some_and(|c| c.is_ascii_digit()))
        .collect();
    versions.dedup();
    versions.sort_by(|a, b| prerelease_last(a, b));
    if versions.is_empty() {
        return Err(AppError::msg("未能解析 Gradle 版本列表"));
    }
    Ok(versions
        .into_iter()
        .map(|v| VersionInfo {
            version: v.clone(),
            date: None,
            size_bytes: None,
            note: None,
            url: None,
            verification: if official {
                VerificationInfo::ChecksumFile {
                    url: format!("{base}gradle-{v}-bin.zip.sha256"),
                    filename: format!("gradle-{v}-bin.zip"),
                    source: "Gradle 官方 SHA-256 校验文件".into(),
                }
            } else {
                VerificationInfo::unverified()
            },
        })
        .collect())
}

/// Rust: 提供 stable/beta/nightly
fn rust_channels() -> Vec<VersionInfo> {
    let rustup_checksum = |channel: &str| VerificationInfo::ChecksumFile {
        url: "https://static.rust-lang.org/rustup/dist/x86_64-pc-windows-msvc/rustup-init.exe.sha256".into(),
        filename: "rustup-init.exe".into(),
        source: format!("rust-lang.org 官方 SHA-256 校验文件（{channel} 渠道共用同一引导程序）"),
    };
    vec![
        VersionInfo {
            version: "stable".into(),
            date: None,
            size_bytes: None,
            note: Some("稳定版工具链".into()),
            url: None,
            verification: rustup_checksum("stable"),
        },
        VersionInfo {
            version: "beta".into(),
            date: None,
            size_bytes: None,
            note: Some("公测版工具链".into()),
            url: None,
            verification: rustup_checksum("beta"),
        },
        VersionInfo {
            version: "nightly".into(),
            date: None,
            size_bytes: None,
            note: Some("每日构建".into()),
            url: None,
            verification: rustup_checksum("nightly"),
        },
    ]
}

// ---------- GitHub Releases 通用 ----------

#[derive(Deserialize)]
struct GhAsset {
    name: String,
    size: Option<u64>,
    browser_download_url: String,
    #[serde(default)]
    digest: Option<String>,
}

#[derive(Deserialize)]
struct GhRelease {
    tag_name: String,
    published_at: Option<String>,
    prerelease: Option<bool>,
    #[serde(default)]
    assets: Vec<GhAsset>,
}

async fn github_releases(repo: &str) -> Result<Vec<GhRelease>> {
    let list: Vec<GhRelease> = http()?
        .get(format!("https://api.github.com/repos/{repo}/releases?per_page=100"))
        .header("Accept", "application/vnd.github+json")
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(list)
}

fn github_asset_verification(asset: &GhAsset, repo: &str) -> VerificationInfo {
    let Some(digest) = asset.digest.as_deref() else {
        return VerificationInfo::unverified();
    };
    let Some(hash) = digest.strip_prefix("sha256:") else {
        return VerificationInfo::Invalid {
            reason: format!("GitHub Releases API {repo} 返回了不支持的摘要格式"),
        };
    };
    publisher_sha256(Some(hash), &format!("GitHub Releases API: {repo}"))
}

/// PHP: windows.php.net releases 目录(仅 8.x,Thread Safe x64 包)
async fn php_releases() -> Result<Vec<VersionInfo>> {
    let html = http()?
        .get("https://windows.php.net/downloads/releases/")
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    parse_php_releases(&html)
}

fn parse_php_releases(html: &str) -> Result<Vec<VersionInfo>> {
    // href 形如 /downloads/releases/php-8.4.5-Win32-vs17-x64.zip
    let mut out: Vec<VersionInfo> = Vec::new();
    for name in extract_all_between(&html, "href=\"", "\"") {
        let Some(file) = name.rsplit('/').next() else { continue };
        let Some(rest) = file.strip_prefix("php-") else { continue };
        // 仅取 Thread Safe x64 zip(排除 nts/源码/debug)
        let Some((ver, tail)) = rest.split_once("-Win32-") else { continue };
        if !tail.ends_with("-x64.zip")
            || !ver.chars().next().is_some_and(|c| c == '8')
            || ver.to_ascii_lowercase().ends_with("-nts")
        {
            continue;
        }
        if out.iter().any(|v: &VersionInfo| v.version == ver) {
            continue;
        }
        out.push(VersionInfo {
            version: ver.to_string(),
            date: None,
            size_bytes: None,
            note: None,
            url: Some(format!("https://windows.php.net/downloads/releases/{file}")),
            // windows.php.net publishes one sha256sum.txt covering the release
            // archives; the parser matches the exact file name inside it.
            verification: VerificationInfo::ChecksumFile {
                url: "https://windows.php.net/downloads/releases/sha256sum.txt".into(),
                filename: file.to_string(),
                source: "windows.php.net 官方 SHA-256 校验文件".into(),
            },
        });
    }
    out.sort_by(|a, b| version_cmp(&b.version, &a.version));
    out.truncate(40);
    if out.is_empty() {
        return Err(AppError::msg("未能解析 PHP 版本列表"));
    }
    Ok(out)
}

/// LLVM: GitHub releases(LLVM-{v}-win64.exe).
async fn llvm_releases() -> Result<Vec<VersionInfo>> {
    let list = github_releases("llvm/llvm-project").await?;
    parse_llvm_releases(list)
}

fn parse_llvm_releases(list: Vec<GhRelease>) -> Result<Vec<VersionInfo>> {
    let mut out: Vec<VersionInfo> = Vec::new();
    for r in list {
        let Some(tag) = r.tag_name.strip_prefix("llvmorg-") else { continue };
        if tag.contains('-') && !tag.chars().next().is_some_and(|c| c.is_ascii_digit()) {
            continue;
        }
        // 版本形如 20.1.0(跳过 llvmorg-init 等)
        if tag.split('.').count() < 2 {
            continue;
        }
        let exe_name = format!("LLVM-{tag}-win64.exe");
        let Some(asset) = r.assets.iter().find(|a| a.name == exe_name) else { continue };
        out.push(VersionInfo {
            version: tag.to_string(),
            date: r.published_at,
            size_bytes: asset.size,
            note: Some(match r.prerelease == Some(true) {
                true => "Windows x64 EXE；预发布".into(),
                false => "Windows x64 EXE；新版 MSI 暂不支持".into(),
            }),
            url: Some(asset.browser_download_url.clone()),
            verification: github_asset_verification(asset, "llvm/llvm-project"),
        });
    }
    if out.is_empty() {
        return Err(AppError::msg("未能解析 LLVM 版本列表"));
    }
    Ok(out)
}

/// Zig: ziglang.org download index JSON
async fn zig_releases() -> Result<Vec<VersionInfo>> {
    let json: serde_json::Value = http()?
        .get("https://ziglang.org/download/index.json")
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    parse_zig_releases(json)
}

fn parse_zig_releases(json: serde_json::Value) -> Result<Vec<VersionInfo>> {
    let mut out: Vec<VersionInfo> = Vec::new();
    if let serde_json::Value::Object(map) = json {
        for (ver, entry) in map {
            if ver == "master" || ver.split('.').count() < 2 {
                continue;
            }
            let win = entry.get("x86_64-windows");
            let tarball = win
                .and_then(|w| w.get("tarball"))
                .and_then(|t| t.as_str())
                .map(|s| s.to_string());
            let Some(tarball) = tarball.filter(|url| {
                url.starts_with("https://ziglang.org/") && url.ends_with(".zip")
            }) else {
                continue;
            };
            let size = win
                .and_then(|w| w.get("size"))
                .and_then(|s| s.as_str())
                .and_then(|s| s.parse().ok());
            let date = entry
                .get("date")
                .and_then(|d| d.as_str())
                .map(|s| s.to_string());
            let verification = win
                .and_then(|asset| asset.get("shasum"))
                .and_then(|hash| hash.as_str())
                .map(|hash| publisher_sha256(Some(hash), "Zig 官方下载索引"))
                .unwrap_or_else(VerificationInfo::unverified);
            out.push(VersionInfo {
                version: ver,
                date,
                size_bytes: size,
                note: None,
                url: Some(tarball),
                verification,
            });
        }
    }
    out.sort_by(|a, b| version_cmp(&b.version, &a.version));
    if out.is_empty() {
        return Err(AppError::msg("未能解析 Zig 版本列表"));
    }
    Ok(out)
}

/// Deno: GitHub releases(deno-x86_64-pc-windows-msvc.zip)
async fn deno_releases() -> Result<Vec<VersionInfo>> {
    let list = github_releases("denoland/deno").await?;
    let mut out: Vec<VersionInfo> = Vec::new();
    for r in list {
        let Some(tag) = r.tag_name.strip_prefix('v') else { continue };
        if tag.split('.').count() < 2 {
            continue;
        }
        let Some(asset) = r
            .assets
            .iter()
            .find(|a| a.name == "deno-x86_64-pc-windows-msvc.zip")
        else {
            continue;
        };
        out.push(VersionInfo {
            version: tag.to_string(),
            date: r.published_at,
            size_bytes: asset.size,
            note: if r.prerelease == Some(true) {
                Some("预发布".into())
            } else {
                None
            },
            url: Some(asset.browser_download_url.clone()),
            verification: github_asset_verification(asset, "denoland/deno"),
        });
    }
    if out.is_empty() {
        return Err(AppError::msg("未能解析 Deno 版本列表"));
    }
    Ok(out)
}

/// Bun: GitHub releases(bun-windows-x64.zip)
async fn bun_releases() -> Result<Vec<VersionInfo>> {
    let list = github_releases("oven-sh/bun").await?;
    let mut out: Vec<VersionInfo> = Vec::new();
    for r in list {
        let Some(tag) = r.tag_name.strip_prefix("bun-v") else { continue };
        if tag.split('.').count() < 2 {
            continue;
        }
        let Some(asset) = r.assets.iter().find(|a| a.name == "bun-windows-x64.zip") else {
            continue;
        };
        out.push(VersionInfo {
            version: tag.to_string(),
            date: r.published_at,
            size_bytes: asset.size,
            note: if r.prerelease == Some(true) {
                Some("预发布".into())
            } else {
                None
            },
            url: Some(asset.browser_download_url.clone()),
            verification: github_asset_verification(asset, "oven-sh/bun"),
        });
    }
    if out.is_empty() {
        return Err(AppError::msg("未能解析 Bun 版本列表"));
    }
    Ok(out)
}

/// Git: GitHub releases(PortableGit-<v>-64-bit.7z.exe,便携自解压版)
async fn git_releases() -> Result<Vec<VersionInfo>> {
    let list = github_releases("git-for-windows/git").await?;
    let mut out: Vec<VersionInfo> = Vec::new();
    for r in list {
        // 跳过 rc 预发布
        if r.tag_name.contains("-rc") {
            continue;
        }
        let Some(tag) = r.tag_name.strip_prefix('v') else { continue };
        // 仅取 64 位便携版自解压包
        let Some(asset) = r.assets.iter().find(|a| {
            a.name.starts_with("PortableGit-") && a.name.ends_with("-64-bit.7z.exe")
        }) else {
            continue;
        };
        out.push(VersionInfo {
            version: tag.to_string(),
            date: r.published_at,
            size_bytes: asset.size,
            note: Some("便携版".into()),
            url: Some(asset.browser_download_url.clone()),
            verification: github_asset_verification(asset, "git-for-windows/git"),
        });
    }
    if out.is_empty() {
        return Err(AppError::msg("未能解析 Git 版本列表"));
    }
    Ok(out)
}

/// GitHub CLI: GitHub releases(gh_<v>_windows_amd64.zip)
async fn gh_releases() -> Result<Vec<VersionInfo>> {
    let list = github_releases("cli/cli").await?;
    let mut out: Vec<VersionInfo> = Vec::new();
    for r in list {
        let Some(tag) = r.tag_name.strip_prefix('v') else { continue };
        if tag.split('.').count() < 2 {
            continue;
        }
        let asset_name = format!("gh_{tag}_windows_amd64.zip");
        let Some(asset) = r.assets.iter().find(|a| a.name == asset_name) else {
            continue;
        };
        out.push(VersionInfo {
            version: tag.to_string(),
            date: r.published_at,
            size_bytes: asset.size,
            note: None,
            url: Some(asset.browser_download_url.clone()),
            verification: github_asset_verification(asset, "cli/cli"),
        });
    }
    if out.is_empty() {
        return Err(AppError::msg("未能解析 GitHub CLI 版本列表"));
    }
    Ok(out)
}

/// C/C++(WinLibs MinGW-w64): GitHub releases,仅取 x86_64 posix seh UCRT 构建的 zip
async fn mingw_releases() -> Result<Vec<VersionInfo>> {
    let list = github_releases("brechtsanders/winlibs_mingw").await?;
    let mut out: Vec<VersionInfo> = Vec::new();
    for r in list {
        // 仅 UCRT 运行时(现代标准);tag 形如 16.2.0posix-14.0.0-ucrt-r1
        if !r.tag_name.contains("ucrt") {
            continue;
        }
        let Some(asset) = r.assets.iter().find(|a| {
            // 资产名形如 winlibs-x86_64-posix-seh-gcc-16.2.0-mingw-w64ucrt-14.0.0-r1.zip
            a.name.starts_with("winlibs-x86_64-posix-seh-gcc-")
                && a.name.ends_with(".zip")
                && a.name.contains("w64ucrt")
        }) else {
            continue;
        };
        // 从资产名提取 GCC 版本:winlibs-x86_64-posix-seh-gcc-16.2.0-mingw-...
        let gcc_ver = asset
            .name
            .split("gcc-")
            .nth(1)
            .and_then(|s| s.split('-').next())
            .unwrap_or(&r.tag_name)
            .to_string();
        out.push(VersionInfo {
            version: r.tag_name.clone(),
            date: r.published_at,
            size_bytes: asset.size,
            note: Some(format!("GCC {gcc_ver}")),
            url: Some(asset.browser_download_url.clone()),
            verification: github_asset_verification(asset, "brechtsanders/winlibs_mingw"),
        });
    }
    if out.is_empty() {
        return Err(AppError::msg("未能解析 C/C++ 版本列表"));
    }
    Ok(out)
}

// ---------- 工具函数 ----------

fn extract_all_between(s: &str, start: &str, end: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = s;
    while let Some(i) = rest.find(start) {
        let after = &rest[i + start.len()..];
        if let Some(j) = after.find(end) {
            out.push(after[..j].to_string());
            rest = &after[j + end.len()..];
        } else {
            break;
        }
    }
    out
}

/// 语义化版本比较(a vs b),支持 17.0.12+7 / 8u412b08 / 3.12.4 / stable
pub fn version_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let pa = parse_ver(a);
    let pb = parse_ver(b);
    pa.cmp(&pb)
}

/// 降序排序比较器,稳定版排在预发布(rc/milestone/alpha/beta)之前
fn prerelease_last(a: &str, b: &str) -> std::cmp::Ordering {
    let pa = is_prerelease(a) as u8;
    let pb = is_prerelease(b) as u8;
    pa.cmp(&pb).then_with(|| version_cmp(b, a))
}

fn is_prerelease(v: &str) -> bool {
    let l = v.to_lowercase();
    l.contains("rc") || l.contains("milestone") || l.contains("alpha") || l.contains("beta")
}

fn parse_ver(v: &str) -> Vec<u64> {
    v.split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .map(|s| s.parse().unwrap_or(0))
        .collect()
}

fn parse_sha256(value: &str) -> Option<String> {
    let value = value.trim();
    (value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit()))
        .then(|| value.to_ascii_lowercase())
}

fn publisher_sha256(value: Option<&str>, source: &str) -> VerificationInfo {
    match value {
        Some(value) => match parse_sha256(value) {
            Some(hash) => VerificationInfo::trusted_sha256(hash, source),
            None => VerificationInfo::Invalid {
                reason: format!("{source} 提供了格式无效的 SHA-256"),
            },
        },
        None => VerificationInfo::unverified(),
    }
}

fn go_asset_verification(base: &str, value: Option<&str>) -> VerificationInfo {
    if base.starts_with("https://go.dev/") {
        publisher_sha256(value, "Go 官方下载索引")
    } else {
        VerificationInfo::unverified()
    }
}

fn downgrade_cached_version(version: &mut VersionInfo) {
    version.verification = VerificationInfo::unverified();
    let previous = version.note.take().unwrap_or_default();
    version.note = Some(format!(
        "离线缓存（在线查询失败，非最新保证；请刷新或切换源）{}",
        if previous.is_empty() {
            String::new()
        } else {
            format!("；{previous}")
        }
    ));
}

// ---------- 缓存 ----------

#[derive(Serialize, Deserialize, Default)]
struct VersionCache {
    #[serde(default)]
    map: HashMap<String, Vec<VersionInfo>>,
}

fn cache_key(env_type: EnvType, source: &str) -> String {
    let t = serde_json::to_string(&env_type)
        .unwrap_or_default()
        .trim_matches('"')
        .to_string();
    format!("{t}:{source}")
}

fn save_cache(
    env_type: EnvType,
    source: &str,
    versions: &[VersionInfo],
    cache_path: &PathBuf,
) {
    let mut cache: VersionCache = std::fs::read_to_string(cache_path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    cache
        .map
        .insert(cache_key(env_type, source), versions.to_vec());
    if let Some(parent) = cache_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let tmp = cache_path.with_extension("tmp");
    if std::fs::write(&tmp, serde_json::to_string(&cache).unwrap_or_default()).is_ok() {
        let _ = std::fs::rename(&tmp, cache_path);
    }
}

fn load_cache(env_type: EnvType, source: &str, cache_path: &PathBuf) -> Option<Vec<VersionInfo>> {
    let cache: VersionCache =
        serde_json::from_str(&std::fs::read_to_string(cache_path).ok()?).ok()?;
    cache.map.get(&cache_key(env_type, source)).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_inventory_filters_windows_zip_sorts_and_keeps_history() {
        let mut input: Vec<_> = (0..150).map(|v| serde_json::json!({
            "version": format!("v22.{v}.0"), "date": "2026-09-01", "files": ["win-x64-zip"], "lts": "Jod"
        })).collect();
        input.push(input[100].clone());
        input.push(serde_json::json!({"version":"v99.0.0", "files":["linux-x64"]}));
        let rows = parse_node_index(&serde_json::to_string(&input).unwrap()).unwrap();
        assert_eq!(rows.len(), 150);
        assert_eq!(rows[0].version, "22.149.0");
        assert_eq!(rows.last().unwrap().version, "22.0.0");
        assert_eq!(rows[0].note.as_deref(), Some("LTS"));
        assert!(parse_node_index("[]").is_err());
        assert!(parse_node_index("not json").is_err());
    }

    #[test]
    fn mirror_jdk_asset_retains_single_zip_suffix() {
        let html = "<a href=\"OpenJDK25U-jdk_x64_windows_hotspot_25.0.1_8.zip\">jdk</a>";
        let names = extract_all_between(html, "href=\"OpenJDK", "\"");
        assert_eq!(jdk_version_from_filename(&names[0], 25).as_deref(), Some("25.0.1+8"));
        assert_eq!(format!("OpenJDK{}", names[0]), "OpenJDK25U-jdk_x64_windows_hotspot_25.0.1_8.zip");
    }

    #[test]
    fn python_list_uses_release_date_and_only_matching_stable_x64_installers() {
        let releases = vec![
            PythonRelease {
                name: "Python 3.15.0".into(),
                release_date: Some("2026-10-09T13:54:59Z".into()),
                is_published: true,
                pre_release: false,
                resource_uri: "https://www.python.org/api/v2/downloads/release/1162/".into(),
            },
            PythonRelease {
                name: "Python 3.11.14".into(),
                release_date: Some("2025-10-09T17:08:22Z".into()),
                is_published: true,
                pre_release: false,
                resource_uri: "https://www.python.org/api/v2/downloads/release/1071/".into(),
            },
            PythonRelease {
                name: "Python 3.16.0a1".into(),
                release_date: Some("2026-10-13T19:02:12Z".into()),
                is_published: true,
                pre_release: true,
                resource_uri: "https://www.python.org/api/v2/downloads/release/1164/".into(),
            },
        ];
        let files = vec![
            PythonReleaseFile {
                name: "Windows installer (64-bit)".into(),
                url: "https://www.python.org/ftp/python/3.15.0/python-3.15.0-amd64.exe".into(),
                filesize: Some(45_383_680),
                release: "https://www.python.org/api/v2/downloads/release/1162/".into(),
                sha256_sum: Some("67d9a63eb145a34aa939f02eadbf8c57f21fcdd6decb901ccac578d2d1f01d6f".into()),
            },
            PythonReleaseFile {
                name: "Gzipped source tarball".into(),
                url: "https://www.python.org/ftp/python/3.11.14/Python-3.11.14.tgz".into(),
                filesize: Some(26_561_042),
                release: "https://www.python.org/api/v2/downloads/release/1071/".into(),
                sha256_sum: Some("67d9a63eb145a34aa939f02eadbf8c57f21fcdd6decb901ccac578d2d1f01d6f".into()),
            },
            PythonReleaseFile {
                name: "Windows installer (64-bit)".into(),
                url: "https://www.python.org/ftp/python/3.16.0/python-3.16.0a1-amd64.exe".into(),
                filesize: Some(1),
                release: "https://www.python.org/api/v2/downloads/release/1164/".into(),
                sha256_sum: None,
            },
            PythonReleaseFile {
                name: "Windows installer (64-bit)".into(),
                url: "https://www.python.org/ftp/python/3.15.0/python-3.15.0-amd64.exe".into(),
                filesize: Some(1),
                release: "https://www.python.org/api/v2/downloads/release/9999/".into(),
                sha256_sum: None,
            },
        ];
        let rows = parse_python_assets(releases, files, true).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].version, "3.15.0");
        assert_eq!(rows[0].date.as_deref(), Some("2026-10-09"));
        assert_eq!(rows[0].size_bytes, Some(45_383_680));
        assert!(matches!(
            &rows[0].verification,
            VerificationInfo::TrustedSha256 { source, .. } if source == "Python 官方 release_file API"
        ));
        assert_eq!(rows[0].url.as_deref(), Some("https://mirrors.huaweicloud.com/python/3.15.0/python-3.15.0-amd64.exe"));
    }

    #[test]
    fn python_mirror_directory_parser_requires_exact_windows_x64_asset() {
        let root = r#"<a href="3.14.8/">3.14.8/</a>
<a href="3.15.0/">3.15.0/</a>
<a href="3.15.0a1/">3.15.0a1/</a>
<a href="../">../</a>"#;
        assert_eq!(
            parse_python_mirror_directories(root),
            vec!["3.15.0", "3.14.8"]
        );
        assert!(parse_python_mirror_asset(
            r#"<a href="python-3.15.0-amd64.exe">installer</a>"#,
            "python-3.15.0-amd64.exe"
        ));
        assert!(!parse_python_mirror_asset(
            r#"<a href="python-3.15.0-arm64.exe">installer</a>"#,
            "python-3.15.0-amd64.exe"
        ));
        assert!(!parse_python_mirror_asset(
            r#"<a href="../python-3.15.0-amd64.exe">installer</a>"#,
            "python-3.15.0-amd64.exe"
        ));
    }

    #[tokio::test]
    async fn python_mirror_listing_survives_official_date_api_failure() {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            for _ in 0..3 {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = [0; 2048];
                let count = stream.read(&mut request).unwrap();
                let line = String::from_utf8_lossy(&request[..count]);
                let path = line.split_whitespace().nth(1).unwrap_or("/");
                let (status, body, content_type) = match path {
                    "/python/" => (200, r#"<a href="3.15.0/">3.15.0/</a>"#, "text/html"),
                    "/api/releases" => (503, "official API unavailable", "text/plain"),
                    "/python/3.15.0/" => (
                        200,
                        r#"<a href="python-3.15.0-amd64.exe">python-3.15.0-amd64.exe</a>"#,
                        "text/html",
                    ),
                    other => panic!("unexpected fixture request: {other}"),
                };
                let reason = if status == 200 { "OK" } else { "Service Unavailable" };
                write!(
                    stream,
                    "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .unwrap();
            }
        });
        let base = format!("http://{address}/");
        let rows = python_mirror_releases_from(
            &format!("{base}python/"),
            &format!("{base}api/releases"),
        )
        .await
        .unwrap();
        server.join().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].version, "3.15.0");
        assert_eq!(rows[0].date, None);
        assert!(rows[0]
            .note
            .as_deref()
            .is_some_and(|note| note.contains("发布日期未知")));
        assert_eq!(
            rows[0].url.as_deref(),
            Some(format!("{base}python/3.15.0/python-3.15.0-amd64.exe").as_str())
        );
    }

    #[test]
    fn apache_listing_dates_keep_day_and_month() {
        assert_eq!(extract_date_from_row("  2026-09-30 20:42     -"), Some("2026-09-30 20:42".into()));
        assert_eq!(extract_date_from_row("  30-Sep-2026 20:42     -"), Some("2026-09-30 20:42".into()));
    }

    #[test]
    fn publisher_hash_metadata_is_trusted_only_when_well_formed() {
        let valid = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        assert!(matches!(
            publisher_sha256(Some(valid), "fixture publisher"),
            VerificationInfo::TrustedSha256 { expected, .. } if expected == valid
        ));
        assert!(matches!(
            publisher_sha256(None, "fixture publisher"),
            VerificationInfo::Unverified { .. }
        ));
        assert!(matches!(
            publisher_sha256(Some("not-a-digest"), "fixture publisher"),
            VerificationInfo::Invalid { .. }
        ));

        let asset = GhAsset {
            name: "fixture.zip".into(),
            size: Some(10),
            browser_download_url: "https://github.com/example/repo/releases/download/v1/fixture.zip".into(),
            digest: Some(format!("sha256:{valid}")),
        };
        assert!(matches!(
            github_asset_verification(&asset, "example/repo"),
            VerificationInfo::TrustedSha256 { .. }
        ));
        let malformed = GhAsset { digest: Some("sha512:abc".into()), ..asset };
        assert!(matches!(
            github_asset_verification(&malformed, "example/repo"),
            VerificationInfo::Invalid { .. }
        ));
        assert!(matches!(
            go_asset_verification("https://go.dev/dl/?mode=json", Some(valid)),
            VerificationInfo::TrustedSha256 { .. }
        ));
        assert!(matches!(
            go_asset_verification("https://golang.google.cn/dl/?mode=json", Some(valid)),
            VerificationInfo::Unverified { .. }
        ));
    }

    #[test]
    fn checksum_file_parser_requires_one_exact_valid_asset_entry() {
        let hash = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let body = format!(
            "{hash}  other.zip\n{hash} *tool.zip\n"
        );
        assert_eq!(parse_sha256_checksum_file(&body, "tool.zip").unwrap(), hash);
        assert!(parse_sha256_checksum_file(&body, "missing.zip").is_err());
        assert!(parse_sha256_checksum_file(&format!("bad  tool.zip\n"), "tool.zip").is_err());
        assert!(parse_sha256_checksum_file(
            &format!("{hash} tool.zip\n{} tool.zip\n", "f".repeat(64)),
            "tool.zip",
        )
        .unwrap_err()
        .to_string()
        .contains("冲突"));
        assert!(parse_sha256_checksum_file(&body, "../tool.zip").is_err());
    }

    #[tokio::test]
    async fn rust_uses_the_publisher_checksum_for_the_official_lead_binary_only() {
        // The official channel resolves rustup-init.exe.sha256 into a trusted digest.
        let channel = rust_channels()
            .into_iter()
            .find(|row| row.version == "stable")
            .unwrap();
        assert!(matches!(
            channel.verification,
            VerificationInfo::ChecksumFile { ref url, ref filename, .. }
                if url == "https://static.rust-lang.org/rustup/dist/x86_64-pc-windows-msvc/rustup-init.exe.sha256"
                    && filename == "rustup-init.exe"
        ));

        let official = download_spec_with_verification(
            EnvType::Rust,
            "stable",
            "target",
            SOURCE_OFFICIAL,
            None,
            Some(&channel.verification),
            None,
        )
        .unwrap();
        // The caller-supplied verification must survive instead of being dropped.
        assert!(matches!(
            official.verification,
            VerificationInfo::ChecksumFile { .. }
        ));

        // A mirror URL is a different file until proven identical: the official
        // digest must never be attached to it.
        let mirror = download_spec_with_verification(
            EnvType::Rust,
            "stable",
            "target",
            SOURCE_MIRROR,
            None,
            Some(&VerificationInfo::trusted_sha256("a".repeat(64), "official")),
            None,
        )
        .unwrap();
        assert_eq!(mirror.verification.expected_sha256(), None);
        assert!(matches!(
            mirror.verification,
            VerificationInfo::Unverified { .. }
        ));
    }

    #[test]
    fn php_and_maven_state_why_their_publisher_checksum_cannot_be_used() {
        let html = r#"<a href="/downloads/releases/php-8.5.11-Win32-vs17-x64.zip">x</a>
                      <a href="/downloads/releases/php-8.5.11-nts-Win32-vs17-x64.zip">y</a>"#;
        let rows = parse_php_releases(html).unwrap();
        assert!(matches!(
            rows[0].verification,
            VerificationInfo::ChecksumFile { ref filename, .. } if filename == "php-8.5.11-Win32-vs17-x64.zip"
        ));

        // Maven's publisher only offers SHA-512, which the verifier does not
        // accept; the row must say that rather than look undocumented.
        let maven = VerificationInfo::Unverified {
            reason: "Apache 仅提供 SHA-512 校验文件；当前只核对 SHA-256，故标为未验证".into(),
        };
        assert!(maven.expected_sha256().is_none());
        assert!(!maven.is_trusted());
    }

    #[tokio::test]
    async fn verification_resolution_keeps_trusted_metadata_and_rejects_insecure_checksum_files() {
        let trusted = VerificationInfo::trusted_sha256("b".repeat(64), "official fixture");
        assert_eq!(resolve_verification(&trusted).await.unwrap(), trusted);

        let unverified = VerificationInfo::unverified();
        assert_eq!(resolve_verification(&unverified).await.unwrap(), unverified);

        let insecure = VerificationInfo::ChecksumFile {
            url: "http://example.test/tool.zip.sha256".into(),
            filename: "tool.zip".into(),
            source: "fixture".into(),
        };
        assert!(resolve_verification(&insecure).await.is_err());
    }

    #[test]
    fn checksum_file_listing_other_assets_is_not_a_silent_match() {
        let hash = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        // The file is valid but describes different assets: version or asset
        // naming changed, so the digest must not be reused.
        let other_assets = format!("{hash}  tool-old.zip\n{hash} *tool-src.zip\n");
        let error = parse_sha256_checksum_file(&other_assets, "tool.zip")
            .unwrap_err()
            .to_string();
        assert!(error.contains("未列出目标资产"), "{error}");
        assert!(error.contains("2 个其他条目"), "{error}");

        // An empty file is a different condition and keeps the shorter message.
        let empty = parse_sha256_checksum_file("\n\n", "tool.zip")
            .unwrap_err()
            .to_string();
        assert!(empty.contains("缺少目标资产"), "{empty}");
        assert!(!empty.contains("其他条目"), "{empty}");
    }

    #[test]
    fn offline_cache_never_keeps_a_trusted_digest_for_a_replaced_asset() {
        // A cached row cannot prove the current publisher asset, so the digest
        // must be dropped even though the URL is unchanged.
        let mut row = VersionInfo {
            version: "1.2.3".into(),
            date: None,
            size_bytes: Some(42),
            note: None,
            url: Some("https://example.test/tool.zip".into()),
            verification: VerificationInfo::trusted_sha256(
                "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                "publisher fixture",
            ),
        };
        downgrade_cached_version(&mut row);
        assert!(
            row.verification.expected_sha256().is_none(),
            "cached verification must not survive as trusted"
        );
        assert!(matches!(row.verification, VerificationInfo::Unverified { .. }));
        // The row still pins the exact URL and size a user selected.
        assert_eq!(row.url.as_deref(), Some("https://example.test/tool.zip"));
        assert_eq!(row.size_bytes, Some(42));
    }

    #[test]
    fn official_node_catalog_uses_pinned_checksum_file_and_mirror_is_unverified() {
        let json = r#"[{"version":"v22.1.0","date":"2026-01-01","files":["win-x64-zip"]}]"#;
        let official = parse_node_index_with_checksum(json, true).unwrap();
        assert!(matches!(
            &official[0].verification,
            VerificationInfo::ChecksumFile { url, filename, .. }
                if url == "https://nodejs.org/dist/v22.1.0/SHASUMS256.txt"
                    && filename == "node-v22.1.0-win-x64.zip"
        ));
        let mirror = parse_node_index_with_checksum(json, false).unwrap();
        assert!(matches!(
            mirror[0].verification,
            VerificationInfo::Unverified { .. }
        ));
    }

    #[test]
    fn cached_version_metadata_is_downgraded_and_preserves_context() {
        let mut version = VersionInfo {
            version: "1.2.3".into(),
            date: None,
            size_bytes: Some(42),
            note: Some("LTS".into()),
            url: None,
            verification: VerificationInfo::trusted_sha256(
                "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                "official fixture",
            ),
        };
        downgrade_cached_version(&mut version);
        assert!(matches!(version.verification, VerificationInfo::Unverified { .. }));
        assert!(version.note.as_deref().unwrap().contains("离线缓存"));
        assert!(version.note.as_deref().unwrap().contains("LTS"));
        assert_eq!(version.size_bytes, Some(42));
    }

    #[test]
    fn selected_row_metadata_and_size_are_bound_to_the_exact_url() {
        let url = "https://example.test/tool.zip";
        let digest = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let row = VersionInfo {
            version: "1.2.3".into(),
            date: None,
            size_bytes: Some(99),
            note: None,
            url: Some(url.into()),
            verification: VerificationInfo::trusted_sha256(digest, "publisher fixture"),
        };
        assert!(select_version(&[row.clone()], "1.2.3", Some(url)).is_ok());
        assert!(select_version(&[row.clone()], "1.2.3", Some("https://example.test/other.zip")).is_err());
        let spec = download_spec_with_verification(
            EnvType::Zig,
            "1.2.3",
            "target",
            SOURCE_OFFICIAL,
            Some(url),
            Some(&row.verification),
            row.size_bytes,
        )
        .unwrap();
        assert_eq!(spec.expected_size, Some(99));
        assert_eq!(spec.verification.expected_sha256(), Some(digest));
    }

    #[test]
    fn zig_uses_the_exact_windows_asset_url_from_official_index() {
        let index = serde_json::json!({
            "0.14.0": {
                "date": "2025-03-05",
                "x86_64-windows": {
                    "tarball": "https://ziglang.org/download/0.14.0/zig-windows-x86_64-0.14.0.zip",
                    "size": "78000000"
                }
            },
            "0.14.1": {
                "date": "2025-04-18",
                "x86_64-windows": {
                    "tarball": "https://ziglang.org/download/0.14.1/zig-x86_64-windows-0.14.1.zip",
                    "size": "78100000"
                }
            },
            "0.15.0": {
                "date": "2025-08-01",
                "x86_64-linux": {
                    "tarball": "https://ziglang.org/download/0.15.0/zig-x86_64-linux-0.15.0.tar.xz",
                    "size": "1"
                }
            }
        });
        let rows = parse_zig_releases(index).unwrap();
        assert_eq!(rows.len(), 2);
        for row in &rows {
            let spec = download_spec(
                EnvType::Zig,
                &row.version,
                "target",
                SOURCE_OFFICIAL,
                row.url.as_deref(),
            )
            .unwrap();
            assert_eq!(spec.url, row.url.as_deref().unwrap());
        }
        assert!(download_spec(EnvType::Zig, "0.14.0", "target", SOURCE_OFFICIAL, None).is_err());
    }

    #[test]
    fn every_environment_has_a_consistent_windows_download_spec() {
        let cases = [
            (EnvType::Jdk, "27+35", Some("https://example.test/jdk.zip"), "zip"),
            (EnvType::Python, "3.15.0", Some("https://www.python.org/ftp/python/3.15.0/python-3.15.0-amd64.exe"), "exe"),
            (EnvType::Node, "26.12.0", None, "zip"),
            (EnvType::Go, "1.27.2", None, "zip"),
            (EnvType::Rust, "stable", None, "exe"),
            (EnvType::Maven, "3.10.0", None, "zip"),
            (EnvType::Gradle, "9.8.1", None, "zip"),
            (EnvType::Php, "8.5.0", Some("https://windows.php.net/downloads/releases/php-8.5.0-Win32-vs17-x64.zip"), "zip"),
            (EnvType::Llvm, "22.1.8", Some("https://github.com/llvm/llvm-project/releases/download/llvmorg-22.1.8/LLVM-22.1.8-win64.exe"), "exe"),
            (EnvType::Zig, "0.17.0", Some("https://ziglang.org/download/0.17.0/zig-x86_64-windows-0.17.0.zip"), "zip"),
            (EnvType::Deno, "2.9.7", Some("https://github.com/denoland/deno/releases/download/v2.9.7/deno-x86_64-pc-windows-msvc.zip"), "zip"),
            (EnvType::Bun, "1.4.3", Some("https://github.com/oven-sh/bun/releases/download/bun-v1.4.3/bun-windows-x64.zip"), "zip"),
            (EnvType::Git, "2.56.0.windows.2", Some("https://github.com/git-for-windows/git/releases/download/v2.56.0.windows.2/PortableGit-2.56.0.2-64-bit.7z.exe"), "exe"),
            (EnvType::Gh, "2.102.0", Some("https://github.com/cli/cli/releases/download/v2.102.0/gh_2.102.0_windows_amd64.zip"), "zip"),
            (EnvType::Mingw, "16.2.0posix-14.0.0-ucrt-r2", Some("https://github.com/brechtsanders/winlibs_mingw/releases/download/16.2.0posix-14.0.0-ucrt-r2/winlibs-x86_64-posix-seh-gcc-16.2.0-mingw-w64ucrt-14.0.0-r2.zip"), "zip"),
            (EnvType::Adb, "37.0.1", Some("https://dl.google.com/android/repository/platform-tools_r37.0.1-win.zip"), "zip"),
        ];
        assert_eq!(cases.len(), crate::types::ALL_ENV_TYPES.len());
        for (kind, version, hint, extension) in cases {
            let spec = download_spec(kind, version, "C:\\EnvCon\\target", SOURCE_OFFICIAL, hint)
                .unwrap_or_else(|error| panic!("{kind:?}: {error}"));
            assert_eq!(spec.method.file_ext(), extension, "{kind:?}");
            assert!(spec.url.starts_with("https://"), "{kind:?}: {}", spec.url);
        }
    }

    #[test]
    fn php_catalog_does_not_label_nts_archive_as_thread_safe() {
        let html = r#"<a href="/downloads/releases/php-8.5.2-nts-Win32-vs17-x64.zip">NTS</a>
<a href="/downloads/releases/php-8.5.2-Win32-vs17-x64.zip">TS</a>
<a href="/downloads/releases/php-8.5.2-Win32-vs17-x86.zip">x86</a>"#;
        let rows = parse_php_releases(html).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].version, "8.5.2");
        assert!(rows[0].url.as_deref().unwrap().ends_with("php-8.5.2-Win32-vs17-x64.zip"));
    }

    #[test]
    fn llvm_skips_msi_only_releases_and_keeps_supported_exe_explicit() {
        let asset = |name: &str| GhAsset {
            name: name.into(),
            size: Some(1),
            browser_download_url: format!("https://github.com/llvm/llvm-project/releases/download/{name}"),
            digest: None,
        };
        let releases = vec![
            GhRelease {
                tag_name: "llvmorg-23.1.3".into(),
                published_at: Some("2026-10-06T14:53:00Z".into()),
                prerelease: Some(false),
                assets: vec![asset("LLVM-23.1.3-win64.msi")],
            },
            GhRelease {
                tag_name: "llvmorg-22.1.8".into(),
                published_at: Some("2026-09-16T14:53:00Z".into()),
                prerelease: Some(false),
                assets: vec![asset("LLVM-22.1.8-win64.exe")],
            },
        ];
        let rows = parse_llvm_releases(releases).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].version, "22.1.8");
        assert!(rows[0].note.as_deref().unwrap().contains("MSI 暂不支持"));
        assert!(download_spec(EnvType::Llvm, "23.1.3", "target", SOURCE_OFFICIAL,
            Some("https://github.com/llvm/llvm-project/releases/download/llvmorg-23.1.3/LLVM-23.1.3-win64.msi"))
            .is_err());
    }

    #[tokio::test]
    #[ignore = "read-only network verification against live Node indexes"]
    async fn live_node_indexes() {
        for url in ["https://nodejs.org/dist/index.json", "https://npmmirror.com/mirrors/node/index.json"] {
            let rows = node_index(url).await.unwrap();
            assert!(rows.len() > 100);
            println!("{url}: {} versions; newest={} date={:?}", rows.len(), rows[0].version, rows[0].date);
        }
    }

    #[tokio::test]
    #[ignore = "read-only inventory of all official Windows download sources; HEAD is not an install or integrity test"]
    async fn live_download_inventory() {
        let dir = crate::test_support::TestDir::new();
        let client = http().unwrap();
        let mut failures = Vec::new();
        for kind in crate::types::ALL_ENV_TYPES {
            for source in available_sources(kind) {
                let cache = dir.path().join(format!("{kind:?}-{}.json", source.id));
                let rows = match list_versions(kind, &source.id, &cache).await {
                    Ok(rows) => rows,
                    Err(e) => {
                        failures.push(format!("{kind:?}/{}: listing failed: {e}", source.id));
                        continue;
                    }
                };
                let row = &rows[0];
                let spec = match download_spec(
                    kind,
                    &row.version,
                    "C:\\Temp\\EnvCon\\target",
                    &source.id,
                    row.url.as_deref(),
                ) {
                    Ok(spec) => spec,
                    Err(e) => {
                        failures.push(format!("{kind:?}/{}: specification failed: {e}", source.id));
                        continue;
                    }
                };
                let head = client
                    .head(&spec.url)
                    .timeout(std::time::Duration::from_secs(15))
                    .send()
                    .await;
                let status = match head {
                    Ok(response) => {
                        if response.status() == reqwest::StatusCode::NOT_FOUND {
                            failures.push(format!("{kind:?}/{}: URL returned 404: {}", source.id, spec.url));
                        }
                        format!("{} length={:?}", response.status(), response.content_length())
                    }
                    Err(e) => format!("HEAD inconclusive: {e}"),
                };
                println!("{kind:?}/{}: {} versions, selected {}, {status}, {}", source.id, rows.len(), row.version, spec.url);
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    #[tokio::test]
    #[ignore = "downloads current official Windows assets into a temporary directory; never installs them"]
    async fn live_download_asset_audit() {
        use sha2::{Digest, Sha256};
        use tokio::io::AsyncWriteExt;

        let temp = crate::test_support::TestDir::new();
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .user_agent("EnvConAssetAudit/0.5.5")
            .build()
            .unwrap();

        for kind in crate::types::ALL_ENV_TYPES {
            let source = available_sources(kind)
                .into_iter()
                .find(|candidate| candidate.id == SOURCE_OFFICIAL)
                .expect("all environments have an official source");
            let cache = temp.path().join(format!("{kind:?}.json"));
            let rows = match list_versions(kind, &source.id, &cache).await {
                Ok(rows) => rows,
                Err(error) => {
                    println!("{}", serde_json::json!({
                        "tool": format!("{kind:?}"), "source": source.label,
                        "result": "listing-failed", "detail": error.to_string()
                    }));
                    continue;
                }
            };
            let row = &rows[0];
            let verification = match resolve_verification(&row.verification).await {
                Ok(verification) => verification,
                Err(error) => {
                    println!("{}", serde_json::json!({
                        "tool": format!("{kind:?}"), "source": source.label,
                        "version": row.version, "result": "verification-resolution-failed",
                        "detail": error.to_string()
                    }));
                    continue;
                }
            };
            if let VerificationInfo::Invalid { reason } = &verification {
                println!("{}", serde_json::json!({
                    "tool": format!("{kind:?}"), "source": source.label,
                    "version": row.version, "result": "official-checksum-invalid",
                    "detail": reason
                }));
                continue;
            }
            let spec = match download_spec_with_verification(
                kind,
                &row.version,
                "C:\\Temp\\EnvConAssetAudit",
                &source.id,
                row.url.as_deref(),
                Some(&verification),
                row.size_bytes,
            ) {
                Ok(spec) => spec,
                Err(error) => {
                    println!("{}", serde_json::json!({
                        "tool": format!("{kind:?}"), "source": source.label,
                        "version": row.version, "result": "specification-failed",
                        "detail": error.to_string()
                    }));
                    continue;
                }
            };
            let response = match client.get(&spec.url).send().await {
                Ok(response) if response.status().is_success() => response,
                Ok(response) => {
                    println!("{}", serde_json::json!({
                        "tool": format!("{kind:?}"), "source": source.label,
                        "version": row.version, "url": spec.url,
                        "result": "http-failed", "status": response.status().as_u16()
                    }));
                    continue;
                }
                Err(error) => {
                    println!("{}", serde_json::json!({
                        "tool": format!("{kind:?}"), "source": source.label,
                        "version": row.version, "url": spec.url,
                        "result": "network-failed", "detail": error.to_string()
                    }));
                    continue;
                }
            };
            let content_length = response.content_length();
            let path = temp.path().join(format!(
                "{}-{}.{}",
                kind.folder(),
                row.version.replace(['/', '\\', ':', '+'], "_"),
                spec.method.file_ext()
            ));
            let mut file = match tokio::fs::File::create(&path).await {
                Ok(file) => file,
                Err(error) => {
                    println!("{}", serde_json::json!({
                        "tool": format!("{kind:?}"), "source": source.label,
                        "version": row.version, "url": spec.url,
                        "result": "temp-file-failed", "detail": error.to_string()
                    }));
                    continue;
                }
            };
            let mut stream = response.bytes_stream();
            let mut downloaded = 0u64;
            let mut digest = Sha256::new();
            let mut first_bytes = Vec::new();
            let mut failed = None;
            while let Some(chunk) = stream.next().await {
                match chunk {
                    Ok(chunk) => {
                        if first_bytes.len() < 4096 {
                            let take = (4096 - first_bytes.len()).min(chunk.len());
                            first_bytes.extend_from_slice(&chunk[..take]);
                        }
                        digest.update(&chunk);
                        downloaded += chunk.len() as u64;
                        if let Err(error) = file.write_all(&chunk).await {
                            failed = Some(error.to_string());
                            break;
                        }
                    }
                    Err(error) => {
                        failed = Some(error.to_string());
                        break;
                    }
                }
            }
            if failed.is_none() {
                if let Err(error) = file.flush().await {
                    failed = Some(error.to_string());
                }
            }
            drop(file);
            if let Some(detail) = failed {
                println!("{}", serde_json::json!({
                    "tool": format!("{kind:?}"), "source": source.label,
                    "version": row.version, "url": spec.url,
                    "result": "download-failed", "bytesReceived": downloaded,
                    "detail": detail
                }));
                continue;
            }

            let length_matches = content_length.is_none_or(|length| length == downloaded);
            let catalog_size_matches = row.size_bytes.is_none_or(|size| size == downloaded);
            let (format_ok, structure) = if spec.method.file_ext() == "zip" {
                let checked = std::fs::File::open(&path)
                    .map_err(|error| error.to_string())
                    .and_then(|input| {
                        let mut archive = zip::ZipArchive::new(input)
                            .map_err(|error| error.to_string())?;
                        if archive.is_empty() {
                            return Err("ZIP 包没有条目".into());
                        }
                        let count = archive.len();
                        let mut buffer = [0u8; 64 * 1024];
                        for index in 0..count {
                            let mut entry = archive.by_index(index)
                                .map_err(|error| error.to_string())?;
                            if entry.enclosed_name().is_none() {
                                return Err(format!("存在越界路径: {}", entry.name()));
                            }
                            std::io::copy(&mut entry, &mut std::io::sink())
                                .map_err(|error| error.to_string())?;
                            let _ = &mut buffer;
                        }
                        Ok(count)
                    });
                match checked {
                    Ok(count) => (true, serde_json::json!({"zipEntries": count, "crcRead": true})),
                    Err(error) => (false, serde_json::json!({"archiveError": error})),
                }
            } else {
                let pe = first_bytes.starts_with(b"MZ")
                    && first_bytes.get(0x3c..0x40).is_some_and(|offset| {
                        let pe_offset = u32::from_le_bytes(offset.try_into().unwrap()) as usize;
                        pe_offset.checked_add(4)
                            .and_then(|end| std::fs::read(&path).ok().and_then(|bytes| bytes.get(pe_offset..end).map(|value| value == b"PE\0\0")))
                            .unwrap_or(false)
                    });
                let has_7z_signature = spec.url.to_ascii_lowercase().contains("portablegit")
                    && std::fs::read(&path).ok().is_some_and(|bytes| {
                        bytes.windows(6).any(|window| window == b"7z\xbc\xaf\x27\x1c")
                    });
                (
                    pe && (!spec.url.to_ascii_lowercase().contains("portablegit") || has_7z_signature),
                    serde_json::json!({"peExecutable": pe, "embedded7zSignature": has_7z_signature}),
                )
            };
            let result = if !length_matches || !catalog_size_matches || !format_ok {
                "downloaded-but-check-failed"
            } else {
                "downloaded-and-structure-checked"
            };
            let local_sha256 = format!("{:x}", digest.finalize());
            let official_sha256 = spec.verification.expected_sha256().map(str::to_string);
            let official_sha256_matches = official_sha256
                .as_deref()
                .map(|expected| expected.eq_ignore_ascii_case(&local_sha256));
            let result = if official_sha256_matches == Some(false) {
                "downloaded-but-check-failed"
            } else {
                result
            };
            println!("{}", serde_json::json!({
                "tool": format!("{kind:?}"), "source": source.label,
                "version": row.version, "url": spec.url, "result": result,
                "bytesDownloaded": downloaded, "contentLength": content_length,
                "contentLengthMatches": length_matches, "catalogSize": row.size_bytes,
                "catalogSizeMatches": catalog_size_matches,
                "sha256Local": local_sha256,
                "officialSha256": official_sha256,
                "officialSha256Matches": official_sha256_matches,
                "verification": spec.verification,
                "formatStructure": structure
            }));
        }
    }
}
