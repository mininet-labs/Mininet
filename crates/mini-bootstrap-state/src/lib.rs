//! The one-way Forge-canonical / Go-Live state (issue #338, governance pause
//! `docs/governance/52_PRE_GO_LIVE_GOVERNANCE_PAUSE.md`, sections 2 and 10).
//!
//! The pause says the transition MUST be one-way and machine-readable: once
//! `go_live` is true the bootstrap exception and Founder canonical integration
//! authority cannot be restored by a Founder action. Prose cannot enforce that.
//! This crate is the part software can enforce.
//!
//! The state is **derived**, never stored as two editable booleans. It is the
//! replay of an append-only log of [`Transition`] records, each carrying the
//! digest of the previous record, so:
//!
//! - there is no API that sets a flag back to false, and no record kind that
//!   means "undo";
//! - [`replay`] accepts only the legal order: `ForgeCanonical`, then `GoLive`,
//!   each at most once, nothing after `GoLive`;
//! - altering, dropping, reordering or inserting a record breaks the hash chain
//!   and the whole log is refused;
//! - a record names evidence by digest only (the outage-proof report, the audit
//!   closure matrix, ...). It carries **no name, DID, pseudonym or other identity
//!   field**, matching the anonymous-only bootstrap rule.
//!
//! What this crate does **not** do, and why that matters:
//!
//! - It does not decide **who may append** a record. That is the canonical Forge
//!   process (exact-state review, governance attestations). A caller that can
//!   write the log file can write a *new* log; the one-way property holds for a
//!   log a host treats as canonical and append-only, for example one stored as
//!   signed Forge objects. Binding the log to that storage and its signatures is
//!   the host's job and is not built here.
//! - It does not check that the evidence digests are real or sufficient.
//! - It activates nothing: no caller in the workspace consults it yet, and
//!   `forge_canonical` is still false because the issue's outage proof is not
//!   complete. Unaudited.

#![forbid(unsafe_code)]

/// The two facts Go-Live needs, as a derived state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BootstrapState {
    forge_canonical: bool,
    go_live: bool,
}

impl BootstrapState {
    /// The pre-Go-Live starting state: both false.
    pub const fn pre_go_live() -> Self {
        Self {
            forge_canonical: false,
            go_live: false,
        }
    }

    /// Whether Forge is canonical.
    pub const fn forge_canonical(&self) -> bool {
        self.forge_canonical
    }

    /// Whether Go-Live has happened. Implies `forge_canonical`.
    pub const fn go_live(&self) -> bool {
        self.go_live
    }

    /// Whether the Pre-Go-Live governance-process pause is in force. It ends
    /// at Go-Live and never returns.
    pub const fn governance_pause_active(&self) -> bool {
        !self.go_live
    }

    /// Whether the bootstrap Founder/custodian integration path may still be
    /// used. It is true only before Go-Live; afterwards it is gone, not merely
    /// quiet.
    pub const fn founder_integration_enabled(&self) -> bool {
        !self.go_live
    }

    /// Fail with a clear error unless the Founder integration path is still
    /// enabled. Call this at every bootstrap-authority decision point.
    pub fn require_founder_integration(&self) -> Result<(), FounderIntegrationEnded> {
        if self.founder_integration_enabled() {
            Ok(())
        } else {
            Err(FounderIntegrationEnded)
        }
    }
}

/// Go-Live has happened; bootstrap Founder integration authority is over.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FounderIntegrationEnded;

impl core::fmt::Display for FounderIntegrationEnded {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("Go-Live has happened: the bootstrap Founder integration path is ended")
    }
}

/// What a record moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// `forge_canonical: false -> true`.
    ForgeCanonical,
    /// `go_live: false -> true`; needs Forge canonicality first.
    GoLive,
}

impl Kind {
    fn tag(self) -> u8 {
        match self {
            Kind::ForgeCanonical => 1,
            Kind::GoLive => 2,
        }
    }
    fn from_tag(tag: u8) -> Option<Self> {
        match tag {
            1 => Some(Kind::ForgeCanonical),
            2 => Some(Kind::GoLive),
            _ => None,
        }
    }
}

/// One link of the log. No identity fields by design.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Transition {
    kind: Kind,
    /// Digest of the evidence package the canonical process accepted (outage
    /// proof, closure matrix, ...). Never all zeros.
    evidence: [u8; 32],
    /// Digest of the previous record, or zeros for the first.
    prev: [u8; 32],
}

const DOMAIN: &[u8] = b"mininet/bootstrap-state/transition/v1";
/// Encoded size of one record.
pub const RECORD_BYTES: usize = 1 + 32 + 32;

