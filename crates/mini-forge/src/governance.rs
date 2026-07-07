//! Merge governance (SPEC-11, UI plan E8.S6–S7): the loop that lets Mininet be
//! built **from inside Mininet**.
//!
//! ## The model, step by step
//!
//! 1. A [`project`] is an object declaring a name, a **maintainer set** (identity-root
//!    DIDs), and a merge policy (`min_approvals`). The project id is the
//!    genesis of its governance chain.
//! 2. Anyone may [`propose`] a PR — open participation is the point. A PR names
//!    the target branch and links the exact `head` commit it proposes and the
//!    chain entry (`base`) it was built against. Discussion rides `mini-crdt`
//!    with the PR object as the doc root — no new machinery.
//! 3. Maintainers [`approve`] (or request changes). An approval is **bound to
//!    the exact head commit reviewed** — a proposer cannot swap commits under
//!    collected approvals.
//! 4. A maintainer records the [`merge`] as a **chain entry** linking the
//!    previous entry. Maintainer-set / policy changes are [`amend`] entries in
//!    the *same* chain, approved under the *current* policy — the governance is
//!    self-amending, with no owner key anywhere (P3).
//! 5. [`resolve_project`] walks the chain deterministically and yields the
//!    canonical branch heads. Every replica with the same objects resolves the
//!    same state.
//!
//! ## The guarantees, encoded as validity rules [FREEZE]
//!
//! - **Money never buys merge (P1/SPEC-11).** A quorum is counted in *distinct
//!   verified identity roots from the maintainer set* (identity root, not human:
//!   personhood is SPEC-02, pending — D-0030). No balance, stake, or payment
//!   appears anywhere in the rule — there is nothing to buy.
//! - **One identity root counts once (P2 applies once personhood exists).** Approvals from many devices of one identity root
//!   collapse to one; the PR **author's own approval never counts** (review
//!   means someone else looked).
//! - **No retroactive capture.** Validity is judged against the maintainer set
//!   *as of the previous chain entry*, so amending the set cannot rewrite
//!   history, and removed maintainers lose power only forward.
//! - **Forks are surfaced, not hidden.** Two valid entries with the same `prev`
//!   resolve deterministically (greatest id) **and** set `forks_detected` — an
//!   honest provisional stand-in until `mini-chain` finality replaces the
//!   tiebreak (the chain replaces the counting, never these objects).

use did_mini::{Controller, Did};
use mini_objects::{Object, ObjectBuilder, ObjectId, ObjectType, Payload};
use mini_store::{Backend, Store};

use crate::oracle::{author_verified, IdentityOracle};
use crate::{take_str, valid_name, ForgeError, Result, MAX_VERSION_BYTES};

/// Project (governance genesis) object type.
pub const PROJECT_TYPE: &str = "mini/project";
/// Pull-request object type.
pub const PR_TYPE: &str = "mini/pr";
/// Review/approval object type.
pub const APPROVE_TYPE: &str = "mini/approve";
/// Governance-chain entry object type (merges and amendments).
pub const CHAIN_TYPE: &str = "mini/chain-entry";
/// Maximum maintainers per project.
pub const MAX_MAINTAINERS: usize = 64;
/// Maximum PR title bytes.
pub const MAX_TITLE_BYTES: usize = 256;
/// Maximum governance-chain length walked (hostile-input bound).
pub const MAX_CHAIN_LEN: usize = 100_000;

const ENTRY_MERGE: u8 = 1;
const ENTRY_AMEND: u8 = 2;
const VERDICT_APPROVE: u8 = 1;

/// A project's governance parameters at some chain position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Policy {
    /// Distinct approving maintainers required (author excluded).
    pub min_approvals: u32,
    /// The maintainer set (identity-root DIDs).
    pub maintainers: Vec<Did>,
}

impl Policy {
    fn contains(&self, identity_root: &Did) -> bool {
        self.maintainers.iter().any(|m| m.as_str() == identity_root.as_str())
    }
}

/// Validate a policy: at least one maintainer, no duplicates, and
/// `min_approvals` in `1..=maintainers.len()` (0 would let anything merge; a
/// value above the set size would deadlock governance forever).
fn valid_policy(policy: &Policy) -> Result<()> {
    let n = policy.maintainers.len();
    if n == 0 || n > MAX_MAINTAINERS {
        return Err(ForgeError::FieldTooLarge);
    }
    if policy.min_approvals == 0 || policy.min_approvals as usize > n {
        return Err(ForgeError::BadObject);
    }
    // No duplicate maintainers (would let one identity root count as several).
    let mut seen: Vec<&str> = Vec::with_capacity(n);
    for m in &policy.maintainers {
        if seen.contains(&m.scid()) {
            return Err(ForgeError::BadObject);
        }
        seen.push(m.scid());
    }
    Ok(())
}


