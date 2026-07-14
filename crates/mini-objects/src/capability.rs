//! Typed capability grants (`MN-104`): exact, non-delegable authorization
//! over one scope and one right, bound to a holder-controlled pseudonym
//! and an unguessable secret token — never a free-form permission string,
//! a bitmask, or a general policy language. See `docs/design/
//! privacy-cost-doctrine-parallel-execution-plan.md` lane L1 and the
//! research it's built from for why this scope was chosen over Macaroons/
//! Biscuit/general attenuation.

use did_mini::{Controller, Did, IndexedSig, Kel};

use crate::codec::{Reader, Writer};
use crate::error::{ObjectError, Result};
use crate::object::ObjectId;

/// This module's grant format version.
pub const CAPABILITY_VERSION: u8 = 1;

const GRANT_SIGNING_DOMAIN: &[u8] = b"mininet/mini-objects/capability-grant/v1";
const TOKEN_COMMITMENT_DOMAIN: &[u8] = b"mininet/mini-objects/capability-token-commitment/v1";
const HOLDER_PROOF_DOMAIN: &[u8] = b"mininet/mini-objects/capability-holder-proof/v1";

const MAX_DID_BYTES: usize = 256;
const MAX_SIGNATURES: usize = 16;
const MAX_SIG_BYTES: usize = 256;

/// What a capability grants. **Closed by design** — never a free-form
/// string or bitmask: each right is independent (`Administer` does not
/// imply `Read`; `Moderate` does not imply `Append`). Higher-level policy
/// may choose to issue several grants together; this type never creates
/// an implicit hierarchy among rights.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CapabilityRight {
    Read,
    Append,
    Reply,
    Moderate,
    Administer,
}

impl CapabilityRight {
    fn tag(self) -> u8 {
        match self {
            CapabilityRight::Read => 1,
            CapabilityRight::Append => 2,
            CapabilityRight::Reply => 3,
            CapabilityRight::Moderate => 4,
            CapabilityRight::Administer => 5,
        }
    }

    fn from_tag(tag: u8) -> Result<Self> {
        match tag {
            1 => Ok(CapabilityRight::Read),
            2 => Ok(CapabilityRight::Append),
            3 => Ok(CapabilityRight::Reply),
            4 => Ok(CapabilityRight::Moderate),
            5 => Ok(CapabilityRight::Administer),
            _ => Err(ObjectError::BadObject),
        }
    }
}

/// What a capability is scoped to. `#[non_exhaustive]` and deliberately
/// minimal in this first version (only what this workspace has an actual
/// id type for) — `Collection`/`Conversation`/`Community` scopes are
/// named as future work, not guessed at here. **No wildcard or prefix
/// scope exists or is planned**: broader authority is represented by
/// issuing several exact grants, never a "match everything under X" rule.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum CapabilityScope {
    Object(ObjectId),
}

impl CapabilityScope {
    fn encode(&self, w: &mut Writer) {
        match self {
            CapabilityScope::Object(id) => {
                w.u8(1);
                w.bytes(id.as_str().as_bytes());
            }
        }
    }

    fn decode(r: &mut Reader) -> Result<Self> {
        match r.u8()? {
            1 => {
                let id_bytes = r.bytes_limited(128)?;
                let id_str = String::from_utf8(id_bytes).map_err(|_| ObjectError::BadObject)?;
                Ok(CapabilityScope::Object(ObjectId::parse(&id_str)?))
            }
            _ => Err(ObjectError::BadObject),
        }
    }
}

/// An unguessable, device-local bearer secret. Possession of both this
/// token *and* a valid holder-proof signature is required to exercise a
/// [`CapabilityGrant`] (holder-bound mode) — a leaked grant (which is
/// public, signed data) alone is not enough, and a leaked token alone
/// (without the grantee's signing key) is not enough either.
pub struct CapabilityToken([u8; 32]);

impl CapabilityToken {
    /// A fresh random token from OS entropy.
    pub fn generate() -> Result<Self> {
        Ok(CapabilityToken(
            mini_crypto::random_32().map_err(ObjectError::Crypto)?,
        ))
    }

