# Open Mininet phone requirements and manufacturer programme

**Status:** proposed requirements; no certified product or operating certification
programme exists. Read with the [proposal](MOBILE_OS_PROPOSAL.md) and
[roadmap](MOBILE_OS_ROADMAP.md). Requirements are for supported reference products;
they are not admission conditions for people or ordinary Mininet clients.

## 1. Why manufacturers might participate

An open software/protocol stack can let manufacturers differentiate through
measurable privacy, long support, repairability, affordability and well-integrated
services instead of building a proprietary social graph. Possible revenue includes
hardware margin, repairs, optional support and replaceable service provision.
These are business hypotheses to validate through partner discovery, not signed
deals or guaranteed demand. Mininet offers interoperability, documentation and an
evidence format; it must not guarantee income, exclusive customers or governance
influence in exchange for preinstallation.

No exclusivity, protocol toll, privileged Human Share allocation, voting weight,
mandatory OEM identity, manufacturer recovery escrow, hidden telemetry, forced
seeding, compulsory update or remote Mininet kill switch is acceptable.
Factory setup must let the owner skip Mininet and disable its services afterwards.
Explain reset/uninstall/data deletion choices separately from identity revocation.

## 2. Three independent capability descriptions

| Description | Meaning | What it never means |
|---|---|---|
| Protocol-compatible client | Passes public wire-format and behaviour tests, including ordinary Android APKs | Unique human, audited device, privileged membership |
| Reference-system supported device | Exact SKU/image has boot, firmware, update/recovery and daily-function evidence plus a funded support commitment | All products from that manufacturer are supported |
| Independently evaluated privacy/security capabilities | Specific third-party review and test evidence for an exact hardware/software revision | “Unhackable”, anonymous cellular use, governance standing or permanent certification |

Avoid a single opaque privacy score that hides trade-offs. Publish a feature matrix
with evidence dates and limitations. Owners and other evaluators can verify reports
offline. A logo/licensing scheme, if later desired, needs a separate governance and
legal proposal; access to the protocol must never check a badge server.

## 3. Reference-device selection requirements

| Area | Proposed requirement | Evidence / failure criterion |
|---|---|---|
| Alternate OS boot | Legitimate unlock, custom OS verification and safe relock for exact SKU; preserve hardware security functionality | Physical install/relock/boot test; reject SKU whose relock bricks or disables required protection |
| Verified Boot | Enforced boot verification and documented rollback protection; visible verification-key identity where supported | Modified boot/system image rejected; safe recovery with allowable versions demonstrated |
| Firmware and kernel | Ongoing vendor firmware/driver/security support, identified upstream and redistribution rights | Bill of materials, patch source/terms, support end date; reject unsupported or illegally redistributable components |
| Support commitment | Proposed partner floor: five years of complete security support from first retail sale, with end date disclosed at sale | Signed/resourced partner commitment; longer support differentiates; P0 verifies feasibility rather than assuming current devices meet it |
| Patch response | Proposed target: available applicable upstream security fixes integrated/tested within 30 days; critical exploitable issues triaged within 72 hours of notice | Timestamped advisory→triage→tested release record; disclose vendor delay, compensating measures and unresolved exposure |
| Key custody | Per-device secure storage, truthful TEE/secure-element properties and supported algorithms | Actual generated-key properties and independent review; no “hardware signing” from AES-at-rest alone |
| Isolation | SELinux enforcing, sandbox separation, production debug access disabled, protected locked-device interfaces | Policy audit, malicious-app/profile tests, locked USB/debug test |
| Updates/recovery | Supported atomic/A-B equivalent update, owner consent, boot failure recovery, compatible state migration | Repeated power-loss/full-disk/bad-payload and anti-rollback tests on exact firmware |
| Radios | BLE central/peripheral behaviour, supported Wi-Fi handoff, ordinary cellular capability documented | Three-device relay, coexistence, radio-off/permission-denial, connectivity handover trials |
| Privacy defaults | No hidden telemetry or factory identity enrollment; minimal app privileges; explicit resource sharing | First boot and idle traffic inventory; storage inspection; refusal/disable paths tested |
| Daily phone use | Calls, SMS, data, audio, camera, encryption, accessibility and alarm reliability | Documented matrix; emergency calling validated through authorized carrier/lab procedures, never casual calls to emergency services |
| Compatibility | Honest app/service matrix and applicable Android compatibility results | Report tested versions; no blanket banking, DRM, NFC-payment, IMS/VoLTE/VoWiFi or Play Integrity promise |
| Repair and affordability | Published battery/spares/repair terms, total support cost and migration guide | Compare factual lifetime costs; no price tier affects protocol rights |
| Continuity | Owner export/recovery and alternate compatible client path; provider-removal drill | OEM endpoints disabled; device identity can be revoked/replaced without OEM approval |

