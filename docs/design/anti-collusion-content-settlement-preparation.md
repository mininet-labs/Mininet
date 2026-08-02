# Anti-collusion content/engagement settlement preparation (Track F5, D-0427)

**Status:** Phase-0 doctrine plus a completed, valueless Phase-2 falsification model (D-0428). No
`mini-settlement-integrity`, `mini-delivery-challenge`, or
`mini-settlement-audit` crate exists. No nullifier, accumulator, anonymous-
credential, or subsidy-issuance dependency is selected or added.
`mini-contribution` (D-0417), `mini-attest` Tier 0 (D-0404),
`mini-engagement`, `mini-execution`, and `mini-settlement` are unmodified.
They remain linkable prototype foundations, not an anti-collusion system.

**Origin and scope:** D-0421 §7 names the missing doctrine this document
fills. `docs/design/mn602-mn603-anonymous-resource-payment-preparation.md`
(D-0099) supplies the doctrine-first and staged-review precedent;
`mini-attest` Tier 0 and its proposed Tier 1/Tier 2 work supply adjacent
receipt/nullifier research; D-0417 supplies the existing requester-funded
publish → seed → request → deliver → receipt → settle path.

This document makes one necessary precision that D-0421's compressed problem
statement did not make explicit:

> An operator controlling requester and provider identities can fabricate
> unlimited *claims*, but cannot extract unlimited net value from a payment
> funded entirely by that operator's own balance. The protocol-loss threat
> begins when a third party, sponsor, treasury, emission budget, or other
> commons-funded mechanism pays the claim, or when the claim creates a
> non-monetary privilege that can be farmed.

That distinction is load-bearing. Anti-collusion machinery must protect
commons and third-party budgets without becoming a permission system over
ordinary voluntary payments.

## Decision

Track F5's one-line label, “provider payments,” covers economically different
operations. They must be different typed settlement classes, never one generic
“reward” path:

1. **Requester-funded market settlement.** The requester knowingly pays the
   full amount from its own finalized balance. Delivery evidence, replay
   protection, exact sequencing, and canonical finality are required; an
   anti-collusion credential is not. Self-dealing with one's own funds is not
   a protocol subsidy attack and must not require a personhood issuer,
   auditor's permission, or proof that two parties are socially independent.
2. **Sponsor-funded settlement.** A named third party escrows a finite budget
   under a precommitted policy. Claims may consume no more than that locked
   budget and may require policy-scoped rate limits, corroboration, or
   challenges. The sponsor cannot rewrite the policy after observing claims.
3. **Protocol-subsidized settlement.** A bounded, precommitted epoch budget
   pays for a public good such as under-covered crawling, historical retention,
   uncommon-language indexing, or privacy infrastructure. This is the class
   that requires the strongest anti-collusion controls. Unused capacity
   expires; there is no per-claim mint faucet and no “emergency” bypass.
4. **Authority-bearing settlement is forbidden.** No payment, receipt,
   provider revenue, audit result, or subsidy eligibility may improve organic
   ranking, moderation authority, personhood, governance weight, validator
   selection, reviewer quorum, or constitutional legitimacy.

**Availability rule:** requester-funded settlement remains usable if every
anti-collusion issuer, auditor, or subsidy service disappears. Sponsor-funded
and protocol-subsidized claims fail closed when their required proof or budget
state is unavailable. A convenience or subsidy subsystem may halt its own
class; it may never halt ordinary payments, public search, public publishing,
or local ranking.

**Finality rule:** anti-collusion review never rewrites canonical ownership.
Before finality, a policy may refuse or withhold a sponsor/protocol claim.
After finality, an objective fraud proof may affect only a separately defined
future subsidy entitlement or an unspent bonded reserve. It may not claw back a
final requester-funded payment, seize an unrelated balance, de-person a user,
or alter governance rights.

**Accounting/privacy split for subsidies:** subsidy origin, epoch, cap, and
remaining budget must be explicit in canonical accounting. Where the eventual
privacy construction permits it, the service provider should not learn whether
a valid spend credential came from a paid or subsidized allotment. “Hidden from
the provider” must never mean “unaccounted for by the ledger.”

## Role separation

Five operational roles remain separable. No future PR may collapse them
without a new decision and threat analysis:

1. **Requester/funder** — chooses the service and, for requester-funded
   settlement, bears its full cost. A requester signature proves consent and
   authorization, not human uniqueness or independence from the provider.
2. **Provider** — performs the typed service and supplies evidence. A provider
   proves the service transcript required by policy, never “genuine human
   demand,” social value, or independence from the requester.
3. **Settlement coordinator / claim constructor** — checks typed evidence and
   constructs claims. It does not submit on behalf of an unwilling payer and
   has no finality authority. Canonical finality remains exclusively the
   existing consensus/execution path.
