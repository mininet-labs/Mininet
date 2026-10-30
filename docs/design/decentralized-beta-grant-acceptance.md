# Decentralized Beta MINI grant acceptance

**Issue:** #339  
**Parent milestone:** #334  
**Status:** implementation scope for a stacked draft PR; Beta-only and non-production

## Exact failure being closed

`mini-beta::BetaMiniLedger` deliberately validates accounting after a grant object has already been selected, but it does not decide which grant proposals are accepted by a shared beta network. Treating the Founder, GitHub, a faucet server, or one signing key as the missing selector would create a trusted mint.

This work adds a separate acceptance layer. The accounting core remains isolated from production value/governance dependencies.

## Chosen mechanism

Use **campaign-scoped threshold authorization with deterministic reward rules**.

A signed Beta grant policy:

- links one exact beta campaign and epoch;
- contains a bounded set of operational authorization DIDs;
- requires at least three distinct authorization DIDs;
- requires a threshold of at least two distinct signers for every accepted grant;
- uses separate testing and participation thresholds;
- fixes one exact testing-grant amount;
- limits participation awards to an explicit ordered set of reward bands;
- has a campaign-bounded validity window and expires automatically; and
- carries no governance, release, personhood, reviewer-rank, or production-MINI semantics.

A grant proposal becomes acceptable only when enough distinct policy members sign approval objects binding the **exact content-addressed grant id and exact policy id**. Recipient identity, GitHub account, wealth, prior rewards, contribution count, balance, employer, and political status are absent from the threshold calculation.

The accepted grant is still re-validated by `BetaMiniLedger`, including campaign linkage, per-campaign testing cap, epoch match, total supply cap, and one-contribution/one-participation-award enforcement.

## Why the other candidate mechanisms are not used alone

### Self-service faucet only — rejected

Without mature unique-human personhood, per-account self-service limits are Sybil limits on keys, not humans. A user can create more accounts. A self-service faucet can still exist later as a UX convenience behind the same distributed acceptance policy, but it is not sufficient as canonical issuance evidence.

### Contribution/reward bands alone — rejected

Deterministic amounts reduce discretion but do not answer who is allowed to attest that the underlying contribution is accepted.

### One threshold signer set forever — rejected

Long-lived authorization keys become an oligarchic mint. Policies are campaign-scoped and expire. A later campaign must publish a fresh bounded policy. In-campaign policy rotation is intentionally deferred until Forge has a canonical conflict/finality rule; inventing a local tie-break between competing policy forks would create retroactive validity changes.

### Token/stake weighted authorization — rejected

Balance, grant volume, contribution quantity, and payment cannot create mint or governance authority.

## Bootstrap trust that remains

The campaign record author anchors the policy for the campaign. This is a **temporary setup authority**, not an issuer: the policy is invalid unless it requires multiple distinct authorization DIDs and no one authorization DID can satisfy either threshold.

This does **not** prove the authorization DIDs belong to independent humans. `did:mini` is not personhood. A malicious bootstrap actor could control several keys. The implementation must label this as residual Pre-Go-Live centralization rather than claiming human decentralization that is not proven.

The long-term fix is #338: Forge-native governance/personhood selects and rotates operational authorizers, GitHub becomes a mirror, and the bootstrap anchor is removed one-way.

## Deterministic rules

### Policy validity

A policy is valid only if:

1. it is a strict signed `mininet.beta/grant-policy/v1` object;
2. its campaign exists and strictly parses;
3. policy author equals the campaign record author;
4. policy epoch equals campaign epoch;
5. its validity interval is non-empty and contained in the campaign interval;
6. it contains 3–32 unique authorization DIDs;
7. testing and participation thresholds are each `>= 2` and `<= member_count`;
8. testing amount is non-zero and no greater than the campaign testing cap;
9. participation reward bands are non-empty, strictly increasing, unique, bounded, and non-zero; and
10. no policy field carries vote weight or production-value conversion semantics.

### One-policy-per-campaign safety rule

A node MUST fail closed if its verified object set contains more than one valid grant policy for the same campaign. It MUST NOT choose a winner by:

- object id;
- timestamp;
- arrival order;
- GitHub/repository state;
- balance or stake;
- authorizer count beyond the declared threshold; or
- whichever policy was seen first locally.

`resolve_unique_campaign_policy` enforces this rule before threshold acceptance. The reason is temporal safety: a grant that was locally accepted under policy A must not silently become governed by policy B merely because policy B replicated later. Until #337/#338 provide canonical replicated conflict/finality resolution, a competing valid policy is an explicit stop condition rather than an excuse to manufacture local finality.

