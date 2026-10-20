//! 集成测试:Maven 下载 + 解压全流程(约 9MB,验证镜像可达与解压正确性)

#[tokio::test]
async fn maven_download_and_unzip() {
    let spec = envcon_lib::sources::download_spec(
        envcon_lib::types::EnvType::Maven,
        "3.9.9",
        r"D:\DevEnvManager\envs\mavens\maven-test",
        envcon_lib::sources::SOURCE_MIRROR,
        None,
    )
    .expect("spec");

    let dest = std::env::temp_dir().join("envcon_test_maven.zip");
    let target = std::env::temp_dir().join("envcon_test_maven_unzip");
    let _ = std::fs::remove_dir_all(&target);
    let _ = std::fs::remove_file(&dest);

    let dm = envcon_lib::download::DownloadManager::new();
    let path = dm
        .download(&spec, &dest, |_, _, _| {})
        .await
        .expect("download");
    assert!(path.exists(), "下载文件应存在");

    envcon_lib::download::unzip(&path, &target, true, |_| {})
        .await
        .expect("unzip");

    // Maven 解压后(strip_top)应直接包含 bin/mvn.cmd 与 lib/maven-core-*.jar
    assert!(target.join("bin").join("mvn.cmd").exists(), "bin/mvn.cmd 应存在");
    assert!(
        target.join("lib").read_dir().unwrap().any(|e| {
            let n = e.unwrap().file_name().to_string_lossy().to_string();
            n.starts_with("maven-core-") && n.ends_with(".jar")
        }),
        "lib/maven-core-*.jar 应存在"
    );

    // 清理
    let _ = std::fs::remove_dir_all(&target);
    let _ = std::fs::remove_file(&dest);
}
