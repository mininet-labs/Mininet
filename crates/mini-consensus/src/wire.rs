//! The consensus wire protocol: a canonical, domain-tagged, length-prefixed,
//! *bounded* codec for the two messages a round puts on the wire.
//!
//! Hand-rolled for the same reason `did_mini`'s codec and every content hash
//! in this tree are: the bytes are fully determined (no framework, no field-
//! ordering or canonicalisation ambiguity), so the exact same message decodes
//! identically on every peer, and every read is bounds-checked against a hard
//! cap before anything is allocated. A malformed or hostile frame can only
//! ever produce [`ConsensusError::Malformed`]/[`ConsensusError::TooLarge`] —
//! never an out-of-bounds read or an unbounded allocation.
//!
//! Decoding makes **no** trust decision. A well-framed message from an
//! attacker decodes fine and is rejected later, on its merits: a proposal by
//! [`crate::round::Round`] (wrong proposer, wrong parent), a vote by
//! [`mini_chain::verify_finality`] (bad signature, non-validator).

use did_mini::{verify_delegation, Capabilities, Controller, Did, IndexedSig, Kel};
use mini_chain::{BlockHeader, Vote};
use mini_crypto::{Signature, SignatureSuite};
use mini_execution::{SettlementBlockBody, MAX_CLAIMS_PER_BLOCK};
use mini_settlement::PaymentClaim;

use crate::error::{ConsensusError, Result};

/// Domain tag: bump the version to evolve the format without ever letting a
/// v1 message be mistaken for a later one.
const DOMAIN: &[u8] = b"mini-consensus/msg/v1";

/// Domain tag for the bytes a proposer signs — distinct from the message
/// framing tag so a proposal signature can never be confused with any other
/// signed object in this tree.
const PROPOSAL_SIGN_DOMAIN: &[u8] = b"mini-consensus/proposal/v1";

/// Hard cap on device signatures in one proposal (a well-formed proposal
/// carries one device's signature; the bound stops a malformed frame forcing
/// an unbounded allocation before verification).
const MAX_SIGS_PER_PROPOSAL: usize = 16;

/// Hard cap on a single encoded consensus message. Matches
/// [`mini_bearer::MAX_FRAME_BYTES`] — the transport under [`crate::net`]
/// already refuses to carry anything larger, and this crate re-checks so the
/// bound holds for any transport, not just that one.
pub const MAX_MESSAGE_BYTES: usize = mini_bearer::MAX_FRAME_BYTES;

/// Hard cap on any single opaque byte field a claim carries (payer/payee
/// key material, `last_known_chain`). Far above any real key, purely an
/// allocation bound on untrusted input.
const MAX_OPAQUE_FIELD_BYTES: usize = 4096;

/// Hard cap on a `did:mini` string from the wire — far above any real SCID.
const MAX_DID_BYTES: usize = 512;

/// A block proposed at one `(height, round)`, **signed by the round's
/// proposer**: the consensus round it belongs to, its `valid_round` (the
/// paper's `validRound`: the round a re-proposed value was previously
/// locked/valid in, or `-1` for a fresh value), the value itself (the header
/// and ordered body everyone needs in full to recompute the block hash they
/// vote on and apply the block once it finalizes), and the proposer's
/// signature over the typed transcript.
///
/// The signed `proposer_root` is the **current round's** proposer — which for
/// a re-proposed `validValue` differs from the value's own
/// `header.proposer` (the original builder). Authenticating the message
/// sender is what closes the front-running gap the round-0/D-0201 slices
/// left open: only [`proposer_for`](crate::proposer_for)'s designated proposer
/// for `(height, round)` can get a value considered that round. Build one with
/// [`sign_proposal`]; never by struct literal (the signature field is
/// private, so a `Proposal` cannot be forged into existence).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Proposal {
    /// The consensus round this proposal is made in.
    pub round: u32,
    /// The proposal's `validRound` (`-1` for a fresh value).
    pub valid_round: i64,
    /// The identity root of this round's proposer (the message signer).
    pub proposer_root: Did,
    /// The delegated device that signed this proposal.
    pub proposer_device: Did,
    /// The proposed block header.
    pub header: BlockHeader,
    /// The ordered claim body the header's `state_root` commits to.
    pub body: SettlementBlockBody,
    signature: Vec<IndexedSig>,
}

