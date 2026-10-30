# Mininet mobile OS and open phone ecosystem proposal

**Status:** design-only proposal; no OS, device certification, partner commitment,
release authorization, or production privacy claim is created by this document.
**Date:** 2026-09-19. **Repository baseline:**
`820d894f3a72736b88be5aa4fb42e888bcef8b3d` (`main`, inspected through GitHub).
**Requested direction:** an Android-derived system with Mininet preinstalled,
and competing manufacturers building privacy-centred phones for a network of people.

Companion documents: [delivery roadmap](MOBILE_OS_ROADMAP.md),
[device and partner requirements](MOBILE_DEVICE_REQUIREMENTS.md), and
[existing Android foundation](ANDROID_FOUNDATION.md).

## 1. Recommendation and decision being proposed

Build outward from the existing Rust core and native Android client into an
**optional Android-derived reference system**, initially for one qualified handset.
Keep the ordinary APK a first-class route into exactly the same network. Publish
open interoperability and security evidence requirements so several manufacturers
can ship compatible devices and compete on privacy, update support, repairability,
price, and battery efficiency. A manufacturer sells a better device, not a stronger
claim to Mininet membership or authority.

The product sequence is **usable mobile client → measured system integration →
one-device reference image → supported pilot → independent manufacturers**.
Investigate the system layer while completing mobile foundations, but do not commit
to maintaining a distribution until an on-device comparison proves its benefit and
named people can maintain its security updates. The reference image is a concrete
target, not an indefinite promise that an APK alone will fulfil this proposal.

This is a new proposal alongside D-0446's
[Node Appliance](../design/mininet-node-appliance.md), which deliberately gates a
full distribution and reserves the name “Mininet OS.” The appliance stays a Debian
deployment profile. This document uses **mobile reference system** as a working
description; an accepted mobile scope/naming decision is required before adopting
that reserved product name or committing OS maintenance resources. No accepted
decision, constitutional invariant, or release-roadmap completion is changed here.

The assumption that most companies will refuse Mininet is a risk hypothesis, not
an established fact. Distribution independence is valuable whether incumbents are
hostile, indifferent, or cooperative. Do not make an adversarial industry forecast
the only business case.

## 2. How this advances the cause

| Mission objective | Mechanism | Evidence that it actually helps | Counter-risk |
|---|---|---|---|
| People can enter without a platform account | Preinstalled client, owner-created identity, nearby onboarding and verifiable offline installation | Complete first-use and recovery with no Google, OEM, or Mininet account server | Preinstallation becomes coercive unless skippable and disableable |
| Build a network of people | Everyday messages, profiles, feeds and files bring repeat use; opted-in nearby peers improve local reach | Retained consenting pilot use; successful phone-to-phone and phone-to-desktop exchanges | Installed phones do not imply active humans or useful density |
| Survive central service failure | Replaceable relays/mirrors, peer-carried artifacts, local store-and-forward | Remove each default provider and still onboard, communicate locally, and import updates | A phone OS does not create long-range connectivity without a physical path |
| Privacy is structural | App isolation, minimal platform permissions, owner controls, no bundled analytics, bounded transport metadata | Traffic captures and adversarial tests tied to exact builds | Firmware, baseband, compromised endpoints and timing remain threats |
| Owners contribute infrastructure | Optional charging/Wi-Fi-bound caching, seeding and relay policies | Measure useful bytes delivered, energy and thermal cost | Mandatory hosting would burden poorer users and expose relationships |
| Companies compete on serving owners | Public test suite and comparable evidence, interoperable implementations | A second independent implementation interoperates without manufacturer credentials | A badge vendor or proprietary extension could become a new gatekeeper |
| Preserve equal rights | Same protocol and eligibility rules across stock Android, custom OS and other clients | No model/price/attestation field reaches vote weight or Human Share entitlement | Premium security must not become premium citizenship |

The strategic feedback loop is useful services → voluntary everyday use → more
reachable peers and useful content → better service availability → more useful
services. Preinstallation reduces entry friction; it cannot substitute for this
loop. Paid service provision is a later, separately gated economic path. Existing
service tickets are unsettled evidence, not money, guaranteed earnings, or passive
income from buying a phone. Human Share remains separate from service earnings.

