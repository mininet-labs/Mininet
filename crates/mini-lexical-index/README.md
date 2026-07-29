# mini-lexical-index

The MiniSearch lexical index. Track E5 of the founder's native-intake /
open-web-search research document
(`docs/research/MININET_NATIVE_INTAKE_PUBLIC_COMMONS_AND_OPEN_WEB_SEARCH_20260718.md`,
§E5), Decision D-0405.

## What this is

A deterministic, immutable inverted index over document fields:

- `IndexBuilder` — accumulate documents (a `UrlId` plus per-`Field` text),
  then `build()` a frozen `IndexSegment`.
- `IndexSegment` — answers structural queries:
  - `term_documents(term)` — documents containing a term in any field;
  - `phrase_documents(phrase)` — documents where the phrase's tokens are
    consecutive **within a single field**, using stored positions;
  - `postings(term)` — raw positions and fields, for a ranker.
- Canonical `to_bytes()` / `from_bytes()`, a BLAKE3 `segment_id()`, and a
  compact `IndexManifest`.
- `tokenize()` — the one deterministic tokenizer both indexing and querying
  use (Unicode-alphanumeric runs, lowercased, position-tracked).

## What this is not

No ranking or scoring (Track E6). No crawler, fetcher, or extractor (Tracks
E3/E4). No query parser or CLI (Track E7). No storage backend — a segment is
a plain value you store wherever you like. And per D-0312, **no payment,
provider, ranking-authority, or governance-weight field anywhere**: the index
records what text exists where, never what it is worth or who paid for it.

## Why determinism is load-bearing

`UrlId` and `IndexSegmentId` are content addresses. A segment's id is the
BLAKE3 digest of its canonical bytes, so the same documents always produce the
same segment and the same id — regardless of insertion order or host. That is
what makes D-0312's plurality real: many participants can build index segments
from the same crawl observations, cache and replicate them by id, and compare
or merge them without trusting whoever built them. `from_bytes` enforces
canonical form (sorted terms, sorted documents, ascending positions, no
dangling document references), so the bytes↔segment mapping is one-to-one and
a segment id means exactly one thing.

## Decoding untrusted bytes

A segment may arrive from another participant, so `from_bytes` treats its
input as hostile: every count is capped before allocation, `Reader::take`
uses checked arithmetic, and any deviation from canonical form is rejected
rather than accepted into a value whose re-serialization would differ from its
own bytes. Truncation at any offset returns an error and never panics.
