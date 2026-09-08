use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result};
use crate::types::EnvType;

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
}

/// 下载方式
#[derive(Debug, Clone)]
pub enum InstallMethod {
    /// 解压 zip;strip_top = zip 内是否有一层顶层目录需要剥掉
    Unzip { strip_top: bool },
    /// exe 静默安装器(Python/LLVM NSIS)
    Installer { args: Vec<String> },
    /// rustup-init 引导安装;dist_server 为 None 时用官方
    RustupInit { dist_server: Option<String> },
}

#[derive(Debug, Clone)]
pub struct DownloadSpec {
    pub url: String,
    pub method: InstallMethod,
}

fn http() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) EnvCon/0.1")
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
    }
}

/// 拉取指定类型+源的版本列表(带本地缓存兜底)
pub async fn list_versions(
    env_type: EnvType,
    source: &str,
    cache_path: &PathBuf,
) -> Result<Vec<VersionInfo>> {
    let result = match (env_type, source) {
        (EnvType::Node, SOURCE_MIRROR) => node_index("https://npmmirror.com/mirrors/node/index.json").await,
        (EnvType::Node, _) => node_index("https://nodejs.org/dist/index.json").await,

        (EnvType::Go, SOURCE_MIRROR) => go_index("https://golang.google.cn/dl/?mode=json&include=all").await,
        (EnvType::Go, _) => go_index("https://go.dev/dl/?mode=json&include=all").await,

        (EnvType::Python, SOURCE_MIRROR) => python_dir("https://mirrors.huaweicloud.com/python/").await,
        (EnvType::Python, _) => python_dir("https://www.python.org/ftp/python/").await,

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
    };

    match result {
        Ok(mut versions) => {
            versions.truncate(100);
            save_cache(env_type, source, &versions, cache_path);
            Ok(versions)
        }
        Err(e) => match load_cache(env_type, source, cache_path) {
            Some(v) => Ok(v),
            None => Err(e),
        },
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
    let gh = |url: &str| -> String {
        if source == SOURCE_MIRROR {
            format!("https://ghfast.top/{url}")
        } else {
            url.to_string()
        }
    };
    match env_type {
        EnvType::Node => {
            let base = if source == SOURCE_MIRROR {
                "https://npmmirror.com/mirrors/node"
            } else {
                "https://nodejs.org/dist"
            };
            Ok(DownloadSpec {
                url: format!("{base}/v{v}/node-v{v}-win-x64.zip", v = version),
                method: InstallMethod::Unzip { strip_top: true },
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
                method: InstallMethod::Unzip { strip_top: false },
            })
        }
        EnvType::Python => {
            let base = if source == SOURCE_MIRROR {
                "https://mirrors.huaweicloud.com/python"
            } else {
                "https://www.python.org/ftp/python"
            };
            Ok(DownloadSpec {
                url: format!("{base}/{v}/python-{v}-amd64.exe", v = version),
                method: InstallMethod::Installer {
                    args: vec![
                        "/quiet".into(),
                        "InstallAllUsers=0".into(),
                        format!("TargetDir={}", target_dir.replace('/', "\\")),
                        "PrependPath=0".into(),
                        "Shortcuts=0".into(),
                        "Include_launcher=0".into(),
                    ],
                },
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
                })
            } else {
                // Adoptium API 直链
                let url = url_hint.ok_or_else(|| {
                    AppError::msg("官方源 JDK 下载链接缺失,请刷新版本列表后重试")
                })?;
                Ok(DownloadSpec {
                    url: url.to_string(),
                    method: InstallMethod::Unzip { strip_top: true },
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
            })
        }
        EnvType::Rust => {
            if source == SOURCE_MIRROR {
                Ok(DownloadSpec {
                    url: "https://rsproxy.cn/rustup/dist/x86_64-pc-windows-msvc/rustup-init.exe".into(),
                    method: InstallMethod::RustupInit {
                        dist_server: Some("https://rsproxy.cn".into()),
                    },
                })
            } else {
                Ok(DownloadSpec {
                    url: "https://static.rust-lang.org/rustup/dist/x86_64-pc-windows-msvc/rustup-init.exe".into(),
                    method: InstallMethod::RustupInit { dist_server: None },
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
            })
        }
        EnvType::Llvm => {
            let url = url_hint.ok_or_else(|| {
                AppError::msg("LLVM 下载链接缺失,请刷新版本列表后重试")
            })?;
            // LLVM 官方 exe 为 NSIS 安装器:/S 静默,/D= 指定目录(必须最后、不带引号)
            Ok(DownloadSpec {
                url: gh(url),
                method: InstallMethod::Installer {
                    args: vec!["/S".into(), format!("/D={}", target_dir.replace('/', "\\"))],
                },
            })
        }
        EnvType::Zig => {
            let url = url_hint
                .map(|u| u.to_string())
                .unwrap_or_else(|| zig_download_url(version));
            Ok(DownloadSpec {
                url,
                method: InstallMethod::Unzip { strip_top: true },
            })
        }
        EnvType::Deno => {
            let url = url_hint.ok_or_else(|| {
                AppError::msg("Deno 下载链接缺失,请刷新版本列表后重试")
            })?;
            Ok(DownloadSpec {
                url: gh(url),
                method: InstallMethod::Unzip { strip_top: false },
            })
        }
        EnvType::Bun => {
            let url = url_hint.ok_or_else(|| {
                AppError::msg("Bun 下载链接缺失,请刷新版本列表后重试")
            })?;
            Ok(DownloadSpec {
                url: gh(url),
                method: InstallMethod::Unzip { strip_top: false },
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
                },
            })
        }
        EnvType::Gh => {
            let url = url_hint.ok_or_else(|| {
                AppError::msg("GitHub CLI 下载链接缺失,请刷新版本列表后重试")
            })?;
            Ok(DownloadSpec {
                url: gh(url),
                method: InstallMethod::Unzip { strip_top: true },
            })
        }
        EnvType::Mingw => {
            let url = url_hint.ok_or_else(|| {
                AppError::msg("C/C++ 下载链接缺失,请刷新版本列表后重试")
            })?;
            Ok(DownloadSpec {
                url: gh(url),
                method: InstallMethod::Unzip { strip_top: true },
            })
        }
    }
}

// ---------- 各源解析 ----------

/// Node 系 index.json(npmmirror / nodejs.org 结构相同)
async fn node_index(base: &str) -> Result<Vec<VersionInfo>> {
    #[derive(Deserialize)]
    struct Entry {
        version: String,
        date: Option<String>,
        /// LTS 版本为代号字符串(如 "Jod"),非 LTS 为 false
        lts: Option<serde_json::Value>,
        security: Option<bool>,
    }
    let list: Vec<Entry> = http()?
        .get(base)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(list
        .into_iter()
        .map(|e| VersionInfo {
            version: e.version.trim_start_matches('v').to_string(),
            date: e.date,
            size_bytes: None,
            note: match &e.lts {
                Some(serde_json::Value::String(_)) => Some("LTS".into()),
                _ if e.security == Some(true) => Some("安全更新".into()),
                _ => None,
            },
            url: None,
        })
        .collect())
}

/// Go 官方 JSON(golang.google.cn / go.dev 结构相同)
async fn go_index(base: &str) -> Result<Vec<VersionInfo>> {
    #[derive(Deserialize)]
    struct File {
        size: i64,
        os: Option<String>,
        arch: Option<String>,
        kind: Option<String>,
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
        .map(|r| {
            let win = r.files.iter().find(|f| {
                f.os.as_deref() == Some("windows")
                    && f.arch.as_deref() == Some("amd64")
                    && f.kind.as_deref() == Some("archive")
            });
            VersionInfo {
                version: r.version.trim_start_matches("go").to_string(),
                date: None,
                size_bytes: win.map(|f| f.size as u64),
                note: None,
                url: None,
            }
        })
        .collect())
}

/// Python 目录页(华为云 / python.org,均为 <a href="X/"> 结构)
async fn python_dir(base: &str) -> Result<Vec<VersionInfo>> {
    let html = http()?.get(base).send().await?.error_for_status()?.text().await?;

    let mut versions: Vec<(String, String)> = Vec::new();
    let mut rest = html.as_str();
    while let Some(i) = rest.find("<a href=\"") {
        let after = &rest[i + "<a href=\"".len()..];
        let Some(j) = after.find("/\"") else { break };
        let name = after[..j].to_string();
        let after_name = &after[j..];

        if name.starts_with('3') && name.contains('.') && !name.contains('-') {
            let date = after_name
                .find("</a>")
                .and_then(|k| extract_date_from_row(&after_name[k..]))
                .unwrap_or_default();
            versions.push((name, date));
        }
        rest = after_name;
    }

    versions.sort_by(|a, b| version_cmp(&b.0, &a.0));
    if versions.is_empty() {
        return Err(AppError::msg("未能解析 Python 版本列表"));
    }
    Ok(versions
        .into_iter()
        .map(|(v, d)| VersionInfo {
            version: v,
            date: if d.is_empty() { None } else { Some(d) },
            size_bytes: None,
            note: None,
            url: None,
        })
        .collect())
}

fn extract_date_from_row(row: &str) -> Option<String> {
    // 形如 2024-06-15 13:20 或 22-Aug-2021 22:18
    let mut chars = row.char_indices();
    let mut start = None;
    for (i, c) in chars.by_ref() {
        if c.is_ascii_digit() && row[i..].starts_with("20") {
            start = Some(i);
            break;
        }
    }
    let start = start?;
    let sub = &row[start..];
    let end = sub
        .char_indices()
        .take_while(|(_, c)| c.is_ascii_digit() || *c == '-' || *c == ':' || *c == ' ')
        .map(|(i, _)| i)
        .last()
        .unwrap_or(0);
    Some(sub[..end].trim().to_string())
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
                    best = Some((v, format!("OpenJDK{name}.zip")));
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
    for major in [25u32, 21, 17, 11, 8] {
        let url = format!(
            "https://api.adoptium.net/v3/assets/latest/{major}/hotspot?architecture=x64&image_type=jdk&os=windows"
        );
        let assets: Vec<Asset> = match http()?
            .get(&url)
            .send()
            .await
            .and_then(|r| r.error_for_status())
        {
            Ok(r) => match r.json().await {
                Ok(v) => v,
                Err(_) => continue,
            },
            Err(_) => continue,
        };
        let Some(a) = assets.first() else { continue };
        // release_name: "jdk-21.0.5+11" / "jdk8u402-b06"
        let version = adoptium_version(&a.release_name, major);
        out.push(VersionInfo {
            version,
            date: None,
            size_bytes: a.binary.package.size,
            note: Some(format!("Adoptium Temurin {major}")),
            url: Some(a.binary.package.link.clone()),
        });
    }
    if out.is_empty() {
        return Err(AppError::msg("未能从 Adoptium API 获取版本列表"));
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
        })
        .collect())
}

