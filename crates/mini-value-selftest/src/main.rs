//! `mininet-value-selftest` --- diagnostics for the value layer.
//!
//! ## Why this is its own program
//!
//! The canonical invariant is P1: **"No balance maps to governance or
//! validator vote weight."** That is a rule about vote weight. It says
//! nothing about what a person may inspect, and reading it as a reason to
//! withhold the money code from the person running the client would put the
//! wall in the user's way instead of in the code --- exactly backwards.
//!
//! What the wall does require is that no dependency path connect the value
//! crates to the governance crates. So the checks live here, in a binary
//! `mini-selftest` **spawns and reads lines from** rather than links. This
//! crate has edges to `mini-value` and `mini-treasury` and none to
//! `mini-forge` or `mini-chain`; `mini-selftest` has the reverse. Neither
//! dependency graph ever reaches the other, and a user still gets one
//! Diagnostics page covering both.
//!
//! That process boundary is the same one `mini-build-runner-wasmtime` uses
//! to keep Wasmtime out of every other crate's graph (D-0069) --- an
//! established pattern in this tree, not a new mechanism invented here.
//!
//! ## Output
//!
//! One line per check on stdout:
//!
//! ```text
//! MNVALUECHK1
//! <area>\t<name>\t<negative>\t<outcome>\t<detail>
//! ```
//!
//! Tab-separated rather than JSON because the only consumer is a sibling
//! process that also wrote the producer; a hand-rolled parser for a format
//! this simple is smaller than the JSON emitter would be, and `--json`
//! exists for everyone else.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

use mini_value::{
    pedersen_commitment_v2, prove_range_v2, public_amount_commitment_v2, verify_balance_v2,
    verify_range_v2,
};

/// Format tag for the line protocol.
pub const MAGIC: &str = "MNVALUECHK1";

type CheckFn = fn() -> Result<String, String>;

fn checks() -> Vec<(&'static str, &'static str, bool, CheckFn)> {
    vec![
        (
            "value",
            "a hidden amount commits and its range proof verifies",
            false,
            check_range_proof as CheckFn,
        ),
        (
            "value",
            "a tampered range proof does not verify",
            true,
            check_tampered_range_proof,
        ),
        (
            "value",
            "a range proof does not verify against a different commitment",
            true,
            check_range_proof_is_bound_to_its_commitment,
        ),
        (
            "value",
            "outputs that balance their inputs verify",
            false,
            check_balanced_transaction,
        ),
        (
            "value",
            "inflating an output breaks the balance check",
            true,
            check_inflation_is_refused,
        ),
        (
            "treasury",
            "a threshold of distinct custodians authorizes a payout",
            false,
            check_threshold_reached,
        ),
        (
            "treasury",
            "one custodian cannot reach the threshold, even by approving twice",
            true,
            check_below_threshold_is_refused,
        ),
        (
            "treasury",
            "an approval from outside the custody set counts for nothing",
            true,
            check_outsider_approval_does_not_count,
        ),
    ]
}

// --- value -----------------------------------------------------------------

fn check_range_proof() -> Result<String, String> {
    let blinding = random_blinding()?;
    let (commitment, proof) =
        prove_range_v2(42_000, blinding).map_err(|error| format!("proving failed: {error}"))?;
    if !verify_range_v2(commitment, &proof) {
        return Err("a genuine range proof did not verify".to_string());
    }
    Ok("a commitment hiding 42000 proved to be in range without revealing the amount".to_string())
}

fn check_tampered_range_proof() -> Result<String, String> {
    let blinding = random_blinding()?;
    let (commitment, proof) =
        prove_range_v2(7, blinding).map_err(|error| format!("proving failed: {error}"))?;
    let mut bytes = proof.to_bytes();
    let last = bytes.len() - 1;
    bytes[last] ^= 0x01;
    match mini_value::RangeProofV2::from_bytes(&bytes) {
        // Either the proof fails to decode at all, or it decodes and fails to
        // verify. Both are refusals; silently verifying would not be.
        None => Ok("a range proof with one flipped byte did not even decode".to_string()),
        Some(tampered) => {
            if verify_range_v2(commitment, &tampered) {
                Err("a tampered range proof verified".to_string())
            } else {
                Ok("a range proof with one flipped byte decoded but did not verify".to_string())
            }
        }
    }
}

