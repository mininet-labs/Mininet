//! FROST Distributed Key Generation — Pedersen DKG with Feldman VSS and a
//! complaint/rebuttal exclusion mechanism, following FROST KeyGen from the
//! original FROST paper (Komlo & Goldberg, <https://eprint.iacr.org/2020/852.pdf>,
//! Figure 1) — **not** RFC 9591, whose scope is threshold *signing*
//! (`frost_sign`'s round 1/2) and which explicitly leaves key generation
//! out of scope; earlier revisions of this doc misattributed the DKG
//! construction to RFC 9591 §4, corrected as part of the Gate #93 audit
//! remediation (D-0506/D-0507). Produces the same
//! [`crate::KeyPackage`]/[`crate::PublicKeyPackage`]
//! [`crate::frost_keygen::trusted_dealer_keygen`] does, so
//! [`crate::frost_sign`] needs no changes at all to sign with a
//! DKG-generated key — this module only changes *how* those types get
//! made, not what they are.
//!
//! **Gate #93 status:** the external audit report that named this
//! module's index-0 vulnerability (fixed below, D-0506) also recommended
//! against maintaining a second, hand-rolled DKG/complaint implementation
//! at all. `mini_custody` (crate) is the production remediation, wrapping
//! `frost_ristretto255::keys::dkg`'s NCC-audited implementation of this
//! same original-FROST-paper construction with a real ceremony state
//! machine (manifest, consistent Round-1 broadcast, encrypted Round-2
//! transport, abort-on-fault, unanimous completion). This module remains
//! for now as `mini_treasury`'s own historical implementation and its
//! existing test/example coverage; production custody DKG ceremonies
//! should use `mini_custody`, not this module directly.
//!
//! ## Why this closes trusted-dealer keygen's P0 gap (D-0048)
//!
//! In [`trusted_dealer_keygen`](crate::frost_keygen::trusted_dealer_keygen),
//! one process holds the whole secret while splitting it. Here, **every**
//! participant runs an independent Feldman VSS of their own freshly-
//! generated random value; the group secret is the *sum* of every
//! (non-excluded) participant's individual secret. No single device — not
//! even briefly, not even the "dealer" — ever holds, computes, or can
//! reconstruct the group secret alone. That additive structure is also
//! what makes exclusion cheap: dropping a misbehaving participant just
//! means leaving their term out of the sum, nothing needs to be "taken
//! back" from anyone else.
//!
//! ## Round shape
//!
//! 1. [`dkg_round1`] — each participant picks a random degree-`(threshold-1)`
//!    polynomial, publishes its Feldman commitments plus a Schnorr proof of
//!    knowledge of the constant term (below).
//! 2. Every participant verifies every other [`DkgRound1Package`]
//!    ([`verify_round1_package`]), then privately sends each other
//!    participant their evaluation of that polynomial at the recipient's
//!    index ([`dkg_generate_round2_shares`]) — **privately**, over
//!    whatever confidential, authenticated channel the caller has (this
//!    crate has "no network, no transport" the same honest limit
//!    `frost_sign`'s round 1/2 already carries; a plaintext `Scalar` here
//!    is as sensitive as an unencrypted secret key and must never cross an
//!    unauthenticated or unencrypted channel).
//! 3. Each recipient Feldman-verifies every share it receives
//!    ([`dkg_verify_received_share`]). A bad share produces a
//!    [`DkgComplaint`]; the accused gets one chance to publicly
//!    [`DkgRebuttal`] by re-disclosing the same value in the clear — see
//!    "Complaints, and why a bare accusation can't exclude anyone" below.
//! 4. [`dkg_resolve`] — a **pure function of the public transcript**
//!    (commitments + complaints + rebuttals) that every honest participant
//!    computes independently and reaches the identical exclusion set from,
//!    with no voting, no consensus round, no coordinator authority.
//! 5. [`dkg_finalize`] — sums the surviving contributions into an ordinary
//!    [`crate::KeyPackage`]/[`crate::PublicKeyPackage`].
//!
//! ## Why the proof of knowledge exists (rogue-key attack)
//!
//! Without it, a malicious participant could publish a commitment chosen
//! *as a function of everyone else's already-published commitments*
//! (e.g. `A_0 = (target_key - sum of honest A_0s)*G`) and bias the group
//! public key to a value they alone control, without ever running a real
//! polynomial at all. Proving knowledge of the discrete log of `A_0`
//! (ordinary Schnorr: `R = k*G`, `c = H(context, index, A_0, R)`,
//! `z = k + c*a_0`, checked via `z*G == R + c*A_0` — the exact identity
//! `frost_sign`'s `Signature`/`verify` already use, restated here with its
//! own domain tag so the two proof types can never be confused) closes
//! that gap: a rogue commitment chosen after seeing others' cannot also
//! come with a valid proof of already knowing its own discrete log.
//!
//! ## Complaints, and why a bare accusation can't exclude anyone
//!
//! Round-2 shares are sent over a private channel, so if a dishonest
//! recipient simply *lied* about what they received, an unrebuttable
//! complaint would let anyone frame anyone. The fix (Pedersen 1991;
//! Gennaro, Jarecki, Krawczyk & Rabin's complaint protocol -- this is a
//! DKG-specific mechanism, not part of RFC 9591, which does not define
//! key generation at all):
//! the accused gets to publicly re-disclose, in the clear, the exact share
//! value they privately sent. Feldman's verification equation has exactly
//! one satisfying value for a fixed public commitment vector, so an
//! accused party cannot fabricate a passing rebuttal without knowing the
//! real evaluation, and a false accuser cannot present a failing rebuttal
//! as if it were the truth without the accused's cooperation. Disclosing
//! one evaluation point of a degree-`(threshold - 1)` polynomial leaks
//! nothing about its constant term (Shamir's own guarantee) as long as
//! fewer than `threshold` points from the *same* polynomial are ever
//! disclosed this way — and if a participant is excluded, their whole
//! polynomial stops mattering to the group secret regardless, so there is
//! no scenario where a legitimate secret is put at risk by a rebuttal.
//!
//! **Honest limit:** this module does not attempt to punish a *false*
//! accuser beyond failing to exclude their target — repeated false
//! complaints are a nuisance (they force the accused to reveal one share
//! each time) but not a way to exclude anyone who successfully rebuts.
//! Whether to treat a pattern of false complaints as its own misbehavior
//! is a policy question for whatever coordinates DKG sessions, not
//! something this protocol layer decides.

