//! AI-generated/AI-mediated disclosure objects (roadmap #63, Phase 8.5).
//!
//! Directive 12 and constitution principle 8 require that AI participation
//! is always labeled and never laundered into looking like human authorship
//! or human governance participation. [`crate::Object`] (a `POST`, a
//! `COMMENT`, a `FILTER_LABEL`, a governance `VOTE`-capable signer, ...) is
//! the human-authored/human-governance envelope: its `verify_provenance`
//! proves a delegated *device* of a *human root* authored the bytes. An
//! `AiObject` is a **structurally separate type** — not an [`ObjectType`]
//! variant, not an optional "is_ai" flag on [`crate::Object`] that a caller
//! could simply leave unset.
//!
//! Three enforcement layers, matching the three things a launderer would
//! need to defeat:
//!
//! 1. **Type-level:** `AiObject` is not `Object`. There is no `From<AiObject>
//!    for Object` and no function in this crate accepts one where the other
//!    is expected — `verify_provenance`, `AuthenticatedObjectOwner::verify`,
//!    and every human-governance-facing API in `mini-forge`/`mini-chain`
//!    take `&Object` or their own domain types, never `&AiObject`. Passing
//!    one where the other belongs is a compile error, not a runtime check
//!    someone could forget.
//! 2. **Wire-level:** the two envelopes use different leading version tags
//!    ([`AI_ENVELOPE_TAG`] vs. `Object`'s `1`), so `Object::from_bytes` on
//!    AI-object bytes and `AiObject::from_bytes` on human-object bytes both
//!    fail rather than silently decoding into the wrong kind.
//! 3. **Capability-level:** authoring one requires the signing device to
//!    hold `Capabilities::AI_DISCLOSE` — a bit disjoint from `POST` (human
//!    content) and `VOTE`/`ATTEST` (human governance/co-presence), off in
//!    both `Capabilities::primary()`/`secondary()` secure defaults, and
//!    never implied by holding those. A device authorized to post as a
//!    human, or to cast the root's governance vote, cannot sign an AI
//!    disclosure object on the strength of that alone, and a device
//!    authorized for AI disclosure cannot vote or post as human content on
//!    the strength of that alone.
//!
//! [`AiProvenance`] is mandatory constructor state, not a setter a caller
//! can skip: [`AiObjectBuilder::new`] takes it directly, so there is no
//! "AI object with unlabeled origin" value this crate's public API can
//! construct.
//!
//! ```compile_fail
//! use mini_objects::{verify_provenance, AiObject};
//! fn launder(ai: &AiObject, root: &did_mini::Kel, device: &did_mini::Kel) {
//!     // `verify_provenance` is the human-authorship/human-governance
//!     // check; it only accepts `&Object`, never `&AiObject` — this does
//!     // not compile, which is the point.
//!     let _ = verify_provenance(ai, root, device);
//! }
//! ```

use did_mini::{verify_delegation, Capabilities, Controller, Did, IndexedSig, Kel};
use mini_crypto::{Signature, SignatureSuite};

use crate::codec::{Reader, Writer};
use crate::error::{ObjectError, Result};
use crate::object::{parse_did, ObjectId, MAX_LINKS, MAX_PAYLOAD_BYTES};
use crate::{Link, Payload};

/// Leading version byte for the AI-disclosure envelope. Deliberately not `1`
/// ([`crate::Object`]'s own leading byte): any bytes fed to the wrong
/// decoder fail on this very first byte instead of decoding into the wrong
/// kind of object.
pub const AI_ENVELOPE_TAG: u8 = 0xA1;

const MAX_SYSTEM_ID_BYTES: usize = 128;
const MAX_MODEL_ID_BYTES: usize = 128;
const MAX_DID_BYTES: usize = 256;
/// Mirrors `did_mini::MAX_SIGNATURES` (see `object.rs`'s identical mirror
/// and its F-10 rationale).
const MAX_SIGNATURES: usize = did_mini::MAX_SIGNATURES;
const MAX_SIG_BYTES: usize = did_mini::MAX_SIGNATURE_BYTES;

