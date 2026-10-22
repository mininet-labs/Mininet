//! Durable, authenticated and fail-closed execution for resettable Beta MINI.
//!
//! The immutable signed object set is the durable state. This crate derives a
//! provisional UTXO-style snapshot from that set; it never treats a mutable
//! balance database, arrival order, timestamp, object hash ordering, GitHub, or
//! a server as monetary authority. Two otherwise valid spends of one output
//! invalidate all conflicting consumers rather than electing a local winner.
//!
//! This is test-value infrastructure. Late evidence can change a provisional
//! snapshot, so nothing here claims production finality or production-MINI
//! convertibility.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

use std::collections::{HashMap, HashSet};

use did_mini::{Controller, Did};
use mini_beta::{
    read_campaign, read_grant_authorization, BetaAccountId, BetaEpochId, BetaError,
    BetaGrantAuthorization, BetaMiniPolicy, GrantClass, BETA_GRANT_TYPE,
};
use mini_beta_grants::{
    parse_grant_approval_object, resolve_unique_campaign_policy, validate_grant_acceptance,
    BetaGrantPolicy, GrantAcceptanceError, BETA_GRANT_APPROVAL_TYPE,
};
use mini_objects::{Object, ObjectBuilder, ObjectError, ObjectId, ObjectType, Payload};
use mini_store::{Backend, Store, StoreError};

/// Signed Beta account registration.
pub const BETA_ACCOUNT_TYPE: &str = "mininet.beta/account/v1";
/// Signed durable Beta transfer.
pub const BETA_TRANSFER_TYPE: &str = "mininet.beta/transfer/v1";

const ACCOUNT_DOMAIN: &[u8] = b"mininet/beta/account/v1";
const SNAPSHOT_DOMAIN: &[u8] = b"mininet/beta/snapshot/v1";
const PAYLOAD_VERSION: u8 = 1;
const MAX_TRANSFER_INPUTS: usize = 64;
const MAX_TRANSFER_OUTPUTS: usize = 64;
const MAX_MEMO_BYTES: usize = 512;
const MAX_OBJECT_ID_BYTES: usize = 256;

/// Durable Beta execution result type.
pub type Result<T> = core::result::Result<T, BetaExecError>;

/// Fail-closed errors from Beta account/transfer parsing or snapshot derivation.
#[derive(Debug)]
#[non_exhaustive]
pub enum BetaExecError {
    InvalidObject,
    InvalidAccount,
    AccountConflict,
    InvalidTransfer,
    WrongEpoch,
    ArithmeticOverflow,
    PolicyConflict,
    GrantAcceptance(GrantAcceptanceError),
    Beta(BetaError),
    Store(StoreError),
    Object(ObjectError),
}

impl core::fmt::Display for BetaExecError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidObject => write!(f, "invalid Beta execution object"),
            Self::InvalidAccount => write!(f, "invalid or unknown Beta account"),
            Self::AccountConflict => write!(f, "conflicting Beta account registrations"),
            Self::InvalidTransfer => write!(f, "invalid Beta transfer"),
            Self::WrongEpoch => write!(f, "wrong Beta epoch"),
            Self::ArithmeticOverflow => write!(f, "Beta execution arithmetic overflow"),
            Self::PolicyConflict => write!(f, "conflicting Beta grant policy"),
            Self::GrantAcceptance(e) => write!(f, "grant acceptance: {e}"),
            Self::Beta(e) => write!(f, "beta: {e}"),
            Self::Store(e) => write!(f, "store: {e}"),
            Self::Object(e) => write!(f, "object: {e}"),
        }
    }
}

impl std::error::Error for BetaExecError {}

impl From<GrantAcceptanceError> for BetaExecError {
    fn from(value: GrantAcceptanceError) -> Self {
        Self::GrantAcceptance(value)
    }
}
impl From<BetaError> for BetaExecError {
    fn from(value: BetaError) -> Self {
        Self::Beta(value)
    }
}
impl From<StoreError> for BetaExecError {
    fn from(value: StoreError) -> Self {
        Self::Store(value)
    }
}
impl From<ObjectError> for BetaExecError {
    fn from(value: ObjectError) -> Self {
        Self::Object(value)
    }
}

