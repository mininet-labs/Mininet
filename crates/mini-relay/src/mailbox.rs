//! Rotating mailbox capabilities (`MN-202`): a holder-bound, token-
//! committed grant authorizing pickup from one opaque mailbox — a
//! separate typed-domain capability from `mini_objects::CapabilityGrant`
//! rather than a retrofit of that crate's `Object`-scoped design, so a
//! relay-mailbox capability can never be confused with (or accidentally
//! satisfy a check meant for) an object capability. Mailbox pickup has no
//! independent-rights structure the way object access does (read/append/
//! reply/moderate/administer) — holding a valid grant means "may collect
//! from this mailbox," full stop.
//!
//! Research §5.2: "rotate relays and queues." Rotation is achieved by
//! issuing a fresh [`MailboxGrant`] (new [`MailboxId`], new token, new
//! validity window) before the old one expires — there is no dedicated
//! rotation API, the same way `mini_objects::CapabilityGrant` has none.

use did_mini::{Controller, Did, IndexedSig, Kel};

use crate::codec::{Reader, Writer};
use crate::error::{RelayError, Result};

/// This module's grant format version.
pub const MAILBOX_GRANT_VERSION: u8 = 1;

const GRANT_SIGNING_DOMAIN: &[u8] = b"mininet/mini-relay/mailbox-grant/v1";
const TOKEN_COMMITMENT_DOMAIN: &[u8] = b"mininet/mini-relay/mailbox-token-commitment/v1";
const HOLDER_PROOF_DOMAIN: &[u8] = b"mininet/mini-relay/mailbox-holder-proof/v1";

const MAX_DID_BYTES: usize = 256;
const MAX_SIGNATURES: usize = 16;
const MAX_SIG_BYTES: usize = 256;

/// An opaque, random mailbox identifier — not content-addressed (a
/// mailbox has no content to hash over) and not derived from any `did:
/// mini` root, so it carries no information about who owns it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MailboxId([u8; 32]);

impl MailboxId {
    /// A fresh random mailbox id from OS entropy.
    pub fn generate() -> Result<Self> {
        Ok(MailboxId(
            mini_crypto::random_32().map_err(RelayError::Crypto)?,
        ))
    }

    pub fn to_bytes(self) -> [u8; 32] {
        self.0
    }

    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        MailboxId(bytes)
    }
}

/// An unguessable, device-local bearer secret. Possession of both this
/// token *and* a valid holder-proof signature is required to exercise a
/// [`MailboxGrant`] — a leaked grant (public, signed data) alone is not
/// enough, and a leaked token alone (without the grantee's signing key)
/// is not enough either. Mirrors `mini_objects::capability::
/// CapabilityToken`'s exact discipline.
pub struct MailboxToken([u8; 32]);

impl MailboxToken {
    /// A fresh random token from OS entropy.
    pub fn generate() -> Result<Self> {
        Ok(MailboxToken(
            mini_crypto::random_32().map_err(RelayError::Crypto)?,
        ))
    }

    /// Rebuild from bytes already generated (device-local storage only —
    /// never transmitted, never logged).
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        MailboxToken(bytes)
    }

    /// The public commitment an issuer binds into a [`MailboxGrant`],
    /// domain-separated by the exact mailbox so a commitment copied into
    /// a grant for a different mailbox never matches.
    pub fn commit(&self, mailbox: MailboxId) -> MailboxTokenCommitment {
        let mut w = Writer::new();
        w.raw(TOKEN_COMMITMENT_DOMAIN);
        w.raw(&mailbox.to_bytes());
        w.raw(&self.0);
        MailboxTokenCommitment(mini_crypto::HashAlgorithm::Blake3.digest(&w.into_bytes()))
    }
}

impl core::fmt::Debug for MailboxToken {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("MailboxToken(REDACTED)")
    }
}

impl Drop for MailboxToken {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.0.zeroize();
    }
}

/// A public commitment to a [`MailboxToken`] — safe to include in a
/// signed, published [`MailboxGrant`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MailboxTokenCommitment([u8; 32]);

