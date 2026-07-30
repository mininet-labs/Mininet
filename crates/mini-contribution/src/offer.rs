//! A single-provider contribution offer: binding one content manifest to
//! one provider's role and declared price for a specific delivery.

use did_mini::Did;
use mini_objects::ObjectId;

use crate::role::DeliveryRole;

/// One provider's offer to deliver a specific manifest, in a specific role,
/// at a specific price. Deliberately single-provider and non-negotiating --
/// no counter-offer, no auction. A real marketplace/negotiation protocol is
/// later, separate work.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContributionOffer {
    pub manifest_id: ObjectId,
    pub provider: Did,
    pub role: DeliveryRole,
    pub price_micro: u64,
}

impl ContributionOffer {
    pub fn new(manifest_id: ObjectId, provider: Did, role: DeliveryRole, price_micro: u64) -> Self {
        Self {
            manifest_id,
            provider,
            role,
            price_micro,
        }
    }
}
