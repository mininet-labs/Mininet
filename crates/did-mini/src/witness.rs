//! KEL witness receipt types (audit #12 finding F4, invariant M3) — Phase 1
//! of `docs/design/kel-witness-receipts-and-duplicity-gossip.md`'s
//! committed phased plan.
//!
//! [`crate::FreshnessPins`] (D-0088) already solves the case where a
//! verifier has *previously seen* an identity: a conflicting event is
//! rejected because it contradicts retained state. It does not solve the
//! harder "never seen a fresher log" gap — a verifier meeting an identity
//! for the *first time* has no prior head to compare against, and two
//! internally-valid, controller-signed branches can both pass ordinary KEL
//! verification in isolation. This module is the receipt/certificate
//! vocabulary the eventual fix (KERI-inspired asynchronous witness
//! receipts plus proof-carrying gossip) is built from.
//!
//! ## Scope: Phase 1 only
//!
//! [`WitnessPolicy`], [`WitnessReceiptStatement`], [`WitnessReceipt`], and
//! [`WitnessedEventCertificate`] — canonical encoding and signature/
//! threshold verification, mirroring `event.rs`'s existing hand-rolled
//! codec discipline. [`sign_witness_receipt`] is the one typed function a
//! witness ever calls to produce a receipt — never a generic
//! `sign(bytes)`, per CLAUDE.md's typed-domain rule.
//!
//! **Not in this module:** the in-memory witness state machine,
//! `ControllerDuplicityProof`/`WitnessEquivocationProof` (Phase 2);
//! `KelAssurance`/KEL-verification integration (Phase 3); receipt
//! collection protocol, gossip, a persistent witness service, witness
//! rotation, public transparency logs, or adversarial network simulation
//! (Phases 4-9). [`WitnessedEventCertificate::verify`] deliberately does
//! **not** cross-check `event_digest` against a real KEL event, and does
//! not evaluate a freshness policy against `observed_epoch` — both are
//! Phase 3's job. Per the design doc, Phase 10's external-review gate
//! applies before any high-value authority decision may depend on this
//! layer, not before this self-contained type PR.

use mini_crypto::{Signature, SignatureSuite, SigningKey, VerifyingKey};

use crate::codec::{Reader, Writer};
use crate::error::{IdentityError, Result};
use crate::event::EventKind;
use crate::limits::{MAX_DID_BYTES, MAX_MULTIHASH_BYTES, MAX_WITNESSES};
use crate::Did;

/// Wire version for [`WitnessReceiptStatement`]/[`WitnessReceipt`]. A typed
/// wrapper rather than a bare `u8` so a receipt version can never be
/// accidentally compared against or substituted for a
/// [`WitnessCertificateVersion`] — CLAUDE.md's typed-domains rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WitnessReceiptVersion(u8);

impl WitnessReceiptVersion {
    /// The only version this module currently produces or accepts.
    pub const V1: WitnessReceiptVersion = WitnessReceiptVersion(1);

    fn as_u8(self) -> u8 {
        self.0
    }

    fn from_u8(v: u8) -> Result<Self> {
        match v {
            1 => Ok(WitnessReceiptVersion(1)),
            other => Err(IdentityError::UnknownWitnessReceiptVersion(other)),
        }
    }
}

/// Wire version for [`WitnessedEventCertificate`]. See
/// [`WitnessReceiptVersion`] for why this is a distinct type rather than
/// sharing one bare `u8`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WitnessCertificateVersion(u8);

impl WitnessCertificateVersion {
    /// The only version this module currently produces or accepts.
    pub const V1: WitnessCertificateVersion = WitnessCertificateVersion(1);

    fn as_u8(self) -> u8 {
        self.0
    }

    fn from_u8(v: u8) -> Result<Self> {
        match v {
            1 => Ok(WitnessCertificateVersion(1)),
            other => Err(IdentityError::UnknownWitnessCertificateVersion(other)),
        }
    }
}

/// A witness's own `did:mini` identifier. Every value happens to be a
/// [`Did`], but this is a distinct type purely for domain clarity: it
/// names specifically "the identity acting in the witness role for this
/// receipt," so a future call site can never accidentally pass an
/// unrelated `Did` (a subject identity, a delegator) where a witness
/// identity is required.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WitnessId(pub Did);

