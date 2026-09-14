//! Open-Beta coordination objects and resettable Beta MINI test accounting.
//!
//! PR #334 deliberately keeps this crate outside production value. Its
//! dependency graph contains identity/object/store primitives only: no
//! `mini-value`, settlement, treasury, chain, consensus, or Forge governance.
//! Beta MINI therefore cannot become production MINI through an accidental
//! call path in this crate.
//!
//! The object types are ordinary signed `mini-objects` custom objects, so they
//! can replicate through the same store/sync substrate Forge already uses.
//! During the Pre-Go-Live anonymous bootstrap, the object author is the intake
//! or record signer. The tester/contributor is represented only by a fresh,
//! artifact-scoped random tag and never by a Mininet DID, username, or stable
//! reputation handle.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

use std::collections::{HashMap, HashSet};

use did_mini::{Controller, Did};
use mini_objects::{Object, ObjectBuilder, ObjectId, ObjectType, Payload};
use mini_store::{Backend, Store, StoreError};

/// Forge-compatible Beta campaign object type.
pub const BETA_CAMPAIGN_TYPE: &str = "mininet.beta/campaign/v1";
/// Forge-compatible structured finding object type.
pub const BETA_FINDING_TYPE: &str = "mininet.beta/finding/v1";
/// Append-only finding-disposition object type.
pub const BETA_FINDING_DISPOSITION_TYPE: &str = "mininet.beta/finding-disposition/v1";
/// Accepted contribution receipt object type.
pub const BETA_CONTRIBUTION_TYPE: &str = "mininet.beta/contribution/v1";
/// Beta MINI grant-authorization evidence object type.
pub const BETA_GRANT_TYPE: &str = "mininet.beta/grant/v1";

/// Integer accounting unit. One Beta MINI is one million micro-BETA-MINI.
pub const MICRO_BETA_MINI_PER_BETA_MINI: u64 = 1_000_000;
/// Default maximum free testing grant per authorization.
pub const DEFAULT_MAX_TESTING_GRANT: u64 = 10_000 * MICRO_BETA_MINI_PER_BETA_MINI;
/// Default maximum participation grant per authorization.
pub const DEFAULT_MAX_PARTICIPATION_GRANT: u64 = 100_000 * MICRO_BETA_MINI_PER_BETA_MINI;
/// Default maximum total issued in one resettable beta epoch.
pub const DEFAULT_MAX_EPOCH_SUPPLY: u64 = 1_000_000_000 * MICRO_BETA_MINI_PER_BETA_MINI;

const PAYLOAD_VERSION: u8 = 1;
const MAX_ITEMS: usize = 64;
const MAX_SHORT_TEXT_BYTES: usize = 512;
const MAX_TEXT_BYTES: usize = 16 * 1024;
const MAX_EVIDENCE_REF_BYTES: usize = 1024;

/// Result type for the beta subsystem.
pub type Result<T> = core::result::Result<T, BetaError>;

/// Failures from beta-object validation or the reference Beta MINI ledger.
#[derive(Debug)]
#[non_exhaustive]
pub enum BetaError {
    /// A signed object or field failed strict beta-schema validation.
    InvalidObject,
    /// A configured beta accounting policy is internally inconsistent.
    InvalidPolicy,
    /// A grant belongs to a different beta epoch.
    WrongEpoch,
    /// A grant authorization or contribution has already been applied.
    DuplicateGrant,
    /// Applying the grant would exceed a per-grant or epoch-supply bound.
    GrantLimitExceeded,
    /// A transfer attempted to spend more Beta MINI than the account holds.
    InsufficientBalance,
    /// Store failure.
    Store(StoreError),
    /// Object construction failure.
    Object(mini_objects::ObjectError),
}

impl core::fmt::Display for BetaError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidObject => write!(f, "invalid beta object"),
            Self::InvalidPolicy => write!(f, "invalid beta policy"),
            Self::WrongEpoch => write!(f, "beta grant belongs to a different epoch"),
            Self::DuplicateGrant => write!(f, "beta grant or contribution already applied"),
            Self::GrantLimitExceeded => write!(f, "beta grant or epoch limit exceeded"),
            Self::InsufficientBalance => write!(f, "insufficient Beta MINI balance"),
            Self::Store(e) => write!(f, "store: {e}"),
            Self::Object(e) => write!(f, "object: {e}"),
        }
    }
}

impl std::error::Error for BetaError {}

impl From<StoreError> for BetaError {
    fn from(value: StoreError) -> Self {
        Self::Store(value)
    }
}

impl From<mini_objects::ObjectError> for BetaError {
    fn from(value: mini_objects::ObjectError) -> Self {
        Self::Object(value)
    }
}

/// A reset boundary for test currency. A new epoch starts from zero balances.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BetaEpochId([u8; 32]);

impl BetaEpochId {
    /// Construct a non-zero beta epoch identifier.
    pub fn new(bytes: [u8; 32]) -> Result<Self> {
        if bytes == [0; 32] {
            return Err(BetaError::InvalidObject);
        }
        Ok(Self(bytes))
    }

    /// Raw canonical bytes.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Fresh per-report handle. It is not an identity and must not be reused to
/// create cross-submission continuity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SubmissionTag([u8; 32]);

impl SubmissionTag {
    /// Construct a non-zero artifact-scoped submission tag.
    pub fn new(bytes: [u8; 32]) -> Result<Self> {
        if bytes == [0; 32] {
            return Err(BetaError::InvalidObject);
        }
        Ok(Self(bytes))
    }

    /// Raw canonical bytes.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Fresh handle used to connect one accepted contribution to its reward claim.
/// It is deliberately not a persistent contributor identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ClaimTag([u8; 32]);

impl ClaimTag {
    /// Construct a non-zero one-contribution claim tag.
    pub fn new(bytes: [u8; 32]) -> Result<Self> {
        if bytes == [0; 32] {
            return Err(BetaError::InvalidObject);
        }
        Ok(Self(bytes))
    }

    /// Raw canonical bytes.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Opaque Beta MINI account. It carries no identity or authority semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BetaAccountId([u8; 32]);

impl BetaAccountId {
    /// Construct a non-zero beta account identifier.
    pub fn new(bytes: [u8; 32]) -> Result<Self> {
        if bytes == [0; 32] {
            return Err(BetaError::InvalidObject);
        }
        Ok(Self(bytes))
    }

    /// Raw canonical bytes.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Evidence class for a beta finding. This prevents emulator/Rust evidence
/// from being silently presented as physical-device evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvidenceClass {
    /// Evidence from a real physical device.
    PhysicalDevice,
    /// Evidence from an emulator or simulator.
    Emulator,
    /// Rust/toolchain or host-side executable evidence.
    RustToolchain,
    /// Research/model/protocol-analysis evidence.
    Research,
    /// Independent external review evidence.
    ExternalReview,
    /// Accessibility or usability evidence.
    Accessibility,
    /// Other explicitly described evidence.
    Other,
}

impl EvidenceClass {
    /// Stable wire label.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PhysicalDevice => "physical-device",
            Self::Emulator => "emulator",
            Self::RustToolchain => "rust-toolchain",
            Self::Research => "research",
            Self::ExternalReview => "external-review",
            Self::Accessibility => "accessibility",
            Self::Other => "other",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "physical-device" => Self::PhysicalDevice,
            "emulator" => Self::Emulator,
            "rust-toolchain" => Self::RustToolchain,
            "research" => Self::Research,
            "external-review" => Self::ExternalReview,
            "accessibility" => Self::Accessibility,
            "other" => Self::Other,
            _ => return None,
        })
    }
}

