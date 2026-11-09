use std::fs;
use std::io::{Read, Write};
use std::os::windows::fs::MetadataExt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::detect::system::path_within;
use crate::error::{AppError, Result};
use crate::types::EnvType;

pub static LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Version-dependent manager semantics that the static catalog cannot express.
/// The caller resolves these from the manager that will actually run.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Capabilities {
    /// Yarn 2+ uses `globalFolder` for global data, not as a Classic global
    /// install directory; migrating it as one would move the wrong resource.
    pub yarn_berry: bool,
}

impl Capabilities {
    pub fn for_tool(tool: &str, version: Option<&str>) -> Self {
        Self {
            yarn_berry: tool == "yarn"
                && version
                    .and_then(|value| value.split('.').next())
                    .and_then(|value| value.parse::<u32>().ok())
                    .is_some_and(|major| major >= 2),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MigrationPlan {
    pub tool: String,
    pub kind: String,
    pub source: String,
    pub target: String,
    pub files: u64,
    pub bytes: u64,
    pub links: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationResult {
    pub target: String,
    pub backup: String,
    pub files: u64,
    pub bytes: u64,
    pub journal: String,
}

#[derive(Debug, PartialEq)]
struct Entry {
    relative: PathBuf,
    size: u64,
    modified: Option<std::time::SystemTime>,
    link: Option<PathBuf>,
    directory: bool,
}

fn inventory(root: &Path) -> Result<Vec<Entry>> {
    let mut entries = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            let meta = fs::symlink_metadata(&path)?;
            let reparse = meta.file_attributes() & 0x400 != 0;
            let link = if reparse {
                // Preserve junction references without copying their targets or traversing cycles.
                if meta.file_attributes() & 0x10 == 0 {
                    return Err(AppError::msg(format!(
                        "暂不迁移文件符号链接: {}",
                        path.display()
                    )));
                }
                Some(fs::read_link(&path).map_err(|_| {
                    AppError::msg(format!("不支持迁移此链接或重解析点: {}", path.display()))
                })?)
            } else {
                None
            };
            if meta.is_dir() && !reparse {
                pending.push(path.clone());
            }
            entries.push(Entry {
                relative: path.strip_prefix(root).unwrap().to_path_buf(),
                size: if meta.is_file() && !reparse {
                    meta.len()
                } else {
                    0
                },
                modified: meta.modified().ok(),
                link,
                directory: meta.is_dir() || reparse,
            });
        }
    }
    entries.sort_by(|a, b| a.relative.cmp(&b.relative));
    Ok(entries)
}

fn absolute_identity(path: &Path) -> Result<PathBuf> {
    if !path.is_absolute()
        || path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(AppError::msg("必须使用不含 .. 的绝对路径"));
    }
    if fs::symlink_metadata(path).is_ok() {
        return Ok(path.canonicalize()?);
    }
    let parent = path.parent().ok_or_else(|| AppError::msg("无效目标路径"))?;
    Ok(absolute_identity(parent)?.join(
        path.file_name()
            .ok_or_else(|| AppError::msg("无效目录名称"))?,
    ))
}

pub fn preview(
    root: &Path,
    tool: &str,
    kind: &str,
    source: &Path,
    capabilities: Capabilities,
) -> Result<MigrationPlan> {
    let definition = crate::package_managers::definition(tool)?;
    if !matches!(kind, "global" | "cache") || (kind == "global" && !definition.migrate_global) {
        return Err(AppError::msg(
            "不支持此迁移类型；解释器内的 Python 包请使用重装功能",
        ));
    }
    if kind == "global" && tool == "yarn" && capabilities.yarn_berry {
        return Err(AppError::msg(
            "Yarn 2+ 的 globalFolder 是全局数据目录，不是 Classic 全局安装目录；不提供该迁移。请使用 Yarn 自身的项目级版本管理",
        ));
    }
    let metadata = fs::symlink_metadata(source)?;
    if metadata.file_attributes() & 0x400 != 0 || !metadata.is_dir() {
        return Err(AppError::msg(
            "源目录必须是普通目录；已迁移的链接无需再次迁移",
        ));
    }
    let source = absolute_identity(source)?;
    if kind == "global" {
        if let Some(issue) = crate::package_managers::legacy_layout_issue(tool, &source) {
            return Err(AppError::msg(format!(
                "{issue}。迁移会保留错误布局；请先使用恢复预览核对工具和入口目录"
            )));
        }
    }
    let target_path = root.join("globals").join(format!("{tool}-{kind}"));
    if fs::symlink_metadata(&target_path).is_ok_and(|meta| meta.file_attributes() & 0x400 != 0) {
        return Err(AppError::msg("目标目录不能是链接"));
    }
    let target = absolute_identity(&target_path)?;
    if path_within(&source, &target) || path_within(&target, &source) {
        return Err(AppError::msg(
            "源目录和目标目录不能相同、互为父子或通过链接指向同一位置",
        ));
    }
    if path_within(&source, root) || path_within(root, &source) {
        return Err(AppError::msg(
            "不能迁移管理根目录或与管理根目录重叠的目录",
        ));
    }
    for protected in [
        dirs::home_dir(),
        dirs::data_dir(),
        dirs::data_local_dir(),
        std::env::var_os("WINDIR").map(PathBuf::from),
        std::env::var_os("ProgramFiles").map(PathBuf::from),
    ]
    .into_iter()
    .flatten()
    {
        if path_within(&source, &protected) {
            return Err(AppError::msg(
                "不能迁移用户目录、系统目录或它们的父目录",
            ));
        }
    }
    if source.parent().is_none()
        || [
            "node.exe",
            "python.exe",
            "pyvenv.cfg",
            "bin/rustup.exe",
            "rustup.exe",
            "bun.exe",
            "dotnet.exe",
            "pwsh.exe",
            "powershell.exe",
        ]
        .iter()
        .any(|p| source.join(p).exists())
    {
        return Err(AppError::msg(
            "源目录包含运行时或虚拟环境，不能整体迁移；请使用独立的全局包目录",
        ));
    }
    if let Ok(meta) = fs::symlink_metadata(&target) {
        if meta.file_attributes() & 0x400 != 0
            || !meta.is_dir()
            || fs::read_dir(&target)?.next().is_some()
        {
            return Err(AppError::msg(
                "目标目录非空或是链接，本工具不会覆盖或合并已有数据",
            ));
        }
    }
    let entries = inventory(&source)?;
    Ok(MigrationPlan {
        tool: tool.into(),
        kind: kind.into(),
        source: source.to_string_lossy().into(),
        target: target.to_string_lossy().into(),
        files: entries.iter().filter(|e| !e.directory).count() as u64,
        bytes: entries.iter().map(|e| e.size).sum(),
        links: entries.iter().filter(|e| e.link.is_some()).count() as u64,
    })
}

fn copy_verified(source: &Path, target: &Path) -> Result<()> {
    let mut input = fs::File::open(source)?;
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(target)?;
    std::io::copy(&mut input, &mut output)?;
    output.flush()?;
    output.sync_all()?;
    drop(output);
    verify_file(source, target)
}

fn verify_file(source: &Path, target: &Path) -> Result<()> {
    let mut a = fs::File::open(source)?;
    let mut b = fs::File::open(target)?;
    let mut left = [0u8; 65536];
    let mut right = [0u8; 65536];
    loop {
        let count = a.read(&mut left)?;
        if count == 0 {
            if b.read(&mut right[..1])? == 0 {
                return Ok(());
            }
            break;
        }
        if b.read_exact(&mut right[..count]).is_err() || left[..count] != right[..count] {
            break;
        }
    }
    Err(AppError::msg(format!("文件校验失败: {}", source.display())))
}

pub fn execute(
    root: &Path,
    requested: &MigrationPlan,
    capabilities: Capabilities,
) -> Result<MigrationResult> {
    let plan = preview(
        root,
        &requested.tool,
        &requested.kind,
        Path::new(&requested.source),
        capabilities,
    )?;
    if &plan != requested {
        return Err(AppError::msg("目录或配置已变化，请重新预览迁移"));
    }
    let source = PathBuf::from(&plan.source);
    let target = PathBuf::from(&plan.target);
    let entries = inventory(&source)?;
    fs::create_dir_all(target.parent().unwrap())?;
    if target.exists() {
        fs::remove_dir(&target)?;
    }
    // Exclusive creation ensures another operation cannot turn this into a merge.
    fs::create_dir(&target)?;
    let operation = || -> Result<(PathBuf, PathBuf)> {
        for entry in &entries {
            let destination = target.join(&entry.relative);
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent)?;
            }
            if let Some(link) = &entry.link {
                let link = if link.is_absolute() {
                    link.clone()
                } else {
                    source.join(&entry.relative).parent().unwrap().join(link)
                };
                junction::create(link, &destination)?;
            } else if entry.directory {
                fs::create_dir_all(&destination)?;
            } else {
                copy_verified(&source.join(&entry.relative), &destination)?;
            }
        }
        if inventory(&source)? != entries {
            return Err(AppError::msg(
                "迁移期间源目录发生变化，请关闭包管理器后重试",
            ));
        }
        let backup = source.with_file_name(format!(
            "{}.envcon-backup-{}",
            source.file_name().unwrap().to_string_lossy(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let journal = target.parent().unwrap().join(format!(
            "{}.json",
            backup.file_name().unwrap().to_string_lossy()
        ));
        let mut record = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&journal)?;
        serde_json::to_writer_pretty(
            &mut record,
            &serde_json::json!({
                "plan": plan, "backup": backup, "recovery": "Inspect source junction and backup before restoring; never merge or overwrite either directory."
            }),
        )?;
        record.flush()?;
        record.sync_all()?;
        fs::rename(&source, &backup)?;
        // Validate once more after renaming so open writers are detected before switching paths.
        let finish = || -> Result<()> {
            if inventory(&backup)? != entries {
                return Err(AppError::msg("源数据在迁移期间变化"));
            }
            for entry in entries.iter().filter(|e| !e.directory) {
                verify_file(&backup.join(&entry.relative), &target.join(&entry.relative))?;
            }
            junction::create(&target, &source)?;
            Ok(())
        };
        if let Err(error) = finish() {
            if let Err(restore) = fs::rename(&backup, &source) {
                return Err(AppError::msg(format!(
                    "{error}；恢复失败: {restore}；原数据备份: {}",
                    backup.display()
                )));
            }
            return Err(error);
        }
        Ok((backup, journal))
    };
    let (backup, journal) = operation().map_err(|error| {
        AppError::msg(format!(
            "{error}；目标副本保留在 {}，未覆盖原数据。重试前请检查并移走该副本",
            target.display()
        ))
    })?;
    Ok(MigrationResult {
        target: plan.target,
        backup: backup.to_string_lossy().into(),
        files: plan.files,
        bytes: plan.bytes,
        journal: journal.to_string_lossy().into(),
    })
}

/// 把一个已纳管的外部环境真正搬进受管目录的计划。
///
/// 与 `MigrationPlan` 的区别：迁移针对包管理器的全局目录，完成时在**原位置留下
/// junction** 指向新位置；本计划针对已纳管环境的外部目标，完成后**原路径不留任何
/// 东西**（用户明确要求）。因此两者的 `target` 语义与收尾动作都不同，不能共用一个
/// 结构。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MovePlan {
    pub env_type: EnvType,
    /// 受管环境名（`envs/<folder>/` 下的目录名）。
    pub name: String,
    /// 当前 junction 指向的外部目录。
    pub source: String,
    /// 搬入后的受管目录。
    pub target: String,
    pub files: u64,
    pub bytes: u64,
    pub links: u64,
}

