//! Pipeline manifests: an ordered set of steps, each either a sandboxed
//! `wasm-component` (capability-declared, trusted-provenance eligible) or
//! an unsandboxed `native-tool` (never trusted-provenance eligible until
//! its own separate OS-isolated mechanism exists and is decided the same
//! explicit way D-0069 decided this one -- see this crate's own docs and
//! `docs/design/self-hosted-forge-spine.md`'s Batch 2b section).

use mini_objects::ObjectId;

use crate::capability::Capability;
use crate::error::{PipelineError, Result};
use crate::limits::ResourceLimits;

/// Maximum steps in one manifest (hostile-input bound).
pub const MAX_STEPS: usize = 256;
/// Maximum bytes for a step or manifest name.
pub const MAX_NAME_BYTES: usize = 128;
/// Maximum capabilities a single `wasm-component` step may declare.
pub const MAX_CAPABILITIES_PER_STEP: usize = 32;
/// Maximum `native-tool` arguments.
pub const MAX_ARGUMENTS: usize = 128;
/// Maximum bytes for a single argument.
pub const MAX_ARGUMENT_BYTES: usize = 4096;

/// What a step actually runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StepKind {
    /// A signed WebAssembly component, executed by
    /// `mini-build-runner-wasmtime` under exactly the declared
    /// capabilities and nothing else.
    WasmComponent {
        /// Content id of the component's bytes.
        component: ObjectId,
        /// The capability list the runner's linker is built from.
        capabilities: Vec<Capability>,
    },
    /// A native host toolchain invocation (`cargo build`, `npm install`,
    /// ...). **Never trusted-provenance eligible** -- see this module's
    /// own docs and D-0069's scope limitation.
    NativeTool {
        /// Content id pinning the exact toolchain image/version.
        toolchain: ObjectId,
        /// Structured arguments -- never a shell string, so there is no
        /// shell-interpretation surface even though this step class is
        /// still unsandboxed at the process level.
        arguments: Vec<String>,
    },
}

/// One step in a pipeline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PipelineStep {
    pub name: String,
    /// Names of steps in the same manifest that must complete first.
    /// Every name here must refer to a step defined *earlier* in the
    /// manifest -- no forward references, so lineage is always
    /// resolvable by construction (`mini-forge`'s PR/chain-entry lineage
    /// checks use the same discipline).
    pub depends_on: Vec<String>,
    pub kind: StepKind,
    pub limits: ResourceLimits,
}

impl PipelineStep {
    /// Whether a *successful, isolated-runner-executed* run of this step
    /// may earn a trusted `mini-provenance` record. Structural, not a
    /// runtime flag: only `WasmComponent` steps ever return `true`.
    pub fn trusted_provenance_eligible(&self) -> bool {
        matches!(self.kind, StepKind::WasmComponent { .. })
    }

    fn validate(&self) -> Result<()> {
        if self.name.is_empty() || self.name.len() > MAX_NAME_BYTES {
            return Err(PipelineError::BadStepName(self.name.clone()));
        }
        self.limits.validate()?;
        match &self.kind {
            StepKind::WasmComponent { capabilities, .. } => {
                if capabilities.len() > MAX_CAPABILITIES_PER_STEP {
                    return Err(PipelineError::FieldTooLarge);
                }
            }
            StepKind::NativeTool { arguments, .. } => {
                if arguments.len() > MAX_ARGUMENTS {
                    return Err(PipelineError::FieldTooLarge);
                }
                if arguments.iter().any(|a| a.len() > MAX_ARGUMENT_BYTES) {
                    return Err(PipelineError::FieldTooLarge);
                }
            }
        }
        Ok(())
    }
}

/// An ordered set of pipeline steps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PipelineManifest {
    pub name: String,
    pub steps: Vec<PipelineStep>,
}

impl PipelineManifest {
    /// Validate structure: name/step bounds, unique step names, and every
    /// `depends_on` resolving to a strictly earlier step in the manifest
    /// (which also rules out cycles -- a dependency can never point
    /// forward or at itself).
    pub fn validate(&self) -> Result<()> {
        if self.name.is_empty() || self.name.len() > MAX_NAME_BYTES {
            return Err(PipelineError::BadStepName(self.name.clone()));
        }
        if self.steps.is_empty() {
            return Err(PipelineError::EmptyManifest);
        }
        if self.steps.len() > MAX_STEPS {
            return Err(PipelineError::FieldTooLarge);
        }

        let mut seen: Vec<&str> = Vec::with_capacity(self.steps.len());
        for step in &self.steps {
            step.validate()?;
            if seen.contains(&step.name.as_str()) {
                return Err(PipelineError::BadStepName(step.name.clone()));
            }
            for dep in &step.depends_on {
                if !seen.contains(&dep.as_str()) {
                    return Err(PipelineError::UnknownDependency {
                        step: step.name.clone(),
                        dependency: dep.clone(),
                    });
                }
            }
            seen.push(step.name.as_str());
        }
        Ok(())
    }

