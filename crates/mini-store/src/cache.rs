//! Cache tiers and seed-on-view (founder decision, 2026-07-07): "watching
//! content can help seed it," bounded by user, battery, metered-data, and
//! privacy policy.
//!
//! ## The default is safe [FREEZE]
//!
//! An object with no recorded tier is [`CacheTier::EphemeralCache`]:
//! held locally, never advertised. Only [`Store::note_view`] — or an explicit
//! [`Store::set_cache_tier`] call, e.g. a user pinning something — can
//! promote a tier toward advertising availability, and encrypted content can
//! **never** be promoted past [`CacheTier::PrivateOnly`], regardless of
//! policy (see the tests in `crates/mini-store/tests/cache.rs`).
//!
//! [`Store::note_view`] deliberately takes no viewer identity: opening
//! content is a local, identity-free operation. It updates only the local
//! cache-tier index for the object being viewed.

use did_mini::BaseDeviceRole;
use mini_objects::{ObjectId, Payload};

use crate::backend::Backend;
use crate::store::Store;
use crate::{Result, StoreError};

/// How locally cached content is treated for storage and seeding purposes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum CacheTier {
    /// Temporary watch cache: held locally, never advertised.
    EphemeralCache,
    /// The user (or seed-on-view policy) agreed to seed this.
    SeedCache,
    /// Committed storage: earns the stronger `mini-reward` storage rate.
    CommittedStorage,
    /// Never advertised, regardless of any policy (encrypted/private data).
    PrivateOnly,
    /// The owner intentionally pinned this; never auto-downgraded.
    PinnedByOwner,
}

impl CacheTier {
    /// Whether a tier advertises availability to other peers.
    pub fn advertises(self) -> bool {
        matches!(
            self,
            CacheTier::SeedCache | CacheTier::CommittedStorage | CacheTier::PinnedByOwner
        )
    }

    fn to_byte(self) -> u8 {
        match self {
            CacheTier::EphemeralCache => 0,
            CacheTier::SeedCache => 1,
            CacheTier::CommittedStorage => 2,
            CacheTier::PrivateOnly => 3,
            CacheTier::PinnedByOwner => 4,
        }
    }

    fn from_byte(b: u8) -> Option<Self> {
        match b {
            0 => Some(CacheTier::EphemeralCache),
            1 => Some(CacheTier::SeedCache),
            2 => Some(CacheTier::CommittedStorage),
            3 => Some(CacheTier::PrivateOnly),
            4 => Some(CacheTier::PinnedByOwner),
            _ => None,
        }
    }
}

/// Live device conditions gating seed-on-view, layered on top of the
/// device's own declared [`BaseDeviceRole`] (battery/availability-window
/// policy already lives there).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ViewConditions {
    /// Battery charge percent (0..=100). Ignored if not `on_battery`.
    pub battery_percent: u8,
    /// Whether the device is currently running on battery power.
    pub on_battery: bool,
    /// Minutes since local midnight (0..1440), for the availability window.
    pub minute_of_day: u16,
    /// Whether the current network connection is metered.
    pub metered_connection: bool,
    /// Whether local storage has budget left to accept more seeded content.
    pub storage_budget_remaining: bool,
}

fn tier_key(id: &ObjectId) -> String {
    format!("cache/{}", id.as_str())
}

impl<B: Backend> Store<B> {
    /// The recorded cache tier for `id`, defaulting to [`CacheTier::EphemeralCache`]
    /// if none has ever been recorded — the safe, non-advertising default.
    pub fn cache_tier(&self, id: &ObjectId) -> Result<CacheTier> {
        match self.backend.get_meta(&tier_key(id))? {
            Some(bytes) => {
                let b = *bytes.first().ok_or(StoreError::Corrupt)?;
                CacheTier::from_byte(b).ok_or(StoreError::Corrupt)
            }
            None => Ok(CacheTier::EphemeralCache),
        }
    }

    /// Explicitly set `id`'s cache tier (e.g. a user pinning content, or an
    /// owner committing storage for it). Does not require the object to be
    /// present, so a tier can be pre-declared before content arrives.
    pub fn set_cache_tier(&mut self, id: &ObjectId, tier: CacheTier) -> Result<()> {
        self.backend.put_meta(&tier_key(id), &[tier.to_byte()])
    }

    /// Record that the local user viewed/opened `id`, which — per policy —
    /// may naturally help seed it. Takes no viewer identity: viewing content
    /// is local and identity-free by construction.
    ///
    /// Rules, most specific first:
    /// 1. [`CacheTier::PinnedByOwner`] and [`CacheTier::CommittedStorage`] are
    ///    never downgraded by a view.
    /// 2. Encrypted content can only ever become [`CacheTier::PrivateOnly`] —
    ///    it is never advertised, no matter how permissive the policy.
    /// 3. Otherwise, the object is promoted to [`CacheTier::SeedCache`] only
    ///    if the base device's `should_seed_now` policy holds *and* the
    ///    connection is unmetered *and* there is storage budget left;
    ///    otherwise it stays [`CacheTier::EphemeralCache`].
    pub fn note_view(
        &mut self,
        id: &ObjectId,
        role: &BaseDeviceRole,
        conditions: ViewConditions,
    ) -> Result<CacheTier> {
        let object = self.get(id)?;
        let current = self.cache_tier(id)?;
        if matches!(
            current,
            CacheTier::PinnedByOwner | CacheTier::CommittedStorage
        ) {
            return Ok(current);
        }
        if matches!(object.payload, Payload::Encrypted(_)) {
            self.set_cache_tier(id, CacheTier::PrivateOnly)?;
            return Ok(CacheTier::PrivateOnly);
        }
        let seed_ok = role.should_seed_now(
            conditions.battery_percent,
            conditions.on_battery,
            conditions.minute_of_day,
        ) && !conditions.metered_connection
            && conditions.storage_budget_remaining;
        let next = if seed_ok {
            CacheTier::SeedCache
        } else {
            CacheTier::EphemeralCache
        };
        self.set_cache_tier(id, next)?;
        Ok(next)
    }
}
