//! State sync / catch-up: a node that missed one or more finalized heights
//! (was offline, joined late, or fell behind a live cluster) can pull
//! already-finalized blocks from any peer and apply them directly, instead
//! of needing to re-run Tendermint rounds for heights that already decided.
//! Closes the gap this workspace has named repeatedly (`docs/STATUS.md` §1,
//! `docs/design/networked-consensus.md`): "no state-sync for a node that
//! missed a whole height."
//!
//! ## Trust model — catch-up is never a shortcut around finality
//!
//! A [`CatchupResponse`] is exactly as untrusted as a gossiped
//! [`crate::wire::ConsensusMessage`]: decoding it makes no trust decision.
//! [`crate::node::ConsensusNode::catch_up`] applies each block through the
//! *identical* [`mini_execution::LedgerChain::apply_finalized_block`] call
//! live consensus uses to commit a height, which independently re-verifies
//! the block's [`QuorumCertificate`] against the current validator set and
//! their KELs. A peer cannot hand a catching-up node a shortcut to a state
//! no real quorum ever decided — a forged or incomplete certificate is
//! rejected exactly as it would be during live consensus, and catch-up
//! simply fails at that block rather than adopting it.
//!
//! ## What this does not do (honest limits, first slice)
//!
//! - **No peer selection or retry policy.** A caller supplies one peer's
//!   [`CatchupResponse`]; choosing which peer to ask, retrying a bad one, or
//!   querying several for agreement is a host concern, not this module's.
//! - **No unbounded history.** A serving node only answers from whatever it
//!   still holds in memory (see [`crate::node::ConsensusNode::history_since`]);
//!   there is no persistence or pruning policy yet — a first slice, the same
//!   honest-limit shape `mini-net`'s `RoutingTable`/`GossipRouter` document
//!   for their own first-slice bounds.
//! - **No partial-batch application.** [`CatchupResponse::from_wire_bytes`]
//!   bounds the count before allocating ([`MAX_CATCHUP_BLOCKS`]), but a
//!   response that fails partway through `catch_up` leaves the node at
//!   whatever height it reached before the failing block — never silently
//!   further, never rolled back.

use mini_chain::{BlockHeader, QuorumCertificate, Vote, MAX_VOTES_PER_CERTIFICATE};
use mini_execution::SettlementBlockBody;

use crate::error::{ConsensusError, Result};
use crate::wire::{decode_body, decode_header, encode_body, encode_header, put_bytes, Reader};

/// Hard cap on blocks in one [`CatchupResponse`] — bounded so a hostile or
/// merely enormous response cannot force unbounded allocation before a
/// single block is verified.
pub const MAX_CATCHUP_BLOCKS: usize = 1024;

const DOMAIN: &[u8] = b"mini-consensus/catchup/v1";
const TAG_REQUEST: u8 = 0;
const TAG_RESPONSE: u8 = 1;

/// One already-finalized block: everything
/// [`mini_execution::LedgerChain::apply_finalized_block`] needs to
/// independently re-verify and apply it. Catch-up trusts nothing beyond
/// what live consensus already trusts — this is not a lighter-weight
/// finality check, it is the same one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinalizedBlock {
    /// The finalized block's header.
    pub header: BlockHeader,
    /// The ordered claim body the header's `state_root` commits to.
    pub body: SettlementBlockBody,
    /// The quorum certificate proving this block finalized.
    pub qc: QuorumCertificate,
}

/// "Send me every block after `from_height`."
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CatchupRequest {
    /// The requester's current finalized height — the peer should answer
    /// with everything after it.
    pub from_height: u64,
}

impl CatchupRequest {
    /// Canonical wire bytes for this request.
    pub fn to_wire_bytes(&self) -> Vec<u8> {
        let mut w = Vec::with_capacity(DOMAIN.len() + 9);
        w.extend_from_slice(DOMAIN);
        w.push(TAG_REQUEST);
        w.extend_from_slice(&self.from_height.to_be_bytes());
        w
    }

