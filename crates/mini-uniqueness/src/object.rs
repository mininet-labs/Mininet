//! Publish/resolve a [`VouchAttestation`] as a real signed, content-addressed
//! object, so it can be stored locally and carried by `mini-sync` like any
//! other object — the same gap `mini-social::post`'s module doc describes
//! closing for posts ("every caller built a raw `ObjectBuilder` directly...
//! no shared, tested decode path a second client could reuse").
//!
//! **Decode and authenticity remain separate**, matching
//! `mini-search-federation`'s own stated convention: [`resolve_vouch`] only
//! bounds-checks and deserializes the two-party attestation exactly as
//! signed; it does not call [`crate::verify_vouch`]. A vouch's real
//! cryptographic proof is the *inner* mutual signature over
//! [`VouchFields::transcript`] — the outer object signature here is only
//! custody: which device chose to publish/carry this attestation, not an
//! assertion that it is valid. A caller must resolve both parties' KELs and
//! call [`crate::verify_vouch`] before treating a resolved attestation as a
//! real mutual vouch.

use did_mini::{Controller, Did, IndexedSig};
use mini_crypto::{Signature, SignatureSuite};
use mini_objects::{Object, ObjectBuilder, ObjectId, ObjectType, Payload};
use mini_store::{Backend, Store};

use crate::error::{Result, UniquenessError};
use crate::vouch::{VouchAttestation, VouchFields, VoucherParty, VOUCH_VERSION};

/// Custom object type tag for a stored vouch attestation.
const VOUCH_OBJECT_TYPE: &str = "mini/vouch-attestation";

/// Bound on the encoded attestation payload: two DIDs, two KEL digests, two
/// nonces, and two small signature sets — generous headroom over a normal
/// Ed25519 attestation's actual size without leaving the bound open-ended.
pub const MAX_VOUCH_BYTES: usize = 4096;

/// Maximum signatures accepted per party (matches
/// `mini-social::pairing`'s own `MAX_PAIRING_SIGNATURES`: one signer today,
/// bounded headroom for a future multi-key device).
const MAX_PARTY_SIGNATURES: usize = 4;
/// Maximum bytes for one encoded signature.
const MAX_SIGNATURE_BYTES: usize = 256;

/// Publish a complete, already mutually-signed [`VouchAttestation`] as a
/// real object. Either party (or anyone holding a copy) may publish it; the
/// object's own author is publishing custody, not a new vouch signature.
pub fn publish_vouch<B: Backend>(
    store: &mut Store<B>,
    publisher_human: &Did,
    publisher_device: &Controller,
    attestation: &VouchAttestation,
    timestamp_ms: u64,
    sequence: u64,
) -> Result<Object> {
    let payload = encode_vouch(attestation)?;
    if payload.len() > MAX_VOUCH_BYTES {
        return Err(UniquenessError::AttestationTooLarge);
    }
    let object = ObjectBuilder::new(ObjectType::Custom(VOUCH_OBJECT_TYPE.to_string()))
        .timestamp_ms(timestamp_ms)
        .sequence(sequence)
        .payload(Payload::Public(payload))
        .sign(publisher_human, publisher_device)
        .map_err(UniquenessError::Object)?;
    store.insert(&object).map_err(UniquenessError::Store)?;
    Ok(object)
}

/// Fetch and decode a stored vouch attestation. Does not verify it — see
/// the module doc.
pub fn resolve_vouch<B: Backend>(store: &Store<B>, id: &ObjectId) -> Result<VouchAttestation> {
    let object = store.get(id).map_err(UniquenessError::Store)?;
    decode_vouch(&object)
}

/// Every vouch attestation `publisher` has published, in deterministic
/// order (newest first, ties broken by id). Malformed entries (e.g. from an
/// old/alternative client) are silently excluded, matching
/// `mini_social::feed`'s own convention.
pub fn vouches_published_by<B: Backend>(
    store: &Store<B>,
    publisher: &Did,
) -> Result<Vec<VouchAttestation>> {
    let mut out = Vec::new();
    for id in store.by_author(publisher).map_err(UniquenessError::Store)? {
        let object = store.get(&id).map_err(UniquenessError::Store)?;
        if let Ok(attestation) = decode_vouch(&object) {
            out.push((object.timestamp_ms, id, attestation));
        }
    }
    out.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.as_str().cmp(a.1.as_str())));
    Ok(out
        .into_iter()
        .map(|(_, _, attestation)| attestation)
        .collect())
}

fn decode_vouch(object: &Object) -> Result<VouchAttestation> {
    if object.object_type != ObjectType::Custom(VOUCH_OBJECT_TYPE.to_string()) {
        return Err(UniquenessError::BadVouchObject);
    }
    let Payload::Public(bytes) = &object.payload else {
        return Err(UniquenessError::BadVouchObject);
    };
    if bytes.len() > MAX_VOUCH_BYTES {
        return Err(UniquenessError::AttestationTooLarge);
    }
    let mut pos = 0;
    let attestation = get_vouch(bytes, &mut pos).ok_or(UniquenessError::BadVouchObject)?;
    if pos != bytes.len() {
        return Err(UniquenessError::BadVouchObject);
    }
    Ok(attestation)
}

