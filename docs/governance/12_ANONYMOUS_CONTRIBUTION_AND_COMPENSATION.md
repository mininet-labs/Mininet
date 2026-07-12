# Anonymous Contribution and Compensation

**Status:** Normative sovereignty specification

## Principle

Mininet makes accepted actions accountable without making public identity mandatory. A contributor may start from scratch, use a one-time anonymous identity, build a persistent pseudonym, or participate publicly.

## Contribution modes

### Anonymous session

A fresh key submits a proposal without continuity claims. Suitable for one-off patches, sensitive disclosures, and participants who do not want reputation linkage.

### Persistent pseudonym

A stable or rotatable identity accumulates accepted work, review history, and reputation without revealing legal identity.

### Selectively verified pseudonym

A participant proves selected facts—human uniqueness, qualification, jurisdiction, conflict absence, or membership—without disclosing unrelated identity data where cryptography and governance permit.

### Public identity

A participant voluntarily links the cryptographic identity to a public person or organization.

No mode receives automatic correctness privilege.

## Submission privacy

Forge design should minimize IP, device, timing, and social-graph metadata. Relays, delayed publication, mix routing, local-first authoring, and selective disclosure should be available as the system matures.

## Reputation

Reputation belongs to a cryptographic lineage, not necessarily a real-world person. It should be multidimensional and evidence-based: accepted work, reversions, security findings, review accuracy, availability, and constitutional conduct.

Reputation must not become purchasable governance weight. Contributors retain the right to abandon a reputation and restart, accepting that continuity benefits do not transfer automatically.

## Bounties

A bounty should define:

- exact problem and acceptance tests;
- risk class and required reviewers;
- reward amount or formula;
- whether multiple partial awards are possible;
- claim privacy options;
- dispute process;
- deadline or open-ended status;
- external gate conditions.

## Claim process

1. Contributor submits work and an optional blinded or private claim reference.
2. Work is reviewed independently of the payment destination.
3. Canonical acceptance creates proof that conditions were satisfied.
4. The contributor reveals or proves control of the claim destination through the chosen privacy mechanism.
5. Treasury pays according to governed policy.
6. Payment record reveals no more identity information than necessary.

## Anti-Sybil boundaries

Sybil resistance may be necessary for scarce review slots, spam, governance, or duplicate bounty claims. It must not silently become mandatory public identity for ordinary contribution.

Possible tools include rate limits, proof of resource, persistent reputation, deposits refundable on honest behavior, verified-human signals, and privacy-preserving uniqueness proofs. Money spent on anti-spam cannot buy governance power.

## Sanctions and safety

A pseudonym may lose delegated authority, bounty eligibility, or reputation for fraud, plagiarism, hidden conflicts, or malicious submissions. The protocol should sanction the relevant identity and action without attempting universal real-world unmasking.

## Taxes and law

Individual participants remain responsible for obligations applicable to them. Mininet should not collect universal identity data merely to anticipate every jurisdiction. Treasury interfaces may support optional compliant disclosure paths without making them mandatory for all participants unless an unavoidable legal constraint applies to a specific payer or channel.

## Privacy-preserving acceptance rule

Technical acceptance, compensation eligibility, payment destination, reputation continuity, and governance authority are separate decisions. A contributor may receive payment without revealing legal identity. Payment does not create review, merge, release, or voting authority. Any legally required disclosure must be scoped to the responsible payment operator and must not become a protocol-wide identity requirement.
