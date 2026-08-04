# Track F6: private query transport — Phase 0 doctrine, Phase 1 and Phase 2 slices

**Decisions:** D-0435 (Phase 1), D-0436 (Phase 2) (see `docs/DECISION_LOG.md`)
**Status:** Phase 1 and Phase 2 implemented and tested. Not a private-information-retrieval scheme — see "What Phase 1 is not" below.
**Refs:** roadmap [#175](../../issues/175) (Track F, distributed/federated search); `docs/design/federated-search-exchange-f1-f2.md` (F1-F5/F7, which this document builds on and does not modify); `mini-search-federation-net`'s own crate doc, which named this exact gap ("Sending a caller's search query to a remote peer for server-side evaluation is Track F6 (private query transport), explicitly undesigned and out of scope"); issue #72 (external crypto audit gate).

## The gap this closes

Every other Track F piece assumes a caller already holds (or has bulk-pulled) a full `IndexSegment`/`Corpus`/`DocumentContextTable` and runs `mini_query::search` against it locally. That is exactly why `mini-search-federation-net`'s existing advertise/pull exchange never needed to send a query anywhere: `federate_query` always ran on data the caller already had. That design has a real limit: pulling an entire index segment (up to 8 MiB, `mini_objects::MAX_PAYLOAD_BYTES`) to answer one query is wasteful when a caller only wants a handful of ranked results from a specific, already-known provider. F6 is the "just ask the provider directly" mode Track F has been missing since `docs/design/federated-search-exchange-f1-f2.md` was first written.

Doing this at all means a query's actual terms cross the wire to the queried provider, which every other Track F piece went out of its way to avoid. That trade only becomes acceptable once it is scoped honestly, which is the point of this Phase 0 doctrine.

## What Phase 1 is (and is not)

**Is:** a bounded, single-provider, confidential-in-transit query/response round trip. A caller sends a raw query string, a `RankingProfile`, and a `max_results` cap to one already-dialed peer over an already-established `mini_bearer::Channel`. The server runs the *unmodified* `mini_query::parse_query` + `mini_query::search` against its own locally-held index/corpus/context tables and returns a bounded list of ranked results.

**Is not:**

- **Not private information retrieval.** The queried provider sees the caller's exact query terms in full — `parse_query`/`search` need real terms to run, and this document does not invent a PIR/oblivious-keyword-search scheme to hide them. A real PIR construction (single- or multi-server, FHE-based or otherwise) is itself a nontrivial, actively-researched cryptographic primitive; composing one here without independent review would violate CLAUDE.md's no-new-cryptography rule as surely as hand-rolling Sphinx would have for `mini-relay`'s mix tier. **The queried provider learning your query is the correct, honestly-stated Phase 1 floor**, not a bug to paper over. Closing it is future work gated the same way the Sphinx/Loopix mix executor is gated behind issue #72's external crypto review — not attempted here.
- **Not requester-identity-hiding beyond what already exists for free.** `mini_bearer::Channel`/CH1 is identity-agnostic by construction — a query round trip needs no `did:mini` proof from the *client* at all, so the requester is exactly as anonymous as any other CH1 session, with zero new code. This document does not add caller authentication because none is needed for this direction; a future decision may add it if a provider wants to rate-limit or bill a specific caller, which is explicitly out of scope here.
- **Not a truth or trust upgrade.** The response is the queried provider's own computed ranking over its own held data — exactly as authoritative (and exactly as unverified against independent corroboration) as any other Track F source. `mini-transport-security`'s optional endpoint authentication (a separate, concurrently-developed crate) can bind *which* peer answered if a caller wants that; this document does not duplicate it.
- **Not integrated into F3's typed merge path in Phase 1.** `federate_query`'s `FederatedResult` machinery expects a real `mini_ranker::Corpus`/`DocumentContextTable`-backed `FederationSource`, not a flat list of remote-computed results. Wiring a remote query's results into that merge (deduplication, tie-breaking, provider tagging) was deliberately deferred rather than rushed into the Phase 1 slice; Phase 2 (below) closes this specific gap.
- **Not a scheduler, cache, rate limiter, or anti-abuse mechanism.** A server that wants to bound how many queries one peer may run, or cache repeated results, builds that on top of `serve_query`; nothing here does it automatically.
- **Not multi-provider fan-out.** One `remote_query` call talks to exactly one already-dialed peer, the same scope discipline `pull_source` (not `pull_from_sources`) uses for a single source.

## Wire design

Two new message types (`query::Msg`), tagged and length-prefixed exactly like the existing `message::Msg` advertise exchange, sent over the same `chan.seal`/`chan.open` pattern `session.rs` already uses:

- `QueryRequest { query: String, profile: RankingProfile, max_results: u32 }` — `query` is bounded to `MAX_QUERY_BYTES` (512, matching the scale of other free-text caps in this crate family — F2b's title/snippet fields); `max_results` is bounded to `MAX_QUERY_RESULTS` (64, the same order of magnitude as `mini_query::search`'s own typical caller bounds). `RankingProfile`'s own fields (a `Multihash`-backed id, a version, six `WeightBps` weights, and a `PersonalizationPolicy` tag) are small and already fully bounded by their own types (`WeightBps::MAX = 10_000`), so they encode directly with no additional caps needed.
- `QueryResponse { results: Vec<WireResult> }` — bounded to the request's own `max_results` (a response with more entries than the caller asked for is a protocol violation, not silently truncated). Each `WireResult` mirrors `mini_query::ResultProvenance`'s fields (`SearchResult` plus `source_observation`/`index_segment`) with a purpose-built, bounded codec local to this module rather than reusing `mini-search-federation`'s internal (`pub(crate)`) F1/F2b codec — this is a different wire message in a different crate answering a different question ("what did you rank for this live query" vs. "here is a durable, signed, storable object"), so a second small codec here is not the kind of duplication F2b's own codec-unification fixed; it is two different messages that happen to both eventually need to represent a `CanonicalUrl`/`AvailabilityState`.

No `Object`/signature wrapping: unlike F1/F2/F2b, a query response is not meant to be durably stored, replayed, or independently re-verified later — it is answered fresh, live, for exactly this request, and its only integrity property is "came from whoever is on the other end of this already-authenticated-if-the-caller-wants-it `Channel`," which the channel's own AEAD already provides. Wrapping it in a signed `Object` would imply a permanence and re-verifiability this data does not have.

## Server-side scope

`serve_query` takes an already-assembled `IndexSegment`/`Corpus`/`DocumentContextTable`/`IndexSegmentId` — precisely the four pieces a provider already builds today to run `federate_query` locally (see F2/F2b) — and answers queries against them. It does not decide *which* index a peer may query, does not select a profile on the caller's behalf, and does not persist or log queries; a caller wanting any of that builds it around `serve_query`, not inside it.

## Constitutional and authority impact

No frozen invariant is touched. No voice/value wall edge (P1, Directive 16): this module adds no new crate dependency to `mini-search-federation-net` beyond what it already has (`mini-query` for `parse_query`/`search`, already a dependency; `mini-bearer` for the channel, already a dependency). No generic `sign(bytes)`/authority surface — there is no signing here at all, deliberately, per "no `Object`/signature wrapping" above. No payment, ranking-authority, or truth-oracle claim: the response is one provider's own computed opinion, exactly as every other Track F source already is.

## Tests

Adversarial coverage in `crates/mini-search-federation-net/src/query.rs` (unit tests, mirroring `session.rs`'s own in-process `Channel` test harness):

- round trip: a real local `IndexSegment`/`Corpus`/`DocumentContextTable` served, queried remotely, and the ranked results match what an equivalent local `mini_query::search` call would produce;
- an oversized query string is rejected before encoding;
- `max_results` of zero or above `MAX_QUERY_RESULTS` is rejected;
- a compliant server never returns more results than the request's own `max_results`;
- every current `AvailabilityState`/`RestrictionReason`/`UnavailabilityReason` variant round-trips through the `WireResult` codec, with a future-variant-safe fallback for both `#[non_exhaustive]` enums, mirroring F2b's own coverage discipline;
- a tampered ciphertext fails closed (the channel's own AEAD authentication, exercised the same way `session.rs`'s tests already prove it for the advertise exchange).

## Phase 2: wiring into F3's merge path (D-0436)

Phase 1 left one of its own named non-goals open: a `remote_query` response could not be blended with a caller's local/pulled Track F sources into one ranked list, because `federate_query`'s merge machinery only ever accepted a `FederationSource` — a real `Corpus`/`DocumentContextTable`-backed local index — not a flat list of already-computed remote results.

**What changed:**

- `mini-search-federation::federate::federate_query`'s dedup/sort/truncate merge step is extracted into a standalone public function, `merge_federated_results(results: Vec<FederatedResult>, max_results: usize) -> Vec<FederatedResult>`. `federate_query` itself now just gathers each source's `search` results and calls this function — its own external behavior and signature are unchanged.
- `mini-search-federation-net` gains a `remote_merge` module:
  - `federated_result_from_wire(wire: WireResult, provider: ProviderPseudonym) -> Result<FederatedResult>` converts one F6 wire result into a typed `mini_query::ResultProvenance`. It rejects (`NetError::Protocol`) any `relevance_score_bps` or `explanation` component above `WeightBps::MAX` — a value a compliant `serve_query` can never produce (`mini_query::search` only ever emits validated `WeightBps`), but which `WireResult`'s own wire codec does not itself bound on decode, so this is the real fail-closed check against a peer that sends an out-of-range score.
  - `merge_remote_results(local: Vec<FederatedResult>, remote: Vec<WireResult>, remote_provider: ProviderPseudonym, max_results: usize) -> Result<Vec<FederatedResult>>` converts and folds a whole `remote_query` response into a caller's own local/pulled results via `merge_federated_results`, failing closed on the first invalid wire result rather than silently dropping it and returning a partial merge.

**The `remote_provider` tag is caller-asserted, not cryptographically verified** — unchanged from Phase 1's own stated floor. A query response carries no `Object`/signature wrapping, and F6 provides no caller/provider authentication beyond the channel itself. A caller names `remote_provider` from whatever it already knows out-of-band about who it dialed (an advertisement it resolved, its own session setup) — exactly as honest, and exactly as unverified, as every other Track F provider label already is once results leave a single signed object's custody. Binding this to `mini-transport-security`'s authenticated peer identity, once that crate lands review, remains real follow-up (see below), not attempted here.

Tests (`crates/mini-search-federation-net/src/remote_merge.rs`): a valid wire result converts and round-trips every field; an out-of-range `relevance_score_bps` is rejected; an out-of-range `explanation` component is rejected; a URL present in both local and remote results deduplicates by score exactly as `federate_query`'s own documented policy promises; `max_results` is respected across the combined set; a single invalid remote result fails the whole merge rather than returning a partial one.

## Required follow-up

- True query-content privacy against the queried provider itself (PIR/oblivious keyword search) — a distinct, harder cryptographic problem, gated behind issue #72's external review, not attempted here.
- Bind `remote_provider` to `mini-transport-security`'s authenticated peer identity once that crate lands review, closing the caller-assertion gap Phase 2 explicitly leaves open.
- Rate limiting, caching, and query logging policy — all left to the caller, as stated above.
- Multi-provider fan-out (`remote_query_many`, mirroring `pull_from_sources`) feeding the same Phase 2 merge in one call, once a real deployment shape motivates it.

## Supersedes / superseded by

New ground — no prior decision addressed sending live query terms to a remote peer. Phase 2 (D-0436) builds directly on Phase 1 (D-0435), completing its named follow-up; it does not modify F1-F5/F7's own object formats or `federate_query`'s external behavior/signature (only its internal implementation, now delegating to the newly extracted `merge_federated_results`). Builds on and does not modify `mini_query::parse_query`/`search` (unmodified, reused exactly as-is), or `mini-search-federation-net`'s existing advertise/pull/assemble exchange (`message.rs`/`session.rs`/`multi.rs`/`assemble.rs`, all untouched).
