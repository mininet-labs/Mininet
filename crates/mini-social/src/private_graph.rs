//! Relationship pseudonyms and signed plaintext linkage assertions (issue #19; D-0536).
//!
//! Pseudonyms compose the existing pairwise inception primitive. A caller must
//! persist each pseudonym root's evolving KEL and its delegated signing device;
//! deriving the same inception again does not recover later delegation events.
//! This module provides no durable identity lifecycle or authenticated pairing.
//!
//! Private FOLLOW objects carry pseudonym DIDs, but remain public graph edges.
//! Signers must be actually delegated POST devices of the supplied pseudonym KEL.
//! Linkage assertions are plaintext: anyone holding their bytes can learn the
//! real-root/pseudonym relationship. Counterpart scoping binds signed context;
//! it provides neither confidentiality nor proof of the verifier's identity.
//! Authenticated confidential delivery, traffic analysis protection and verified
//! two-store synchronization remain separate integration requirements.

use did_mini::{verify_delegation, Capabilities, Controller, Did, IndexedSig, Kel};
use mini_crypto::{Signature, SignatureSuite};

use crate::{get_str, put_str, Result, SocialError};

/// Domain-separation context prefix for a relationship pseudonym's
/// derivation, so it can never collide with any other
/// [`Controller::incept_pairwise_pseudonym`] caller elsewhere in Mininet.
/// Versioned in the string itself (`v1`).
const RELATIONSHIP_CONTEXT_PREFIX: &[u8] = b"mininet/mini-social/relationship-pseudonym/v1/";

/// Bound on a linkage's embedded root KEL.
pub const MAX_LINKAGE_ROOT_KEL_BYTES: usize = 16 * 1024;
/// Bound on a linkage's embedded device KEL.
pub const MAX_LINKAGE_DEVICE_KEL_BYTES: usize = 4 * 1024;
/// Bound on signatures carried by one linkage.
const MAX_LINKAGE_SIGNATURES: usize = 4;
/// Bound on one detached signature's raw bytes (largest suite today).
const MAX_LINKAGE_SIGNATURE_BYTES: usize = 4096;

const LINKAGE_MAGIC: &[u8; 8] = b"MINIRL01";

/// The per-relationship derivation context for `counterpart`: distinct for
/// every distinct counterpart, so [`derive_relationship_pseudonym`] never
/// reuses a pseudonym across relationships.
fn relationship_context(counterpart: &Did) -> Vec<u8> {
    let mut ctx = RELATIONSHIP_CONTEXT_PREFIX.to_vec();
    ctx.extend_from_slice(counterpart.as_str().as_bytes());
    ctx
}

/// Deterministically derive this relationship's independent pseudonym root
/// from `root`'s own key material and `counterpart`'s `Did` — the same
/// pseudonym every time for the same counterpart, an unlinkable-by-default
/// different one for every other counterpart. Requires `root` to be a
/// single-key (1-of-1) identity, same requirement as
/// [`Controller::incept_pairwise_pseudonym`] itself.
pub fn derive_relationship_pseudonym(root: &Controller, counterpart: &Did) -> Result<Controller> {
    root.incept_pairwise_pseudonym(&relationship_context(counterpart))
        .map_err(SocialError::from)
}

/// Publish a FOLLOW using caller-persisted pseudonym identity state.
///
/// `pseudonym_root` must be the current KEL of the relationship pseudonym.
/// `device` must be its active POST delegate. Persist both identities and KEL
/// updates outside this function; never re-incept the same pseudonym and create
/// a fresh delegation history on each call. The target must have been
/// authenticated separately. All provenance checks precede store mutation.
#[allow(clippy::too_many_arguments)]
pub fn set_private_follow<B: mini_store::Backend>(
    store: &mut mini_store::Store<B>,
    pseudonym_root: &Kel,
    device: &Controller,
    target_pseudonym: &Did,
    follow: bool,
    timestamp_ms: u64,
    sequence: u64,
) -> Result<mini_objects::Object> {
    let capabilities = verify_delegation(pseudonym_root, &device.kel())?;
    if !capabilities.contains(Capabilities::POST) {
        return Err(SocialError::PairingCapabilityMissing);
    }
    let mut payload = vec![u8::from(follow)];
    put_str(&mut payload, target_pseudonym.as_str());
    let object = mini_objects::ObjectBuilder::new(mini_objects::ObjectType::FOLLOW)
        .timestamp_ms(timestamp_ms)
        .sequence(sequence)
        .payload(mini_objects::Payload::Public(payload))
        .sign(&pseudonym_root.did(), device)?;
    mini_objects::verify_provenance(&object, pseudonym_root, &device.kel())?;
    store.insert(&object)?;
    Ok(object)
}