impl Proposal {
    /// The exact bytes the proposer signs: domain-tagged and binding every
    /// field that must be unforgeable across heights/rounds/values — the
    /// block's height, the consensus round, the `valid_round`, the value id
    /// (block hash), and the proposer root. Two distinct proposals can never
    /// share a transcript.
    fn transcript(
        height: u64,
        round: u32,
        valid_round: i64,
        block_hash: &[u8; 32],
        proposer_root: &Did,
    ) -> Vec<u8> {
        let mut w = Vec::new();
        w.extend_from_slice(PROPOSAL_SIGN_DOMAIN);
        w.extend_from_slice(&height.to_be_bytes());
        w.extend_from_slice(&round.to_be_bytes());
        w.extend_from_slice(&valid_round.to_be_bytes());
        w.extend_from_slice(block_hash);
        let r = proposer_root.as_str().as_bytes();
        w.extend_from_slice(&(r.len() as u32).to_be_bytes());
        w.extend_from_slice(r);
        w
    }

    /// The proposer's device signatures over this proposal's transcript.
    pub fn signature(&self) -> &[IndexedSig] {
        &self.signature
    }
}

/// Sign a proposal as `(height, round)`'s proposer, with a `VOTE`-capable
/// delegated `device` of `proposer_root`. Like [`mini_chain::sign_vote`],
/// signing is local and free; the `VOTE`-capability and proposer-designation
/// checks happen on the verifying side ([`verify_proposal`] plus the node's
/// `proposer_for` check). A typed request, never a generic `sign(bytes)`.
pub fn sign_proposal(
    round: u32,
    valid_round: i64,
    header: BlockHeader,
    body: SettlementBlockBody,
    proposer_root: &Did,
    device: &Controller,
) -> Proposal {
    let transcript = Proposal::transcript(
        header.height,
        round,
        valid_round,
        &header.hash(),
        proposer_root,
    );
    let signature = device.sign_message(&transcript);
    Proposal {
        round,
        valid_round,
        proposer_root: proposer_root.clone(),
        proposer_device: device.did(),
        header,
        body,
        signature,
    }
}

/// Verify a proposal's signature: the supplied KELs must be exactly this
/// proposal's claimed root/device, the device must be a currently-delegated,
/// unrevoked, `VOTE`-capable device of that root, and the signature must
/// verify over the transcript. Does **not** by itself check that
/// `proposer_root` is the *designated* proposer for `(height, round)` — that
/// is the caller's `proposer_for` check (the node's job, which holds the
/// validator set), the same way [`mini_chain::verify_vote`] leaves validator-
/// set membership to its caller.
pub fn verify_proposal(proposal: &Proposal, root_kel: &Kel, device_kel: &Kel) -> Result<()> {
    if device_kel.did().as_str() != proposal.proposer_device.as_str()
        || root_kel.did().as_str() != proposal.proposer_root.as_str()
    {
        return Err(ConsensusError::Malformed);
    }
    let caps = verify_delegation(root_kel, device_kel).map_err(|_| ConsensusError::Malformed)?;
    if !caps.contains(Capabilities::VOTE) {
        return Err(ConsensusError::Malformed);
    }
    let transcript = Proposal::transcript(
        proposal.header.height,
        proposal.round,
        proposal.valid_round,
        &proposal.header.hash(),
        &proposal.proposer_root,
    );
    device_kel
        .verify_message(&transcript, &proposal.signature)
        .map_err(|_| ConsensusError::Malformed)
}

/// Everything a consensus round sends over the wire.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum ConsensusMessage {
    /// A proposer's block proposal for a height.
    Proposal(Proposal),
    /// A validator device's signed prevote or precommit.
    Vote(Vote),
}

const TAG_PROPOSAL: u8 = 0;
const TAG_VOTE: u8 = 1;

impl ConsensusMessage {
    /// Canonical wire bytes for this message.
    pub fn to_wire_bytes(&self) -> Vec<u8> {
        let mut w = Vec::new();
        w.extend_from_slice(DOMAIN);
        match self {
            ConsensusMessage::Proposal(p) => {
                w.push(TAG_PROPOSAL);
                w.extend_from_slice(&p.round.to_be_bytes());
                w.extend_from_slice(&p.valid_round.to_be_bytes());
                put_bytes(&mut w, p.proposer_root.as_str().as_bytes());
                put_bytes(&mut w, p.proposer_device.as_str().as_bytes());
                encode_header(&mut w, &p.header);
                encode_body(&mut w, &p.body);
                encode_indexed_sigs(&mut w, &p.signature);
            }
            ConsensusMessage::Vote(v) => {
                w.push(TAG_VOTE);
                put_bytes(&mut w, &v.to_wire_bytes());
            }
        }
        w
    }

