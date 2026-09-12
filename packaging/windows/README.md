# Windows packaging

Everything needed to produce and install a Mininet Windows client release.

| File | What it does |
| --- | --- |
| `build-release.sh` | Build a release on Linux, macOS, or Git Bash/WSL |
| `Build-WindowsRelease.ps1` | The same build, natively on Windows |
| `Install-Mininet.ps1` | Verify a package, print the plan, then install it |

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

No administrator. No service. No scheduled task. No network access. Files go to
`%LOCALAPPDATA%\Programs\Mininet`; identities, posts, and settings live
separately in `%LOCALAPPDATA%\Mininet` and are never touched by installing,
upgrading, or removing the program.

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
- **No MSI, and no per-machine install.** An MSI would be a second
  implementation of installing — one without the checks the engine has — so
  there deliberately is not one. A managed deployment should run
  `mininet-setup.exe --silent` per user (an Intune Win32 app in user context, or
  a logon script), which is the same tested code path an individual gets.
  A real per-machine install into `Program Files` needs elevation, a different
  update story, and its own threat model; it is not done.
- **No delta or background updates.** Every install is a whole package a person
  chose to install. Nothing polls.
- **No installer localization.** The wizard is English only.
