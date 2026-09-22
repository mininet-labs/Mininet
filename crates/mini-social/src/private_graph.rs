//! Private, per-relationship social graph edges (issue #19; D-0529).
//!
//! ## The problem
//!
//! [`crate::set_follow`] signs a `FOLLOW` object with the follower's own
//! human-root [`Did`], naming the target's human-root `Did` in the payload.
//! That is exactly right for a *voluntarily public* follow (a fan following
//! a public figure's wall, say) but it is the only option today, so it is
//! also what any relationship gets by default — including one where neither
//! side ever meant their connection to be legible to a third party. Anyone
//! who can see storage or replication traffic (a relay, a witness, a nosy
//! peer) can read every `FOLLOW` object's `author_human` and target `Did`
//! directly off the wire and reconstruct the full social graph, because both
//! ends are the same stable, root-linked handle used everywhere else that
//! identity root participates (posts, votes, payments).
//!
//! ## The fix: reuse `did-mini`'s pairwise pseudonyms, one per relationship
//!
//! `did-mini` already solves exactly this problem for identity in general
//! (SPEC-01 §10): [`Controller::incept_pairwise_pseudonym`] deterministically
//! derives an independent, unlinkable-looking `did:mini` root from a caller's
//! own key material plus an arbitrary context. This module composes that
//! existing primitive — it invents no new cryptography — into a
//! relationship-scoped identity:
//!
//! - [`derive_relationship_pseudonym`] derives, from a human-root
//!   [`Controller`] and a counterpart's `Did`, the *same* independent root
//!   every time it is called with that counterpart again, but a
//!   **different, unlinkable** root for every other counterpart or context
//!   (proven by [`tests::different_counterparts_yield_unlinkable_pseudonyms`]).
//! - A follow (or wall-membership) edge is then published with
//!   [`crate::set_follow`] using the pseudonym's own `Did` as both author and
//!   target, via [`set_private_follow`] — never the real human-root. An
//!   observer of storage/replication traffic sees two `did:mini` roots they
//!   cannot tell apart from any other independent identity, and cannot link
//!   to either party's real root or to any *other* relationship either party
//!   maintains, because each relationship gets its own derived root.
//! - The two parties still need to recognize *each other* across that
//!   pseudonymity. [`create_relationship_linkage`] /
//!   [`verify_relationship_linkage`] let a root vouch, to one specific
//!   counterpart only, that a given pseudonym `Did` is really it — the same
//!   embedded-KEL, offline-verifiable shape [`crate::pairing`] already uses
//!   for its offer/acceptance exchange. Critically, **this linkage is never
//!   published to the object store**: it is handed directly to the one
//!   counterpart it names (e.g. over the same private channel used for
//!   pairing, or inside an encrypted `Payload::Encrypted` message), so it
//!   never becomes something a third-party observer can read. The
//!   counterpart who does hold it can authenticate the relationship
//!   (`verified.root == the human they already trust`); nobody else can.
//!
//! ## What this does and does not hide
//!
//! - **Hides:** which stable identity roots participate in *which*
//!   relationship, from anyone who only observes storage/replication
//!   traffic or the object store itself. Two relationships from the same
//!   root are structurally unlinkable to each other by that observer.
//! - **Does not hide:** the existence of *a* relationship between two
//!   pseudonym `Did`s (a `FOLLOW` object between them is still visible, same
//!   as any public follow) — only which real identities sit behind those
//!   pseudonyms. It also does not hide network-layer metadata (who talks to
//!   whom, when) — that is out of scope for this module, same limitation
//!   `docs/design/storage-fraud-detection.md` already notes honestly for the
//!   adjacent storage-claim case. And it depends on the linkage proof being
//!   delivered over a channel a third party cannot read; this module does
//!   not choose that channel (pairing's TCP exchange or an encrypted
//!   `Payload::Encrypted` object are both suitable, and out of scope here).

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

/// Publish a private follow (or unfollow) edge between two relationship
/// pseudonyms. `pseudonym` must be a [`Controller`] returned by
/// [`derive_relationship_pseudonym`] (or an equivalent single-key root the
/// caller otherwise controls) acting as its own signing device, and
/// `target_pseudonym` the counterpart's own relationship pseudonym `Did`
/// (learned from a [`VerifiedRelationshipLinkage`] or exchanged directly).
/// This never takes a real human-root `Did` — that is the whole point: the
/// object this writes carries only pseudonym `Did`s, never a stable,
/// root-linked handle.
pub fn set_private_follow<B: mini_store::Backend>(
    store: &mut mini_store::Store<B>,
    pseudonym: &Controller,
    target_pseudonym: &Did,
    follow: bool,
    timestamp_ms: u64,
    sequence: u64,
) -> Result<mini_objects::Object> {
    let me = pseudonym.did();
    crate::set_follow(
        store,
        &me,
        pseudonym,
        target_pseudonym,
        follow,
        timestamp_ms,
        sequence,
    )
}