/// Gradle 目录(腾讯云 / gradle.org,均为 *-bin.zip 文件列表;href 可能为相对或绝对路径)
async fn gradle_dir(base: &str) -> Result<Vec<VersionInfo>> {
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
            version: v,
            date: None,
            size_bytes: None,
            note: None,
            url: None,
        })
        .collect())
}

/// Rust: 提供 stable/beta/nightly
fn rust_channels() -> Vec<VersionInfo> {
    vec![
        VersionInfo {
            version: "stable".into(),
            date: None,
            size_bytes: None,
            note: Some("稳定版工具链".into()),
            url: None,
        },
        VersionInfo {
            version: "beta".into(),
            date: None,
            size_bytes: None,
            note: Some("公测版工具链".into()),
            url: None,
        },
        VersionInfo {
            version: "nightly".into(),
            date: None,
            size_bytes: None,
            note: Some("每日构建".into()),
            url: None,
        },
    ]
}

// ---------- GitHub Releases 通用 ----------

#[derive(Deserialize)]
struct GhAsset {
    name: String,
    size: Option<u64>,
    browser_download_url: String,
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

/// PHP: windows.php.net releases 目录(仅 8.x,Thread Safe x64 包)
async fn php_releases() -> Result<Vec<VersionInfo>> {
    let html = http()?
        .get("https://windows.php.net/downloads/releases/")
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;

    // href 形如 /downloads/releases/php-8.4.5-Win32-vs17-x64.zip
    let mut out: Vec<VersionInfo> = Vec::new();
    for name in extract_all_between(&html, "href=\"", "\"") {
        let Some(file) = name.rsplit('/').next() else { continue };
        let Some(rest) = file.strip_prefix("php-") else { continue };
        // 仅取 Thread Safe x64 zip(排除 nts/源码/debug)
        let Some((ver, tail)) = rest.split_once("-Win32-") else { continue };
        if !tail.ends_with("-x64.zip") || !ver.chars().next().is_some_and(|c| c == '8') {
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
        });
    }
    out.sort_by(|a, b| version_cmp(&b.version, &a.version));
    out.truncate(40);
    if out.is_empty() {
        return Err(AppError::msg("未能解析 PHP 版本列表"));
    }
    Ok(out)
}

