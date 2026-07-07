//! The object envelope: canonical encoding, content-address derivation, and
//! layered verification.

use did_mini::{verify_delegation, Capabilities, Controller, Did, IndexedSig, Kel};
use mini_crypto::{encoding, HashAlgorithm, Multihash, Signature, SignatureSuite};

use crate::codec::{Reader, Writer};
use crate::error::{ObjectError, Result};

/// Hard decode limits for untrusted objects (Tier-T; conservative for beta).
pub const MAX_PAYLOAD_BYTES: usize = 8 * 1024 * 1024;
/// Maximum number of links one object may carry.
pub const MAX_LINKS: usize = 256;
const MAX_ID_BYTES: usize = 128;
const MAX_DID_BYTES: usize = 256;
const MAX_TYPE_BYTES: usize = 64;
const MAX_REL_BYTES: usize = 32;
const MAX_SIGNATURES: usize = 16;
const MAX_SIG_BYTES: usize = 256;

/// A content id: multibase(base58btc, multihash(BLAKE3, canonical bytes)).
/// Tamper-evident and self-verifying, like a `did:mini` SCID.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ObjectId(String);

impl ObjectId {
    /// Parse and structurally validate an id (must be base58btc multibase whose
    /// bytes decode to a canonical strong multihash).
    pub fn parse(s: &str) -> Result<Self> {
        if !s.starts_with(encoding::BASE58BTC) {
            return Err(ObjectError::BadObject);
        }
        let bytes = encoding::decode(s).map_err(ObjectError::Crypto)?;
        Multihash::from_bytes(&bytes).map_err(ObjectError::Crypto)?;
        Ok(ObjectId(s.to_string()))
    }

    /// The string form.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn of(bytes: &[u8]) -> ObjectId {
        let mh = Multihash::of(HashAlgorithm::Blake3, bytes);
        ObjectId(
            encoding::encode(encoding::BASE58BTC, &mh.to_bytes())
                .expect("base58btc encoding is always valid"),
        )
    }
}

/// The extensible type tag (SPEC-09: well-known core set + Tier-O custom types).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObjectType {
    /// A well-known core type.
    WellKnown(u16),
    /// A community-defined type, named to avoid collisions (e.g. `"chess/move"`).
    Custom(String),
}

impl ObjectType {
    /// Microblog / feed post.
    pub const POST: ObjectType = ObjectType::WellKnown(1);
    /// Comment / reply.
    pub const COMMENT: ObjectType = ObjectType::WellKnown(2);
    /// Profile document.
    pub const PROFILE: ObjectType = ObjectType::WellKnown(3);
    /// Follow edge.
    pub const FOLLOW: ObjectType = ObjectType::WellKnown(4);
    /// Reaction / support.
    pub const REACTION: ObjectType = ObjectType::WellKnown(5);
    /// Media chunk manifest.
    pub const MEDIA_MANIFEST: ObjectType = ObjectType::WellKnown(6);
    /// Community charter/card.
    pub const COMMUNITY: ObjectType = ObjectType::WellKnown(7);
    /// A CRDT operation (threads, shared docs, forge discussions).
    pub const CRDT_OP: ObjectType = ObjectType::WellKnown(8);
    /// Moderation filter / label (SPEC-10 labeler pattern).
    pub const FILTER_LABEL: ObjectType = ObjectType::WellKnown(9);
    /// Forge commit.
    pub const COMMIT: ObjectType = ObjectType::WellKnown(10);
    /// Release-registry record.
    pub const RELEASE: ObjectType = ObjectType::WellKnown(11);
    /// Signed head pointer (single-author mutable state, SPEC-09 §3): payload =
    /// subject name, one `"target"` link = latest version.
    pub const HEAD: ObjectType = ObjectType::WellKnown(12);
}

/// The payload: the signature always covers the object; encryption only hides
/// the content (SPEC-09 §4). Hosts can serve ciphertext blind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Payload {
    /// Plaintext, anyone can read.
    Public(Vec<u8>),
    /// Ciphertext; only authorized readers hold keys.
    Encrypted(Vec<u8>),
}

/// A typed reference to another object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    /// The relation, short and lowercase (e.g. `"re"`, `"root"`, `"embed"`,
    /// `"prev"`, `"topic"`).
    pub rel: String,
    /// The target object's content id.
    pub target: ObjectId,
}

