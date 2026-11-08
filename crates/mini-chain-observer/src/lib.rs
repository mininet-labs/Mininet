//! Read-only observation of external-chain transfers (research PR #429, P2-1).
//!
//! This crate decides one thing: *how sure are we that an external transfer
//! happened*, from reports by independently operated sources. It is an edge
//! service in the research's sense and deliberately powerless:
//!
//! - it holds **no keys** and signs nothing, so it cannot move funds;
//! - it **mints nothing**: its strongest output, [`Status::Finalized`], is
//!   evidence for a separately reviewed process to act on, never a credit;
//! - it has **no network code**: callers feed it [`SourceReport`]s they fetched
//!   (from their own pinned RPC clients), so a malicious RPC can only supply a
//!   report, which is then weighed against the others;
//! - it depends on nothing in the money, governance or personhood crates.
//!
//! ## Rules (each has a test)
//!
//! - Only sources in the configured allow-list count; others are ignored.
//! - One source counts once, however many reports it sends (latest wins).
//! - A report for another network (mainnet vs testnet) is refused.
//! - Reports from the future, or older than the profile's age bound, are
//!   dropped, so clock drift cannot hold a transfer open or fake freshness.
//! - A transfer is [`Status::Finalized`] only when enough *distinct* sources
//!   agree on the same inclusion block **and** each saw the profile's required
//!   confirmations (XRPL additionally requires a *validated* ledger).
//! - Sources disagreeing about a transfer's fields (amount, destination, asset)
//!   or its inclusion block make it [`Status::Ambiguous`]; ambiguity never
//!   upgrades on its own.
//! - A transfer once finalized and later contradicted is reported
//!   [`Status::Ambiguous`] and the contradiction is journaled, never erased.
//!
//! Honest limits: unaudited; it cannot tell a lying quorum from an honest one
//! (the allow-list and its independence are an operator and legal question);
//! a configured source that lies can block progress (liveness) but not forge a
//! finalized status unless it controls a quorum. XRPL and Monero finality
//! assumptions are encoded as profile *data*, to be checked against each
//! chain's current rules before use. Monero payments are private by design, so
//! "observing" one needs a view-key or transaction-proof model that is out of
//! scope here; the shared core only weighs reports it is given.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// Which external chain a report is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Chain {
    /// XRP Ledger.
    Xrpl,
    /// Monero.
    Monero,
}

impl Chain {
    fn tag(self) -> u8 {
        match self {
            Chain::Xrpl => 1,
            Chain::Monero => 2,
        }
    }
    fn from_tag(tag: u8) -> Option<Self> {
        match tag {
            1 => Some(Chain::Xrpl),
            2 => Some(Chain::Monero),
            _ => None,
        }
    }
}

/// Exactly what a transfer is claimed to be. Every field must match for two
/// reports to describe the same transfer; a differing field is a conflict.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Transfer {
    /// The chain.
    pub chain: Chain,
    /// Chain network id (XRPL: 0 main, 1 test, 2 dev; Monero: 0 main, 1 test, 2 stage).
    pub network: u32,
    /// External transaction hash.
    pub tx_hash: [u8; 32],
    /// Asset identifier (XRPL currency/issuer hash, or the native asset).
    pub asset: [u8; 32],
    /// Hash of the destination (never the destination itself).
    pub destination_hash: [u8; 32],
    /// Amount in the chain's minor unit.
    pub amount_minor: u64,
}

/// What one source says it sees.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Seen {
    /// The source cannot find the transfer.
    NotFound,
    /// The transfer failed on chain (for example an XRPL `tec` result).
    Failed,
    /// Included in a block.
    Included {
        /// Block height.
        height: u64,
        /// Block hash.
        block_hash: [u8; 32],
        /// XRPL: the ledger is validated by consensus. Monero: always true.
        validated: bool,
    },
}

/// One source's report. The caller authenticates the transport to the source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceReport {
    /// Identifier of the independently operated source.
    pub source: [u8; 32],
    /// The transfer the report is about.
    pub transfer_hash: [u8; 32],
    /// The chain tip height the source saw when it answered.
    pub tip_height: u64,
    /// What it saw.
    pub seen: Seen,
    /// When it answered, by the observer host's clock domain (ms).
    pub observed_at_ms: u64,
}