This direction supports FD-01/02/03/06/09/11/14/16/18: human benefit, removable
dependencies, continuity, graceful failure, privacy, weak devices, simplicity,
the voice/value wall, and replaceable edge providers. It preserves U1–U3, P1/P2,
P5/P6, ID1, and A1 in [the invariant register](../INVARIANTS.md).

## 3. Current foundations and gaps

This is a source inspection, not a new Android build or hardware result. The
[living status](../STATUS.md) takes precedence over older planning prose.

| Existing surface at the baseline | Reuse | Missing evidence or implementation |
|---|---|---|
| `app/android`, `crates/mini-ffi`: Compose/UniFFI and shared Rust logic | Native shell and versioned core boundary | Full daily-use mobile client; emulator and physical-device evidence for complete flows |
| `RootCore`, `AndroidKeystoreCipher` | Identity/delegation primitives and Keystore-encrypted persistence | Current protocol signing keys are software keys; AES storage protection is not hardware-backed protocol signing; recovery and multi-device ceremonies need completion |
| QR/LAN signed follow pairing | First in-person social connection | Real two-phone camera/LAN validation; not proof of a unique human |
| `mini-bearer`, `mini-mesh`, `mini-ffi::mesh`, Kotlin BLE adapters | Existing bearer/CH1 and bounded relay composition | `BleMeshService` is not wired into UI; no demonstrated real-radio mesh; flooding is not internet-scale routing |
| `OperationLifecycle` | Typed checkpoint/suspend/failure semantics | Android foreground-service/WorkManager integration, Doze, process-death and restart handling |
| `mini-social`, `mini-profile`, `mini-sync`, connected desktop | Reuse signed objects, profile/feed semantics and transfer protocols | Mobile UI/FFI composition; durable outbox; reliable relay/rendezvous/CGNAT operation |
| Desktop media, Library, service tickets and peer search | Shared object formats and verified retrieval | Mobile delivery/playback and resource controls; service-ticket settlement remains absent |
| `mini-forge`, `mini-provenance`, `mini-update`, `mini-installer` | Release evidence, verification and adoption concepts | APK/OS signing custody and exact platform adapters; these crates do not currently flash phones or implement Android OTA |
| Android build/reproducibility workflows | Existing CI starting point | Governed APK provenance, image manifests, vendor-blob accounting, real image recovery/update trials |

Source anchors: [Android manifest](../../app/android/app/src/main/AndroidManifest.xml),
[FFI](../../crates/mini-ffi/src/lib.rs),
[BLE orchestration](../../app/android/app/src/main/java/org/mininet/app/BleMeshService.kt),
[mesh design](../design/ble-mesh-relay.md), and
[connected-client proposal](../proposals/connected-mininet-client.md).
The Android foundation's older “only INTERNET” prose predates the manifest's BLE
permissions; use the manifest and current status when specifying today's surface.
Likewise, earlier “mobile not started” summaries do not erase later partial work.

## 4. Platform choice

| Route | Useful properties | Cost or limitation | Proposed role |
|---|---|---|---|
| Ordinary Android APK | Broad reach; reuses existing code; smallest maintenance addition | Subject to device background restrictions and host-OS privacy | Permanent baseline and first delivery |
| Integrate with an established privacy Android distribution | Reuses maintained security work; helps test demand | Upstream acceptance, support policy and redistribution/branding cannot be assumed | Preferred collaboration investigation; no partnership claimed |
| Thin Android-derived reference product | Controls first boot, preinstallation, system settings and narrowly justified scheduling integration | Owns integration security, signing, OTA, firmware compatibility and support | One-device prototype after feasibility gate |
| Broad multi-device ROM fork immediately | Many nominal targets | Unsustainable device matrix and vendor patch burden before proving utility | Defer until one device is supported sustainably |
| New kernel, independent mobile Linux stack or hardware design now | Greater potential platform independence | Rebuilds drivers, telephony, app ecosystem and safety-critical daily-use support | Outside this proposal |

Use AOSP as the architectural base and evaluate maintained hardened downstreams
before selecting a pinned source baseline. Prefer a small product manifest and
reviewable integration patches; avoid a permanent framework fork where ordinary
APIs suffice. Native Kotlin/Compose remains the shell, Rust remains the protocol
implementation; do not reopen the Failure Book's Flutter decision.

