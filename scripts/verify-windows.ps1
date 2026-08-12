[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$BundleRoot,
    [Parameter(Mandatory = $true)]
    [string]$ExpectedVersion,
    [ValidateSet("X64", "Arm64")]
    [string]$Architecture = "X64",
    [bool]$RequireSigned = $false,
    [bool]$SmokeInstall = $false
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

if ([System.Environment]::OSVersion.Platform -ne [System.PlatformID]::Win32NT) {
    throw "Windows bundle verification must run on Windows."
}

$resolvedBundleRoot = (Resolve-Path -LiteralPath $BundleRoot).Path
$releaseRoot = Split-Path -Parent $resolvedBundleRoot
$application = Join-Path $releaseRoot "minetrace.exe"
$expectedMachine = if ($Architecture -eq "Arm64") { 0xAA64 } else { 0x8664 }
$architectureLabel = if ($Architecture -eq "Arm64") { "ARM64" } else { "x86-64" }
$artifactTag = if ($Architecture -eq "Arm64") { "arm64" } else { "x64" }
$nsisInstallers = @(Get-ChildItem -LiteralPath (Join-Path $resolvedBundleRoot "nsis") -Filter "*.exe" -File)
$msiInstallers = @(Get-ChildItem -LiteralPath (Join-Path $resolvedBundleRoot "msi") -Filter "*.msi" -File)

if (-not (Test-Path -LiteralPath $application -PathType Leaf)) {
    throw "The packaged application executable is missing: $application"
}
if ($nsisInstallers.Count -ne 1) {
    throw "Expected exactly one NSIS installer, found $($nsisInstallers.Count)."
}
if ($msiInstallers.Count -ne 1) {
    throw "Expected exactly one MSI installer, found $($msiInstallers.Count)."
}

function Assert-TargetPortableExecutable {
    param([Parameter(Mandatory = $true)][string]$Path)

    $stream = [System.IO.File]::OpenRead($Path)
    try {
        $reader = [System.IO.BinaryReader]::new($stream)
        if ($reader.ReadUInt16() -ne 0x5A4D) {
            throw "$Path is not a PE executable."
        }
        $stream.Position = 0x3C
        $peOffset = $reader.ReadUInt32()
        $stream.Position = $peOffset
        if ($reader.ReadUInt32() -ne 0x00004550) {
            throw "$Path has an invalid PE header."
        }
        if ($reader.ReadUInt16() -ne $expectedMachine) {
            throw "$Path is not a $architectureLabel executable."
        }
    }
    finally {
        $stream.Dispose()
    }
}

function Assert-PortableExecutable {
    param([Parameter(Mandatory = $true)][string]$Path)

    $stream = [System.IO.File]::OpenRead($Path)
    try {
        $reader = [System.IO.BinaryReader]::new($stream)
        if ($reader.ReadUInt16() -ne 0x5A4D) {
            throw "$Path is not a PE executable."
        }
        $stream.Position = 0x3C
        $peOffset = $reader.ReadUInt32()
        $stream.Position = $peOffset
        if ($reader.ReadUInt32() -ne 0x00004550) {
            throw "$Path has an invalid PE header."
        }
    }
    finally {
        $stream.Dispose()
    }
}

function Assert-Signature {
    param([Parameter(Mandatory = $true)][string]$Path)

    $signature = Get-AuthenticodeSignature -LiteralPath $Path
    if ($RequireSigned -and $signature.Status -ne [System.Management.Automation.SignatureStatus]::Valid) {
        throw "Authenticode verification failed for ${Path}: $($signature.Status)."
    }
    if (-not $RequireSigned -and $signature.Status -notin @(
        [System.Management.Automation.SignatureStatus]::NotSigned,
        [System.Management.Automation.SignatureStatus]::Valid
    )) {
        throw "Unexpected Authenticode status for ${Path}: $($signature.Status)."
    }
    Write-Host "Signature $($signature.Status): $Path"
}

Assert-TargetPortableExecutable -Path $application
Assert-PortableExecutable -Path $nsisInstallers[0].FullName
if ($nsisInstallers[0].Name -notmatch "_$artifactTag-setup\.exe$") {
    throw "The NSIS installer name does not identify a $architectureLabel payload: $($nsisInstallers[0].Name)"
}
if ($msiInstallers[0].Name -notmatch "_$artifactTag(?:_|\.msi$)") {
    throw "The MSI installer name does not identify a $architectureLabel payload: $($msiInstallers[0].Name)"
}

$version = [System.Diagnostics.FileVersionInfo]::GetVersionInfo($application)
if ($version.ProductName -ne "MineTrace") {
    throw "Unexpected Windows ProductName '$($version.ProductName)'."
}
$actualFileVersion = [System.Version]::Parse($version.FileVersion)
$configuredVersion = [System.Version]::Parse($ExpectedVersion)
$versionMatches =
    $actualFileVersion.Major -eq $configuredVersion.Major -and
    $actualFileVersion.Minor -eq $configuredVersion.Minor -and
    $actualFileVersion.Build -eq $configuredVersion.Build -and
    ($configuredVersion.Revision -lt 0 -or $actualFileVersion.Revision -eq $configuredVersion.Revision)
if (-not $versionMatches) {
    throw "Unexpected Windows file version '$($version.FileVersion)'; expected '$ExpectedVersion'."
}

$artifacts = @($application, $nsisInstallers[0].FullName, $msiInstallers[0].FullName)
foreach ($artifact in $artifacts) {
    Assert-Signature -Path $artifact
}

$checksumPath = Join-Path $resolvedBundleRoot "SHA256SUMS.txt"
$checksumLines = foreach ($artifact in $artifacts) {
    $hash = Get-FileHash -LiteralPath $artifact -Algorithm SHA256
    "{0}  {1}" -f $hash.Hash.ToLowerInvariant(), (Split-Path -Leaf $artifact)
}
[System.IO.File]::WriteAllLines($checksumPath, $checksumLines, [System.Text.UTF8Encoding]::new($false))
Write-Host "Wrote $checksumPath"

if ($SmokeInstall) {
    $hostArchitecture = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()
    if ($hostArchitecture -ne $Architecture) {
        throw "The $architectureLabel smoke test requires a matching Windows host; this host is $hostArchitecture."
    }

    $installRoot = Join-Path $env:LOCALAPPDATA "MineTrace"
    $installedApplication = Join-Path $installRoot "minetrace.exe"
    $uninstaller = Join-Path $installRoot "uninstall.exe"
    if (Test-Path -LiteralPath $installRoot) {
        throw "Refusing to overwrite an existing MineTrace installation at $installRoot."
    }

    Write-Host "Running clean per-user NSIS install smoke test."
    $installerProcess = Start-Process -FilePath $nsisInstallers[0].FullName -ArgumentList "/S" -PassThru -Wait
    if ($installerProcess.ExitCode -ne 0) {
        throw "Silent NSIS installation failed with exit code $($installerProcess.ExitCode)."
    }
    if (-not (Test-Path -LiteralPath $installedApplication -PathType Leaf)) {
        throw "The installed MineTrace executable was not found."
    }
    if (-not (Test-Path -LiteralPath $uninstaller -PathType Leaf)) {
        throw "The MineTrace uninstaller was not found."
    }

    $appProcess = $null
    try {
        $appProcess = Start-Process -FilePath $installedApplication -PassThru
        Start-Sleep -Seconds 8
        $appProcess.Refresh()
        if ($appProcess.HasExited) {
            throw "MineTrace exited during the Windows launch smoke test with code $($appProcess.ExitCode)."
        }
        Write-Host "Installed MineTrace remained live through the launch smoke test."
    }
    finally {
        if ($null -ne $appProcess -and -not $appProcess.HasExited) {
            Stop-Process -Id $appProcess.Id -Force
            $appProcess.WaitForExit(5000) | Out-Null
        }
    }

    $uninstallProcess = Start-Process -FilePath $uninstaller -ArgumentList "/S" -PassThru -Wait
    if ($uninstallProcess.ExitCode -ne 0) {
        throw "Silent NSIS uninstall failed with exit code $($uninstallProcess.ExitCode)."
    }
    if (Test-Path -LiteralPath $installedApplication) {
        throw "The installed executable remained after uninstall."
    }
    Write-Host "NSIS install, launch, and uninstall smoke test passed."
}

Write-Host "Windows artifact verification passed."
