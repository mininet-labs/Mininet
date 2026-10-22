//! Forge-native contributor coordination objects.
//!
//! This module is the first bridge from the Beta contributor routes to the
//! self-hosted Forge. It stores four kinds of signed, content-addressed
//! coordination evidence:
//!
//! - working-group charter proposals, using the existing Governance Pack
//!   vocabulary;
//! - task briefs, linked to a Forge project and optionally to a charter;
//! - explicit, expiring work claims; and
//! - exact-state technical-review handoffs.
//!
//! None of these objects grants authority. In particular, a charter is not a
//! delegation, a claim is not an assignment or approval, and a technical
//! review is not a Forge approval. AI evidence is labelled separately and is
//! never converted into approval weight. The objects are deliberately useful
//! before the unresolved policy questions around personnel directories,
//! group activation, matching, and Forge cutover are settled.

use did_mini::{Controller, Did};
use mini_objects::{Object, ObjectBuilder, ObjectId, ObjectType, Payload};
use mini_store::{Backend, Store};

use crate::oracle::{author_verified, IdentityOracle};
use crate::{take_str, ForgeError, Result};

/// Signed working-group charter object type.
pub const WORKING_GROUP_CHARTER_TYPE: &str = "mininet.gov/working-group-charter/v1";
/// Signed contributor task brief object type.
pub const TASK_BRIEF_TYPE: &str = "mininet.gov/task-brief/v1";
/// Signed contributor work-claim object type.
pub const WORK_CLAIM_TYPE: &str = "mininet.gov/work-claim/v1";
/// Signed exact-state technical-review handoff object type.
pub const TECHNICAL_REVIEW_TYPE: &str = "mininet.gov/technical-review/v1";

const PAYLOAD_VERSION: u8 = 1;
const MAX_ITEMS: usize = 64;
const MAX_TEXT_BYTES: usize = 4096;
const MAX_SHORT_TEXT_BYTES: usize = 256;
const MAX_PATH_BYTES: usize = 256;
const MAX_GROUP_ID_BYTES: usize = 128;
const MAX_SUGGESTIONS: usize = 100;

/// Lifecycle labels from the existing working-group charter schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkingGroupLifecycle {
    /// A charter exists but carries no delegated authority.
    Proposed,
    /// A group may organize work and provide advisory review only.
    Incubating,
    /// A future policy may attach explicit scoped delegations.
    Active,
    /// A group has demonstrated continuity and cross-group integration.
    Mature,
    /// Contributions continue while delegated authority is frozen.
    Suspended,
    /// The group is preparing a responsibility split.
    Splitting,
    /// The group is preparing a responsibility merge.
    Merging,
    /// The group is retired and its history remains verifiable.
    Retired,
}

impl WorkingGroupLifecycle {
    /// The schema's canonical string label.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Proposed => "proposed",
            Self::Incubating => "incubating",
            Self::Active => "active",
            Self::Mature => "mature",
            Self::Suspended => "suspended",
            Self::Splitting => "splitting",
            Self::Merging => "merging",
            Self::Retired => "retired",
        }
    }

    /// Parse the schema's canonical string label.
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "proposed" => Self::Proposed,
            "incubating" => Self::Incubating,
            "active" => Self::Active,
            "mature" => Self::Mature,
            "suspended" => Self::Suspended,
            "splitting" => Self::Splitting,
            "merging" => Self::Merging,
            "retired" => Self::Retired,
            _ => return None,
        })
    }
}

/// Whether a technical-review handoff came from peer, external, or AI
/// evidence. This is classification only; it is not an authority level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewKind {
    /// A peer's technical observations.
    Peer,
    /// Evidence supplied by an external participant or reviewer.
    External,
    /// AI-produced observations; these have zero approval weight.
    Ai,
}

impl ReviewKind {
    /// Stable display label.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Peer => "peer",
            Self::External => "external",
            Self::Ai => "ai",
        }
    }

    /// Parse the stable display label.
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "peer" => Self::Peer,
            "external" => Self::External,
            "ai" => Self::Ai,
            _ => return None,
        })
    }
}

/// A non-authorizing review disposition. There is intentionally no approval
/// variant: authorized Forge approval remains `mini-forge::approve` and is a
/// separate, exact-head-bound object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewDisposition {
    /// The handoff records observations without requesting a change.
    Observations,
    /// The handoff identifies work that should change before handoff.
    NeedsChanges,
    /// The handoff cannot be completed with the available evidence.
    Blocked,
}

impl ReviewDisposition {
    /// Stable display label.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Observations => "observations",
            Self::NeedsChanges => "needs-changes",
            Self::Blocked => "blocked",
        }
    }

    /// Parse the stable display label.
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "observations" => Self::Observations,
            "needs-changes" => Self::NeedsChanges,
            "blocked" => Self::Blocked,
            _ => return None,
        })
    }
}