/// One fresh account-scoped Beta wallet registration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BetaAccountRegistration {
    pub id: ObjectId,
    pub owner: Did,
    pub epoch: BetaEpochId,
    pub nonce: [u8; 32],
    pub account: BetaAccountId,
    pub timestamp_ms: u64,
    pub sequence: u64,
}

/// Exact immutable output reference. Grant output index is always zero;
/// transfer outputs are zero-based in payload order.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BetaOutputRef {
    pub source_id: ObjectId,
    pub index: u16,
}

/// One transfer output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BetaTransferOutput {
    pub account: BetaAccountId,
    pub amount: u64,
}

/// One signed durable Beta transfer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BetaTransfer {
    pub id: ObjectId,
    pub record_author: Did,
    pub epoch: BetaEpochId,
    pub sender: BetaAccountId,
    pub inputs: Vec<BetaOutputRef>,
    pub outputs: Vec<BetaTransferOutput>,
    pub memo: String,
    pub timestamp_ms: u64,
    pub sequence: u64,
}

/// Derived immutable output state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedOutput {
    pub output_ref: BetaOutputRef,
    pub account: BetaAccountId,
    pub amount: u64,
    pub spent: bool,
}

/// One account balance in a resolved snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AccountBalance {
    pub account: BetaAccountId,
    pub amount: u64,
}

/// Deterministic provisional state for one Beta epoch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BetaSnapshot {
    pub epoch: BetaEpochId,
    pub issuance_conflict: bool,
    pub account_conflicts: Vec<BetaAccountId>,
    pub transfer_conflicts: Vec<ObjectId>,
    pub invalid_transfers: Vec<ObjectId>,
    pub outputs: Vec<ResolvedOutput>,
    pub balances: Vec<AccountBalance>,
    pub total_issued: u64,
    pub digest: [u8; 32],
}

impl BetaSnapshot {
    pub fn balance(&self, account: &BetaAccountId) -> u64 {
        self.balances
            .iter()
            .find(|entry| &entry.account == account)
            .map(|entry| entry.amount)
            .unwrap_or(0)
    }

    /// Always true for this v1 resolver: late replicated conflict evidence can
    /// still change the derived state. Product surfaces must label it BETA.
    pub fn is_provisional(&self) -> bool {
        true
    }
}

/// Derive an account id from exact epoch + owner root DID + random nonce.
pub fn derive_account_id(
    epoch: BetaEpochId,
    owner: &Did,
    nonce: [u8; 32],
) -> Result<BetaAccountId> {
    let mut hasher = blake3::Hasher::new();
    hasher.update(ACCOUNT_DOMAIN);
    hasher.update(epoch.as_bytes());
    hasher.update(owner.as_str().as_bytes());
    hasher.update(&nonce);
    let mut bytes = [0u8; 32];
    bytes.copy_from_slice(hasher.finalize().as_bytes());
    BetaAccountId::new(bytes).map_err(BetaExecError::Beta)
}

/// Create and store one fresh account registration.
pub fn create_account_registration<B: Backend>(
    store: &mut Store<B>,
    owner_human: &Did,
    owner_device: &Controller,
    epoch: BetaEpochId,
    nonce: [u8; 32],
    timestamp_ms: u64,
    sequence: u64,
) -> Result<Object> {
    if nonce == [0; 32] {
        return Err(BetaExecError::InvalidAccount);
    }
    let account = derive_account_id(epoch, owner_human, nonce)?;
    let mut payload = vec![PAYLOAD_VERSION];
    payload.extend_from_slice(epoch.as_bytes());
    payload.extend_from_slice(&nonce);
    payload.extend_from_slice(account.as_bytes());
    let object = ObjectBuilder::new(ObjectType::Custom(BETA_ACCOUNT_TYPE.to_string()))
        .timestamp_ms(timestamp_ms)
        .sequence(sequence)
        .payload(Payload::Public(payload))
        .sign(owner_human, owner_device)?;
    store.insert(&object)?;
    Ok(object)
}