use std::collections::{BTreeMap, BTreeSet};

use curve25519_dalek::traits::Identity;
use zeroize::Zeroize;

use crate::curve::{
    basepoint, hash_to_scalar, random_scalar, CompressedRistretto, RistrettoPoint, Scalar,
};
use crate::error::{Result, TreasuryError};
use crate::frost_keygen::{KeyPackage, PublicKeyPackage, MAX_PARTICIPANTS};

/// Explicit, typed acknowledgment that DKG (like every other primitive in
/// this crate) is a founder-reviewed, AI-authored prototype pending
/// external cryptography audit (D-0037, [roadmap #93](../../issues/93)) —
/// architecturally real, but not yet reviewed by anyone outside this
/// project. The same typed-authority discipline `AcknowledgedPrototypeOnly`
/// applies to `trusted_dealer_keygen`; this is a separate type because the
/// honest limit is different (DKG's gap is "unaudited," not "briefly
/// centralized").
#[derive(Debug, Clone, Copy)]
pub struct AcknowledgedUnauditedDkg {
    _private: (),
}

impl AcknowledgedUnauditedDkg {
    /// Name it, don't hide it.
    pub const fn dkg_is_an_unaudited_ai_authored_prototype() -> Self {
        AcknowledgedUnauditedDkg { _private: () }
    }
}

pub(crate) fn index_scalar(index: u16) -> Scalar {
    Scalar::from(index as u64)
}

/// Evaluate `a_0 + a_1*x + ...` via Horner's method (same construction
/// `frost_keygen::eval_polynomial` uses; duplicated rather than shared so
/// this module stays independently reviewable, the same choice
/// `curve.rs`'s own docs explain for `mini_value`/`mini_treasury`).
pub(crate) fn eval_polynomial(coefficients: &[Scalar], x: Scalar) -> Scalar {
    let mut acc = Scalar::ZERO;
    for coeff in coefficients.iter().rev() {
        acc = acc * x + coeff;
    }
    acc
}

/// Evaluate `A_0 + A_1*x + ...` in the exponent via Horner's method — the
/// same evaluation as [`eval_polynomial`], but on the public Feldman
/// commitments instead of the private coefficients. Anyone can compute
/// this; it is how a share (or a participant's final verifying share) gets
/// checked against a public commitment vector without ever seeing the
/// polynomial itself.
pub(crate) fn eval_commitments(commitments: &[RistrettoPoint], x: Scalar) -> RistrettoPoint {
    let mut acc = RistrettoPoint::identity();
    for commitment in commitments.iter().rev() {
        acc = acc * x + commitment;
    }
    acc
}

const POK_DOMAIN: &[u8] = b"mini-treasury/frost/dkg/proof-of-knowledge";

/// A Schnorr proof of knowledge of the discrete log of a published
/// commitment — see the module docs' "rogue-key attack" section. Not the
/// same type as `frost_sign::Signature`: same math, different domain tag,
/// deliberately kept separate so the two can never be mixed up.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ProofOfKnowledge {
    pub(crate) r: CompressedRistretto,
    pub(crate) z: Scalar,
}

pub(crate) fn pok_challenge(
    index: u16,
    context: &[u8],
    public: RistrettoPoint,
    r: RistrettoPoint,
) -> Scalar {
    hash_to_scalar(&[
        POK_DOMAIN,
        &index.to_be_bytes(),
        context,
        public.compress().as_bytes(),
        r.compress().as_bytes(),
    ])
}

pub(crate) fn prove_knowledge(
    secret: Scalar,
    public: RistrettoPoint,
    index: u16,
    context: &[u8],
) -> Result<ProofOfKnowledge> {
    let k = random_scalar()?;
    let r = basepoint() * k;
    let c = pok_challenge(index, context, public, r);
    Ok(ProofOfKnowledge {
        r: r.compress(),
        z: k + c * secret,
    })
}

