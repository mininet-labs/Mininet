# mini-storage

Mutually-signed storage-served receipts for Mininet: proof that one
verified identity root's device served content to another, offline-
verifiable — closes the gap `mini-reward`/`mini-store` flagged as `pending`:
the receipt-signing/verification pipeline connecting
`mini-store::CacheTier::CommittedStorage` to a real, witnessed
`mini_reward::accrue_storage` input.

Mirrors `mini-presence`'s proven pattern exactly. Two delegated,
`ATTEST`-capable devices — a host and a witness — sign one deterministic
transcript binding: the content id, bytes served, a digest of what the
witness actually received, both device ids, fresh nonces, and a timestamp.
`verify_serve` requires, for both sides: the device is currently delegated,
unrevoked, and `ATTEST`-capable; the signature verifies; nonces are fresh
and non-replayed; the receipt is within a freshness policy; and the two
identity roots are distinct (a host cannot witness, and be rewarded for, its
own storage). The resulting `ServeVerdict` feeds `mini_reward::accrue_storage`
directly — the same relationship `mini_presence::PresenceVerdict` has to
`mini_reward::accrue`.

**Honest limit:** a receipt proves a serve *happened once*, at a point in
time. It does not prove the host keeps serving that content tomorrow —
durable storage-over-time needs a harder property (challenge-response
proof-of-storage), which remains `pending`, the same honest limit
`mini-presence` states for its distance-bounding. Automatic receipt emission
as a side effect of a real `mini-sync` exchange is also `pending` — this
crate verifies receipts, it does not yet produce them automatically.

```sh
cargo test -p mini-storage
```

Freshness is bounded in both directions: the default accepts receipts up to one
day old and at most five minutes ahead of the verifier's clock. Rejecting
far-future timestamps prevents a forged clock from making a signed receipt stay
fresh indefinitely after durable replay state is lost.

License: CC0-1.0 (public domain).