    /// Decode a message produced by [`ConsensusMessage::to_wire_bytes`].
    /// Structural only — see the module docs: a decoded message is exactly as
    /// untrusted as one that never left the attacker's machine.
    pub fn from_wire_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > MAX_MESSAGE_BYTES {
            return Err(ConsensusError::TooLarge);
        }
        let mut r = Reader::new(bytes);
        if r.take(DOMAIN.len())? != DOMAIN {
            return Err(ConsensusError::Malformed);
        }
        let msg = match r.u8()? {
            TAG_PROPOSAL => {
                let round = r.u32()?;
                let valid_round = r.i64()?;
                let proposer_root = decode_did(&mut r)?;
                let proposer_device = decode_did(&mut r)?;
                let header = decode_header(&mut r)?;
                let body = decode_body(&mut r)?;
                let signature = decode_indexed_sigs(&mut r)?;
                ConsensusMessage::Proposal(Proposal {
                    round,
                    valid_round,
                    proposer_root,
                    proposer_device,
                    header,
                    body,
                    signature,
                })
            }
            TAG_VOTE => {
                let vote_bytes = r.bytes(MAX_MESSAGE_BYTES)?;
                let vote =
                    Vote::from_wire_bytes(vote_bytes).map_err(|_| ConsensusError::Malformed)?;
                ConsensusMessage::Vote(vote)
            }
            _ => return Err(ConsensusError::Malformed),
        };
        if !r.finished() {
            return Err(ConsensusError::Malformed);
        }
        Ok(msg)
    }
}

fn encode_header(w: &mut Vec<u8>, h: &BlockHeader) {
    w.extend_from_slice(&h.height.to_be_bytes());
    w.extend_from_slice(&h.prev_hash);
    w.extend_from_slice(&h.state_root);
    w.extend_from_slice(&h.timestamp_ms.to_be_bytes());
    put_bytes(w, h.proposer.as_str().as_bytes());
}

fn decode_header(r: &mut Reader<'_>) -> Result<BlockHeader> {
    let height = r.u64()?;
    let mut prev_hash = [0u8; 32];
    prev_hash.copy_from_slice(r.take(32)?);
    let mut state_root = [0u8; 32];
    state_root.copy_from_slice(r.take(32)?);
    let timestamp_ms = r.u64()?;
    let proposer = decode_did(r)?;
    Ok(BlockHeader {
        height,
        prev_hash,
        state_root,
        timestamp_ms,
        proposer,
    })
}

fn encode_body(w: &mut Vec<u8>, b: &SettlementBlockBody) {
    w.extend_from_slice(&(b.claims.len() as u32).to_be_bytes());
    for c in &b.claims {
        put_bytes(w, &c.payer);
        put_bytes(w, &c.payee);
        w.extend_from_slice(&c.amount_micro.to_be_bytes());
        w.extend_from_slice(&c.sequence.to_be_bytes());
        w.extend_from_slice(&c.valid_until_ms.to_be_bytes());
        put_bytes(w, &c.last_known_chain);
        w.push(c.signature.suite().tag());
        w.extend_from_slice(&c.signature.to_bytes());
    }
}

fn decode_body(r: &mut Reader<'_>) -> Result<SettlementBlockBody> {
    let count = r.u32()? as usize;
    if count > MAX_CLAIMS_PER_BLOCK {
        return Err(ConsensusError::TooLarge);
    }
    // `count` is bounded above, but it is still attacker-supplied: reserve
    // only a modest amount up front and let the Vec grow, so a lone frame
    // claiming the maximum cannot force a large speculative allocation before
    // the (much smaller) real bytes are even read.
    let mut claims = Vec::with_capacity(count.min(64));
    for _ in 0..count {
        let payer = r.bytes(MAX_OPAQUE_FIELD_BYTES)?.to_vec();
        let payee = r.bytes(MAX_OPAQUE_FIELD_BYTES)?.to_vec();
        let amount_micro = r.u64()?;
        let sequence = r.u64()?;
        let valid_until_ms = r.u64()?;
        let last_known_chain = r.bytes(MAX_OPAQUE_FIELD_BYTES)?.to_vec();
        let suite = SignatureSuite::from_tag(r.u8()?).map_err(|_| ConsensusError::Malformed)?;
        let sig_bytes = r.take(suite.signature_len())?;
        let signature =
            Signature::from_suite_bytes(suite, sig_bytes).map_err(|_| ConsensusError::Malformed)?;
        claims.push(PaymentClaim {
            payer,
            payee,
            amount_micro,
            sequence,
            valid_until_ms,
            last_known_chain,
            signature,
        });
    }
    Ok(SettlementBlockBody::new(claims))
}

