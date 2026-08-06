# `deploy/` — Mininet Node Appliance (Day 0)

Full doctrine, staged plan, and constitutional constraints:
[`docs/design/mininet-node-appliance.md`](../docs/design/mininet-node-appliance.md).

This is an OS-shaped **deployment profile** for Debian Stable, not an
operating system. Debian owns the kernel, bootloader, hardware support,
security patches, package infrastructure, and base userspace; this
directory is everything Mininet owns on top of that: the package
manifest, filesystem layout, systemd units, security defaults, users,
network policy, and the installer that ties them together.

```
deploy/
├── packages.lock       pinned apt package manifest (declared set; see file header)
├── systemd/            mininet.target + the sync-listen supervisor unit
├── sysctl.d/           minimal kernel-hardening baseline
├── nftables/           default-deny firewall ruleset
├── users/              declarative mininet system user (sysusers.d format)
├── installer/          install.sh: bare Debian Stable -> Mininet node
├── image/              Phase 2 placeholder (not built)
└── verification/       verify.sh: lints the manifests + checks live state
```

## Quick start (disposable VM only — do not run on a machine you care about)

```sh
sudo deploy/installer/install.sh
deploy/verification/verify.sh
```

## What this does not do

Verify or activate a Mininet release (that's `mini-forge::release` +
`mini-installer`, invoked afterward via the `mini` binary this installer
builds and installs), build or sign an image (Phase 2), provide an
immutable/A-B-updating root (Phase 3), or constitute a real operating
system (Phase 4 — a decision gate, not a default). It also does not run
`mini`'s build/release/provenance/installer commands as a background
service: those stay human-invoked, on purpose.
