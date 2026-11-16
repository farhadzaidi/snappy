# Build Snappy's Windows packages for one architecture into dist\release\:
#
#   Snappy-<version>-windows-<x64|arm64>.msi         (WiX 5)
#   Snappy-<version>-windows-<x64|arm64>-setup.exe   (NSIS 3)
#
# Both install for the current user, with no administrator rights, into
# %LOCALAPPDATA%\Programs\Snappy. Neither is signed.
#
#   pwsh tools\release\windows.ps1 -Arch x64
#
# Needs the build tools from CONTRIBUTING.md, plus WiX 5 with its Util extension and NSIS:
#
#   dotnet tool install --global wix --version 5.0.2
#   wix extension add --global WixToolset.Util.wixext/5.0.2
#   winget install NSIS.NSIS
#
# -NoBuild packages binaries that are already built.
param(
    [Parameter(Mandatory)][ValidateSet('x64', 'arm64')][string]$Arch,
    [switch]$NoBuild
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
Set-Location $root

# Stops at the first native command that fails, as `set -e` would.
function Invoke-Checked {
    param([Parameter(Mandatory)][string]$Program, [string[]]$Arguments = @())
    & $Program @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "$Program $($Arguments -join ' ') exited with $LASTEXITCODE"
    }
}

# Snappy's version: `version` in `[workspace.package]` of the workspace's Cargo.toml.
function Get-SnappyVersion {
    $inPackage = $false
    foreach ($line in Get-Content (Join-Path $root 'Cargo.toml')) {
        if ($line -match '^\[') { $inPackage = ($line -eq '[workspace.package]') }
        elseif ($inPackage -and $line -match '^version\s*=\s*"([^"]+)"') { return $Matches[1] }
    }
    throw "Can't find the version in Cargo.toml"
}

function Find-Program {
    param([string]$Name, [string[]]$Fallbacks = @())
    $command = Get-Command $Name -ErrorAction SilentlyContinue
    if ($command) { return $command.Source }
    foreach ($path in $Fallbacks) {
        if (Test-Path $path) { return $path }
    }
    throw "Can't find $Name. See the top of tools\release\windows.ps1 for what to install."
}

$version = Get-SnappyVersion
$target = @{ x64 = 'x86_64-pc-windows-msvc'; arm64 = 'aarch64-pc-windows-msvc' }[$Arch]
$binaries = Join-Path $root "target\$target\release"
$out = Join-Path $root 'dist\release'
New-Item -ItemType Directory -Force $out | Out-Null

$wix = Find-Program 'wix' @("$env:USERPROFILE\.dotnet\tools\wix.exe")
$makensis = Find-Program 'makensis' @(
    "${env:ProgramFiles(x86)}\NSIS\makensis.exe",
    "$env:ProgramFiles\NSIS\makensis.exe"
)

if (-not $NoBuild) {
    Invoke-Checked 'pnpm' @('-C', 'apps/app/ui', 'build')
    Invoke-Checked 'cargo' @('build', '--release', '--locked', '--target', $target, '-p', 'snappy', '-p', 'snappy-app')
}
foreach ($program in 'snappy.exe', 'snappy-app.exe') {
    if (-not (Test-Path (Join-Path $binaries $program))) { throw "Missing $binaries\$program" }
}

$msi = Join-Path $out "Snappy-$version-windows-$Arch.msi"
Invoke-Checked $wix @(
    'build', 'packaging\windows\snappy.wxs',
    '-arch', $Arch,
    '-ext', 'WixToolset.Util.wixext',
    '-d', "Version=$version",
    '-d', "BinDir=$binaries",
    '-d', "Root=$root",
    '-o', $msi
)

$setup = Join-Path $out "Snappy-$version-windows-$Arch-setup.exe"
Invoke-Checked $makensis @(
    '/V3', '/INPUTCHARSET', 'UTF8',
    "/DVERSION=$version",
    "/DARCH=$Arch",
    "/DBINDIR=$binaries",
    "/DROOT=$root",
    "/DOUTFILE=$setup",
    'packaging\windows\snappy.nsi'
)

# WiX leaves its debug symbols beside the package; only the packages are released.
Remove-Item (Join-Path $out "Snappy-$version-windows-$Arch.wixpdb") -ErrorAction SilentlyContinue

Write-Host "Built:"
Get-Item $msi, $setup | Format-Table Name, Length -AutoSize
