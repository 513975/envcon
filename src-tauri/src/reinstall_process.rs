use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::os::windows::process::CommandExt;
use std::process::{Output, Stdio};
use std::time::Duration;
use tokio::process::Command;
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
    SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};
use windows_sys::Win32::Foundation::{WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows_sys::Win32::System::Threading::WaitForSingleObject;
use crate::error::{AppError, Result};

// Closing this job also terminates Node/Python and installer descendants of a .cmd launcher.
pub async fn output(mut command: Command, timeout: Duration) -> Result<Output> {
    let job = unsafe {
        let raw = CreateJobObjectW(std::ptr::null(), std::ptr::null());
        if raw.is_null() { return Err(std::io::Error::last_os_error().into()); }
        OwnedHandle::from_raw_handle(raw)
    };
    let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    let configured = unsafe { SetInformationJobObject(job.as_raw_handle(), JobObjectExtendedLimitInformation,
        &limits as *const _ as *const _, std::mem::size_of_val(&limits) as u32) };
    if configured == 0 { return Err(std::io::Error::last_os_error().into()); }
    let mut child = command.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true).spawn()?;
    let assigned = unsafe { AssignProcessToJobObject(job.as_raw_handle(), child.raw_handle().unwrap()) };
    if assigned == 0 {
        let error = std::io::Error::last_os_error();
        let _ = child.kill().await;
        return Err(AppError::msg(format!("无法隔离安装进程: {error}")));
    }
    let result = tokio::time::timeout(timeout, child.wait_with_output()).await;
    drop(job);
    match result {
        Ok(output) => Ok(output?),
        Err(_) => Err(AppError::msg("命令超时，已终止安装进程树；新目录保留供检查")),
    }
}

/// Blocking variant for callers that already run on a worker thread.
///
/// The process is assigned to a kill-on-close Job Object, so a timeout also
/// terminates installer descendants rather than leaving them behind.
pub fn output_blocking(mut command: std::process::Command, timeout: Duration) -> Result<Output> {
    let job = unsafe {
        let raw = CreateJobObjectW(std::ptr::null(), std::ptr::null());
        if raw.is_null() { return Err(std::io::Error::last_os_error().into()); }
        OwnedHandle::from_raw_handle(raw)
    };
    let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    let configured = unsafe { SetInformationJobObject(job.as_raw_handle(), JobObjectExtendedLimitInformation,
        &limits as *const _ as *const _, std::mem::size_of_val(&limits) as u32) };
    if configured == 0 { return Err(std::io::Error::last_os_error().into()); }
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(0x08000000)
        .spawn()?;
    let assigned = unsafe { AssignProcessToJobObject(job.as_raw_handle(), child.as_raw_handle()) };
    if assigned == 0 {
        let error = std::io::Error::last_os_error();
        let _ = child.kill();
        let _ = child.wait();
        return Err(AppError::msg(format!("无法隔离卸载进程: {error}")));
    }
    let wait_ms = timeout.as_millis().min(u32::MAX as u128) as u32;
    match unsafe { WaitForSingleObject(child.as_raw_handle(), wait_ms) } {
        WAIT_OBJECT_0 => {}
        WAIT_TIMEOUT => {
            drop(job);
            let _ = child.kill();
            let _ = child.wait();
            return Err(AppError::msg("卸载命令超时，已终止卸载进程树；残留内容保留供检查"));
        }
        _ => {
            let error = std::io::Error::last_os_error();
            drop(job);
            let _ = child.kill();
            let _ = child.wait();
            return Err(AppError::msg(format!("等待卸载命令失败: {error}")));
        }
    }
    // Close the job after the root process exits so any descendants holding
    // stdout/stderr pipes are terminated before wait_with_output drains them.
    drop(job);
    Ok(child.wait_with_output()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn timeout_terminates_descendant_processes() {
        use windows_sys::Win32::System::Threading::{OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE};
        let dir = crate::test_support::TestDir::new();
        let pid_path = dir.path().join("child.pid");
        let executable = std::env::current_exe().unwrap();
        let mut cmd = Command::new(executable);
        cmd.args([
            "--ignored",
            "--exact",
            "reinstall_process::tests::timeout_process_tree_child_helper",
        ])
        .env("ENVCON_CHILD_PID_FILE", &pid_path);
        cmd.creation_flags(0x08000000);
        let result = output(cmd, Duration::from_secs(3)).await;
        let error = result.expect_err("child process must time out").to_string();
        assert!(error.contains("命令超时"), "unexpected process error: {error}");
        let pid: u32 = std::fs::read_to_string(&pid_path)
            .unwrap_or_else(|e| panic!("child PID fixture was not written: {e}; {error}"))
            .parse()
            .unwrap();
        let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
        if !handle.is_null() {
            let process = unsafe { OwnedHandle::from_raw_handle(handle) };
            assert_eq!(unsafe { WaitForSingleObject(process.as_raw_handle(), 5000) }, 0);
        }
    }

    #[test]
    fn blocking_timeout_terminates_descendant_processes() {
        use windows_sys::Win32::System::Threading::{OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE};
        let dir = crate::test_support::TestDir::new();
        let pid_path = dir.path().join("child.pid");
        let executable = std::env::current_exe().unwrap();
        let mut cmd = std::process::Command::new(executable);
        cmd.args([
            "--ignored",
            "--exact",
            "reinstall_process::tests::timeout_process_tree_child_helper",
        ])
        .env("ENVCON_CHILD_PID_FILE", &pid_path);
        let result = output_blocking(cmd, Duration::from_secs(3));
        let error = result.expect_err("child process must time out").to_string();
        assert!(error.contains("卸载命令超时"), "unexpected process error: {error}");
        let pid: u32 = std::fs::read_to_string(&pid_path)
            .unwrap_or_else(|e| panic!("child PID fixture was not written: {e}; {error}"))
            .parse()
            .unwrap();
        let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
        if !handle.is_null() {
            let process = unsafe { OwnedHandle::from_raw_handle(handle) };
            assert_eq!(unsafe { WaitForSingleObject(process.as_raw_handle(), 5000) }, 0);
        }
    }

    #[test]
    #[ignore = "started by timeout_terminates_descendant_processes as a child fixture"]
    fn timeout_process_tree_child_helper() {
        let executable = std::env::current_exe().unwrap();
        let path = std::env::var_os("ENVCON_CHILD_PID_FILE").expect("parent supplies PID output path");
        let mut child = std::process::Command::new(executable)
            .args([
                "--ignored",
                "--exact",
                "reinstall_process::tests::timeout_process_tree_grandchild_helper",
            ])
            .env("ENVCON_CHILD_PID_FILE", &path)
            .spawn()
            .unwrap();
        while !std::path::Path::new(&path).exists() {
            std::thread::sleep(Duration::from_millis(10));
        }
        let _ = child.wait();
    }

    #[test]
    #[ignore = "started by timeout_process_tree_child_helper as a grandchild fixture"]
    fn timeout_process_tree_grandchild_helper() {
        let path = std::env::var_os("ENVCON_CHILD_PID_FILE").expect("parent supplies PID output path");
        std::fs::write(path, std::process::id().to_string()).unwrap();
        std::thread::sleep(Duration::from_secs(30));
    }
}