    /// Rebuild from bytes already generated (device-local storage only —
    /// never transmitted, never logged).
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        CapabilityToken(bytes)
    }

    /// The public commitment an issuer binds into a [`CapabilityGrant`],
    /// domain-separated by the exact scope and right so a commitment
    /// copied into a different grant (different scope/right) never
    /// matches.
    pub fn commit(
        &self,
        scope: &CapabilityScope,
        right: CapabilityRight,
    ) -> CapabilityTokenCommitment {
        let mut w = Writer::new();
        w.raw(TOKEN_COMMITMENT_DOMAIN);
        scope.encode(&mut w);
        w.u8(right.tag());
        w.raw(&self.0);
        CapabilityTokenCommitment(mini_crypto::HashAlgorithm::Blake3.digest(&w.into_bytes()))
    }
}

impl core::fmt::Debug for CapabilityToken {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("CapabilityToken(REDACTED)")
    }
}

impl Drop for CapabilityToken {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.0.zeroize();
    }
}

/// A public commitment to a [`CapabilityToken`] — safe to include in a
/// signed, published [`CapabilityGrant`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CapabilityTokenCommitment([u8; 32]);

/// An exact, non-delegable authorization: `grantee` may exercise `right`
/// over `scope`, provided it also presents the token behind
/// `token_commitment` and proves control of `grantee` — see
/// [`CapabilityGrant::validate`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityGrant {
    pub issuer: Did,
    pub grantee: Did,
    pub scope: CapabilityScope,
    pub right: CapabilityRight,
    pub token_commitment: CapabilityTokenCommitment,
    pub not_before_ms: Option<u64>,
    pub expires_at_ms: Option<u64>,
    nonce: [u8; 16],
    signature: Vec<IndexedSig>,
}

impl CapabilityGrant {
    /// Issue and sign a grant. `token` is only used to compute its public
    /// commitment here — the secret itself never enters the grant.
    #[allow(clippy::too_many_arguments)]
    pub fn issue(
        issuer: &Controller,
        grantee: Did,
        scope: CapabilityScope,
        right: CapabilityRight,
        token: &CapabilityToken,
        not_before_ms: Option<u64>,
        expires_at_ms: Option<u64>,
    ) -> Result<Self> {
        let token_commitment = token.commit(&scope, right);
        let nonce = mini_crypto::random_32().map_err(ObjectError::Crypto)?;
        let nonce: [u8; 16] = nonce[..16]
            .try_into()
            .expect("first 16 bytes of a 32-byte array always convert");
        let mut grant = CapabilityGrant {
            issuer: issuer.did(),
            grantee,
            scope,
            right,
            token_commitment,
            not_before_ms,
            expires_at_ms,
            nonce,
            signature: Vec::new(),
        };
        grant.signature = issuer.sign_message(&grant.signing_bytes());
        Ok(grant)
    }

    fn signing_bytes(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.raw(GRANT_SIGNING_DOMAIN);
        w.u8(CAPABILITY_VERSION);
        w.bytes(self.issuer.as_str().as_bytes());
        w.bytes(self.grantee.as_str().as_bytes());
        self.scope.encode(&mut w);
        w.u8(self.right.tag());
        w.raw(&self.token_commitment.0);
        w.u8(self.not_before_ms.is_some() as u8);
        w.u64(self.not_before_ms.unwrap_or(0));
        w.u8(self.expires_at_ms.is_some() as u8);
        w.u64(self.expires_at_ms.unwrap_or(0));
        w.raw(&self.nonce);
        w.into_bytes()
    }

    /// The message a holder must sign with the grantee's current keys to
    /// prove possession — domain-separated from every other signing
    /// operation in this workspace, and bound to this exact grant (via its
    /// nonce and token commitment) so a holder proof for one grant can
    /// never be replayed against another.
    fn holder_proof_message(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.raw(HOLDER_PROOF_DOMAIN);
        w.raw(&self.nonce);
        w.raw(&self.token_commitment.0);
        w.into_bytes()
    }

    /// Sign the holder-proof message with the grantee's controller —
    /// call this on the device holding the grantee pseudonym's keys, then
    /// present the result alongside the token to whoever calls
    /// [`CapabilityGrant::validate`].
    pub fn prove_holder(&self, grantee: &Controller) -> Vec<IndexedSig> {
        grantee.sign_message(&self.holder_proof_message())
    }

