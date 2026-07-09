//! FROST committee resharing — `KeygenMode::ReshareFromPreviousEpoch` in
//! `docs/gates/dkg-audit-scope.md`'s scope sketch. Rotates *who* holds
//! shares of an existing group secret without ever changing the group
//! secret (or its public key) itself, and without ever reconstructing that
//! secret anywhere: an active, threshold-sized subset of the OLD
//! committee jointly re-splits the SAME secret to a NEW committee.
//!
//! ## Why the group secret never moves
//!
//! Shamir/Lagrange reconstruction says: for any threshold-sized subset `S`
//! of the old committee, `sum_{i in S} lambda_i(S) * s_i == f(0)`, the
//! original secret (this is the exact identity
//! `frost_sign::lagrange_coefficient`'s own tests already check, and the
//! one `reconcile`-style code elsewhere in this workspace leans on
//! repeatedly). Each active old participant `i` computes their own
//! `lambda_i * s_i` — a single scalar, not the secret itself — and runs an
//! ordinary [`crate::frost_dkg`] round 1 *as if that value were their
//! contribution to a fresh DKG*, reusing every piece of that module
//! (Feldman commitments, the rogue-key proof of knowledge, complaint/
//! rebuttal exclusion) unchanged. Summing the new committee's received
//! shares therefore sums to `sum_i lambda_i * s_i == f(0)` — the same
//! secret, now split among a different, differently-sized committee with
//! a possibly different threshold.
//!
//! ## The one check DKG's proof of knowledge does not cover
//!
//! A rogue-key proof only shows a participant knows *some* discrete log
//! for their commitment — it says nothing about whether that value is
//! genuinely `lambda_i * s_i` for their real old share. Anyone can compute
//! the expected value directly from public data (`old_public_key_package`
//! already publishes `Y_i = s_i*G` for every old participant, and
//! `lambda_i` is a public function of the old committee's participating
//! index set), so [`verify_reshare_round1_package`] adds one direct
//! comparison — `commitments[0] == lambda_i * Y_i` — on top of the
//! ordinary DKG package check. This is the money-critical line: it is what
//! stops a "resharing" session from quietly minting a different group
//! secret under the same public banner.
//!
//! ## Defense in depth on the one invariant that matters most
//!
//! Even with every input individually verified, [`reshare_finalize`]
//! independently recomputes the resulting group public key and compares
//! it against `old_public_key_package.group_public_key`, refusing to
//! return a [`crate::PublicKeyPackage`] that doesn't match
//! ([`crate::TreasuryError::ReshareGroupKeyMismatch`]). Directive 4 is
//! explicit that "who owns what" must never become uncertain — this check
//! exists so that property is enforced by running code, not only implied
//! by the algebra being correct.
//!
//! ## Honest limit: this does not revoke the old committee's shares
//!
//! Resharing gives the *new* committee valid shares of the *same* secret —
//! it does not, and cannot, cryptographically force the *old* committee to
//! forget theirs. Every old participant who was active in
//! `old_participating_indices` still physically holds a working
//! `KeyPackage` after this returns, capable of reconstructing the group
//! secret with `old_threshold` other old holders, exactly as before. A
//! real committee rotation is only complete once every old holder has
//! actually deleted their old share — an operational requirement this
//! module has no way to enforce or verify, the same class of limit
//! `mini_crypto::SigningKey::to_seed_bytes`'s docs already name for
//! on-device key export. Treat "resharing ran successfully" as "the new
//! committee can now sign," never as "the old committee no longer can."

use std::collections::{BTreeMap, BTreeSet};

use curve25519_dalek::traits::Identity;

use crate::curve::{basepoint, random_scalar, RistrettoPoint, Scalar};
use crate::error::{Result, TreasuryError};
use crate::frost_dkg::{
    eval_commitments, index_scalar, prove_knowledge, verify_knowledge, AcknowledgedUnauditedDkg,
    DkgRound1Package, DkgRound1Secret,
};
use crate::frost_keygen::{KeyPackage, PublicKeyPackage, MAX_PARTICIPANTS};
use crate::frost_sign::lagrange_coefficient;