/// Reporter-claimed impact. Triage may later disposition it differently.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FindingSeverity {
    /// Observation or improvement with no demonstrated failure.
    Observation,
    /// Low-impact defect.
    Low,
    /// Material but bounded defect.
    Medium,
    /// Serious security/reliability/usability defect.
    High,
    /// Safety-critical or release-blocking defect.
    Critical,
}

impl FindingSeverity {
    /// Stable wire label.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Observation => "observation",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Critical => "critical",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "observation" => Self::Observation,
            "low" => Self::Low,
            "medium" => Self::Medium,
            "high" => Self::High,
            "critical" => Self::Critical,
            _ => return None,
        })
    }
}

/// Append-only triage/disposition state for a finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FindingState {
    /// Finding was acknowledged as actionable evidence.
    Accepted,
    /// Finding duplicates an already-recorded report.
    Duplicate,
    /// More evidence is required.
    NeedsInformation,
    /// The issue is fixed in an exact linked state.
    Fixed,
    /// The described result could not be reproduced with the recorded evidence.
    CannotReproduce,
    /// The claim was examined and rejected with rationale.
    Rejected,
}

impl FindingState {
    /// Stable wire label.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::Duplicate => "duplicate",
            Self::NeedsInformation => "needs-information",
            Self::Fixed => "fixed",
            Self::CannotReproduce => "cannot-reproduce",
            Self::Rejected => "rejected",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "accepted" => Self::Accepted,
            "duplicate" => Self::Duplicate,
            "needs-information" => Self::NeedsInformation,
            "fixed" => Self::Fixed,
            "cannot-reproduce" => Self::CannotReproduce,
            "rejected" => Self::Rejected,
            _ => return None,
        })
    }
}

/// Kind of useful work represented by an accepted contribution receipt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContributionKind {
    /// New beta test evidence.
    Testing,
    /// Reproducing or falsifying an existing finding.
    Reproduction,
    /// Device/hardware matrix work.
    Hardware,
    /// Accessibility or usability work.
    Accessibility,
    /// Documentation improvement.
    Documentation,
    /// Security/threat-model work.
    Security,
    /// Research/protocol analysis.
    Research,
    /// Code or tests.
    Code,
    /// Technical review.
    Review,
    /// Build/release reproducibility work.
    Reproducibility,
    /// Operational work that produced inspectable evidence.
    Operations,
}

impl ContributionKind {
    /// Stable wire label.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Testing => "testing",
            Self::Reproduction => "reproduction",
            Self::Hardware => "hardware",
            Self::Accessibility => "accessibility",
            Self::Documentation => "documentation",
            Self::Security => "security",
            Self::Research => "research",
            Self::Code => "code",
            Self::Review => "review",
            Self::Reproducibility => "reproducibility",
            Self::Operations => "operations",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "testing" => Self::Testing,
            "reproduction" => Self::Reproduction,
            "hardware" => Self::Hardware,
            "accessibility" => Self::Accessibility,
            "documentation" => Self::Documentation,
            "security" => Self::Security,
            "research" => Self::Research,
            "code" => Self::Code,
            "review" => Self::Review,
            "reproducibility" => Self::Reproducibility,
            "operations" => Self::Operations,
            _ => return None,
        })
    }
}

/// Why Beta MINI is being granted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrantClass {
    /// Free test balance used to exercise product flows.
    Testing,
    /// Recognition for an accepted contribution artifact.
    Participation,
}

impl GrantClass {
    /// Stable wire label.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Testing => "testing",
            Self::Participation => "participation",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "testing" => Self::Testing,
            "participation" => Self::Participation,
            _ => return None,
        })
    }
}

/// A beta campaign/test mission published into the Forge object substrate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BetaCampaign {
    /// Content-addressed campaign id.
    pub id: ObjectId,
    /// Signer that recorded the campaign. Not a tester identity.
    pub record_author: Did,
    /// Exact release/commit/object state under test.
    pub target_id: ObjectId,
    /// Resettable Beta MINI epoch used by this campaign.
    pub epoch: BetaEpochId,
    /// Short public title.
    pub title: String,
    /// Test instructions and safety limits.
    pub instructions: String,
    /// Routes such as `one-phone`, `two-phone`, `ble`, `accessibility`.
    pub routes: Vec<String>,
    /// Campaign start time.
    pub starts_ms: u64,
    /// Campaign end time.
    pub ends_ms: u64,
    /// Default free test-currency grant for this campaign.
    pub default_testing_grant: u64,
    /// Object timestamp.
    pub timestamp_ms: u64,
    /// Signer-scoped sequence.
    pub sequence: u64,
}

/// Structured beta evidence. There is intentionally no contributor identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BetaFinding {
    /// Content-addressed finding id.
    pub id: ObjectId,
    /// Intake/record signer, not the anonymous tester.
    pub record_author: Did,
    /// Campaign being tested.
    pub campaign_id: ObjectId,
    /// Fresh artifact-scoped submission handle.
    pub submission_tag: SubmissionTag,
    /// Evidence class.
    pub evidence_class: EvidenceClass,
    /// Reporter-claimed severity.
    pub severity: FindingSeverity,
    /// Affected component/surface.
    pub component: String,
    /// Short finding summary.
    pub summary: String,
    /// Reproducible environment description.
    pub environment: String,
    /// Reproduction/test steps.
    pub steps: String,
    /// Expected result.
    pub expected: String,
    /// Observed result.
    pub observed: String,
    /// Evidence references, hashes, or redacted artifact locations.
    pub evidence: Vec<String>,
    /// Explicit statement of what the report does not prove.
    pub limitations: String,
    /// Reporter/intake assertion that secrets and unnecessary personal data were redacted.
    pub privacy_redacted: bool,
    /// Object timestamp.
    pub timestamp_ms: u64,
    /// Signer-scoped sequence.
    pub sequence: u64,
}

/// Append-only triage record for a finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FindingDisposition {
    /// Content-addressed disposition id.
    pub id: ObjectId,
    /// Record signer.
    pub record_author: Did,
    /// Finding being dispositioned.
    pub finding_id: ObjectId,
    /// Optional Forge task created from the finding.
    pub task_id: Option<ObjectId>,
    /// Optional exact state that fixed or superseded the finding.
    pub resolved_in: Option<ObjectId>,
    /// Disposition state.
    pub state: FindingState,
    /// Human-readable rationale.
    pub rationale: String,
    /// Object timestamp.
    pub timestamp_ms: u64,
    /// Signer-scoped sequence.
    pub sequence: u64,
}

