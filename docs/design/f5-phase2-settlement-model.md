# F5 Phase 2: settlement transcript, adversary/economic model, and falsification gates

**Status:** Phase-2 model complete; implementation and activation remain unauthorized.  
**Decision:** D-0428, pending merge of the exact reviewed state.  
**Primary references:** D-0421 §7; D-0427; `docs/design/anti-collusion-content-settlement-preparation.md`; D-0417; D-0404; roadmap #18; roadmap #175; roadmap #228/#229.  
**Executable model:** `tools/f5_phase2_model.py`.  
**Adversarial tests:** `tools/test_f5_phase2_model.py`.  
**Exact-output test:** `tools/test_f5_phase2_vectors.py`.  
**Frozen vector:** `tools/fixtures/f5_phase2_report.jsonl`.

## Decision and overall result

Phase 2 is complete as a **deterministic, valueless falsification model**. It
specifies the settlement classes, policy and claim fields, delivery-challenge
transcript, validation order, accounting state, privacy declarations, threat
model, fixed vectors, and precommitted numeric gates required by D-0427.

It does **not** implement a production settlement system, a credential, a
nullifier, an issuer protocol, an audit network, a subsidy, or any movement of
MINI.

The result is deliberately not a success claim:

| Property | Result | Exact mechanism or failure |
|---|---|---|
| Requester-funded sovereignty | **PASS** | A requester-funded policy cannot contain an issuer threshold, auditor threshold, program budget, or collusion-limit domain. The fixed vector settles with zero issuers and zero auditors. |
| Budget conservation | **PASS** | Sponsor/protocol claims debit one finite precommitted budget only after all checks pass. All 24 tested orderings of a four-claim race converge without a negative balance or overrun. |
| Retry/replay idempotence | **PASS** | The exact same claim returns `AlreadyAccepted` and consumes zero additional value. A second claim for the same modeled economic-event commitment is rejected. |
| Cross-policy/domain substitution | **PASS** | Policy, class, service, epoch, event, transcript, duplicate domain, and rate-limit domain are bound into deterministic model commitments. |
| Canonical finality wall | **PASS** | The model rejects caller-supplied finality and gives audit evaluation no mutable ledger handle. Audit output can request future-program action only. |
| Authority isolation | **PASS** | The model exposes no ranking, personhood, governance, validator, moderation, reviewer, or constitutional-authority output. `AuthorityBearing` policy construction fails. |
| Delivery integrity | **PARTIAL** | A transcript binds a fresh challenge and typed response, but this abstract model has no real transport, clock, signature, storage, or cryptographic challenge implementation. |
| Collusion resistance | **FAIL** | One hundred attacker-controlled requester/provider pairs with unique roots/tags perform real delivery and drain 100% of a bounded protocol budget. The gate permits at most 10%. |
| Issuer/auditor independence | **PARTIAL** | Threshold counts and outage behavior are modeled, but no construction or independently operated set is selected or measured. Several keys may still be one authority. |
| Privacy | **PARTIAL** | The declared cross-context score is zero and root DIDs/raw queries are excluded, but policy-local pairwise linkability and network metadata are not eliminated or measured. |
| Weak-device cost | **PARTIAL** | Wire size, retained-state estimate, and abstract work are bounded; physical CPU and peak-memory measurements do not exist. |

**Overall Phase-2 judgment:** the accounting shell resists unbounded issuance,
replay, and central permission over voluntary payments, but the design does not
yet resist economically valid collusion. Therefore the generated report sets
`phase3_authorized` to `false`. This decision does not silently advance the
roadmap.

A later proposal may still request a narrowly valueless delivery-integrity
prototype, but it must state that genuine colluders are expected to pass and
must not call that prototype anti-collusion. No sponsor-funded or
protocol-subsidized real-value activation is justified by this model.

## 1. Scope boundary

The model answers four questions:

1. What exact value source backs each accepted claim?
2. What fields define one modeled economic event and one policy domain?
3. What can be rejected mechanically without a trusted authority judging
   social intent?
