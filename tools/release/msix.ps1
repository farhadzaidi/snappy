<#
.SYNOPSIS
Packs Snappy for the Microsoft Store: one MSIX package per architecture, then a bundle of them.

.DESCRIPTION
Pack, after `pnpm -C apps/app/ui build` and
`cargo build --release --target <triple> -p snappy -p snappy-app`:

    tools/release/msix.ps1 -Arch x64 -IdentityName <name> -Publisher <CN=...>

stages snappy.exe, snappy-app.exe, and the logos in packaging/msix/Assets, renders
packaging/msix/AppxManifest.xml.template, indexes the logos with MakePri, and writes
target/msix/packages/Snappy_<version>_<arch>.msix.

Bundle, once each architecture is packed:

    tools/release/msix.ps1 -Bundle

writes target/msix/Snappy_<version>.msixbundle from every package in target/msix/packages, and
target/msix/Snappy_<version>.msixupload, the bundle zipped the way Partner Center takes it.

The version is the daemon's Cargo version with a fourth part of 0, which the Store requires.
The Store signs what it publishes, so nothing needs signing for it. To install a package
outside the Store, sign it with -Certificate: a .pfx whose subject is the -Publisher given
when packing. docs/releasing/microsoft-store.md shows how to make one for testing.

Needs MakeAppx and MakePri from the Windows SDK, and SignTool to sign.
#>
[CmdletBinding(DefaultParameterSetName = 'Pack')]
param(
    [Parameter(ParameterSetName = 'Pack', Mandatory)]
    [ValidateSet('x64', 'arm64')]
    [string] $Arch,

    # The package's identity name and publisher, from Partner Center's Product identity page.
    [Parameter(ParameterSetName = 'Pack')]
    [string] $IdentityName = $env:MSIX_IDENTITY_NAME,
    [Parameter(ParameterSetName = 'Pack')]
    [string] $Publisher = $env:MSIX_PUBLISHER,

    # Where the release executables are; by default Cargo's folder for the architecture's target.
    [Parameter(ParameterSetName = 'Pack')]
    [string] $BinDir,

    [Parameter(ParameterSetName = 'Bundle', Mandatory)]
    [switch] $Bundle,

    [string] $Certificate,
    [SecureString] $CertificatePassword
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$out = Join-Path $root 'target\msix'
$packages = Join-Path $out 'packages'
$targets = @{ x64 = 'x86_64-pc-windows-msvc'; arm64 = 'aarch64-pc-windows-msvc' }

# The daemon's version as MSIX wants it: four numbers, the last 0.
function Get-PackageVersion {
    $metadata = cargo metadata --no-deps --format-version 1 --manifest-path (Join-Path $root 'Cargo.toml')
    if ($LASTEXITCODE -ne 0) { throw 'cargo metadata failed.' }
    $version = (($metadata -join "`n" | ConvertFrom-Json).packages | Where-Object name -EQ 'snappy').version
    if ($version -notmatch '^(\d+)\.(\d+)\.(\d+)$') {
        throw "Version $version isn't plain X.Y.Z, which is all an MSIX version can hold."
    }
    foreach ($part in 1..3) {
        if ([int]$Matches[$part] -gt 65535) { throw "Version $version has a part over 65535." }
    }
    "$version.0"
}

# The newest Windows SDK's copy of a tool, for this machine's architecture.
function Find-SdkTool([string] $name) {
    $kits = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits\10\bin'
    $hostArch = if ($env:PROCESSOR_ARCHITECTURE -eq 'ARM64') { 'arm64' } else { 'x64' }
    $found = Get-ChildItem $kits -Directory -Filter '10.*' -ErrorAction SilentlyContinue |
        Sort-Object { [version]$_.Name } -Descending |
        ForEach-Object { Join-Path $_.FullName "$hostArch\$name" } |
        Where-Object { Test-Path $_ } |
        Select-Object -First 1
    if (-not $found) { throw "$name not found under $kits. Install the Windows SDK." }
    $found
}

function Invoke-Tool([string] $tool, [string[]] $arguments) {
    & $tool @arguments
    if ($LASTEXITCODE -ne 0) { throw "$(Split-Path $tool -Leaf) failed with exit code $LASTEXITCODE." }
}

function Set-Signature([string] $file) {
    if (-not $Certificate) { return }
    $arguments = @('sign', '/fd', 'SHA256', '/f', (Resolve-Path $Certificate).Path)
    if ($CertificatePassword) {
        $arguments += @('/p', [Net.NetworkCredential]::new('', $CertificatePassword).Password)
    }
    Invoke-Tool (Find-SdkTool 'signtool.exe') ($arguments + $file)
}

function New-Package {
    if (-not $IdentityName -or -not $Publisher) {
        throw 'Give -IdentityName and -Publisher, or set MSIX_IDENTITY_NAME and MSIX_PUBLISHER.'
    }
    if (-not $BinDir) { $BinDir = Join-Path $root "target\$($targets[$Arch])\release" }
    $version = Get-PackageVersion

    $work = Join-Path $out $Arch
    $stage = Join-Path $work 'stage'
    if (Test-Path $stage) { Remove-Item $stage -Recurse -Force }
    New-Item -ItemType Directory -Force -Path (Join-Path $stage 'Assets'), $packages | Out-Null

    foreach ($exe in 'snappy.exe', 'snappy-app.exe') {
        $path = Join-Path $BinDir $exe
        if (-not (Test-Path $path)) { throw "$path not found. Build the release first." }
        Copy-Item $path $stage
    }
    Copy-Item (Join-Path $root 'packaging\msix\Assets\*.png') (Join-Path $stage 'Assets')

    $values = @{
        IDENTITY_NAME = $IdentityName
        PUBLISHER     = $Publisher
        VERSION       = $version
        ARCHITECTURE  = $Arch
    }
    $manifest = Get-Content -Raw (Join-Path $root 'packaging\msix\AppxManifest.xml.template')
    foreach ($key in $values.Keys) {
        $manifest = $manifest.Replace("{{$key}}", [Security.SecurityElement]::Escape($values[$key]))
    }
    if ($manifest -match '\{\{\w+\}\}') { throw "The manifest template has an unfilled $($Matches[0])." }
    $manifestPath = Join-Path $stage 'AppxManifest.xml'
    [IO.File]::WriteAllText($manifestPath, $manifest, [Text.UTF8Encoding]::new($false))

    # resources.pri maps the logos' scale and target-size names to the plain names the manifest uses.
    $makePri = Find-SdkTool 'makepri.exe'
    $priConfig = Join-Path $work 'priconfig.xml'
    Invoke-Tool $makePri @('createconfig', '/cf', $priConfig, '/dq', 'en-US', '/pv', '10.0.0', '/o')
    Invoke-Tool $makePri @('new', '/pr', $stage, '/cf', $priConfig, '/mn', $manifestPath,
        '/of', (Join-Path $stage 'resources.pri'), '/o')

    $package = Join-Path $packages "Snappy_$($version)_$Arch.msix"
    Invoke-Tool (Find-SdkTool 'makeappx.exe') @('pack', '/d', $stage, '/p', $package, '/o')
    Set-Signature $package
    Write-Host "Wrote $package"
}

function New-Bundle {
    $version = Get-PackageVersion
    $found = @(Get-ChildItem $packages -Filter "Snappy_$($version)_*.msix" -ErrorAction SilentlyContinue)
    if ($found.Count -eq 0) { throw "No Snappy $version packages in $packages. Pack each architecture first." }
    $others = @(Get-ChildItem $packages -File | Where-Object { $found.FullName -notcontains $_.FullName })
    if ($others.Count -gt 0) { throw "$packages holds other files too: $($others.Name -join ', ')." }

    $bundle = Join-Path $out "Snappy_$version.msixbundle"
    Invoke-Tool (Find-SdkTool 'makeappx.exe') @('bundle', '/d', $packages, '/p', $bundle, '/bv', $version, '/o')
    Set-Signature $bundle

    # An .msixupload is a zip holding the bundle.
    $upload = Join-Path $out "Snappy_$version.msixupload"
    $zip = "$upload.zip"
    Compress-Archive -Path $bundle -DestinationPath $zip -Force
    Move-Item $zip $upload -Force
    Write-Host "Wrote $bundle and $upload"
}

if ($Bundle) { New-Bundle } else { New-Package }