fn decode_did(r: &mut Reader<'_>) -> Result<Did> {
    let raw = r.bytes(MAX_DID_BYTES)?;
    let s = core::str::from_utf8(raw).map_err(|_| ConsensusError::Malformed)?;
    Did::parse(s).map_err(|_| ConsensusError::Malformed)
}

fn encode_indexed_sigs(w: &mut Vec<u8>, sigs: &[IndexedSig]) {
    w.extend_from_slice(&(sigs.len() as u32).to_be_bytes());
    for sig in sigs {
        w.extend_from_slice(&sig.index.to_be_bytes());
        w.push(sig.signature.suite().tag());
        w.extend_from_slice(&sig.signature.to_bytes());
    }
}

fn decode_indexed_sigs(r: &mut Reader<'_>) -> Result<Vec<IndexedSig>> {
    let count = r.u32()? as usize;
    if count > MAX_SIGS_PER_PROPOSAL {
        return Err(ConsensusError::TooLarge);
    }
    let mut sigs = Vec::with_capacity(count);
    for _ in 0..count {
        let index = r.u32()?;
        let suite = SignatureSuite::from_tag(r.u8()?).map_err(|_| ConsensusError::Malformed)?;
        let sig_bytes = r.take(suite.signature_len())?;
        let signature =
            Signature::from_suite_bytes(suite, sig_bytes).map_err(|_| ConsensusError::Malformed)?;
        sigs.push(IndexedSig { index, signature });
    }
    Ok(sigs)
}

/// Append a length-prefixed byte string (`u32` big-endian length, then bytes).
fn put_bytes(w: &mut Vec<u8>, b: &[u8]) {
    w.extend_from_slice(&(b.len() as u32).to_be_bytes());
    w.extend_from_slice(b);
}

