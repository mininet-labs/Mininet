# Mininet Open Beta — start here

**Open Beta participation is open. Production/Go-Live is not.**

This is the public door into Mininet's final pre-Go-Live phase. You do not need
to be a programmer, disclose a legal identity, own special hardware, buy
anything, or ask permission to begin. What matters is reproducible evidence tied
to an exact build/state.

Mininet is still experimental. Do **not** put real money, irreplaceable keys,
private production data, or safety-critical communications into this beta.
Prototype cryptography and production-value paths remain subject to their
substantive external review gates.

## Pick the easiest useful thing you can do

| What you have | Good first contribution | What to submit |
|---|---|---|
| One Android phone | install/start/lifecycle, permission denial, Bluetooth off/on, local UI, restart/recovery | one-device Beta test report |
| Two Android phones | direct LAN/QR/BLE discovery, connect, exchange, reconnect, replay rejection, internet-off behavior | two-device Beta test report |
| Three or more phones | A—B—C relay, churn, duplicate paths, partitions/merges, slow peer, multi-vendor behavior | multi-hop Beta test report |
| Emulator only | UI/state/error handling that does not require real radio claims | emulator-class report |
| Rust toolchain | reproduce CI/test failures, fuzz boundaries, write regression tests, inspect invariants | Rust/toolchain report or implementation PR |
| Security/research skills | attack assumptions, protocol analysis, malformed/adversarial cases, threat model, external review | security route; use private disclosure for exploitable secrets |
| Accessibility/usability experience | first-run clarity, readable errors, permission flow, recovery, assistive technology | accessibility report |
| Documentation skills | reproduce docs against the product/code and fix misleading or stale instructions | docs issue/PR |
| Build/release skills | reproducible APK/build/install/rollback evidence | reproducibility report |

You can contribute once and disappear. Pre-Go-Live Mininet does not require or
create a persistent contributor identity or reputation profile.

## Submit a finding

For ordinary findings use the GitHub **Beta test report** issue form:

`https://github.com/mininet-labs/Mininet/issues/new?template=beta-test-report.yml`

That form is a temporary GitHub adapter onto the Forge-native beta schema in
`mini-beta`. A useful report contains:

1. exact commit/release/object under test;
2. evidence class — physical device, emulator, Rust/toolchain, research,
   external review, accessibility, or other;
3. affected component/surface;
4. environment needed to reproduce, with personal identifiers removed;
5. numbered reproduction/test steps;
6. expected result;
7. observed result;
8. severity/impact claim and why;
9. logs/screenshots/hashes/minimal reproduction where useful;
10. what the test does **not** establish; and
11. confirmation that secrets and unnecessary personal data were removed.

Do **not** post a private key, seed, recovery material, stable device identifier,
home/work location, private message contents, or exploit details that would put
users at immediate risk. Security-sensitive findings should use the private
security route linked by the repository issue configuration rather than a public
Beta issue.

### What happens to a good finding

The intended artifact chain is:

```text
exact beta campaign/build
        |
        v
structured finding
        |
        +--> disposition (accepted / duplicate / needs-info / fixed / ...)
        |
        +--> Forge task brief when work is needed
                  |
                  v
             work claim
                  |
                  v
       fix / reproduction / review evidence
                  |
                  v
       accepted contribution receipt
                  |
                  v
      optional Beta MINI participation grant
```

Every arrow is evidence. None grants governance power.

## Beta MINI

Open Beta introduces **Beta MINI**, a test-domain currency for exercising the
product and recognizing useful participation before real value is safe.

There are two grant classes:

- **testing grant** — free Beta MINI so a tester can exercise balances,
  transfers, provider/payment UX, fees, failure cases, and recovery flows;
- **participation grant** — Beta MINI attached to an accepted contribution
  receipt for useful testing, reproduction, device work, accessibility,
  documentation, security/research, code/tests, review, reproducibility, or
  operational evidence.

Beta MINI is not production MINI. It is bound to an explicit beta epoch, can be
reset to zero between epochs, has per-grant and epoch supply caps, has no
protocol conversion right into production MINI, and never creates votes,
reviewer status, release authority, personhood, reputation weight, or privileged
access.

**Do not put a Beta MINI account/claim handle into a public issue.** When a
report/contribution is accepted, its reward path uses a fresh one-contribution
claim handle and a fresh opaque beta account. This avoids turning a public
GitHub account into a Mininet identity or creating a cross-submission profile.

