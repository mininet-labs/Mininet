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
  crash; and
- the Android manifest requests no network, Bluetooth, location, contacts,
  camera, media, notification, or telemetry permission.

The reducer is deliberately stateless across FFI calls. Its complete input and
output are values, so Kotlin never shares mutable protocol state with Rust and
tests can replay every transition deterministically.

## What does not work yet

- Android Keystore key generation, attestation, signing, or encrypted recovery;
- human-root inception or delegated-device enrollment;
- persistence across process death;
- public profile creation, discovery, follow, feed, or synchronization;
- LAN, BLE, Wi-Fi Direct, relay, background sync, notifications, media, or
  calls;
- iOS bindings or a SwiftUI shell;
- a reproducible or governed APK release; and
- physical-device or emulator verification in this workspace.

The UI stops at `RootCreationReady` and emits `RootCreationPending`. That is an
intentional honesty boundary, not a placeholder success path.

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
2026.06.00, `compileSdk`/`targetSdk` 37, JDK 17, and UniFFI 0.32.0. AGP 9's
built-in Kotlin support is used; the incompatible legacy
`org.jetbrains.kotlin.android` plugin is not applied.

Required locally:

- JDK 17;
- Android SDK platform 37 and build tools 36.0.0;
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
verified locally. The Gradle wrapper, dependency verification metadata,
Android CI job, emulator smoke test, and two-device test remain required before
this can be described as a tested Android build or reproducible APK.

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

## Next implementation slice

The next PR state should add an opaque Android Keystore signer adapter and the
real root-to-device delegation ceremony. Its acceptance test is:

1. create a root through an explicit recovery flow;
2. generate a non-exportable phone device key;
3. delegate only the required device capabilities;
4. restart the process and recover the same public identity without exposing
   key bytes to Kotlin; and
5. revoke the phone from a second enrolled device.

After PR #170 is available, two Android devices can then exercise public
profile exchange and one-button follow over QR/LAN before BLE is introduced.

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
