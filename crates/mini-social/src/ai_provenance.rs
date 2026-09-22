//! Structural authorship-provenance labeling for AI-generated or AI-mediated
//! content (roadmap issue #63: "Design object types for AI-generated or
//! AI-mediated content ... that are clearly labeled as such and cannot be
//! laundered into looking like human-authored content or human governance
//! participation" — `docs/FOUNDER_DIRECTIVES.md` Directive 12 ("AI may
//! propose. Humans decide."), `docs/governance/01_DEVELOPMENT_CONSTITUTION.md`
//! Article VI ("AI serves humanity": AI output can constitute useful
//! evidence but "AI assistance must be attributable to a persistent proposal
//! record") and Article VIII ("Voice/value separation": no signal may buy
//! governance weight it did not earn through the real process).
//!
//! ## What this closes, precisely
//!
//! An ordinary [`crate::post::PostKind::Plain`]/`Media` post has no
//! provenance field at all — nothing distinguishes AI-authored text from
//! human-authored text in the wire format. [`AuthorshipProvenance`] is a new
//! post shape (`mini/post-ai-provenance`, a distinct [`ObjectType::Custom`])
//! whose [`AuthorshipProvenance`] claim is **encoded inside the signed
//! payload itself** — [`build_labeled_post`]/[`publish_labeled_post`] take
//! it as a required, non-optional constructor argument (never
//! `Option<AuthorshipProvenance>`), and [`decode_labeled_post`] refuses to
//! decode an object of this type at all unless a well-formed provenance
//! claim is present. Because the claim is part of what the device signs
//! (`Object::signing_bytes`, unchanged in this crate — see
//! `mini-objects::object`), it cannot be added, changed, or stripped after
//! signing without invalidating every device signature on the object; see
//! the `tampering_the_provenance_byte_breaks_the_signature` test below for a
//! direct proof of that property.
//!
//! ## What this does *not* and cannot do — read before relying on it
//!
//! This makes **honest** labeling easy, and makes an AI-labeled claim
//! **unstrippable** and **independently checkable** by any holder (the
//! payload is public plaintext, like an ordinary post). It does **not**,
//! and cannot, stop a dishonest human from publishing AI-generated or
//! AI-mediated text through the *ordinary* [`crate::post::publish_post`]
//! path instead, with no provenance object anywhere — exactly the way a
//! human can already lie about anything else they sign. No cryptographic
//! construction proves the *absence* of AI involvement in a plain post, and
//! this module does not claim otherwise. What it enables is the falsifiable
//! half: a plaintext post's content is public, so anyone (this device, a
//! peer, an external detector) can compare it against other evidence — most
//! directly, an identical or near-identical passage that *does* carry a
//! verified `AiGenerated`/`AiAssisted` claim elsewhere — and challenge the
//! omission through the ordinary reputation/moderation tooling this
//! workspace already has (`mini-objects::ObjectType::FILTER_LABEL`, device-
//! local mute lists). No new detection mechanism is built or claimed here;
//! this module supplies the unstrippable label for the honest case and the
//! plaintext for the challenge case, nothing more.
//!
//! ## Never governance or personhood weight (Directive 12, Article VI/VIII)
//!
//! `mini-social` depends on `did-mini`, `mini-crypto`, `mini-objects`, and
//! `mini-store` only (see `Cargo.toml`) — it has no dependency edge to
//! `mini-forge` (code-governance quorum), `mini-chain` (finality voting), or
//! `mini-uniqueness`/`mini-presence` (personhood-signal fusion), and no
//! crate in this workspace reads `mini/post-ai-provenance` objects for any
//! of those three. That is the same structural argument `mini-social::wall`
//! already makes for `WALL` objects, extended here: an
//! [`AuthorshipProvenance`] label is, and can only ever be, informational
//! content metadata. Publishing one requires exactly the ordinary
//! [`did_mini::Capabilities::POST`] capability (`mini-objects::object::
//! required_capability` maps every `ObjectType::Custom` to `POST`, never
//! `SIGN`/`VOTE`) — the same capability any other post requires, never
//! more. This mirrors, for the social/content case, the "purely
//! informational: never counted toward quorum" pattern `mini-forge::
//! governance::declare_ai_assistance`/`ai_assistance` already established
//! for the code-governance case (D-0067).

