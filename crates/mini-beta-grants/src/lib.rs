//! Multi-party acceptance evidence for Beta MINI grants.
//!
//! This crate closes the deliberate gap between a signed grant proposal and a
//! grant that a shared beta network is willing to apply. It is Beta-only: it
//! has no production MINI, treasury, consensus, personhood, or governance
//! dependency. A balance or reward can never become authorization weight here.
//!
//! The current Pre-Go-Live model still has one explicit setup dependency: the
//! beta campaign record author selects a short-lived campaign policy. That
//! author cannot issue alone because every valid policy requires at least three
//! distinct authorization DIDs and thresholds of at least two. Distinct DIDs
//! are not proof of distinct humans; mature selection/rotation belongs to the
//! Forge-canonical transition rather than being faked in this crate.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

use std::collections::{HashMap, HashSet};

use did_mini::{Controller, Did};
use mini_beta::{
    read_campaign, read_contribution_receipt, read_grant_authorization, BetaAccountId, BetaEpochId,
    BetaError, BetaMiniLedger, BetaMiniPolicy, GrantClass, DEFAULT_MAX_PARTICIPATION_GRANT,
    DEFAULT_MAX_TESTING_GRANT,
};
use mini_objects::{Object, ObjectBuilder, ObjectError, ObjectId, ObjectType, Payload};
use mini_store::{Backend, Store, StoreError};

/// Signed campaign-scoped authorization-policy object.
pub const BETA_GRANT_POLICY_TYPE: &str = "mininet.beta/grant-policy/v1";
/// Signed exact-grant approval object.
pub const BETA_GRANT_APPROVAL_TYPE: &str = "mininet.beta/grant-approval/v1";

const PAYLOAD_VERSION: u8 = 1;
const MAX_AUTHORIZERS: usize = 32;
const MIN_AUTHORIZERS: usize = 3;
const MAX_REWARD_BANDS: usize = 16;
const MAX_APPROVAL_EVIDENCE: usize = 128;

/// Result type for the Beta grant-acceptance layer.
pub type Result<T> = core::result::Result<T, GrantAcceptanceError>;

/// Fail-closed validation errors for grant policy/approval evidence.
#[derive(Debug)]
#[non_exhaustive]
pub enum GrantAcceptanceError {
    /// Object type, links, payload, or field encoding was not canonical.
    InvalidObject,
    /// Policy semantics violate the campaign-scoped multi-party rules.
    InvalidPolicy,
    /// A policy was not signed by the campaign's record authority.
    PolicyAuthorMismatch,
    /// More than one valid policy exists for the same campaign. Fail closed; do not invent a local tie-break.
    PolicyConflict,
    /// A grant did not satisfy deterministic policy rules.
    GrantRuleViolation,
    /// Approval evidence did not bind the requested policy/grant or signer.
    InvalidApproval,
    /// An authorization set did not reach the class-specific threshold.
    ThresholdNotMet { required: u16, observed: u16 },
    /// Too much caller-supplied evidence was provided.
    EvidenceLimit,
    /// Underlying Beta object/accounting validation failed.
    Beta(BetaError),
    /// Object store access failed.
    Store(StoreError),
    /// Signed-object construction failed.
    Object(ObjectError),
}

impl core::fmt::Display for GrantAcceptanceError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidObject => write!(f, "invalid Beta grant acceptance object"),
            Self::InvalidPolicy => write!(f, "invalid Beta grant policy"),
            Self::PolicyAuthorMismatch => {
                write!(
                    f,
                    "Beta grant policy author is not the campaign record author"
                )
            }
            Self::PolicyConflict => write!(
                f,
                "multiple valid Beta grant policies exist for one campaign"
            ),
            Self::GrantRuleViolation => write!(f, "Beta grant violates deterministic policy"),
            Self::InvalidApproval => write!(f, "invalid Beta grant approval evidence"),
            Self::ThresholdNotMet { required, observed } => write!(
                f,
                "Beta grant threshold not met: required {required}, observed {observed}"
            ),
            Self::EvidenceLimit => write!(f, "too much Beta grant approval evidence"),
            Self::Beta(e) => write!(f, "beta: {e}"),
            Self::Store(e) => write!(f, "store: {e}"),
            Self::Object(e) => write!(f, "object: {e}"),
        }
    }
}