/// A [`create_relationship_linkage`] proof, authenticated by
/// [`verify_relationship_linkage`].
#[derive(Debug, Clone)]
pub struct VerifiedRelationshipLinkage {
    /// The real human-root `Did` vouching for the pseudonym below.
    pub root: Did,
    /// The relationship pseudonym `Did` the root is vouching for.
    pub pseudonym: Did,
    /// Signed counterpart context checked against a caller-supplied DID.
    /// This does not authenticate the verifier or conceal the plaintext
    /// root/pseudonym linkage from anyone who possesses its bytes.
    pub counterpart: Did,
    pub issued_at_ms: u64,
}

/// Build a signed plaintext root/pseudonym assertion with embedded KELs.
///
/// Never publish this to the object store or a broadcast surface. Anyone
/// holding these bytes can recover the root/pseudonym linkage. Delivery must
/// provide separately authenticated confidentiality; this module implements
/// neither delivery nor recipient authentication.
pub fn create_relationship_linkage(
    root_kel: &Kel,
    device: &Controller,
    pseudonym: &Did,
    counterpart: &Did,
    issued_at_ms: u64,
) -> Result<Vec<u8>> {
    let root_kel_bytes = root_kel.to_bytes();
    let device_kel_bytes = device.kel().to_bytes();
    if root_kel_bytes.len() > MAX_LINKAGE_ROOT_KEL_BYTES
        || device_kel_bytes.len() > MAX_LINKAGE_DEVICE_KEL_BYTES
    {
        return Err(SocialError::FieldTooLarge);
    }

    let mut msg = Vec::new();
    msg.extend_from_slice(LINKAGE_MAGIC);
    put_bytes(&mut msg, &root_kel_bytes);
    put_bytes(&mut msg, &device_kel_bytes);
    put_str(&mut msg, pseudonym.as_str());
    put_str(&mut msg, counterpart.as_str());
    msg.extend_from_slice(&issued_at_ms.to_be_bytes());

    let sigs = device.sign_message(&msg);
    let mut out = msg;
    put_sigs(&mut out, &sigs)?;
    Ok(out)
}

/// Verify delegation, signature and signed counterpart context.
///
/// `expected_counterpart` is supplied by the caller, not proof that the caller
/// controls that identity. A leaked assertion remains readable and verifiable
/// by anyone who supplies its named counterpart DID.
pub fn verify_relationship_linkage(
    bytes: &[u8],
    expected_counterpart: &Did,
) -> Result<VerifiedRelationshipLinkage> {
    if bytes.len() < LINKAGE_MAGIC.len() || &bytes[..LINKAGE_MAGIC.len()] != LINKAGE_MAGIC {
        return Err(SocialError::PairingMalformed);
    }
    let mut pos = LINKAGE_MAGIC.len();
    let root_kel_bytes = get_bytes(bytes, &mut pos, MAX_LINKAGE_ROOT_KEL_BYTES)
        .ok_or(SocialError::PairingMalformed)?;
    let device_kel_bytes = get_bytes(bytes, &mut pos, MAX_LINKAGE_DEVICE_KEL_BYTES)
        .ok_or(SocialError::PairingMalformed)?;
    let pseudonym = get_str(bytes, &mut pos).ok_or(SocialError::PairingMalformed)?;
    let counterpart = get_str(bytes, &mut pos).ok_or(SocialError::PairingMalformed)?;
    let issued_at_ms = get_u64(bytes, &mut pos).ok_or(SocialError::PairingMalformed)?;
    let msg_end = pos;
    let sigs = get_sigs(bytes, &mut pos).ok_or(SocialError::PairingMalformed)?;
    if pos != bytes.len() {
        return Err(SocialError::PairingMalformed);
    }

    let root_kel = Kel::from_bytes(&root_kel_bytes)?;
    let device_kel = Kel::from_bytes(&device_kel_bytes)?;
    let capabilities = verify_delegation(&root_kel, &device_kel)?;
    if !capabilities.contains(Capabilities::POST) {
        return Err(SocialError::PairingCapabilityMissing);
    }
    device_kel.verify_message(&bytes[..msg_end], &sigs)?;

    let pseudonym = Did::parse(&pseudonym)?;
    let counterpart = Did::parse(&counterpart)?;
    if counterpart.as_str() != expected_counterpart.as_str() {
        return Err(SocialError::PairingMalformed);
    }

    Ok(VerifiedRelationshipLinkage {
        root: root_kel.did(),
        pseudonym,
        counterpart,
        issued_at_ms,
    })
}

fn put_bytes(w: &mut Vec<u8>, b: &[u8]) {
    w.extend_from_slice(&(b.len() as u32).to_be_bytes());
    w.extend_from_slice(b);
}

