# Forge-native Open Beta CLI

**Issue:** #336  
**Parent milestone:** #334  
**Status:** implementation scope for a stacked draft PR

## Goal

Make the Open Beta evidence path usable through the existing `mini` developer spine with **no GitHub account and no GitHub API dependency**.

The first acceptance target is a scriptable native path, not a polished app:

```text
campaign discovery
  -> privacy-safe finding
  -> verified sync to another node
  -> append-only disposition
  -> finding -> Forge task
  -> anonymous task claim
  -> exact-state review/evidence
  -> accepted contribution receipt
  -> fresh private Beta claim/account handoff
```

The CLI is an interface to signed Mininet objects. It is not a new authority layer.

## Privacy model

Pre-Go-Live Mininet participation remains anonymous. The ordinary persistent developer identity in `~/.mininet` MUST NOT silently become the contributor identity for Beta findings or claims.

Participant-authored Beta artifacts therefore use a **fresh artifact-scoped signing root/device pair**:

1. create a fresh root controller from OS entropy;
2. create a fresh delegated device under that root;
3. grant only the capability required for the artifact;
4. publish both KEL carriers into the object store;
5. sign exactly the artifact/claim;
6. discard the ephemeral secret controllers after the command completes.

The signed object remains verifiable and syncable, but two submissions are not linkable through a stable author DID merely because they were created by the same local user.

This mechanism proves unlinkability-by-default only at the object identity layer. Network metadata, filesystem access, timing, logs, or a user voluntarily reusing evidence can still correlate activity and must not be described as solved anonymity.

Bootstrap/operator artifacts such as campaign publication, disposition, task creation, review, and accepted-contribution recording may use the local operational identity because they express temporary workflow authority, not the anonymous contributor's identity. That authority remains subject to the current bootstrap governance documents and must disappear at the Forge handoff.

## Native command surface

Initial commands:

```text
mini beta campaign list
mini beta campaign show <campaign-id>
mini beta finding submit <campaign-id> ...
mini beta finding list [--campaign <id>]
mini beta finding show <finding-id>
mini beta finding disposition <finding-id> ...
mini beta finding to-task <project> <finding-id> ...
mini beta disposition list [--finding <id>]
mini beta disposition show <disposition-id>
mini beta task claim <task-id> ...
mini beta contribution accept <source-id> ...
mini beta contribution list [--campaign <id>]
mini beta contribution show <contribution-id>
mini beta claim new
```

All commands should support `--json` because the no-GitHub acceptance test must be scriptable without scraping human text.

`mini sync` remains the transport. This PR must not invent a Beta-specific network protocol.

## Finding submission

Required inputs:

- campaign id;
- evidence class;
- reporter-claimed severity;
- component;
- summary;
- environment;
- steps;
- expected result;
- observed result;
- at least one redacted evidence reference;
- explicit limitations;
- explicit privacy-redacted acknowledgement.

`SubmissionTag` is generated locally from OS entropy. It is printed once as artifact-scoped metadata but is not saved as a persistent contributor profile.

A finding command MUST fail if the user does not explicitly affirm redaction.

## Finding disposition

Disposition is append-only. The original finding is never mutated.

The command may record accepted, duplicate, needs-information, fixed, cannot-reproduce, or rejected state plus rationale and optional task/resolved state links. `fixed` requires exact resolved-state evidence.

This is an operational record. It does not delete contrary evidence.

## Finding -> task

An accepted finding can become a standard `mini-forge` task brief. The task must preserve the finding id as evidence and must use existing Forge task semantics rather than creating a parallel Beta task type.

The handoff must not transfer:

- reporter identity;
- Beta balance;
- reward history;
- invitation ancestry; or
- GitHub metadata

into task priority, assignment, review weight, or merge/release authority.

## Anonymous task claim

Existing `mini task claim` uses the persistent local developer identity. During the current anonymous-only Pre-Go-Live phase, `mini beta task claim` instead signs the existing Forge work-claim object with a fresh artifact-scoped root/device pair and publishes the corresponding KEL carriers.