4. **Collusion-limit credential issuer set** — exists only for settlement
   classes whose policy requires a scarce, rate-limited entitlement. It must
   be plural, thresholded or otherwise independently replaceable, and
   issuer-unlinkable if the selected reviewed construction supports that
   property. No single issuer may approve or deny individual beneficiaries,
   learn both issuance and spend context, alter a live epoch's cap, or become
   necessary for requester-funded settlement. If credible operational
   independence cannot be demonstrated, work stops at a valueless phase.
5. **Audit/challenge network** — permissionless verifiers apply deterministic
   sampling and objective fraud-proof rules. The rule and randomness-source
   commitment are fixed publicly before claims, but realized entropy must remain
   unpredictable and unbiasable until claims are immutable; no auditor or beacon
   operator chooses targets privately. Heuristic
   suspicion may guide local policy or research, but only a protocol-defined,
   independently verifiable proof may affect a sponsor/protocol budget. An
   auditor has no custody, minting, finality, personhood, ranking, or governance
   power.

The canonical ledger/consensus layer is deliberately not one of these
discretionary roles. It orders valid claims and preserves ownership; it does not
judge social usefulness or whether two real people colluded.

## What a resolution needs (D-0421 §7's requirements, made precise)

Any future implementation must satisfy all of the following:

- **Typed funding class and policy hash.** Every non-requester-funded claim is
  bound to an immutable policy identifying funding class, service class,
  epoch, budget commitment, cap rules, accepted evidence, nullifier domain,
  challenge rules, audit rule, and version. A policy change applies only to a
  future epoch.
- **No unbounded issuance.** A claim can spend only an existing requester
  balance, an already locked sponsor escrow, or a precommitted protocol budget.
  Total accepted claims cannot exceed that budget under any ordering,
  concurrency, replay, identity-splitting, or failure scenario.
- **Delivery integrity, honestly scoped.** Unpredictable challenges may prove
  that bytes or a typed service were delivered after a fresh request. They
  prevent precomputation, replay, and “store nothing” fraud. They do **not**
  prove requester/provider independence: two colluding parties can answer a
  real challenge and transfer real bytes. No document, test, or UI may claim a
  delivery challenge alone is an anti-collusion proof.
- **Cross-claim replay and duplicate resistance.** Claim identity, service
  transcript, policy, funding epoch, payer/sponsor budget, provider, and
  nullifier context are domain-separated and bound. Splitting one economic
  event into many receipts must not multiply a capped entitlement.
- **Privacy-preserving rate limiting where needed.** Sponsor/protocol policy may
  require a scoped nullifier or scarce-resource credential. It must reveal no
  stable cross-context identifier and must use a settlement-specific domain,
  distinct from governance, personhood, `mini-attest`, resource-payment, and
  search-query domains. This is reusable research, not permission to reuse a
  secret or nullifier directly across domains.
- **No false personhood claim.** Identity-root count is not human count.
  Until #18 produces an accepted, audited credential, no settlement phase may
  claim “one human, one reward.” A scarce-resource or deposit-bound limit may
  be researched as a different economic assumption, but it must be labeled as
  such and can never become governance weight.
- **Delayed, randomized, privacy-bounded audit.** Sampling rules and the
  randomness-source commitment are public before claims, while realized entropy
  is unavailable until the claim set is immutable. A known seed with variable
  claim inputs is grindable and fails closed. Audit proves only declared
  transcript/accounting predicates. It must not publish a global
  requester-provider graph, raw private query, protected content identifier,
  root DID, or stable activity history merely to make analysis easier.
- **Objective response only.** A valid fraud proof may reject an unfinalized
  subsidized claim, consume an explicitly bonded subsidy reserve, or reduce a
  future subsidy allowance under a predeclared rule. It may not reverse
  canonical finality, confiscate unrelated funds, lower humanness, or create a
  blacklist authority.
- **No pay-to-rank or pay-to-govern.** Provider earnings and funding class are
  absent from organic ranking inputs and every governance/personhood input.
  Search results may separately disclose that a service was sponsored, but
  payment cannot alter the organic score.
- **Weak-device and bounded-work requirements.** Verification, duplicate
  checking, and audit proofs have explicit size, memory, CPU, expiry, and
  pruning bounds before untrusted network use. An attacker cannot force an old
  phone to retain an unbounded receipt graph.
- **Role-disappearance safety.** Loss or compromise of an issuer/auditor set
  can halt only the affected sponsor/protocol program. It cannot forge
  requester authorization, spend balances, finalize claims, block ordinary
  search, or become a permanent network dependency.
