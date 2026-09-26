//! Materialize timeline cards off the renderer thread. This removes repeated
//! disk scans from repaint; it does not claim the underlying feed is indexed.

use did_mini::Did;
use mini_objects::{AiObject, ObjectId, ObjectType, Payload};
use mini_social::{
    comments, feed, following, reaction_counts, resolve_post, resolve_profile, FeedFilter,
    FeedReason, PostKind,
};
use mini_store::{Backend, FsBackend, Store};
use std::collections::HashMap;
use std::path::Path;

/// Posts per snapshot. Older history stays on disk and is not walked.
pub const PAGE: usize = 50;

/// Whose posts a timeline shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// Your posts and the people you follow — `mini_social::feed`.
    Following,
    /// Every verified post this device has received, regardless of follow
    /// state. Useful on a fresh device before any follow edge exists.
    Everyone,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Card {
    pub id: ObjectId,
    pub author: String,
    pub did: String,
    pub body: String,
    pub timestamp_ms: u64,
    pub reason: &'static str,
    pub support_count: usize,
    pub comment_count: usize,
    pub media: Option<ObjectId>,
    pub own: bool,
    /// The author's profile photo manifest, when their signed profile has one.
    pub avatar: Option<ObjectId>,
    /// Set only when this card renders an [`mini_objects::AiObject`]
    /// (Directive 12 / constitution principle 8: AI participation is never
    /// laundered into looking like human authorship). `None` for every
    /// ordinary human-authored [`mini_objects::Object`]-backed card built by
    /// [`build`]/[`from_service`] — those two paths never touch an
    /// `AiObject` and so can never set this field, by construction. The
    /// renderer should show this string distinctly (never as, or beside,
    /// the human `author`/`did` fields) wherever it is `Some`.
    pub ai_disclosure: Option<String>,
}

/// Convert the bounded application-core feed view into renderer cards.
///
/// The renderer does not reopen the store for this path; object identifiers
/// are parsed and bounded data is copied from the service contract.
pub fn from_service(cards: Vec<mini_app_protocol::FeedCard>) -> Result<Vec<Card>, String> {
    cards
        .into_iter()
        .map(|card| {
            Ok(Card {
                id: ObjectId::parse(&card.id).map_err(|error| error.to_string())?,
                author: card.author,
                did: card.did,
                body: card.body,
                timestamp_ms: card.timestamp_ms,
                reason: match card.reason {
                    mini_app_protocol::FeedReason::Own => "Your post",
                    mini_app_protocol::FeedReason::Followed => "You follow this author",
                    mini_app_protocol::FeedReason::Received => "Received from a peer",
                },
                support_count: card.support_count as usize,
                comment_count: card.comment_count as usize,
                media: card
                    .media
                    .map(|id| ObjectId::parse(&id).map_err(|error| error.to_string()))
                    .transpose()?,
                own: card.own,
                avatar: card
                    .avatar
                    .map(|id| ObjectId::parse(&id).map_err(|error| error.to_string()))
                    .transpose()?,
                ai_disclosure: None,
            })
        })
        .collect()
}

/// Build a timeline card for an [`AiObject`] — the disclosure-rendering
/// consumer required by Directive 12 / constitution principle 8: an
/// AI-generated/AI-mediated object that a client displays in the same list
/// as human posts must render distinctly labeled, never as if a human wrote
/// it. There is no `From<AiObject> for Card`/`TryFrom` here on purpose: a
/// caller must go through this function (or [`build`]/[`from_service`],
/// neither of which can ever produce one from an `AiObject`) rather than
/// having a generic conversion quietly drop the disclosure.
///
/// `author` is the operator's display name (never claimed as the content's
/// author — see [`AiObject::operator_human`]'s doc comment on that
/// distinction); the card body is the object's public payload text when
/// present, or a plain placeholder for encrypted/non-UTF-8 payloads so this
/// never panics or silently substitutes human-looking text.
///
/// `mini-desktop` is a binary-only crate (no `lib.rs`), so `pub` here does
/// not create an external consumer the way it would in a library crate:
/// rustc's `dead_code` lint judges reachability from `main`, not from other
/// crates. D-0537 documents, as its own stated "Failure point," that
/// nothing in the running UI loop calls this yet — `mini-store`/
/// `mini-social` have no `AiObject` persistence/indexing to source one
/// from, and retrofitting that here would be exactly the scope creep the
/// same decision explicitly declined. Until that follow-up lands and gives
/// this a real caller, `#[allow(dead_code)]` keeps that honestly-documented
/// gap from failing `-D warnings` builds; it is exercised today only by
/// `ai_card_carries_a_disclosure_that_ordinary_cards_never_get` below.
#[allow(dead_code)]
pub fn ai_card(ai: &AiObject, author: String, did: String) -> Card {
    let body = match &ai.payload {
        Payload::Public(bytes) => {
            String::from_utf8(bytes.clone()).unwrap_or_else(|_| "[unreadable AI payload]".into())
        }
        Payload::Encrypted(_) => "[encrypted AI content]".into(),
    };
    Card {
        id: ai.id().clone(),
        author,
        did,
        body,
        timestamp_ms: ai.provenance.produced_at_ms,
        reason: "AI system output",
        support_count: 0,
        comment_count: 0,
        media: None,
        own: false,
        avatar: None,
        ai_disclosure: Some(ai.render_disclosure()),
    }
}

