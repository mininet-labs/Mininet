# Connected Mininet: one application for a shared internet

Proposal date: 2026-09-14. Base: `471b25751e4d4d75acf96081d6f00e40f448e7f1`.
Maturity: product specification plus an initial, uncompiled desktop implementation.
This is not a deployed network, completed platform replacement, or Windows release.

## Product direction

Mininet should open into a fast social timeline, with the visual clarity of X:
black background, clear typography, a persistent navigation rail, central
conversation stream, and contextual discovery column on wider windows. Feed
order belongs to the user. System controls belong in settings, not at the
centre of everyday use. The interface must expose real people and content,
never mock activity, fabricated timestamps, or buttons pretending an absent
service is running.

The product combines these workflows through shared services:

| Inspiration | Mininet experience | Required completion beyond this patch |
|---|---|---|
| X | Following timeline, posts, replies, reposts, lists, notifications | Indexed live feed, reposts, lists, notifications, moderation |
| WhatsApp | Private direct/group messaging, attachments, calls | Authenticated session establishment, ratchet, asynchronous mailboxes, device fanout, call engine |
| Instagram | Photos, albums, creator profiles, optional expiring stories | Inline image pipeline, albums, image variants; expiry cannot revoke copies already received |
| TikTok | Vertical short-video stream with user-selected recommendations | Decoder/player, swipe interface, prefetch budget, creator subscriptions, optional autoplay |
| Reddit | Communities, nested discussion, community rules and moderation | Thread UI, reports, mod queues, portable labels and owner-selected filters |
| YouTube | Long videos, channels, playlists, subscriptions, live rooms | Adaptive playback/transcoding, superblock fetch, resumable delivery, live transport |
| Torrents | Content-addressed, verified, resumable multi-peer distribution | Swarm availability, chunk scheduling, provider diversity, quotas, sandboxed optional BitTorrent adapter |
| Google Search | One query across Mininet and optionally the public web | Query service composition, crawl-to-index orchestration, index-peer transport and user-selectable providers |
| Google Maps | Places, local services, maps, routes and saved destinations | Map renderer, replaceable tile/geocoding/routing providers, offline packs, separate location permission |
| Tinder | Separate, opt-in adult dating profile, mutual matching and private chat | Audience separation, age-assurance design, coarse discovery, mutual-consent state, reporting and blocking |
| Business profiles | Organisation pages, services, catalogues, opening hours, enquiries | Organisation/member authorization, structured listings, place links, messaging, portable reviews and exit |

The table describes desired capabilities, not integrations with those companies'
accounts or APIs. Import/adapters would be separate, optional work. No Google,
Meta, ByteDance, X, or other account should be necessary to use native Mininet.

## One graph, distinct audiences

A person can share an object into a community, attach the same media to a
post, save it to their Library, and message a permitted link without uploading
four copies or recreating four identities. Content-addressed storage and
signed provenance remain the common foundation.

A common client must not mean public linkage between every aspect of a life.
The root remains private; delegated devices and scoped personas implement
public social, private conversations, business roles, and dating audiences.
Do not repurpose the current public-profile custom fields as a dating database.
Do not infer dating participation from age or activity. Matching and precise
location must never leak into the public feed, search index, or provider
reputation. Blocking must cover messages, matching, discovery and notifications.

Organisation authority is scoped to the organisation. It never grants human
personhood, validator weight, governance authority or control over a member's
personal identity. Reviews describe transactions or experience, not human worth.

## Connectivity is a service, not a repeated button press

The intended normal experience is: create/import identity, select a transparent
connection policy, then communicate over the internet. Once that policy is
chosen, peers reconnect, content arrives, uploads resume, and messages queue
without asking the user to re-enter addresses for each action. An offline
cache preserves utility during outages; offline isolation is not the product.

The initial implementation only adds an owner-started 15-minute **public**
peer-sync session. It pins the selected endpoint for that session, retries
failed exchanges with capped backoff, stops scheduling when disabled/expired,
and does not persist activation across restarts. It does not auto-sync private
conversation routes. The receiver must already accept successive connections.
No public bootstrap endpoint has been invented or silently selected.

An in-flight exchange can finish after Stop; expiry stops new exchanges, not
necessarily the current socket. Existing transport read/write timeouts are
per I/O, not an end-to-end deadline. DNS resolution also needs an explicit
budget in a future connection service. A successful exchange confirms that
protocol exchange, not global connectivity, the intended peer's identity,
or delivery of every object to every follower.

The deployable successor needs:

1. A single application service that serializes store mutation and signing,
   keeps UI and key custody separate, and emits bounded events to clients.
2. A durable outbox with explicit queued/sent/acknowledged states, replay-safe
   retries, fair scheduling, cancellation, end-to-end deadlines and byte caps.
3. Signed, expiring endpoint advertisements and invite/deep-link onboarding;
   rendezvous hints remain untrusted until provenance is checked.
4. Several independently replaceable relay/rendezvous operators plus direct
   IP paths and NAT traversal. A bootstrap list is replaceable availability
   information, never a trust root or permanent authoritative directory.
5. Opaque, expiring mailbox envelopes for private delivery. A relay may store
   ciphertext; it must not receive conversation keys or the complete social graph.
6. Replicated content availability and chunk retrieval, separate from settlement
   and from governance authority. Peer quantity is not a legitimacy vote.
7. Owner-controlled connection, upload, storage, metered-network and seeding
   budgets. Removal of one provider must leave identity and local state intact.

Before claiming an internet beta, demonstrate two Windows clients behind
**different NATs**, without manually opening inbound ports: discovery, follow,
post, private delivery, media resume, relay outage/failover, block enforcement,
restart recovery and zero-network mode. A loopback success is insufficient.

