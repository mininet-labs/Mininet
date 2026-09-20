# Mininet on a hardened Android OS: integration design

**Status: unreviewed P3 platform-feasibility spike, not an accepted decision.**
[`docs/mobile/MOBILE_OS_ROADMAP.md`](../mobile/MOBILE_OS_ROADMAP.md) (the
accepted mobile-OS roadmap) explicitly reserves the OS source baseline,
hardware target, and support commitment as future decisions requiring
review — GrapheneOS below is one *candidate* baseline this document
evaluates, not a chosen one. The roadmap also states its P3 (reference
image) work depends on P0–P2 (device inventory, custody/recovery,
lifecycle, real mesh — see the roadmap's M00–M08) landing first, with one
named exception: *"The P3 feasibility spike can run alongside client
work, but cannot consume the work needed to make the network usable."*
This document and its accompanying `os/grapheneos-overlay/` overlay are
exactly that early, parallel P3 spike — evaluating whether a GrapheneOS
base is even feasible — not a substitute for M00's exact-revision
two-phone baseline or a claim that the OS work is scheduled ahead of it.
If GrapheneOS is later accepted as the baseline, this spike's output
becomes the seed for the roadmap's M09 ("reference image spike"), likely
relocated under a path like `platform/android/` per that work package's
own naming; until then it stays here, clearly labeled as unadopted.

## Goal

Ship Mininet as a preinstalled, preconfigured system app on a
privacy/security-hardened Android OS, rather than as a normal Play
Store-style install. "Preinstalled" here specifically means: the existing
[`app/android`](../../app/android) app, built as a **privileged system app**
(`priv-app`) baked into the OS image, auto-granted the runtime permissions it
needs, and optionally auto-started, instead of a user having to find and
sideload it.

This is deliberately scoped as **OS integration of the existing app**, not a
new app. `app/android` and `crates/mini-ffi` do not change; only where and how
the resulting APK is installed changes.

## Why fork, not build AOSP from scratch

Building and hardening an AOSP fork from zero (verified boot chain, sane
default permission model, no Google Play Services, security patch cadence)
is years of ongoing security engineering that
[GrapheneOS](https://grapheneos.org) and
[CalyxOS](https://calyxos.org) already do, continuously, and battle-tested
against real threat models. Re-deriving that from scratch would mean Mininet
inheriting *our* mistakes in a domain neither this project's crates nor its
existing contributors have expertise in.

**This spike evaluates GrapheneOS as the leading candidate**, without
deciding it — the roadmap proposal itself names GrapheneOS only as "a
comparator, not an endorsement, affiliation, or chosen base"
([`MOBILE_OS_PROPOSAL.md`](../mobile/MOBILE_OS_PROPOSAL.md) §9). It's
attractive as a candidate because it already ships:
- A minimal, hardened `system_server` and kernel config, with a much smaller
  attack surface than stock AOSP.
- No baked-in Google Play Services (avoids exactly the kind of default
  telemetry/tracking surface Mininet's own no-analytics stance argues
  against — see `app/android`'s manifest comment on this).
- Per-app network/sensor permission toggles beyond stock Android
  (Contacts, Storage, sensors) that only make Mininet's own
  privilege-minimal design story stronger.
- A documented, scriptable, reproducible build process
  (https://grapheneos.org/build), which this design's `BUILD.md` builds on
  directly instead of reinventing.

CalyxOS is a reasonable alternative (also AOSP-derived, also no Play
Services) if a different hardware support matrix or governance model is
preferred later; the overlay described below does not depend on which of
the two is chosen — only `BUILD.md`'s manifest URL changes.

## What "integration" means concretely

1. **Priv-app install, not user install.** `app/android`'s APK is added as a
   product package in the OS build (`PRODUCT_PACKAGES`) and placed under
   `/system/priv-app/Mininet/` in the resulting image, the same install
   class as the OS's own Settings or Dialer app — not `/data/app` where a
   normal user-installed APK lives.
2. **Pre-granted runtime permissions.** A `privapp-permissions` allowlist
   entry (Android requires every priv-app's sensitive permissions to be
   explicitly allowlisted by the OS build, or the app silently loses them)
   plus a `default-permissions` grant so Bluetooth/mesh access does not
   require a first-run permission dialog — see
   [`os/grapheneos-overlay`](../../os/grapheneos-overlay) for the exact
   XML.
3. **No other OS behavior changes yet.** This design does *not* move any of
   Mininet's logic into `system_server`, does not add a new Binder service,
   and does not touch boot (`init.rc`). Those are real future steps (see
   "Deliberately out of scope" below) but each is a separate, independently
   risky change that deserves its own review — bundling them here would
   make this first integration unreviewable.

## Repository layout

```
docs/android-os/
  DESIGN.md   <- this file
  BUILD.md    <- step-by-step build instructions (needs a Linux build host)
os/grapheneos-overlay/
  local_manifest/mininet.xml         <- repo-tool local manifest snippet
  device/mininet/os_overlay/
    mininet_overlay.mk               <- product makefile: adds Mininet as PRODUCT_PACKAGES
    Android.bp                       <- imports the separately Gradle-built APK as a priv-app module
    Mininet.apk                      <- NOT committed; built locally per BUILD.md step 4
    etc/permissions/privapp-permissions-org.mininet.app.xml
    etc/sysconfig/default-permissions-org.mininet.app.xml
```

`os/grapheneos-overlay` is *not* a full device tree or a GrapheneOS source
checkout — cloning either into this repo would be tens of GB and does not
belong in `mininet-labs/mininet`'s own history. It is the small set of files
that get **layered on top of** a GrapheneOS checkout via `repo`'s
local-manifest mechanism, on a separate Linux build machine, per `BUILD.md`.

## Threat-model notes

- Priv-app status is itself a trust escalation: a priv-app can be granted
  permissions a normal app cannot, without a runtime prompt. The
  allowlist in this overlay grants exactly the permissions
  `app/android`'s own `AndroidManifest.xml` already declares
  (`BLUETOOTH_SCAN`/`ADVERTISE`/`CONNECT`, `INTERNET`) — nothing broader.
  Any future permission added to the app must be a conscious, reviewed
  addition to both files, not just the manifest.
- This does not change Mininet's own cryptographic trust model
  (`did-mini`, `RootCore`, KEL history) at all — it only changes how the
  APK reaches the device and what permission prompts the user sees (none,
  for the allowlisted set) on first boot.

## Deliberately out of scope (future work, not this change)

- A native Mininet **system service** running in `system_server` or as a
  standalone Binder service, so other preinstalled apps can talk to
  Mininet without going through `app/android`'s own UI process. This is
  the natural next step toward "connected to by almost any other device"
  as a genuine OS primitive, but it's a much larger, security-sensitive
  change (new IPC surface, new SELinux domain) that deserves its own
  design doc and its own review, once this first integration has actually
  been built and tested by someone with a real GrapheneOS build
  environment.
- Auto-start at boot (`init.rc` service entry) — currently the app starts
  like any other launcher icon; auto-start needs its own justification
  (battery/behavior expectations) before being added.
- Custom recovery/OTA signing, device-specific kernel changes, or
  supported-hardware matrix decisions. Those follow GrapheneOS's own
  per-device support list unchanged.
