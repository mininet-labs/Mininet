# Testing this repository

Who this is for: the founder cohort, Michal, and anyone else reviewing this
tree before a merge or before relying on it for anything real. It's a
concrete, copy-pasteable set of steps — what to run, what "pass" looks like,
and what a red flag would look like — not a restatement of the design docs.

Start with `docs/DECISION_LOG.md` if you haven't reviewed the founder
decisions yet (`D-0036` through `D-0041` are the newest and highest-stakes:
the AI-authorship policy override and the four cryptography prototypes it
produced). This document assumes you've read those and just want to verify
the claims.

## 0. Prerequisites

- Rust via `rustup` — the toolchain is pinned in `rust-toolchain.toml`
  (`1.83.0` + `rustfmt` + `clippy`); `rustup show` from the repo root will
  install it automatically if you don't have it.
- No network access is required to build or test once dependencies are
  fetched once (`Cargo.lock` is committed, so dependency versions are fixed).
- No phone, BLE hardware, or second machine is required for anything below —
  every demo runs on one machine today (see the root README's honest note on
  what doesn't have real network transport yet).

## 1. Whole-workspace verification (run this first, always)

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features --workspace -- -D warnings
cargo test --all --all-features
```

**Pass:** all three exit 0 with no output beyond normal cargo progress
lines, and the test line for each of the 22 crates reads
`test result: ok. N passed; 0 failed`.
**Red flag:** any `FAILED`, any clippy `error:`, or any panic message. This
should never happen on a committed `main`/PR branch — if it does, stop and
report it before reviewing anything else, since it means the tree you're
looking at doesn't match what CI/the PR claims.

Optional, and slower (only the Bulletproofs range-proof tests in
`mini-value` are noticeably slower in debug mode — this is expected, not a
bug, see that crate's README):

```sh
cargo test --all --all-features --release
```

## 2. The two live, runnable demos

Neither of these requires a phone or a second machine — both simulate
multiple parties within one process, honestly labeled as such (see each
demo's own doc comment for exactly what "live" does and doesn't mean).

### 2a. The keystone demo (identity + presence + reward)

```sh
cargo run -p mini-keystone --example keystone
```

**What you're checking:** two independently-generated `did:mini` identities
exchange over an anonymous encrypted channel, verify each other's identity
without seeing the other's real identifier on the wire, produce a mutually-
signed range-bound presence attestation, and each accrues (identical,
non-spendable) reward. **Pass:** it prints both sides' identity roots
(different every run — they're freshly generated), a channel binding tag,
and ends with `identity verified offline · presence range-bound & mutually
signed · one identity root, one accrual (P2)`. **Red flag:** identical
identity roots on both sides (would mean the "two separate parties" claim is
fake), or reward accruing without presence being verified first.

### 2b. The FROST live multi-device treasury-signing demo

```sh
cargo run -p mini-treasury --example frost_live_demo
```

**What you're checking:** this is the highest-stakes prototype in the repo
(whitepaper §11: treasury custody is "a permanent honeypot by nature"). Five
separate OS threads, each holding only its own key share, sign a 3-of-5
treasury payout live over real channel-based message passing, and a second
session demonstrates a tampered signature share getting caught and
attributed *before* any signature is produced. **Pass:** session 1 ends with
`independent verification against the group public key: VALID`; session 2
ends with `REJECTED before producing any signature:` and names the tampered
device. **Red flag:** session 2 ever printing a signature (means a corrupted
share slipped through verification), or either session hanging/panicking.
Run it a few times — nonces and the group key are freshly randomized every
run, so the specific hex output changes each time; the pass/fail *shape*
should not.

## 3. Reviewing the cryptography prototypes specifically

These four are AI-authored under the founder's explicit D-0037 policy
override, pending external audit — the review focus here isn't "does it
compile," it's "does the *design* hold up." For each, the fastest path to a
meaningful review is: read the crate's module-level doc comment (it states
the construction and, for the newest three, the hand-derived algebraic
identity the implementation rests on), then check the tests actually
exercise adversarial cases, not just the happy path.

| Prototype | Where | What to check | Adversarial tests already present |
|---|---|---|---|
| Stealth addresses | `mini-value::stealth_impl` | One-time output addresses are unlinkable to the real recipient address without the view key | Wrong-view-key non-recognition |
| Ring signatures | `mini-value::ring_impl` | Signer anonymity within the ring; key image prevents double-spend | Wrong ring, tampered signature, double-spend via same key image |
| Bulletproofs confidential amounts | `mini-value::bp_range`, `confidential_impl` | The range proof genuinely constrains `value ∈ [0, 2^64)`; balance check is exact homomorphic equality | Tampered proof component (7 independent tamper tests), malformed encoding, additive-homomorphism sanity check |
| FROST threshold custody | `mini-treasury::frost_keygen`, `frost_sign` | Individual-share verification catches a bad signer before aggregation; any threshold-sized subset produces a valid signature | Tampered share caught with attribution, wrong-message/wrong-key verification failure, insufficient-signer rejection |

**A meaningful negative-review outcome looks like:** "I checked the hand-
derived identity in `bp_range.rs`/`frost_sign.rs` against the paper and it's
wrong at step N" or "the test suite doesn't actually cover case X, which
would break property Y." A pass on `cargo test` is necessary but not
sufficient — it only proves the code does what the code's own author
believed was correct.

## 4. Reporting a finding

Open a PR comment or GitHub issue against this repo referencing the crate
and, if it applies, the `D-00xx` decision it relates to. If it's a
correctness issue in one of the four cryptography prototypes above, say so
explicitly — those get priority over style/ergonomics feedback, per the
D-0033 review floor.

## 5. Checklist (copy this into a PR comment or issue when you finish a pass)

- [ ] `cargo fmt --all -- --check` clean
- [ ] `cargo clippy --all-targets --all-features --workspace -- -D warnings` clean
- [ ] `cargo test --all --all-features` clean, all 22 crates
- [ ] Keystone demo (`cargo run -p mini-keystone --example keystone`) behaves as described in §2a
- [ ] FROST live demo (`cargo run -p mini-treasury --example frost_live_demo`) behaves as described in §2b, including the adversarial-tamper session
- [ ] Reviewed at least one cryptography prototype's module docs + hand-derived identity against its source paper (§3)
- [ ] Any finding reported per §4, with severity noted

## What this document does not cover

This is a correctness/functional-review checklist for the code as it exists
today — it is not a security audit, a penetration test, or a substitute for
the external cryptography review every 🧪-tagged prototype in the root
README still needs before real value or custody depends on it. See the root
README's "Path to a global launch" section for the full list of what stands
between this tree and a production deployment.
