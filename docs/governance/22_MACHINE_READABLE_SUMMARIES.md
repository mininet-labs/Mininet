# Machine-Readable Governance Summaries

**Status:** Experimental specification  
**Version:** 1.1

## 1. Purpose

Machine-readable summaries allow tooling to detect missing references, inconsistent authority claims, untested transitions, and superseded rules. They supplement human-readable documents and are not yet an independent source of constitutional authority.

## 2. Recommended schema

```yaml
document:
  id: GOV-ARCH-000
  title: Architecture of Mininet Governance
  version: "0.3"
  status: normative
  authority_class: protocol-governance
  supersedes: []
  superseded_by: []

traceability:
  constitution: [SPEC-00]
  invariants: [INV-U1, INV-AI1]
  decisions: [D-0067, D-0069, D-0070, D-0071]

ontology:
  defines: [Proposal, Review, Approval]
  uses: [Authority, Evidence, Canonicalization]

actors:
  - type: Contributor
    identity_modes: [anonymous, pseudonymous, public, automated]

transitions:
  - id: proposal-to-review
    from: EvidenceReady
    action: open_review
    to: UnderReview
    authority: proposal-owner
    evidence: [exact_state_digest]

requirements:
  - id: GOV-REQ-001
    level: MUST
    text_digest: "..."
    enforcement: [forge-policy, github-ruleset]
    tests: [GOV-REVIEW-002]

privacy:
  compulsory_disclosures: []
  optional_disclosures: [public_attribution]

implementation:
  github_bootstrap: protected_pull_request
  forge_target: signed_proposal_object
```

## 3. Required fields

A summary SHOULD include:

- stable document ID;
- semantic version;
- status and authority class;
- higher-authority references;
- terms defined and used;
- actors and allowed identity modes;
- state transitions;
- requirement identifiers and strength;
- enforcement mappings;
- test scenario references;
- privacy and compensation effects;
- implementation status.

## 4. Validation rules

Tooling SHOULD reject or warn when:

- a term is used but not defined or imported;
- a lower-authority document claims to override a higher source;
- a MUST requirement has no enforcement mapping or test plan;
- a state transition lacks authority or evidence;
- an approval is not exact-state bound;
- a document claims production readiness while required audit gates remain open;
- identity disclosure is compulsory without a stated necessity and authority;
- money, storage, or hardware is mapped to political weight;
- release governance is mapped directly to forced adoption.

## 5. Storage and signing

Summaries SHOULD be stored beside their documents or generated deterministically. Once Forge governance adopts them as protocol objects, summaries SHOULD be content-addressed and signed or included in the document digest.

For new v1.1 summaries, the deterministic package convention is `<document-stem>.summary.json` beside the Markdown source. The summary SHOULD include a `document.source` repository-relative path and conform to `governance/document-summary.schema.json` after repository integration. Document 50 follows this convention. Legacy documents remain under the migration rule below and are not falsely represented as having summary instances.

The reference validator currently performs exact identity and activation-state checks for the Document 50 summary only. It does not yet implement general JSON Schema validation or require summaries for the legacy corpus.

## 6. Migration path

1. v0.3: the summary format was documented and summaries remained optional.
2. v0.4: schema and validator scaffolding were introduced without pack-wide summary instances or general instance validation.
3. v1.1: Document 50 becomes the first packaged summary instance with exact identity and activation-state checks.
4. Future: migrate the legacy corpus, implement general schema/reference validation, and test Forge import before claiming pack-wide or Forge-native summary enforcement.
5. Later governance may decide whether structured summaries become normative objects.
