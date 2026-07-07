//! Deterministic, non-spendable reward accrual from verified presence.
//!
//! This is the demo stub that makes "presence becomes protocol value"
//! visible before any chain exists. It is a **pure function** over verified
//! [`PresenceVerdict`]s — no I/O, no clock of its own, no state beyond its inputs —
//! so the same verdicts always produce the same account.
//!
//! ## What it honors (constitution)
//!
//! - **P1 — money never buys voice.** A [`RewardAccount`] carries no governance
//!   weight. There is deliberately no field, method, or path here that turns
//!   points into votes, and nothing here is spendable.
//! - **P2 — one identity root, one vote / one accrual.** Accrual is per **identity root**
//!   (the delegator named in a verdict), never per device, so extra devices cannot
//!   multiply reward.
//! - **P4 — slow, presence-conditioned vesting.** Three brakes: a per-window rate
//!   cap (you cannot accrue quickly), a maturation delay (recent presence does not
//!   vest immediately), and diversity-weighting (repeated encounters with the same
//!   counterparty decay). Sustained, varied, real-world presence is what pays.
//!
//! ## What it is not
//!
//! Not Sybil resistance — that is personhood's job (SPEC-02). Diversity-weighting
//! and rate caps only blunt farming; they do not prove humanness. And not money:
//! the chain reward module (later) is the real thing; this stub deliberately has no
//! transfer, no balance ledger, and no spend.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

use std::collections::{HashMap, HashSet};

use did_mini::Did;
use mini_presence::PresenceVerdict;

/// Parameters governing accrual. All integer, so accrual is exactly reproducible.
#[derive(Debug, Clone)]
pub struct RewardParams {
    /// Points for a fresh (first-time) co-presence with a counterparty.
    pub base_points: u64,
    /// After this many encounters with the *same* counterparty, further ones give
    /// nothing. Repeats before that decay by halving (`base >> k`).
    pub max_repeats_per_counterparty: u32,
    /// Rate-cap window length in ms (`0` disables the cap).
    pub window_ms: u64,
    /// Maximum points that can accrue within any one window (the P4 rate brake).
    pub max_points_per_window: u64,
    /// A contribution only vests after this delay past its event time (P4).
    pub maturation_ms: u64,
}

impl RewardParams {
    /// A conservative demo profile: slow, diversity-weighted, day-scale maturation.
    pub fn demo_default() -> Self {
        RewardParams {
            base_points: 1_000,
            max_repeats_per_counterparty: 5,
            window_ms: 3_600_000, // 1 hour
            max_points_per_window: 5_000,
            maturation_ms: 86_400_000, // 1 day
        }
    }
}

/// An identity root's accrual, derived purely from verified presence. Non-spendable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RewardAccount {
    /// The identity root this account belongs to.
    pub identity_root: Did,
    /// Total points accrued (rate-capped, diversity-weighted).
    pub accrued_points: u64,
    /// Portion that has matured (vested) as of the `now_ms` passed to [`accrue`].
    pub vested_points: u64,
    /// How many distinct counterparties this identity root has been present with.
    pub distinct_counterparties: u32,
    /// How many co-presence events involved this identity root.
    pub event_count: u32,
}

/// Accrue one identity root's account from the verdict set, as of `now_ms`.
///
/// Deterministic: verdicts are processed in a canonical (time, counterparty) order,
/// so input ordering does not change the result. Self-pairings (a verdict whose two
/// identity roots are equal) contribute nothing.
pub fn accrue(
    identity_root: &Did,
    verdicts: &[PresenceVerdict],
    params: &RewardParams,
    now_ms: u64,
) -> RewardAccount {
    // Collect this identity root's co-presence events as (counterparty, at_ms).
    let mut events: Vec<(String, u64)> = Vec::new();
    for v in verdicts {
        let counterparty = if v.initiator_root.as_str() == identity_root.as_str() {
            &v.responder_root
        } else if v.responder_root.as_str() == identity_root.as_str() {
            &v.initiator_root
        } else {
            continue;
        };
        if counterparty.as_str() == identity_root.as_str() {
            continue; // defensive: ignore self-pairings
        }
        events.push((counterparty.as_str().to_string(), v.at_ms));
    }
    // Canonical order for reproducibility.
    events.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(&b.0)));

    let event_count = events.len() as u32;
    let mut seen_counts: HashMap<String, u32> = HashMap::new();
    let mut distinct: HashSet<String> = HashSet::new();
    let mut window_used: HashMap<u64, u64> = HashMap::new();
    let mut accrued: u64 = 0;
    let mut vested: u64 = 0;

    for (counterparty, at_ms) in &events {
        let at_ms = *at_ms;
        distinct.insert(counterparty.clone());

        let k = {
            let entry = seen_counts.entry(counterparty.clone()).or_insert(0);
            let prior = *entry;
            *entry += 1;
            prior
        };
        if k >= params.max_repeats_per_counterparty {
            continue;
        }
        let raw = params.base_points >> k.min(63);
        if raw == 0 {
            continue;
        }

        // Per-window rate cap.
        let credited = if params.window_ms == 0 {
            raw
        } else {
            let w = at_ms / params.window_ms;
            let used = window_used.entry(w).or_insert(0);
            let room = params.max_points_per_window.saturating_sub(*used);
            let c = raw.min(room);
            *used += c;
            c
        };

        accrued = accrued.saturating_add(credited);
        if at_ms.saturating_add(params.maturation_ms) <= now_ms {
            vested = vested.saturating_add(credited);
        }
    }

    RewardAccount {
        identity_root: identity_root.clone(),
        accrued_points: accrued,
        vested_points: vested,
        distinct_counterparties: distinct.len() as u32,
        event_count,
    }
}

/// Accrue accounts for every identity root appearing in the verdict set, sorted by
/// identifier for a stable, reproducible ledger.
pub fn ledger(
    verdicts: &[PresenceVerdict],
    params: &RewardParams,
    now_ms: u64,
) -> Vec<RewardAccount> {
    let mut roots: Vec<Did> = Vec::new();
    for v in verdicts {
        for h in [&v.initiator_root, &v.responder_root] {
            if !roots.iter().any(|x| x.as_str() == h.as_str()) {
                roots.push(h.clone());
            }
        }
    }
    roots.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    roots
        .iter()
        .map(|h| accrue(h, verdicts, params, now_ms))
        .collect()
}
