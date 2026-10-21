# Integrated beta delivery program

The goal is an installed Mininet application whose identity, contacts, public feed, private conversations, calls, search and Resident AI work together through a shared runtime. Compiling the component crates is insufficient evidence of that experience. Each area below needs an owner-visible journey on installed devices, truthful errors, bounded resources and restart/revocation behavior.

[PR #379](https://github.com/mininet-labs/mininet/pull/379) delivers the first bounded runtime/public-outbox backend slice. It is not an integrated-beta readiness declaration. The [public-delivery design](../design/durable-public-delivery.md) records its exact IPC workflow, security properties, resource limits and remaining gaps. Desktop controls and bounded pipelined service IPC are implemented in stacked proposals; installed-device validation remains a separate checkpoint.

## Acceptance matrix

| Area | Implemented in this slice | Acceptance still outstanding |
| --- | --- | --- |
| **A — Shared application runtime** | Single-writer application-service Core; bounded typed stdio IPC; identity/profile/post/feed and journaled follow/unfollow public Like and comment commands; durable exact-public-post outbox, peer grants and coarse delivery events. | Follow/unfollow, Like and comment desktop surfaces now use this authority; route the remaining everyday mutations through it; remove competing writers; verify reconnect, cancellation, migrations and error recovery across installed app/service boundaries. |
| **B — Everyday reachability** | Owner-authorized numeric TCP peers; channel-bound mutual authentication; explicit default-off network grant; one expected-peer listener; bounded dial/session deadlines and retries. | Authenticated discovery/rendezvous, direct and relay routes, NAT handling, reconnect and endpoint updates; demonstrate ordinary devices finding each other without manual reachable-IP setup. |
| **C — Secure asynchronous messaging** | Public text-post delivery only. Durable acknowledgements and retry identity are exercised; these do not constitute a private messaging protocol. | Prekeys, ratchets, multi-device behavior, revocation and mailbox delivery; verify confidentiality, offline/reordered delivery and recovery without silent downgrade or duplicate messages. |
| **D — Calling** | No calling implementation or activation in this slice. | Establish authenticated 1:1 audio first, with explicit microphone permission, incoming-call controls, device selection, interruption/reconnect and usable teardown. |
| **E — Operated local / Mininet / public-web search** | No search orchestration added by the outbox. Existing search primitives remain independent. | Operate all three scopes with clear source/provenance, consent for external requests, bounded retrieval, cancellation and honest unavailable/no-result states. |
| **F — Resident AI** | No model setup, retrieval, memory or action permissions changed here. | Integrate permission-aware retrieval and memory, usable model setup and reviewable actions; preserve explicit owner approval for writes and external effects. Verify permissions across restarts and surfaces. |
| **G — Content delivery and cross-surface safety** | One exact own signed public unlinked POST; provenance/shape checks before receiver insertion; exact-ID authenticated acknowledgement after durable reread; bounded retained journal and peer records. | Broader files/media/attachments, resumable retrieval and presentation; maintain private/public separation and untrusted-content handling in feeds, search, previews and AI context. Exercise malicious and interrupted content journeys across surfaces. |
| **H — Installed-device lifecycle** | Service entrypoint, persistent policy/peer/journal records, single-writer lock and locked restart semantics; interrupted attempts recover without re-signing. | Install/update/uninstall, service/desktop launch supervision, secure key-store behavior, migrations, diagnostics, recovery and two-device upgrade/restart tests on supported platforms. |

## Current public-post journey

The backend supports: create/unlock the owner identity through IPC, publish a local profile/post, explicitly export public root/device KELs for pairing, authorize a peer endpoint, queue one exact public post and explicitly enable networking. The recipient configures its listener for that expected peer. The host performs bounded idle scheduling; the receiver verifies provenance, stores and rereads the object before acknowledging its exact ID. Missing acknowledgements remain retryable even when insertion succeeded. Receipt state and exact signed bytes survive locked restart; no network activity resumes until unlock under the persisted grant.

This journey requires reachable numeric TCP addresses and external owner verification of pairing material. It provides no NAT/relay route, private session, call, full-store transfer, wallet operation or governance activation. Delivery means authenticated durable-storage acknowledgement, not a read receipt or global KEL freshness.

## Checkpoints and evidence

1. **Backend authority and durable delivery — PR #379:** focused protocol/Core/journal/peer/transport tests cover restart, identity binding, malformed input, retained bounds, revocation, lost/wrong acknowledgements and exact-object retries. Review the exact PR head and CI results before treating the backend as integrated.
2. **Desktop public-delivery controls — implemented proposal:** pairing, peer authorization, pending/retrying/delivered status, the explicit network grant, expected-peer listener, reviewed queuing and paginated receipts use service IPC without raw seeds. Focused codec, request-bound and stable-retry checks pass. Still demonstrate the complete journey between two running application instances, including disable, lock, restart and unreachable-peer recovery.
3. **Desktop IPC responsiveness — implemented proposal:** controls can be forwarded before prior replies; bounded control reserves, exact response correlation, uncertain failure handling and child supervision have focused checks. Installed Windows behavior and full application teardown still need device acceptance.
4. **Installed-device public journey:** package and supervise the service; test two separate devices on supported platforms, persistent permissions, bounded UI delays and failure recovery. This is still narrower than the A–H beta.
5. **Integrated beta:** close the outstanding acceptance in each matrix row, record executable journey evidence and known limitations, then make a separate release-readiness decision. Crate compilation and loopback success alone do not close these gates.

Native coding collaboration remains a parallel contributor program. [PR #378](https://github.com/mininet-labs/mininet/pull/378) covers reviewed workspace snapshots and native commit/proposal handoff; it does not implement the messaging/calling/reachability beta. [PR #377](https://github.com/mininet-labs/mininet/pull/377) supplies contributor welcome material. Neither proposal automatically authorizes code adoption, approvals, merges or governance changes.

The social runtime slices migrate desktop follow/unfollow, Like and comment signing to the
core, with durable intent-bound receipts and no renderer-key fallback. See
[social mutation recovery](../design/runtime-social-mutations.md). This is progress
on A and H; profile details, media and other signers plus the broader journeys remain open.
