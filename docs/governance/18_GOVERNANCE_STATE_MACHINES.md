# Mininet Governance State Machines

**Status:** Normative transition model  
**Version:** 0.3

## 1. General transition rules

Every transition MUST identify:

- source state;
- exact object digest;
- authorized Actor;
- required Evidence;
- destination state;
- durable audit record;
- failure outcome.

Material modification creates a new object state and invalidates approvals that do not bind it.

## 2. Proposal state machine

`Draft -> Published -> Evidence Ready -> Under Review -> Integration Candidate -> Canonical`

Terminal or alternate states: `Rejected`, `Withdrawn`, `Superseded`, `Expired`.

### Draft to Published

Actor: Contributor.  
Requirement: content-bound Proposal with target and scope.  
Authority: proposal submission only.

### Published to Evidence Ready

Requirement: declared acceptance criteria and minimum required tests/evidence are attached. This transition does not claim the evidence is correct.

### Evidence Ready to Under Review

Requirement: exact state frozen for review; conflicts disclosed; reviewers selected under applicable independence policy.

### Under Review to Integration Candidate

Requirement: required approvals, objections resolved or explicitly overruled by authorized process, all blocking checks pass, and the candidate includes an integration plan.

### Integration Candidate to Canonical

Requirement: combined-state evidence, canonicalization authority, and protected transition. Component-level success alone is insufficient.

## 3. Review state machine

`Opened -> Findings Recorded -> Author Response -> Resolved | Blocking | Superseded`

A Review MUST bind the Exact Proposal State. An AI review MAY enter findings but cannot become an Approval unless future constitutional authority explicitly permits it.

## 4. Approval state machine

`Eligible -> Signed -> Active -> Stale | Revoked | Consumed`

An Approval becomes `Stale` when the reviewed state materially changes. It becomes `Consumed` when used in the authorized transition. Reuse across unrelated transitions is prohibited unless explicitly allowed.

## 5. Integration state machine

`Created -> Components Added -> Combined Checks -> Adversarial Integration Review -> Ready -> Canonicalized | Failed`

A failed integration MUST preserve the previously canonical state. Integration branches or Forge objects are disposable staging states, not independent canonical authorities.

## 6. Release state machine

`Source Canonical -> Build Requested -> Built -> Proven -> Governed -> Available -> Owner Approved -> Staged -> Activated -> Healthy`

Recovery states: `Rejected`, `Expired`, `Stale`, `Equivocation Detected`, `Activation Failed`, `Rolled Back`.

A release MUST NOT skip provenance or governance gates where policy requires them. `Available` MUST NOT imply `Owner Approved`. `Governed` MUST NOT imply `Activated`.

## 7. Bounty state machine

`Draft -> Funded -> Open -> Claim Submitted -> Evaluated -> Accepted | Partially Accepted | Rejected -> Payable -> Paid`

Additional states: `Disputed`, `Cancelled`, `Expired`.

A Claim MAY use anonymous or pseudonymous submission. Payment eligibility derives from accepted work, not compulsory legal identity. Treasury policy MAY require anti-fraud evidence but SHOULD use the least identity-revealing mechanism practical.

## 8. Delegation state machine

`Proposed -> Accepted -> Active -> Suspended -> Revoked | Expired | Succeeded`

Delegation MUST state Scope, duration, permitted Actions, revocation authority, and succession behavior. Loss of platform permissions MUST NOT be treated as the sole revocation record once Forge governance is active.

## 9. Working-group state machine

`Proposed -> Chartered -> Bootstrapping -> Active -> Review Due -> Renewed | Split | Merged | Retired`

A Working Group MUST NOT become permanent by inactivity. Charter review, maintainer succession, and authority audit are required at defined intervals.

## 10. Constitutional amendment state machine

`Draft -> Classified -> Public Review -> Adversarial Review -> Cooling Off -> Decision -> Ratified -> Activated`

Alternate states: `Rejected`, `Withdrawn`, `Superseded`, `Forked`.

Classification determines whether the proposal is constitutional, protocol-governance, operational, or editorial. An operational process MUST NOT ratify a constitutional change.

## 11. Emergency action state machine

`Detected -> Contained -> Temporarily Authorized -> Executed -> Publicly Recorded -> Reviewed -> Reverted | Ratified`

Emergency authority MUST be narrow, time-bounded, and incapable of creating a permanent hidden constitutional change. It MUST NOT enable forced owner updates, universal unmasking, or an administrative kill switch.

## 12. GitHub-to-Forge transition state machine

`GitHub Primary -> Dual Publish -> Forge Verification -> Forge Canonical / GitHub Mirror -> GitHub Read Only -> GitHub Archived or Disabled`

Transition requires demonstrated proposal, review, governance, build, release, recovery, identity, availability, and audit continuity. Aspirational code or a successful demo is insufficient.