fn encode_vouch(attestation: &VouchAttestation) -> Result<Vec<u8>> {
    let mut w = Vec::new();
    let f = &attestation.fields;
    w.push(f.version);
    w.extend_from_slice(&f.channel_binding);
    w.push(f.transport.tag());
    put_party(&mut w, &f.a);
    put_party(&mut w, &f.b);
    w.extend_from_slice(&f.asserted_at_ms.to_be_bytes());
    put_sigs(&mut w, &attestation.a_sig)?;
    put_sigs(&mut w, &attestation.b_sig)?;
    Ok(w)
}

fn get_vouch(b: &[u8], pos: &mut usize) -> Option<VouchAttestation> {
    let version = *b.get(*pos)?;
    *pos += 1;
    if version != VOUCH_VERSION {
        return None;
    }
    let channel_binding: [u8; 32] = b.get(*pos..*pos + 32)?.try_into().ok()?;
    *pos += 32;
    let transport = mini_presence::TransportKind::from_tag(*b.get(*pos)?)?;
    *pos += 1;
    let a = get_party(b, pos)?;
    let party_b = get_party(b, pos)?;
    let asserted_at_ms = u64::from_be_bytes(b.get(*pos..*pos + 8)?.try_into().ok()?);
    *pos += 8;
    let a_sig = get_sigs(b, pos)?;
    let b_sig = get_sigs(b, pos)?;
    Some(VouchAttestation::new(
        VouchFields {
            version,
            channel_binding,
            transport,
            a,
            b: party_b,
            asserted_at_ms,
        },
        a_sig,
        b_sig,
    ))
}

fn put_party(w: &mut Vec<u8>, p: &VoucherParty) {
    let did = p.device.as_str().as_bytes();
    w.extend_from_slice(&(did.len() as u32).to_be_bytes());
    w.extend_from_slice(did);
    w.extend_from_slice(&p.kel_digest);
    w.extend_from_slice(&p.nonce);
}

fn get_party(b: &[u8], pos: &mut usize) -> Option<VoucherParty> {
    let did_len = u32::from_be_bytes(b.get(*pos..*pos + 4)?.try_into().ok()?) as usize;
    *pos += 4;
    if did_len > 512 || *pos + did_len > b.len() {
        return None;
    }
    let did_str = std::str::from_utf8(&b[*pos..*pos + did_len]).ok()?;
    let device = Did::parse(did_str).ok()?;
    *pos += did_len;
    let kel_digest: [u8; 32] = b.get(*pos..*pos + 32)?.try_into().ok()?;
    *pos += 32;
    let nonce: [u8; 32] = b.get(*pos..*pos + 32)?.try_into().ok()?;
    *pos += 32;
    Some(VoucherParty {
        device,
        kel_digest,
        nonce,
    })
}

fn put_sigs(w: &mut Vec<u8>, sigs: &[IndexedSig]) -> Result<()> {
    if sigs.is_empty() || sigs.len() > MAX_PARTY_SIGNATURES {
        return Err(UniquenessError::BadVouchObject);
    }
    w.push(sigs.len() as u8);
    for sig in sigs {
        w.extend_from_slice(&sig.index.to_be_bytes());
        w.push(sig.signature.suite().tag());
        let bytes = sig.signature.to_bytes();
        w.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
        w.extend_from_slice(&bytes);
    }
    Ok(())
}

fn get_sigs(b: &[u8], pos: &mut usize) -> Option<Vec<IndexedSig>> {
    let count = *b.get(*pos)? as usize;
    *pos += 1;
    if count == 0 || count > MAX_PARTY_SIGNATURES {
        return None;
    }
    let mut sigs = Vec::with_capacity(count);
    for _ in 0..count {
        let index = u32::from_be_bytes(b.get(*pos..*pos + 4)?.try_into().ok()?);
        *pos += 4;
        let suite = SignatureSuite::from_tag(*b.get(*pos)?).ok()?;
        *pos += 1;
        let sig_len = u32::from_be_bytes(b.get(*pos..*pos + 4)?.try_into().ok()?) as usize;
        *pos += 4;
        if sig_len > MAX_SIGNATURE_BYTES || *pos + sig_len > b.len() {
            return None;
        }
        let sig_bytes = &b[*pos..*pos + sig_len];
        *pos += sig_len;
        let signature = Signature::from_suite_bytes(suite, sig_bytes).ok()?;
        sigs.push(IndexedSig { index, signature });
    }
    Some(sigs)
}
