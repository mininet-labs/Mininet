//! Votes: a validator device's signed commitment to a block hash at a given
//! height/round.
//!
//! ## `Capabilities::VOTE`'s first real consumer
//!
//! `did-mini` has carried `Capabilities::VOTE` since SPEC-01 §6 with the
//! documented meaning "this device may cast the root's single equal vote,"
//! but until this crate nothing actually checked it. [`verify_vote`] is that
//! check: a vote only counts if signed by a device the human-root delegated
//! with `VOTE` — capability scoping narrows *which* device may vote, it
//! never adds a vote (the root is still counted at most once, enforced one
//! layer up in [`crate::finality::verify_finality`]).

use did_mini::{verify_delegation, Capabilities, Controller, Did, IndexedSig, Kel};
use mini_crypto::{Signature, SignatureSuite};

use crate::error::{ChainError, Result};

/// Hard cap on device signatures accepted in one wire-encoded vote: a
/// well-formed vote carries a single device's signature(s); this generous
/// bound stops a malformed frame from forcing an unbounded allocation
/// before any verification runs (the same discipline
/// [`crate::MAX_VOTES_PER_CERTIFICATE`] applies one layer up).
const MAX_SIGS_PER_VOTE: usize = 16;

/// Hard cap on the length of a `did:mini` string accepted from the wire —
/// far above any real SCID, purely an allocation bound on untrusted input.
const MAX_DID_BYTES: usize = 512;

/// The two Tendermint-style voting phases. Only [`VoteKind::Precommit`]
/// counts toward finality ([`crate::finality::verify_finality`]);
/// `Prevote` exists so the type is honest about the two-phase protocol even
/// though this batch only verifies the phase that finalizes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum VoteKind {
    /// First-round signal of intent to commit a block.
    Prevote,
    /// The phase that finalizes: `>2/3` distinct Precommits on the same
    /// block at the same height/round is what quorum certificates count.
    Precommit,
}

impl VoteKind {
    fn to_byte(self) -> u8 {
        match self {
            VoteKind::Prevote => 0,
            VoteKind::Precommit => 1,
        }
    }
}

/// A signed vote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Vote {
    /// Which voting phase this is.
    pub kind: VoteKind,
    /// The height being voted on.
    pub height: u64,
    /// The consensus round within that height.
    pub round: u32,
    /// The block hash being voted for.
    pub block_hash: [u8; 32],
    /// The voting validator's identity root.
    pub validator_root: Did,
    /// The delegated device that signed this vote.
    pub validator_device: Did,
    signature: Vec<IndexedSig>,
}

impl Vote {
    /// What the device actually signs: everything that must be pinned for
    /// the vote to be unambiguous and unforgeable across heights/rounds/
    /// blocks/phases.
    fn transcript(kind: VoteKind, height: u64, round: u32, block_hash: &[u8; 32]) -> Vec<u8> {
        let mut w = Vec::with_capacity(1 + 8 + 4 + 32);
        w.push(kind.to_byte());
        w.extend_from_slice(&height.to_be_bytes());
        w.extend_from_slice(&round.to_be_bytes());
        w.extend_from_slice(block_hash);
        w
    }

    /// The device signatures over this vote's transcript.
    pub fn signature(&self) -> &[IndexedSig] {
        &self.signature
    }

    /// Canonical wire bytes for this vote: domain-tagged, every field width-
    /// or length-prefixed, so no two distinct votes ever encode alike and the
    /// exact same bytes are produced on every platform (the same discipline
    /// [`BlockHeader::canonical_bytes`](crate::BlockHeader::canonical_bytes)
    /// and `did_mini`'s own codec use).
    ///
    /// A signed vote is inherently a *network* object — this is the layer at
    /// which `did:mini` votes leave a device — so its wire form lives here in
    /// `mini-chain` rather than being reconstructed field-by-field by every
    /// networking crate that must carry it. Keeping it here also keeps the
    /// private [`Vote::signature`] field's invariant local: the only way to
    /// obtain a `Vote` is still [`sign_vote`] or a round-trip through these
    /// two methods, never an ad-hoc struct literal in another crate.
    pub fn to_wire_bytes(&self) -> Vec<u8> {
        let mut w = Vec::new();
        w.extend_from_slice(b"mini-chain/vote/v1");
        w.push(self.kind.to_byte());
        w.extend_from_slice(&self.height.to_be_bytes());
        w.extend_from_slice(&self.round.to_be_bytes());
        w.extend_from_slice(&self.block_hash);
        put_str(&mut w, self.validator_root.as_str());
        put_str(&mut w, self.validator_device.as_str());
        w.extend_from_slice(&(self.signature.len() as u32).to_be_bytes());
        for sig in &self.signature {
            w.extend_from_slice(&sig.index.to_be_bytes());
            w.push(sig.signature.suite().tag());
            w.extend_from_slice(&sig.signature.to_bytes());
        }
        w
    }

