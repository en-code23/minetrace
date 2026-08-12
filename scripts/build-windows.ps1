[CmdletBinding()]
param(
    [ValidateSet("Local", "Release")]
    [string]$Mode = "Local",
    [ValidateSet("X64", "Arm64")]
    [string]$Architecture = "X64",
    [switch]$SkipChecks,
    [switch]$SkipTargetTests,
    [switch]$SmokeInstall,
    [switch]$UpgradeSmokeInstall
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

if ([System.Environment]::OSVersion.Platform -ne [System.PlatformID]::Win32NT) {
    throw "MineTrace Windows installers must be built on Windows."
}

$repoRoot = Split-Path -Parent $PSScriptRoot
$tauriRoot = Join-Path $repoRoot "src-tauri"
$tauriConfig = Get-Content -LiteralPath (Join-Path $tauriRoot "tauri.conf.json") -Raw | ConvertFrom-Json
$expectedVersion = [string]$tauriConfig.version
if ([string]::IsNullOrWhiteSpace($expectedVersion)) {
    throw "src-tauri/tauri.conf.json does not define an application version."
}
$targetTriple = if ($Architecture -eq "Arm64") {
    "aarch64-pc-windows-msvc"
}
else {
    "x86_64-pc-windows-msvc"
}
$architectureLabel = if ($Architecture -eq "Arm64") { "ARM64" } else { "x64" }
$bundleRoot = Join-Path $tauriRoot "target\$targetTriple\release\bundle"
$verifyScript = Join-Path $PSScriptRoot "verify-windows.ps1"
$hostArchitecture = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()

if (-not $SkipChecks -and -not $SkipTargetTests -and $hostArchitecture -ne $Architecture) {
    throw "$architectureLabel target tests require a matching Windows host. Use -SkipTargetTests only for a cross-build; native CI must still run the tests."
}
if (($SmokeInstall -or $UpgradeSmokeInstall) -and $hostArchitecture -ne $Architecture) {
    throw "$architectureLabel install/launch verification requires a matching Windows host; this host is $hostArchitecture."
}

function Invoke-NativeStep {
    param(
        [Parameter(Mandatory = $true)][string]$Label,
        [Parameter(Mandatory = $true)][scriptblock]$Command
    )

    Write-Host "`n==> $Label" -ForegroundColor Cyan
    & $Command
    if ($LASTEXITCODE -ne 0) {
        throw "$Label failed with exit code $LASTEXITCODE."
    }
}

Push-Location $repoRoot
try {
    foreach ($command in @("node", "pnpm", "cargo", "rustc")) {
        if (-not (Get-Command $command -ErrorAction SilentlyContinue)) {
            throw "Required command '$command' is not available."
        }
    }

    if (-not $SkipChecks) {
        Invoke-NativeStep "Frontend lint" { pnpm lint }
        Invoke-NativeStep "Frontend typecheck" { pnpm typecheck }
        Invoke-NativeStep "Frontend tests" { pnpm test }
        Invoke-NativeStep "Rust formatting" { cargo fmt --manifest-path "$tauriRoot\Cargo.toml" --all -- --check }
        Invoke-NativeStep "$architectureLabel Windows Rust check" { cargo check --manifest-path "$tauriRoot\Cargo.toml" --all-targets --target $targetTriple }
        if (-not $SkipTargetTests) {
            Invoke-NativeStep "$architectureLabel Windows Rust tests" { cargo test --manifest-path "$tauriRoot\Cargo.toml" --all-targets --target $targetTriple }
        }
        Invoke-NativeStep "$architectureLabel Windows Rust clippy" { cargo clippy --manifest-path "$tauriRoot\Cargo.toml" --all-targets --all-features --target $targetTriple -- -D warnings }
    }

    $buildArgs = @(
        "tauri", "build",
        "--config", "src-tauri/tauri.windows.conf.json",
        "--target", $targetTriple,
        "--bundles", "nsis,msi",
        "--ci"
    )

    $requireSigned = $Mode -eq "Release"
    if ($requireSigned) {
        $thumbprint = $env:MINETRACE_WINDOWS_CERTIFICATE_THUMBPRINT
        $timestampUrl = $env:MINETRACE_WINDOWS_TIMESTAMP_URL
        if ([string]::IsNullOrWhiteSpace($thumbprint)) {
            throw "Release mode requires MINETRACE_WINDOWS_CERTIFICATE_THUMBPRINT."
        }
        if ([string]::IsNullOrWhiteSpace($timestampUrl)) {
            throw "Release mode requires MINETRACE_WINDOWS_TIMESTAMP_URL."
        }

        $signingOverlay = @{
            bundle = @{
                windows = @{
                    certificateThumbprint = $thumbprint
                    digestAlgorithm = "sha256"
                    timestampUrl = $timestampUrl
                    tsp = $true
                }
            }
        } | ConvertTo-Json -Depth 5 -Compress
        $buildArgs += @("--config", $signingOverlay)
    }
    else {
        $buildArgs += "--no-sign"
    }

    Invoke-NativeStep "Build MineTrace Windows $architectureLabel installers" { pnpm @buildArgs }

    $verifyArgs = @{
        BundleRoot = $bundleRoot
        ExpectedVersion = $expectedVersion
        Architecture = $Architecture
        RequireSigned = $requireSigned
        SmokeInstall = [bool]$SmokeInstall
        UpgradeSmokeInstall = [bool]$UpgradeSmokeInstall
    }
    & $verifyScript @verifyArgs
}
finally {
    Pop-Location
}
