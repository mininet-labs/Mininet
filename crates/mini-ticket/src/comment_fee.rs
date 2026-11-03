//! A fee on commenting that rises fast with how much one author comments.
//!
//! Ordinary use is free of it: an author's first few comments in a window cost
//! nothing extra. Past that allowance the fee starts at a small base and
//! **doubles with every further comment** in the same window, so a person
//! posting normally never notices it while a flood becomes expensive within a
//! few dozen comments. The fee is owed by the comment's author; it is computed
//! from the author's own signed comment objects, so any device holding them
//! computes the same amount.
//!
//! Honest limits:
//! - Unsettled credit like everything in this crate: nothing is charged or
//!   paid; settlement is gated behind the external audit (D-0047).
//! - A comment's time is declared by its author, so a custom client can
//!   backdate comments to spread them across windows. The fee only binds where
//!   a counterparty (a host, a settlement layer) uses the same function over
//!   comments it accepted on arrival, which is future work.
//! - It is per identity root. Someone who mints many roots pays the allowance
//!   once per root; this raises the cost of flooding from one root, it does not
//!   stop a Sybil flood (the personhood layer's job).
//! - How the fee is shared among the hosts that kept the comment available is
//!   not built.

use did_mini::Did;
use mini_objects::{ObjectId, ObjectType};
use mini_store::{Backend, Store};

use crate::{Result, TicketError};

/// Trailing window, in milliseconds, over which an author's comments count.
pub const FEE_WINDOW_MS: u64 = 60 * 60 * 1000;
/// Comments in a window that carry no fee.
pub const FREE_COMMENTS_PER_WINDOW: u32 = 3;
/// Fee, in micro-MINI, for the first comment past the allowance.
pub const BASE_COMMENT_FEE_MICRO: u64 = 10;
/// The fee stops doubling after this many doublings (about 10.5 MINI at the
/// base), so it never overflows and a runaway account hits a fixed, large cost.
pub const MAX_DOUBLINGS: u32 = 20;

/// Fee for a comment when the author already has `prior_in_window` earlier
/// comments in the trailing window: 0 within the allowance, then the base,
/// doubling with each further comment.
pub fn comment_fee(prior_in_window: u32) -> u64 {
    if prior_in_window < FREE_COMMENTS_PER_WINDOW {
        return 0;
    }
    let doublings = (prior_in_window - FREE_COMMENTS_PER_WINDOW).min(MAX_DOUBLINGS);
    BASE_COMMENT_FEE_MICRO << doublings
}

/// One comment and the fee it carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommentFeeLine {
    pub comment: ObjectId,
    pub timestamp_ms: u64,
    pub prior_in_window: u32,
    pub fee_micro: u64,
}

/// Everything an author owes in comment fees according to the comments on this
/// device.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CommentFees {
    /// Comments that carry a fee (the rest are within the allowance).
    pub lines: Vec<CommentFeeLine>,
    pub total_micro: u64,
    /// All comments by the author seen on this device.
    pub comments: usize,
}

/// Comment fees owed by `author`, from their comment objects in `store`.
pub fn comment_fees<B: Backend>(store: &Store<B>, author: &Did) -> Result<CommentFees> {
    let ids = store
        .by_type(&ObjectType::COMMENT)
        .map_err(|error| TicketError::Store(error.to_string()))?;
    let mut mine: Vec<(u64, ObjectId)> = Vec::new();
    for id in ids {
        let object = store
            .get(&id)
            .map_err(|error| TicketError::Store(error.to_string()))?;
        if &object.author_human == author {
            mine.push((object.timestamp_ms, id));
        }
    }
    mine.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.as_str().cmp(b.1.as_str())));
    let mut fees = CommentFees {
        comments: mine.len(),
        ..CommentFees::default()
    };
    let mut start = 0usize;
    for (index, (timestamp_ms, id)) in mine.iter().enumerate() {
        while mine[start].0.saturating_add(FEE_WINDOW_MS) <= *timestamp_ms {
            start += 1;
        }
        let prior = u32::try_from(index - start).unwrap_or(u32::MAX);
        let fee_micro = comment_fee(prior);
        if fee_micro > 0 {
            fees.total_micro = fees
                .total_micro
                .checked_add(fee_micro)
                .ok_or(TicketError::Overflow)?;
            fees.lines.push(CommentFeeLine {
                comment: id.clone(),
                timestamp_ms: *timestamp_ms,
                prior_in_window: prior,
                fee_micro,
            });
        }
    }
    Ok(fees)
}

