# `mini-objects` envelope forward-extensibility review (issue #64)

**Date:** 2026-09-21 · **Refs:** D-0529; `crates/mini-objects/src/{object,
envelope_v2}.rs`; D-0021 (original unified envelope decision); `MN-103`
(`ObjectEnvelope` v2, the L1 lane, D-0300).

## Question

Can entirely new object types be added decades from now without a breaking
migration, and without any single party's schema proposal getting special
authority over the format?

## What the envelope already gets right

- **Explicit schema-version field.** Both wire formats lead with a version
  byte checked strictly on decode: v1 `Object::from_bytes` requires `1`
  (`ObjectError::BadObject` otherwise); v2 `ObjectEnvelopeV2::from_bytes`
  requires `ENVELOPE_VERSION` (`2`, `ObjectError::UnsupportedEnvelopeVersion`
  otherwise). No parser ever falls back from one version to the other
  (`envelope_v2.rs` module docs, tested by
  `a_v1_object_is_rejected_by_the_v2_parser` /
  `a_v2_envelope_is_rejected_by_the_v1_parser`). A genuinely new structural
  format gets its own version byte and its own decoder rather than
  overloading an old one — the correct place to spend a breaking change,
  since the alternative (silently reinterpreting bytes) is worse than a
  clean reject.
- **Object *type* is already open, not a closed enum on the wire.**
  `ObjectType::WellKnown(u16)` accepts any tag value and `Custom(String)`
  accepts any name; `Object::from_bytes` never rejects a value merely for
  being unrecognized by the running build. This was already true before
  this review — `ObjectType`'s own doc comment calls it "SPEC-09: well-known
  core set + Tier-O custom types" — but it was untested and undocumented as
  a *forward-compatibility* property specifically. This review added:
  - `object_of_a_future_unknown_well_known_type_round_trips_losslessly` and
    `object_of_a_future_unknown_custom_type_round_trips_losslessly`
    (`crates/mini-objects/src/object.rs`): sign, encode, decode, and
    re-encode an object of a type tag/name this build has no associated
    constant for, asserting byte-identical round-trip and that integrity
    (`verify_integrity`) and signature (`verify_signature`) verification
    succeed without needing to understand the type at all.
  - `an_unrecognized_well_known_type_falls_back_to_the_narrowest_capability`:
    `required_capability` is total over the whole `u16` space (a `match`
    with a `_` arm to `Capabilities::SIGN`, the narrowest scope), so
    provenance verification of an unrecognized type never panics or
    silently grants a broader capability than the type deserves.
  - `mini-objects/tests/objects.rs` already had
    `custom_types_and_encrypted_payloads_round_trip` for `Custom`; this
    review's new tests extend the same property to unrecognized
    `WellKnown` tags and add explicit assertions about what an *old* node
    (one that predates the type) can still safely do with the object.
- **No single-party gate on new type identifiers.** A new `WellKnown`
  associated constant (e.g. `ObjectType::WALL`) is an ordinary source change
  to `mini-objects`, merged through the same `mini-forge`
  propose/approve/merge path (2-approval protocol floor, `KelDirectory`
  oracle) as any other change — there is no maintainer allowlist, code-owner
  veto, or external registry service with special authority over which type
  identifiers exist. `Custom(String)` needs no gate or registry at all: any
  author mints a namespaced name (`"chess/move"`) unilaterally, at the cost
  of self-managed collision risk the naming convention is meant to keep low.
  This matches Directive-level "no privileged party" framing and required no
  code change — it was already the case.
- **Error surface is `#[non_exhaustive]`.** `ObjectError` already carries
  `#[non_exhaustive]`, so a future variant (for a new failure mode a v3
  format might introduce) doesn't force every downstream `match` to be
  rewritten in lockstep — only to already have a wildcard arm, which is a
  compile-time nudge in the right direction, not a hard requirement (this
  review did not add exhaustiveness enforcement; that's a possible follow-up
  if a `match` without `_` is ever found on this type).