fn check_range_proof_is_bound_to_its_commitment() -> Result<String, String> {
    let first_blinding = random_blinding()?;
    let second_blinding = random_blinding()?;
    let (_, proof) =
        prove_range_v2(100, first_blinding).map_err(|error| format!("proving failed: {error}"))?;
    let (other_commitment, _) =
        prove_range_v2(100, second_blinding).map_err(|error| format!("proving failed: {error}"))?;
    if verify_range_v2(other_commitment, &proof) {
        return Err(
            "a range proof verified against a commitment it was not made for, so proofs \
             could be reused across outputs"
                .to_string(),
        );
    }
    Ok("a range proof did not verify against another output's commitment".to_string())
}

fn check_balanced_transaction() -> Result<String, String> {
    // One public input of 1000 spent to two hidden outputs of 600 and 400.
    let input = public_amount_commitment_v2(1_000).to_vec();
    let first_blinding = fixed_blinding();
    let first = pedersen_commitment_v2(600, &first_blinding)
        .ok_or_else(|| "could not commit to 600".to_string())?;
    // The second output's blinding must cancel the first so the two sides
    // balance. `balancing_blinding` computes exactly that difference, so this
    // uses the crate's own helper rather than re-deriving curve arithmetic.
    let complement = mini_value::balancing_blinding(&[], &[first_blinding]);
    let second = pedersen_commitment_v2(400, &complement)
        .ok_or_else(|| "could not commit to 400".to_string())?;
    if !verify_balance_v2(&[input], &[first.to_vec(), second.to_vec()]) {
        return Err("a transaction whose outputs sum to its input did not balance".to_string());
    }
    Ok("1000 in, 600 + 400 out, amounts hidden, and the balance still checked".to_string())
}

fn check_inflation_is_refused() -> Result<String, String> {
    let input = public_amount_commitment_v2(1_000).to_vec();
    let first_blinding = fixed_blinding();
    let first = pedersen_commitment_v2(600, &first_blinding)
        .ok_or_else(|| "could not commit to 600".to_string())?;
    let complement = mini_value::balancing_blinding(&[], &[first_blinding]);
    // The same blinding that balanced 400, but paying out 401: one extra
    // micro-unit conjured from nothing.
    let inflated = pedersen_commitment_v2(401, &complement)
        .ok_or_else(|| "could not commit to 401".to_string())?;
    if verify_balance_v2(&[input], &[first.to_vec(), inflated.to_vec()]) {
        return Err(
            "outputs worth more than their inputs balanced, which would let anyone mint money"
                .to_string(),
        );
    }
    Ok("paying out one unit more than was put in failed the balance check".to_string())
}

/// A fixed, canonical blinding factor.
///
/// Built from a `Scalar` rather than written as a byte pattern: a literal
/// like `[0x11; 32]` reads as a valid scalar and is not one --- it exceeds the
/// group order, so every commitment using it silently returns `None`. Going
/// through `Scalar` makes that impossible to get wrong.
fn fixed_blinding() -> [u8; 32] {
    curve25519_dalek::scalar::Scalar::from(7_777_u64).to_bytes()
}

/// A fresh blinding factor.
fn random_blinding() -> Result<curve25519_dalek::scalar::Scalar, String> {
    let bytes = mini_value::random_scalar_bytes()
        .map_err(|error| format!("could not draw a blinding factor: {error}"))?;
    Ok(curve25519_dalek::scalar::Scalar::from_bytes_mod_order(
        bytes,
    ))
}

// --- treasury --------------------------------------------------------------

fn custodians(count: usize) -> Result<Vec<did_mini::Did>, String> {
    let mut out = Vec::new();
    for seed in 0..count as u8 {
        let controller = did_mini::Controller::incept_single_from_seeds(
            &[seed.wrapping_mul(9).wrapping_add(1); 32],
            &[seed.wrapping_mul(9).wrapping_add(2); 32],
        )
        .map_err(|error| format!("inception failed: {error}"))?;
        out.push(controller.did());
    }
    Ok(out)
}