/// 预览把外部纳管环境搬进受管目录的完整计划。
///
/// 只读：不创建、不移动、不删除任何东西。所有拒绝理由都在触碰磁盘之前给出。
///
/// 关键点：纳管本身就是把 `envs/<folder>/<name>` 这个 junction 指向外部目录，因此
/// **搬入的目标路径与当前链接是同一个位置**。搬移的动作是「删掉链接，再在该位置
/// 建一个装着同样内容的真实目录」，而不是往另一个空位复制。
pub fn preview_move(root: &Path, env_type: EnvType, name: &str) -> Result<MovePlan> {
    crate::switcher::validate_name(name)?;
    if !root.is_dir() {
        return Err(AppError::msg("管理根目录不存在，请先在设置中配置"));
    }
    let cat_dir = root.join("envs").join(env_type.folder());
    let target = cat_dir.join(name);
    // 搬移的前提是这一条目确实是本应用纳管的链接。受管普通目录本来就在受管位置，
    // 对它执行「搬入」会变成自己搬自己。
    if !crate::detect::system::is_reparse_path(&target) {
        return Err(AppError::msg(
            "该条目不是外部纳管链接；只有纳管的外部环境可以搬入受管目录",
        ));
    }
    let raw = std::fs::read_link(&target)?;
    let source = if raw.is_absolute() {
        raw
    } else {
        target.parent().unwrap_or(root).join(raw)
    };
    let source = source.canonicalize().map_err(|error| {
        AppError::msg(format!("外部目录无法访问，请确认它仍然存在: {error}"))
    })?;
    // 源必须是普通目录：若它本身又是链接，搬移会变成搬移别人的指向。
    let meta = fs::symlink_metadata(&source)?;
    if meta.file_attributes() & 0x400 != 0 || !meta.is_dir() {
        return Err(AppError::msg(
            "外部目标的身份不是普通目录，不能搬入受管目录",
        ));
    }
    // 分类目录与 current 都不能是重定向点：搬移会先删链接再建真实目录，
    // 若父级可被重定向，实际写入位置就不受控了。
    if crate::detect::system::has_reparse_component(&cat_dir) {
        return Err(AppError::msg(
            "环境分类目录或其上级是符号链接，禁止在其中创建真实目录",
        ));
    }
    if crate::detect::system::has_reparse_component(&root.join("current")) {
        return Err(AppError::msg("current 父目录是重定向点，禁止修改环境"));
    }
    let root_identity = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    // 源在管理根目录内说明它已经是受管内容，没有外部目录可搬。
    // 注意参数顺序：path_within(root, candidate) 表示 candidate 在 root 内。
    if path_within(&root_identity, &source) {
        return Err(AppError::msg("外部目录位于管理根目录内，无需搬移"));
    }
    for protected in [
        dirs::home_dir(),
        dirs::data_dir(),
        dirs::data_local_dir(),
        std::env::var_os("WINDIR").map(PathBuf::from),
        std::env::var_os("ProgramFiles").map(PathBuf::from),
    ]
    .into_iter()
    .flatten()
    {
        // 只拒绝「源是受保护目录本身或其祖先」：搬 C:\Users 会把整个用户目录连根
        // 搬走。源位于受保护目录**之下**是正常情况（软件常装在 AppData\Local），
        // 不能一概拒绝。参数顺序：path_within(root, candidate) 表示 candidate 在
        // root 内，因此这里读作「protected 在 source 内」。
        if path_within(&source, &protected) {
            return Err(AppError::msg(
                "不能搬移用户目录、系统目录或它们的父目录",
            ));
        }
    }
    let entries = inventory(&source)?;
    Ok(MovePlan {
        env_type,
        name: name.to_string(),
        source: source.to_string_lossy().into(),
        target: target.to_string_lossy().into(),
        files: entries.iter().filter(|entry| !entry.directory).count() as u64,
        bytes: entries.iter().map(|entry| entry.size).sum(),
        links: entries.iter().filter(|entry| entry.link.is_some()).count() as u64,
    })
}