/// A bounds-checked cursor over untrusted bytes.
struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(buf: &'a [u8]) -> Self {
        Reader { buf, pos: 0 }
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self.pos.checked_add(n).ok_or(ConsensusError::Malformed)?;
        let slice = self
            .buf
            .get(self.pos..end)
            .ok_or(ConsensusError::Malformed)?;
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

    fn i64(&mut self) -> Result<i64> {
        let mut a = [0u8; 8];
        a.copy_from_slice(self.take(8)?);
        Ok(i64::from_be_bytes(a))
    }

    /// A length-prefixed byte string, rejected if it declares more than `max`
    /// bytes *before* the slice is taken (so a lying length cannot allocate).
    fn bytes(&mut self, max: usize) -> Result<&'a [u8]> {
        let len = self.u32()? as usize;
        if len > max {
            return Err(ConsensusError::TooLarge);
        }
        self.take(len)
    }

    fn finished(&self) -> bool {
        self.pos == self.buf.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use did_mini::{Capabilities, Controller};
    use mini_chain::{sign_vote, VoteKind};
    use mini_crypto::SigningKey;
    use mini_settlement::sign_claim;

    fn header() -> BlockHeader {
        let root = Controller::incept_single_from_seeds(&[1u8; 32], &[2u8; 32]).unwrap();
        BlockHeader {
            height: 7,
            prev_hash: [3u8; 32],
            state_root: [4u8; 32],
            timestamp_ms: 12_345,
            proposer: root.did(),
        }
    }

    fn body() -> SettlementBlockBody {
        let payer = SigningKey::from_seed(&[9u8; 32]);
        let c1 = sign_claim(&payer, b"payee-one", 500, 0, 10_000, b"chain-1", 0).unwrap();
        let c2 = sign_claim(&payer, b"payee-two", 750, 1, 10_000, b"chain-1", 0).unwrap();
        SettlementBlockBody::new(vec![c1, c2])
    }

    /// A proposer identity: root + `VOTE`-capable delegated device.
    fn proposer() -> (Controller, Controller) {
        let mut root = Controller::incept_single_from_seeds(&[20u8; 32], &[21u8; 32]).unwrap();
        let device =
            Controller::incept_device_single_from_seeds(&root.did(), &[22u8; 32], &[23u8; 32])
                .unwrap();
        root.delegate_device(&device.did(), Capabilities::primary())
            .unwrap();
        (root, device)
    }

    /// A signed proposal over `header`/`body` from the fixture proposer.
    fn signed(
        round: u32,
        valid_round: i64,
        header: BlockHeader,
        body: SettlementBlockBody,
    ) -> Proposal {
        let (root, device) = proposer();
        sign_proposal(round, valid_round, header, body, &root.did(), &device)
    }

    #[test]
    fn a_proposal_round_trips_and_preserves_the_block_hash() {
        let original = signed(0, -1, header(), body());
        let hash_before = original.header.hash();
        let body_hash_before = original.body.hash();

        let bytes = ConsensusMessage::Proposal(original.clone()).to_wire_bytes();
        let decoded = ConsensusMessage::from_wire_bytes(&bytes).unwrap();
        let ConsensusMessage::Proposal(p) = decoded else {
            panic!("expected a proposal");
        };
        assert_eq!(p, original);
        // The whole point: the reconstructed block hashes identically, so a
        // vote cast on the decoded header is a vote on the original block.
        assert_eq!(p.header.hash(), hash_before);
        assert_eq!(p.body.hash(), body_hash_before);
    }

    #[test]
    fn an_empty_body_proposal_round_trips() {
        let original = signed(0, -1, header(), SettlementBlockBody::new(vec![]));
        let bytes = ConsensusMessage::Proposal(original.clone()).to_wire_bytes();
        let ConsensusMessage::Proposal(p) = ConsensusMessage::from_wire_bytes(&bytes).unwrap()
        else {
            panic!("expected a proposal");
        };
        assert_eq!(p, original);
    }

    #[test]
    fn a_signed_proposal_verifies_and_a_tampered_one_does_not() {
        let (root, device) = proposer();
        let p = sign_proposal(2, -1, header(), body(), &root.did(), &device);
        // Genuine proposal verifies against the real KELs.
        verify_proposal(&p, &root.kel(), &device.kel()).unwrap();

        // Tamper with a signed field (the round) and it must fail.
        let mut tampered = p.clone();
        tampered.round = 3;
        assert!(verify_proposal(&tampered, &root.kel(), &device.kel()).is_err());

        // A different validator's KELs must not verify this proposal.
        let (other_root, other_device) = {
            let mut r = Controller::incept_single_from_seeds(&[40u8; 32], &[41u8; 32]).unwrap();
            let d = Controller::incept_device_single_from_seeds(&r.did(), &[42u8; 32], &[43u8; 32])
                .unwrap();
            r.delegate_device(&d.did(), Capabilities::primary())
                .unwrap();
            (r, d)
        };
        assert!(verify_proposal(&p, &other_root.kel(), &other_device.kel()).is_err());
    }

    #[test]
    fn a_vote_message_round_trips() {
        let mut root = Controller::incept_single_from_seeds(&[5u8; 32], &[6u8; 32]).unwrap();
        let device =
            Controller::incept_device_single_from_seeds(&root.did(), &[7u8; 32], &[8u8; 32])
                .unwrap();
        root.delegate_device(&device.did(), Capabilities::primary())
            .unwrap();
        let vote = sign_vote(VoteKind::Precommit, 7, 0, [4u8; 32], &root.did(), &device);

        let bytes = ConsensusMessage::Vote(vote.clone()).to_wire_bytes();
        let ConsensusMessage::Vote(v) = ConsensusMessage::from_wire_bytes(&bytes).unwrap() else {
            panic!("expected a vote");
        };
        assert_eq!(v, vote);
    }

    #[test]
    fn truncation_at_every_length_is_rejected_never_panics() {
        let bytes = ConsensusMessage::Proposal(signed(0, -1, header(), body())).to_wire_bytes();
        for cut in 0..bytes.len() {
            assert!(ConsensusMessage::from_wire_bytes(&bytes[..cut]).is_err());
        }
    }

    #[test]
    fn trailing_bytes_are_rejected() {
        let mut bytes =
            ConsensusMessage::Proposal(signed(0, -1, header(), SettlementBlockBody::new(vec![])))
                .to_wire_bytes();
        bytes.push(0xFF);
        assert_eq!(
            ConsensusMessage::from_wire_bytes(&bytes).unwrap_err(),
            ConsensusError::Malformed
        );
    }

    #[test]
    fn an_absurd_claim_count_is_rejected_on_the_bound() {
        let mut bytes =
            ConsensusMessage::Proposal(signed(0, -1, header(), SettlementBlockBody::new(vec![])))
                .to_wire_bytes();
        // Layout after the (empty) body is the trailing signature block: a
        // single-key device signs with exactly one IndexedSig, so the tail is
        // count(4) + index(4) + suite(1) + sig(64) = 73 bytes, and the empty
        // body's own u32 claim-count sits in the 4 bytes just before it.
        let sig_tail = 4 + 4 + 1 + SignatureSuite::Ed25519.signature_len();
        let count_pos = bytes.len() - sig_tail - 4;
        bytes[count_pos..count_pos + 4].copy_from_slice(&u32::MAX.to_be_bytes());
        assert_eq!(
            ConsensusMessage::from_wire_bytes(&bytes).unwrap_err(),
            ConsensusError::TooLarge
        );
    }
}
