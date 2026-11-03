# One identity root per physical device ("burned device") — founder proposal mapped to the tree

**Status:** proposal only. Written 2026-11-03 in response to a founder chat
direction. Not a Decision-Log entry, not implemented, not activated, no frozen
row weakened. Chat direction is task authorization, not a canonical Decision.
It builds on `humanness-score-and-peer-vouching-proposal.md` and
`android-keystore-signer-adapter.md`, and must be read with
`docs/FOUNDER_DIRECTIVES.md` Directives 9, 11, 15, 17 and 18.

## The requirement (founder, paraphrased)

A phone or PC may be used to register **one** `did:mini` identity root, ever.
Once used, the device is burned for creating new roots. The burn must not expose
anyone: only some identifier is blacklisted once used. Emulated devices (virtual
machines, virtual phones) must not be able to register at all. Purpose: Sybil
resistance.

## Why the requirement cannot ship as a mandatory network rule

Each point below is a conflict with something already canonical, not a
preference.

1. **Telling real hardware from an emulator needs a vendor's signature.** The
   only available proof is a remote attestation chain rooted in a vendor
   (Android Key Attestation, Apple App Attest, a TPM endorsement certificate).
   Directive 18 asks of any outside service, "if it disappeared tonight, does
   anyone lose more than convenience?" If registration *requires* a vendor
   attestation and the vendor stops issuing it, nobody new can register. That is
   more than convenience.
2. **Humanness signals are a closed set.** INV-18-03 says no provider-derived
   variant may be added. A vendor-attested device cannot become a humanness
   signal. It can only be an admission or rate-limiting rule that never feeds a
   `HumanRecord`.
3. **A global burn list is a linkable registry of devices.** Directive 9: the
   safest information is information the network never learns. If a verifier
   sees the attested hardware key to check it, it learns which device made which
   root. Hiding that requires a zero-knowledge proof of "an attested, unburned
   device" that outputs a one-way nullifier. That is a cryptographic design
   nobody outside this repo has reviewed in this exact form, which our rules
   forbid inventing (D-0063 line: compose already-reviewed constructions;
   external audit, D-0047, gates anything value-bearing).
4. **"Permanent" cannot be delivered with privacy.** On phones the attested key
   is per key, not per device: a factory reset makes a new key and a new
   nullifier, so the burn is evaded by anyone willing to reset. A stable
   per-device identifier exists (device-ID attestation, a TPM endorsement key,
   Apple DeviceCheck bits), but it is exactly the tracking handle point 3 rules
   out, and DeviceCheck stores its bits on Apple's servers (point 1).
5. **A mandatory permanent burn locks out honest people.** A second-hand phone,
   a stolen-then-wiped phone, and the family or village phone that several
   people share would all be permanently unable to register a second person
   (Directives 11 and 15; the "future child" test, Directive 17). It also
   contradicts the founder's earlier instruction that Sybil defence must not
   keep honest participants from money or votes.

## Proposed form: an optional "burned-device lane"

Keep the goal — a physical device is a scarce, burnable resource — but make it a
*lane*, not a gate.

- **Two lanes to a usable identity.** The **social lane** is the existing one:
  vouching plus physical presence, graded standing (policy v3). The **device
  lane** is new and optional: register from an attested real device and receive
  a *bounded onboarding allowance* (for example, a larger free allowance in the
  action-fee schedule, D-0546, for the first weeks).
- **The device lane never grants standing, votes or money.** It is not a
  humanness signal (INV-18-03) and does not feed `HumanRecord`. It only lowers
  friction, so the voice/value wall (Directive 16) is untouched.
- **The burn applies to the lane.** A device can use the device lane once. A
  second root created on the same device goes through the social lane like
  anyone else. Nobody is locked out of anything, so resale, theft and shared
  phones are harmless.
- **Emulators are excluded from the lane, not from the network.** A virtual
  device cannot produce a valid attestation, so it cannot take the cheap lane
  and must earn standing socially. That is the Sybil benefit: a farm of
  emulators gets nothing from the device lane, and a farm of real phones pays
  for a phone each.
- **Vendor roots are published data, not a service.** Accepted attestation
  roots are a governed list verified offline. If a vendor vanishes, that vendor's
  devices simply lose the lane; the social lane keeps working (Directive 18).
- **The burn identifier.** A one-way nullifier derived from the attested key and
  a fixed domain string, shown to verifiers only as a proof, never the key. Until
  a reviewed zero-knowledge construction exists, the lane **must not ship**: the
  interim alternative (verifier sees a hash of the attested key) is a privacy
  cost Directive 9 does not allow us to accept quietly.

## What this honestly buys

- Real, but bounded: each fast-lane identity costs a physical device. A
  determined farm resets phones (point 4) or buys cheap used ones. It raises the
  price of cheap fake identities; it does not make them impossible.
- On PCs a TPM endorsement key persists across OS reinstalls, so the burn would be
  stronger there — and so is the tracking risk, which is why the proof must hide
  the key.
- It does not touch the open problem: an identity root is still not a verified
  human.

## Decisions needed from the founder

1. Accept the **optional lane** form, rather than a mandatory burn? A mandatory
   form needs the lawful amendment process for the constitutional rows in
   points 1–2, not chat direction.
2. What may the lane grant? Recommendation: only a bounded onboarding allowance
   (reduced action fees), never standing, votes or money.
3. Agree that the lane **waits for a reviewed zero-knowledge nullifier
   construction** (external review) rather than shipping a linkable interim?
4. Is "phones" in the original request about emulated phones only? This
   proposal assumes yes.

## What would be built after approval (nothing yet)

1. A `RegistrationLane` / `BurnRegistry` abstraction in `mini-uniqueness`, with an
   in-memory implementation for tests and no vendor code.
2. A fixed registration-policy hook so a lane's allowance is a published,
   versioned parameter, like `PersonhoodPolicy`.
3. Platform attestation adapters as **edge** crates (Android Key Attestation,
   later others), kept outside the core by the edge wall (INV-18-01), with the
   attestation roots as governed data.
4. The nullifier proof, only after external review.

## Not built, not decided

Everything above. No device identifier is collected today, none is blacklisted,
and no attestation is checked anywhere in the tree.