/// Accepted useful work. The claim tag is fresh for this receipt and is not a
/// stable contributor identity or reputation key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContributionReceipt {
    /// Content-addressed receipt id.
    pub id: ObjectId,
    /// Record signer.
    pub record_author: Did,
    /// Exact finding/task/review/commit/evidence object accepted as the source.
    pub source_id: ObjectId,
    /// Optional beta campaign in which this contribution occurred.
    pub campaign_id: Option<ObjectId>,
    /// Fresh one-contribution reward-claim handle.
    pub claim_tag: ClaimTag,
    /// Contribution category.
    pub kind: ContributionKind,
    /// Why this work was useful/accepted.
    pub summary: String,
    /// Reproducible evidence supporting acceptance.
    pub evidence: Vec<String>,
    /// Object timestamp.
    pub timestamp_ms: u64,
    /// Signer-scoped sequence.
    pub sequence: u64,
}

/// Evidence authorizing one Beta MINI grant. It is not production value and
/// does not itself decide which signers a node accepts as beta authorizers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BetaGrantAuthorization {
    /// Content-addressed authorization id.
    pub id: ObjectId,
    /// Record signer.
    pub record_author: Did,
    /// Campaign whose beta epoch and policy apply.
    pub campaign_id: ObjectId,
    /// Participation grants must link their accepted contribution receipt.
    pub contribution_id: Option<ObjectId>,
    /// Resettable beta epoch.
    pub epoch: BetaEpochId,
    /// Opaque beta account destination.
    pub account: BetaAccountId,
    /// Testing or participation grant.
    pub class: GrantClass,
    /// Amount in micro-BETA-MINI.
    pub amount: u64,
    /// Public reason, never a contributor identity.
    pub memo: String,
    /// Object timestamp.
    pub timestamp_ms: u64,
    /// Signer-scoped sequence.
    pub sequence: u64,
}

/// Create and store a Forge-compatible beta campaign.
#[allow(clippy::too_many_arguments)]
pub fn create_campaign<B: Backend>(
    store: &mut Store<B>,
    record_human: &Did,
    record_device: &Controller,
    target_id: &ObjectId,
    epoch: BetaEpochId,
    title: &str,
    instructions: &str,
    routes: &[String],
    starts_ms: u64,
    ends_ms: u64,
    default_testing_grant: u64,
    timestamp_ms: u64,
    sequence: u64,
) -> Result<Object> {
    let _ = store.get(target_id)?;
    validate_text(title, MAX_SHORT_TEXT_BYTES, true)?;
    validate_text(instructions, MAX_TEXT_BYTES, true)?;
    validate_list(routes, MAX_SHORT_TEXT_BYTES, true)?;
    if ends_ms <= starts_ms || default_testing_grant == 0 {
        return Err(BetaError::InvalidObject);
    }

    let mut payload = vec![PAYLOAD_VERSION];
    payload.extend_from_slice(epoch.as_bytes());
    put_u64(&mut payload, starts_ms);
    put_u64(&mut payload, ends_ms);
    put_u64(&mut payload, default_testing_grant);
    put_str(&mut payload, title);
    put_str(&mut payload, instructions);
    put_list(&mut payload, routes);

    let object = ObjectBuilder::new(ObjectType::Custom(BETA_CAMPAIGN_TYPE.to_string()))
        .timestamp_ms(timestamp_ms)
        .sequence(sequence)
        .payload(Payload::Public(payload))
        .link("target", target_id.clone())
        .sign(record_human, record_device)?;
    store.insert(&object)?;
    Ok(object)
}

/// Create and store a structured beta finding. The submission tag must be fresh
/// for this report; no contributor DID or username is encoded.
#[allow(clippy::too_many_arguments)]
pub fn create_finding<B: Backend>(
    store: &mut Store<B>,
    record_human: &Did,
    record_device: &Controller,
    campaign_id: &ObjectId,
    submission_tag: SubmissionTag,
    evidence_class: EvidenceClass,
    severity: FindingSeverity,
    component: &str,
    summary: &str,
    environment: &str,
    steps: &str,
    expected: &str,
    observed: &str,
    evidence: &[String],
    limitations: &str,
    privacy_redacted: bool,
    timestamp_ms: u64,
    sequence: u64,
) -> Result<Object> {
    let campaign = store.get(campaign_id)?;
    let _ = parse_campaign_object(&campaign)?;
    validate_text(component, MAX_SHORT_TEXT_BYTES, true)?;
    validate_text(summary, MAX_SHORT_TEXT_BYTES, true)?;
    validate_text(environment, MAX_TEXT_BYTES, true)?;
    validate_text(steps, MAX_TEXT_BYTES, true)?;
    validate_text(expected, MAX_TEXT_BYTES, true)?;
    validate_text(observed, MAX_TEXT_BYTES, true)?;
    validate_list(evidence, MAX_EVIDENCE_REF_BYTES, true)?;
    validate_text(limitations, MAX_TEXT_BYTES, true)?;
    if !privacy_redacted {
        return Err(BetaError::InvalidObject);
    }

    let mut payload = vec![PAYLOAD_VERSION];
    payload.extend_from_slice(submission_tag.as_bytes());
    put_str(&mut payload, evidence_class.as_str());
    put_str(&mut payload, severity.as_str());
    put_str(&mut payload, component);
    put_str(&mut payload, summary);
    put_str(&mut payload, environment);
    put_str(&mut payload, steps);
    put_str(&mut payload, expected);
    put_str(&mut payload, observed);
    put_list(&mut payload, evidence);
    put_str(&mut payload, limitations);
    payload.push(u8::from(privacy_redacted));

    let object = ObjectBuilder::new(ObjectType::Custom(BETA_FINDING_TYPE.to_string()))
        .timestamp_ms(timestamp_ms)
        .sequence(sequence)
        .payload(Payload::Public(payload))
        .link("campaign", campaign_id.clone())
        .sign(record_human, record_device)?;
    store.insert(&object)?;
    Ok(object)
}

/// Append a triage/disposition record without mutating the original finding.
#[allow(clippy::too_many_arguments)]
pub fn create_finding_disposition<B: Backend>(
    store: &mut Store<B>,
    record_human: &Did,
    record_device: &Controller,
    finding_id: &ObjectId,
    task_id: Option<&ObjectId>,
    resolved_in: Option<&ObjectId>,
    state: FindingState,
    rationale: &str,
    timestamp_ms: u64,
    sequence: u64,
) -> Result<Object> {
    let finding = store.get(finding_id)?;
    let _ = parse_finding_object(&finding)?;
    if let Some(task) = task_id {
        let _ = store.get(task)?;
    }
    if let Some(resolved) = resolved_in {
        let _ = store.get(resolved)?;
    }
    validate_text(rationale, MAX_TEXT_BYTES, true)?;
    if state == FindingState::Fixed && resolved_in.is_none() {
        return Err(BetaError::InvalidObject);
    }

    let mut payload = vec![PAYLOAD_VERSION];
    put_str(&mut payload, state.as_str());
    put_str(&mut payload, rationale);

    let mut builder = ObjectBuilder::new(ObjectType::Custom(
        BETA_FINDING_DISPOSITION_TYPE.to_string(),
    ))
    .timestamp_ms(timestamp_ms)
    .sequence(sequence)
    .payload(Payload::Public(payload))
    .link("finding", finding_id.clone());
    if let Some(task) = task_id {
        builder = builder.link("task", task.clone());
    }
    if let Some(resolved) = resolved_in {
        builder = builder.link("resolved", resolved.clone());
    }
    let object = builder.sign(record_human, record_device)?;
    store.insert(&object)?;
    Ok(object)
}

