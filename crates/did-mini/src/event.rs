//! Key events — the entries of a `did:mini` Key Event Log (SPEC-01 §4).
//!
//! Event kinds (KERI's establishment / non-establishment split):
//!   - **Inception** (`icp`): establishes the identifier (SCID), the initial key
//!     set + signing threshold, and the *pre-rotation commitment* to the next key
//!     set (SPEC-01 §5). sn 0, no prior.
//!   - **Delegated inception** (`dip`): like `icp`, but the identifier also commits
//!     to a delegator — a device's genesis under a human-root (SPEC-01 §6).
//!   - **Rotation** (`rot`): reveals the pre-committed next keys as the new current
//!     keys and commits to a fresh next set (SPEC-01 §5).
//!   - **Interaction** (`ixn`): anchors arbitrary data under the current keys.
//!   - **Seal** (`sl`): anchors delegation/revocation seals under the current keys
//!     — a human-root authorizing or revoking devices (SPEC-01 §6).
//!
//! ## Serialization is the security boundary
//!
//! Every event has exactly one canonical byte layout (see [`Mode`]); digests and
//! signatures are computed over those bytes, so an event verified on one device is
//! verified identically on another.

use mini_crypto::encoding;
use mini_crypto::{HashAlgorithm, Multihash, Signature, SignatureSuite, VerifyingKey};

use crate::codec::{Reader, Writer};
use crate::delegation::{decode_seal, encode_seal, Seal};
use crate::error::{IdentityError, Result};
use crate::limits::{
    MAX_ANCHORS, MAX_DID_BYTES, MAX_KEYS, MAX_KEY_BYTES, MAX_MULTIHASH_BYTES, MAX_NEXT,
    MAX_PRIOR_BYTES, MAX_SCID_BYTES, MAX_SEALS, MAX_SIGNATURES, MAX_SIGNATURE_BYTES, MAX_WITNESSES,
};

pub(crate) const TAG_ICP: u8 = 0x01;
pub(crate) const TAG_ROT: u8 = 0x02;
pub(crate) const TAG_IXN: u8 = 0x03;
pub(crate) const TAG_SEAL: u8 = 0x04;
pub(crate) const TAG_DIP: u8 = 0x05;

/// Establishment configuration carried by an inception or rotation event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Establishment {
    /// The keys authoritative *after* this event.
    pub keys: Vec<VerifyingKey>,
    /// How many of `keys` must sign to act (M-of-N).
    pub threshold: u32,
    /// Pre-rotation commitments: the multihash bytes of each *next* key. The next
    /// keys themselves stay secret until the rotation that reveals them.
    pub next: Vec<Vec<u8>>,
    /// The threshold that will apply to the next key set.
    pub next_threshold: u32,
    /// Witness identifiers (SPEC-01 §7). Reserved; empty until the witness batch.
    pub witnesses: Vec<Vec<u8>>,
}

/// The body of a key event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventKind {
    /// Genesis event: establishes the identifier and initial control.
    Inception(Establishment),
    /// Genesis event for a delegated (device) identifier, committing to its
    /// delegator's `did:mini` string (SPEC-01 §6).
    DelegatedInception {
        /// The establishment config (this device's own keys + pre-rotation).
        establishment: Establishment,
        /// The delegator's `did:mini:<scid>` string (the human-root).
        delegator: String,
    },
    /// Rotates control to the pre-committed next keys.
    Rotation(Establishment),
    /// Anchors data under the current keys without changing control.
    Interaction {
        /// Arbitrary 32-byte seals (e.g. content digests).
        anchors: Vec<[u8; 32]>,
    },
    /// Anchors delegation/revocation seals under the current keys (SPEC-01 §6).
    Seal {
        /// The delegation seals carried by this event.
        seals: Vec<Seal>,
    },
}

/// A signature together with the index of the key (within the authoritative set)
/// that produced it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexedSig {
    /// Index into the authoritative key set.
    pub index: u32,
    /// The signature over the event's signing bytes.
    pub signature: Signature,
}