AOSP source availability is not complete hardware independence. Device firmware,
drivers, redistribution rights, signing arrangements and support lifetime remain
constraints. Upstream security work helps only if someone integrates, tests and
ships it on time. Verify the current release and patch workflow at selection time.

Android app compatibility is a separate test programme. AOSP/CTS compatibility does
not by itself grant Google Mobile Services licensing, Play Integrity acceptance,
banking/DRM compatibility, or permission to use another project's branding.
Google services, if lawfully offered later, are an explicitly chosen compatibility
option and never a dependency of core Mininet functionality. Do not promise that
every Android app will work.

## 5. Proposed architecture and scoped features

### 5.1 Boundaries

1. **Shared Rust core:** identities, signed objects, synchronization, verification,
   transport policy and release evidence. Use existing crates and extend `mini-ffi`
   in bounded slices; never fork protocol rules into the OS or Kotlin UI.
2. **Unprivileged Android application:** Compose UI, explicit enrollment/recovery,
   profiles, contacts, messaging, feeds, search and Library as each backend permits.
3. **Optional platform adapter:** a narrowly scoped Binder interface for justified
   system facilities. Start without it; require an ordinary-APK experiment showing
   why each proposed privileged method is necessary. Check callers, user/profile,
   capability, payload bounds and cancellation. No protocol root keys in this service.
4. **Reference image:** upstream OS plus device support, app, setup integration and
   owner settings. SELinux enforcing, distinct UIDs, release keys, least privilege;
   no root/debug configuration in supported images and no shared platform UID for
   Mininet. Preinstalled does not mean privileged.
5. **Replaceable network services:** relays, rendezvous, mirrors, search and storage
   providers remain protocol-compatible and owner-selectable. OEMs cannot require
   their account, registry or server as the exclusive access path.

### 5.2 Feature contract

Stages refer to the [roadmap](MOBILE_OS_ROADMAP.md). “Required” below means required
for that proposed stage's exit; it does not claim that code exists now.

| ID / feature | Stage | Defined behaviour | Acceptance evidence |
|---|---|---|---|
| MOB-01 owner onboarding | P1 | Skip Mininet entirely or create/enroll a delegated device; explain recovery; no OEM account or automatic root custody | New and returning user flows; second-device revocation and encrypted recovery; cancellation leaves no enrollment |
| MOB-02 social and private delivery | P1 | Profiles/follow/feed and consented private messages over shared formats; durable encrypted outbox; distinguish queued, peer-acknowledged and read states | Restart/network loss during send; duplicate delivery; revoked/stale-key cases; phone↔desktop tests |
| MOB-03 nearby network | P1/P2 | Owner-started QR/BLE discovery; bounded mesh relay; negotiated local Wi-Fi transfer where available; explicit time/data caps | Two phones pair; three phones relay with endpoints out of direct range; permission denial and radio-off recovery |
| MOB-04 internet reach | P2 | Shared mobile/desktop reconnect, multiple replaceable relays and rendezvous; authenticated sessions where required | Different NATs/CGNAT, changing mobile IP, relay loss and malicious referrals; no default-provider dependency |
| MOB-05 lifecycle and notification | P1/P2 | Bounded sync, visible foreground mode when required, safe checkpoints; generic private lock-screen notifications | Doze, standby, process kill, reboot and denied permission; latency/energy curves; no mandatory Firebase route |
| MOB-06 resource contribution | P2 | Off by default; per-role storage/bandwidth/battery/thermal limits; charging/unmetered presets; stop immediately | Quotas cannot be bypassed; low-power and thermal stop; private/device-only content never seeded |
| MOB-07 shared content services | P2 | Feed, local/peer search and verified Library fetch; media only where actual codecs work; explicit scope indicators | Partial/corrupt download, offline view, bounded index and storage; no claim of global search completeness |
| MOB-08 owner privacy controls | P2/P3 | Local dashboard of permissions, roles, quotas and actual connections; discovery timeouts; quiet mode | Controls alter runtime behaviour, not just UI; no raw contact/location telemetry; all roles stoppable |
| MOB-09 release and recovery | P3 | Verified APK/image manifests, owner-approved installation, platform OTA and safe fallback; peer/offline artifact import | Wrong key/model/version rejected; interrupted update recovered; separate rollback-protection tests |
| MOB-10 developer integration | P2/P4 | Versioned documented app API with explicit grants, revocation, per-app identities/scopes where designed, samples | Malicious app cannot enumerate contacts, sign silently, extract keys or reuse another app's grant |
| MOB-11 first-boot system integration | P3 | Preinstalled client, optional setup step, Settings/quick toggle with narrowly scoped adapter only if needed | Setup skipped; app disabled; phone calls and ordinary apps still work; no implicit relay opt-in |
| MOB-12 interoperable OEM builds | P5 | Public capability report and tests, cross-vendor identity migration and content exchange | Two independently built products interoperate without vendor-specific protocol branches |