/// The ladder. Only `Finalized` is evidence a downstream process may act on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Status {
    /// No usable report.
    Unobserved,
    /// Some source reports it, but no block inclusion is agreed.
    Observed,
    /// Included, with too few agreeing sources or confirmations.
    Included,
    /// Enough independent sources agree with enough confirmations.
    Finalized,
    /// Sources disagree about the transfer or its block.
    Ambiguous,
    /// Enough sources agree it failed on chain.
    Failed,
}

impl Status {
    fn tag(self) -> u8 {
        match self {
            Status::Unobserved => 0,
            Status::Observed => 1,
            Status::Included => 2,
            Status::Finalized => 3,
            Status::Ambiguous => 4,
            Status::Failed => 5,
        }
    }
    fn from_tag(tag: u8) -> Option<Self> {
        Some(match tag {
            0 => Status::Unobserved,
            1 => Status::Observed,
            2 => Status::Included,
            3 => Status::Finalized,
            4 => Status::Ambiguous,
            5 => Status::Failed,
            _ => return None,
        })
    }

    /// Whether a separately reviewed process may rely on this as "happened".
    pub fn is_final_evidence(self) -> bool {
        self == Status::Finalized
    }
}

/// Per-chain finality policy, as data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinalityProfile {
    /// The chain this profile is for.
    pub chain: Chain,
    /// The only network accepted.
    pub network: u32,
    /// Distinct agreeing sources needed.
    pub min_sources: usize,
    /// Confirmations each source must have seen (tip - height).
    pub confirmations: u64,
    /// Whether inclusion must be marked validated (XRPL).
    pub require_validated: bool,
    /// Reports older than this are dropped (ms).
    pub max_report_age_ms: u64,
    /// Reports further in the future than this are dropped (ms).
    pub max_future_skew_ms: u64,
    /// The sources whose reports count.
    pub sources: BTreeSet<[u8; 32]>,
}

impl FinalityProfile {
    /// A conservative XRPL starting point. Confirm against current XRPL rules.
    pub fn xrpl(network: u32, sources: BTreeSet<[u8; 32]>) -> Self {
        Self {
            chain: Chain::Xrpl,
            network,
            min_sources: 2,
            confirmations: 1,
            require_validated: true,
            max_report_age_ms: 120_000,
            max_future_skew_ms: 30_000,
            sources,
        }
    }

    /// A conservative Monero starting point (default unlock depth of 10).
    pub fn monero(network: u32, sources: BTreeSet<[u8; 32]>) -> Self {
        Self {
            chain: Chain::Monero,
            network,
            min_sources: 2,
            confirmations: 10,
            require_validated: false,
            max_report_age_ms: 600_000,
            max_future_skew_ms: 60_000,
            sources,
        }
    }
}

/// Why a profile was rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileError {
    /// `min_sources` must be at least one: zero would let any report finalize.
    NoQuorum,
    /// The allow-list is smaller than the quorum, so nothing could ever finalize.
    TooFewSources,
}

impl FinalityProfile {
    /// Check the profile can only produce honest results.
    pub fn validate(&self) -> Result<(), ProfileError> {
        if self.min_sources == 0 {
            return Err(ProfileError::NoQuorum);
        }
        if self.sources.len() < self.min_sources {
            return Err(ProfileError::TooFewSources);
        }
        Ok(())
    }
}

/// Why a report was not used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// Another chain or network.
    WrongNetwork,
    /// Not in the allow-list.
    UnknownSource,
    /// Older than the age bound.
    Stale,
    /// Further in the future than the skew bound.
    FromTheFuture,
    /// The journal is full.
    JournalFull,
    /// The journal could not be written durably.
    JournalWrite,
}

/// The weighed result for one transfer hash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Assessment {
    /// The status.
    pub status: Status,
    /// How many distinct sources agree with the status's facts.
    pub agreeing_sources: usize,
    /// The agreed inclusion, when there is one.
    pub inclusion: Option<(u64, [u8; 32])>,
}

#[derive(Debug, Clone)]
struct Held {
    transfer: Transfer,
    report: SourceReport,
    /// The same source sent two different reports with the same timestamp, so
    /// arrival order must not decide between them: the source counts as
    /// contradicting itself until it sends a strictly newer report.
    conflicted: bool,
}

