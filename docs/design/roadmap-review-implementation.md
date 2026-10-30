# Review roadmap implementation against the current tree

The supplied *Mininet Project Review and Roadmap* describes an earlier,
22-crate snapshot. This is an engineering gap map, not an adopted protocol
change or evidence that any external legitimacy gate has closed.

## Implemented in this change

`mini-value` now exposes `derive_tagged_output`,
`StealthSharedSecret::view_tag`, and `recognizes_tagged_output`. Tags use
BLAKE3's derive-key mode with the versioned
`MINI_VALUE_STEALTH_VIEW_TAG_V1` context and the existing compressed DH
shared point. Existing output structs, address derivation, legacy scanner,
and wire encodings remain unchanged. Callers explicitly opt in; no wallet
or ledger transport currently carries this hint.

The receiver must still calculate `b*R` before checking the tag. For honest
unrelated outputs, only about 1/256 proceed to scalar derivation and the
second, basepoint multiplication. This does **not** eliminate DH work or
promise an overall 99.6% speedup. Matching tags never authorize payments;
full address comparison is mandatory. Corrupted tags can hide a payment,
so a future wire integration must bind them to authenticated output data
and preserve an unfiltered recovery scan. Malicious senders can force tag
matches. No performance benchmark or constant-time audit is claimed.

The PDF's illustrative byte-mixing hash is not a cryptographic hash and
was not used. Its callback-based DKG and settlement sketches are not
substitutes for actual proofs or canonical ledger transitions.

## Other tracks: existing code and remaining work

| Roadmap track | Current implementation entrypoints | Remaining boundary |
|---|---|---|
| A1: DKG and resharing | `crates/mini-treasury/src/frost_dkg.rs`, `frost_reshare.rs`; `docs/gates/dkg-audit-scope.md` | Prototype review, adversarial network integration, external audit; not production custody |
| A2: stealth tags / compact proofs | This change; `mini-value/src/mlsag.rs`, `bp_range_v2.rs` | Tags need authenticated wire/wallet adoption; CLSAG and Bulletproofs+ require separately reviewed protocol and migration work, not a renamed MLSAG or standard Bulletproof |
| B1/B2: presence and transport | `mini-presence/src/ranging.rs`, `mini-bearer/src/tcp.rs`, `android_ble.rs`, `encrypted_link.rs` | Native UWB/FTM hardware evidence, local Wi-Fi discovery/radio integration and physical relay testing remain; a Rust marker cannot authenticate hardware measurements |
| B3: graph/routing | `mini-net`, `mini-uniqueness`; `tools/f5_phase2_model.py` | Sybil/personhood bounds remain research; neither spectral damping nor self-reported diversity proves unique humans |
| C1: offline reconciliation | `mini-settlement/src/reconcile.rs`, `claim_v2.rs`; `mini-private-payment/src/reconcile.rs` | Escrow channels with dispute windows are a new protocol, not a safe replacement for existing signed offline promises and canonical acceptance |
| C2/C3: treasury and emissions | `mini-treasury/src/rate.rs`, `receipt.rs`; `mini-economy/src/issuance.rs`; `mini-reward/src/lib.rs` | External economic review and unified service-reward incentives remain; do not introduce automatic cross-chain custody/watchtowers without protocol review |
| C4: consensus guard | `mini-consensus/src/round.rs`, `mini-chain`; `mini-value` balance verifiers | Preserve specified consensus/quorum and voice/value dependency wall; an imported 80% UNL rule is not adopted by this change |
| D1: backup and failure history | `deploy/backup/`, `docs/FAILURE_BOOK.md` | Backup tooling does not establish a mandatory, verified off-device identity inception gate in every client |
| D2/D3: reproducibility and appliance | `.github/workflows/reproducibility.yml`, `deploy/`, `mini-update`, `os/` | Existing reproducibility workflow is active; hardware image/A-B rollout and independent builder evidence must be evaluated separately |
| D4: economic simulation | `crates/mini-econ-sim/` | Deterministic cohort model exists; its README explicitly excludes prices, service-reward concavity, personhood costs and market shocks. Monte Carlo inputs alone would not validate them |
| A4/B4: external legitimacy | `docs/gates/crypto-audit-scope.md`, `dkg-audit-scope.md` | Credentialed external audit and hardware-in-loop results cannot be supplied by coding agents; all existing release gates remain open |

This PR is the bounded view-tag implementation slice. It does not claim to
complete the full multi-wave roadmap, implement physical adapters, or
change governance, economic, or consensus policy. It follows Directives
9–12 and 14 by reusing existing primitives and keeping the optimization
additive, explicit, and independently reviewable under D-0036/D-0037.