/// A signed working-group charter proposal. The fields mirror the existing
/// `forge-native/schemas/working-group-charter.schema.json`; the object adds
/// only the normal Forge author/id/timestamp metadata and an optional project
/// link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkingGroupCharter {
    /// Content id of the charter object.
    pub id: ObjectId,
    /// Identity root that signed the proposal.
    pub author: Did,
    /// Optional Forge project link.
    pub project_id: Option<ObjectId>,
    /// Immutable group identifier from the charter vocabulary.
    pub group_id: String,
    /// Human-readable name.
    pub name: String,
    /// Purpose statement.
    pub purpose: String,
    /// Domain path boundaries.
    pub domain_paths: Vec<String>,
    /// Ordinary implementation actions the group may organize.
    pub autonomous_actions: Vec<String>,
    /// Actions reserved for broader policy or governance.
    pub reserved_actions: Vec<String>,
    /// Term/expiry policy statement.
    pub term_policy: String,
    /// Appeal policy statement.
    pub appeal_policy: String,
    /// Current proposal lifecycle label.
    pub lifecycle: WorkingGroupLifecycle,
    /// Neighboring-group dependencies.
    pub dependencies: Vec<String>,
    /// Author-claimed creation time.
    pub timestamp_ms: u64,
    /// Author-scoped sequence number.
    pub sequence: u64,
}

/// A signed task brief that a contributor can discover and explicitly claim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskBrief {
    /// Content id of the task object.
    pub id: ObjectId,
    /// Identity root that authored the brief.
    pub author: Did,
    /// Forge project to which the task relates.
    pub project_id: ObjectId,
    /// Optional working-group charter.
    pub charter_id: Option<ObjectId>,
    /// Contributor route such as `rust`, `android`, or `documentation`.
    pub route: String,
    /// Descriptive risk label. It does not grant or remove authority.
    pub risk_class: String,
    /// Short task title.
    pub title: String,
    /// Problem and desired outcome.
    pub description: String,
    /// Expected repository paths or path patterns.
    pub paths: Vec<String>,
    /// Acceptance evidence expected from the contributor.
    pub evidence: Vec<String>,
    /// Acceptance conditions.
    pub acceptance: String,
    /// Explicit non-goals.
    pub non_goals: Vec<String>,
    /// Author-claimed creation time.
    pub timestamp_ms: u64,
    /// Author-scoped sequence number.
    pub sequence: u64,
}

/// A signed, expiring declaration that a contributor intends to work on a
/// task. It coordinates parallel work; it does not assign a person or grant
/// review, merge, release, or governance authority.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkClaim {
    /// Content id of the claim object.
    pub id: ObjectId,
    /// Identity root that signed the claim.
    pub author: Did,
    /// Task being claimed.
    pub task_id: ObjectId,
    /// Human-readable contributor route/role selected by the claimant.
    pub role: String,
    /// Declared path scope.
    pub paths: Vec<String>,
    /// Optional exact object used as the claimant's base.
    pub base_id: Option<ObjectId>,
    /// Expiry after which the claim is stale for coordination purposes.
    pub lease_expires_ms: u64,
    /// Optional handoff note.
    pub notes: String,
    /// Author-claimed creation time.
    pub timestamp_ms: u64,
    /// Author-scoped sequence number.
    pub sequence: u64,
}

/// An exact-state technical review handoff. This deliberately has no approval
/// bit and cannot participate in Forge quorum counting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TechnicalReview {
    /// Content id of the review object.
    pub id: ObjectId,
    /// Identity root that signed the handoff.
    pub author: Did,
    /// Task under review.
    pub task_id: ObjectId,
    /// Optional claim whose work was reviewed.
    pub claim_id: Option<ObjectId>,
    /// Exact object/revision inspected.
    pub reviewed_head: ObjectId,
    /// Evidence classification, not authority.
    pub kind: ReviewKind,
    /// Non-authorizing technical disposition.
    pub disposition: ReviewDisposition,
    /// Findings or explicit observation text.
    pub findings: String,
    /// Evidence inspected or produced.
    pub evidence: Vec<String>,
    /// What this handoff did not establish.
    pub limitations: String,
    /// Author-claimed creation time.
    pub timestamp_ms: u64,
    /// Author-scoped sequence number.
    pub sequence: u64,
}

/// All coordination evidence attached to one task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskSnapshot {
    /// The verified task brief.
    pub task: TaskBrief,
    /// Verified work claims linked to the task.
    pub claims: Vec<WorkClaim>,
    /// Verified review handoffs linked to the task.
    pub reviews: Vec<TechnicalReview>,
}

/// Create and store a signed working-group charter proposal.
#[allow(clippy::too_many_arguments)]
pub fn create_working_group_charter<B: Backend>(
    store: &mut Store<B>,
    human: &Did,
    device: &Controller,
    project_id: Option<&ObjectId>,
    group_id: &str,
    name: &str,
    purpose: &str,
    domain_paths: &[String],
    autonomous_actions: &[String],
    reserved_actions: &[String],
    term_policy: &str,
    appeal_policy: &str,
    lifecycle: WorkingGroupLifecycle,
    dependencies: &[String],
    timestamp_ms: u64,
    sequence: u64,
) -> Result<Object> {
    validate_text(group_id, MAX_GROUP_ID_BYTES, true)?;
    validate_text(name, MAX_SHORT_TEXT_BYTES, true)?;
    validate_text(purpose, MAX_TEXT_BYTES, true)?;
    validate_text(term_policy, MAX_TEXT_BYTES, true)?;
    validate_text(appeal_policy, MAX_TEXT_BYTES, true)?;
    validate_items(domain_paths, MAX_PATH_BYTES, true)?;
    validate_items(autonomous_actions, MAX_SHORT_TEXT_BYTES, false)?;
    validate_items(reserved_actions, MAX_SHORT_TEXT_BYTES, false)?;
    validate_items(dependencies, MAX_GROUP_ID_BYTES, false)?;

    let mut payload = vec![PAYLOAD_VERSION];
    put_str(&mut payload, group_id);
    put_str(&mut payload, name);
    put_str(&mut payload, purpose);
    put_list(&mut payload, domain_paths);
    put_list(&mut payload, autonomous_actions);
    put_list(&mut payload, reserved_actions);
    put_str(&mut payload, term_policy);
    put_str(&mut payload, appeal_policy);
    put_str(&mut payload, lifecycle.as_str());
    put_list(&mut payload, dependencies);

    let mut builder =
        ObjectBuilder::new(ObjectType::Custom(WORKING_GROUP_CHARTER_TYPE.to_string()))
            .timestamp_ms(timestamp_ms)
            .sequence(sequence)
            .payload(Payload::Public(payload));
    if let Some(project) = project_id {
        builder = builder.link("project", project.clone());
    }
    let obj = builder.sign(human, device)?;
    store.insert(&obj)?;
    Ok(obj)
}

