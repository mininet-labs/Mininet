//! Opt-in contribution budgets (research doctrine §7, Track C2): "Public
//! users may voluntarily contribute bounded local resources. Free
//! participation must not require unlimited storage, bandwidth, battery,
//! CPU, mobile data, or continuous availability. Contribution limits must
//! be visible, configurable, and revocable."
//!
//! [`ContributionBudget`] is the typed opposite of the free commons rights
//! in [`crate::PublicCommonsPolicy`]: those are things every identity root
//! may *consume* for free; this is what an identity root may *voluntarily
//! give back* (storage, bandwidth, CPU, battery, network, background
//! operation), always bounded and always revocable. It grants no protocol
//! authority, ranking boost, or governance weight for contributing --
//! consuming that would collapse the same money/voice wall
//! [`crate::PublicCommonsPolicy`] exists to keep intact.

/// A single resource contribution: `None` means "not opted in" (the
/// default for every field). An identity root that never touches this type
/// contributes nothing and loses no entitlement in [`crate::PublicCommonsPolicy`]
/// for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ContributionBudget {
    /// Bounded local storage offered, in bytes. `None` = not contributing
    /// storage.
    pub storage_bytes: Option<u64>,
    /// Bounded bandwidth offered per day, in bytes. `None` = not
    /// contributing bandwidth.
    pub bandwidth_bytes_per_day: Option<u64>,
    /// Bounded CPU share offered, 0-100. `None` = not contributing CPU.
    pub cpu_percent: Option<CpuPercent>,
    /// When this device is willing to run contribution work at all.
    pub battery_policy: BatteryPolicy,
    /// Which network conditions this device is willing to contribute
    /// bandwidth under.
    pub network_policy: NetworkPolicy,
    /// Whether contribution work may run while the app is backgrounded.
    /// `false` (the default) means contribution only happens while the
    /// owner has the app open and in view.
    pub background_operation: bool,
}

/// A CPU share in whole percent, clamped to `0..=100` at construction so no
/// caller can encode an impossible "150% of one core" budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CpuPercent(u8);

impl CpuPercent {
    /// Build a [`CpuPercent`], clamping to the valid `0..=100` range.
    pub fn new(percent: u8) -> Self {
        CpuPercent(percent.min(100))
    }

    /// The clamped percent value.
    pub fn value(self) -> u8 {
        self.0
    }
}

/// When contribution work may run relative to battery/charging state.
/// The default, [`BatteryPolicy::NeverOnBattery`], is the most
/// conservative option -- opting into a looser policy is always an
/// explicit caller choice, never implied by opting into any resource
/// budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum BatteryPolicy {
    /// Never contribute while running on battery power.
    #[default]
    NeverOnBattery,
    /// Contribute only while charging (plugged in).
    OnlyWhileCharging,
    /// Contribute regardless of power source.
    RegardlessOfPower,
}

/// Which network conditions contributed bandwidth may be spent under. The
/// default, [`NetworkPolicy::WifiOnly`], never spends a caller's mobile
/// data allowance without an explicit opt-in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum NetworkPolicy {
    /// Only contribute over Wi-Fi or another unmetered connection.
    #[default]
    WifiOnly,
    /// Contribute over any connection, including metered mobile data.
    AnyConnection,
}

impl ContributionBudget {
    /// The default budget: opted out of every resource. Matches the
    /// research doctrine's "free participation must not require" language
    /// by construction -- there is no field here that a caller must set to
    /// avoid contributing something.
    pub const fn opted_out() -> Self {
        ContributionBudget {
            storage_bytes: None,
            bandwidth_bytes_per_day: None,
            cpu_percent: None,
            battery_policy: BatteryPolicy::NeverOnBattery,
            network_policy: NetworkPolicy::WifiOnly,
            background_operation: false,
        }
    }

    /// Revoke every contribution, resetting back to [`Self::opted_out`].
    /// Always available, always immediate -- the doctrine's "revocable"
    /// requirement.
    pub fn revoke(&mut self) {
        *self = Self::opted_out();
    }