pub(crate) fn verify_knowledge(
    proof: &ProofOfKnowledge,
    public: RistrettoPoint,
    index: u16,
    context: &[u8],
) -> bool {
    let Some(r) = proof.r.decompress() else {
        return false;
    };
    let c = pok_challenge(index, context, public, r);
    (basepoint() * proof.z).compress() == (r + c * public).compress()
}

/// A participant's private DKG round-1 state: the coefficients of their
/// own random polynomial, constant term first. Same hardening as
/// `frost_sign::SigningNonces` (D-0059, issue #93) — no `Copy`/`Clone`,
/// zeroized on [`Drop`], `Debug` redacted — for the same reason: this is
/// as sensitive as a raw secret key until it's been folded into the final
/// share sum in [`dkg_finalize`].
pub struct DkgRound1Secret {
    pub(crate) coefficients: Vec<Scalar>,
    /// The exact set of recipient identifiers this session's real roster
    /// permits, fixed at generation time. Recorded so
    /// [`dkg_generate_round2_shares`] can bound which indices it will ever
    /// evaluate at to this session's actual participants — see that
    /// function's docs for why an unbounded recipient list is itself a
    /// secret-recovery oracle, not just an index-0 problem. Not a bare
    /// numeric range: [`crate::frost_reshare::reshare_round1`]'s new
    /// committee identifiers are not required to be sequential from `1`,
    /// so the allowed set has to be the roster's actual identifiers, not
    /// `1..=n`.
    pub(crate) allowed_recipients: BTreeSet<u16>,
}

impl core::fmt::Debug for DkgRound1Secret {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("DkgRound1Secret")
            .field("coefficients", &"[redacted]")
            .finish()
    }
}

impl Drop for DkgRound1Secret {
    fn drop(&mut self) {
        self.coefficients.zeroize();
    }
}

/// A participant's public round-1 broadcast: Feldman commitments to their
/// polynomial, plus a proof they know the constant term's discrete log.
/// Safe, and necessary, to publish to every other participant.
#[derive(Debug, Clone)]
pub struct DkgRound1Package {
    /// Which participant published this.
    pub index: u16,
    pub(crate) commitments: Vec<RistrettoPoint>,
    pub(crate) proof_of_knowledge: ProofOfKnowledge,
}

/// Round 1: generate a fresh degree-`(threshold - 1)` polynomial for
/// `index`, and the public package announcing it. Every participant in the
/// session calls this once, with the same `n`/`threshold`/`context`.
/// `context` must be identical across the whole session (a session ID,
/// purpose string, epoch number — whatever the caller uses to keep two
/// different DKG runs from being confused with each other) — packages and
/// proofs from one `context` are cryptographically inert in another, by
/// construction (see [`verify_round1_package`]'s "replayed rounds" note).
pub fn dkg_round1(
    index: u16,
    n: u16,
    threshold: u16,
    context: &[u8],
    _ack: AcknowledgedUnauditedDkg,
) -> Result<(DkgRound1Secret, DkgRound1Package)> {
    if n == 0 || n > MAX_PARTICIPANTS || threshold == 0 || threshold > n || index == 0 || index > n
    {
        return Err(TreasuryError::InvalidFrostParameters);
    }

    let mut coefficients = Vec::with_capacity(threshold as usize);
    for _ in 0..threshold {
        coefficients.push(random_scalar()?);
    }
    let commitments: Vec<RistrettoPoint> = coefficients.iter().map(|c| basepoint() * c).collect();
    let proof_of_knowledge = prove_knowledge(coefficients[0], commitments[0], index, context)?;

    Ok((
        DkgRound1Secret {
            coefficients,
            allowed_recipients: (1..=n).collect(),
        },
        DkgRound1Package {
            index,
            commitments,
            proof_of_knowledge,
        },
    ))
}

