# mini-index-exchange

MiniSearch distributed-search index exchange. Track F2 of the founder's
native-intake / open-web-search research document
(`docs/research/MININET_NATIVE_INTAKE_PUBLIC_COMMONS_AND_OPEN_WEB_SEARCH_20260718.md`,
§29 — "Content-addressed index segments: publish and verify immutable index
segments"), Decision D-0422.

## What this is

Publish and verify immutable index segments across providers **without trust**:

- `SegmentPublication::publish(manifest, &signing_key)` — a provider signs the
  `IndexManifest` of a segment it publishes.
- `SegmentPublication::verify()` — check the signature and name the provider
  (a `ProviderPseudonym` derived from the verifying key).
- `SegmentPublication::verify_segment(&segment)` — additionally check the
  segment's re-derived content address equals the published `segment_id` and
  its shape matches the manifest.
- `accept_published_segment(segment_bytes, publication_bytes)` — the full
  receive path: decode untrusted bytes, verify both legs, return the validated
  segment and its provider, or an error naming the exact failure.

## The trust model

Acceptance rests on two independent checks, both required:

1. **Content address** — an `IndexSegment` (from `mini-lexical-index`) has a
   BLAKE3 `segment_id` over its canonical bytes. A receiver re-derives that id
   from the bytes it was given; a provider cannot attach an id to content it
   did not produce.
2. **Signature** — the manifest is signed, so a third party cannot forge a
   publication in a provider's name.

So "provider P published exactly this segment" is verifiable from bytes alone,
with no trusted registry. That is the mechanism behind D-0312's plurality: many
providers publish index segments built from the same crawl observations, and
anyone caches, replicates, and compares them by id without trusting whoever
sent them.

## What this is not

No network or transport (Track F6 and the existing `mini-bearer`/`mini-sync`).
No storage. No federated query merging (F3), local re-ranking (F4), or provider
payments (F5). Per Directive 16, a publication carries no balance, stake,
weight, or ranking entitlement — it attests *provenance*, never worth; which of
several published segments a searcher uses is a selection choice made
elsewhere. Per Directive 14 / D-0421, no new cryptography: signing and
verification are `mini-crypto`'s existing Ed25519 / ML-DSA-65 primitives.

## Decoding untrusted bytes

A publication and a segment both arrive from an untrusted peer. Decoding is
bounded before allocation, uses checked arithmetic, and rejects trailing bytes,
truncation (at any offset), unknown signature suites, wrong-length key/signature
material, forged signatures, and segment bytes that do not match the published
content address — each with a distinct error, so a caller can tell a forgery
from a mismatch from a malformed frame.
