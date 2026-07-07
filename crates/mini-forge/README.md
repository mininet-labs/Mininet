# mini-forge

The forge core (SPEC-11): the groundwork for building Mininet **from inside
Mininet**.

**Repos are objects.** Files, nested trees, and commits are signed,
content-addressed objects; trees link their entries, so `mini-sync`'s
want-list pulls a whole repository from one commit id — no hosting service.
Branches are signed head pointers (LWW convergence). History is
content-addressed: old commits stay checkout-able forever.

**Releases encode the guarantees.** `verify_release_artifact_only` checks the
artifact (it is **not** adoption-safe on its own); `verify_governed_release`
is the only adoption gate, adding the governed source-lineage bind (D-0030). The
artifact checks are: independent reproducible-build attestations counted per
**verified identity root**
(many devices count once, the author never counts, balances appear nowhere —
money never buys release authority, P1/SPEC-11 [FREEZE]); a **timelock**
(time to inspect, object, fork); and a **complete, digest-checked artifact**
carried as a `mini-media` manifest. The module only verifies — no execution,
no remote trigger, no path by which anyone can push code onto a device
[FREEZE: no forced update, no kill path]. Until `mini-chain` lands, this
attestation rule is the labeled-provisional quorum; the chain replaces the
counting, not the objects.

**Protocol-repo approval floor (founder decision, 2026-07-07, D-0033).**
`PROTOCOL_MIN_APPROVALS` is currently `2`: `valid_policy_for_protocol_repo`
rejects any policy for a protocol-critical repo requiring fewer than 2
maintainer approvals — no 1-of-1 canonical merge path — mirroring the
existing `ADOPTION_MIN_ATTESTATIONS` release-attestation floor. This is a
floor, not the final design: it upgrades to personhood-root quorum once
SPEC-02 lands.

Next batches: PR/review objects on `mini-crdt`, merge-as-governance quorum,
git SHA-256 interop.

License: CC0-1.0 (public domain).
