# Beta workflow: mapping from the temporary GitHub adapter to the Forge-native path

Issue #336 asks for "a documented mapping back to the temporary GitHub adapter
while GitHub remains useful". This is that mapping. GitHub is a mirror and intake
adapter during the bootstrap; the Forge-native column is the canonical record.
Nothing here makes GitHub canonical, and no step below needs GitHub to work
(`beta_native_loop.rs`, `beta_native_flow.rs` and `beta_two_node_loop.rs` run the
native column with no GitHub call).

| Beta workflow step | GitHub adapter today | Forge-native equivalent (exists) | What is still adapter-only |
| --- | --- | --- | --- |
| Discover a campaign | A pinned issue / `docs/` page naming the exact build | `mini beta campaign list` / `show` (campaign object, replicates over `mini sync`) | Announcing a campaign to people who do not yet run a node |
| Report a finding | `.github/ISSUE_TEMPLATE/beta-test-report.yml` (fields mirror the finding schema) | `mini beta finding submit` (strict `mininet.beta/finding/v1` object, anonymous submission tag, privacy-redacted required) | The issue form is a convenience front end; copying an issue into a finding is manual |
| Triage / duplicate links | Labels, comments, "duplicate of" | `mini beta finding disposition` (append-only disposition objects) | none |
| Turn a finding into work | An implementation issue | `mini beta finding to-task` (Forge task brief linked to the finding) | none |
| Claim a task | Assigning the issue | `mini beta task claim` (expiring, anonymous work claim); `mini task show` lists untrusted claims separately | none |
| Evidence of work, review | PR and review comments | Exact-state technical-review handoff (`mini-forge`); the PR itself is still the integration path | **Merging into this repository is still a GitHub PR plus the bootstrap custodian** until `forge_canonical` is true (issue #338) |
| Record accepted work | Maintainer comment | `mini beta contribution accept --campaign --claim-tag` (receipt carries only a public claim tag) | none |
| Reward handoff | n/a (never on GitHub) | `mini beta claim new` / `claim redeem`, `grant create --redemption`, threshold `grant-approve` | Redeem is in the CLI and the desktop panel; the Android app's coverage was not checked |
| Cross-node evidence | Everything is visible in the issue | `mini sync listen` / `connect`; strict parsers refuse malformed, oversized, unknown-link or wrong-type objects (`crates/mini-beta/tests/strict_parsing.rs`) | none |

## What stays on GitHub, honestly

- Issue forms and labels are intake conveniences; they are not read by any
  Forge-native command and decide nothing.
- Repository integration (merging code) is still the bootstrap path. The one-way
  `forge_canonical` / `go_live` state exists as a library
  (`mini-bootstrap-state`, D-0555) but nothing consults it yet.
- GitHub import/export mirroring of Forge objects is not automated.

## Verification

`cargo test -p mini-beta --test strict_parsing` (strict parsing) and the three
`mini-cli` beta integration tests named above (no-GitHub path).
