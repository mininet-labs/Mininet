//! The off switch -- device-local, and only device-local.

use std::collections::HashSet;

use did_mini::Did;

use crate::declaration::{ProviderDeclaration, ServiceClassTag};

/// FD-18 / T5. Lives in the user's client, under the user's key.
///
/// There is deliberately NO network-wide equivalent of this type and there
/// must never be one (INV-18-05): a network-level disable switch for a
/// service is indistinguishable in code from a network-level disable
/// switch for a person. Every mutation here is local, permanent-until-
/// reversed, penalty-free, and never published anywhere -- this type has
/// no encode/decode, no wire format, and no network dependency, on
/// purpose. The network never learns that a given human disabled a given
/// provider (T4).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LocalProviderPolicy {
    disabled: HashSet<Did>,
    disabled_classes: HashSet<ServiceClassTag>,
}

impl LocalProviderPolicy {
    pub fn new() -> Self {
        Self::default()
    }

    /// Permanent until [`Self::enable`] is called, and leaves no residue
    /// beyond this in-memory/on-device set.
    pub fn disable(&mut self, provider: Did) {
        self.disabled.insert(provider);
    }

    pub fn enable(&mut self, provider: &Did) {
        self.disabled.remove(provider);
    }

    pub fn disable_class(&mut self, class: ServiceClassTag) {
        self.disabled_classes.insert(class);
    }

    pub fn enable_class(&mut self, class: ServiceClassTag) {
        self.disabled_classes.remove(&class);
    }

    pub fn is_provider_disabled(&self, provider: &Did) -> bool {
        self.disabled.contains(provider)
    }

    pub fn is_class_disabled(&self, class: ServiceClassTag) -> bool {
        self.disabled_classes.contains(&class)
    }

    /// Whether this policy allows rendering/using `d`. `false` if either
    /// the exact declarant or the declared service class has been
    /// disabled.
    pub fn allows(&self, d: &ProviderDeclaration) -> bool {
        !self.disabled.contains(&d.declarant) && !self.disabled_classes.contains(&d.service.tag())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::declaration::{
        CustodyPosture, DeathDisposition, ExitTerms, FreezePowers, ServiceClass,
    };
    use did_mini::Controller;

    fn declaration_for(declarant: Did, service: ServiceClass) -> ProviderDeclaration {
        ProviderDeclaration {
            declarant,
            service,
            description: String::new(),
            jurisdictions: vec![],
            data_required: vec![],
            custody: CustodyPosture::NoneHeld,
            freeze_powers: FreezePowers {
                can_freeze_user: false,
                grounds: vec![],
                notifies_user: true,
            },
            death_disposition: DeathDisposition::NothingHeld,
            exit: ExitTerms {
                notice_required_ms: None,
                exit_fee_micromini: 0,
                retained_data: vec![],
            },
            expires_at_ms: u64::MAX,
        }
    }

    #[test]
    fn a_fresh_policy_allows_everything() {
        let policy = LocalProviderPolicy::new();
        let d = declaration_for(
            Controller::incept_single().unwrap().did(),
            ServiceClass::Conversion,
        );
        assert!(policy.allows(&d));
    }

    #[test]
    fn disabling_a_provider_blocks_only_that_provider() {
        let mut policy = LocalProviderPolicy::new();
        let blocked = Controller::incept_single().unwrap().did();
        let other = Controller::incept_single().unwrap().did();
        policy.disable(blocked.clone());

        assert!(!policy.allows(&declaration_for(blocked.clone(), ServiceClass::Conversion)));
        assert!(policy.allows(&declaration_for(other, ServiceClass::Conversion)));

        policy.enable(&blocked);
        assert!(policy.allows(&declaration_for(blocked, ServiceClass::Conversion)));
    }

    #[test]
    fn disabling_a_class_blocks_every_provider_in_it_including_other_variants() {
        let mut policy = LocalProviderPolicy::new();
        policy.disable_class(ServiceClassTag::Other);

        let a = declaration_for(
            Controller::incept_single().unwrap().did(),
            ServiceClass::Other("bank A".to_string()),
        );
        let b = declaration_for(
            Controller::incept_single().unwrap().did(),
            ServiceClass::Other("bank B".to_string()),
        );
        let c = declaration_for(
            Controller::incept_single().unwrap().did(),
            ServiceClass::Conversion,
        );

        assert!(!policy.allows(&a));
        assert!(!policy.allows(&b));
        assert!(policy.allows(&c));

        policy.enable_class(ServiceClassTag::Other);
        assert!(policy.allows(&a));
    }

    #[test]
    fn is_provider_disabled_and_is_class_disabled_reflect_direct_queries() {
        let mut policy = LocalProviderPolicy::new();
        let provider = Controller::incept_single().unwrap().did();
        assert!(!policy.is_provider_disabled(&provider));
        assert!(!policy.is_class_disabled(ServiceClassTag::Custody));

        policy.disable(provider.clone());
        policy.disable_class(ServiceClassTag::Custody);

        assert!(policy.is_provider_disabled(&provider));
        assert!(policy.is_class_disabled(ServiceClassTag::Custody));
    }
}
