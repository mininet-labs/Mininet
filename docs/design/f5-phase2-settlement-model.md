# F5 Phase 2: settlement transcript, adversary/economic model, and falsification gates

**Status:** Draft work scope; active implementation/research claim.  
**Proposed decision:** D-0428, subject to collision check at merge time.  
**Branch:** `codex/f5-phase2-model`  
**Primary references:** D-0421 §7; D-0427; `docs/design/anti-collusion-content-settlement-preparation.md`; D-0417; D-0404; roadmap #175; roadmap #18; roadmap #228/#229.

This file exists immediately so parallel contributors can see that F5 Phase 2 is in progress and avoid independently designing the same surface. It is a scope reservation and working contract, not a completed model and not authorization to build a payment, credential, nullifier, issuer, audit, or subsidy implementation.

## Goal

Turn D-0427's Phase-0 doctrine into an exact, falsifiable Phase-2 model before any F5 implementation begins.

The completed PR must define:

1. a typed settlement-class and policy schema;
2. exact claim/transcript fields and domain boundaries;
3. the adversary and assumptions;
4. deterministic budget and attack accounting;
5. a privacy/linkability budget;
6. a valueless simulator or executable model with fixed test vectors; and
7. numeric pass/fail thresholds chosen before simulation results are inspected.

The output must be sufficient for an independent reviewer to answer, without trusting an operator or interpreting prose:

- what value source funds a claim;
- what exact budget can be lost;
- which fields are bound into one economic event;
- which replays, splits, duplicates, and cross-domain substitutions are invalid;
- what a delivery challenge proves and does not prove;
- what information each role learns;
- what happens when issuers, auditors, providers, requesters, or network links fail; and
- whether any settlement output can influence ranking, personhood, governance, validator selection, moderation, or review quorum.

## Settlement classes in scope

The model preserves D-0427's four-class distinction. It must never collapse these into one generic reward path.

### 1. Requester-funded market settlement

The requester knowingly pays the full amount from its own finalized balance.

Required properties:

- no anti-collusion issuer, auditor, or personhood service is required;
- no claim can spend more than the payer's canonical available balance;
- replay, duplicate, sequence, expiry, and canonical-finality rules still apply;
- self-payment may create arbitrary gross claim volume but cannot be counted as protocol or commons extraction; and
- this path remains available when every F5-specific issuer/auditor/subsidy service disappears.

### 2. Sponsor-funded settlement

A named third party escrows a finite budget under an immutable policy commitment.

Required properties:

- claims can consume only the already-locked sponsor budget;
- the policy cannot be rewritten after claims are observed;
- any required rate limit, challenge, duplicate rule, or audit rule is policy-scoped; and
- issuer/auditor outage halts only this program.

### 3. Protocol-subsidized settlement

A finite, precommitted epoch budget pays for a narrowly defined public-good service.

Required properties:

- no per-claim mint faucet;
- unused capacity expires rather than accumulating or rolling forward silently;
- total accepted claims remain at or below the epoch budget under every ordering and concurrency case;
- the strongest duplicate/rate-limit/audit controls apply here; and
- the program shuts down at its objective cap or when a declared safety assumption fails.

### 4. Authority-bearing settlement

Forbidden. The schema and model must make it impossible to treat payment, provider revenue, subsidy eligibility, audit outcomes, receipt history, or service volume as an input to:

- organic ranking;
- personhood or human evidence;
- governance or proposal ordering;
- validator selection or vote weight;
- moderation authority;
- reviewer quorum; or
- constitutional legitimacy.

## Planned typed schema

The completed model must define a canonical abstract schema before choosing any cryptographic construction. At minimum it will cover the following typed concepts.

### `SettlementPolicy`

- version;
- settlement class;
- service class;
- policy id / commitment;
- funding source and budget commitment;
- epoch and expiry;
- maximum claim amount;
- maximum program amount;
- accepted evidence predicates;
- duplicate/replay domain;
- optional rate-limit domain;
- challenge rule;
- audit sampling rule;
- fraud-proof rule;
- shutdown rule; and
- privacy disclosure declaration.

