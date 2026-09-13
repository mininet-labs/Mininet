# mini-windows-setup

The Windows install engine: package format, digest verification, per-user
install, activation, rollback, and uninstall.

`mini-installer` (D-0071) already did this for Unix, by swapping a symlink.
Nothing did it for Windows, where a running executable is locked and there is
no symlink to swap, so the only way to "install" the client was
`cargo run -p mini-desktop`.

## What it guarantees

- **Nothing installs unverified.** Every file carries a length plus a BLAKE3
  and a SHA-256 digest in a canonical text manifest. Bytes are checked coming
  out of the container, and re-read from disk and checked again after writing,
  before anything is activated.
- **Nothing installs unapproved.** `Setup::install` requires an
  `InstallApproval` naming the exact manifest digest. An approval for one
  build cannot install another.
- **No silent downgrade.** Activation runs `mini_forge::check_no_rollback`,
  the same check the update path uses.
- **No admin, no service, no network.** Per-user directories, `HKCU` only,
  and no networking dependency exists in the tree.
- **Uninstall keeps identities by default.** Program files and user data are
  separate directories; destroying identities needs a second constructor that
  names the exact path.

## Layout

```text
%LOCALAPPDATA%\Programs\Mininet\
  versions\<version>\      program files, one directory per version
  manifests\<version>.txt  the manifest that produced it
  current.txt              which version is active
  previous.txt             which version a rollback returns to
  setup-log.txt            append-only record of what setup did
```

Identities, objects, and settings stay in `%LOCALAPPDATA%\Mininet`, which this
crate never reads or writes.

## Testable without Windows

Only two things genuinely need Windows: the `.lnk` shortcut and the
Apps & features registry entry. Both sit behind the `ShellIntegration` trait,
so `RecordingShell` captures the decision on any platform and the test suite
asserts on it. Everything else runs for real against a temporary directory in
CI, including installing an executable and then executing it.

## Honest limits

- A per-user directory is writable by anything else running as that user. This
  is not tamper-proof storage.
- Nothing is code-signed yet. SmartScreen will warn on first run, and should.
  The two digests are what a careful user can check with `Get-FileHash`;
  `mini-forge`'s release attestations are what make a digest *authentic*
  rather than merely self-consistent.
- The manifest's trailing digest catches corruption and careless edits, not a
  forger who can recompute it.
- No automatic update. Nothing here polls, fetches, or self-invokes.
