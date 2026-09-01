[CmdletBinding()]
param(
    [switch]$Clean
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$root = (Resolve-Path (Join-Path $PSScriptRoot ".")).Path
$targetDir = Join-Path $root "target"
$releaseDir = Join-Path $targetDir "release"
$distDir = Join-Path $root "dist"

function Invoke-Cargo {
    param(
        [Parameter(Mandatory = $true)]
        [string[]]$Arguments
    )

    & cargo @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "cargo $($Arguments -join ' ') failed with exit code $LASTEXITCODE."
    }
}

Write-Host "==> Building OxideTerm Windows release binaries"
Write-Host "    Root: $root"
Write-Host "    Output: $distDir"

if ($Clean -and (Test-Path -LiteralPath $targetDir)) {
    Write-Host "==> Removing target directory"
    Remove-Item -LiteralPath $targetDir -Recurse -Force
}

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    throw "cargo was not found. Install Rust and ensure cargo is available in PATH."
}
if (-not (Get-Command rustc -ErrorAction SilentlyContinue)) {
    throw "rustc was not found. Install Rust and ensure rustc is available in PATH."
}

$rustcVersion = & rustc -vV
if ($LASTEXITCODE -ne 0) {
    throw "rustc -vV failed with exit code $LASTEXITCODE."
}
$hostLine = $rustcVersion | Where-Object { $_ -like "host: *" } | Select-Object -First 1
if (-not $hostLine) {
    throw "rustc did not report its host target."
}
$hostTriple = $hostLine.Substring("host: ".Length).Trim()
if ($hostTriple -notlike "*-windows-*") {
    throw "build-windows.ps1 must run with a Windows Rust toolchain, found: $hostTriple"
}

Invoke-Cargo @(
    "build",
    "--locked",
    "--release",
    "-p",
    "oxideterm-gpui-app"
)

Invoke-Cargo @(
    "build",
    "--locked",
    "--release",
    "-p",
    "oxideterm-cli"
)

$helperPackages = @(
    "oxideterm-rdp-helper",
    "oxideterm-vnc-helper"
)
foreach ($helperPackage in $helperPackages) {
    Invoke-Cargo @(
        "build",
        "--locked",
        "--release",
        "-p",
        $helperPackage
    )
}

$appSource = Join-Path $releaseDir "oxideterm-native.exe"
$cliSource = Join-Path $releaseDir "oxideterm.exe"
$helperSources = @{}
foreach ($helperPackage in $helperPackages) {
    $helperSources[$helperPackage] = Join-Path $releaseDir "$helperPackage.exe"
}
foreach ($source in @($appSource, $cliSource) + @($helperSources.Values)) {
    if (-not (Test-Path -LiteralPath $source -PathType Leaf)) {
        throw "Expected build artifact was not found: $source"
    }
}

function Assert-WindowsGuiExecutable {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path
    )

    # PE subsystem 2 is the Windows GUI subsystem; 3 would create a console window.
    $bytes = [System.IO.File]::ReadAllBytes($Path)
    if ($bytes.Length -lt 64 -or $bytes[0] -ne 0x4d -or $bytes[1] -ne 0x5a) {
        throw "The application is not a valid Windows executable: $Path"
    }
    $peOffset = [BitConverter]::ToInt32($bytes, 0x3c)
    if ($peOffset -lt 0 -or $peOffset + 24 + 70 -gt $bytes.Length) {
        throw "The application has an invalid PE header: $Path"
    }
    if ($bytes[$peOffset] -ne 0x50 -or $bytes[$peOffset + 1] -ne 0x45 -or
        $bytes[$peOffset + 2] -ne 0x00 -or $bytes[$peOffset + 3] -ne 0x00) {
        throw "The application has an invalid PE signature: $Path"
    }
    $subsystem = [BitConverter]::ToUInt16($bytes, $peOffset + 24 + 68)
    if ($subsystem -ne 2) {
        throw "The application was not linked as a Windows GUI executable (subsystem $subsystem): $Path"
    }
}

Assert-WindowsGuiExecutable -Path $appSource

if (-not (Test-Path -LiteralPath $distDir)) {
    New-Item -ItemType Directory -Path $distDir | Out-Null
}

$generatedPaths = @(
    (Join-Path $distDir "oxideterm-native.exe"),
    (Join-Path $distDir "oxideterm.exe"),
    (Join-Path $distDir "resources"),
    (Join-Path $distDir "SHA256SUMS.txt")
)
foreach ($generatedPath in $generatedPaths) {
    if (Test-Path -LiteralPath $generatedPath) {
        Remove-Item -LiteralPath $generatedPath -Recurse -Force
    }
}

$appDestination = Join-Path $distDir "oxideterm-native.exe"
$cliDestination = Join-Path $distDir "oxideterm.exe"
Copy-Item -LiteralPath $appSource -Destination $appDestination
Copy-Item -LiteralPath $cliSource -Destination $cliDestination

$resourceSource = Join-Path $root "crates\oxideterm-gpui-app\resources"
$resourceDestination = Join-Path $distDir "resources"
if (Test-Path -LiteralPath $resourceSource -PathType Container) {
    Copy-Item -LiteralPath $resourceSource -Destination $resourceDestination -Recurse
}

$helperResourceDir = Join-Path $resourceDestination "helpers"
$targetHelperResourceDir = Join-Path $helperResourceDir $hostTriple
New-Item -ItemType Directory -Path $helperResourceDir -Force | Out-Null
New-Item -ItemType Directory -Path $targetHelperResourceDir -Force | Out-Null

$helperDestinations = foreach ($helperPackage in $helperPackages) {
    $primaryDestination = Join-Path $helperResourceDir "$helperPackage.exe"
    $targetDestination = Join-Path $targetHelperResourceDir "$helperPackage.exe"
    Copy-Item -LiteralPath $helperSources[$helperPackage] -Destination $primaryDestination
    Copy-Item -LiteralPath $helperSources[$helperPackage] -Destination $targetDestination
    $primaryDestination
    $targetDestination
}

$hashArtifacts = @($appDestination, $cliDestination) + @($helperDestinations)
$hashLines = foreach ($artifact in $hashArtifacts) {
    $hash = Get-FileHash -LiteralPath $artifact -Algorithm SHA256
    $relativePath = $artifact.Substring($distDir.Length).TrimStart([char[]]"\/")
    "{0}  {1}" -f $hash.Hash.ToLowerInvariant(), $relativePath
}
$hashFile = Join-Path $distDir "SHA256SUMS.txt"
$hashLines | Set-Content -LiteralPath $hashFile -Encoding ascii

Write-Host "==> Build complete"
Get-ChildItem -LiteralPath $distDir -File | Select-Object Name, Length
Write-Host "SHA256 checksums:"
$hashLines | ForEach-Object { Write-Host "    $_" }