fn encode_policy(payload: &mut Vec<u8>, policy: &Policy) {
    payload.extend_from_slice(&policy.min_approvals.to_be_bytes());
    payload.extend_from_slice(&(policy.maintainers.len() as u32).to_be_bytes());
    for m in &policy.maintainers {
        put_str(payload, m.as_str());
    }
}

fn decode_policy(b: &[u8], off: &mut usize) -> Option<Policy> {
    if *off + 8 > b.len() {
        return None;
    }
    let min_approvals = u32::from_be_bytes([b[*off], b[*off + 1], b[*off + 2], b[*off + 3]]);
    let n = u32::from_be_bytes([b[*off + 4], b[*off + 5], b[*off + 6], b[*off + 7]]) as usize;
    *off += 8;
    if n == 0 || n > MAX_MAINTAINERS {
        return None;
    }
    let mut maintainers = Vec::with_capacity(n);
    for _ in 0..n {
        let s = take_str(b, off)?;
        maintainers.push(Did::parse(&s).ok()?);
    }
    Some(Policy {
        min_approvals,
        maintainers,
    })
}

/// Create a project: name + initial policy. The returned object's id is the
/// governance-chain genesis.
pub fn project<B: Backend>(
    store: &mut Store<B>,
    human: &Did,
    device: &Controller,
    name: &str,
    policy: &Policy,
) -> Result<Object> {
    if !valid_name(name) {
        return Err(ForgeError::BadName);
    }
    valid_policy(policy)?;
    let mut payload = Vec::new();
    put_str(&mut payload, name);
    encode_policy(&mut payload, policy);
    let obj = ObjectBuilder::new(ObjectType::Custom(PROJECT_TYPE.to_string()))
        .payload(Payload::Public(payload))
        .sign(human, device)?;
    store.insert(&obj)?;
    Ok(obj)
}

/// Propose a PR: move `branch` of `project_id` to commit `head`, built against
/// chain entry `base` (the project id itself for the first PR). Anyone may
/// propose.
#[allow(clippy::too_many_arguments)]
pub fn propose<B: Backend>(
    store: &mut Store<B>,
    human: &Did,
    device: &Controller,
    project_id: &ObjectId,
    branch: &str,
    title: &str,
    head: &ObjectId,
    base: &ObjectId,
    timestamp_ms: u64,
    sequence: u64,
) -> Result<Object> {
    if !valid_name(branch) {
        return Err(ForgeError::BadName);
    }
    if title.len() > MAX_TITLE_BYTES || branch.len() > MAX_VERSION_BYTES {
        return Err(ForgeError::FieldTooLarge);
    }
    let mut payload = Vec::new();
    put_str(&mut payload, branch);
    put_str(&mut payload, title);
    let obj = ObjectBuilder::new(ObjectType::Custom(PR_TYPE.to_string()))
        .timestamp_ms(timestamp_ms)
        .sequence(sequence)
        .payload(Payload::Public(payload))
        .link("project", project_id.clone())
        .link("head", head.clone())
        .link("base", base.clone())
        .sign(human, device)?;
    store.insert(&obj)?;
    Ok(obj)
}

/// Record a review verdict on `pr_id`, **bound to the exact head commit
/// reviewed** — approvals cannot be reused for a swapped commit.
#[allow(clippy::too_many_arguments)]
pub fn approve<B: Backend>(
    store: &mut Store<B>,
    human: &Did,
    device: &Controller,
    pr_id: &ObjectId,
    reviewed_head: &ObjectId,
    approve: bool,
    timestamp_ms: u64,
    sequence: u64,
) -> Result<Object> {
    let mut payload = Vec::new();
    payload.push(if approve { VERDICT_APPROVE } else { 0 });
    put_str(&mut payload, reviewed_head.as_str());
    let obj = ObjectBuilder::new(ObjectType::Custom(APPROVE_TYPE.to_string()))
        .timestamp_ms(timestamp_ms)
        .sequence(sequence)
        .payload(Payload::Public(payload))
        .link("pr", pr_id.clone())
        .sign(human, device)?;
    store.insert(&obj)?;
    Ok(obj)
}