/// Create and store a signed task brief.
#[allow(clippy::too_many_arguments)]
pub fn create_task_brief<B: Backend>(
    store: &mut Store<B>,
    human: &Did,
    device: &Controller,
    project_id: &ObjectId,
    charter_id: Option<&ObjectId>,
    route: &str,
    risk_class: &str,
    title: &str,
    description: &str,
    paths: &[String],
    evidence: &[String],
    acceptance: &str,
    non_goals: &[String],
    timestamp_ms: u64,
    sequence: u64,
) -> Result<Object> {
    if let Some(charter) = charter_id {
        let charter_object = store.get(charter)?;
        if parse_working_group_charter_object(&charter_object).is_none() {
            return Err(ForgeError::BadObject);
        }
    }
    validate_text(route, MAX_SHORT_TEXT_BYTES, true)?;
    validate_text(risk_class, MAX_SHORT_TEXT_BYTES, true)?;
    validate_text(title, MAX_SHORT_TEXT_BYTES, true)?;
    validate_text(description, MAX_TEXT_BYTES, true)?;
    validate_text(acceptance, MAX_TEXT_BYTES, true)?;
    validate_items(paths, MAX_PATH_BYTES, true)?;
    validate_items(evidence, MAX_TEXT_BYTES, true)?;
    validate_items(non_goals, MAX_TEXT_BYTES, true)?;

    let mut payload = vec![PAYLOAD_VERSION];
    put_str(&mut payload, route);
    put_str(&mut payload, risk_class);
    put_str(&mut payload, title);
    put_str(&mut payload, description);
    put_list(&mut payload, paths);
    put_list(&mut payload, evidence);
    put_str(&mut payload, acceptance);
    put_list(&mut payload, non_goals);

    let mut builder = ObjectBuilder::new(ObjectType::Custom(TASK_BRIEF_TYPE.to_string()))
        .timestamp_ms(timestamp_ms)
        .sequence(sequence)
        .payload(Payload::Public(payload))
        .link("project", project_id.clone());
    if let Some(charter) = charter_id {
        builder = builder.link("charter", charter.clone());
    }
    let obj = builder.sign(human, device)?;
    store.insert(&obj)?;
    Ok(obj)
}

/// Create and store an explicit, expiring work claim for a task.
#[allow(clippy::too_many_arguments)]
pub fn create_work_claim<B: Backend>(
    store: &mut Store<B>,
    human: &Did,
    device: &Controller,
    task_id: &ObjectId,
    role: &str,
    paths: &[String],
    base_id: Option<&ObjectId>,
    lease_expires_ms: u64,
    notes: &str,
    timestamp_ms: u64,
    sequence: u64,
) -> Result<Object> {
    let task = store.get(task_id)?;
    if parse_task_brief_object(&task).is_none() {
        return Err(ForgeError::BadObject);
    }
    validate_text(role, MAX_SHORT_TEXT_BYTES, true)?;
    validate_items(paths, MAX_PATH_BYTES, true)?;
    validate_text(notes, MAX_TEXT_BYTES, false)?;
    if lease_expires_ms <= timestamp_ms {
        return Err(ForgeError::BadObject);
    }

    let mut payload = vec![PAYLOAD_VERSION];
    put_str(&mut payload, role);
    payload.extend_from_slice(&lease_expires_ms.to_be_bytes());
    put_list(&mut payload, paths);
    put_str(&mut payload, notes);

    let mut builder = ObjectBuilder::new(ObjectType::Custom(WORK_CLAIM_TYPE.to_string()))
        .timestamp_ms(timestamp_ms)
        .sequence(sequence)
        .payload(Payload::Public(payload))
        .link("task", task_id.clone());
    if let Some(base) = base_id {
        builder = builder.link("base", base.clone());
    }
    let obj = builder.sign(human, device)?;
    store.insert(&obj)?;
    Ok(obj)
}

