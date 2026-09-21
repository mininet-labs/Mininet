//! Which parts of this repository the diagnostics actually exercise --- and,
//! just as importantly, which they do not.
//!
//! ## Why this table exists
//!
//! "Can I test everything?" deserves an auditable answer, not an assurance.
//! Before this table, the suite covered 13 of 82 crates and nothing said so;
//! a user reading a green result would reasonably have concluded the whole
//! system had been checked.
//!
//! So every workspace crate is classified here, and
//! [`tests/coverage.rs`](../../tests/coverage.rs) parses the workspace
//! `Cargo.toml` and fails if any member is missing from this table or listed
//! twice. A new crate therefore cannot be added without someone deciding, on
//! the record, whether a user can exercise it. The classification can say
//! "not covered, because X" --- it cannot say nothing.
//!
//! ## What the statuses mean
//!
//! [`Coverage::Exercised`] is the only one that means a check runs this
//! crate's own code. [`Coverage::Transitive`] means another area's check
//! cannot pass unless this crate works, which is real evidence but not a
//! test of the crate's own surface. [`Coverage::SeparateBinary`] means the
//! checks exist but run in another process, on purpose (see
//! [`crate::coverage::VALUE_BINARY`]). [`Coverage::Gap`] means no diagnostic
//! touches it, and carries the reason.

/// How well a user can exercise one crate from the diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Coverage {
    /// Checks in this area run this crate's own code.
    Exercised {
        /// The [`crate::AREAS`] entry whose checks cover it.
        area: &'static str,
    },
    /// No check targets it directly, but a covered check cannot pass unless
    /// it works.
    Transitive {
        /// Which area depends on it.
        via: &'static str,
    },
    /// Covered by checks that run in a separate process.
    SeparateBinary {
        /// The [`crate::AREAS`] entry those checks report under, so this
        /// status credits an area exactly the way [`Coverage::Exercised`]
        /// does and the two views of the suite cannot drift apart.
        area: &'static str,
        /// Which binary runs them.
        binary: &'static str,
        /// Why it is a separate process rather than a linked dependency.
        reason: &'static str,
    },
    /// Not exercised by any diagnostic.
    Gap {
        /// Why not. Never empty.
        reason: &'static str,
    },
}

impl Coverage {
    /// A short, stable machine name.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Exercised { .. } => "exercised",
            Self::Transitive { .. } => "transitive",
            Self::SeparateBinary { .. } => "separate_binary",
            Self::Gap { .. } => "gap",
        }
    }

    /// The explanatory text for this classification.
    pub fn detail(&self) -> &'static str {
        match self {
            Self::Exercised { area } | Self::Transitive { via: area } => area,
            Self::SeparateBinary { reason, .. } | Self::Gap { reason } => reason,
        }
    }

    /// True when a user can run this crate's code from the diagnostics,
    /// whether in this process or a spawned one.
    pub fn is_runnable(&self) -> bool {
        matches!(self, Self::Exercised { .. } | Self::SeparateBinary { .. })
    }
}

/// The binary that runs the value-layer checks.
///
/// Separate on purpose. The canonical invariant is P1, "no balance maps to
/// governance or validator vote weight" --- a rule about vote weight, not
/// about what a user may look at. Nothing stops a person testing the money
/// code and the governance code from one window, and refusing them that
/// would put the wall in the user's way instead of in the code, which is
/// exactly backwards.
///
/// What the wall does require is that no dependency path connect the value
/// crates to the governance crates. So the value checks live in their own
/// binary that this crate **spawns and reads JSON from**, never links ---
/// the same boundary `mini-build-runner-wasmtime` uses for Wasmtime
/// (D-0069). `mini-selftest` keeps its `mini-forge` edge, the value binary
/// keeps its `mini-value` edge, and neither graph ever reaches the other.
pub const VALUE_BINARY: &str = "mininet-value-selftest";