/// Collects reports and weighs them. Pure logic; no clock of its own.
#[derive(Debug)]
pub struct Observer {
    profile: FinalityProfile,
    // (transfer hash) -> source -> latest report with its claimed transfer.
    reports: BTreeMap<[u8; 32], BTreeMap<[u8; 32], Held>>,
}

impl Observer {
    /// An observer with the given policy. Refuses a profile that could
    /// finalize on no evidence (zero quorum) or never (quorum above the
    /// allow-list).
    pub fn new(profile: FinalityProfile) -> Result<Self, ProfileError> {
        profile.validate()?;
        Ok(Self {
            profile,
            reports: BTreeMap::new(),
        })
    }

    /// The policy in force.
    pub fn profile(&self) -> &FinalityProfile {
        &self.profile
    }

    /// Record one source's report about `transfer`.
    pub fn record(
        &mut self,
        transfer: Transfer,
        report: SourceReport,
        now_ms: u64,
    ) -> Result<(), Refusal> {
        if transfer.chain != self.profile.chain
            || transfer.network != self.profile.network
            || report.transfer_hash != transfer.tx_hash
        {
            return Err(Refusal::WrongNetwork);
        }
        if !self.profile.sources.contains(&report.source) {
            return Err(Refusal::UnknownSource);
        }
        if report.observed_at_ms > now_ms.saturating_add(self.profile.max_future_skew_ms) {
            return Err(Refusal::FromTheFuture);
        }
        if now_ms.saturating_sub(report.observed_at_ms) > self.profile.max_report_age_ms {
            return Err(Refusal::Stale);
        }
        let by_source = self.reports.entry(transfer.tx_hash).or_default();
        match by_source.get_mut(&report.source) {
            None => {
                by_source.insert(
                    report.source,
                    Held {
                        transfer,
                        report,
                        conflicted: false,
                    },
                );
            }
            // A strictly newer report from a source replaces its earlier one.
            Some(held) if report.observed_at_ms > held.report.observed_at_ms => {
                *held = Held {
                    transfer,
                    report,
                    conflicted: false,
                };
            }
            // Same instant: identical is a no-op; different is a conflict that
            // does not depend on which arrived first.
            Some(held) if report.observed_at_ms == held.report.observed_at_ms => {
                if held.transfer != transfer || held.report != report {
                    held.conflicted = true;
                }
            }
            Some(_) => {}
        }
        Ok(())
    }

    /// Weigh everything currently held for `tx_hash` at `now_ms`.
    pub fn assess(&self, tx_hash: &[u8; 32], now_ms: u64) -> Assessment {
        let none = Assessment {
            status: Status::Unobserved,
            agreeing_sources: 0,
            inclusion: None,
        };
        let Some(by_source) = self.reports.get(tx_hash) else {
            return none;
        };
        let live: Vec<&Held> = by_source
            .values()
            .filter(|held| {
                now_ms.saturating_sub(held.report.observed_at_ms) <= self.profile.max_report_age_ms
            })
            .collect();
        if live.is_empty() {
            return none;
        }
        if live.iter().any(|held| held.conflicted) {
            return Assessment {
                status: Status::Ambiguous,
                agreeing_sources: 0,
                inclusion: None,
            };
        }
        // Reports that say "found" must describe the identical transfer.
        let found: Vec<&&Held> = live
            .iter()
            .filter(|h| !matches!(h.report.seen, Seen::NotFound))
            .collect();
        let fields: BTreeSet<&Transfer> = found.iter().map(|h| &h.transfer).collect();
        if fields.len() > 1 {
            return Assessment {
                status: Status::Ambiguous,
                agreeing_sources: 0,
                inclusion: None,
            };
        }
        let failed = live
            .iter()
            .filter(|h| matches!(h.report.seen, Seen::Failed))
            .count();
        let mut inclusions: BTreeMap<(u64, [u8; 32]), (usize, usize)> = BTreeMap::new();
        for held in &live {
            if let Seen::Included {
                height,
                block_hash,
                validated,
            } = held.report.seen
            {
                let entry = inclusions.entry((height, block_hash)).or_default();
                entry.0 += 1;
                let deep =
                    held.report.tip_height.saturating_sub(height) >= self.profile.confirmations;
                if deep && (validated || !self.profile.require_validated) {
                    entry.1 += 1;
                }
            }
        }
        if failed > 0 && !inclusions.is_empty() {
            return Assessment {
                status: Status::Ambiguous,
                agreeing_sources: 0,
                inclusion: None,
            };
        }
        if failed >= self.profile.min_sources {
            return Assessment {
                status: Status::Failed,
                agreeing_sources: failed,
                inclusion: None,
            };
        }
        if inclusions.len() > 1 {
            // Two different blocks for one transfer: a fork, a reorg in
            // progress, or a lying source. Deny until it resolves.
            return Assessment {
                status: Status::Ambiguous,
                agreeing_sources: 0,
                inclusion: None,
            };
        }
        if let Some((&(height, block_hash), &(seen, deep))) = inclusions.iter().next() {
            let status = if deep >= self.profile.min_sources {
                Status::Finalized
            } else {
                Status::Included
            };
            return Assessment {
                status,
                agreeing_sources: if status == Status::Finalized {
                    deep
                } else {
                    seen
                },
                inclusion: Some((height, block_hash)),
            };
        }
        if found.is_empty() {
            return none;
        }
        Assessment {
            status: Status::Observed,
            agreeing_sources: found.len(),
            inclusion: None,
        }
    }
}

