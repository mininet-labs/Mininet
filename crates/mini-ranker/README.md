# mini-ranker

The MiniSearch transparent ranker. Track E6 of the founder's native-intake /
open-web-search research document
(`docs/research/MININET_NATIVE_INTAKE_PUBLIC_COMMONS_AND_OPEN_WEB_SEARCH_20260718.md`,
§E6), Decision D-0406.

## What this is

`rank(index, corpus, profile, query, now_ms, max_results)` turns a query, a
lexical index (`mini_lexical_index::IndexSegment`), per-document metadata, and
a versioned `RankingProfile` into a deterministic ordering of displayable
`SearchResult`s. Each result carries a `RankingExplanation` breaking its score
down by signal.

Six transparent signals, each a deterministic integer basis-point score:

- **lexical relevance** — query-term coverage, with a bounded frequency boost;
- **phrase match** — full credit when the query's exact phrase is adjacent;
- **link** — a bounded, log-scaled inbound-link hint (a placeholder, not a
  link-graph analysis);
- **freshness** — newer scores higher, against an explicit query time;
- **originality** — exact duplicates (same content digest) are removed;
- **domain diversity** — repeated results from one host are demoted, not
  deleted.

They are combined under the profile's declared weights (a weighted average in
basis points, normalized by the actual weight sum, so a forked profile need not
total 10000).

## What this is not

No query parser or CLI (Track E7). No result provenance beyond the explanation
(Track E8). No crawler, fetcher, extractor, network, or storage. No learned
ranking or click feedback.

## The doctrine, enforced structurally (D-0312)

- **No pay-to-rank.** `rank` has no payment, bid, or provider input. Ranking
  cannot be bought because there is nothing to buy it with.
- **No personalization by default.** The ranker takes no per-user state, so the
  public default holds by construction, not by a flag.
- **Availability is not a relevance penalty.** Restricted or unavailable
  documents are filtered out before scoring, never scored down — an
  availability decision cannot be laundered into the relevance number. (The
  `mini_web_types::SearchResult::displayable` constructor enforces this at the
  type level.)
- **Deterministic ordering.** Every score is integer, the only time input is an
  explicit `now_ms`, and every tie breaks on `UrlId` bytes — the same query,
  index, profile, and time produce byte-identical results anywhere.
- **Explicit, forkable profile.** Weights and version live in the caller's
  `RankingProfile`; a different community ranks the same index differently by
  supplying a different profile, and every result names the profile that
  produced it.

## First-slice limits

The diversity-aware selection is a greedy O(n²) pass, fine for the result-set
sizes a first slice handles. Near-duplicate (non-identical) detection, a real
link-graph signal, and streamed ranking over large segments are named
follow-ups, not present here.