/// Create and store an exact-state technical-review handoff. `claim_id`, when
/// present, must be a claim for the same task. The reviewed object must be
/// present locally so the reviewer can honestly name an exact state.
#[allow(clippy::too_many_arguments)]
pub fn create_technical_review<B: Backend>(
    store: &mut Store<B>,
    human: &Did,
    device: &Controller,
    task_id: &ObjectId,
    claim_id: Option<&ObjectId>,
    reviewed_head: &ObjectId,
    kind: ReviewKind,
    disposition: ReviewDisposition,
    findings: &str,
    evidence: &[String],
    limitations: &str,
    timestamp_ms: u64,
    sequence: u64,
) -> Result<Object> {
    let task = store.get(task_id)?;
    if parse_task_brief_object(&task).is_none() {
        return Err(ForgeError::BadObject);
    }
    if let Some(claim) = claim_id {
        let claim_obj = store.get(claim)?;
        let parsed = parse_work_claim_object(&claim_obj).ok_or(ForgeError::BadObject)?;
        if parsed.task_id != *task_id {
            return Err(ForgeError::BadObject);
        }
    }
    let _ = store.get(reviewed_head)?;
    validate_text(findings, MAX_TEXT_BYTES, true)?;
    validate_items(evidence, MAX_TEXT_BYTES, true)?;
    validate_text(limitations, MAX_TEXT_BYTES, true)?;

    let mut payload = vec![PAYLOAD_VERSION];
    put_str(&mut payload, kind.as_str());
    put_str(&mut payload, disposition.as_str());
    put_str(&mut payload, findings);
    put_list(&mut payload, evidence);
    put_str(&mut payload, limitations);

    let mut builder = ObjectBuilder::new(ObjectType::Custom(TECHNICAL_REVIEW_TYPE.to_string()))
        .timestamp_ms(timestamp_ms)
        .sequence(sequence)
        .payload(Payload::Public(payload))
        .link("task", task_id.clone())
        .link("reviewed", reviewed_head.clone());
    if let Some(claim) = claim_id {
        builder = builder.link("claim", claim.clone());
    }
    let obj = builder.sign(human, device)?;
    store.insert(&obj)?;
    Ok(obj)
}

/// Read a charter after checking its object shape and author provenance.
pub fn read_working_group_charter<B: Backend>(
    store: &Store<B>,
    oracle: &dyn IdentityOracle,
    id: &ObjectId,
) -> Result<WorkingGroupCharter> {
    let obj = store.get(id)?;
    let parsed = parse_working_group_charter_object(&obj).ok_or(ForgeError::BadObject)?;
    if !author_verified(oracle, &obj) {
        return Err(ForgeError::BadObject);
    }
    Ok(parsed)
}

/// Read a task after checking its object shape and author provenance.
pub fn read_task_brief<B: Backend>(
    store: &Store<B>,
    oracle: &dyn IdentityOracle,
    id: &ObjectId,
) -> Result<TaskBrief> {
    let obj = store.get(id)?;
    let parsed = parse_task_brief_object(&obj).ok_or(ForgeError::BadObject)?;
    if !author_verified(oracle, &obj) {
        return Err(ForgeError::BadObject);
    }
    Ok(parsed)
}

/// Read a claim after checking its object shape and author provenance.
pub fn read_work_claim<B: Backend>(
    store: &Store<B>,
    oracle: &dyn IdentityOracle,
    id: &ObjectId,
) -> Result<WorkClaim> {
    let obj = store.get(id)?;
    let parsed = parse_work_claim_object(&obj).ok_or(ForgeError::BadObject)?;
    if !author_verified(oracle, &obj) {
        return Err(ForgeError::BadObject);
    }
    Ok(parsed)
}

/// Read a technical-review handoff after checking its object shape and author
/// provenance.
pub fn read_technical_review<B: Backend>(
    store: &Store<B>,
    oracle: &dyn IdentityOracle,
    id: &ObjectId,
) -> Result<TechnicalReview> {
    let obj = store.get(id)?;
    let parsed = parse_technical_review_object(&obj).ok_or(ForgeError::BadObject)?;
    if !author_verified(oracle, &obj) {
        return Err(ForgeError::BadObject);
    }
    Ok(parsed)
}

/// List verified charter proposals, in deterministic `(timestamp, id)` order.
pub fn list_working_group_charters<B: Backend>(
    store: &Store<B>,
    oracle: &dyn IdentityOracle,
) -> Result<Vec<WorkingGroupCharter>> {
    let object_type = ObjectType::Custom(WORKING_GROUP_CHARTER_TYPE.to_string());
    let mut out = Vec::new();
    for id in store.by_type(&object_type)? {
        let Ok(obj) = store.get(&id) else { continue };
        if !author_verified(oracle, &obj) {
            continue;
        }
        if let Some(parsed) = parse_working_group_charter_object(&obj) {
            out.push(parsed);
        }
    }
    sort_objects(&mut out, |item| (&item.timestamp_ms, item.id.as_str()));
    Ok(out)
}

/// List verified task briefs, in deterministic `(timestamp, id)` order.
pub fn list_task_briefs<B: Backend>(
    store: &Store<B>,
    oracle: &dyn IdentityOracle,
) -> Result<Vec<TaskBrief>> {
    let object_type = ObjectType::Custom(TASK_BRIEF_TYPE.to_string());
    let mut out = Vec::new();
    for id in store.by_type(&object_type)? {
        let Ok(obj) = store.get(&id) else { continue };
        if !author_verified(oracle, &obj) {
            continue;
        }
        if let Some(parsed) = parse_task_brief_object(&obj) {
            out.push(parsed);
        }
    }
    sort_objects(&mut out, |item| (&item.timestamp_ms, item.id.as_str()));
    Ok(out)
}