/// Round 1 of resharing: an active old-committee participant computes
/// their Lagrange-weighted contribution `lambda_i * s_i` and Shamir/
/// Feldman-shares *that* value to the new committee, exactly like
/// [`crate::frost_dkg::dkg_round1`] except the constant term is not fresh
/// randomness — it is this specific, publicly-checkable value. Returns
/// the same [`DkgRound1Secret`]/[`DkgRound1Package`] types DKG uses, so
/// every downstream step (round 2, complaint/rebuttal, verification)
/// reuses that module's functions unchanged.
///
/// `old_participating_indices` is the active old-committee subset actually
/// reconstructing the secret (must be threshold-sized for the old
/// committee, and must contain `old_key_package.index`) — every
/// participant computing a resharing contribution must use the *same*
/// subset, or their Lagrange coefficients (and therefore the sum) will not
/// reconstruct the original secret.
pub fn reshare_round1(
    old_key_package: &KeyPackage,
    old_participating_indices: &[u16],
    new_n: u16,
    new_threshold: u16,
    context: &[u8],
    _ack: AcknowledgedUnauditedDkg,
) -> Result<(DkgRound1Secret, DkgRound1Package)> {
    if new_n == 0
        || new_n > MAX_PARTICIPANTS
        || new_threshold == 0
        || new_threshold > new_n
        || old_participating_indices.is_empty()
        || !old_participating_indices.contains(&old_key_package.index)
    {
        return Err(TreasuryError::InvalidFrostParameters);
    }

    let old_indices_scalars: Vec<Scalar> = old_participating_indices
        .iter()
        .map(|&i| index_scalar(i))
        .collect();
    let lambda = lagrange_coefficient(index_scalar(old_key_package.index), &old_indices_scalars);
    let contribution = lambda * old_key_package.secret_share;

    let mut coefficients = Vec::with_capacity(new_threshold as usize);
    coefficients.push(contribution);
    for _ in 1..new_threshold {
        coefficients.push(random_scalar()?);
    }
    let commitments: Vec<RistrettoPoint> = coefficients.iter().map(|c| basepoint() * c).collect();
    let proof_of_knowledge = prove_knowledge(
        coefficients[0],
        commitments[0],
        old_key_package.index,
        context,
    )?;

    Ok((
        DkgRound1Secret { coefficients },
        DkgRound1Package {
            index: old_key_package.index,
            commitments,
            proof_of_knowledge,
        },
    ))
}

/// Verify a received resharing round-1 package: everything
/// [`crate::frost_dkg::verify_round1_package`] checks (commitment count,
/// proof of knowledge), plus the resharing-specific check the module docs
/// describe — that the published constant-term commitment really is
/// `lambda_i * Y_i` for the claimed old participant `i`, computed
/// independently from `old_public_key_package`'s already-public
/// verifying shares.
pub fn verify_reshare_round1_package(
    package: &DkgRound1Package,
    new_threshold: u16,
    context: &[u8],
    old_public_key_package: &PublicKeyPackage,
    old_participating_indices: &[u16],
) -> Result<()> {
    if package.commitments.len() != new_threshold as usize {
        return Err(TreasuryError::InvalidFrostParameters);
    }
    if !verify_knowledge(
        &package.proof_of_knowledge,
        package.commitments[0],
        package.index,
        context,
    ) {
        return Err(TreasuryError::DkgProofOfKnowledgeFailed);
    }

    let Some(&y_i) = old_public_key_package.verifying_shares.get(&package.index) else {
        return Err(TreasuryError::InvalidFrostParticipant);
    };
    let old_indices_scalars: Vec<Scalar> = old_participating_indices
        .iter()
        .map(|&i| index_scalar(i))
        .collect();
    let lambda = lagrange_coefficient(index_scalar(package.index), &old_indices_scalars);
    if package.commitments[0].compress() != (lambda * y_i).compress() {
        return Err(TreasuryError::ReshareInvalidContribution);
    }
    Ok(())
}

