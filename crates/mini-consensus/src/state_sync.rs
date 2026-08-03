//! Snapshot-aware state-sync request and response framing.
//!
//! The response either carries a contiguous finalized-block suffix or one
//! independently verifiable snapshot followed by a contiguous bounded suffix.
//! A server can also report that its retained checkpoint cannot satisfy the
//! request. No response grants authority to the serving peer.

use crate::catchup::FinalizedBlock;
use crate::error::{ConsensusError, Result};
use crate::snapshot::ConsensusSnapshot;
use crate::wire::Reader;

/// Maximum finalized blocks in one state-sync response.
pub const MAX_STATE_SYNC_BLOCKS: usize = 256;

const DOMAIN: &[u8] = b"mini-consensus/state-sync/v1";
const TAG_WRONG_NETWORK: u8 = 0;
const TAG_UNAVAILABLE: u8 = 1;
const TAG_BLOCKS: u8 = 2;
const TAG_SNAPSHOT: u8 = 3;

/// Ask for canonical state strictly after `from_height` on one exact network.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StateSyncRequest {
    pub network_id: [u8; 32],
    pub from_height: u64,
}

impl StateSyncRequest {
    pub fn to_wire_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(DOMAIN.len() + 40);
        out.extend_from_slice(DOMAIN);
        out.extend_from_slice(&self.network_id);
        out.extend_from_slice(&self.from_height.to_be_bytes());
        out
    }

    pub fn from_wire_bytes(bytes: &[u8]) -> Result<Self> {
        let mut reader = Reader::new(bytes);
        if reader.take(DOMAIN.len())? != DOMAIN {
            return Err(ConsensusError::Malformed);
        }
        let mut network_id = [0; 32];
        network_id.copy_from_slice(reader.take(32)?);
        let from_height = reader.u64()?;
        if !reader.finished() {
            return Err(ConsensusError::Malformed);
        }
        Ok(Self {
            network_id,
            from_height,
        })
    }
}

/// What a peer can supply for one bounded state-sync round.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StateSyncPayload {
    /// The request names a different settlement/consensus network.
    WrongNetwork,
    /// The peer no longer retains a checkpoint/suffix covering this request.
    Unavailable {
        earliest_height: u64,
        tip_height: u64,
    },
    /// A contiguous run beginning at `request.from_height + 1`.
    Blocks(Vec<FinalizedBlock>),
    /// A newer authenticated checkpoint plus blocks immediately after it.
    /// Boxed so the control/blocks-only variants do not pay the full in-memory
    /// size of the state-bearing snapshot on every response value.
    Snapshot {
        snapshot: Box<ConsensusSnapshot>,
        blocks: Vec<FinalizedBlock>,
    },
}

/// A response is explicitly bound to the network it describes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateSyncResponse {
    pub network_id: [u8; 32],
    pub payload: StateSyncPayload,
}

impl StateSyncResponse {
    pub fn wrong_network(expected_network_id: [u8; 32]) -> Self {
        Self {
            network_id: expected_network_id,
            payload: StateSyncPayload::WrongNetwork,
        }
    }

    pub fn unavailable(network_id: [u8; 32], earliest_height: u64, tip_height: u64) -> Self {
        Self {
            network_id,
            payload: StateSyncPayload::Unavailable {
                earliest_height,
                tip_height,
            },
        }
    }

    pub fn blocks(network_id: [u8; 32], blocks: Vec<FinalizedBlock>) -> Self {
        Self {
            network_id,
            payload: StateSyncPayload::Blocks(blocks),
        }
    }

    pub fn snapshot(
        network_id: [u8; 32],
        snapshot: ConsensusSnapshot,
        blocks: Vec<FinalizedBlock>,
    ) -> Self {
        Self {
            network_id,
            payload: StateSyncPayload::Snapshot {
                snapshot: Box::new(snapshot),
                blocks,
            },
        }
    }

    pub fn block_count(&self) -> usize {
        match &self.payload {
            StateSyncPayload::Blocks(blocks) | StateSyncPayload::Snapshot { blocks, .. } => {
                blocks.len()
            }
            _ => 0,
        }
    }

    pub fn target_height(&self) -> Option<u64> {
        match &self.payload {
            StateSyncPayload::Blocks(blocks) => blocks.last().map(|block| block.header.height),
            StateSyncPayload::Snapshot { snapshot, blocks } => Some(
                blocks
                    .last()
                    .map_or(snapshot.height(), |block| block.header.height),
            ),
            StateSyncPayload::Unavailable { tip_height, .. } => Some(*tip_height),
            StateSyncPayload::WrongNetwork => None,
        }
    }

    /// Exact encoded length before adding any finalized suffix blocks. This
    /// serializes a snapshot once for its length but never clones its complete
    /// execution state merely to probe candidate page sizes.
    pub(crate) fn base_wire_len(snapshot: Option<&ConsensusSnapshot>) -> Result<usize> {
        // domain + network id + tag + block count
        let mut length = DOMAIN
            .len()
            .checked_add(32 + 1 + 4)
            .ok_or(ConsensusError::TooLarge)?;
        if let Some(snapshot) = snapshot {
            let snapshot_len = snapshot.to_wire_bytes()?.len();
            length = length
                .checked_add(4)
                .and_then(|value| value.checked_add(snapshot_len))
                .ok_or(ConsensusError::TooLarge)?;
        }
        if length > mini_bearer::MAX_CHANNEL_PLAINTEXT_BYTES {
            return Err(ConsensusError::TooLarge);
        }
        Ok(length)
    }

