//! Fetching missing V3 claim bytes from a peer (research PR #429, T14 follow-up,
//! D-0553).
//!
//! A validator cannot vote for a block whose claim bytes it does not hold (a
//! missing claim is a hard failure, see `ClaimVerifier`). Until now the host had
//! to get those bytes by some unspecified means. This module is the **message
//! layer** of a fetch protocol: pure request/response types, strict codecs, a
//! server that answers from a [`ClaimEvidencePoolV3`], and an ingest step that
//! feeds a response back into a pool. It opens no sockets and runs no tasks; a
//! host carries the two messages over whatever authenticated channel it has.
//!
//! Threat model, and what each rule is for:
//!
//! - **Unsolicited or substituted bytes.** [`ingest`] accepts a claim only if its
//!   `claim_id` is one the caller asked for. The id covers every byte of the
//!   claim, so a peer can neither swap in different bytes under a requested id
//!   nor use a response to push arbitrary data into the pool.
//! - **Amplification and memory.** A request is at most [`MAX_REQUEST_IDS`] ids
//!   (about 2 KiB); a response is at most [`MAX_RESPONSE_CLAIMS`] claims and
//!   [`MAX_RESPONSE_BYTES`] bytes in total, enforced by the codec before any claim
//!   is parsed. A peer that sends more is rejected whole.
//! - **Hostile bytes.** Every claim goes through `ClaimEvidencePoolV3::insert`,
//!   the same strict, canonical-or-refused admission as locally submitted
//!   evidence. Admission is structural; verification still happens later.
//! - **Withholding.** A peer can answer with fewer claims than asked. [`ingest`]
//!   reports what is still missing so the host can ask another peer; nothing here
//!   treats silence as proof of absence.
//! - **Request privacy.** Asking for an id tells the peer you care about that
//!   claim. Claim bytes are public ledger data, so this leaks interest, not
//!   content; hiding interest needs private information retrieval, which is out
//!   of scope and not built.
//!
//! Not built: the transport, peer selection, rate limiting per peer, and any
//! authentication of the peer (a wrong answer is harmless here, but an
//! unauthenticated flood is the host's problem). Unaudited.

use std::collections::BTreeSet;

use crate::v3::ClaimEvidencePoolV3;

/// Most claim ids in one request.
pub const MAX_REQUEST_IDS: usize = 64;
/// Most claims in one response.
pub const MAX_RESPONSE_CLAIMS: usize = 64;
/// Most claim bytes in one response (the sum over all claims).
pub const MAX_RESPONSE_BYTES: usize = 4 * 1024 * 1024;

const REQUEST_MAGIC: &[u8] = b"mininet/claim-fetch/v3/request\n";
const RESPONSE_MAGIC: &[u8] = b"mininet/claim-fetch/v3/response\n";

/// Why a message was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExchangeError {
    /// Wrong magic, truncated, trailing bytes, or a length field out of range.
    Malformed,
    /// More ids or claims than the limits allow.
    TooMany,
    /// The response is larger than [`MAX_RESPONSE_BYTES`].
    TooLarge,
    /// The same id appears twice in a request.
    DuplicateId,
}

/// A request for claims by `claim_id`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimRequest {
    ids: Vec<[u8; 32]>,
}

impl ClaimRequest {
    /// Build a request. Duplicates are removed; more than [`MAX_REQUEST_IDS`]
    /// distinct ids is refused (ask in several requests).
    pub fn new(ids: impl IntoIterator<Item = [u8; 32]>) -> Result<Self, ExchangeError> {
        let set: BTreeSet<[u8; 32]> = ids.into_iter().collect();
        if set.len() > MAX_REQUEST_IDS {
            return Err(ExchangeError::TooMany);
        }
        Ok(Self {
            ids: set.into_iter().collect(),
        })
    }

    /// The requested ids, sorted and distinct.
    pub fn ids(&self) -> &[[u8; 32]] {
        &self.ids
    }

    /// Canonical bytes.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(REQUEST_MAGIC.len() + 2 + self.ids.len() * 32);
        out.extend_from_slice(REQUEST_MAGIC);
        out.extend_from_slice(&(self.ids.len() as u16).to_be_bytes());
        for id in &self.ids {
            out.extend_from_slice(id);
        }
        out
    }

    /// Strict decode: exact length, sorted, distinct.
    pub fn decode(bytes: &[u8]) -> Result<Self, ExchangeError> {
        let rest = bytes
            .strip_prefix(REQUEST_MAGIC)
            .ok_or(ExchangeError::Malformed)?;
        let (count, rest) = rest.split_at_checked(2).ok_or(ExchangeError::Malformed)?;
        let count = u16::from_be_bytes([count[0], count[1]]) as usize;
        if count > MAX_REQUEST_IDS {
            return Err(ExchangeError::TooMany);
        }
        if rest.len() != count * 32 {
            return Err(ExchangeError::Malformed);
        }
        let mut ids = Vec::with_capacity(count);
        for chunk in rest.chunks_exact(32) {
            let mut id = [0u8; 32];
            id.copy_from_slice(chunk);
            if ids.last().is_some_and(|last| *last >= id) {
                return Err(ExchangeError::DuplicateId);
            }
            ids.push(id);
        }
        Ok(Self { ids })
    }
}