use did_mini::{Controller, Did};
use mini_objects::{Link, Object, ObjectBuilder, ObjectId, ObjectType, Payload};
use mini_store::{Backend, Store};

use crate::post::MEDIA_LINK_REL;
use crate::{get_str, put_str, Result, SocialError, MAX_POST_BYTES};

/// The `mini/post-ai-provenance` custom object type: a post whose signed
/// payload always begins with an [`AuthorshipProvenance`] claim. Distinct
/// from [`mini_objects::ObjectType::POST`] on purpose — reusing `POST`'s
/// existing wire shape would make an AI-provenance claim just one more
/// optional prefix a producer could choose to omit; a new type makes
/// "this object carries a provenance claim" a decode-time structural fact,
/// not a convention.
pub const AI_LABELED_POST_TYPE: &str = "mini/post-ai-provenance";

/// Maximum UTF-8 bytes in a declared model identifier (e.g.
/// `"claude-sonnet-5"`, `"local/llama-3-8b-instruct"`). Free text — this
/// workspace mints no model registry — but bounded like every other
/// untrusted string field.
pub const MAX_MODEL_BYTES: usize = 256;

/// A structural authorship-provenance claim. Required (never `Option`) by
/// every constructor in this module — a caller publishing through
/// [`build_labeled_post`]/[`publish_labeled_post`] must pick one variant;
/// there is no "unset" state for an object of [`AI_LABELED_POST_TYPE`].
///
/// New variants are additive-only (`#[non_exhaustive]`): loosening what
/// counts as a valid claim is a decision for a future entry, not a default.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum AuthorshipProvenance {
    /// The author affirmatively states the content is human-authored,
    /// published through this same unstrippable pathway — so an honest
    /// "not AI" claim is exactly as easy to make, and exactly as
    /// challengeable if it later proves false, as an honest AI claim. This
    /// is a claim, not a proof: see the module-level limitation above.
    Human,
    /// Wholly produced by the named model. `assisted_by` optionally names
    /// the human or agent `did:mini` that requested or orchestrated the
    /// generation (e.g. a human prompting an assistant, or one agent
    /// invoking another) — informational attribution, not an
    /// accountability requirement the way [`Self::AiAssisted`]'s editor is.
    AiGenerated {
        /// Free-text model identifier, bounded by [`MAX_MODEL_BYTES`].
        model: String,
        /// The human or agent that requested/orchestrated the generation,
        /// if the author chooses to name one.
        assisted_by: Option<Did>,
    },
    /// AI-drafted or AI-substantially-produced content that a named human
    /// edited or approved before publication. `human_editor` is mandatory
    /// — mirrors `mini-forge::governance::declare_ai_assistance`'s own
    /// rule that an AI-assisted claim always names one accountable human,
    /// never an ambiguous "assisted by no one in particular."
    AiAssisted {
        /// Free-text model identifier, bounded by [`MAX_MODEL_BYTES`].
        model: String,
        /// The accountable human editor's `did:mini`.
        human_editor: Did,
    },
}

fn encode_provenance(p: &AuthorshipProvenance, out: &mut Vec<u8>) -> Result<()> {
    match p {
        AuthorshipProvenance::Human => out.push(0),
        AuthorshipProvenance::AiGenerated { model, assisted_by } => {
            if model.is_empty() || model.len() > MAX_MODEL_BYTES {
                return Err(SocialError::FieldTooLarge);
            }
            out.push(1);
            put_str(out, model);
            match assisted_by {
                Some(did) => {
                    out.push(1);
                    put_str(out, did.as_str());
                }
                None => out.push(0),
            }
        }
        AuthorshipProvenance::AiAssisted {
            model,
            human_editor,
        } => {
            if model.is_empty() || model.len() > MAX_MODEL_BYTES {
                return Err(SocialError::FieldTooLarge);
            }
            out.push(2);
            put_str(out, model);
            put_str(out, human_editor.as_str());
        }
    }
    Ok(())
}

