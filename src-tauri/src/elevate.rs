//! 以管理员权限写入系统级环境变量。
//!
//! **产品保持单 exe**：同一个可执行文件带 `--env-write <file>` 启动时不开 GUI，
//! 只执行写入、写结果、退出。主程序把操作序列化到临时文件，用 `ShellExecuteW`
//! 以 `runas` 启动自身，等待子进程退出后读取结果。
//!
//! 这样日常使用（扫描、切换、用户级变量）完全不需要管理员权限，也不会每次启动
//! 都弹 UAC；只有真正写 HKLM 的那一刻才提权一次。

use std::ffi::OsStr;
use std::fs;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use windows_sys::Win32::Foundation::CloseHandle;
use windows_sys::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject, INFINITE};
use windows_sys::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};
use windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE;

use crate::error::{AppError, Result};
use crate::pathman::{EnvScope, EnvWriteOp};

/// 子进程识别用的命令行开关。
pub const SUBCOMMAND: &str = "--env-write";

/// 提权子进程的退出码：全部操作成功。
const EXIT_OK: u32 = 0;
/// 提权子进程的退出码：至少一个操作失败（原因写在结果文件里）。
const EXIT_FAILED: u32 = 1;

#[derive(Serialize, Deserialize)]
struct Outcome {
    error: Option<String>,
}

fn wide(value: &OsStr) -> Vec<u16> {
    value.encode_wide().chain(std::iter::once(0)).collect()
}

/// 结果文件的路径（与请求文件同目录）。
fn outcome_path(request: &Path) -> PathBuf {
    let mut name = request.file_name().unwrap_or_default().to_os_string();
    name.push(".result");
    request.with_file_name(name)
}

/// 子进程入口：若命令行是本模块的子命令则执行并返回退出码。
///
/// 返回 `None` 表示不是子命令，调用方应继续正常启动 GUI。
pub fn run_subcommand_if_requested() -> Option<i32> {
    let mut args = std::env::args_os().skip(1);
    let first = args.next()?;
    if first != OsStr::new(SUBCOMMAND) {
        return None;
    }
    let Some(request) = args.next().map(PathBuf::from) else {
        return Some(EXIT_FAILED as i32);
    };
    Some(execute_request(&request) as i32)
}

/// 在已提权的子进程里执行请求文件里的全部操作。
fn execute_request(request: &Path) -> u32 {
    let outcome = outcome_path(request);
    let result = (|| -> Result<()> {
        let bytes = fs::read(request)?;
        let ops: Vec<EnvWriteOp> = serde_json::from_slice(&bytes)
            .map_err(|error| AppError::msg(format!("提权请求格式无效: {error}")))?;
        // 子进程自己解析备份目录：便携模式下 data_dir 由 exe 位置推导，与主进程一致。
        let backups = crate::config::AppConfig::load().backups_dir();
        for op in ops {
            match op {
                EnvWriteOp::Set { name, value } => {
                    crate::pathman::set_env_var(EnvScope::System, &name, &value, &backups)?;
                }
                EnvWriteOp::Delete { name } => {
                    crate::pathman::delete_env_var(EnvScope::System, &name, &backups)?;
                }
                EnvWriteOp::Path { plan } => {
                    crate::pathman::apply_system_path_edit(&plan, &backups)?;
                }
            }
        }
        Ok(())
    })();

    let payload = Outcome {
        error: result.as_ref().err().map(|error| error.to_string()),
    };
    if let Ok(text) = serde_json::to_vec(&payload) {
        let _ = fs::write(&outcome, text);
    }
    match result {
        Ok(()) => EXIT_OK,
        Err(_) => EXIT_FAILED,
    }
}