/// Return deterministic advisory task suggestions. Matching is deliberately
/// conservative: route matching is exact and path matching accepts exact
/// paths or a task path ending in `/**` that contains the requested path. The
/// result is a suggestion, never an assignment or authority decision.
pub fn suggest_tasks<B: Backend>(
    store: &Store<B>,
    oracle: &dyn IdentityOracle,
    route: Option<&str>,
    path: Option<&str>,
    limit: usize,
) -> Result<Vec<TaskBrief>> {
    if limit > MAX_SUGGESTIONS {
        return Err(ForgeError::FieldTooLarge);
    }
    let mut tasks = list_task_briefs(store, oracle)?;
    tasks.retain(|task| {
        let route_ok = route.map(|wanted| task.route == wanted).unwrap_or(true);
        let path_ok = path
            .map(|wanted| {
                task.paths
                    .iter()
                    .any(|declared| path_matches(declared, wanted))
            })
            .unwrap_or(true);
        route_ok && path_ok
    });
    tasks.truncate(limit);
    Ok(tasks)
}

/// List verified claims attached to a task.
pub fn list_work_claims<B: Backend>(
    store: &Store<B>,
    oracle: &dyn IdentityOracle,
    task_id: &ObjectId,
) -> Result<Vec<WorkClaim>> {
    let mut out = Vec::new();
    for id in store.linking_to(task_id)? {
        let Ok(obj) = store.get(&id) else { continue };
        if obj.object_type != ObjectType::Custom(WORK_CLAIM_TYPE.to_string())
            || !author_verified(oracle, &obj)
        {
            continue;
        }
        if let Some(parsed) = parse_work_claim_object(&obj) {
            if parsed.task_id == *task_id {
                out.push(parsed);
            }
        }
    }
    sort_objects(&mut out, |item| (&item.timestamp_ms, item.id.as_str()));
    Ok(out)
}

/// List verified review handoffs attached to a task.
pub fn list_technical_reviews<B: Backend>(
    store: &Store<B>,
    oracle: &dyn IdentityOracle,
    task_id: &ObjectId,
) -> Result<Vec<TechnicalReview>> {
    let mut out = Vec::new();
    for id in store.linking_to(task_id)? {
        let Ok(obj) = store.get(&id) else { continue };
        if obj.object_type != ObjectType::Custom(TECHNICAL_REVIEW_TYPE.to_string())
            || !author_verified(oracle, &obj)
        {
            continue;
        }
        if let Some(parsed) = parse_technical_review_object(&obj) {
            if parsed.task_id == *task_id {
                out.push(parsed);
            }
        }
    }
    sort_objects(&mut out, |item| (&item.timestamp_ms, item.id.as_str()));
    Ok(out)
}

/// Read a task and all verified coordination evidence attached to it.
pub fn task_snapshot<B: Backend>(
    store: &Store<B>,
    oracle: &dyn IdentityOracle,
    task_id: &ObjectId,
) -> Result<TaskSnapshot> {
    let task = read_task_brief(store, oracle, task_id)?;
    let claims = list_work_claims(store, oracle, task_id)?;
    let reviews = list_technical_reviews(store, oracle, task_id)?;
    Ok(TaskSnapshot {
        task,
        claims,
        reviews,
    })
}

fn parse_working_group_charter_object(obj: &Object) -> Option<WorkingGroupCharter> {
    if obj.object_type != ObjectType::Custom(WORKING_GROUP_CHARTER_TYPE.to_string()) {
        return None;
    }
    let payload = public_payload(obj)?;
    let mut off = 0usize;
    if take_byte(payload, &mut off)? != PAYLOAD_VERSION {
        return None;
    }
    let group_id = take_bounded_str(payload, &mut off, MAX_GROUP_ID_BYTES, true)?;
    let name = take_bounded_str(payload, &mut off, MAX_SHORT_TEXT_BYTES, true)?;
    let purpose = take_bounded_str(payload, &mut off, MAX_TEXT_BYTES, true)?;
    let domain_paths = take_list(payload, &mut off, MAX_PATH_BYTES, true)?;
    let autonomous_actions = take_list(payload, &mut off, MAX_SHORT_TEXT_BYTES, false)?;
    let reserved_actions = take_list(payload, &mut off, MAX_SHORT_TEXT_BYTES, false)?;
    let term_policy = take_bounded_str(payload, &mut off, MAX_TEXT_BYTES, true)?;
    let appeal_policy = take_bounded_str(payload, &mut off, MAX_TEXT_BYTES, true)?;
    let lifecycle = WorkingGroupLifecycle::parse(&take_str(payload, &mut off)?)?;
    let dependencies = take_list(payload, &mut off, MAX_GROUP_ID_BYTES, false)?;
    if off != payload.len() {
        return None;
    }
    let project_id = unique_link(obj, "project")?;
    if obj.links.iter().any(|link| link.rel != "project") {
        return None;
    }
    Some(WorkingGroupCharter {
        id: obj.id().clone(),
        author: obj.author_human.clone(),
        project_id,
        group_id,
        name,
        purpose,
        domain_paths,
        autonomous_actions,
        reserved_actions,
        term_policy,
        appeal_policy,
        lifecycle,
        dependencies,
        timestamp_ms: obj.timestamp_ms,
        sequence: obj.sequence,
    })
}