/// Every workspace crate, and what a user can exercise of it.
///
/// Kept in `Cargo.toml` member order so a reviewer can diff the two lists by
/// eye; the exhaustiveness test does it mechanically.
pub const COVERAGE: &[(&str, Coverage)] = &[
    (
        "mini-durable",
        Coverage::Transitive { via: "storage: every store-backed check commits through it" },
    ),
    (
        "mini-crypto",
        Coverage::Exercised { area: "crypto" },
    ),
    (
        "did-mini",
        Coverage::Exercised { area: "identity" },
    ),
    (
        "mini-witness-service",
        Coverage::Gap {
            reason: "a durable witness journal for KEL receipts; exercising it means running a witness service, which is a node role rather than a client one",
        },
    ),
    (
        "mini-bearer",
        Coverage::Exercised { area: "sync" },
    ),
    (
        "mini-presence",
        Coverage::Gap {
            reason: "needs two physically co-present devices and a real ranging radio; this is the hardware gate the project already names as a launch blocker, not an oversight",
        },
    ),
    (
        "mini-reward",
        Coverage::Exercised { area: "reward" },
    ),
    (
        "mini-keystone",
        Coverage::Gap {
            reason: "needs two physically co-present devices and a real ranging radio; this is the hardware gate the project already names as a launch blocker, not an oversight",
        },
    ),
    (
        "mini-ffi",
        Coverage::Gap {
            reason: "a UniFFI boundary for mobile shells; exercising it needs a mobile host, not a desktop process",
        },
    ),
    (
        "mini-objects",
        Coverage::Exercised { area: "storage" },
    ),
    (
        "mini-store",
        Coverage::Exercised { area: "storage" },
    ),
    (
        "mini-crdt",
        Coverage::Exercised { area: "storage" },
    ),
    (
        "mini-sync",
        Coverage::Exercised { area: "sync" },
    ),
    (
        "mini-messaging",
        Coverage::Exercised { area: "messaging" },
    ),
    (
        "mini-social",
        Coverage::Exercised { area: "social" },
    ),
    (
        "mini-media",
        Coverage::Exercised { area: "media" },
    ),
    (
        "mini-forge",
        Coverage::Exercised { area: "forge" },
    ),
    (
        "mini-bootstrap",
        Coverage::Gap {
            reason: "genesis capsules are a first-run network-formation step; a client that already has an identity has nothing meaningful to bootstrap from",
        },
    ),
    (
        "mini-update",
        Coverage::Transitive { via: "install: the release-verification gates it wraps are what the install checks act on" },
    ),
    (
        "mini-chain",
        Coverage::Exercised { area: "consensus" },
    ),
    (
        "mini-storage",
        Coverage::Gap {
            reason: "a serve receipt is co-signed by two devices that actually exchanged bytes; forging both halves in one process would test the signature code, not the property",
        },
    ),
    (
        "mini-net",
        Coverage::Exercised { area: "network" },
    ),
    (
        "mini-uniqueness",
        Coverage::Exercised { area: "personhood" },
    ),
    (
        "mini-spacetime",
        Coverage::Exercised { area: "spacetime" },
    ),
    (
        "mini-treasury",
        Coverage::SeparateBinary {
            area: "value",
            binary: VALUE_BINARY,
            reason: "value crate: checked in a spawned process, so no crate in this tree links both the value layer and the governance layer",
        },
    ),
    (
        "mini-custody",
        Coverage::Gap {
            reason: "its DKG ceremony needs several custodians exchanging packages over a transport; the threshold rule it protects is covered by the treasury checks in the value binary",
        },
    ),
    (
        "mini-value",
        Coverage::SeparateBinary {
            area: "value",
            binary: VALUE_BINARY,
            reason: "value crate: checked in a spawned process, so no crate in this tree links both the value layer and the governance layer",
        },
    ),
    (
        "mini-private-payment",
        Coverage::Gap {
            reason: "a shielded payment needs a funded output set to spend from; the range proofs and balance rule it rests on are covered by the value checks",
        },
    ),
    (
        "mini-bounty",
        Coverage::Gap {
            reason: "an anonymous bounty claim needs a funded pool and an approved-contributor set; the ring-signature primitives under it are not yet exercised",
        },
    ),
    (
        "mini-settlement",
        Coverage::Exercised { area: "settlement" },
    ),
    (
        "mini-dtn",
        Coverage::Gap {
            reason: "store-carry-forward only means something across a real disruption and a second carrier; the in-memory half is covered by its own crate suite",
        },
    ),
    (
        "mini-execution",
        Coverage::Gap {
            reason: "a canonical ledger view needs finalized blocks from a running chain; the claim half it consumes is covered by the settlement area",
        },
    ),
    (
        "mini-consensus",
        Coverage::Gap {
            reason: "a consensus round needs several validators exchanging votes over a transport; single-vote signing and binding are covered by the consensus area",
        },
    ),
    (
        "mini-porep",
        Coverage::Gap {
            reason: "sealing is deliberately slow and sequential -- that slowness is the security property -- so a real proof takes far longer than a diagnostics run should",
        },
    ),
    (
        "mini-erasure",
        Coverage::Exercised { area: "erasure" },
    ),
    (
        "mini-cli",
        Coverage::Gap {
            reason: "the developer command-line tool itself; it runs these diagnostics rather than being one of them",
        },
    ),
    (
        "mini-provenance",
        Coverage::Gap {
            reason: "builder agreement needs several independent builders signing the same output; one process can only play all of them, which proves nothing about independence",
        },
    ),
    (
        "mini-pipeline",
        Coverage::Gap {
            reason: "pure pipeline manifest and capability types consumed by the build runner, which a client does not ship",
        },
    ),
    (
        "mini-pipeline-protocol",
        Coverage::Gap {
            reason: "wire framing between a coordinator and the Wasmtime runner; exercising it means spawning that runner, which has its own adversarial suite",
        },
    ),
    (
        "mini-build-runner-wasmtime",
        Coverage::Gap {
            reason: "the sandboxed build runner compiles real wasm guests and has a 12-point adversarial suite of its own; running it from a client would mean shipping a compiler",
        },
    ),
    (
        "mini-installer",
        Coverage::Gap {
            reason: "the shipped Windows client uses mini-windows-setup; the legacy POSIX installer is Unix-only and has its own install/activation test suite",
        },
    ),
    (
        "mini-privacy-policy",
        Coverage::Exercised { area: "policy" },
    ),
    (
        "mini-transport-policy",
        Coverage::Gap {
            reason: "routes a privacy request to mechanisms that do not exist yet as running transports; the cost schedule it routes on is covered by the policy area",
        },
    ),
    (
        "mini-transport-security",
        Coverage::Gap {
            reason: "channel-bound peer authentication needs a second peer and a real channel to be worth checking",
        },
    ),
    (
        "mini-resource-pricing",
        Coverage::Exercised { area: "policy" },
    ),
    (
        "mini-relay",
        Coverage::Gap {
            reason: "needs a second live party on a real transport; a single-process check would prove only that the types compile",
        },
    ),
    (
        "mini-bridge",
        Coverage::Gap {
            reason: "pluggable entry transports need a real censored path and a real bridge operator to mean anything",
        },
    ),
    (
        "mini-private-index",
        Coverage::Exercised { area: "policy" },
    ),
    (
        "mini-intake-types",
        Coverage::Gap {
            reason: "shared intake vocabulary; the coordinator that gives it behaviour is itself not exercised here",
        },
    ),
    (
        "mini-web-types",
        Coverage::Transitive { via: "search: crawl and document types are constructed by the search checks" },
    ),
    (
        "mini-intake",
        Coverage::Gap {
            reason: "the intake coordinator ingests external documents; a diagnostic that pulled in outside content is one this client must not have",
        },
    ),
    (
        "mini-intake-social",
        Coverage::Gap {
            reason: "bridges accepted intake envelopes into posts; needs the intake coordinator above to run first",
        },
    ),
    (
        "mini-crawler",
        Coverage::Gap {
            reason: "crawl planning is meaningful against a real frontier of sites; the admission policy alone would be a types check",
        },
    ),
    (
        "mini-crawler-fetch",
        Coverage::Gap {
            reason: "makes real outbound HTTPS requests; a diagnostic that reaches the network is one this client must not have",
        },
    ),
    (
        "mini-desktop",
        Coverage::Gap {
            reason: "the client shell that hosts these diagnostics; its own logic is unit-tested in-crate",
        },
    ),
    (
        "mini-app-protocol",
        Coverage::Gap {
            reason: "the local IPC framing/command types between the desktop shell and its application-service process; framing, limit-rejection, and round-trip behavior are unit-tested in-crate",
        },
    ),
    (
        "mini-app-service",
        Coverage::Gap {
            reason: "the per-user application-service process itself; identity/profile/post/feed flow, restart-idempotent publication, and single-instance refusal are covered by its own in-crate and cross-process tests, not by this in-process diagnostics suite",
        },
    ),
    (
        "mini-windows-vault",
        Coverage::Exercised { area: "identity" },
    ),
    (
        "mini-extract-protocol",
        Coverage::Gap {
            reason: "wire framing to the isolated extractor worker; covered by that worker's own suite",
        },
    ),
    (
        "mini-extract-host",
        Coverage::Gap {
            reason: "spawns an isolated worker process; exercising it from a client would mean shipping that worker",
        },
    ),
    (
        "mini-provider",
        Coverage::Gap {
            reason: "edge-provider declarations describe a commercial role no client fills by itself",
        },
    ),
    (
        "mini-engagement",
        Coverage::Gap {
            reason: "an escrowed engagement needs a funded escrow and a counterparty; it belongs with the value binary once that grows a settlement fixture",
        },
    ),
    (
        "mini-pq-anchor",
        Coverage::Gap {
            reason: "post-quantum anchor pre-provisioning is inventory for a migration that has not happened; there is no behaviour yet for a user to exercise",
        },
    ),
    (
        "mini-attest",
        Coverage::Gap {
            reason: "engagement-proven reviews need a completed, paid engagement first",
        },
    ),
    (
        "mini-airdrop",
        Coverage::Gap {
            reason: "eligibility is computed against a network-wide snapshot that does not exist outside a testnet",
        },
    ),
    (
        "mini-airdrop-treasury",
        Coverage::Gap {
            reason: "bridges airdrop outcomes to treasury payouts; needs the snapshot above to exist first",
        },
    ),
    (
        "mini-commons-policy",
        Coverage::Gap {
            reason: "public-commons entitlements are priced against a wallet standing the client does not yet hold",
        },
    ),
    (
        "mini-publication-policy",
        Coverage::Gap {
            reason: "publication routing plans target transports that are not running yet",
        },
    ),
    (
        "mini-web-extract",
        Coverage::Exercised { area: "search" },
    ),
    (
        "mini-lexical-index",
        Coverage::Exercised { area: "search" },
    ),
    (
        "mini-ranker",
        Coverage::Exercised { area: "search" },
    ),
    (
        "mini-replication-policy",
        Coverage::Exercised { area: "policy" },
    ),
    (
        "mini-economy",
        Coverage::Gap {
            reason: "genesis issuance happens once, at network formation; there is no per-client behaviour to exercise",
        },
    ),
    (
        "mini-econ-sim",
        Coverage::Gap {
            reason: "a long-running economic simulation, not a property a client can check in seconds",
        },
    ),
    (
        "mini-contribution",
        Coverage::Gap {
            reason: "composes engagement, storage and settlement; a meaningful check needs a funded escrow, so it belongs with the value binary",
        },
    ),
    (
        "mini-ticket",
        Coverage::Gap {
            reason: "service-ticket encoding and the provider-only redemption rule are unit-tested in-crate and exercised end to end by mini-desktop's real-TCP host test; a diagnostic would only repeat those",
        },
    ),
    (
        "mini-query",
        Coverage::Exercised { area: "search" },
    ),
    (
        "mini-search-federation",
        Coverage::Gap {
            reason: "federated exchange needs a second federating peer; the local index and ranking it exchanges are covered by the search area",
        },
    ),
    (
        "mini-search-federation-net",
        Coverage::Gap {
            reason: "needs a second live party on a real transport; a single-process check would prove only that the types compile",
        },
    ),
    (
        "mini-storage-fraud",
        Coverage::Gap {
            reason: "cross-identity fraud detection needs several holders claiming the same replica, which means several real storage parties",
        },
    ),
    (
        "mini-shielded-verify",
        Coverage::Gap {
            reason: "verifies shielded spends against a canonical ledger view, which needs a running chain",
        },
    ),
    (
        "mini-mesh",
        Coverage::Gap {
            reason: "a dedup-flood mesh needs several linked peers; single-node behaviour would not show the flooding property at all",
        },
    ),
    (
        "mini-windows-setup",
        Coverage::Exercised { area: "install" },
    ),
    (
        "mini-setup",
        Coverage::Gap {
            reason: "the installer program; its own suite drives the shipped binary across a process boundary",
        },
    ),
    (
        "mini-value-selftest",
        Coverage::Gap {
            reason: "the value diagnostics themselves, run as a separate process so no crate links both the value layer and the governance layer",
        },
    ),
    (
        "mini-selftest",
        Coverage::Gap {
            reason: "this crate: it is the diagnostics, not a subject of them",
        },
    ),
];

/// Crates a user can run code from, as a fraction of all workspace crates.
pub fn runnable_count() -> usize {
    COVERAGE
        .iter()
        .filter(|(_, coverage)| coverage.is_runnable())
        .count()
}

/// Crates no diagnostic touches.
pub fn gaps() -> Vec<(&'static str, &'static str)> {
    COVERAGE
        .iter()
        .filter_map(|(name, coverage)| match coverage {
            Coverage::Gap { reason } => Some((*name, *reason)),
            _ => None,
        })
        .collect()
}

/// One line summarising coverage, for a UI header or a CLI footer.
pub fn summary() -> String {
    format!(
        "{} of {} crates are runnable from diagnostics; {} are covered only as a dependency; {} have no diagnostic, each with a stated reason",
        runnable_count(),
        COVERAGE.len(),
        COVERAGE
            .iter()
            .filter(|(_, c)| matches!(c, Coverage::Transitive { .. }))
            .count(),
        gaps().len()
    )
}