impl std::error::Error for GrantAcceptanceError {}

impl From<BetaError> for GrantAcceptanceError {
    fn from(value: BetaError) -> Self {
        Self::Beta(value)
    }
}

impl From<StoreError> for GrantAcceptanceError {
    fn from(value: StoreError) -> Self {
        Self::Store(value)
    }
}

impl From<ObjectError> for GrantAcceptanceError {
    fn from(value: ObjectError) -> Self {
        Self::Object(value)
    }
}

/// One immutable, campaign-scoped set of operational grant authorizers.
///
/// Membership is deliberately unweighted. A DID appearing in this vector has
/// one approval unit regardless of wealth, balance, rewards, employer,
/// contribution count, or political status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BetaGrantPolicy {
    /// Content-addressed policy id.
    pub id: ObjectId,
    /// Campaign record authority that published this policy.
    pub record_author: Did,
    /// Exact beta campaign governed by this policy.
    pub campaign_id: ObjectId,
    /// Resettable beta epoch.
    pub epoch: BetaEpochId,
    /// Canonically sorted unique operational authorization DIDs.
    pub members: Vec<Did>,
    /// Distinct members required for a testing grant.
    pub testing_threshold: u16,
    /// Distinct members required for a participation grant.
    pub participation_threshold: u16,
    /// Exact amount for each testing grant under this policy.
    pub testing_grant_amount: u64,
    /// Canonically increasing allowed participation reward amounts.
    pub participation_reward_bands: Vec<u64>,
    /// First timestamp at which grants/approvals may use this policy.
    pub valid_from_ms: u64,
    /// Last timestamp at which grants/approvals may use this policy.
    pub valid_until_ms: u64,
    /// Object timestamp.
    pub timestamp_ms: u64,
    /// Signer-scoped sequence.
    pub sequence: u64,
}

/// One signed approval of one exact grant under one exact policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BetaGrantApproval {
    /// Content-addressed approval id.
    pub id: ObjectId,
    /// Operational authorization DID that signed the approval.
    pub record_author: Did,
    /// Exact policy being used.
    pub policy_id: ObjectId,
    /// Exact grant being approved.
    pub grant_id: ObjectId,
    /// Object timestamp.
    pub timestamp_ms: u64,
    /// Signer-scoped sequence.
    pub sequence: u64,
}

/// Deterministic proof that one exact grant reached its configured threshold.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrantAcceptance {
    pub policy_id: ObjectId,
    pub grant_id: ObjectId,
    pub class: GrantClass,
    pub required_approvals: u16,
    pub distinct_approvers: u16,
}

/// Evidence that one operational authorizer signed two grants competing for the
/// same reward scope. Detection is evidence only; automatic blacklisting would
/// itself be an authority decision and is deliberately not implemented here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizerEquivocation {
    pub authorizer: Did,
    pub first_grant_id: ObjectId,
    pub second_grant_id: ObjectId,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum GrantScope {
    Testing(BetaAccountId),
    Participation(ObjectId),
}

