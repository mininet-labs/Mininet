//! Device-local abuse handling (roadmap #76, Phase 10.6).
//!
//! ## What this is [FREEZE]
//!
//! Constitution principle 10 puts abuse handling — harassment, illegal
//! content distribution, coordinated manipulation — entirely in
//! **user/community filters, indexes, and blocklists**, never in a central
//! moderation authority. Constitution principle 7 says nobody can be forced
//! out of, or denied, basic network use. Put together: this crate can only
//! change what **one identity's own device shows that identity**. It can
//! never:
//!
//! - remove, quarantine, or annotate an object anywhere but the local
//!   viewer's own render path (nothing here touches `mini-store`, `mini-net`
//!   replication, or any object's existence);
//! - stop a blocked peer from publishing, replicating, or being served by
//!   anyone else's node, or from continuing to use the network at all;
//! - aggregate into a shared or majority-enforced verdict — a
//!   [`SafetyProfile`] belongs to exactly one viewer and nothing here syncs,
//!   ranks, or counts profiles across viewers. A "community blocklist" under
//!   principle 10 is just an ordinary shareable object (e.g. published
//!   through `mini-objects`/`mini-social`) that another viewer may *choose*
//!   to import as **their own** local rules via [`SafetyProfile::import`];
//!   importing never happens automatically and an imported rule is
//!   indistinguishable afterward from one the viewer typed in themselves —
//!   there is no way for the source of a rule to un-import it, override the
//!   viewer's own edits, or learn who imported it.
//!
//! This mirrors, and formalizes as a typed, tested API, the ad hoc
//! `muted.txt` device-local mute list shipped in `mini-desktop` (D-0523).
//! That was the local half of blocking with no shared type; this crate is
//! the shared, reusable primitive every client (desktop, CLI, future
//! mobile) layers on top of `mini-social`/`mini-objects`/`mini-store` reads,
//! so each client does not reinvent (and risk diverging on) the same
//! block/mute semantics.
//!
//! ## Typed domains, not `filter(bytes)` [FREEZE]
//!
//! Every state change is a specific, named request type consumed by exactly
//! one method: [`BlockIdentityRequest`], [`UnblockIdentityRequest`],
//! [`AddMuteRuleRequest`], [`RemoveMuteRuleRequest`]. There is no generic
//! "apply this opaque rule" entry point, so the set of things a caller can
//! do to a [`SafetyProfile`] is fixed by this file, not by whatever a
//! caller assembles.
//!
//! ## Everything is opt-in and local
//!
//! A fresh [`SafetyProfile`] blocks and mutes nothing. Every entry exists
//! because this viewer's device called [`SafetyProfile::block_identity`],
//! [`SafetyProfile::add_mute_rule`], or [`SafetyProfile::import`] on rules
//! the viewer chose to import. Filtering never runs unless the caller
//! explicitly asks [`SafetyProfile::visibility_for`] or
//! [`SafetyProfile::filter`].

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

use did_mini::Did;
use std::collections::BTreeMap;

/// Why a piece of content is hidden from this viewer, or that it is shown.
///
/// This is a *local render decision*, never a network effect: nothing here
/// implies the content was removed, unreplicated, or unavailable to anyone
/// else. A caller UI is expected to still offer "show anyway" for a single
/// item, since hiding is this viewer's own reversible choice.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Visibility {
    /// Nothing local matched; show it.
    Visible,
    /// The author is on this viewer's own block list.
    HiddenBlockedAuthor,
    /// A local mute rule matched the content.
    HiddenMuted {
        /// Which rule matched, so a UI can explain and offer to remove it.
        rule: MuteRuleId,
    },
}

impl Visibility {
    /// True for anything other than [`Visibility::Visible`].
    pub fn is_hidden(&self) -> bool {
        !matches!(self, Visibility::Visible)
    }
}

/// A stable, viewer-local identifier for one mute rule.
///
/// Identifiers are assigned in insertion order starting at 1 and are never
/// reused, including after removal, so a UI reference to "rule 3" stays
/// meaningful for the life of the profile even if other rules are deleted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MuteRuleId(u64);

