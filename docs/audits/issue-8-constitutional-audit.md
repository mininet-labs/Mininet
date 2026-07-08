# Constitutional audit

Tracks [roadmap issue #8](https://github.com/britak420/Mininet/issues/8)
(Phase 0.1). Scope, per the issue: cross-check every Specification/major
subsystem against the Constitution, produce a PASS/PARTIAL/FAIL matrix,
and document every centralization vector and trust assumption — even
latent or theoretical ones, not just currently-exploitable ones.

## Methodology and an important distinction

`docs/INVARIANTS.md` already tracks each Tier-F frozen invariant's
**Enforced by** status (✅ / partial / pending) as of the current tree.
This audit builds on that table rather than duplicating it, adding two
things INVARIANTS.md deliberately doesn't carry: a **centralization
vector** (what could this become, if something here were subtly wrong?)
and a **trust assumption** (who or what has to behave honestly for the
guarantee to hold?) per row.

**A category this audit does not use: "FAIL."** No invariant in this
tree is *violated* by existing code — that would be a critical bug, not
an audit finding, and this pass found none. The real three-way split is:

- **PASS** — provably enforced in code today, with tests exercising the
  adversarial case, not just the happy path.
- **PARTIAL** — enforced for the pieces that exist; the guarantee has a
  real gap where a not-yet-built piece (usually the networked chain) would
  need to extend it.
- **NOT YET BUILT** — no code path exists yet that could violate this
  invariant, because the subsystem it governs doesn't exist yet. This is
  not a violation, but it's also not a positive guarantee — flagged
  separately so "nothing to check yet" is never conflated with "checked
  and safe."

## The matrix

| Invariant | Result | Centralization vector | Trust assumption |
|---|---|---|---|
| P1 — no balance maps to vote/validator weight | PASS | If a future "reputation" or "stake-like" field were ever added to `ValidatorSet` or governance counting without this audit's scrutiny, it could reintroduce plutocracy through the back door | Reviewers of every future PR touching `ValidatorSet`/`governance` must keep enforcing "no weight field," since the guarantee is structural (no field to weight) rather than a runtime check that could itself be bypassed |
| P2 — one human, one equal vote | PASS (for what's built) | A future personhood implementation that allows one human to acquire two distinct "verified" identity roots would let that human vote twice — this is precisely why Phase 2 (#17-#21) exists | Depends entirely on Sybil-resistance actually holding (see #18); today's guarantee is "one identity root, one vote," not yet "one human, one vote," and the codebase is honest about that distinction everywhere it matters |
| P3 — no owner/admin key, no off switch | PARTIAL | The genesis and release pipeline (still `pending`) is exactly where an off-switch or admin key could be smuggled in if built carelessly — `mini-update::AdoptionState` already refuses to let a stale decision be trusted, which is the right instinct to carry forward | The eventual release-registry chain code must be reviewed at least as carefully as identity code; CC0 licensing removes the *legal* off-switch today, code removes the *technical* one only once the chain lands |
| P4 — slow, presence-conditioned vesting | PASS (for what's built) | A future change to `mini-reward`'s rate cap or maturation delay that "temporarily" relaxes it for one cohort would be exactly the kind of subtle regression Directive 10 warns about ("invisible complexity eventually becomes invisible centralization") | `mini-reward`'s accrual math is trusted to run identically for every identity root — no privileged fast lane exists in the code today, and none should ever be added |
| P5 — no raw personal data required | PASS (for what's built) | The eventual ZK personhood attestation (Phase 2, especially #21) is the highest-risk future addition — a construction that *claims* zero-knowledge but leaks correlatable metadata would violate this invariant while looking compliant | Cryptographic soundness of whatever ZK construction eventually ships; today's channel/identity layer already avoids this by design, not by promise (Directive 9) |
| P6 — no forced replication, no compelled decryption | NOT YET BUILT | The storage fabric doesn't exist yet, so there's no code to audit — but this is exactly the invariant a naive "make storage efficient" optimization could erode first, since forced replication *looks* like a reasonable reliability feature | Whoever builds the storage fabric (Phase 4) must treat this as a hard constraint from the first line of code, not a retrofit |
| Crypto-agility (no hard-wired algorithm) | PASS | If a future crate bypassed `mini_crypto::suite`/`agreement`/`aead`/`kdf` and called a raw crypto primitive directly "for performance," that one shortcut would recreate exactly the hard-wired-forever risk this exists to prevent | Every future crypto-touching crate continues routing through these versioned-suite modules rather than a direct dependency call |
| Strong-hash content addressing (no SHA-1) | PASS | None found — the enforcement is structural (no `Sha1` variant exists in the enum at all), not a runtime check that could be individually bypassed | None beyond "the enum stays small" — verified directly in this audit's companion CID review, [issue #29](https://github.com/britak420/Mininet/issues/29) |
| Keys never leave the device | PASS | A future convenience feature ("back up your key to the cloud") would be the classic way this gets violated — worth flagging explicitly as a permanently-rejected feature class per Directive 9 | Every future feature request that sounds like "just export the key for X" needs to be rejected by design, not by policy |
| Self-certifying identifier, no central registry | PASS | None structural; a future "identity lookup service" convenience layer could *feel* like a registry without technically being one — worth watching for at the UX layer, not just the protocol layer | `did-mini`'s SCID re-derivation math; no external party needs to be trusted to validate an identity |
| Pre-rotation protection | PASS (for what's built) | On-chain anchoring is still pending — until it lands, a sufficiently patient attacker who compromises an old (but not yet rotated-away) key still has a narrow window; this is a known, documented gap, not a hidden one | The reveal-based pre-rotation check in `did-mini::Kel::verify`; anchoring will add a second, independent trust layer once built |
| Many devices, provably one human | PASS | A delegation bug that let a device claim membership in an identity root without that root's explicit seal would let an attacker attach unauthorized devices | Both directions of the mutual commitment (`did-mini::verify_delegation`) hold: the device claims the root, and the root must have sealed the device — neither alone is sufficient, by test |
| Range-bound, mutually-signed co-presence | PARTIAL | The "software RTT bound, no ranging radio" gap (`mini-presence`) is precisely where a relay attack lives today — [issue #17](https://github.com/britak420/Mininet/issues/17) is the dedicated follow-up | Trusts that the software timing bound is hard enough to beat in practice; UWB hardware ranging (already scaffolded) closes this further once deployed |
| Bootstrap/update independent of external services | PARTIAL | The release registry (on-chain) is the missing piece; until then, "no external services" is true for genesis/capsule mechanics but the *governed release* half still needs the chain | Trusts `mini-forge`'s release verification gates once the registry exists; today, `mini-bootstrap`/`mini-update` already don't depend on any specific external service |
| BLE/local exchange works with no internet | PARTIAL | Real BLE/Wi-Fi transport is the gap ([issue #22](https://github.com/britak420/Mininet/issues/22)) — until it lands, "works with no internet" is proven at the protocol-logic level (D-0042's `TcpBearer` demo) but not yet on real radio hardware | Trusts the eventual BLE adapter to correctly implement the same `Bearer` trait `TcpBearer` already validates the contract of |
| No forced auto-update | PASS | A future "critical security patch" fast-path (see [issue #53](https://github.com/britak420/Mininet/issues/53)) is exactly where this could be quietly weakened "for the user's own good" | `mini-update::AdoptionState`'s `evaluate`/`adopt`/`refuse` split — refusal is a first-class, unblocked outcome, verified by test |
| Malformed-input rejection in identity decoders | PASS | None found — every decoder path caps size and validates structure before any verification logic runs | Decoder code itself; no external validator needs to be trusted |
| Weak/ambiguous peer input rejection in the encrypted channel | PASS | None found — small-order handshakes, all-zero shared secrets, and oversized frames are all rejected before crypto operations proceed | `mini-crypto::agreement`/`aead`, `mini-bearer::Channel` |
| Public walls create no privilege | PASS | A future "verified wall" badge feature that quietly required `VOTE` capability to obtain would violate this — worth flagging as a permanently-rejected feature shape | `mini-social::PublicWall` requires only `POST`; test asserts a wall publish never needs or implies `VOTE` |
| Base devices create no governance weight | PASS | A future "primary device" concept that accumulates trust/weight over time would violate this even if unintentional | `did-mini::BaseDeviceRole` carries no `Capabilities` bit structurally |
| Storage/seeding earns value, never voice | PASS (for what's built) | A future reward-for-storage mechanism that also nudged governance weight (e.g. "top storers get a proposal fast-track") would violate this even if framed as a UX improvement, not a voting change | `mini-storage`/`mini-reward`'s accrual math; durable storage-over-time proof is still a gap this audit's companion Phase 4 issues track |
| Seed-on-view is user-controlled | PASS | None found — policy gates exist at multiple layers (device role, battery, connection metering) and encrypted content is structurally capped below `CacheTier::PrivateOnly` | `mini-store::Store::note_view` and `CacheTier` |
| Radio/LoRa permanently out of scope | PASS | None — this is a closed question (D-0033) enforced by absence: no radio/LoRa bearer exists or is planned anywhere in the tree | Documentation + the Failure Book entry recording why this was closed |
| Rust as the implementation language | PASS | None — every workspace member is Rust; this is trivially auditable from `Cargo.toml` | None |
| Equal-weight BFT finality | PARTIAL | The single largest unbuilt piece in the whole constitutional picture: networked consensus (proposer rotation, gossip, view-change) doesn't exist yet, only finality *verification* given already-valid votes does | Trusts the eventual networked-consensus implementation to preserve the equal-weight property `verify_finality` already assumes and checks for; see Phase 5 (#36-#45) |
| AI may draft, human review mandatory | PARTIAL | A future "AI-approved" auto-merge path, even for low-risk changes, would be exactly the kind of erosion Directive 12 exists to prevent | `PROTOCOL_MIN_APPROVALS`'s 2-human floor for protocol-critical repos; a dedicated "this PR was AI-assisted" flag is still `pending`, tracked at [#78](https://github.com/britak420/Mininet/issues/78) |

## Aggregate result

Of 26 rows: **18 PASS**, **7 PARTIAL**, **1 NOT YET BUILT**, **0 FAIL**.

Every PARTIAL result traces to the same root cause: **the networked chain
and storage fabric are the two largest pieces of this system that don't
exist yet**, and every constitutional guarantee that ultimately depends on
consensus or durable storage is honestly marked partial rather than
falsely marked complete. This matches the roadmap's own Phase 5
(Consensus) and Phase 4 (Storage) being the largest phases by issue count.

**No hidden centralization vector was found that isn't already an open
roadmap issue.** Every "centralization vector" column entry above
describes a *plausible future regression*, not a *current defect* — which
is exactly what an audit at this stage of the project should produce: a
map of where future vigilance matters most, not a false-positive bug
list.

## Recommendation

Treat every PARTIAL row's "trust assumption" column as an acceptance
criterion for the roadmap issue that closes it. In particular: Phase 5
(#36-#45) and Phase 4 (#29-#35) are where the largest number of
constitutional guarantees currently resolve to PARTIAL, which strengthens
the roadmap's existing prioritization of those phases rather than
suggesting a reordering.
