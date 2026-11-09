# V3 shielded path: the three decisions code cannot make (proposal, D-0553)

Status: **proposal only. Nothing here is decided, and nothing is activated.** These
three items were left open by D-0550 because each needs an authorised decision
(or hardware) rather than more code. This note gives the decision-maker what they
need: the options, the recommended choice, what must be true first, and what the
repository already provides. It adds no behaviour. Real-value release remains
NO-GO (Gates #72, #93, `docs/REAL_VALUE_RELEASE_CHECKLIST.md`).

Under the active governance, a chat instruction is task authorization, not
protocol authority; adopting any option below needs the canonical Decision process.

## 1. Retiring the transparent (V2) path

**Fact base.** V2 and V3 use different commitment generator bases, so one ledger
cannot serve both. A network's shielded genesis is committed under exactly one
basis, and the V3 verifier refuses a V2-basis genesis (test T05). There is no
converter and, deliberately, no fallback. So "retire V2" is not a switch: it is
either a new genesis or a state cutover.

| Option | What it means | Cost / risk |
| --- | --- | --- |
| A. New V3 network, V2 networks left to age out | Start any value-bearing network on V3 from genesis; never migrate V2 state | No migration code, no history rewrite. Existing V2 beta state is abandoned (it is test value). |
| B. Cutover with a published snapshot | Freeze V2, take a snapshot, re-issue each balance as a V3 genesis allocation | Needs state-history rules, an auditable mapping, and a way to deal with unspent-but-unspendable V2 outputs. Highest risk. |
| C. Keep both, forever, on separate networks | Status quo | Two verifiers and two bases to maintain and audit; the transparent path stays reachable. |

**Recommendation: A**, because every current balance is Beta test value that resets
each epoch anyway, and it avoids inventing a migration the audit would have to cover.
**Preconditions:** a decision that no real value ever lived on V2; the external
audit (#72) covering V3; the consensus change (if any) going through the Tier-F
amendment process. **Already in the repo:** the V3 verifier, the genesis-basis
check, and the composition tests. **Not touched:** V2 code stays.

## 2. Fee-policy activation

**Fact base.** `fee_policy_id` is signed into every V3 claim but `quote_fee_micro`
ignores it and returns a placeholder (1000 + 200 per input + 100 per output micro).
The fee is deterministic and balanced inside the proof, so it cannot be gamed per
claim, but nothing yet says who may change the policy or how it is calibrated.

Open questions for the decision-maker: (1) who may change a policy and with what
notice (it must stay separate from balances and from voting weight, P1); (2) the
calibration target: anti-spam floor versus cost recovery; (3) activation rule:
by epoch boundary so every validator quotes the same fee for the same block.

**Recommendation:** activate by epoch, with the policy id selecting a fee table
that is data in the network's genesis (not code), changeable only through the same
process as other consensus parameters. **Precondition:** the weak-device numbers
below, so the floor is not set below real verification cost. **Already in the
repo:** the signed `fee_policy_id`, deterministic `quote_fee_micro`, tests that
altering the policy after signing invalidates the spend (T07).

## 3. Weak-device acceptance baseline

**Fact base.** T22 is a measurement harness
(`mini-private-payment/tests/budget_v3.rs`, `--ignored`). Sandbox release numbers:
a 16-in/16-out claim is 51,052 of 65,536 bytes; build 0.30 s, verify 0.10 s. A
sandbox is not a phone.

**Needs hardware, not a decision alone.** Choose the lowest-end device the project
promises to support, run the harness there (release build, thermal steady state),
and record build and verify time and memory. **Recommendation:** pick the device
from the Android target list first, then set the maximum inputs/outputs per claim
from the measured verify time, and feed the same numbers into the fee floor above.
Until then no weak-device DoS claim is made (conformance T22 stays `partial`).

## Not in this note

Bridges and XRPL/Monero signers (separate threat model and audit), the external
audits themselves (procurement), and personhood credentials (research only).