4. Which anti-collusion claims remain false after the mechanical protections
   succeed?

It does not attempt to decide whether demand is genuine, content is useful, two
participants are independent, a DID root is a human, or an operator is honest.
Those are not derivable from delivery evidence.

All quantities are **model units**, not MINI. Model commitments use SHA-256 only
to produce compact deterministic vectors. That use does not select a production
hash, signature, transcript, anonymous credential, nullifier, or proof system.

## 2. Settlement classes

### 2.1 Requester-funded market settlement

The requester knowingly pays the full amount from an existing finalized
balance.

Structural rules:

- `program_budget_units == 0`;
- `issuer_threshold == 0`;
- `auditor_threshold == 0`;
- `rate_limit_domain == None`;
- no F5 issuer, auditor, personhood service, or subsidy coordinator is required;
- insufficient payer balance fails before mutation; and
- requester/provider self-payment counts as gross transfer volume but as zero
  protocol or sponsor extraction.

This class remains usable if every F5-specific service disappears.

### 2.2 Sponsor-funded settlement

A named sponsor locks a finite budget under an immutable policy commitment.

Structural rules:

- the budget is positive and finite;
- a single claim cannot exceed the whole program budget;
- issuer and auditor availability thresholds are positive;
- a settlement-specific rate-limit domain is mandatory;
- claims debit only the already registered program budget; and
- an outage halts this program without affecting requester-funded settlement.

### 2.3 Protocol-subsidized settlement

A finite epoch budget pays for a narrowly named public-good service.

It has the same mechanical budget and fail-closed rules as sponsor settlement,
plus the doctrine requirement that unused capacity expires and cannot become a
silent rollover or per-claim mint faucet. The Phase-2 model contains one epoch
at a time and therefore proves only the no-overrun property inside that epoch;
it does not implement epoch transition or expiry disposal.

### 2.4 Authority-bearing settlement

Forbidden. `SettlementPolicy.__post_init__` rejects this class. No settlement
receipt, revenue, audit verdict, service count, subsidy eligibility, or model
score can become an input to:

- organic ranking;
- personhood or human evidence;
- governance, proposal ordering, or vote weight;
- validator selection;
- moderation authority;
- reviewer quorum; or
- constitutional legitimacy.

## 3. Typed policy schema

`SettlementPolicy` contains:

| Field | Meaning |
|---|---|
| `version` | Model schema version; exactly `1`. |
| `settlement_class` | Requester-funded, sponsor-funded, or protocol-subsidized. |
| `service_class` | Typed service, currently a model enum rather than an extensible production registry. |
| `policy_name` | Human-readable bounded identifier used only inside the model. |
| `funding_source_commitment` | Commitment-like identifier for payer balance, sponsor escrow, or epoch budget. |
| `epoch` | Non-negative funding epoch. |
| `starts_at_ms`, `expires_at_ms` | Model validity window. |
| `program_budget_units` | Zero for requester-funded; positive finite cap otherwise. |
| `max_claim_units` | Positive per-claim cap. |
| `duplicate_domain` | Settlement-specific economic-event domain. |
| `rate_limit_domain` | Absent for requester-funded; settlement-specific for sponsor/protocol classes. |
| `challenge_required` | Whether a delivery transcript is required. Fixed to true in supplied vectors. |
| `issuer_threshold` | Availability count only; not a selected cryptographic threshold. |
| `auditor_threshold` | Availability count only; not proof of independent operators. |
| `audit_sample_bps` | Precommitted public sample rate in basis points. |
| `max_retained_keys` | Per-policy bound on event and rate-tag replay keys. |
| `max_claim_proof_wire_bytes` | Combined model claim/transcript byte cap. |
| `max_abstract_verification_ops` | Abstract work cap; not physical CPU time. |
| `privacy` | Role-by-role disclosure declaration. |

The deterministic `policy.commitment` binds the complete policy. There is no
method to edit a registered policy in place. A changed field produces a new
commitment and must apply to a future policy/epoch.

## 4. Typed delivery transcript

