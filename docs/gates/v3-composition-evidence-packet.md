# V3 shielded-payment composition: evidence packet for external review

**This is a hand-off document, not an audit result.** No independent applied-
cryptography review of this composition has been performed. Internal tests and AI
review do not close Gate #72 or #93. Real-value release remains NO-GO.

Prepared for research PR #429's P0-4. Fill in the revision before sending.

- **Source revision:** `<exact git SHA of the PR head at hand-off>`
- **Toolchain / lockfile:** `rust-toolchain.toml` as committed; `Cargo.lock` as committed.

## Scope to review (the composition, not its parts)

| Layer | Files |
|---|---|
| Stealth outputs | `crates/mini-value/src/stealth_v3.rs` |
| Ring spend proof | `crates/mini-value/src/mlsag_v3.rs` |
| Range proof (library `bulletproofs 5.0.0`) | `crates/mini-value/src/bp_range_v2.rs` |
| Claim format, three digests, verify | `crates/mini-private-payment/src/claim_v3.rs`, `memo_v3.rs` |
| Validator adapter (new) | `crates/mini-shielded-verify/src/v3.rs` |
| Canonical membership, key images, atomic apply | `crates/mini-execution/src/{shielded,nullifier,state}.rs` |
| Wallet scan / disclosure (new) | `crates/mini-private-payment/src/wallet_v3.rs` |
| FROST custody | `crates/mini-custody/src/{signing,session,manifest,share_store,rotation}.rs` |

## What the reviewer should be asked

1. Is `verify_v3` plus `ShieldedClaimVerifierV3` plus execution's membership and
   key-image checks *complete*? What can a malicious proposer, wallet or peer cause
   to finalize that should not?
2. Is the generator basis (`bp_range_v2`) used consistently by the range proof, the
   pseudo-commitment balance and the MLSAG commitment column?
3. Do the three digests (`memo_context`, `signing`, `claim_id`) separate every
   domain, and is anything signed that should be bound but is not?
4. Is the placeholder fee function safe to ship as a *rule* before calibration, or
   must the fee policy be fixed first?
5. Is the 16-member fixed ring and its decoy distribution defensible, given there is
   no real spend-age data?
6. FROST: nonce discipline across crashes, restores and coordinator misbehaviour;
   the effect of a whole-disk restore.

## Evidence in the tree

Run from the repository root:

```text
cargo test -p mini-shielded-verify --test canonical_chain_v3
cargo test -p mini-private-payment --test wallet_v3
cargo test -p mini-custody --test legacy_quarantine
cargo test -p mini-chain-observer
cargo test -p mini-private-payment --release --test budget_v3 -- --ignored --nocapture
```

`docs/design/xrpl-monero-research-conformance.md` maps each research test (T01–T30)
to a test or states the gap.

## Known limits the reviewer should not have to discover

- The fee function is a placeholder; `fee_policy_id` is signed but not interpreted.
- Evidence admission is structural; verification is separate and mandatory.
- No claim-evidence gossip or durable retention exists.
- No fuzz harness is set up.
- A whole-disk restore of a signer's journal is not detectable locally.
- Cost numbers in the conformance document were measured on a development sandbox,
  not on a weak phone.
- Identity roots are not verified humans; nothing here changes that.

## What closes the gate

An independent, signed report for this exact revision, with every Critical/High
finding fixed and re-reviewed, recorded in the release checklist by the
authorised decision-maker. This document cannot do that.