/// An exact, non-delegable authorization: `grantee` may collect from
/// `mailbox`, provided it also presents the token behind
/// `token_commitment` and proves control of `grantee` — see
/// [`MailboxGrant::validate`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MailboxGrant {
    pub issuer: Did,
    pub grantee: Did,
    pub mailbox: MailboxId,
    pub token_commitment: MailboxTokenCommitment,
    pub not_before_ms: Option<u64>,
    pub expires_at_ms: Option<u64>,
    nonce: [u8; 16],
    signature: Vec<IndexedSig>,
}

impl MailboxGrant {
    /// Issue and sign a grant. `token` is only used to compute its public
    /// commitment here — the secret itself never enters the grant.
    pub fn issue(
        issuer: &Controller,
        grantee: Did,
        mailbox: MailboxId,
        token: &MailboxToken,
        not_before_ms: Option<u64>,
        expires_at_ms: Option<u64>,
    ) -> Result<Self> {
        let token_commitment = token.commit(mailbox);
        let nonce = mini_crypto::random_32().map_err(RelayError::Crypto)?;
        let nonce: [u8; 16] = nonce[..16]
            .try_into()
            .expect("first 16 bytes of a 32-byte array always convert");
        let mut grant = MailboxGrant {
            issuer: issuer.did(),
            grantee,
            mailbox,
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
        w.u8(MAILBOX_GRANT_VERSION);
        w.bytes(self.issuer.as_str().as_bytes());
        w.bytes(self.grantee.as_str().as_bytes());
        w.raw(&self.mailbox.to_bytes());
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
    /// operation in this workspace, and bound to this exact grant (via
    /// its nonce and token commitment) so a holder proof for one grant
    /// can never be replayed against another.
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
    /// [`MailboxGrant::validate`].
    pub fn prove_holder(&self, grantee: &Controller) -> Vec<IndexedSig> {
        grantee.sign_message(&self.holder_proof_message())
    }

    /// Full validation: issuer signature, exact mailbox match, token
    /// possession, validity window, and holder proof. Fails closed on any
    /// mismatch — never partially authorizes.
    #[allow(clippy::too_many_arguments)]
    pub fn validate(
        &self,
        issuer_kel: &Kel,
        requested_mailbox: MailboxId,
        token: &MailboxToken,
        grantee_kel: &Kel,
        holder_proof: &[IndexedSig],
        now_ms: u64,
    ) -> Result<()> {
        if issuer_kel.did().as_str() != self.issuer.as_str() {
            return Err(RelayError::MailboxIssuerMismatch);
        }
        issuer_kel
            .verify_message(&self.signing_bytes(), &self.signature)
            .map_err(RelayError::Identity)?;
        if self.mailbox != requested_mailbox {
            return Err(RelayError::MailboxMismatch);
        }
        if token.commit(self.mailbox) != self.token_commitment {
            return Err(RelayError::MailboxTokenMismatch);
        }
        if let Some(not_before) = self.not_before_ms {
            if now_ms < not_before {
                return Err(RelayError::MailboxNotYetValid);
            }
        }
        if let Some(expires_at) = self.expires_at_ms {
            if now_ms >= expires_at {
                return Err(RelayError::MailboxExpired);
            }
        }
        if grantee_kel.did().as_str() != self.grantee.as_str() {
            return Err(RelayError::MailboxGranteeMismatch);
        }
        grantee_kel
            .verify_message(&self.holder_proof_message(), holder_proof)
            .map_err(|_| RelayError::MailboxGranteeMismatch)?;
        Ok(())
    }

    /// Canonical wire bytes (fields + signature), for storage/transport.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.u8(MAILBOX_GRANT_VERSION);
        w.bytes(self.issuer.as_str().as_bytes());
        w.bytes(self.grantee.as_str().as_bytes());
        w.raw(&self.mailbox.to_bytes());
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
    /// [`MAILBOX_GRANT_VERSION`], and trailing bytes. Does **not** verify
    /// the signature — call [`MailboxGrant::validate`] with the issuer's
    /// KEL for that.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let mut r = Reader::new(bytes);
        if r.u8()? != MAILBOX_GRANT_VERSION {
            return Err(RelayError::UnsupportedMailboxGrantVersion);
        }
        let issuer = parse_did(r.bytes_limited(MAX_DID_BYTES)?)?;
        let grantee = parse_did(r.bytes_limited(MAX_DID_BYTES)?)?;
        let mailbox_bytes: [u8; 32] = r
            .raw(32)?
            .try_into()
            .expect("Reader::raw(32) always returns exactly 32 bytes");
        let mailbox = MailboxId::from_bytes(mailbox_bytes);
        let token_commitment_bytes: [u8; 32] = r
            .raw(32)?
            .try_into()
            .expect("Reader::raw(32) always returns exactly 32 bytes");
        let token_commitment = MailboxTokenCommitment(token_commitment_bytes);
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
            return Err(RelayError::LimitExceeded);
        }
        let mut signature = Vec::with_capacity(nsigs);
        for _ in 0..nsigs {
            let index = r.u32()?;
            let sig_suite =
                mini_crypto::SignatureSuite::from_tag(r.u8()?).map_err(RelayError::Crypto)?;
            let sig_bytes = r.bytes_limited(MAX_SIG_BYTES)?;
            let sig = mini_crypto::Signature::from_suite_bytes(sig_suite, &sig_bytes)
                .map_err(RelayError::Crypto)?;
            signature.push(IndexedSig {
                index,
                signature: sig,
            });
        }
        if !r.finished() {
            return Err(RelayError::TrailingBytes);
        }
        Ok(MailboxGrant {
            issuer,
            grantee,
            mailbox,
            token_commitment,
            not_before_ms,
            expires_at_ms,
            nonce,
            signature,
        })
    }
}

