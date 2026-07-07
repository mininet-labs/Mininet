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
`pending`, the same honesty convention used everywhere in this tree. Real
BLE/local-Wi-Fi transport and the `MINI/BT0` handshake phases are
`mini-bearer`'s job and remain `pending`; this crate is transport-agnostic
and fully testable offline.

```sh
cargo test -p mini-bootstrap
```

License: CC0-1.0 (public domain).
