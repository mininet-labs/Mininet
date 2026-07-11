//! A persisted, append-only, hash-chained record of what [`crate::Installer`]
//! actually did -- durable evidence a caller (a `mini installer history`
//! CLI command, an auditor, this crate's own tests) can inspect after the
//! process exits, distinct from the type-state pipeline in `lib.rs`.
//!
//! **Boundary rule, load-bearing:** this log is evidence of what the
//! installer did. It is not permission to do anything. Every event here is
//! written *after* the type-state transition it describes already
//! succeeded -- nothing in this module, and nothing that reads this log,
//! may gate or trigger an installer action. `Installer::activate` still
//! requires a real [`crate::OwnerApproval`] to run; the `OwnerApproved`
//! event merely records, after the fact, that one was presented.
//!
//! No serde/JSON dependency exists anywhere in this workspace (by
//! established convention -- see `mini-forge`'s git-object framing,
//! `TreeEntry` encoding, KEL encoding); this log follows the same
//! hand-rolled, length-prefixed canonical binary encoding rather than
//! introducing one.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

use mini_crypto::HashAlgorithm;
use mini_objects::ObjectId;

/// A BLAKE3 digest, the same 32-byte shape every other content address in
/// this workspace uses.
pub type EventHash = [u8; 32];

/// One step this crate's type-state pipeline actually took (or, for
/// `Discovered`/`Verified`, a fact it received and is recording). Kept
/// `#[non_exhaustive]` for the same reason [`crate::InstallState`] is: a
/// future crate revision may add a step without that being a breaking
/// change for callers that already match exhaustively elsewhere.
///
/// `FailedWithNoPriorRelease` is not in the founder's original sketch for
/// this enum -- added because silently mapping "the very first activation
/// failed its health check, with nothing to restore" onto `RolledBack`
/// would misrepresent what happened (nothing was restored). It mirrors
/// [`crate::HealthCheckOutcome::FailedWithNoPriorRelease`]'s existing name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum InstallEventKind {
    Discovered,
    Verified,
    Staged,
    PreflightPassed,
    AwaitingOwnerApproval,
    OwnerApproved,
    Activating,
    HealthCheckStarted,
    HealthCheckPassed,
    HealthCheckFailed,
    RollbackStarted,
    RolledBack,
    PreviousReleaseActive,
    FailedWithNoPriorRelease,
}

impl InstallEventKind {
    fn tag(self) -> u8 {
        match self {
            InstallEventKind::Discovered => 0,
            InstallEventKind::Verified => 1,
            InstallEventKind::Staged => 2,
            InstallEventKind::PreflightPassed => 3,
            InstallEventKind::AwaitingOwnerApproval => 4,
            InstallEventKind::OwnerApproved => 5,
            InstallEventKind::Activating => 6,
            InstallEventKind::HealthCheckStarted => 7,
            InstallEventKind::HealthCheckPassed => 8,
            InstallEventKind::HealthCheckFailed => 9,
            InstallEventKind::RollbackStarted => 10,
            InstallEventKind::RolledBack => 11,
            InstallEventKind::PreviousReleaseActive => 12,
            InstallEventKind::FailedWithNoPriorRelease => 13,
        }
    }

    fn from_tag(tag: u8) -> Result<Self, InstallLogError> {
        Ok(match tag {
            0 => InstallEventKind::Discovered,
            1 => InstallEventKind::Verified,
            2 => InstallEventKind::Staged,
            3 => InstallEventKind::PreflightPassed,
            4 => InstallEventKind::AwaitingOwnerApproval,
            5 => InstallEventKind::OwnerApproved,
            6 => InstallEventKind::Activating,
            7 => InstallEventKind::HealthCheckStarted,
            8 => InstallEventKind::HealthCheckPassed,
            9 => InstallEventKind::HealthCheckFailed,
            10 => InstallEventKind::RollbackStarted,
            11 => InstallEventKind::RolledBack,
            12 => InstallEventKind::PreviousReleaseActive,
            13 => InstallEventKind::FailedWithNoPriorRelease,
            _ => return Err(InstallLogError::Corrupt("unknown event kind tag")),
        })
    }