/// Verify a received [`DkgRound1Package`]: the right number of
/// commitments for `threshold`, and a valid proof of knowledge of the
/// constant term — rejects a rogue-key attempt before any share is ever
/// exchanged. `context` must match exactly what the sender used in
/// [`dkg_round1`]; a package replayed from a *different* session's
/// `context` fails here even if every other field is byte-identical,
/// because the proof's challenge is bound to `context`.
pub fn verify_round1_package(
    package: &DkgRound1Package,
    threshold: u16,
    context: &[u8],
) -> Result<()> {
    if package.commitments.len() != threshold as usize {
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
    Ok(())
}

/// Round 2: evaluate this participant's own polynomial at every recipient
/// index in `recipient_indices` (ordinarily every other participant in the
/// session, `1..=n` excluding `self`'s own index, though a caller may pass
/// a smaller set if some participants have already dropped out). Each
/// entry must be sent **privately** to that one recipient — see the module
/// docs' transport note.
///
/// Rejects a recipient index of `0`: in Shamir/Feldman sharing the constant
/// term of the polynomial *is* `f(0)` — the participant's own DKG secret
/// contribution, the exact value [`DkgRound1Secret`]'s own docs say must
/// never be exposed outside [`dkg_finalize`]'s sum. `dkg_round1` already
/// refuses index `0` for a polynomial's *owner*; this closes the matching
/// gap on the *recipient* side, where nothing previously stopped a caller
/// (or a malicious coordinator relaying `recipient_indices`) from asking
/// for evaluation at `0` and receiving the raw secret back.
///
/// Also rejects any index outside `secret.allowed_recipients` (fixed at
/// [`dkg_round1`]/[`crate::frost_reshare::reshare_round1`] time) and any
/// index repeated within `recipient_indices`. Index `0` alone is not the
/// whole vulnerability class: a degree-`(threshold - 1)` polynomial is
/// recoverable at *any* point from `threshold` of its evaluations by
/// Lagrange interpolation, including `f(0)`, so a coordinator able to ask
/// this function to evaluate at arbitrarily many *nonzero* indices —
/// "phantom" indices with no real participant behind them — is just as
/// much a secret-recovery oracle as asking for `f(0)` directly, once it
/// has collected `threshold` distinct answers. Bounding every recipient to
/// this session's actual roster caps what any combination of calls can
/// ever expose at the same real participants' shares the protocol already
/// intentionally discloses to them.
pub fn dkg_generate_round2_shares(
    secret: &DkgRound1Secret,
    recipient_indices: &[u16],
) -> Result<BTreeMap<u16, Scalar>> {
    let mut seen = BTreeSet::new();
    for &j in recipient_indices {
        if j == 0 || !secret.allowed_recipients.contains(&j) || !seen.insert(j) {
            return Err(TreasuryError::InvalidFrostParticipant);
        }
    }
    Ok(recipient_indices
        .iter()
        .map(|&j| (j, eval_polynomial(&secret.coefficients, index_scalar(j))))
        .collect())
}

/// Feldman-verify a share this participant received from `from_package`'s
/// author, evaluated at `at_index` (this participant's own index): does
/// `share*G` equal the sender's commitment polynomial evaluated at
/// `at_index`? A `false` here is the trigger for filing a [`DkgComplaint`]
/// — it does not by itself exclude anyone (see the module docs).
pub fn dkg_verify_received_share(
    from_package: &DkgRound1Package,
    at_index: u16,
    share: Scalar,
) -> bool {
    (basepoint() * share).compress()
        == eval_commitments(&from_package.commitments, index_scalar(at_index)).compress()
}

/// Filed by `accuser` against `accused` after
/// [`dkg_verify_received_share`] returns `false` for a share `accuser`
/// claims to have received from `accused`. A complaint alone excludes no
/// one — see the module docs' "Complaints, and why a bare accusation can't
/// exclude anyone."
#[derive(Debug, Clone, Copy)]
pub struct DkgComplaint {
    pub accuser: u16,
    pub accused: u16,
}

/// `accused`'s public response to a [`DkgComplaint`]: the exact share
/// value they privately sent to `accuser`, now disclosed in the clear so
/// every participant can independently check it. Safe to disclose — see
/// the module docs' note on why revealing one evaluation point of an
/// excluded-or-not polynomial never endangers the group secret.
#[derive(Debug, Clone, Copy)]
pub struct DkgRebuttal {
    pub accused: u16,
    pub accuser: u16,
    pub revealed_share: Scalar,
}

/// The outcome of resolving every complaint/rebuttal against the public
/// round-1 transcript: who is excluded from the final key, and which
/// complaints turned out to be false (informational only — see the module
/// docs' honest limit on not punishing false accusers).
#[derive(Debug, Clone, Default)]
pub struct DkgResolution {
    pub excluded: BTreeSet<u16>,
    pub false_complaints: Vec<DkgComplaint>,
}

/// Resolve every complaint deterministically against the public
/// transcript (`round1_packages`, `complaints`, `rebuttals`) — a pure
/// function of public data. Every honest participant who sees the same
/// transcript computes the identical [`DkgResolution`], with no voting, no
/// coordinator, and no consensus round beyond "everyone saw the same
/// broadcast," the same assumption `frost_sign::SigningPackage` already
/// relies on for its own commitment list.
///
/// Rule, per complaint: if `accused` never rebuts, they are excluded
/// (failure to defend is itself disqualifying — silence cannot be
/// distinguished from guilt in a protocol with no further rounds). If they
/// do rebut, [`dkg_verify_received_share`] on the disclosed value decides:
/// a passing rebuttal excludes no one and marks the complaint false; a
/// failing rebuttal excludes `accused`.
///
/// Only `accused` must be a key in `round1_packages` — their package is
/// what a rebuttal gets checked against. `accuser` is used only as a
/// Feldman evaluation index, not looked up as a participant, so this
/// function works unchanged whether accusers are drawn from the same
/// roster as `round1_packages` (ordinary DKG, where every participant is
/// both sender and potential accuser) or from an entirely different one
/// (resharing, where accusers are new-committee recipients and
/// `round1_packages` holds only the old committee's senders).
pub fn dkg_resolve(
    round1_packages: &BTreeMap<u16, DkgRound1Package>,
    complaints: &[DkgComplaint],
    rebuttals: &[DkgRebuttal],
) -> Result<DkgResolution> {
    let mut resolution = DkgResolution::default();
    for complaint in complaints {
        // `accuser` is deliberately not required to be a key in
        // `round1_packages` (see this function's own docs: resharing
        // legitimately draws accusers from a different roster than
        // `round1_packages`), but it is still used directly as a Feldman
        // evaluation index below, and index `0` is never a valid
        // participant identifier in *any* roster this crate constructs
        // (`dkg_round1` already refuses it) -- so a rebuttal answering an
        // `accuser: 0` complaint would disclose `f(0)`, the accused's raw
        // DKG secret contribution, not an ordinary non-revealing
        // evaluation point. Reject it before any rebuttal is even looked
        // up, regardless of which roster this call is resolving for.
        if complaint.accuser == 0 {
            return Err(TreasuryError::InvalidFrostParticipant);
        }
        let Some(accused_package) = round1_packages.get(&complaint.accused) else {
            return Err(TreasuryError::InvalidFrostParticipant);
        };

        let rebuttal = rebuttals
            .iter()
            .find(|r| r.accused == complaint.accused && r.accuser == complaint.accuser);

        match rebuttal {
            Some(rebuttal)
                if dkg_verify_received_share(
                    accused_package,
                    complaint.accuser,
                    rebuttal.revealed_share,
                ) =>
            {
                resolution.false_complaints.push(*complaint);
            }
            _ => {
                resolution.excluded.insert(complaint.accused);
            }
        }
    }
    Ok(resolution)
}

/// Finalize: sum every non-excluded participant's contribution into this
/// participant's [`KeyPackage`] and the shared [`PublicKeyPackage`] —
/// identical output types to [`crate::frost_keygen::trusted_dealer_keygen`],
/// so [`crate::frost_sign`] needs no DKG-specific code path at all.
/// `my_secret` is consumed (and zeroized on drop) here; it must never be
/// used again after this call. `received_shares` must contain a verified
/// entry (see [`dkg_verify_received_share`]) for every participant in
/// `round1_packages` that is *not* `my_index` and *not* in `excluded`.
///
/// Every honest participant who calls this with the same
/// `round1_packages`/`excluded` computes the identical
/// [`PublicKeyPackage`] — a property worth testing directly, not just
/// trusting the algebra (Directive 4: "who owns what" must never become
/// uncertain).
pub fn dkg_finalize(
    my_index: u16,
    my_secret: DkgRound1Secret,
    round1_packages: &BTreeMap<u16, DkgRound1Package>,
    received_shares: &BTreeMap<u16, Scalar>,
    excluded: &BTreeSet<u16>,
    threshold: u16,
) -> Result<(KeyPackage, PublicKeyPackage)> {
    let included: Vec<u16> = round1_packages
        .keys()
        .copied()
        .filter(|i| !excluded.contains(i))
        .collect();
    if included.len() < threshold as usize || !included.contains(&my_index) {
        return Err(TreasuryError::NotEnoughSigners);
    }

    let mut secret_share = Scalar::ZERO;
    for &j in &included {
        if j == my_index {
            secret_share += eval_polynomial(&my_secret.coefficients, index_scalar(my_index));
        } else {
            let Some(&share) = received_shares.get(&j) else {
                return Err(TreasuryError::InvalidFrostShare);
            };
            secret_share += share;
        }
    }

    let mut group_public_key = RistrettoPoint::identity();
    for &j in &included {
        group_public_key += round1_packages[&j].commitments[0];
    }

    let mut verifying_shares = BTreeMap::new();
    for &k in &included {
        let mut y_k = RistrettoPoint::identity();
        for &j in &included {
            y_k += eval_commitments(&round1_packages[&j].commitments, index_scalar(k));
        }
        verifying_shares.insert(k, y_k);
    }

    Ok((
        KeyPackage {
            index: my_index,
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

    fn ack() -> AcknowledgedUnauditedDkg {
        AcknowledgedUnauditedDkg::dkg_is_an_unaudited_ai_authored_prototype()
    }

    struct Session {
        n: u16,
        threshold: u16,
        secrets: BTreeMap<u16, DkgRound1Secret>,
        packages: BTreeMap<u16, DkgRound1Package>,
    }

    fn run_round1(n: u16, threshold: u16, context: &[u8]) -> Session {
        let mut secrets = BTreeMap::new();
        let mut packages = BTreeMap::new();
        for i in 1..=n {
            let (secret, package) = dkg_round1(i, n, threshold, context, ack()).unwrap();
            secrets.insert(i, secret);
            packages.insert(i, package);
        }
        for package in packages.values() {
            verify_round1_package(package, threshold, context).unwrap();
        }
        Session {
            n,
            threshold,
            secrets,
            packages,
        }
    }

    /// Every participant generates round-2 shares for every other
    /// participant and verifies what they receive; returns each
    /// participant's verified inbox.
    fn exchange_round2(session: &Session) -> BTreeMap<u16, BTreeMap<u16, Scalar>> {
        let all_indices: Vec<u16> = (1..=session.n).collect();
        let mut outboxes: BTreeMap<u16, BTreeMap<u16, Scalar>> = BTreeMap::new();
        for &i in &all_indices {
            let recipients: Vec<u16> = all_indices.iter().copied().filter(|&j| j != i).collect();
            outboxes.insert(
                i,
                dkg_generate_round2_shares(&session.secrets[&i], &recipients).unwrap(),
            );
        }

        let mut inboxes: BTreeMap<u16, BTreeMap<u16, Scalar>> = BTreeMap::new();
        for &recipient in &all_indices {
            let mut inbox = BTreeMap::new();
            for &sender in &all_indices {
                if sender == recipient {
                    continue;
                }
                let share = outboxes[&sender][&recipient];
                assert!(dkg_verify_received_share(
                    &session.packages[&sender],
                    recipient,
                    share
                ));
                inbox.insert(sender, share);
            }
            inboxes.insert(recipient, inbox);
        }
        inboxes
    }

    fn finalize_all(
        session: Session,
        inboxes: BTreeMap<u16, BTreeMap<u16, Scalar>>,
    ) -> BTreeMap<u16, (KeyPackage, PublicKeyPackage)> {
        let excluded = BTreeSet::new();
        let mut out = BTreeMap::new();
        let Session {
            secrets,
            packages,
            threshold,
            ..
        } = session;
        for (i, secret) in secrets {
            let result =
                dkg_finalize(i, secret, &packages, &inboxes[&i], &excluded, threshold).unwrap();
            out.insert(i, result);
        }
        out
    }

    #[test]
    fn rejects_invalid_parameters() {
        assert_eq!(
            dkg_round1(1, 0, 1, b"ctx", ack()).unwrap_err(),
            TreasuryError::InvalidFrostParameters
        );
        assert_eq!(
            dkg_round1(0, 5, 3, b"ctx", ack()).unwrap_err(),
            TreasuryError::InvalidFrostParameters
        );
        assert_eq!(
            dkg_round1(6, 5, 3, b"ctx", ack()).unwrap_err(),
            TreasuryError::InvalidFrostParameters
        );
        assert_eq!(
            dkg_round1(1, 5, 6, b"ctx", ack()).unwrap_err(),
            TreasuryError::InvalidFrostParameters
        );
    }

    #[test]
    fn a_full_honest_run_produces_matching_group_keys_and_verifiable_shares() {
        let session = run_round1(5, 3, b"session-1");
        let inboxes = exchange_round2(&session);
        let results = finalize_all(session, inboxes);

        let group_keys: BTreeSet<_> = results
            .values()
            .map(|(_, public)| public.group_public_key.compress().to_bytes())
            .collect();
        assert_eq!(
            group_keys.len(),
            1,
            "every honest participant must converge on the same group key"
        );

        for (index, (key_package, public)) in &results {
            let expected = *public.verifying_shares.get(index).unwrap();
            assert_eq!(
                (basepoint() * key_package.secret_share).compress(),
                expected.compress()
            );
        }
    }

    #[test]
    fn a_forged_round1_proof_of_knowledge_is_rejected() {
        let (_, mut package) = dkg_round1(1, 5, 3, b"ctx", ack()).unwrap();
        // Tamper with the published constant-term commitment after the
        // proof was made for the original one.
        package.commitments[0] = basepoint() * Scalar::from(999u64);
        assert_eq!(
            verify_round1_package(&package, 3, b"ctx").unwrap_err(),
            TreasuryError::DkgProofOfKnowledgeFailed
        );
    }

    #[test]
    fn a_round1_package_from_one_session_does_not_verify_under_a_different_context() {
        let (_, package) = dkg_round1(1, 5, 3, b"session-a", ack()).unwrap();
        assert!(verify_round1_package(&package, 3, b"session-a").is_ok());
        assert_eq!(
            verify_round1_package(&package, 3, b"session-b").unwrap_err(),
            TreasuryError::DkgProofOfKnowledgeFailed
        );
    }

    #[test]
    fn wrong_commitment_count_for_threshold_is_rejected() {
        let (_, package) = dkg_round1(1, 5, 3, b"ctx", ack()).unwrap();
        assert_eq!(
            verify_round1_package(&package, 4, b"ctx").unwrap_err(),
            TreasuryError::InvalidFrostParameters
        );
    }

    #[test]
    fn requesting_a_round2_share_for_recipient_index_0_is_rejected() {
        // Index 0 is the polynomial's constant term -- the participant's
        // own raw DKG secret contribution, not an ordinary share. Nothing
        // may ever be allowed to ask for it back out.
        let (secret, _package) = dkg_round1(1, 5, 3, b"ctx", ack()).unwrap();
        assert_eq!(
            dkg_generate_round2_shares(&secret, &[2, 0, 3]).unwrap_err(),
            TreasuryError::InvalidFrostParticipant
        );
    }

    #[test]
    fn a_recipient_index_beyond_the_session_roster_is_rejected() {
        // n = 5: only indices 1..=5 are real participants. Anything past
        // that is a "phantom" index with no real participant behind it --
        // rejecting index 0 alone does not stop a coordinator from
        // collecting `threshold` evaluations at made-up indices like
        // 6, 7, 8 and interpolating f(0) from those instead.
        let (secret, _package) = dkg_round1(1, 5, 3, b"ctx", ack()).unwrap();
        assert_eq!(
            dkg_generate_round2_shares(&secret, &[2, 6]).unwrap_err(),
            TreasuryError::InvalidFrostParticipant
        );
    }

    #[test]
    fn a_repeated_recipient_index_in_one_call_is_rejected() {
        let (secret, _package) = dkg_round1(1, 5, 3, b"ctx", ack()).unwrap();
        assert_eq!(
            dkg_generate_round2_shares(&secret, &[2, 3, 2]).unwrap_err(),
            TreasuryError::InvalidFrostParticipant
        );
    }

    #[test]
    fn a_coordinator_cannot_collect_enough_phantom_evaluations_to_interpolate_the_secret() {
        // End-to-end demonstration, not just a boundary check: even a
        // coordinator who can call `dkg_generate_round2_shares` directly
        // (bypassing any real per-recipient transport) can never gather
        // `threshold` evaluations at indices this session didn't actually
        // have, because every out-of-range index is refused individually
        // -- there is no way to accumulate enough points to run Lagrange
        // interpolation against f(0).
        let (secret, _package) = dkg_round1(1, 5, 3, b"ctx", ack()).unwrap();
        // The only accessible indices are 1..=5; asking for any index in
        // 6..=1000 always fails, so no amount of calling this function
        // can ever produce `threshold` (3) evaluations outside the real
        // roster.
        for phantom in 6u16..=1000 {
            assert!(dkg_generate_round2_shares(&secret, &[phantom]).is_err());
        }
    }

    #[test]
    fn a_complaint_accusing_index_0_is_rejected_before_any_rebuttal_lookup() {
        // A rebuttal answering an `accuser: 0` complaint would disclose
        // f(0) -- the accused's raw secret -- not an ordinary Feldman
        // evaluation point. This must be refused outright, independent of
        // whether a rebuttal is even supplied.
        let session = run_round1(5, 3, b"ctx");
        let complaints = vec![DkgComplaint {
            accuser: 0,
            accused: 1,
        }];
        assert_eq!(
            dkg_resolve(&session.packages, &complaints, &[]).unwrap_err(),
            TreasuryError::InvalidFrostParticipant
        );
    }

    #[test]
    fn a_tampered_share_fails_feldman_verification() {
        let (secret, package) = dkg_round1(1, 5, 3, b"ctx", ack()).unwrap();
        let mut share = dkg_generate_round2_shares(&secret, &[2]).unwrap()[&2];
        share += Scalar::ONE;
        assert!(!dkg_verify_received_share(&package, 2, share));
    }

    #[test]
    fn an_unrebutted_complaint_excludes_the_accused() {
        let session = run_round1(5, 3, b"ctx");
        let complaints = vec![DkgComplaint {
            accuser: 2,
            accused: 1,
        }];
        let resolution = dkg_resolve(&session.packages, &complaints, &[]).unwrap();
        assert!(resolution.excluded.contains(&1));
        assert!(resolution.false_complaints.is_empty());
    }

    #[test]
    fn a_true_rebuttal_excludes_no_one_and_marks_the_complaint_false() {
        let session = run_round1(5, 3, b"ctx");
        let genuine_share = eval_polynomial(&session.secrets[&1].coefficients, index_scalar(2));
        let complaints = vec![DkgComplaint {
            accuser: 2,
            accused: 1,
        }];
        let rebuttals = vec![DkgRebuttal {
            accused: 1,
            accuser: 2,
            revealed_share: genuine_share,
        }];
        let resolution = dkg_resolve(&session.packages, &complaints, &rebuttals).unwrap();
        assert!(resolution.excluded.is_empty());
        assert_eq!(resolution.false_complaints.len(), 1);
    }

    #[test]
    fn a_false_rebuttal_still_excludes_the_accused() {
        let session = run_round1(5, 3, b"ctx");
        let complaints = vec![DkgComplaint {
            accuser: 2,
            accused: 1,
        }];
        let rebuttals = vec![DkgRebuttal {
            accused: 1,
            accuser: 2,
            revealed_share: Scalar::from(12345u64), // not the real evaluation
        }];
        let resolution = dkg_resolve(&session.packages, &complaints, &rebuttals).unwrap();
        assert!(resolution.excluded.contains(&1));
    }

    #[test]
    fn an_accuser_cannot_frame_an_honest_sender_who_correctly_rebuts() {
        // The exact false-accusation scenario the module docs describe:
        // participant 2 lies about what they received from participant 1.
        let session = run_round1(5, 3, b"ctx");
        let complaints = vec![DkgComplaint {
            accuser: 2,
            accused: 1,
        }];
        // Participant 1 rebuts with the true value, proving 2 lied.
        let genuine_share = eval_polynomial(&session.secrets[&1].coefficients, index_scalar(2));
        let rebuttals = vec![DkgRebuttal {
            accused: 1,
            accuser: 2,
            revealed_share: genuine_share,
        }];
        let resolution = dkg_resolve(&session.packages, &complaints, &rebuttals).unwrap();
        assert!(
            !resolution.excluded.contains(&1),
            "an honest, correctly-rebutting sender must never be excluded"
        );
    }

    #[test]
    fn complaint_referencing_an_unknown_participant_is_rejected() {
        let session = run_round1(5, 3, b"ctx");
        let complaints = vec![DkgComplaint {
            accuser: 2,
            accused: 99,
        }];
        assert_eq!(
            dkg_resolve(&session.packages, &complaints, &[]).unwrap_err(),
            TreasuryError::InvalidFrostParticipant
        );
    }

    #[test]
    fn finalize_fails_when_exclusions_drop_below_threshold() {
        let session = run_round1(5, 3, b"ctx");
        let inboxes = exchange_round2(&session);
        let mut excluded = BTreeSet::new();
        excluded.insert(4u16);
        excluded.insert(5u16);
        excluded.insert(2u16); // only participants 1 and 3 remain -- below threshold 3
        let Session {
            secrets,
            packages,
            threshold,
            ..
        } = session;
        let secret_1 = secrets.into_iter().find(|(i, _)| *i == 1).unwrap().1;
        let err =
            dkg_finalize(1, secret_1, &packages, &inboxes[&1], &excluded, threshold).unwrap_err();
        assert_eq!(err, TreasuryError::NotEnoughSigners);
    }

    /// A participant who is a live, non-excluded member of `round1_packages`
    /// (never complained about, never rebutted, no reason to distrust them)
    /// but whose share simply never arrived -- an aborting or unresponsive
    /// participant, not a detected-bad one. `dkg_finalize` must refuse to
    /// silently proceed as if that contribution were zero.
    #[test]
    fn a_missing_share_from_a_non_excluded_participant_is_rejected_not_silently_dropped() {
        let session = run_round1(5, 3, b"ctx");
        let mut inboxes = exchange_round2(&session);
        inboxes.get_mut(&1).unwrap().remove(&2); // participant 2's share to 1 never arrived
        let excluded = BTreeSet::new(); // participant 2 was never accused or excluded

        let Session {
            secrets,
            packages,
            threshold,
            ..
        } = session;
        let secret_1 = secrets.into_iter().find(|(i, _)| *i == 1).unwrap().1;
        let err =
            dkg_finalize(1, secret_1, &packages, &inboxes[&1], &excluded, threshold).unwrap_err();
        assert_eq!(err, TreasuryError::InvalidFrostShare);
    }

    /// An equivocating sender: participant 1 sends recipient 2 the correct
    /// evaluation of their polynomial, but sends recipient 3 a *different*,
    /// inconsistent value -- both privately, so neither recipient alone can
    /// tell from their own inbox whether the other's copy differs. Each
    /// recipient's own local Feldman check still catches whichever of the
    /// two disagrees with the sender's public commitments (equivocation
    /// cannot produce two simultaneously-valid-looking shares against one
    /// fixed commitment vector, the same uniqueness property the module
    /// docs' complaint section relies on).
    #[test]
    fn an_equivocating_sender_is_caught_by_whichever_recipient_got_the_inconsistent_share() {
        let session = run_round1(5, 3, b"ctx");
        let genuine_share_to_2 =
            dkg_generate_round2_shares(&session.secrets[&1], &[2]).unwrap()[&2];
        let equivocated_share_to_3 = genuine_share_to_2 + Scalar::ONE; // deliberately different

        assert!(dkg_verify_received_share(
            &session.packages[&1],
            2,
            genuine_share_to_2
        ));
        assert!(!dkg_verify_received_share(
            &session.packages[&1],
            3,
            equivocated_share_to_3
        ));
    }

    #[test]
    fn finalize_excludes_a_misbehaving_participant_and_the_rest_still_converge() {
        let session = run_round1(5, 3, b"ctx");
        let inboxes = exchange_round2(&session);
        let mut excluded = BTreeSet::new();
        excluded.insert(3u16);

        let Session {
            secrets,
            packages,
            threshold,
            ..
        } = session;
        let mut results = BTreeMap::new();
        for (i, secret) in secrets {
            if i == 3 {
                continue; // the excluded participant does not finalize
            }
            let result =
                dkg_finalize(i, secret, &packages, &inboxes[&i], &excluded, threshold).unwrap();
            results.insert(i, result);
        }

        let group_keys: BTreeSet<_> = results
            .values()
            .map(|(_, public)| public.group_public_key.compress().to_bytes())
            .collect();
        assert_eq!(group_keys.len(), 1);
        assert!(
            !results[&1].1.verifying_shares.contains_key(&3),
            "excluded participant must not appear in the final key"
        );
    }

    #[test]
    fn a_finalized_dkg_key_signs_and_verifies_through_ordinary_frost_signing() {
        use crate::frost_sign::{aggregate, round1_commit, round2_sign, verify, SigningPackage};

        let session = run_round1(5, 3, b"ctx");
        let inboxes = exchange_round2(&session);
        let results = finalize_all(session, inboxes);

        let signer_indices = [1u16, 2, 4];
        let public = results[&1].1.clone();
        let message = b"treasury payout signed by a DKG-generated key";

        let mut nonces_by_index = BTreeMap::new();
        let mut commitments = Vec::new();
        for &i in &signer_indices {
            let (nonces, commitment) = round1_commit(i).unwrap();
            nonces_by_index.insert(i, nonces);
            commitments.push(commitment);
        }
        let signing_package = SigningPackage::new(3, message.to_vec(), commitments).unwrap();

        let mut shares = BTreeMap::new();
        for &i in &signer_indices {
            let key_package = &results[&i].0;
            let nonces = nonces_by_index.remove(&i).unwrap();
            let z = round2_sign(key_package, nonces, &signing_package).unwrap();
            shares.insert(i, z);
        }

        let signature = aggregate(&signing_package, &shares, &public).unwrap();
        assert!(verify(&signature, message, public.group_public_key));
    }
}
