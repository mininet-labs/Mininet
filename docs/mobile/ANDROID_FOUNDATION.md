# Android foundation

**Maturity:** prototype integration foundation

**Tracking:** issue #178, draft PR #179

**Architecture:** D-0020, `docs/UI_BETA_PLAN.md` E1
**Dependency for the first social device flow:** draft PR #170

This slice starts Mininet's Android-first client without creating a second
implementation of Mininet in Kotlin. `mini-ffi` owns a typed, versioned,
deterministic command/event contract. The Compose application renders that
contract and reports platform capabilities. No protocol secret crosses FFI.

## What works in this slice

- the UniFFI UDL generates Kotlin bindings in the Android build;
- the Rust core returns an initial onboarding snapshot;
- Compose renders welcome, root-safety, and device-readiness screens;
- version mismatch, malformed request labels, contradictory platform-security
  claims, caller-forged security snapshots, overflow, and invalid transitions
  fail closed;
- missing native libraries produce a visible setup screen instead of a silent
  crash;
- the Android manifest requests no network, Bluetooth, location, contacts,
  camera, media, notification, or telemetry permission; and
- **`RootCore` (D-0335, issue #197 slice)**: a real root identity can be
  created, a device can be delegated under it with the default capability
  set, and a device can be revoked — all in-process, in memory, using
  ordinary `mini_crypto::SigningKey`s exactly like every other identity in
  this workspace today; and
- **`OperationLifecycle` (D-0348, issue #202 slice)**: a typed state
  machine tracking whether a backgroundable LAN/QR pairing exchange or BLE
  bearer transfer is currently safe to suspend. `InFlight` always answers
  `MustCompleteOrFailClosed` to a suspend request; only a caller-reported
  `AtCheckpoint` transitions cleanly to `Suspended`/resumable. A failure
  is always recorded as a typed, visible `LifecycleFailureReason`, never a
  silent partial/corrupt result; and
- **the Compose UI actually calls `RootCore`** (D-0351): tapping "Create
  root" at `RootCreationReady` now really creates a root and delegates a
  first device, and shows both DIDs — not a stub notice. The reducer
  itself still never creates identity (`dispatch` always answers
  `RootCreationPending` at that stage by design); the UI calls `RootCore`
  directly instead of routing through it.

The onboarding reducer (`start`/`dispatch`) is deliberately stateless across
FFI calls. Its complete input and output are values, so Kotlin never shares
mutable protocol state with Rust and tests can replay every transition
deterministically. `RootCore` is a separate, additive UniFFI interface with
its own in-process state, instantiated once `dispatch` reaches
`RootCreationReady`; it never leaks a mutable Rust reference into a value
that crosses the FFI boundary.

## What does not work yet

- **Android Keystore key generation, attestation, or hardware-backed
  signing** — `RootCore`'s keys are software-only; D-0334's design doc names
  three options for genuine hardware backing, none chosen yet;
- **persistence across process death** — closing the app loses the root and
  every delegated device `RootCore` created; the UI now really creates
  them (D-0351), it just can't keep them past a restart yet, since no
  `StorageCipher`/Android Keystore-backed encrypted storage adapter
  exists (issue #198's actual acceptance test);
- **restart-and-recover** (acceptance-test step 4) — depends directly on
  persistence above;
- **root and device on separate physical devices** — this MVP holds both in
  one process for dev-testing convenience; the real split happens once LAN/QR
  pairing (issue #200) exists;
- **a real foreground `Service`/`WorkManager` wired to `OperationLifecycle`**
  — this slice (issue #202) only ships the typed Rust-side state machine
  the Kotlin lifecycle glue must query; no Android `Service` declaration,
  Doze/App Standby handling, or a real backgrounded-device test exists yet
  (Codex/the founder's local machine, per this slice's own division of
  labor);
- **a signed, persisted `mini-provenance` record for the Android build**
  (D-0349, issue #205) — `android-reproducibility.yml` computes the exact
  environment/commands/output digests `mini provenance record` needs and
  prints them, but does not call it: that requires a durable CI signing
  identity and a persistent store across ephemeral runners, both real
  policy decisions for the founder to make, not something invented here;
- public profile creation, discovery, follow, feed, or synchronization;
- LAN, BLE, Wi-Fi Direct, relay, background sync, notifications, media, or
  calls;
- iOS bindings or a SwiftUI shell;
- a reproducible or governed APK release; and
- physical-device or emulator verification in this workspace (no JDK/SDK/
  NDK/Gradle/emulator here — Codex/the founder's local machine is required
  for that).

The UI now creates a real root and delegates a first device when the user
taps "Create root" at `RootCreationReady` (D-0351) — `RootCore`'s Rust-side
contract is called for real, not just proven in isolation. What's left
before this can be called a working golden path: encrypted on-device
persistence (issue #198) so that identity survives a restart, and a real
emulator/device compile-and-click-through verification (Codex/the
founder's local machine — no JDK/SDK/NDK/Gradle/emulator exists here).

## Security boundary

The Android shell currently reports that Android application key storage is
available but never reports hardware backing. Hardware backing must be proven
by the future Keystore adapter from the generated key's security properties;
it must not be inferred from Android version, handset model, biometrics, or a
vendor claim.

The eventual custody model is:

1. the human root controls delegation and recovery;
2. each phone holds a separate revocable device key;
3. ordinary app operations use the delegated device, not the root;
4. backup is an explicit encrypted recovery ceremony, never Android Auto
   Backup, cloud synchronization, or an implicit PC key copy; and
5. private-key bytes remain inside the custody adapter and are never returned
   through UniFFI.

`android:allowBackup="false"` is set now so a future developer cannot
accidentally inherit an unsafe platform backup default before the explicit
recovery design lands.

## Toolchain and local build

The Android project pins AGP 9.3.0, Kotlin/Compose compiler 2.3.21, Compose BOM
2026.06.00, `compileSdk`/`targetSdk` 36, JDK 17, and UniFFI 0.32.0. AGP 9's
built-in Kotlin support is used; the incompatible legacy
`org.jetbrains.kotlin.android` plugin is not applied.

`compileSdk`/`targetSdk` 36 (D-0344) rather than 37: platform 37 does not
exist as a plain integer in the real Android SDK repository — verified by
`.github/workflows/android-ci.yml`'s first real CI run and a follow-up
`sdkmanager --list` — only versioned sub-releases (`platforms;android-37.0`,
`37.1`) exist there, a naming scheme AGP's plain-integer `compileSdk`/
`targetSdk` properties were not confirmed compatible with. 36 is the
highest plain-integer platform actually published.

Required locally:

- JDK 17;
- Android SDK platform 36 and build tools 36.0.0;
- Android NDK 28.2.13676358;
- Gradle 9.5.0 or a compatible Android Studio installation;
- Rust targets `aarch64-linux-android` and `x86_64-linux-android`; and
- `cargo-ndk` 4.1.2 installed with `--locked`.

Build the Rust libraries from the repository root:

```powershell
rustup target add aarch64-linux-android x86_64-linux-android
cargo install cargo-ndk --version 4.1.2 --locked
app\android\scripts\build-rust.ps1 debug
```

Then build the application with an installed Gradle 9.5.0:

```powershell
gradle -p app\android :app:assembleDebug
```

The Android `preBuild` task regenerates Kotlin from the reviewed UDL. Generated
bindings and native `.so` files are build artifacts and are not committed.

This machine currently has no JDK, Android SDK, NDK, Gradle, emulator, or
physical device attached, so only the Rust tests and binding generation are
verified locally. `.github/workflows/android-ci.yml` (D-0343, issue #204)
now assembles the debug APK for real on a GitHub-hosted runner, and
`.github/workflows/android-reproducibility.yml` (D-0349, issue #205) checks
two independent clean builds hash-identically; the emulator smoke test and
two-device test remain required before this can be described as a fully
tested Android build.

## Verification performed

```text
cargo test -p mini-ffi
  9 passed

cargo clippy -p mini-ffi --all-targets --all-features -- -D warnings
  passed

uniffi-bindgen generate ... --language kotlin
  generated org/mininet/core/mini_ffi.kt
```

The tests include a 10,000-call deterministic reducer run and an adversarial
attempt to strengthen a caller-constructed security snapshot. This exercises
the command boundary; it is not a memory-safety audit, Android lifecycle test,
or device security proof.

## Beta roadmap (target: ~PR #200, hub issue #196)

The founder's aim is an Android beta release, full Rust test suite green,
targeted around PR #200 as an approximate milestone — other concurrent lanes
in this repo also consume PR numbers, so the exact number will drift; the aim
is the sequencing, not the digit. Hub issue #196 tracks this list; slices 1-9
each have their own filed issue (#197-#205) and get their own draft PR once
work actually starts on them, not an empty shell opened ahead of time.

**Division of labor**, now that the founder has confirmed Codex can run
Android emulators locally and can supply a few physical devices: this
environment (no JDK/SDK/NDK/Gradle/emulator) implements and verifies the
Rust-side crate logic, tests, docs, and decision-log entries; Codex/the
founder's local machine runs Gradle sync, APK assembly, emulator/device
tests, Android lint, and `cargo deny check` — anything that actually needs
the Android toolchain. Every slice's PR states plainly which half is done and
which half still needs that local verification.

1. **#197 — Android Keystore signer adapter + root-to-device delegation
   ceremony.** An opaque Android Keystore signer adapter and the real
   root-to-device delegation ceremony. Acceptance test:
   1. create a root through an explicit recovery flow;
   2. generate a non-exportable phone device key;
   3. delegate only the required device capabilities;
   4. restart the process and recover the same public identity without
      exposing key bytes to Kotlin; and
   5. revoke the phone from a second enrolled device.
2. **#198 — Persisted app state across process death.** Encrypted local
   state so onboarding/delegation state survives a restart without
   re-running the ceremony; a corrupted/forged persisted snapshot fails
   closed. Depends on 1.
3. **#199 — Device enrollment/revocation multi-device flow.** A second
   device enrolls against an existing root; the root revokes a phone.
   Depends on 1-2.
4. **#200 — LAN/QR pairing exchange.** Profile exchange and one-button
   follow over QR/LAN. Depends on draft PR #170 (public profile/follow) and
   on 2-3.
5. **#201 — BLE bearer integration for Android.** Wires the existing
   `mini-bearer` BLE-first bootstrap into the Android transport layer.
   Depends on 4; gated on real hardware.
6. **#202 — Background lifecycle policy.** Foreground-service/battery-
   constraint handling so sync behaves correctly when backgrounded. Depends
   on 4-5.
7. **#203 — Dependency verification.** Gradle dependency-verification
   metadata plus `cargo-deny`/`cargo vet` wiring for the Android build. No
   hard dependency on the other slices.
8. **#204 — Android CI.** A real GitHub Actions job that assembles the APK
   on a GitHub-hosted runner's real JDK/Android SDK/NDK — the first point
   "does it build" gets a real automated answer instead of a local claim.
9. **#205 — Reproducible APK proof.** Two clean builds, hash comparison,
   and a `mini-provenance` record for the Android artifact, mirroring the
   existing `reproducibility.yml` discipline. Depends on 8.
10. **Gate, not a PR: external security review (D-0047).** Beta is
    explicitly pre-review; no production/custody claim is made before this
    gate clears.

## Primary implementation references

- UniFFI user guide: <https://mozilla.github.io/uniffi-rs/latest/>
- UniFFI Android/Gradle integration:
  <https://mozilla.github.io/uniffi-rs/latest/kotlin/gradle.html>
- Android built-in Kotlin migration:
  <https://developer.android.com/build/migrate-to-built-in-kotlin>
- Android Gradle plugin compatibility:
  <https://developer.android.com/build/releases/agp-9-3-0-release-notes>
- Compose compiler setup:
  <https://developer.android.com/develop/ui/compose/setup-compose-dependencies-and-compiler>