fn parse_task_brief_object(obj: &Object) -> Option<TaskBrief> {
    if obj.object_type != ObjectType::Custom(TASK_BRIEF_TYPE.to_string()) {
        return None;
    }
    let payload = public_payload(obj)?;
    let mut off = 0usize;
    if take_byte(payload, &mut off)? != PAYLOAD_VERSION {
        return None;
    }
    let route = take_bounded_str(payload, &mut off, MAX_SHORT_TEXT_BYTES, true)?;
    let risk_class = take_bounded_str(payload, &mut off, MAX_SHORT_TEXT_BYTES, true)?;
    let title = take_bounded_str(payload, &mut off, MAX_SHORT_TEXT_BYTES, true)?;
    let description = take_bounded_str(payload, &mut off, MAX_TEXT_BYTES, true)?;
    let paths = take_list(payload, &mut off, MAX_PATH_BYTES, true)?;
    let evidence = take_list(payload, &mut off, MAX_TEXT_BYTES, true)?;
    let acceptance = take_bounded_str(payload, &mut off, MAX_TEXT_BYTES, true)?;
    let non_goals = take_list(payload, &mut off, MAX_TEXT_BYTES, true)?;
    if off != payload.len() {
        return None;
    }
    let project_id = unique_link(obj, "project")??;
    let charter_id = unique_link(obj, "charter")?;
    if obj
        .links
        .iter()
        .any(|link| link.rel != "project" && link.rel != "charter")
    {
        return None;
    }
    Some(TaskBrief {
        id: obj.id().clone(),
        author: obj.author_human.clone(),
        project_id,
        charter_id,
        route,
        risk_class,
        title,
        description,
        paths,
        evidence,
        acceptance,
        non_goals,
        timestamp_ms: obj.timestamp_ms,
        sequence: obj.sequence,
    })
}

fn parse_work_claim_object(obj: &Object) -> Option<WorkClaim> {
    if obj.object_type != ObjectType::Custom(WORK_CLAIM_TYPE.to_string()) {
        return None;
    }
    let payload = public_payload(obj)?;
    let mut off = 0usize;
    if take_byte(payload, &mut off)? != PAYLOAD_VERSION {
        return None;
    }
    let role = take_bounded_str(payload, &mut off, MAX_SHORT_TEXT_BYTES, true)?;
    let lease_expires_ms = take_u64(payload, &mut off)?;
    let paths = take_list(payload, &mut off, MAX_PATH_BYTES, true)?;
    let notes = take_bounded_str(payload, &mut off, MAX_TEXT_BYTES, false)?;
    if off != payload.len() {
        return None;
    }
    let task_id = unique_link(obj, "task")??;
    let base_id = unique_link(obj, "base")?;
    if obj
        .links
        .iter()
        .any(|link| link.rel != "task" && link.rel != "base")
    {
        return None;
    }
    if lease_expires_ms <= obj.timestamp_ms {
        return None;
    }
    Some(WorkClaim {
        id: obj.id().clone(),
        author: obj.author_human.clone(),
        task_id,
        role,
        paths,
        base_id,
        lease_expires_ms,
        notes,
        timestamp_ms: obj.timestamp_ms,
        sequence: obj.sequence,
    })
}

fn parse_technical_review_object(obj: &Object) -> Option<TechnicalReview> {
    if obj.object_type != ObjectType::Custom(TECHNICAL_REVIEW_TYPE.to_string()) {
        return None;
    }
    let payload = public_payload(obj)?;
    let mut off = 0usize;
    if take_byte(payload, &mut off)? != PAYLOAD_VERSION {
        return None;
    }
    let kind = ReviewKind::parse(&take_str(payload, &mut off)?)?;
    let disposition = ReviewDisposition::parse(&take_str(payload, &mut off)?)?;
    let findings = take_bounded_str(payload, &mut off, MAX_TEXT_BYTES, true)?;
    let evidence = take_list(payload, &mut off, MAX_TEXT_BYTES, true)?;
    let limitations = take_bounded_str(payload, &mut off, MAX_TEXT_BYTES, true)?;
    if off != payload.len() {
        return None;
    }
    let task_id = unique_link(obj, "task")??;
    let reviewed_head = unique_link(obj, "reviewed")??;
    let claim_id = unique_link(obj, "claim")?;
    if obj
        .links
        .iter()
        .any(|link| link.rel != "task" && link.rel != "reviewed" && link.rel != "claim")
    {
        return None;
    }
    Some(TechnicalReview {
        id: obj.id().clone(),
        author: obj.author_human.clone(),
        task_id,
        claim_id,
        reviewed_head,
        kind,
        disposition,
        findings,
        evidence,
        limitations,
        timestamp_ms: obj.timestamp_ms,
        sequence: obj.sequence,
    })
}

fn public_payload(obj: &Object) -> Option<&[u8]> {
    match &obj.payload {
        Payload::Public(bytes) => Some(bytes),
        Payload::Encrypted(_) => None,
    }
}

fn unique_link(obj: &Object, relation: &str) -> Option<Option<ObjectId>> {
    let links: Vec<&ObjectId> = obj
        .links
        .iter()
        .filter(|link| link.rel == relation)
        .map(|link| &link.target)
        .collect();
    match links.as_slice() {
        [] => Some(None),
        [target] => Some(Some((*target).clone())),
        _ => None,
    }
}

fn validate_text(s: &str, max: usize, required: bool) -> Result<()> {
    if s.len() > max || (required && s.is_empty()) {
        return Err(ForgeError::FieldTooLarge);
    }
    Ok(())
}

