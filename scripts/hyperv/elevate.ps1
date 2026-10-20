#Requires -Version 5.1
<#
.SYNOPSIS
  Run a script block or command with an elevated token and bring the output back.

.DESCRIPTION
  Why this exists: the agent's shell runs with a filtered (Medium integrity)
  token, so Hyper-V cmdlets such as Get-VM fail with "you do not have the
  permission required". Elevating used to mean a UAC prompt for every call.

  With ConsentPromptBehaviorAdmin = 0 the elevation is silent, but the caller
  still has to spawn an elevated child process, and the child's output has to
  travel back through a file because Start-Process cannot redirect to the
  parent's console. This wraps that so callers write:

      scripts\hyperv\elevate.ps1 -Command "Get-VM | Format-Table"

  instead of hand-rolling base64 + EncodedCommand + temp-file plumbing each
  time. Getting that plumbing wrong is exactly what produced several silent
  failures with no log earlier.

  Deliberate choices:
  - The child writes stdout and stderr to temp files that the parent reads and
    deletes, so a crash still surfaces something.
  - A hard timeout: an elevation that unexpectedly waits (for example after the
    silent setting has been reverted) must not hang the caller forever.
  - -AsCurrentUser runs the command in-process. Some tasks (creating files under
    the caller's profile, reading the caller's environment) behave differently
    under elevation, and forcing elevation on those would be wrong.
#>
[CmdletBinding(DefaultParameterSetName = "Elevated")]
param(
    [Parameter(Mandatory = $true, ParameterSetName = "Elevated")]
    [Parameter(Mandatory = $true, ParameterSetName = "Current")]
    [string]$Command,

    [Parameter(ParameterSetName = "Current")]
    [switch]$AsCurrentUser,

    [Parameter(ParameterSetName = "Elevated")]
    [int]$TimeoutSeconds = 300,

    # Keep the temp transcript after the run, for post-mortem of a failure.
    [switch]$KeepLog
)

$ErrorActionPreference = "Stop"

if ($AsCurrentUser) {
    # Straight execution: used for anything that must see the caller's profile.
    $global:LASTEXITCODE = 0
    & ([scriptblock]::Create($Command))
    exit $LASTEXITCODE
}

$stamp = [guid]::NewGuid().ToString("N")
$stdout = Join-Path $env:TEMP "elevate-$stamp.out"
$stderr = Join-Path $env:TEMP "elevate-$stamp.err"

# The child redirects its own streams and records its own exit code.
#
# This cannot be done with Start-Process -RedirectStandardOutput: that parameter
# set is mutually exclusive with -Verb RunAs, and using both fails with "the
# parameter set cannot be resolved". The child therefore receives the output
# paths and writes them itself, which also survives the case where the child
# crashes after producing partial output.
$wrapped = @"
`$ErrorActionPreference = 'Continue'
try {
    `$result = & {
$Command
    } 2>&1 | Out-String
    `$result | Set-Content -LiteralPath '$stdout' -Encoding UTF8
    exit 0
} catch {
    (`$_.Exception.Message + [Environment]::NewLine + `$_.ScriptStackTrace) |
        Set-Content -LiteralPath '$stderr' -Encoding UTF8
    exit 1
}
"@

$encoded = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($wrapped))

$arguments = @(
    "-NoProfile",
    "-ExecutionPolicy", "Bypass",
    "-EncodedCommand", $encoded
)

$process = Start-Process -FilePath "$env:SystemRoot\System32\WindowsPowerShell\v1.0\powershell.exe" `
    -ArgumentList $arguments `
    -Verb RunAs `
    -WindowStyle Hidden `
    -PassThru

$timedOut = -not $process.WaitForExit($TimeoutSeconds * 1000)
if ($timedOut) {
    try { $process.Kill() } catch { }
}

$outText = if (Test-Path -LiteralPath $stdout) { Get-Content -LiteralPath $stdout -Raw } else { "" }
$errText = if (Test-Path -LiteralPath $stderr) { Get-Content -LiteralPath $stderr -Raw } else { "" }

if (-not $KeepLog) {
    Remove-Item -LiteralPath $stdout, $stderr -Force -ErrorAction SilentlyContinue
} else {
    # Report only files that actually exist. The stderr file is created solely on
    # failure, so naming it unconditionally would send the caller looking for a
    # file that was never written.
    $kept = @($stdout, $stderr) | Where-Object { Test-Path -LiteralPath $_ }
    if ($kept) {
        Write-Host ("log kept: " + ($kept -join " ; ")) -ForegroundColor DarkGray
    } else {
        Write-Host "log kept: (the child produced no output files)" -ForegroundColor DarkGray
    }
}

if ($timedOut) {
    Write-Host "ELEVATE TIMEOUT after ${TimeoutSeconds}s. Partial output:" -ForegroundColor Red
    if ($outText) { Write-Host $outText }
    if ($errText) { Write-Host $errText -ForegroundColor Red }
    exit 124
}

if ($outText) { Write-Host $outText.TrimEnd() }
if ($errText.Trim()) { Write-Host $errText.TrimEnd() -ForegroundColor Red }

exit $process.ExitCode
