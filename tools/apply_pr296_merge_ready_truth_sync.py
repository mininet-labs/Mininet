#!/usr/bin/env python3
"""Truth-sync PR #296 after the final protocol implementation is complete."""
from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    p = Path(path)
    text = p.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected one match, found {count}: {old[:160]!r}")
    p.write_text(text.replace(old, new), encoding="utf-8")


f6 = "docs/design/f6-private-query-transport.md"
replace_once(
    f6,
    """**Status:** Phases 1 and 2 are merged. Phase 3 is implemented in draft PR #296. None is a private-information-retrieval scheme — see "What Phase 1 is not" below.
""",
    """**Status:** Phases 1 and 2 are merged. Phase 3 is implemented and tested in PR #296, pending independent human review and merge. None is a private-information-retrieval scheme — see "What Phase 1 is not" below.
""",
)
replace_once(
    f6,
    """- **Not requester-identity-hiding beyond what already exists for free.** `mini_bearer::Channel`/CH1 is identity-agnostic by construction — a query round trip needs no `did:mini` proof from the *client* at all, so the requester is exactly as anonymous as any other CH1 session, with zero new code. This document does not add caller authentication because none is needed for this direction; a future decision may add it if a provider wants to rate-limit or bill a specific caller, which is explicitly out of scope here.
""",
    """- **Not requester-identity-hiding beyond what already exists for free.** The Phase 1 anonymous `remote_query` path uses identity-agnostic CH1 and needs no client `did:mini` proof. Phase 3 deliberately adds a separate mutual-authentication path for callers choosing peer-bound provider provenance; that named path discloses the requester’s selected root or pairwise identity. There is still no server-only provider-authentication mode for an entirely unnamed requester.
""",
)
replace_once(
    f6,
    """No frozen invariant is touched. No voice/value wall edge (P1, Directive 16): this module adds no new crate dependency to `mini-search-federation-net` beyond what it already has (`mini-query` for `parse_query`/`search`, already a dependency; `mini-bearer` for the channel, already a dependency). No generic `sign(bytes)`/authority surface — there is no signing here at all, deliberately, per "no `Object`/signature wrapping" above. No payment, ranking-authority, or truth-oracle claim: the response is one provider's own computed opinion, exactly as every other Track F source already is.
""",
    """No frozen invariant is touched and no voice/value wall edge is introduced (P1, Directive 16). Phases 1 and 2 added no dependency beyond the existing query/channel/search stack. Phase 3 adds the existing local path dependency on `mini-transport-security` solely for optional typed channel authentication; that crate exposes identity binding, not ranking, personhood, payment, discovery, or governance authority. The fresh F6 response remains unsigned and non-durable. The named transport exchange does use `did:mini` signatures inside `mini-transport-security`, but it adds no generic `sign(bytes)` surface and no response-signing or re-verifiability claim. No payment, ranking-authority, or truth-oracle claim exists: the response remains one provider's own computed opinion.
""",
)

planning = "docs/planning/privacy-transport-runtime-convergence.md"
replace_once(
    planning,
    """**Status:** implementation complete in draft PR #296; merge and production
claims remain gated on exact-head CI and human review.  
""",
    """**Status:** implementation and focused validation complete in PR #296; merge
and production claims remain gated on exact-head CI and independent human review.  
""",
)

status = "docs/STATUS.md"
replace_once(
    status,
    """  **Not** a private-information-retrieval scheme: the queried peer sees
  the query text in full; true query-content privacy is separate future
  work gated behind issue #72's external crypto review, the same gate
  Sphinx/Loopix sits behind. Requester anonymity needed no new code —
  `mini_bearer::Channel`/CH1 already discloses no client identity. Not
  wrapped in a signed `Object` (a live answer, not a durable one). Real,
  tested: 6 unit tests (round trip matches local `search`; oversized
  query/`max_results` bounds; a compliant server never over-returns; every
  current `AvailabilityState`/`RestrictionReason`/`UnavailabilityReason`/
  `Scheme`/`PersonalizationPolicy` variant round-trips with a future-safe
  fallback; tampered ciphertext fails closed) plus one real `TcpBearer`
  socket test. See `docs/design/f6-private-query-transport.md`.
""",
    """  **Not** a private-information-retrieval scheme: the queried peer sees
  the query text in full; true query-content privacy is separate future
  work gated behind issue #72's external crypto review, the same gate
  Sphinx/Loopix sits behind. The anonymous Phase 1 path uses identity-free
  CH1; PR #296's optional named path is mutual authentication and therefore
  discloses a requester root or pairwise identity. Responses are not wrapped
  in signed `Object`s (live answers, not durable ones). Real, tested: local/
  remote equivalence; request/result/field bounds before encoding and after
  decoding; canonical URL/profile validation; requested-profile attribution;
  displayability preservation; unknown future wire tags failing closed;
  tamper rejection; and real `TcpBearer` socket coverage. See
  `docs/design/f6-private-query-transport.md`.
""",
)
replace_once(
    status,
    """  `federated_result_from_wire` converts one `WireResult` into a typed
  `mini_query::ResultProvenance` (rejecting any `relevance_score_bps` or
  `explanation` component above `WeightBps::MAX`, since `WireResult`'s
  wire codec does not itself bound those fields on decode), and
  `merge_remote_results` folds a whole response into a caller's own
  local/pulled results, failing closed on the first invalid entry rather
  than dropping it silently. The merged result's provider tag is
  caller-asserted, not cryptographically verified — F6 provides no
  caller/provider authentication beyond the channel itself; binding it to
  `mini-transport-security`'s authenticated peer identity is named
  follow-up, not attempted here. Real, tested: 8 new unit tests (valid
  round trip; out-of-range score and explanation component each rejected;
  URL-collision dedup by score; `max_results` respected across the
  combined set; an invalid remote result fails the whole merge). See
  `docs/design/f6-private-query-transport.md`.
""",
    """  `federated_result_from_wire` converts one `WireResult` into a typed
  `mini_query::ResultProvenance` and invokes the same canonical-field,
  multihash, URL, score, and displayability validator as the wire decoder,
  repeating typed score conversion as defense in depth. `merge_remote_results`
  folds a whole response into a caller's local/pulled results and fails closed
  on the first invalid entry. Its legacy provider tag remains caller-asserted
  for anonymous/out-of-band use. PR #296 adds a separate named path:
  `SearchQuery`-purpose `AuthenticatedConnection`, private
  `AuthenticatedQueryResults`, an endpoint-derived provider label stable across
  channels to preserve F3 determinism and prevent handshake grinding, and a
  sealed merge that accepts no caller-selected replacement label. Endpoint
  rotation intentionally rotates the label; provider honesty and cross-rotation
  continuity remain unsolved. See `docs/design/f6-private-query-transport.md`.
""",
)

log = "docs/DECISION_LOG.md"
replace_once(
    log,
    """**Implementation status:** complete in draft PR #296. Permanent code adds the
""",
    """**Implementation status:** complete and focused-test green in PR #296, pending
independent human review and merge. Permanent code adds the
""",
)

print("PR 296 merge-ready truth sync applied")