/// 搬移的结果。外部目录**仍然存在**：按需求方决定，流程是「先移动，再可选删除
/// 旧目录」，删除由独立的 `preview_discard`/`execute_discard` 处理，不在这一步顺手
/// 删掉用户的原始数据。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveResult {
    pub target: String,
    /// 外部原始目录，仍然保留，待用户确认后单独删除。
    pub source: String,
    pub files: u64,
    pub bytes: u64,
}

/// 清理搬移过程中自己创建的临时目录。
///
/// 递归删除只允许作用在本函数亲手建的那一个目录上，因此删之前逐一确认：位于目标
/// 分类目录内、名字符合本函数生成的形状、本身不是重解析点。任一不满足就放弃清理
/// 并保留现场，绝不猜测。
/// 清理同一环境此前遗留的临时目录。
///
/// 搬移在复制阶段被中断（进程被杀、断电）时，`.{name}-moving-<stamp>` 会留在分类
/// 目录里。扫描会跳过它（以 `.` 开头），所以不会显示成环境，但它会一直占着磁盘，
/// 且下一次搬移无法复用这个名字——于是每中断一次就多一份残留。
///
/// 只清理**同名**前缀的目录：别的环境的残留不归本次管，普通环境目录更不碰。删除
/// 前逐一确认是普通目录而非重解析点，任一不满足就跳过并保留现场。
fn sweep_stale_staging(cat_dir: &Path, name: &str) {
    let prefix = format!(".{name}-moving-");
    let Ok(reader) = fs::read_dir(cat_dir) else {
        return;
    };
    for entry in reader.flatten() {
        let file_name = entry.file_name().to_string_lossy().to_string();
        if !file_name.starts_with(&prefix) {
            continue;
        }
        let path = entry.path();
        if crate::detect::system::is_reparse_path(&path) {
            continue;
        }
        if !fs::symlink_metadata(&path).is_ok_and(|meta| meta.is_dir()) {
            continue;
        }
        let _ = fs::remove_dir_all(&path);
    }
}

fn discard_staging(staging: &Path, cat_dir: &Path, name: &str) {
    let Some(file_name) = staging.file_name().map(|value| value.to_string_lossy().to_string()) else {
        return;
    };
    if !file_name.starts_with(&format!(".{name}-moving-")) {
        return;
    }
    if staging.parent() != Some(cat_dir) {
        return;
    }
    if crate::detect::system::is_reparse_path(staging) {
        return;
    }
    if !fs::symlink_metadata(staging).is_ok_and(|meta| meta.is_dir()) {
        return;
    }
    let _ = fs::remove_dir_all(staging);
}

