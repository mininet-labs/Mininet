# mini-setup

`mininet-setup` — the installer a person double-clicks, and the scriptable
console modes behind it.

```powershell
mininet-setup                      # open the wizard
mininet-setup --silent             # install with no window
mininet-setup --dry-run            # print every change, make none
mininet-setup --verify             # re-hash the package or the installation
mininet-setup --status             # what is installed
mininet-setup --rollback           # return to the previous version
mininet-setup --uninstall          # remove it, keeping identities
mininet-setup --uninstall --destroy-identities   # and delete identities
```

Add `--json` to any mode for a single-line envelope, using the field names in
`mini_windows_setup::report` so `mini windows-setup` reports the same facts
identically.

## The wizard

Five pages: what this is, where and how, review, result, and a maintenance
page that opens instead when the client is already installed. The Review page
lists every file, both shortcuts, the registry key, and the package digest,
and Install stays disabled until the approval box is ticked. Editing any
option withdraws that approval, and the install performs the exact bytes the
window read and displayed — not a re-read that could have changed underneath.

Removing identities requires ticking a separate box and typing `DESTROY`,
because it deletes the only copy of the device's signing keys.

## Where the package comes from

In order: `--payload <FILE>`, `MININET_SETUP_PAYLOAD`, bytes embedded at
build time, then a `.mnpkg` beside the executable. The embedded case is what
makes a single downloadable `mininet-setup.exe`;
`packaging/windows/Build-WindowsRelease.ps1` produces it.

Setup never fetches its own payload. A program that can choose its own bytes
is a program whoever controls that name can redirect.

## Tested by running it

`tests/cli.rs` drives the built executable across the process boundary —
install, upgrade, rollback, verify a tampered install, uninstall, exit
codes, `--json` envelopes — and on Unix runs the client it installed. The
`.lnk` and `HKCU` halves are skipped on non-Windows with a note in the
output, which those tests also assert.