The code lives in `crates/mini-beta`. Its dependency graph deliberately contains
no production value, settlement, treasury, chain, consensus, or governance
crate.

## Core physical-device itinerary

Record exact device models/OS versions only to the degree needed to reproduce;
do not record serial numbers, advertising identifiers, phone numbers, account
names, or other stable identifiers.

### 1. One-phone lifecycle

- fresh install / clean app state;
- deny requested Bluetooth/network permissions, verify clean failure;
- grant only required permissions, retry;
- Bluetooth off -> on;
- background/foreground;
- screen off/on;
- process kill/restart;
- repeated start/stop;
- confirm no stuck scanning/advertising/worker state;
- confirm error text is understandable and recoverable.

### 2. Two-phone direct path

- disable internet on both phones;
- discover/connect over the intended local bearer;
- complete the identity/presence path supported by the current build;
- exchange a small test payload;
- disconnect and reconnect;
- restart one side and repeat;
- attempt replay of stale evidence where the UI/tooling exposes it;
- transfer Beta MINI once that product surface is wired;
- record latency/failure honestly without generalizing one device pair to all Android devices.

### 3. Three-phone relay

Arrange A—B—C so A and C do not have a direct usable edge if the environment
allows it.

- A sends toward the mesh; C receives through B;
- repeat payload and confirm duplicate suppression;
- remove B, confirm failure is bounded and does not freeze the app;
- restore B and verify recovery;
- create a second path where possible and check duplicates remain bounded;
- send oversized/malformed input only in a controlled test and record the exact limit/error.

### 4. Churn and hostile-resource cases

- rapid connect/disconnect;
- slow receiver;
- permission revocation during operation;
- repeated discovery callbacks;
- duplicate peers/paths;
- queue pressure;
- malformed frames;
- many small reports/messages;
- near-limit payloads;
- stop/close racing with callbacks.

Never perform denial-of-service testing against devices or networks you do not
own or have permission to test.

### 5. Privacy review

For every test ask:

- did a relay learn application plaintext it did not need?
- did logs expose stable device identifiers or identity roots?
- did the product unnecessarily require a public name/account?
- did a reward or contribution flow link otherwise separate submissions?
- did any balance affect a permission, vote, review, release, or personhood decision?

Treat any last item as a release-blocking invariant violation.

## Contribution routes beyond findings

If you want a scoped task rather than reporting a result, use the Contributor
intake form or an existing open issue. The long-term route is Forge-native task
brief -> explicit expiring claim -> exact-state review handoff. GitHub is only
the current mirror.

Good final-phase work includes:

- reproducing an existing report on different hardware;
- converting a bug into a deterministic regression test;
- proving an alleged issue cannot be reproduced under stated conditions;
- Android lifecycle/permission/radio hardening;
- accessibility and first-run fixes;
- build/reproducibility evidence;
- redacted diagnostics and observability;
- threat-model attacks and fixes;
- documentation truth-sync;
- Forge beta workflow/UI/CLI work;
- GitHub-outage testing of Forge/store/sync workflows; and
- closing substantive audit/readiness findings with exact-state evidence.

## Forge transition is the milestone

The final pre-Go-Live goal is not a perfect GitHub beta program. It is to stop
needing GitHub as an authority or single coordination surface.

`crates/mini-beta` now defines the campaign/finding/disposition/contribution/
grant object vocabulary directly on Mininet's signed, content-addressed object
substrate. The remaining transition work is to expose those objects through
Forge/CLI/app surfaces, replicate them over Mininet sync, complete task/review
handoffs without GitHub, and prove the workflow during a GitHub outage.

See [`design/beta-open-forge-transition.md`](design/beta-open-forge-transition.md)
for the milestone contract and [`FORGE_BETA_MIGRATION.md`](FORGE_BETA_MIGRATION.md)
for the cutover matrix.

## What Open Beta does not mean

Open Beta does **not** mean:

- production value is live;
- crypto/custody/settlement audits are waived;
- unique-human personhood is solved;
- every feature is safe or complete;
- GitHub is canonical forever;
- the Founder/bootstrap custodian becomes a permanent issuer or administrator;
- AI review is approval; or
- an accepted contribution buys political power.

The project reaches Go-Live only through the explicit one-way Forge-canonical /
Go-Live transition recorded by the bootstrap governance state. Until then, make
the evidence better and make the dependency on centralized infrastructure
smaller.
