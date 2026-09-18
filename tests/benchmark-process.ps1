param([Parameter(Mandatory=$true)][string]$Exe)
$ErrorActionPreference = 'Stop'
# A private kill-on-close job includes WebView and command descendants, including short-lived ones.
# Only the copied benchmark executable is launched; no existing user process is attached.
Add-Type -TypeDefinition @'
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class BenchJob {
  [StructLayout(LayoutKind.Sequential)] public struct IO {
    public ulong ReadOperations, WriteOperations, OtherOperations, ReadBytes, WriteBytes, OtherBytes;
  }
  [StructLayout(LayoutKind.Sequential)] public struct BasicLimits {
    public long ProcessTime, JobTime; public uint Flags;
    public UIntPtr MinWorking, MaxWorking; public uint ActiveLimit; public UIntPtr Affinity;
    public uint Priority, Scheduling;
  }
  [StructLayout(LayoutKind.Sequential)] public struct Limits {
    public BasicLimits Basic; public IO Io;
    public UIntPtr ProcessMemory, JobMemory, PeakProcessMemory, PeakJobMemory;
  }
  [StructLayout(LayoutKind.Sequential)] public struct Accounting {
    public long UserTime, KernelTime, PeriodUserTime, PeriodKernelTime;
    public uint PageFaults, TotalProcesses, ActiveProcesses, TerminatedProcesses;
    public IO Io;
  }
  [StructLayout(LayoutKind.Sequential, CharSet=CharSet.Unicode)] public struct Startup {
    public int Size; public string Reserved, Desktop, Title;
    public uint X,Y,XSize,YSize,XChars,YChars,Fill,Flags;
    public ushort Show, ReservedSize; public IntPtr ReservedPtr, Input, Output, Error;
  }
  [StructLayout(LayoutKind.Sequential)] public struct ProcessInfo {
    public IntPtr Process, Thread; public uint Pid, Tid;
  }
  [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)] static extern IntPtr CreateJobObject(IntPtr attrs, string name);
  [DllImport("kernel32.dll", SetLastError=true)] static extern bool SetInformationJobObject(IntPtr job, int kind, ref Limits value, int length);
  [DllImport("kernel32.dll", SetLastError=true)] static extern bool QueryInformationJobObject(IntPtr job, int kind, out Accounting value, int length, IntPtr returned);
  [DllImport("kernel32.dll", EntryPoint="QueryInformationJobObject", SetLastError=true)] static extern bool QueryLimits(IntPtr job, int kind, out Limits value, int length, IntPtr returned);
  [DllImport("kernel32.dll", SetLastError=true)] static extern bool AssignProcessToJobObject(IntPtr job, IntPtr process);
  [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)] static extern bool CreateProcess(string exe, StringBuilder args, IntPtr pa, IntPtr ta, bool inherit, uint flags, IntPtr env, string cwd, ref Startup si, out ProcessInfo pi);
  [DllImport("kernel32.dll")] static extern uint ResumeThread(IntPtr thread);
  [DllImport("kernel32.dll")] static extern bool TerminateProcess(IntPtr process, uint code);
  [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr handle);
  static IntPtr job;
  public static uint Launch(string exe) {
    job = CreateJobObject(IntPtr.Zero, null);
    if (job == IntPtr.Zero) throw new System.ComponentModel.Win32Exception();
    var limits = new Limits(); limits.Basic.Flags = 0x2000;
    if (!SetInformationJobObject(job, 9, ref limits, Marshal.SizeOf(limits))) throw new System.ComponentModel.Win32Exception();
    var si = new Startup(); si.Size = Marshal.SizeOf(si); si.Flags = 1; si.Show = 0;
    ProcessInfo pi;
    if (!CreateProcess(exe, new StringBuilder("\"" + exe + "\""), IntPtr.Zero, IntPtr.Zero, false, 0x08000004, IntPtr.Zero, System.IO.Path.GetDirectoryName(exe), ref si, out pi)) throw new System.ComponentModel.Win32Exception();
    try {
      if (!AssignProcessToJobObject(job, pi.Process)) { TerminateProcess(pi.Process, 1); throw new System.ComponentModel.Win32Exception(); }
      if (ResumeThread(pi.Thread) == uint.MaxValue) { TerminateProcess(pi.Process, 1); throw new System.ComponentModel.Win32Exception(); }
      return pi.Pid;
    } finally { CloseHandle(pi.Thread); CloseHandle(pi.Process); }
  }
  public static Accounting Counters() {
    Accounting value;
    if (!QueryInformationJobObject(job, 8, out value, Marshal.SizeOf(typeof(Accounting)), IntPtr.Zero)) throw new System.ComponentModel.Win32Exception();
    return value;
  }
  public static ulong PeakMemory() {
    Limits value;
    if (!QueryLimits(job, 9, out value, Marshal.SizeOf(typeof(Limits)), IntPtr.Zero)) throw new System.ComponentModel.Win32Exception();
    return value.PeakJobMemory.ToUInt64();
  }
  public static void Stop() { if (job != IntPtr.Zero) { CloseHandle(job); job = IntPtr.Zero; } }
}
'@
try {
  $launchTime = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
  $appPid = [BenchJob]::Launch($Exe)
  @{type='ready'; pid=$appPid; launchTime=$launchTime} | ConvertTo-Json -Compress
  while ($true) {
    $request = [Console]::ReadLine()
    if ($null -eq $request -or $request -eq 'stop') { break }
    $sample = [BenchJob]::Counters()
    @{type='sample'; elapsedMs=([DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()-$launchTime); processes=$sample.TotalProcesses; activeProcesses=$sample.ActiveProcesses; peakCommitBytes=[BenchJob]::PeakMemory(); readBytes=$sample.Io.ReadBytes; writeBytes=$sample.Io.WriteBytes; readOperations=$sample.Io.ReadOperations; writeOperations=$sample.Io.WriteOperations; cpuMs=(($sample.UserTime+$sample.KernelTime)/10000)} | ConvertTo-Json -Compress
  }
} finally { [BenchJob]::Stop() }