/// Record a merge of `pr_id` as the chain entry after `prev` (the project id
/// or the current tip). Validity is judged at [`resolve_project`] time against
/// the policy as of `prev`.
pub fn merge<B: Backend>(
    store: &mut Store<B>,
    human: &Did,
    device: &Controller,
    project_id: &ObjectId,
    prev: &ObjectId,
    pr_id: &ObjectId,
    timestamp_ms: u64,
    sequence: u64,
) -> Result<Object> {
    let obj = ObjectBuilder::new(ObjectType::Custom(CHAIN_TYPE.to_string()))
        .timestamp_ms(timestamp_ms)
        .sequence(sequence)
        .payload(Payload::Public(vec![ENTRY_MERGE]))
        .link("project", project_id.clone())
        .link("prev", prev.clone())
        .link("pr", pr_id.clone())
        .sign(human, device)?;
    store.insert(&obj)?;
    Ok(obj)
}

/// Record a governance amendment (new policy) as the chain entry after `prev`.
/// Approved under the *current* policy via approvals on this entry's id —
/// self-amending, no owner key.
#[allow(clippy::too_many_arguments)]
pub fn amend<B: Backend>(
    store: &mut Store<B>,
    human: &Did,
    device: &Controller,
    project_id: &ObjectId,
    prev: &ObjectId,
    new_policy: &Policy,
    timestamp_ms: u64,
    sequence: u64,
) -> Result<Object> {
    valid_policy(new_policy)?;
    let mut payload = vec![ENTRY_AMEND];
    encode_policy(&mut payload, new_policy);
    let obj = ObjectBuilder::new(ObjectType::Custom(CHAIN_TYPE.to_string()))
        .timestamp_ms(timestamp_ms)
        .sequence(sequence)
        .payload(Payload::Public(payload))
        .link("project", project_id.clone())
        .link("prev", prev.clone())
        .sign(human, device)?;
    store.insert(&obj)?;
    Ok(obj)
}

/// The resolved canonical state of a project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectState {
    /// Project name.
    pub name: String,
    /// Policy in force at the tip.
    pub policy: Policy,
    /// Canonical `(branch, commit)` heads.
    pub branches: Vec<(String, ObjectId)>,
    /// The chain tip (project id if no entries applied).
    pub tip: ObjectId,
    /// Number of applied chain entries.
    pub entries: usize,
    /// True if competing valid entries were seen (provisional tiebreak used;
    /// chain finality resolves this for real later).
    pub forks_detected: bool,
}

/// Count distinct approving maintainers for (`pr_or_entry`, exact `head`),
/// under `policy`, excluding `author`.
fn quorum<B: Backend>(
    store: &Store<B>,
    oracle: &dyn IdentityOracle,
    target: &ObjectId,
    bound_to: Option<&ObjectId>,
    policy: &Policy,
    author: &Did,
) -> Result<u32> {
    let mut roots: Vec<String> = Vec::new();
    for id in store.linking_to(target)? {
        let a = match store.get(&id) {
            Ok(a) => a,
            Err(_) => continue,
        };
        if a.object_type != ObjectType::Custom(APPROVE_TYPE.to_string()) {
            continue;
        }
        let reviewed = match parse_approval_payload_strict(&a) {
            Some(reviewed) => reviewed,
            None => continue,
        };
        if let Some(required) = bound_to {
            if reviewed != required.as_str() {
                continue; // approval bound to a different commit
            }
        } else if reviewed != target.as_str() {
            continue; // amendments: approval must name the entry itself
        }
        if a.author_human.as_str() == author.as_str() {
            continue; // the author never counts
        }
        if !policy.contains(&a.author_human) {
            continue; // only maintainers count
        }
        // Re-bind: only a currently-verified identity root's approval counts.
        if !author_verified(oracle, &a) {
            continue;
        }
        let scid = a.author_human.scid().to_string();
        if !roots.contains(&scid) {
            roots.push(scid); // one identity root counts once, whatever the device
        }
    }
    Ok(roots.len() as u32)
}