impl MuteRuleId {
    /// The raw counter value, for display/persistence.
    pub fn value(&self) -> u64 {
        self.0
    }
}

/// One local content-matching rule.
///
/// Matching is deliberately simple substring/exact matching over text the
/// *caller* extracts from an object (a post body, a wall bio, a comment) —
/// this crate never parses `mini-objects` payloads itself, so it stays
/// reusable across every content shape without a dependency edge back onto
/// `mini-objects`/`mini-social`. Matching is case-insensitive ASCII-folded
/// so "SPAM" and "spam" are the same rule.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum MuteRule {
    /// Hide anything whose text contains this substring.
    Keyword(String),
    /// Hide exactly one object, named by its content-addressed id string
    /// (e.g. `ObjectId::as_str()`), regardless of its text.
    ExactObject(String),
}

impl MuteRule {
    fn matches(&self, object_id: &str, text: &str) -> bool {
        match self {
            MuteRule::Keyword(k) => {
                !k.is_empty() && text.to_ascii_lowercase().contains(&k.to_ascii_lowercase())
            }
            MuteRule::ExactObject(id) => id == object_id,
        }
    }

    fn decode(line: &str) -> Result<Self, SafetyError> {
        let (kind, rest) = line
            .split_once('\t')
            .ok_or_else(|| SafetyError::Malformed(line.to_string()))?;
        match kind {
            "keyword" => Ok(MuteRule::Keyword(rest.to_string())),
            "object" => Ok(MuteRule::ExactObject(rest.to_string())),
            _ => Err(SafetyError::Malformed(line.to_string())),
        }
    }
}

/// Request to add this identity root to the viewer's own block list.
///
/// Blocking is a statement about what *this device* renders, never a
/// network action: it does not revoke, unfollow-and-forbid-refollow at the
/// protocol level, or notify anyone. A `reason` is for the viewer's own
/// later reference only — it is never transmitted anywhere by this crate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockIdentityRequest {
    pub blocked: Did,
    pub reason: Option<String>,
}

/// Request to remove an identity root from the viewer's block list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnblockIdentityRequest {
    pub blocked: Did,
}

/// Request to add one local content-mute rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddMuteRuleRequest {
    pub rule: MuteRule,
}

/// Request to remove a previously added mute rule by its assigned id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RemoveMuteRuleRequest {
    pub rule_id: MuteRuleId,
}

/// Errors from parsing a persisted profile. Never returned by the
/// in-memory mutation methods, which cannot fail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SafetyError {
    /// A persisted line was not in a recognized encoding.
    Malformed(String),
    /// No more stable rule identifiers can be allocated.
    RuleIdsExhausted,
}

impl core::fmt::Display for SafetyError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            SafetyError::Malformed(line) => write!(f, "malformed safety-profile line: {line:?}"),
            SafetyError::RuleIdsExhausted => f.write_str("safety rule identifiers exhausted"),
        }
    }
}

impl std::error::Error for SafetyError {}

/// One viewer's local safety state: who they block, and what content they
/// mute. Belongs to exactly one identity's device; never merged, ranked, or
/// compared against another viewer's profile by anything in this crate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SafetyProfile {
    // Keyed by `Did::as_str()`: `Did` has no `Ord` (identity roots are
    // never meant to be sorted/compared by protocol code), so a stable
    // (sorted, deterministic) iteration/persistence order is obtained via
    // the string form instead, without adding an ordering to `did-mini`
    // itself.
    blocked: BTreeMap<String, Did>,
    block_reasons: BTreeMap<String, String>,
    mute_rules: BTreeMap<u64, MuteRule>,
    next_rule_id: u64,
}

impl Default for SafetyProfile {
    fn default() -> Self {
        Self::new()
    }
}

impl SafetyProfile {
    /// A fresh profile: blocks and mutes nothing.
    pub fn new() -> Self {
        Self {
            blocked: BTreeMap::new(),
            block_reasons: BTreeMap::new(),
            mute_rules: BTreeMap::new(),
            next_rule_id: 1,
        }
    }