fn parse_did(bytes: Vec<u8>) -> Result<Did> {
    let s = String::from_utf8(bytes).map_err(|_| RelayError::TrailingBytes)?;
    Ok(Did::parse(&s)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture {
        issuer: Controller,
        grantee: Controller,
        mailbox: MailboxId,
        token: MailboxToken,
    }

    fn fixture() -> Fixture {
        Fixture {
            issuer: Controller::incept_single().unwrap(),
            grantee: Controller::incept_single().unwrap(),
            mailbox: MailboxId::generate().unwrap(),
            token: MailboxToken::generate().unwrap(),
        }
    }

    fn issue_and_prove(f: &Fixture) -> (MailboxGrant, Vec<IndexedSig>) {
        let grant =
            MailboxGrant::issue(&f.issuer, f.grantee.did(), f.mailbox, &f.token, None, None)
                .unwrap();
        let proof = grant.prove_holder(&f.grantee);
        (grant, proof)
    }

    #[test]
    fn a_valid_grant_and_proof_validate_successfully() {
        let f = fixture();
        let (grant, proof) = issue_and_prove(&f);
        grant
            .validate(
                &f.issuer.kel(),
                f.mailbox,
                &f.token,
                &f.grantee.kel(),
                &proof,
                0,
            )
            .unwrap();
    }

    #[test]
    fn a_grant_for_mailbox_a_fails_against_mailbox_b() {
        let f = fixture();
        let (grant, proof) = issue_and_prove(&f);
        let other_mailbox = MailboxId::generate().unwrap();
        let err = grant
            .validate(
                &f.issuer.kel(),
                other_mailbox,
                &f.token,
                &f.grantee.kel(),
                &proof,
                0,
            )
            .unwrap_err();
        assert_eq!(err, RelayError::MailboxMismatch);
    }

    #[test]
    fn an_incorrect_token_fails() {
        let f = fixture();
        let (grant, proof) = issue_and_prove(&f);
        let wrong_token = MailboxToken::generate().unwrap();
        let err = grant
            .validate(
                &f.issuer.kel(),
                f.mailbox,
                &wrong_token,
                &f.grantee.kel(),
                &proof,
                0,
            )
            .unwrap_err();
        assert_eq!(err, RelayError::MailboxTokenMismatch);
    }

    #[test]
    fn a_token_commitment_copied_into_a_different_mailbox_grant_does_not_validate() {
        let f = fixture();
        let other_mailbox = MailboxId::generate().unwrap();
        // Same token, but issued (committed) for a different mailbox.
        let grant = MailboxGrant::issue(
            &f.issuer,
            f.grantee.did(),
            other_mailbox,
            &f.token,
            None,
            None,
        )
        .unwrap();
        let proof = grant.prove_holder(&f.grantee);
        let err = grant
            .validate(
                &f.issuer.kel(),
                f.mailbox,
                &f.token,
                &f.grantee.kel(),
                &proof,
                0,
            )
            .unwrap_err();
        assert_eq!(err, RelayError::MailboxMismatch);
    }

    #[test]
    fn a_valid_token_without_grantee_proof_fails() {
        let f = fixture();
        let (grant, _proof) = issue_and_prove(&f);
        let bogus_proof = vec![];
        let err = grant
            .validate(
                &f.issuer.kel(),
                f.mailbox,
                &f.token,
                &f.grantee.kel(),
                &bogus_proof,
                0,
            )
            .unwrap_err();
        assert_eq!(err, RelayError::MailboxGranteeMismatch);
    }

    #[test]
    fn proof_from_another_pseudonym_fails() {
        let f = fixture();
        let (grant, _proof) = issue_and_prove(&f);
        let impostor = Controller::incept_single().unwrap();
        let impostor_proof = impostor.sign_message(&grant.holder_proof_message());
        let err = grant
            .validate(
                &f.issuer.kel(),
                f.mailbox,
                &f.token,
                &f.grantee.kel(),
                &impostor_proof,
                0,
            )
            .unwrap_err();
        assert!(matches!(
            err,
            RelayError::MailboxGranteeMismatch | RelayError::Identity(_)
        ));
    }

    #[test]
    fn an_expired_grant_fails() {
        let f = fixture();
        let grant = MailboxGrant::issue(
            &f.issuer,
            f.grantee.did(),
            f.mailbox,
            &f.token,
            None,
            Some(1_000),
        )
        .unwrap();
        let proof = grant.prove_holder(&f.grantee);
        let err = grant
            .validate(
                &f.issuer.kel(),
                f.mailbox,
                &f.token,
                &f.grantee.kel(),
                &proof,
                1_000,
            )
            .unwrap_err();
        assert_eq!(err, RelayError::MailboxExpired);
    }

    #[test]
    fn a_not_yet_valid_grant_fails() {
        let f = fixture();
        let grant = MailboxGrant::issue(
            &f.issuer,
            f.grantee.did(),
            f.mailbox,
            &f.token,
            Some(1_000),
            None,
        )
        .unwrap();
        let proof = grant.prove_holder(&f.grantee);
        let err = grant
            .validate(
                &f.issuer.kel(),
                f.mailbox,
                &f.token,
                &f.grantee.kel(),
                &proof,
                500,
            )
            .unwrap_err();
        assert_eq!(err, RelayError::MailboxNotYetValid);
    }

    #[test]
    fn a_grant_signed_by_one_issuer_does_not_validate_against_another() {
        let f = fixture();
        let (grant, proof) = issue_and_prove(&f);
        let other_issuer = Controller::incept_single().unwrap();
        let err = grant
            .validate(
                &other_issuer.kel(),
                f.mailbox,
                &f.token,
                &f.grantee.kel(),
                &proof,
                0,
            )
            .unwrap_err();
        assert_eq!(err, RelayError::MailboxIssuerMismatch);
    }

    #[test]
    fn a_grant_round_trips_through_wire_bytes() {
        let f = fixture();
        let (grant, _proof) = issue_and_prove(&f);
        let decoded = MailboxGrant::from_bytes(&grant.to_bytes()).unwrap();
        assert_eq!(decoded, grant);
    }

    #[test]
    fn a_grant_with_a_validity_window_round_trips_and_still_enforces_it() {
        let f = fixture();
        let grant = MailboxGrant::issue(
            &f.issuer,
            f.grantee.did(),
            f.mailbox,
            &f.token,
            Some(1_000),
            Some(2_000),
        )
        .unwrap();
        let proof = grant.prove_holder(&f.grantee);
        let decoded = MailboxGrant::from_bytes(&grant.to_bytes()).unwrap();
        assert_eq!(decoded.not_before_ms, Some(1_000));
        assert_eq!(decoded.expires_at_ms, Some(2_000));
        assert_eq!(
            decoded
                .validate(
                    &f.issuer.kel(),
                    f.mailbox,
                    &f.token,
                    &f.grantee.kel(),
                    &proof,
                    500
                )
                .unwrap_err(),
            RelayError::MailboxNotYetValid
        );
        decoded
            .validate(
                &f.issuer.kel(),
                f.mailbox,
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
                    f.mailbox,
                    &f.token,
                    &f.grantee.kel(),
                    &proof,
                    2_000
                )
                .unwrap_err(),
            RelayError::MailboxExpired
        );
    }

    #[test]
    fn a_decoded_grant_still_validates() {
        let f = fixture();
        let (grant, proof) = issue_and_prove(&f);
        let decoded = MailboxGrant::from_bytes(&grant.to_bytes()).unwrap();
        decoded
            .validate(
                &f.issuer.kel(),
                f.mailbox,
                &f.token,
                &f.grantee.kel(),
                &proof,
                0,
            )
            .unwrap();
    }

    #[test]
    fn an_unknown_grant_version_is_rejected() {
        let mut w = Writer::new();
        w.u8(0xee);
        assert_eq!(
            MailboxGrant::from_bytes(&w.into_bytes()),
            Err(RelayError::UnsupportedMailboxGrantVersion)
        );
    }

    #[test]
    fn a_truncated_grant_is_rejected_at_every_length() {
        let f = fixture();
        let (grant, _proof) = issue_and_prove(&f);
        let full = grant.to_bytes();
        for cut in 0..full.len() {
            assert!(
                MailboxGrant::from_bytes(&full[..cut]).is_err(),
                "truncating to {cut} bytes must be rejected"
            );
        }
    }

    #[test]
    fn trailing_bytes_are_rejected() {
        let f = fixture();
        let (grant, _proof) = issue_and_prove(&f);
        let mut bytes = grant.to_bytes();
        bytes.push(0xff);
        assert_eq!(
            MailboxGrant::from_bytes(&bytes),
            Err(RelayError::TrailingBytes)
        );
    }

    #[test]
    fn rotating_to_a_fresh_mailbox_and_token_invalidates_the_old_pair() {
        // "Rotation" is just issuing a new grant with a new mailbox/token —
        // there is no dedicated rotation API. The old grant/token pair must
        // not satisfy validation against the new mailbox.
        let f = fixture();
        let (old_grant, old_proof) = issue_and_prove(&f);
        let new_mailbox = MailboxId::generate().unwrap();
        let new_token = MailboxToken::generate().unwrap();
        let new_grant = MailboxGrant::issue(
            &f.issuer,
            f.grantee.did(),
            new_mailbox,
            &new_token,
            None,
            None,
        )
        .unwrap();
        let new_proof = new_grant.prove_holder(&f.grantee);

        // Old grant still works for the old mailbox...
        old_grant
            .validate(
                &f.issuer.kel(),
                f.mailbox,
                &f.token,
                &f.grantee.kel(),
                &old_proof,
                0,
            )
            .unwrap();
        // ...but the old token/proof do not satisfy the new grant's mailbox.
        let err = new_grant
            .validate(
                &f.issuer.kel(),
                new_mailbox,
                &f.token,
                &f.grantee.kel(),
                &old_proof,
                0,
            )
            .unwrap_err();
        assert_eq!(err, RelayError::MailboxTokenMismatch);

        new_grant
            .validate(
                &f.issuer.kel(),
                new_mailbox,
                &new_token,
                &f.grantee.kel(),
                &new_proof,
                0,
            )
            .unwrap();
    }
}
