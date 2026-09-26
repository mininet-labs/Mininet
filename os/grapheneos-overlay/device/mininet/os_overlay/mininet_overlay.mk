# Product overlay: adds Mininet as a preinstalled priv-app.
#
# `inherit-product` this file from your device's device.mk (see
# docs/android-os/BUILD.md step 3). Depends on:
#   1. This repo synced at device/mininet/mininet via
#      os/grapheneos-overlay/local_manifest/mininet.xml, which is what
#      makes this directory visible inside the AOSP tree at all (as
#      device/mininet/mininet/os/grapheneos-overlay/device/mininet/os_overlay).
#   2. Mininet.apk already built with Gradle and dropped next to this
#      directory's Android.bp (docs/android-os/BUILD.md step 4) -- Soong
#      imports a prebuilt APK here rather than building app/android's
#      Gradle project directly.

PRODUCT_PACKAGES += \
    Mininet

PRODUCT_COPY_FILES += \
    device/mininet/mininet/os/grapheneos-overlay/device/mininet/os_overlay/etc/permissions/privapp-permissions-org.mininet.app.xml:$(TARGET_COPY_OUT_SYSTEM)/etc/permissions/privapp-permissions-org.mininet.app.xml \
    device/mininet/mininet/os/grapheneos-overlay/device/mininet/os_overlay/etc/default-permissions/default-permissions-org.mininet.app.xml:$(TARGET_COPY_OUT_SYSTEM)/etc/default-permissions/default-permissions-org.mininet.app.xml
