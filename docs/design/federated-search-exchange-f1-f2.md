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

`publish_crawl_observation` wraps an existing `mini_web_types::CrawlObservation` in a signed, content-addressed `mini_objects::Object`. `read_crawl_observation` decodes the canonical payload and rejects the wrong object type, encrypted payloads, malformed fields, unsupported wire tags, and oversized collections.

Authentication remains layered:

1. decode the object and payload;
2. verify the wrapping object's integrity/signature/provenance against the publisher's KEL; and
3. only then use the decoded observation as an authenticated statement from that publisher.

The reader does not silently perform step 2. A well-formed payload is not automatically authentic. The observation still contains a caller-supplied `CrawlObservationId`; deriving and enforcing that ID remains a separate integrity gap.

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

### Typed insertion and provenance preservation

`SnapshotIndex::insert_observation` accepts exactly:

```text
(ObjectId of the signed F1 wrapper, decoded CrawlObservation)
```

It does not accept separate URL, timestamp, digest, crawler, or status arguments. Every indexed field is derived from the typed observation, preventing unsigned shadow fields from disagreeing with the signed payload.

The index keys by `observation.final_url`. It preserves the complete observation—including requested URL, redirect chain, crawler pseudonym, fetch status, media type, byte length, claimed timestamp, and digest—beside the wrapper `ObjectId`.

The same object ID and exact observation are idempotent. Reusing one object ID for different observation bytes or another final URL fails closed as `FederationError::ConflictingObjectBinding`.

### Explicit bounds

`SnapshotLimits` bounds:

- the number of final URLs;
- snapshots per final URL; and
- total snapshots.

Conservative defaults are exported, and callers may choose smaller limits. All three limits are checked before mutation; zero disables insertion for that dimension. This is a local weak-device safety bound, not a network quota or provider entitlement.

### Deterministic order without false chronology

Histories are ordered by crawler-claimed `observed_at_ms`, then object ID for deterministic storage. That order is not represented as canonical time or proof of when the origin changed.

`latest` and `at_or_before` return the entire greatest equal-timestamp group. They do not silently choose one provider when equally timestamped observations disagree.

### Version relations

Each `Snapshot` receives one of:

- `Baseline` — first digest-bearing observation after the locally held history has an agreed comparison base;
- `Unchanged` — same digest as the last earlier agreed digest;
- `Changed` — different digest from the last earlier agreed digest;
- `Unknown` — no digest, therefore no version claim; or
- `SameTimestampDisagreement` — two or more known digests disagree at the same claimed timestamp.

Unknown observations do not reset the previous known digest and do not create a false change. A same-timestamp disagreement is exposed through `disagreements`; no arbitrary object-ID ordering is promoted into a temporal version transition. A later agreed digest after an unresolved disagreement becomes a new local baseline.

`distinct_versions` includes only `Baseline`/`Changed` observations, excludes unknown/disputed groups, and collapses same-timestamp same-digest corroboration to one deterministic representative.

These are relations among locally held observations. They do not prove that content was true, complete, globally visible, or changed at a precise real-world instant.

## Tests

The crate's existing suites cover F1/F2 canonical round trips and signature-layer separation, F3 merge order/deduplication/provenance, and F4 score recombination.

F7's adversarial suite covers:

- empty history;
- final-URL derivation and preservation of the full typed observation;
- insertion-order-independent sorting;
- exact duplicate idempotence;
- refusal to rebind an object ID to altered bytes or another final URL;
- URL, per-URL, total, and zero limits;
- equal-timestamp groups for `latest`/`at_or_before`;
- lower-inclusive/upper-exclusive windows;
- unknown-digest gaps;
- same-timestamp digest disagreement;
- same-timestamp corroboration; and
- independent final-URL histories.

Passing these tests proves the stated local mechanics. It does not authenticate a caller that skipped F1 signature/provenance verification, establish a trustworthy timestamp, corroborate a remote page, or create a deployed federation.

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

- treats successful decode as signature/provenance verification;
- treats crawler timestamps as canonical time;
- selects one disagreeing provider through an arbitrary tie-break and calls it truth;
- feeds unbounded observations into a weak device;
- lets provider identity or payment alter relevance;
- presents a local `SnapshotIndex` as a globally complete archive; or
- introduces a central transport/payment/history service that other search operations cannot survive without.

## Required follow-up

- Enforce a canonical derivation rule for `CrawlObservationId`.
- Wire F1/F2 objects through a bounded real transport with authenticated peer behavior and source-count limits.
- Persist or exchange history only after defining conflict, omission, provenance, and privacy semantics for a shared history object.
- Keep F5 behind D-0427's Phase-2 transcript/threat/economic-model gate; do not jump directly to a nullifier or payment crate.
- Write a separate F6 private-query-transport doctrine before implementation.

## Supersedes / superseded by

Builds on and does not supersede D-0316, D-0405, D-0406, or D-0420. D-0427 supplies the separate doctrine for F5; it does not modify F1-F4/F7 behavior.