# Social mutations through the application runtime

Follow/unfollow, public Likes and comments now join profile and plain-post publication behind the existing
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

This migration does not make follows private messages, provide private FOLLOW delivery, authenticate the legacy nearby sync path, or remove every
legacy renderer signer. Profile details, media and other
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
Renderer Like paths no longer load/unlock keys or allocate sequences.
Profile details, media and other mutations remain separate migrations.

## Comments and community topics

All four desktop comment paths—timeline replies, media comments, community topics
and discussion replies—send `PublishComment { operation_id, parent_id, text }`
to the core. New signatures require an unlocked identity, nonblank UTF-8 text
bounded to 16 KiB and an existing public parent. Public community objects are
valid parents as well as posts and comments. Missing or private parents are refused
before reserving a sequence.

The store-free comment builder preserves the existing single `re` link and
length-prefixed text encoding. Recovery verifies exact parent, text, object type
and canonical shape against the journal before insertion. Matching completed
receipts remain available while locked or after the parent cache is removed;
conflicting reuse of an operation ID is refused.

Desktop retries retain the exact parent/text operation ID. Editors remain intact
while publication is pending. A successful receipt clears a draft only when its
parent and original raw fields still match, preserving edits or a newly selected
thread made while waiting. These objects are saved locally; a separate reviewed public-delivery action can
queue an exact COMMENT through the authenticated outbox. Updated journal tags require the
updated service, and no private messaging protocol is implied.

## Community membership

Community join/leave controls use `SetMembership { operation_id, community_id,
joined }` through the same signing authority. A new request requires an unlocked
identity and a locally available, valid public COMMUNITY card before sequence
reservation. Joining is available for Open communities. ApprovalRequired joins
are refused and shown as unavailable in the desktop until an admission protocol
exists; a signed declaration alone is not proof of admission. Leaving remains
available for either mode.

Membership objects preserve the legacy public boolean payload and single
`community` link. Exact input-bound journal recovery and completed receipts follow
the same rules as other social mutations, including locked receipt retries after
parent-cache removal. Desktop retries reuse only the same community/state intent;
there is no renderer key unlock or sequence allocation. A stale join receipt
cannot create a newer join after a leave. Admission decisions,
private groups remain separate work. Exact public membership objects can be
queued separately through reviewed delivery; this does not establish admission.

## Open community creation

Desktop creation sends `PublishCommunity { operation_id, name, charter }` to the
core. This command explicitly creates Open communities, matching the previous
creation form. It does not offer an approval mode without an admission workflow.
Name and charter are bounded to 96 and 4096 UTF-8 bytes; a new signature requires
an unlocked identity and a nonblank name before sequence reservation.

The store-free community builder preserves the existing wire format, including
both membership modes for legacy library callers. Canonical decoding rejects
unexpected links, trailing data and private payloads. The journal binds exact
name, charter, Open mode and signed object bytes before insertion. Matching
operation receipts work while locked; conflicting reuse is rejected.

The desktop retains its submitted name/charter operation ID after uncertain
responses. It clears the form only when both raw fields still match the submitted
snapshot, preserving changes made while waiting. Creation has no renderer key
unlock fallback. Community cards remain local until exchanged or explicitly queued through reviewed
public delivery; this does not add admission governance or private groups.

## Full-size comment recovery

Comment decoding and threaded reads use the declared 16 KiB comment bound rather
than the unrelated shared 4 KiB string helper. Previously a canonical comment
above 4096 bytes could be signed and journaled, then refused at commit/recovery.
The corrected decoder accepts its exact length-prefixed UTF-8 bytes, without
raising other profile/community string limits. Existing valid pending large
comments can therefore recover without changing their signatures or operation IDs.
Boundary checks cover 4096/4097 bytes, the full limit, multibyte text and overflow.
