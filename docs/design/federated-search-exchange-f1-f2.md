# Federated search exchange, merging, local re-ranking, and observation history: Track F1/F2/F3/F4/F7

**Decisions:** D-0422, D-0423, D-0424, D-0426  
**Status:** Shipped and tested within the bounds stated below. No real federation transport, peer discovery, provider payment implementation, private query transport, or shared history consensus exists.

**Refs:** `docs/research/MININET_NATIVE_INTAKE_PUBLIC_COMMONS_AND_OPEN_WEB_SEARCH_20260718.md` §29; roadmap issue #175; D-0316 (`mini-web-types`); D-0405 (`mini-lexical-index`); D-0406 (`mini-ranker`); D-0420 (`mini-query`); D-0427 (F5 doctrine).

## Scope

`mini-search-federation` supplies the deterministic local and signed-object pieces two independent MiniSearch providers need before a real transport can be built:

- **F1:** signed crawl-observation objects;
- **F2:** signed immutable index-segment objects;
- **F3:** deterministic merging of per-provider query results;
- **F4:** caller-local re-ranking under a selected profile; and
- **F7:** a bounded, rebuildable local history over authenticated crawl observations.

These pieces preserve provenance and plurality. They do not appoint a canonical search provider, canonical index, truth oracle, trusted timestamp service, or payment authority.

## F1 — signed crawl-observation exchange

`publish_crawl_observation` wraps an existing `mini_web_types::CrawlObservation` in a signed, content-addressed `mini_objects::Object`. `read_crawl_observation` decodes the canonical payload and rejects the wrong object type, encrypted payloads, malformed fields, unsupported wire tags, and oversized fields or collections.

Publication applies the same host/path/query/media-type/redirect/multihash bounds before encoding. A caller cannot construct a large but typed in-memory observation that the publisher accepts and the crate's own reader later rejects solely because it violates the reader's wire limits. `observation_wire_len` computes the exact bounded canonical payload size without allocating the encoded payload first; F7 reuses that one source of truth for byte-budget accounting.

Authentication remains layered:

1. canonical object/content-address integrity;
2. wrapping-object signature and provenance against the publisher's KEL; and
3. typed F1 payload decoding and use as that publisher's statement.

`read_crawl_observation` performs step 3, not step 2. `SnapshotIndex::insert_observation` additionally re-parses the object's current bytes and verifies its stored content id still matches them, but still has no KEL input and therefore cannot authenticate the publisher. A well-formed, content-address-consistent payload is not automatically authentic.

The observation still contains a caller-supplied `CrawlObservationId`; deriving and enforcing that ID remains a separate integrity gap.

`ProviderPseudonym` is carried inside the observation. The signed object may itself be authored under a scoped `did:mini` pseudonym selected by the caller. This crate does not invent a second pseudonym scheme or force a root identity into search history.

## F2 — content-addressed index segments

`publish_index_segment` signs the existing canonical `IndexSegment::to_bytes()` representation. `read_index_segment` delegates canonical-form validation to `IndexSegment::from_bytes()` rather than introducing a second index codec.

A segment is bounded by `mini_objects::MAX_PAYLOAD_BYTES` (8 MiB). Segment splitting is not implemented. If that limit becomes operationally restrictive, a separately reviewed composition similar to `mini-media` superblocks is the existing precedent.

## F3 — deterministic federated query merging

`federate_query` runs the unmodified `mini_query::search` once per local `FederationSource`, with the same query, profile, time input, and per-source result bound. It then:

1. tags every result with the supplying `ProviderPseudonym`;
2. deduplicates by canonical URL, keeping the higher score and breaking equal-score ties by provider-pseudonym bytes;
3. sorts by score descending, then canonical URL; and
4. truncates to `max_results`.

The result is independent of source-list order. The function adds no payment, bid, stake, provider trust, or governance input. It performs no network I/O and has no bound on how many sources the caller may supply; a real remote federation layer must impose its own concurrency/work limits.

## F4 — local re-ranking

`local_rerank` takes F3 output and recomputes the final score under a caller-selected `RankingProfile` without querying a provider again. It calls `mini_ranker::rescore`, which shares one private weighted-average implementation with `rank`; local re-ranking therefore does not maintain a second scoring formula that can drift.

The result's `ranking_profile` is updated and the list is re-sorted deterministically. The original `diversity_bps` component is reused rather than recomputed because it depends on the original result ordering. A caller requiring diversity to be recalculated must perform a fresh ranking operation.

Payment cannot improve organic relevance: no payment/provider-revenue field exists in `rank`, `rescore`, `federate_query`, or `local_rerank`.

## F7 — bounded local history over observations

F1 already stores multiple independent observations of the same resource. F7 adds a local search/view layer over those objects without inventing a canonical history.

### Canonical-object insertion and provenance preservation

`SnapshotIndex::insert_observation` accepts the actual F1 `mini_objects::Object`.
It does not accept an independently supplied object id, URL, timestamp, digest, crawler, status, or decoded observation. Before mutation it:

1. serializes and re-parses the object's current canonical bytes;
2. verifies that the object's stored content id still matches the id derived from those bytes, catching an in-memory object mutated after signing;
3. applies the existing F1 type/visibility/field decoder; and
4. derives all indexed state from that decoded observation.

This removes the unsigned-shadow-field problem and prevents a caller from pairing an arbitrary valid-looking `ObjectId` with unrelated observation data. It does **not** verify the publisher's signature or KEL provenance; callers still perform that separate F1 authentication step before treating the history as authenticated.

The index keys by `observation.final_url`. It preserves the complete observation—including requested URL, redirect chain, crawler pseudonym, fetch status, media type, byte length, claimed timestamp, and digest—beside the canonical wrapper `ObjectId`.