/// Strict account parser. Account id is re-derived from the signed author DID,
/// epoch and nonce rather than trusted from payload.
pub fn parse_account_registration_object(object: &Object) -> Result<BetaAccountRegistration> {
    ensure_type_no_links(object, BETA_ACCOUNT_TYPE)?;
    let bytes = public_payload(object)?;
    if bytes.len() != 1 + 32 + 32 + 32 || bytes[0] != PAYLOAD_VERSION {
        return Err(BetaExecError::InvalidObject);
    }
    let mut off = 1usize;
    let epoch = BetaEpochId::new(take_32(bytes, &mut off)?).map_err(BetaExecError::Beta)?;
    let nonce = take_32(bytes, &mut off)?;
    if nonce == [0; 32] {
        return Err(BetaExecError::InvalidAccount);
    }
    let encoded = BetaAccountId::new(take_32(bytes, &mut off)?).map_err(BetaExecError::Beta)?;
    let derived = derive_account_id(epoch, &object.author_human, nonce)?;
    if encoded != derived || off != bytes.len() {
        return Err(BetaExecError::InvalidAccount);
    }
    Ok(BetaAccountRegistration {
        id: object.id().clone(),
        owner: object.author_human.clone(),
        epoch,
        nonce,
        account: derived,
        timestamp_ms: object.timestamp_ms,
        sequence: object.sequence,
    })
}

pub fn read_account_registration<B: Backend>(
    store: &Store<B>,
    id: &ObjectId,
) -> Result<BetaAccountRegistration> {
    parse_account_registration_object(&store.get(id)?)
}

/// Create one signed value-conserving transfer. The resolver remains the final
/// semantic authority because producer validity/conflicts depend on the full
/// replicated object set.
#[allow(clippy::too_many_arguments)]
pub fn create_transfer<B: Backend>(
    store: &mut Store<B>,
    owner_human: &Did,
    owner_device: &Controller,
    epoch: BetaEpochId,
    sender: BetaAccountId,
    inputs: &[BetaOutputRef],
    outputs: &[BetaTransferOutput],
    memo: &str,
    timestamp_ms: u64,
    sequence: u64,
) -> Result<Object> {
    validate_transfer_shape(inputs, outputs, memo)?;
    let mut payload = vec![PAYLOAD_VERSION];
    payload.extend_from_slice(epoch.as_bytes());
    payload.extend_from_slice(sender.as_bytes());
    put_u16(&mut payload, inputs.len() as u16);
    for input in inputs {
        put_str(&mut payload, input.source_id.as_str())?;
        put_u16(&mut payload, input.index);
    }
    put_u16(&mut payload, outputs.len() as u16);
    for output in outputs {
        payload.extend_from_slice(output.account.as_bytes());
        put_u64(&mut payload, output.amount);
    }
    put_str(&mut payload, memo)?;
    let object = ObjectBuilder::new(ObjectType::Custom(BETA_TRANSFER_TYPE.to_string()))
        .timestamp_ms(timestamp_ms)
        .sequence(sequence)
        .payload(Payload::Public(payload))
        .sign(owner_human, owner_device)?;
    store.insert(&object)?;
    Ok(object)
}

pub fn parse_transfer_object(object: &Object) -> Result<BetaTransfer> {
    ensure_type_no_links(object, BETA_TRANSFER_TYPE)?;
    let bytes = public_payload(object)?;
    let mut off = 0usize;
    if take_u8(bytes, &mut off)? != PAYLOAD_VERSION {
        return Err(BetaExecError::InvalidObject);
    }
    let epoch = BetaEpochId::new(take_32(bytes, &mut off)?).map_err(BetaExecError::Beta)?;
    let sender = BetaAccountId::new(take_32(bytes, &mut off)?).map_err(BetaExecError::Beta)?;
    let input_count = take_u16(bytes, &mut off)? as usize;
    if input_count == 0 || input_count > MAX_TRANSFER_INPUTS {
        return Err(BetaExecError::InvalidTransfer);
    }
    let mut inputs = Vec::with_capacity(input_count);
    for _ in 0..input_count {
        let source = take_str(bytes, &mut off, MAX_OBJECT_ID_BYTES)?;
        let source_id = ObjectId::parse(&source).map_err(|_| BetaExecError::InvalidObject)?;
        let index = take_u16(bytes, &mut off)?;
        inputs.push(BetaOutputRef { source_id, index });
    }
    let output_count = take_u16(bytes, &mut off)? as usize;
    if output_count == 0 || output_count > MAX_TRANSFER_OUTPUTS {
        return Err(BetaExecError::InvalidTransfer);
    }
    let mut outputs = Vec::with_capacity(output_count);
    for _ in 0..output_count {
        let account = BetaAccountId::new(take_32(bytes, &mut off)?).map_err(BetaExecError::Beta)?;
        let amount = take_u64(bytes, &mut off)?;
        outputs.push(BetaTransferOutput { account, amount });
    }
    let memo = take_str_allow_empty(bytes, &mut off, MAX_MEMO_BYTES)?;
    if off != bytes.len() {
        return Err(BetaExecError::InvalidObject);
    }
    validate_transfer_shape(&inputs, &outputs, &memo)?;
    Ok(BetaTransfer {
        id: object.id().clone(),
        record_author: object.author_human.clone(),
        epoch,
        sender,
        inputs,
        outputs,
        memo,
        timestamp_ms: object.timestamp_ms,
        sequence: object.sequence,
    })
}

