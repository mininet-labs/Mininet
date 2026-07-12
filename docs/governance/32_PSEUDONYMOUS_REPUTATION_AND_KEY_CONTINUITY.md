# Pseudonymous Reputation and Key Continuity

**Status:** Normative design specification

## 1. Principle

Reputation is evidence associated with a cryptographic lineage. It is not proof of legal identity, human uniqueness, moral worth, or permanent authority.

## 2. Reputation dimensions

Reputation SHOULD remain multidimensional:

- accepted implementations;
- later reversions or regressions;
- review accuracy;
- security findings;
- integration reliability;
- documentation and research quality;
- response to disclosed conflicts;
- availability for delegated duties;
- constitutional conduct;
- bounty completion.

A single scalar score is discouraged because it hides domain differences and is easier to game.

## 3. Key rotation

A pseudonym MUST be able to rotate keys while preserving continuity through a signed key-event lineage. Rotation MUST NOT permit one lineage to multiply into several independent identities for quorum purposes.

## 4. Restart right

A participant MAY abandon a lineage and restart anonymously. Positive authority and reputation do not transfer automatically. Historical signed actions remain attributable to the old lineage, but legal identity need not be revealed.

## 5. Selective disclosure

A participant MAY prove selected facts—such as membership in an authorized reviewer set or absence from a counted approval lineage—without exposing unrelated history where practical.

## 6. Negative evidence

Verified misconduct findings MAY attach to the relevant lineage. They MUST identify scope, evidence, appeal status, and expiry or rehabilitation policy. A global unappealable blacklist is prohibited.

## 7. Authority boundary

Reputation may inform nomination or reviewer assignment, but it MUST NOT automatically grant constitutional, release, treasury, or voting authority. Authority always requires an explicit, revocable delegation or governance decision.

## 8. Personhood boundary

One identity root is not necessarily one human. Reputation systems MUST NOT describe root counting as one-human-one-vote until a legitimate privacy-preserving personhood mechanism actually supports that claim.