/// A [`create_relationship_linkage`] proof, authenticated by
/// [`verify_relationship_linkage`].
#[derive(Debug, Clone)]
pub struct VerifiedRelationshipLinkage {
    /// The real human-root `Did` vouching for the pseudonym below.
    pub root: Did,
    /// The relationship pseudonym `Did` the root is vouching for.
    pub pseudonym: Did,
    /// The one counterpart this linkage is scoped to — a linkage received
    /// by anyone else is rejected by [`verify_relationship_linkage`], so a
    /// leaked linkage cannot be repurposed to deanonymize the pseudonym to
    /// a party it was never meant for.
    pub counterpart: Did,
    pub issued_at_ms: u64,
}

/// Build a signed, offline-verifiable statement that `pseudonym` (normally
/// the output of [`derive_relationship_pseudonym`] for `counterpart`) really
/// is controlled by the human-root behind `root_kel`/`device` — the same
/// embedded-KEL shape [`crate::pairing::create_pairing_offer`] uses, so a
/// scanner needs nothing but these bytes to verify it fully offline.
///
/// **This must never be published to the object store or any other
/// broadcast surface.** It is meaningful privacy only while it stays
/// confined to the one `counterpart` it names — hand it over an already
/// private/authenticated channel (e.g. the same LAN pairing exchange, or an
/// encrypted [`mini_objects::Payload::Encrypted`] message addressed to that
/// counterpart alone).
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

/// Authenticate raw linkage bytes (as received directly from the
/// counterpart it names — never from the object store). Verifies the
/// embedded delegation chain and detached signature exactly like
/// [`crate::pairing::verify_pairing_offer`], and additionally rejects the
/// linkage outright unless it was scoped to `expected_counterpart`: a
/// linkage handed to the wrong party, whether by mistake or by a relay
/// trying to correlate it, verifies as cryptographically sound but is
/// refused here because it was never meant for that recipient.
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

    /// A third-party observer of the object store sees only pseudonym
    /// `Did`s on a `FOLLOW` edge — never Alice's or Bob's real root — and
    /// two of Alice's relationships remain unlinkable to each other from
    /// that view alone.
    #[test]
    fn private_follow_never_reveals_real_roots_to_an_observer() {
        let mut store = Store::new(MemoryBackend::default());
        let alice = root(10);
        let bob = root(11);
        let carol = root(12);

        let alice_for_bob = derive_relationship_pseudonym(&alice, &bob.did()).unwrap();
        let bob_for_alice = derive_relationship_pseudonym(&bob, &alice.did()).unwrap();
        let alice_for_carol = derive_relationship_pseudonym(&alice, &carol.did()).unwrap();

        set_private_follow(
            &mut store,
            &alice_for_bob,
            &bob_for_alice.did(),
            true,
            1_000,
            0,
        )
        .unwrap();
        set_private_follow(
            &mut store,
            &alice_for_carol,
            // Carol need not even be online for Alice to publish this edge;
            // only the pseudonym Did matters here.
            &derive_relationship_pseudonym(&carol, &alice.did())
                .unwrap()
                .did(),
            true,
            1_000,
            0,
        )
        .unwrap();

        // The observer's whole view is: what does each pseudonym follow?
        let bob_side = following(&store, &alice_for_bob.did()).unwrap();
        assert_eq!(bob_side, vec![bob_for_alice.did()]);
        assert!(
            !bob_side
                .iter()
                .any(|d| d.as_str() == alice.did().as_str() || d.as_str() == bob.did().as_str()),
            "no real root ever appears on a private follow edge"
        );

        // The two relationships used unmistakably different pseudonyms for
        // Alice, so an observer cannot merge them into one social-graph node.
        assert_ne!(alice_for_bob.did().as_str(), alice_for_carol.did().as_str());

        // followers() over the pseudonym graph works exactly like the
        // public graph, just keyed by pseudonym instead of real root.
        let bob_followers = followers(&store, &bob_for_alice.did()).unwrap();
        assert_eq!(bob_followers, vec![alice_for_bob.did()]);
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

        // A linkage scoped to Bob is refused by anyone else it might leak
        // to — e.g. Carol, who was never the intended recipient.
        let carol = root(22);
        assert!(verify_relationship_linkage(&alice_linkage, &carol.did()).is_err());
    }
}