fn check_threshold_reached() -> Result<String, String> {
    let signers = custodians(3)?;
    let set = mini_treasury::TreasurySignerSet::new(signers.clone(), 2)
        .map_err(|error| format!("building the signer set failed: {error}"))?;
    if !mini_treasury::meets_threshold(&set, &signers[..2]) {
        return Err("two of three custodians did not meet a threshold of two".to_string());
    }
    Ok(
        "a 2-of-3 custody set authorized a payout once two distinct custodians approved"
            .to_string(),
    )
}

fn check_below_threshold_is_refused() -> Result<String, String> {
    let signers = custodians(3)?;
    let set = mini_treasury::TreasurySignerSet::new(signers.clone(), 2)
        .map_err(|error| format!("building the signer set failed: {error}"))?;
    if mini_treasury::meets_threshold(&set, &signers[..1]) {
        return Err("one custodian met a threshold of two".to_string());
    }
    // The sharper case: the same custodian approving twice must not count
    // twice, or any single holder could reach any threshold alone.
    let doubled = vec![signers[0].clone(), signers[0].clone()];
    if mini_treasury::meets_threshold(&set, &doubled) {
        return Err(
            "one custodian approving twice met a threshold of two, so a single key could \
             move funds alone"
                .to_string(),
        );
    }
    let counted = mini_treasury::count_valid_approvals(&set, &doubled);
    Ok(format!(
        "one custodian could not reach a threshold of two, and approving twice still counted {counted}"
    ))
}

fn check_outsider_approval_does_not_count() -> Result<String, String> {
    let signers = custodians(3)?;
    let outsiders = custodians(9)?;
    let set = mini_treasury::TreasurySignerSet::new(signers.clone(), 2)
        .map_err(|error| format!("building the signer set failed: {error}"))?;
    let mixed = vec![signers[0].clone(), outsiders[7].clone()];
    if mini_treasury::meets_threshold(&set, &mixed) {
        return Err(
            "an approval from someone outside the custody set counted toward the threshold"
                .to_string(),
        );
    }
    Ok("an approval from outside the custody set counted for nothing".to_string())
}

// --- output ----------------------------------------------------------------

fn main() -> std::process::ExitCode {
    let json = std::env::args().any(|argument| argument == "--json");
    let mut failed = 0usize;
    let mut lines = Vec::new();
    for (area, name, negative, function) in checks() {
        let (outcome, detail) = match function() {
            Ok(detail) => ("passed", detail),
            Err(detail) => {
                failed += 1;
                ("failed", detail)
            }
        };
        lines.push((area, name, negative, outcome, detail));
    }

    if json {
        let escape = |text: &str| {
            text.replace('\\', "\\\\")
                .replace('"', "\\\"")
                .replace('\n', "\\n")
        };
        let body: Vec<String> = lines
            .iter()
            .map(|(area, name, negative, outcome, detail)| {
                format!(
                    "{{\"area\":\"{}\",\"name\":\"{}\",\"negative\":{},\"outcome\":\"{}\",\"detail\":\"{}\"}}",
                    escape(area),
                    escape(name),
                    negative,
                    outcome,
                    escape(detail)
                )
            })
            .collect();
        println!(
            "{{\"ok\":{},\"kind\":\"value.selftest\",\"checks\":[{}]}}",
            failed == 0,
            body.join(",")
        );
    } else {
        println!("{MAGIC}");
        for (area, name, negative, outcome, detail) in &lines {
            // Tabs and newlines would break the line protocol, so they are
            // replaced rather than escaped: no check's detail legitimately
            // contains either.
            let detail = detail.replace(['\t', '\n', '\r'], " ");
            println!("{area}\t{name}\t{negative}\t{outcome}\t{detail}");
        }
    }

    if failed == 0 {
        std::process::ExitCode::SUCCESS
    } else {
        std::process::ExitCode::FAILURE
    }
}