    /// Full validation: issuer signature, exact scope/right match, token
    /// possession, validity window, and holder proof. Fails closed on any
    /// mismatch — never partially authorizes.
    #[allow(clippy::too_many_arguments)]
    pub fn validate(
        &self,
        issuer_kel: &Kel,
        requested_scope: &CapabilityScope,
        requested_right: CapabilityRight,
        token: &CapabilityToken,
        grantee_kel: &Kel,
        holder_proof: &[IndexedSig],
        now_ms: u64,
    ) -> Result<()> {
        if issuer_kel.did().as_str() != self.issuer.as_str() {
            return Err(ObjectError::DeviceMismatch);
        }
        issuer_kel
            .verify_message(&self.signing_bytes(), &self.signature)
            .map_err(ObjectError::Identity)?;
        if &self.scope != requested_scope {
            return Err(ObjectError::CapabilityScopeMismatch);
        }
        if self.right != requested_right {
            return Err(ObjectError::CapabilityRightMismatch);
        }
        if token.commit(&self.scope, self.right) != self.token_commitment {
            return Err(ObjectError::CapabilityTokenMismatch);
        }
        if let Some(not_before) = self.not_before_ms {
            if now_ms < not_before {
                return Err(ObjectError::CapabilityNotYetValid);
            }
        }
        if let Some(expires_at) = self.expires_at_ms {
            if now_ms >= expires_at {
                return Err(ObjectError::CapabilityExpired);
            }
        }
        if grantee_kel.did().as_str() != self.grantee.as_str() {
            return Err(ObjectError::CapabilityGranteeMismatch);
        }
        grantee_kel
            .verify_message(&self.holder_proof_message(), holder_proof)
            .map_err(|_| ObjectError::CapabilityGranteeMismatch)?;
        Ok(())
    }

    /// Canonical wire bytes (fields + signature), for storage/transport.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.u8(CAPABILITY_VERSION);
        w.bytes(self.issuer.as_str().as_bytes());
        w.bytes(self.grantee.as_str().as_bytes());
        self.scope.encode(&mut w);
        w.u8(self.right.tag());
        w.raw(&self.token_commitment.0);
        w.u8(self.not_before_ms.is_some() as u8);
        w.u64(self.not_before_ms.unwrap_or(0));
        w.u8(self.expires_at_ms.is_some() as u8);
        w.u64(self.expires_at_ms.unwrap_or(0));
        w.raw(&self.nonce);
        w.u32(self.signature.len() as u32);
        for s in &self.signature {
            w.u32(s.index);
            w.u8(s.signature.suite().tag());
            w.bytes(&s.signature.to_bytes());
        }
        w.into_bytes()
    }

    /// Decode a grant from untrusted bytes. Rejects an unrecognized
    /// [`CAPABILITY_VERSION`], a malformed scope/right tag, and trailing
    /// bytes. Does **not** verify the signature — call
    /// [`CapabilityGrant::validate`] with the issuer's KEL for that.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let mut r = Reader::new(bytes);
        if r.u8()? != CAPABILITY_VERSION {
            return Err(ObjectError::UnsupportedCapabilityVersion);
        }
        let issuer = parse_did(r.bytes_limited(MAX_DID_BYTES)?)?;
        let grantee = parse_did(r.bytes_limited(MAX_DID_BYTES)?)?;
        let scope = CapabilityScope::decode(&mut r)?;
        let right = CapabilityRight::from_tag(r.u8()?)?;
        let token_commitment_bytes: [u8; 32] = r
            .raw(32)?
            .try_into()
            .expect("Reader::raw(32) always returns exactly 32 bytes");
        let token_commitment = CapabilityTokenCommitment(token_commitment_bytes);
        let not_before_ms = if r.u8()? != 0 {
            Some(r.u64()?)
        } else {
            r.u64()?;
            None
        };
        let expires_at_ms = if r.u8()? != 0 {
            Some(r.u64()?)
        } else {
            r.u64()?;
            None
        };
        let nonce: [u8; 16] = r
            .raw(16)?
            .try_into()
            .expect("Reader::raw(16) always returns exactly 16 bytes");
        let nsigs = r.u32()? as usize;
        if nsigs > MAX_SIGNATURES {
            return Err(ObjectError::LimitExceeded);
        }
        let mut signature = Vec::with_capacity(nsigs);
        for _ in 0..nsigs {
            let index = r.u32()?;
            let sig_suite =
                mini_crypto::SignatureSuite::from_tag(r.u8()?).map_err(ObjectError::Crypto)?;
            let sig_bytes = r.bytes_limited(MAX_SIG_BYTES)?;
            let sig = mini_crypto::Signature::from_suite_bytes(sig_suite, &sig_bytes)
                .map_err(ObjectError::Crypto)?;
            signature.push(IndexedSig {
                index,
                signature: sig,
            });
        }
        if !r.finished() {
            return Err(ObjectError::TrailingBytes);
        }
        Ok(CapabilityGrant {
            issuer,
            grantee,
            scope,
            right,
            token_commitment,
            not_before_ms,
            expires_at_ms,
            nonce,
            signature,
        })
    }
}