fn encode_did(w: &mut Writer, did: &Did) {
    w.bytes(did.as_str().as_bytes());
}

fn decode_did(r: &mut Reader) -> Result<Did> {
    let bytes = r.bytes_limited("did", MAX_DID_BYTES)?;
    let s = String::from_utf8(bytes).map_err(|_| IdentityError::DidFormat)?;
    Did::parse(&s)
}

fn encode_digest(w: &mut Writer, digest: &[u8]) {
    w.bytes(digest);
}

fn decode_digest(r: &mut Reader) -> Result<Vec<u8>> {
    r.bytes_limited("event_digest", MAX_MULTIHASH_BYTES)
}

/// Which shape of key event a receipt is binding to — a tag-only mirror of
/// [`crate::EventKind`]'s discriminant. A receipt does not carry (and
/// should not carry) the full established key material, only which kind
/// of event was witnessed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyEventKind {
    Inception,
    DelegatedInception,
    Rotation,
    Interaction,
    Seal,
}

impl KeyEventKind {
    fn tag(self) -> u8 {
        match self {
            KeyEventKind::Inception => 1,
            KeyEventKind::DelegatedInception => 2,
            KeyEventKind::Rotation => 3,
            KeyEventKind::Interaction => 4,
            KeyEventKind::Seal => 5,
        }
    }

    fn from_tag(t: u8) -> Result<Self> {
        match t {
            1 => Ok(KeyEventKind::Inception),
            2 => Ok(KeyEventKind::DelegatedInception),
            3 => Ok(KeyEventKind::Rotation),
            4 => Ok(KeyEventKind::Interaction),
            5 => Ok(KeyEventKind::Seal),
            other => Err(IdentityError::UnknownKeyEventKindTag(other)),
        }
    }
}

impl From<&EventKind> for KeyEventKind {
    fn from(k: &EventKind) -> Self {
        match k {
            EventKind::Inception(_) => KeyEventKind::Inception,
            EventKind::DelegatedInception { .. } => KeyEventKind::DelegatedInception,
            EventKind::Rotation(_) => KeyEventKind::Rotation,
            EventKind::Interaction { .. } => KeyEventKind::Interaction,
            EventKind::Seal { .. } => KeyEventKind::Seal,
        }
    }
}

/// A witness set + threshold + generation, carried by an establishment
/// event (SPEC-01 §7). `generation` exists so a receipt issued under one
/// witness-set version can never be misapplied after that witness is
/// removed, the threshold changes, or a policy reset — a receipt binds
/// its exact `witness_policy_generation`, and [`WitnessedEventCertificate::
/// verify`] rejects a generation mismatch outright.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WitnessPolicy {
    pub generation: u64,
    pub witnesses: Vec<WitnessId>,
    pub threshold: u16,
}

impl WitnessPolicy {
    /// Construct a policy, validating `1 <= threshold <= witnesses.len()`
    /// and rejecting duplicate witness identifiers — the same
    /// reject-malformed-input-at-construction discipline
    /// `event::validate_establishment` already applies to key sets.
    pub fn new(generation: u64, witnesses: Vec<WitnessId>, threshold: u16) -> Result<Self> {
        if witnesses.is_empty() {
            return Err(IdentityError::EmptyWitnessSet);
        }
        if witnesses.len() > MAX_WITNESSES {
            return Err(IdentityError::TooManyItems {
                field: "witnesses",
                max: MAX_WITNESSES,
                got: witnesses.len(),
            });
        }
        if threshold == 0 || threshold as usize > witnesses.len() {
            return Err(IdentityError::InvalidWitnessThreshold {
                threshold,
                witness_count: witnesses.len(),
            });
        }
        let mut seen: Vec<&WitnessId> = Vec::new();
        for w in &witnesses {
            if seen.contains(&w) {
                return Err(IdentityError::DuplicateWitness);
            }
            seen.push(w);
        }
        Ok(WitnessPolicy {
            generation,
            witnesses,
            threshold,
        })
    }

    /// Whether `id` is a member of this policy's witness set.
    pub fn contains(&self, id: &WitnessId) -> bool {
        self.witnesses.contains(id)
    }

