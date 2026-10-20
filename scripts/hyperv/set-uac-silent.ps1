#Requires -RunAsAdministrator
<#
.SYNOPSIS
  Set UAC to elevate administrators without prompting, and record the previous
  values so the change can be reverted exactly.

.DESCRIPTION
  Why this exists: Hyper-V cmdlets (Get-VM, Start-VM, New-VM, ...) require an
  elevated token. With ConsentPromptBehaviorAdmin=5 every call raised a UAC
  consent dialog, which made an automated verification run impractical.

  What it changes, and what it does not:
  - ConsentPromptBehaviorAdmin = 0  -> "Elevate without prompting" for members
    of the Administrators group. No dialog is shown; the elevation happens.
  - EnableLUA stays at 1. UAC itself remains enabled, so standard users are
    still separated from administrators; only the consent prompt for admins goes
    away.
  - PromptOnSecureDesktop is left alone because the consent prompt it controls
    no longer appears for administrators.

  Security consequence, stated plainly: any process that asks for elevation now
  receives it silently. A malicious binary that would previously have shown a
  dialog the user could refuse will instead run elevated. This is a real
  reduction in defense for as long as the setting is in place.

  The previous values are written to a JSON file next to the log so
  -Restore can put them back without guesswork.
#>
[CmdletBinding()]
param(
    [switch]$Restore,
    [string]$StateFile = (Join-Path $env:ProgramData "EnvConVerify\uac-state.json")
)

$ErrorActionPreference = "Stop"

function Out-Line([string]$text) { Write-Host $text }

Out-Line "=== UAC state ==="
$policyPath = "HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\System"
$names = @("EnableLUA", "ConsentPromptBehaviorAdmin", "ConsentPromptBehaviorUser", "PromptOnSecureDesktop")

$current = [ordered]@{}
foreach ($name in $names) {
    $value = (Get-ItemProperty -Path $policyPath -Name $name -ErrorAction SilentlyContinue).$name
    $current[$name] = $value
    Out-Line ("  {0} = {1}" -f $name, $value)
}

if ($Restore) {
    if (-not (Test-Path -LiteralPath $StateFile)) {
        throw "No saved state at $StateFile; cannot restore. The tool refuses to guess original values."
    }
    $saved = Get-Content -LiteralPath $StateFile -Raw | ConvertFrom-Json
    Out-Line ""
    Out-Line "=== Restoring from $StateFile (captured $($saved.capturedAt)) ==="
    foreach ($name in $names) {
        $target = $saved.values.$name
        if ($null -eq $target) { continue }
        Set-ItemProperty -Path $policyPath -Name $name -Value ([int]$target) -Type DWord
        $readBack = (Get-ItemProperty -Path $policyPath -Name $name).$name
        Out-Line ("  {0}: restored {1} (now {2})" -f $name, $target, $readBack)
    }
    Out-Line ""
    Out-Line "UAC restored. Sign out and back in for the token policy to apply everywhere."
    exit 0
}

# Save before changing anything, so an interrupted run still leaves a record.
$stateDir = Split-Path -Parent $StateFile
New-Item -ItemType Directory -Path $stateDir -Force | Out-Null
[pscustomobject]@{
    capturedAt = (Get-Date).ToString("o")
    computer   = $env:COMPUTERNAME
    changedBy  = "scripts/hyperv/set-uac-silent.ps1"
    values     = $current
} | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath $StateFile -Encoding UTF8
Out-Line ""
Out-Line "Previous values saved to $StateFile"

Out-Line ""
Out-Line "=== Applying ConsentPromptBehaviorAdmin = 0 ==="
Set-ItemProperty -Path $policyPath -Name "ConsentPromptBehaviorAdmin" -Value 0 -Type DWord
$after = (Get-ItemProperty -Path $policyPath -Name "ConsentPromptBehaviorAdmin").ConsentPromptBehaviorAdmin
Out-Line ("  ConsentPromptBehaviorAdmin = $after (0 means elevate without prompting)")

Out-Line ""
Out-Line "=== Verify ==="
$lua = (Get-ItemProperty -Path $policyPath -Name "EnableLUA").EnableLUA
Out-Line ("  EnableLUA = $lua (UAC stays enabled)")
if ($after -ne 0) { throw "Setting did not take effect." }

Out-Line ""
Out-Line "Done. New elevations from an Administrators account no longer prompt."
Out-Line "To revert: run this script with -Restore"
