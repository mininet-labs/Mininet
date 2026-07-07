# mini-social

The personal social layer: profiles, public walls, the follow graph, and
feeds — all pure functions over a `mini-store`, offline-first.

**Public walls are first-class (founder decision, 2026-07-07).** A
`PublicWall` is a voluntary public identity surface, published under whatever
DID the user chooses — their primary root or an independent pseudonym root.
It never carries a human-root field, never requires or implies a governance
vote (publishing one needs only ordinary `POST` capability), and never
registers a new "human" anywhere. The *only* way to bind a wall to another
DID is an explicit, self-signed `publish_wall_linkage` call — absent by
default. One human may run many unlinkable walls this way; see
`crates/mini-social/src/wall.rs` and its tests.

**The feed is a locally computed view [FREEZE].** It is not a stored object and
not a server's opinion: it is computed on the reader's device from what their
overlay has seen. Ranking is a **user-chosen filter** passed explicitly —
never a hidden algorithm — filters are total orderings (they reorder, never
silently drop followed speech), and every item carries a `FeedReason`, so
"why am I seeing this" is always answerable.

**Profiles** resolve through signed head pointers (edits converge by LWW on
every replica). **Follows** are ordinary signed objects with a state byte;
per (follower, target) the latest wins by `(sequence, id)` — the same
convergence rule as everywhere. The public graph is derivable by anyone from
public objects; pseudonymous/private graphs arrive with pairwise identifiers
(SPEC-01 §10) and are noted honestly, not promised early.

```sh
cargo test -p mini-social
```

License: CC0-1.0 (public domain).
