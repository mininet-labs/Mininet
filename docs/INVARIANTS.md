# Invariants — the engineer's primary checklist

This is the working mirror of the Constitution's canonical register
(SPEC-00 §12). The Constitution governs; if this file and SPEC-00 ever
disagree, **SPEC-00 wins** and this file is in error. For *why* these
invariants exist and how to reason about a case they don't obviously
cover, see `docs/FOUNDER_DIRECTIVES.md` — that document sits underneath
this one, not alongside it.

The sprint's Definition of Done requires that frozen invariants are
*"encoded as checks, not conventions."* The **Enforced by** column tracks
exactly where each one becomes code. `pending` means the owning crate/
module is not in the tree yet. **This file states current implementation
status only where it's load-bearing for the invariant's meaning; the
detailed, living account of what's built lives in `docs/STATUS.md` — this
file's job is the checklist, not the narrative.**

## The traceability chain

Every Tier-F row below carries a stable ID and an explicit **Directive**
column, so the full chain is walkable in either direction without relying
on anyone's memory:

```
Founder Directive  →  Invariant (this file)  →  Source (Spec/D-00xx)  →  Enforced by (crate + test)
```

**Worked example**, start-to-finish:

```
Directive 16 (Preserve the Voice/Value Wall at All Costs)
  → P1: "No balance maps to governance or validator vote weight"
    → Source: SPEC-00 P1
      → Enforced by: mini-chain::ValidatorSet (no weight field, by
        construction) + mini-forge::governance (test
        author_never_counts_and_one_identity_root_counts_once)
```

