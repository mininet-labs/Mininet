# The Mininet Node Appliance — an OS-shaped deployment profile, not an OS (D-0446)

**Founder direction (2026-08-06):** do not build a full Debian-based
operating system from day 0. A real distribution immediately creates
permanent, unbounded work (package repositories and signing, security
update coordination, kernel maintenance, installer maintenance, hardware
compatibility testing, image publishing, upgrade/rollback guarantees,
vulnerability response, release engineering) and none of that work proves
that Mininet can host, build, verify, and recover itself. It could consume
the project before the forge spine works. Build an OS-shaped deployment
profile instead: Mininet owns the manifest, layout, and policy; Debian
Stable continues to own the kernel, bootloader, hardware support, security
patches, package infrastructure, and base userspace. This document is that
plan, staged, with an explicit gate for when (if ever) a genuine
distribution becomes justified.

**Naming rule:** this product is called the **Mininet Node Appliance**
(or **Mininet Appliance**) everywhere — in docs, issue titles, commit
messages, and any future branding. It is never called "Mininet OS." That
name is reserved for a decision that has not been made (Phase 4 below),
and using it early would misstate what this is to every reader who
encounters it before that decision exists.

## The ownership split

| Layer | Owner |
|---|---|
| Kernel, bootloader, hardware support, security patches, package infrastructure, base userspace | **Debian** |
| Package manifest, filesystem layout, systemd units, security defaults, users/permissions, networking configuration, update policy, installation state machine, recovery procedure, VM/bare-metal images | **Mininet** |

The installation contract is a composition, not a replacement:

```
Known Debian Stable base
        +
Mininet manifest (deploy/packages.lock, deploy/systemd/, deploy/nftables/, deploy/users/, deploy/sysctl.d/)
        +
Mininet installer (deploy/installer/)
        =
Reproducible Mininet node
```

Rebuilding a node from scratch against this contract must be safer and
faster than hand-repairing a heavily modified machine. If the installer
cannot reliably reach the same end state twice on a clean Debian Stable
box, the contract is broken and nothing downstream (image building,
immutable roots, a real OS) is worth attempting yet.

## Relationship to the existing forge spine

This is a **new, lower layer**, not a replacement for anything that
already ships. Keep the two "installer" concepts distinct:

- **`mini-installer`** (the existing crate, D-0071/self-hosted-forge-spine
  Batch 4) activates an already-verified *Mininet release* on a machine
  that is already running Mininet: `Discovered -> Staged -> Preflight ->
  Active` / `RolledBack`, with a typed `OwnerApproval` and automatic
  rollback on a failed health check. It answers "is this specific signed
  release safe to run here."
- **The appliance installer** (`deploy/installer/`, this document) answers
  a question one layer down: "does a bare Debian Stable machine even have
  the packages, users, filesystem layout, network policy, and systemd
  wiring for `mini-installer` and `mini-cli` to run at all." It provisions
  the *machine*; `mini-installer` then governs the *release* running on
  it. The appliance installer's last step is expected to hand off to the
  existing `mini` binary (`mini build`/`release`/`installer`/`sync`)
  rather than reimplementing any of that logic.

Neither installer may become a second source of release authority. The
appliance installer never verifies or activates a Mininet release itself
— that stays exactly where D-0070/D-0071 already put it
(`mini-forge::release` + `mini-installer`).

## Constitutional constraint carried over from D-0020

D-0020's frozen clause applies directly to appliance images:

> [FREEZE] No canonical distribution point may ever be required: a fresh
> device must be able to obtain, verify, and run the client from a nearby
> peer alone.

Consequence: a signed QCOW2/raw/cloud image or ISO built under Phase 2
below must be obtainable and verifiable the same way a release binary
already is — as a content-addressed object synced peer-to-peer (the same
`mini-sync`/`mini-net` path D-0080 already proved carries the entire
spine over plain TCP with no shared filesystem) — with any
Mininet-operated download mirror treated as one optional convenience
mirror among many, never the only path. No appliance image build step may
introduce a mandatory phone-home, forced-update, or remote-kill mechanism
(D-0011/P3); the appliance's own update policy is bounded by the same
freshness/staleness/provenance-quorum gates `mini-update` already
implements, not a new authority.

## Why a full OS is premature

