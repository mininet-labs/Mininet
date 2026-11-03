//! A fee on every spammable action that rises fast with how much one author
//! does in an hour.
//!
//! Ordinary use never meets it: each kind of action has a free allowance per
//! trailing hour (a few posts or comments, a few dozen likes). Past that the
//! fee starts at a small base for that kind and **doubles with every further
//! action** of the same kind in the same window, so a flood becomes expensive
//! within a few dozen actions. The fee is owed by the action's author and is
//! computed from the author's own signed objects, so any device holding them
//! computes the same amount.
//!
//! Honest limits:
//! - Unsettled credit like everything in this crate: nothing is charged or
//!   paid; settlement is gated behind the external audit (D-0047).
//! - An object's time is declared by its author, so a custom client can
//!   backdate to spread actions across windows. The fee only binds where a
//!   counterparty (a host, a settlement layer) applies the same function to
//!   objects it accepted on arrival, which is future work.
//! - It is per identity root. Someone who mints many roots gets the free
//!   allowance once per root; this raises the cost of flooding from one root,
//!   it does not stop a Sybil flood (the personhood layer's job).
//! - Allowances and bases below are starting points, not measured values.
//! - A reaction counts whether it adds or withdraws, so toggling a like back
//!   and forth is spam like any other.

use std::collections::BTreeMap;

use did_mini::Did;
use mini_objects::{ObjectId, ObjectType};
use mini_store::{Backend, Store};

use crate::{Result, ServiceTicket, TicketError};

/// Trailing window, in milliseconds, over which an author's actions count.
pub const FEE_WINDOW_MS: u64 = 60 * 60 * 1000;
/// The fee stops doubling after this many doublings, so it never overflows and
/// a runaway account meets a fixed, large cost.
pub const MAX_DOUBLINGS: u32 = 20;
/// Community-membership objects use this custom type (same string as
/// `mini-social`; this crate does not depend on it).
const MEMBERSHIP_TYPE: &str = "mini/community-membership";

/// A kind of action that anyone can repeat cheaply.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ActionKind {
    Post,
    Comment,
    /// A like, downvote or the withdrawal of either.
    Reaction,
    Follow,
    /// Joining or leaving a community.
    Membership,
    /// Creating a community.
    Community,
    /// Publishing a media file.
    Media,
}

/// Every kind that carries a fee.
pub const ALL_KINDS: [ActionKind; 7] = [
    ActionKind::Post,
    ActionKind::Comment,
    ActionKind::Reaction,
    ActionKind::Follow,
    ActionKind::Membership,
    ActionKind::Community,
    ActionKind::Media,
];

impl ActionKind {
    /// Free actions per window, then the fee in micro-MINI for the first one
    /// past that.
    pub fn schedule(self) -> (u32, u64) {
        match self {
            ActionKind::Post => (5, 10),
            ActionKind::Comment => (3, 10),
            ActionKind::Reaction => (30, 1),
            ActionKind::Follow => (20, 5),
            ActionKind::Membership => (10, 10),
            ActionKind::Community => (1, 100),
            ActionKind::Media => (3, 50),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ActionKind::Post => "post",
            ActionKind::Comment => "comment",
            ActionKind::Reaction => "like or vote",
            ActionKind::Follow => "follow",
            ActionKind::Membership => "community join or leave",
            ActionKind::Community => "community creation",
            ActionKind::Media => "media upload",
        }
    }

    fn object_type(self) -> ObjectType {
        match self {
            ActionKind::Post => ObjectType::POST,
            ActionKind::Comment => ObjectType::COMMENT,
            ActionKind::Reaction => ObjectType::REACTION,
            ActionKind::Follow => ObjectType::FOLLOW,
            ActionKind::Membership => ObjectType::Custom(MEMBERSHIP_TYPE.into()),
            ActionKind::Community => ObjectType::COMMUNITY,
            ActionKind::Media => ObjectType::MEDIA_MANIFEST,
        }
    }
}

/// Fee for an action of `kind` when the author already has `prior_in_window`
/// earlier actions of that kind in the trailing window: 0 within the
/// allowance, then the kind's base, doubling with each further action.
pub fn action_fee(kind: ActionKind, prior_in_window: u32) -> u64 {
    let (free, base) = kind.schedule();
    if prior_in_window < free {
        return 0;
    }
    let doublings = (prior_in_window - free).min(MAX_DOUBLINGS);
    base << doublings
}