impl Transition {
    /// The kind.
    pub fn kind(&self) -> Kind {
        self.kind
    }

    /// The evidence digest.
    pub fn evidence(&self) -> &[u8; 32] {
        &self.evidence
    }

    /// Canonical bytes.
    pub fn encode(&self) -> [u8; RECORD_BYTES] {
        let mut out = [0u8; RECORD_BYTES];
        out[0] = self.kind.tag();
        out[1..33].copy_from_slice(&self.evidence);
        out[33..].copy_from_slice(&self.prev);
        out
    }

    /// This record's digest, which the next record must carry as `prev`.
    pub fn digest(&self) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(DOMAIN);
        hasher.update(&self.encode());
        *hasher.finalize().as_bytes()
    }
}

/// Why a log or an append was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateError {
    /// Bytes are not a whole number of well-formed records.
    Malformed,
    /// The evidence digest is all zeros.
    EmptyEvidence,
    /// `GoLive` before `ForgeCanonical`.
    GoLiveBeforeForgeCanonical,
    /// The kind already happened; the transition is one-way and happens once.
    AlreadyDone,
    /// Anything after `GoLive`: nothing may follow it.
    AfterGoLive,
    /// A record's `prev` does not match the digest of the record before it.
    BrokenChain,
}

impl core::fmt::Display for StateError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            StateError::Malformed => "malformed bootstrap-state log",
            StateError::EmptyEvidence => "a transition needs a non-empty evidence digest",
            StateError::GoLiveBeforeForgeCanonical => "Go-Live requires Forge canonicality first",
            StateError::AlreadyDone => "that transition already happened and is one-way",
            StateError::AfterGoLive => "nothing may follow Go-Live",
            StateError::BrokenChain => "the transition log's hash chain is broken",
        })
    }
}

impl std::error::Error for StateError {}

/// An append-only log and the state derived from it.
#[derive(Debug, Clone, Default)]
pub struct Log {
    records: Vec<Transition>,
    state: BootstrapState,
}

impl Log {
    /// An empty log: pre-Go-Live.
    pub fn new() -> Self {
        Self::default()
    }

    /// The derived state.
    pub fn state(&self) -> BootstrapState {
        self.state
    }

    /// The records, oldest first.
    pub fn records(&self) -> &[Transition] {
        &self.records
    }

    /// Append the next legal transition. The only way to change the state, and
    /// it can only move a flag from false to true.
    pub fn append(&mut self, kind: Kind, evidence: [u8; 32]) -> Result<Transition, StateError> {
        if evidence == [0u8; 32] {
            return Err(StateError::EmptyEvidence);
        }
        if self.state.go_live {
            return Err(StateError::AfterGoLive);
        }
        match kind {
            Kind::ForgeCanonical if self.state.forge_canonical => {
                return Err(StateError::AlreadyDone)
            }
            Kind::GoLive if !self.state.forge_canonical => {
                return Err(StateError::GoLiveBeforeForgeCanonical)
            }
            _ => {}
        }
        let record = Transition {
            kind,
            evidence,
            prev: self.records.last().map_or([0u8; 32], Transition::digest),
        };
        match kind {
            Kind::ForgeCanonical => self.state.forge_canonical = true,
            Kind::GoLive => self.state.go_live = true,
        }
        self.records.push(record);
        Ok(record)
    }

    /// Canonical bytes: the concatenated records.
    pub fn encode(&self) -> Vec<u8> {
        self.records.iter().flat_map(|r| r.encode()).collect()
    }
}

/// Rebuild a log from bytes, refusing anything that is not a legal history. The
/// state is recomputed from the records, never read from the input.
pub fn replay(bytes: &[u8]) -> Result<Log, StateError> {
    if bytes.len() % RECORD_BYTES != 0 || bytes.len() > 2 * RECORD_BYTES {
        return Err(StateError::Malformed);
    }
    let mut log = Log::new();
    for chunk in bytes.chunks_exact(RECORD_BYTES) {
        let kind = Kind::from_tag(chunk[0]).ok_or(StateError::Malformed)?;
        let mut evidence = [0u8; 32];
        evidence.copy_from_slice(&chunk[1..33]);
        let mut prev = [0u8; 32];
        prev.copy_from_slice(&chunk[33..]);
        let expected_prev = log.records.last().map_or([0u8; 32], Transition::digest);
        // Legality first (order, once-only, after-go-live), then the chain.
        let appended = log.append(kind, evidence)?;
        if prev != expected_prev {
            return Err(StateError::BrokenChain);
        }
        debug_assert_eq!(appended.prev, expected_prev);
    }
    Ok(log)
}