/// One signed, typed, content-addressed object (SPEC-09 §2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Object {
    /// The content id over the canonical full bytes.
    id: ObjectId,
    /// Signature suite of the signing device's keys.
    suite: SignatureSuite,
    /// What this object is.
    pub object_type: ObjectType,
    /// The authoring human-root (may be a pairwise pseudonym).
    pub author_human: Did,
    /// The delegated device that signed.
    pub author_device: Did,
    /// Author-claimed creation time (ms). Ordering hint, not a proof.
    pub timestamp_ms: u64,
    /// Author-scoped sequence number (ordering hint within one author).
    pub sequence: u64,
    /// References to other objects.
    pub links: Vec<Link>,
    /// The content.
    pub payload: Payload,
    /// Device signatures over the signing bytes.
    signatures: Vec<IndexedSig>,
}

impl Object {
    /// The content id.
    pub fn id(&self) -> &ObjectId {
        &self.id
    }

    /// The device signatures.
    pub fn signatures(&self) -> &[IndexedSig] {
        &self.signatures
    }

    /// Canonical bytes without id and signatures — what the device signs.
    fn signing_bytes(&self) -> Vec<u8> {
        self.encode(EncodeMode::Signing)
    }

    /// Canonical full bytes — what the id is derived from and what travels.
    pub fn to_bytes(&self) -> Vec<u8> {
        self.encode(EncodeMode::Full)
    }