### 5.3 Privacy and identity rules

Protocol enrollment is not proof of unique humanity. Hardware attestation may help
an owner inspect device security; it cannot determine personhood, increase votes,
or make Human Share conditional on buying a phone. Do not transmit serial numbers,
IMEI, stable attestation identifiers, biometrics, browsing history or location
history to establish membership. Optional future personhood work stays within the
existing research and external-review gates; this OS does not solve it.

Keep the human root separate from ordinary delegated phone operations. Hardware
signer feasibility must check Mininet's actual signature suites against device
Keystore support; StrongBox availability alone proves no algorithm compatibility.
An unsupported suite needs an honest software-protected status, not silent suite
substitution or a claim that encrypting a key makes signing hardware-backed.
Biometric unlock, if offered, remains a local authorization option, not uniqueness.

Nearby discovery is opt-in and time-bounded. Avoid stable identity advertisements;
authenticate after connection as the chosen protocol requires. Packet captures
must evaluate residual identifiers and traffic patterns. Encrypted links do not
hide proximity, all metadata, or a global observer. Existing Mixed/Burst privacy
tiers must remain fail-closed until their executors and required audit exist.

Private messages need end-to-end protection between intended participants in
addition to hop encryption. Relays must not receive message decryption keys;
reject a delivery path that silently substitutes transport-only confidentiality.
Blocked senders, spam controls and metadata-minimizing queue limits belong in the
messaging slice and must not require a public directory of people's contacts.

The default phone is a bounded client, not a full validator, archive, exit relay,
or storage farm. None of these roles is an enrollment requirement. A local mesh can
bridge connected people and carry delayed data; it cannot bridge an arbitrary gap
without an actual carrier or route. BLE is a bootstrap/small-payload path, not a
promise of broadband. LoRa remains out of scope under the Failure Book/N1.

## 6. Update, custody and recovery model

Separate three chains of trust: Mininet governed release evidence, the Android
APK/image signing identity, and the hardware boot trust root. Evidence in one chain
does not automatically satisfy the other two. Ordinary Android update installers
and AVB remain the enforcement mechanisms for their respective layers; a future
adapter verifies Mininet evidence before handing an approved artifact to them.

Each manifest binds source revision, pinned toolchain, artifact hashes, exact
device/SKU, firmware prerequisites, security patch level, schema compatibility,
signing identity and recovery procedure. Compare independently rebuilt unsigned
payloads where signing is not byte-reproducible; verify signatures separately and
list proprietary blob hashes and unreproducible boundaries. Do not label an entire
image reproducible because its APK reproduced.

OS builds may distribute through ordinary mirrors and peer-carried chunks. Import
must verify offline against a previously trusted key/checkpoint; availability from
a nearby peer does not make that peer trusted or establish global freshness on a
first install. Document key acquisition, pinning, rotation, compromise recovery and
stale/unknown evidence. Offline operation must show uncertainty rather than infer
“latest” from a plausible timestamp or majority of downloads.

Installation is an explicit exact-version owner choice under U1. Declining an
update gives an honest security warning without a Mininet remote kill switch or
loss of protocol ownership. This proposal does not introduce blanket consent to
future versions. Allow schedules for an already approved artifact only. Urgent
patch notification and fast availability do not mean compelled activation.