/// One action and the fee it carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionFeeLine {
    pub kind: ActionKind,
    pub object: ObjectId,
    pub timestamp_ms: u64,
    pub prior_in_window: u32,
    pub fee_micro: u64,
}

/// Everything an author owes in action fees according to the objects on this
/// device.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ActionFees {
    /// Actions that carry a fee (the rest are within the allowance).
    pub lines: Vec<ActionFeeLine>,
    pub total_micro: u64,
    /// All fee-eligible actions by the author seen on this device.
    pub actions: usize,
}

impl ActionFees {
    /// What this author owes for one kind.
    pub fn total_for(&self, kind: ActionKind) -> u64 {
        self.lines
            .iter()
            .filter(|line| line.kind == kind)
            .map(|line| line.fee_micro)
            .sum()
    }
}

/// Fee lines for one author's actions of one kind.
fn fees_for(kind: ActionKind, mut mine: Vec<(u64, ObjectId)>) -> Result<ActionFees> {
    mine.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.as_str().cmp(b.1.as_str())));
    let mut fees = ActionFees {
        actions: mine.len(),
        ..ActionFees::default()
    };
    let mut start = 0usize;
    for (index, (timestamp_ms, id)) in mine.iter().enumerate() {
        while mine[start].0.saturating_add(FEE_WINDOW_MS) <= *timestamp_ms {
            start += 1;
        }
        let prior = u32::try_from(index - start).unwrap_or(u32::MAX);
        let fee_micro = action_fee(kind, prior);
        if fee_micro > 0 {
            fees.total_micro = fees
                .total_micro
                .checked_add(fee_micro)
                .ok_or(TicketError::Overflow)?;
            fees.lines.push(ActionFeeLine {
                kind,
                object: id.clone(),
                timestamp_ms: *timestamp_ms,
                prior_in_window: prior,
                fee_micro,
            });
        }
    }
    Ok(fees)
}

/// Every author's objects of one kind on this device, as `(timestamp, id)`.
fn by_author<B: Backend>(
    store: &Store<B>,
    kind: ActionKind,
) -> Result<BTreeMap<String, Vec<(u64, ObjectId)>>> {
    let ids = store
        .by_type(&kind.object_type())
        .map_err(|error| TicketError::Store(error.to_string()))?;
    let mut out: BTreeMap<String, Vec<(u64, ObjectId)>> = BTreeMap::new();
    for id in ids {
        let object = store
            .get(&id)
            .map_err(|error| TicketError::Store(error.to_string()))?;
        out.entry(object.author_human.as_str().to_owned())
            .or_default()
            .push((object.timestamp_ms, id));
    }
    Ok(out)
}

/// Action fees owed by `author` across every kind.
pub fn action_fees<B: Backend>(store: &Store<B>, author: &Did) -> Result<ActionFees> {
    let mut total = ActionFees::default();
    for kind in ALL_KINDS {
        let mut all = by_author(store, kind)?;
        let fees = fees_for(kind, all.remove(author.as_str()).unwrap_or_default())?;
        total.actions += fees.actions;
        total.total_micro = total
            .total_micro
            .checked_add(fees.total_micro)
            .ok_or(TicketError::Overflow)?;
        total.lines.extend(fees.lines);
    }
    Ok(total)
}

/// What the author's next action of `kind`, made at `now_ms`, would cost. For
/// showing the price before the author acts.
pub fn next_action_fee<B: Backend>(
    store: &Store<B>,
    author: &Did,
    kind: ActionKind,
    now_ms: u64,
) -> Result<u64> {
    let mut all = by_author(store, kind)?;
    let prior = all
        .remove(author.as_str())
        .unwrap_or_default()
        .into_iter()
        .filter(|(at, _)| *at <= now_ms && at.saturating_add(FEE_WINDOW_MS) > now_ms)
        .count();
    Ok(action_fee(kind, u32::try_from(prior).unwrap_or(u32::MAX)))
}

