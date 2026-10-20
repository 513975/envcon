use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result};

/// The public release channel used by the in-app update check. This module only
/// reports metadata; it never downloads or executes a release asset.
const RELEASES_API: &str = "https://api.github.com/repos/513975/envcon/releases/latest";

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum UpdateCheckStatus {
    UpToDate,
    UpdateAvailable,
    NoRelease,
    UnknownVersion,
    Error,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateAsset {
    pub name: String,
    pub size: u64,
    /// Publisher-provided digest, when the release API exposes one. It is shown
    /// as metadata only and is not verified by this check.
    pub digest: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AppUpdateStatus {
    pub status: UpdateCheckStatus,
    pub current_version: String,
    pub latest_version: Option<String>,
    pub release_name: Option<String>,
    pub release_url: Option<String>,
    pub notes: Option<String>,
    pub published_at: Option<String>,
    pub assets: Vec<UpdateAsset>,
    pub error: Option<String>,
}

impl AppUpdateStatus {
    fn error(current_version: &str, message: impl Into<String>) -> Self {
        Self {
            status: UpdateCheckStatus::Error,
            current_version: current_version.to_string(),
            latest_version: None,
            release_name: None,
            release_url: None,
            notes: None,
            published_at: None,
            assets: Vec::new(),
            error: Some(message.into()),
        }
    }
}

/// Check the public release feed without mutating the installation.
///
/// Network and rate-limit failures are returned as structured `error` states so
/// the UI can offer a retry without treating them as "up to date".
pub async fn check_latest() -> AppUpdateStatus {
    let current = env!("CARGO_PKG_VERSION");
    let response = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .user_agent(format!("EnvCon/{current} (update-check)"))
        .build()
    {
        Ok(client) => client.get(RELEASES_API).send().await,
        Err(error) => return AppUpdateStatus::error(current, format!("创建更新检查请求失败: {error}")),
    };
    let response = match response {
        Ok(response) => response,
        Err(error) => return AppUpdateStatus::error(current, format!("无法连接 GitHub Releases: {error}")),
    };
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return AppUpdateStatus {
            status: UpdateCheckStatus::NoRelease,
            current_version: current.to_string(),
            latest_version: None,
            release_name: None,
            release_url: None,
            notes: None,
            published_at: None,
            assets: Vec::new(),
            error: None,
        };
    }
    if !response.status().is_success() {
        let status = response.status();
        let detail = if status == reqwest::StatusCode::FORBIDDEN {
            "GitHub API 限流或拒绝访问，请稍后重试".to_string()
        } else {
            format!("GitHub Releases 返回 HTTP {status}")
        };
        return AppUpdateStatus::error(current, detail);
    }
    let body = match response.text().await {
        Ok(body) => body,
        Err(error) => return AppUpdateStatus::error(current, format!("读取更新信息失败: {error}")),
    };
    match parse_release(&body, current) {
        Ok(status) => status,
        Err(error) => AppUpdateStatus::error(current, error.to_string()),
    }
}

#[derive(Deserialize)]
struct ReleaseResponse {
    tag_name: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    html_url: Option<String>,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    published_at: Option<String>,
    #[serde(default)]
    assets: Vec<ReleaseAsset>,
}

#[derive(Deserialize)]
struct ReleaseAsset {
    name: String,
    #[serde(default)]
    size: u64,
    #[serde(default)]
    digest: Option<String>,
}

fn parse_release(body: &str, current: &str) -> Result<AppUpdateStatus> {
    let release: ReleaseResponse = serde_json::from_str(body)
        .map_err(|error| AppError::msg(format!("更新信息格式无效: {error}")))?;
    let latest = release.tag_name.trim().trim_start_matches(['v', 'V']).to_string();
    if latest.is_empty() {
        return Err(AppError::msg("发布信息缺少版本号"));
    }
    let status = match compare_versions(current, &latest) {
        Some(std::cmp::Ordering::Less) => UpdateCheckStatus::UpdateAvailable,
        Some(_) => UpdateCheckStatus::UpToDate,
        None => UpdateCheckStatus::UnknownVersion,
    };
    Ok(AppUpdateStatus {
        status,
        current_version: current.to_string(),
        latest_version: Some(latest),
        release_name: release.name,
        release_url: release.html_url.filter(|url| safe_release_url(url)),
        notes: release.body,
        published_at: release.published_at,
        assets: release
            .assets
            .into_iter()
            .map(|asset| UpdateAsset {
                name: asset.name,
                size: asset.size,
                digest: asset.digest,
            })
            .collect(),
        error: None,
    })
}

fn safe_release_url(url: &str) -> bool {
    url.starts_with("https://github.com/513975/envcon/releases/")
}

#[derive(Debug, PartialEq, Eq)]
struct ParsedVersion {
    numbers: Vec<u64>,
    prerelease: Option<String>,
}

fn parse_version(value: &str) -> Option<ParsedVersion> {
    let normalized = value.trim().trim_start_matches(['v', 'V']);
    if normalized.is_empty() {
        return None;
    }
    let (core, prerelease) = match normalized.split_once('-') {
        Some((core, suffix)) if !suffix.is_empty() => (core, Some(suffix.to_ascii_lowercase())),
        Some(_) => return None,
        None => (normalized, None),
    };
    if core.is_empty() || core.split('.').any(|part| part.is_empty()) {
        return None;
    }
    let numbers = core
        .split('.')
        .map(|part| part.parse::<u64>().ok())
        .collect::<Option<Vec<_>>>()?;
    if numbers.is_empty() || numbers.len() > 4 {
        return None;
    }
    Some(ParsedVersion { numbers, prerelease })
}

fn compare_versions(current: &str, latest: &str) -> Option<std::cmp::Ordering> {
    let current = parse_version(current)?;
    let latest = parse_version(latest)?;
    let width = current.numbers.len().max(latest.numbers.len());
    for index in 0..width {
        let left = current.numbers.get(index).copied().unwrap_or(0);
        let right = latest.numbers.get(index).copied().unwrap_or(0);
        match left.cmp(&right) {
            std::cmp::Ordering::Equal => {}
            ordering => return Some(ordering),
        }
    }
    match (&current.prerelease, &latest.prerelease) {
        (None, None) => Some(std::cmp::Ordering::Equal),
        (None, Some(_)) => Some(std::cmp::Ordering::Greater),
        (Some(_), None) => Some(std::cmp::Ordering::Less),
        (Some(left), Some(right)) => Some(left.cmp(right)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RELEASE: &str = r#"{
        "tag_name": "v0.6.0",
        "name": "EnvCon v0.6.0",
        "html_url": "https://github.com/513975/envcon/releases/tag/v0.6.0",
        "body": "release notes",
        "published_at": "2026-10-19T00:00:00Z",
        "assets": [{"name": "EnvCon_0.6.0_x64-setup.exe", "size": 1234, "digest": "sha256:abc"}]
    }"#;

    #[test]
    fn release_metadata_is_parsed_and_newer_version_is_available() {
        let status = parse_release(RELEASE, "0.5.5").unwrap();
        assert_eq!(status.status, UpdateCheckStatus::UpdateAvailable);
        assert_eq!(status.latest_version.as_deref(), Some("0.6.0"));
        assert_eq!(status.release_name.as_deref(), Some("EnvCon v0.6.0"));
        assert_eq!(status.notes.as_deref(), Some("release notes"));
        assert_eq!(status.assets[0].digest.as_deref(), Some("sha256:abc"));
    }

    #[test]
    fn equal_older_and_prerelease_versions_do_not_offer_a_downgrade() {
        assert_eq!(
            parse_release(RELEASE, "0.6.0").unwrap().status,
            UpdateCheckStatus::UpToDate
        );
        assert_eq!(
            parse_release(RELEASE, "0.6.1").unwrap().status,
            UpdateCheckStatus::UpToDate
        );
        let prerelease = RELEASE.replace("v0.6.0", "v0.6.0-rc.1");
        assert_eq!(
            parse_release(&prerelease, "0.6.0").unwrap().status,
            UpdateCheckStatus::UpToDate
        );
        assert_eq!(
            parse_release(&prerelease, "0.5.9").unwrap().status,
            UpdateCheckStatus::UpdateAvailable
        );
    }

    #[test]
    fn malformed_or_foreign_release_metadata_is_rejected() {
        assert!(parse_release("{}", "0.5.5").is_err());
        assert!(parse_release(r#"{"tag_name":""}"#, "0.5.5").is_err());
        assert_eq!(
            parse_release(
                r#"{"tag_name":"next","html_url":"https://example.test/release"}"#,
                "0.5.5"
            )
            .unwrap()
            .status,
            UpdateCheckStatus::UnknownVersion
        );
        let foreign = parse_release(
            r#"{"tag_name":"v0.6.0","html_url":"https://example.test/release"}"#,
            "0.5.5",
        )
        .unwrap();
        assert_eq!(foreign.release_url, None);
    }

    #[test]
    fn version_parser_rejects_non_numeric_components() {
        assert!(parse_version("0.6").is_some());
        assert!(parse_version("v1.2.3").is_some());
        assert!(parse_version("1.2.3-rc.1").is_some());
        assert!(parse_version("1.2.x").is_none());
        assert!(parse_version("").is_none());
    }

    #[tokio::test]
    #[ignore = "requires external network access to GitHub Releases"]
    async fn live_release_check_is_structured() {
        let status = check_latest().await;
        assert!(matches!(
            status.status,
            UpdateCheckStatus::UpToDate
                | UpdateCheckStatus::UpdateAvailable
                | UpdateCheckStatus::NoRelease
                | UpdateCheckStatus::UnknownVersion
                | UpdateCheckStatus::Error
        ));
    }
}
