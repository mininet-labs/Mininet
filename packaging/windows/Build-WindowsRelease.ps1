<#
.SYNOPSIS
    Build a Mininet Windows client release on Windows.

.DESCRIPTION
    Does exactly what packaging/windows/build-release.sh does, for people who
    would rather not install a POSIX shell. Both scripts call the same
    `mini windows pack`, so neither can drift into producing a
    differently-shaped package.

    Steps: build mini-desktop, mini-cli and mini-setup in release mode; stage
    the files; pack a verified container plus a readable manifest; rebuild
    mini-setup with the container embedded so there is one file to hand
    someone; write SHA256SUMS.txt.

    Reproducibility: the package's build timestamp comes from the git commit's
    author date, not the clock, so two people building the same commit get
    byte-identical containers and can compare digests. -BuiltAtMs overrides it
    for a build outside a checkout.

    Nothing here signs anything. Authenticode needs a certificate and a
    governed signing process that do not exist yet, so SmartScreen will warn
    on first run and should. What a careful user gets instead is the manifest:
    every file's length, BLAKE3, and SHA-256, checkable with Get-FileHash and
    no Mininet binary at all.

.PARAMETER Target
    Rust target triple. Default x86_64-pc-windows-msvc.

.PARAMETER Version
    Package version. Default: read from crates/mini-desktop/Cargo.toml.

.PARAMETER BuiltAtMs
    Build timestamp in milliseconds since the Unix epoch.

.PARAMETER OutDir
    Output directory. Default dist\windows.

.PARAMETER SkipSetupEmbed
    Stop after the container; do not rebuild setup with the payload embedded.

.EXAMPLE
    .\packaging\windows\Build-WindowsRelease.ps1

.EXAMPLE
    .\packaging\windows\Build-WindowsRelease.ps1 -Version 0.2.0 -OutDir C:\out
