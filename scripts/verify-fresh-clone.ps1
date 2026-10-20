[CmdletBinding()]
param(
    [string]$Source = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path,
    [switch]$SkipInstall,
    [switch]$SkipTests,
    [switch]$KeepClone
)

$ErrorActionPreference = "Stop"

function Invoke-Checked {
    param([string]$Program, [string[]]$Arguments, [string]$WorkingDirectory)
    Push-Location $WorkingDirectory
    try {
        & $Program @Arguments
        if ($LASTEXITCODE -ne 0) {
            throw "$Program $($Arguments -join ' ') exited with $LASTEXITCODE"
        }
    } finally {
        Pop-Location
    }
}

$sourcePath = (Resolve-Path $Source).Path
$cloneRoot = Join-Path ([IO.Path]::GetTempPath()) ("envcon-fresh-" + [guid]::NewGuid().ToString("N"))
$required = @(
    "ROADMAP.md",
    "PRODUCT_ASSESSMENT.md",
    "README.md",
    "AGENTS.md",
    "package.json",
    "package-lock.json",
    "src-tauri/Cargo.toml",
    "src-tauri/Cargo.lock",
    "src-tauri/tests/download_test.rs",
    "src-tauri/tests/sources_test.rs",
    "src-tauri/tests/switcher_test.rs",
    "tests/verify.mjs",
    "tests/run-ui.mjs",
    "tests/scan-ui.mjs",
    "tests/package-manager-update-ui.mjs",
    "tests/packages-ui.mjs",
    ".github/workflows/windows.yml",
    "docs/README.md",
    "docs/guides/release-process.md",
    "docs/guides/testing.md",
    "scripts/record-release.ps1",
    "scripts/verify-fresh-clone.ps1"
)
$failures = [System.Collections.Generic.List[string]]::new()
$missing = [System.Collections.Generic.List[string]]::new()

try {
    Write-Host "Source: $sourcePath"
    Write-Host "Clone:  $cloneRoot"

    foreach ($relative in $required) {
        & git -C $sourcePath check-ignore -q -- $relative
        if ($LASTEXITCODE -eq 0) {
            $failures.Add("$relative is ignored by the source worktree .gitignore")
        } elseif ($LASTEXITCODE -ne 1) {
            $failures.Add("could not evaluate ignore rules for $relative (git exit $LASTEXITCODE)")
        }
    }

    Invoke-Checked -Program "git" -Arguments @("clone", "--no-hardlinks", $sourcePath, $cloneRoot) -WorkingDirectory $sourcePath

    foreach ($relative in $required) {
        $candidate = Join-Path $cloneRoot $relative
        if (-not (Test-Path -LiteralPath $candidate -PathType Leaf)) {
            $missing.Add($relative)
            & git -C $cloneRoot check-ignore -q -- $relative
            $ignored = $LASTEXITCODE -eq 0
            $why = if ($ignored) { "ignored by .gitignore" } else { "not present in the committed clone" }
            $failures.Add("$relative is missing ($why)")
        }
    }

    if ($missing.Count -gt 0) {
        Write-Host ""
        Write-Host "Missing committed files:"
        foreach ($item in $missing) { Write-Host "  - $item" }
        Write-Host "Commit or unignore these files before treating this tree as release-ready."
    }

    if (-not $SkipInstall -and $failures.Count -eq 0) {
        Invoke-Checked -Program "npm" -Arguments @("ci") -WorkingDirectory $cloneRoot
    }
    if (-not $SkipTests -and $failures.Count -eq 0) {
        Invoke-Checked -Program "npm" -Arguments @("run", "build") -WorkingDirectory $cloneRoot
        Invoke-Checked -Program "cargo" -Arguments @("test", "--lib", "--offline") -WorkingDirectory (Join-Path $cloneRoot "src-tauri")
    }

    $result = [ordered]@{
        source = $sourcePath
        clone = $cloneRoot
        requiredFiles = $required.Count
        missingFiles = @($missing)
        skippedInstall = [bool]$SkipInstall
        skippedTests = [bool]$SkipTests
        passed = ($failures.Count -eq 0)
        failures = @($failures)
    }
    $result | ConvertTo-Json -Depth 5
    if ($failures.Count -gt 0) {
        throw "Fresh-clone verification failed with $($failures.Count) finding(s)."
    }
    Write-Host "PASS: fresh clone contains required files, installs, builds and passes library tests."
} finally {
    if (-not $KeepClone -and (Test-Path -LiteralPath $cloneRoot)) {
        $resolved = (Resolve-Path $cloneRoot).Path
        $tempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
        if ($resolved.StartsWith($tempRoot, [StringComparison]::OrdinalIgnoreCase)) {
            Remove-Item -LiteralPath $resolved -Recurse -Force
        } else {
            Write-Warning "Refusing to remove clone outside the temporary directory: $resolved"
        }
    }
}