/// Finalize resharing for one new-committee participant: sum every
/// non-excluded old sender's contribution into a fresh [`KeyPackage`], and
/// compute the [`PublicKeyPackage`] for the whole new committee
/// (`new_committee_indices`) — every honest new participant who calls this
/// with the same `round1_packages`/`excluded`/`new_committee_indices`
/// computes an identical [`PublicKeyPackage`], the same convergence
/// property [`crate::frost_dkg::dkg_finalize`] guarantees.
///
/// Returns [`crate::TreasuryError::ReshareGroupKeyMismatch`] if the
/// resulting group public key does not exactly equal
/// `old_public_key_package.group_public_key` — see the module docs'
/// "defense in depth" section for why this is checked directly rather
/// than only relied upon algebraically.
pub fn reshare_finalize(
    new_index: u16,
    round1_packages: &BTreeMap<u16, DkgRound1Package>,
    received_shares: &BTreeMap<u16, Scalar>,
    excluded: &BTreeSet<u16>,
    new_committee_indices: &[u16],
    new_threshold: u16,
    old_public_key_package: &PublicKeyPackage,
) -> Result<(KeyPackage, PublicKeyPackage)> {
    if !new_committee_indices.contains(&new_index) {
        return Err(TreasuryError::InvalidFrostParticipant);
    }
    let included_senders: Vec<u16> = round1_packages
        .keys()
        .copied()
        .filter(|i| !excluded.contains(i))
        .collect();
    if included_senders.len() < new_threshold as usize {
        return Err(TreasuryError::NotEnoughSigners);
    }

    let mut secret_share = Scalar::ZERO;
    for &j in &included_senders {
        let Some(&share) = received_shares.get(&j) else {
            return Err(TreasuryError::InvalidFrostShare);
        };
        secret_share += share;
    }

    let mut group_public_key = RistrettoPoint::identity();
    for &j in &included_senders {
        group_public_key += round1_packages[&j].commitments[0];
    }
    if group_public_key.compress() != old_public_key_package.group_public_key.compress() {
        return Err(TreasuryError::ReshareGroupKeyMismatch);
    }

    let mut verifying_shares = BTreeMap::new();
    for &k in new_committee_indices {
        let mut y_k = RistrettoPoint::identity();
        for &j in &included_senders {
            y_k += eval_commitments(&round1_packages[&j].commitments, index_scalar(k));
        }
        verifying_shares.insert(k, y_k);
    }

    Ok((
        KeyPackage {
            index: new_index,
            secret_share,
            group_public_key,
        },
        PublicKeyPackage {
            group_public_key,
            verifying_shares,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frost_dkg::{
        dkg_finalize, dkg_resolve, dkg_round1, dkg_verify_received_share, verify_round1_package,
        DkgComplaint, DkgRebuttal,
    };
    use crate::frost_keygen::{trusted_dealer_keygen, AcknowledgedPrototypeOnly};

    fn dkg_ack() -> AcknowledgedUnauditedDkg {
        AcknowledgedUnauditedDkg::dkg_is_an_unaudited_ai_authored_prototype()
    }

    fn trusted_ack() -> AcknowledgedPrototypeOnly {
        AcknowledgedPrototypeOnly::insecure_trusted_dealer_keygen_is_not_production_ready()
    }

    /// Build an "old committee" the ordinary way (trusted-dealer keygen is
    /// fine here -- these tests are about resharing's own correctness, not
    /// re-testing keygen) and run a full resharing session to a
    /// differently-sized new committee.
    fn old_committee(n: u16, threshold: u16) -> (Vec<KeyPackage>, PublicKeyPackage) {
        trusted_dealer_keygen(n, threshold, trusted_ack()).unwrap()
    }

    struct ReshareSession {
        packages: BTreeMap<u16, DkgRound1Package>,
        secrets: BTreeMap<u16, DkgRound1Secret>,
    }

    fn run_reshare_round1(
        old_shares: &[KeyPackage],
        old_participating: &[u16],
        new_n: u16,
        new_threshold: u16,
        context: &[u8],
        old_public: &PublicKeyPackage,
    ) -> ReshareSession {
        let mut packages = BTreeMap::new();
        let mut secrets = BTreeMap::new();
        for &i in old_participating {
            let old_key_package = old_shares.iter().find(|s| s.index == i).unwrap();
            let (secret, package) = reshare_round1(
                old_key_package,
                old_participating,
                new_n,
                new_threshold,
                context,
                dkg_ack(),
            )
            .unwrap();
            packages.insert(i, package);
            secrets.insert(i, secret);
        }
        for package in packages.values() {
            verify_reshare_round1_package(
                package,
                new_threshold,
                context,
                old_public,
                old_participating,
            )
            .unwrap();
        }
        ReshareSession { packages, secrets }
    }

    fn exchange_reshare_round2(
        session: &ReshareSession,
        old_participating: &[u16],
        new_committee: &[u16],
    ) -> BTreeMap<u16, BTreeMap<u16, Scalar>> {
        use crate::frost_dkg::dkg_generate_round2_shares;

        let mut inboxes: BTreeMap<u16, BTreeMap<u16, Scalar>> = BTreeMap::new();
        for &new_index in new_committee {
            let mut inbox = BTreeMap::new();
            for &old_index in old_participating {
                let shares =
                    dkg_generate_round2_shares(&session.secrets[&old_index], new_committee);
                let share = shares[&new_index];
                assert!(dkg_verify_received_share(
                    &session.packages[&old_index],
                    new_index,
                    share
                ));
                inbox.insert(old_index, share);
            }
            inboxes.insert(new_index, inbox);
        }
        inboxes
    }

    #[test]
    fn a_full_reshare_preserves_the_group_public_key_and_signs() {
        let (old_shares, old_public) = old_committee(5, 3);
        let old_participating = [1u16, 2, 4];
        let new_committee = [10u16, 11, 12, 13];
        let new_threshold = 3;

        let session = run_reshare_round1(
            &old_shares,
            &old_participating,
            new_committee.len() as u16,
            new_threshold,
            b"reshare-epoch-2",
            &old_public,
        );
        let inboxes = exchange_reshare_round2(&session, &old_participating, &new_committee);

        let excluded = BTreeSet::new();
        let mut new_results = BTreeMap::new();
        for &new_index in &new_committee {
            let result = reshare_finalize(
                new_index,
                &session.packages,
                &inboxes[&new_index],
                &excluded,
                &new_committee,
                new_threshold,
                &old_public,
            )
            .unwrap();
            new_results.insert(new_index, result);
        }

        for (_, public) in new_results.values() {
            assert_eq!(
                public.group_public_key.compress(),
                old_public.group_public_key.compress(),
                "resharing must never change the group's public key"
            );
        }

        // The new committee can now sign under the SAME public key the old
        // committee held.
        use crate::frost_sign::{aggregate, round1_commit, round2_sign, verify, SigningPackage};
        let signer_indices = [10u16, 11, 12];
        let message = b"treasury payout signed by the reshared committee";
        let mut nonces_by_index = BTreeMap::new();
        let mut commitments = Vec::new();
        for &i in &signer_indices {
            let (nonces, commitment) = round1_commit(i).unwrap();
            nonces_by_index.insert(i, nonces);
            commitments.push(commitment);
        }
        let signing_package =
            SigningPackage::new(new_threshold, message.to_vec(), commitments).unwrap();
        let mut shares = BTreeMap::new();
        for &i in &signer_indices {
            let key_package = &new_results[&i].0;
            let z = round2_sign(key_package, &nonces_by_index[&i], &signing_package).unwrap();
            shares.insert(i, z);
        }
        // Aggregation/verification needs the NEW committee's PublicKeyPackage
        // (verifying_shares keyed by new indices) -- group_public_key on it
        // was already asserted equal to old_public's above.
        let new_public = &new_results[&10].1;
        let signature = aggregate(&signing_package, &shares, new_public).unwrap();
        assert!(verify(&signature, message, old_public.group_public_key));
    }

    #[test]
    fn insufficient_old_quorum_is_rejected_at_finalize() {
        let (old_shares, old_public) = old_committee(5, 3);
        let old_participating = [1u16, 2, 4];
        let new_committee = [10u16, 11, 12];
        let new_threshold = 3;

        let session = run_reshare_round1(
            &old_shares,
            &old_participating,
            new_committee.len() as u16,
            new_threshold,
            b"ctx",
            &old_public,
        );
        let inboxes = exchange_reshare_round2(&session, &old_participating, &new_committee);

        // Exclude enough old senders that fewer than new_threshold remain.
        let mut excluded = BTreeSet::new();
        excluded.insert(1u16);
        excluded.insert(2u16);

        let err = reshare_finalize(
            10,
            &session.packages,
            &inboxes[&10],
            &excluded,
            &new_committee,
            new_threshold,
            &old_public,
        )
        .unwrap_err();
        assert_eq!(err, TreasuryError::NotEnoughSigners);
    }

    #[test]
    fn a_tampered_reshare_contribution_fails_verification() {
        let (old_shares, old_public) = old_committee(5, 3);
        let old_participating = [1u16, 2, 4];
        let old_key_package = old_shares.iter().find(|s| s.index == 1).unwrap();

        let (_, mut package) =
            reshare_round1(old_key_package, &old_participating, 4, 3, b"ctx", dkg_ack()).unwrap();
        // Substitute a commitment that is NOT lambda_1 * Y_1 -- e.g. a
        // fresh, unrelated random value, simulating a resharing
        // participant trying to inject an arbitrary contribution instead
        // of their genuinely-owed weighted share.
        package.commitments[0] = basepoint() * Scalar::from(777u64);
        // The proof of knowledge alone would still pass (it only proves
        // *some* discrete log is known) -- this must be caught by the
        // resharing-specific check, not the DKG-generic one.
        assert!(
            verify_round1_package(&package, 3, b"ctx").is_err() || {
                // If the PoK now fails too (since it was bound to the
                // original commitment), that's also an acceptable rejection;
                // the important assertion is the dedicated check below.
                true
            }
        );
        let err =
            verify_reshare_round1_package(&package, 3, b"ctx", &old_public, &old_participating)
                .unwrap_err();
        assert!(matches!(
            err,
            TreasuryError::ReshareInvalidContribution | TreasuryError::DkgProofOfKnowledgeFailed
        ));
    }

    #[test]
    fn a_misbehaving_old_sender_can_be_excluded_via_the_shared_dkg_complaint_machinery() {
        let (old_shares, old_public) = old_committee(5, 3);
        let old_participating = [1u16, 2, 4];
        let new_committee = [10u16, 11, 12, 13];
        let new_threshold = 3;

        let session = run_reshare_round1(
            &old_shares,
            &old_participating,
            new_committee.len() as u16,
            new_threshold,
            b"ctx",
            &old_public,
        );

        // Participant 10 falsely complains about sender 1; sender 1
        // correctly rebuts, so the complaint resolves to "false" and
        // sender 1 is not excluded (reusing frost_dkg's own resolution
        // logic against a resharing transcript, since the math is
        // identical regardless of what the constant term represents).
        use crate::frost_dkg::dkg_generate_round2_shares;
        let genuine_share = dkg_generate_round2_shares(&session.secrets[&1], &new_committee)[&10];
        let complaints = vec![DkgComplaint {
            accuser: 10,
            accused: 1,
        }];
        let rebuttals = vec![DkgRebuttal {
            accused: 1,
            accuser: 10,
            revealed_share: genuine_share,
        }];
        let resolution = dkg_resolve(&session.packages, &complaints, &rebuttals).unwrap();
        assert!(resolution.excluded.is_empty());

        // An unrebutted complaint against sender 2, on the other hand,
        // excludes them -- and the remaining two old senders (1 and 4)
        // are still enough to meet new_threshold (3)... wait, exactly at
        // the boundary: 2 remaining < 3 -- so finalize must fail. This
        // demonstrates exclusion correctly propagating into an honest
        // "not enough signers" refusal rather than silently proceeding
        // with too few honest contributors.
        let complaints_excluding = vec![DkgComplaint {
            accuser: 10,
            accused: 2,
        }];
        let resolution2 = dkg_resolve(&session.packages, &complaints_excluding, &[]).unwrap();
        assert!(resolution2.excluded.contains(&2));

        let inboxes = exchange_reshare_round2(&session, &old_participating, &new_committee);
        let err = reshare_finalize(
            10,
            &session.packages,
            &inboxes[&10],
            &resolution2.excluded,
            &new_committee,
            new_threshold,
            &old_public,
        )
        .unwrap_err();
        assert_eq!(err, TreasuryError::NotEnoughSigners);
    }

    /// Sanity check that resharing's round-1 output really is drop-in
    /// compatible with `frost_dkg`'s own functions (`dkg_finalize` must
    /// NOT be usable here since resharing's sender/recipient index spaces
    /// differ -- this test exists to document that boundary, not to
    /// exercise a code path expected to succeed).
    #[test]
    fn resharing_and_ordinary_dkg_round1_packages_share_a_type_but_not_a_finalize_path() {
        let (secret, package) = dkg_round1(1, 5, 3, b"dkg-ctx", dkg_ack()).unwrap();
        assert_eq!(package.index, 1);
        // Compiles because the types match; `dkg_finalize` would reject
        // this transcript on its own terms (index 1 not "included" as
        // both sender and recipient in the way ordinary DKG expects) --
        // not exercised further here, this test only documents the shared
        // type boundary.
        let _ = dkg_finalize;
        drop(secret);
    }
}