const MAX_JOURNAL_RECORDS: usize = 65_536;

/// One durable line per transfer: the strongest honest statement so far.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Receipt {
    /// The chain.
    pub chain: Chain,
    /// The chain's network (mainnet, testnet, ...): part of the receipt's
    /// identity, so test-network evidence can never pass for mainnet evidence.
    pub network: u32,
    /// The transfer hash.
    pub tx_hash: [u8; 32],
    /// The last recorded status.
    pub status: Status,
    /// The agreed inclusion, if any.
    pub inclusion: Option<(u64, [u8; 32])>,
    /// Whether a finalized receipt was later contradicted. Never cleared.
    pub contradicted_after_finality: bool,
}

type ReceiptKey = (u8, u32, [u8; 32]);

fn key_of(chain: Chain, network: u32, tx_hash: [u8; 32]) -> ReceiptKey {
    (chain.tag(), network, tx_hash)
}

/// A durable, idempotent receipt journal. The whole file is rewritten
/// atomically, so a crash leaves either the old or the new journal; every
/// update takes an exclusive lock and re-reads the file first, so two
/// instances (or processes) on one path cannot overwrite each other's records.
#[derive(Debug)]
pub struct ReceiptJournal {
    path: PathBuf,
    receipts: BTreeMap<ReceiptKey, Receipt>,
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex32(text: &str) -> Option<[u8; 32]> {
    if text.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(text.get(i * 2..i * 2 + 2)?, 16).ok()?;
    }
    Some(out)
}

impl ReceiptJournal {
    /// Open (or start) the journal at `path`.
    pub fn open(path: impl Into<PathBuf>) -> std::io::Result<Self> {
        let path = path.into();
        let receipts = Self::load(&path)?;
        Ok(Self { path, receipts })
    }

