# mini-objects

The unified object model (SPEC-09 §2): one signed, typed, content-addressed
envelope that every surface — feed, forums, media, forge, releases — reads and
writes. A forum comment can link a commit and embed a media manifest with zero
integration work, because there are no per-surface formats. [FREEZE]

**Envelope:** extensible type (well-known core set + custom), author human-root +
signing device (either may be pseudonymous), timestamp/sequence, typed links to
other objects, payload (public or encrypted — the signature always covers the
object; encryption only hides content), device signatures.

**Content-addressed:** an object's id is a strong multihash over its canonical
bytes — tamper-evident, deduplicated, servable by any holder.

**Layered verification:** (1) integrity — id matches bytes, no keys needed;
(2) authenticity — the named device signed it (device KEL); (3) provenance —
the device is a delegated, unrevoked device of the named human holding the
required capability (`POST` for content types).

Decoding untrusted objects is bounded before allocation (payload/link/signature
caps), same hardening standard as `did-mini`.

Mutable state (signed heads, CRDT op-logs, computed feeds — SPEC-09 §3) builds on
top in the next batches (`mini-store`, E2.S2+).

```sh
cargo test -p mini-objects
```

License: CC0-1.0 (public domain).