/// What the author's next comment, made at `now_ms`, would cost. For showing
/// the price before the author posts.
pub fn next_comment_fee<B: Backend>(store: &Store<B>, author: &Did, now_ms: u64) -> Result<u64> {
    let ids = store
        .by_type(&ObjectType::COMMENT)
        .map_err(|error| TicketError::Store(error.to_string()))?;
    let mut prior: u32 = 0;
    for id in ids {
        let object = store
            .get(&id)
            .map_err(|error| TicketError::Store(error.to_string()))?;
        if &object.author_human == author
            && object.timestamp_ms <= now_ms
            && object.timestamp_ms.saturating_add(FEE_WINDOW_MS) > now_ms
        {
            prior = prior.saturating_add(1);
        }
    }
    Ok(comment_fee(prior))
}

#[cfg(test)]
mod tests {
    use super::*;
    use did_mini::{Capabilities, Controller};
    use mini_objects::{ObjectBuilder, Payload};
    use mini_store::MemoryBackend;

    fn person(seed: u8) -> (Controller, Controller) {
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

    fn comment(
        store: &mut Store<MemoryBackend>,
        who: &(Controller, Controller),
        parent: &ObjectId,
        at_ms: u64,
        sequence: u64,
    ) {
        let object = ObjectBuilder::new(ObjectType::COMMENT)
            .timestamp_ms(at_ms)
            .sequence(sequence)
            .link("parent", parent.clone())
            .payload(Payload::Public(format!("c{sequence}").into_bytes()))
            .sign(&who.0.did(), &who.1)
            .unwrap();
        store.insert(&object).unwrap();
    }

    #[test]
    fn the_fee_is_free_then_small_then_doubles() {
        assert_eq!(comment_fee(0), 0);
        assert_eq!(comment_fee(FREE_COMMENTS_PER_WINDOW - 1), 0);
        assert_eq!(
            comment_fee(FREE_COMMENTS_PER_WINDOW),
            BASE_COMMENT_FEE_MICRO
        );
        assert_eq!(
            comment_fee(FREE_COMMENTS_PER_WINDOW + 1),
            2 * BASE_COMMENT_FEE_MICRO
        );
        assert_eq!(
            comment_fee(FREE_COMMENTS_PER_WINDOW + 5),
            32 * BASE_COMMENT_FEE_MICRO
        );
        // Capped: never overflows however long the burst.
        assert_eq!(
            comment_fee(u32::MAX),
            BASE_COMMENT_FEE_MICRO << MAX_DOUBLINGS
        );
    }

    #[test]
    fn a_normal_commenter_pays_nothing_and_a_burst_gets_expensive() {
        let mut store = Store::new(MemoryBackend::new());
        let normal = person(10);
        let spammer = person(20);
        let parent = ObjectBuilder::new(ObjectType::POST)
            .payload(Payload::Public(b"post".to_vec()))
            .sign(&normal.0.did(), &normal.1)
            .unwrap();
        store.insert(&parent).unwrap();
        let hour = FEE_WINDOW_MS;
        // The normal author comments three times an hour for a day.
        for n in 0..72u64 {
            comment(
                &mut store,
                &normal,
                parent.id(),
                1_000 + n * (hour / 3),
                n + 1,
            );
        }
        // The spammer posts 20 comments inside one minute.
        for n in 0..20u64 {
            comment(&mut store, &spammer, parent.id(), 5_000 + n * 1_000, n + 1);
        }
        let normal_fees = comment_fees(&store, &normal.0.did()).unwrap();
        assert_eq!(normal_fees.comments, 72);
        assert_eq!(normal_fees.total_micro, 0);
        let spam_fees = comment_fees(&store, &spammer.0.did()).unwrap();
        // 17 comments past the 3 free ones: 10 * (2^17 - 1) micro.
        assert_eq!(spam_fees.lines.len(), 17);
        assert_eq!(
            spam_fees.total_micro,
            BASE_COMMENT_FEE_MICRO * ((1 << 17) - 1)
        );
        // The price of the next one is shown before posting.
        let next = next_comment_fee(&store, &spammer.0.did(), 30_000).unwrap();
        assert_eq!(next, BASE_COMMENT_FEE_MICRO << 17);
        // An hour later the window has emptied and the price drops back.
        let later = next_comment_fee(&store, &spammer.0.did(), 5_000 + 2 * hour).unwrap();
        assert_eq!(later, 0);
    }
}
