# Building a GrapheneOS image with Mininet preinstalled

**This is an unreviewed P3 feasibility spike, not the accepted mobile-OS
plan** — read [`DESIGN.md`](DESIGN.md)'s status note first, and see
[`docs/mobile/MOBILE_OS_ROADMAP.md`](../mobile/MOBILE_OS_ROADMAP.md) for
the actual accepted sequencing (M00 device inventory first).

This cannot be done on this Windows checkout, or on any machine without
serious build resources. Plan for:

- A **Linux** build host (GrapheneOS's own build docs require it; macOS/
  Windows are not supported for the OS build itself).
- **~500 GB** free disk (AOSP source + build output; GrapheneOS's own docs
  say 500 GB minimum, more for multiple device targets).
- **64 GB RAM** recommended (16 GB is a realistic floor, expect a much
  slower build).
- **Several hours** for the first build (subsequent incremental builds are
  much faster).
- A **supported device** to flash the result to — GrapheneOS supports a
  specific list of Pixel hardware only
  (https://grapheneos.org/faq#supported-devices). This is not a
  workaround-able limitation: GrapheneOS's hardened verified-boot chain
  depends on that hardware's specific security features.

## 1. Set up the GrapheneOS build environment

Follow GrapheneOS's own build guide exactly, up through "Setting up the
build environment" and "Requesting the source code":
https://grapheneos.org/build

Do **not** run `m target-files-package` or flash anything yet — stop once
you have a synced, unmodified GrapheneOS source tree building on its own.
Confirming a clean GrapheneOS build works first makes it possible to tell
whether a later build failure came from this overlay or from the base OS.

## 2. Layer this repository's overlay in

From the root of your GrapheneOS source checkout (the directory containing
`.repo/`):

First, pick the exact commit of this repository you have reviewed and want
baked into the image — **never `main`**: `main` moves, and a preinstalled app
compiled and signed into an OS image from a moving branch can neither be
reproduced nor audited later (a later `repo sync` would silently pull in
whatever `main` has become by then, and the checked-in manifest's own
`revision="main"` default is only a template — it must be overridden, not
used as-is). Set the reviewed commit once as a shell variable:

```bash
# The exact commit you reviewed, e.g. from `git rev-parse HEAD` on a commit
# you trust, or a signed release tag once mini-forge release tooling covers
# this artifact.
MININET_REVIEWED_SHA=<pinned-commit-sha>

# Fetch the manifest template from that same pinned commit (not `main`), so
# an attacker who lands a malicious commit on `main` after your review
# cannot substitute a different manifest at fetch time.
mkdir -p .repo/local_manifests
curl -o .repo/local_manifests/mininet.xml \
  "https://raw.githubusercontent.com/mininet-labs/mininet/${MININET_REVIEWED_SHA}/os/grapheneos-overlay/local_manifest/mininet.xml"

# Pin the actual checkout to that exact commit too -- the template's
# revision="main" is only a placeholder and must be overridden here.
sed -i "s/revision=\"main\"/revision=\"${MININET_REVIEWED_SHA}\"/" \
  .repo/local_manifests/mininet.xml
grep -q "revision=\"${MININET_REVIEWED_SHA}\"" .repo/local_manifests/mininet.xml || {
  echo "failed to pin manifest to ${MININET_REVIEWED_SHA}; aborting" >&2
  exit 1
}

repo sync device/mininet/mininet
```

This makes the whole `mininet-labs/mininet` repo available inside the AOSP
tree at `device/mininet/mininet` — including `app/android` (the Gradle
project) and `os/grapheneos-overlay/device/mininet/os_overlay` (this
overlay's `Android.bp` and permission XML, now visible to Soong because
it's inside the source tree).

## 3. Wire the overlay into your device's product config

Every GrapheneOS device target has a `device.mk` (e.g.
`device/google/<codename>/device.mk`). Add one line to include this
overlay's package list:

```makefile
$(call inherit-product, device/mininet/mininet/os/grapheneos-overlay/device/mininet/os_overlay/mininet_overlay.mk)
```

This adds `Mininet` to `PRODUCT_PACKAGES` as an unprivileged system app (imported from a
prebuilt APK — see step 4), and installs the revocable default-permission file
from that same overlay directory.

## 4. Build the APK, then the OS image

AOSP's Soong build system does not build Gradle projects directly, so
`app/android` is still built with Gradle first, exactly as for a normal
non-OS install, and the resulting APK is copied in as a prebuilt:

Install JDK 17, Gradle 9.5.0, and the standalone Android SDK command-line
tools first, as specified in [ANDROID_FOUNDATION.md](../mobile/ANDROID_FOUNDATION.md).
AOSP's own bundled Java/tools do not supply these app-build prerequisites.
From the AOSP checkout root, in a shell dedicated to the APK build:

```bash
MININET_AOSP_ROOT="$PWD"
export JAVA_HOME=/absolute/path/to/jdk-17
export ANDROID_HOME=/absolute/path/to/android-sdk
export PATH="$JAVA_HOME/bin:$ANDROID_HOME/cmdline-tools/latest/bin:$PATH"
sdkmanager --licenses
sdkmanager "platforms;android-36" "build-tools;36.0.0" "ndk;28.2.13676358"
export ANDROID_NDK_HOME="$ANDROID_HOME/ndk/28.2.13676358"
java -version
gradle --version # must report 9.5.0 and JVM 17

cd "$MININET_AOSP_ROOT/device/mininet/mininet/app/android"
rustup target add aarch64-linux-android x86_64-linux-android
cargo install cargo-ndk --version 4.1.2 --locked
./scripts/build-rust.sh release
gradle :app:assembleRelease
cp app/build/outputs/apk/release/app-release-unsigned.apk \
   ../../os/grapheneos-overlay/device/mininet/os_overlay/Mininet.apk
```

Use a fresh shell for the OS build so the APK's JDK/PATH overrides do not
replace the OS branch's toolchain. Return to the AOSP checkout root:

```bash
source build/envsetup.sh
lunch <product>-<release_config>-user
# Example only: shiba-cur-user, if the selected OS branch exposes it.
# Use that branch's documented lunch choices; the release config is required.
m target-files-package
```

Confirm Mininet is installed under `system/app/Mininet/`, without
privileged permissions or shared platform identity. This initial build uses
the explicit development `testkey` certificate from `Android.bp`.
It is not a release-signed artifact. Preserve the target-files archive for
the separate offline release signing stage below.

## 5. Sign, flash, and verify

Follow GrapheneOS's own signing and flashing instructions
(https://grapheneos.org/build#signing-builds and
https://grapheneos.org/install/cli) exactly — this overlay does not change
GrapheneOS's release/verified-boot signing process. Configure the offline
signing stage to replace Mininet's development certificate with the intended
release APK certificate; verify the final APK signer and AVB chain before
flashing. Never distribute the development-signed build as a release.
This spike has not been built, flashed, or hardware-validated. After flashing:

1. Confirm **Settings → Apps → Mininet** shows it as a system app (no
   uninstall option, only "disable" — that's the expected preinstalled system-app
   behavior).
2. Verify the three Bluetooth permissions were actually pre-granted —
   **do not** rely on the absence of a permission dialog on first launch as
   proof: `app/android` has no runtime-permission request flow of its own
   yet, and `BleMeshService` is not wired into the UI, so no dialog would
   appear either way regardless of whether the allowlist took effect.
   Check each permission directly instead:

   ```bash
   for p in BLUETOOTH_SCAN BLUETOOTH_ADVERTISE BLUETOOTH_CONNECT; do
     adb shell pm check-permission "android.permission.$p" org.mininet.app
   done
   ```

   Each should print `Permission granted` when the OS applied the
   default grants. If denied, inspect the installed `etc/default-permissions`
   XML, package name, manifest and the selected OS's grant policy. These
   runtime permissions do not require privileged app status or a privapp
   allowlist. Verify the user can revoke each grant in Settings and that
   the app handles denial.
3. From here, testing Mininet itself (pairing, mesh, identity) is
   identical to testing the plain `app/android` build — nothing about its
   own behavior changed, only its install class.

## Troubleshooting

- **`Mininet` package not found during `m target-files-package`**: confirm
  `device/mininet/mininet` actually synced (`ls device/mininet/mininet/app/android/build.gradle.kts`
  should exist) and that `Mininet.apk` was actually copied into
  `device/mininet/mininet/os/grapheneos-overlay/device/mininet/os_overlay/`
  before running `m` — `android_app_import` fails the build (not silently
  skips) if the `apk:` path in `Android.bp` doesn't exist. Also confirm
  step 3's `inherit-product` line was added to the *device* you actually
  `lunch`ed, not a different target.
- **App installs but crashes on launch**: check `adb logcat` for
  `UnsatisfiedLinkError` on `libmini_ffi.so` first — this means step 4's
  `build-rust.sh release` was skipped, or its output wasn't picked up by
  `gradle :app:assembleRelease`, so the APK has no native library at all
  for the device's ABI.
- **Bluetooth permission denied**: check the installed default-permissions
  file and selected OS policy, then test explicit user grants and revocation.
  Do not add platform privileges to bypass a runtime permission problem.