/// Record accepted useful work without creating a persistent contributor
/// profile. `source_id` is the exact evidence/work object being recognized.
#[allow(clippy::too_many_arguments)]
pub fn create_contribution_receipt<B: Backend>(
    store: &mut Store<B>,
    record_human: &Did,
    record_device: &Controller,
    source_id: &ObjectId,
    campaign_id: Option<&ObjectId>,
    claim_tag: ClaimTag,
    kind: ContributionKind,
    summary: &str,
    evidence: &[String],
    timestamp_ms: u64,
    sequence: u64,
) -> Result<Object> {
    let _ = store.get(source_id)?;
    if let Some(campaign_id) = campaign_id {
        let campaign = store.get(campaign_id)?;
        let _ = parse_campaign_object(&campaign)?;
    }
    validate_text(summary, MAX_TEXT_BYTES, true)?;
    validate_list(evidence, MAX_EVIDENCE_REF_BYTES, true)?;

    let mut payload = vec![PAYLOAD_VERSION];
    payload.extend_from_slice(claim_tag.as_bytes());
    put_str(&mut payload, kind.as_str());
    put_str(&mut payload, summary);
    put_list(&mut payload, evidence);

    let mut builder = ObjectBuilder::new(ObjectType::Custom(BETA_CONTRIBUTION_TYPE.to_string()))
        .timestamp_ms(timestamp_ms)
        .sequence(sequence)
        .payload(Payload::Public(payload))
        .link("source", source_id.clone());
    if let Some(campaign_id) = campaign_id {
        builder = builder.link("campaign", campaign_id.clone());
    }
    let object = builder.sign(record_human, record_device)?;
    store.insert(&object)?;
    Ok(object)
}

/// Create Beta MINI grant evidence. Participation grants require an accepted
/// contribution from the same campaign. Testing grants are capped by the
/// campaign's declared default grant before they even reach the ledger policy.
#[allow(clippy::too_many_arguments)]
pub fn create_grant_authorization<B: Backend>(
    store: &mut Store<B>,
    record_human: &Did,
    record_device: &Controller,
    campaign_id: &ObjectId,
    contribution_id: Option<&ObjectId>,
    epoch: BetaEpochId,
    account: BetaAccountId,
    class: GrantClass,
    amount: u64,
    memo: &str,
    timestamp_ms: u64,
    sequence: u64,
) -> Result<Object> {
    let campaign_obj = store.get(campaign_id)?;
    let campaign = parse_campaign_object(&campaign_obj)?;
    if epoch != campaign.epoch || amount == 0 {
        return Err(BetaError::InvalidObject);
    }
    if class == GrantClass::Testing && amount > campaign.default_testing_grant {
        return Err(BetaError::GrantLimitExceeded);
    }

    let contribution = match (class, contribution_id) {
        (GrantClass::Participation, Some(id)) => {
            let object = store.get(id)?;
            Some(parse_contribution_object(&object)?)
        }
        (GrantClass::Participation, None) => return Err(BetaError::InvalidObject),
        (GrantClass::Testing, Some(_)) => return Err(BetaError::InvalidObject),
        (GrantClass::Testing, None) => None,
    };
    if let Some(contribution) = contribution.as_ref() {
        if contribution.campaign_id.as_ref() != Some(campaign_id) {
            return Err(BetaError::InvalidObject);
        }
    }
    validate_text(memo, MAX_SHORT_TEXT_BYTES, true)?;

    let mut payload = vec![PAYLOAD_VERSION];
    payload.extend_from_slice(epoch.as_bytes());
    payload.extend_from_slice(account.as_bytes());
    put_str(&mut payload, class.as_str());
    put_u64(&mut payload, amount);
    put_str(&mut payload, memo);

    let mut builder = ObjectBuilder::new(ObjectType::Custom(BETA_GRANT_TYPE.to_string()))
        .timestamp_ms(timestamp_ms)
        .sequence(sequence)
        .payload(Payload::Public(payload))
        .link("campaign", campaign_id.clone());
    if let Some(contribution_id) = contribution_id {
        builder = builder.link("contribution", contribution_id.clone());
    }
    let object = builder.sign(record_human, record_device)?;
    store.insert(&object)?;
    Ok(object)
}

/// Strictly read a campaign object from the store.
pub fn read_campaign<B: Backend>(store: &Store<B>, id: &ObjectId) -> Result<BetaCampaign> {
    parse_campaign_object(&store.get(id)?)
}

/// Strictly read a finding object from the store.
pub fn read_finding<B: Backend>(store: &Store<B>, id: &ObjectId) -> Result<BetaFinding> {
    parse_finding_object(&store.get(id)?)
}

/// Strictly read a finding disposition from the store.
pub fn read_finding_disposition<B: Backend>(
    store: &Store<B>,
    id: &ObjectId,
) -> Result<FindingDisposition> {
    parse_finding_disposition_object(&store.get(id)?)
}

/// Strictly read an accepted contribution receipt from the store.
pub fn read_contribution_receipt<B: Backend>(
    store: &Store<B>,
    id: &ObjectId,
) -> Result<ContributionReceipt> {
    parse_contribution_object(&store.get(id)?)
}

/// Strictly read Beta MINI grant evidence from the store.
pub fn read_grant_authorization<B: Backend>(
    store: &Store<B>,
    id: &ObjectId,
) -> Result<BetaGrantAuthorization> {
    parse_grant_object(&store.get(id)?)
}

/// Strict parser for a campaign received through sync/Forge replication.
pub fn parse_campaign_object(object: &Object) -> Result<BetaCampaign> {
    ensure_type_and_links(object, BETA_CAMPAIGN_TYPE, &["target"])?;
    let bytes = public_payload(object)?;
    let mut off = 0usize;
    if take_u8(bytes, &mut off)? != PAYLOAD_VERSION {
        return Err(BetaError::InvalidObject);
    }
    let epoch = BetaEpochId::new(take_32(bytes, &mut off)?)?;
    let starts_ms = take_u64(bytes, &mut off)?;
    let ends_ms = take_u64(bytes, &mut off)?;
    let default_testing_grant = take_u64(bytes, &mut off)?;
    let title = take_str(bytes, &mut off, MAX_SHORT_TEXT_BYTES, true)?;
    let instructions = take_str(bytes, &mut off, MAX_TEXT_BYTES, true)?;
    let routes = take_list(bytes, &mut off, MAX_SHORT_TEXT_BYTES, true)?;
    if off != bytes.len() || ends_ms <= starts_ms || default_testing_grant == 0 {
        return Err(BetaError::InvalidObject);
    }
    Ok(BetaCampaign {
        id: object.id().clone(),
        record_author: object.author_human.clone(),
        target_id: required_link(object, "target")?,
        epoch,
        title,
        instructions,
        routes,
        starts_ms,
        ends_ms,
        default_testing_grant,
        timestamp_ms: object.timestamp_ms,
        sequence: object.sequence,
    })
}

