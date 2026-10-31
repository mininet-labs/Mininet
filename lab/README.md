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
| Network drill | `sudo lab/netlab/outage-drill.sh` | Two network namespaces joined only by a veth, no default route: a Beta campaign replicates over real TCP `mini sync`. Proves no dependence on GitHub/DNS/internet on one machine. **Not** the open Beta item "drill on separate physical machines". |
| Desktop GUI | `lab/gui/screenshot.sh [out.png] [secs]` | Starts `mininet-desktop` on Xvfb (software GL) and captures a PNG. Proves it launches and renders; not real-display/DPI/vault behaviour. |

Not covered here (needs hardware or a Founder path): real BLE radios and
multi-hop evidence (#335), physical multi-machine drills, Android emulator,
external audit gates. `. lab/env.sh` exports the wine/mingw cargo variables.

Typical agent loop: `lab/doctor.sh` → code → `lab/ci-local.sh --quick` →
`lab/windows-test.sh -p <touched crate>` → `sudo lab/netlab/outage-drill.sh` →
`lab/ci-local.sh` before pushing.
