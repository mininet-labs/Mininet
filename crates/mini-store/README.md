# mini-store

Content-addressed local storage for Mininet objects: blob persistence,
deterministic indexes (author / type / links), SPEC-09 §3 **signed head
pointers**, and want-list helpers that seed sync.

**Trust model, stated plainly:** the store is persistence, not a trust
boundary. Integrity holds by construction (an object's id derives from its
bytes). Signature and provenance verification (`mini-objects` layers 2–3) happen
in the ingest pipeline *before* insertion — the sync layer's job.

**Heads (mutable state without mutation):** a head is a normal signed object
(`ObjectType::HEAD`) whose payload names the subject ("profile", a post id…)
and whose single `"target"` link points at the latest version. Replicas
converge deterministically — highest sequence wins, ties break on greatest
object id — in any arrival order, and a head only ever moves *its own author's*
slot.

**Backends:** `MemoryBackend` (tests), `FsBackend` (atomic tmp+rename, fanout
dirs, path-traversal-hardened keys). A SQLite backend slots in behind the same
`Backend` trait at integration (D-0020), changing nothing above it.

```sh
cargo test -p mini-store
```

License: CC0-1.0 (public domain).