/// Create and store a campaign-scoped Beta grant policy.
#[allow(clippy::too_many_arguments)]
pub fn create_grant_policy<B: Backend>(
    store: &mut Store<B>,
    record_human: &Did,
    record_device: &Controller,
    campaign_id: &ObjectId,
    members: &[Did],
    testing_threshold: u16,
    participation_threshold: u16,
    testing_grant_amount: u64,
    participation_reward_bands: &[u64],
    valid_from_ms: u64,
    valid_until_ms: u64,
    timestamp_ms: u64,
    sequence: u64,
) -> Result<Object> {
    let campaign = read_campaign(store, campaign_id)?;
    if record_human != &campaign.record_author {
        return Err(GrantAcceptanceError::PolicyAuthorMismatch);
    }

    let mut canonical_members = members.to_vec();
    canonical_members.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    if canonical_members.windows(2).any(|w| w[0] == w[1]) {
        return Err(GrantAcceptanceError::InvalidPolicy);
    }

    let mut canonical_bands = participation_reward_bands.to_vec();
    canonical_bands.sort_unstable();
    if canonical_bands.windows(2).any(|w| w[0] == w[1]) {
        return Err(GrantAcceptanceError::InvalidPolicy);
    }

    validate_policy_fields(
        &campaign,
        &canonical_members,
        testing_threshold,
        participation_threshold,
        testing_grant_amount,
        &canonical_bands,
        valid_from_ms,
        valid_until_ms,
        timestamp_ms,
    )?;

    let mut payload = vec![PAYLOAD_VERSION];
    payload.extend_from_slice(campaign.epoch.as_bytes());
    put_u64(&mut payload, valid_from_ms);
    put_u64(&mut payload, valid_until_ms);
    put_u16(&mut payload, testing_threshold);
    put_u16(&mut payload, participation_threshold);
    put_u64(&mut payload, testing_grant_amount);
    put_u16(&mut payload, canonical_members.len() as u16);
    for member in &canonical_members {
        put_str(&mut payload, member.as_str());
    }
    put_u16(&mut payload, canonical_bands.len() as u16);
    for band in &canonical_bands {
        put_u64(&mut payload, *band);
    }

    let object = ObjectBuilder::new(ObjectType::Custom(BETA_GRANT_POLICY_TYPE.to_string()))
        .timestamp_ms(timestamp_ms)
        .sequence(sequence)
        .payload(Payload::Public(payload))
        .link("campaign", campaign_id.clone())
        .sign(record_human, record_device)?;
    store.insert(&object)?;
    Ok(object)
}

/// Create and store one exact-grant approval from a policy member.
pub fn create_grant_approval<B: Backend>(
    store: &mut Store<B>,
    authorizer_human: &Did,
    authorizer_device: &Controller,
    policy_id: &ObjectId,
    grant_id: &ObjectId,
    timestamp_ms: u64,
    sequence: u64,
) -> Result<Object> {
    let policy = validate_policy(store, policy_id)?;
    let grant = validate_grant_against_policy(store, &policy, grant_id)?;
    if !policy
        .members
        .iter()
        .any(|member| member == authorizer_human)
    {
        return Err(GrantAcceptanceError::InvalidApproval);
    }
    if timestamp_ms < grant.timestamp_ms
        || timestamp_ms < policy.valid_from_ms
        || timestamp_ms > policy.valid_until_ms
    {
        return Err(GrantAcceptanceError::InvalidApproval);
    }

    let object = ObjectBuilder::new(ObjectType::Custom(BETA_GRANT_APPROVAL_TYPE.to_string()))
        .timestamp_ms(timestamp_ms)
        .sequence(sequence)
        .payload(Payload::Public(vec![PAYLOAD_VERSION]))
        .link("policy", policy_id.clone())
        .link("grant", grant_id.clone())
        .sign(authorizer_human, authorizer_device)?;
    store.insert(&object)?;
    Ok(object)
}

