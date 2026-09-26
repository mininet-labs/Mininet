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
baked into the image — **never `main`**: `main` moves, and a priv-app
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

This adds `Mininet` to `PRODUCT_PACKAGES` as a priv-app (imported from a
prebuilt APK — see step 4), and installs the permission allowlist files
from that same overlay directory.

## 4. Build the APK, then the OS image

AOSP's Soong build system does not build Gradle projects directly, so
`app/android` is still built with Gradle first, exactly as for a normal
non-OS install, and the resulting APK is copied in as a prebuilt:

```bash
# From device/mininet/mininet (this repo, now inside the AOSP tree):
cd device/mininet/mininet/app/android

# mini-ffi's native library still needs cross-compiling for
# arm64-v8a/x86_64 first, same as any normal build of this app.
rustup target add aarch64-linux-android x86_64-linux-android
command -v cargo-ndk >/dev/null || cargo install cargo-ndk --version 4.1.2 --locked
./scripts/build-rust.sh release

# app/android has no Gradle wrapper committed; use an installed Gradle
# 9.5.0, the same way android-ci.yml and ANDROID_FOUNDATION.md do.
gradle :app:assembleRelease
cp app/build/outputs/apk/release/app-release-unsigned.apk \
   ../../os/grapheneos-overlay/device/mininet/os_overlay/Mininet.apk

# Back to the AOSP tree root:
cd -
source build/envsetup.sh
lunch <device_target>-user   # e.g. shiba-user for a Pixel 8; see GrapheneOS docs
m target-files-package
```

Watch the build log for `Mininet` — confirm it gets imported and installed
to `system/priv-app/Mininet/`. `android_app_import`'s `presigned: false`
(set in this overlay's `Android.bp`) means the OS build's own release key
re-signs the APK during `m target-files-package` — you do not need to sign
`Mininet.apk` yourself before this step, only build it.

## 5. Sign, flash, and verify

Follow GrapheneOS's own signing and flashing instructions
(https://grapheneos.org/build#signing-builds and
https://grapheneos.org/install/cli) exactly — this overlay does not change
GrapheneOS's release/verified-boot signing process at all. After flashing:

1. Confirm **Settings → Apps → Mininet** shows it as a system app (no
   uninstall option, only "disable" — that's the expected priv-app
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

   Each should print `Permission granted`. If any prints `Permission
   denied`, the `privapp-permissions`/`default-permissions` XML did not get
   picked up; check `adb logcat | grep -i privapp` for the specific denial
   reason (Android logs exactly which permission failed allowlist
   verification).
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
- **Permission still prompts at runtime**: priv-app permission allowlisting
  is strict about the exact package name and permission string matching;
  diff `os/grapheneos-overlay/device/mininet/os_overlay/etc/permissions/privapp-permissions-org.mininet.app.xml`
  against `app/android/app/src/main/AndroidManifest.xml`'s current
  `<uses-permission>` list — if the manifest gained a permission this
  overlay predates, the allowlist needs the same addition.
