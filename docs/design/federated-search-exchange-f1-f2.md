# Federated search exchange format, query merging, local re-ranking, and history: Track F1/F2/F3/F4/F7 (D-0422, D-0423, D-0424, D-0426)

**Status:** Shipped (`mini-search-federation`). Wire format,
signed-object publish/read, deterministic per-provider result merging,
local re-ranking under a caller's own profile, and a local snapshot-
history index — no network transport, no peer discovery, no scheduling.

**Refs:** `docs/research/
MININET_NATIVE_INTAKE_PUBLIC_COMMONS_AND_OPEN_WEB_SEARCH_20260718.md`
§29 ("Track F: Distributed search"), PR F1 ("Signed crawl-observation
exchange"), PR F2 ("Content-addressed index segments"), PR F3
("Federated query"), PR F4 ("Local re-ranking"), PR F7 ("Historical
snapshots"); roadmap issue #175; D-0316 (`mini-web-types`); D-0405
(`mini-lexical-index`); D-0420 (`mini-query`); D-0406 (`mini-ranker`);
D-0422 (F1/F2); D-0423 (F3); D-0424 (F4).

## What this closes

Track F's own research doc gives each of its seven PRs (F1-F7) one line
of description. F1 and F2 are the two PRs every later Track F piece
depends on: F3 ("federated query," merging candidates from multiple
providers) cannot exist until there is an agreed wire format for what a
"provider" actually exchanges, and F1/F2 are exactly that format for the
two object kinds MiniSearch already produces — a crawl observation
(what a crawler saw) and an index segment (what an indexer built from
observations).

## Decision

Add `mini-search-federation`, wrapping each already-existing type in a
signed, content-addressed `mini_objects::Object` — the identical pattern
`mini-media`'s `publish_media` and `mini-forge`'s `git_import` already
use, not a new signing or storage model:

- **F1** — `publish_crawl_observation`/`read_crawl_observation` wrap a
  `mini_web_types::CrawlObservation` (already shipped, D-0316) in a
  hand-rolled canonical codec (mirroring `mini-lexical-index`'s own
  `Writer`/`Reader` discipline: big-endian integers, u32-length-prefixed
  byte strings, hard caps before allocation) and sign it.
- **F2** — `publish_index_segment`/`read_index_segment` wrap an
  `mini_lexical_index::IndexSegment`'s own already-canonical
  `to_bytes()`/`from_bytes()` (D-0405) directly — no re-encoding, since
  that codec is already content-addressed and canonical-form-enforcing.

Both readers reject the wrong object type and an encrypted payload.
Neither reader verifies the wrapping object's *signature* — that stays
the caller's job via `mini_objects::Object::verify_signature`/
`verify_provenance` against the publishing peer's KEL, the same two-step
"decode, then separately authenticate" pattern every signed-object reader
in this workspace already follows (`mini-forge::git_import`,
`mini-media::read_manifest`, `mini-provenance`).

## Why not a new signing identity

`mini_web_types::ProviderPseudonym` already exists as the crawler's
chosen pseudonymous identifier *inside* a `CrawlObservation`'s own
`crawler` field — this crate does not touch it, and does not invent a
second pseudonym mechanism for the object's own `did-mini` signer. A
caller wanting the object-level signer to be a scoped pseudonym rather
than a root identity already has SPEC-01 §10's
`Controller::incept_pairwise_pseudonym` (shipped, used elsewhere in this
workspace) for that; `publish_crawl_observation`/`publish_index_segment`
take whatever `Did`/`Controller` the caller passes, exactly like
`publish_media` does, so this decision does not have to choose a privacy
posture on the caller's behalf.

## F3: federated query merging (D-0423)

`federate_query` runs the *unmodified* `mini_query::search` once per
[`FederationSource`] (a provider's own `IndexSegment`/`Corpus`/
`DocumentContextTable`/`IndexSegmentId`), then deterministically merges
the per-provider result lists:

1. Concatenate every provider's results, tagging each with the
   `ProviderPseudonym` that supplied it.
2. Deduplicate by canonical URL string: the higher `relevance_score_bps`
   wins; ties break on the smaller provider-pseudonym bytes, so the
   outcome never depends on the order sources were queried in.
3. Sort the deduplicated set by score descending, tie-breaking on
   canonical URL string bytes (mirroring `mini_ranker::rank`'s own
   `UrlId`-byte tiebreak discipline), and truncate to `max_results`.

No new scoring, filtering, or provenance logic is added — merging is the
only new behavior, and it composes E6-E8's already-deterministic,
already-provenanced per-provider output rather than re-deriving any of
it. Every provider is queried with the identical profile/query/`now_ms`,
so scores are directly comparable across providers without this module
needing to normalize anything itself.

## F4: local re-ranking (D-0424)

`local_rerank` takes an already-merged `FederatedResult` list (typically
`federate_query`'s own output) and recomputes each result's final score
under a *different*, caller-chosen `RankingProfile` — with no index,
corpus, or network round trip. Every `SearchResult` already carries a
`RankingExplanation` (the six per-signal scores from whichever profile
originally produced it); re-ranking is exactly recombining those six
numbers under a new set of weights.

To make that recombination honest rather than a second, possibly-
drifting implementation of the same math, this batch adds one small,
purely additive export to `mini-ranker` itself: `pub fn rescore
(explanation: &RankingExplanation, profile: &RankingProfile) ->
Result<WeightBps>`. `rank`'s own internal `combine` function is
refactored (behavior unchanged, all 10 pre-existing `mini-ranker` tests
still pass unmodified) to route through the same private
`weighted_average` helper `rescore` calls — so a score computed via
`rescore` under profile P is bit-for-bit identical to what a fresh `rank`
call under profile P would have produced from the same signals, by
construction, not by inspection.

`local_rerank` then re-sorts by the new scores (descending,
canonical-URL-string tiebreak — the identical convention
`federate_query` uses) and truncates to `max_results`. Each result's
`ranking_profile` field is updated to the new profile's id, so it
honestly names whichever profile actually produced the displayed score.
The `diversity_bps` signal is *not* recomputed — it depends on the
result set's own original ordering (how many higher-ranked results
already shared a host), not a raw per-document property, so re-ranking
reuses it as originally computed rather than re-running the
diversity-aware greedy selection loop, which would be materially more
work than "the user changed weights."

## F7: historical snapshots (D-0426)

F1's `publish_crawl_observation` already lets a caller store as many
independent `CrawlObservation`s of the same URL over time as it likes —
nothing about the F1 wire format assumes one observation per URL. What
was missing was the *search* half: given a URL, find its observation
history, what it looked like at a given time, or which observations
represent a distinct version rather than a repeat fetch of unchanged
content.

`SnapshotIndex` is a small, local, in-memory structure a caller builds
by feeding it observations as they arrive (typically via F1's own
`read_crawl_observation`) — mirroring `mini_query::DocumentContextTable`'s
own "caller-built local table, not itself signed or stored" pattern —
and then queries:

- `insert_observation(url, object_id, observed_at_ms, content_digest)` —
  idempotent (inserting the same `object_id` twice is a no-op), keeps
  each URL's history sorted by time, and recomputes which snapshots
  represent a genuine content change (`content_changed`) so insertion
  order never affects the result.
- `history(url)` — the full sorted history.
- `latest(url)` — the most recent snapshot.
- `at_or_before(url, ms)` — "what did this page look like at time T,"
  the most recent snapshot observed at or before `ms`.
- `between(url, after_ms, before_ms)` — snapshots in a time window,
  using the identical inclusive-lower/exclusive-upper convention
  `mini_query::ParsedQuery`'s own `after_ms`/`before_ms` fields already
  use, so a caller can pass those fields straight through without
  re-deriving the boundary semantics.
- `distinct_versions(url)` — only the snapshots that represent a real
  version (the first, plus every later one whose content digest
  actually changed), filtering out repeat fetches of unchanged content
  without the caller having to do it by hand.

A `content_digest` of `None` for two consecutive snapshots is treated as
"no signal either way" (not a change) — this module does not invent a
rule for what an unknown digest means, it just declines to claim a
change it cannot actually observe.

## What's deliberately not here

- No network transport. Nothing in this crate opens a socket, dials a
  peer, or knows what a "peer" is. `federate_query` takes already-local
  sources; it does not fetch anything.
- No peer discovery, request/response protocol, or want-list logic —
  `mini-sync`'s existing type-agnostic replication (D-0080) is the
  closest existing analogue for "how would two nodes actually exchange
  these objects," but wiring it up is separate, later work.
- No F5 (provider payments) or F6 (private query transport). Each
  remains a one-line research-doc description, not designed here.
- No cross-provider trust weighting: `federate_query` does not treat any
  provider as more or less trustworthy than another, and does not detect
  a provider flooding the merge with many near-duplicate low-quality
  results beyond what `search`'s own `max_results` bound per provider
  already limits.
- No diversity recomputation on re-rank (see F4 section above) and no
  re-ranking of anything other than a `FederatedResult` list — a
  single-provider `ResultProvenance` list from `mini-query::search`
  directly is not accepted by `local_rerank` today.
- `SnapshotIndex` is not itself signed, persisted, or exchanged — it is
  a local view a caller builds from observations it already holds
  (however it obtained them); this module does not decide how a
  snapshot history is shared with, or verified against, a peer.

## Constitutional impact

None intended. No frozen invariant is amended. Almost entirely additive:
no existing crate's function signature *changes* (`mini-web-types`,
`mini-lexical-index`, `mini-objects`, `mini-store`, `mini-query` are all
unmodified); `mini-ranker` gains one new public function (`rescore`) and
one internal refactor (`combine` now routes through the same helper
`rescore` uses) with zero behavior change to `rank` itself, verified by
all 10 pre-existing `mini-ranker` tests passing unmodified plus 2 new
ones. No new cryptography — reuses `mini-crypto`'s existing
Multihash/Ed25519/BLAKE3 exactly as every other signed object in this
workspace already does; F3/F4 perform no cryptographic operations at
all.

## Implementation status

`crates/mini-search-federation/`: `error.rs` (`FederationError`),
`codec.rs` (`Writer`/`Reader`, private), `observation.rs` (F1),
`segment.rs` (F2), `federate.rs` (F3), `rerank.rs` (F4), `history.rs`
(F7), `lib.rs`. `crates/mini-ranker/src/rank.rs`: `rescore` (new,
public) and `weighted_average` (new, private, shared by `combine` and
`rescore`).

8 integration tests (`tests/federation.rs`, F1/F2): round trip with
every field populated, round trip with every optional field absent,
wrong-object-type rejection for both object kinds, encrypted-payload
rejection for both object kinds, a non-canonical index-segment payload
caught at `IndexSegment::from_bytes` (not just this crate's own type
check), and a tampered payload proven to still decode (well-formed
bytes, different content) but fail signature verification. 6 integration
tests (`tests/federate.rs`, F3): results from every provider merged and
correctly tagged, a shared URL across two providers keeping the
higher-scoring copy, merge output proven order-independent (forward vs.
reversed source list), `max_results` truncation, an empty source list
producing no results, and each result retaining its own
`index_segment`/`source_observation` provenance. 5 integration tests
(`tests/rerank.rs`, F4): score and `ranking_profile` update correctly,
re-ranking under the *same* profile reproduces the original order
exactly, re-ranking under a genuinely different (single-signal) profile
flips the winner between two documents engineered to win on opposite
signals, `max_results` truncation, an empty list re-ranks to empty. Plus
2 new `mini-ranker` unit tests: `rescore` under the original profile
reproduces the original score exactly; `rescore` under a lexical-only
profile collapses the score to exactly the lexical signal and differs
from the public-default score. 9 integration tests (`tests/history.rs`,
F7): empty history for an unrecorded URL, snapshots returned oldest-
first regardless of insertion order, idempotent re-insertion of the same
object id, `latest`, `at_or_before` at a point in time, `between`'s
inclusive-lower/exclusive-upper bounds (including one-sided ranges),
`distinct_versions` correctly skipping repeat fetches of unchanged
content, two consecutive unknown digests not being treated as a change,
and independent per-URL histories.

## Failure point

`publish_index_segment` bounds a segment to `mini_objects::MAX_PAYLOAD_
BYTES` (8 MiB) with no splitting mechanism — a segment larger than that
cannot be published through this crate today; `mini-media`'s superblock
pattern (D-0419) is the precedent for how a future "segment too large"
gap would be closed if one is ever hit, not something this crate
pre-emptively builds. `CrawlObservationId` is trusted as caller-supplied
with no derivation rule enforced here (none is defined anywhere in this
workspace yet) — a caller could construct an observation with a
misleading id, an integrity gap federation transport/discovery work will
need to close, not this wire-format layer. `federate_query` queries
every source for up to `max_results` of its own candidates before
merging — correct for a bounded, known source list, but the cost is
linear in the number of sources with no cap on how many sources a caller
may pass; a real federation layer will need its own bound on
concurrently-queried providers, not something this module enforces.
`local_rerank` only accepts `FederatedResult` (F3's own output type),
not a bare single-provider result list, and never recomputes diversity
(see F4 section above) — both deliberate, narrow scope choices, not
oversights. `SnapshotIndex` is entirely in-memory and per-process — it
is not itself persisted, signed, or shared between peers; a caller
restarting from scratch has to rebuild it by replaying whatever
observations it can still reach via `mini-store`. Its `content_changed`
signal is only as good as the `content_digest` an observation actually
carries — a crawler that never populates that field gets no version
detection at all, just an undifferentiated timeline.

## Required follow-up

F5 (provider payments) and F6 (private query transport) remain the
un-designed Track F tail — see `docs/design/
cryptographic-architecture-and-flagship-research-protocol.md` (D-0421)
for why F5 in particular needs its own dedicated anti-collusion-
settlement doctrine before any implementation, not a quick composition.
Wiring F1/F2's objects into a real transport (likely via `mini-sync`'s
existing replication machinery, per D-0080's own finding that it already
carries arbitrary object types over real TCP) — and, once that exists,
wiring `federate_query` to real peer-fetched sources rather than only
local ones, and persisting/sharing a `SnapshotIndex` across peers rather
than rebuilding it per-process — is separate follow-up work, not
started.

## Supersedes / superseded by

Builds on and does not supersede D-0316, D-0405, D-0406, or D-0420.
Does not modify `mini-web-types`, `mini-lexical-index`, or `mini-query`.
Extends (does not supersede) `mini-ranker`'s D-0406 with one additive
public function.