/// Strictly parse a policy object without consulting its linked campaign.
pub fn parse_grant_policy_object(object: &Object) -> Result<BetaGrantPolicy> {
    ensure_type_and_links(object, BETA_GRANT_POLICY_TYPE, &["campaign"])?;
    let bytes = public_payload(object)?;
    let mut off = 0usize;
    if take_u8(bytes, &mut off)? != PAYLOAD_VERSION {
        return Err(GrantAcceptanceError::InvalidObject);
    }
    let epoch = BetaEpochId::new(take_32(bytes, &mut off)?).map_err(GrantAcceptanceError::Beta)?;
    let valid_from_ms = take_u64(bytes, &mut off)?;
    let valid_until_ms = take_u64(bytes, &mut off)?;
    let testing_threshold = take_u16(bytes, &mut off)?;
    let participation_threshold = take_u16(bytes, &mut off)?;
    let testing_grant_amount = take_u64(bytes, &mut off)?;
    let member_count = take_u16(bytes, &mut off)? as usize;
    if !(MIN_AUTHORIZERS..=MAX_AUTHORIZERS).contains(&member_count) {
        return Err(GrantAcceptanceError::InvalidPolicy);
    }
    let mut members = Vec::with_capacity(member_count);
    for _ in 0..member_count {
        let raw = take_str(bytes, &mut off, did_mini::MAX_DID_BYTES)?;
        let did = Did::parse(&raw).map_err(|_| GrantAcceptanceError::InvalidObject)?;
        members.push(did);
    }
    if members.windows(2).any(|w| w[0].as_str() >= w[1].as_str()) {
        return Err(GrantAcceptanceError::InvalidPolicy);
    }

    let band_count = take_u16(bytes, &mut off)? as usize;
    if band_count == 0 || band_count > MAX_REWARD_BANDS {
        return Err(GrantAcceptanceError::InvalidPolicy);
    }
    let mut bands = Vec::with_capacity(band_count);
    for _ in 0..band_count {
        bands.push(take_u64(bytes, &mut off)?);
    }
    if bands.windows(2).any(|w| w[0] >= w[1]) || off != bytes.len() {
        return Err(GrantAcceptanceError::InvalidPolicy);
    }

    Ok(BetaGrantPolicy {
        id: object.id().clone(),
        record_author: object.author_human.clone(),
        campaign_id: required_link(object, "campaign")?,
        epoch,
        members,
        testing_threshold,
        participation_threshold,
        testing_grant_amount,
        participation_reward_bands: bands,
        valid_from_ms,
        valid_until_ms,
        timestamp_ms: object.timestamp_ms,
        sequence: object.sequence,
    })
}

/// Strictly parse a grant-approval object.
pub fn parse_grant_approval_object(object: &Object) -> Result<BetaGrantApproval> {
    ensure_type_and_links(object, BETA_GRANT_APPROVAL_TYPE, &["policy", "grant"])?;
    let bytes = public_payload(object)?;
    if bytes != [PAYLOAD_VERSION] {
        return Err(GrantAcceptanceError::InvalidObject);
    }
    Ok(BetaGrantApproval {
        id: object.id().clone(),
        record_author: object.author_human.clone(),
        policy_id: required_link(object, "policy")?,
        grant_id: required_link(object, "grant")?,
        timestamp_ms: object.timestamp_ms,
        sequence: object.sequence,
    })
}

/// Strictly read and fully validate a policy against its campaign.
pub fn validate_policy<B: Backend>(
    store: &Store<B>,
    policy_id: &ObjectId,
) -> Result<BetaGrantPolicy> {
    let policy = parse_grant_policy_object(&store.get(policy_id)?)?;
    let campaign = read_campaign(store, &policy.campaign_id)?;
    if policy.record_author != campaign.record_author {
        return Err(GrantAcceptanceError::PolicyAuthorMismatch);
    }
    if policy.epoch != campaign.epoch {
        return Err(GrantAcceptanceError::InvalidPolicy);
    }
    validate_policy_fields(
        &campaign,
        &policy.members,
        policy.testing_threshold,
        policy.participation_threshold,
        policy.testing_grant_amount,
        &policy.participation_reward_bands,
        policy.valid_from_ms,
        policy.valid_until_ms,
        policy.timestamp_ms,
    )?;
    Ok(policy)
}

/// Strictly read a signed approval object.
pub fn read_grant_approval<B: Backend>(
    store: &Store<B>,
    approval_id: &ObjectId,
) -> Result<BetaGrantApproval> {
    parse_grant_approval_object(&store.get(approval_id)?)
}