    /// The kind(s) this transition may validly follow, within one
    /// `release_id`'s own subsequence of events -- `None` means "only
    /// valid as the first event for this release id". This is the actual
    /// state machine [`verify_install_event_log`] checks per-lineage.
    fn valid_predecessors(self) -> &'static [InstallEventKind] {
        use InstallEventKind::*;
        match self {
            Discovered => &[],
            Verified => &[Discovered],
            Staged => &[Verified],
            PreflightPassed => &[Staged],
            AwaitingOwnerApproval => &[PreflightPassed],
            OwnerApproved => &[AwaitingOwnerApproval],
            Activating => &[OwnerApproved],
            HealthCheckStarted => &[Activating],
            HealthCheckPassed => &[HealthCheckStarted],
            HealthCheckFailed => &[HealthCheckStarted],
            // Three legitimate predecessors, matching what `Installer`'s
            // real API actually allows: automatic rollback right after a
            // failed health check (`HealthCheckFailed`); a caller manually
            // rolling back a release that already passed its health check
            // (`HealthCheckPassed`); or a caller manually rolling back
            // without ever running a health check at all (`Activating`).
            // Only the first of these needs no `reason` field -- see the
            // "unexplained rollback" check in `verify_install_event_log`.
            RollbackStarted => &[HealthCheckFailed, HealthCheckPassed, Activating],
            RolledBack => &[RollbackStarted],
            // PreviousReleaseActive is checked separately, globally, against
            // the immediately preceding event across the whole log -- see
            // `verify_install_event_log`'s dedicated handling below, since
            // it names the *restored* release, not the failed one.
            PreviousReleaseActive => &[],
            FailedWithNoPriorRelease => &[HealthCheckFailed],
        }
    }
}

/// One entry in the persisted installer event log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallEvent {
    pub sequence: u64,
    pub previous_event_hash: Option<EventHash>,
    pub event_hash: EventHash,
    pub kind: InstallEventKind,
    pub release_id: ObjectId,
    pub artifact_digest: Option<[u8; 32]>,
    pub from_version: Option<String>,
    pub to_version: Option<String>,
    pub reason: Option<String>,
    pub timestamp_ms: u64,
}

#[allow(clippy::too_many_arguments)]
fn signable_bytes(
    sequence: u64,
    previous_event_hash: Option<&EventHash>,
    kind: InstallEventKind,
    release_id: &ObjectId,
    artifact_digest: Option<&[u8; 32]>,
    from_version: Option<&str>,
    to_version: Option<&str>,
    reason: Option<&str>,
    timestamp_ms: u64,
) -> Vec<u8> {
    let mut buf = Vec::new();
    buf.extend_from_slice(&sequence.to_be_bytes());
    push_opt_bytes(&mut buf, previous_event_hash.map(|h| h.as_slice()));
    buf.push(kind.tag());
    push_str(&mut buf, release_id.as_str());
    push_opt_bytes(&mut buf, artifact_digest.map(|d| d.as_slice()));
    push_opt_str(&mut buf, from_version);
    push_opt_str(&mut buf, to_version);
    push_opt_str(&mut buf, reason);
    buf.extend_from_slice(&timestamp_ms.to_be_bytes());
    buf
}

fn push_str(buf: &mut Vec<u8>, s: &str) {
    buf.extend_from_slice(&(s.len() as u32).to_be_bytes());
    buf.extend_from_slice(s.as_bytes());
}

fn push_opt_str(buf: &mut Vec<u8>, s: Option<&str>) {
    match s {
        Some(s) => {
            buf.push(1);
            push_str(buf, s);
        }
        None => buf.push(0),
    }
}

fn push_opt_bytes(buf: &mut Vec<u8>, b: Option<&[u8]>) {
    match b {
        Some(b) => {
            buf.push(1);
            buf.extend_from_slice(&(b.len() as u32).to_be_bytes());
            buf.extend_from_slice(b);
        }
        None => buf.push(0),
    }
}

fn take_str(b: &[u8], off: &mut usize) -> Result<String, InstallLogError> {
    if *off + 4 > b.len() {
        return Err(InstallLogError::Corrupt("truncated string length"));
    }
    let len = u32::from_be_bytes([b[*off], b[*off + 1], b[*off + 2], b[*off + 3]]) as usize;
    *off += 4;
    if *off + len > b.len() {
        return Err(InstallLogError::Corrupt("truncated string body"));
    }
    let s = String::from_utf8(b[*off..*off + len].to_vec())
        .map_err(|_| InstallLogError::Corrupt("non-utf8 string field"))?;
    *off += len;
    Ok(s)
}