A policy change applies only to a future epoch. No live policy is mutable after claims can be formed against it.

### `SettlementClaim`

- version;
- policy commitment;
- settlement class;
- service class;
- request/event commitment;
- requester/funder commitment;
- provider commitment;
- typed delivery-evidence commitment;
- amount;
- funding epoch;
- expiry;
- replay/duplicate identifier;
- optional rate-limit/nullifier placeholder;
- claim id; and
- canonical finality reference or pending-state marker.

The model will not select a nullifier, anonymous credential, accumulator, proving system, signature suite, or issuer protocol. Any placeholder is an interface requirement only.

### `DeliveryChallengeTranscript`

The transcript will bind the typed request, fresh unpredictable challenge, service response, policy, parties/scoped pseudonyms, time/expiry window, and evidence commitment.

Its claim is intentionally narrow:

> A valid transcript may prove that a typed service or byte transfer occurred after a fresh challenge and was not a replay of an earlier transcript.

It must never be described as proving requester/provider independence, genuine demand, human attention, usefulness, organizational diversity, or one-human-one-claim. Colluding endpoints that genuinely perform the challenged service are expected to pass.

## Adversary model floor

The model must include an adaptive attacker controlling any combination of:

- requester, provider, creator, coordinator, or service content;
- many `did:mini` roots;
- paid or coerced real humans;
- some credential issuers or auditors;
- the timing and routing of genuine byte transfers;
- claim splitting across identities, intermediaries, services, or epochs;
- replay across policy, service, epoch, funding, review, personhood, and resource-payment domains;
- selective participation intended to evade delayed sampling;
- network delays, partitions, retries, reordering, and concurrent claims;
- metadata observation and timing correlation; and
- honest-looking self-dealing designed to maximize third-party or commons extraction.

The attacker is not assumed able to forge valid signatures, break the selected hash function, violate canonical consensus, or spend a balance it does not control.

## Accounting definitions

The model must distinguish these quantities explicitly:

- **gross claim volume:** sum of all submitted claims, including self-payments;
- **finalized transfer volume:** sum of canonically finalized transfers;
- **requester-funded net extraction:** zero by definition, excluding fee externalities, because the payer funds the transfer;
- **sponsor extraction:** sponsor escrow consumed by attacker-controlled providers/requesters;
- **protocol extraction:** protocol-subsidy budget consumed by attacker-controlled providers/requesters;
- **duplicate multiplication factor:** accepted economic claims divided by distinct modeled economic events;
- **budget overrun:** accepted program value above the precommitted budget;
- **honest false rejection:** valid honest claims rejected by the modeled policy; and
- **privacy/linkability leakage:** declared information learned or correlatable by each role.

The simulator and report must never use gross self-payment volume as a proxy for protocol loss.

## Required invariants

The executable model and its tests must encode at least these invariants:

1. **Budget conservation:** accepted sponsor/protocol claims never exceed the locked/precommitted budget under any claim order, replay, concurrency, or failure case.
2. **Requester sovereignty:** requester-funded settlement has no dependency on F5 credential issuers or auditors.
3. **No unbounded issuance:** every accepted claim is backed by an existing payer balance, locked sponsor escrow, or precommitted protocol budget.
4. **No duplicate multiplication:** one modeled economic event cannot consume a capped entitlement more than once within its declared policy domain.
5. **Cross-domain separation:** review, personhood, resource-payment, governance, search, and settlement identifiers cannot be substituted across domains.
6. **Finality preservation:** audit/challenge logic cannot reverse canonical ownership, confiscate unrelated balances, or mark a local result final.
7. **Role-disappearance safety:** issuer/auditor loss can halt only the affected sponsor/protocol program.
8. **Authority isolation:** no model output becomes ranking, personhood, governance, moderation, validator, or reviewer authority.
9. **Privacy boundedness:** the model does not require publication of a global requester-provider graph or stable activity identifier.
10. **Bounded verification:** proof/claim sizes, retained state, CPU, memory, expiry, and pruning requirements are explicit for the weakest supported device.

