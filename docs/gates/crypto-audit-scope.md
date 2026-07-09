# External cryptography audit — scope package

Gates [roadmap #72](https://github.com/britak420/Mininet/issues/72), P0.
**Founder action required: engage a real applied-cryptography auditor or
credentialed academic.** Nothing in this repository can close this gate —
D-0037 explicitly states AI-authored, founder-reviewed crypto is not
audit-equivalent, and every crate below is exactly that: real, tested,
and unaudited.

## Scope — crates to review

| Crate | What it implements | Founder-review record |
|---|---|---|
| `mini-value` | Stealth addresses, linkable ring signatures, Bulletproofs confidential amounts | D-0036, D-0040 |
| `mini-treasury` | FROST threshold signing (trusted-dealer keygen today — see `dkg-audit-scope.md` for the separate DKG scope) | D-0041 |
| `mini-settlement` | Offline payment claims, reconciliation state machine (composes `mini-crypto` only — no new primitives, but the *composition* should still be reviewed) | D-0055 |
| `mini-bounty` | Anonymous bounty claims (ring signature + stealth address reuse, zero new primitives) | D-0049 |

## The questions an auditor must answer

- **`mini-value`:**
  - Can stealth addresses be linked across transactions by an observer who isn't the intended recipient?
  - Can the ring signature's key image be forged, predicted, or made to collide across unrelated signings?
  - Can a ring member be deanonymized through signature malleability, timing, or repeated-usage patterns?
  - Does the Bulletproofs range proof implementation (`bp_range.rs`, `bp_ipa.rs`, `bp_generators.rs`) actually bind the committed value, or can a malformed proof pass verification for an out-of-range amount?
  - Is the curve arithmetic (`curve.rs`, built on `curve25519-dalek`) used correctly — no missing subgroup checks, no non-constant-time comparisons on secret data?
- **`mini-treasury` (signing only, keygen is `dkg-audit-scope.md`):**
  - Can FROST signing leak partial key material or nonces across sessions?
  - Is nonce reuse detectable/prevented (the classic Schnorr/FROST nonce-reuse key-recovery attack)?
  - Does the live multi-process demo's session handling generalize safely to adversarial (not just faulty) co-signers?
- **`mini-settlement`:**
  - Is the claim-signing message construction (`claim_message` in `claim.rs`) actually collision-resistant against a chosen-field attack (can two different `(payer, payee, amount, nonce, ...)` tuples ever encode to the same signed bytes)?
  - Does `reconcile()`'s state machine have any path where a claim could be read as `Finalized` without a matching `CanonicalLedgerView` entry?
- **`mini-bounty`:**
  - Does the length-prefixed `claim_message` binding actually prevent cross-pool replay and payout-address tampering, as claimed in the crate's own tests?
  - Is the "ring never shrinks" anonymity argument (claimed grants stay in the ring) actually sound, or does claim timing leak information the ring-size argument doesn't account for?

## What's already done, so the auditor isn't starting from zero

- 22/22 crates in the workspace `forbid(unsafe_code)`; dependency-tree unsafe usage is documented (`docs/audits/issue-71-memory-safety-audit.md`).
- Every claimed cryptographic property above already has an adversarial test in the crate's own test suite — the auditor's job is to verify the tests are actually sufficient, not to write coverage from scratch.
- `docs/DECISION_LOG.md` D-0036/D-0040/D-0041/D-0049/D-0055 record why each construction was chosen, so "why not just use library X" questions have an answer already.

## Hard constraints on the review

Per Directive 2/P3: an auditor may recommend algorithm changes, parameter changes, or additional checks. An auditor may **not** be used as the mechanism to introduce an admin key, a backdoor, or a "recovery" path that isn't `did-mini::Controller::recover_from_kel`'s existing escrowed-key model. If a finding seems to require one, the answer is "redesign without it," not "add it because the auditor suggested it."

## What closes this gate

A written audit report covering the questions above, findings triaged and either fixed (with a new D-number recording the fix) or explicitly accepted as a known limitation with a named severity — the same discipline this repo's own internal audits (`docs/audits/`) already use. Filed as a new dated document in `docs/audits/` once received, cross-referenced from this file.