## Real gap found and fixed

`Object`'s wire encoding uses `u16::MAX` as the tag meaning "this is
`Custom`, read a name next" (`type_tag == u16::MAX` in `from_bytes`,
previously written as the literal `u16::MAX` in both encode and decode).
Nothing stopped a caller from constructing `ObjectType::WellKnown(u16::MAX)`
and signing it: `Object::to_bytes` would emit exactly the bytes a decoder
reads back as `Custom` with a zero-length name — which `from_bytes` itself
already rejects (`if name.is_empty() { return Err(BadObject) }`), so the
concrete failure mode was "silently un-decodable, at the *wrong* layer of
the stack, days or years after the object was signed" rather than an
immediate, legible error at construction time. In a system meant to run for
decades, a single accidental or malicious use of tag `65535` — e.g. by
whatever process eventually allocates the last well-known slot — would have
produced permanently corrupt (able to be signed, unable to ever cleanly
decode) objects.

**Fix:** named the reserved value `CUSTOM_TYPE_MARKER` (replacing both
`u16::MAX` literals so encode/decode/guard all read from one source of
truth), and `ObjectBuilder::sign` now rejects
`ObjectType::WellKnown(CUSTOM_TYPE_MARKER)` with `ObjectError::BadObject`
before it ever reaches the wire. Test:
`signing_the_reserved_custom_marker_as_a_well_known_tag_is_rejected`. This
does not shrink the usable `WellKnown` space in any way that matters:
14 of `u16::MAX - 1` ≈ 65,534 well-known slots are allocated today.

## What this review deliberately did not change

- **No move away from `WellKnown(u16)` + `Custom(String)`.** The task
  description raised "move from a closed Rust enum to an open/extensible
  representation (tagged variant + raw-bytes fallback for unrecognized
  tags) if that's what's missing" as a *possible* fix — but `ObjectType` is
  already exactly that shape and was already exercised by an existing test
  (`custom_types_and_encrypted_payloads_round_trip`). Replacing a
  already-open representation with another open representation would be
  churn, not a fix; Directive 14 (simplicity is security) argues for
  leaving it alone once the round-trip property is actually proven, which
  it now is.
- **`Payload`'s two variants (`Public`/`Encrypted`) are a genuinely closed
  enum on the wire** (`match r.u8()? { 0 => ..., 1 => ..., _ => Err(BadObject) }`).
  A third payload *mechanism* (not object type — e.g. a different AEAD
  framing) would need a new envelope version, the same way v1 → v2 already
  worked, rather than a silent new payload tag. This is intentional and
  out of scope for this review: `Payload` describes wire-level confidentiality
  mechanism, not application object type, and the "new object types without
  breaking migration" question issue #64 asks about is about the latter.
  Flagging it here so a future payload-mechanism change doesn't mistake this
  silence for an oversight.
- **`ObjectEnvelopeV2`'s `RetentionClass` is `#[non_exhaustive]` but its
  wire decode is still a closed match on 3 tags** (`from_tag`). Same
  reasoning as `Payload`: this is envelope-level storage metadata, not
  object type, and a new retention class is exactly the kind of thing that
  should require a version bump (so old storage nodes don't silently
  misclassify retention they don't understand) rather than silently
  round-tripping an unknown tag as data.

## Conclusion

Issue #64's core property — new object types addable decades from now
without a breaking migration or single-party gate — was **already true in
the shipped design** (D-0021's original "extensible type" decision), but
untested as a forward-compat guarantee and carrying one latent
wire-encoding bug (`WellKnown(u16::MAX)` colliding with the `Custom`
discriminator). This review adds the missing round-trip tests, closes the
collision with a construction-time guard, and documents the "no
single-party gate" property, which required no code change — it already
routes new type identifiers through `mini-forge`'s existing governance path
like any other change to this repository.