For the first target prefer a currently maintained, independently documented
reference handset with demonstrable custom-OS boot security and enough remaining
firmware support. Select the exact model only after P0 verification. A familiar
brand name, an unlocked bootloader or a generic image that boots is insufficient.

## 4. Where vendors can compete

Publish measured update latency, remaining support, idle network contacts, radio
privacy controls, key-isolation features, battery cost per useful transfer,
repairability, locally available spare parts, accessibility and price. Optional
hardware microphone/camera disconnects or stronger secure elements can be useful
if tested; software toggles must not be marketed as physical disconnection.

Require common protocol behaviour across these choices. Vendor optimizations must
negotiate capabilities and retain a documented fallback. No vendor-private packet
extension may be necessary to contact another manufacturer or recover an identity.
App developers should target the public client interface, not an OEM-specific fork.

## 5. Partner onboarding and exit

1. Publish requirements, contribution guidelines and the reference test plan.
2. Invite non-exclusive technical evaluation only after the prototype is credible;
   no outreach or commercial representation is performed by this proposal.
3. Obtain exact hardware/support/firmware evidence, commercial maintenance owner
   and distribution rights; disclose gaps before branding discussions.
4. Build and test independently; compare source manifests, blob hashes, privacy
   defaults, security review and cross-vendor behaviour.
5. Pilot under applicable gates, with owner consent and an explicit end-of-support
   route. Require evidence for each model and release, not a company-wide promise.
6. Expand only when both upstream patching and local support are sustainable.
7. On missed commitments publish factual status/expiry, suspend supported-product
   claims, and offer migration. Do not remotely disable clients or revoke people's
   identities because an OEM lost programme standing.

Report fields: hardware SKU/revision; image/source commit and hashes; Android/vendor
patch levels; build manifest and unreproducible components; verification-key
fingerprints; tests with exact outputs; assessor/scope/date; unresolved findings;
support end date; patch and incident contacts; migration instructions. Avoid IMEI,
owner identity, private keys, raw contact graphs or individual traffic histories.
Report signatures establish authorship, not truth or protocol legitimacy.

## 6. Edge-provider doctrine tests

| FD-18 test | Required outcome |
|---|---|
| T1 Disappearance | OEM/relay closure cannot take Mininet identity or canonical rights away; offline/local client continues, with honest hardware end-of-support limits |
| T2 Substitution | Owner chooses another compatible phone, client, relay or mirror and retains recoverable identity/data within protocol rules |
| T3 Voice wall | Spending, sponsorship, shipment count and certification buy zero votes, review authority or validator standing |
| T4 Learning | No mandatory personal history, contact upload, legal identity, hardware identifier or traffic database is created for the provider |
| T5 Off switch | Owner can stop the service; vendor cannot remotely turn off Mininet membership or command software adoption |

The programme introduces an optional product-support relationship, not a new
network authority. The existing provider doctrine and its non-negotiables remain
unchanged. Standard cellular/network operators still learn information inherent in
their service; neither this matrix nor an OS can truthfully promise its elimination.