/// Strict parser for a finding received through sync/Forge replication.
pub fn parse_finding_object(object: &Object) -> Result<BetaFinding> {
    ensure_type_and_links(object, BETA_FINDING_TYPE, &["campaign"])?;
    let bytes = public_payload(object)?;
    let mut off = 0usize;
    if take_u8(bytes, &mut off)? != PAYLOAD_VERSION {
        return Err(BetaError::InvalidObject);
    }
    let submission_tag = SubmissionTag::new(take_32(bytes, &mut off)?)?;
    let evidence_class =
        EvidenceClass::parse(&take_str(bytes, &mut off, MAX_SHORT_TEXT_BYTES, true)?)
            .ok_or(BetaError::InvalidObject)?;
    let severity = FindingSeverity::parse(&take_str(bytes, &mut off, MAX_SHORT_TEXT_BYTES, true)?)
        .ok_or(BetaError::InvalidObject)?;
    let component = take_str(bytes, &mut off, MAX_SHORT_TEXT_BYTES, true)?;
    let summary = take_str(bytes, &mut off, MAX_SHORT_TEXT_BYTES, true)?;
    let environment = take_str(bytes, &mut off, MAX_TEXT_BYTES, true)?;
    let steps = take_str(bytes, &mut off, MAX_TEXT_BYTES, true)?;
    let expected = take_str(bytes, &mut off, MAX_TEXT_BYTES, true)?;
    let observed = take_str(bytes, &mut off, MAX_TEXT_BYTES, true)?;
    let evidence = take_list(bytes, &mut off, MAX_EVIDENCE_REF_BYTES, true)?;
    let limitations = take_str(bytes, &mut off, MAX_TEXT_BYTES, true)?;
    let privacy_redacted = match take_u8(bytes, &mut off)? {
        1 => true,
        0 => false,
        _ => return Err(BetaError::InvalidObject),
    };
    if off != bytes.len() || !privacy_redacted {
        return Err(BetaError::InvalidObject);
    }
    Ok(BetaFinding {
        id: object.id().clone(),
        record_author: object.author_human.clone(),
        campaign_id: required_link(object, "campaign")?,
        submission_tag,
        evidence_class,
        severity,
        component,
        summary,
        environment,
        steps,
        expected,
        observed,
        evidence,
        limitations,
        privacy_redacted,
        timestamp_ms: object.timestamp_ms,
        sequence: object.sequence,
    })
}

/// Strict parser for an append-only finding disposition.
pub fn parse_finding_disposition_object(object: &Object) -> Result<FindingDisposition> {
    ensure_type_and_links(
        object,
        BETA_FINDING_DISPOSITION_TYPE,
        &["finding", "task", "resolved"],
    )?;
    let finding_id = required_link(object, "finding")?;
    let task_id = optional_link(object, "task")?;
    let resolved_in = optional_link(object, "resolved")?;
    let bytes = public_payload(object)?;
    let mut off = 0usize;
    if take_u8(bytes, &mut off)? != PAYLOAD_VERSION {
        return Err(BetaError::InvalidObject);
    }
    let state = FindingState::parse(&take_str(bytes, &mut off, MAX_SHORT_TEXT_BYTES, true)?)
        .ok_or(BetaError::InvalidObject)?;
    let rationale = take_str(bytes, &mut off, MAX_TEXT_BYTES, true)?;
    if off != bytes.len() || (state == FindingState::Fixed && resolved_in.is_none()) {
        return Err(BetaError::InvalidObject);
    }
    Ok(FindingDisposition {
        id: object.id().clone(),
        record_author: object.author_human.clone(),
        finding_id,
        task_id,
        resolved_in,
        state,
        rationale,
        timestamp_ms: object.timestamp_ms,
        sequence: object.sequence,
    })
}

/// Strict parser for an accepted contribution receipt.
pub fn parse_contribution_object(object: &Object) -> Result<ContributionReceipt> {
    ensure_type_and_links(object, BETA_CONTRIBUTION_TYPE, &["source", "campaign"])?;
    let bytes = public_payload(object)?;
    let mut off = 0usize;
    if take_u8(bytes, &mut off)? != PAYLOAD_VERSION {
        return Err(BetaError::InvalidObject);
    }
    let claim_tag = ClaimTag::new(take_32(bytes, &mut off)?)?;
    let kind = ContributionKind::parse(&take_str(bytes, &mut off, MAX_SHORT_TEXT_BYTES, true)?)
        .ok_or(BetaError::InvalidObject)?;
    let summary = take_str(bytes, &mut off, MAX_TEXT_BYTES, true)?;
    let evidence = take_list(bytes, &mut off, MAX_EVIDENCE_REF_BYTES, true)?;
    if off != bytes.len() {
        return Err(BetaError::InvalidObject);
    }
    Ok(ContributionReceipt {
        id: object.id().clone(),
        record_author: object.author_human.clone(),
        source_id: required_link(object, "source")?,
        campaign_id: optional_link(object, "campaign")?,
        claim_tag,
        kind,
        summary,
        evidence,
        timestamp_ms: object.timestamp_ms,
        sequence: object.sequence,
    })
}

/// Strict parser for Beta MINI grant authorization evidence.
pub fn parse_grant_object(object: &Object) -> Result<BetaGrantAuthorization> {
    ensure_type_and_links(object, BETA_GRANT_TYPE, &["campaign", "contribution"])?;
    let bytes = public_payload(object)?;
    let mut off = 0usize;
    if take_u8(bytes, &mut off)? != PAYLOAD_VERSION {
        return Err(BetaError::InvalidObject);
    }
    let epoch = BetaEpochId::new(take_32(bytes, &mut off)?)?;
    let account = BetaAccountId::new(take_32(bytes, &mut off)?)?;
    let class = GrantClass::parse(&take_str(bytes, &mut off, MAX_SHORT_TEXT_BYTES, true)?)
        .ok_or(BetaError::InvalidObject)?;
    let amount = take_u64(bytes, &mut off)?;
    let memo = take_str(bytes, &mut off, MAX_SHORT_TEXT_BYTES, true)?;
    let contribution_id = optional_link(object, "contribution")?;
    if off != bytes.len() || amount == 0 {
        return Err(BetaError::InvalidObject);
    }
    match (class, contribution_id.as_ref()) {
        (GrantClass::Participation, Some(_)) | (GrantClass::Testing, None) => {}
        (GrantClass::Participation, None) | (GrantClass::Testing, Some(_)) => {
            return Err(BetaError::InvalidObject)
        }
    }
    Ok(BetaGrantAuthorization {
        id: object.id().clone(),
        record_author: object.author_human.clone(),
        campaign_id: required_link(object, "campaign")?,
        contribution_id,
        epoch,
        account,
        class,
        amount,
        memo,
        timestamp_ms: object.timestamp_ms,
        sequence: object.sequence,
    })
}

