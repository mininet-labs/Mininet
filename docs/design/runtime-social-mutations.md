# Social mutations through the application runtime

Follow/unfollow and public Likes now join profile and plain-post publication behind the existing
application core. Desktop channel, People, suggestion, connection-card and manual
follow actions send `SetFollow { operation_id, target_did, following }` over typed
IPC. Those actions no longer unlock renderer-held keys or allocate a renderer
sequence. The service requires an existing unlocked identity for a new signature;
there is no automatic identity creation or unlock fallback.

## Recovery and intent binding

The core parses a bounded DID, checks the durable operation receipt, and reserves
one sequence only for a new mutation. `mini_social::build_follow` builds the same
wire object as the legacy `set_follow` API, without inserting it. The journal
persists the exact signed bytes before store insertion. Recovery checks the exact
single FOLLOW object, canonical action/target payload and absence of unexpected
links against the journal input before insertion.

A repeated operation ID with the same target/state returns its original receipt,
including after a locked restart. A conflicting input is refused. Retrying an old
follow after a newer unfollow returns the old object rather than allocating a new
sequence and reviving the relationship. Desktop retries retain their original
operation ID until success; an explicitly different target/state creates a new
intent. Publication and feed events update the existing UI read paths.

Nearby friend-request sending remains a separate existing network operation. Its
endpoint is captured at the user gesture and may start only after successful
follow publication, within ten seconds, with an idle network slot. Delayed or
busy completion leaves the follow saved locally and asks the owner to use
Connections. An IPC response cannot select a newly edited endpoint or trigger a
stale nearby send.

## Compatibility and remaining work

The follow object wire format is unchanged. Existing post/profile journal tags
remain readable; new follow command/kind/input tags are appended. Profiles with
follow receipts require the updated service. An older service cannot interpret
those new records and must not delete them to force a downgrade.

This migration does not make follows private messages, deliver FOLLOW objects via
the exact-post outbox, authenticate the legacy nearby sync path, or remove every
legacy renderer signer. Comments, profile details, media and other
mutations still need their own service migrations. The shared runtime and beta
acceptance register remains in [integrated beta delivery](../planning/integrated-beta-delivery.md).

Focused checks cover legacy wire parity, bounded protocol round trips, feed and
follow state, locked restart receipts, conflicting/invalid inputs without sequence
writes, valid/tampered pending recovery, stable desktop retry intent and stale
nearby-send refusal. Installed-device acceptance remains separate.

## Public Likes

Timeline, media and discussion Like controls now send
`SetLike { operation_id, target_id, active }` to the core. The service signs only
the Like reaction kind in this API. New operations require an existing public POST
or COMMENT target and an unlocked identity; private, missing and unrelated target
types are refused before sequence allocation. Exact signed bytes, target and
active state are journaled before insertion, with the same receipt/restart rules
as follows. A stale successful Like retry cannot allocate a newer sequence and
revive a later removed Like.

The shared reaction builder preserves the legacy wire format and other kinds for
existing library callers. The core command does not expose Save/bookmark markers:
the legacy Save reaction encoding is public and cannot provide private bookmarks.
Renderer Like paths no longer load/unlock keys or allocate sequences. Comments,
profile details, media and other mutations remain separate migrations.