- **Measured falsification criteria.** Before implementation, the project sets
  numeric limits for maximum budget loss under the modeled colluding set,
  duplicate false-negative rate, honest false-rejection rate, proof/linkability
  leakage, issuer concentration, audit coverage, and weakest-device cost.
  Metrics are chosen before the pilot results are known.

**Threat model floor.** The model includes an attacker controlling requester,
provider, creator, multiple identity roots, some issuers/auditors, the served
content, and the timing of genuine byte transfers; paid real humans; claim
splitting and routing through intermediaries; replay across services/epochs;
selective participation to evade samples; network metadata correlation; and
an adaptive strategy after public policies are known. The attacker is not
assumed able to forge signatures, break hashes, violate canonical consensus,
or spend a balance it does not control.

**Explicit non-goals.** This track cannot prove that a request reflects genuine
interest, that content is socially valuable, that two participants are
organizationally independent, that an issuer set is independent merely because
it has several keys, or that one root equals one human. Cryptography can bind
transcripts, caps, and hidden credentials; it cannot manufacture honest demand.

## Proposed (not yet built) crate boundaries

The names below are provisional responsibility boundaries, not an approved API
or permission to start code:

- `mini-settlement-integrity` — typed settlement-class/policy vocabulary,
  deterministic cap accounting, replay/nullifier verification interfaces, and
  proof verdicts. It holds no issuer secret, chooses no beneficiary, submits no
  claim, and depends on no governance crate.
- `mini-delivery-challenge` — fresh challenge/transcript construction and
  verification for a typed service. Its API and documentation must state that
  it proves delivery integrity only, not economic independence or usefulness.
- `mini-settlement-audit` — deterministic delayed sampling and objective
  fraud-proof verification over finalized claim commitments. It contains no
  discretionary “suspicious account” registry and cannot mutate balances,
  personhood, ranking, or governance state.

Credential issuance may eventually be a separately reviewed adapter or shared
infrastructure with `mini-attest`; this document does not preselect a scheme or
place issuer keys in any of the three crates. Simplicity review may collapse
pure-code modules later, but never the operational roles or authority
boundaries above.

## Voice/value wall (hard rule, restated for this track)

All proposed components live on the value/service side. None may be imported by
`mini-forge::governance`, governance tallying, validator vote-weight code,
personhood scoring, or reviewer-quorum code. No output named “fraud,” “audit,”
“provider reputation,” “service history,” or “subsidy eligibility” carries any
political authority. A provider may earn value; it gains exactly zero voice.

The wall is bidirectional: governance may set bounded future subsidy policy,
but may not select individual winning claims, retroactively rewrite an epoch,
award favored providers, or turn settlement data into a political loyalty
score.

## No new cryptography

Directive 14 applies in full. No hash, signature, accumulator, nullifier,
credential, proving system, or randomness primitive is invented here.
Candidate families named elsewhere (Privacy Pass, GNU Taler, Coconut/BBS-style
credentials, Semaphore-style nullifiers, standard accumulators and ZK systems)
remain research references, not selections.

Before a construction backs real MINI or private user activity it requires:

- a canonical protocol transcript and domain-separation registry entries;
- replay, downgrade, issuer-collusion, auditor-collusion, metadata, and recovery
  analysis;
- test vectors and malformed/oversized-input tests;
- two interoperable implementations for the proof-critical verifier path;
- mobile/weakest-device benchmarks;
- external cryptographic and privacy review under D-0047's production gate;
- independent economic/mechanism review against D-0073/D-0074 and
  `docs/gates/economic-simulation-spec.md`; and
- a separate governed activation decision for the exact reviewed version and
  parameters.

Internal tests, founder review, several agreeing AIs, or a threshold of keys do
not substitute for independence or external review.

## Phased path (mirrors mn602-mn603's nine phases)

0. **Doctrine — this document.** Freeze scope, non-goals, authority boundaries,
   settlement classes, and falsification gates before code.
1. **Existing linkable baseline, recognized rather than newly shipped.**
   D-0417 provides requester-funded claim construction from verified delivery
   evidence. `mini-attest` Tier 0 separately provides linkable engagement-
   backed review receipts. They are not integrated and neither is anti-
   collusion. This document starts no Phase-1 code.
2. **Executable threat/economic model and transcript specification.** Model
   colluding identities, genuine self-delivery, claim splitting, issuer/auditor
   compromise, budget races, and metadata leakage. Precommit numeric pass/fail
   thresholds. No live value or authority.
3. **Valueless delivery-challenge prototype.** Prove freshness, transcript
   binding, replay refusal, and bounded verification. State and test that
   colluding endpoints still pass when they genuinely serve bytes.
4. **Valueless settlement-integrity prototype.** Synthetic sponsor/protocol
   budgets, deterministic cap accounting, scoped duplicate/nullifier checks,
   issuer-outage behavior, and a second independent verifier implementation.
   No real personhood or “one-human” claim.
