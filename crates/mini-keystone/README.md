# mini-keystone

The keystone demo (SPEC-03), composed end to end from the beta crates and runnable
entirely offline:

1. Two identity roots incept `did:mini` roots and delegate `ATTEST`-capable devices.
2. The devices form an **anonymous, forward-secret encrypted channel** over a
   bearer — no identity on the wire (P5).
3. They exchange KELs **through the encrypted channel** and verify each other's
   identity + delegation offline (self-certifying; no registry, no server).
4. Both sign a **range-bound presence attestation** bound to this very channel;
   each side verifies it independently.
5. Verified presence accrues **non-spendable, slowly-maturing** reward per identity root
   (P2, P4) with no governance weight (P1).

`run_demo` drives the flow over any two connected `Bearer` endpoints: the CI test
uses the in-process bearer; the same call runs unchanged over the real BLE /
local-Wi-Fi adapter on phones (no internet, no radio beyond BLE/Wi-Fi).

```sh
cargo test -p mini-keystone
cargo run -p mini-keystone --example keystone
```

License: CC0-1.0 (public domain).
