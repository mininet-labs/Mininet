//! Base device role (founder decision, 2026-07-07): the user's recommended
//! main/static device for hosting, storage, and seeding.
//!
//! ## Operational infrastructure, not political power [FREEZE]
//!
//! A [`BaseDeviceRole`] is metadata a device — already delegated through
//! [`crate::delegation`] like any other device — declares about itself. It is
//! deliberately **not** a [`crate::Capabilities`] bit and cannot become one:
//! nothing about running a base device changes what its device may sign, and
//! nothing about committing storage, bandwidth, or uptime changes the
//! human-root's vote. Storage and seeding earn value (see `mini-reward`),
//! never voice (P1).
//!
//! Everyone is *recommended* to pick one base device for hosting, storage,
//! seeding, and participation — this is advisory only. Nothing here enforces
//! "exactly one"; a human may run zero or many, and no code path treats a
//! base device as more authoritative than any other delegated device.

/// How a base device throttles work while running on battery power.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum BatteryPolicy {
    /// No battery constraint (mains-powered / always-on hardware).
    Unconstrained,
    /// Pause background storage/seeding work below this battery percentage
    /// (0..=100) while on battery power.
    PauseBelowPercent(u8),
    /// Never perform background storage/seeding work while on battery power.
    NeverOnBattery,
}

/// How much a base device reveals about what it is willing to serve.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum PrivacyMode {
    /// Advertise availability for content this device holds/seeds.
    Advertise,
    /// Serve on direct request only; never advertise or announce holdings.
    RequestOnly,
}

/// A time-of-day availability window, in minutes since local midnight
/// (`0..1440`). `start <= end` is a same-day window; `start > end` wraps
/// past midnight (e.g. 22:00–06:00 is `start_minute: 1320, end_minute: 360`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AvailabilityWindow {
    /// Window start, in minutes since local midnight (0..1440).
    pub start_minute: u16,
    /// Window end, in minutes since local midnight (0..1440).
    pub end_minute: u16,
}

impl AvailabilityWindow {
    /// Always available.
    pub const ALWAYS: AvailabilityWindow = AvailabilityWindow {
        start_minute: 0,
        end_minute: 1440,
    };

    /// Whether `minute_of_day` (0..1440) falls inside this window.
    pub fn contains(&self, minute_of_day: u16) -> bool {
        let m = minute_of_day.min(1439);
        let start = self.start_minute.min(1439);
        let end = self.end_minute.min(1440);
        if start <= end {
            m >= start && m < end
        } else {
            m >= start || m < end
        }
    }
}

/// A device's self-declared role as the user's base/anchor device.
///
/// Every field is operational (what the device is willing to do), never
/// political (nothing here is counted toward governance).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BaseDeviceRole {
    /// Bytes of local storage this device commits to Mininet content.
    pub storage_commitment_bytes: u64,
    /// Whether this device relays traffic for other peers.
    pub relay_enabled: bool,
    /// Whether viewing content on this device should naturally help seed it
    /// (see the `mini-store` cache tiers), subject to the policy below.
    pub seed_on_view_enabled: bool,
    /// Time-of-day window in which this device does background storage work.
    pub availability_window: AvailabilityWindow,
    /// Upload bandwidth ceiling for seeding, in bytes/sec. `None` = no cap.
    pub bandwidth_limit_bytes_per_sec: Option<u64>,
    /// Battery throttling policy.
    pub battery_policy: BatteryPolicy,
    /// How much this device reveals about its holdings.
    pub privacy_mode: PrivacyMode,
}

impl BaseDeviceRole {
    /// A conservative always-on default: mains-powered box, seeds on view,
    /// no bandwidth cap, advertises availability.
    pub fn always_on_default() -> Self {
        BaseDeviceRole {
            storage_commitment_bytes: 0,
            relay_enabled: true,
            seed_on_view_enabled: true,
            availability_window: AvailabilityWindow::ALWAYS,
            bandwidth_limit_bytes_per_sec: None,
            battery_policy: BatteryPolicy::Unconstrained,
            privacy_mode: PrivacyMode::Advertise,
        }
    }

    /// A conservative default for a phone acting as its owner's base device:
    /// seeds on view but pauses under 30% battery, and never relays.
    pub fn battery_aware_default() -> Self {
        BaseDeviceRole {
            storage_commitment_bytes: 0,
            relay_enabled: false,
            seed_on_view_enabled: true,
            availability_window: AvailabilityWindow::ALWAYS,
            bandwidth_limit_bytes_per_sec: None,
            battery_policy: BatteryPolicy::PauseBelowPercent(30),
            privacy_mode: PrivacyMode::Advertise,
        }
    }

    /// Whether this base device should currently perform background
    /// storage/seeding work, given live device conditions. Pure and total —
    /// no I/O, no clock, no global state.
    pub fn should_seed_now(
        &self,
        battery_percent: u8,
        on_battery: bool,
        minute_of_day: u16,
    ) -> bool {
        if !self.seed_on_view_enabled {
            return false;
        }
        if !self.availability_window.contains(minute_of_day) {
            return false;
        }
        match self.battery_policy {
            BatteryPolicy::Unconstrained => true,
            BatteryPolicy::NeverOnBattery => !on_battery,
            BatteryPolicy::PauseBelowPercent(floor) => !on_battery || battery_percent >= floor,
        }
    }
}