`DeliveryChallengeTranscript` binds:

- model version and delivery domain;
- policy commitment;
- settlement class and service class;
- request/economic-event commitment;
- policy-scoped requester and provider identifiers;
- challenge and response commitments;
- issue and expiry times; and
- a derived evidence commitment.

The verifier checks all bindings, checks the current model time is inside the
challenge window, and checks the transcript expires no later than the claim.

The only permitted interpretation is:

> The abstract transcript is consistent with a typed response after a fresh
> challenge and is not a replay of a different bound transcript.

It does **not** establish independent control, genuine interest, human
attention, useful content, organizational diversity, physical proximity, or
one-human-one-claim. The failing collusion vector exists to prevent later code
or user interfaces from quietly making that false inference.

## 5. Typed claim schema

`SettlementClaim` contains:

| Field | Bound meaning |
|---|---|
| `version` | Model schema version. |
| `policy_commitment` | Exact immutable policy. |
| `settlement_class`, `service_class` | Must match the policy. |
| `request_event_commitment` | Caller-supplied modeled economic-event commitment. |
| `requester_scope` | Policy-scoped requester label. |
| `funder_commitment` | Existing payer or registered budget source. |
| `provider_scope` | Policy-scoped provider label. |
| `delivery_evidence_commitment` | Transcript evidence commitment. |
| `amount_units` | Positive amount at or below the policy cap. |
| `funding_epoch` | Must equal policy epoch. |
| `expires_at_ms` | Must be current and inside policy expiry. |
| `duplicate_identifier` | Derived from duplicate domain, policy, service, epoch, and event commitment. |
| `rate_limit_tag` | Optional scoped placeholder; mandatory only for sponsor/protocol policies. |
| `claim_id` | Commitment over every claim field plus the model claim domain. |
| `finality_reference` | Must be absent on submission; only the canonical model creates one. |

The rate-limit tag is deliberately a placeholder. The model accepts no claim
that substitutes a personhood, review, resource-payment, governance, or other
non-settlement domain, but it does not prove that the tag was honestly issued,
scarce, unlinkable, or controlled by a unique human.

## 6. Domain registry

The Phase-2 model uses distinct labels:

- `mininet/f5/settlement-claim/model-v1`;
- `mininet/f5/delivery-challenge/model-v1`;
- `mininet/f5/delivery-evidence/model-v1`;
- `mininet/f5/settlement-duplicate/v1/<policy-name>`;
- `mininet/f5/rate-limit/v1/<policy-name>`; and
- `mininet/f5/model-commitment/v1`.

These are model namespace labels, not production wire tags. Any later
construction must enter the repository-wide domain-separation registry and be
reviewed against #228/#229, personhood, resource payment, review, governance,
and private-query domains.

## 7. Validation and mutation order

`SettlementModel.submit` performs checks before mutation in this order:

1. locate the exact registered policy;
2. reject the forbidden authority class;
3. match policy commitment, settlement class, and service class;
4. match funding epoch;
5. enforce policy and claim expiry windows;
6. reject caller-supplied finality;
7. enforce positive amount and per-claim cap;
8. recompute and verify claim ID;
9. recompute and verify the economic-event duplicate identifier;
10. return `AlreadyAccepted` for an exact accepted retry;
11. enforce combined wire-size and abstract-work caps;
12. require and verify the delivery transcript when policy requires it;
13. reject a previously accepted economic event in the policy domain;
14. require, domain-check, and deduplicate the rate-limit tag where required;
15. fail closed before exceeding retained replay-state capacity;
16. check issuer/auditor availability only for sponsor/protocol classes;
17. check payer balance or program budget; and
18. atomically debit one source, credit the modeled recipient, record replay
    keys, and create a canonical model finality reference.

No rejection after step 17 mutates value or replay state. The exact same claim
is idempotent; it does not charge twice.

`submit_canonical_batch` sorts claims by claim ID before submitting them so all
permutations produce one deterministic fixed vector. **This is test-model
ordering only.** Claim-ID ordering must not become a production allocation
rule: an attacker may be able to grind event data or identifiers to obtain an
earlier ID. A later design must select and review a non-grindable allocation
rule or explicitly accept first-finalized ordering and its capture effects.

