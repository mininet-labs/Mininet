# Mininet product architecture and production gap map

Status date: 2026-07-19

Mininet's product is one sovereign client over one identity, object graph,
local store, trust model, and transport fabric. It is not a menu of unrelated
clones. Feed posts, forum threads, videos, messages, repositories, web results,
and human recommendations must compose through typed links and open in the
same shell without surrendering the user's identity or graph to a host.

This document separates three things that the UI must never blur:

1. a protocol or tested library exists;
2. an end-user workflow is integrated;
3. a capability is production-ready after deployment, abuse, security,
   recovery, and scale gates.

## Day-one product shell

After first-run root creation and optional public-account setup, the primary
navigation should be:

- **Home** — chosen feeds, followed people, communities, media, releases, and
  explainable recommendations in one locally assembled timeline.
- **Inbox** — private conversations, group spaces, requests, attachments,
  receipts, and call history.
- **Calls** — voice/video rooms and direct calls, with transport and encryption
  state visible during every call.
- **Discover** — people, communities, tags, videos, repositories, public web,
  and locally installed search providers, with ranking explanations.
- **Watch** — short video, long-form video, live media, playlists, subscriptions,
  downloads, and user-authorized external catalog adapters.
- **Communities** — Reddit/forum-style spaces, threaded discussion, moderation
  policy, wikis, events, and community-selected views.
- **Forge** — repositories, issues, changes, reviews, governed merge, builds,
  releases, installation, and rollback.
- **Web** — search, reader tabs, saved pages, provenance, local archive, and
  optional crawler/index contribution.
- **Library** — saved objects, downloads, media, files, repositories, offline
  bundles, storage policy, and seeding commitments.
- **People** — contacts, follows, friends, trust context, device/key state,
  blocks, mutes, and safe DID/QR exchange.
- **System** — identity custody, sync/relay health, privacy, permissions,
  resource budgets, updates, diagnostics, and honest readiness status.

Global search, compose, notifications, connection state, identity lock, and
privacy mode belong in the persistent shell. A user should not have to know
which legacy platform inspired a feature before using it.

## Shared backend shape

```text
Windows / mobile / web / CLI clients
                 |
        application services
  inbox | calls | social | media | forge | web/search
                 |
       verified object graph API
 identity | signatures | capabilities | moderation labels
                 |
 local content store + indexes + private envelope mailbox
                 |
 sync scheduler + peer routing + relay/rendezvous + offline bundles
                 |
 LAN | direct Internet | operator relay | content swarm | removable media
```

Every application service must use the same boundaries:

- immutable, typed, content-addressed objects;
- signed mutable heads or convergent operation logs;
- local-first materialized views that can be rebuilt from objects;
- explicit capability checks before signing or privileged actions;
- public metadata only when the feature requires it;
- opaque v2 envelopes for private application metadata;
- transport-independent sync, with no central account database assumed;
- explainable user-selected ranking and filtering;
- adapters isolated from identity keys and the trusted object store.

## Capability maturity

| Product capability | Repository reality now | Next production slice |
|---|---|---|
| Root identity and public account | Windows onboarding, DPAPI vault, profiles and walls integrated | Hardware-backed keys, recovery, multi-device enrollment, installer/signing review |
| Public social feed | Profiles, follows, posts, comments, reactions, communities and local ranking integrated | Blocks/mutes, moderation UI, notification service, scalable local view database |
| Private text | Encrypted v2 persistence, message semantics, checksummed trusted-channel beta invites, route-scoped TCP sync, DPAPI conversation state, and Inbox beta implemented | Authenticated prekeys/session ratchet, mailbox relay, multi-device fanout, provenance UI and background delivery |
| Voice/video calls | No complete call protocol or client | Signed call invitations, authenticated ephemeral sessions, NAT traversal/relay, media engine, group topology |
| YouTube/TikTok-style media | Chunk manifests, publishing and linked social posts exist | Streaming player, transcoding profiles, thumbnails, subscriptions, resumable fetch scheduler, live media |
| Torrent/Stremio-style distribution | Content-addressed chunks and voluntary seeding policy foundations exist | Swarm availability exchange, rarest-first scheduler, integrity-aware streaming, lawful adapter sandbox and catalog UI |
| Reddit-style forums | Community cards, membership, threaded comments, votes/reactions exist | Roles, mod queues, community rules, wikis, reports, durable notification views |
| GitHub replacement | Forge objects, governed merge, build provenance, releases, install and rollback are substantial | Desktop/daemon workflow, arbitrary Git import, issues/projects UI, distributed workers, scale indexes |
| Public web search | URL/search vocabulary and bounded crawler planning only | Fetcher, robots cache, HTML extraction, canonicalization, lexical index, ranker, query service, federated segments, UI |
| Crawlers and indexers | Planner/admission policy only | Sandboxed fetch workers, content extraction, spam/malware pipeline, segment publication and resource accounting |
| Human recommendations | Social graph and signed objects can carry links/reactions | Typed curation lists, provenance, conflict-of-interest labels, reputation views that do not become global identity scores |
| Internet connectivity | Manual direct TCP sync and partial relay/routing primitives | Background sync service, authenticated discovery, NAT traversal, reconnect/backoff, deployable relay operations |
| Censorship resistance | Offline bundles, self-hosting, content addressing and multiple transport foundations | Relay diversity, bridge distribution, traffic-analysis research, pluggable transports, measurable blocking drills |