pub fn snapshot(
    root: &Path,
    human: &Did,
    filter: FeedFilter,
    scope: Scope,
) -> Result<Vec<Card>, String> {
    let store = Store::new(FsBackend::open(root).map_err(|error| error.to_string())?);
    build(&store, human, filter, scope)
}

type Item = (ObjectId, Did, u64, &'static str, usize);

fn build<B: Backend>(
    store: &Store<B>,
    human: &Did,
    filter: FeedFilter,
    scope: Scope,
) -> Result<Vec<Card>, String> {
    let items: Vec<Item> = match scope {
        Scope::Following => feed(store, human, filter, PAGE)
            .map_err(|error| error.to_string())?
            .into_iter()
            .map(|item| {
                (
                    item.id,
                    item.author,
                    item.timestamp_ms,
                    match item.reason {
                        FeedReason::Own => "Your post",
                        FeedReason::Followed => "You follow this author",
                    },
                    item.support_count,
                )
            })
            .collect(),
        Scope::Everyone => {
            let followed = following(store, human).map_err(|error| error.to_string())?;
            let mut items: Vec<Item> = Vec::new();
            for id in store
                .by_type(&ObjectType::POST)
                .map_err(|error| error.to_string())?
            {
                // Same canonical validator the feed uses; an object that
                // fails it is skipped, never shown as a broken card.
                let Ok(post) = resolve_post(store, &id) else {
                    continue;
                };
                let support = reaction_counts(store, &id)
                    .map_err(|error| error.to_string())?
                    .into_iter()
                    .map(|(_, count)| count)
                    .sum();
                let reason = if &post.author == human {
                    "Your post"
                } else if followed.contains(&post.author) {
                    "You follow this author"
                } else {
                    "Received from a peer"
                };
                items.push((id, post.author, post.timestamp_ms, reason, support));
            }
            match filter {
                FeedFilter::MostSupported => items.sort_by(|a, b| {
                    b.4.cmp(&a.4)
                        .then_with(|| b.2.cmp(&a.2))
                        .then_with(|| b.0.as_str().cmp(a.0.as_str()))
                }),
                _ => {
                    items.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| b.0.as_str().cmp(a.0.as_str())))
                }
            }
            items.truncate(PAGE);
            items
        }
    };
    let mut names: HashMap<String, (String, Option<ObjectId>)> = HashMap::new();
    items
        .into_iter()
        .map(|(id, author, timestamp_ms, reason, support_count)| {
            let did = author.as_str().to_owned();
            let (name, avatar) = match names.get(&did) {
                Some(entry) => entry.clone(),
                None => {
                    let entry = resolve_profile(store, &author)
                        .map_err(|error| error.to_string())?
                        .map(|profile| (profile.display_name, profile.avatar))
                        .unwrap_or_else(|| ("Mininet participant".into(), None));
                    names.insert(did.clone(), entry.clone());
                    entry
                }
            };
            let post = resolve_post(store, &id).map_err(|error| error.to_string())?;
            Ok(Card {
                own: &author == human,
                id: id.clone(),
                author: name,
                did,
                body: post.text,
                timestamp_ms,
                reason,
                support_count,
                comment_count: comments(store, &id)
                    .map_err(|error| error.to_string())?
                    .len(),
                media: match post.kind {
                    PostKind::Media { media } => Some(media),
                    _ => None,
                },
                avatar,
                ai_disclosure: None,
            })
        })
        .collect()
}

pub fn age(timestamp_ms: u64, now_ms: u64) -> String {
    if timestamp_ms > now_ms {
        return "future author time".into();
    }
    let seconds = (now_ms - timestamp_ms) / 1000;
    match seconds {
        0..60 => "now".into(),
        60..3600 => format!("{}m", seconds / 60),
        3600..86400 => format!("{}h", seconds / 3600),
        _ => format!("{}d", seconds / 86400),
    }
}