    /// Encode to the canonical wire form.
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.u64(self.generation);
        w.u32(self.witnesses.len() as u32);
        for wit in &self.witnesses {
            encode_did(&mut w, &wit.0);
        }
        w.u32(self.threshold as u32);
        w.into_bytes()
    }

    /// Decode from [`Self::encode`]'s wire form, re-validating exactly as
    /// [`Self::new`] does — a decoded policy can never bypass the
    /// threshold/duplicate checks a constructed one already enforces.
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let mut r = Reader::new(bytes);
        let generation = r.u64()?;
        let n = checked_count(r.u32()? as usize, MAX_WITNESSES, "witnesses")?;
        let mut witnesses = Vec::with_capacity(n);
        for _ in 0..n {
            witnesses.push(WitnessId(decode_did(&mut r)?));
        }
        let threshold_u32 = r.u32()?;
        let threshold: u16 =
            threshold_u32
                .try_into()
                .map_err(|_| IdentityError::InvalidWitnessThreshold {
                    threshold: u16::MAX,
                    witness_count: witnesses.len(),
                })?;
        if !r.finished() {
            return Err(IdentityError::TrailingBytes);
        }
        WitnessPolicy::new(generation, witnesses, threshold)
    }
}

/// The exact typed statement a witness signs — never a generic
/// `sign(bytes)`. [`sign_witness_receipt`] is the only way to produce a
/// [`WitnessReceipt`] from one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WitnessReceiptStatement {
    pub version: WitnessReceiptVersion,
    pub identity: Did,
    pub sequence: u64,
    /// Multihash bytes of the witnessed event (matches
    /// `did_mini::event::Event::digest`'s own output shape).
    pub event_digest: Vec<u8>,
    /// Multihash bytes of the prior event, `None` only for an inception.
    pub prior_event_digest: Option<Vec<u8>>,
    pub event_kind: KeyEventKind,
    pub witness_policy_generation: u64,
    pub witness_id: WitnessId,
    /// A coarse network epoch, deliberately not an exact timestamp (per
    /// the research report §8.7: an exact timestamp increases clock
    /// dependency, leaks witness timing, and can become a tracking
    /// surface).
    pub observed_epoch: u64,
}

impl WitnessReceiptStatement {
    /// Encode to the canonical wire form these bytes are signed over.
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.u8(self.version.as_u8());
        encode_did(&mut w, &self.identity);
        w.u64(self.sequence);
        encode_digest(&mut w, &self.event_digest);
        match &self.prior_event_digest {
            None => w.u8(0),
            Some(d) => {
                w.u8(1);
                encode_digest(&mut w, d);
            }
        }
        w.u8(self.event_kind.tag());
        w.u64(self.witness_policy_generation);
        encode_did(&mut w, &self.witness_id.0);
        w.u64(self.observed_epoch);
        w.into_bytes()
    }

    /// Decode from [`Self::encode`]'s wire form. Strict: rejects trailing
    /// bytes.
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let mut r = Reader::new(bytes);
        let version = WitnessReceiptVersion::from_u8(r.u8()?)?;
        let identity = decode_did(&mut r)?;
        let sequence = r.u64()?;
        let event_digest = decode_digest(&mut r)?;
        let prior_event_digest = match r.u8()? {
            0 => None,
            1 => Some(decode_digest(&mut r)?),
            _ => return Err(IdentityError::BadEvent),
        };
        let event_kind = KeyEventKind::from_tag(r.u8()?)?;
        let witness_policy_generation = r.u64()?;
        let witness_id = WitnessId(decode_did(&mut r)?);
        let observed_epoch = r.u64()?;
        if !r.finished() {
            return Err(IdentityError::TrailingBytes);
        }
        Ok(WitnessReceiptStatement {
            version,
            identity,
            sequence,
            event_digest,
            prior_event_digest,
            event_kind,
            witness_policy_generation,
            witness_id,
            observed_epoch,
        })
    }
}

/// Sign `statement` with `witness_key`, producing the one message a
/// witness ever emits for an observed event. Named to match the research
/// report's own API sketch (`sign_witness_receipt`) rather than a generic
/// `sign` — CLAUDE.md's typed-domain rule.
pub fn sign_witness_receipt(
    statement: WitnessReceiptStatement,
    witness_key: &SigningKey,
) -> WitnessReceipt {
    let signature = witness_key.sign(&statement.encode());
    WitnessReceipt {
        statement,
        signature,
    }
}