/// 以管理员权限执行一批系统级写入。
///
/// 用户取消 UAC 时返回明确错误，不静默当作成功。
pub fn write_system_env(ops: &[EnvWriteOp]) -> Result<()> {
    if ops.is_empty() {
        return Ok(());
    }
    if crate::pathman::is_elevated() {
        // 已经是管理员（例如用户从提权终端启动），无需再起子进程。
        let backups = crate::config::AppConfig::load().backups_dir();
        for op in ops {
            match op {
                EnvWriteOp::Set { name, value } => {
                    crate::pathman::set_env_var(EnvScope::System, name, value, &backups)?;
                }
                EnvWriteOp::Delete { name } => {
                    crate::pathman::delete_env_var(EnvScope::System, name, &backups)?;
                }
                EnvWriteOp::Path { plan } => {
                    crate::pathman::apply_system_path_edit(plan, &backups)?;
                }
            }
        }
        return Ok(());
    }

    let request = write_request(ops)?;
    let outcome = outcome_path(&request);
    let exe = std::env::current_exe()
        .map_err(|error| AppError::msg(format!("无法定位程序自身路径: {error}")))?;
    let parameters = format!("{SUBCOMMAND} \"{}\"", request.display());

    let code = launch_elevated(&exe, &parameters)?;
    // 无论成败都清理请求文件；结果文件读完再删。
    let outcome_error = fs::read(&outcome)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Outcome>(&bytes).ok())
        .and_then(|value| value.error);
    let _ = fs::remove_file(&request);
    let _ = fs::remove_file(&outcome);

    if let Some(error) = outcome_error {
        return Err(AppError::msg(error));
    }
    if code != EXIT_OK {
        return Err(AppError::msg(format!(
            "提权写入失败（退出码 {code}），系统环境变量未修改"
        )));
    }
    Ok(())
}

/// 把操作写入一个仅本进程可读的临时请求文件。
fn write_request(ops: &[EnvWriteOp]) -> Result<PathBuf> {
    let dir = std::env::temp_dir();
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let path = dir.join(format!(
        "envcon-envwrite-{}-{stamp}.json",
        std::process::id()
    ));
    let bytes = serde_json::to_vec(ops)?;
    // create_new：不覆盖既有文件，避免与同名临时文件竞争。
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;
    use std::io::Write;
    file.write_all(&bytes)?;
    file.sync_all()?;
    Ok(path)
}

/// 用 `runas` 启动自身并等待退出，返回子进程退出码。
///
/// 必须用 `ShellExecuteExW`：`ShellExecuteW` 不填充 `SHELLEXECUTEINFOW`，拿不到
/// 进程句柄，就无法等待子进程结束、也无法确认它到底写成功没有。
fn launch_elevated(exe: &Path, parameters: &str) -> Result<u32> {
    let verb = wide(OsStr::new("runas"));
    let file = wide(exe.as_os_str());
    let params = wide(OsStr::new(parameters));
    let mut info: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
    info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
    info.fMask = SEE_MASK_NOCLOSEPROCESS;
    info.lpVerb = verb.as_ptr();
    info.lpFile = file.as_ptr();
    info.lpParameters = params.as_ptr();
    info.nShow = SW_HIDE;

    let ok = unsafe { ShellExecuteExW(&mut info) };
    if ok == 0 {
        // 取不到错误码时给出可执行的建议，而不是把 0 直接抛给用户。
        let error = std::io::Error::last_os_error();
        return Err(AppError::msg(match error.raw_os_error() {
            Some(1223) => "已取消管理员授权，系统环境变量未修改".to_string(),
            Some(5) => "提权被拒绝：请允许提权请求，或以管理员身份重新启动 EnvCon".to_string(),
            _ => format!("无法启动提权进程: {error}"),
        }));
    }
    // SEE_MASK_NOCLOSEPROCESS 时 hProcess 有效；为空说明启动方式不符合预期。
    let process = info.hProcess;
    if process.is_null() {
        return Err(AppError::msg("提权进程句柄无效，无法确认写入结果"));
    }
    unsafe {
        WaitForSingleObject(process, INFINITE);
        let mut exit_code: u32 = EXIT_FAILED;
        let read = GetExitCodeProcess(process, &mut exit_code);
        CloseHandle(process);
        if read == 0 {
            return Err(AppError::msg(
                "无法读取提权进程退出码，系统环境变量状态未知；请打开系统属性核对",
            ));
        }
        Ok(exit_code)
    }
}