/// Decode one [`AuthorshipProvenance`] claim from `b` starting at `*pos`,
/// advancing `*pos` past it. `None` on any structurally invalid encoding —
/// callers must not guess a default provenance for malformed bytes.
fn decode_provenance(b: &[u8], pos: &mut usize) -> Option<AuthorshipProvenance> {
    let tag = *b.get(*pos)?;
    *pos += 1;
    match tag {
        0 => Some(AuthorshipProvenance::Human),
        1 => {
            let model = get_str(b, pos)?;
            if model.is_empty() || model.len() > MAX_MODEL_BYTES {
                return None;
            }
            let has_assisted_by = *b.get(*pos)?;
            *pos += 1;
            let assisted_by = match has_assisted_by {
                0 => None,
                1 => Some(Did::parse(&get_str(b, pos)?).ok()?),
                _ => return None,
            };
            Some(AuthorshipProvenance::AiGenerated { model, assisted_by })
        }
        2 => {
            let model = get_str(b, pos)?;
            if model.is_empty() || model.len() > MAX_MODEL_BYTES {
                return None;
            }
            let human_editor = Did::parse(&get_str(b, pos)?).ok()?;
            Some(AuthorshipProvenance::AiAssisted {
                model,
                human_editor,
            })
        }
        _ => None,
    }
}

/// A resolved, structurally validated AI-provenance-labeled post.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabeledPost {
    /// The post's content id.
    pub id: ObjectId,
    /// The author (the human-root the signing device is delegated from —
    /// this is a claim the same way any other post's author is; layer-3
    /// `mini_objects::object::verify_provenance` is what actually proves
    /// the signing device was really delegated by this DID).
    pub author: Did,
    /// The structural authorship-provenance claim.
    pub provenance: AuthorshipProvenance,
    /// Post text (caption, if `media` is set).
    pub text: String,
    /// Author-claimed creation time.
    pub timestamp_ms: u64,
    /// An optional linked media manifest, exactly like
    /// [`crate::post::PostKind::Media`].
    pub media: Option<ObjectId>,
}

/// Build (sign) an AI-provenance-labeled post without inserting it anywhere
/// — the signing half of [`publish_labeled_post`], split out for the same
/// crash-recovery reason [`crate::post::build_post`] is split out.
#[allow(clippy::too_many_arguments)]
pub fn build_labeled_post(
    human: &Did,
    device: &Controller,
    provenance: AuthorshipProvenance,
    text: &str,
    media: Option<ObjectId>,
    timestamp_ms: u64,
    sequence: u64,
) -> Result<Object> {
    if text.len() > MAX_POST_BYTES {
        return Err(SocialError::FieldTooLarge);
    }
    let mut payload = Vec::new();
    encode_provenance(&provenance, &mut payload)?;
    put_str(&mut payload, text);
    let mut builder = ObjectBuilder::new(ObjectType::Custom(AI_LABELED_POST_TYPE.to_string()))
        .timestamp_ms(timestamp_ms)
        .sequence(sequence)
        .payload(Payload::Public(payload));
    if let Some(target) = &media {
        builder = builder.link(MEDIA_LINK_REL, target.clone());
    }
    Ok(builder.sign(human, device)?)
}

/// Publish an AI-provenance-labeled post: sign it and insert it into
/// `store`. `provenance` is required, never optional — see the module docs.
#[allow(clippy::too_many_arguments)]
pub fn publish_labeled_post<B: Backend>(
    store: &mut Store<B>,
    human: &Did,
    device: &Controller,
    provenance: AuthorshipProvenance,
    text: &str,
    media: Option<ObjectId>,
    timestamp_ms: u64,
    sequence: u64,
) -> Result<Object> {
    let post = build_labeled_post(human, device, provenance, text, media, timestamp_ms, sequence)?;
    store.insert(&post)?;
    Ok(post)
}