5. **Permissionless delayed audit/fraud-proof prototype.** Public sampling
   randomness, privacy-bounded commitments, objective verdicts, false-positive
   handling, pruning, and proof that no verdict can mutate finalized ownership.
6. **Integrated adversarial simulation and closed valueless system test.** Join
   Phases 3-5 against one narrow service class. Attack colluding pairs, many
   roots, paid humans, replay, timing, partial issuer/auditor capture, network
   partitions, and weakest-device resource exhaustion.
7. **Independent review.** Cryptographic, privacy, economic/mechanism,
   accessibility, implementation, and operational-independence findings are
   resolved. D-0047 remains a hard gate.
8. **Closed valueless pilot with rollback.** Publish residual risks and measured
   thresholds. Issuer/auditor disappearance must degrade only the pilot.
9. **Separate, limited real-value activation.** Only a new exact-state governed
   decision may activate one reviewed sponsor/protocol-funded service class,
   with a small precommitted budget and automatic shutdown at its cap. This
   phase additionally requires an accepted rate-limit assumption — audited
   personhood from #18 or an explicitly different scarce-resource model whose
   inequality and capture consequences are accepted in that activation. It
   does not delay or place a central issuer in the requester-funded market path.

No implementation phase is authorized by this document. Phase 1 predates it.
Phase 2 is now complete under D-0428 as a deterministic Python model with a
checked-in fixed report; it moves no value and selects no production
construction. Its genuine-delivery collusion vector drains 100% of the bounded
protocol budget against a precommitted 10% loss gate. A second attack gives
claimants the realized audit seed before claim construction; all 60 submitted
claims grind outside the 5% sample. The configured replay-state capacity is
9,600,000 estimated bytes against an 8 MiB ceiling, so that gate also fails.
Cross-policy semantic overlap remains unmeasured without a global activity
registry, so the report sets
`phase3_authorized` to `false`. Phases 3-9 remain unstarted and
unauthorized.

## Constitutional impact

No frozen invariant is amended and no authority is granted. The doctrine is
constrained by and strengthens Directives 2, 4, 5, 9, 14, 16, and 18: no
central dependency, canonical ownership remains final, privacy is structural,
complexity is bounded, money never becomes voice, and edge reward services can
disappear without taking the core with them.

## Implementation status

Doctrine plus D-0428's non-production Phase-2 model: deterministic Python
accounting/transcript state, adversarial tests, and an exact JSONL result vector.
There are still zero production F5 crate lines, zero new dependencies, zero
selected cryptographic constructions, and zero activation.

## Failure point

This doctrine fails if any implementation:

- subjects requester-funded voluntary payments to a central eligibility gate;
- calls delivery challenges “collusion proof”;
- treats identity roots as humans;
- lets one issuer or auditor choose who may earn;
- pays from an uncapped or retrospectively enlarged commons budget;
- builds a public requester-provider surveillance graph;
- converts a heuristic suspicion into balance seizure or de-personing;
- reverses canonical finality;
- lets payment affect organic ranking or governance; or
- continues operating a subsidy program after its objective cap/audit safety
  assumptions fail; or
- reveals target-selection entropy while claimants can still grind claim IDs;
  or
- reports a bounded-state PASS from friendly observed fixtures while the
  configured adversarial capacity exceeds the threshold.

This doctrine can drift. Every later F5 proposal must re-check its factual
references against then-current code and truth-sync this file and
`docs/STATUS.md` in the same proposal.

## Required follow-up

D-0428 completes Phase 2 and preserves its failed colluding-extraction gate.
The next work is still **not a nullifier crate** and not real-value activation.
A later, separately reviewed proposal may either narrow Phase 3 to a valueless
delivery-integrity prototype that explicitly admits genuine colluders pass, or
research an established scarcity construction, privacy-preserving policy-family
allocation rule, and delayed decentralized randomness construction capable of
bringing the declared colluding set below the precommitted 10% loss ceiling and
preventing adaptive sample grinding. Coordinate with roadmap #228/#229 so review- and
settlement-context derivations cannot collide, and with #18 without pretending
#18 is solved. No production anti-collusion or sponsor/protocol activation PR
should be accepted while the D-0428 authorization result remains false.

## Supersedes / superseded by

Fulfills D-0421's named Phase-0 follow-up and clarifies one imprecise reading of
D-0421 §7: ordinary requester-funded self-payment permits unlimited claim
*volume* but not unlimited net protocol extraction. The anti-collusion loss
model applies to third-party/commons-funded value and farmable side effects.
It does not supersede D-0421's composition-over-invention doctrine, D-0417's
requester-funded settlement baseline, D-0404's Tier-0 review receipts, or
D-0099's anonymous resource-payment preparation.