## Private messaging boundary added in this change

`mini-store` now persists `ObjectEnvelopeV2` bytes by content id and indexes
them only by opaque route. It cannot see or index private author, type,
timestamp, links, or payload fields. `mini-messaging` adds signed encrypted
text/system messages, replies, attachment links, delivery/read receipts,
deterministic conversation scans, signature-verification hooks, and explicit
reporting of rejected envelopes.

This is a real storage, semantics, and manual beta delivery layer, but it is not
yet a production secure-chat product. The desktop beta can transfer a
capability-bearing invite through a trusted channel and sync exactly that route
over foreground encrypted TCP. `ConversationSecret::established` still
deliberately requires an existing opaque route and symmetric key. The next
messaging protocol must provide:

1. authenticated contact/device key discovery;
2. signed one-time and rotating prekeys;
3. asynchronous session setup through a blind mailbox;
4. forward secrecy and post-compromise recovery via a reviewed ratchet;
5. per-device fanout, device removal, and key rotation;
6. group membership epochs and sender-key rotation;
7. replay windows, delivery acknowledgements, spam/request isolation, and
   disappearing-message policy;
8. backup/recovery that cannot silently become server-readable key escrow.

No UI should offer “secure chat” until those properties are implemented and
independently reviewed.

## Backend implementation order

### 1. Make Inbox a complete vertical product

- implement authenticated prekey bundles and a versioned session state machine;
- persist ratchet/session state in the OS-protected vault;
- add relay mailbox put/poll/ack with quotas and request isolation;
- add background delivery, multi-device fanout, notifications, and inbox views;
- integrate the Windows Inbox only after cross-process and adversarial tests.

This work also supplies signed signalling, identity, delivery, and notification
pieces reused by calls.

### 2. Make networking an operated service, not a demo

- introduce a local daemon/service with one durable sync scheduler;
- connect discovery, routing, direct bearer, relay and bridge policy;
- add NAT traversal and relay fallback without leaking private object metadata;
- implement reconnect/backoff, bandwidth/battery budgets, peer scoring limited
  to routing behavior, and observable health without telemetry;
- run multi-host, lossy-network, partition, firewall, and blocking drills.

### 3. Add calls over the messaging/session layer

- signed call offer/answer/end objects with anti-replay and expiration;
- authenticated ephemeral media keys independent of long-term identity keys;
- direct path discovery plus relay fallback and an explicit route indicator;
- audio first, then video/screen sharing, then group calls;
- local permission controls and a hard guarantee that ringing never grants
  microphone/camera access.

### 4. Turn media objects into a streaming network

- versioned rendition manifests, thumbnails, captions, playlists and channels;
- content-aware range planning over existing chunk manifests;
- resumable parallel fetch, integrity checks before decode, and adaptive
  playback based on locally measured bandwidth;
- optional seeding with visible storage/battery/network budgets;
- sandboxed adapters that return catalog metadata and lawful content sources,
  never receive identity secrets, and cannot execute inside the trusted client.

### 5. Complete MiniSearch

- fetch workers respecting admission policy, robots and resource limits;
- deterministic HTML/text extraction and canonical URL handling;
- immutable lexical index segments plus separately versioned ranking profiles;
- query service that merges multiple segments/providers and exposes why every
  result ranked where it did;
- explicit spam, malware, legal and user-filter layers represented as
  availability reasons, never hidden organic-rank manipulation;
- human-curated lists and annotations as signed objects with provenance.

### 6. Expose the forge as an everyday product

- local daemon APIs for repository checkout, issues, proposals and review;
- Git import/mirror automation while preserving Mininet-native signatures;
- build-worker scheduling and artifact availability;
- desktop Forge views that preserve review/approval/merge/release/adoption as
  distinct actions rather than one privileged button.

## Production gates shared by every surface

A feature is not production-ready until it has all applicable gates:

- versioned wire formats, strict bounded decoding and migration rules;
- signature, provenance, authorization, replay and revocation tests;
- encrypted secret storage and memory-lifetime review;
- malformed-input fuzzing and independent cryptographic/security review;
- multi-process, multi-device, lossy-network and long-running soak tests;
- accessible keyboard/screen-reader flows and clear recovery from partial work;
- resource quotas, abuse/report/block paths and safe defaults;
- reproducible signed packaging, rollback and incident response;
- no mandatory telemetry, no third-party analytics SDK, and documented network
  destinations;
- honest in-product maturity labels backed by executable checks where possible.

Resistance to trackers or keyloggers is bounded by the operating system trust
boundary. Mininet can avoid embedding trackers, isolate adapters, minimize
permissions, use hardware-backed keys, sign/reproduce builds, and make network
activity inspectable. It cannot guarantee secrecy on an already-compromised
Windows installation; the UI and documentation must say so plainly.

## Definition of a credible working model

The first credible integrated model is not “every icon exists.” It is two
ordinary Windows installations, behind different networks, that can create and
recover identities, discover each other safely, exchange authenticated private
messages asynchronously, call with relay fallback, follow and sync public
content, stream a published video, participate in one community, clone and
review one repository, search a small independently built web index, survive a
network partition, and export/import all user-owned state without a Mininet
central account. Every unavailable capability must remain visibly unavailable,
not simulated.
