# Installing the Mininet Windows client

For the person installing it. Operators building a release want
`packaging/windows/README.md`; the engineering rationale is D-0520.

## What you get

One file: `mininet-setup-<version>-x86_64-pc-windows-msvc.exe`. The client is
inside it. Double-click it and a five-page installer opens.

You can also install without a window:

```powershell
mininet-setup.exe --dry-run    # print every change, make none
mininet-setup.exe --silent     # install using the defaults
```

## What it does, and what it will not do

It installs into `%LOCALAPPDATA%\Programs\Mininet`, inside your own user
profile. **No administrator prompt, no service, no scheduled task, and no
network connection at any point.** The setup program has no networking code
in it at all; it installs the bytes it was handed.

Every file is checked against the package manifest as it is written, then read
back off the disk and checked again before anything is switched over. A file
that does not match is refused rather than installed.

The Review page lists every change before you agree to any of it: each file,
the Start Menu entry, the Apps & features registration, and the package
digest. Install stays disabled until you tick the approval box, and changing
any option withdraws that approval, because it was given for a different set
of changes.

## Windows will warn you, and it is right to

These builds are not code-signed, so SmartScreen shows "Windows protected your
PC" on first run. That warning is accurate: Windows genuinely cannot tell who
built this. Code signing needs a certificate and a governed signing process
that do not exist yet, and claiming otherwise would be the overclaiming this
project treats as a bug.

What you can do instead is check the bytes yourself, without running anything
we shipped:

```powershell
Get-Content .\mininet-client-0.1.0-x86_64-pc-windows-msvc.manifest.txt
Get-FileHash -Algorithm SHA256 "$env:LOCALAPPDATA\Programs\Mininet\versions\0.1.0\mininet-desktop.exe"
```

Manifest `file` lines read `file <bytes> <blake3> <sha256> <path>`. A matching
SHA-256 means the file on disk is the file the manifest describes.

That still does not tell you *who wrote the manifest*. Anyone who can rewrite
the manifest can recompute its own digest. Authenticity comes from release
attestations in `mini-forge`, where independent builders sign that they built
the same bytes.

## Your data is not in the install directory

| Directory | What is in it | Removed by uninstalling? |
| --- | --- | --- |
| `%LOCALAPPDATA%\Programs\Mininet` | The program | Yes |
| `%LOCALAPPDATA%\Mininet` | Identities, posts, settings | **No**, unless you ask |

That separation is the point. Removing the client cannot destroy an identity
you cannot recreate. Deleting identities is a separate checkbox that also
requires typing `DESTROY`, because it erases the only copy of this device's
signing keys: nobody can restore them and you cannot re-create the same
identity.

## Updating

There is no automatic update. Nothing polls, nothing downloads, nothing
installs on a timer. To update, run the newer setup program yourself with a
package you obtained however you chose.

Installing an *older* version over a newer one is refused unless you pass
`--allow-downgrade`, because a silent downgrade is how a fixed vulnerability
comes back.

The previous version stays on disk, so **Version & install** in the client (or
`mininet-setup.exe --rollback`) can go back to it. Rollback re-hashes every
file of that older version first: rolling back onto a corrupted install would
turn one broken client into two.

## If something looks wrong

```powershell
mininet-setup.exe --verify     # re-hash every installed file
mininet-setup.exe --status     # version, digest, rollback target, paths
```

`--verify` exits non-zero when the installation does not match its manifest,
so it works in a script. It lists every problem it found, not just the first.

The client's **Diagnostics** page runs the real identity, storage, social,
media, messaging, sync, governance, erasure-coding and storage-proof code on
your machine and shows what happened. Roughly a third of those checks confirm
that something is *refused*. The same suite is `mini selftest`.

`%LOCALAPPDATA%\Programs\Mininet\setup-log.txt` records what setup did, as
plain text. It carries versions, digests and timestamps and deliberately no
path from your profile, so it is safe to paste into a bug report.

## Deploying it to many machines

`packaging/windows/Mininet.wxs` builds an MSI that Intune, Group Policy, or
Configuration Manager can consume:

```powershell
msiexec /i Mininet-0.1.0.msi /quiet /norestart
msiexec /x Mininet-0.1.0.msi /quiet /norestart
```

Deploy it **in user context**. The MSI does not know how to install Mininet;
it lays down the setup program and a package and calls `mininet-setup.exe`, so
every check above still applies. Removing it removes the client and keeps
identities. There is deliberately no way to make it destroy them: a deployment
tool must not be able to erase somebody's signing keys as a side effect of
removing an application.

## Removing it

Apps & features, or:

```powershell
mininet-setup.exe --uninstall                        # keeps your identities
mininet-setup.exe --uninstall --destroy-identities   # irreversible
```

## Known limits

- Not code-signed; SmartScreen warns on first run.
- No per-machine install. There is an MSI for managed deployment, but it
  installs per user like everything else here.
- The installer window is English only.
- A per-user install directory is writable by anything else running as you. It
  is not tamper-proof storage, and no arrangement of files could make it so.
