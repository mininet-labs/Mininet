//! Resident AI is owner-controlled advisory assistance, never a human signer,
//! governance approval, shell executor, or a currency-spending capability.
#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

mod config;
mod workspace;
pub use config::*;
pub mod provider;
pub use provider::*;
pub use workspace::*;

use serde::{Deserialize, Serialize};
use std::fmt;

pub const MAX_QUESTION_BYTES: usize = 16 * 1024;
pub const MAX_CONTEXT_BYTES: usize = 256 * 1024;
pub const MAX_CONTEXT_FILES: usize = 32;
pub const MAX_REPLY_BYTES: usize = 128 * 1024;

#[derive(Debug)]
pub struct ResidentError(pub String);
impl fmt::Display for ResidentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
impl std::error::Error for ResidentError {}
impl From<std::io::Error> for ResidentError {
    fn from(e: std::io::Error) -> Self {
        Self(e.to_string())
    }
}
impl From<serde_json::Error> for ResidentError {
    fn from(e: serde_json::Error) -> Self {
        Self(e.to_string())
    }
}
pub type Result<T> = std::result::Result<T, ResidentError>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderConfig {
    pub endpoint: String,
    pub model: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextItem {
    pub path: String,
    pub content: String,
    pub hash: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResidentRequest {
    pub question: String,
    pub context: Vec<ContextItem>,
}

/// Local UI provenance. This is deliberately not mini_objects::AiObject or
/// Object: publishing signed AI evidence requires the existing AI envelope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AiProvenance {
    pub label: String,
    pub advisory: bool,
    pub human_approval: bool,
}
impl Default for AiProvenance {
    fn default() -> Self {
        Self {
            label: "Resident AI — AI-generated advisory evidence".into(),
            advisory: true,
            human_approval: false,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResidentReply {
    pub text: String,
    pub model: String,
    pub provenance: AiProvenance,
}
impl ResidentReply {
    pub fn new(text: String, model: String) -> Result<Self> {
        if text.len() > MAX_REPLY_BYTES || model.is_empty() || model.len() > 256 {
            return Err(ResidentError(
                "model reply exceeds Resident AI bounds".into(),
            ));
        }
        Ok(Self {
            text,
            model,
            provenance: AiProvenance::default(),
        })
    }
}
pub fn content_hash(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}
pub fn validate_request(request: &ResidentRequest) -> Result<()> {
    if request.question.trim().is_empty()
        || request.question.len() > MAX_QUESTION_BYTES
        || request.context.len() > MAX_CONTEXT_FILES
    {
        return Err(ResidentError(
            "question/context exceeds Resident AI bounds".into(),
        ));
    }
    let mut bytes = 0usize;
    let mut seen = std::collections::BTreeSet::new();
    for item in &request.context {
        validate_context_path(&item.path)?;
        bytes = bytes
            .checked_add(item.content.len())
            .ok_or_else(|| ResidentError("context overflow".into()))?;
        if bytes > MAX_CONTEXT_BYTES
            || !seen.insert(&item.path)
            || item.hash != content_hash(item.content.as_bytes())
        {
            return Err(ResidentError(
                "context is oversized, duplicated, or has a mismatched hash".into(),
            ));
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Domain {
    Coding,
    Checks,
    Search,
    Governance,
    Hosting,
    Fees,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DomainPolicy {
    pub domain: Domain,
    pub advisory_only: bool,
    pub may_execute_commands: bool,
    pub may_approve_governance: bool,
    pub may_spend_currency: bool,
    pub owner_controls_context: bool,
}
pub fn domain_policy(domain: Domain) -> DomainPolicy {
    DomainPolicy {
        domain,
        advisory_only: true,
        may_execute_commands: false,
        may_approve_governance: false,
        may_spend_currency: false,
        owner_controls_context: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_domains_are_advisory_and_context_is_integrity_checked() {
        for d in [
            Domain::Coding,
            Domain::Checks,
            Domain::Search,
            Domain::Governance,
            Domain::Hosting,
            Domain::Fees,
        ] {
            let p = domain_policy(d);
            assert!(
                p.advisory_only
                    && !p.may_spend_currency
                    && !p.may_approve_governance
                    && !p.may_execute_commands
            );
        }
        let reply = ResidentReply::new("help".into(), "local".into()).unwrap();
        assert!(!reply.provenance.human_approval);
        let mut req = ResidentRequest {
            question: "explain".into(),
            context: vec![ContextItem {
                path: "src/lib.rs".into(),
                content: "code".into(),
                hash: content_hash(b"code"),
            }],
        };
        validate_request(&req).unwrap();
        req.context[0].content.push('!');
        assert!(validate_request(&req).is_err());
    }
}
