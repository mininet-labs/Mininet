# Mininet Lab

A disposable Linux environment so an engineer (human or agent) can test
everything locally instead of waiting for CI. It adds **tooling only**: no
crate, protocol, or governance surface changes.

| Capability | Script | What it proves / does NOT prove |
|---|---|---|
| Setup | `lab/bootstrap.sh` (Debian/Ubuntu) or `lab/Dockerfile` | Installs rust target, mingw-w64, wine64, Xvfb, ImageMagick, iproute2. |
| Health | `lab/doctor.sh` | Table of what works on this machine. |
| CI mirror | `lab/ci-local.sh [--quick]` | fmt, workspace clippy, tests, Beta adapters, nav index, decision check — the `check` job. Not CodeQL, deny/audit, Windows, Android. |
| Windows binaries | `lab/windows-test.sh [-p crate]…` | Runs `x86_64-pc-windows-gnu` tests under **wine**: catches path-prefix / IO-semantics bugs. Wine is not Windows; the `windows-client` CI job stays authoritative. |
| Network drill | `lab/netlab/outage-drill.sh` (no sudo) | Two rootless network namespaces joined only by a veth, no default route: a Beta campaign replicates over real TCP `mini sync`. Proves no dependence on GitHub/DNS/internet on one machine. **Not** the open Beta item "drill on separate physical machines". |
| Desktop GUI | `lab/gui/screenshot.sh [out.png] [secs]` | Starts `mininet-desktop` on Xvfb (software GL) and captures a PNG. Proves it launches and renders; not real-display/DPI/vault behaviour. |

Not covered here (needs hardware or a Founder path): real BLE radios and
multi-hop evidence (#335), physical multi-machine drills, Android emulator,
external audit gates. `. lab/env.sh` exports the wine/mingw cargo variables.

Typical agent loop: `lab/doctor.sh` → code → `lab/ci-local.sh --quick` →
`lab/windows-test.sh -p <touched crate>` → `lab/netlab/outage-drill.sh` →
`lab/ci-local.sh` before pushing.

## Trust model

Nothing in the lab needs root or `sudo`. `netlab/outage-drill.sh` runs entirely
inside a throwaway rootless user+mount+network namespace (`unshare -Urmn`);
"root" there is just you, mapped to uid 0 inside the namespace, with no power
over the host. The script refuses to run as host root, so a hostile checkout
cannot use it to escalate. Wine tests, the CI mirror and the GUI screenshot also
run as the caller. Running untrusted code is still running untrusted code as
yourself, so prefer a disposable VM or container for checkouts you have not
reviewed.