    fn load(path: &Path) -> std::io::Result<BTreeMap<ReceiptKey, Receipt>> {
        let mut receipts = BTreeMap::new();
        match std::fs::read_to_string(path) {
            Ok(text) => {
                for line in text.lines().take(MAX_JOURNAL_RECORDS) {
                    let receipt = Self::parse(line).ok_or_else(|| {
                        std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            "corrupt receipt journal",
                        )
                    })?;
                    receipts.insert(
                        key_of(receipt.chain, receipt.network, receipt.tx_hash),
                        receipt,
                    );
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        Ok(receipts)
    }

    fn lock_path(&self) -> PathBuf {
        let mut name = self
            .path
            .file_name()
            .map(|n| n.to_os_string())
            .unwrap_or_default();
        name.push(".lock");
        self.path.with_file_name(name)
    }

    fn parse(line: &str) -> Option<Receipt> {
        let mut parts = line.split(' ');
        let chain = Chain::from_tag(parts.next()?.parse().ok()?)?;
        let network: u32 = parts.next()?.parse().ok()?;
        let tx_hash = unhex32(parts.next()?)?;
        let status = Status::from_tag(parts.next()?.parse().ok()?)?;
        let height: u64 = parts.next()?.parse().ok()?;
        let block = parts.next()?;
        let contradicted = parts.next()? == "1";
        let inclusion = if block == "-" {
            None
        } else {
            Some((height, unhex32(block)?))
        };
        if parts.next().is_some() {
            return None;
        }
        Some(Receipt {
            chain,
            network,
            tx_hash,
            status,
            inclusion,
            contradicted_after_finality: contradicted,
        })
    }

    fn render_of(receipts: &BTreeMap<ReceiptKey, Receipt>) -> String {
        let mut out = String::new();
        for receipt in receipts.values() {
            let (height, block) = receipt
                .inclusion
                .map(|(h, b)| (h, hex(&b)))
                .unwrap_or((0, "-".to_string()));
            out.push_str(&format!(
                "{} {} {} {} {} {} {}\n",
                receipt.chain.tag(),
                receipt.network,
                hex(&receipt.tx_hash),
                receipt.status.tag(),
                height,
                block,
                u8::from(receipt.contradicted_after_finality)
            ));
        }
        out
    }

    /// The receipt for a transfer on one chain and network, if any.
    pub fn get(&self, chain: Chain, network: u32, tx_hash: &[u8; 32]) -> Option<&Receipt> {
        self.receipts.get(&key_of(chain, network, *tx_hash))
    }

    /// Record an assessment. Idempotent, and durable before it is visible: the
    /// journal is locked and re-read, the new state is written, and only then
    /// does it become the in-memory state, so a failed write changes nothing
    /// and a retry writes it.
    ///
    /// A finalized receipt is never silently downgraded or removed. Report
    /// expiry or a temporary loss of quorum (`Unobserved`, `Observed`, or
    /// `Included` at the same block) is **not** contradictory evidence and
    /// leaves it alone. It becomes `Ambiguous` with `contradicted_after_finality`
    /// set for good only on real contradiction: a different inclusion, an
    /// explicit `Ambiguous`, or `Failed`.
    pub fn record(
        &mut self,
        chain: Chain,
        network: u32,
        tx_hash: [u8; 32],
        assessment: &Assessment,
    ) -> Result<&Receipt, Refusal> {
        let _lock =
            mini_durable::lock_exclusive(&self.lock_path()).map_err(|_| Refusal::JournalWrite)?;
        // Another instance may have written since we last looked.
        self.receipts = Self::load(&self.path).map_err(|_| Refusal::JournalWrite)?;
        let key = key_of(chain, network, tx_hash);
        if !self.receipts.contains_key(&key) && self.receipts.len() >= MAX_JOURNAL_RECORDS {
            return Err(Refusal::JournalFull);
        }
        let next = match self.receipts.get(&key).cloned() {
            Some(prev) if prev.contradicted_after_finality => prev,
            Some(prev) if prev.status == Status::Finalized => {
                let contradicted = match assessment.status {
                    Status::Ambiguous | Status::Failed => true,
                    Status::Finalized | Status::Included => {
                        assessment.inclusion.is_some() && assessment.inclusion != prev.inclusion
                    }
                    Status::Unobserved | Status::Observed => false,
                };
                if contradicted {
                    Receipt {
                        status: Status::Ambiguous,
                        contradicted_after_finality: true,
                        ..prev
                    }
                } else {
                    prev
                }
            }
            _ => Receipt {
                chain,
                network,
                tx_hash,
                status: assessment.status,
                inclusion: assessment.inclusion,
                contradicted_after_finality: false,
            },
        };
        if self.receipts.get(&key) != Some(&next) {
            let mut staged = self.receipts.clone();
            staged.insert(key, next);
            mini_durable::atomic_replace(&self.path, Self::render_of(&staged).as_bytes())
                .map_err(|_| Refusal::JournalWrite)?;
            self.receipts = staged;
        }
        Ok(&self.receipts[&key])
    }

    /// The journal's file path.
    pub fn path(&self) -> &Path {
        &self.path
    }
}