## Simulator / executable-model scope

The completed PR will contain a deterministic, valueless model kept outside all production settlement crates. It will model, at minimum:

- honest requester-funded transfer;
- requester/provider self-payment;
- finite sponsor escrow;
- finite protocol epoch subsidy;
- duplicate claim replay;
- claim splitting across identities;
- claim splitting across epochs/policies;
- concurrent budget races;
- real delivery between colluding endpoints;
- absent or compromised issuer subsets;
- absent or compromised auditor subsets;
- delayed random sampling;
- false fraud allegations;
- network partition/retry behavior;
- paid-human and many-root strategies; and
- weakest-device verification/retention cost inputs.

The model must use deterministic seeds and fixed vectors. It must report both successful attacks and rejected attacks rather than only a final pass/fail summary.

## Numeric falsification gates

The final PR must replace every `TBD` below with a number **before the first result-bearing simulator commit is reviewed**. Values cannot be chosen after seeing whether the model passes.

- maximum budget overrun: `TBD`;
- maximum protocol/sponsor loss under the declared colluding set: `TBD`;
- maximum duplicate false-negative rate: `TBD`;
- maximum honest false-rejection rate: `TBD`;
- maximum cross-context linkability/leakage score under the declared privacy model: `TBD`;
- minimum audit coverage/detection probability for the declared attack size: `TBD`;
- maximum issuer concentration / tolerated compromised fraction: `TBD`;
- maximum auditor concentration / tolerated compromised fraction: `TBD`;
- maximum verification CPU time on the weakest benchmark device: `TBD`;
- maximum verification memory on the weakest benchmark device: `TBD`;
- maximum retained state per policy epoch: `TBD`; and
- maximum claim/proof wire size: `TBD`.

A threshold missed by the model is a failure to revise the design or narrow the claim, not a reason to weaken the threshold after the fact.

## Files expected in this PR

This is the planned footprint. It may narrow after code/repository inspection, but it must not silently expand into production implementation.

- `docs/design/f5-phase2-settlement-model.md` — this scope, then the complete transcript/threat/economic model;
- a deterministic valueless simulator or executable model under a non-production path;
- fixed vectors and adversarial tests for the model;
- `docs/DECISION_LOG.md` — D-0428 only after the exact completed state is known;
- `docs/STATUS.md` — Phase 2 status and honest remaining gaps; and
- generated/navigation updates only if current repository policy requires them at merge time.

## Explicit non-goals

This PR will not:

- add `mini-settlement-integrity`, `mini-delivery-challenge`, or `mini-settlement-audit` production crates;
- add a nullifier, accumulator, anonymous credential, blind token, or proving-system dependency;
- select an issuer set or auditor set;
- activate real MINI, real subsidies, real sponsor funds, or real provider payment;
- modify requester-funded `mini-contribution` settlement behavior;
- claim one-human-one-reward;
- solve roadmap #18;
- implement F6 private query transport;
- create slashing, confiscation, blacklist, de-personing, or finality-reversal paths;
- add payment or provider revenue to organic ranking; or
- grant AI-authored evidence any approval weight.

## Merge gate

This PR must remain draft and must not merge until all of the following are true:

- every planned transcript field and state transition is specified;
- every assumption and non-goal is explicit;
- numeric falsification thresholds contain no `TBD`;
- the deterministic model and adversarial vectors are present and reproducible;
- requester-funded settlement is demonstrably independent of issuer/auditor availability;
- budget conservation holds under concurrent/replayed/split claims;
- delivery challenges are never represented as collusion proof;
- no public surveillance graph is required;
- no finality, personhood, ranking, moderation, governance, validator, or reviewer authority is added;
- D-0427 is truth-synced against then-current code;
- exact-head CI is green; and
- an independent human reviews the exact final head. AI drafting and AI review carry zero approval weight.

## Stop condition

This PR stops at D-0427 Phase 2. A passing model is not permission to begin Phase 3. Any delivery-challenge prototype, settlement-integrity prototype, credential construction, audit network, or real-value pilot requires its own later proposal and review.