## 8. Accounting state and definitions

The model stores:

- immutable policies by commitment;
- initial and remaining sponsor/protocol budgets;
- requester balances;
- provider receipts;
- accepted claim records;
- accepted economic-event identifiers;
- accepted policy-scoped rate tags; and
- attempted and finalized volume counters.

Definitions:

- **gross claim volume:** sum of submitted amounts, including rejected claims
  and self-payments;
- **finalized transfer volume:** sum of newly accepted transfers;
- **requester-funded net protocol extraction:** zero by definition in this
  model, because the requester supplies the value; fee and resource
  externalities are excluded and remain a separate concern;
- **sponsor extraction:** sponsor budget received by an attacker-controlled
  requester or provider scope;
- **protocol extraction:** protocol budget received by an attacker-controlled
  requester or provider scope;
- **budget overrun:** accepted program value beyond the registered initial cap;
- **duplicate false negative:** a second accepted spend for the same modeled
  economic-event identifier or rate tag;
- **honest false rejection:** rejection of a vector declared honest before
  execution; and
- **cross-context leakage score:** the maximum role score for explicitly
  forbidden bridge fields, not a general anonymity metric.

Requester-funded conservation requires the sum of modeled payer/recipient
balances to remain constant. Sponsor/protocol conservation requires:

`remaining_budget + accepted_receipts_for_policy == initial_budget`.

## 9. Adversary and assumptions

The attacker may control any combination of:

- requester, provider, creator, coordinator, and served content;
- arbitrarily many `did:mini` roots;
- paid or coerced real humans;
- some issuer or auditor identities;
- genuine byte transfers and their timing;
- request/event labels;
- claim splitting across identities, intermediaries, services, policies, and
  epochs;
- replay, retry, reordering, and concurrent submission;
- selective participation intended to avoid samples; and
- network metadata observation and timing correlation.

The attacker is not assumed able to:

- break the model commitment function;
- mutate the model atomically during one submit call;
- spend a requester balance it does not control;
- exceed a registered budget without the model detecting it; or
- forge a future production signature, because no production signature exists
  here.

Three assumptions are intentionally exposed rather than hidden:

1. `request_event_commitment` is supplied by the scenario. If an attacker can
   create a fresh commitment for semantically identical work, the duplicate
   rule cannot recognize that equivalence.
2. Rate tags are supplied placeholders. Unique strings are not proof of scarce
   entitlement, personhood, issuer independence, or honest issuance.
3. `Availability` is a count. Several available keys may still be controlled by
   one organization or one machine.

Those assumptions are the exact point where a purely mechanical model stops
being anti-collusion.

## 10. Privacy declaration and limits

The default declaration exposes only policy-scoped commitments and coarse epoch
information appropriate to each model role. It excludes root DID, raw query,
raw content identifier, exact timestamp, and cross-domain stable identifier.
The precommitted cross-context leakage score is:

- root DID: 100;
- cross-domain stable identifier: 100;
- raw query: 50;
- raw content identifier: 25; and
- exact timestamp: 10.

The gate takes the maximum role score and requires exactly `0`. A deliberately
leaky vector exposing root DID and raw query scores `150` and is rejected.

This metric is narrow. A policy-scoped requester/provider identifier can still
create a graph **within one policy**, and timing/network observations can link
otherwise separate contexts. The model does not claim unlinkability. Any later
prototype must measure policy-local graph exposure, issuer issuance/spend
linkage, provider linkage, auditor linkage, and transport metadata separately.
It must not publish a global requester-provider graph to make fraud analysis
convenient.

## 11. Audit semantics

Audit sampling is deterministic from a precommitted public seed, claim ID, and
policy sample rate. The model uses a 5% rate. For 60 independently sampleable,
objectively invalid claims, the modeled probability of sampling at least one is
9,539 basis points, above the 9,500-basis-point gate.