/// What kind of AI participation this object discloses. A closed,
/// non-extensible set (unlike [`crate::ObjectType`]'s custom tail) on
/// purpose: adding a new AI-participation shape is a protocol decision,
/// not something a caller should be able to name into existence with an
/// arbitrary string the way `ObjectType::Custom` lets community content
/// types do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiOrigin {
    /// Content a model produced (a summary, a draft reply, generated media,
    /// ...). Never a human's own words, however lightly edited.
    GeneratedContent,
    /// A moderation/labeling/routing decision an AI system made or
    /// materially drove (e.g. an automated filter label, an AI-suggested
    /// review finding). Never a human governance action: it cannot satisfy
    /// any vote, approval, or human-attestation requirement anywhere in
    /// this workspace.
    MediatedDecision,
}

impl AiOrigin {
    fn tag(self) -> u8 {
        match self {
            AiOrigin::GeneratedContent => 0,
            AiOrigin::MediatedDecision => 1,
        }
    }

    fn from_tag(tag: u8) -> Result<Self> {
        match tag {
            0 => Ok(AiOrigin::GeneratedContent),
            1 => Ok(AiOrigin::MediatedDecision),
            _ => Err(ObjectError::BadObject),
        }
    }

    /// A short, honest, render-ready label. Consumers should show this
    /// verbatim (or a localized equivalent) wherever the object's content
    /// appears — never blank, never a human-looking byline.
    pub fn disclosure_label(self) -> &'static str {
        match self {
            AiOrigin::GeneratedContent => "AI-generated content — not human-authored",
            AiOrigin::MediatedDecision => "AI-mediated decision — not a human governance action",
        }
    }
}

/// Mandatory provenance for an AI disclosure object: what produced it and
/// when. There is no constructor path that leaves this unset or empty —
/// [`AiObjectBuilder::new`] requires it, and [`AiObject::from_bytes`]
/// rejects empty `system_id`/`model_id` on decode (`MissingAiProvenance`),
/// so a decoded object's provenance is guaranteed non-empty by the type,
/// not by caller discipline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiProvenance {
    /// The system that produced or mediated this object, e.g.
    /// `"mini-forge-review-assistant/0.3.0"`. Identifies *this project's*
    /// integration, not just the underlying model.
    pub system_id: String,
    /// The underlying model/vendor label, e.g. `"anthropic:claude-..."` or
    /// `"local:llama-3-8b"`. Honesty over polish: name what actually ran.
    pub model_id: String,
    /// When the system produced this output (ms). Distinct from the
    /// object's own `timestamp_ms` field so a later re-disclosure can be
    /// told apart from original production time if the two ever diverge.
    pub produced_at_ms: u64,
}

impl AiProvenance {
    fn validate(&self) -> Result<()> {
        if self.system_id.is_empty()
            || self.system_id.len() > MAX_SYSTEM_ID_BYTES
            || self.model_id.is_empty()
            || self.model_id.len() > MAX_MODEL_ID_BYTES
        {
            return Err(ObjectError::MissingAiProvenance);
        }
        Ok(())
    }
}

/// One signed, typed, content-addressed AI-disclosure object. See the
/// module docs for why this is not [`crate::Object`] and cannot be
/// substituted for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiObject {
    id: ObjectId,
    suite: SignatureSuite,
    /// What kind of AI participation this discloses.
    pub origin: AiOrigin,
    /// Mandatory: what produced this object, and when.
    pub provenance: AiProvenance,
    /// The human/organization root operating the AI system that produced
    /// this object — accountability for *running* the system, never an
    /// authorship claim over the content, and never usable to satisfy a
    /// human-attestation or human-authored-content check (there is no
    /// function anywhere in this workspace that accepts an `&AiObject`
    /// where a human authorship/attestation proof is required).
    pub operator_human: Did,
    /// The delegated device that signed, on the operator's behalf.
    pub operator_device: Did,
    /// References to other objects (e.g. the human-authored object a
    /// `MediatedDecision` concerns).
    pub links: Vec<Link>,
    /// The content.
    pub payload: Payload,
    signatures: Vec<IndexedSig>,
}

impl AiObject {
    /// The content id.
    pub fn id(&self) -> &ObjectId {
        &self.id
    }

    /// The device signatures.
    pub fn signatures(&self) -> &[IndexedSig] {
        &self.signatures
    }

    fn signing_bytes(&self) -> Vec<u8> {
        self.encode(EncodeMode::Signing)
    }

    /// Canonical full bytes — what the id is derived from and what travels.
    pub fn to_bytes(&self) -> Vec<u8> {
        self.encode(EncodeMode::Full)
    }

