//! 集成测试:各镜像源的版本列表拉取与解析(mirror + official)

use envcon_lib::sources::{self, SOURCE_MIRROR, SOURCE_OFFICIAL};
use envcon_lib::types::{EnvType, ALL_ENV_TYPES};

#[test]
fn download_spec_rust_channel_is_independent_of_directory_name() {
    for channel in ["stable", "beta", "nightly", "1.85.0"] {
        for source in [SOURCE_MIRROR, SOURCE_OFFICIAL] {
            let spec = sources::download_spec(EnvType::Rust, channel, "my-rust-stable", source, None).unwrap();
            let sources::InstallMethod::RustupInit { channel: actual, .. } = spec.method else { panic!("expected rustup") };
            assert_eq!(actual, channel);
        }
    }
}

#[tokio::test]
async fn all_sources_list_versions_mirror() {
    let cache = std::env::temp_dir().join("envcon_test_version_cache.json");
    let _ = std::fs::remove_file(&cache);

    for t in ALL_ENV_TYPES {
        let versions = sources::list_versions(t, SOURCE_MIRROR, &cache).await;
        match versions {
            Ok(v) => {
                assert!(!v.is_empty(), "{t:?} 版本列表不应为空");
                println!("{t:?}(mirror): {} 个,第一个: {}", v.len(), v[0].version);
            }
            Err(e) => panic!("{t:?} 版本列表失败: {e}"),
        }
    }
}

#[tokio::test]
async fn official_sources_list_versions() {
    let cache = std::env::temp_dir().join("envcon_test_version_cache_official.json");
    let _ = std::fs::remove_file(&cache);

    // 官方源逐类验证(排除 GitHub 系:与 mirror 共用列表逻辑)
    for t in [
        EnvType::Node,
        EnvType::Go,
        EnvType::Python,
        EnvType::Jdk,
        EnvType::Maven,
        EnvType::Gradle,
        EnvType::Zig,
        EnvType::Php,
    ] {
        let versions = sources::list_versions(t, SOURCE_OFFICIAL, &cache).await;
        match versions {
            Ok(v) => {
                assert!(!v.is_empty(), "{t:?} 官方源版本列表不应为空");
                println!("{t:?}(official): {} 个,第一个: {}", v.len(), v[0].version);
            }
            Err(e) => panic!("{t:?} 官方源失败: {e}"),
        }
    }
}

#[test]
fn download_spec_github_mirror_prefix() {
    // mirror 源应走 ghfast 前缀
    let spec = sources::download_spec(
        EnvType::Deno,
        "2.1.0",
        r"D:\DevEnvManager\envs\denos\deno-2",
        sources::SOURCE_MIRROR,
        Some("https://github.com/denoland/deno/releases/download/v2.1.0/deno-x86_64-pc-windows-msvc.zip"),
    )
    .expect("spec");
    assert!(spec.url.starts_with("https://ghfast.top/https://github.com/"), "mirror 应加 ghfast 前缀: {}", spec.url);

    let spec2 = sources::download_spec(
        EnvType::Deno,
        "2.1.0",
        r"D:\DevEnvManager\envs\denos\deno-2",
        sources::SOURCE_OFFICIAL,
        Some("https://github.com/denoland/deno/releases/download/v2.1.0/deno-x86_64-pc-windows-msvc.zip"),
    )
    .expect("spec");
    assert!(spec2.url.starts_with("https://github.com/"), "official 应为 GitHub 直链");
}

#[test]
fn download_spec_git_portable_sfx_args() {
    // PortableGit 自解压包:mirror 走 ghfast,参数含 -o<目录> 与 -y
    let spec = sources::download_spec(
        EnvType::Git,
        "2.55.0.windows.5",
        r"D:\DevEnvManager\envs\gits\git-2.55",
        sources::SOURCE_MIRROR,
        Some("https://github.com/git-for-windows/git/releases/download/v2.55.0.windows.5/PortableGit-2.55.0.5-64-bit.7z.exe"),
    )
    .expect("spec");
    assert!(spec.url.starts_with("https://ghfast.top/"), "git mirror 前缀: {}", spec.url);
    match &spec.method {
        sources::InstallMethod::Installer { args } => {
            assert!(args.iter().any(|a| a.starts_with("-o") && a.contains("git-2.55")), "应含 -o<目录>: {args:?}");
            assert!(args.contains(&"-y".to_string()), "应含 -y: {args:?}");
        }
        other => panic!("git 应为 Installer 方式,实际 {other:?}"),
    }
}