/// A single, signed key event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    /// Signature suite for this event's keys and signatures.
    pub suite: SignatureSuite,
    /// The identifier this event belongs to (`<scid>`, the multibase string).
    pub scid: String,
    /// Sequence number; 0 for inception.
    pub sn: u64,
    /// Multihash bytes of the prior event; empty for inception.
    pub prior: Vec<u8>,
    /// The event body.
    pub kind: EventKind,
    /// Signatures by the keys authoritative for this event.
    pub signatures: Vec<IndexedSig>,
}

/// Which fields to include when serialising an event.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mode {
    /// Input to SCID derivation: identifier field blanked, signatures omitted.
    ScidInput,
    /// Bytes that signatures are computed over: identifier present, signatures
    /// omitted.
    Signing,
    /// The full wire/storage form: identifier present, signatures included.
    Full,
}

impl Event {
    /// Serialise this event in the given [`Mode`].
    pub(crate) fn encode(&self, mode: Mode) -> Vec<u8> {
        let mut w = Writer::new();
        w.u8(self.tag());
        w.u8(self.suite.tag());
        // The identifier is blanked for SCID derivation so the SCID cannot depend
        // on itself (it is derived *from* this very serialization).
        if mode == Mode::ScidInput {
            w.bytes(b"");
        } else {
            w.bytes(self.scid.as_bytes());
        }
        w.u64(self.sn);
        w.bytes(&self.prior);
        match &self.kind {
            EventKind::Inception(est) | EventKind::Rotation(est) => {
                encode_establishment(&mut w, est)
            }
            EventKind::DelegatedInception {
                establishment,
                delegator,
            } => {
                encode_establishment(&mut w, establishment);
                w.bytes(delegator.as_bytes());
            }
            EventKind::Interaction { anchors } => {
                w.u32(anchors.len() as u32);
                for a in anchors {
                    w.bytes(a);
                }
            }
            EventKind::Seal { seals } => {
                w.u32(seals.len() as u32);
                for s in seals {
                    encode_seal(&mut w, s);
                }
            }
        }
        if mode == Mode::Full {
            w.u32(self.signatures.len() as u32);
            for s in &self.signatures {
                w.u32(s.index);
                w.u8(s.signature.suite().tag());
                w.bytes(&s.signature.to_bytes());
            }
        }
        w.into_bytes()
    }

    pub(crate) fn tag(&self) -> u8 {
        match &self.kind {
            EventKind::Inception(_) => TAG_ICP,
            EventKind::DelegatedInception { .. } => TAG_DIP,
            EventKind::Rotation(_) => TAG_ROT,
            EventKind::Interaction { .. } => TAG_IXN,
            EventKind::Seal { .. } => TAG_SEAL,
        }
    }

    /// The bytes signatures are computed over (identifier present, no signatures).
    pub(crate) fn signing_bytes(&self) -> Vec<u8> {
        self.encode(Mode::Signing)
    }

    /// The full wire/storage bytes (identifier present, signatures included).
    pub(crate) fn full_bytes(&self) -> Vec<u8> {
        self.encode(Mode::Full)
    }

    /// The content digest of this event (over its full bytes), as multihash bytes.
    /// This is what the next event references in its `prior` field.
    pub(crate) fn digest(&self) -> Vec<u8> {
        Multihash::of(HashAlgorithm::Blake3, &self.full_bytes()).to_bytes()
    }
}

fn encode_establishment(w: &mut Writer, est: &Establishment) {
    w.u32(est.keys.len() as u32);
    for k in &est.keys {
        w.u8(k.suite().tag());
        w.bytes(&k.to_bytes());
    }
    w.u32(est.threshold);
    w.u32(est.next.len() as u32);
    for n in &est.next {
        w.bytes(n);
    }
    w.u32(est.next_threshold);
    w.u32(est.witnesses.len() as u32);
    for wit in &est.witnesses {
        w.bytes(wit);
    }
}

