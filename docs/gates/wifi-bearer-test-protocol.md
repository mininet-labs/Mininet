# Local Wi-Fi bearer — validation protocol

Gates [roadmap #98](../../issues/98)
(split from #22, the local-data-transport half — see `docs/gates/
hardware-test-protocol.md` for the physical-proximity half, #97). **Founder
action required: same phones/router/laptop as #97** — real hardware, no
sandbox substitute.

## What Wi-Fi bearer can and cannot prove

At best: *"this device appears to share a local network context with the
verifier/home node during this epoch."* It must never be read as
*"this is a unique human"* or *"this device is physically present beyond
doubt"* — local network co-membership is easier to fake than UWB ranging
(a VPN, tunnel, or shared hotspot can manufacture it), so this stays a
lower-priority, lower-weight signal than #97's presence/ranging evidence.
In `docs/design/human-continuity-proof.md`'s terms this is connectivity
evidence feeding the "device or home-node continuity" signal class
(capped at 15/100), never a standalone trust source.

## Hardware

Minimum: 2 phones, a home router, a laptop for logs. Better: 2 routers, a
Raspberry Pi/home node, a mobile hotspot, a VPN/tunnel setup, access to a
public/cafe Wi-Fi network for the false-positive test.

## Test classes

- **W1 — same-SSID baseline.** Do devices see the same SSID/BSSID/local
  network context?
- **W2 — same-LAN challenge.** Verifier issues a local-network challenge
  to the prover (local UDP challenge, mDNS discovery, local HTTP
  challenge to a home node, router-neighborhood fingerprint).
- **W3 — MAC randomization.** Do phone privacy features (rotating MAC
  addresses) break continuity tracking?
- **W4 — router reboot/change.** How stable is the signal after a router
  restart, ISP change, or SSID change?
- **W5 — mobile-hotspot sybil farm.** How easily can one hotspot plus
  many phones fabricate a fake household-like context?
- **W6 — VPN/tunnel attack.** Can a remote device appear local through a
  tunnel or proxy?
- **W7 — public Wi-Fi false positive.** Does a cafe/library/public
  network make unrelated strangers look like shared household context?

## Log schema

Shared with #97 — `docs/gates/hardware-test-log-template.csv`:
`issue, test_id, run_id, timestamp_utc, device_a, device_b, network_type,
ssid_visible, bssid_visible, same_subnet, local_challenge_success,
latency_ms, vpn_enabled, hotspot_enabled, public_network,
router_fingerprint_match, success, suspected_false_positive, notes`.

## Acceptance criteria

Marked hardware-tested when: home router, hotspot, VPN/tunnel, and
public-Wi-Fi cases are all tested; the protocol demonstrates whether
Wi-Fi bearer adds meaningful evidence beyond bare self-report; the trust
contribution is capped and documented; and it's explicit whether this
belongs in an MVP or is post-MVP (recommendation below leans post-MVP,
lower priority than #97, since it's a convenience/robustness signal, not
a launch-blocking security one — matching roadmap #98's own stated
priority relative to parent #22).

## Recommended design, pending the actual test results

Treat Wi-Fi bearer only as a weak score component: useful for household
continuity, useful alongside home hardware evidence, useful for local
peer discovery, useful during disaster/local-first mode (`docs/gates/
dtn-design-constraints.md`) — **not** useful as standalone sybil
resistance on its own.

## What closes this gate

A completed test run across W1–W7 with logged results, a documented
strong/medium/weak/unusable classification, and an explicit MVP vs.
post-MVP recommendation — same evidentiary bar as #97, cross-referenced
from whichever presence/personhood audit doc ends up citing it.