    fn encode(&self, mode: EncodeMode) -> Vec<u8> {
        let mut w = Writer::new();
        w.u8(AI_ENVELOPE_TAG);
        w.u8(self.suite.tag());
        w.u8(self.origin.tag());
        w.bytes(self.provenance.system_id.as_bytes());
        w.bytes(self.provenance.model_id.as_bytes());
        w.u64(self.provenance.produced_at_ms);
        w.bytes(self.operator_human.as_str().as_bytes());
        w.bytes(self.operator_device.as_str().as_bytes());
        w.u32(self.links.len() as u32);
        for l in &self.links {
            w.bytes(l.rel.as_bytes());
            w.bytes(l.target.as_str().as_bytes());
        }
        match &self.payload {
            Payload::Public(b) => {
                w.u8(0);
                w.bytes(b);
            }
            Payload::Encrypted(b) => {
                w.u8(1);
                w.bytes(b);
            }
        }
        if mode == EncodeMode::Full {
            w.u32(self.signatures.len() as u32);
            for s in &self.signatures {
                w.u32(s.index);
                w.u8(s.signature.suite().tag());
                w.bytes(&s.signature.to_bytes());
            }
        }
        w.into_bytes()
    }

    /// Decode an AI-disclosure object from untrusted bytes. Fails with
    /// [`ObjectError::WrongEnvelopeKind`] if the leading byte belongs to
    /// the human-authored [`crate::Object`] envelope instead.
    pub fn from_bytes(bytes: &[u8]) -> Result<AiObject> {
        let mut r = Reader::new(bytes);
        let tag = r.u8()?;
        if tag != AI_ENVELOPE_TAG {
            return Err(ObjectError::WrongEnvelopeKind);
        }
        let suite = SignatureSuite::from_tag(r.u8()?).map_err(ObjectError::Crypto)?;
        let origin = AiOrigin::from_tag(r.u8()?)?;
        let system_id_bytes = r.bytes_limited(MAX_SYSTEM_ID_BYTES)?;
        let system_id = String::from_utf8(system_id_bytes).map_err(|_| ObjectError::BadObject)?;
        let model_id_bytes = r.bytes_limited(MAX_MODEL_ID_BYTES)?;
        let model_id = String::from_utf8(model_id_bytes).map_err(|_| ObjectError::BadObject)?;
        let produced_at_ms = r.u64()?;
        let provenance = AiProvenance {
            system_id,
            model_id,
            produced_at_ms,
        };
        provenance.validate()?;

        let operator_human = parse_did(r.bytes_limited(MAX_DID_BYTES)?)?;
        let operator_device = parse_did(r.bytes_limited(MAX_DID_BYTES)?)?;

        let nlinks = r.u32()? as usize;
        if nlinks > MAX_LINKS {
            return Err(ObjectError::LimitExceeded);
        }
        let mut links = Vec::with_capacity(nlinks);
        for _ in 0..nlinks {
            let rel_bytes = r.bytes_limited(32)?;
            let rel = String::from_utf8(rel_bytes).map_err(|_| ObjectError::BadObject)?;
            if rel.is_empty() {
                return Err(ObjectError::BadObject);
            }
            let id_bytes = r.bytes_limited(128)?;
            let id_str = String::from_utf8(id_bytes).map_err(|_| ObjectError::BadObject)?;
            links.push(Link {
                rel,
                target: ObjectId::parse(&id_str)?,
            });
        }

        let payload = match r.u8()? {
            0 => Payload::Public(r.bytes_limited(MAX_PAYLOAD_BYTES)?),
            1 => Payload::Encrypted(r.bytes_limited(MAX_PAYLOAD_BYTES)?),
            _ => return Err(ObjectError::BadObject),
        };

        let nsigs = r.u32()? as usize;
        if nsigs == 0 || nsigs > MAX_SIGNATURES {
            return Err(ObjectError::LimitExceeded);
        }
        let mut signatures = Vec::with_capacity(nsigs);
        for _ in 0..nsigs {
            let index = r.u32()?;
            let sig_suite = SignatureSuite::from_tag(r.u8()?).map_err(ObjectError::Crypto)?;
            let sig_bytes = r.bytes_limited(MAX_SIG_BYTES)?;
            let signature =
                Signature::from_suite_bytes(sig_suite, &sig_bytes).map_err(ObjectError::Crypto)?;
            signatures.push(IndexedSig { index, signature });
        }
        if !did_mini::signatures_are_canonical(&signatures) {
            return Err(ObjectError::NoncanonicalSignatureOrder);
        }
        if !r.finished() {
            return Err(ObjectError::TrailingBytes);
        }

        let mut obj = AiObject {
            id: ObjectId::placeholder(),
            suite,
            origin,
            provenance,
            operator_human,
            operator_device,
            links,
            payload,
            signatures,
        };
        obj.id = ObjectId::of(&obj.to_bytes());
        Ok(obj)
    }

