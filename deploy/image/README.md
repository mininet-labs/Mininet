# Appliance images — Phase 2 (not built yet)

This directory is a placeholder for Phase 2 of
[`docs/design/mininet-node-appliance.md`](../../docs/design/mininet-node-appliance.md):
signed QCOW2, raw disk, and cloud images (and an installation ISO if still
needed) produced by applying `deploy/`'s manifest to a pinned Debian Stable
base in a reproducible build.

**Nothing here is built yet.** Day 0's scope is the deployment profile
(`packages.lock`, `systemd/`, `sysctl.d/`, `nftables/`, `users/`,
`installer/`, `verification/`) applied to an already-installed Debian
Stable machine or VM by a human running `deploy/installer/install.sh`
directly. Producing a bootable image from that profile — and reaching the
same reproducibility bar the rest of the forge spine holds (two
independent builders, bit-identical output) — is Phase 2 work, gated on
Day 0's installer actually being proven reliable first.
