# Governance Test Suite

**Status:** Normative scenario catalogue  
**Version:** 1.1

## 1. Test format

Each scenario contains:

- **ID**
- **Given** initial state
- **When** attempted action
- **Then** required result
- **Evidence** proving the result

A governance feature SHOULD NOT be described as operational until its applicable scenarios have been demonstrated.

## 2. Contribution and review

### GOV-CONTRIB-001 — Anonymous proposal

**Given:** an unaffiliated contributor with no public identity.  
**When:** they submit a content-bound proposal and payment address.  
**Then:** the proposal can enter review without legal identity disclosure.  
**Evidence:** accepted proposal record contains no compulsory legal identity field.

### GOV-REVIEW-001 — AI cannot self-approve

**Given:** an AI-authored proposal and multiple AI review outputs.  
**When:** canonicalization is attempted without required authorized human or governance approval.  
**Then:** the transition fails.  
**Evidence:** policy rejection identifies missing quorum.

### GOV-REVIEW-002 — Approval becomes stale

**Given:** approval of digest A.  
**When:** the author modifies the proposal to digest B.  
**Then:** approval of A cannot authorize B.  
**Evidence:** exact-state mismatch rejection.

### GOV-REVIEW-003 — Author not independent

**Given:** an R2 proposal authored by identity X.  
**When:** X attempts to count as an independent approver.  
**Then:** quorum remains unsatisfied.

## 3. Integration

### GOV-INT-001 — Two independent features conflict

**Given:** proposals A and B each pass component tests.  
**When:** the combined integration candidate fails an invariant test.  
**Then:** neither is canonicalized as the combined batch; previous canonical state remains unchanged.

### GOV-INT-002 — One contributor disappears

**Given:** an integration candidate with complete content-addressed inputs and acceptance criteria.  
**When:** one author becomes unavailable.  
**Then:** another authorized contributor can continue without impersonation or hidden local state.

## 4. Release and adoption

### GOV-REL-001 — Governance cannot force activation

**Given:** a valid governed release.  
**When:** no Owner Approval names that release.  
**Then:** activation does not occur.

### GOV-REL-002 — Stale release rejected

**Given:** device state has accepted a higher release sequence.  
**When:** an older otherwise valid release is offered.  
**Then:** normal adoption rejects rollback unless an explicit sovereign fork/reset process is used.

### GOV-REL-003 — Broken release rolls back

**Given:** exact Owner Approval and successful staging.  
**When:** the health check fails after activation.  
**Then:** the installer returns to the previous owner-approved healthy state and records evidence.

### GOV-REL-004 — Provenance not bound to execution

**Given:** a builder declares outputs without a valid bound execution result.  
**When:** release quorum evaluation occurs.  
**Then:** the builder does not count toward trusted provenance.

## 5. Compensation

### GOV-BTY-001 — Anonymous accepted claim paid

**Given:** an open funded bounty and anonymous qualifying submission.  
**When:** authorized evaluation accepts the exact work.  
**Then:** payment can be authorized to the supplied address without legal identity disclosure.

### GOV-BTY-002 — Sponsor cannot buy governance

**Given:** a sponsor funds a large bounty.  
**When:** the sponsor requests extra protocol voting weight.  
**Then:** the request is rejected.

### GOV-BTY-003 — Duplicate attribution dispute

**Given:** two claimants assert authorship.  
**When:** evidence is insufficient for exclusive attribution.  
**Then:** payment is deferred, split, or rejected under published dispute policy; no private administrator invents a result.

## 6. Authority and continuity

### GOV-AUTH-001 — Scope escape

**Given:** a maintainer delegated authority over documentation.  
**When:** they attempt to authorize a treasury change.  
**Then:** authorization fails due to scope mismatch.

### GOV-AUTH-002 — Revoked key

**Given:** a maintainer key has been validly revoked.  
**When:** it signs a later approval.  
**Then:** the approval is invalid.

### GOV-CONT-001 — Founder unavailable

**Given:** founder access is lost.  
**When:** an ordinary approved proposal is ready.  
**Then:** the documented successor mechanism either continues safely or explicitly reports that bootstrap continuity is not yet complete; no hidden bypass is used.

### GOV-CONT-002 — GitHub deletion

**Given:** GitHub repository becomes unavailable.  
**When:** independent maintainers reconstruct current source and governance evidence.  
**Then:** hashes, signatures, and history match the last valid checkpoint.

### GOV-CONT-003 — Forge equivocation

**Given:** a malicious Forge presents incompatible canonical heads to different users.  
**When:** signed checkpoints are compared.  
**Then:** equivocation is detectable and neither conflicting presentation silently replaces continuity.

## 7. Constitutional integrity

### GOV-CONST-001 — Operational smuggling

**Given:** an ordinary repository settings change that would enable forced updates or money-weighted voting.  
**When:** it is proposed as an operational change.  
**Then:** classification detects constitutional effect and requires the higher amendment process or rejects it as invariant-breaking.

### GOV-CONST-002 — Free fork, distinct legitimacy

**Given:** a participant copies all code and history.  
**When:** the fork claims to be the same canonical Mininet without authorized continuity.  
**Then:** technical use remains free, but legitimacy verification distinguishes the fork.

## 8. Scaling

### GOV-SCALE-001 — Two engineers

**Given:** two engineers and founder bootstrap authority.  
**When:** both develop independent R2 changes.  
**Then:** each reviews the other's exact state, both merge into a protected integration candidate, combined checks run, and founder or third reviewer supplies any required additional independent approval.

### GOV-SCALE-002 — One hundred contributors

**Given:** multiple Working Groups and high proposal volume.  
**When:** proposals cross group boundaries.  
**Then:** scoped review occurs within groups, integration representatives evaluate cross-domain effects, and no single maintainer must review everything.

## 9. AI session charter and adapter

The scenarios `GOV-AI-050-01` through `GOV-AI-050-06` defined in Document 50 Section 19 are incorporated into this catalogue by reference. They cover:

- exact activation-record, adapter, charter, summary, and authority-boundary integrity;
- rejection of self-consistent activation data that is not present in the separately verified canonical checkpoint;
- trust-before-load rejection of changed or newly introduced worktree instruction surfaces;
- structured final Decision, current-phase, cooling-basis, effective-time, and append-only supersession checks;
- denial of AI self-approval and quorum;
- precedence when a higher source conflicts;
- safe phase transition without Founder Authority passing to AI;
- model and session-loader replacement; and
- proactive engineering scope boundaries.

Repository validator unit tests cover file presence, adapter/summary identity, three content digests and structured-Decision consistency, canonical-checkpoint separation, trust-before-load instruction-surface equality, branch self-activation rejection, phase/time/cooling gates, append-only supersession, explicit Authority-grant patterns, review routing, policy protection, and protected-path proposal classification. Those structural tests do not prove checkpoint legitimacy or model behavior. Until the applicable behavioral scenarios have recorded evidence, the charter and adapter may be described as specified or packaged, but not behaviorally verified or activated.

## 10. Exit criteria

The governance test suite is mature enough for v1.1 only when:

- every constitutional invariant has positive and adversarial scenarios;
- all authority classes have scope and revocation tests;
- GitHub-loss and founder-loss drills pass;
- Forge can execute and preserve the same semantics;
- anonymous contribution and compensation are demonstrated without compulsory identity;
- owner refusal and rollback are demonstrated end to end;
- an activated AI session adapter passes the applicable `GOV-AI-050-*` scenarios, including tool replacement and self-approval denial.
