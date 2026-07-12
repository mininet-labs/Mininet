# Anonymous Bounty Lifecycle

**Status:** Normative design specification

## 1. Objective

A contributor MUST be able to discover work, submit a solution, prove acceptance eligibility, and receive compensation without being required to reveal a legal identity to the protocol.

The design MUST separate technical merit from payment identity and payment from governance power.

## 2. Objects

### BountyOffer

Contains:

- problem statement and immutable acceptance criteria;
- reward amount, asset, and funding commitment;
- risk class and required review policy;
- deadline or open-ended status;
- whether partial or multiple awards are permitted;
- claim privacy modes;
- dispute and cancellation rules;
- jurisdiction-specific payment caveats, if an operator cannot avoid them;
- sponsor and treasury authorization.

### ClaimCommitment

A hiding commitment linking a submission to a future claimant without publishing the destination or legal identity.

It SHOULD bind:

- bounty identifier;
- proposal identifier;
- claimant secret or nullifier construction;
- optional payout-key commitment;
- anti-double-claim domain separator.

### AcceptanceAttestation

Created only after the accepted work becomes canonical or satisfies the offer's explicitly defined acceptance event.

It MUST identify:

- accepted proposal/integration digest;
- criteria satisfied;
- reviewers and policy;
- award allocation;
- challenge window.

### ClaimProof

Proves control of the committed claim without requiring public linkage to the contribution key where the chosen mode supports unlinkability.

### PaymentAuthorization

Treasury authorization that binds the accepted claim, amount, destination commitment or stealth address, and settlement conditions.

### SettlementReceipt

Proves that the authorized amount was settled or records why it failed. The receipt SHOULD reveal no more than needed for treasury accountability and double-payment prevention.

## 3. Lifecycle

```text
Draft -> Funded -> Open -> SubmissionLinked -> Reviewed -> Accepted
      -> ChallengeWindow -> ClaimProved -> PaymentAuthorized -> Settled -> Closed
```

Alternative terminal states:

```text
Cancelled | Expired | Rejected | Disputed | PartiallyAwarded | Refunded
```

## 4. Rules

1. Work MUST be reviewed without knowledge of the payout destination whenever practical.
2. A sponsor MUST NOT change acceptance criteria after a linked submission except through a visible amendment that preserves earlier claimant rights.
3. Funds MUST be committed or transparently contingent before the offer is represented as funded.
4. Acceptance MUST NOT automatically expose the claimant's contribution key or network metadata.
5. One accepted unit of work MUST NOT be paid twice under the same exclusive offer.
6. Payment MUST NOT create review, maintainer, governance, or voting authority.
7. Anonymous contributors MAY abandon reputation continuity without losing the right to payment for already accepted work.
8. A legal or payment intermediary's disclosure requirement MUST remain scoped to that intermediary and MUST NOT become a protocol-wide identity requirement.

## 5. Multiple contributors

Offers MUST state one of:

- first valid accepted solution;
- best solution selected under published criteria;
- proportional partial awards;
- independent awards for multiple compatible contributions;
- milestone rewards.

Selection MUST be evidence-based and appealable. Timing alone SHOULD NOT decide security-sensitive work when a later submission materially improves safety.

## 6. Anti-fraud controls

Possible controls include:

- claim nullifiers;
- plagiarized-work evidence;
- disclosure of common control between supposedly independent reviewers;
- refundable anti-spam deposits;
- blinded persistent reputation;
- duplicate-output detection;
- challenge windows;
- treasury rate limits.

Anti-fraud controls MUST NOT make wealth a source of political power.

## 7. Cancellation

A funded bounty may be cancelled only under its published rules. Cancellation after substantial good-faith work SHOULD permit expense or partial-work claims where the offer allowed them. Emergency cancellation MUST produce a signed reason and appeal path.

## 8. Acceptance tests

- fresh anonymous key can submit and claim;
- payout destination is not visible during technical review;
- double claim fails;
- sponsor cannot silently alter criteria;
- rejected claim does not reveal claimant linkage;
- accepted work can be paid to a stealth or otherwise privacy-preserving destination;
- payment does not change governance weight;
- dispute can freeze only the contested payment, not unrelated treasury or contributor activity.