/// Accounting bounds for a resettable Beta MINI epoch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BetaMiniPolicy {
    /// Maximum one testing grant may issue.
    pub max_testing_grant: u64,
    /// Maximum one accepted-participation grant may issue.
    pub max_participation_grant: u64,
    /// Maximum total issuance during this epoch.
    pub max_epoch_supply: u64,
}

impl BetaMiniPolicy {
    /// Conservative default for broad beta testing. These are test units only.
    pub fn open_beta_default() -> Self {
        Self {
            max_testing_grant: DEFAULT_MAX_TESTING_GRANT,
            max_participation_grant: DEFAULT_MAX_PARTICIPATION_GRANT,
            max_epoch_supply: DEFAULT_MAX_EPOCH_SUPPLY,
        }
    }

    fn validate(self) -> Result<()> {
        if self.max_testing_grant == 0
            || self.max_participation_grant == 0
            || self.max_epoch_supply == 0
            || self.max_testing_grant > self.max_epoch_supply
            || self.max_participation_grant > self.max_epoch_supply
        {
            return Err(BetaError::InvalidPolicy);
        }
        Ok(())
    }
}

/// Reference accounting state for one Beta MINI epoch.
///
/// This ledger intentionally accepts already-selected grant authorizations; it
/// does not decide which signer has authority to issue them. That selection is
/// an outer beta/Forge policy concern and, critically, balances never feed back
/// into it. The apply path nevertheless re-loads every referenced signed object
/// from the store and enforces object/campaign/contribution consistency; a
/// convenience-builder check is never treated as a security boundary.
#[derive(Debug, Clone)]
pub struct BetaMiniLedger {
    epoch: BetaEpochId,
    policy: BetaMiniPolicy,
    balances: HashMap<BetaAccountId, u64>,
    applied_grants: HashSet<ObjectId>,
    spent_contributions: HashSet<ObjectId>,
    total_issued: u64,
}

impl BetaMiniLedger {
    /// Start a fresh zero-balance beta epoch.
    pub fn new(epoch: BetaEpochId, policy: BetaMiniPolicy) -> Result<Self> {
        policy.validate()?;
        Ok(Self {
            epoch,
            policy,
            balances: HashMap::new(),
            applied_grants: HashSet::new(),
            spent_contributions: HashSet::new(),
            total_issued: 0,
        })
    }

    /// Current beta epoch.
    pub fn epoch(&self) -> BetaEpochId {
        self.epoch
    }

    /// Total Beta MINI ever issued in this epoch, in micro units.
    pub fn total_issued(&self) -> u64 {
        self.total_issued
    }

    /// Current balance for an opaque beta account.
    pub fn balance(&self, account: &BetaAccountId) -> u64 {
        self.balances.get(account).copied().unwrap_or(0)
    }

    /// Apply one stored, strictly parsed grant exactly once.
    ///
    /// The ledger reloads the grant, campaign and (for participation grants)
    /// contribution from the content-addressed store. This prevents a caller
    /// from bypassing campaign caps or contribution linkage by constructing a
    /// `BetaGrantAuthorization` struct directly instead of using the builder.
    pub fn apply_grant<B: Backend>(&mut self, store: &Store<B>, grant_id: &ObjectId) -> Result<()> {
        let grant = parse_grant_object(&store.get(grant_id)?)?;
        if grant.epoch != self.epoch {
            return Err(BetaError::WrongEpoch);
        }
        if self.applied_grants.contains(&grant.id) {
            return Err(BetaError::DuplicateGrant);
        }

        let campaign = parse_campaign_object(&store.get(&grant.campaign_id)?)?;
        if campaign.epoch != grant.epoch {
            return Err(BetaError::InvalidObject);
        }

        let contribution_to_spend = match grant.class {
            GrantClass::Testing => {
                let per_grant_limit = self
                    .policy
                    .max_testing_grant
                    .min(campaign.default_testing_grant);
                if grant.amount > per_grant_limit {
                    return Err(BetaError::GrantLimitExceeded);
                }
                None
            }
            GrantClass::Participation => {
                if grant.amount > self.policy.max_participation_grant {
                    return Err(BetaError::GrantLimitExceeded);
                }
                let contribution_id = grant
                    .contribution_id
                    .as_ref()
                    .ok_or(BetaError::InvalidObject)?;
                let contribution = parse_contribution_object(&store.get(contribution_id)?)?;
                if contribution.campaign_id.as_ref() != Some(&grant.campaign_id) {
                    return Err(BetaError::InvalidObject);
                }
                // Keyed on the underlying accepted work (`source_id`), not the
                // receipt object's own id. `create_contribution_receipt` does
                // not itself enforce that only one receipt is ever created per
                // `source_id` -- it is a bookkeeping record, not a uniqueness
                // authority -- so two distinct receipt objects can otherwise
                // reference the identical accepted work. Keying on the receipt
                // id would let each such receipt back its own participation
                // grant, doubling the reward for one piece of work; keying on
                // `source_id` makes the second grant attempt collide exactly
                // like a genuine duplicate, regardless of how many receipt
                // objects the record signer created for it.
                if self.spent_contributions.contains(&contribution.source_id) {
                    return Err(BetaError::DuplicateGrant);
                }
                Some(contribution.source_id.clone())
            }
        };

        let total_issued = self
            .total_issued
            .checked_add(grant.amount)
            .ok_or(BetaError::GrantLimitExceeded)?;
        if total_issued > self.policy.max_epoch_supply {
            return Err(BetaError::GrantLimitExceeded);
        }
        let current = self.balance(&grant.account);
        let balance = current
            .checked_add(grant.amount)
            .ok_or(BetaError::GrantLimitExceeded)?;

        self.balances.insert(grant.account, balance);
        self.total_issued = total_issued;
        self.applied_grants.insert(grant.id.clone());
        if let Some(contribution_id) = contribution_to_spend {
            self.spent_contributions.insert(contribution_id);
        }
        Ok(())
    }

    /// Transfer Beta MINI between opaque accounts. Transfers mint nothing and
    /// confer no authority.
    pub fn transfer(
        &mut self,
        from: &BetaAccountId,
        to: &BetaAccountId,
        amount: u64,
    ) -> Result<()> {
        if amount == 0 {
            return Err(BetaError::InvalidObject);
        }
        let from_balance = self.balance(from);
        if from_balance < amount {
            return Err(BetaError::InsufficientBalance);
        }
        if from == to {
            return Ok(());
        }
        let to_balance = self
            .balance(to)
            .checked_add(amount)
            .ok_or(BetaError::GrantLimitExceeded)?;
        self.balances.insert(*from, from_balance - amount);
        self.balances.insert(*to, to_balance);
        Ok(())
    }