/// Walk the governance chain deterministically and return the canonical state.
/// Pure over the store: every replica with the same objects resolves the same
/// state.
pub fn resolve_project<B: Backend>(
    store: &Store<B>,
    oracle: &dyn IdentityOracle,
    project_id: &ObjectId,
) -> Result<ProjectState> {
    let genesis = store.get(project_id)?;
    if genesis.object_type != ObjectType::Custom(PROJECT_TYPE.to_string()) {
        return Err(ForgeError::BadObject);
    }
    // The governance genesis must itself be authored by a verified identity root.
    if !author_verified(oracle, &genesis) {
        return Err(ForgeError::BadObject);
    }
    let gb = match &genesis.payload {
        Payload::Public(b) => b,
        Payload::Encrypted(_) => return Err(ForgeError::BadObject),
    };
    let mut off = 0usize;
    let name = take_str(gb, &mut off).ok_or(ForgeError::BadObject)?;
    if !valid_name(&name) {
        return Err(ForgeError::BadObject);
    }
    let mut policy = decode_policy(gb, &mut off).ok_or(ForgeError::BadObject)?;
    // The governance root itself must be a valid policy — a hand-crafted signed
    // project object cannot smuggle a zero-approval, duplicate-maintainer set,
    // impossible quorum, bad name, or trailing payload past resolution.
    valid_policy(&policy).map_err(|_| ForgeError::BadObject)?;
    if off != gb.len() {
        return Err(ForgeError::BadObject);
    }

    let mut branches: Vec<(String, ObjectId)> = Vec::new();
    let mut tip = project_id.clone();
    let mut entries = 0usize;
    let mut forks_detected = false;

    loop {
        if entries > MAX_CHAIN_LEN {
            return Err(ForgeError::FieldTooLarge);
        }
        // Candidates: chain entries whose "prev" is the current tip and which
        // are valid under the CURRENT policy.
        let mut valid: Vec<Object> = Vec::new();
        for id in store.linking_to(&tip)? {
            let e = match store.get(&id) {
                Ok(e) => e,
                Err(_) => continue,
            };
            if e.object_type != ObjectType::Custom(CHAIN_TYPE.to_string()) {
                continue;
            }
            let prev_ok = e.links.iter().any(|l| l.rel == "prev" && l.target == tip);
            let proj_ok = e
                .links
                .iter()
                .any(|l| l.rel == "project" && l.target == *project_id);
            if !prev_ok || !proj_ok {
                continue;
            }
            // The entry's recorder must be a current maintainer AND a
            // currently-verified identity root.
            if !policy.contains(&e.author_human) || !author_verified(oracle, &e) {
                continue;
            }
            if entry_is_valid(store, oracle, project_id, &e, &policy)? {
                valid.push(e);
            }
        }
        if valid.is_empty() {
            break;
        }
        if valid.len() > 1 {
            forks_detected = true;
        }
        // Deterministic winner: greatest id (provisional; chain finality later).
        valid.sort_by(|a, b| a.id().as_str().cmp(b.id().as_str()));
        let winner = valid.pop().expect("non-empty");

        apply_entry(store, &winner, &mut policy, &mut branches)?;
        tip = winner.id().clone();
        entries += 1;
    }

    Ok(ProjectState {
        name,
        policy,
        branches,
        tip,
        entries,
        forks_detected,
    })
}

fn entry_is_valid<B: Backend>(
    store: &Store<B>,
    oracle: &dyn IdentityOracle,
    project_id: &ObjectId,
    e: &Object,
    policy: &Policy,
) -> Result<bool> {
    let entry = match parse_chain_entry_payload_strict(e) {
        Some(entry) => entry,
        None => return Ok(false),
    };
    match entry {
        ParsedChainEntry::Merge => {
            let pr_id = match e.links.iter().find(|l| l.rel == "pr") {
                Some(l) => l.target.clone(),
                None => return Ok(false),
            };
            let pr = match store.get(&pr_id) {
                Ok(p) => p,
                Err(_) => return Ok(false),
            };
            if pr.object_type != ObjectType::Custom(PR_TYPE.to_string()) {
                return Ok(false);
            }
            // Re-bind the PR author too (the quorum excludes them by identity).
            if !author_verified(oracle, &pr) {
                return Ok(false);
            }
            // The PR payload must parse under the one canonical, strict parser.
            if parse_pr_payload_strict(&pr).is_none() {
                return Ok(false);
            }
            // Lineage: the PR must target THIS project, and have been built
            // against the entry it is being merged onto (its `base` == entry
            // `prev`) — no cross-project or stale-base merges.
            let pr_project = pr.links.iter().find(|l| l.rel == "project").map(|l| &l.target);
            if pr_project != Some(project_id) {
                return Ok(false);
            }
            let entry_prev = e.links.iter().find(|l| l.rel == "prev").map(|l| l.target.clone());
            let pr_base = pr.links.iter().find(|l| l.rel == "base").map(|l| l.target.clone());
            if entry_prev.is_none() || pr_base != entry_prev {
                return Ok(false);
            }
            // The head must exist and be a real commit.
            let head = match pr.links.iter().find(|l| l.rel == "head") {
                Some(l) => l.target.clone(),
                None => return Ok(false),
            };
            match store.get(&head) {
                Ok(h) if h.object_type == ObjectType::COMMIT => {}
                _ => return Ok(false),
            }
            let n = quorum(store, oracle, &pr_id, Some(&head), policy, &pr.author_human)?;
            Ok(n >= policy.min_approvals)
        }
        ParsedChainEntry::Amend(new_policy) => {
            valid_policy(&new_policy).map_err(|_| ForgeError::BadObject)?;
            let n = quorum(store, oracle, e.id(), None, policy, &e.author_human)?;
            Ok(n >= policy.min_approvals)
        }
    }
}

