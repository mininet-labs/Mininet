# Federated search exchange format: Track F1/F2 (D-0422)

**Status:** Shipped (`mini-search-federation`). Wire format and
signed-object publish/read only — no network transport, no peer
discovery, no scheduling, no federated query merging.

**Refs:** `docs/research/
MININET_NATIVE_INTAKE_PUBLIC_COMMONS_AND_OPEN_WEB_SEARCH_20260718.md`
§29 ("Track F: Distributed search"), PR F1 ("Signed crawl-observation
exchange") and PR F2 ("Content-addressed index segments"); roadmap issue
#175; D-0316 (`mini-web-types`); D-0405 (`mini-lexical-index`).

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

## What's deliberately not here

- No network transport. Nothing in this crate opens a socket, dials a
  peer, or knows what a "peer" is.
- No peer discovery, request/response protocol, or want-list logic —
  `mini-sync`'s existing type-agnostic replication (D-0080) is the
  closest existing analogue for "how would two nodes actually exchange
  these objects," but wiring it up is separate, later work.
- No F3 (federated query merging), F4 (local re-ranking — already
  possible today via `mini-query`'s `search`, just not across multiple
  providers' segments), F5 (provider payments), F6 (private query
  transport), or F7 (historical snapshots). Each remains a one-line
  research-doc description, not designed here.
- No deduplication policy across peers publishing overlapping or
  conflicting observations of the same URL — a real federation layer
  needs one; this crate only makes each individual observation/segment
  exchangeable and verifiable, it does not reconcile between them.

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
`segment.rs` (F2), `lib.rs`. 8 integration tests
(`tests/federation.rs`): round trip with every field populated, round
trip with every optional field absent, wrong-object-type rejection for
both object kinds, encrypted-payload rejection for both object kinds, a
non-canonical index-segment payload caught at `IndexSegment::from_bytes`
(not just this crate's own type check), and a tampered payload proven to
still decode (well-formed bytes, different content) but fail signature
verification — demonstrating decode-success and authenticity are
genuinely separate checks, not one conflated with the other.

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
need to close, not this wire-format layer.

## Required follow-up

F3 (federated query merging) is the natural next Track F piece and the
first one that actually needs multiple providers' segments composed —
out of scope here. Wiring F1/F2's objects into a real transport (likely
via `mini-sync`'s existing replication machinery, per D-0080's own
finding that it already carries arbitrary object types over real TCP) is
separate follow-up work, not started.

## Supersedes / superseded by

Builds on and does not supersede D-0316 or D-0405. Does not modify
`mini-web-types` or `mini-lexical-index`.