    /// Start a different epoch with zero balances, zero issued supply, and no
    /// applied-grant or spent-contribution history. There is deliberately no
    /// carry-over or conversion mechanism.
    pub fn rollover(&self, new_epoch: BetaEpochId) -> Result<Self> {
        if new_epoch == self.epoch {
            return Err(BetaError::WrongEpoch);
        }
        Self::new(new_epoch, self.policy)
    }
}

fn validate_text(value: &str, max: usize, required: bool) -> Result<()> {
    if value.len() > max || (required && value.trim().is_empty()) {
        return Err(BetaError::InvalidObject);
    }
    Ok(())
}

fn validate_list(values: &[String], max_item: usize, required: bool) -> Result<()> {
    if values.len() > MAX_ITEMS || (required && values.is_empty()) {
        return Err(BetaError::InvalidObject);
    }
    for value in values {
        validate_text(value, max_item, true)?;
    }
    Ok(())
}

fn put_u64(out: &mut Vec<u8>, value: u64) {
    out.extend_from_slice(&value.to_be_bytes());
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

fn take_u8(bytes: &[u8], off: &mut usize) -> Result<u8> {
    let value = *bytes.get(*off).ok_or(BetaError::InvalidObject)?;
    *off += 1;
    Ok(value)
}

fn take_u64(bytes: &[u8], off: &mut usize) -> Result<u64> {
    let end = (*off).checked_add(8).ok_or(BetaError::InvalidObject)?;
    let raw = bytes.get(*off..end).ok_or(BetaError::InvalidObject)?;
    *off = end;
    Ok(u64::from_be_bytes(
        raw.try_into().map_err(|_| BetaError::InvalidObject)?,
    ))
}

fn take_32(bytes: &[u8], off: &mut usize) -> Result<[u8; 32]> {
    let end = (*off).checked_add(32).ok_or(BetaError::InvalidObject)?;
    let raw = bytes.get(*off..end).ok_or(BetaError::InvalidObject)?;
    *off = end;
    raw.try_into().map_err(|_| BetaError::InvalidObject)
}

fn take_str(bytes: &[u8], off: &mut usize, max: usize, required: bool) -> Result<String> {
    let len_end = (*off).checked_add(4).ok_or(BetaError::InvalidObject)?;
    let len_bytes = bytes.get(*off..len_end).ok_or(BetaError::InvalidObject)?;
    let len =
        u32::from_be_bytes(len_bytes.try_into().map_err(|_| BetaError::InvalidObject)?) as usize;
    *off = len_end;
    if len > max || (required && len == 0) {
        return Err(BetaError::InvalidObject);
    }
    let end = (*off).checked_add(len).ok_or(BetaError::InvalidObject)?;
    let value = bytes.get(*off..end).ok_or(BetaError::InvalidObject)?;
    *off = end;
    let value = String::from_utf8(value.to_vec()).map_err(|_| BetaError::InvalidObject)?;
    if required && value.trim().is_empty() {
        return Err(BetaError::InvalidObject);
    }
    Ok(value)
}

fn take_list(
    bytes: &[u8],
    off: &mut usize,
    max_item: usize,
    required: bool,
) -> Result<Vec<String>> {
    let count_end = (*off).checked_add(4).ok_or(BetaError::InvalidObject)?;
    let count_bytes = bytes.get(*off..count_end).ok_or(BetaError::InvalidObject)?;
    let count = u32::from_be_bytes(
        count_bytes
            .try_into()
            .map_err(|_| BetaError::InvalidObject)?,
    ) as usize;
    *off = count_end;
    if count > MAX_ITEMS || (required && count == 0) {
        return Err(BetaError::InvalidObject);
    }
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        out.push(take_str(bytes, off, max_item, true)?);
    }
    Ok(out)
}

fn public_payload(object: &Object) -> Result<&[u8]> {
    match &object.payload {
        Payload::Public(bytes) => Ok(bytes),
        Payload::Encrypted(_) => Err(BetaError::InvalidObject),
    }
}

fn ensure_type_and_links(object: &Object, expected: &str, allowed: &[&str]) -> Result<()> {
    if object.object_type != ObjectType::Custom(expected.to_string()) {
        return Err(BetaError::InvalidObject);
    }
    for link in &object.links {
        if !allowed.contains(&link.rel.as_str()) {
            return Err(BetaError::InvalidObject);
        }
        if object
            .links
            .iter()
            .filter(|candidate| candidate.rel == link.rel)
            .count()
            != 1
        {
            return Err(BetaError::InvalidObject);
        }
    }
    Ok(())
}

fn required_link(object: &Object, rel: &str) -> Result<ObjectId> {
    optional_link(object, rel)?.ok_or(BetaError::InvalidObject)
}