    /// Layer 1 — integrity: confirm `claimed` names exactly these bytes.
    pub fn verify_integrity(&self, claimed: &ObjectId) -> Result<()> {
        if &self.id == claimed {
            Ok(())
        } else {
            Err(ObjectError::IdMismatch)
        }
    }

    /// Layer 2 — authenticity: the named device signed these bytes.
    pub fn verify_signature(&self, device: &Kel) -> Result<()> {
        if device.did().as_str() != self.operator_device.as_str() {
            return Err(ObjectError::DeviceMismatch);
        }
        device
            .verify_message(&self.signing_bytes(), &self.signatures)
            .map_err(ObjectError::Identity)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum EncodeMode {
    Signing,
    Full,
}

/// Layer 3 — provenance: the signing device is a delegated, unrevoked
/// device of the object's named operator root, holding
/// [`Capabilities::AI_DISCLOSE`]. Deliberately a different capability than
/// [`Capabilities::POST`] (human content) and [`Capabilities::VOTE`]/
/// [`Capabilities::ATTEST`] (human governance/co-presence): a device
/// authorized for one is not, on that basis alone, authorized for the
/// other. This is the only function in this crate that authenticates an
/// `AiObject`, and it never returns anything usable to satisfy a
/// human-authorship or human-governance check elsewhere.
pub fn verify_ai_provenance(object: &AiObject, root: &Kel, device: &Kel) -> Result<Capabilities> {
    if root.did().as_str() != object.operator_human.as_str() {
        return Err(ObjectError::DeviceMismatch);
    }
    object.verify_signature(device)?;
    let caps = verify_delegation(root, device).map_err(ObjectError::Identity)?;
    if !caps.contains(Capabilities::AI_DISCLOSE) {
        return Err(ObjectError::MissingCapability);
    }
    Ok(caps)
}

/// Builds and signs an AI-disclosure object on-device. `provenance` is a
/// required constructor argument, not a settable-or-not field, so there is
/// no builder state representing an unlabeled AI object.
#[derive(Debug)]
pub struct AiObjectBuilder {
    origin: AiOrigin,
    provenance: AiProvenance,
    links: Vec<Link>,
    payload: Payload,
}

impl AiObjectBuilder {
    /// Start a new AI-disclosure object of `origin`, with its mandatory
    /// `provenance` supplied up front.
    pub fn new(origin: AiOrigin, provenance: AiProvenance) -> Self {
        AiObjectBuilder {
            origin,
            provenance,
            links: Vec::new(),
            payload: Payload::Public(Vec::new()),
        }
    }

    /// Add a typed link to another object.
    pub fn link(mut self, rel: &str, target: ObjectId) -> Self {
        self.links.push(Link {
            rel: rel.to_string(),
            target,
        });
        self
    }

    /// Set the payload.
    pub fn payload(mut self, payload: Payload) -> Self {
        self.payload = payload;
        self
    }

    /// Sign with the operating device and seal the content id. Fails with
    /// [`ObjectError::MissingAiProvenance`] before any signing happens if
    /// `provenance`'s `system_id`/`model_id` are empty or oversized.
    pub fn sign(self, operator_human: &Did, device: &Controller) -> Result<AiObject> {
        self.provenance.validate()?;
        if self.links.len() > MAX_LINKS {
            return Err(ObjectError::LimitExceeded);
        }
        let payload_len = match &self.payload {
            Payload::Public(b) | Payload::Encrypted(b) => b.len(),
        };
        if payload_len > MAX_PAYLOAD_BYTES {
            return Err(ObjectError::LimitExceeded);
        }
        let mut obj = AiObject {
            id: ObjectId::placeholder(),
            suite: device.key_state().keys[0].suite(),
            origin: self.origin,
            provenance: self.provenance,
            operator_human: operator_human.clone(),
            operator_device: device.did(),
            links: self.links,
            payload: self.payload,
            signatures: Vec::new(),
        };
        obj.signatures = device.sign_message(&obj.signing_bytes());
        obj.id = ObjectId::of(&obj.to_bytes());
        Ok(obj)
    }
}
