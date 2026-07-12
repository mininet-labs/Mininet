# Release and Owner Adoption

**Status:** Normative

## Separation of stages

Development completion, canonicalization, release eligibility, distribution, installation, and activation are separate events.

A developer cannot release merely by finishing code. Governance cannot activate merely by authorizing a release. A distributor cannot redefine canonical metadata. An installer cannot activate without owner approval.

## Governed release evidence

A release should bind:

- exact canonical source state;
- dependency and toolchain locks;
- pipeline definition;
- build runner and configuration digests;
- capabilities and resource limits;
- artifact and SBOM digests;
- independent builder attestations where required;
- release approval threshold;
- transparency-log checkpoint;
- rollback sequence and predecessor;
- issued-at and expiry/freshness data;
- known limitations and external gate status.

## Release authority

Release signers may be public or pseudonymous but must use persistent scoped keys and satisfy governance policy. Release signatures do not grant governance votes beyond their delegated role.

## Distribution

Any mirror may distribute artifacts. Verification must not depend on trusting the mirror. Mirrors may be blocked, replaced, or disappear without changing release legitimacy.

## Owner adoption

Activation requires an explicit typed owner approval naming the exact release or a narrowly defined policy voluntarily configured by the owner.

Owners may:

- inspect evidence;
- stage without activating;
- defer indefinitely;
- reject;
- select a different trusted channel;
- fork;
- roll back when local safety policy permits.

No hidden administrative channel may override this choice.

## Safety and content layers

Release availability of moderation, safety, filtering, or warning components does not make them compulsory. Owners choose which layers to subscribe to. Protocol-integrity checks remain mandatory where disabling them would falsify shared state.

## Failure handling

Installation must stage separately, verify all artifacts, activate atomically, perform health checks, journal state across crashes, and roll back to an owner-approved version on failure. Failed activation evidence should remain locally inspectable.

## Emergency releases

Emergency urgency may shorten deliberation and timelock only under explicit policy. It cannot create a forced-update path. Owners receive clear risk information and retain the final decision.

## Release/adoption separation

A release decision establishes eligibility for adoption, not activation. Every activation path must consume explicit owner approval, re-verify the exact release and applicable policy from raw evidence, and preserve refusal as a normal result. No emergency policy, governance vote, or safety classification may create a hidden forced-update path.