That PASS does **not** apply to genuine-delivery collusion. A colluding claim
that satisfies every objective transcript/accounting predicate is not an
objectively invalid claim, so more sampling does not reveal its social intent.
This is why the audit gate passes while the colluding-extraction gate fails.

`evaluate_audit_allegation` accepts either a heuristic allegation or an
objective transcript-failure allegation. It has no mutable settlement state.
A heuristic or false allegation causes no action. A valid objective proof may
request a future-program halt, but cannot reverse canonical ownership,
confiscate unrelated funds, alter personhood, or create a blacklist.

## 12. Precommitted falsification thresholds

The following values were committed before the fixed result vector was frozen:

| Gate | Threshold | Reason |
|---|---:|---|
| Budget overrun | 0 model units | A finite budget is a hard accounting invariant, not a probabilistic target. |
| Colluding extraction | at most 1,000 bps of the declared program budget | A bounded pilot that can lose more than 10% to the declared colluding set is not an acceptable anti-collusion design. This is a pilot research ceiling, not an accepted production-loss rate. |
| Duplicate false negatives | 0 bps in fixed duplicate vectors | Exact replay and exact modeled-event duplication must never multiply value. |
| Honest false rejection | at most 100 bps | A future measured system must not protect a subsidy by broadly excluding honest participants. The current fixed honest set is small and therefore only a structural check. |
| Cross-context leakage score | 0 | Root or cross-domain identifiers are forbidden by construction. |
| Audit detection | at least 9,500 bps for 60 objectively invalid claims | A large repeated objective fraud campaign should be detected with at least 95% probability under the declared sample rule. |
| Compromised issuers | at most 3,333 bps | More than one third compromised is outside the tentative tolerance; no issuer construction is selected, so result remains unmeasured. |
| Compromised auditors | at most 3,333 bps | Same operational-concentration ceiling; key count alone is insufficient. |
| Physical verification CPU | at most 50 ms per claim on the weakest benchmark device | Keeps verification interactive and abuse-bounded; unmeasured in Phase 2. |
| Physical verification memory | at most 4 MiB peak | Keeps proof verification viable on weak devices; unmeasured in Phase 2. |
| Retained state | at most 8 MiB per policy epoch | Prevents an unbounded receipt graph on a participant device. |
| Claim plus proof wire size | at most 16 KiB | Bounds untrusted input before any production proof scheme is selected. |
| Abstract verification work | at most 10,000 model operations | Deterministic complexity guard only; cannot substitute for CPU measurement. |

A missed threshold is not permission to weaken it after observing the result.
The design must change, the claim must narrow, or the program must stay off.

## 13. Fixed-vector results

`tools/fixtures/f5_phase2_report.jsonl` is byte-for-byte compared with
`render_report()` in CI. A semantic model change therefore requires an explicit
reviewed vector change.

| Vector | Result | Observation |
|---|---|---|
| Requester-funded settlement with all F5 roles absent | **PASS** | One claim settles; no issuer/auditor dependency appears. |
| Requester/provider self-payment | **PASS** | 40 units of volume, unchanged aggregate balance, zero protocol extraction. |
| Finite sponsor budget | **PASS** | Two 40-unit claims accepted, third rejected, 20 of 100 remain. |
| Four-claim concurrent race | **PASS** | All 24 input permutations produce the same three accepted claims and zero overrun. |
| Exact network retry | **PASS** | Second submission is `AlreadyAccepted` with zero spend. |
| Same event under different identity scopes | **PASS** | Claim IDs differ but the event duplicate identifier matches; second spend is rejected. |
| Cross-policy/epoch substitution | **PASS** | Rejected before spend. |
| Personhood-domain tag substituted into settlement | **PASS** | Rejected for rate-limit domain mismatch. |
| Issuer/auditor outage | **PASS** | Market claim succeeds; sponsor claim fails closed. |
| Retained-state exhaustion | **PASS** | New claim is rejected instead of evicting replay state. |
| False or heuristic audit allegation | **PASS** | No canonical state mutation. |
| Root-DID/raw-query privacy declaration | **PASS** | Deliberately leaky declaration exceeds the zero-leakage gate. |
| Genuine-delivery collusion | **FAIL** | 100 attacker claims drain all 1,000 program units; loss is bounded but not prevented. |

