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
$logDir = Join-Path $root "logs"
$traceId = [Guid]::NewGuid().ToString("N")
$logPath = Join-Path $logDir ("build-windows-dev-{0}.log" -f (Get-Date -Format "yyyy-MM-dd"))

New-Item -ItemType Directory -Path $logDir -Force | Out-Null

function Write-BuildAudit {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Stage,
        [Parameter(Mandatory = $true)]
        [string]$Result,
        [Parameter(Mandatory = $true)]
        [string]$Message
    )

    $timestamp = [DateTime]::UtcNow.ToString("o")
    Add-Content -LiteralPath $logPath -Encoding utf8 -Value (
        "[{0}] traceId={1} stage={2} result={3} message={4}" -f
        $timestamp, $traceId, $Stage, $Result, $Message
    )
}

function Invoke-Cargo {
    param(
        [Parameter(Mandatory = $true)]
        [string[]]$Arguments
    )

    $commandDescription = "cargo $($Arguments -join ' ')"
    Write-BuildAudit -Stage "cargo-command-start" -Result "started" -Message $commandDescription
    & cargo @Arguments
    $cargoExitCode = $LASTEXITCODE
    if ($cargoExitCode -ne 0) {
        Write-BuildAudit -Stage "cargo-command-finish" -Result "failed" -Message (
            "$commandDescription exited with code $cargoExitCode; the requested Windows package was not produced"
        )
        throw "$commandDescription failed with exit code $cargoExitCode."
    }
    Write-BuildAudit -Stage "cargo-command-finish" -Result "completed" -Message (
        "$commandDescription completed successfully"
    )
}

function Stop-RunningBuildOutputs {
    # The packaging step replaces dist\*.exe. If a previous build is still
    # running, Windows locks those files and the removal/copy below fails with
    # "access denied", so stop our own binaries first (graceful, then forced).
    param(
        [Parameter(Mandatory = $true)]
        [string[]]$ProcessNames
    )

    foreach ($processName in $ProcessNames) {
        $running = @(Get-Process -Name $processName -ErrorAction SilentlyContinue)
        foreach ($process in $running) {
            Write-Host "==> Stopping running $processName (PID $($process.Id)) to replace its binary"
            Write-BuildAudit -Stage "artifact-lock" -Result "stopping" -Message (
                "stopped running process $processName (PID $($process.Id)) because it locked an output binary"
            )
            try {
                $process.CloseMainWindow() | Out-Null
                if (-not $process.WaitForExit(3000)) {
                    Stop-Process -Id $process.Id -Force -ErrorAction Stop
                }
            } catch {
                Write-Host "    Could not stop $processName (PID $($process.Id)): $($_.Exception.Message)"
                Write-BuildAudit -Stage "artifact-lock" -Result "failed" -Message (
                    "failed to stop $processName (PID $($process.Id)): $($_.Exception.Message)"
                )
            }
        }
    }
}

Write-Host "==> Building OxideTerm Windows release binaries"
Write-Host "    Root: $root"
Write-Host "    Output: $distDir"
Write-BuildAudit -Stage "build-request" -Result "accepted" -Message (
    "Windows release build started; root=$root output=$distDir clean=$Clean"
)

if ($Clean -and (Test-Path -LiteralPath $targetDir)) {
    Write-Host "==> Removing target directory"
    Remove-Item -LiteralPath $targetDir -Recurse -Force
}

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    throw "cargo was not found. Install Rust and ensure cargo is available in PATH."
}
if (-not (Get-Command rustc -ErrorAction SilentlyContinue)) {
    Write-BuildAudit -Stage "toolchain-validation" -Result "rejected" -Message (
        "rustc was not found; no build command was started"
    )
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
    Write-BuildAudit -Stage "toolchain-validation" -Result "rejected" -Message (
        "Rust host $hostTriple is not a Windows target; no release artifact was produced"
    )
    throw "build-windows.ps1 must run with a Windows Rust toolchain, found: $hostTriple"
}