    /// Add an identity root to this viewer's own block list. Idempotent: a
    /// repeat block request for an already-blocked identity replaces its
    /// stored reason and returns `false`.
    pub fn block_identity(&mut self, request: BlockIdentityRequest) -> bool {
        let key = request.blocked.as_str().to_string();
        let is_new = self.blocked.insert(key.clone(), request.blocked).is_none();
        match request.reason {
            Some(reason) => {
                self.block_reasons.insert(key, reason);
            }
            None => {
                self.block_reasons.remove(&key);
            }
        }
        is_new
    }

    /// Remove an identity root from this viewer's block list. Returns
    /// `true` if it was previously blocked.
    pub fn unblock_identity(&mut self, request: UnblockIdentityRequest) -> bool {
        let key = request.blocked.as_str();
        self.block_reasons.remove(key);
        self.blocked.remove(key).is_some()
    }

    /// True if this identity root is on the viewer's own block list.
    pub fn is_blocked(&self, identity: &Did) -> bool {
        self.blocked.contains_key(identity.as_str())
    }

    /// The viewer's own note for why they blocked this identity, if any.
    pub fn block_reason(&self, identity: &Did) -> Option<&str> {
        self.block_reasons
            .get(identity.as_str())
            .map(String::as_str)
    }

    /// Every currently blocked identity root, in a stable (sorted) order.
    pub fn blocked_identities(&self) -> impl Iterator<Item = &Did> {
        self.blocked.values()
    }

    /// Add a local content-mute rule and return its assigned id.
    pub fn add_mute_rule(
        &mut self,
        request: AddMuteRuleRequest,
    ) -> Result<MuteRuleId, SafetyError> {
        let id = self.next_rule_id;
        let next = id.checked_add(1).ok_or(SafetyError::RuleIdsExhausted)?;
        self.mute_rules.insert(id, request.rule);
        self.next_rule_id = next;
        Ok(MuteRuleId(id))
    }

    /// Remove a mute rule by id. Returns `true` if it existed.
    pub fn remove_mute_rule(&mut self, request: RemoveMuteRuleRequest) -> bool {
        self.mute_rules.remove(&request.rule_id.0).is_some()
    }

    /// Every currently active mute rule with its id, in insertion order.
    pub fn mute_rules(&self) -> impl Iterator<Item = (MuteRuleId, &MuteRule)> {
        self.mute_rules
            .iter()
            .map(|(id, rule)| (MuteRuleId(*id), rule))
    }

    /// How many identities are blocked.
    pub fn blocked_count(&self) -> usize {
        self.blocked.len()
    }

    /// How many mute rules are active.
    pub fn mute_rule_count(&self) -> usize {
        self.mute_rules.len()
    }

    /// Decide whether one piece of content should be shown to this viewer.
    ///
    /// `object_id` and `text` are supplied by the caller — this crate never
    /// reads `mini-objects`/`mini-social` types directly, so it stays
    /// reusable across every content shape (a post body, a wall bio, a
    /// comment) a caller wants to filter. An author block always wins over
    /// mute rules, since blocking an identity is a stronger, whole-author
    /// statement than any one keyword.
    pub fn visibility_for(&self, author: &Did, object_id: &str, text: &str) -> Visibility {
        if self.is_blocked(author) {
            return Visibility::HiddenBlockedAuthor;
        }
        for (id, rule) in &self.mute_rules {
            if rule.matches(object_id, text) {
                return Visibility::HiddenMuted {
                    rule: MuteRuleId(*id),
                };
            }
        }
        Visibility::Visible
    }

    /// Filter an already-assembled list of `(author, object_id, text)`
    /// items down to the ones this viewer's profile does not hide,
    /// preserving order. This never mutates, reorders, or removes anything
    /// from the source the items came from — it only decides what this one
    /// call surfaces to this one viewer.
    pub fn filter<I, T>(&self, items: I, extract: impl Fn(&T) -> (&Did, &str, &str)) -> Vec<T>
    where
        I: IntoIterator<Item = T>,
    {
        items
            .into_iter()
            .filter(|item| {
                let (author, object_id, text) = extract(item);
                !self.visibility_for(author, object_id, text).is_hidden()
            })
            .collect()
    }

