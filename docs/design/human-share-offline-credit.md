# Reusable offline allowances and private account settlement

Status: **proposed, non-production prototype**. D-0529. This is separate from
[PR #351](https://github.com/mininet-labs/mininet/pull/351), the mobile OS proposal.
No issuer, credit policy, new mint, consensus timing or wallet deployment is activated.

## Purpose and requested behavior

A person without liquid MINI should be able to authorize bounded IOUs against
future Human Share. The collective chooses the allowance policy; the person
connects to obtain an authorization for their signing device. The same
**authorization** supports many individually signed payments until their total
reaches its limit. The sender device enforces that total before releasing a
payment. The recipient does not maintain the sender's running allowance.

Example, expressed in MINI for readability: an online authorization grants 100;
the device signs 30, 20 and 50, then refuses 1 more. These are three different
payment coupons under one authorization. Reconnecting may grant a new allowance,
but never resets already accepted debt or still-outstanding authorizations.
An exact copy of one payment cannot redeem twice. This is a credit limit, **not
insurance, a guaranteed payout, or permission to mint missing funds**.

This supports Mininet's purpose by letting people exchange useful work when
connectivity or current liquidity is limited, without a bank account or giving
wealth governance power. It also introduces default risk and potential coercive
debt collection; collective policy must bound exposure and protect Human Share.
An OS can make the local journal and encrypted transport convenient, but cannot
turn signatures into uncopyable objects or create offline consensus.

## Implemented in this proposal

| Component | Behavior | Boundary |
| --- | --- | --- |
| `mini-settlement::credit_permit` | Domain-separated signed authorization, fixed-size codecs, borrower-signed recipient/amount/sequence coupons, cumulative sender signing API | Ed25519 prototype; host supplies authorized issuer and canonical registration |
| `mini-settlement::credit_journal` | Encrypted allowance/outbox, stable process lock, compare-and-record, atomic replacement and file/directory flushes before returning signature | Owner-controlled filesystem and dedicated external wallet key; no hardware rollback resistance |
| `mini-economy::credit` | Shared human cap across permits/devices, canonical coupon deduplication, bounded debt, expiry, FIFO repayment from actual released share | In-memory transition engine; canonical persistence/private transfers remain host obligations |
| `mini-private-payment::account` | Private local account projection separating available, reserved, pending, conflicting and spent amounts | Requires complete wallet history and one coherent validated ledger snapshot |
| `mini-private-payment::reconcile` | All input key images must finalize the same exact claim before finality; partial inclusion errors | Strengthens existing behavior; does not implement a new consensus engine |
| `mini-economy::cadence` | Checked round/issuance scheduling arithmetic | No timers, validators or throughput/finality guarantees |

All amounts in code are integer micro-MINI. Policies have no production defaults.
The account projection concerns existing private cash payments; IOU receivables
are separate and must never be added to the spendable balance.

## Sender authorization and journal

The authorization binds network, policy revision, unique serial, device public
key, total allowance, issuing height and last submission height. Verify the
issuer signature and canonical admission before offline use. The prototype
accepts an issuer key supplied by its host; it installs no network admin key.
Production needs collectively authorized issuance/admission and audited private
eligibility proofs. A high personhood score alone does not establish unique-human
eligibility, and several roots must not multiply one person's allowance.

Each coupon binds the authorization ID, monotonically allocated local sequence,
recipient payment-instruction commitment and positive amount. It cannot be
redirected or increased without a new borrower signature. `sign_next_credit_iou`
loads committed usage, checks the new cumulative total, signs, then requires an
atomic journal commit of both the new counter and the exact coupon before
returning it. Low-level `sign_credit_iou` is a cryptographic primitive, not a
wallet allowance boundary; wallets must use the journaled path.

`FileCreditJournal` stores the exact outbox encrypted with ChaCha20-Poly1305,
fresh random nonces and authorization-bound associated data. It derives counters
from signature-checked saved coupons; two handles use an OS lock and stale
compare-and-record fails. The caller retains the dedicated storage key in its
wallet vault; no key file is created. Debug output redacts financial contents.
The journal is bounded at 4,096 coupons per authorization; exhaustion fails
closed even if some monetary allowance remains.

Create a journal once when a new authorization is admitted; use `open` on restart.
A durable initialization marker prevents treating a deleted data file as an
unused authorization. Corruption, missing files, wrong keys and wrong permits
fail closed. After an ambiguous disk error, reopen and retransmit saved coupons;
never refund allowance merely because delivery failed. Restoring/copying an
entire directory, using a different directory for the same authorization, or
modifying the signing program can still create competing offline coupons.
The prototype cannot solve that with software signatures alone. Secure hardware
may reduce rollback risk but cannot replace canonical double-spend checks.

## Canonical reconciliation and renewal

The production host must apply each transition in canonical order against an
authenticated state snapshot. Calling the accounting library is not proof of
canonical inclusion. It registers the exact online authorization before use and
validates collective policy, unique-human subject and device binding.

For each human, `unpaid debt + unused live authorizations <= approved cap`.
A new authorization consumes free capacity immediately, including those issued
to other devices. A valid submitted coupon moves its amount from unused
allowance to debt. The exact `(authorization, sequence, signed terms)` replay is
inert; different contents at the same sequence conflict. Different sequences
still cannot exceed the authorization's aggregate total. Canonical ordering
chooses admissible claims; a local receipt is never final money (M1/M2/M3).

Repayment frees capacity for a **new online authorization**, not new use of an
old exhausted signature. Expiry frees only the unused authorization capacity;
accepted debt remains. An authorization's last height is a **submission deadline**,
not proof of when an offline signature was made. A disconnected recipient may
miss that deadline and lose the opportunity to register its receivable. The wallet
must display this risk; expiry/grace policy needs collective review before use.
Replacing an authorization early must not erase still-valid coupons. The initial
model leaves them outstanding until expiry; revocation, recovery and grace rules
are explicit future work.

## Human Share repayment

A canonical release supplies actual newly released/vested Human Share. The
accounting engine applies the chosen fraction to oldest accepted unpaid coupons
and returns private payment instructions plus the person's remaining release.
Fractional micro-unit carry prevents frequent tiny releases from starving debt
repayment. An exact release-epoch replay returns its prior result with
`newly_applied = false`; an inconsistent replay or older epoch fails. The host
must atomically commit debt reduction, epoch state and corresponding private
transfers, never transfer again on replay, and never supply the same real release
under a different epoch. That transactional integration is not implemented here.

No coupon creates MINI. `gross released = repayments + recipient remainder`.
No projected/unvested share enters cash balance. D-0074's existing issuance ceiling,
Human Share floor and one-year vesting remain in force. Advancing receivables
against that future release does not repeal vesting. If eligibility or issuance
ends, repayment can stop; there is no protocol guarantee or implicit reserve.
Minimum subsistence share, maximum debt horizon, loss allocation, eligibility
changes and insolvency treatment require explicit collective decisions.

## Regular settlement and privacy

Separate fast **cash settlement rounds**, **issuance accounting epochs**, and
**vesting release**. A scheduling example is four-second target rounds and
21,600 rounds per daily issuance epoch; these are illustrative values, not
activated policy. No wall-clock catch-up mints extra money or makes offline IOUs
final. Measure actual quorum finality under realistic latency, partition,
validator churn, adversarial traffic and low-end hardware before selecting a
production target.

XRPL documents typical ledger closure around 3–5 seconds; this is a comparison
target, not a promise that Mininet matches its consensus:
[XRPL ledger close times](https://xrpl.org/docs/concepts/ledgers/ledger-close-times).
Monero's [RingCT](https://www.getmonero.org/resources/moneropedia/ringCT.html)
and [stealth addresses](https://www.getmonero.org/resources/moneropedia/stealthaddress.html)
illustrate separate amount and recipient protections. Combining a fast target
with private transfers does not automatically achieve equivalent anonymity.

Existing Mininet private payment components are prototypes with their own audit
and migration gates. The new credit authorization/coupon encoding is **not
confidential**: it exposes a linkable borrower key, amount and permit relation to
its receiver. Use encrypted local storage and authenticated encrypted transport
in experiments. Do not publish those records on a public ledger. Production
requires private entitlement/debt commitments, nullifier proofs, confidential
repayment and analysis of traffic, timing, amount and issuer-correlation leakage.
No public identity-to-debt table is proposed as a shortcut.

## Roadmap and acceptance gates

1. **This PR — executable primitives and review.** Verify sender cumulative
   limits, crash/restart outbox recovery, copied-coupon deduplication, renewal,
   expiry, repayment conservation and complete private-payment finality. Document
   external trust boundaries. Keep code unconnected to production money.
2. **Collective policy proposal.** Decide cap calculation from eligible future
   releases, permitted repayment fraction, personhood/unique-human evidence,
   submission window, loss/recovery rules and quorum-authorized permit admission.
   Confirm constitutional/vesting compatibility; no balance affects voice.
3. **Canonical private accounting.** Specify versioned commitments/nullifiers and
   eligibility/debt proofs; bind all transitions to the canonical state root.
   Implement bounded durable replayable storage and atomically couple each share
   release with its private repayments. Test competing devices, partitions,
   repeated epochs, corrupted state, recovery and migration. Cryptographic review
   must precede public testnet exposure of real user metadata.
4. **Wallet and device integration.** Connect the existing device delegation and
   vault to the journaled API, encrypted transport and reconnect authorization
   request. Show cash, IOUs owed/to receive, allowance remaining and submission
   deadline separately. Test cancellation, delivery ambiguity, lost devices,
   backup rollback, low storage and two processes. Never reset allowance locally.
5. **Settlement performance.** Integrate cadence with consensus scheduling;
   benchmark finality percentiles, proof throughput, block limits, fees and energy
   on mobile hardware. Publish results before claiming a four-second target.
6. **Independent review and opt-in pilot.** Obtain protocol/privacy/economic
   reviews, required human approvals and a governed release. Run capped,
   non-production-value trials and adversarial duplicate/rollback simulations.
   Expand only after the above evidence; owners choose software adoption.

No calendar dates are promised: later stages depend on review and explicit policy.
The mobile OS can reuse these wallet primitives after their acceptance gates; it
is not a prerequisite for prototyping them on existing supported devices.

## Runnable local example

Run `cargo run --locked -p mini-economy --example offline_credit`. The example
uses synthetic keys and policy, signs 30 + 20 + 50 MINI, reopens the encrypted
journal, refuses another 1, reconciles each coupon twice without double-counting,
repays 60 then 40 from simulated actual releases, and issues a distinct online
renewal. It deletes its temporary encrypted journal after successful completion.
This exercises the library integration; no network or real payment is performed.

## Evidence and remaining risks

Tests live in `mini-settlement/tests/credit_permit.rs`,
`mini-economy/tests/credit.rs`, `mini-private-payment/tests/account.rs` and cadence
unit tests. Run targeted package tests and Clippy with the committed lockfile.
An independent AI adversarial review identified debug metadata leakage,
fractional repayment starvation and unsafe recreation after journal-file loss;
all three were addressed, with regression coverage for the latter two. AI review
is evidence, not external audit or human approval. Broader repository release
blockers in `docs/STATUS.md` remain in force.