# cargo links Windows binaries with the MSVC link.exe. When this script runs
# outside a "Developer PowerShell", import the Visual Studio x64 environment so
# the linker is on PATH instead of failing later during the binary link step.
if (-not (Get-Command link.exe -ErrorAction SilentlyContinue)) {
    try {
        $programFilesX86 = ${env:ProgramFiles(x86)}
        $vswherePath = if ($programFilesX86) {
            Join-Path $programFilesX86 "Microsoft Visual Studio\Installer\vswhere.exe"
        } else {
            $null
        }
        $vcvarsPath = $null
        if ($vswherePath -and (Test-Path -LiteralPath $vswherePath)) {
            $vsInstallPath = & $vswherePath -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
            if ($vsInstallPath) {
                $candidate = Join-Path $vsInstallPath "VC\Auxiliary\Build\vcvars64.bat"
                if (Test-Path -LiteralPath $candidate) {
                    $vcvarsPath = $candidate
                }
            }
        }
        if ($vcvarsPath) {
            Write-Host "==> Importing MSVC environment from $vcvarsPath"
            Write-BuildAudit -Stage "msvc-toolchain" -Result "importing" -Message (
                "link.exe was missing; importing the Visual Studio x64 developer environment"
            )
            cmd /c "`"$vcvarsPath`" >nul && set" | ForEach-Object {
                if ($_ -match '^([^=]+)=(.*)$') {
                    Set-Item -Path ("Env:" + $matches[1]) -Value $matches[2] -ErrorAction SilentlyContinue
                }
            }
        }
    } catch {
        Write-Host "    MSVC environment bootstrap failed: $($_.Exception.Message)"
        Write-BuildAudit -Stage "msvc-toolchain" -Result "failed" -Message (
            "MSVC environment bootstrap failed: $($_.Exception.Message)"
        )
    }
}
if (-not (Get-Command link.exe -ErrorAction SilentlyContinue)) {
    Write-BuildAudit -Stage "msvc-toolchain" -Result "rejected" -Message (
        "link.exe was not found; the MSVC linker is required to link Windows binaries"
    )
    throw "link.exe was not found. Run this script from 'Developer PowerShell for Visual Studio', or install the Visual Studio C++ build tools."
}

# aws-lc-sys requires NASM for optimized Windows builds. Prefer an installed
# executable, but use its maintained prebuilt fallback when NASM is absent.
if ($env:AWS_LC_SYS_NO_ASM) {
    Remove-Item Env:\AWS_LC_SYS_NO_ASM
    Write-BuildAudit -Stage "aws-lc-toolchain" -Result "corrected" -Message (
        "cleared AWS_LC_SYS_NO_ASM because aws-lc-sys permits it only for debug builds"
    )
}
$nasmCommand = Get-Command nasm -ErrorAction SilentlyContinue
if ($nasmCommand) {
    Write-Host "    NASM: $($nasmCommand.Source)"
    Write-BuildAudit -Stage "aws-lc-toolchain" -Result "available" -Message (
        "using installed NASM at $($nasmCommand.Source)"
    )
} else {
    $env:AWS_LC_SYS_PREBUILT_NASM = "1"
    Write-Host "    NASM: using aws-lc-sys prebuilt fallback"
    Write-BuildAudit -Stage "aws-lc-toolchain" -Result "fallback" -Message (
        "system NASM was not found; enabled the aws-lc-sys prebuilt NASM fallback for the release build"
    )
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
        Write-BuildAudit -Stage "artifact-validation" -Result "failed" -Message (
            "expected build artifact was missing: $source"
        )
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

# A still-running previous build locks dist\*.exe and makes the replacement fail.
Stop-RunningBuildOutputs -ProcessNames (@("oxideterm-native", "oxideterm") + $helperPackages)
Start-Sleep -Milliseconds 500

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
Write-BuildAudit -Stage "build-response" -Result "completed" -Message (
    "Windows release package completed; artifacts=$($hashArtifacts.Count) checksumFile=$hashFile"
)