    /// Reconstruct a vote from [`Vote::to_wire_bytes`]. Purely structural:
    /// it validates the framing (domain tag, bounds, no trailing bytes) but
    /// makes **no** trust decision — a decoded vote is exactly as untrusted
    /// as one that arrived any other way, and [`verify_vote`] remains the
    /// only thing that decides whether it counts.
    pub fn from_wire_bytes(bytes: &[u8]) -> Result<Self> {
        let mut r = SliceReader::new(bytes);
        let domain = r.take(b"mini-chain/vote/v1".len())?;
        if domain != b"mini-chain/vote/v1" {
            return Err(ChainError::Malformed);
        }
        let kind = match r.u8()? {
            0 => VoteKind::Prevote,
            1 => VoteKind::Precommit,
            _ => return Err(ChainError::Malformed),
        };
        let height = r.u64()?;
        let round = r.u32()?;
        let mut block_hash = [0u8; 32];
        block_hash.copy_from_slice(r.take(32)?);
        let validator_root = take_did(&mut r)?;
        let validator_device = take_did(&mut r)?;
        let sig_count = r.u32()? as usize;
        if sig_count > MAX_SIGS_PER_VOTE {
            return Err(ChainError::Malformed);
        }
        let mut signature = Vec::with_capacity(sig_count);
        for _ in 0..sig_count {
            let index = r.u32()?;
            let suite = SignatureSuite::from_tag(r.u8()?).map_err(|_| ChainError::Malformed)?;
            let sig_bytes = r.take(suite.signature_len())?;
            let sig =
                Signature::from_suite_bytes(suite, sig_bytes).map_err(|_| ChainError::Malformed)?;
            signature.push(IndexedSig {
                index,
                signature: sig,
            });
        }
        if !r.finished() {
            return Err(ChainError::Malformed);
        }
        Ok(Vote {
            kind,
            height,
            round,
            block_hash,
            validator_root,
            validator_device,
            signature,
        })
    }
}

/// Append a length-prefixed UTF-8 string (`u32` big-endian length, then bytes).
fn put_str(w: &mut Vec<u8>, s: &str) {
    w.extend_from_slice(&(s.len() as u32).to_be_bytes());
    w.extend_from_slice(s.as_bytes());
}

/// Read a length-prefixed, bounded `did:mini` string and parse it.
fn take_did(r: &mut SliceReader<'_>) -> Result<Did> {
    let len = r.u32()? as usize;
    if len > MAX_DID_BYTES {
        return Err(ChainError::Malformed);
    }
    let s = core::str::from_utf8(r.take(len)?).map_err(|_| ChainError::Malformed)?;
    Did::parse(s).map_err(|_| ChainError::Malformed)
}

/// A minimal cursor over untrusted bytes: every read is bounds-checked and
/// returns [`ChainError::Malformed`] on truncation, so a short or lying
/// frame can never index out of range or over-allocate.
struct SliceReader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> SliceReader<'a> {
    fn new(buf: &'a [u8]) -> Self {
        SliceReader { buf, pos: 0 }
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self.pos.checked_add(n).ok_or(ChainError::Malformed)?;
        let slice = self.buf.get(self.pos..end).ok_or(ChainError::Malformed)?;
        self.pos = end;
        Ok(slice)
    }

    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }

    fn u32(&mut self) -> Result<u32> {
        let mut a = [0u8; 4];
        a.copy_from_slice(self.take(4)?);
        Ok(u32::from_be_bytes(a))
    }

    fn u64(&mut self) -> Result<u64> {
        let mut a = [0u8; 8];
        a.copy_from_slice(self.take(8)?);
        Ok(u64::from_be_bytes(a))
    }

    fn finished(&self) -> bool {
        self.pos == self.buf.len()
    }
}

/// Sign a vote with a delegated device. Does not itself check that `device`
/// holds `VOTE` — that is enforced on the verifying side
/// ([`verify_vote`]), matching this tree's convention that signing is
/// local and free, verification is where trust decisions happen.
pub fn sign_vote(
    kind: VoteKind,
    height: u64,
    round: u32,
    block_hash: [u8; 32],
    validator_root: &Did,
    device: &Controller,
) -> Vote {
    let transcript = Vote::transcript(kind, height, round, &block_hash);
    let signature = device.sign_message(&transcript);
    Vote {
        kind,
        height,
        round,
        block_hash,
        validator_root: validator_root.clone(),
        validator_device: device.did(),
        signature,
    }
}