pub fn read_transfer<B: Backend>(store: &Store<B>, id: &ObjectId) -> Result<BetaTransfer> {
    parse_transfer_object(&store.get(id)?)
}

#[derive(Debug, Clone)]
struct OutputValue {
    account: BetaAccountId,
    amount: u64,
}

#[derive(Debug, Clone)]
struct GrantCandidate {
    grant: BetaGrantAuthorization,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VisitState {
    Visiting,
    Valid,
    Invalid,
}

/// Resolve a deterministic provisional snapshot from one complete verified
/// object set. Caller-selected ordering never affects the result.
pub fn resolve_snapshot<B: Backend>(
    store: &Store<B>,
    epoch: BetaEpochId,
    limits: BetaMiniPolicy,
) -> Result<BetaSnapshot> {
    let (registrations, mut account_conflicts) = resolve_registrations(store, epoch)?;
    let (grant_outputs, total_issued, issuance_conflict) =
        resolve_grant_outputs(store, epoch, limits, &registrations)?;

    let mut transfers = HashMap::<ObjectId, BetaTransfer>::new();
    let transfer_ids = store.by_type(&ObjectType::Custom(BETA_TRANSFER_TYPE.to_string()))?;
    let mut invalid_transfers = Vec::<ObjectId>::new();
    for id in transfer_ids {
        let object = store.get(&id)?;
        match parse_transfer_object(&object) {
            Ok(transfer) if transfer.epoch == epoch => {
                if let Some(registration) = registrations.get(&transfer.sender) {
                    if registration.owner == transfer.record_author
                        && transfer
                            .outputs
                            .iter()
                            .all(|output| registrations.contains_key(&output.account))
                    {
                        transfers.insert(id, transfer);
                    } else {
                        invalid_transfers.push(id);
                    }
                } else {
                    invalid_transfers.push(id);
                }
            }
            Ok(_) => {}
            Err(_) => invalid_transfers.push(id),
        }
    }

    let mut visit = HashMap::<ObjectId, VisitState>::new();
    let ids: Vec<ObjectId> = transfers.keys().cloned().collect();
    for id in &ids {
        let _ = economic_valid(id, &transfers, &grant_outputs, &registrations, &mut visit);
    }
    let base_valid: HashSet<ObjectId> = visit
        .iter()
        .filter_map(|(id, state)| (*state == VisitState::Valid).then_some(id.clone()))
        .collect();

    let mut consumers: HashMap<BetaOutputRef, Vec<ObjectId>> = HashMap::new();
    for id in &base_valid {
        if let Some(transfer) = transfers.get(id) {
            for input in &transfer.inputs {
                consumers.entry(input.clone()).or_default().push(id.clone());
            }
        }
    }
    let mut conflict_set = HashSet::<ObjectId>::new();
    for ids in consumers.values() {
        if ids.len() > 1 {
            for id in ids {
                conflict_set.insert(id.clone());
            }
        }
    }

    let mut final_valid = base_valid.clone();
    for id in &conflict_set {
        final_valid.remove(id);
    }
    // Cascade invalidity from conflicted/invalid producers until a fixed point.
    loop {
        let before = final_valid.len();
        let current: Vec<ObjectId> = final_valid.iter().cloned().collect();
        for id in current {
            let Some(transfer) = transfers.get(&id) else {
                final_valid.remove(&id);
                continue;
            };
            if transfer.inputs.iter().any(|input| {
                transfers.contains_key(&input.source_id) && !final_valid.contains(&input.source_id)
            }) {
                final_valid.remove(&id);
            }
        }
        if final_valid.len() == before {
            break;
        }
    }

    for id in transfers.keys() {
        if !final_valid.contains(id) && !conflict_set.contains(id) {
            invalid_transfers.push(id.clone());
        }
    }
    invalid_transfers.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    invalid_transfers.dedup();

    let mut output_values = grant_outputs;
    for id in &final_valid {
        let transfer = &transfers[id];
        for (index, output) in transfer.outputs.iter().enumerate() {
            output_values.insert(
                BetaOutputRef {
                    source_id: id.clone(),
                    index: index as u16,
                },
                OutputValue {
                    account: output.account,
                    amount: output.amount,
                },
            );
        }
    }

    let mut spent = HashSet::<BetaOutputRef>::new();
    for id in &final_valid {
        for input in &transfers[id].inputs {
            spent.insert(input.clone());
        }
    }

    let mut outputs = Vec::new();
    let mut balance_map = HashMap::<BetaAccountId, u64>::new();
    if !issuance_conflict {
        for (output_ref, value) in output_values {
            let is_spent = spent.contains(&output_ref);
            if !is_spent {
                let next = balance_map
                    .get(&value.account)
                    .copied()
                    .unwrap_or(0)
                    .checked_add(value.amount)
                    .ok_or(BetaExecError::ArithmeticOverflow)?;
                balance_map.insert(value.account, next);
            }
            outputs.push(ResolvedOutput {
                output_ref,
                account: value.account,
                amount: value.amount,
                spent: is_spent,
            });
        }
    }

    outputs.sort_by(output_order);
    let mut balances: Vec<AccountBalance> = balance_map
        .into_iter()
        .map(|(account, amount)| AccountBalance { account, amount })
        .collect();
    balances.sort_by(|a, b| a.account.as_bytes().cmp(b.account.as_bytes()));
    account_conflicts.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
    account_conflicts.dedup();
    let mut transfer_conflicts: Vec<ObjectId> = conflict_set.into_iter().collect();
    transfer_conflicts.sort_by(|a, b| a.as_str().cmp(b.as_str()));

    let mut snapshot = BetaSnapshot {
        epoch,
        issuance_conflict,
        account_conflicts,
        transfer_conflicts,
        invalid_transfers,
        outputs,
        balances,
        total_issued: if issuance_conflict { 0 } else { total_issued },
        digest: [0; 32],
    };
    snapshot.digest = snapshot_digest(&snapshot);
    Ok(snapshot)
}

fn resolve_registrations<B: Backend>(
    store: &Store<B>,
    epoch: BetaEpochId,
) -> Result<(
    HashMap<BetaAccountId, BetaAccountRegistration>,
    Vec<BetaAccountId>,
)> {
    let ids = store.by_type(&ObjectType::Custom(BETA_ACCOUNT_TYPE.to_string()))?;
    let mut all: HashMap<BetaAccountId, Vec<BetaAccountRegistration>> = HashMap::new();
    for id in ids {
        let object = store.get(&id)?;
        let Ok(registration) = parse_account_registration_object(&object) else {
            continue;
        };
        if registration.epoch == epoch {
            all.entry(registration.account)
                .or_default()
                .push(registration);
        }
    }
    let mut valid = HashMap::new();
    let mut conflicts = Vec::new();
    for (account, registrations) in all {
        if registrations.len() == 1 {
            valid.insert(account, registrations.into_iter().next().expect("one item"));
        } else {
            conflicts.push(account);
        }
    }
    Ok((valid, conflicts))
}

fn resolve_grant_outputs<B: Backend>(
    store: &Store<B>,
    epoch: BetaEpochId,
    limits: BetaMiniPolicy,
    registrations: &HashMap<BetaAccountId, BetaAccountRegistration>,
) -> Result<(HashMap<BetaOutputRef, OutputValue>, u64, bool)> {
    let approval_objects =
        store.by_type(&ObjectType::Custom(BETA_GRANT_APPROVAL_TYPE.to_string()))?;
    let mut approvals = Vec::new();
    for id in approval_objects {
        let object = store.get(&id)?;
        if let Ok(approval) = parse_grant_approval_object(&object) {
            approvals.push(approval);
        }
    }

    let grant_ids = store.by_type(&ObjectType::Custom(BETA_GRANT_TYPE.to_string()))?;
    let mut candidates = Vec::<GrantCandidate>::new();
    for grant_id in grant_ids {
        let grant = match read_grant_authorization(store, &grant_id) {
            Ok(grant) if grant.epoch == epoch => grant,
            Ok(_) => continue,
            Err(_) => continue,
        };
        if !registrations.contains_key(&grant.account) {
            continue;
        }
        let campaign = match read_campaign(store, &grant.campaign_id) {
            Ok(campaign) if campaign.epoch == epoch => campaign,
            Ok(_) | Err(_) => continue,
        };
        let policy = match resolve_unique_campaign_policy(store, &campaign.id) {
            Ok(policy) => policy,
            Err(GrantAcceptanceError::InvalidPolicy) => continue,
            Err(GrantAcceptanceError::PolicyConflict) => return Err(BetaExecError::PolicyConflict),
            Err(error) => return Err(error.into()),
        };
        if !grant_within_limits(&grant, limits) {
            continue;
        }
        let approval_ids = candidate_approval_ids(&approvals, &policy, &grant);
        match validate_grant_acceptance(store, &policy.id, &grant.id, &approval_ids) {
            Ok(_) => candidates.push(GrantCandidate { grant }),
            Err(GrantAcceptanceError::PolicyConflict) => return Err(BetaExecError::PolicyConflict),
            Err(GrantAcceptanceError::Store(error)) => return Err(BetaExecError::Store(error)),
            Err(_) => continue,
        }
    }

    // Participation grants compete by accepted contribution. If more than one
    // threshold-approved grant exists, none wins by object ordering.
    let mut contribution_counts = HashMap::<ObjectId, usize>::new();
    for candidate in &candidates {
        if candidate.grant.class == GrantClass::Participation {
            if let Some(id) = &candidate.grant.contribution_id {
                *contribution_counts.entry(id.clone()).or_default() += 1;
            }
        }
    }
    candidates.retain(|candidate| {
        candidate
            .grant
            .contribution_id
            .as_ref()
            .map(|id| contribution_counts.get(id).copied().unwrap_or(0) <= 1)
            .unwrap_or(true)
    });

    let mut total = 0u64;
    for candidate in &candidates {
        total = match total.checked_add(candidate.grant.amount) {
            Some(total) => total,
            None => return Ok((HashMap::new(), 0, true)),
        };
    }
    if total > limits.max_epoch_supply {
        return Ok((HashMap::new(), 0, true));
    }

    let mut outputs = HashMap::new();
    for candidate in candidates {
        outputs.insert(
            BetaOutputRef {
                source_id: candidate.grant.id,
                index: 0,
            },
            OutputValue {
                account: candidate.grant.account,
                amount: candidate.grant.amount,
            },
        );
    }
    Ok((outputs, total, false))
}

fn candidate_approval_ids(
    approvals: &[mini_beta_grants::BetaGrantApproval],
    policy: &BetaGrantPolicy,
    grant: &BetaGrantAuthorization,
) -> Vec<ObjectId> {
    let mut candidates: Vec<_> = approvals
        .iter()
        .filter(|approval| {
            approval.policy_id == policy.id
                && approval.grant_id == grant.id
                && policy.members.contains(&approval.record_author)
                && approval.timestamp_ms >= grant.timestamp_ms
                && approval.timestamp_ms >= policy.valid_from_ms
                && approval.timestamp_ms <= policy.valid_until_ms
        })
        .collect();
    candidates.sort_by(|a, b| {
        a.record_author
            .as_str()
            .cmp(b.record_author.as_str())
            .then_with(|| a.id.as_str().cmp(b.id.as_str()))
    });
    let mut seen = HashSet::<Did>::new();
    let mut ids = Vec::new();
    for approval in candidates {
        if seen.insert(approval.record_author.clone()) {
            ids.push(approval.id.clone());
        }
    }
    ids
}

fn grant_within_limits(grant: &BetaGrantAuthorization, limits: BetaMiniPolicy) -> bool {
    match grant.class {
        GrantClass::Testing => grant.amount <= limits.max_testing_grant,
        GrantClass::Participation => grant.amount <= limits.max_participation_grant,
    }
}

fn economic_valid(
    id: &ObjectId,
    transfers: &HashMap<ObjectId, BetaTransfer>,
    grants: &HashMap<BetaOutputRef, OutputValue>,
    registrations: &HashMap<BetaAccountId, BetaAccountRegistration>,
    visit: &mut HashMap<ObjectId, VisitState>,
) -> bool {
    if let Some(state) = visit.get(id) {
        return *state == VisitState::Valid;
    }
    visit.insert(id.clone(), VisitState::Visiting);
    let Some(transfer) = transfers.get(id) else {
        visit.insert(id.clone(), VisitState::Invalid);
        return false;
    };
    let mut input_sum = 0u64;
    for input in &transfer.inputs {
        let value = if let Some(value) = grants.get(input) {
            Some(value.clone())
        } else if let Some(producer) = transfers.get(&input.source_id) {
            if matches!(visit.get(&input.source_id), Some(VisitState::Visiting))
                || !economic_valid(&input.source_id, transfers, grants, registrations, visit)
            {
                None
            } else {
                producer
                    .outputs
                    .get(input.index as usize)
                    .map(|output| OutputValue {
                        account: output.account,
                        amount: output.amount,
                    })
            }
        } else {
            None
        };
        let Some(value) = value else {
            visit.insert(id.clone(), VisitState::Invalid);
            return false;
        };
        if value.account != transfer.sender || !registrations.contains_key(&value.account) {
            visit.insert(id.clone(), VisitState::Invalid);
            return false;
        }
        input_sum = match input_sum.checked_add(value.amount) {
            Some(sum) => sum,
            None => {
                visit.insert(id.clone(), VisitState::Invalid);
                return false;
            }
        };
    }
    let mut output_sum = 0u64;
    for output in &transfer.outputs {
        output_sum = match output_sum.checked_add(output.amount) {
            Some(sum) => sum,
            None => {
                visit.insert(id.clone(), VisitState::Invalid);
                return false;
            }
        };
    }
    let valid = input_sum == output_sum;
    visit.insert(
        id.clone(),
        if valid {
            VisitState::Valid
        } else {
            VisitState::Invalid
        },
    );
    valid
}

fn snapshot_digest(snapshot: &BetaSnapshot) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(SNAPSHOT_DOMAIN);
    hasher.update(snapshot.epoch.as_bytes());
    hasher.update(&[u8::from(snapshot.issuance_conflict)]);
    for account in &snapshot.account_conflicts {
        hasher.update(b"account-conflict");
        hasher.update(account.as_bytes());
    }
    for id in &snapshot.transfer_conflicts {
        hasher.update(b"transfer-conflict");
        hasher.update(id.as_str().as_bytes());
    }
    for id in &snapshot.invalid_transfers {
        hasher.update(b"invalid-transfer");
        hasher.update(id.as_str().as_bytes());
    }
    for output in &snapshot.outputs {
        hasher.update(output.output_ref.source_id.as_str().as_bytes());
        hasher.update(&output.output_ref.index.to_be_bytes());
        hasher.update(output.account.as_bytes());
        hasher.update(&output.amount.to_be_bytes());
        hasher.update(&[u8::from(output.spent)]);
    }
    for balance in &snapshot.balances {
        hasher.update(balance.account.as_bytes());
        hasher.update(&balance.amount.to_be_bytes());
    }
    hasher.update(&snapshot.total_issued.to_be_bytes());
    *hasher.finalize().as_bytes()
}