/// A witness's signed observation of one event. Carries no arbitrary
/// notes or extensible untyped metadata — everything a verifier needs is
/// in the typed [`WitnessReceiptStatement`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WitnessReceipt {
    pub statement: WitnessReceiptStatement,
    pub signature: Signature,
}

impl WitnessReceipt {
    /// Verify this receipt's signature against `witness_key`. Checking
    /// that `witness_key` is actually the real, currently-valid key for
    /// `self.statement.witness_id` (e.g. resolved via that witness's own
    /// KEL) is the caller's job — this method only checks the
    /// cryptographic binding between the statement and the signature,
    /// not witness-identity resolution, which is Phase 3's job.
    pub fn verify(&self, witness_key: &VerifyingKey) -> Result<()> {
        witness_key.verify(&self.statement.encode(), &self.signature)?;
        Ok(())
    }

    /// Encode to the canonical wire form.
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::new();
        let statement_bytes = self.statement.encode();
        w.bytes(&statement_bytes);
        w.u8(self.signature.suite().tag());
        w.bytes(&self.signature.to_bytes());
        w.into_bytes()
    }

    /// Decode from [`Self::encode`]'s wire form. Strict: rejects trailing
    /// bytes.
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let mut r = Reader::new(bytes);
        let statement_bytes = r.bytes_limited("statement", MAX_STATEMENT_BYTES)?;
        let statement = WitnessReceiptStatement::decode(&statement_bytes)?;
        let suite = SignatureSuite::from_tag(r.u8()?)?;
        let sig_bytes = r.bytes_limited("signature", MAX_SIGNATURE_BYTES)?;
        let signature = Signature::from_suite_bytes(suite, &sig_bytes)?;
        if !r.finished() {
            return Err(IdentityError::TrailingBytes);
        }
        Ok(WitnessReceipt {
            statement,
            signature,
        })
    }
}

/// A statement's own encoded size is bounded by its constituent fields'
/// bounds (two DIDs, two digests, a handful of fixed-width integers);
/// this is a generous ceiling on the whole encoded statement, not a
/// separate independently-tuned limit.
const MAX_STATEMENT_BYTES: usize = 2 * MAX_DID_BYTES + 2 * MAX_MULTIHASH_BYTES + 64;
const MAX_SIGNATURE_BYTES: usize = 4096;

/// Enough [`WitnessReceipt`]s against one [`WitnessPolicy`]'s threshold,
/// bundled so a verifier processes one self-contained certificate instead
/// of loose receipts from arbitrary sources.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WitnessedEventCertificate {
    pub version: WitnessCertificateVersion,
    pub identity: Did,
    pub sequence: u64,
    pub event_digest: Vec<u8>,
    pub witness_policy_generation: u64,
    /// Canonically sorted by witness DID string for deterministic
    /// encoding (research report §10.1) — [`Self::assemble`] sorts;
    /// [`Self::decode`] does not re-sort, so a peer-supplied certificate
    /// with out-of-order receipts is preserved exactly as received rather
    /// than silently reordered (still verifies correctly either way,
    /// since [`Self::verify`] does not depend on order).
    pub receipts: Vec<WitnessReceipt>,
}

impl WitnessedEventCertificate {
    /// Assemble a certificate from receipts that all claim the same
    /// event, rejecting any receipt that does not exactly match
    /// `identity`/`sequence`/`event_digest`/`witness_policy_generation`
    /// before it is ever accepted into the bundle.
    pub fn assemble(
        identity: Did,
        sequence: u64,
        event_digest: Vec<u8>,
        witness_policy_generation: u64,
        mut receipts: Vec<WitnessReceipt>,
    ) -> Result<Self> {
        if receipts.is_empty() {
            return Err(IdentityError::EmptyWitnessSet);
        }
        if receipts.len() > MAX_WITNESSES {
            return Err(IdentityError::TooManyItems {
                field: "receipts",
                max: MAX_WITNESSES,
                got: receipts.len(),
            });
        }
        for r in &receipts {
            check_receipt_matches(
                r,
                &identity,
                sequence,
                &event_digest,
                witness_policy_generation,
            )?;
        }
        receipts.sort_by(|a, b| {
            a.statement
                .witness_id
                .0
                .as_str()
                .cmp(b.statement.witness_id.0.as_str())
        });
        Ok(WitnessedEventCertificate {
            version: WitnessCertificateVersion::V1,
            identity,
            sequence,
            event_digest,
            witness_policy_generation,
            receipts,
        })
    }

