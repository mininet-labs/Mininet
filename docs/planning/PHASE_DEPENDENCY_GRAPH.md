# Phase dependency graph

Which decisions/artifacts need to exist before coding in later phases —
and, as of 2026-07-10, how much of this dependency chain is already
satisfied vs. still open.

## Spine dependency

The self-hosted forge spine is the main dependency chain everything else
eventually feeds:

1. identity and authorship — **shipped** (`did-mini`, `mini-cli identity`)
2. repository and change objects — **shipped** (`mini-forge`, `mini-objects`)
3. review and approval objects — **shipped** (`mini-forge::governance`,
   predating the audit that named it as a gap)
4. governed merge — **shipped** (`mini-forge::governance::merge`)
5. reproducible build — **shipped** (`mini-provenance`,
   `mini-build-runner-wasmtime`, D-0068/D-0069)
6. release verification — **shipped** (`mini-forge::release`, transparency
   log + rollback protection, D-0070)
7. safe installer adoption — **shipped** (`mini-installer`, D-0071)
8. health check — **shipped** (`mini-installer::Installer::health_check`)
9. rollback — **shipped** (`mini-installer::Installer::rollback`, and
   automatic on failed health check)
10. Failure Book entry if rollout fails — **process exists**
    (`docs/FAILURE_BOOK.md`, #91); not yet exercised by a real rollout
    failure, by design (nothing has shipped to real users yet)

Steps 1-9 are demonstrated end to end today, locally, over a real TCP
connection (`mini sync listen`/`connect`) — not yet a live distributed
network with concurrent installers or real network partitions. That gap
is exactly Batch 5's scope below.

## Dependency graph

```text
Founder Directives
  -> Decision Log (D-0001 to D-0075)
  -> Failure Book (live, #91)
  -> Roadmap Matrix (#92, this file, PRE_CODING_ISSUE_MATRIX.md)
  -> Self-Hosted Forge Spine (#102)
      -> Identity/Authorship .......... shipped
      -> Repository Objects ........... shipped
      -> Review Objects ............... shipped
      -> Merge Legitimacy .............. shipped
      -> Build Sandbox ................. shipped (Wasmtime, isolated)
      -> Build Provenance .............. shipped
      -> Release Transparency .......... shipped
      -> Installer State Machine ....... shipped
      -> Health Check/Rollback ......... shipped
      -> P2P Forge Sync (Batch 5) ...... NOT STARTED
      -> Horizontal Breadth (Batch 6) .. PARTIALLY STARTED (economics/
                                          personhood design done;
                                          everything else in #36-#45,
                                          #22, #18-20, #46-51 not started)
  -> External Gates (#99)
      -> Economics Simulation/Audit .... spec+harness done, external
                                          review not started
      -> Human Trust Research .......... spec done, research funding
                                          not started
      -> Hardware Presence Validation .. test protocol done, hardware
                                          execution not started
      -> DTN/Satellite Review .......... design reasoning done, expert
                                          engagement not started
      -> Legal Review ................. deferred to "day 0" (#96)
```

## Do-not-code-before list (still applies going forward)

### Do not code treasury/governance economics before
Economic failure thresholds (**have them**, `docs/gates/
economic-simulation-spec.md`); whale/capture model (**have it**, D-0074);
parameter-change timelock rules (**have them**, D-0073 §12/D-0074's
governance may/may-not list); treasury-drain scenarios (**have them**,
in the stress-test list, not yet run against real code since there's no
real treasury implementation to run them against).

### Do not code strong personhood rewards before
Human-trust signals explicitly weighted (**have it**, D-0075 §3); privacy
budget defined (**have it**, D-0075 §13); no single signal can mint full
authority (**enforced**, D-0054's live-vouching requirement); sybil cost
assumptions stated (**have them**, D-0075 §12 and the threats catalogue).

### Do not code BLE/UWB as proof before
Physical relay tests (**protocol written**, `docs/gates/
hardware-test-protocol.md` T4/T5 — not yet run, needs hardware);
same-room/different-room false-positive tests (**protocol written**, T1-
T3); device support matrix (**pending real hardware**); conclusion on
weak vs. strong signal (**pending real hardware**).

### Do not code local Wi-Fi as identity before
Router-fingerprinting risks understood (**documented**, `docs/gates/
wifi-bearer-test-protocol.md`); VPN/tunnel attack tested (**protocol
written**, W6 — not yet run); MAC-randomization behavior tested
(**protocol written**, W3); public Wi-Fi false positives measured
(**protocol written**, W7).

### Do not code disaster/global finality before
Partition mode defined (**have it**, `docs/gates/
dtn-design-constraints.md` Mode 2); local usefulness vs. global finality
separated (**have it**, same doc's finality rules — and already
architecturally true of `mini-settlement`'s pending/canonical split);
delayed settlement defined (**have it**, `mini-settlement`, D-0055); abuse
during partition modeled (**have it** as a rule list, not yet as
implemented code — nothing implements partition mode yet).

## Recommended order from here

1. ~~#102 Batches 1-4: developer/review/merge/build/release/install
   spine.~~ **Done.**
2. ~~#91 Failure Book seeded; #99 gate docs written.~~ **Done and
   maintained.**
3. ~~#47/#50 internal economic simulation; #21 human-trust architecture
   and interim spec; #97/#98 hardware test protocols; #28 disaster/
   local-first DTN constraints.~~ **Spec/simulation prep done** (see
   `PRE_CODING_ISSUE_MATRIX.md`'s high-risk table).
4. **Founder-decision-gated, current:** Batch 5 (P2P forge sync — closes
   the "still secretly depends on GitHub" gap) vs. resuming Batch 6
   (networked consensus, real BLE/UWB wiring, deeper `mini-value`/
   `mini-porep` work) — sequencing call, not spec-gated, explicitly the
   founder's per `docs/design/self-hosted-forge-spine.md`.
5. Whichever of Batch 5/6 is chosen, plus: hardware execution for #97/#98
   whenever devices are available; domain-expert engagement for #28;
   mechanism-design/research engagement for #47/#50/#21 whenever that's
   scheduled (deferred as its own topic per 2026-07-10 founder
   direction).
6. Only then, broaden feature coding further — the same discipline this
   file's original draft stated, still true, just with steps 1-3 now
   crossed off instead of pending.