/// Validate one exact grant against policy amounts, campaign, epoch and time.
pub fn validate_grant_against_policy<B: Backend>(
    store: &Store<B>,
    policy: &BetaGrantPolicy,
    grant_id: &ObjectId,
) -> Result<mini_beta::BetaGrantAuthorization> {
    let grant = read_grant_authorization(store, grant_id)?;
    if grant.campaign_id != policy.campaign_id
        || grant.epoch != policy.epoch
        || grant.timestamp_ms < policy.valid_from_ms
        || grant.timestamp_ms > policy.valid_until_ms
    {
        return Err(GrantAcceptanceError::GrantRuleViolation);
    }

    match grant.class {
        GrantClass::Testing => {
            if grant.contribution_id.is_some() || grant.amount != policy.testing_grant_amount {
                return Err(GrantAcceptanceError::GrantRuleViolation);
            }
        }
        GrantClass::Participation => {
            if !policy.participation_reward_bands.contains(&grant.amount) {
                return Err(GrantAcceptanceError::GrantRuleViolation);
            }
            let contribution_id = grant
                .contribution_id
                .as_ref()
                .ok_or(GrantAcceptanceError::GrantRuleViolation)?;
            let contribution = read_contribution_receipt(store, contribution_id)?;
            if contribution.campaign_id.as_ref() != Some(&policy.campaign_id) {
                return Err(GrantAcceptanceError::GrantRuleViolation);
            }
        }
    }
    Ok(grant)
}

/// Resolve the only valid grant policy for one campaign.  Competing valid
/// policies fail closed: eventual replication must never cause one node to
/// pick a local winner by timestamp, object id, arrival order, or repository.
pub fn resolve_unique_campaign_policy<B: Backend>(
    store: &Store<B>,
    campaign_id: &ObjectId,
) -> Result<BetaGrantPolicy> {
    let campaign = read_campaign(store, campaign_id)?;
    let ids = store.by_type(&ObjectType::Custom(BETA_GRANT_POLICY_TYPE.to_string()))?;
    let mut found: Option<BetaGrantPolicy> = None;
    for id in ids {
        let object = store.get(&id)?;
        let candidate = match parse_grant_policy_object(&object) {
            Ok(candidate) => candidate,
            // Any DID can publish an object with this custom type. Malformed
            // third-party policy-shaped noise must not gain veto power merely
            // by sharing the type index.
            Err(_) => continue,
        };
        if &candidate.campaign_id != campaign_id
            || candidate.record_author != campaign.record_author
        {
            continue;
        }
        let candidate = match validate_policy(store, &id) {
            Ok(candidate) => candidate,
            // Only fully valid policies by the campaign record authority are
            // candidates for the uniqueness rule. Invalid objects do not become
            // a second policy and therefore cannot manufacture PolicyConflict.
            Err(GrantAcceptanceError::InvalidPolicy)
            | Err(GrantAcceptanceError::PolicyAuthorMismatch)
            | Err(GrantAcceptanceError::InvalidObject) => continue,
            Err(error) => return Err(error),
        };
        if found.is_some() {
            return Err(GrantAcceptanceError::PolicyConflict);
        }
        found = Some(candidate);
    }
    found.ok_or(GrantAcceptanceError::InvalidPolicy)
}

/// Deterministically validate threshold evidence for one exact grant.
///
/// Approval order is irrelevant and repeated approvals from one DID count once.
pub fn validate_grant_acceptance<B: Backend>(
    store: &Store<B>,
    policy_id: &ObjectId,
    grant_id: &ObjectId,
    approval_ids: &[ObjectId],
) -> Result<GrantAcceptance> {
    if approval_ids.len() > MAX_APPROVAL_EVIDENCE {
        return Err(GrantAcceptanceError::EvidenceLimit);
    }
    let policy = validate_policy(store, policy_id)?;
    let unique = resolve_unique_campaign_policy(store, &policy.campaign_id)?;
    if unique.id != *policy_id {
        return Err(GrantAcceptanceError::PolicyConflict);
    }
    let grant = validate_grant_against_policy(store, &policy, grant_id)?;
    let required = match grant.class {
        GrantClass::Testing => policy.testing_threshold,
        GrantClass::Participation => policy.participation_threshold,
    };

    let mut distinct = HashSet::<Did>::new();
    for approval_id in approval_ids {
        let approval = read_grant_approval(store, approval_id)?;
        if approval.policy_id != *policy_id || approval.grant_id != *grant_id {
            return Err(GrantAcceptanceError::InvalidApproval);
        }
        if !policy
            .members
            .iter()
            .any(|member| member == &approval.record_author)
            || approval.timestamp_ms < grant.timestamp_ms
            || approval.timestamp_ms < policy.valid_from_ms
            || approval.timestamp_ms > policy.valid_until_ms
        {
            return Err(GrantAcceptanceError::InvalidApproval);
        }
        distinct.insert(approval.record_author);
    }

    let observed = distinct.len() as u16;
    if observed < required {
        return Err(GrantAcceptanceError::ThresholdNotMet { required, observed });
    }

    Ok(GrantAcceptance {
        policy_id: policy.id,
        grant_id: grant.id,
        class: grant.class,
        required_approvals: required,
        distinct_approvers: observed,
    })
}

