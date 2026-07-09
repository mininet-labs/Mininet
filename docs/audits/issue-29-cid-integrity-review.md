# Content-address (CID) integrity review

Tracks [roadmap issue #29](../../issues/29)
(Phase 4.1). Scope, per the issue: audit every place content-addressing is
used across `mini-crypto`, `mini-store`, `mini-objects`, and `mini-media`
for integrity edge cases — hash-algorithm downgrade, multihash/multicodec
confusion, and truncation or collision-adjacent inputs.

## 1. `mini-crypto::multihash` — the base encoding

`Multihash::from_bytes` (`crates/mini-crypto/src/multihash.rs`):

- Rejects SHA-1 (`0x11`) explicitly, before anything else — the structural
  enforcement of SPEC-11's frozen strong-hash rule.
- Rejects any code other than BLAKE3 (`0x1e`) or SHA2-256 (`0x12`) — no
  "unknown algorithm, accept anyway" fallback exists.
- Checks the declared length against the *algorithm's* expected digest
  length (`algorithm.digest_len()`), not just against however many bytes
  happen to remain in the input — closes a length-confusion path where a
  short/long digest could otherwise be silently accepted for a given code.
- **Re-encodes the parsed value and compares byte-for-byte against the
  original input** (`parsed.to_bytes() != bytes` -> reject). This closes
  varint-malleability: a non-minimal LEB128 encoding of the same logical
  code/length would decode to the same `Multihash` value but is rejected
  as a distinct, non-canonical byte string — so two different byte
  sequences can never both be accepted as "the" encoding of one hash.

**Verdict: PASS.** No downgrade path (closed algorithm set, no SHA-1), no
multicodec confusion (unknown codes rejected outright), no encoding
malleability (canonical round-trip enforced).

## 2. `mini_objects::ObjectId` — is the id ever trusted instead of recomputed?

`ObjectId::of` (`crates/mini-objects/src/object.rs`) hardcodes
`HashAlgorithm::Blake3` — callers cannot choose a weaker algorithm for an
object id, even though `Multihash` itself is algorithm-generic. There is
no parameter, flag, or code path that lets an object be addressed with
SHA-1 or any other algorithm.

More importantly: **`Object::from_bytes` never trusts a claimed id from
the wire.** The id is not even part of the serialized format read back —
`from_bytes` parses the object's fields, re-serializes them via
`to_bytes()` (`EncodeMode::Full`, deterministic), and computes
`ObjectId::of(&obj.to_bytes())` fresh, every single time an object is
decoded. There is no field in the byte format an attacker could set to
claim a false id — the id is *always* a real function of the actual
parsed content.

**Verdict: PASS.**

## 3. `mini_store::Store` — is content-addressing enforced on read, or only assumed?

`Store::get` (`crates/mini-store/src/store.rs`) fetches raw bytes from the
backend, parses them into an `Object` (which — per §2 — recomputes its own
id from its own content), and explicitly checks
`obj.id().as_str() != id.as_str()` before returning it, rejecting with
`StoreError::Corrupt` on mismatch. This is checked on **every** `get`
call, not once at insert time and then trusted forever — a compromised,
buggy, or malicious backend cannot substitute different content under an
existing id without detection.

**Verdict: PASS.** This is the property the crate's own doc comment
claims ("a backend can never substitute content") and it holds under
direct code inspection, not just by assertion.

## 4. `mini_media` — chunked/large-content assembly

`assemble()` (`crates/mini-media/src/lib.rs`) reassembles a manifest's
chunks and, before returning, checks **both**:
- the assembled length equals `manifest.total_len`, and
- `HashAlgorithm::Blake3.digest(&out) == manifest.digest` (the whole-file
  digest, independent of each chunk's own individual content-addressing
  via `Store::get`).

This closes the truncation concern directly: a chunk silently dropped,
duplicated, or reordered changes either the length or the whole-payload
digest (almost certainly both), and either mismatch aborts assembly with
`MediaError::DigestMismatch` rather than returning corrupted content
silently.

**Verdict: PASS.**

## Summary

| Layer | Concern | Result |
|---|---|---|
| `mini-crypto::multihash` | algorithm downgrade | PASS — closed algorithm set, SHA-1 structurally unreachable |
| `mini-crypto::multihash` | multicodec confusion | PASS — unknown codes rejected outright |
| `mini-crypto::multihash` | encoding malleability | PASS — canonical round-trip enforced |
| `mini-objects::ObjectId` | claimed-vs-actual id | PASS — id always recomputed from parsed content, never trusted from the wire |
| `mini-store::Store` | backend content substitution | PASS — checked on every read |
| `mini-media` | truncation across chunked reassembly | PASS — whole-payload length + digest both checked |

**No integrity gap found in the content-addressing path across these four
crates.** This review is scoped to the CID mechanism's own correctness —
it does not cover provenance/signature verification (a separate concern,
already noted as "the ingest pipeline's job" in `mini-store`'s own doc
comments) or availability (a corrupted/missing chunk is *detected* here,
not *recovered* — recovery is Phase 4's replication/self-healing work,
[issues #30](../../issues/30)/
[#32](../../issues/32)).