Measured gates from the frozen output:

- budget overrun: **PASS**, observed `0`;
- colluding extraction: **FAIL**, observed `10,000 bps` versus `1,000 bps`;
- duplicate false negatives: **PASS**, observed `0 bps`;
- honest false rejection in the declared vector: **PASS**, observed `0 bps`;
- cross-context leakage score: **PASS**, observed `0` under the narrow declared
  metric;
- audit detection for 60 objectively invalid claims: **PASS**, observed
  `9,539 bps`;
- issuer concentration: **PARTIAL**, no construction/operator set;
- auditor concentration: **PARTIAL**, no operator set;
- physical CPU: **PARTIAL**, no weakest-device benchmark;
- physical memory: **PARTIAL**, no allocator benchmark;
- retained state: **PASS**, observed estimate `19,200 bytes`;
- claim plus transcript: **PASS**, observed maximum `1,521 bytes`; and
- abstract work: **PASS**, observed maximum `848` operations.

## 14. Exact failures and engineering consequences

### 14.1 Real-delivery collusion drains the whole bounded subsidy — **FAIL**

**Location:** `run_fixed_vectors`, vector
`real-delivery-collusion-drains-the-bounded-program`.

**Failure:** delivery challenges reject replay and non-delivery, but colluding
endpoints can deliver real bytes. With many roots and unique placeholder tags,
they consume 100% of the budget.

**Long-term solution:** select no production mechanism yet. First define and
externally review one explicit scarcity assumption—audited unique-human
credential, a separately accepted scarce-resource model, or a narrowly capped
allocation mechanism—and rerun the same colluding set. The model must fall below
the 10% gate without giving an issuer discretionary permission over ordinary
requester-funded settlement.

### 14.2 Economic-event equivalence is caller supplied — **PARTIAL**

**Location:** `request_event_commitment` and derived `duplicate_identifier`.

**Failure:** the model catches two claims using the same event commitment, but
cannot know that two different commitments describe the same semantic work.

**Long-term solution:** each service class needs an externally reviewable,
canonical event definition derived from immutable request/service facts, plus
an explicit statement of which semantically similar events are intentionally
allowed to be paid twice. Do not add a central event adjudicator.

### 14.3 Rate-limit scarcity is not implemented — **PARTIAL**

**Location:** `ScopedRateTag` and scenario-supplied tag values.

**Failure:** domain separation and duplicate use are checked, but anyone in the
model can invent a fresh value.

**Long-term solution:** later research must compare established constructions
for issuer-unlinkable, policy/epoch-scoped scarcity. It must prove outage and
multi-issuer behavior, avoid cross-context linkage, and pass D-0047 review.
Identity-root count is not an acceptable stand-in for humans.

### 14.4 Deterministic claim-ID ordering is grindable if copied into production — **PARTIAL**

**Location:** `submit_canonical_batch`.

**Failure:** sorting makes simulations order-independent, but a production
claimant may manipulate inputs to obtain an earlier ID when a budget is scarce.

**Long-term solution:** a later allocation design must use a reviewed
non-grindable public randomness/batch rule, pro-rata rule, or an explicitly
accepted canonical-finality rule. The choice must be policy-visible and cannot
be selected by an operator after claims arrive.

### 14.5 Threshold counts do not prove independent authorities — **PARTIAL**

**Location:** `Availability.issuers_available` and `auditors_available`.

**Failure:** three keys on one administrator's infrastructure still satisfy the
model count.

**Long-term solution:** specify operational-independence evidence, disjoint
failure domains, rotation, recovery, and capture handling. A single issuer or
auditor must never become necessary for requester-funded settlement.

### 14.6 Privacy is declared, not cryptographically achieved — **PARTIAL**

**Location:** `PrivacyDeclaration`.