fn optional_link(object: &Object, rel: &str) -> Result<Option<ObjectId>> {
    let mut matches = object.links.iter().filter(|link| link.rel == rel);
    let first = matches.next().map(|link| link.target.clone());
    if matches.next().is_some() {
        return Err(BetaError::InvalidObject);
    }
    Ok(first)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mini_store::MemoryBackend;

    fn signer(seed: u8) -> Controller {
        Controller::incept_single_from_seeds(&[seed; 32], &[seed.wrapping_add(1); 32]).unwrap()
    }

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    fn target(store: &mut Store<MemoryBackend>, signer: &Controller) -> ObjectId {
        let object = ObjectBuilder::new(ObjectType::Custom("mini/beta-target".to_string()))
            .payload(Payload::Public(b"exact beta build".to_vec()))
            .sign(&signer.did(), signer)
            .unwrap();
        store.insert(&object).unwrap();
        object.id().clone()
    }

    #[test]
    fn forge_beta_objects_round_trip_without_a_contributor_identity() {
        let record_signer = signer(10);
        let mut store = Store::new(MemoryBackend::new());
        let target = target(&mut store, &record_signer);
        let epoch = BetaEpochId::new([7; 32]).unwrap();
        let campaign = create_campaign(
            &mut store,
            &record_signer.did(),
            &record_signer,
            &target,
            epoch,
            "offline phones",
            "turn internet off and run the two-phone path",
            &strings(&["two-phone", "ble", "offline"]),
            100,
            1_000,
            500 * MICRO_BETA_MINI_PER_BETA_MINI,
            100,
            1,
        )
        .unwrap();
        let finding = create_finding(
            &mut store,
            &record_signer.did(),
            &record_signer,
            campaign.id(),
            SubmissionTag::new([8; 32]).unwrap(),
            EvidenceClass::PhysicalDevice,
            FindingSeverity::High,
            "ble-mesh",
            "relay does not recover after reconnect",
            "two Android phones, internet disabled",
            "disconnect B, reconnect B, retry message",
            "message relays after reconnect",
            "message remains queued",
            &strings(&["sha256:deadbeef", "redacted-log:1"]),
            "does not prove other Android vendors",
            true,
            200,
            2,
        )
        .unwrap();
        let disposition = create_finding_disposition(
            &mut store,
            &record_signer.did(),
            &record_signer,
            finding.id(),
            None,
            Some(&target),
            FindingState::Fixed,
            "reconnect cleanup corrected and retested",
            300,
            3,
        )
        .unwrap();
        let contribution = create_contribution_receipt(
            &mut store,
            &record_signer.did(),
            &record_signer,
            disposition.id(),
            Some(campaign.id()),
            ClaimTag::new([9; 32]).unwrap(),
            ContributionKind::Testing,
            "found and verified the reconnect failure",
            &strings(&["finding", "fixed-state", "retest"]),
            400,
            4,
        )
        .unwrap();
        let grant = create_grant_authorization(
            &mut store,
            &record_signer.did(),
            &record_signer,
            campaign.id(),
            Some(contribution.id()),
            epoch,
            BetaAccountId::new([10; 32]).unwrap(),
            GrantClass::Participation,
            1_000 * MICRO_BETA_MINI_PER_BETA_MINI,
            "accepted reconnect testing contribution",
            500,
            5,
        )
        .unwrap();

        let parsed_campaign = read_campaign(&store, campaign.id()).unwrap();
        let parsed_finding = read_finding(&store, finding.id()).unwrap();
        let parsed_disposition = read_finding_disposition(&store, disposition.id()).unwrap();
        let parsed_contribution = read_contribution_receipt(&store, contribution.id()).unwrap();
        let parsed_grant = read_grant_authorization(&store, grant.id()).unwrap();

        assert_eq!(parsed_campaign.epoch, epoch);
        assert_eq!(
            parsed_finding.submission_tag,
            SubmissionTag::new([8; 32]).unwrap()
        );
        assert_eq!(parsed_disposition.state, FindingState::Fixed);
        assert_eq!(
            parsed_contribution.claim_tag,
            ClaimTag::new([9; 32]).unwrap()
        );
        assert_eq!(parsed_grant.class, GrantClass::Participation);
        assert_eq!(
            parsed_grant.contribution_id,
            Some(contribution.id().clone())
        );
        assert_eq!(parsed_finding.record_author, record_signer.did());
    }

    #[test]
    fn beta_mini_is_bounded_transferable_test_value_and_resets_between_epochs() {
        let record_signer = signer(30);
        let mut store = Store::new(MemoryBackend::new());
        let target = target(&mut store, &record_signer);
        let epoch = BetaEpochId::new([1; 32]).unwrap();
        let campaign = create_campaign(
            &mut store,
            &record_signer.did(),
            &record_signer,
            &target,
            epoch,
            "currency UX",
            "exercise grant and transfer flows",
            &strings(&["wallet", "transfer"]),
            1,
            10,
            1_000 * MICRO_BETA_MINI_PER_BETA_MINI,
            1,
            1,
        )
        .unwrap();
        let alice = BetaAccountId::new([2; 32]).unwrap();
        let bob = BetaAccountId::new([3; 32]).unwrap();
        let grant_object = create_grant_authorization(
            &mut store,
            &record_signer.did(),
            &record_signer,
            campaign.id(),
            None,
            epoch,
            alice,
            GrantClass::Testing,
            500 * MICRO_BETA_MINI_PER_BETA_MINI,
            "test balance",
            2,
            2,
        )
        .unwrap();
        let mut ledger = BetaMiniLedger::new(epoch, BetaMiniPolicy::open_beta_default()).unwrap();
        ledger.apply_grant(&store, grant_object.id()).unwrap();
        assert!(matches!(
            ledger.apply_grant(&store, grant_object.id()),
            Err(BetaError::DuplicateGrant)
        ));
        ledger
            .transfer(&alice, &bob, 125 * MICRO_BETA_MINI_PER_BETA_MINI)
            .unwrap();
        assert_eq!(ledger.balance(&alice), 375 * MICRO_BETA_MINI_PER_BETA_MINI);
        assert_eq!(ledger.balance(&bob), 125 * MICRO_BETA_MINI_PER_BETA_MINI);

        let next = ledger.rollover(BetaEpochId::new([4; 32]).unwrap()).unwrap();
        assert_eq!(next.balance(&alice), 0);
        assert_eq!(next.balance(&bob), 0);
        assert_eq!(next.total_issued(), 0);
    }

    #[test]
    fn participation_grants_require_accepted_contribution_evidence() {
        let signer = signer(50);
        let mut store = Store::new(MemoryBackend::new());
        let target = target(&mut store, &signer);
        let epoch = BetaEpochId::new([5; 32]).unwrap();
        let campaign = create_campaign(
            &mut store,
            &signer.did(),
            &signer,
            &target,
            epoch,
            "participation",
            "test participation grant rules",
            &strings(&["testing"]),
            1,
            10,
            100 * MICRO_BETA_MINI_PER_BETA_MINI,
            1,
            1,
        )
        .unwrap();
        let result = create_grant_authorization(
            &mut store,
            &signer.did(),
            &signer,
            campaign.id(),
            None,
            epoch,
            BetaAccountId::new([6; 32]).unwrap(),
            GrantClass::Participation,
            10 * MICRO_BETA_MINI_PER_BETA_MINI,
            "must fail without contribution",
            2,
            2,
        );
        assert!(matches!(result, Err(BetaError::InvalidObject)));
    }

    #[test]
    fn cross_epoch_grants_cannot_be_applied_after_reset() {
        let record_signer = signer(70);
        let mut store = Store::new(MemoryBackend::new());
        let target = target(&mut store, &record_signer);
        let epoch_a = BetaEpochId::new([11; 32]).unwrap();
        let epoch_b = BetaEpochId::new([12; 32]).unwrap();
        let campaign = create_campaign(
            &mut store,
            &record_signer.did(),
            &record_signer,
            &target,
            epoch_a,
            "epoch A",
            "old beta epoch",
            &strings(&["wallet"]),
            1,
            10,
            100 * MICRO_BETA_MINI_PER_BETA_MINI,
            1,
            1,
        )
        .unwrap();
        let grant = create_grant_authorization(
            &mut store,
            &record_signer.did(),
            &record_signer,
            campaign.id(),
            None,
            epoch_a,
            BetaAccountId::new([13; 32]).unwrap(),
            GrantClass::Testing,
            MICRO_BETA_MINI_PER_BETA_MINI,
            "old epoch grant",
            2,
            2,
        )
        .unwrap();
        let mut ledger = BetaMiniLedger::new(epoch_b, BetaMiniPolicy::open_beta_default()).unwrap();
        assert!(matches!(
            ledger.apply_grant(&store, grant.id()),
            Err(BetaError::WrongEpoch)
        ));
    }

    #[test]
    fn zero_handles_are_rejected_so_missing_values_are_not_ambiguous() {
        assert!(matches!(
            BetaEpochId::new([0; 32]),
            Err(BetaError::InvalidObject)
        ));
        assert!(matches!(
            SubmissionTag::new([0; 32]),
            Err(BetaError::InvalidObject)
        ));
        assert!(matches!(
            ClaimTag::new([0; 32]),
            Err(BetaError::InvalidObject)
        ));
        assert!(matches!(
            BetaAccountId::new([0; 32]),
            Err(BetaError::InvalidObject)
        ));
    }
}