    /// Verify this certificate against `policy`, resolving each witness's
    /// verifying key via `resolve_witness_key`. Checks (per the research
    /// report §10): every receipt statement matches this certificate's
    /// own claimed event; `policy`'s generation matches this
    /// certificate's claimed generation; every witness belongs to
    /// `policy`; no witness is counted twice toward the threshold; every
    /// signature verifies; the threshold is met.
    ///
    /// Deliberately does **not** cross-check `event_digest` against a
    /// real KEL event, and does not evaluate a caller's receipt-freshness
    /// policy against `observed_epoch` — both are Phase 3's job (KEL
    /// verification integration), not this self-contained type/threshold
    /// check.
    pub fn verify(
        &self,
        policy: &WitnessPolicy,
        resolve_witness_key: impl Fn(&WitnessId) -> Option<VerifyingKey>,
    ) -> Result<()> {
        if policy.generation != self.witness_policy_generation {
            return Err(IdentityError::WitnessPolicyGenerationMismatch {
                expected: policy.generation,
                got: self.witness_policy_generation,
            });
        }
        let mut counted: Vec<&WitnessId> = Vec::new();
        for r in &self.receipts {
            check_receipt_matches(
                r,
                &self.identity,
                self.sequence,
                &self.event_digest,
                self.witness_policy_generation,
            )?;
            if !policy.contains(&r.statement.witness_id) {
                return Err(IdentityError::WitnessNotInPolicy);
            }
            if counted.contains(&&r.statement.witness_id) {
                // Already counted this witness once; a repeated receipt
                // (or a duplicate entry) never counts twice toward the
                // threshold, but its mere presence is not itself fatal.
                continue;
            }
            let key = resolve_witness_key(&r.statement.witness_id)
                .ok_or(IdentityError::UnresolvedWitnessKey)?;
            r.verify(&key)?;
            counted.push(&r.statement.witness_id);
        }
        if (counted.len() as u16) < policy.threshold {
            return Err(IdentityError::WitnessThresholdNotMet {
                needed: policy.threshold,
                got: counted.len() as u16,
            });
        }
        Ok(())
    }

    /// Encode to the canonical wire form.
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.u8(self.version.as_u8());
        encode_did(&mut w, &self.identity);
        w.u64(self.sequence);
        encode_digest(&mut w, &self.event_digest);
        w.u64(self.witness_policy_generation);
        w.u32(self.receipts.len() as u32);
        for r in &self.receipts {
            let bytes = r.encode();
            w.bytes(&bytes);
        }
        w.into_bytes()
    }

    /// Decode from [`Self::encode`]'s wire form. Strict: rejects trailing
    /// bytes and an over-bound receipt count.
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let mut r = Reader::new(bytes);
        let version = WitnessCertificateVersion::from_u8(r.u8()?)?;
        let identity = decode_did(&mut r)?;
        let sequence = r.u64()?;
        let event_digest = decode_digest(&mut r)?;
        let witness_policy_generation = r.u64()?;
        let n = checked_count(r.u32()? as usize, MAX_WITNESSES, "receipts")?;
        let mut receipts = Vec::with_capacity(n);
        for _ in 0..n {
            let receipt_bytes = r.bytes_limited("receipt", MAX_RECEIPT_BYTES)?;
            receipts.push(WitnessReceipt::decode(&receipt_bytes)?);
        }
        if !r.finished() {
            return Err(IdentityError::TrailingBytes);
        }
        if receipts.is_empty() {
            return Err(IdentityError::EmptyWitnessSet);
        }
        Ok(WitnessedEventCertificate {
            version,
            identity,
            sequence,
            event_digest,
            witness_policy_generation,
            receipts,
        })
    }
}

const MAX_RECEIPT_BYTES: usize = MAX_STATEMENT_BYTES + MAX_SIGNATURE_BYTES + 64;

fn check_receipt_matches(
    r: &WitnessReceipt,
    identity: &Did,
    sequence: u64,
    event_digest: &[u8],
    witness_policy_generation: u64,
) -> Result<()> {
    if r.statement.identity != *identity
        || r.statement.sequence != sequence
        || r.statement.event_digest != event_digest
        || r.statement.witness_policy_generation != witness_policy_generation
    {
        return Err(IdentityError::WitnessReceiptMismatch);
    }
    Ok(())
}

