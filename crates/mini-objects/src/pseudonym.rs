//! Scoped pseudonym derivation (`MN-104`): the same root produces a stable,
//! independent pseudonym per `(purpose, scope)` pair, with no public
//! relationship between pseudonyms in different scopes or for different
//! purposes in the same scope.
//!
//! This reuses `did-mini`'s existing SPEC-01 §10 pairwise-pseudonym
//! mechanism (`Controller::incept_pairwise_pseudonym`, HKDF-SHA256 over the
//! root's own current-key seed) rather than deriving a second, competing
//! HKDF call site — the only new thing here is the domain-separated
//! `context` bytes this module builds before calling it.

use did_mini::Controller;

use crate::error::Result;

const DOMAIN: &[u8] = b"mininet/mini-objects/scoped-pseudonym/v1";

/// What a scoped pseudonym is being derived for. Distinct purposes in the
/// same scope must produce unrelated pseudonyms — otherwise a capability
/// holder pseudonym would double as a public object-authorship handle,
/// reintroducing exactly the cross-context correlation this module exists
/// to prevent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum PseudonymPurpose {
    /// Authoring a [`crate::private_object::PrivateObject`] under this scope.
    ObjectAuthor,
    /// Holding a [`crate::capability::CapabilityGrant`] issued for this scope.
    CapabilityHolder,
}

impl PseudonymPurpose {
    fn tag(self) -> u8 {
        match self {
            PseudonymPurpose::ObjectAuthor => 1,
            PseudonymPurpose::CapabilityHolder => 2,
        }
    }
}

/// Derive a scoped pseudonym `Controller` from `root` for `purpose` within
/// `scope_id` (a canonical, stable, opaque byte identifier — never a
/// display name or anything else that can change without the pseudonym
/// silently changing underneath a caller). Deterministic: the same
/// `(root, purpose, scope_id)` always derives the same pseudonym.
///
/// `root` must be a single-key (non-multisig) controller — the same
/// restriction `Controller::incept_pairwise_pseudonym` already enforces.
pub fn derive_scoped_pseudonym(
    root: &Controller,
    purpose: PseudonymPurpose,
    scope_id: &[u8],
) -> Result<Controller> {
    let mut context = Vec::with_capacity(DOMAIN.len() + 1 + scope_id.len());
    context.extend_from_slice(DOMAIN);
    context.push(purpose.tag());
    context.extend_from_slice(scope_id);
    Ok(root.incept_pairwise_pseudonym(&context)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> Controller {
        Controller::incept_single().unwrap()
    }

    #[test]
    fn the_same_root_purpose_and_scope_derive_the_same_pseudonym() {
        let r = root();
        let a = derive_scoped_pseudonym(&r, PseudonymPurpose::ObjectAuthor, b"scope-a").unwrap();
        let b = derive_scoped_pseudonym(&r, PseudonymPurpose::ObjectAuthor, b"scope-a").unwrap();
        assert_eq!(a.did(), b.did());
    }

    #[test]
    fn different_scopes_derive_different_pseudonyms() {
        let r = root();
        let a = derive_scoped_pseudonym(&r, PseudonymPurpose::ObjectAuthor, b"scope-a").unwrap();
        let b = derive_scoped_pseudonym(&r, PseudonymPurpose::ObjectAuthor, b"scope-b").unwrap();
        assert_ne!(a.did(), b.did());
    }

    #[test]
    fn different_purposes_in_the_same_scope_derive_different_pseudonyms() {
        let r = root();
        let a = derive_scoped_pseudonym(&r, PseudonymPurpose::ObjectAuthor, b"scope-a").unwrap();
        let b =
            derive_scoped_pseudonym(&r, PseudonymPurpose::CapabilityHolder, b"scope-a").unwrap();
        assert_ne!(a.did(), b.did());
    }

    #[test]
    fn different_roots_derive_different_pseudonyms_even_for_the_same_scope() {
        let a =
            derive_scoped_pseudonym(&root(), PseudonymPurpose::ObjectAuthor, b"scope-a").unwrap();
        let b =
            derive_scoped_pseudonym(&root(), PseudonymPurpose::ObjectAuthor, b"scope-a").unwrap();
        assert_ne!(a.did(), b.did());
    }

    #[test]
    fn a_scoped_pseudonym_can_sign_and_be_independently_verified() {
        let r = root();
        let pseudonym =
            derive_scoped_pseudonym(&r, PseudonymPurpose::ObjectAuthor, b"scope-a").unwrap();
        let sigs = pseudonym.sign_message(b"hello");
        pseudonym.kel().verify_message(b"hello", &sigs).unwrap();
    }
}
