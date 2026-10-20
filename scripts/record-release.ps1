[CmdletBinding()]
param(
    [string]$Output,
    [string[]]$ArtifactPath = @(),
    [string]$TestEvidence = "not-recorded-by-this-script"
)

$ErrorActionPreference = "Stop"
$root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path

function Read-JsonFile([string]$Path) {
    Get-Content -LiteralPath $Path -Raw | ConvertFrom-Json
}

function Get-Sha256([string]$Path) {
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { return $null }
    (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

function Get-CommandVersion([string]$Program, [string[]]$Arguments) {
    try {
        $value = & $Program @Arguments 2>$null
        if ($LASTEXITCODE -eq 0) { return ($value | Select-Object -First 1) }
    } catch {}
    return $null
}

$package = Read-JsonFile (Join-Path $root "package.json")
$tauri = Read-JsonFile (Join-Path $root "src-tauri/tauri.conf.json")
$cargoToml = Get-Content -LiteralPath (Join-Path $root "src-tauri/Cargo.toml") -Raw
$cargoVersion = [regex]::Match($cargoToml, '(?ms)^\[package\].*?^version\s*=\s*"([^"]+)"').Groups[1].Value

$versions = [ordered]@{
    packageJson = [string]$package.version
    tauriConfig = [string]$tauri.version
    cargoToml = $cargoVersion
}
$distinct = @($versions.Values | Sort-Object -Unique)
$versionConsistent = $distinct.Count -eq 1 -and -not [string]::IsNullOrWhiteSpace($distinct[0])
$version = if ($versionConsistent) { $distinct[0] } else { $distinct -join "/" }

$commit = (& git -C $root rev-parse HEAD 2>$null)
$branch = (& git -C $root rev-parse --abbrev-ref HEAD 2>$null)
$dirtyLines = @(& git -C $root status --porcelain 2>$null)
$dirty = $dirtyLines.Count -gt 0

$lockfiles = [ordered]@{
    packageLock = [ordered]@{
        path = "package-lock.json"
        sha256 = (Get-Sha256 (Join-Path $root "package-lock.json"))
    }
    cargoLock = [ordered]@{
        path = "src-tauri/Cargo.lock"
        sha256 = (Get-Sha256 (Join-Path $root "src-tauri/Cargo.lock"))
    }
}

$buildEnvironment = [ordered]@{
    os = [System.Environment]::OSVersion.VersionString
    architecture = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()
    node = (Get-CommandVersion "node" @("--version"))
    npm = (Get-CommandVersion "npm" @("--version"))
    rustc = (Get-CommandVersion "rustc" @("--version"))
    cargo = (Get-CommandVersion "cargo" @("--version"))
}

$artifacts = [System.Collections.Generic.List[object]]::new()
if ($ArtifactPath.Count -gt 0) {
    $candidates = $ArtifactPath | ForEach-Object {
        if ([IO.Path]::IsPathRooted($_)) { $_ } else { Join-Path $root $_ }
    }
} else {
    $bundleDirs = Get-ChildItem -LiteralPath (Join-Path $root "src-tauri") -Directory -Filter "target*" -ErrorAction SilentlyContinue |
        ForEach-Object { Join-Path $_.FullName "release/bundle" } |
        Where-Object { Test-Path -LiteralPath $_ }
    $candidates = @($bundleDirs | ForEach-Object {
        Get-ChildItem -LiteralPath $_ -Recurse -File -ErrorAction SilentlyContinue |
            Where-Object { $_.Extension -in @(".exe", ".msi") }
    })
}
foreach ($file in @($candidates)) {
    if (-not (Test-Path -LiteralPath $file -PathType Leaf)) {
        $artifacts.Add([ordered]@{ path = $file; status = "missing" })
        continue
    }
    $info = Get-Item -LiteralPath $file
    $relative = [IO.Path]::GetRelativePath($root, $info.FullName)
    $artifacts.Add([ordered]@{
        path = $relative
        status = "present-unverified-provenance"
        bytes = $info.Length
        lastWriteTimeUtc = $info.LastWriteTimeUtc.ToString("o")
        sha256 = Get-Sha256 $info.FullName
    })
}

$timestamp = (Get-Date).ToUniversalTime().ToString("yyyyMMdd-HHmmss")
if ([string]::IsNullOrWhiteSpace($Output)) {
    $Output = Join-Path $root "docs/release-record-$version-$timestamp.json"
} elseif (-not [IO.Path]::IsPathRooted($Output)) {
    $Output = Join-Path $root $Output
}
$outputDir = Split-Path -Parent $Output
if ($outputDir) { New-Item -ItemType Directory -Force -Path $outputDir | Out-Null }

$record = [ordered]@{
    schemaVersion = 1
    generatedAt = (Get-Date).ToUniversalTime().ToString("o")
    version = $version
    versionConsistent = $versionConsistent
    versionSources = $versions
    git = [ordered]@{
        commit = if ($commit) { [string]$commit } else { $null }
        branch = if ($branch) { [string]$branch } else { $null }
        dirty = $dirty
    }
    buildEnvironment = $buildEnvironment
    lockfiles = $lockfiles
    artifacts = @($artifacts)
    artifactDiscovery = "Scanned src-tauri/target*/release/bundle. Artifact presence does not prove it was built from the recorded commit."
    notes = "A dirty worktree or a pre-existing artifact cannot be treated as a release build. Rebuild from the recorded commit before publishing."
    status = [ordered]@{
        source = "recorded"
        tests = $TestEvidence
        package = if ($artifacts.Count -gt 0) { "artifact-present-unverified-provenance" } else { "not-built" }
        release = "not-published"
    }
    verification = [ordered]@{
        evidence = "ROADMAP.md (2026-10-19 R4 / REL-01 section)"
        status = $TestEvidence
    }
}

$record | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $Output -Encoding utf8
Write-Host "Release record: $Output"
if (-not $versionConsistent) {
    throw "Version mismatch: package.json=$($versions.packageJson), tauri.conf.json=$($versions.tauriConfig), Cargo.toml=$($versions.cargoToml)"
}