/// Detect signed authorizer equivocation within one policy's observed evidence.
///
/// For participation, the conflict scope is one contribution receipt. For
/// testing, it is one opaque beta account. The result is deterministic after
/// the same immutable objects have replicated to each node.
pub fn detect_authorizer_equivocations<B: Backend>(
    store: &Store<B>,
    policy_id: &ObjectId,
    approval_ids: &[ObjectId],
) -> Result<Vec<AuthorizerEquivocation>> {
    if approval_ids.len() > MAX_APPROVAL_EVIDENCE {
        return Err(GrantAcceptanceError::EvidenceLimit);
    }
    let policy = validate_policy(store, policy_id)?;
    // All distinct grant ids seen per (authorizer, scope), not just the
    // first one -- see the fix note below for why.
    let mut seen: HashMap<(Did, GrantScope), Vec<ObjectId>> = HashMap::new();

    for approval_id in approval_ids {
        let approval = read_grant_approval(store, approval_id)?;
        if approval.policy_id != *policy_id
            || !policy
                .members
                .iter()
                .any(|member| member == &approval.record_author)
        {
            return Err(GrantAcceptanceError::InvalidApproval);
        }
        let grant = validate_grant_against_policy(store, &policy, &approval.grant_id)?;
        if approval.timestamp_ms < grant.timestamp_ms
            || approval.timestamp_ms < policy.valid_from_ms
            || approval.timestamp_ms > policy.valid_until_ms
        {
            return Err(GrantAcceptanceError::InvalidApproval);
        }
        let scope = match grant.class {
            GrantClass::Testing => GrantScope::Testing(grant.account),
            GrantClass::Participation => GrantScope::Participation(
                grant
                    .contribution_id
                    .clone()
                    .ok_or(GrantAcceptanceError::GrantRuleViolation)?,
            ),
        };
        let key = (approval.record_author.clone(), scope);
        let grant_ids = seen.entry(key).or_default();
        if !grant_ids.contains(&approval.grant_id) {
            grant_ids.push(approval.grant_id.clone());
        }
    }

    // Emit every pairwise conflict among the distinct grant ids observed per
    // key, rather than only pairs against whichever grant happened to be
    // seen first. Recording only first-seen-vs-rest made the result depend
    // on `approval_ids`' order: with three mutually conflicting approvals,
    // processing them in a different order (e.g. because gossip delivered
    // them to two honest nodes in different sequence) could report a
    // different *set* of conflicting pairs, contradicting this function's
    // own documented guarantee that the result is deterministic once the
    // same immutable objects have replicated. Since every grant id in
    // `grant_ids` is already known to be mutually distinct, and the whole
    // point of equivocation evidence is "this authorizer signed more than
    // one grant for the same scope," reporting all pairs is both the
    // order-independent answer and the more complete evidence.
    let mut conflicts = Vec::new();
    for ((authorizer, _scope), mut grant_ids) in seen {
        if grant_ids.len() < 2 {
            continue;
        }
        grant_ids.sort_by(|a, b| a.as_str().cmp(b.as_str()));
        for i in 0..grant_ids.len() {
            for j in (i + 1)..grant_ids.len() {
                conflicts.push(AuthorizerEquivocation {
                    authorizer: authorizer.clone(),
                    first_grant_id: grant_ids[i].clone(),
                    second_grant_id: grant_ids[j].clone(),
                });
            }
        }
    }

    conflicts.sort_by(|a, b| {
        a.authorizer
            .as_str()
            .cmp(b.authorizer.as_str())
            .then_with(|| a.first_grant_id.as_str().cmp(b.first_grant_id.as_str()))
            .then_with(|| a.second_grant_id.as_str().cmp(b.second_grant_id.as_str()))
    });
    Ok(conflicts)
}