    /// Decode a request produced by [`CatchupRequest::to_wire_bytes`].
    pub fn from_wire_bytes(bytes: &[u8]) -> Result<Self> {
        let mut r = Reader::new(bytes);
        if r.take(DOMAIN.len())? != DOMAIN {
            return Err(ConsensusError::Malformed);
        }
        if r.u8()? != TAG_REQUEST {
            return Err(ConsensusError::Malformed);
        }
        let from_height = r.u64()?;
        if !r.finished() {
            return Err(ConsensusError::Malformed);
        }
        Ok(CatchupRequest { from_height })
    }
}

/// A bounded run of already-finalized blocks, in ascending height order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatchupResponse {
    /// The finalized blocks being handed to the catching-up peer.
    pub blocks: Vec<FinalizedBlock>,
}

impl CatchupResponse {
    /// Canonical wire bytes for this response.
    pub fn to_wire_bytes(&self) -> Vec<u8> {
        let mut w = Vec::new();
        w.extend_from_slice(DOMAIN);
        w.push(TAG_RESPONSE);
        w.extend_from_slice(&(self.blocks.len() as u32).to_be_bytes());
        for block in &self.blocks {
            encode_header(&mut w, &block.header);
            encode_body(&mut w, &block.body);
            encode_qc(&mut w, &block.qc);
        }
        w
    }

    /// Decode a response produced by [`CatchupResponse::to_wire_bytes`].
    /// Structural only, exactly like [`crate::wire::ConsensusMessage::from_wire_bytes`]:
    /// a well-framed but hostile response decodes fine and is rejected later,
    /// on its merits, when [`crate::node::ConsensusNode::catch_up`] tries to
    /// apply it.
    pub fn from_wire_bytes(bytes: &[u8]) -> Result<Self> {
        let mut r = Reader::new(bytes);
        if r.take(DOMAIN.len())? != DOMAIN {
            return Err(ConsensusError::Malformed);
        }
        if r.u8()? != TAG_RESPONSE {
            return Err(ConsensusError::Malformed);
        }
        let count = r.u32()? as usize;
        if count > MAX_CATCHUP_BLOCKS {
            return Err(ConsensusError::TooLarge);
        }
        let mut blocks = Vec::with_capacity(count.min(64));
        for _ in 0..count {
            let header = decode_header(&mut r)?;
            let body = decode_body(&mut r)?;
            let qc = decode_qc(&mut r)?;
            blocks.push(FinalizedBlock { header, body, qc });
        }
        if !r.finished() {
            return Err(ConsensusError::Malformed);
        }
        Ok(CatchupResponse { blocks })
    }
}

fn encode_qc(w: &mut Vec<u8>, qc: &QuorumCertificate) {
    w.extend_from_slice(&qc.height.to_be_bytes());
    w.extend_from_slice(&qc.round.to_be_bytes());
    w.extend_from_slice(&qc.block_hash);
    w.extend_from_slice(&(qc.votes.len() as u32).to_be_bytes());
    for vote in &qc.votes {
        put_bytes(w, &vote.to_wire_bytes());
    }
}