/// Claim bytes offered in answer to a request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimResponse {
    claims: Vec<Vec<u8>>,
}

impl ClaimResponse {
    /// The claim byte strings.
    pub fn claims(&self) -> &[Vec<u8>] {
        &self.claims
    }

    /// Canonical bytes.
    pub fn encode(&self) -> Vec<u8> {
        let total: usize = self.claims.iter().map(|c| c.len() + 4).sum();
        let mut out = Vec::with_capacity(RESPONSE_MAGIC.len() + 2 + total);
        out.extend_from_slice(RESPONSE_MAGIC);
        out.extend_from_slice(&(self.claims.len() as u16).to_be_bytes());
        for claim in &self.claims {
            out.extend_from_slice(&(claim.len() as u32).to_be_bytes());
            out.extend_from_slice(claim);
        }
        out
    }

    /// Strict decode. The count and total size are checked before any claim is
    /// copied out, so an oversized message costs nothing.
    pub fn decode(bytes: &[u8]) -> Result<Self, ExchangeError> {
        if bytes.len() > MAX_RESPONSE_BYTES + MAX_RESPONSE_CLAIMS * 4 + 64 {
            return Err(ExchangeError::TooLarge);
        }
        let rest = bytes
            .strip_prefix(RESPONSE_MAGIC)
            .ok_or(ExchangeError::Malformed)?;
        let (count, mut rest) = rest.split_at_checked(2).ok_or(ExchangeError::Malformed)?;
        let count = u16::from_be_bytes([count[0], count[1]]) as usize;
        if count > MAX_RESPONSE_CLAIMS {
            return Err(ExchangeError::TooMany);
        }
        let mut claims = Vec::with_capacity(count);
        let mut total = 0usize;
        for _ in 0..count {
            let (len, tail) = rest.split_at_checked(4).ok_or(ExchangeError::Malformed)?;
            let len = u32::from_be_bytes([len[0], len[1], len[2], len[3]]) as usize;
            total = total.checked_add(len).ok_or(ExchangeError::TooLarge)?;
            if total > MAX_RESPONSE_BYTES {
                return Err(ExchangeError::TooLarge);
            }
            let (claim, tail) = tail.split_at_checked(len).ok_or(ExchangeError::Malformed)?;
            claims.push(claim.to_vec());
            rest = tail;
        }
        if !rest.is_empty() {
            return Err(ExchangeError::Malformed);
        }
        Ok(Self { claims })
    }
}

/// Answer a request from `pool`: every requested claim it holds, stopping before
/// the byte limit. Unknown ids are simply omitted; the pool is not changed.
pub fn serve(pool: &ClaimEvidencePoolV3, request: &ClaimRequest) -> ClaimResponse {
    let mut claims = Vec::new();
    let mut total = 0usize;
    for id in request.ids() {
        if let Some(bytes) = pool.get(id) {
            if total + bytes.len() > MAX_RESPONSE_BYTES || claims.len() >= MAX_RESPONSE_CLAIMS {
                break;
            }
            total += bytes.len();
            claims.push(bytes);
        }
    }
    ClaimResponse { claims }
}

/// What [`ingest`] did with a response.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct IngestReport {
    /// Requested claims now in the pool (including ones already there).
    pub stored: Vec<[u8; 32]>,
    /// Requested ids the response did not deliver: ask someone else.
    pub still_missing: Vec<[u8; 32]>,
    /// Claims offered that were not asked for; ignored, never stored.
    pub unsolicited: usize,
    /// Requested claims the pool refused (malformed, full, over a limit).
    pub refused: usize,
}

/// Feed a response into `pool`, keeping only what `request` asked for.
pub fn ingest(
    pool: &ClaimEvidencePoolV3,
    request: &ClaimRequest,
    response: &ClaimResponse,
) -> IngestReport {
    let wanted: BTreeSet<[u8; 32]> = request.ids().iter().copied().collect();
    let mut report = IngestReport::default();
    // Requested claims the pool already holds need no fetching.
    let mut delivered: BTreeSet<[u8; 32]> = wanted
        .iter()
        .copied()
        .filter(|id| pool.get(id).is_some())
        .collect();
    for bytes in response.claims() {
        // Identify the claim without storing it: only solicited ids may enter.
        let Some(id) = ClaimEvidencePoolV3::claim_id_of(bytes) else {
            report.refused += 1;
            continue;
        };
        if !wanted.contains(&id) {
            report.unsolicited += 1;
            continue;
        }
        match pool.insert(bytes.clone()) {
            Ok(stored) if stored == id => {
                delivered.insert(id);
            }
            _ => report.refused += 1,
        }
    }
    report.stored = delivered.iter().copied().collect();
    report.still_missing = wanted.difference(&delivered).copied().collect();
    report
}