**Failure:** cross-context bridge fields are excluded, but policy-local graphs,
issuance/spend linkage, network timing, and access patterns remain.

**Long-term solution:** select standard reviewed privacy constructions only
after a role-by-role metadata analysis. Pairwise identifiers must rotate by
policy/epoch, and auditing must operate over commitments/proofs rather than a
published social graph.

### 14.7 Retained-state exhaustion can halt a subsidy program — **PARTIAL**

**Location:** `max_retained_keys` fail-closed check.

**Failure:** refusing to evict replay state prevents duplicate spend but lets an
attacker fill the finite state allowance and stop new claims.

**Long-term solution:** define epoch expiry, authenticated compaction,
checkpointing, and bounded archival proofs before network use. The safe failure
is program-local halt, never eviction that silently re-enables replay.

### 14.8 Physical weak-device cost is unmeasured — **PARTIAL**

**Location:** abstract operation, wire, and retained-state estimates.

**Failure:** Python object/JSON costs do not predict a low-end phone's real
cryptographic verifier cost.

**Long-term solution:** benchmark the exact selected proof/verifier on the
weakest supported devices and keep the precommitted 50 ms/4 MiB/16 KiB gates.

## 15. What this model proves and does not prove

It proves inside the deterministic model that:

- every accepted transfer has exactly one modeled source;
- requester-funded settlement does not depend on F5 gatekeepers;
- sponsor/protocol budget cannot go negative;
- exact retry and same-event replay do not multiply spend;
- policy/epoch/domain substitutions fail;
- bounded input/state checks fail closed;
- audit evaluation cannot rewrite finality; and
- the full report is reproducible byte-for-byte.

It does not prove:

- production cryptographic security;
- unique-human control;
- issuer or auditor independence;
- genuine demand or useful service;
- semantic event uniqueness;
- anonymous or unlinkable settlement;
- fair scarce-budget allocation;
- physical performance;
- network partition liveness;
- production atomicity across processes; or
- resistance to genuine-delivery collusion.

## 16. Reproduction

From the repository root:

```sh
python3 tools/f5_phase2_model.py
python3 tools/f5_phase2_model.py --check tools/fixtures/f5_phase2_report.jsonl
python3 -m unittest tools/test_f5_phase2_model.py
python3 -m unittest tools/test_f5_phase2_vectors.py
```

The ordinary governance workflow also runs every `tools/test_*.py` file. The
fixed-vector test fails if output changes without an explicit fixture update.

## 17. Phase transition rule

Merging D-0428 records the model and its failures. It authorizes **zero**
production crates, credentials, issuers, auditors, subsidies, or real-value
claims.

A later Phase-3 proposal must:

1. cite this exact fixed report;
2. remain valueless;
3. limit its claim to delivery freshness/transcript integrity;
4. state that real colluders can pass;
5. introduce no requester-funded permission gate;
6. select no real-value activation path; and
7. receive independent exact-head human review.

Any later proposal claiming anti-collusion or activating sponsor/protocol value
must additionally resolve or explicitly narrow every FAIL/PARTIAL item above,
meet D-0047, and publish new precommitted attack thresholds before results are
known.

## 18. Failure point

D-0428 fails if it is used to claim that F5 anti-collusion is implemented, that
delivery proves independent demand, that identity roots are humans, that
threshold key count proves independent operators, or that a bounded 100% budget
drain is acceptable merely because no overrun occurred.

It also fails if a later implementation copies model-only claim-ID ordering,
model-only SHA-256 commitments, arbitrary rate tags, or abstract timing into a
production protocol without a separate exact-state decision and external
review.

## 19. Implementation status

Phase 2 only:

- one deterministic Python model;
- one adversarial unit-test suite;
- one exact-output test;
- one checked-in JSONL fixed vector;
- no external dependency;
- no production crate;
- no selected cryptographic construction;
- no wallet, chain, search, ranker, personhood, governance, or provider behavior
  changed; and
- no activation.

Phase 3 and every later phase remain unstarted and unauthorized.