fn decode_qc(r: &mut Reader<'_>) -> Result<QuorumCertificate> {
    let height = r.u64()?;
    let round = r.u32()?;
    let mut block_hash = [0u8; 32];
    block_hash.copy_from_slice(r.take(32)?);
    let vote_count = r.u32()? as usize;
    if vote_count > MAX_VOTES_PER_CERTIFICATE {
        return Err(ConsensusError::TooLarge);
    }
    let mut votes = Vec::with_capacity(vote_count.min(64));
    for _ in 0..vote_count {
        let vote_bytes = r.bytes(crate::MAX_MESSAGE_BYTES)?;
        let vote = Vote::from_wire_bytes(vote_bytes).map_err(|_| ConsensusError::Malformed)?;
        votes.push(vote);
    }
    Ok(QuorumCertificate {
        height,
        round,
        block_hash,
        votes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use did_mini::{Capabilities, Controller};
    use mini_chain::{sign_vote, VoteKind};
    use mini_settlement::sign_claim;

    fn header(height: u64) -> BlockHeader {
        let root = Controller::incept_single_from_seeds(&[1u8; 32], &[2u8; 32]).unwrap();
        BlockHeader {
            height,
            prev_hash: [3u8; 32],
            state_root: [4u8; 32],
            timestamp_ms: height,
            proposer: root.did(),
        }
    }

    fn body() -> SettlementBlockBody {
        let payer = mini_crypto::SigningKey::from_seed(&[9u8; 32]);
        let claim = sign_claim(&payer, b"payee", 100, 0, 10_000, b"chain", 0).unwrap();
        SettlementBlockBody::new(vec![claim])
    }

    fn qc(height: u64) -> QuorumCertificate {
        let mut root = Controller::incept_single_from_seeds(&[20u8; 32], &[21u8; 32]).unwrap();
        let device =
            Controller::incept_device_single_from_seeds(&root.did(), &[22u8; 32], &[23u8; 32])
                .unwrap();
        root.delegate_device(&device.did(), Capabilities::primary())
            .unwrap();
        let vote = sign_vote(
            VoteKind::Precommit,
            height,
            0,
            [7u8; 32],
            &root.did(),
            &device,
        );
        QuorumCertificate {
            height,
            round: 0,
            block_hash: [7u8; 32],
            votes: vec![vote],
        }
    }

    #[test]
    fn a_request_round_trips_through_encode_decode() {
        let req = CatchupRequest { from_height: 42 };
        assert_eq!(
            CatchupRequest::from_wire_bytes(&req.to_wire_bytes()).unwrap(),
            req
        );
    }

    #[test]
    fn a_response_round_trips_through_encode_decode() {
        let response = CatchupResponse {
            blocks: vec![
                FinalizedBlock {
                    header: header(1),
                    body: body(),
                    qc: qc(1),
                },
                FinalizedBlock {
                    header: header(2),
                    body: body(),
                    qc: qc(2),
                },
            ],
        };
        let decoded = CatchupResponse::from_wire_bytes(&response.to_wire_bytes()).unwrap();
        assert_eq!(decoded, response);
    }

    #[test]
    fn an_empty_response_round_trips() {
        let response = CatchupResponse { blocks: Vec::new() };
        assert_eq!(
            CatchupResponse::from_wire_bytes(&response.to_wire_bytes()).unwrap(),
            response
        );
    }

    #[test]
    fn truncation_at_every_length_is_rejected_never_panics() {
        let response = CatchupResponse {
            blocks: vec![FinalizedBlock {
                header: header(1),
                body: body(),
                qc: qc(1),
            }],
        };
        let bytes = response.to_wire_bytes();
        for cut in 0..bytes.len() {
            assert!(CatchupResponse::from_wire_bytes(&bytes[..cut]).is_err());
        }
    }

    #[test]
    fn trailing_bytes_are_rejected() {
        let mut bytes = CatchupResponse { blocks: Vec::new() }.to_wire_bytes();
        bytes.push(0xff);
        assert_eq!(
            CatchupResponse::from_wire_bytes(&bytes).unwrap_err(),
            ConsensusError::Malformed
        );
    }

    #[test]
    fn a_claimed_block_count_over_the_cap_is_rejected_before_allocating() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(DOMAIN);
        bytes.push(TAG_RESPONSE);
        bytes.extend_from_slice(&((MAX_CATCHUP_BLOCKS as u32) + 1).to_be_bytes());
        assert_eq!(
            CatchupResponse::from_wire_bytes(&bytes).unwrap_err(),
            ConsensusError::TooLarge
        );
    }

    #[test]
    fn a_claimed_vote_count_over_the_cap_is_rejected_before_allocating() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(DOMAIN);
        bytes.push(TAG_RESPONSE);
        bytes.extend_from_slice(&1u32.to_be_bytes()); // one block
        encode_header(&mut bytes, &header(1));
        encode_body(&mut bytes, &body());
        // Now a QC with an absurd vote count.
        bytes.extend_from_slice(&1u64.to_be_bytes()); // height
        bytes.extend_from_slice(&0u32.to_be_bytes()); // round
        bytes.extend_from_slice(&[7u8; 32]); // block_hash
        bytes.extend_from_slice(&((MAX_VOTES_PER_CERTIFICATE as u32) + 1).to_be_bytes());
        assert_eq!(
            CatchupResponse::from_wire_bytes(&bytes).unwrap_err(),
            ConsensusError::TooLarge
        );
    }
}
