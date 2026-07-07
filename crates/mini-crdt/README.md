# mini-crdt

Mininet's own op-log CRDT (SPEC-09 §3): multi-author mutable state as append-only
logs of **signed operations** that merge conflict-free and offline-first. Threads,
forum discussions, shared docs, and forge PR conversations all run on this one
machinery.

Ops are ordinary signed `CRDT_OP` objects — signing, provenance, storage, and
sync come from the existing layers. Three operations: **Add** (a node under a
parent), **Edit**, **Tombstone** (a retraction, honestly not an erasure).

**Why we own it — one-human authorship:** edit/tombstone authority belongs to the
node's *human*, not a device: your phone may edit what your laptop wrote; nobody
else's device may. Moderation acts through filters/labels (SPEC-10), never by
rewriting someone's words.

**Convergence by construction:** `replay` folds over the op *set* with
order-independent rules (Adds = membership, Edits = per-node LWW by
`(sequence, id)`, Tombstones = monotone), so every replica derives identical
state in any arrival order — proven in tests across all 24 permutations of a
two-human thread. Invalid ops are deterministically excluded and reported, never
fatal; orphans are `pending` until their parent syncs.

```sh
cargo test -p mini-crdt
```

License: CC0-1.0 (public domain).
