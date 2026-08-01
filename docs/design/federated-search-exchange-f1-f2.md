# Federated search exchange format and query merging: Track F1/F2/F3 (D-0422, D-0423)

**Status:** Shipped (`mini-search-federation`). Wire format,
signed-object publish/read, and deterministic per-provider result
merging — no network transport, no peer discovery, no scheduling, no
local re-ranking against a second profile (F4).

**Refs:** `docs/research/
MININET_NATIVE_INTAKE_PUBLIC_COMMONS_AND_OPEN_WEB_SEARCH_20260718.md`
§29 ("Track F: Distributed search"), PR F1 ("Signed crawl-observation
exchange"), PR F2 ("Content-addressed index segments"), PR F3
("Federated query"); roadmap issue #175; D-0316 (`mini-web-types`);
D-0405 (`mini-lexical-index`); D-0420 (`mini-query`); D-0422 (F1/F2).

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

## What's deliberately not here

- No network transport. Nothing in this crate opens a socket, dials a
  peer, or knows what a "peer" is. `federate_query` takes already-local
  sources; it does not fetch anything.
- No peer discovery, request/response protocol, or want-list logic —
  `mini-sync`'s existing type-agnostic replication (D-0080) is the
  closest existing analogue for "how would two nodes actually exchange
  these objects," but wiring it up is separate, later work.
- No F4 (local re-ranking against a second, personalized profile — F3's
  merge already applies one shared profile, but does not let a caller
  overlay their own after the fact), F5 (provider payments), F6 (private
  query transport), or F7 (historical snapshots). Each remains a one-line
  research-doc description, not designed here.
- No cross-provider trust weighting: `federate_query` does not treat any
  provider as more or less trustworthy than another, and does not detect
  a provider flooding the merge with many near-duplicate low-quality
  results beyond what `search`'s own `max_results` bound per provider
  already limits.

## Constitutional impact

None intended. No frozen invariant is amended. Purely additive: no
existing crate's function signature changes (`mini-web-types`,
`mini-lexical-index`, `mini-objects`, `mini-store` are all unmodified).
No new cryptography — reuses `mini-crypto`'s existing
Multihash/Ed25519/BLAKE3 exactly as every other signed object in this
workspace already does.

## Implementation status

`crates/mini-search-federation/`: `error.rs` (`FederationError`),
`codec.rs` (`Writer`/`Reader`, private), `observation.rs` (F1),
`segment.rs` (F2), `federate.rs` (F3), `lib.rs`. 8 integration tests
(`tests/federation.rs`, F1/F2): round trip with every field populated,
round trip with every optional field absent, wrong-object-type rejection
for both object kinds, encrypted-payload rejection for both object
kinds, a non-canonical index-segment payload caught at
`IndexSegment::from_bytes` (not just this crate's own type check), and a
tampered payload proven to still decode (well-formed bytes, different
content) but fail signature verification — demonstrating decode-success
and authenticity are genuinely separate checks, not one conflated with
the other. 6 integration tests (`tests/federate.rs`, F3): results from
every provider merged and correctly tagged, a shared URL across two
providers keeping the higher-scoring copy, merge output proven
order-independent (forward vs. reversed source list), `max_results`
truncation, an empty source list producing no results, and each result
retaining its own `index_segment`/`source_observation` provenance.

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
need to close, not this wire-format layer. `federate_query` queries every
source for up to `max_results` of its own candidates before merging —
correct for a bounded, known source list, but the cost is linear in the
number of sources with no cap on how many sources a caller may pass; a
real federation layer will need its own bound on concurrently-queried
providers, not something this module enforces.

## Required follow-up

F4 (local re-ranking against a caller's own personalized profile after
the merge) is the natural next Track F piece. Wiring F1/F2's objects
into a real transport (likely via `mini-sync`'s existing replication
machinery, per D-0080's own finding that it already carries arbitrary
object types over real TCP) — and, once that exists, wiring
`federate_query` to real peer-fetched sources rather than only local
ones — is separate follow-up work, not started.

## Supersedes / superseded by

Builds on and does not supersede D-0316, D-0405, or D-0420. Does not
modify `mini-web-types`, `mini-lexical-index`, or `mini-query`.