fn take_opt_str(b: &[u8], off: &mut usize) -> Result<Option<String>, InstallLogError> {
    if *off >= b.len() {
        return Err(InstallLogError::Corrupt("truncated optional-string flag"));
    }
    let present = b[*off];
    *off += 1;
    match present {
        0 => Ok(None),
        1 => Ok(Some(take_str(b, off)?)),
        _ => Err(InstallLogError::Corrupt("bad optional-string flag")),
    }
}

fn take_opt_bytes32(b: &[u8], off: &mut usize) -> Result<Option<[u8; 32]>, InstallLogError> {
    if *off >= b.len() {
        return Err(InstallLogError::Corrupt("truncated optional-digest flag"));
    }
    let present = b[*off];
    *off += 1;
    match present {
        0 => Ok(None),
        1 => {
            if *off + 4 > b.len() {
                return Err(InstallLogError::Corrupt("truncated digest length"));
            }
            let len = u32::from_be_bytes([b[*off], b[*off + 1], b[*off + 2], b[*off + 3]]) as usize;
            *off += 4;
            if len != 32 || *off + 32 > b.len() {
                return Err(InstallLogError::Corrupt("bad digest length"));
            }
            let mut out = [0u8; 32];
            out.copy_from_slice(&b[*off..*off + 32]);
            *off += 32;
            Ok(Some(out))
        }
        _ => Err(InstallLogError::Corrupt("bad optional-digest flag")),
    }
}

impl InstallEvent {
    /// Construct and hash a new event chained onto `previous_event_hash`.
    /// `sequence` and `previous_event_hash` are the caller's job to derive
    /// correctly from the log so far (see `crate::Installer`'s internal
    /// writer) -- this constructor only computes the hash over the given
    /// fields, it does not itself read or write the log file.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        sequence: u64,
        previous_event_hash: Option<EventHash>,
        kind: InstallEventKind,
        release_id: ObjectId,
        artifact_digest: Option<[u8; 32]>,
        from_version: Option<String>,
        to_version: Option<String>,
        reason: Option<String>,
        timestamp_ms: u64,
    ) -> Self {
        let bytes = signable_bytes(
            sequence,
            previous_event_hash.as_ref(),
            kind,
            &release_id,
            artifact_digest.as_ref(),
            from_version.as_deref(),
            to_version.as_deref(),
            reason.as_deref(),
            timestamp_ms,
        );
        let event_hash = HashAlgorithm::Blake3.digest(&bytes);
        InstallEvent {
            sequence,
            previous_event_hash,
            event_hash,
            kind,
            release_id,
            artifact_digest,
            from_version,
            to_version,
            reason,
            timestamp_ms,
        }
    }

    /// Recompute this event's hash from its own fields -- used by
    /// [`verify_install_event_log`] to detect tampering: a stored
    /// `event_hash` that doesn't match this is proof the record (or its
    /// position in the chain) was altered after being written.
    fn recompute_hash(&self) -> EventHash {
        let bytes = signable_bytes(
            self.sequence,
            self.previous_event_hash.as_ref(),
            self.kind,
            &self.release_id,
            self.artifact_digest.as_ref(),
            self.from_version.as_deref(),
            self.to_version.as_deref(),
            self.reason.as_deref(),
            self.timestamp_ms,
        );
        HashAlgorithm::Blake3.digest(&bytes)
    }

    fn encode(&self) -> Vec<u8> {
        let mut body = Vec::new();
        body.extend_from_slice(&self.sequence.to_be_bytes());
        push_opt_bytes(
            &mut body,
            self.previous_event_hash.as_ref().map(|h| h.as_slice()),
        );
        body.extend_from_slice(&self.event_hash);
        body.push(self.kind.tag());
        push_str(&mut body, self.release_id.as_str());
        push_opt_bytes(
            &mut body,
            self.artifact_digest.as_ref().map(|d| d.as_slice()),
        );
        push_opt_str(&mut body, self.from_version.as_deref());
        push_opt_str(&mut body, self.to_version.as_deref());
        push_opt_str(&mut body, self.reason.as_deref());
        body.extend_from_slice(&self.timestamp_ms.to_be_bytes());

        let mut framed = Vec::with_capacity(body.len() + 4);
        framed.extend_from_slice(&(body.len() as u32).to_be_bytes());
        framed.extend_from_slice(&body);
        framed
    }

    fn decode_body(b: &[u8]) -> Result<Self, InstallLogError> {
        let mut off = 0usize;
        if b.len() < 8 {
            return Err(InstallLogError::Corrupt("record too short for sequence"));
        }
        let sequence = u64::from_be_bytes(b[0..8].try_into().unwrap());
        off += 8;

        if off >= b.len() {
            return Err(InstallLogError::Corrupt("truncated previous-hash flag"));
        }
        let has_prev = b[off];
        off += 1;
        let previous_event_hash = match has_prev {
            0 => None,
            1 => {
                if off + 4 > b.len() {
                    return Err(InstallLogError::Corrupt("truncated previous-hash length"));
                }
                let len = u32::from_be_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]]) as usize;
                off += 4;
                if len != 32 || off + 32 > b.len() {
                    return Err(InstallLogError::Corrupt("bad previous-hash length"));
                }
                let mut h = [0u8; 32];
                h.copy_from_slice(&b[off..off + 32]);
                off += 32;
                Some(h)
            }
            _ => return Err(InstallLogError::Corrupt("bad previous-hash flag")),
        };

        if off + 32 > b.len() {
            return Err(InstallLogError::Corrupt("truncated event hash"));
        }
        let mut event_hash = [0u8; 32];
        event_hash.copy_from_slice(&b[off..off + 32]);
        off += 32;

        if off >= b.len() {
            return Err(InstallLogError::Corrupt("truncated kind tag"));
        }
        let kind = InstallEventKind::from_tag(b[off])?;
        off += 1;

        let release_id_str = take_str(b, &mut off)?;
        let release_id = ObjectId::parse(&release_id_str)
            .map_err(|_| InstallLogError::Corrupt("bad release id"))?;

        let artifact_digest = take_opt_bytes32(b, &mut off)?;
        let from_version = take_opt_str(b, &mut off)?;
        let to_version = take_opt_str(b, &mut off)?;
        let reason = take_opt_str(b, &mut off)?;

        if off + 8 > b.len() {
            return Err(InstallLogError::Corrupt("truncated timestamp"));
        }
        let timestamp_ms = u64::from_be_bytes(b[off..off + 8].try_into().unwrap());
        off += 8;

        if off != b.len() {
            return Err(InstallLogError::Corrupt("trailing bytes in event record"));
        }

        Ok(InstallEvent {
            sequence,
            previous_event_hash,
            event_hash,
            kind,
            release_id,
            artifact_digest,
            from_version,
            to_version,
            reason,
            timestamp_ms,
        })
    }
}

