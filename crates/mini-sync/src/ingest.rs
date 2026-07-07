//! The verified-ingest pipeline: the trust boundary between the wire and the
//! store, plus KEL distribution as ordinary objects.

use std::collections::BTreeMap;

use did_mini::{Controller, Did, Kel};
use mini_objects::{verify_provenance, Object, ObjectBuilder, ObjectType, Payload};

/// Maximum KELs kept in one cache (DoS bound for hostile peers).
pub const MAX_CACHED_KELS: usize = 10_000;
/// Maximum size of one KEL carrier payload accepted from the wire.
pub const MAX_KEL_CARRIER_BYTES: usize = 1024 * 1024;

/// The custom object type that carries a Key Event Log: payload = the KEL's
/// canonical bytes (`Kel::to_bytes`). A KEL is self-certifying, so the carrier
/// needs no extra trust — the embedded log proves itself.
pub const KEL_CARRIER: &str = "mini/kel";

/// Wrap a KEL as a carrier object so identity replicates like any content.
/// Sign with any device of `human` (the carrier's own provenance is then
/// checkable once its author's KELs are cached — self-KELs bootstrap because
/// the embedded log itself verifies).
pub fn kel_carrier(
    kel: &Kel,
    human: &Did,
    device: &Controller,
) -> core::result::Result<Object, mini_objects::ObjectError> {
    ObjectBuilder::new(ObjectType::Custom(KEL_CARRIER.to_string()))
        .payload(Payload::Public(kel.to_bytes()))
        .sign(human, device)
}

/// Verified KELs by scid — both human roots and devices.
#[derive(Debug, Default)]
pub struct KelCache {
    kels: BTreeMap<String, Kel>,
}

impl KelCache {
    /// An empty cache.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a locally-known, already-trusted KEL (e.g. your own identities).
    pub fn insert_verified(&mut self, kel: Kel) {
        self.kels.insert(kel.scid().to_string(), kel);
    }

    /// A cached KEL by DID.
    pub fn get(&self, did: &Did) -> Option<&Kel> {
        self.kels.get(did.scid())
    }

    /// Number of cached identities.
    pub fn len(&self) -> usize {
        self.kels.len()
    }

    /// Whether the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.kels.is_empty()
    }

    /// Try to absorb a carrier object: the embedded KEL must decode and verify
    /// (self-certifying — SCID re-derivation, chain, pre-rotation). Longer
    /// valid logs replace shorter ones for the same scid (rotations/revocations
    /// propagate); a *different* log for a known scid is rejected here and
    /// surfaced as duplicity for the witness layer (SPEC-01 M3) later.
    fn absorb_carrier(&mut self, obj: &Object) -> CarrierAbsorb {
        if obj.object_type != ObjectType::Custom(KEL_CARRIER.to_string()) {
            return CarrierAbsorb::Rejected;
        }
        let bytes = match &obj.payload {
            Payload::Public(b) if b.len() <= MAX_KEL_CARRIER_BYTES => b,
            Payload::Public(_) | Payload::Encrypted(_) => return CarrierAbsorb::Rejected,
        };
        let kel = match Kel::from_bytes(bytes) {
            Ok(k) => k,
            Err(_) => return CarrierAbsorb::Rejected,
        };
        if kel.verify().is_err() {
            return CarrierAbsorb::Rejected;
        }
        // The embedded KEL must be one of the carrier's own claimed identities —
        // a peer cannot wrap a third party's KEL in a misleading envelope.
        let embedded = kel.did();
        let is_device = embedded.as_str() == obj.author_device.as_str();
        let is_root = embedded.as_str() == obj.author_human.as_str();
        if !is_device && !is_root {
            return CarrierAbsorb::Rejected;
        }
        // A *device* carrier's envelope is proven directly by the device key it
        // embeds — signature-check it before absorbing (D-0030).
        if is_device && obj.verify_signature(&kel).is_err() {
            return CarrierAbsorb::Rejected;
        }
        // Absorb / upgrade the self-certifying KEL. Longer valid logs replace
        // shorter ones for the same scid (rotations/revocations propagate); a
        // conflicting fork is refused and surfaced as duplicity later (SPEC-01 M3).
        let scid = kel.scid().to_string();
        match self.kels.get(&scid) {
            None => {
                if self.kels.len() >= MAX_CACHED_KELS {
                    return CarrierAbsorb::Rejected;
                }
                self.kels.insert(scid, kel);
            }
            Some(existing) => {
                let old = existing.events();
                let new = kel.events();
                if new.len() >= old.len() && new[..old.len()] == *old {
                    self.kels.insert(scid, kel);
                } else {
                    return CarrierAbsorb::Rejected; // conflicting history
                }
            }
        }
        // Envelope authorship decides whether the carrier *object* may be indexed
        // as authored content. A device carrier is already signature-proven above.
        // A *root-only* carrier is signed by some device of the root whose KEL may
        // not be cached yet: index the object only if that device is known and
        // full provenance (delegated, unrevoked, capability-scoped) holds now.
        // Otherwise the KEL stays absorbed (useful for identity) but the object is
        // transport-only — closing the root-carrier index-pollution vector
        // (SPEC-01 boundary). Sync re-checks deferred carriers once the batch's
        // other carriers are absorbed, so an in-band device carrier still lets its
        // root carrier index on a second pass.
        if is_device {
            return CarrierAbsorb::VerifiedEnvelope;
        }
        let envelope_ok = matches!(
            (
                self.kels.get(obj.author_human.scid()),
                self.kels.get(obj.author_device.scid()),
            ),
            (Some(root), Some(device)) if verify_provenance(obj, root, device).is_ok()
        );
        if envelope_ok {
            CarrierAbsorb::VerifiedEnvelope
        } else {
            CarrierAbsorb::KelOnly
        }
    }
}

