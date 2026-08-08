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
├── packages.lock       apt requirement set (names; see the file header)
├── systemd/            mininet.target, the sync-listen supervisor, the verify timer
├── sysctl.d/           minimal kernel-hardening baseline
├── nftables/           default-deny firewall ruleset
├── journald/           log retention, set short for privacy
├── users/              declarative mininet system user (sysusers.d format)
├── installer/          preflight.sh -> install.sh -> uninstall.sh
├── backup/             backup.sh / restore.sh — the node's identity
├── image/              Phase 2 placeholder (not built)
└── verification/       verify.sh (lint + live state), lock-packages.sh (resolve pins)
```

Supported: Debian Stable on **amd64 or arm64**. arm64 is not an
afterthought — the cheap machine most people can actually buy is a
single-board ARM computer, and a profile that only ran on x86-64 servers
would serve exactly the operators the design exists not to privilege
(Directive 11).

## Quick start (disposable VM only — do not run on a machine you care about)

```sh
deploy/installer/preflight.sh          # changes nothing; says if this box will work
sudo deploy/installer/install.sh
deploy/verification/verify.sh
sudo deploy/backup/backup.sh /media/somewhere-else   # do not skip this
```

Step-by-step instructions for a real operator, including what each file
touches and why, are in
[`docs/guides/node-operator-guide.md`](../docs/guides/node-operator-guide.md).

## Back up before you have anything to lose

`/var/lib/mininet` holds the node's `did:mini` identity. Mininet has **no
custodial recovery anywhere in it** (ID1) — not by an operator, not by the
founder, not by any quorum — because a system that can restore your keys
is a system somebody can be compelled to use against you. The consequence
is blunt: a dead disk without a backup is a permanently lost identity, and
cheap hardware is exactly the hardware whose storage fails.

`deploy/backup/backup.sh` writes a passphrase-encrypted archive locally and
uploads it nowhere. Where it goes afterward is the operator's decision, and
`restore.sh` refuses to overwrite live state without an explicit
confirmation — restoring onto a second machine while the first still runs
gives two machines one identity, which is equivocation and looks like an
attack to everyone else.

## What this does not do

Verify or activate a Mininet release (that's `mini-forge::release` +
`mini-installer`, invoked afterward via the `mini` binary this installer
builds and installs), build or sign an image (Phase 2), provide an
immutable/A-B-updating root (Phase 3), or constitute a real operating
system (Phase 4 — a decision gate, not a default). It also does not run
`mini`'s build/release/provenance/installer commands as a background
service: those stay human-invoked, on purpose.

It does not repair itself, either. `mininet-verify.timer` runs the
verification daily and **reports without fixing anything** — infrastructure
that silently reapplies its own configuration is infrastructure that
overrides deliberate operator changes, which is the wrong default on a
machine somebody owns (Directive 1).

And it does not update the operating system. Debian's security updates are
Debian's, on the operator's schedule; Mininet's own updates go through the
governed release path (`mini release verify` -> `mini installer stage/
preflight/activate`), which requires an explicit approval naming the exact
release and rolls back on a failed health check (U1, D-0071). A protocol
that could push updates to your machine would be a protocol with an off
switch.

## Verifying this directory without installing anything

```sh
deploy/verification/verify.sh
```

Lints every unit with `systemd-analyze verify`, the ruleset with `nft
--check`, and every script with `shellcheck`; confirms the scripts are
executable and the journald policy actually declares a retention bound;
then best-effort checks live state if this host was provisioned. Needs no
root and no prior install.
