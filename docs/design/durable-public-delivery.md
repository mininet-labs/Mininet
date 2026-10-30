# Durable public delivery through the application service

Status: implemented backend slice in [PR #379](https://github.com/mininet-labs/mininet/pull/379), subject to review and integration. This is a bounded public-post delivery path, not a complete integrated beta. See the [delivery program](../planning/integrated-beta-delivery.md).

## Authority and owner workflow

`mininet-app-service --stdio` hosts the per-user `mini-app-service::Core`. Versioned, bounded `mini-app-protocol` requests cross stdin/stdout; the service owns signing, storage, peer configuration and the journal under one writer lock. The renderer does not receive seed material or a signing controller. Its stdout is IPC, not an interactive shell. `MININET_HOME/objects` selects storage when `MININET_HOME` is set; otherwise the host uses its platform data-root fallback.

The existing IPC identity/profile/post/feed commands remain available. Delivery adds this explicit sequence:

1. Unlock an existing owner identity. `ExportDeliveryIdentity` returns only public root and device KEL bytes. Exchange these public pairing records through a separately trusted channel and verify the intended person/device; this protocol does not discover or authenticate that external introduction.
2. `AuthorizeDeliveryPeer` records an owner-chosen label, numeric TCP endpoint and the verified root/device KELs. Labels cannot switch DIDs, roll back KEL history or accept a divergent history. Each root DID has exactly one retained peer label/device: aliases and additional devices for that root are refused, including disabled labels, so an alias cannot bypass a learned revocation. `DeliveryPeers` inspects grants; `RevokeDeliveryPeer` retains a disabled record. An authentic extending root update revoking the trusted device persists a disabled grant even if the accompanying device bytes or endpoint are invalid; it retains the last trusted device history and endpoint.
3. Publish a local public text post, then `QueuePublicDelivery` with a unique operation ID, peer label and exact existing object ID. Queuing selects one own signed public POST without links; it never selects a directory, private envelope or an entire store. Queuing and dialing both require a current enabled peer authorization.
4. Networking defaults off. `SetDeliveryEnabled` explicitly persists the owner's network grant. For incoming traffic, `SetDeliveryListener` selects a numeric bind address and exactly one authorized expected peer. The listener accepts only that peer's authenticated root/device identity. Configuring a listener alone does not enable networking. `StopDeliveryListener` removes that listener configuration; disabling delivery closes the active listener and prevents dialing.
5. While enabled and unlocked, the host schedules delivery between IPC turns. `AttemptDelivery` can explicitly attempt one eligible operation. Inspect `DeliveryStatus`, `ListDeliveriesAfter` (bounded cursor pages), `ListDeliveries` (first page) and `DeliveryRuntimeStatus`; `CancelDelivery` prevents further attempts for a non-delivered operation.

Enabling delivery requires an unlocked identity. The persisted grant and listener configuration survive restart, but restart leaves signing locked: neither listening nor dialing resumes until the owner unlocks. The grant can be disabled while locked. Public pairing and peer labels do not grant authority for money, Forge approval or governance.

## Exact bytes and acknowledgements

The durable journal stores the original signed object bytes, ID, peer label, state, attempt count, next-attempt time and coarse failure reason. Reusing an operation ID with identical content returns its existing record; conflicting reuse fails. Beginning an attempt is durable before network I/O. A restart converts interrupted in-flight attempts to immediately eligible retry records without signing or republishing the object.

Each connection establishes existing CH1 encryption, then transport-security `StateSync` channel-bound mutual identity authentication against the locally authorized KELs. Application payloads use a separate delivery domain and protocol magic. There is no anonymous fallback, discovery exchange or store enumeration.

The receiver validates canonical bytes, public unlinked POST shape and text bounds, exact author root/device, integrity and delegated signature provenance before insertion. It inserts durably, rereads the exact content-addressed bytes, then sends an encrypted acknowledgement containing the exact object ID. The sender records `Delivered` only after that authenticated matching acknowledgement. A missing or wrong acknowledgement remains a failure even if the receiver already persisted the object. Retrying inserts the same content-addressed object; it does not create a second publication.

`Delivered` means this authenticated receiver acknowledged durable storage. It does not establish that a person read the post, that every replica has it, or that the receiver will retain it forever. Locally trusted KEL continuity and channel freshness do not prove globally freshest revocation knowledge.

## Bounds and scheduling

| Resource | Bound / behavior |
| --- | --- |
| Exact signed object bytes | 128 KiB maximum; public plain-post validation may impose a smaller text bound |
| Retained operations | 1,024 total, including delivered/cancelled receipts; no automatic receipt eviction |
| Peer book | 64 retained labels, including disabled tombstones |
| TCP dial | 5-second timeout |
| Established session | 5-second absolute deadline across handshake, authentication, payload and acknowledgement; remaining timeout reset per syscall to resist slow dribbling |
| Framing | 160 KiB maximum frame; 512 KiB combined session I/O budget, checked before allocation |
| Host scheduling | At most one incoming session and one outgoing attempt per 250 ms scheduling turn; the next turn starts after current work completes |
| Retry | Exponential delay from 1 second, capped at 5 minutes; counters/timestamps saturate |
| Idle work | Bounded in-memory due metadata; idle scheduling does not reread every queued payload |

Network work runs on the service authority thread and can delay IPC within these bounds: up to one incoming session plus one dial/outgoing session in a turn. This is deliberately limited concurrency, not a responsive high-throughput network daemon. One stalled connection cannot block indefinitely. Durable-write ambiguity poisons outbox scheduling until reopen rather than trusting possibly stale in-memory state.

Journal and peer records use bounded strict decoding and atomic replacement. Paths reject symlinks and traversal, and only recognized bounded orphan temporary files are cleaned after committed records validate. This assumes the service's single-writer ownership and an owner-controlled local store; it is not a cross-process transaction or a defense against a privileged process rewriting files concurrently.

## Remaining integration limits

Peers need reachable numeric TCP endpoints. This slice supplies no NAT traversal, relay fallback, address discovery, transport migration or automatic KEL refresh. Unreachable peers wait and retry. The single listener configuration serves one expected peer, not an open multi-contact inbox. Revocation stops future authorized attempts but cannot retract data already delivered or bytes already transmitted in an active session.

Private sessions, private messages, calls, media attachment delivery, full-store sync, background service installation and wallet actions are outside this path. Desktop controls, packaging, two-device usability and broader acceptance remain separate work. Existing native Forge migration work in [PR #378](https://github.com/mininet-labs/mininet/pull/378) is independent; public-post delivery neither synchronizes code nor approves or merges proposals.
