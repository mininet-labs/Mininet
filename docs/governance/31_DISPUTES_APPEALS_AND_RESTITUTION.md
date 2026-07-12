# Disputes, Appeals, and Restitution

**Status:** Normative governance specification

## 1. Purpose

Disputes correct process failures without creating a permanent court, universal administrator, or identity-unmasking authority.

## 2. Dispute classes

- technical rejection or acceptance error;
- reviewer conflict or collusion;
- plagiarism or stolen work;
- bounty criteria ambiguity or mutation;
- award allocation disagreement;
- payment failure or redirection;
- delegation abuse;
- working-group capture;
- emergency-action abuse;
- constitutional or invariant conflict.

## 3. Filing

A dispute object MUST identify the contested object, requested remedy, evidence, filing authority if required, and privacy needs. Anonymous filing SHOULD be supported for vulnerability and retaliation-sensitive claims.

## 4. Independence

No actor may decide an appeal of their own action. Review panels MUST exclude:

- proposal authors;
- original decisive reviewers;
- bounty sponsors with direct financial interest;
- common-control identities where independence is required;
- AI systems counted as authorities.

AI MAY summarize evidence and search for contradictions but cannot satisfy the deciding quorum.

## 5. Remedies

Remedies are scoped and SHOULD minimize collateral effects:

- reconsideration by a fresh panel;
- additional evidence request;
- partial or corrected award;
- payment release or refund;
- review invalidation;
- delegation suspension or revocation;
- correction object appended to history;
- release withdrawal;
- restitution from bonded funds where policy provides;
- fork recognition where constitutional disagreement cannot be resolved.

Signed history MUST NOT be silently erased.

## 6. Timelines

Policies SHOULD define filing windows, response windows, and maximum suspension periods. Security-sensitive evidence MAY be temporarily sealed, but the existence and eventual disposition of the case should be auditable.

## 7. Finality

Ordinary disputes should become final after one appeal. Constitutional/invariant disputes MAY escalate to governance under a higher threshold and cooling-off period. Finality does not prevent later correction upon genuinely new evidence, but reopening MUST itself be justified.

## 8. Restitution

Restitution SHOULD restore the harmed party without granting extra authority. Compensation for an incorrect rejection or payment failure does not create maintainership or governance weight.