    pub fn to_wire_bytes(&self) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        out.extend_from_slice(DOMAIN);
        out.extend_from_slice(&self.network_id);
        match &self.payload {
            StateSyncPayload::WrongNetwork => out.push(TAG_WRONG_NETWORK),
            StateSyncPayload::Unavailable {
                earliest_height,
                tip_height,
            } => {
                out.push(TAG_UNAVAILABLE);
                out.extend_from_slice(&earliest_height.to_be_bytes());
                out.extend_from_slice(&tip_height.to_be_bytes());
            }
            StateSyncPayload::Blocks(blocks) => {
                out.push(TAG_BLOCKS);
                encode_blocks(&mut out, blocks)?;
            }
            StateSyncPayload::Snapshot { snapshot, blocks } => {
                out.push(TAG_SNAPSHOT);
                let snapshot = snapshot.to_wire_bytes()?;
                put_bytes(&mut out, &snapshot)?;
                encode_blocks(&mut out, blocks)?;
            }
        }
        if out.len() > mini_bearer::MAX_CHANNEL_PLAINTEXT_BYTES {
            return Err(ConsensusError::TooLarge);
        }
        Ok(out)
    }

    pub fn from_wire_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > mini_bearer::MAX_CHANNEL_PLAINTEXT_BYTES {
            return Err(ConsensusError::TooLarge);
        }
        let mut reader = Reader::new(bytes);
        if reader.take(DOMAIN.len())? != DOMAIN {
            return Err(ConsensusError::Malformed);
        }
        let mut network_id = [0; 32];
        network_id.copy_from_slice(reader.take(32)?);
        let payload = match reader.u8()? {
            TAG_WRONG_NETWORK => StateSyncPayload::WrongNetwork,
            TAG_UNAVAILABLE => StateSyncPayload::Unavailable {
                earliest_height: reader.u64()?,
                tip_height: reader.u64()?,
            },
            TAG_BLOCKS => StateSyncPayload::Blocks(decode_blocks(&mut reader)?),
            TAG_SNAPSHOT => {
                let snapshot = ConsensusSnapshot::from_wire_bytes(
                    reader.bytes(mini_bearer::MAX_CHANNEL_PLAINTEXT_BYTES)?,
                )?;
                let blocks = decode_blocks(&mut reader)?;
                StateSyncPayload::Snapshot {
                    snapshot: Box::new(snapshot),
                    blocks,
                }
            }
            _ => return Err(ConsensusError::Malformed),
        };
        if !reader.finished() {
            return Err(ConsensusError::Malformed);
        }
        let response = Self {
            network_id,
            payload,
        };
        if response.to_wire_bytes()?.as_slice() != bytes {
            return Err(ConsensusError::Malformed);
        }
        Ok(response)
    }
}

fn encode_blocks(out: &mut Vec<u8>, blocks: &[FinalizedBlock]) -> Result<()> {
    if blocks.len() > MAX_STATE_SYNC_BLOCKS {
        return Err(ConsensusError::TooLarge);
    }
    out.extend_from_slice(&(blocks.len() as u32).to_be_bytes());
    for block in blocks {
        let bytes = block.to_wire_bytes()?;
        put_bytes(out, &bytes)?;
    }
    Ok(())
}

fn decode_blocks(reader: &mut Reader<'_>) -> Result<Vec<FinalizedBlock>> {
    let count = reader.u32()? as usize;
    if count > MAX_STATE_SYNC_BLOCKS {
        return Err(ConsensusError::TooLarge);
    }
    let mut blocks = Vec::with_capacity(count.min(32));
    for _ in 0..count {
        blocks.push(FinalizedBlock::from_wire_bytes(
            reader.bytes(crate::MAX_MESSAGE_BYTES)?,
        )?);
    }
    Ok(blocks)
}

fn put_bytes(out: &mut Vec<u8>, bytes: &[u8]) -> Result<()> {
    let len = u32::try_from(bytes.len()).map_err(|_| ConsensusError::TooLarge)?;
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(bytes);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_round_trips_and_rejects_trailing_bytes() {
        let request = StateSyncRequest {
            network_id: [4; 32],
            from_height: 99,
        };
        let bytes = request.to_wire_bytes();
        assert_eq!(StateSyncRequest::from_wire_bytes(&bytes).unwrap(), request);
        let mut trailing = bytes;
        trailing.push(0);
        assert!(StateSyncRequest::from_wire_bytes(&trailing).is_err());
    }

    #[test]
    fn computed_empty_blocks_base_length_matches_the_real_wire() {
        let response = StateSyncResponse::blocks([7; 32], Vec::new());
        assert_eq!(
            StateSyncResponse::base_wire_len(None).unwrap(),
            response.to_wire_bytes().unwrap().len()
        );
    }

    #[test]
    fn control_responses_round_trip() {
        for response in [
            StateSyncResponse::wrong_network([1; 32]),
            StateSyncResponse::unavailable([2; 32], 10, 30),
            StateSyncResponse::blocks([3; 32], Vec::new()),
        ] {
            let bytes = response.to_wire_bytes().unwrap();
            assert_eq!(
                StateSyncResponse::from_wire_bytes(&bytes).unwrap(),
                response
            );
        }
    }
}