fn output_order(a: &ResolvedOutput, b: &ResolvedOutput) -> core::cmp::Ordering {
    a.output_ref
        .source_id
        .as_str()
        .cmp(b.output_ref.source_id.as_str())
        .then_with(|| a.output_ref.index.cmp(&b.output_ref.index))
}

fn validate_transfer_shape(
    inputs: &[BetaOutputRef],
    outputs: &[BetaTransferOutput],
    memo: &str,
) -> Result<()> {
    if inputs.is_empty()
        || inputs.len() > MAX_TRANSFER_INPUTS
        || outputs.is_empty()
        || outputs.len() > MAX_TRANSFER_OUTPUTS
        || memo.len() > MAX_MEMO_BYTES
        || outputs.iter().any(|output| output.amount == 0)
    {
        return Err(BetaExecError::InvalidTransfer);
    }
    let mut seen = HashSet::new();
    if inputs.iter().any(|input| !seen.insert(input.clone())) {
        return Err(BetaExecError::InvalidTransfer);
    }
    Ok(())
}

fn ensure_type_no_links(object: &Object, expected: &str) -> Result<()> {
    match &object.object_type {
        ObjectType::Custom(value) if value == expected && object.links.is_empty() => Ok(()),
        _ => Err(BetaExecError::InvalidObject),
    }
}