/// How every action fee on this device is shared among the hosts that kept
/// the object available: the fee on an object is divided between the distinct
/// providers whose (deduplicated) tickets list it as received (completed media
/// counts for its manifest), weighted by the bytes each attested; any
/// remainder goes to the first provider by DID so shares always add up to the
/// fee. An object no ticket lists earns nobody anything: its fee stays owed
/// but unclaimed.
pub fn host_shares<B: Backend>(
    store: &Store<B>,
    tickets: &[ServiceTicket],
) -> Result<BTreeMap<String, u64>> {
    // object id -> provider -> bytes attested
    let mut served: BTreeMap<&str, BTreeMap<&str, u64>> = BTreeMap::new();
    for ticket in tickets {
        let provider = ticket.fields.provider.as_str();
        let listed = ticket
            .fields
            .served
            .iter()
            .map(|object| (object.object.as_str(), object.bytes))
            .chain(
                ticket
                    .fields
                    .completed_media
                    .iter()
                    .map(|media| (media.manifest.as_str(), media.bytes)),
            );
        for (object, bytes) in listed {
            let weight = served
                .entry(object)
                .or_default()
                .entry(provider)
                .or_insert(0);
            *weight = weight.saturating_add(bytes.max(1));
        }
    }
    let mut shares: BTreeMap<String, u64> = BTreeMap::new();
    for kind in ALL_KINDS {
        for (_, mine) in by_author(store, kind)? {
            for line in fees_for(kind, mine)?.lines {
                let Some(hosts) = served.get(line.object.as_str()) else {
                    continue;
                };
                let total_weight: u128 = hosts.values().map(|w| u128::from(*w)).sum();
                let mut paid: u64 = 0;
                for (host, weight) in hosts {
                    let part =
                        (u128::from(line.fee_micro) * u128::from(*weight) / total_weight) as u64;
                    paid += part;
                    *shares.entry((*host).to_owned()).or_insert(0) += part;
                }
                if let Some((first, _)) = hosts.iter().next() {
                    *shares.entry((*first).to_owned()).or_insert(0) += line.fee_micro - paid;
                }
            }
        }
    }
    Ok(shares)
}

#[cfg(test)]
mod tests {
    use super::*;
    use did_mini::{Capabilities, Controller};
    use mini_objects::{ObjectBuilder, Payload};
    use mini_store::MemoryBackend;

    type Person = (Controller, Controller);

    fn person(seed: u8) -> Person {
        let mut root = Controller::incept_single_from_seeds(&[seed; 32], &[seed + 1; 32]).unwrap();
        let device = Controller::incept_device_single_from_seeds(
            &root.did(),
            &[seed + 2; 32],
            &[seed + 3; 32],
        )
        .unwrap();
        root.delegate_device(&device.did(), Capabilities::primary())
            .unwrap();
        (root, device)
    }

    fn act(
        store: &mut Store<MemoryBackend>,
        who: &Person,
        kind: ActionKind,
        at_ms: u64,
        sequence: u64,
    ) -> ObjectId {
        let object = ObjectBuilder::new(kind.object_type())
            .timestamp_ms(at_ms)
            .sequence(sequence)
            .payload(Payload::Public(format!("a{sequence}").into_bytes()))
            .sign(&who.0.did(), &who.1)
            .unwrap();
        store.insert(&object).unwrap();
        object.id().clone()
    }

    #[test]
    fn every_kind_is_free_then_small_then_doubles_and_is_capped() {
        for kind in ALL_KINDS {
            let (free, base) = kind.schedule();
            assert_eq!(action_fee(kind, 0), 0, "{kind:?}");
            assert_eq!(action_fee(kind, free - 1), 0, "{kind:?}");
            assert_eq!(action_fee(kind, free), base, "{kind:?}");
            assert_eq!(action_fee(kind, free + 3), 8 * base, "{kind:?}");
            assert_eq!(
                action_fee(kind, u32::MAX),
                base << MAX_DOUBLINGS,
                "{kind:?}"
            );
        }
        // Heavier actions cost more to repeat than lighter ones.
        assert!(action_fee(ActionKind::Community, 1) > action_fee(ActionKind::Reaction, 30));
    }

