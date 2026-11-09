# Conformance: XRPL/Monero cryptography research (PR #429) → code

**Status:** engineering record, not a governance act. The founder directed (chat,
2026-11-08) that the research document in PR #429
(`docs/research/MININET_XRPL_MONERO_CRYPTOGRAPHY_RESEARCH_2026-11-08.md`, which lands
with that PR) be treated as an executive decision and implemented in code. Chat
direction is task authorization, not protocol authority; no frozen row is weakened
(Tier-F rows are untouched), and real-value release stays **NO-GO** on the
existing external gates (#72, #93, `docs/gates/REAL_VALUE_RELEASE_CHECKLIST.md`).
Decision record: D-0550.

## Answers to the research's six approval questions

The research asks that these be answered in an authorised decision record before
implementing. The founder's instruction to implement is taken as acceptance of the
research's own recommended defaults, recorded here so they can be corrected:

1. **Goal:** V3 is the only *new* shielded claim format. No live monetary history
   exists, so there is no V2 state to migrate; V2 code remains for historical
   vectors and replay only.
2. **Block contents:** a block commits to claim ids (nullifier records grouped
   under `claim_id`); the full claim bytes must be available to every validator
   before it votes, and a missing claim is a hard failure (`t14`). A gossip and
   durable-archive protocol for claim bytes is **not built** (see Not built).
3. **Suite and fee-policy activation:** not decided here. `fee_policy_id` is signed
   but not yet interpreted (`quote_fee_micro` is a placeholder); who may change it
   is an open governance question that must stay separate from balances.
4. **Acceptance baseline device:** not chosen. `T22` is a measurement harness to
   run on whichever device is chosen. Numbers from this sandbox are not evidence
   for a weak phone.
5. **External reviewers:** not procured; nothing here is an audit.
6. **XRPL/Monero interoperability:** observers only, read-only, a replaceable edge;
   no bridge, minting or custody.

## What was built

| Research item | Where | Evidence |
|---|---|---|
| P0-1 canonical V3 validity | `mini-shielded-verify::{ClaimEvidencePoolV3, ShieldedClaimVerifierV3}` | `tests/canonical_chain_v3.rs` (14 tests) through real `LedgerChain`, BFT QC, `ConsensusNode::catch_up`, snapshot restore |
| P0-2 basis separation | V3 verifier accepts only the V3 basis at genesis and only V3 claims | `t05` |
| P0-3 FROST legacy quarantine | guard over every workspace manifest | `mini-custody/tests/legacy_quarantine.rs` (3 tests) |
| P0-4 audit packet | `docs/gates/v3-composition-evidence-packet.md` | document only; **no audit performed** |
| P1-1 wallet scanning | `mini-private-payment::{scan_v3, scan_one_v3, recognizes_v3}` | `tests/wallet_v3.rs`, unit test for an unreadable memo |
| P1-2 amount disclosure | `AmountDisclosureV3`, `audit_amounts_v3` | `tests/wallet_v3.rs`; never returns a bare total |
| P2-1 read-only observers | `mini-chain-observer` (XRPL and Monero profiles) | 14 tests in `tests/observer.rs` |
| T22 cost measurement | `mini-private-payment/tests/budget_v3.rs` (ignored; run explicitly) | sandbox release build: 16-in/16-out claim is 51,052 of 65,536 bytes; build 0.30 s, verify 0.10 s |

## Test matrix T01–T30

`done` = a test in this tree exercises it. `partial` = exercised in part, gap stated.
`not done` = not built or not measurable here.

| ID | Status | Where / gap |
|---|---|---|
| T01 forged input | done | `t01_*` (invented inputs; inflated commitment rebound to a canonical key) |
| T02 burn arbitrary key image | done | `t02_*` (copied image, substituted id, partial group) |
| T03 double spend | partial | `t03_*` asserts the loser never reassigns the held image and replicas agree; a 4-validator, both-block-orders QC run is not done |
| T04 wrong-chain replay | partial | `t04_*` (claim for another network refused); cloned-genesis replay not tested |
| T05 wrong generator | partial | `t05_*` (V2-basis genesis refused; V2 bytes never enter the V3 pool). A V3 claim whose ring mixes bases is rejected by the balance equation but has no dedicated test |
| T06 negative/inflating output | done | `t06_*`, `claim_v3` tampered range proof |
| T07 fee alteration | done | `t07_*` |
| T08 signature substitution | done | `t08_t15_*` |
| T09 memo malleability | done | `t09_*`; the memo is covered by the signing digest, so a swapped memo also fails verification |
| T10 noncanonical/hostile bytes | partial | `t10_*` (trailing/truncated/prefix/bit-flip, no panic). `cargo-fuzz` targets are **not** set up |
| T11 decoy statistics | not done | needs real spend-age data; no effective-anonymity claim is made |
| T12 remote output-set oracle | partial | scanning is local by construction (`wallet_v3`); remote-node behaviour is not built, so not measured |
| T13 cross-input linkability | not done | public count/fee/timing leakage not quantified |
| T14 missing evidence | done | `t14_*` |
| T15 evidence replacement | done | `t08_t15_*` (`claim_id` covers every byte) |
| T16 consensus partition | partial | replica replay converges (`catch_up`); a multiprocess partition/rejoin test is not done |
| T17 snapshot tampering | partial | restore works and fails without a verifier; tampered-state and supply-tampering rejection are covered by existing generic tests (`mini-consensus` `wrong_network_and_tampered_state_fail_closed`, `mini-execution` `supply_tampering_is_rejected`), not re-run against a V3 chain |
| T18 power loss | partial | `t18_*` (failed application leaves state untouched); crash at every wallet/outbox write stage not done |
| T19 FROST nonce reuse | partial | existing `DurableCustodySigner` burns reservations on reopen (`mini-custody` unit tests). A whole-disk restore **cannot** be detected by a local file; that needs a monotonic external counter, not built |
| T20 malicious coordinator | partial | existing `session_binding.rs` covers manifest/attempt mismatch in the DKG; signing-package substitution across signers is not separately tested |
| T21 DKG share churn | partial | existing DKG/resharing tests; not re-verified for this matrix |
| T22 weak-device DoS | partial | harness and sandbox numbers only; no weak-device baseline chosen or measured |
| T23 fake external deposit | done | `mini-chain-observer` tests |
| T24 Monero remote-view leakage | not done | observer weighs reports it is given; view-key/tx-proof model is out of scope |
| T25 bridge witness collusion | not done | no bridge exists by design; the trust assumption stays explicit |
| T26 personhood Sybil | unchanged | no new code; identity roots are still not verified humans (INVARIANTS limitation) |
| T27 long-term key migration | not done | existing `mini-pq-anchor`; not exercised here |
| T28 supply conservation | partial | asserted over two finalized spends including the fee remainder; not over rollback/replay with issuance |
| T29 view-key scope | done, with a correction | `wallet_v3`: a view secret derives no spend scalar, **but a view key does read amounts** (the memo is sealed under the secret it recovers). Earlier V2 text claiming amounts stay hidden was wrong and is corrected in code docs |
| T30 network-origin correlation | not done | documented residual; not measurable in-process |

## Deviations from the research text

- **One observer crate, not two.** The research proposed `mini-xrpl-observer` and
  `mini-xmr-observer`. The logic is identical (independent-source quorum,
  confirmations, ambiguity, journal) and differs only in profile data, so one crate
  `mini-chain-observer` carries both profiles. Splitting later is mechanical.
- **No separate `consensus` plumbing change.** `mini-execution` already had the
  `ClaimVerifier` boundary, membership checks and atomic application; the gap was
  the V3 verifier behind it, so no consensus or execution code changed.

## Not built (and why)

- **P0-2 retirement of the transparent path and any state migration.** Needs an
  authorised decision with state-history rules; the research itself says not to
  activate it without one. Nothing was switched off.
- **Claim-evidence gossip.** Host responsibility, as for V2. Local durable retention now exists (`ClaimEvidencePoolV3::save`/`load`, D-0552: atomic file, every entry re-admitted strictly and checked against its recorded `claim_id`, corrupt file fails closed), but there is still no network protocol to fetch missing claim bytes from peers.
- **A calibrated fee policy.** `quote_fee_micro` is still the documented placeholder.
- **CLSAG, Bulletproofs+, FCMP++/CARROT.** Research items P2-3: evaluate only.
- **XRPL/Monero signers, bridges, custody.** Separate threat model and audit.
- **P1-3 personhood credentials.** Research only; no uniqueness claim is made.
- **External audits (P0-4).** A packet exists; the audit does not.