The claim can reserve coordination scope for its bounded lease, but it creates no persistent reputation identity. A later accepted contribution is linked by object id, not by silently correlating author DIDs.

This is intentionally separate from production/public Forge identity policy. When canonical community governance later permits voluntary persistent pseudonyms/public profiles, ordinary `mini task claim` remains available under those rules.

## Contribution acceptance and private reward handoff

`mini beta contribution accept` records an accepted `mininet.beta/contribution/v1` receipt using the operational record signer. The contributor is represented by:

- exact source object id;
- optional campaign id;
- fresh random `ClaimTag`;
- contribution kind;
- summary; and
- reproducible evidence references.

The command outputs the `ClaimTag` as **private claim material** and warns against posting it in a public issue/log. It does not create a stable contributor directory.

`mini beta claim new` can also generate fresh `ClaimTag` and `BetaAccountId` material locally for handoff to #337/#339 surfaces. Generation uses operating-system randomness through `mini-crypto`; no deterministic username/account derivation is allowed.

## Verification and sync boundary

Locally created objects may be inserted directly because the local command just created and signed them. Remote objects must continue to enter through the existing `mini-sync` verified-ingest boundary before they become usable store state.

Acceptance proof uses two independently opened homes/stores:

1. node A creates a finding with ephemeral KEL carriers;
2. node B runs the ordinary `mini sync` listener;
3. node A connects and sends its object set;
4. node B accepts the KEL carriers and then the finding through strict verified ingest;
5. node B can list/show the exact finding;
6. node B records a disposition/task/contribution chain;
7. the chain syncs back to node A;
8. both nodes read the same immutable object ids.

Directly copying an unverified remote object into `mini-store` is not acceptable evidence for this test.

## No-GitHub acceptance test

The test harness must not call GitHub, inspect GitHub environment variables, or depend on a Git remote. It should use temporary homes, filesystem stores and the same local TCP sync code used by `mini sync`.

The test should cover:

- two independent stores;
- campaign discovery;
- anonymous finding creation;
- KEL-carrier verified ingestion;
- listing/showing after sync;
- disposition;
- finding -> task;
- anonymous work claim;
- accepted contribution;
- fresh private claim/account material;
- round-trip sync; and
- malformed/oversized/unknown-link rejection through the underlying strict parsers/ingest path.

## Authority wall

The Beta CLI may create coordination/evidence objects. It MUST NOT make Beta balance or reward evidence influence:

- task suggestion score;
- task claim priority;
- review acceptance;
- merge authority;
- release authority;
- personhood;
- Parliament/community voting; or
- canonical-state weight.

A dependency from task/review/governance logic back into `mini-beta` balances would be an immediate failure.

## Deliberate limits

This PR does not make Forge canonical, does not make Beta MINI durable, does not solve personhood, and does not remove the temporary operational signer used for campaign/disposition/contribution records.

The exact remaining centralization after this PR is workflow acceptance authority: the bootstrap operator can still disposition findings and record accepted contributions. #338 must replace that with canonical Forge governance and then shut bootstrap authority down one-way.

## Exit criteria

- native commands create/read the exact `mini-beta`/`mini-forge` object types;
- participant-authored findings and claims do not use the persistent local identity;
- ephemeral KEL carriers survive ordinary verified sync;
- two-node no-GitHub integration test proves the evidence chain;
- all CLI surfaces support deterministic JSON output;
- no Beta value/reward field enters Forge/governance authority logic;
- exact-head format, Clippy, tests, sync/reproducibility and governance checks pass.

## Values verdict

- **GitHub independence — PASS only when the two-node no-GitHub test is green.**
- **Participant privacy — PASS at object identity only with fresh artifact signers; network/timing privacy remains PARTIAL.**
- **Voice/value wall — PASS if task/review/governance code never reads Beta balances/reward magnitude.**
- **Bootstrap workflow authority — PARTIAL.** Operator disposition/contribution acceptance remains temporary central authority until #338.
- **Forge canonicality — FAIL in this PR by design.** Exact fix: #338 outage proof plus irreversible canonical handoff.