/// Outcome of trying to absorb a KEL carrier into the cache.
enum CarrierAbsorb {
    /// KEL absorbed AND the carrier envelope's authorship is proven now — the
    /// object may be indexed as authored content.
    VerifiedEnvelope,
    /// KEL absorbed for identity resolution, but the envelope's authorship is not
    /// (yet) provable — the object is transport-only and must NOT be indexed.
    KelOnly,
    /// Nothing absorbed: invalid carrier, foreign KEL, or conflicting history.
    Rejected,
}

/// Per-object ingest outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IngestOutcome {
    /// Verified and safe to insert.
    Accepted,
    /// A KEL carrier whose embedded KEL was absorbed AND whose envelope
    /// authorship is proven — safe to insert and index as an object.
    AcceptedCarrier,
    /// A KEL carrier whose embedded KEL was absorbed into the cache, but whose
    /// envelope authorship is not (yet) provable. Safe for identity resolution,
    /// but the object is transport-only and must NOT be indexed as authored
    /// content (SPEC-01 boundary). Sync re-checks it once the batch's other
    /// carriers are absorbed.
    AcceptedKelOnly,
    /// Rejected: the author's KELs are not in the cache.
    UnknownAuthor,
    /// Rejected: signature or provenance failed, or the carrier was invalid.
    Invalid,
}

/// The strict verify-before-insert policy.
#[derive(Debug, Default)]
pub struct Ingest;

impl Ingest {
    /// Verify one decoded object against the cache. Carriers are absorbed;
    /// everything else needs its author's root and device KELs cached and must
    /// pass full provenance (delegated, unrevoked, capability-scoped).
    pub fn check(cache: &mut KelCache, obj: &Object) -> IngestOutcome {
        if obj.object_type == ObjectType::Custom(KEL_CARRIER.to_string()) {
            return match cache.absorb_carrier(obj) {
                CarrierAbsorb::VerifiedEnvelope => IngestOutcome::AcceptedCarrier,
                CarrierAbsorb::KelOnly => IngestOutcome::AcceptedKelOnly,
                CarrierAbsorb::Rejected => IngestOutcome::Invalid,
            };
        }
        let root = match cache.get(&obj.author_human) {
            Some(k) => k.clone(),
            None => return IngestOutcome::UnknownAuthor,
        };
        let device = match cache.get(&obj.author_device) {
            Some(k) => k.clone(),
            None => return IngestOutcome::UnknownAuthor,
        };
        match verify_provenance(obj, &root, &device) {
            Ok(_) => IngestOutcome::Accepted,
            Err(_) => IngestOutcome::Invalid,
        }
    }
}