/// Append `event` to the log file at `path`, creating it if necessary.
/// Genuinely append-only: opens with `O_APPEND` semantics
/// ([`OpenOptions::append`]), never truncates or rewrites earlier bytes.
pub(crate) fn append_event(path: &Path, event: &InstallEvent) -> std::io::Result<()> {
    let mut f = OpenOptions::new().create(true).append(true).open(path)?;
    f.write_all(&event.encode())?;
    Ok(())
}

/// Read every event in the log at `path`, in file order. An absent file
/// (nothing has ever been logged here) reads as an empty log, not an
/// error -- the same "no error for the not-yet-happened case" convention
/// [`crate::Installer::current`] already uses.
pub(crate) fn read_events(path: &Path) -> Result<Vec<InstallEvent>, InstallLogError> {
    let bytes = match fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(InstallLogError::Io(e)),
    };
    let mut events = Vec::new();
    let mut off = 0usize;
    while off < bytes.len() {
        if off + 4 > bytes.len() {
            return Err(InstallLogError::Corrupt("truncated record length prefix"));
        }
        let len = u32::from_be_bytes([bytes[off], bytes[off + 1], bytes[off + 2], bytes[off + 3]])
            as usize;
        off += 4;
        if off + len > bytes.len() {
            return Err(InstallLogError::Corrupt("truncated record body"));
        }
        let event = InstallEvent::decode_body(&bytes[off..off + len])?;
        off += len;
        events.push(event);
    }
    Ok(events)
}