fn validate_items(items: &[String], max_item: usize, required: bool) -> Result<()> {
    if items.len() > MAX_ITEMS || (required && items.is_empty()) {
        return Err(ForgeError::FieldTooLarge);
    }
    for item in items {
        validate_text(item, max_item, true)?;
    }
    Ok(())
}

fn put_str(out: &mut Vec<u8>, value: &str) {
    out.extend_from_slice(&(value.len() as u32).to_be_bytes());
    out.extend_from_slice(value.as_bytes());
}

fn put_list(out: &mut Vec<u8>, values: &[String]) {
    out.extend_from_slice(&(values.len() as u32).to_be_bytes());
    for value in values {
        put_str(out, value);
    }
}

fn take_byte(bytes: &[u8], off: &mut usize) -> Option<u8> {
    let value = *bytes.get(*off)?;
    *off += 1;
    Some(value)
}

fn take_u64(bytes: &[u8], off: &mut usize) -> Option<u64> {
    let end = off.checked_add(8)?;
    let raw = bytes.get(*off..end)?;
    *off = end;
    Some(u64::from_be_bytes(raw.try_into().ok()?))
}

fn take_bounded_str(bytes: &[u8], off: &mut usize, max: usize, required: bool) -> Option<String> {
    let value = take_str(bytes, off)?;
    if value.len() > max || (required && value.is_empty()) {
        return None;
    }
    Some(value)
}

fn take_list(
    bytes: &[u8],
    off: &mut usize,
    max_item: usize,
    required: bool,
) -> Option<Vec<String>> {
    let count_bytes = bytes.get(*off..(*off).checked_add(4)?)?;
    let count = u32::from_be_bytes(count_bytes.try_into().ok()?) as usize;
    *off += 4;
    if count > MAX_ITEMS || (required && count == 0) {
        return None;
    }
    let mut values = Vec::with_capacity(count);
    for _ in 0..count {
        values.push(take_bounded_str(bytes, off, max_item, true)?);
    }
    Some(values)
}

fn path_matches(declared: &str, requested: &str) -> bool {
    declared == requested
        || declared
            .strip_suffix("/**")
            .map(|prefix| {
                let prefix = prefix.trim_end_matches('/');
                requested == prefix || requested.starts_with(&format!("{prefix}/"))
            })
            .unwrap_or(false)
}