#[cfg(test)]
mod tests {
    use super::{age, ai_card, build, Scope};
    use did_mini::{Capabilities, Controller};
    use mini_objects::{AiObjectBuilder, AiOrigin, AiProvenance, Payload};
    use mini_social::{publish_post, publish_profile, set_follow, FeedFilter};
    use mini_store::{MemoryBackend, Store};

    fn person(root_seed: u8) -> (Controller, Controller) {
        let mut root =
            Controller::incept_single_from_seeds(&[root_seed; 32], &[root_seed + 1; 32]).unwrap();
        let device = Controller::incept_device_single_from_seeds(
            &root.did(),
            &[root_seed + 2; 32],
            &[root_seed + 3; 32],
        )
        .unwrap();
        root.delegate_device(&device.did(), Capabilities::primary())
            .unwrap();
        (root, device)
    }

    #[test]
    fn author_time_is_real_and_future_time_is_not_underflowed() {
        assert_eq!(age(0, 120_000), "2m");
        assert_eq!(age(0, 7_200_000), "2h");
        assert_eq!(age(0, 172_800_000), "2d");
        assert_eq!(age(u64::MAX, 0), "future author time");
    }

    #[test]
    fn everyone_scope_shows_received_posts_before_any_follow_exists() {
        let mut store = Store::new(MemoryBackend::new());
        let (me, me_device) = person(10);
        let (other, other_device) = person(20);
        publish_profile(
            &mut store,
            &other.did(),
            &other_device,
            "Other",
            "",
            None,
            1_000,
            1,
        )
        .unwrap();
        publish_post(
            &mut store,
            &other.did(),
            &other_device,
            "hello from other",
            2_000,
            2,
        )
        .unwrap();
        publish_post(&mut store, &me.did(), &me_device, "hello from me", 3_000, 1).unwrap();

        let following = build(
            &store,
            &me.did(),
            FeedFilter::Chronological,
            Scope::Following,
        )
        .unwrap();
        assert_eq!(following.len(), 1);
        assert!(following[0].own);

        let everyone = build(
            &store,
            &me.did(),
            FeedFilter::Chronological,
            Scope::Everyone,
        )
        .unwrap();
        assert_eq!(everyone.len(), 2);
        assert_eq!(everyone[0].body, "hello from me");
        assert_eq!(everyone[1].author, "Other");
        assert_eq!(everyone[1].reason, "Received from a peer");
        assert!(!everyone[1].own);
        assert!(everyone[1].media.is_none());

        set_follow(
            &mut store,
            &me.did(),
            &me_device,
            &other.did(),
            true,
            4_000,
            2,
        )
        .unwrap();
        let following = build(
            &store,
            &me.did(),
            FeedFilter::Chronological,
            Scope::Following,
        )
        .unwrap();
        assert_eq!(following.len(), 2);
        assert_eq!(following[1].reason, "You follow this author");
    }

    #[test]
    fn ai_card_carries_a_disclosure_that_ordinary_cards_never_get() {
        let mut store = Store::new(MemoryBackend::new());
        let (me, me_device) = person(30);
        publish_post(&mut store, &me.did(), &me_device, "a human post", 1_000, 1).unwrap();
        let human_cards = build(
            &store,
            &me.did(),
            FeedFilter::Chronological,
            Scope::Everyone,
        )
        .unwrap();
        assert_eq!(human_cards.len(), 1);
        assert!(
            human_cards[0].ai_disclosure.is_none(),
            "a human-authored Object must never carry an AI disclosure"
        );

        let (operator, operator_device) = person(40);
        let ai_obj = AiObjectBuilder::new(
            AiOrigin::GeneratedContent,
            AiProvenance {
                system_id: "mini-forge-review-assistant/0.1.0".to_string(),
                model_id: "test:fixture-model".to_string(),
                produced_at_ms: 5_000,
            },
        )
        .payload(Payload::Public(b"a generated summary".to_vec()))
        .sign(&operator.did(), &operator_device)
        .unwrap();

        let card = ai_card(
            &ai_obj,
            "Operator Org".to_string(),
            operator.did().to_string(),
        );
        let disclosure = card
            .ai_disclosure
            .as_ref()
            .expect("an AiObject-rendered card must always carry a disclosure");
        assert!(!disclosure.is_empty());
        assert!(disclosure.contains("AI-generated content"));
        assert_eq!(card.body, "a generated summary");
        assert!(!card.own);
    }
}