/// Decode and structurally validate an already-fetched `mini/post-ai-
/// provenance` object — pure, no store access. Rejects: wrong object type,
/// encrypted payload, oversized payload, a malformed or missing
/// [`AuthorshipProvenance`] claim, non-UTF-8 text, trailing bytes, and any
/// link shape other than zero links or exactly one `"media"` link.
pub fn decode_labeled_post(object: &Object) -> Result<LabeledPost> {
    if object.object_type != ObjectType::Custom(AI_LABELED_POST_TYPE.to_string()) {
        return Err(SocialError::BadAiProvenance);
    }
    let Payload::Public(bytes) = &object.payload else {
        return Err(SocialError::BadAiProvenance);
    };
    let mut pos = 0usize;
    let provenance = decode_provenance(bytes, &mut pos).ok_or(SocialError::BadAiProvenance)?;
    let text = get_str(bytes, &mut pos).ok_or(SocialError::BadAiProvenance)?;
    if pos != bytes.len() {
        return Err(SocialError::BadAiProvenance); // strict: no trailing bytes
    }
    if text.len() > MAX_POST_BYTES {
        return Err(SocialError::BadAiProvenance);
    }

    let media = match object.links.as_slice() {
        [] => None,
        [Link { rel, target }] if rel == MEDIA_LINK_REL => Some(target.clone()),
        _ => return Err(SocialError::BadAiProvenance),
    };

    Ok(LabeledPost {
        id: object.id().clone(),
        author: object.author_human.clone(),
        provenance,
        text,
        timestamp_ms: object.timestamp_ms,
        media,
    })
}

