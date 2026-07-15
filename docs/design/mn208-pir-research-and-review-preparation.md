# PIR research and external-review preparation (MN-208 Phase 9, D-0098)

**Status:** Research and review preparation only. No PIR crate, no PIR
code, no cryptographic dependency added. `mini-private-index`'s existing
`LookupPrivacyClass::PrivatePIR` remains unimplemented and gated behind
D-0047, exactly as it was before this document.

**Full research:** `docs/research/
MN208_PIR_RESEARCH_AND_REVIEW_PREPARATION_20260715.md` (founder-supplied,
2026-07-15). This document does not reproduce that report — it records
what direction was adopted from it and why, and links back for the full
candidate comparison, benchmark methodology, and external-review question
set the report itself lays out.

## Decision

Per the report's own executive conclusion, Mininet does not select or
implement a production PIR protocol yet. `LookupPrivacyClass::PrivatePIR`
(`docs/design/private-lookup-and-dht-boundary.md`, D-0310) was already
named-but-unimplemented and gated behind external cryptographic review
(D-0047); this decision adds the concrete research and benchmark
programme that must happen before that gate can open, without touching
any existing type or behavior.

The report's own closing line states the correct first deliverable
plainly: "a research-only PR containing the fixed workload, benchmark
methodology, candidate shortlist, and external-review questions. No PIR
crate should be added until that package has been reviewed and the
whole-index and two-server baselines have been measured." This PR is
exactly that package — prose and structure, no Rust.

## The frozen first workload

The report's central discipline: choose the operation before choosing
the cryptography. The narrow first PIR target this workspace should ever
benchmark against is:

> Retrieve one fixed-size encrypted mailbox or private-provider
> descriptor from one immutable, epoch-versioned database containing
> equal-length records.

Not arbitrary object retrieval, full-text search, dynamic key-value
operations, variable-size results, private writes, subscriptions,
general ORAM, or arbitrary application queries — each of those is a
different workload with different cryptographic economics, and the
report is explicit that conflating them is the first mistake a PIR
programme can make.

This workload maps directly onto `mini-private-index`'s existing
`PrivateIndexRecord`/`RecordSizeClass` model (D-0310): PIR research
should target retrieval from an immutable, signed, fixed-size-class
epoch database built from records that already exist in that crate's
vocabulary — not a new database shape invented for PIR's sake.

## Candidate portfolio (research targets, not selections)

The report's decision sequence — never a single fixed answer:

```
Is complete epoch download within the target budget?
    yes → use complete download
    no  ↓
Are genuinely independent replicas available?
    yes → benchmark two-server PIR
    no  ↓
Is reviewed single-server PIR within client/server budget?
    yes → use selected CPIR experimentally
    no  ↓
Use proxied bundled lookup with explicit weaker assurance.
```

Four candidates are in scope for the benchmark/review programme:

- **Candidate 0 — whole-index download.** The mandatory baseline. No PIR
  trust assumption at all: a client that downloads every row cannot have
  its row selection observed, because the server sees every row
  requested. For small community namespaces this may be the *correct
  final design*, not merely a benchmark floor.
- **Candidate 1 — two-server information-theoretic PIR.** The preferred
  first true-PIR research candidate: aligns with Mininet's existing
  preference for independently operated infrastructure (Tier 1 relay
  role separation, D-0306), no heavy client-side cryptography, and a
  security claim ("neither operator alone learns the row") that is easy
  to state precisely and easy to falsify if the operators are not
  actually independent.
- **Candidate 2 — one mature single-server lattice PIR.** Spiral or a
  current SimplePIR-family successor, benchmarked (not chosen) for the
  case where genuinely independent replicas do not exist. SealPIR stays
  a compatibility/correctness baseline, not a preferred endpoint.
- **Watchlist — ZipPIR.** A 2026 single-server design reporting high
  throughput without large client-stored hints. Explicitly *not* on the
  implementation shortlist — insufficient independent review history and
  deployment evidence as of this decision. Reassess after independent
  cryptanalysis and implementation maturity, not before.

ORAM and general searchable encryption are explicitly out of scope: a
different, much larger leakage surface than the fixed-record retrieval
problem this phase is scoped to.

## What "achieved privacy" must say

The report's own naming discipline, carried forward as the shape any
future implementation's result type must take — a typed, named property,
never an undifferentiated boolean:

```
FullIndexDownloaded
TwoServerPir { assumed_non_collusion: 1 }
SingleServerComputationalPir { scheme, parameter_set }
ProxiedBundledLookup
```

None of these types exist in code yet. This is a naming constraint on
whatever Phase 10+ eventually builds, recorded now so "just return
`Private: bool`" is never the default reached for later.

## What PIR does not solve (must never be overclaimed)

Carried forward verbatim from the report because it is the single most
important scoping fact for every future PIR PR: PIR can hide *which row*
a client retrieved. It does not hide the client's network address, query
timing, which database/epoch was queried, request size or frequency,
correlation with a later direct object fetch, replica collusion, or
malicious/inconsistent server responses. A future implementation must
compose PIR with relay/mix transport (already existing, D-0306/D-0308)
and delayed, decoupled object fetch — PIR is one layer of private
retrieval, never the entire system.

## Required before any PIR code PR

Per the report's own release gates (§39), all of the following must
happen — in this order — before a single PIR crate or dependency enters
this workspace:

1. the fixed-record, immutable-epoch workload is frozen (this document);
2. a whole-index-download baseline is benchmarked;
3. two-server information-theoretic PIR is benchmarked against real
   replica infrastructure;
4. one mature single-server scheme is benchmarked (not more — the report
   is explicit that benchmarking ten schemes superficially is worse than
   one scheme rigorously);
5. database-update and mobile-client costs are simulated, not just
   server throughput;
6. a replica-independence policy and a malicious-server threat model are
   written down;
7. an external cryptographic review happens over the above;
8. only then is one candidate selected for an experimental
   implementation — and even that implementation stays out-of-process
   behind a sandboxed worker boundary (mirroring `mini-build-runner-
   wasmtime`'s isolation precedent, D-0069) rather than linked directly
   into `mini-private-index`.

None of steps 2-8 are started by this PR. This document exists so that
when they do start, they start from a frozen workload and a named
candidate set instead of an implementer's individual judgment call.

## Constitutional posture

No new cryptography (Directive 14): nothing here composes or invents a
primitive — it names research targets. No voice/value dependency
possible (this track has zero relationship to `mini-value`/`mini-
bounty`/`mini-treasury`). No claim beyond what's built: `mini-private-
index`'s STATUS.md entry continues to say `PrivatePIR` is unimplemented
and gated, and this decision does not change that sentence's truth even
slightly — it only makes the gate's opening criteria concrete.
