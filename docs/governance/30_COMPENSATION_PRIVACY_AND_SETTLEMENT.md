# Compensation Privacy and Treasury Settlement

**Status:** Normative design specification

## 1. Separation of concerns

Mininet compensation consists of five independent questions:

1. Was work accepted?
2. Who or what controls the valid claim?
3. What amount is owed?
4. Where should it be settled?
5. What minimum record is needed to prevent fraud and audit treasury behavior?

No layer should learn more than it needs.

## 2. Privacy modes

### Public payout

Contribution identity and destination are intentionally linked.

### Pseudonymous payout

A persistent contribution identity receives payment without legal-name disclosure.

### Unlinked payout

The claimant proves a committed right and supplies a destination not publicly linkable to the contribution key.

### Selectively disclosed payout

The claimant proves a required property to a payer or jurisdiction-specific operator without publishing it to Forge or the network.

The offer MUST declare supported modes before submissions are linked.

## 3. Treasury authorization

Payment authorization MUST require:

- a valid funded bounty;
- an accepted and challenge-final claim;
- no prior settlement nullifier;
- amount consistent with offer/allocation;
- valid treasury policy and signer threshold;
- destination proof appropriate to the selected mode;
- expiry and replay domain.

Treasury signers authorize settlement; they do not re-decide technical acceptance unless fraud evidence invokes the dispute process.

## 4. Minimal public record

The public record SHOULD contain:

- bounty and acceptance identifiers;
- amount or governed privacy-preserving commitment;
- settlement status;
- nullifier preventing duplicate payment;
- treasury authorization proof;
- dispute outcome if applicable.

It SHOULD NOT contain legal names, addresses, tax identifiers, network addresses, device identifiers, or unrelated identity proofs.

## 5. Compliance boundary

Mininet cannot guarantee that every payment rail in every jurisdiction permits anonymous settlement. Where a specific treasury operator or rail requires disclosure:

- the requirement MUST be stated before claim finalization where possible;
- disclosure MUST be limited to the responsible operator;
- alternative rails SHOULD remain available where lawful and technically possible;
- disclosed data MUST NOT be copied into canonical governance history;
- inability to use one rail MUST NOT invalidate accepted technical work.

## 6. Key loss and recovery

Offers MUST define whether claim recovery is available. Strong privacy may make recovery impossible. Recovery mechanisms MUST NOT permit sponsors or treasury actors to redirect claims unilaterally.

Possible recovery designs include precommitted recovery keys, threshold recovery delegates, or delayed reclaim after an explicit dormancy period.

## 7. Treasury capture resistance

No single sponsor or treasury signer should be able to:

- pay an unaccepted claim;
- redirect a claimant's destination;
- suppress a valid settlement without producing evidence;
- infer contribution identity from unnecessary metadata;
- grant governance authority through payment.

## 8. Accounting

Accounting MAY use commitments and zero-knowledge proofs for amount privacy, but solvency, authorized issuance, and no-double-payment properties MUST remain independently verifiable. Until such cryptography is audited, the system MUST label confidential settlement as experimental.