/// LLVM: GitHub releases(LLVM-{v}-win64.exe)
async fn llvm_releases() -> Result<Vec<VersionInfo>> {
    let list = github_releases("llvm/llvm-project").await?;
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
        let asset_name = format!("LLVM-{tag}-win64.exe");
        let Some(asset) = r.assets.iter().find(|a| a.name == asset_name) else { continue };
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
            let size = win
                .and_then(|w| w.get("size"))
                .and_then(|s| s.as_str())
                .and_then(|s| s.parse().ok());
            let date = entry
                .get("date")
                .and_then(|d| d.as_str())
                .map(|s| s.to_string());
            out.push(VersionInfo {
                version: ver,
                date,
                size_bytes: size,
                note: None,
                url: tarball,
            });
        }
    }
    out.sort_by(|a, b| version_cmp(&b.version, &a.version));
    if out.is_empty() {
        return Err(AppError::msg("未能解析 Zig 版本列表"));
    }
    Ok(out)
}

/// Zig 下载 URL 兜底推导(0.14+ 与旧版文件名格式不同)
fn zig_download_url(version: &str) -> String {
    let (major, minor) = version.split_once('.').unwrap_or((version, "0"));
    let major_n: u32 = major.parse().unwrap_or(0);
    let minor_n: u32 = minor.parse().unwrap_or(0);
    // 0.14+ → zig-x86_64-windows-{v}.zip;更旧 → zig-windows-x86_64-{v}.zip
    if major_n > 0 || minor_n >= 14 {
        format!("https://ziglang.org/download/{version}/zig-x86_64-windows-{version}.zip")
    } else {
        format!("https://ziglang.org/download/{version}/zig-windows-x86_64-{version}.zip")
    }
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