/// Decode a full-form event from `r`.
pub(crate) fn decode(r: &mut Reader) -> Result<Event> {
    let tag = r.u8()?;
    let suite = SignatureSuite::from_tag(r.u8()?)?;
    let scid = String::from_utf8(r.bytes_limited("event.scid", MAX_SCID_BYTES)?)
        .map_err(|_| IdentityError::BadEvent)?;
    let sn = r.u64()?;
    let prior = r.bytes_limited("event.prior", MAX_PRIOR_BYTES)?;
    let kind = match tag {
        TAG_ICP => EventKind::Inception(decode_establishment(r)?),
        TAG_DIP => {
            let establishment = decode_establishment(r)?;
            let delegator = String::from_utf8(r.bytes_limited("event.delegator", MAX_DID_BYTES)?)
                .map_err(|_| IdentityError::BadEvent)?;
            crate::Did::parse(&delegator)?;
            EventKind::DelegatedInception {
                establishment,
                delegator,
            }
        }
        TAG_ROT => EventKind::Rotation(decode_establishment(r)?),
        TAG_IXN => {
            let n = checked_count(r.u32()? as usize, MAX_ANCHORS, "anchors")?;
            let mut anchors = Vec::with_capacity(n);
            for _ in 0..n {
                let b = r.bytes_limited("anchor", 32)?;
                let arr: [u8; 32] = b
                    .as_slice()
                    .try_into()
                    .map_err(|_| IdentityError::BadEvent)?;
                anchors.push(arr);
            }
            EventKind::Interaction { anchors }
        }
        TAG_SEAL => {
            let n = checked_count(r.u32()? as usize, MAX_SEALS, "seals")?;
            let mut seals = Vec::with_capacity(n);
            for _ in 0..n {
                seals.push(decode_seal(r)?);
            }
            EventKind::Seal { seals }
        }
        other => return Err(IdentityError::UnknownEventTag(other)),
    };
    let nsig = checked_count(r.u32()? as usize, MAX_SIGNATURES, "signatures")?;
    let mut signatures = Vec::with_capacity(nsig);
    for _ in 0..nsig {
        let index = r.u32()?;
        let sig_suite = SignatureSuite::from_tag(r.u8()?)?;
        let sig_bytes = r.bytes_limited("signature", MAX_SIGNATURE_BYTES)?;
        let signature = Signature::from_suite_bytes(sig_suite, &sig_bytes)?;
        signatures.push(IndexedSig { index, signature });
    }
    Ok(Event {
        suite,
        scid,
        sn,
        prior,
        kind,
        signatures,
    })
}

fn decode_establishment(r: &mut Reader) -> Result<Establishment> {
    let nkeys = checked_count(r.u32()? as usize, MAX_KEYS, "keys")?;
    let mut keys = Vec::with_capacity(nkeys);
    for _ in 0..nkeys {
        let suite = SignatureSuite::from_tag(r.u8()?)?;
        let key_bytes = r.bytes_limited("key", MAX_KEY_BYTES)?;
        keys.push(VerifyingKey::from_suite_bytes(suite, &key_bytes)?);
    }
    let threshold = r.u32()?;
    let nnext = checked_count(r.u32()? as usize, MAX_NEXT, "next commitments")?;
    let mut next = Vec::with_capacity(nnext);
    for _ in 0..nnext {
        let commitment = r.bytes_limited("next commitment", MAX_MULTIHASH_BYTES)?;
        Multihash::from_bytes(&commitment)?;
        next.push(commitment);
    }
    let next_threshold = r.u32()?;
    let nwit = checked_count(r.u32()? as usize, MAX_WITNESSES, "witnesses")?;
    let mut witnesses = Vec::with_capacity(nwit);
    for _ in 0..nwit {
        witnesses.push(r.bytes_limited("witness", MAX_DID_BYTES)?);
    }
    let est = Establishment {
        keys,
        threshold,
        next,
        next_threshold,
        witnesses,
    };
    validate_establishment(&est)?;
    Ok(est)
}