This does not make already-executed distributed side effects magically reversible. Therefore a durable/shared product integration MUST NOT execute grants before the canonical policy/finality layer exists. The current reference wrapper is test-domain accounting only.

### Approval validity

A signed `mininet.beta/grant-approval/v1` object counts only if:

- it links the exact policy id and exact grant id;
- its author is one of the policy authorization DIDs;
- its timestamp is not before the grant and is within the policy window;
- the grant matches the policy campaign and epoch; and
- that author has not already been counted for the same grant.

Multiple approvals from the same DID count once, never as extra weight.

### Testing grants

Testing grants:

- require the testing threshold;
- have no contribution link;
- must use the policy's exact testing amount;
- remain bounded by the campaign cap and epoch supply cap; and
- create no personhood/reputation/governance state.

### Participation grants

Participation grants:

- require the stronger/equal participation threshold configured by policy;
- must reference an accepted contribution in the same campaign;
- must use one of the policy's explicit reward bands; and
- remain subject to the ledger's one-contribution/one-award rule.

## Equivocation and convergence

An authorization DID can sign conflicting grant proposals. That is evidence of equivocation, but this layer must not pretend an eventually replicated object graph provides instant BFT finality.

The safe rule in this PR is:

- threshold validation is deterministic for one exact grant and one complete approval set;
- duplicate participation minting is blocked by the accounting core even if reviewers sign competing grant ids;
- competing valid policies for one campaign fail closed instead of using a local tie-break;
- tests must prove two independently constructed nodes return the same acceptance result once they possess the same immutable objects;
- tests must also prove stale/wrong-epoch/wrong-policy/duplicate-signer evidence cannot satisfy a threshold; and
- durable shared execution must not claim rollback-free finality until #337/#338 provide replicated state resolution and canonical conflict handling.

A future canonical Forge resolver may additionally publish authorizer-equivocation evidence and exclude compromised operational keys. That exclusion mechanism must itself be governed; local software must not silently blacklist people or keys.

## Crate boundary

Implement the acceptance mechanism in a new `mini-beta-grants` crate rather than importing Forge, treasury, consensus, production value, or governance into `mini-beta`.

Allowed runtime dependencies:

- `did-mini`
- `mini-beta`
- `mini-objects`
- `mini-store`

Forbidden authority shortcut dependencies include production value, settlement, treasury, chain/consensus, personhood/economy, and GitHub APIs.

The shared-beta wrapper may call `BetaMiniLedger::apply_grant` only after threshold acceptance succeeds.

## Adversarial acceptance tests

The PR is not complete without tests for:

- one signer cannot mint;
- duplicate signatures from one DID do not increase weight;
- non-member approval rejection;
- wrong-policy and wrong-grant approval rejection;
- stale/expired policy rejection;
- wrong campaign/epoch rejection;
- policy author different from campaign record author rejection;
- two valid policies for one campaign fail closed rather than selecting a local winner;
- testing amount outside the deterministic policy amount rejection;
- participation amount outside reward bands rejection;
- participation grant without same-campaign contribution rejection;
- two threshold-approved grants for one contribution still cannot mint twice;
- authorizer equivocation does not bypass ledger duplicate protection;
- independent stores with the same objects reach the same result regardless of approval input order;
- offline replication followed by complete object sync converges; and
- epoch rollover carries no accepted balance or production conversion right.

## Values verdict

- **No single issuer — PASS if threshold >= 2 is enforced in code.**
- **Human decentralization — PARTIAL.** Distinct DIDs are not proof of distinct humans. Exact failure: bootstrap policy membership can still be selected by the temporary campaign authority.
- **Voice/value wall — PASS if no balance/reward/contribution volume enters authorization weight.**
- **Deterministic shared validation — PASS once code/tests prove identical object sets yield identical results.**
- **Policy-fork safety — PASS at validation time.** Competing valid policies fail closed; no node-local tie-break manufactures authority.
- **Rollback-free distributed finality — FAIL in this PR by design.** Exact fix belongs to #337/#338 canonical replicated state resolution, not a fake local tie-break.
- **Production value activation — PASS against it.** This mechanism is Beta-only and provides no production conversion promise.

## Exit condition

The #339 threshold-acceptance engineering slice is complete when the policy and approval objects, shared-beta wrapper, unique-policy fail-closed rule, Forge-native schemas, dependency-wall test, and adversarial convergence tests are green on exact-head CI, with the residual bootstrap membership-selection centralization documented rather than hidden. Mature independent-human authorizer selection and canonical policy/finality remain #338/personhood work and must not be implied by closing this implementation slice.