    /// Whether this budget currently offers any resource at all. `false`
    /// exactly when this is bit-for-bit [`Self::opted_out`].
    pub fn is_contributing(&self) -> bool {
        self.storage_bytes.is_some()
            || self.bandwidth_bytes_per_day.is_some()
            || self.cpu_percent.is_some()
    }

    /// A stable, human-readable summary of exactly which resources this
    /// budget currently offers -- the doctrine's "visible" requirement.
    /// Names only what is actually opted in; an opted-out budget summarizes
    /// to an empty list, never a placeholder claim.
    pub fn active_grants(&self) -> Vec<&'static str> {
        let mut grants = Vec::new();
        if self.storage_bytes.is_some() {
            grants.push("storage");
        }
        if self.bandwidth_bytes_per_day.is_some() {
            grants.push("bandwidth");
        }
        if self.cpu_percent.is_some() {
            grants.push("cpu");
        }
        if self.background_operation {
            grants.push("background_operation");
        }
        grants
    }
}

impl Default for ContributionBudget {
    fn default() -> Self {
        Self::opted_out()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_budget_is_opted_out_of_every_resource() {
        let b = ContributionBudget::default();
        assert_eq!(b, ContributionBudget::opted_out());
        assert!(b.storage_bytes.is_none());
        assert!(b.bandwidth_bytes_per_day.is_none());
        assert!(b.cpu_percent.is_none());
        assert!(!b.background_operation);
        assert_eq!(b.battery_policy, BatteryPolicy::NeverOnBattery);
        assert_eq!(b.network_policy, NetworkPolicy::WifiOnly);
    }

    #[test]
    fn the_default_budget_is_not_contributing() {
        assert!(!ContributionBudget::default().is_contributing());
    }

    #[test]
    fn opted_out_reports_no_active_grants() {
        assert!(ContributionBudget::opted_out().active_grants().is_empty());
    }

    #[test]
    fn opting_into_one_resource_does_not_imply_any_other() {
        let mut b = ContributionBudget::opted_out();
        b.storage_bytes = Some(1_000_000);
        assert!(b.is_contributing());
        assert!(b.bandwidth_bytes_per_day.is_none());
        assert!(b.cpu_percent.is_none());
        assert!(!b.background_operation);
        assert_eq!(b.active_grants(), vec!["storage"]);
    }

    #[test]
    fn active_grants_lists_every_opted_in_resource() {
        let b = ContributionBudget {
            storage_bytes: Some(1),
            bandwidth_bytes_per_day: Some(1),
            cpu_percent: Some(CpuPercent::new(10)),
            background_operation: true,
            ..ContributionBudget::opted_out()
        };
        assert_eq!(
            b.active_grants(),
            vec!["storage", "bandwidth", "cpu", "background_operation"]
        );
    }

    #[test]
    fn revoke_resets_a_fully_opted_in_budget_back_to_opted_out() {
        let mut b = ContributionBudget {
            storage_bytes: Some(1),
            bandwidth_bytes_per_day: Some(1),
            cpu_percent: Some(CpuPercent::new(100)),
            battery_policy: BatteryPolicy::RegardlessOfPower,
            network_policy: NetworkPolicy::AnyConnection,
            background_operation: true,
        };
        b.revoke();
        assert_eq!(b, ContributionBudget::opted_out());
        assert!(!b.is_contributing());
    }

    #[test]
    fn cpu_percent_clamps_at_the_construction_boundary() {
        assert_eq!(CpuPercent::new(150).value(), 100);
        assert_eq!(CpuPercent::new(0).value(), 0);
        assert_eq!(CpuPercent::new(100).value(), 100);
        assert_eq!(CpuPercent::new(99).value(), 99);
    }

    #[test]
    fn battery_and_network_policy_default_to_the_most_conservative_choice() {
        assert_eq!(BatteryPolicy::default(), BatteryPolicy::NeverOnBattery);
        assert_eq!(NetworkPolicy::default(), NetworkPolicy::WifiOnly);
    }
}
