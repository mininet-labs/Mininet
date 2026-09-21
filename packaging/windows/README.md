# Windows packaging

Everything needed to produce and install a Mininet Windows client release.

| File | What it does |
| --- | --- |
| `build-release.sh` | Build a release on Linux, macOS, or Git Bash/WSL |
| `Build-WindowsRelease.ps1` | The same build, natively on Windows |
| `Install-Mininet.ps1` | Verify a package, print the plan, then install it |
| `Mininet.wxs` | MSI for managed deployment, wrapping the same installer |

Both build scripts call the same `mini windows pack`, and the install script
calls `mininet-setup.exe` rather than installing anything itself. There is one
implementation of "install" in this repository, in
`crates/mini-windows-setup`, and it is the one with the manifest
re-verification, the approval check, and the downgrade refusal.

## Building a release

```sh
packaging/windows/build-release.sh
```

```powershell
.\packaging\windows\Build-WindowsRelease.ps1
```

Output in `dist/windows`:

The verified `.mnpkg` contains both `mininet-desktop.exe` and
`mininet-app-service.exe`; the latter is the per-user identity/social core
required by migrated W1 signing flows.

| Artifact | What it is |
| --- | --- |
| `mininet-setup-<version>-<target>.exe` | One file to hand someone. The wizard, with the package inside it. |
| `mininet-client-<version>-<target>.mnpkg` | The package on its own, for offline or scripted installs. |
| `mininet-client-<version>-<target>.manifest.txt` | Readable manifest: length, BLAKE3, and SHA-256 per file. |
| `SHA256SUMS.txt` | Digests of everything above. |

### Reproducible by default

The package's build timestamp is the **git commit's author date**, not the
clock. Two people who build the same commit get byte-identical containers and
can compare digests to conclude something. A script that stamped "now" would
make every rebuild differ and quietly destroy that check, which is why
`mini windows pack` requires `--built-at-ms` rather than defaulting to the
current time.

To confirm somebody else's build:

```sh
packaging/windows/build-release.sh --out /tmp/check
diff /tmp/check/mininet-client-*.mnpkg ./their-copy.mnpkg
```

Full byte-for-byte reproducibility of the Rust binaries themselves is a
separate, unfinished piece of work (SPEC-11, `.github/workflows/reproducibility.yml`).
The *container* is reproducible today; the compiler output inside it is not
yet guaranteed to be.

### Testing the pipeline without a Windows toolchain

```sh
packaging/windows/build-release.sh --host --out /tmp/dist
```

`--host` builds host binaries and stages them under their Windows names. The
resulting package is structurally real — it installs, verifies, upgrades, rolls
back, and uninstalls — but its executables are not Windows ones. This is what
CI runs, so the packaging path is exercised on every change rather than only
when somebody happens to be on Windows.

## Installing

```powershell
mininet-setup-0.1.0-x86_64-pc-windows-msvc.exe            # the wizard
mininet-setup-0.1.0-x86_64-pc-windows-msvc.exe --dry-run  # print every change
mininet-setup-0.1.0-x86_64-pc-windows-msvc.exe --silent   # no window
```

Or, from an extracted release directory:

```powershell
.\Install-Mininet.ps1 -WhatIf     # verify and print the plan
.\Install-Mininet.ps1             # and then install
```

No administrator and no administrator-owned Windows Service. The desktop
launches the packaged `mininet-app-service.exe` as a per-user sibling process;
there is no scheduled task and installation itself performs no network access.
Files go to `%LOCALAPPDATA%\Programs\Mininet`; identities, posts, and settings live
separately in `%LOCALAPPDATA%\Mininet` and are never touched by installing,
upgrading, or removing the program.

## Managed deployment

```powershell
.\packaging\windows\Build-WindowsRelease.ps1 -Msi
msiexec /i dist\windows\Mininet-0.1.0.msi /quiet /norestart
```

Needs WiX v5: `dotnet tool install --global wix` and
`wix extension add --global WixToolset.Util.wixext`.

The MSI deliberately does not know how to install Mininet. It lays down
`mininet-setup.exe` and a package, then calls the setup program, so the
manifest re-verification, the digest-bound approval, and the downgrade refusal
all still apply. Expressing the install as MSI components instead would be a
second implementation with none of those, and it would be the one nobody
tests.

Two details that follow from that split, and that matter if you edit the
`.wxs`: the payload lives in `%LOCALAPPDATA%\Mininet Setup`, a *sibling* of
the install root rather than inside it, because the uninstall action removes
that root recursively; and the install action suppresses setup's own Apps &
features registration so the MSI's is the only one. CI installs and removes
through `msiexec` on a real Windows runner and checks both.

Removing the MSI keeps identities, and there is no property to change that. A
deployment tool must not be able to erase somebody's signing keys as a side
effect of removing an application.

## Verifying without trusting anything here

The manifest records a SHA-256 beside Mininet's own BLAKE3 for exactly this
reason. Nothing below runs a Mininet binary:

```powershell
Get-Content .\mininet-client-0.1.0-x86_64-pc-windows-msvc.manifest.txt
Get-FileHash -Algorithm SHA256 "$env:LOCALAPPDATA\Programs\Mininet\versions\0.1.0\mininet-desktop.exe"
```

The `file` lines are `file <bytes> <blake3> <sha256> <path>`. A matching
SHA-256 means the file on disk is the file the manifest describes.

What that does *not* establish is who wrote the manifest. The manifest's
trailing `end` digest catches corruption and careless edits, not a forger who
can recompute it. Authenticity comes from `mini-forge`'s release attestations
(`mini release attest` / `verify`), where independent builders sign that they
built the same bytes.

## What is missing, plainly

- **No code signing.** Authenticode needs a certificate and a governed signing
  process, neither of which exists yet. SmartScreen will warn on first run, and
  it is right to. The manifest is what a careful user has instead.
- **No per-machine install.** The MSI below installs per user, like
  everything else here. A real per-machine install into `Program Files` needs
  elevation, a different update story, and its own threat model; it is not
  done.
- **No delta or background updates.** Every install is a whole package a person
  chose to install. Nothing polls.
- **No installer localization.** The wizard is English only.