    /// Import another viewer's shared rule set as this viewer's own,
    /// additively. This is the *only* path by which a "community
    /// blocklist" under constitution principle 10 (an ordinary object
    /// someone published, decoded by the caller into requests) can affect
    /// this profile, and it always requires this call to be made — nothing
    /// in this crate fetches, subscribes to, or auto-applies another
    /// viewer's rules. Existing entries are left untouched; only new
    /// identities/rules are added, and the newly added rules become
    /// ordinary local entries indistinguishable from ones the viewer typed
    /// themselves (same removal path, same [`MuteRuleId`] allocation).
    /// Returns how many new blocks and new mute rules were actually added
    /// (duplicates of existing entries are not double-counted).
    pub fn import(
        &mut self,
        blocks: &[Did],
        mutes: &[MuteRule],
    ) -> Result<(usize, usize), SafetyError> {
        let mut staged = self.clone();
        let result = staged.import_inner(blocks, mutes)?;
        *self = staged;
        Ok(result)
    }

    fn import_inner(
        &mut self,
        blocks: &[Did],
        mutes: &[MuteRule],
    ) -> Result<(usize, usize), SafetyError> {
        let mut added_blocks = 0;
        for did in blocks {
            if self
                .blocked
                .insert(did.as_str().to_string(), did.clone())
                .is_none()
            {
                added_blocks += 1;
            }
        }
        let mut added_mutes = 0;
        for rule in mutes {
            let already_present = self.mute_rules.values().any(|existing| existing == rule);
            if !already_present {
                self
                    .add_mute_rule(AddMuteRuleRequest { rule: rule.clone() })?;
                    added_mutes += 1;
                }
            }
            Ok((added_blocks, added_mutes))
        }
    
        /// Serialize to plain lines a caller can write to a local file (the
        /// same convention as `mini-desktop`'s `muted.txt`, generalized). Block
        /// reasons and mute rules round-trip; `MuteRuleId` allocation order is
        /// preserved on reload.
        pub fn to_lines(&self) -> Vec<String> {
            let mut lines = Vec::with_capacity(1 + self.blocked.len() + self.mute_rules.len());
            lines.push(format!("safety\t2\t{}", self.next_rule_id));
            for (key, did) in &self.blocked {
                match self.block_reasons.get(key) {
                    Some(reason) => {
                        lines.push(format!("block\t{}\t{}", did.as_str(), escape_field(reason)));
                    }
                    None => lines.push(format!("block\t{}", did.as_str())),
                }
            }
            for (id, rule) in &self.mute_rules {
                let (kind, value) = match rule {
                    MuteRule::Keyword(value) => ("keyword", value),
                    MuteRule::ExactObject(value) => ("object", value),
                };
                lines.push(format!("mute\t{id}\t{kind}\t{}", escape_field(value)));
            }
            lines
        }
    