fn checked_count(n: usize, max: usize, field: &'static str) -> Result<usize> {
    if n > max {
        return Err(IdentityError::TooManyItems { field, max, got: n });
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Controller;

    /// A witness identity: a real `did:mini` (from a throwaway inception,
    /// so `Did::parse` is exercised on genuine SCIDs) paired with an
    /// independent freshly generated keypair. Phase 1 has no witness-KEL
    /// resolution yet, so these tests never need the witness's own KEL --
    /// only that it has *a* real `Did` and *a* real keypair.
    fn a_witness() -> (WitnessId, SigningKey, VerifyingKey) {
        let did = a_did();
        let key = SigningKey::generate().unwrap();
        let vk = key.verifying_key();
        (WitnessId(did), key, vk)
    }

    fn a_did() -> Did {
        Controller::incept_single().unwrap().did()
    }

    fn a_statement(witness_id: WitnessId, generation: u64) -> WitnessReceiptStatement {
        WitnessReceiptStatement {
            version: WitnessReceiptVersion::V1,
            identity: a_did(),
            sequence: 3,
            event_digest: vec![0xAA; 34],
            prior_event_digest: Some(vec![0xBB; 34]),
            event_kind: KeyEventKind::Rotation,
            witness_policy_generation: generation,
            witness_id,
            observed_epoch: 42,
        }
    }

    #[test]
    fn a_witness_policy_round_trips() {
        let (w1, _, _) = a_witness();
        let (w2, _, _) = a_witness();
        let policy = WitnessPolicy::new(1, vec![w1, w2], 2).unwrap();
        let decoded = WitnessPolicy::decode(&policy.encode()).unwrap();
        assert_eq!(decoded, policy);
    }

    #[test]
    fn a_witness_policy_rejects_zero_threshold() {
        let (w1, _, _) = a_witness();
        assert_eq!(
            WitnessPolicy::new(1, vec![w1], 0),
            Err(IdentityError::InvalidWitnessThreshold {
                threshold: 0,
                witness_count: 1
            })
        );
    }

    #[test]
    fn a_witness_policy_rejects_threshold_over_witness_count() {
        let (w1, _, _) = a_witness();
        assert_eq!(
            WitnessPolicy::new(1, vec![w1], 2),
            Err(IdentityError::InvalidWitnessThreshold {
                threshold: 2,
                witness_count: 1
            })
        );
    }

    #[test]
    fn a_witness_policy_rejects_an_empty_witness_set() {
        assert_eq!(
            WitnessPolicy::new(1, vec![], 1),
            Err(IdentityError::EmptyWitnessSet)
        );
    }

    #[test]
    fn a_witness_policy_rejects_a_duplicate_witness() {
        let (w1, _, _) = a_witness();
        let dup = WitnessId(w1.0.clone());
        assert_eq!(
            WitnessPolicy::new(1, vec![w1, dup], 1),
            Err(IdentityError::DuplicateWitness)
        );
    }

    #[test]
    fn a_witness_receipt_statement_round_trips() {
        let (w1, _, _) = a_witness();
        let statement = a_statement(w1, 1);
        let decoded = WitnessReceiptStatement::decode(&statement.encode()).unwrap();
        assert_eq!(decoded, statement);
    }

    #[test]
    fn an_inception_statement_has_no_prior_digest_and_round_trips() {
        let (w1, _, _) = a_witness();
        let mut statement = a_statement(w1, 1);
        statement.prior_event_digest = None;
        statement.event_kind = KeyEventKind::Inception;
        let decoded = WitnessReceiptStatement::decode(&statement.encode()).unwrap();
        assert_eq!(decoded, statement);
        assert_eq!(decoded.prior_event_digest, None);
    }

    #[test]
    fn a_signed_receipt_verifies_against_the_witness_key() {
        let (w1, key, vk) = a_witness();
        let statement = a_statement(w1, 1);
        let receipt = sign_witness_receipt(statement, &key);
        receipt.verify(&vk).unwrap();
    }

    #[test]
    fn a_receipt_signed_by_a_different_key_fails_verification() {
        let (w1, _key, _vk) = a_witness();
        let (_w2, other_key, _other_vk) = a_witness();
        let statement = a_statement(w1, 1);
        let receipt = sign_witness_receipt(statement, &other_key);
        let unrelated_vk = SigningKey::generate().unwrap().verifying_key();
        assert!(receipt.verify(&unrelated_vk).is_err());
    }

    #[test]
    fn a_receipt_round_trips() {
        let (w1, key, _) = a_witness();
        let statement = a_statement(w1, 1);
        let receipt = sign_witness_receipt(statement, &key);
        let decoded = WitnessReceipt::decode(&receipt.encode()).unwrap();
        assert_eq!(decoded, receipt);
    }

    fn assemble_certificate(
        witnesses: &[(WitnessId, SigningKey, VerifyingKey)],
        generation: u64,
    ) -> (Did, u64, Vec<u8>, WitnessedEventCertificate) {
        let identity = a_did();
        let sequence = 5;
        let event_digest = vec![0xCC; 34];
        let receipts: Vec<WitnessReceipt> = witnesses
            .iter()
            .map(|(wid, key, _)| {
                let statement = WitnessReceiptStatement {
                    version: WitnessReceiptVersion::V1,
                    identity: identity.clone(),
                    sequence,
                    event_digest: event_digest.clone(),
                    prior_event_digest: Some(vec![0xDD; 34]),
                    event_kind: KeyEventKind::Rotation,
                    witness_policy_generation: generation,
                    witness_id: wid.clone(),
                    observed_epoch: 100,
                };
                sign_witness_receipt(statement, key)
            })
            .collect();
        let cert = WitnessedEventCertificate::assemble(
            identity.clone(),
            sequence,
            event_digest.clone(),
            generation,
            receipts,
        )
        .unwrap();
        (identity, sequence, event_digest, cert)
    }

    #[test]
    fn a_certificate_meeting_threshold_verifies() {
        let witnesses: Vec<_> = (0..3).map(|_| a_witness()).collect();
        let policy = WitnessPolicy::new(
            1,
            witnesses.iter().map(|(id, _, _)| id.clone()).collect(),
            2,
        )
        .unwrap();
        let (_, _, _, cert) = assemble_certificate(&witnesses, 1);
        let resolve = |id: &WitnessId| {
            witnesses
                .iter()
                .find(|(wid, _, _)| wid == id)
                .map(|(_, _, vk)| vk.clone())
        };
        cert.verify(&policy, resolve).unwrap();
    }

    #[test]
    fn a_certificate_below_threshold_is_rejected() {
        let witnesses: Vec<_> = (0..3).map(|_| a_witness()).collect();
        let policy = WitnessPolicy::new(
            1,
            witnesses.iter().map(|(id, _, _)| id.clone()).collect(),
            3,
        )
        .unwrap();
        // Only assemble receipts from the first two witnesses -- one
        // short of the policy's threshold of 3.
        let two_witnesses = &witnesses[..2];
        let (_, _, _, cert) = assemble_certificate(two_witnesses, 1);
        let resolve = |id: &WitnessId| {
            witnesses
                .iter()
                .find(|(wid, _, _)| wid == id)
                .map(|(_, _, vk)| vk.clone())
        };
        assert_eq!(
            cert.verify(&policy, resolve),
            Err(IdentityError::WitnessThresholdNotMet { needed: 3, got: 2 })
        );
    }

    #[test]
    fn a_certificate_with_a_witness_outside_the_policy_is_rejected() {
        let witnesses: Vec<_> = (0..2).map(|_| a_witness()).collect();
        let outsider = a_witness();
        let policy = WitnessPolicy::new(
            1,
            witnesses.iter().map(|(id, _, _)| id.clone()).collect(),
            1,
        )
        .unwrap();
        let mixed = vec![witnesses[0].clone(), outsider.clone()];
        let (_, _, _, cert) = assemble_certificate(&mixed, 1);
        let mut resolvable = witnesses.clone();
        resolvable.push(outsider);
        let resolve = |id: &WitnessId| {
            resolvable
                .iter()
                .find(|(wid, _, _)| wid == id)
                .map(|(_, _, vk)| vk.clone())
        };
        assert_eq!(
            cert.verify(&policy, resolve),
            Err(IdentityError::WitnessNotInPolicy)
        );
    }

    #[test]
    fn a_certificate_with_a_stale_generation_is_rejected() {
        let witnesses: Vec<_> = (0..2).map(|_| a_witness()).collect();
        let policy = WitnessPolicy::new(
            2,
            witnesses.iter().map(|(id, _, _)| id.clone()).collect(),
            1,
        )
        .unwrap();
        // Certificate claims generation 1, but the active policy is
        // generation 2 -- must be rejected before any signature check.
        let (_, _, _, cert) = assemble_certificate(&witnesses, 1);
        let resolve = |id: &WitnessId| {
            witnesses
                .iter()
                .find(|(wid, _, _)| wid == id)
                .map(|(_, _, vk)| vk.clone())
        };
        assert_eq!(
            cert.verify(&policy, resolve),
            Err(IdentityError::WitnessPolicyGenerationMismatch {
                expected: 2,
                got: 1
            })
        );
    }

    #[test]
    fn assembling_a_mismatched_receipt_is_rejected() {
        let (w1, key1, _) = a_witness();
        let (w2, key2, _) = a_witness();
        let identity = {
            let c = Controller::incept_single().unwrap();
            Did::parse(&format!("did:mini:{}", c.scid())).unwrap()
        };
        let good = sign_witness_receipt(
            WitnessReceiptStatement {
                version: WitnessReceiptVersion::V1,
                identity: identity.clone(),
                sequence: 1,
                event_digest: vec![1u8; 34],
                prior_event_digest: None,
                event_kind: KeyEventKind::Inception,
                witness_policy_generation: 1,
                witness_id: w1,
                observed_epoch: 1,
            },
            &key1,
        );
        // Different event digest -- must not be admitted into the same
        // certificate as `good`.
        let mismatched = sign_witness_receipt(
            WitnessReceiptStatement {
                version: WitnessReceiptVersion::V1,
                identity: identity.clone(),
                sequence: 1,
                event_digest: vec![2u8; 34],
                prior_event_digest: None,
                event_kind: KeyEventKind::Inception,
                witness_policy_generation: 1,
                witness_id: w2,
                observed_epoch: 1,
            },
            &key2,
        );
        assert_eq!(
            WitnessedEventCertificate::assemble(
                identity,
                1,
                vec![1u8; 34],
                1,
                vec![good, mismatched],
            ),
            Err(IdentityError::WitnessReceiptMismatch)
        );
    }

    #[test]
    fn a_certificate_round_trips() {
        let witnesses: Vec<_> = (0..2).map(|_| a_witness()).collect();
        let (_, _, _, cert) = assemble_certificate(&witnesses, 1);
        let decoded = WitnessedEventCertificate::decode(&cert.encode()).unwrap();
        assert_eq!(decoded, cert);
    }

    #[test]
    fn an_unresolvable_witness_key_is_rejected_rather_than_skipped() {
        let witnesses: Vec<_> = (0..2).map(|_| a_witness()).collect();
        let policy = WitnessPolicy::new(
            1,
            witnesses.iter().map(|(id, _, _)| id.clone()).collect(),
            2,
        )
        .unwrap();
        let (_, _, _, cert) = assemble_certificate(&witnesses, 1);
        // A resolver that can never find any key -- must fail closed, not
        // silently treat an unresolvable witness as absent-and-skippable.
        let resolve = |_: &WitnessId| None;
        assert_eq!(
            cert.verify(&policy, resolve),
            Err(IdentityError::UnresolvedWitnessKey)
        );
    }

    #[test]
    fn trailing_bytes_are_rejected_on_every_decode() {
        let (w1, key, _) = a_witness();
        let statement = a_statement(w1.clone(), 1);
        let mut policy_bytes = WitnessPolicy::new(1, vec![w1.clone()], 1).unwrap().encode();
        policy_bytes.push(0xFF);
        assert!(WitnessPolicy::decode(&policy_bytes).is_err());

        let mut statement_bytes = statement.encode();
        statement_bytes.push(0xFF);
        assert!(WitnessReceiptStatement::decode(&statement_bytes).is_err());

        let receipt = sign_witness_receipt(a_statement(w1, 1), &key);
        let mut receipt_bytes = receipt.encode();
        receipt_bytes.push(0xFF);
        assert!(WitnessReceipt::decode(&receipt_bytes).is_err());
    }
}