pub(crate) fn validate_establishment(est: &Establishment) -> Result<()> {
    if est.keys.is_empty() {
        return Err(IdentityError::EmptyKeySet);
    }
    if est.threshold == 0 || est.threshold as usize > est.keys.len() {
        return Err(IdentityError::InvalidThreshold {
            threshold: est.threshold,
            key_count: est.keys.len(),
        });
    }
    let mut seen_keys: Vec<Vec<u8>> = Vec::new();
    for key in &est.keys {
        let fp = key_fingerprint(key);
        if seen_keys.contains(&fp) {
            return Err(IdentityError::DuplicateKey);
        }
        seen_keys.push(fp);
    }
    // Pre-public profile decision: every establishment event must commit to a
    // next key set. A future KERI-style retirement must be a distinct explicit
    // event kind, not an ambiguous empty pre-rotation commitment.
    if est.next.is_empty()
        || est.next_threshold == 0
        || est.next_threshold as usize > est.next.len()
    {
        return Err(IdentityError::InvalidNextThreshold {
            threshold: est.next_threshold,
            commitment_count: est.next.len(),
        });
    }
    let mut seen_next: Vec<&[u8]> = Vec::new();
    for commitment in &est.next {
        Multihash::from_bytes(commitment)?;
        if seen_next.contains(&commitment.as_slice()) {
            return Err(IdentityError::BadEvent);
        }
        seen_next.push(commitment.as_slice());
    }
    Ok(())
}

fn checked_count(n: usize, max: usize, field: &'static str) -> Result<usize> {
    if n > max {
        return Err(IdentityError::TooManyItems { field, max, got: n });
    }
    Ok(n)
}

fn key_fingerprint(vk: &VerifyingKey) -> Vec<u8> {
    let mut buf = Vec::with_capacity(1 + vk.suite().public_key_len());
    buf.push(vk.suite().tag());
    buf.extend_from_slice(&vk.to_bytes());
    buf
}

/// The pre-rotation commitment for a key: the multihash bytes of its
/// suite-tagged public key.
pub(crate) fn key_commitment(vk: &VerifyingKey) -> Vec<u8> {
    Multihash::of(HashAlgorithm::Blake3, &key_fingerprint(vk)).to_bytes()
}

/// Derive the self-certifying identifier (`<scid>`) from an inception event:
/// `multibase(base58btc, multihash(blake3, scid_input(icp)))`. For a delegated
/// inception this also commits to the delegator, so a device id self-certifies
/// who delegated it (SPEC-01 §3, §6).
pub(crate) fn derive_scid(icp: &Event) -> String {
    let mh = Multihash::of(HashAlgorithm::Blake3, &icp.encode(Mode::ScidInput));
    encoding::encode(encoding::BASE58BTC, &mh.to_bytes())
        .expect("base58btc encoding is always valid")
}

/// Verify that `event` carries at least `threshold` valid signatures from
/// *distinct* keys in `keys`, over the event's signing bytes.
pub(crate) fn verify_threshold(event: &Event, keys: &[VerifyingKey], threshold: u32) -> Result<()> {
    if threshold == 0 {
        return Err(IdentityError::ThresholdNotMet {
            sn: event.sn,
            needed: threshold,
            got: 0,
        });
    }
    let count = count_valid_signers(&event.signing_bytes(), keys, &event.signatures);
    if count >= threshold {
        Ok(())
    } else {
        Err(IdentityError::ThresholdNotMet {
            sn: event.sn,
            needed: threshold,
            got: count,
        })
    }
}

/// Count how many *distinct* keys in `keys` produced a valid signature over `msg`.
///
/// Distinct by both index and public-key fingerprint, so a malformed key set that
/// repeats a public key cannot inflate the count. Shared by event-threshold checks
/// and detached message verification (e.g. presence attestations).
pub(crate) fn count_valid_signers(msg: &[u8], keys: &[VerifyingKey], sigs: &[IndexedSig]) -> u32 {
    let mut seen_indices: Vec<u32> = Vec::new();
    let mut seen_keys: Vec<Vec<u8>> = Vec::new();
    let mut count: u32 = 0;
    for s in sigs {
        let idx = s.index as usize;
        if idx >= keys.len() || seen_indices.contains(&s.index) {
            continue;
        }
        let fp = key_fingerprint(&keys[idx]);
        if seen_keys.contains(&fp) {
            continue;
        }
        if keys[idx].verify(msg, &s.signature).is_ok() {
            seen_indices.push(s.index);
            seen_keys.push(fp);
            count += 1;
        }
    }
    count
}