Walk it forward from a principle ("what protects the voice/value wall?")
or backward from a failing test ("which founding principle does this
protect?") — both directions resolve to the same chain. `docs/
THREAT_MODEL.md` is the companion document that starts one hop earlier:
which invariant, if it failed, closes off which attack.

Tier F is organized into nine sections so a reviewer can jump straight to
the domain a PR touches, rather than scanning one large table. Within
each section, invariants are listed roughly most-load-bearing first.

## ⚠ Hard, temporary limitations — read these before anything else

Two gaps in the current tree are severe enough that treating them as
ordinary "partial" rows would understate the risk. Both are honestly
documented at the crate level already; this section exists so they can
never be missed by only skimming a table.

- **Every "verified identity" in this tree today is a verified `did:mini`
  identity *root*, not a verified human.** Forge quorums, finality votes,
  and reward accrual all count distinct identity roots. Nothing currently
  prevents one human from controlling multiple roots except cost — and
  whether that cost is actually high enough is Phase 2's open, unresolved
  question ([roadmap #18](../../issues/18),
  [`docs/audits/issue-10-frozen-invariants-review.md`](audits/issue-10-frozen-invariants-review.md)).
  **No code path anywhere in this tree may be read as enforcing
  "one-human-one-vote" (P2) until personhood, not identity-root counting,
  is what's actually verified.** This is not a note; it is a hard
  constraint on how every quorum/vote-counting result in this codebase
  must be described, in code comments, docs, and product copy alike.
- **The proof-of-space-time storage scheme (`mini-spacetime`, D-0039)
  proves continuous possession, not replication uniqueness.** A single
  well-resourced server can answer every challenge for many claimed
  identities from one copy of the data — it cannot yet be distinguished
  from a thousand honest small devices each holding their own copy. This
  is exactly the warehouse-consolidation attack the whitepaper's
  egalitarian "thousand cheap machines beat one warehouse" thesis (§7)
  depends on resisting. **No storage-reward or block-production-weight
  claim may be read as resistant to storage consolidation until real
  proof-of-replication ([roadmap #31](../../issues/31))
  lands.**

## 1. Voice / value — money and power stay separated

| # | Frozen invariant | Directive | Source | Enforced by |
|---|---|---|---|---|
| P1 | No balance maps to governance or validator vote weight | D16 | SPEC-00 P1 | partial — `mini-chain::ValidatorSet` has no weight field anywhere (equal per identity root by construction, not a balance-weighted count); `mini-forge::governance` quorums are likewise counted per identity root, never balance (test `author_never_counts_and_one_identity_root_counts_once`); full chain/consensus integration is `pending` |
| V1 | Instant deterministic finality via adapted Tendermint/CometBFT-style BFT quorum, equal validator weight per verified identity root | D16, D8 | SPEC-05 + D-0008 | partial — `mini-chain::verify_finality` requires `>2/3` distinct, currently-delegated, `VOTE`-capable validator roots to precommit the same block/height/round before treating it as final; real networked consensus is `pending` — see [roadmap #36-#45](../../issues/36) |
| V2 | Storage/seeding earns value, never voice | D16 | SPEC-00 P1 + D-0033 | ✅ `mini-storage::verify_serve` / `mini-reward::accrue_storage`; see the storage warehouse-attack hard limitation above for the adjacent, unresolved risk |
| V3 | Public profiles/walls do not create privilege | D16 | SPEC-09 §6.1 + D-0033 | ✅ `mini-social::PublicWall` requires only `Capabilities::POST`, never `VOTE`; no wall registry exists |
| V4 | Base devices do not create governance weight | D16 | SPEC-01 §6 + D-0033 | ✅ `did-mini::BaseDeviceRole` carries no `Capabilities` bit and cannot grant one |

## 2. Personhood — read the hard limitation above first

| # | Frozen invariant | Directive | Source | Enforced by |
|---|---|---|---|---|
| P2 | One verified human, one **equal** vote; early grants no extra | D8, D16 | SPEC-00 P2 | partial — `did-mini` binds many devices to one human-root with capability scoping that cannot create extra votes; `mini-chain::verify_finality` counts a validator root at most once regardless of device count — **but see the hard limitation above: this is "one identity root, one vote" today, not yet "one human, one vote"** |
| PH1 | Co-presence is range-bound and mutually signed; relay can't fake it | D8, D15 | SPEC-02/SPEC-03 | partial — `mini-presence` requires proximity transport, delegated `ATTEST` device, distinct-key signatures, channel binding, fresh nonces, RTT under policy; a tighter BLE/UWB ranging bound is `pending` — see [roadmap #17](../../issues/17) |

## 3. Identity & key custody

| # | Frozen invariant | Directive | Source | Enforced by |
|---|---|---|---|---|
| ID1 | Keys never leave the device; no custodial recovery | D2, D9 | SPEC-01 G1 | ✅ `mini-crypto::keys`/`agreement`/`aead` (export only via explicit methods; `Debug` redacts key material) + `did-mini::Controller` |
| ID2 | Self-certifying identifier; no central registry to verify | D2, D8 | SPEC-01 §3/G8 | ✅ `did-mini` (`Kel::verify` re-derives the SCID from inception) |
| ID3 | Security-critical key events are pre-rotation-protected & anchored | D2, D6 | SPEC-01 §16 | ✅ pre-rotation in `did-mini`; on-chain anchoring `pending` |
| ID4 | Many devices provably one human; mutual, revocable, capability-scoped | D8 | SPEC-01 §6/G3 | ✅ `did-mini::verify_delegation` (device claims root **and** root seals device; both required) |
| ID5 | KEL/device-delegation wire decoders reject malformed, oversized, and ambiguous input before verification | D6, D15 | SPEC-01 + D-0013 | ✅ `did-mini` decoder caps, SCID validation, strict multihash lengths |

## 4. Money & finality

| # | Frozen invariant | Directive | Source | Enforced by |
|---|---|---|---|---|
| P4 | Slow, presence-conditioned vesting; never a lump sum | D4, D13 | SPEC-00 P4 | partial — `mini-reward` accrual is rate-capped per window, diversity-weighted, maturation-delayed; on-chain vesting module is `pending` |
| M1 | **Money does not merge. CRDT/automatic conflict-resolution is forbidden for spendable value.** | D5, D4 | Founder review, D-0045 | ✅ `mini-settlement` (D-0055) has no merge function anywhere in its API — `reconcile()` only ever answers "did this exact claim win," never "combine these claims"; `mini-crdt` remains scoped explicitly to non-spendable content (threads/docs) and must never be extended to cover value |
| M2 | **Offline/local payment is never final. It is a signed pending claim until canonical chain inclusion; wallets must distinguish pending / accepted / finalized.** | D5 | Founder review, D-0045 | ✅ `mini-settlement::SettlementState`/`WalletLabel` (D-0055) make the distinction a type — `is_final()` is true only for `Finalized`, never `AcceptedLocal`; `CanonicalLedgerView` (the real chain-execution backing) is `pending`, tracked by [roadmap #41](../../issues/41) |
| M3 | **Canonical ordering alone decides conflicting spends (double-spends). No local committee, hotspot, relay, or cache may finalize ownership.** | D5, D4 | Founder review, D-0045 | ✅ `mini-settlement::reconcile` (D-0055) only ever finalizes a claim by reading a `CanonicalLedgerView`; test `conflicting_claims_at_the_same_nonce_never_both_finalize` proves exactly one of two conflicting claims resolves. A real chain-backed `CanonicalLedgerView` is `pending`, tracked by [roadmap #40](../../issues/40) |

## 5. Updates & forks

| # | Frozen invariant | Directive | Source | Enforced by |
|---|---|---|---|---|
| P3 | No owner/admin key; public-domain license; no off switch | D2, D3 | SPEC-00 P3 | partial — `LICENSE` (CC0); `pending` — genesis & release pipeline |
| U1 | No forced auto-update / no off switch | D3, D2 | SPEC-00 P3 + SPEC-11 | ✅ `mini-update::AdoptionState` — `evaluate` never mutates state, `adopt` always re-verifies from scratch, `refuse` is a normal, unblocked outcome |
| U2 | Core software bootstrap and updates cannot rely on external services | D2, D3 | SPEC-11 + D-0011 | partial — `mini-bootstrap::CapsuleHeader`/`GenesisSeed` + `mini-update::AdoptionState`; the release registry itself is `pending` |
| U3 | Bluetooth-only identity + genesis/update chunk exchange must work with no internet | D11, D6 | SPEC-03 keystone + D-0012 | partial — protocol-logic done in `mini-bootstrap`; real BLE/local-Wi-Fi transport is `pending` in `mini-bearer` (D-0042 shipped real TCP transport as a stand-in, proven live — see [roadmap #22](../../issues/22) for what's still missing) |
| F1 | **Forking the software is free. Inheriting Mininet's legitimacy is not — it requires continuity of the frozen invariants, the personhood-root history, release-registry continuity, and canonical chain state. A code copy alone confers none of it.** | D7 | Founder review, D-0046 | `pending` — no code enforces or even represents "legitimacy" as a concept yet, since there is nothing yet to fork off of in the networked-chain sense; this row is the frozen constraint any future fork-handling/registry design must satisfy, and the criterion `docs/FAILURE_BOOK.md`/decision-log entries should judge a claimed fork against |

## 6. Privacy

| # | Frozen invariant | Directive | Source | Enforced by |
|---|---|---|---|---|
| P5 | No protocol requirement for raw personal data; ZK attestation only | D9 | SPEC-00 P5 | partial — `mini-crypto` keeps secrets on-device; `mini-bearer` gives an anonymous, forward-secret channel whose handshake carries no identity; ZK personhood attestation still `pending` |
| P6 | No forced replication; no compelled decryption; device-only honored | D9 | SPEC-00 P6 | `pending` — storage fabric |
| PR1 | Local encrypted channel primitives reject ambiguous or weak peer input | D9, D15 | SPEC-03 + D-0014/D-0015 | ✅ `mini-crypto::agreement` rejects all-zero X25519 shared secrets; `mini-bearer::Channel` caps plaintext/ciphertext before crypto and rejects small-order handshakes |
| PR2 | Seed-on-view is user-controlled and policy-bound | D9 | SPEC-00 P6 + D-0033 | ✅ `mini-store::Store::note_view` — encrypted content never promoted past `CacheTier::PrivateOnly` |

## 7. Storage — read the hard limitation above first

| # | Frozen invariant | Directive | Source | Enforced by |
|---|---|---|---|---|
| S1 | Proof-of-space-time backs block-production weight | D11, D16 | Whitepaper §7/§8.1, D-0039 | partial — `mini-spacetime::storage_proof` (Merkle/PDP challenge-response) proves continuous possession; **does not** prove replication uniqueness — see the hard limitation section above |

## 8. Networking

| # | Frozen invariant | Directive | Source | Enforced by |
|---|---|---|---|---|
| N1 | Radio/LoRa is not part of Mininet | D14, D10 | D-0009 (amended) + D-0033 | ✅ documentation-enforced, permanently closed — see `docs/FAILURE_BOOK.md` |
| N2 | Real transport now exists for IP-reachable connectivity (not BLE) | D2, D6 | D-0042 | ✅ `mini_bearer::TcpBearer`, proven live via `mini-net`'s multi-process gossip demo; BLE/local-Wi-Fi radio adapters remain `pending` and need real phone hardware — see [roadmap #22](../../issues/22) |

## 9. AI & audit gates

| # | Frozen invariant | Directive | Source | Enforced by |
|---|---|---|---|---|
| AI1 | AI may draft sensitive code, but human review is mandatory | D12 | SPEC-11 §2 + D-0033/D-0037 | partial — `mini-forge::governance::PROTOCOL_MIN_APPROVALS` enforces a 2-approval floor with no 1-of-1 canonical merge path; a dedicated "AI-assisted" flag on commits/PRs is `pending` — see [roadmap #78](../../issues/78) |
| A1 | **Production use — real value, real treasury custody, real consensus, real personhood proofs — requires external cryptography audit as a hard gate, not a desirable-but-optional step. Tests passing is not audit. Founder review is not audit.** | D12, D4 | Founder review, D-0047 | `pending` — no code path in this tree currently allows "production use" of any of these (everything is prototype-labeled and founder-reviewed only); this row exists so that remains true until an actual external audit occurs, not until someone decides it's no longer necessary |

## Foundational (cross-cutting, don't fit one domain above)

| # | Frozen invariant | Directive | Source | Enforced by |
|---|---|---|---|---|
| X1 | Core implementation language is Rust | D14 | D-0001 + D-0008 | ✅ the entire workspace is Rust |
| X2 | Crypto-agility: no signature, DH, AEAD, or KDF algorithm hard-wired for life | D13, D6 | SPEC-01 §13 + D-0014 | ✅ `mini-crypto::suite`/`agreement`/`aead`/`kdf` |
| X3 | Strong-hash content addressing; never SHA-1 | D5, D9 | SPEC-11 | ✅ `mini-crypto::hash`/`multihash` — verified end-to-end in `docs/audits/issue-29-cid-integrity-review.md` |

## Tier T — Tunable within limits (one-human-one-vote + timelock + bounds-check)

These are *parameters*, changeable only within frozen floors/ceilings. Recorded
here so no module silently treats one as frozen or unbounded.

- Current **default signature/DH/AEAD/KDF suites** (must remain migratable) — see D-0003 and D-0014.
- Content-address default algorithm (within the strong-hash set) — see D-0004.
- Personhood thresholds / decay; verification tier rates / dwell windows / K
  attesters (within frozen safety floors) — `pending`.
- Reward-curve constants; fee value targets; epoch length; committee size;
  timelock durations; treasury signer-set size — `pending` (chain).
- Pinned toolchain version; K independent builders (within a frozen minimum) —
  see D-0006.
- Which external audits count as satisfying **A1** (§9) and their cadence —
  `pending`; the *requirement* is frozen, the *process* around it is tunable.

## Tier O — Organic (permissionless; no vote)

App surfaces, feed-ranking plugins, client software, new bearers, new storage
clients, new application modules, moderation filter lists. Constrained only in
that they may not cause a Tier-F violation.

---

### How to use this file in review

When a PR adds or changes a frozen-domain behavior, it should:
1. Point to the SPEC-00 §12 line it implements (or, for a founder-review-
   sourced row, the decision-log entry that established it — see the
   **Source** column).
2. Cite which Founder Directive(s) the change serves or is constrained by
   — the **Directive** column is not decoration, it's the top of the
   traceability chain described above.
3. Move the relevant **Enforced by** cell from `pending` to the concrete
   module path, ideally with a test name.
4. Add a `D-00xx` decision-log entry if a \[FREEZE\] choice was made — see
   `docs/DECISION_LOG.md`'s header for the current entry format, which
   itself requires a "Constitutional impact" field naming the invariant
   ID(s) touched.
5. Update `docs/STATUS.md` if the change moves a subsystem's implementation
   status — **not** this file's Enforced-by column alone, and **not** the
   decision log.
6. Check `docs/THREAT_MODEL.md` for whether the change closes, weakens, or
   is silent on any listed threat — update the relevant entry's
   **Mitigated by** field if so.

A frozen invariant should be impossible to express in code (Layer 1) wherever we
can manage it, and rejected on validation (Layer 2) everywhere else — never left
to a reviewer's memory.
