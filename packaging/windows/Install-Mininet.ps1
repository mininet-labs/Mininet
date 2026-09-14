<#
.SYNOPSIS
    Install the Mininet Windows client from a package, verifying it first.

.DESCRIPTION
    A thin, readable wrapper for deployments and for anyone who prefers a
    script they can read over an executable they cannot. It does no installing
    of its own: it verifies the package, prints the plan, and then calls
    mininet-setup.exe, which is the one implementation of installing.

    That indirection is the point. Two programs that both know how to install
    are two programs to keep honest, and a PowerShell reimplementation would
    be the one without the manifest re-verification, the approval check, or the
    downgrade refusal.

    No administrator rights. No network access. Nothing is written outside the
    current user's profile.

.PARAMETER Package
    Path to a .mnpkg container. Default: the only one beside this script.

.PARAMETER Setup
    Path to mininet-setup.exe. Default: the only one beside this script, or the
    one inside an existing installation.

.PARAMETER InstallRoot
    Where to install. Default %LOCALAPPDATA%\Programs\Mininet.

.PARAMETER NoStartMenu
    Do not create a Start Menu entry.

.PARAMETER DesktopShortcut
    Also create a Desktop shortcut.

.PARAMETER NoRegister
    Do not list the client in Apps & features.

.PARAMETER WhatIf
    Print the plan and stop.

.EXAMPLE
    .\Install-Mininet.ps1

.EXAMPLE
    .\Install-Mininet.ps1 -WhatIf

.EXAMPLE
    .\Install-Mininet.ps1 -InstallRoot D:\Apps\Mininet -DesktopShortcut
#>
[CmdletBinding(SupportsShouldProcess)]
param(
    [string]$Package,
    [string]$Setup,
    [string]$InstallRoot,
    [switch]$NoStartMenu,
    [switch]$DesktopShortcut,
    [switch]$NoRegister
)

$ErrorActionPreference = 'Stop'

function Find-One {
    param([string]$Pattern, [string]$What)
    $found = @(Get-ChildItem -LiteralPath $PSScriptRoot -Filter $Pattern -File -ErrorAction SilentlyContinue)
    if ($found.Count -eq 1) { return $found[0].FullName }
    if ($found.Count -eq 0) { throw "No $What found beside this script. Pass it explicitly." }
    throw "More than one $What beside this script; pass the one you want explicitly."
}

if (-not $Package) { $Package = Find-One '*.mnpkg' 'package (.mnpkg)' }
if (-not $Setup) { $Setup = Find-One 'mininet-setup*.exe' 'setup program' }

$common = @('--payload', $Package)
if ($InstallRoot) { $common += @('--install-root', $InstallRoot) }
if ($NoStartMenu) { $common += '--no-start-menu' }
if ($DesktopShortcut) { $common += '--desktop-shortcut' }
if ($NoRegister) { $common += '--no-register' }

Write-Host "-- verifying $([System.IO.Path]::GetFileName($Package))"
& $Setup --verify @common
if ($LASTEXITCODE -ne 0) {
    throw "The package did not verify (exit code $LASTEXITCODE). Do not install it."
}

Write-Host ''
Write-Host '-- what this would change'
& $Setup --dry-run @common
if ($LASTEXITCODE -ne 0) { throw "Could not plan the install (exit code $LASTEXITCODE)." }

if (-not $PSCmdlet.ShouldProcess($Package, 'install the Mininet client')) {
    Write-Host ''
    Write-Host 'Stopped before installing (-WhatIf).'
    return
}

Write-Host ''
Write-Host '-- installing'
& $Setup --silent @common
if ($LASTEXITCODE -ne 0) { throw "The install failed (exit code $LASTEXITCODE)." }

Write-Host ''
& $Setup --status @($common | Where-Object { $_ -ne '--payload' -and $_ -ne $Package })
Write-Host ''
Write-Host 'To remove it later: Apps & features, or mininet-setup.exe --uninstall'
Write-Host 'Your identities and posts are kept unless you also pass --destroy-identities.'