/// Everything wrong a persisted event log can independently be caught
/// having -- distinct from [`crate::InstallerError`], since these are
/// findings about *evidence*, not failures of an installer action.
#[derive(Debug)]
#[non_exhaustive]
pub enum InstallLogError {
    Io(std::io::Error),
    /// The log file's bytes don't parse as a well-formed sequence of
    /// records at all (truncated, bad lengths, bad UTF-8, unknown tag).
    Corrupt(&'static str),
    /// A stored `event_hash` does not match the hash recomputed from that
    /// event's own fields -- the record was altered after being written.
    TamperedEventHash {
        sequence: u64,
    },
    /// An event's `previous_event_hash` does not match the actual
    /// preceding event's `event_hash` -- a record was reordered, removed,
    /// or inserted.
    BrokenHashChain {
        sequence: u64,
    },
    /// Two events share the same `sequence` number.
    DuplicateSequence {
        sequence: u64,
    },
    /// Sequence numbers are not the contiguous run `0..events.len()` --
    /// catches a deleted middle record even in the (extremely unlikely)
    /// case its removal somehow left the hash chain looking intact.
    NonContiguousSequence {
        expected: u64,
        found: u64,
    },
    /// A kind appeared for a `release_id` whose own event subsequence
    /// doesn't allow it next (e.g. `HealthCheckStarted` with no preceding
    /// `Activating` for that same release -- "missing activation before
    /// health check"; `Activating` with no preceding `OwnerApproved` --
    /// "activation without owner approval"; any kind appearing after that
    /// release's lineage already reached a terminal kind).
    InvalidTransition {
        release_id: ObjectId,
        kind: InstallEventKind,
        sequence: u64,
    },
    /// A `RollbackStarted`/`RolledBack` event for a release whose own
    /// lineage never recorded a `HealthCheckFailed`, and which also
    /// carries no `reason` explaining an alternate trigger (e.g. a
    /// caller-initiated manual rollback) -- an unexplained rollback is
    /// not evidence, it's a gap.
    UnexplainedRollback {
        release_id: ObjectId,
        sequence: u64,
    },
    /// A `PreviousReleaseActive` event whose immediately preceding event
    /// (by global sequence) is not a `RolledBack` event.
    PreviousReleaseActiveWithoutRollback {
        sequence: u64,
    },
    /// A `RolledBack` event's `to_version` does not match the version
    /// that was actually last known-active (via `HealthCheckPassed` or
    /// `PreviousReleaseActive`) immediately before this rollback began --
    /// evidence of rolling back to a version that was never the real
    /// prior state.
    StaleRollbackTarget {
        sequence: u64,
        claimed: Option<String>,
        actual: Option<String>,
    },
}

impl std::fmt::Display for InstallLogError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InstallLogError::Io(e) => write!(f, "install log I/O error: {e}"),
            InstallLogError::Corrupt(why) => write!(f, "install log is corrupt: {why}"),
            InstallLogError::TamperedEventHash { sequence } => {
                write!(f, "event {sequence}'s hash does not match its own fields")
            }
            InstallLogError::BrokenHashChain { sequence } => {
                write!(f, "event {sequence}'s previous-hash link is broken")
            }
            InstallLogError::DuplicateSequence { sequence } => {
                write!(f, "sequence number {sequence} appears more than once")
            }
            InstallLogError::NonContiguousSequence { expected, found } => write!(
                f,
                "expected sequence {expected} next, found {found} -- a record is missing"
            ),
            InstallLogError::InvalidTransition {
                release_id,
                kind,
                sequence,
            } => write!(
                f,
                "event {sequence} ({kind:?}) is not a valid next step for release {}",
                release_id.as_str()
            ),
            InstallLogError::UnexplainedRollback {
                release_id,
                sequence,
            } => write!(
                f,
                "event {sequence}: rollback for release {} has no preceding health-check failure and no reason",
                release_id.as_str()
            ),
            InstallLogError::PreviousReleaseActiveWithoutRollback { sequence } => write!(
                f,
                "event {sequence}: PreviousReleaseActive does not immediately follow a RolledBack event"
            ),
            InstallLogError::StaleRollbackTarget {
                sequence,
                claimed,
                actual,
            } => write!(
                f,
                "event {sequence}: rollback claims to restore {claimed:?} but the actual last-known-good version was {actual:?}"
            ),
        }
    }
}

impl std::error::Error for InstallLogError {}