Use the device's supported A/B or equivalent atomic update path. A failed new slot
may fall back only within the platform's supported rollback policy. Android
anti-rollback counters can prohibit installing older firmware even when the owner
wants it: recover with an admissible fixed image, not disabled Verified Boot. Test
database migration/backup compatibility separately from slot boot success. Never
advance an irreversible rollback index without a documented, tested recovery path.

OEMs must not factory-generate a person's root identity. Separate build/release
keys from identity custody and network authority. Publish signing custody,
rotation and end-of-support plans; secure production key ceremonies are a later
authorized operation, not a new secret or CI identity created by this proposal.

## 7. Limits, trade-offs and falsification

| Failure or counterargument | Response and stopping condition |
|---|---|
| The APK delivers the same benefit | Measure the same flows on stock and reference systems. If system changes add no material reliability/control benefit, retain APK and partner preload work and stop the fork |
| Security updates consume the team | Require funded maintenance ownership and repeatable patch/OTA drills before pilot expansion; stop new device targets when patch targets are missed |
| OEM firmware ends support | Disclose exact dates and migrate users; community userspace patches cannot be represented as full-device security support |
| Network still lacks useful services or remote reach | P1/P2 gates precede a daily-driver pitch; shipping a ROM does not close CGNAT, outbox or application gaps |
| Only expensive phones qualify | Preserve ordinary clients and weaker-device participation; separate security capability reporting from rights |
| Adoption is mistaken for Sybil resistance | No installed-device count, receipt ledger or OEM certificate is evidence of unique humans |
| Carrier/OS surveillance is claimed solved | Publish residual baseband/firmware, endpoint, timing, coercion and third-party-app risks; test cellular/Wi-Fi independently |
| A branded gateway becomes mandatory | Provider-removal drills and interoperable alternatives; no vendor account or online badge check in protocol admission |
| Mass ROM support damages security | One qualified device first, then require per-device staffing, patch and recovery evidence |
| Revenue depends on tracking or locked services | OEM margin and optional support/services are acceptable business hypotheses; no mandatory data extraction, proprietary network toll, votes or guaranteed token yield |

Out of the first release: new consensus/monetary rules, production payments,
biometric personhood, baseband replacement, a new app runtime, full replacement of
every incumbent app, iOS system replacement, custom silicon and bespoke long-range
radios. Calls/video calling, maps, dating and full business workflows require later
service-specific scope; bundling placeholders is not delivery.

## 8. Decisions required before implementation stages

The roadmap's gates name their evidence and accountable roles. The first review
should accept or revise this optional mobile product direction, its relation to
D-0446, and its bounded feasibility budget. Later decisions select the source
baseline, one hardware target, support commitment, signing custodians and audited
release scope. Those are explicit future decisions, not approvals implied by this
document or a GitHub merge. Existing A1, personhood, consensus and real-value gates
remain binding; [the release roadmap](../ROADMAP_TO_RELEASE.md) stays authoritative.

## 9. External engineering references

Primary sources checked 2026-09-19; revalidate at device/base selection:

- [Android compatibility programme](https://source.android.com/docs/compatibility/overview):
  CDD/CTS and ecosystem compatibility are separate from an open-source build.
- [Android background task restrictions](https://developer.android.com/develop/background-work/background-tasks/bg-work-restrictions):
  background behaviour must be measured under the actual OS policies.
- [Android Verified Boot](https://source.android.com/docs/security/features/verifiedboot/verified-boot):
  boot verification and rollback protection inform the platform adapter.
- [Android Keystore](https://developer.android.com/privacy-and-security/keystore):
  key isolation and actual hardware security properties must be distinguished.
- [GrapheneOS device-support requirements](https://grapheneos.org/faq#future-devices):
  hardware, firmware maintenance and support resources constrain secure device
  selection. This is a comparator, not an endorsement, affiliation or chosen base.

## 10. Review record

Prepared as an AI-assisted planning proposal at the user's request. Source
inspection and a self-review identified the main constraints: stale Android
summary prose, unvalidated physical BLE flows, missing CGNAT connectivity,
hardware-signing ambiguity, OS update authority versus owner consent, and the
danger of making premium hardware a citizenship gate. The sections above preserve
those findings. An independent human/security review remains required before
security-sensitive implementation or any production claim. No APK/image build,
hardware benchmark, external audit, manufacturer agreement or deployment was
performed for this documentation PR.