fn public_payload(object: &Object) -> Result<&[u8]> {
    match &object.payload {
        Payload::Public(bytes) => Ok(bytes),
        Payload::Encrypted(_) => Err(BetaExecError::InvalidObject),
    }
}

fn put_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_be_bytes());
}
fn put_u64(out: &mut Vec<u8>, value: u64) {
    out.extend_from_slice(&value.to_be_bytes());
}
fn put_str(out: &mut Vec<u8>, value: &str) -> Result<()> {
    if value.len() > u32::MAX as usize {
        return Err(BetaExecError::InvalidObject);
    }
    out.extend_from_slice(&(value.len() as u32).to_be_bytes());
    out.extend_from_slice(value.as_bytes());
    Ok(())
}
fn take_u8(bytes: &[u8], off: &mut usize) -> Result<u8> {
    let value = *bytes.get(*off).ok_or(BetaExecError::InvalidObject)?;
    *off += 1;
    Ok(value)
}
fn take_u16(bytes: &[u8], off: &mut usize) -> Result<u16> {
    let end = off.checked_add(2).ok_or(BetaExecError::InvalidObject)?;
    let raw = bytes.get(*off..end).ok_or(BetaExecError::InvalidObject)?;
    *off = end;
    Ok(u16::from_be_bytes(
        raw.try_into().map_err(|_| BetaExecError::InvalidObject)?,
    ))
}
fn take_u64(bytes: &[u8], off: &mut usize) -> Result<u64> {
    let end = off.checked_add(8).ok_or(BetaExecError::InvalidObject)?;
    let raw = bytes.get(*off..end).ok_or(BetaExecError::InvalidObject)?;
    *off = end;
    Ok(u64::from_be_bytes(
        raw.try_into().map_err(|_| BetaExecError::InvalidObject)?,
    ))
}
fn take_32(bytes: &[u8], off: &mut usize) -> Result<[u8; 32]> {
    let end = off.checked_add(32).ok_or(BetaExecError::InvalidObject)?;
    let raw = bytes.get(*off..end).ok_or(BetaExecError::InvalidObject)?;
    *off = end;
    raw.try_into().map_err(|_| BetaExecError::InvalidObject)
}
fn take_str(bytes: &[u8], off: &mut usize, max: usize) -> Result<String> {
    let value = take_str_allow_empty(bytes, off, max)?;
    if value.is_empty() {
        Err(BetaExecError::InvalidObject)
    } else {
        Ok(value)
    }
}
fn take_str_allow_empty(bytes: &[u8], off: &mut usize, max: usize) -> Result<String> {
    let end = off.checked_add(4).ok_or(BetaExecError::InvalidObject)?;
    let raw = bytes.get(*off..end).ok_or(BetaExecError::InvalidObject)?;
    let len =
        u32::from_be_bytes(raw.try_into().map_err(|_| BetaExecError::InvalidObject)?) as usize;
    *off = end;
    if len > max {
        return Err(BetaExecError::InvalidObject);
    }
    let end = off.checked_add(len).ok_or(BetaExecError::InvalidObject)?;
    let raw = bytes.get(*off..end).ok_or(BetaExecError::InvalidObject)?;
    *off = end;
    core::str::from_utf8(raw)
        .map(str::to_string)
        .map_err(|_| BetaExecError::InvalidObject)
}