Reinserting the exact same canonical object is idempotent. A different canonical object, even one carrying the same observation, remains a distinct publisher statement/corroboration object.

### Explicit count and byte bounds

`SnapshotLimits` bounds all five independent growth dimensions:

- the number of final URLs;
- snapshots per final URL;
- total snapshots;
- canonical F1 payload bytes for one snapshot; and
- total canonical F1 payload bytes across the index.

Every limit is checked before mutation; zero disables insertion for that dimension. A `Snapshot` records its canonical `wire_bytes`, and the index exposes `total_wire_bytes`. Exact Rust allocator overhead is platform-dependent, so wire bytes are explicitly a deterministic budget proxy rather than a claim of exact resident RAM.

The exported defaults are finite, but they are **not** represented as weakest-device benchmarks. Production defaults still require measurement on the oldest supported phones; callers may lower the limits immediately. This is a local safety budget, not a network quota, provider entitlement, or completeness guarantee.

### Deterministic order without false chronology

Histories are ordered by crawler-claimed `observed_at_ms`, then object ID for deterministic storage. That order is not represented as canonical time or proof of when the origin changed.

`latest` and `at_or_before` return the entire greatest equal-timestamp group. They do not silently choose one provider when equally timestamped observations disagree.

### Version relations

Each `Snapshot` receives one of:

- `Baseline` — an agreed digest when no earlier agreed digest exists in the locally held history;
- `Unchanged` — same digest as the last earlier agreed digest;
- `Changed` — different digest from the last earlier agreed digest;
- `Unknown` — no digest, therefore no version claim; or
- `SameTimestampDisagreement` — two or more known digests disagree at the same claimed timestamp.

Unknown observations do not reset the previous known digest and do not create a false change. A same-timestamp disagreement is exposed through `disagreements`; no arbitrary object-ID ordering is promoted into a temporal version transition, and no disagreeing digest becomes the next comparison base. A later agreed digest compares with the last earlier agreed digest when one exists; only when none exists does it establish a local baseline.

`distinct_versions` includes only `Baseline`/`Changed` observations, excludes unknown/disputed groups, and collapses same-timestamp same-digest corroboration to one deterministic representative.

These are relations among locally held observations. They do not prove that content was true, complete, globally visible, or changed at a precise real-world instant.

## Tests

The crate's existing suites cover F1/F2 canonical round trips and signature-layer separation, F3 merge order/deduplication/provenance, and F4 score recombination.

The F1 hardening tests prove the publisher refuses typed observations with an overlong URL field or redirect chain instead of creating self-undecodable objects.

F7's 16-test adversarial suite covers:

- empty history;
- canonical-object-derived identity/final URL and preservation of the full typed observation;
- rejection of an object mutated after signing while retaining a stale content id;
- reuse of F1 wrong-type and encrypted-payload rejection;
- insertion-order-independent sorting;
- exact-object idempotence without double-counting bytes;
- URL, per-URL, total-count, per-snapshot-byte, total-byte, and zero limits;
- equal-timestamp groups for `latest`/`at_or_before`;
- lower-inclusive/upper-exclusive windows;
- unknown-digest gaps;
- same-timestamp digest disagreement;
- preservation of an earlier comparison base across disagreement;
- same-timestamp corroboration; and
- independent final-URL histories.

Passing these tests proves the stated local mechanics. It does not authenticate a caller that skipped F1 signature/provenance verification, establish a trustworthy timestamp, corroborate a remote page, benchmark the default budgets on weak hardware, or create a deployed federation.

## What's deliberately not here

- No network transport, peer discovery, request protocol, want-list, or scheduler.
- No automatic signature/KEL verification inside the payload readers or history index.
- No canonical provider roster or cross-provider trust weight.
- No shared/persisted/signed `SnapshotIndex`; it is rebuilt from held observations.
- No consensus over page history or authoritative answer to “what was true at T.”
- No provider-payment implementation. F5 now has a dedicated Phase-0 doctrine in `docs/design/anti-collusion-content-settlement-preparation.md` (D-0427), but no implementation phase is authorized or started.
- No private query transport (F6); it remains undesigned.
- No payment, subsidy, or provider revenue in organic ranking.

## Constitutional and authority impact

No frozen invariant is amended. Search providers remain replaceable edge participants. Signed observations are claims by their publishers, not institutional truth. Indexes and rankings remain plural and forkable. Provider work or payment creates no governance, personhood, moderation, validator, or organic-ranking authority.

## Failure points

This design fails if a caller:

- treats content-address consistency or successful decode as signature/provenance verification;
- treats crawler timestamps as canonical time;
- selects one disagreeing provider through an arbitrary tie-break and calls it truth;
- bypasses or misrepresents the finite count/byte budgets;
- markets unbenchmarked defaults as proven weak-device capacity;
- lets provider identity or payment alter relevance;
- presents a local `SnapshotIndex` as a globally complete archive; or
- introduces a central transport/payment/history service that other search operations cannot survive without.

## Required follow-up

- Enforce a canonical derivation rule for `CrawlObservationId`.
- Benchmark F7 budgets on weakest supported devices before production defaults are claimed.
- Wire F1/F2 objects through a bounded real transport with authenticated peer behavior and source-count limits.
- Persist or exchange history only after defining conflict, omission, provenance, and privacy semantics for a shared history object.
- Keep F5 behind D-0427's Phase-2 transcript/threat/economic-model gate; do not jump directly to a nullifier or payment crate.
- Write a separate F6 private-query-transport doctrine before implementation.

## Supersedes / superseded by

Builds on and does not supersede D-0316, D-0405, D-0406, or D-0420. D-0427 supplies the separate doctrine for F5; it does not modify F1-F4/F7 behavior.