#>
[CmdletBinding()]
param(
    [string]$Target = 'x86_64-pc-windows-msvc',
    [string]$Version,
    [long]$BuiltAtMs = 0,
    [string]$OutDir,
    [switch]$SkipSetupEmbed,
    [switch]$Msi
)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
Push-Location $repoRoot
try {
    if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
        throw 'cargo is not on PATH. Install Rust from https://rustup.rs first.'
    }

    if (-not $Version) {
        $manifest = Get-Content 'crates\mini-desktop\Cargo.toml'
        $match = $manifest | Select-String -Pattern '^version = "(.*)"' | Select-Object -First 1
        if (-not $match) { throw 'Could not read a version; pass -Version.' }
        $Version = $match.Matches[0].Groups[1].Value
    }

    if ($BuiltAtMs -le 0) {
        $commitSeconds = (& git log -1 --format=%at 2>$null)
        if ($LASTEXITCODE -ne 0 -or -not $commitSeconds) {
            throw 'Not a git checkout, so there is no commit date to use as a reproducible build timestamp. Pass -BuiltAtMs.'
        }
        $BuiltAtMs = [long]$commitSeconds * 1000
    }

    if (-not $OutDir) { $OutDir = Join-Path $repoRoot 'dist\windows' }
    $stageDir = Join-Path $OutDir 'stage'
    $binDir = Join-Path $repoRoot "target\$Target\release"
    $container = Join-Path $OutDir "mininet-client-$Version-$Target.mnpkg"
    $setupOut = Join-Path $OutDir "mininet-setup-$Version-$Target.exe"

    Write-Host '== Mininet Windows release =='
    Write-Host "   version      $Version"
    Write-Host "   target       $Target"
    Write-Host "   built-at-ms  $BuiltAtMs"
    Write-Host "   out          $OutDir"
    Write-Host ''

    if (Test-Path $stageDir) { Remove-Item -LiteralPath $stageDir -Recurse -Force }
    New-Item -ItemType Directory -Force -Path (Join-Path $stageDir 'docs') | Out-Null

    Write-Host '-- building client, cli, and setup'
    & cargo build --release --target $Target -p mini-desktop -p mini-cli -p mini-setup -p mini-value-selftest
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed with exit code $LASTEXITCODE" }

    Write-Host '-- staging package files'
    Copy-Item (Join-Path $binDir 'mininet-desktop.exe') (Join-Path $stageDir 'mininet-desktop.exe')
    Copy-Item (Join-Path $binDir 'mini.exe') (Join-Path $stageDir 'mini.exe')
    # The setup program inside the package is the variant *without* an
    # embedded payload: it is what Apps & features runs to uninstall, verify,
    # or roll back, so it never needs to carry a copy of the package it came
    # from. The distributable executable below is the same code with the
    # payload embedded.
    Copy-Item (Join-Path $binDir 'mininet-setup.exe') (Join-Path $stageDir 'mininet-setup.exe')
    # The value-layer diagnostics run in their own process, so the client has
    # to ship that process. Without it every installed copy silently skips the
    # value and treasury checks the Diagnostics page advertises.
    Copy-Item (Join-Path $binDir 'mininet-value-selftest.exe') (Join-Path $stageDir 'mininet-value-selftest.exe')
    Copy-Item 'docs\WINDOWS_CLIENT_SECURITY.md' (Join-Path $stageDir 'docs\SECURITY.txt')
    Copy-Item 'crates\mini-desktop\README.md' (Join-Path $stageDir 'docs\CLIENT.txt')
    Copy-Item 'LICENSE' (Join-Path $stageDir 'docs\LICENSE.txt')
    Copy-Item 'docs\guides\windows-install-guide.md' (Join-Path $stageDir 'docs\INSTALL.txt')

    Write-Host '-- packing the container'
    & cargo run --release -q -p mini-cli -- windows pack `
        --source $stageDir `
        --out $container `
        --version $Version `
        --target $Target `
        --built-at-ms $BuiltAtMs `
        --shortcut 'mininet-desktop.exe=Mininet'
    if ($LASTEXITCODE -ne 0) { throw "packing failed with exit code $LASTEXITCODE" }

    Write-Host ''
    Write-Host '-- verifying the container reads back exactly'
    & cargo run --release -q -p mini-cli -- windows inspect $container
    if ($LASTEXITCODE -ne 0) { throw "container verification failed with exit code $LASTEXITCODE" }

    if (-not $SkipSetupEmbed) {
        Write-Host ''
        Write-Host '-- rebuilding setup with the package embedded'
        $env:MININET_SETUP_PAYLOAD = $container
        try {
            & cargo build --release --target $Target -p mini-setup
            if ($LASTEXITCODE -ne 0) { throw "embedding build failed with exit code $LASTEXITCODE" }
        } finally {
            Remove-Item Env:\MININET_SETUP_PAYLOAD -ErrorAction SilentlyContinue
        }
        Copy-Item (Join-Path $binDir 'mininet-setup.exe') $setupOut -Force
        Write-Host "   $setupOut"
    }

    if ($Msi) {
        Write-Host ''
        Write-Host '-- building the MSI for managed deployment'
        if (-not (Get-Command wix -ErrorAction SilentlyContinue)) {
            throw 'wix is not on PATH. Install it with: dotnet tool install --global wix'
        }
        # The MSI wraps mininet-setup.exe rather than reimplementing the
        # install, so it needs the setup program and a package side by side
        # under fixed names.
        $msiPayload = Join-Path $OutDir 'msi-payload'
        if (Test-Path $msiPayload) { Remove-Item -LiteralPath $msiPayload -Recurse -Force }
        New-Item -ItemType Directory -Force -Path $msiPayload | Out-Null
        if (-not (Test-Path $setupOut)) {
            throw 'the MSI needs the self-contained setup executable; do not combine -Msi with -SkipSetupEmbed'
        }
        Copy-Item $setupOut (Join-Path $msiPayload 'mininet-setup.exe')
        Copy-Item $container (Join-Path $msiPayload 'mininet-client.mnpkg')
        $msiOut = Join-Path $OutDir "Mininet-$Version.msi"
        & wix build (Join-Path $PSScriptRoot 'Mininet.wxs') `
            -ext WixToolset.Util.wixext `
            -d Version=$Version `
            -d Payload=$msiPayload `
            -o $msiOut
        if ($LASTEXITCODE -ne 0) { throw "wix build failed with exit code $LASTEXITCODE" }
        Write-Host "   $msiOut"
        Remove-Item -LiteralPath $msiPayload -Recurse -Force
    }

    Write-Host ''
    Write-Host '-- writing SHA256SUMS.txt'
    $sums = Join-Path $OutDir 'SHA256SUMS.txt'
    Get-ChildItem -LiteralPath $OutDir -File |
        Where-Object { $_.Name -ne 'SHA256SUMS.txt' } |
        Sort-Object Name |
        ForEach-Object {
            $hash = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLower()
            "$hash  $($_.Name)"
        } | Set-Content -LiteralPath $sums -Encoding ascii
    Get-Content -LiteralPath $sums

    Write-Host ''
    Write-Host 'Done. To install:'
    Write-Host "  $(Split-Path -Leaf $setupOut)                 open the installer window"
    Write-Host "  $(Split-Path -Leaf $setupOut) --silent        install with no window"
    Write-Host "  $(Split-Path -Leaf $setupOut) --dry-run       print every change, make none"
    Write-Host ''
    Write-Host 'This build is not code-signed. SmartScreen will warn on first run.'
    Write-Host 'Every file can be checked with Get-FileHash against'
    Write-Host "  $(Split-Path -Leaf ($container -replace '\.mnpkg$', '.manifest.txt'))"
    Write-Host 'needing no Mininet binary at all.'
} finally {
    Pop-Location
}
