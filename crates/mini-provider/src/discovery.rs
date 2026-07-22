//! Discovery without a directory.

use did_mini::Did;
use mini_objects::ObjectId;

use crate::declaration::ProviderDeclaration;

/// Whatever local, device-held context a [`ProviderRanker`] implementation
/// wants to rank against (a user's history, stated preferences, prior
/// engagements). Deliberately opaque and minimal in this Wave 1 vocabulary
/// crate: ranking plugins own their own context shape, and this type
/// exists only so the trait has something to accept.
#[derive(Debug, Clone, Copy)]
pub struct LocalContext {
    pub now_ms: u64,
}

/// Providers are found the way content is found: content-addressed
/// objects surfaced by open-source ranking plugins running on the user's
/// own device.
///
/// There is no canonical registry type in this crate, and adding one is a
/// constitutional violation, not a feature request (INV-18-04) -- a
/// registry is a licensing board and a licensing board is an owner.
pub trait ProviderRanker {
    fn rank(&self, candidates: &[ProviderDeclaration], ctx: &LocalContext) -> Vec<ObjectId>;
}

/// Communities may publish curated lists as ordinary subscribable objects.
/// Users choose which curators, or none. A curator has no protocol
/// standing whatsoever -- this type carries no capability, no weight, and
/// no path to becoming a canonical list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CuratedList {
    pub curator: Did,
    pub entries: Vec<ObjectId>,
    pub rationale: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::declaration::{
        CustodyPosture, DeathDisposition, ExitTerms, FreezePowers, ServiceClass,
    };
    use did_mini::Controller;
    use mini_objects::{ObjectBuilder, ObjectType, Payload};

    fn sample_object_id(seed: u8) -> ObjectId {
        let root =
            Controller::incept_single_from_seeds(&[seed; 32], &[seed.wrapping_add(1); 32]).unwrap();
        let device = Controller::incept_device_single_from_seeds(
            &root.did(),
            &[seed.wrapping_add(2); 32],
            &[seed.wrapping_add(3); 32],
        )
        .unwrap();
        let obj = ObjectBuilder::new(ObjectType::Custom("test".to_string()))
            .payload(Payload::Public(vec![seed]))
            .sign(&root.did(), &device)
            .unwrap();
        obj.id().clone()
    }

    fn declaration_with_id_seed(seed: u8) -> ProviderDeclaration {
        let root =
            Controller::incept_single_from_seeds(&[seed; 32], &[seed.wrapping_add(9); 32]).unwrap();
        ProviderDeclaration {
            declarant: root.did(),
            service: ServiceClass::Conversion,
            description: String::new(),
            jurisdictions: vec![],
            data_required: vec![],
            custody: CustodyPosture::NoneHeld,
            freeze_powers: FreezePowers {
                can_freeze_user: false,
                grounds: vec![],
                notifies_user: true,
            },
            death_disposition: DeathDisposition::NothingHeld,
            exit: ExitTerms {
                notice_required_ms: None,
                exit_fee_micromini: 0,
                retained_data: vec![],
            },
            expires_at_ms: u64::MAX,
        }
    }

    /// A trivial ranker so the trait can be exercised end-to-end with real
    /// `ProviderDeclaration` values -- proving `ProviderRanker` is
    /// implementable against this crate's actual types, not just
    /// theoretically shaped.
    struct FirstOnlyRanker;

    impl ProviderRanker for FirstOnlyRanker {
        fn rank(&self, candidates: &[ProviderDeclaration], _ctx: &LocalContext) -> Vec<ObjectId> {
            candidates
                .first()
                .map(|_| sample_object_id(42))
                .into_iter()
                .collect()
        }
    }

    #[test]
    fn a_ranker_implementation_runs_against_real_declarations() {
        let candidates = vec![declaration_with_id_seed(1), declaration_with_id_seed(2)];
        let ctx = LocalContext { now_ms: 1_000 };
        let ranked = FirstOnlyRanker.rank(&candidates, &ctx);
        assert_eq!(ranked.len(), 1);
    }

    #[test]
    fn a_curated_list_carries_no_protocol_capability_it_is_plain_data() {
        let curator = Controller::incept_single().unwrap().did();
        let list = CuratedList {
            curator: curator.clone(),
            entries: vec![sample_object_id(5), sample_object_id(6)],
            rationale: "vetted by community X".to_string(),
        };
        assert_eq!(list.curator, curator);
        assert_eq!(list.entries.len(), 2);
    }
}