/// Fetch and [`decode_labeled_post`] a stored object.
pub fn resolve_labeled_post<B: Backend>(store: &Store<B>, id: &ObjectId) -> Result<LabeledPost> {
    let object = store.get(id)?;
    decode_labeled_post(&object)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mini_store::MemoryBackend;

    fn human_and_device() -> (Did, Controller) {
        let device = Controller::incept_single().unwrap();
        (device.did(), device)
    }

    #[test]
    fn ai_generated_round_trips_with_and_without_assisted_by() {
        let (human, device) = human_and_device();
        let (assister, _) = human_and_device();

        let post = build_labeled_post(
            &human,
            &device,
            AuthorshipProvenance::AiGenerated {
                model: "claude-sonnet-5".to_string(),
                assisted_by: Some(assister.clone()),
            },
            "hello from a model",
            None,
            1_000,
            1,
        )
        .unwrap();
        let decoded = decode_labeled_post(&post).unwrap();
        assert_eq!(
            decoded.provenance,
            AuthorshipProvenance::AiGenerated {
                model: "claude-sonnet-5".to_string(),
                assisted_by: Some(assister),
            }
        );
        assert_eq!(decoded.text, "hello from a model");
        assert_eq!(decoded.media, None);

        let post2 = build_labeled_post(
            &human,
            &device,
            AuthorshipProvenance::AiGenerated {
                model: "local/llama-3".to_string(),
                assisted_by: None,
            },
            "no named assister",
            None,
            2_000,
            2,
        )
        .unwrap();
        let decoded2 = decode_labeled_post(&post2).unwrap();
        assert_eq!(
            decoded2.provenance,
            AuthorshipProvenance::AiGenerated {
                model: "local/llama-3".to_string(),
                assisted_by: None,
            }
        );
    }

    #[test]
    fn ai_assisted_requires_and_round_trips_a_human_editor() {
        let (human, device) = human_and_device();
        let (editor, _) = human_and_device();

        let post = build_labeled_post(
            &human,
            &device,
            AuthorshipProvenance::AiAssisted {
                model: "claude-sonnet-5".to_string(),
                human_editor: editor.clone(),
            },
            "drafted by AI, edited by a human",
            None,
            3_000,
            1,
        )
        .unwrap();
        let decoded = decode_labeled_post(&post).unwrap();
        assert_eq!(
            decoded.provenance,
            AuthorshipProvenance::AiAssisted {
                model: "claude-sonnet-5".to_string(),
                human_editor: editor,
            }
        );
        // AiAssisted::human_editor is a mandatory `Did` field, not
        // `Option<Did>` — the type system, not a runtime check, is what
        // makes "AI-assisted with no accountable human" unconstructible.
    }

    #[test]
    fn human_claim_round_trips() {
        let (human, device) = human_and_device();
        let post = build_labeled_post(
            &human,
            &device,
            AuthorshipProvenance::Human,
            "I wrote every word of this myself",
            None,
            4_000,
            1,
        )
        .unwrap();
        let decoded = decode_labeled_post(&post).unwrap();
        assert_eq!(decoded.provenance, AuthorshipProvenance::Human);
    }

    #[test]
    fn media_link_round_trips_like_an_ordinary_media_post() {
        let (human, device) = human_and_device();
        let media_id = ObjectId::of_for_test();
        let post = build_labeled_post(
            &human,
            &device,
            AuthorshipProvenance::AiGenerated {
                model: "diffusion-x".to_string(),
                assisted_by: None,
            },
            "a generated image",
            Some(media_id.clone()),
            5_000,
            1,
        )
        .unwrap();
        let decoded = decode_labeled_post(&post).unwrap();
        assert_eq!(decoded.media, Some(media_id));
    }

    #[test]
    fn empty_model_name_is_rejected() {
        let (human, device) = human_and_device();
        let err = build_labeled_post(
            &human,
            &device,
            AuthorshipProvenance::AiGenerated {
                model: String::new(),
                assisted_by: None,
            },
            "text",
            None,
            1,
            1,
        )
        .unwrap_err();
        assert_eq!(err, SocialError::FieldTooLarge);
    }

    #[test]
    fn oversized_model_name_is_rejected() {
        let (human, device) = human_and_device();
        let err = build_labeled_post(
            &human,
            &device,
            AuthorshipProvenance::AiAssisted {
                model: "x".repeat(MAX_MODEL_BYTES + 1),
                human_editor: human.clone(),
            },
            "text",
            None,
            1,
            1,
        )
        .unwrap_err();
        assert_eq!(err, SocialError::FieldTooLarge);
    }

    #[test]
    fn decoding_an_ordinary_post_as_a_labeled_post_is_rejected() {
        // A plain `POST` object never carries a provenance claim -- the
        // wrong-type rejection is the structural boundary between the two
        // shapes, not a payload convention either side could ignore.
        let (human, device) = human_and_device();
        let ordinary = crate::post::build_post(&human, &device, "just a post", 1, 1).unwrap();
        assert_eq!(
            decode_labeled_post(&ordinary).unwrap_err(),
            SocialError::BadAiProvenance
        );
    }

    #[test]
    fn tampering_the_provenance_byte_breaks_the_signature() {
        // The provenance claim is part of the signed payload, not a
        // separate, omittable field: mutate the encoded provenance tag
        // (byte 0, AiGenerated -> Human) after signing, leaving the
        // original device signature untouched, and prove verification now
        // fails. Stripping or forging a claim this way is detectable, not
        // silent.
        let (human, device) = human_and_device();
        let signed = build_labeled_post(
            &human,
            &device,
            AuthorshipProvenance::AiGenerated {
                model: "claude-sonnet-5".to_string(),
                assisted_by: None,
            },
            "generated text",
            None,
            1_000,
            1,
        )
        .unwrap();

        let Payload::Public(original_bytes) = &signed.payload else {
            panic!("labeled posts are always public payloads");
        };
        let mut tampered_bytes = original_bytes.clone();
        tampered_bytes[0] = 0; // AiGenerated (1) -> Human (0)
        assert_ne!(&tampered_bytes, original_bytes);

        let mut tampered = signed.clone();
        tampered.payload = Payload::Public(tampered_bytes);

        // The tampered bytes still decode structurally (as a `Human`
        // claim) -- decoding alone cannot catch this; signature
        // verification is what must, and does.
        let decoded = decode_labeled_post(&tampered).unwrap();
        assert_eq!(decoded.provenance, AuthorshipProvenance::Human);

        assert!(tampered.verify_signature(&device.kel()).is_err());
        // The untampered original still verifies against the same device.
        assert!(signed.verify_signature(&device.kel()).is_ok());
    }

    #[test]
    fn requires_only_the_ordinary_post_capability() {
        // Same claim the module docs make, checked mechanically: a
        // `mini/post-ai-provenance` object's required capability is
        // `POST`, exactly like any other content object, never `SIGN` or
        // any governance-adjacent capability -- so a device that can post
        // can label a post's provenance, and nothing more.
        use did_mini::Capabilities;
        assert_eq!(
            mini_objects::required_capability_for_test(&ObjectType::Custom(
                AI_LABELED_POST_TYPE.to_string()
            )),
            Capabilities::POST
        );
    }
}