    /// The steps in an order that respects every `depends_on` edge.
    /// Because `validate()` already guarantees every dependency points to
    /// a strictly earlier step, a manifest's own declaration order is
    /// already a valid topological order -- this is a cheap confirming
    /// pass, not a real topological sort, and returns
    /// [`PipelineError::DependencyCycle`] only if that invariant was
    /// somehow violated (defense in depth, not the primary check).
    pub fn execution_order(&self) -> Result<Vec<&PipelineStep>> {
        self.validate()?;
        let mut completed: Vec<&str> = Vec::with_capacity(self.steps.len());
        for step in &self.steps {
            if step
                .depends_on
                .iter()
                .any(|d| !completed.contains(&d.as_str()))
            {
                return Err(PipelineError::DependencyCycle);
            }
            completed.push(step.name.as_str());
        }
        Ok(self.steps.iter().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn any_id(byte: u8) -> ObjectId {
        let root = did_mini::Controller::incept_single_from_seeds(
            &[byte; 32],
            &[byte.wrapping_add(1); 32],
        )
        .unwrap();
        let device = did_mini::Controller::incept_device_single_from_seeds(
            &root.did(),
            &[byte.wrapping_add(2); 32],
            &[byte.wrapping_add(3); 32],
        )
        .unwrap();
        mini_objects::ObjectBuilder::new(mini_objects::ObjectType::Custom("test".to_string()))
            .payload(mini_objects::Payload::Public(vec![byte]))
            .sign(&root.did(), &device)
            .unwrap()
            .id()
            .clone()
    }

    fn wasm_step(name: &str, depends_on: &[&str], caps: Vec<Capability>) -> PipelineStep {
        PipelineStep {
            name: name.to_string(),
            depends_on: depends_on.iter().map(|s| s.to_string()).collect(),
            kind: StepKind::WasmComponent {
                component: any_id(1),
                capabilities: caps,
            },
            limits: ResourceLimits::conservative_default(),
        }
    }

    #[test]
    fn a_wasm_component_step_is_trusted_provenance_eligible() {
        let step = wasm_step("check", &[], vec![Capability::WorkspaceRead]);
        assert!(step.trusted_provenance_eligible());
    }

    #[test]
    fn a_native_tool_step_is_never_trusted_provenance_eligible() {
        let step = PipelineStep {
            name: "build".to_string(),
            depends_on: vec![],
            kind: StepKind::NativeTool {
                toolchain: any_id(2),
                arguments: vec!["build".to_string(), "--locked".to_string()],
            },
            limits: ResourceLimits::conservative_default(),
        };
        assert!(!step.trusted_provenance_eligible());
    }

    #[test]
    fn empty_manifest_is_rejected() {
        let m = PipelineManifest {
            name: "empty".to_string(),
            steps: vec![],
        };
        assert_eq!(m.validate(), Err(PipelineError::EmptyManifest));
    }

    #[test]
    fn duplicate_step_names_are_rejected() {
        let m = PipelineManifest {
            name: "dup".to_string(),
            steps: vec![
                wasm_step("check", &[], vec![]),
                wasm_step("check", &[], vec![]),
            ],
        };
        assert!(matches!(m.validate(), Err(PipelineError::BadStepName(_))));
    }

    #[test]
    fn a_dependency_on_an_undefined_step_is_rejected() {
        let m = PipelineManifest {
            name: "bad-dep".to_string(),
            steps: vec![wasm_step("test", &["check"], vec![])],
        };
        assert!(matches!(
            m.validate(),
            Err(PipelineError::UnknownDependency { .. })
        ));
    }

    #[test]
    fn a_forward_reference_is_rejected() {
        // "check" depends on "test", but "test" is defined AFTER "check" --
        // not resolvable at the point "check" is declared.
        let m = PipelineManifest {
            name: "forward-ref".to_string(),
            steps: vec![
                wasm_step("check", &["test"], vec![]),
                wasm_step("test", &[], vec![]),
            ],
        };
        assert!(matches!(
            m.validate(),
            Err(PipelineError::UnknownDependency { .. })
        ));
    }

    #[test]
    fn a_valid_chain_produces_a_respecting_execution_order() {
        let m = PipelineManifest {
            name: "chain".to_string(),
            steps: vec![
                wasm_step("check", &[], vec![Capability::WorkspaceRead]),
                wasm_step("test", &["check"], vec![Capability::WorkspaceRead]),
                wasm_step("release", &["test"], vec![Capability::ArtifactsWrite]),
            ],
        };
        let order = m.execution_order().unwrap();
        let names: Vec<&str> = order.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["check", "test", "release"]);
    }

    #[test]
    fn too_many_steps_is_rejected() {
        let steps: Vec<PipelineStep> = (0..MAX_STEPS + 1)
            .map(|i| wasm_step(&format!("s{i}"), &[], vec![]))
            .collect();
        let m = PipelineManifest {
            name: "huge".to_string(),
            steps,
        };
        assert_eq!(m.validate(), Err(PipelineError::FieldTooLarge));
    }

    #[test]
    fn a_step_with_a_zero_resource_limit_is_rejected() {
        let mut step = wasm_step("check", &[], vec![]);
        step.limits.max_fuel = 0;
        let m = PipelineManifest {
            name: "bad-limits".to_string(),
            steps: vec![step],
        };
        assert_eq!(m.validate(), Err(PipelineError::BadResourceLimit));
    }
}