/// Shared-beta accounting wrapper: minting can enter the core ledger only after
/// exact threshold evidence validates. Transfers remain ordinary accounting and
/// never create authorization weight.
#[derive(Debug)]
pub struct SharedBetaLedger {
    inner: BetaMiniLedger,
}

impl SharedBetaLedger {
    pub fn new(epoch: BetaEpochId, policy: BetaMiniPolicy) -> Result<Self> {
        Ok(Self {
            inner: BetaMiniLedger::new(epoch, policy)?,
        })
    }

    pub fn epoch(&self) -> BetaEpochId {
        self.inner.epoch()
    }

    pub fn total_issued(&self) -> u64 {
        self.inner.total_issued()
    }

    pub fn balance(&self, account: &BetaAccountId) -> u64 {
        self.inner.balance(account)
    }

    pub fn transfer(
        &mut self,
        from: &BetaAccountId,
        to: &BetaAccountId,
        amount: u64,
    ) -> Result<()> {
        self.inner.transfer(from, to, amount)?;
        Ok(())
    }

    /// Apply one grant only after its exact policy/approval evidence reaches the
    /// class-specific threshold.
    pub fn apply_accepted_grant<B: Backend>(
        &mut self,
        store: &Store<B>,
        policy_id: &ObjectId,
        grant_id: &ObjectId,
        approval_ids: &[ObjectId],
    ) -> Result<GrantAcceptance> {
        let acceptance = validate_grant_acceptance(store, policy_id, grant_id, approval_ids)?;
        self.inner.apply_grant(store, grant_id)?;
        Ok(acceptance)
    }

    /// Start another resettable beta epoch with zero balances and issuance.
    pub fn rollover(&self, new_epoch: BetaEpochId) -> Result<Self> {
        Ok(Self {
            inner: self.inner.rollover(new_epoch)?,
        })
    }
}

#[allow(clippy::too_many_arguments)]
fn validate_policy_fields(
    campaign: &mini_beta::BetaCampaign,
    members: &[Did],
    testing_threshold: u16,
    participation_threshold: u16,
    testing_grant_amount: u64,
    participation_reward_bands: &[u64],
    valid_from_ms: u64,
    valid_until_ms: u64,
    policy_timestamp_ms: u64,
) -> Result<()> {
    if !(MIN_AUTHORIZERS..=MAX_AUTHORIZERS).contains(&members.len())
        || members.windows(2).any(|w| w[0].as_str() >= w[1].as_str())
        || testing_threshold < 2
        || participation_threshold < testing_threshold
        || usize::from(testing_threshold) > members.len()
        || usize::from(participation_threshold) > members.len()
        || testing_grant_amount == 0
        || testing_grant_amount > campaign.default_testing_grant
        || testing_grant_amount > DEFAULT_MAX_TESTING_GRANT
        || participation_reward_bands.is_empty()
        || participation_reward_bands.len() > MAX_REWARD_BANDS
        || participation_reward_bands[0] == 0
        || participation_reward_bands
            .iter()
            .any(|band| *band > DEFAULT_MAX_PARTICIPATION_GRANT)
        || participation_reward_bands.windows(2).any(|w| w[0] >= w[1])
        || valid_from_ms < campaign.starts_ms
        || valid_until_ms > campaign.ends_ms
        || valid_from_ms >= valid_until_ms
        || policy_timestamp_ms > valid_from_ms
    {
        return Err(GrantAcceptanceError::InvalidPolicy);
    }
    Ok(())
}

