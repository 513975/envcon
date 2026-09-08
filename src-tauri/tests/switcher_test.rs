use std::fs;
use std::path::{Path, PathBuf};

use envcon_lib::switcher;
use envcon_lib::types::EnvType;

fn setup_root(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "envcon-switch-test-{tag}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    for (ver, marker) in [("a", "marker-a.txt"), ("b", "marker-b.txt")] {
        let d = root.join("envs").join("jdks").join(ver);
        fs::create_dir_all(&d).unwrap();
        fs::write(d.join(marker), ver).unwrap();
    }
    fs::create_dir_all(root.join("current")).unwrap();
    root
}

fn current_jdk(root: &Path) -> PathBuf {
    root.join("current").join("jdk")
}

fn is_junction(p: &Path) -> bool {
    std::fs::symlink_metadata(p)
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
}

fn old_backups(root: &Path) -> Vec<String> {
    fs::read_dir(root.join("current"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .filter(|n| n.starts_with("jdk.old-"))
        .collect()
}

/// 首次切换创建 junction;之后反复切换重指链接。
/// 回归:旧实现删除旧链接时会残留空目录,导致重建失败,
/// 下一次切换再报"不是链接(可能是真实目录)"。
#[test]
fn switch_creates_and_retargets_junction() {
    let root = setup_root("retarget");

    switcher::switch(&root, EnvType::Jdk, "a").unwrap();
    assert!(is_junction(&current_jdk(&root)));
    assert!(current_jdk(&root).join("marker-a.txt").exists());

    // 二次切换:旧 bug 在此步失败(残留空目录挡住重建)
    switcher::switch(&root, EnvType::Jdk, "b").unwrap();
    assert!(is_junction(&current_jdk(&root)));
    assert!(current_jdk(&root).join("marker-b.txt").exists());
    assert!(!current_jdk(&root).join("marker-a.txt").exists());
    assert!(old_backups(&root).is_empty());

    // 反复切换再验证一次
    switcher::switch(&root, EnvType::Jdk, "a").unwrap();
    assert!(current_jdk(&root).join("marker-a.txt").exists());

    fs::remove_dir_all(&root).unwrap();
}

/// current\<名> 是历史损坏残留的空真实目录时,切换应自动清理并成功
#[test]
fn switch_recovers_from_leftover_empty_dir() {
    let root = setup_root("empty-dir");
    switcher::switch(&root, EnvType::Jdk, "a").unwrap();

    // 模拟损坏状态:junction 变成空真实目录
    fs::remove_dir(current_jdk(&root)).unwrap();
    fs::create_dir(current_jdk(&root)).unwrap();

    switcher::switch(&root, EnvType::Jdk, "b").unwrap();
    assert!(is_junction(&current_jdk(&root)));
    assert!(current_jdk(&root).join("marker-b.txt").exists());
    // 空目录直接清理,不应产生备份
    assert!(old_backups(&root).is_empty());

    fs::remove_dir_all(&root).unwrap();
}

/// current\<名> 是含数据的真实目录时,切换应把数据挪开备份而不是报错或误删
#[test]
fn switch_preserves_real_dir_data() {
    let root = setup_root("real-dir");
    switcher::switch(&root, EnvType::Jdk, "a").unwrap();

    // 模拟:有人把真实内容放进了 current\jdk
    fs::remove_dir(current_jdk(&root)).unwrap();
    fs::create_dir(current_jdk(&root)).unwrap();
    fs::write(current_jdk(&root).join("user-file.txt"), "data").unwrap();

    switcher::switch(&root, EnvType::Jdk, "b").unwrap();
    assert!(is_junction(&current_jdk(&root)));
    assert!(current_jdk(&root).join("marker-b.txt").exists());

    // 原真实目录被改名挪开,数据仍在
    let baks = old_backups(&root);
    assert_eq!(baks.len(), 1);
    assert!(root.join("current").join(&baks[0]).join("user-file.txt").exists());

    fs::remove_dir_all(&root).unwrap();
}
