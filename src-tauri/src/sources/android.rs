use serde::Deserialize;

use super::{AppError, DownloadSpec, InstallMethod, Result, VersionInfo};

const REPOSITORY: &str = "https://dl.google.com/android/repository/";

#[derive(Deserialize)]
struct Repository {
    #[serde(rename = "remotePackage", default)]
    packages: Vec<Package>,
}

#[derive(Deserialize)]
struct Package {
    #[serde(rename = "@path")]
    path: String,
    revision: Revision,
    #[serde(rename = "channelRef")]
    channel: Option<Channel>,
    obsolete: Option<String>,
    archives: Archives,
}

#[derive(Deserialize)]
struct Revision {
    major: u32,
    #[serde(default)]
    minor: u32,
    #[serde(default)]
    micro: u32,
    preview: Option<u32>,
}

#[derive(Deserialize)]
struct Channel {
    #[serde(rename = "@ref")]
    reference: String,
}

#[derive(Deserialize)]
struct Archives {
    #[serde(rename = "archive", default)]
    items: Vec<Archive>,
}

#[derive(Deserialize)]
struct Archive {
    #[serde(rename = "host-os")]
    host: Option<String>,
    complete: Option<Complete>,
}

#[derive(Deserialize)]
struct Complete {
    size: u64,
    url: String,
}

pub(super) async fn releases() -> Result<Vec<VersionInfo>> {
    let xml = super::http()?.get(format!("{REPOSITORY}repository2-1.xml"))
        .send().await?.error_for_status()?.text().await?;
    parse_repository(&xml)
}

fn parse_repository(xml: &str) -> Result<Vec<VersionInfo>> {
    let repository: Repository = quick_xml::de::from_str(xml)
        .map_err(|e| AppError::msg(format!("Android SDK 索引解析失败: {e}")))?;
    let mut versions = Vec::new();
    for package in repository.packages {
        if package.path != "platform-tools" || package.obsolete.is_some()
            || package.revision.preview.is_some()
            || package.channel.is_some_and(|c| c.reference != "channel-0") { continue; }
        let revision = package.revision;
        let version = format!("{}.{}.{}", revision.major, revision.minor, revision.micro);
        for archive in package.archives.items {
            if archive.host.as_deref() != Some("windows") { continue; }
            let Some(file) = archive.complete.filter(|f| f.size > 0) else { continue };
            let url = format!("{REPOSITORY}{}", file.url);
            download_spec(&version, Some(&url))?;
            versions.push(VersionInfo {
                version: version.clone(), date: None, size_bytes: Some(file.size),
                note: Some("Android SDK Platform Tools（ADB / Fastboot）；Google 当前稳定版索引，非完整历史归档".into()),
                url: Some(url),
            });
        }
    }
    if versions.is_empty() { return Err(AppError::msg("Android SDK 索引没有可用的 Windows Platform Tools 稳定版")); }
    Ok(versions)
}

pub(super) fn download_spec(version: &str, hint: Option<&str>) -> Result<DownloadSpec> {
    let parts: Vec<_> = version.split('.').collect();
    if parts.len() != 3 || parts.iter().any(|p| p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit())) {
        return Err(AppError::msg("无效的 Platform Tools 版本"));
    }
    let url = hint.ok_or_else(|| AppError::msg("ADB 下载链接缺失，请刷新 Google 官方版本列表"))?;
    // Require a pinned official Windows archive. Never silently substitute the moving latest ZIP.
    let expected = format!("{REPOSITORY}platform-tools_r{version}-win.zip");
    if url != expected {
        return Err(AppError::msg("ADB 下载链接与所选版本不匹配，请刷新 Google 官方版本列表"));
    }
    Ok(DownloadSpec { url: url.into(), method: InstallMethod::Unzip { strip_top: true } })
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = r#"<sdk:sdk-repository xmlns:sdk="http://schemas.android.com/sdk/android/repo/repository2/01">
      <remotePackage path="platform-tools"><revision><major>36</major><minor>0</minor><micro>2</micro></revision><channelRef ref="channel-0"/>
        <archives><archive><host-os>linux</host-os><complete><size>20</size><url>platform-tools_r36.0.2-linux.zip</url></complete></archive>
        <archive><host-os>windows</host-os><complete><size>123</size><url>platform-tools_r36.0.2-win.zip</url></complete></archive></archives>
      </remotePackage></sdk:sdk-repository>"#;

    #[test]
    fn adb_repository_filters_platform_and_channel() {
        let rows = parse_repository(FIXTURE).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].version, "36.0.2");
        assert_eq!(rows[0].size_bytes, Some(123));
        for xml in [FIXTURE.replace("channel-0", "channel-1"), FIXTURE.replace("<micro>2</micro>", "<micro>2</micro><preview>1</preview>"),
            FIXTURE.replace("<archives>", "<obsolete/><archives>"), FIXTURE.replace("path=\"platform-tools\"", "path=\"build-tools;36.0.2\""),
            FIXTURE.replace("<host-os>windows</host-os>", "<host-os>macosx</host-os>")] {
            assert!(parse_repository(&xml).is_err());
        }
        assert!(parse_repository("invalid xml").is_err());
    }

    #[test]
    fn adb_download_rejects_moving_or_mismatched_archives() {
        assert!(download_spec("36.0.2", Some("https://dl.google.com/android/repository/platform-tools_r36.0.2-win.zip")).is_ok());
        for url in ["https://dl.google.com/android/repository/platform-tools-latest-windows.zip",
            "https://example.com/platform-tools_r36.0.2-win.zip", "https://dl.google.com/android/repository/platform-tools_r36.0.1-win.zip"] {
            assert!(download_spec("36.0.2", Some(url)).is_err());
        }
        assert!(download_spec("../36.0.2", None).is_err());
        assert!(download_spec("36.0.2", None).is_err());
    }

    #[tokio::test]
    #[ignore = "Requires access to Google's official SDK repository"]
    async fn live_adb_repository() {
        let rows = releases().await.unwrap();
        for row in rows {
            let spec = download_spec(&row.version, row.url.as_deref()).unwrap();
            let response = super::super::http().unwrap().head(&spec.url).send().await.unwrap().error_for_status().unwrap();
            assert!(response.content_length().unwrap_or(0) > 0);
            println!("ADB {}: {}", row.version, spec.url);
        }
    }
}