fn ensure_type_and_links(object: &Object, expected_type: &str, relations: &[&str]) -> Result<()> {
    match &object.object_type {
        ObjectType::Custom(value) if value == expected_type => {}
        _ => return Err(GrantAcceptanceError::InvalidObject),
    }
    if object.links.len() != relations.len() {
        return Err(GrantAcceptanceError::InvalidObject);
    }
    for relation in relations {
        if object
            .links
            .iter()
            .filter(|link| link.rel == *relation)
            .count()
            != 1
        {
            return Err(GrantAcceptanceError::InvalidObject);
        }
    }
    Ok(())
}

fn required_link(object: &Object, relation: &str) -> Result<ObjectId> {
    object
        .links
        .iter()
        .find(|link| link.rel == relation)
        .map(|link| link.target.clone())
        .ok_or(GrantAcceptanceError::InvalidObject)
}

fn public_payload(object: &Object) -> Result<&[u8]> {
    match &object.payload {
        Payload::Public(bytes) => Ok(bytes),
        Payload::Encrypted(_) => Err(GrantAcceptanceError::InvalidObject),
    }
}

fn put_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_be_bytes());
}

fn put_u64(out: &mut Vec<u8>, value: u64) {
    out.extend_from_slice(&value.to_be_bytes());
}

fn put_str(out: &mut Vec<u8>, value: &str) {
    out.extend_from_slice(&(value.len() as u32).to_be_bytes());
    out.extend_from_slice(value.as_bytes());
}

fn take_u8(bytes: &[u8], off: &mut usize) -> Result<u8> {
    let value = *bytes.get(*off).ok_or(GrantAcceptanceError::InvalidObject)?;
    *off += 1;
    Ok(value)
}

fn take_u16(bytes: &[u8], off: &mut usize) -> Result<u16> {
    let end = off
        .checked_add(2)
        .ok_or(GrantAcceptanceError::InvalidObject)?;
    let raw = bytes
        .get(*off..end)
        .ok_or(GrantAcceptanceError::InvalidObject)?;
    *off = end;
    Ok(u16::from_be_bytes(
        raw.try_into()
            .map_err(|_| GrantAcceptanceError::InvalidObject)?,
    ))
}

fn take_u64(bytes: &[u8], off: &mut usize) -> Result<u64> {
    let end = off
        .checked_add(8)
        .ok_or(GrantAcceptanceError::InvalidObject)?;
    let raw = bytes
        .get(*off..end)
        .ok_or(GrantAcceptanceError::InvalidObject)?;
    *off = end;
    Ok(u64::from_be_bytes(
        raw.try_into()
            .map_err(|_| GrantAcceptanceError::InvalidObject)?,
    ))
}

fn take_32(bytes: &[u8], off: &mut usize) -> Result<[u8; 32]> {
    let end = off
        .checked_add(32)
        .ok_or(GrantAcceptanceError::InvalidObject)?;
    let raw = bytes
        .get(*off..end)
        .ok_or(GrantAcceptanceError::InvalidObject)?;
    *off = end;
    raw.try_into()
        .map_err(|_| GrantAcceptanceError::InvalidObject)
}

fn take_str(bytes: &[u8], off: &mut usize, max: usize) -> Result<String> {
    let len_end = off
        .checked_add(4)
        .ok_or(GrantAcceptanceError::InvalidObject)?;
    let len_bytes = bytes
        .get(*off..len_end)
        .ok_or(GrantAcceptanceError::InvalidObject)?;
    let len = u32::from_be_bytes(
        len_bytes
            .try_into()
            .map_err(|_| GrantAcceptanceError::InvalidObject)?,
    ) as usize;
    *off = len_end;
    if len == 0 || len > max {
        return Err(GrantAcceptanceError::InvalidObject);
    }
    let end = off
        .checked_add(len)
        .ok_or(GrantAcceptanceError::InvalidObject)?;
    let raw = bytes
        .get(*off..end)
        .ok_or(GrantAcceptanceError::InvalidObject)?;
    *off = end;
    core::str::from_utf8(raw)
        .map(str::to_string)
        .map_err(|_| GrantAcceptanceError::InvalidObject)
}