/// 把已纳管的外部环境真正搬进受管目录。
///
/// 顺序是「先复制、再校验、最后替换」，任何一步失败都保留原有链接与外部目录：
/// 1. 复核预览，外部目录或环境状态变化则拒绝；
/// 2. 复制到同一分类目录下的临时目录（以 `.` 开头，扫描会跳过）；
/// 3. 逐字节校验，并确认复制期间源目录没有变化；
/// 4. 删除纳管链接（走 `link_ownership`，只删本应用创建的链接）；
/// 5. 把临时目录重命名到目标位置；失败则重建链接回滚。
///
/// 完成后 `envs/<folder>/<name>` 是真实目录，原**位置**不再有链接或占位；外部目录
/// 本身保留，供用户核对无误后单独删除。
pub fn execute_move(root: &Path, requested: &MovePlan) -> Result<MoveResult> {
    let plan = preview_move(root, requested.env_type, &requested.name)?;
    if &plan != requested {
        return Err(AppError::msg("外部目录或环境状态已变化，请重新预览"));
    }
    let source = PathBuf::from(&plan.source);
    let target = PathBuf::from(&plan.target);
    let cat_dir = target
        .parent()
        .ok_or_else(|| AppError::msg("目标分类目录无效"))?
        .to_path_buf();
    let entries = inventory(&source)?;
    if entries.iter().filter(|entry| !entry.directory).count() as u64 != plan.files
        || entries.iter().map(|entry| entry.size).sum::<u64>() != plan.bytes
    {
        return Err(AppError::msg("外部目录内容已变化，请重新预览"));
    }

    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let staging = cat_dir.join(format!(".{}-moving-{stamp}", plan.name));
    // 上一次搬移可能被中断而留下同名临时目录。它被扫描跳过、不会显示成环境，
    // 但会一直占着磁盘；不清理则每中断一次就多一份残留。
    sweep_stale_staging(&cat_dir, &plan.name);
    fs::create_dir(&staging)?;

    let copy = || -> Result<()> {
        for entry in &entries {
            let destination = staging.join(&entry.relative);
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent)?;
            }
            if let Some(link) = &entry.link {
                // 相对链接按原位置解析，否则搬移后指向会变。
                let link = if link.is_absolute() {
                    link.clone()
                } else {
                    source.join(&entry.relative).parent().unwrap().join(link)
                };
                junction::create(link, &destination)?;
            } else if entry.directory {
                fs::create_dir_all(&destination)?;
            } else {
                copy_verified(&source.join(&entry.relative), &destination)?;
            }
        }
        Ok(())
    };
    if let Err(error) = copy() {
        discard_staging(&staging, &cat_dir, &plan.name);
        return Err(AppError::msg(format!(
            "复制失败，原外部目录与纳管链接均保留: {error}"
        )));
    }
    // 复制可能耗时，期间源目录可能被改动；改动过的内容不能当成已验证的副本。
    if inventory(&source)? != entries {
        discard_staging(&staging, &cat_dir, &plan.name);
        return Err(AppError::msg(
            "复制期间外部目录发生变化，已放弃并保留原链接；请关闭相关程序后重试",
        ));
    }

    let fingerprint = crate::link_ownership::fingerprint(root, &target)?;
    if let Err(error) = crate::link_ownership::remove(root, &target, Some(&fingerprint)) {
        discard_staging(&staging, &cat_dir, &plan.name);
        return Err(AppError::msg(format!(
            "移除纳管链接失败，已放弃搬移并保留原链接: {error}"
        )));
    }
    if let Err(error) = fs::rename(&staging, &target) {
        // 链接已删，必须把它恢复回去，否则环境列表会凭空少一条。
        let restored = crate::link_ownership::create(root, &target, &source);
        discard_staging(&staging, &cat_dir, &plan.name);
        return Err(AppError::msg(format!(
            "替换为真实目录失败: {error}；原纳管链接恢复结果: {}",
            match restored {
                Ok(()) => "已恢复".to_string(),
                Err(restore) => format!("恢复失败，请手动重建链接: {restore}"),
            }
        )));
    }
    // 记录来源：搬移后受管位置是真实目录，不再有链接指向外部旧目录，只能靠这条
    // 记录回答「旧目录在哪」。没有它，用户一旦取消删除就再也找不到入口。
    // 记录失败不算搬移失败——搬移本身已经成功，只是少了后续入口；如实上报原因。
    let journal = crate::move_journal::record(
        root,
        plan.env_type,
        &plan.name,
        Path::new(&plan.source),
    )
    .map_err(|error| {
        AppError::msg(format!(
            "已搬入受管目录，但来源记录写入失败，后续将无法从这里删除旧目录: {error}"
        ))
    });
    journal?;
    Ok(MoveResult {
        target: plan.target,
        source: plan.source,
        files: plan.files,
        bytes: plan.bytes,
    })
}

/// 删除搬移后残留的外部旧目录的计划。
///
/// 这是**不可恢复**操作，因此前置条件比搬移更严：必须由调用方显式给出外部路径
/// （搬移完成后受管目录已不指向它，无法从链接反推），且必须证明受管目录中有一份
/// 相对路径与字节数完全一致的副本。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DiscardPlan {
    pub env_type: EnvType,
    /// 已搬入受管目录的环境名。
    pub name: String,
    /// 待删除的外部旧目录。
    pub source: String,
    pub files: u64,
    pub bytes: u64,
}

/// 两个目录的相对路径集合与字节数必须完全相同。
///
/// 只比较结构与大小，不比较内容哈希：搬移时已经逐字节校验过，这里要回答的是
/// 「受管副本是否仍然覆盖旧目录的每一项」，而不是再做一次全量读取。
fn same_shape(left: &[Entry], right: &[Entry]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter().zip(right).all(|(a, b)| {
        a.relative == b.relative && a.size == b.size && a.directory == b.directory
    })
}

/// 预览删除外部旧目录。只读，不删除任何东西。
pub fn preview_discard(
    root: &Path,
    env_type: EnvType,
    name: &str,
    source: &Path,
) -> Result<DiscardPlan> {
    crate::switcher::validate_name(name)?;
    let managed = root.join("envs").join(env_type.folder()).join(name);
    // 受管位置必须是真实目录：仍是链接说明还没搬移，此时删外部目录等于删掉环境本身。
    if crate::detect::system::is_reparse_path(&managed) {
        return Err(AppError::msg(
            "该环境尚未搬入受管目录，不能删除外部目录",
        ));
    }
    if !managed.is_dir() {
        return Err(AppError::msg("受管环境目录不存在"));
    }
    let source = source
        .canonicalize()
        .map_err(|error| AppError::msg(format!("外部旧目录无法访问: {error}")))?;
    let meta = fs::symlink_metadata(&source)?;
    if meta.file_attributes() & 0x400 != 0 || !meta.is_dir() {
        return Err(AppError::msg("待删除的外部路径不是普通目录"));
    }
    // 绝不能删到管理根目录或它的祖先；也不能删受管目录本身。
    let root_identity = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    if path_within(&source, &root_identity) || path_within(&root_identity, &source) {
        return Err(AppError::msg(
            "不能删除管理根目录、其祖先或其内部目录",
        ));
    }
    for protected in [
        dirs::home_dir(),
        dirs::data_dir(),
        dirs::data_local_dir(),
        std::env::var_os("WINDIR").map(PathBuf::from),
        std::env::var_os("ProgramFiles").map(PathBuf::from),
    ]
    .into_iter()
    .flatten()
    {
        if path_within(&source, &protected) {
            return Err(AppError::msg(
                "不能删除用户目录、系统目录或它们的父目录",
            ));
        }
    }
    let external = inventory(&source)?;
    let copied = inventory(&managed)?;
    if !same_shape(&external, &copied) {
        return Err(AppError::msg(
            "受管副本与外部旧目录不完全一致（相对路径或大小不同），拒绝删除；请先重新搬移或手动核对",
        ));
    }
    Ok(DiscardPlan {
        env_type,
        name: name.to_string(),
        source: source.to_string_lossy().into(),
        files: external.iter().filter(|entry| !entry.directory).count() as u64,
        bytes: external.iter().map(|entry| entry.size).sum(),
    })
}