        /// Read versioned stable IDs or migrate the legacy value-only format.
        /// Legacy files cannot recover IDs that were already lost before this change.
        pub fn from_lines<I, S>(lines: I) -> Result<Self, SafetyError>
        where
            I: IntoIterator<Item = S>,
            S: AsRef<str>,
        {
            let lines: Vec<String> = lines
                .into_iter()
                .map(|line| line.as_ref().to_string())
                .collect();
            for line in &lines {
                if line.contains(['\r', '\n']) {
                    return Err(SafetyError::Malformed(line.clone()));
                }
            }
            let Some(first) = lines.iter().position(|line| !line.trim().is_empty()) else {
                return Ok(Self::new());
            };
            if !lines[first].starts_with("safety\t") {
                return Self::from_legacy_lines(&lines);
            }
            let malformed = |line: &str| SafetyError::Malformed(line.to_string());
            let header: Vec<&str> = lines[first].split('\t').collect();
            if header.len() != 3 || header[1] != "2" {
                return Err(malformed(&lines[first]));
            }
            let next: u64 = header[2].parse().map_err(|_| malformed(&lines[first]))?;
            if next == 0 {
                return Err(malformed(&lines[first]));
            }
            let mut profile = Self::new();
            profile.next_rule_id = next;
            for line in &lines[first + 1..] {
                if line.trim().is_empty() {
                    continue;
                }
                let fields: Vec<&str> = line.split('\t').collect();
                match fields.as_slice() {
                    ["block", scid] | ["block", scid, _] => {
                        let did = Did::parse(scid).map_err(|_| malformed(line))?;
                        if profile.is_blocked(&did) {
                            return Err(malformed(line));
                        }
                        let reason = if fields.len() == 3 {
                            Some(unescape_field(fields[2])?)
                        } else {
                            None
                        };
                        profile.block_identity(BlockIdentityRequest { blocked: did, reason });
                    }
                    ["mute", id, kind, value] => {
                        let id: u64 = id.parse().map_err(|_| malformed(line))?;
                        if id == 0 || id >= next || profile.mute_rules.contains_key(&id) {
                            return Err(malformed(line));
                        }
                        let value = unescape_field(value)?;
                        let rule = match *kind {
                            "keyword" => MuteRule::Keyword(value),
                            "object" => MuteRule::ExactObject(value),
                            _ => return Err(malformed(line)),
                        };
                        profile.mute_rules.insert(id, rule);
                    }
                    _ => return Err(malformed(line)),
                }
            }
            Ok(profile)
        }
    