Listed here because "why not do the ambitious thing" needs a written
answer, not just a founder preference: a real distribution's *permanent*
surface area (repo signing, CVE triage and backport SLAs, kernel
maintenance, hardware enablement, image publishing infrastructure,
upgrade/rollback guarantees across arbitrary prior states) scales with
calendar time regardless of whether anyone is using it, and none of it
advances the actual open question — whether Mininet can host, build,
verify, and recover itself end to end. Every hour spent on Debian's job is
an hour not spent proving the forge spine (already shipped through Batch
5) actually works as a day-to-day operational substrate. The appliance
profile gets the "feels like an OS" outcome — a known, reproducible,
recoverable node — while leaving every one of those permanent burdens
with Debian, who already carries them for the entire world.

## Staged plan

### Day 0 — Debian deployment profile (this decision's scope)

A `deploy/` tree that transforms a supported Debian Stable installation
into a known Mininet node:

```
deploy/
├── packages.lock       — pinned apt package manifest (hashes, not just names)
├── systemd/            — unit files for the Mininet node's OS-level services
├── sysctl.d/           — minimal kernel-hardening baseline
├── nftables/           — default-deny firewall ruleset
├── users/              — dedicated non-root system user/group spec
├── installer/          — idempotent script: bare Debian -> Mininet node
├── image/              — Phase 2 placeholder (not built yet; see below)
└── verification/       — checks the manifest was actually applied correctly
```

**Honest scope for Day 0:** this provisions the *machine*. It does not
attempt kernel hardening beyond a minimal sysctl baseline, does not
attempt SELinux/AppArmor profiles, does not build or sign any image, and
does not run a persistent Mininet daemon — because none exists yet
(`mini sync listen`/`connect` is one connection per invocation, no daemon;
see `mini-cli`'s own module docs). The systemd units in this batch wrap
the CLI entrypoints that exist today and are written to be extended, not
replaced, once a real daemon lands. Claiming more than that here would be
exactly the overclaiming the project's honesty rule forbids.

### Phase 2 — Reproducible appliance image

Signed QCOW2, raw disk, and cloud images, and an installation ISO if
still needed once the deployment profile is proven, built by applying the
Day 0 manifest to a pinned Debian Stable base in a scripted, reproducible
build (same reproducibility bar as the rest of the forge spine — two
independent builders, bit-identical output, before the image is trusted).
Still Debian underneath; branding may say "Mininet Appliance," the
`/etc/debian_version` file does not lie about what it is.

### Phase 3 — Managed immutable system

Only after upgrade, recovery, and rollback have been exercised for real
under Phase 2 does read-only/immutable root, A/B system updates,
declarative configuration, signed system images, and a Mininet-controlled
update channel become worth considering. Attempting this before Phase 2
is proven would be building reliability guarantees on top of an
unreliable base.

### Phase 4 — Distribution decision gate

A genuine from-scratch Mininet OS is justified only when Debian
*materially blocks* a requirement Phase 3 has actually run into, not on
general principle:

- deterministic whole-system builds Debian's own tooling cannot provide,
- unusually strong offline operation requirements,
- transactional updates Debian's package manager cannot express,
- specialized networking Debian cannot carry,
- a trusted-computing boundary tighter than Debian's userspace allows,
- hardware-appliance certification requirements Debian doesn't meet.

Until one of those is a real, encountered blocker — not a hypothetical —
this gate stays closed and the appliance stays Debian-based.

## Reference platform (initial, narrow, on purpose)

Debian Stable · x86-64 first · UEFI · systemd · ext4 · QEMU/KVM as the
reference test environment · bare-metal support only after VM validation.
Every one of these is a scope-narrowing choice, not a permanent
commitment — widening (other architectures, other init systems, other
filesystems) is exactly the kind of decision that gets its own D-number
when a real need appears, not decided speculatively here.

## Honest non-claims

This document does not claim: a package repository, image signing
infrastructure, a CVE response process, hardware compatibility testing
beyond QEMU/KVM, an installer that has run on real hardware, or any
persistent Mininet system service. All of Day 0's artifacts are meant to
be read, audited, and run by a human on a disposable VM before anyone
treats them as anything more than a checked-in manifest.

## Refs

Founder direction 2026-08-06 (this document); D-0020 (sovereignty-first
distribution FREEZE); D-0066/`docs/design/self-hosted-forge-spine.md`
(the forge spine this appliance sits underneath); D-0070/D-0071
(`mini-forge::release` + `mini-installer`, the release-activation layer
this appliance hands off to, never replaces); D-0011/P3 (no forced update,
no remote kill).
