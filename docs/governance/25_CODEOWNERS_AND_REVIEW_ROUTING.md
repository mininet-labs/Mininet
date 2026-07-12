# CODEOWNERS and Review Routing

**Status:** Operational bootstrap specification  
**Version:** 1.1

## 1. Purpose

CODEOWNERS routes proposals to competent review teams. It does not make those teams constitutional authorities and does not prove the independence or uniqueness of reviewers.

## 2. Team model

The repository owner SHOULD create these GitHub teams before activating the provided template:

- `core-maintainers`
- `reviewers-constitution`
- `reviewers-identity`
- `reviewers-consensus`
- `reviewers-forge-release`
- `reviewers-storage`
- `reviewers-value-crypto`
- `reviewers-network`
- `security-stewards`
- `release-signers`

Every critical domain SHOULD have at least two active reviewers and a succession path. A single person MUST NOT be the sole required owner for a constitutional, release, identity, consensus or custody path.

## 3. Routing principles

- Constitution and invariants require constitutional and domain review.
- Cryptography requires qualified cryptography review and honest audit-gate status.
- Forge, provenance, update and installer changes require cross-review because their trust boundaries compose.
- CI and dependency-policy changes require security review.
- The standing AI charter, root `AGENTS.md`, and model-specific session loaders require constitutional review routing because instruction drift can change effective engineering behavior.
- Cross-domain changes require all materially affected owners.

## 4. Scaling

At two contributors, CODEOWNERS may route to broad founder/core teams while the final main proposal still requires a second independent authorization. At one hundred contributors, domain teams own ordinary review, while cross-domain and constitutional proposals escalate to an integration council or governance process.

## 5. Anti-capture rules

Team membership MUST be reviewable and revocable. Compensation, stake, hardware contribution or sponsorship MUST NOT automatically grant team membership. Inactive or captured teams MUST have a documented replacement path.