/// 删除外部旧目录。**不可恢复**，因此每一步都重新验证而不是依赖预览时的结论。
pub fn execute_discard(root: &Path, requested: &DiscardPlan) -> Result<()> {
    let plan = preview_discard(
        root,
        requested.env_type,
        &requested.name,
        Path::new(&requested.source),
    )?;
    if &plan != requested {
        return Err(AppError::msg("外部目录或受管副本已变化，请重新预览"));
    }
    let source = PathBuf::from(&plan.source);
    // 最后一道闸：递归删除前再确认它不是重解析点。若这里已是链接，删掉的会是
    // 别人的目录，而不是我们核对过的那一个。
    if crate::detect::system::is_reparse_path(&source) {
        return Err(AppError::msg("待删除路径在删除前变成了链接，已停止"));
    }
    if !fs::symlink_metadata(&source).is_ok_and(|meta| meta.is_dir()) {
        return Err(AppError::msg("待删除路径在删除前已不是普通目录，已停止"));
    }
    fs::remove_dir_all(&source).map_err(|error| {
        AppError::msg(format!(
            "删除外部旧目录失败，可能有程序占用: {error}；受管副本不受影响"
        ))
    })?;
    // 旧目录已删除，来源记录不再有指向对象。清除失败不影响结果：下次读取时
    // `preview_discard` 会发现路径不存在并拒绝，不会误删别的东西。
    let _ = crate::move_journal::forget(root, plan.env_type, &plan.name);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestDir;

    /// 纳管一个外部环境，返回 (root, 外部目录, 纳管链接)。
    ///
    /// 走 `link_ownership::create` 而不是裸 `junction::create`：真实纳管
    /// （`integrate_external_env`）会同时写入来源记录，执行搬移时需要这份 record
    /// 才能证明链接归属。
    fn managed_external(
        fixture: &TestDir,
        env_type: EnvType,
        name: &str,
    ) -> (PathBuf, PathBuf, PathBuf) {
        let root = fixture.path().join("managed");
        let external = fixture.path().join("external-install");
        fs::create_dir_all(external.join("bin")).unwrap();
        fs::write(external.join("bin/tool.exe"), b"payload").unwrap();
        fs::write(external.join("version.txt"), b"1.0").unwrap();
        let link = root.join("envs").join(env_type.folder()).join(name);
        fs::create_dir_all(link.parent().unwrap()).unwrap();
        fs::create_dir_all(root.join("current")).unwrap();
        crate::link_ownership::create(&root, &link, &external).unwrap();
        (root, external, link)
    }

    /// 预览必须如实报告源、目标、文件数与字节数，且不触碰磁盘。
    #[test]
    fn move_preview_reports_source_target_and_inventory_without_touching_disk() {
        let fixture = TestDir::new();
        let (root, external, link) = managed_external(&fixture, EnvType::Node, "system-22");

        let plan = preview_move(&root, EnvType::Node, "system-22").unwrap();
        assert_eq!(plan.name, "system-22");
        assert_eq!(
            fs::canonicalize(&plan.source).unwrap(),
            fs::canonicalize(&external).unwrap()
        );
        // 目标就是当前纳管链接所在的位置：搬移＝把链接换成真实目录。
        // 不能 canonicalize 它——那会跟随链接解析到外部目录，比较必然不等。
        assert_eq!(Path::new(&plan.target), link.as_path());
        assert_eq!(plan.files, 2);
        assert_eq!(plan.bytes, b"payload".len() as u64 + b"1.0".len() as u64);
        assert_eq!(plan.links, 0);

        // 预览是只读的：链接对象仍在、外部内容原样。
        assert!(fs::symlink_metadata(&link).is_ok(), "链接对象必须保留");
        assert_eq!(fs::read(external.join("version.txt")).unwrap(), b"1.0");
    }

    /// 只有纳管链接可以搬入；受管普通目录与不存在的名字都必须被拒绝。
    #[test]
    fn move_preview_rejects_entries_that_are_not_external_links() {
        let fixture = TestDir::new();
        let root = fixture.path().join("managed");
        let managed_dir = root.join("envs").join(EnvType::Node.folder()).join("node-22");
        fs::create_dir_all(&managed_dir).unwrap();
        fs::write(managed_dir.join("node.exe"), b"fixture").unwrap();

        // 受管普通目录：本来就在受管位置，无需搬移。
        let error = preview_move(&root, EnvType::Node, "node-22")
            .unwrap_err()
            .to_string();
        assert!(error.contains("不是外部纳管链接"), "unexpected: {error}");

        // 不存在的名字。
        assert!(preview_move(&root, EnvType::Node, "absent").is_err());

        // 非法名称不能借道逃出受管目录。
        assert!(preview_move(&root, EnvType::Node, "../outside").is_err());
        assert!(preview_move(&root, EnvType::Node, "D:\\outside").is_err());
    }

    /// 目标悬空时报告「外部目录无法访问」，而不是给出一个注定失败的移动计划。
    #[test]
    fn move_preview_reports_a_vanished_external_target() {
        let fixture = TestDir::new();
        let (root, external, link) = managed_external(&fixture, EnvType::Node, "system-22");
        // 保留链接、删掉外部目录：这是「用户手工删了源目录」的真实状态。
        fs::remove_dir_all(&external).unwrap();
        let error = preview_move(&root, EnvType::Node, "system-22")
            .unwrap_err()
            .to_string();
        assert!(error.contains("无法访问"), "unexpected: {error}");
        // 悬空链接对象仍在：预览失败不应顺手删除链接。
        // 不能用 junction::exists——它会解析目标，目标已删就返回 false。
        assert!(fs::symlink_metadata(&link).is_ok(), "悬空链接也必须保留");
    }

    /// 外部目标位于管理根目录内（即已经受管）时拒绝，避免自己搬自己。
    #[test]
    fn move_preview_rejects_a_target_already_inside_the_managed_root() {
        let fixture = TestDir::new();
        let root = fixture.path().join("managed");
        let inner = root.join("envs/nodes/real-node");
        fs::create_dir_all(&inner).unwrap();
        fs::write(inner.join("node.exe"), b"fixture").unwrap();
        let link = root.join("envs").join(EnvType::Node.folder()).join("alias");
        fs::create_dir_all(link.parent().unwrap()).unwrap();
        fs::create_dir_all(root.join("current")).unwrap();
        junction::create(&inner, &link).unwrap();

        let error = preview_move(&root, EnvType::Node, "alias")
            .unwrap_err()
            .to_string();
        assert!(error.contains("管理根目录内"), "unexpected: {error}");
    }

    /// 收集分类目录下本应用自己的临时目录残留（名字里带 `-moving-`）。
    fn staging_leftovers(root: &Path, env_type: EnvType) -> Vec<String> {
        fs::read_dir(root.join("envs").join(env_type.folder()))
            .map(|reader| {
                reader
                    .filter_map(|entry| entry.ok())
                    .map(|entry| entry.file_name().to_string_lossy().to_string())
                    .filter(|name| name.contains("-moving-"))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// 搬移被中断会留下 `.name-moving-<stamp>` 残留。它被扫描跳过、不会显示成环境，
    /// 但会一直占磁盘。下一次搬移同一环境时必须清掉自己的残留，且只清自己的。
    #[test]
    fn a_new_move_sweeps_its_own_stale_staging_and_nothing_else() {
        let fixture = TestDir::new();
        let (root, _external, _link) = managed_external(&fixture, EnvType::Node, "system-22");
        let cat_dir = root.join("envs").join(EnvType::Node.folder());

        // 模拟三次中断残留：本环境的两个、别的环境的一个。
        for name in [".system-22-moving-1", ".system-22-moving-2", ".system-23-moving-9"] {
            let path = cat_dir.join(name);
            fs::create_dir_all(&path).unwrap();
            fs::write(path.join("half-copied.bin"), b"partial").unwrap();
        }
        // 普通环境目录不能被当成残留删掉。
        let real = cat_dir.join("system-22-old");
        fs::create_dir_all(&real).unwrap();
        fs::write(real.join("keep"), b"keep").unwrap();

        let plan = preview_move(&root, EnvType::Node, "system-22").unwrap();
        execute_move(&root, &plan).unwrap();

        // 本环境的残留被清掉。
        assert!(!cat_dir.join(".system-22-moving-1").exists());
        assert!(!cat_dir.join(".system-22-moving-2").exists());
        // 别的环境的残留不归本次管，必须原样保留。
        assert!(cat_dir.join(".system-23-moving-9").join("half-copied.bin").is_file());
        // 普通目录不受影响。
        assert_eq!(fs::read(real.join("keep")).unwrap(), b"keep");
        // 本次搬移自己的临时目录也不残留。
        assert!(staging_leftovers(&root, EnvType::Node)
            .iter()
            .all(|name| !name.starts_with(".system-22-moving-") || name == ".system-22-moving-1" || name == ".system-22-moving-2"));
    }

    /// 移动完成后，受管位置必须是真实目录，内容逐字节一致；外部原始目录按「先移动、
    /// 再可选删除」的设计**仍然保留**，但外部原路径不留下任何回指链接。
    #[test]
    fn move_exec_materializes_a_real_directory_and_leaves_no_back_link() {
        let fixture = TestDir::new();
        let (root, external, link) = managed_external(&fixture, EnvType::Node, "system-22");
        assert!(
            crate::detect::system::is_reparse_path(&link),
            "前置：纳管后应是链接"
        );
        let plan = preview_move(&root, EnvType::Node, "system-22").unwrap();
        let result = execute_move(&root, &plan).unwrap();

        assert_eq!(result.files, 2);
        assert_eq!(result.bytes, plan.bytes);
        // 受管位置变成真实目录，不再是链接。
        assert!(!crate::detect::system::is_reparse_path(&link));
        assert!(fs::symlink_metadata(&link).unwrap().is_dir());
        // 内容完整复制。
        assert_eq!(fs::read(link.join("bin/tool.exe")).unwrap(), b"payload");
        assert_eq!(fs::read(link.join("version.txt")).unwrap(), b"1.0");
        // 外部原始目录仍保留：删除是独立的后续步骤，不能顺手删用户数据。
        assert_eq!(fs::read(external.join("version.txt")).unwrap(), b"1.0");
        // 外部原路径不保留任何东西——特别是不留回指链接。
        assert!(!crate::detect::system::is_reparse_path(&external));
        // 临时目录必须清理干净，否则会以 `.` 开头藏在分类目录里。
        assert!(staging_leftovers(&root, EnvType::Node).is_empty());
        // 来源必须被记录下来：搬移后受管位置是真实目录，没有链接指向外部旧目录，
        // 若不记录，用户取消删除后就再也找不到「删除旧目录」的入口。
        let recorded = crate::move_journal::read(&root, EnvType::Node, "system-22")
            .expect("搬移成功后必须留下来源记录");
        assert_eq!(
            crate::detect::system::path_key_with_long_names(Path::new(&recorded)),
            crate::detect::system::path_key_with_long_names(&external)
        );
    }

    /// 搬移 → 记录 → 删除旧目录 → 清除记录，整条链路的闭环。
    #[test]
    fn move_then_discard_clears_the_source_record() {
        let fixture = TestDir::new();
        let (root, external, link) = managed_external(&fixture, EnvType::Node, "system-22");
        let move_plan = preview_move(&root, EnvType::Node, "system-22").unwrap();
        execute_move(&root, &move_plan).unwrap();
        assert!(crate::move_journal::read(&root, EnvType::Node, "system-22").is_some());

        let discard = preview_discard(&root, EnvType::Node, "system-22", &external).unwrap();
        execute_discard(&root, &discard).unwrap();

        // 旧目录消失，记录随之清除：再点「删除旧目录」不会指向一个已不存在的路径。
        assert!(!external.exists());
        assert_eq!(crate::move_journal::read(&root, EnvType::Node, "system-22"), None);
        // 受管副本完好。
        assert_eq!(fs::read(link.join("version.txt")).unwrap(), b"1.0");
    }

    /// 搬移失败时不能留下来源记录：否则界面会出现一个指向未搬移环境的删除入口。
    #[test]
    fn a_failed_move_leaves_no_source_record() {
        use std::os::windows::fs::OpenOptionsExt;
        let fixture = TestDir::new();
        let (root, external, _link) = managed_external(&fixture, EnvType::Node, "system-22");
        let plan = preview_move(&root, EnvType::Node, "system-22").unwrap();
        let lock = fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(external.join("version.txt"))
            .unwrap();

        assert!(execute_move(&root, &plan).is_err());
        drop(lock);
        assert_eq!(crate::move_journal::read(&root, EnvType::Node, "system-22"), None);
    }

    /// 预览之后外部目录被改动，执行必须拒绝，且不动链接与外部内容。
    #[test]
    fn move_exec_refuses_when_the_external_directory_changed_after_preview() {
        let fixture = TestDir::new();
        let (root, external, link) = managed_external(&fixture, EnvType::Node, "system-22");
        let plan = preview_move(&root, EnvType::Node, "system-22").unwrap();
        fs::write(external.join("added.txt"), b"new").unwrap();

        assert!(execute_move(&root, &plan).is_err());
        // 链接与外部内容都必须保持原样：拒绝发生在任何破坏之前。
        assert!(crate::detect::system::is_reparse_path(&link));
        assert_eq!(fs::read(external.join("added.txt")).unwrap(), b"new");
        assert!(staging_leftovers(&root, EnvType::Node).is_empty());
    }

    /// 复制阶段失败（源文件被独占打开）时：原链接保留、外部内容不动、临时目录清理。
    /// 不精确断言失败发生在哪一步——三条不变量在任何失败点都必须成立。
    #[test]
    fn move_exec_failure_preserves_the_link_and_discards_staging() {
        use std::os::windows::fs::OpenOptionsExt;
        let fixture = TestDir::new();
        let (root, external, link) = managed_external(&fixture, EnvType::Node, "system-22");
        let plan = preview_move(&root, EnvType::Node, "system-22").unwrap();
        let lock = fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(external.join("version.txt"))
            .unwrap();

        assert!(execute_move(&root, &plan).is_err());
        drop(lock);

        assert!(
            crate::detect::system::is_reparse_path(&link),
            "失败后原纳管链接必须仍在"
        );
        assert_eq!(fs::read(external.join("version.txt")).unwrap(), b"1.0");
        assert!(staging_leftovers(&root, EnvType::Node).is_empty());
    }

    /// 搬移完成后删除外部旧目录：受管副本保持完整，外部路径消失。
    #[test]
    fn discard_removes_the_external_copy_only_after_a_verified_move() {
        let fixture = TestDir::new();
        let (root, external, link) = managed_external(&fixture, EnvType::Node, "system-22");
        let plan = preview_move(&root, EnvType::Node, "system-22").unwrap();
        execute_move(&root, &plan).unwrap();

        let discard = preview_discard(&root, EnvType::Node, "system-22", &external).unwrap();
        assert_eq!(discard.files, 2);
        execute_discard(&root, &discard).unwrap();

        // 外部旧目录消失，受管副本完好。
        assert!(!external.exists());
        assert_eq!(fs::read(link.join("version.txt")).unwrap(), b"1.0");
        assert_eq!(fs::read(link.join("bin/tool.exe")).unwrap(), b"payload");
    }

    /// 还没搬移（受管位置仍是链接）时不能删外部目录，否则等于删掉环境本身。
    #[test]
    fn discard_refuses_before_the_environment_was_actually_moved() {
        let fixture = TestDir::new();
        let (root, external, _link) = managed_external(&fixture, EnvType::Node, "system-22");
        let error = preview_discard(&root, EnvType::Node, "system-22", &external)
            .unwrap_err()
            .to_string();
        assert!(error.contains("尚未搬入受管目录"), "unexpected: {error}");
        assert!(external.join("version.txt").is_file());
    }

    /// 受管副本与外部旧目录结构不一致时拒绝删除——不能让用户丢掉未复制的文件。
    #[test]
    fn discard_refuses_when_the_managed_copy_does_not_cover_the_external_one() {
        let fixture = TestDir::new();
        let (root, external, _link) = managed_external(&fixture, EnvType::Node, "system-22");
        let plan = preview_move(&root, EnvType::Node, "system-22").unwrap();
        execute_move(&root, &plan).unwrap();
        // 搬移之后外部又多了内容：受管副本并不覆盖它。
        fs::write(external.join("added-after-move.txt"), b"late").unwrap();

        let error = preview_discard(&root, EnvType::Node, "system-22", &external)
            .unwrap_err()
            .to_string();
        assert!(error.contains("不完全一致"), "unexpected: {error}");
        assert!(external.join("added-after-move.txt").is_file());
    }

    /// 外部路径落在管理根目录内（或就是它）时拒绝，避免递归删掉整个受管区。
    #[test]
    fn discard_refuses_paths_inside_the_managed_root() {
        let fixture = TestDir::new();
        let (root, _external, _link) = managed_external(&fixture, EnvType::Node, "system-22");
        let plan = preview_move(&root, EnvType::Node, "system-22").unwrap();
        execute_move(&root, &plan).unwrap();

        // 管理根目录本身。
        let error = preview_discard(&root, EnvType::Node, "system-22", &root)
            .unwrap_err()
            .to_string();
        assert!(error.contains("管理根目录"), "unexpected: {error}");
        // 受管环境目录本身：那是副本，不能当旧目录删掉。
        let managed = root.join("envs").join(EnvType::Node.folder()).join("system-22");
        assert!(preview_discard(&root, EnvType::Node, "system-22", &managed).is_err());
        assert!(managed.join("version.txt").is_file());
    }

    /// 预览之后外部目录被改动，执行必须拒绝，且不删除任何东西。
    #[test]
    fn discard_execute_refuses_when_the_plan_no_longer_matches() {
        let fixture = TestDir::new();
        let (root, external, link) = managed_external(&fixture, EnvType::Node, "system-22");
        let move_plan = preview_move(&root, EnvType::Node, "system-22").unwrap();
        execute_move(&root, &move_plan).unwrap();
        let discard = preview_discard(&root, EnvType::Node, "system-22", &external).unwrap();
        // 预览后外部内容变化，字节数不再与计划一致。
        fs::write(external.join("version.txt"), b"changed").unwrap();

        assert!(execute_discard(&root, &discard).is_err());
        assert!(external.exists(), "拒绝执行时不能删除");
        assert_eq!(fs::read(link.join("version.txt")).unwrap(), b"1.0");
    }

    #[test]
    fn migrates_files_keeps_backup_and_old_command_paths() {
        let fixture = TestDir::new();
        let root = fixture.path().join("managed");
        let source = fixture.path().join("npm");
        fs::create_dir_all(source.join("node_modules/tool")).unwrap();
        fs::create_dir_all(&root).unwrap();
        fs::write(
            source.join("node_modules/tool/index.js"),
            "console.log('ok')",
        )
        .unwrap();
        fs::write(
            source.join("tool.cmd"),
            "@node %~dp0\\node_modules\\tool\\index.js",
        )
        .unwrap();
        junction::create(source.join("node_modules/tool"), source.join("reference")).unwrap();
        let plan = preview(&root, "npm", "global", &source, Capabilities::default()).unwrap();
        assert_eq!(plan.files, 2);
        assert_eq!(plan.links, 1);
        let result = execute(&root, &plan, Capabilities::default()).unwrap();
        assert_eq!(
            fs::read(source.join("tool.cmd")).unwrap(),
            fs::read(Path::new(&result.backup).join("tool.cmd")).unwrap()
        );
        assert!(source.join("reference/index.js").is_file());
        assert!(preview(&root, "npm", "global", &source, Capabilities::default()).is_err());
    }

    #[test]
    fn refuses_existing_targets_runtime_roots_and_changed_preview() {
        let fixture = TestDir::new();
        let root = fixture.path().join("managed");
        let source = fixture.path().join("source");
        fs::create_dir_all(&source).unwrap();
        fs::create_dir_all(&root).unwrap();
        let plan = preview(&root, "pip", "cache", &source, Capabilities::default()).unwrap();
        fs::write(source.join("new"), b"changed").unwrap();
        assert!(execute(&root, &plan, Capabilities::default()).is_err());
        fs::create_dir_all(root.join("globals/pip-cache")).unwrap();
        fs::write(root.join("globals/pip-cache/existing"), b"keep").unwrap();
        assert!(preview(&root, "pip", "cache", &source, Capabilities::default()).is_err());
        fs::write(source.join("node.exe"), b"runtime").unwrap();
        assert!(preview(&root, "npm", "global", &source, Capabilities::default()).is_err());
        assert!(
            preview(&root, "npm", "global", fixture.path(), Capabilities::default()).is_err()
        );
        assert!(source.join("new").is_file());
    }

    #[test]
    fn refuses_sources_inside_or_aliasing_the_managed_root() {
        let fixture = TestDir::new();
        let root = fixture.path().join("managed");
        fs::create_dir_all(root.join("nested/source")).unwrap();
        assert!(
            preview(
                &root,
                "npm",
                "global",
                &root.join("nested/source"),
                Capabilities::default()
            )
            .is_err()
        );

        let alias = fixture.path().join("managed-alias");
        junction::create(&root, &alias).unwrap();
        fs::create_dir_all(alias.join("source")).unwrap();
        assert!(
            preview(
                &root,
                "npm",
                "global",
                &alias.join("source"),
                Capabilities::default()
            )
            .is_err()
        );
    }

    #[test]
    fn rejects_tampered_target_and_detects_changed_contents() {
        let fixture = TestDir::new();
        let root = fixture.path().join("managed");
        let source = fixture.path().join("source");
        fs::create_dir_all(&source).unwrap();
        fs::create_dir_all(&root).unwrap();
        let mut plan = preview(&root, "npm", "cache", &source, Capabilities::default()).unwrap();
        plan.target = fixture.path().join("elsewhere").to_string_lossy().into();
        assert!(execute(&root, &plan, Capabilities::default()).is_err());
        fs::write(source.join("a"), b"aaa").unwrap();
        fs::write(source.join("b"), b"bbb").unwrap();
        assert!(verify_file(&source.join("a"), &source.join("b")).is_err());
    }

    #[test]
    fn supports_empty_target_created_by_configuration() {
        let fixture = TestDir::new();
        let root = fixture.path().join("managed");
        let source = fixture.path().join("cache");
        fs::create_dir_all(&source).unwrap();
        fs::create_dir_all(root.join("globals/pip-cache")).unwrap();
        fs::write(source.join("entry"), b"cache").unwrap();
        let plan = preview(&root, "pip", "cache", &source, Capabilities::default()).unwrap();
        let result = execute(&root, &plan, Capabilities::default()).unwrap();
        assert!(Path::new(&result.journal).is_file());
        assert_eq!(fs::read(source.join("entry")).unwrap(), b"cache");
    }

    #[test]
    fn locked_file_failure_preserves_source_and_reports_partial_target() {
        use std::os::windows::fs::OpenOptionsExt;
        let fixture = TestDir::new();
        let root = fixture.path().join("managed");
        let source = fixture.path().join("source");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("locked"), b"keep").unwrap();
        let plan = preview(&root, "npm", "cache", &source, Capabilities::default()).unwrap();
        let lock = fs::OpenOptions::new()
            .write(true)
            .share_mode(0)
            .open(source.join("locked"))
            .unwrap();
        assert!(execute(&root, &plan, Capabilities::default()).is_err());
        drop(lock);
        assert!(!junction::exists(&source).unwrap_or(false));
        assert_eq!(fs::read(source.join("locked")).unwrap(), b"keep");
    }

    #[test]
    fn refuses_to_migrate_a_legacy_uv_layout_as_is() {
        let fixture = TestDir::new();
        let root = fixture.path().join("managed");
        let source = fixture.path().join("uv-tools");
        fs::create_dir_all(source.join("bin")).unwrap();
        fs::create_dir_all(source.join("cache")).unwrap();
        fs::create_dir_all(&root).unwrap();
        assert!(preview(&root, "uv", "global", &source, Capabilities::default()).is_err());
        fs::remove_dir(source.join("bin")).unwrap();
        fs::remove_dir(source.join("cache")).unwrap();
        assert!(preview(&root, "uv", "global", &source, Capabilities::default()).is_ok());
    }

    #[test]
    fn yarn_berry_never_migrates_global_folder_as_a_classic_install_root() {
        let fixture = TestDir::new();
        let root = fixture.path().join("managed");
        let source = fixture.path().join("yarn-global");
        fs::create_dir_all(&source).unwrap();
        fs::create_dir_all(&root).unwrap();

        // Yarn Classic keeps the global install directory semantics.
        assert!(preview(&root, "yarn", "global", &source, Capabilities::default()).is_ok());
        // Berry uses globalFolder for global data, so the global-kind migration
        // must be refused before any directory is touched.
        let berry = Capabilities::for_tool("yarn", Some("4.1.0"));
        let error = preview(&root, "yarn", "global", &source, berry).unwrap_err();
        assert!(
            error.to_string().contains("globalFolder"),
            "unexpected error: {error}"
        );
        assert_eq!(fs::read_dir(&source).unwrap().count(), 0);
        // The cache kind stays available because cacheFolder is a real cache.
        assert!(preview(&root, "yarn", "cache", &source, berry).is_ok());
    }

    #[test]
    fn capabilities_recognize_classic_and_berry_versions() {
        assert!(!Capabilities::for_tool("yarn", Some("1.22.22")).yarn_berry);
        assert!(Capabilities::for_tool("yarn", Some("2.4.3")).yarn_berry);
        assert!(Capabilities::for_tool("yarn", Some("4.1.0")).yarn_berry);
        assert!(!Capabilities::for_tool("yarn", None).yarn_berry);
        assert!(!Capabilities::for_tool("npm", Some("11.0.0")).yarn_berry);
        assert!(!Capabilities::for_tool("yarn", Some("unknown")).yarn_berry);
    }
}