/// Verify one vote: the supplied KELs must actually be this vote's claimed
/// root/device, the device must currently be a delegated, unrevoked,
/// `VOTE`-capable device of that root, and the signature must verify over
/// the exact (kind, height, round, block_hash) transcript.
pub fn verify_vote(vote: &Vote, root_kel: &Kel, device_kel: &Kel) -> Result<()> {
    if device_kel.did().as_str() != vote.validator_device.as_str() {
        return Err(ChainError::DeviceMismatch);
    }
    if root_kel.did().as_str() != vote.validator_root.as_str() {
        return Err(ChainError::DeviceMismatch);
    }
    let caps = verify_delegation(root_kel, device_kel)?;
    if !caps.contains(Capabilities::VOTE) {
        return Err(ChainError::MissingVoteCapability);
    }
    let transcript = Vote::transcript(vote.kind, vote.height, vote.round, &vote.block_hash);
    device_kel.verify_message(&transcript, &vote.signature)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use did_mini::Capabilities;

    fn voter() -> (Controller, Controller) {
        let mut root = Controller::incept_single_from_seeds(&[7u8; 32], &[8u8; 32]).unwrap();
        let device =
            Controller::incept_device_single_from_seeds(&root.did(), &[9u8; 32], &[10u8; 32])
                .unwrap();
        root.delegate_device(&device.did(), Capabilities::primary())
            .unwrap();
        (root, device)
    }

    #[test]
    fn a_signed_vote_survives_a_wire_round_trip_byte_for_byte() {
        let (root, device) = voter();
        let vote = sign_vote(
            VoteKind::Precommit,
            42,
            3,
            [0xABu8; 32],
            &root.did(),
            &device,
        );
        let bytes = vote.to_wire_bytes();
        let back = Vote::from_wire_bytes(&bytes).unwrap();
        assert_eq!(vote, back);
        // And the round-tripped vote still verifies against the real KELs --
        // decoding preserves the signature, not just the structural fields.
        verify_vote(&back, &root.kel(), &device.kel()).unwrap();
    }

    #[test]
    fn prevote_and_precommit_both_round_trip() {
        let (root, device) = voter();
        for kind in [VoteKind::Prevote, VoteKind::Precommit] {
            let vote = sign_vote(kind, 1, 0, [1u8; 32], &root.did(), &device);
            assert_eq!(Vote::from_wire_bytes(&vote.to_wire_bytes()).unwrap(), vote);
        }
    }

    #[test]
    fn signed_vote_cannot_be_replayed_in_another_context() {
        let (root, device) = voter();
        let vote = sign_vote(VoteKind::Prevote, 7, 2, [0xAA; 32], &root.did(), &device);

        let mut wrong_phase = vote.clone();
        wrong_phase.kind = VoteKind::Precommit;
        assert!(verify_vote(&wrong_phase, &root.kel(), &device.kel()).is_err());

        let mut wrong_height = vote.clone();
        wrong_height.height += 1;
        assert!(verify_vote(&wrong_height, &root.kel(), &device.kel()).is_err());

        let mut wrong_round = vote.clone();
        wrong_round.round += 1;
        assert!(verify_vote(&wrong_round, &root.kel(), &device.kel()).is_err());

        let mut wrong_block = vote;
        wrong_block.block_hash[0] ^= 1;
        assert!(verify_vote(&wrong_block, &root.kel(), &device.kel()).is_err());
    }

    #[test]
    fn a_truncated_frame_is_rejected_not_panicked() {
        let (root, device) = voter();
        let bytes =
            sign_vote(VoteKind::Precommit, 1, 0, [1u8; 32], &root.did(), &device).to_wire_bytes();
        for cut in 0..bytes.len() {
            assert_eq!(
                Vote::from_wire_bytes(&bytes[..cut]).unwrap_err(),
                ChainError::Malformed
            );
        }
    }

    #[test]
    fn trailing_garbage_after_a_valid_vote_is_rejected() {
        let (root, device) = voter();
        let mut bytes =
            sign_vote(VoteKind::Precommit, 1, 0, [1u8; 32], &root.did(), &device).to_wire_bytes();
        bytes.push(0);
        assert_eq!(
            Vote::from_wire_bytes(&bytes).unwrap_err(),
            ChainError::Malformed
        );
    }

    #[test]
    fn an_absurd_signature_count_is_rejected_before_allocating() {
        // A frame that claims u32::MAX signatures must fail on the bound, not
        // try to reserve capacity for billions of entries.
        let (root, device) = voter();
        let good =
            sign_vote(VoteKind::Precommit, 1, 0, [1u8; 32], &root.did(), &device).to_wire_bytes();
        // The signature count is the last u32 before the signature entries.
        // Rebuild the prefix and splice in a huge count.
        let mut bytes = good.clone();
        let count_pos = bytes.len()
            - (4 + 1 + SignatureSuite::Ed25519.signature_len()) // one sig entry
            - 4; // the count field itself
        bytes[count_pos..count_pos + 4].copy_from_slice(&u32::MAX.to_be_bytes());
        assert_eq!(
            Vote::from_wire_bytes(&bytes).unwrap_err(),
            ChainError::Malformed
        );
    }
}