/// A log that has passed every check in [`verify_install_event_log`] --
/// holding one of these is the only way to have observed that the whole
/// chain is hash-linked, sequence-contiguous, and state-machine-valid.
#[derive(Debug, Clone)]
pub struct VerifiedInstallHistory {
    pub events: Vec<InstallEvent>,
}

impl VerifiedInstallHistory {
    /// This history's events for one release, in order.
    pub fn for_release<'a>(&'a self, release_id: &'a ObjectId) -> Vec<&'a InstallEvent> {
        self.events
            .iter()
            .filter(|e| &e.release_id == release_id)
            .collect()
    }
}

/// Verify `events` forms genuine evidence: an intact hash chain,
/// contiguous unique sequence numbers, and, per `release_id`, a state
/// machine transition that could actually have happened -- not merely
/// telemetry a caller can trust at face value.
pub fn verify_install_event_log(
    events: &[InstallEvent],
) -> Result<VerifiedInstallHistory, InstallLogError> {
    // 1. Sequence contiguity and uniqueness, and hash-chain integrity,
    //    checked in file order (global, not per-release).
    let mut seen_sequences = std::collections::BTreeSet::new();
    for (i, event) in events.iter().enumerate() {
        if !seen_sequences.insert(event.sequence) {
            return Err(InstallLogError::DuplicateSequence {
                sequence: event.sequence,
            });
        }
        if event.sequence != i as u64 {
            return Err(InstallLogError::NonContiguousSequence {
                expected: i as u64,
                found: event.sequence,
            });
        }
        if event.recompute_hash() != event.event_hash {
            return Err(InstallLogError::TamperedEventHash {
                sequence: event.sequence,
            });
        }
        let expected_prev = if i == 0 {
            None
        } else {
            Some(events[i - 1].event_hash)
        };
        if event.previous_event_hash != expected_prev {
            return Err(InstallLogError::BrokenHashChain {
                sequence: event.sequence,
            });
        }
    }

    // 2. Per-release state machine: walk each release_id's own
    //    subsequence and check every kind against its allowed
    //    predecessor(s) within that same subsequence.
    use std::collections::HashMap;
    let mut last_kind_for: HashMap<&ObjectId, InstallEventKind> = HashMap::new();

    // Tracks the version most recently known to be genuinely active
    // (HealthCheckPassed or PreviousReleaseActive), for the stale-
    // rollback-target check below.
    let mut last_known_active_version: Option<String> = None;

    for event in events {
        if event.kind == InstallEventKind::PreviousReleaseActive {
            // Global check: must immediately follow a RolledBack event.
            let idx = event.sequence as usize;
            let follows_rollback = idx > 0
                && events
                    .get(idx - 1)
                    .is_some_and(|prev| prev.kind == InstallEventKind::RolledBack);
            if !follows_rollback {
                return Err(InstallLogError::PreviousReleaseActiveWithoutRollback {
                    sequence: event.sequence,
                });
            }
            last_known_active_version = event.to_version.clone();
            continue;
        }

        let predecessor = last_kind_for.get(&event.release_id).copied();
        let allowed = event.kind.valid_predecessors();
        let ok = match predecessor {
            None => allowed.is_empty(),
            Some(prev_kind) => allowed.contains(&prev_kind),
        };
        if !ok {
            return Err(InstallLogError::InvalidTransition {
                release_id: event.release_id.clone(),
                kind: event.kind,
                sequence: event.sequence,
            });
        }

        if event.kind == InstallEventKind::RollbackStarted {
            let had_failure = predecessor == Some(InstallEventKind::HealthCheckFailed);
            if !had_failure && event.reason.is_none() {
                return Err(InstallLogError::UnexplainedRollback {
                    release_id: event.release_id.clone(),
                    sequence: event.sequence,
                });
            }
        }

        if event.kind == InstallEventKind::RolledBack
            && event.to_version != last_known_active_version
        {
            return Err(InstallLogError::StaleRollbackTarget {
                sequence: event.sequence,
                claimed: event.to_version.clone(),
                actual: last_known_active_version.clone(),
            });
        }

        if event.kind == InstallEventKind::HealthCheckPassed {
            last_known_active_version = event.to_version.clone();
        }

        last_kind_for.insert(&event.release_id, event.kind);
    }

    Ok(VerifiedInstallHistory {
        events: events.to_vec(),
    })
}
