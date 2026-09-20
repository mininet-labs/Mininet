# GrapheneOS overlay

Files layered onto a [GrapheneOS](https://grapheneos.org) source tree to
build Mininet in as a preinstalled priv-app. This is not a device tree or a
GrapheneOS checkout itself — see [`docs/android-os/DESIGN.md`](../../docs/android-os/DESIGN.md)
for why, and [`docs/android-os/BUILD.md`](../../docs/android-os/BUILD.md)
for how to actually use these files on a real Linux build host.

| Path | Purpose |
| --- | --- |
| `local_manifest/mininet.xml` | `repo` local manifest: syncs this repo into the AOSP tree. |
| `device/mininet/os_overlay/mininet_overlay.mk` | Product makefile: adds `Mininet` to `PRODUCT_PACKAGES`. |
| `device/mininet/os_overlay/Android.bp` | Imports the Gradle-built `Mininet.apk` as a priv-app Soong module. |
| `device/mininet/os_overlay/etc/permissions/` | Priv-app permission allowlist (mirrors `app/android`'s manifest). |
| `device/mininet/os_overlay/etc/sysconfig/` | Default-grants the same permissions so first boot has no prompt. |