fn parse_did(bytes: Vec<u8>) -> Result<Did> {
    let s = String::from_utf8(bytes).map_err(|_| ObjectError::BadObject)?;
    Did::parse(&s).map_err(ObjectError::Identity)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture {
        issuer: Controller,
        grantee: Controller,
        scope: CapabilityScope,
        token: CapabilityToken,
    }

    fn fixture() -> Fixture {
        Fixture {
            issuer: Controller::incept_single().unwrap(),
            grantee: Controller::incept_single().unwrap(),
            scope: CapabilityScope::Object(ObjectId::of(b"some-object")),
            token: CapabilityToken::generate().unwrap(),
        }
    }

    fn issue_and_prove(f: &Fixture, right: CapabilityRight) -> (CapabilityGrant, Vec<IndexedSig>) {
        let grant = CapabilityGrant::issue(
            &f.issuer,
            f.grantee.did(),
            f.scope.clone(),
            right,
            &f.token,
            None,
            None,
        )
        .unwrap();
        let proof = grant.prove_holder(&f.grantee);
        (grant, proof)
    }

    #[test]
    fn every_right_round_trips_through_its_tag() {
        for right in [
            CapabilityRight::Read,
            CapabilityRight::Append,
            CapabilityRight::Reply,
            CapabilityRight::Moderate,
            CapabilityRight::Administer,
        ] {
            assert_eq!(CapabilityRight::from_tag(right.tag()).unwrap(), right);
        }
    }

    #[test]
    fn an_unknown_right_tag_is_rejected() {
        assert_eq!(CapabilityRight::from_tag(0xee), Err(ObjectError::BadObject));
    }

    #[test]
    fn a_valid_grant_and_proof_validate_successfully() {
        let f = fixture();
        let (grant, proof) = issue_and_prove(&f, CapabilityRight::Read);
        grant
            .validate(
                &f.issuer.kel(),
                &f.scope,
                CapabilityRight::Read,
                &f.token,
                &f.grantee.kel(),
                &proof,
                0,
            )
            .unwrap();
    }

    #[test]
    fn a_read_grant_does_not_validate_as_append() {
        let f = fixture();
        let (grant, proof) = issue_and_prove(&f, CapabilityRight::Read);
        let err = grant
            .validate(
                &f.issuer.kel(),
                &f.scope,
                CapabilityRight::Append,
                &f.token,
                &f.grantee.kel(),
                &proof,
                0,
            )
            .unwrap_err();
        assert_eq!(err, ObjectError::CapabilityRightMismatch);
    }

    #[test]
    fn an_administer_grant_does_not_implicitly_validate_as_another_right() {
        let f = fixture();
        let (grant, proof) = issue_and_prove(&f, CapabilityRight::Administer);
        let err = grant
            .validate(
                &f.issuer.kel(),
                &f.scope,
                CapabilityRight::Read,
                &f.token,
                &f.grantee.kel(),
                &proof,
                0,
            )
            .unwrap_err();
        assert_eq!(err, ObjectError::CapabilityRightMismatch);
    }

    #[test]
    fn a_grant_for_scope_a_fails_against_scope_b() {
        let f = fixture();
        let (grant, proof) = issue_and_prove(&f, CapabilityRight::Read);
        let other_scope = CapabilityScope::Object(ObjectId::of(b"a-different-object"));
        let err = grant
            .validate(
                &f.issuer.kel(),
                &other_scope,
                CapabilityRight::Read,
                &f.token,
                &f.grantee.kel(),
                &proof,
                0,
            )
            .unwrap_err();
        assert_eq!(err, ObjectError::CapabilityScopeMismatch);
    }

    #[test]
    fn an_incorrect_token_fails() {
        let f = fixture();
        let (grant, proof) = issue_and_prove(&f, CapabilityRight::Read);
        let wrong_token = CapabilityToken::generate().unwrap();
        let err = grant
            .validate(
                &f.issuer.kel(),
                &f.scope,
                CapabilityRight::Read,
                &wrong_token,
                &f.grantee.kel(),
                &proof,
                0,
            )
            .unwrap_err();
        assert_eq!(err, ObjectError::CapabilityTokenMismatch);
    }

    #[test]
    fn a_token_commitment_copied_into_a_different_scope_grant_does_not_validate() {
        let f = fixture();
        let other_scope = CapabilityScope::Object(ObjectId::of(b"a-different-object"));
        // Same token, but issued (committed) for a different scope.
        let grant = CapabilityGrant::issue(
            &f.issuer,
            f.grantee.did(),
            other_scope,
            CapabilityRight::Read,
            &f.token,
            None,
            None,
        )
        .unwrap();
        let proof = grant.prove_holder(&f.grantee);
        // Validating against the *original* scope with the same token must fail:
        // the grant's own commitment was computed for `other_scope`, not `f.scope`.
        let err = grant
            .validate(
                &f.issuer.kel(),
                &f.scope,
                CapabilityRight::Read,
                &f.token,
                &f.grantee.kel(),
                &proof,
                0,
            )
            .unwrap_err();
        assert_eq!(err, ObjectError::CapabilityScopeMismatch);
    }

    #[test]
    fn a_valid_token_without_grantee_proof_fails() {
        let f = fixture();
        let (grant, _proof) = issue_and_prove(&f, CapabilityRight::Read);
        let bogus_proof = vec![];
        let err = grant
            .validate(
                &f.issuer.kel(),
                &f.scope,
                CapabilityRight::Read,
                &f.token,
                &f.grantee.kel(),
                &bogus_proof,
                0,
            )
            .unwrap_err();
        assert_eq!(err, ObjectError::CapabilityGranteeMismatch);
    }

    #[test]
    fn proof_from_another_pseudonym_fails() {
        let f = fixture();
        let (grant, _proof) = issue_and_prove(&f, CapabilityRight::Read);
        let impostor = Controller::incept_single().unwrap();
        let impostor_proof = impostor.sign_message(&grant.holder_proof_message());
        let err = grant
            .validate(
                &f.issuer.kel(),
                &f.scope,
                CapabilityRight::Read,
                &f.token,
                &f.grantee.kel(),
                &impostor_proof,
                0,
            )
            .unwrap_err();
        // Wrong KEL for the claimed grantee is caught by the DID check;
        // even swapping in the impostor's own KEL, the signature itself is
        // over the right message so this specific call would only reach
        // this point if callers mismatch KEL/proof themselves. Assert the
        // grantee-mismatch class of error either way.
        assert!(matches!(
            err,
            ObjectError::CapabilityGranteeMismatch | ObjectError::Identity(_)
        ));
    }

    #[test]
    fn an_expired_grant_fails() {
        let f = fixture();
        let grant = CapabilityGrant::issue(
            &f.issuer,
            f.grantee.did(),
            f.scope.clone(),
            CapabilityRight::Read,
            &f.token,
            None,
            Some(1_000),
        )
        .unwrap();
        let proof = grant.prove_holder(&f.grantee);
        let err = grant
            .validate(
                &f.issuer.kel(),
                &f.scope,
                CapabilityRight::Read,
                &f.token,
                &f.grantee.kel(),
                &proof,
                1_000,
            )
            .unwrap_err();
        assert_eq!(err, ObjectError::CapabilityExpired);
    }

    #[test]
    fn a_not_yet_valid_grant_fails() {
        let f = fixture();
        let grant = CapabilityGrant::issue(
            &f.issuer,
            f.grantee.did(),
            f.scope.clone(),
            CapabilityRight::Read,
            &f.token,
            Some(1_000),
            None,
        )
        .unwrap();
        let proof = grant.prove_holder(&f.grantee);
        let err = grant
            .validate(
                &f.issuer.kel(),
                &f.scope,
                CapabilityRight::Read,
                &f.token,
                &f.grantee.kel(),
                &proof,
                500,
            )
            .unwrap_err();
        assert_eq!(err, ObjectError::CapabilityNotYetValid);
    }

    #[test]
    fn a_grant_signed_by_one_issuer_does_not_validate_against_another() {
        let f = fixture();
        let (grant, proof) = issue_and_prove(&f, CapabilityRight::Read);
        let other_issuer = Controller::incept_single().unwrap();
        let err = grant
            .validate(
                &other_issuer.kel(),
                &f.scope,
                CapabilityRight::Read,
                &f.token,
                &f.grantee.kel(),
                &proof,
                0,
            )
            .unwrap_err();
        assert_eq!(err, ObjectError::DeviceMismatch);
    }

    #[test]
    fn a_grant_round_trips_through_wire_bytes() {
        let f = fixture();
        let (grant, _proof) = issue_and_prove(&f, CapabilityRight::Moderate);
        let decoded = CapabilityGrant::from_bytes(&grant.to_bytes()).unwrap();
        assert_eq!(decoded, grant);
    }

    #[test]
    fn a_grant_with_a_validity_window_round_trips_and_still_enforces_it() {
        let f = fixture();
        let grant = CapabilityGrant::issue(
            &f.issuer,
            f.grantee.did(),
            f.scope.clone(),
            CapabilityRight::Read,
            &f.token,
            Some(1_000),
            Some(2_000),
        )
        .unwrap();
        let proof = grant.prove_holder(&f.grantee);
        let decoded = CapabilityGrant::from_bytes(&grant.to_bytes()).unwrap();
        assert_eq!(decoded.not_before_ms, Some(1_000));
        assert_eq!(decoded.expires_at_ms, Some(2_000));
        assert_eq!(
            decoded
                .validate(
                    &f.issuer.kel(),
                    &f.scope,
                    CapabilityRight::Read,
                    &f.token,
                    &f.grantee.kel(),
                    &proof,
                    500,
                )
                .unwrap_err(),
            ObjectError::CapabilityNotYetValid
        );
        decoded
            .validate(
                &f.issuer.kel(),
                &f.scope,
                CapabilityRight::Read,
                &f.token,
                &f.grantee.kel(),
                &proof,
                1_500,
            )
            .unwrap();
        assert_eq!(
            decoded
                .validate(
                    &f.issuer.kel(),
                    &f.scope,
                    CapabilityRight::Read,
                    &f.token,
                    &f.grantee.kel(),
                    &proof,
                    2_000,
                )
                .unwrap_err(),
            ObjectError::CapabilityExpired
        );
    }

    #[test]
    fn a_decoded_grant_still_validates() {
        let f = fixture();
        let (grant, proof) = issue_and_prove(&f, CapabilityRight::Reply);
        let decoded = CapabilityGrant::from_bytes(&grant.to_bytes()).unwrap();
        decoded
            .validate(
                &f.issuer.kel(),
                &f.scope,
                CapabilityRight::Reply,
                &f.token,
                &f.grantee.kel(),
                &proof,
                0,
            )
            .unwrap();
    }

    #[test]
    fn an_unknown_capability_version_is_rejected() {
        let mut w = Writer::new();
        w.u8(0xee);
        assert_eq!(
            CapabilityGrant::from_bytes(&w.into_bytes()),
            Err(ObjectError::UnsupportedCapabilityVersion)
        );
    }

    #[test]
    fn a_truncated_grant_is_rejected_at_every_length() {
        let f = fixture();
        let (grant, _proof) = issue_and_prove(&f, CapabilityRight::Read);
        let full = grant.to_bytes();
        for cut in 0..full.len() {
            assert!(
                CapabilityGrant::from_bytes(&full[..cut]).is_err(),
                "truncating to {cut} bytes must be rejected"
            );
        }
    }

    #[test]
    fn trailing_bytes_are_rejected() {
        let f = fixture();
        let (grant, _proof) = issue_and_prove(&f, CapabilityRight::Read);
        let mut bytes = grant.to_bytes();
        bytes.push(0xff);
        assert_eq!(
            CapabilityGrant::from_bytes(&bytes),
            Err(ObjectError::TrailingBytes)
        );
    }

    #[test]
    fn an_empty_or_default_token_is_still_a_valid_32_byte_secret() {
        // Not "rejected" -- CapabilityToken has no all-zero check because
        // it is device-generated, never caller-supplied untrusted input
        // (unlike a wire-decoded field). Document the boundary: this is a
        // local secret type, not a decode target.
        let token = CapabilityToken::from_bytes([0u8; 32]);
        let commitment_a = token.commit(
            &CapabilityScope::Object(ObjectId::of(b"x")),
            CapabilityRight::Read,
        );
        let commitment_b = token.commit(
            &CapabilityScope::Object(ObjectId::of(b"x")),
            CapabilityRight::Append,
        );
        assert_ne!(commitment_a, commitment_b);
    }
}