fn apply_entry<B: Backend>(
    store: &Store<B>,
    e: &Object,
    policy: &mut Policy,
    branches: &mut Vec<(String, ObjectId)>,
) -> Result<()> {
    let entry = parse_chain_entry_payload_strict(e).ok_or(ForgeError::BadObject)?;
    match entry {
        ParsedChainEntry::Merge => {
            let pr_id = e
                .links
                .iter()
                .find(|l| l.rel == "pr")
                .ok_or(ForgeError::BadObject)?
                .target
                .clone();
            let pr = store.get(&pr_id)?;
            let (branch, _title) =
                parse_pr_payload_strict(&pr).ok_or(ForgeError::BadObject)?;
            let head = pr
                .links
                .iter()
                .find(|l| l.rel == "head")
                .ok_or(ForgeError::BadObject)?
                .target
                .clone();
            match branches.iter_mut().find(|(n, _)| *n == branch) {
                Some((_, h)) => *h = head,
                None => branches.push((branch, head)),
            }
        }
        ParsedChainEntry::Amend(new_policy) => {
            *policy = new_policy;
        }
    }
    Ok(())
}

fn put_str(w: &mut Vec<u8>, s: &str) {
    w.extend_from_slice(&(s.len() as u32).to_be_bytes());
    w.extend_from_slice(s.as_bytes());
}


#[derive(Debug, Clone, PartialEq, Eq)]
enum ParsedChainEntry {
    Merge,
    Amend(Policy),
}

/// The single canonical approval parser. Payload = verdict byte + reviewed id,
/// with exact EOF. Loose approval parsing would let future callers disagree
/// about what was actually reviewed.
fn parse_approval_payload_strict(approval: &Object) -> Option<String> {
    let b = match &approval.payload {
        Payload::Public(b) => b,
        Payload::Encrypted(_) => return None,
    };
    if b.first().copied()? != VERDICT_APPROVE {
        return None;
    }
    let mut off = 1usize;
    let reviewed = take_str(b, &mut off)?;
    if ObjectId::parse(&reviewed).is_err() {
        return None;
    }
    if off != b.len() {
        return None;
    }
    Some(reviewed)
}

/// The single canonical chain-entry parser. Merge payloads are exactly one byte;
/// amendment payloads are exactly tag + policy. This prevents canonical branch
/// resolution from depending on loose parsing or ignored trailing bytes.
fn parse_chain_entry_payload_strict(entry: &Object) -> Option<ParsedChainEntry> {
    let b = match &entry.payload {
        Payload::Public(b) => b,
        Payload::Encrypted(_) => return None,
    };
    match b.first().copied()? {
        ENTRY_MERGE => {
            if b.len() == 1 {
                Some(ParsedChainEntry::Merge)
            } else {
                None
            }
        }
        ENTRY_AMEND => {
            let mut off = 1usize;
            let policy = decode_policy(b, &mut off)?;
            if off != b.len() || valid_policy(&policy).is_err() {
                return None;
            }
            Some(ParsedChainEntry::Amend(policy))
        }
        _ => None,
    }
}

/// The single canonical PR-payload parser: `(branch, title)`. Used by both
/// validation and application so canonical branch state can never depend on
/// loose parsing. Enforces a valid branch name, the title cap, and exact EOF.
fn parse_pr_payload_strict(pr: &Object) -> Option<(String, String)> {
    let b = match &pr.payload {
        Payload::Public(b) => b,
        Payload::Encrypted(_) => return None,
    };
    let mut off = 0usize;
    let branch = take_str(b, &mut off)?;
    let title = take_str(b, &mut off)?;
    if !valid_name(&branch) || branch.len() > MAX_VERSION_BYTES {
        return None;
    }
    if title.len() > MAX_TITLE_BYTES {
        return None;
    }
    if off != b.len() {
        return None; // strict: no trailing bytes
    }
    Some((branch, title))
}
