use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::process::{Output, Stdio};
use std::time::Duration;
use tokio::process::Command;
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
    SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};
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

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn timeout_terminates_descendant_processes() {
        use windows_sys::Win32::System::Threading::{OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE};
        let dir = crate::test_support::TestDir::new();
        let pid_path = dir.path().join("child.pid");
        let launcher = dir.path().join("launcher.cmd");
        std::fs::write(&launcher, "@powershell.exe -NoProfile -Command \"[IO.File]::WriteAllText('%~dp0child.pid', [string]$PID); Start-Sleep -Seconds 30\"").unwrap();
        let mut cmd = Command::new(launcher);
        cmd.creation_flags(0x08000000);
        let result = output(cmd, Duration::from_secs(3)).await;
        assert!(result.is_err(), "unexpected early exit: {result:?}");
        let pid: u32 = std::fs::read_to_string(pid_path).unwrap().parse().unwrap();
        let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
        if !handle.is_null() {
            let process = unsafe { OwnedHandle::from_raw_handle(handle) };
            assert_eq!(unsafe { WaitForSingleObject(process.as_raw_handle(), 5000) }, 0);
        }
    }
}
