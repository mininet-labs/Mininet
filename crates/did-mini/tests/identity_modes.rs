//! Tests for the identity-mode taxonomy and the base-device role (founder
//! decision, 2026-07-07): neither concept may grant capabilities, votes, or
//! any other governance weight.

use did_mini::{
    verify_delegation, AvailabilityWindow, BaseDeviceRole, BatteryPolicy, Capabilities, Controller,
    IdentityMode, PrivacyMode,
};

#[test]
fn every_identity_mode_never_multiplies_standing() {
    for mode in IdentityMode::ALL {
        assert!(mode.never_multiplies_standing());
        assert!(!mode.describe().is_empty());
    }
}

#[test]
fn anonymous_action_is_the_only_pending_mode() {
    // Honesty convention: only the ZK-dependent mode is unimplemented today.
    for mode in IdentityMode::ALL {
        let expected = mode != IdentityMode::AnonymousAction;
        assert_eq!(mode.implemented(), expected, "{mode:?}");
    }
}

#[test]
fn base_device_role_never_requires_or_implies_capabilities() {
    // A device may declare itself the base device while holding the empty
    // capability set — base-device metadata lives entirely outside
    // `Capabilities` and cannot be used to smuggle in privilege.
    let mut root = Controller::incept_single_from_seeds(&[1u8; 32], &[2u8; 32]).unwrap();
    let device =
        Controller::incept_device_single_from_seeds(&root.did(), &[3u8; 32], &[4u8; 32]).unwrap();
    root.delegate_device(&device.did(), Capabilities::empty())
        .unwrap();

    let caps = verify_delegation(&root.kel(), &device.kel()).unwrap();
    assert_eq!(caps, Capabilities::empty());
    assert!(!caps.contains(Capabilities::VOTE));

    // The role itself is just local, un-signed data — attaching it changes
    // nothing about the delegation that was just verified.
    let _role = BaseDeviceRole::always_on_default();
    let caps_again = verify_delegation(&root.kel(), &device.kel()).unwrap();
    assert_eq!(caps_again, Capabilities::empty());
}

#[test]
fn availability_window_handles_same_day_and_wraparound() {
    let daytime = AvailabilityWindow {
        start_minute: 480,
        end_minute: 1320,
    }; // 08:00–22:00
    assert!(daytime.contains(600)); // 10:00
    assert!(!daytime.contains(60)); // 01:00

    let overnight = AvailabilityWindow {
        start_minute: 1320,
        end_minute: 360,
    }; // 22:00–06:00
    assert!(overnight.contains(1380)); // 23:00
    assert!(overnight.contains(120)); // 02:00
    assert!(!overnight.contains(720)); // 12:00

    assert!(AvailabilityWindow::ALWAYS.contains(0));
    assert!(AvailabilityWindow::ALWAYS.contains(1439));
}

#[test]
fn should_seed_now_respects_the_disable_switch() {
    let mut role = BaseDeviceRole::always_on_default();
    role.seed_on_view_enabled = false;
    assert!(!role.should_seed_now(100, false, 600));
}

#[test]
fn should_seed_now_respects_battery_policy() {
    let mut role = BaseDeviceRole::battery_aware_default();
    role.battery_policy = BatteryPolicy::PauseBelowPercent(30);

    // Plugged in: battery percent is irrelevant.
    assert!(role.should_seed_now(5, false, 600));
    // On battery, above the floor: fine.
    assert!(role.should_seed_now(50, true, 600));
    // On battery, below the floor: paused.
    assert!(!role.should_seed_now(10, true, 600));

    let mut never_on_battery = role;
    never_on_battery.battery_policy = BatteryPolicy::NeverOnBattery;
    assert!(!never_on_battery.should_seed_now(100, true, 600));
    assert!(never_on_battery.should_seed_now(100, false, 600));
}

#[test]
fn should_seed_now_respects_availability_window() {
    let mut role = BaseDeviceRole::always_on_default();
    role.availability_window = AvailabilityWindow {
        start_minute: 480,
        end_minute: 1320,
    };
    assert!(role.should_seed_now(100, false, 600)); // inside window
    assert!(!role.should_seed_now(100, false, 60)); // outside window
}

#[test]
fn privacy_mode_is_a_plain_local_choice() {
    // Just exercising the type: RequestOnly vs Advertise is read by
    // `mini-store` policy, not enforced here — this crate only carries it.
    let role = BaseDeviceRole {
        privacy_mode: PrivacyMode::RequestOnly,
        ..BaseDeviceRole::always_on_default()
    };
    assert_eq!(role.privacy_mode, PrivacyMode::RequestOnly);
}
