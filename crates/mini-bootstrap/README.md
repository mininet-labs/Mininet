# mini-bootstrap

Self-contained bootstrap (`docs/BOOTSTRAP_AND_UPDATE.md`): the pieces that
let a brand-new device with **zero prior state** decide whether to trust,
and then pull, a genesis or update bundle — no internet, no DNS, no app
store, no external service of any kind.

**The shape of an exchange:** a node broadcasts a tiny `GenesisSeed`
(chain id + `PeerCard` + a hash pinning the exact `CapsuleHeader`), a
receiver fetches that small header and checks the hash before committing to
anything larger, `capsule_want_list` says what to pull next (manifest, then
chunks — reusing `mini-media`'s existing Merkle-manifest machinery
unmodified), and `assemble_capsule` reassembles with a full digest check.
Interrupted exchanges resume by idempotence, never restart from zero.

**Honest scope:** `CapsuleHeader` implements the load-bearing structural
piece — self-certifying, chunk-exchangeable, verifiable fully offline. The
fuller genesis-file schema in `docs/BOOTSTRAP_AND_UPDATE.md` (separate
genesis-block hash, invariant-register hash, a release-manifest CID distinct
from the bootstrap bundle, build-recipe hash, initial verifier KEL roots,
rescue-bundle hashes) is not all represented as distinct fields yet — noted
`pending`, the same honesty convention used everywhere in this tree. This
crate itself stays transport-agnostic and fully testable offline — it never
gets its own wire protocol — by design.

### Live two-process demo over real TCP (D-0062, closes roadmap #23)

```sh
cargo run -p mini-bootstrap --example bootstrap_live_demo -- seed 9100      # terminal 1
cargo run -p mini-bootstrap --example bootstrap_live_demo -- fresh 127.0.0.1:9100   # terminal 2
```

A genuinely fresh device (empty store, empty `KelCache`, zero prior trust)
bootstraps a signed genesis capsule from a seed peer over a real socket by
composing already-real pieces: the seed sends its `GenesisSeed` first
(standing in for a BLE advertisement), the two sides handshake a
`mini_bearer::Channel`, then `mini_sync::sync_bidirectional` — ordinary
bucketed set reconciliation — pulls everything (the capsule header, its
bundle manifest, and every chunk are all just `mini_objects::Object`s in
the store). The fresh device reassembles and digest-verifies the bundle,
byte-identical to what the seed peer published — see the example's own
module docs for the full "what this proves, honestly" account.

**Still `pending`:** real BLE/local-Wi-Fi *radio* adapters — the demo above
uses TCP as a stand-in, since real BLE/Wi-Fi needs actual phone hardware
this environment doesn't have (roadmap #22). What real-transport wiring
*can* prove without that hardware — that the protocol pieces genuinely
interoperate over a socket, not just in-process — is now proven.

```sh
cargo test -p mini-bootstrap
```

License: CC0-1.0 (public domain).