    #[test]
    fn normal_use_pays_nothing_and_a_burst_of_any_kind_gets_expensive() {
        let mut store = Store::new(MemoryBackend::new());
        let normal = person(10);
        let spammer = person(20);
        let hour = FEE_WINDOW_MS;
        // Normal: every kind at a rate inside its allowance, for a day.
        for kind in ALL_KINDS {
            let (free, _) = kind.schedule();
            for n in 0..(24 * u64::from(free)) {
                act(
                    &mut store,
                    &normal,
                    kind,
                    1_000 + n * (hour / u64::from(free)),
                    n + 1,
                );
            }
        }
        assert_eq!(action_fees(&store, &normal.0.did()).unwrap().total_micro, 0);

        // Spammer: 40 likes and 10 posts inside one minute.
        for n in 0..40u64 {
            act(
                &mut store,
                &spammer,
                ActionKind::Reaction,
                5_000 + n * 1_000,
                n + 1,
            );
        }
        for n in 0..10u64 {
            act(
                &mut store,
                &spammer,
                ActionKind::Post,
                5_000 + n * 1_000,
                n + 1,
            );
        }
        let fees = action_fees(&store, &spammer.0.did()).unwrap();
        // Likes: 30 free, then 1,2,4,...,512 = 1023. Posts: 5 free, then 10,20,40,80,160 = 310.
        assert_eq!(fees.total_for(ActionKind::Reaction), 1023);
        assert_eq!(fees.total_for(ActionKind::Post), 310);
        assert_eq!(fees.total_micro, 1333);
        assert_eq!(fees.lines.len(), 10 + 5);
        // The price of the next one is shown before acting, then resets.
        let next = next_action_fee(&store, &spammer.0.did(), ActionKind::Post, 20_000).unwrap();
        assert_eq!(next, 10 << 5);
        let later =
            next_action_fee(&store, &spammer.0.did(), ActionKind::Post, 5_000 + 2 * hour).unwrap();
        assert_eq!(later, 0);
        // Other kinds are priced on their own count.
        let follow = next_action_fee(&store, &spammer.0.did(), ActionKind::Follow, 20_000).unwrap();
        assert_eq!(follow, 0);
    }

    #[test]
    fn a_fee_is_shared_among_the_hosts_that_kept_the_object_available() {
        use crate::{ServedObject, Service, TicketFields};
        let mut store = Store::new(MemoryBackend::new());
        let author = person(10);
        let host_a = person(20);
        let host_b = person(30);
        let consumer = person(40);
        // Seven quick posts: the 6th costs 10, the 7th costs 20.
        let ids: Vec<ObjectId> = (0..7u64)
            .map(|n| act(&mut store, &author, ActionKind::Post, 1_000 + n, n + 1))
            .collect();
        let fees = action_fees(&store, &author.0.did()).unwrap();
        assert_eq!(fees.total_micro, 30);
        let (sixth, seventh) = (ids[5].clone(), ids[6].clone());
        let ticket = |provider: &Did, object: &ObjectId, bytes: u64, nonce: u8| ServiceTicket {
            id: object.clone(),
            consumer: consumer.0.did(),
            fields: TicketFields {
                provider: provider.clone(),
                service: Service::PublicSync,
                rate_micro_per_mb: 0,
                bytes_received: bytes,
                objects_received: 1,
                channel_binding: [0; 32],
                nonce: [nonce; 32],
                completed_media: Vec::new(),
                creator_bps: Some(500),
                served: vec![ServedObject {
                    object: object.clone(),
                    bytes,
                }],
            },
            timestamp_ms: 10,
        };
        let tickets = vec![
            ticket(&host_a.0.did(), &sixth, 200, 1),
            ticket(&host_b.0.did(), &sixth, 100, 2),
            ticket(&host_a.0.did(), &seventh, 100, 3),
        ];
        let shares = host_shares(&store, &tickets).unwrap();
        let a = shares.get(host_a.0.did().as_str()).copied().unwrap_or(0);
        let b = shares.get(host_b.0.did().as_str()).copied().unwrap_or(0);
        assert_eq!(a + b, 30);
        assert_eq!(b, 3); // 10 * 100 / 300
        assert_eq!(a, 27); // 6 + remainder 1, plus all 20
        assert!(host_shares(&store, &[]).unwrap().is_empty());
    }
}