fn get_bytes(b: &[u8], pos: &mut usize, max: usize) -> Option<Vec<u8>> {
    let len = u32::from_be_bytes(b.get(*pos..*pos + 4)?.try_into().ok()?) as usize;
    *pos += 4;
    if len > max || *pos + len > b.len() {
        return None;
    }
    let out = b[*pos..*pos + len].to_vec();
    *pos += len;
    Some(out)
}

fn get_u64(b: &[u8], pos: &mut usize) -> Option<u64> {
    let v = u64::from_be_bytes(b.get(*pos..*pos + 8)?.try_into().ok()?);
    *pos += 8;
    Some(v)
}

fn put_sigs(w: &mut Vec<u8>, sigs: &[IndexedSig]) -> Result<()> {
    if sigs.is_empty() || sigs.len() > MAX_LINKAGE_SIGNATURES {
        return Err(SocialError::PairingMalformed);
    }
    w.push(sigs.len() as u8);
    for sig in sigs {
        w.extend_from_slice(&sig.index.to_be_bytes());
        w.push(sig.signature.suite().tag());
        put_bytes(w, &sig.signature.to_bytes());
    }
    Ok(())
}

fn get_sigs(b: &[u8], pos: &mut usize) -> Option<Vec<IndexedSig>> {
    let count = *b.get(*pos)? as usize;
    *pos += 1;
    if count == 0 || count > MAX_LINKAGE_SIGNATURES {
        return None;
    }
    let mut sigs = Vec::with_capacity(count);
    for _ in 0..count {
        let index = u32::from_be_bytes(b.get(*pos..*pos + 4)?.try_into().ok()?);
        *pos += 4;
        let suite = SignatureSuite::from_tag(*b.get(*pos)?).ok()?;
        *pos += 1;
        let sig_bytes = get_bytes(b, pos, MAX_LINKAGE_SIGNATURE_BYTES)?;
        let signature = Signature::from_suite_bytes(suite, &sig_bytes).ok()?;
        sigs.push(IndexedSig { index, signature });
    }
    Some(sigs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{followers, following};
    use mini_store::{MemoryBackend, Store};

    fn root(seed: u8) -> Controller {
        Controller::incept_single_from_seeds(&[seed; 32], &[seed.wrapping_add(1); 32]).unwrap()
    }

    /// A root plus a device it has delegated `POST` to — the shape
    /// [`create_relationship_linkage`] needs, mirroring
    /// [`crate::pairing`]'s own test fixture.
    fn delegated_pair(seed: u8) -> (Controller, Controller) {
        let mut root =
            Controller::incept_single_from_seeds(&[seed; 32], &[seed.wrapping_add(1); 32]).unwrap();
        let device = Controller::incept_device_single_from_seeds(
            &root.did(),
            &[seed.wrapping_add(2); 32],
            &[seed.wrapping_add(3); 32],
        )
        .unwrap();
        root.delegate_device(&device.did(), Capabilities::primary())
            .unwrap();
        (root, device)
    }

    /// The same root deriving pseudonyms for two different counterparts gets
    /// two different, unlinkable-looking `did:mini` roots — an observer who
    /// sees both `FOLLOW` edges cannot tell they share a common real root.
    #[test]
    fn different_counterparts_yield_unlinkable_pseudonyms() {
        let alice = root(1);
        let bob = Did::parse(root(2).did().as_str()).unwrap();
        let carol = Did::parse(root(3).did().as_str()).unwrap();

        let pseudonym_for_bob = derive_relationship_pseudonym(&alice, &bob).unwrap();
        let pseudonym_for_carol = derive_relationship_pseudonym(&alice, &carol).unwrap();

        assert_ne!(
            pseudonym_for_bob.did().as_str(),
            pseudonym_for_carol.did().as_str(),
            "distinct relationships must not share a pseudonym"
        );
        assert_ne!(
            pseudonym_for_bob.did().as_str(),
            alice.did().as_str(),
            "a relationship pseudonym must never equal the real root"
        );

        // Deterministic: re-deriving for the same counterpart recovers the
        // identical pseudonym with no extra seed storage.
        let rederived = derive_relationship_pseudonym(&alice, &bob).unwrap();
        assert_eq!(rederived.did().as_str(), pseudonym_for_bob.did().as_str());
    }

    fn delegate_pseudonym(pseudonym: &mut Controller, seed: u8) -> Controller {
        let device = Controller::incept_device_single_from_seeds(
            &pseudonym.did(),
            &[seed; 32],
            &[seed.wrapping_add(1); 32],
        )
        .unwrap();
        pseudonym
            .delegate_device(&device.did(), Capabilities::POST)
            .unwrap();
        device
    }

    #[test]
    fn private_follow_accepts_persisted_delegated_provenance() {
        let mut store = Store::new(MemoryBackend::default());
        let alice = root(10);
        let bob = root(11);
        let mut pseudonym = derive_relationship_pseudonym(&alice, &bob.did()).unwrap();
        let target = derive_relationship_pseudonym(&bob, &alice.did()).unwrap();
        let device = delegate_pseudonym(&mut pseudonym, 40);
        // Reload the caller's current KEL, including its delegation event.
        let persisted_kel = Kel::from_bytes(&pseudonym.kel().to_bytes()).unwrap();
        let object = set_private_follow(
            &mut store,
            &persisted_kel,
            &device,
            &target.did(),
            true,
            1_000,
            0,
        )
        .unwrap();
        mini_objects::verify_provenance(&object, &persisted_kel, &device.kel()).unwrap();
        assert_eq!(object.author_human, pseudonym.did());
        assert_eq!(object.author_device, device.did());
        assert_ne!(object.author_human, alice.did());
        assert_eq!(
            following(&store, &pseudonym.did()).unwrap(),
            vec![target.did()]
        );
        assert_eq!(
            followers(&store, &target.did()).unwrap(),
            vec![pseudonym.did()]
        );
    }

    #[test]
    fn invalid_private_follow_signers_leave_store_unchanged() {
        let mut store = Store::new(MemoryBackend::default());
        let (mut pseudonym, valid_device) = delegated_pair(50);
        let (_, foreign_device) = delegated_pair(60);
        let target = root(70).did();
        let undelegated = Controller::incept_device_single_from_seeds(
            &pseudonym.did(),
            &[80; 32],
            &[81; 32],
        )
        .unwrap();
        let sign_only = Controller::incept_device_single_from_seeds(
            &pseudonym.did(),
            &[82; 32],
            &[83; 32],
        )
        .unwrap();
        pseudonym
            .delegate_device(&sign_only.did(), Capabilities::SIGN)
            .unwrap();
        // Root-as-device, foreign, undelegated and insufficient capability.
        for device in [&pseudonym, &foreign_device, &undelegated, &sign_only] {
            assert!(set_private_follow(
                &mut store,
                &pseudonym.kel(),
                device,
                &target,
                true,
                1_000,
                0,
            )
            .is_err());
            assert!(store
                .by_type(&mini_objects::ObjectType::FOLLOW)
                .unwrap()
                .is_empty());
            assert!(store.by_author(&pseudonym.did()).unwrap().is_empty());
        }
        pseudonym.revoke_device(&valid_device.did()).unwrap();
        assert!(set_private_follow(
            &mut store,
            &pseudonym.kel(),
            &valid_device,
            &target,
            true,
            1_000,
            0,
        )
        .is_err());
        assert!(store
            .by_type(&mini_objects::ObjectType::FOLLOW)
            .unwrap()
            .is_empty());
    }

    /// Both sides of one relationship can still mutually authenticate: Bob
    /// receives Alice's linkage proof (privately, never through the store)
    /// and confirms the pseudonym following him really is Alice.
    #[test]
    fn both_sides_mutually_authenticate_via_private_linkage() {
        let (alice, alice_device) = delegated_pair(20);
        let (bob, bob_device) = delegated_pair(21);
        let alice_for_bob = derive_relationship_pseudonym(&alice, &bob.did()).unwrap();
        let bob_for_alice = derive_relationship_pseudonym(&bob, &alice.did()).unwrap();

        // Alice hands Bob a linkage proving her pseudonym is really her,
        // scoped to Bob and nobody else.
        let alice_linkage = create_relationship_linkage(
            &alice.kel(),
            &alice_device,
            &alice_for_bob.did(),
            &bob.did(),
            5_000,
        )
        .unwrap();
        let verified = verify_relationship_linkage(&alice_linkage, &bob.did()).unwrap();
        assert_eq!(verified.root.as_str(), alice.did().as_str());
        assert_eq!(verified.pseudonym.as_str(), alice_for_bob.did().as_str());
        assert_eq!(verified.counterpart.as_str(), bob.did().as_str());

        // Bob does the same for Alice — genuine mutual authentication.
        let bob_linkage = create_relationship_linkage(
            &bob.kel(),
            &bob_device,
            &bob_for_alice.did(),
            &alice.did(),
            5_000,
        )
        .unwrap();
        let verified_bob = verify_relationship_linkage(&bob_linkage, &alice.did()).unwrap();
        assert_eq!(verified_bob.root.as_str(), bob.did().as_str());

        // A mismatched expected context is rejected; this is not confidentiality.
        let carol = root(22);
        assert!(verify_relationship_linkage(&alice_linkage, &carol.did()).is_err());
        // Any holder of leaked bytes can supply Bob's public DID and verify.
        assert!(verify_relationship_linkage(&alice_linkage, &bob.did()).is_ok());
    }
}
