# mini-sync

Store-and-forward replication: reconcile two `mini-store`s over any bearer,
inside the anonymous encrypted channel. This is what turns the stack into a
network — and it is also the app's own distribution channel (D-0020: the network
is the store).

**Protocol (MINI/SYNC1):** pull-based and strictly alternating (never deadlocks
on half-duplex bearers). Bucketed set reconciliation — only differing buckets
exchange id lists — then batched object transfer. **Resume = idempotence:**
objects are content-addressed, so an interrupted encounter loses nothing; the
next one reconciles what remains (A3 store-and-forward).

**The trust boundary lives here.** Every received object passes verified ingest
before insertion: integrity → KEL carriers absorbed first (identities travel as
ordinary `mini/kel` objects whose embedded logs self-certify; conflicting
histories for a known scid are refused) → full signature + provenance
(delegated, unrevoked, capability-scoped). Unknown authors are **rejected, not
quarantined**: whoever wants you to hold content must hand you the identity that
signed it. Hostile ops cost the sender bandwidth, never your state.

`MINI/PRIVATE-SYNC1` is a separate beta protocol for opaque v2 envelopes. The
caller selects exactly one already-shared `OpaqueRoute`; both peers compare the
route inside the encrypted channel before either sends envelope ids. Only
integrity-valid envelopes naming that route are accepted. Decryption, message
schema checks, and author provenance remain the private application's job.
This is foreground route-capability sync, not a mailbox, prekey service,
ratchet, authenticated endpoint-discovery protocol, or traffic-analysis shield.

```sh
cargo test -p mini-sync
```

License: CC0-1.0 (public domain).