        /// Parse lines produced by [`Self::to_lines`] (or hand-written in the
        /// same format) back into a profile. An unrecognized line is a hard
        /// error rather than a silent skip, so a corrupted or truncated local
        /// file is never mistaken for "nothing is blocked".
        fn from_legacy_lines<I, S>(lines: I) -> Result<Self, SafetyError>
        where
            I: IntoIterator<Item = S>,
            S: AsRef<str>,
        {
            let mut profile = Self::new();
            for raw in lines {
                let line = raw.as_ref();
                if line.trim().is_empty() {
                    continue;
                }
                let (kind, rest) = line
                    .split_once('\t')
                    .ok_or_else(|| SafetyError::Malformed(line.to_string()))?;
                match kind {
                    "block" => {
                        let mut parts = rest.splitn(2, '\t');
                        let scid = parts
                            .next()
                            .ok_or_else(|| SafetyError::Malformed(line.to_string()))?;
                        let did =
                            Did::parse(scid).map_err(|_| SafetyError::Malformed(line.to_string()))?;
                        let reason = parts.next().map(str::to_string);
                        profile.block_identity(BlockIdentityRequest {
                            blocked: did,
                            reason,
                        });
                    }
                    "mute" => {
                        let rule = MuteRule::decode(rest)?;
                        profile.add_mute_rule(AddMuteRuleRequest { rule })?;
                    }
                    _ => return Err(SafetyError::Malformed(line.to_string())),
                }
            }
            Ok(profile)
        }
    }
    
    fn escape_field(value: &str) -> String {
        let mut out = String::new();
        for c in value.chars() {
            match c {
                '\\' => out.push_str("\\\\"),
                '\t' => out.push_str("\\t"),
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                _ => out.push(c),
            }
        }
        out
    }
    
    fn unescape_field(value: &str) -> Result<String, SafetyError> {
        let mut out = String::new();
        let mut chars = value.chars();
        while let Some(c) = chars.next() {
            if c != '\\' {
                out.push(c);
                continue;
            }
            let decoded = match chars.next() {
                Some('\\') => '\\',
                Some('t') => '\t',
                Some('n') => '\n',
                Some('r') => '\r',
                _ => return Err(SafetyError::Malformed(value.to_string())),
            };
            out.push(decoded);
        }
        Ok(out)
    }
    
    #[cfg(test)]
    mod tests {
        use super::*;
    
        fn did(n: u8) -> Did {
            // Deterministic, valid did:mini identity roots for tests, via the
            // real inception path (did-mini exposes no test-only DID
            // constructor other than parsing an already-valid string).
            let controller =
                did_mini::Controller::incept_single_from_seeds(&[n; 32], &[n.wrapping_add(1); 32])
                    .expect("incept");
            controller.did()
        }
    
        #[test]
        fn fresh_profile_hides_nothing() {
            let profile = SafetyProfile::new();
            assert_eq!(
                profile.visibility_for(&did(1), "obj-1", "hello"),
                Visibility::Visible
            );
        }
    
        #[test]
        fn block_identity_hides_all_their_content_and_is_reversible() {
            let mut profile = SafetyProfile::new();
            let author = did(1);
            assert!(profile.block_identity(BlockIdentityRequest {
                blocked: author.clone(),
                reason: Some("harassment".into()),
            }));
            assert!(profile.is_blocked(&author));
            assert_eq!(profile.block_reason(&author), Some("harassment"));
            assert_eq!(
                profile.visibility_for(&author, "obj-1", "anything"),
                Visibility::HiddenBlockedAuthor
            );
    
            assert!(profile.unblock_identity(UnblockIdentityRequest {
                blocked: author.clone()
            }));
            assert!(!profile.is_blocked(&author));
            assert_eq!(
                profile.visibility_for(&author, "obj-1", "anything"),
                Visibility::Visible
            );
        }
    
        #[test]
        fn re_blocking_is_idempotent_and_updates_reason() {
            let mut profile = SafetyProfile::new();
            let author = did(2);
            assert!(profile.block_identity(BlockIdentityRequest {
                blocked: author.clone(),
                reason: None,
            }));
            assert!(!profile.block_identity(BlockIdentityRequest {
                blocked: author.clone(),
                reason: Some("spam".into()),
            }));
            assert_eq!(profile.blocked_count(), 1);
            assert_eq!(profile.block_reason(&author), Some("spam"));
        }
    
        #[test]
        fn keyword_mute_is_case_insensitive_and_removable() {
            let mut profile = SafetyProfile::new();
            let author = did(3);
            let id = profile.add_mute_rule(AddMuteRuleRequest {
                rule: MuteRule::Keyword("SpAm".into()),
            })
                    .unwrap();
        assert_eq!(
            profile.visibility_for(&author, "obj-1", "this is spam content"),
            Visibility::HiddenMuted { rule: id }
        );
        assert_eq!(
            profile.visibility_for(&author, "obj-2", "totally fine"),
            Visibility::Visible
        );
        assert!(profile.remove_mute_rule(RemoveMuteRuleRequest { rule_id: id }));
        assert_eq!(
            profile.visibility_for(&author, "obj-1", "this is spam content"),
            Visibility::Visible
        );
    }

    #[test]
    fn exact_object_mute_ignores_text() {
        let mut profile = SafetyProfile::new();
        let author = did(4);
        profile
            .add_mute_rule(AddMuteRuleRequest {
                rule: MuteRule::ExactObject("obj-target".into()),
            })
            .unwrap();
        assert!(profile
            .visibility_for(&author, "obj-target", "irrelevant text")
            .is_hidden());
        assert!(!profile
            .visibility_for(&author, "obj-other", "irrelevant text")
            .is_hidden());
    }

    #[test]
    fn blocked_author_wins_over_absence_of_mute_and_vice_versa() {
        let mut profile = SafetyProfile::new();
        let blocked_author = did(5);
        let normal_author = did(6);
        profile.block_identity(BlockIdentityRequest {
            blocked: blocked_author.clone(),
            reason: None,
        });
        profile
            .add_mute_rule(AddMuteRuleRequest {
                rule: MuteRule::Keyword("banned".into()),
            })
            .unwrap();
        assert_eq!(
            profile.visibility_for(&blocked_author, "o1", "hello"),
            Visibility::HiddenBlockedAuthor
        );
        assert!(profile
            .visibility_for(&normal_author, "o2", "banned word here")
            .is_hidden());
    }

    #[test]
    fn filter_preserves_order_and_drops_only_hidden_items() {
        let mut profile = SafetyProfile::new();
        let blocked_author = did(7);
        let ok_author = did(8);
        profile.block_identity(BlockIdentityRequest {
            blocked: blocked_author.clone(),
            reason: None,
        });

        #[derive(Clone)]
        struct Item {
            author: Did,
            id: String,
            text: String,
        }
        let items = vec![
            Item {
                author: ok_author.clone(),
                id: "a".into(),
                text: "first".into(),
            },
            Item {
                author: blocked_author.clone(),
                id: "b".into(),
                text: "second".into(),
            },
            Item {
                author: ok_author.clone(),
                id: "c".into(),
                text: "third".into(),
            },
        ];

        let visible = profile.filter(items, |item: &Item| {
            (&item.author, item.id.as_str(), item.text.as_str())
        });
        assert_eq!(visible.len(), 2);
        assert_eq!(visible[0].id, "a");
        assert_eq!(visible[1].id, "c");
    }

    #[test]
    fn round_trips_through_lines() {
        let mut profile = SafetyProfile::new();
        let author = did(9);
        profile.block_identity(BlockIdentityRequest {
            blocked: author.clone(),
            reason: Some("coordinated manipulation".into()),
        });
        profile
            .add_mute_rule(AddMuteRuleRequest {
                rule: MuteRule::Keyword("scam".into()),
            })
            .unwrap();
        profile
            .add_mute_rule(AddMuteRuleRequest {
                rule: MuteRule::ExactObject("obj-xyz".into()),
            })
            .unwrap();

        let lines = profile.to_lines();
        let restored = SafetyProfile::from_lines(&lines).expect("valid lines");
        assert_eq!(restored, profile);
    }

    #[test]
    fn from_lines_rejects_malformed_input() {
        let err = SafetyProfile::from_lines(["not-a-valid-line-at-all-no-tab"])
            .expect_err("should reject");
        assert!(matches!(err, SafetyError::Malformed(_)));

        let err = SafetyProfile::from_lines(["mystery\tvalue"]).expect_err("should reject");
        assert!(matches!(err, SafetyError::Malformed(_)));
    }

    #[test]
    fn from_lines_skips_blank_lines() {
        let profile = SafetyProfile::from_lines(["", "  ", ""]).expect("blank ok");
        assert_eq!(profile.blocked_count(), 0);
        assert_eq!(profile.mute_rule_count(), 0);
    }

    #[test]
    fn import_is_additive_opt_in_and_does_not_double_count_duplicates() {
        let mut viewer_profile = SafetyProfile::new();
        let existing_block = did(10);
        viewer_profile.block_identity(BlockIdentityRequest {
            blocked: existing_block.clone(),
            reason: None,
        });

        let shared_block = did(11);
        let shared_rule = MuteRule::Keyword("harassment-term".into());
        let (added_blocks, added_mutes) = viewer_profile
            .import(
                &[existing_block.clone(), shared_block.clone()],
                std::slice::from_ref(&shared_rule),
            )
            .unwrap();
        assert_eq!(
            added_blocks, 1,
            "the already-blocked identity is not double counted"
        );
        assert_eq!(added_mutes, 1);
        assert!(viewer_profile.is_blocked(&shared_block));
        assert_eq!(viewer_profile.mute_rule_count(), 1);

        // Importing the same rule set again adds nothing new.
        let (added_blocks_again, added_mutes_again) = viewer_profile
            .import(
                &[existing_block, shared_block],
                std::slice::from_ref(&shared_rule),
            )
            .unwrap();
        assert_eq!(added_blocks_again, 0);
        assert_eq!(added_mutes_again, 0);
        assert_eq!(viewer_profile.mute_rule_count(), 1);
    }

    #[test]
    fn mute_rule_ids_are_never_reused_after_removal() {
        let mut profile = SafetyProfile::new();
        let id1 = profile
            .add_mute_rule(AddMuteRuleRequest {
                rule: MuteRule::Keyword("a".into()),
            })
            .unwrap();
        profile.remove_mute_rule(RemoveMuteRuleRequest { rule_id: id1 });
        let id2 = profile
            .add_mute_rule(AddMuteRuleRequest {
                rule: MuteRule::Keyword("b".into()),
            })
            .unwrap();
        assert_ne!(id1, id2);
        assert_eq!(id2.value(), 2);
    }

    #[test]
    fn default_and_restart_preserve_rule_identifiers() {
        let mut profile = SafetyProfile::default();
        assert_eq!(profile, SafetyProfile::new());
        let first = profile
            .add_mute_rule(AddMuteRuleRequest {
                rule: MuteRule::Keyword("first".into()),
            })
            .unwrap();
        let second = profile
            .add_mute_rule(AddMuteRuleRequest {
                rule: MuteRule::Keyword("second".into()),
            })
            .unwrap();
        assert_eq!(first.value(), 1);
        profile.remove_mute_rule(RemoveMuteRuleRequest { rule_id: first });
        let file = profile.to_lines().join("\n");
        let mut restored = SafetyProfile::from_lines(file.lines()).unwrap();
        assert_eq!(restored, profile);
        assert_eq!(
            restored.visibility_for(&did(12), "obj", "second"),
            Visibility::HiddenMuted { rule: second }
        );
        assert!(!restored.remove_mute_rule(RemoveMuteRuleRequest { rule_id: first }));
        let next = restored
            .add_mute_rule(AddMuteRuleRequest {
                rule: MuteRule::Keyword("third".into()),
            })
            .unwrap();
        assert_eq!(next.value(), 3);
    }

    #[test]
    fn escaped_fields_cannot_inject_persisted_rules() {
        let mut profile = SafetyProfile::new();
        profile.block_identity(BlockIdentityRequest {
            blocked: did(13),
            reason: Some("note\nblock\tother\r\\literal".into()),
        });
        profile
            .add_mute_rule(AddMuteRuleRequest {
                rule: MuteRule::Keyword("spam\nmute\tobject\tinjected\\end".into()),
            })
            .unwrap();
        let lines = profile.to_lines();
        assert!(lines.iter().all(|line| !line.contains(['\n', '\r'])));
        assert_eq!(
            SafetyProfile::from_lines(lines.join("\n").lines()).unwrap(),
            profile
        );
    }

    #[test]
    fn legacy_backslashes_are_not_reinterpreted_as_escapes() {
        let profile = SafetyProfile::from_lines(["mute\tkeyword\tC:\\new\\test"]).unwrap();
        assert_eq!(
            profile.mute_rules().next().unwrap().1,
            &MuteRule::Keyword("C:\\new\\test".into())
        );
        assert_eq!(
            SafetyProfile::from_lines(profile.to_lines()).unwrap(),
            profile
        );
    }

    #[test]
    fn versioned_rules_reject_ambiguous_or_corrupt_ids() {
        for lines in [
            vec!["safety\t2\t3", "mute\t1\tkeyword\ta", "mute\t1\tobject\tb"],
            vec!["safety\t2\t2", "mute\t2\tkeyword\ta"],
            vec!["safety\t2\t2", "mute\t0\tkeyword\ta"],
            vec!["safety\t2\t0"],
            vec!["safety\t3\t1"],
            vec!["safety\t2\t2", "mute\t1\tkeyword\tbad\\q"],
            vec!["safety\t2\t2", "mute\t1\tkeyword\tbad\nmute\tobject\tinjected"],
        ] {
            assert!(SafetyProfile::from_lines(lines).is_err());
        }
    }

    #[test]
    fn exhausted_ids_fail_without_partial_import_or_rule_mutation() {
        let header = format!("safety\t2\t{}", u64::MAX);
        let mut profile = SafetyProfile::from_lines([header]).unwrap();
        let before = profile.clone();
        let rule = MuteRule::Keyword("new".into());
        assert_eq!(
            profile.add_mute_rule(AddMuteRuleRequest { rule: rule.clone() }),
            Err(SafetyError::RuleIdsExhausted)
        );
        assert_eq!(profile, before);
        assert_eq!(
            profile.import(&[did(14)], &[rule]),
            Err(SafetyError::RuleIdsExhausted)
        );
        assert_eq!(profile, before);
    }
}