    fn encode(&self, mode: EncodeMode) -> Vec<u8> {
        let mut w = Writer::new();
        w.u8(1); // envelope version
        w.u8(self.suite.tag());
        match &self.object_type {
            ObjectType::WellKnown(t) => {
                w.u16(*t);
                w.bytes(b"");
            }
            ObjectType::Custom(name) => {
                w.u16(u16::MAX);
                w.bytes(name.as_bytes());
            }
        }
        w.bytes(self.author_human.as_str().as_bytes());
        w.bytes(self.author_device.as_str().as_bytes());
        w.u64(self.timestamp_ms);
        w.u64(self.sequence);
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

    /// Decode an object from untrusted bytes (bounded), verifying that its
    /// content id matches the bytes. Signature/provenance are separate layers.
    pub fn from_bytes(bytes: &[u8]) -> Result<Object> {
        let mut r = Reader::new(bytes);
        if r.u8()? != 1 {
            return Err(ObjectError::BadObject);
        }
        let suite = SignatureSuite::from_tag(r.u8()?).map_err(ObjectError::Crypto)?;
        let type_tag = r.u16()?;
        let type_name = r.bytes_limited(MAX_TYPE_BYTES)?;
        let object_type = if type_tag == u16::MAX {
            let name = String::from_utf8(type_name).map_err(|_| ObjectError::BadObject)?;
            if name.is_empty() {
                return Err(ObjectError::BadObject);
            }
            ObjectType::Custom(name)
        } else {
            if !type_name.is_empty() {
                return Err(ObjectError::BadObject);
            }
            ObjectType::WellKnown(type_tag)
        };
        let author_human = parse_did(r.bytes_limited(MAX_DID_BYTES)?)?;
        let author_device = parse_did(r.bytes_limited(MAX_DID_BYTES)?)?;
        let timestamp_ms = r.u64()?;
        let sequence = r.u64()?;
        let nlinks = r.u32()? as usize;
        if nlinks > MAX_LINKS {
            return Err(ObjectError::LimitExceeded);
        }
        let mut links = Vec::with_capacity(nlinks);
        for _ in 0..nlinks {
            let rel_bytes = r.bytes_limited(MAX_REL_BYTES)?;
            let rel = String::from_utf8(rel_bytes).map_err(|_| ObjectError::BadObject)?;
            if rel.is_empty() {
                return Err(ObjectError::BadObject);
            }
            let id_bytes = r.bytes_limited(MAX_ID_BYTES)?;
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
        if !r.finished() {
            return Err(ObjectError::TrailingBytes);
        }

        let mut obj = Object {
            id: ObjectId(String::new()),
            suite,
            object_type,
            author_human,
            author_device,
            timestamp_ms,
            sequence,
            links,
            payload,
            signatures,
        };
        obj.id = ObjectId::of(&obj.to_bytes());
        Ok(obj)
    }

    /// Layer 1 — integrity: confirm `claimed` names exactly these bytes. Any
    /// holder can check this with no keys at all.
    pub fn verify_integrity(&self, claimed: &ObjectId) -> Result<()> {
        if &self.id == claimed {
            Ok(())
        } else {
            Err(ObjectError::IdMismatch)
        }
    }

    /// Layer 2 — authenticity: the named device signed these bytes, verified
    /// against the device's KEL (current key state, distinct-key threshold).
    pub fn verify_signature(&self, device: &Kel) -> Result<()> {
        if device.did().as_str() != self.author_device.as_str() {
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

fn parse_did(bytes: Vec<u8>) -> Result<Did> {
    let s = String::from_utf8(bytes).map_err(|_| ObjectError::BadObject)?;
    Did::parse(&s).map_err(ObjectError::Identity)
}

/// Layer 3 — provenance: the signing device is a delegated, unrevoked device of
/// the object's named human-root, holding the capability this object type
/// requires (`POST` for content; SPEC-01 §6 scoping). Returns the device's
/// capability set.
pub fn verify_provenance(object: &Object, root: &Kel, device: &Kel) -> Result<Capabilities> {
    if root.did().as_str() != object.author_human.as_str() {
        return Err(ObjectError::DeviceMismatch);
    }
    object.verify_signature(device)?;
    let caps = verify_delegation(root, device).map_err(ObjectError::Identity)?;
    if !caps.contains(required_capability(&object.object_type)) {
        return Err(ObjectError::MissingCapability);
    }
    Ok(caps)
}

/// The capability a device must hold to author each type. Content types need
/// `POST`; everything defaults to `SIGN` (narrowest useful scope).
fn required_capability(t: &ObjectType) -> Capabilities {
    match t {
        ObjectType::WellKnown(tag) => match *tag {
            1..=9 | 12 => Capabilities::POST, // content/community types + heads
            _ => Capabilities::SIGN,          // forge/release & unknown well-known
        },
        ObjectType::Custom(_) => Capabilities::POST,
    }
}

/// Builds and signs an object on-device. Secrets never leave the controller;
/// only signatures enter the envelope (G1).
#[derive(Debug)]
pub struct ObjectBuilder {
    object_type: ObjectType,
    timestamp_ms: u64,
    sequence: u64,
    links: Vec<Link>,
    payload: Payload,
}

impl ObjectBuilder {
    /// Start a new object of `object_type`.
    pub fn new(object_type: ObjectType) -> Self {
        ObjectBuilder {
            object_type,
            timestamp_ms: 0,
            sequence: 0,
            links: Vec::new(),
            payload: Payload::Public(Vec::new()),
        }
    }

    /// Author-claimed creation time (ms).
    pub fn timestamp_ms(mut self, t: u64) -> Self {
        self.timestamp_ms = t;
        self
    }

    /// Author-scoped sequence number.
    pub fn sequence(mut self, s: u64) -> Self {
        self.sequence = s;
        self
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

    /// Sign with the authoring device and seal the content id. `human` is the
    /// author identity the object claims; layer-3 verification later proves the
    /// device really is that human's delegate.
    pub fn sign(self, human: &Did, device: &Controller) -> Result<Object> {
        if self.links.len() > MAX_LINKS {
            return Err(ObjectError::LimitExceeded);
        }
        let payload_len = match &self.payload {
            Payload::Public(b) | Payload::Encrypted(b) => b.len(),
        };
        if payload_len > MAX_PAYLOAD_BYTES {
            return Err(ObjectError::LimitExceeded);
        }
        let mut obj = Object {
            id: ObjectId(String::new()),
            suite: device.key_state().keys[0].suite(),
            object_type: self.object_type,
            author_human: human.clone(),
            author_device: device.did(),
            timestamp_ms: self.timestamp_ms,
            sequence: self.sequence,
            links: self.links,
            payload: self.payload,
            signatures: Vec::new(),
        };
        obj.signatures = device.sign_message(&obj.signing_bytes());
        obj.id = ObjectId::of(&obj.to_bytes());
        Ok(obj)
    }
}