fn sort_objects<T, F>(items: &mut [T], mut key: F)
where
    F: FnMut(&T) -> (&u64, &str),
{
    items.sort_by(|a, b| key(a).cmp(&key(b)));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::KelDirectory;
    use did_mini::Capabilities;
    use mini_store::MemoryBackend;

    fn identity(seed: u8) -> (Controller, Controller) {
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

    fn oracle(people: &[&(Controller, Controller)]) -> KelDirectory {
        let mut out = KelDirectory::new();
        for (root, device) in people {
            out.insert(root.kel());
            out.insert(device.kel());
        }
        out
    }

    fn project_id(
        store: &mut Store<MemoryBackend>,
        root: &Controller,
        device: &Controller,
    ) -> ObjectId {
        let project = ObjectBuilder::new(ObjectType::Custom("mini/project".to_string()))
            .payload(Payload::Public(b"test project".to_vec()))
            .sign(&root.did(), device)
            .unwrap();
        store.insert(&project).unwrap();
        project.id().clone()
    }

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    #[test]
    fn charter_task_claim_and_review_round_trip() {
        let a = identity(10);
        let b = identity(50);
        let mut store = Store::new(MemoryBackend::new());
        let project = project_id(&mut store, &a.0, &a.1);
        let charter = create_working_group_charter(
            &mut store,
            &a.0.did(),
            &a.1,
            Some(&project),
            "wg-forge",
            "Forge",
            "coordinate Forge work",
            &strings(&["crates/mini-forge/**"]),
            &strings(&["ordinary implementation review"]),
            &strings(&["invariant amendment"]),
            "expiring terms",
            "cross-group appeal",
            WorkingGroupLifecycle::Proposed,
            &[],
            10,
            1,
        )
        .unwrap();
        let task = create_task_brief(
            &mut store,
            &a.0.did(),
            &a.1,
            &project,
            Some(charter.id()),
            "rust",
            "routine",
            "wire Forge tasks",
            "connect the contributor route",
            &strings(&["crates/mini-forge/**"]),
            &strings(&["unit tests", "exact object ids"]),
            "a second home can inspect the objects",
            &strings(&["no group authority", "no GitHub cutover"]),
            20,
            2,
        )
        .unwrap();
        let claim = create_work_claim(
            &mut store,
            &b.0.did(),
            &b.1,
            task.id(),
            "rust contributor",
            &strings(&["crates/mini-forge/**"]),
            None,
            200,
            "I will send a review handoff",
            30,
            1,
        )
        .unwrap();
        let reviewed = ObjectBuilder::new(ObjectType::Custom("mini/revision".to_string()))
            .payload(Payload::Public(b"exact state".to_vec()))
            .sign(&b.0.did(), &b.1)
            .unwrap();
        store.insert(&reviewed).unwrap();
        create_technical_review(
            &mut store,
            &a.0.did(),
            &a.1,
            task.id(),
            Some(claim.id()),
            reviewed.id(),
            ReviewKind::Peer,
            ReviewDisposition::Observations,
            "the route is explicit",
            &strings(&["cargo test"]),
            "no external audit or authority decision",
            40,
            3,
        )
        .unwrap();

        let trust = oracle(&[&a, &b]);
        let snapshot = task_snapshot(&store, &trust, task.id()).unwrap();
        assert_eq!(snapshot.task.charter_id, Some(charter.id().clone()));
        assert_eq!(snapshot.claims.len(), 1);
        assert_eq!(snapshot.reviews.len(), 1);
        assert_eq!(snapshot.reviews[0].reviewed_head, *reviewed.id());
    }

    #[test]
    fn suggestions_are_deterministic_and_conservative() {
        let a = identity(80);
        let mut store = Store::new(MemoryBackend::new());
        let project = project_id(&mut store, &a.0, &a.1);
        create_task_brief(
            &mut store,
            &a.0.did(),
            &a.1,
            &project,
            None,
            "rust",
            "routine",
            "matching task",
            "do the work",
            &strings(&["crates/mini-forge/**"]),
            &strings(&["test"]),
            "tests pass",
            &strings(&["no authority"]),
            10,
            1,
        )
        .unwrap();
        create_task_brief(
            &mut store,
            &a.0.did(),
            &a.1,
            &project,
            None,
            "android",
            "sensitive",
            "different route",
            "do other work",
            &strings(&["app/android/**"]),
            &strings(&["device test"]),
            "device result",
            &strings(&["no production claim"]),
            11,
            2,
        )
        .unwrap();
        let trust = oracle(&[&a]);
        let suggestions = suggest_tasks(
            &store,
            &trust,
            Some("rust"),
            Some("crates/mini-forge/src/lib.rs"),
            10,
        )
        .unwrap();
        assert_eq!(suggestions.len(), 1);
        assert_eq!(suggestions[0].route, "rust");
        assert!(suggest_tasks(
            &store,
            &trust,
            Some("rust"),
            Some("app/android/Main.kt"),
            10
        )
        .unwrap()
        .is_empty());
    }

    #[test]
    fn unverified_authors_are_not_read_as_coordination_evidence() {
        let a = identity(110);
        let mut store = Store::new(MemoryBackend::new());
        let project = project_id(&mut store, &a.0, &a.1);
        let task = create_task_brief(
            &mut store,
            &a.0.did(),
            &a.1,
            &project,
            None,
            "security",
            "sensitive",
            "unverified task",
            "must not be surfaced",
            &strings(&["crates/**"]),
            &strings(&["external review"]),
            "an oracle must vouch for the author",
            &strings(&["no authority"]),
            10,
            1,
        )
        .unwrap();
        let empty_oracle = KelDirectory::new();
        assert_eq!(
            read_task_brief(&store, &empty_oracle, task.id()),
            Err(ForgeError::BadObject)
        );
        assert!(list_task_briefs(&store, &empty_oracle).unwrap().is_empty());
    }

    #[test]
    fn malformed_trailing_payload_is_not_read() {
        let a = identity(120);
        let mut store = Store::new(MemoryBackend::new());
        let project = project_id(&mut store, &a.0, &a.1);
        let task = create_task_brief(
            &mut store,
            &a.0.did(),
            &a.1,
            &project,
            None,
            "docs",
            "routine",
            "strict parser",
            "read exact bytes",
            &strings(&["docs/**"]),
            &strings(&["test"]),
            "trailing bytes reject",
            &strings(&["no mutation"]),
            10,
            1,
        )
        .unwrap();
        let obj = store.get(task.id()).unwrap();
        let payload = match obj.payload {
            Payload::Public(bytes) => bytes,
            Payload::Encrypted(_) => unreachable!(),
        };
        let malformed = ObjectBuilder::new(ObjectType::Custom(TASK_BRIEF_TYPE.to_string()))
            .payload(Payload::Public([payload, vec![0]].concat()))
            .link("project", project)
            .sign(&a.0.did(), &a.1)
            .unwrap();
        store.insert(&malformed).unwrap();
        assert!(parse_task_brief_object(&malformed).is_none());
    }

    #[test]
    fn ai_review_is_classification_not_approval() {
        assert_eq!(ReviewKind::Ai.as_str(), "ai");
        assert_ne!(ReviewKind::Ai, ReviewKind::Peer);
        assert_eq!(ReviewDisposition::Observations.as_str(), "observations");
    }

    #[test]
    fn expired_claims_are_rejected_by_the_parser() {
        let a = identity(150);
        let mut store = Store::new(MemoryBackend::new());
        let project = project_id(&mut store, &a.0, &a.1);
        let task = create_task_brief(
            &mut store,
            &a.0.did(),
            &a.1,
            &project,
            None,
            "rust",
            "routine",
            "expiry",
            "check lease",
            &strings(&["crates/**"]),
            &strings(&["test"]),
            "expired is stale",
            &strings(&["no assignment"]),
            100,
            1,
        )
        .unwrap();
        let malformed = ObjectBuilder::new(ObjectType::Custom(WORK_CLAIM_TYPE.to_string()))
            .timestamp_ms(100)
            .payload({
                let mut bytes = vec![PAYLOAD_VERSION];
                put_str(&mut bytes, "rust");
                bytes.extend_from_slice(&100u64.to_be_bytes());
                put_list(&mut bytes, &strings(&["crates/**"]));
                put_str(&mut bytes, "stale");
                Payload::Public(bytes)
            })
            .link("task", task.id().clone())
            .sign(&a.0.did(), &a.1)
            .unwrap();
        store.insert(&malformed).unwrap();
        assert!(parse_work_claim_object(&malformed).is_none());
    }
}