## Performance design

The existing UI performed feed/profile/reply-count disk work during rendering.
This patch materializes up to 50 timeline cards in one worker, renders cached
results, surfaces errors instead of silently calling them an empty feed, and
refreshes at most once per five seconds. The worker opens a read view directly;
it does not load identity vaults or enumerate sequence state. Theme setup is
performed once per app instance. The existing social query can still scan
history, and other screens retain synchronous work. This is removal of one
specific render-thread bottleneck, not a measured scale claim.

Next: incremental indexed views, cursor-based pages, virtualized rendering,
lazy image decode with bounded caches, background thumbnail generation,
cancellation of stale queries, and one prioritized transport scheduler. Keep
message latency ahead of bulk seeding. Do not eagerly download original video
files to show a feed, run diagnostics during startup, or decrypt keys to browse.

Proposed acceptance budgets, to be measured on a 4 GB RAM, two-core Windows
machine with integrated graphics and 100,000 received objects:

- Warm interactive launch under 2 seconds; cold under 5 seconds.
- Input response p95 under 100 ms; frame p95 under 16.7 ms while scrolling.
- Steady text-feed working set under 250 MB; explicitly budget decoded media.
- Local indexed first-page query p95 under 100 ms, independent of old history.
- Idle CPU under 1%; no media fetching unless the selected policy permits it.
- Interrupted media retrieves missing chunks, not the whole file again.

These are targets, not results. Record the hardware, Windows build, exact
revision, test dataset, network shaping, traces and peak memory for each run.

## Founder alignment and boundaries

FD-01/11: usable on weak hardware; no required payment for ordinary participation.
FD-02/03/18: providers are replaceable; disappearance loses convenience rather
than identity, ownership or the ability to continue the network.
FD-09 and P5/P6: minimize public data; distinct audiences; no compelled seeding.
FD-14: retain the existing Rust core, primitives and object formats; avoid
inventing a replacement crypto or eleven separate application backends.
FD-16/P1: commercial profiles, storage income, ads and balances grant no voice.
U1: connection permission is not permission to download or activate an update.
M1/M2: social reconciliation never makes an offline payment final.

Personalization should be selectable and explainable, with chronological feeds
available. Paid placements, if added, must be labelled and opt-in policy-controlled;
payment must never masquerade as organic support or governance standing.
Creators and resource providers can earn through separately validated value
services. This proposal does not activate payouts or alter Human Share issuance.

FD-18 tests: disappearance retains local identities/data; substitution changes
only the chosen adapter; voice never depends on payment/provider status;
providers receive only task-required data; disabling is local to the user.
This patch itself adds no provider adapter, registry or new dependency.

Rejected shortcuts: embedding eleven third-party sites, hardcoding a single
central account server, fabricating an online feed, treating a TCP connection
as authenticated identity, leaking dating/location data into public profiles,
and claiming existing shared-key beta messaging is production WhatsApp security.

## Implementation sequence and release gates

1. **This proposal:** desktop layout, honest author/time/connection state,
   off-render-thread timeline snapshots, scoped timeline search/media filter,
   expiring public-sync scheduler and regression tests.
2. **Connected beta:** application service, real peer discovery/rendezvous,
   persistent outbox, relay/NAT path, cancellation and store-write coordination.
3. **Everyday social and chat:** moderation/blocking, notifications, profile
   links, ratcheted encrypted messaging, group/device fanout and attachments.
4. **Creator/media and distribution:** image pipeline, player, chunk swarm,
   playlists/subscriptions, mobile bandwidth policy and voluntary seeding.
5. **Discovery and commerce:** distributed native/web search, maps/providers,
   organisation profiles/catalogues and private enquiries.
6. **Dating and calls:** isolated opt-in audiences, mutual matching, adult
   access and abuse controls; authenticated call sessions and relay delivery.

Each step is a working vertical slice with cross-device tests. Native Windows
packaging must follow a verified build; installed binaries, MSI, signing and
release approval are not implied by this source patch. Governance, cryptography
and real-value external audit gates remain unchanged.

## Evidence and review record

Prepared against the merged Windows PR #345 base above. Reversible Tier-O
client proposal; no canonical invariant, protocol format, monetary rule,
release, repository setting or activation record changed.

The governance runtime checker passed with a pre-existing warning that the
bootstrap operating-state verification is older than 30 days. No hardened
trust-before-load claim is made for this session.

Update (2026-09-14, D-0522): applied against the stated base commit in a
Rust-toolchain environment and independently validated — `cargo fmt --all --
--check`, `cargo clippy --all-targets --all-features --workspace -- -D
warnings`, and `cargo test --workspace --all-features` (including this
patch's own `network_session`/`timeline` tests) all pass clean. That
supersedes this section's original "Rust/Cargo/rustfmt/Clippy are absent"
and "added Rust tests have not run" statements below, which describe the
authoring environment only. The native UI has still not been launched or
visually verified, and no Windows binary has been built — Windows QA
remains outstanding. Before merge: Windows UI tests, and review of network
consent and privacy copy.

`git diff --check` and clean-base patch application were the authoring
environment's own local validation, made before the toolchain checks above.

The original authoring environment reported that public branch publication
was rejected by its automatic approval review as an external disclosure
needing explicit authorization. This revision was committed directly by the
repository owner's own agent session, not published externally; no PR or
release is claimed here either.

Rollback is removal/reversion of this client patch; no schema migration, persisted
network activation, new dependency or stored-object rewrite is introduced.
Known limits: a single peer, no deployed relay, no NAT traversal, per-I/O rather
than whole-session deadlines, manual private sync, no video playback, received-
timeline-only search, and unmeasured performance. The next bottleneck is the
connected-beta service in step 2, not adding more navigation labels.
