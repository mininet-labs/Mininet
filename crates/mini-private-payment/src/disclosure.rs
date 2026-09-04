//! Choosing to be audited, without making everyone else auditable.
//!
//! # The problem this solves
//!
//! Some payments genuinely should be checkable by anyone. A treasury
//! disbursement under D-0073 is the clearest case: an account spending
//! commonly-held value ought to be answerable for it, and "trust us" is not
//! an answer this project accepts from anybody, including itself.
//!
//! The obvious way to get that is to keep a transparent payment format
//! alongside the private one and use it for those cases. That is the wrong
//! shape, for a reason worth stating plainly: **if both formats exist, the
//! choice to use the private one is itself a signal.** Every private payment
//! then carries an implicit "why did this person need privacy?", and privacy
//! that must be opted into is privacy for nobody — the people who most need
//! it are exactly the people whose opting-in is most visible.
//!
//! So auditability moves from being a property of the *format* to a
//! disclosure a party makes *about itself*. Nothing is public by default.
//! An account that wants to be auditable publishes its view key, and from
//! then on anyone can scan its income. The treasury can be fully accountable
//! without anyone else being exposed.
//!
//! # What publishing a view key actually does
//!
//! Read this before using anything in this module.
//!
//! A view key is what recognizes payments to an account. Publishing it lets
//! anyone do what the account holder can do: identify every payment ever
//! made to that account and read every memo attached to them.
//!
//! - **It is retroactive.** Not "from now on" — a view key decrypts payments
//!   received *before* the disclosure just as well as after. There is no
//!   cryptographic way to disclose only the future.
//! - **It is irrevocable.** You cannot unpublish a key. Rotating to a new
//!   account limits future exposure and does nothing about the past.
//! - **It exposes counterparties, not just the discloser.** Every memo the
//!   senders wrote becomes readable. They did not consent to that and were
//!   probably never asked.
//!
//! Because of the third point especially, [`ViewKeyDisclosure::create`]
//! requires a typed [`AcknowledgedIrreversibleDisclosure`], following the
//! same discipline as `mini_installer`'s `OwnerApproval` and
//! `mini-treasury`'s `AcknowledgedUnauditedDkg`. There is no convenience
//! path, and there should never be one: this is not an operation anybody
//! should be able to perform by passing an extra `true`.
//!
//! # What it does not do
//!
//! - **It does not reveal spending.** A view key recognizes *incoming*
//!   payments. It does not identify which outputs the account later spent,
//!   because that requires the spend key. An audit of a disclosed account
//!   sees money arriving, not money leaving — a real and asymmetric limit.
//! - **It does not reveal amounts.** A view key recognizes an output; it
//!   does not open the Pedersen commitment holding its value. An account
//!   that wants its sums checkable publishes [`crate::AmountDisclosure`]s
//!   as well — a separate, per-output, deliberately *chosen* act, which is
//!   why [`crate::audit_amounts`] reports what was left unopened rather
//!   than a total that would quietly mean less than it looks like.
//! - **It does not prove completeness.** A disclosure covers one account.
//!   Nothing here proves that account is the only one its holder controls,
//!   and no cryptography can prove that. "The treasury disclosed a view key"
//!   means what that account received is checkable, never that the treasury
//!   has only one account.
//! - **It carries no identity binding.** Like `mini_settlement::PaymentClaim`
//!   and its opaque payer bytes, this crate takes no position on who an
//!   account belongs to. Binding a disclosure to a `did:mini` root is the
//!   caller's business, and keeping it out avoids an identity dependency in
//!   a value crate.

use mini_crypto::HashAlgorithm;

use crate::claim::VerifiedPrivateClaim;
use crate::codec::{Reader, Writer};
use crate::error::{DecodeFailure, PrivatePaymentError, Result};
use crate::scan::{scan, ScanOutcome};

/// Domain separator for the disclosure transcript.
pub const DISCLOSURE_DOMAIN: &[u8] = b"mininet/mini-private-payment/view-key-disclosure/v1";

/// Wire format version.
pub const DISCLOSURE_VERSION: u8 = 1;

/// Proof that the caller understands what publishing a view key does.
///
/// Deliberately unconstructable by accident: there is exactly one
/// constructor, it is named for the consequence rather than the action, and
/// it takes the acknowledgement as prose the caller must write out. A
/// boolean would be typed as `true` by someone skimming; this cannot be.
///
/// The same pattern as `mini_installer::OwnerApproval` and
/// `mini-treasury`'s `AcknowledgedUnauditedDkg` — the project's standing
/// answer to "this is irreversible and somebody will do it by mistake".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcknowledgedIrreversibleDisclosure {
    _private: (),
}

impl AcknowledgedIrreversibleDisclosure {
    /// The exact phrase a caller must supply, verbatim.
    pub const REQUIRED_ACKNOWLEDGEMENT: &'static str =
        "publishing this view key is permanent and reveals every payment ever \
         made to this account, including memos written by people who did not \
         agree to this";

    /// Acknowledge the consequence. `acknowledgement` must equal
    /// [`Self::REQUIRED_ACKNOWLEDGEMENT`] exactly.
    ///
    /// The phrase is long on purpose. A short one gets copied without being
    /// read; a long one that names the third-party exposure is harder to
    /// paste past without noticing what it says.
    pub fn new(acknowledgement: &str) -> Option<Self> {
        if acknowledgement == Self::REQUIRED_ACKNOWLEDGEMENT {
            Some(Self { _private: () })
        } else {
            None
        }
    }
}

/// A published view key: an account volunteering to be auditable.
///
/// **Fields are private on purpose.** Public fields would make
/// [`Self::create`]'s acknowledgement decorative — anyone could write the
/// struct literal and never encounter
/// [`AcknowledgedIrreversibleDisclosure`] at all, which is exactly the
/// "authority assembled from raw parts" shape the typed-domains rule exists
/// to refuse. There are two ways to get one: [`Self::create`], which is
/// publishing and takes the acknowledgement, and [`Self::decode`], which is
/// *reading someone else's already-published* disclosure and needs no
/// acknowledgement because the irreversible act already happened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewKeyDisclosure {
    spend_public: Vec<u8>,
    view_public: Vec<u8>,
    view_secret: Vec<u8>,
    reason: Vec<u8>,
    disclosed_at_ms: u64,
}

/// A disclosure whose keys have been checked to actually belong together.
///
/// Constructing one outside [`verify_disclosure`] is impossible.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedDisclosure {
    disclosure: ViewKeyDisclosure,
    digest: [u8; 32],
}

impl VerifiedDisclosure {
    pub fn disclosure(&self) -> &ViewKeyDisclosure {
        &self.disclosure
    }

    /// BLAKE3 over the canonical encoding — this disclosure's identity.
    pub fn digest(&self) -> &[u8; 32] {
        &self.digest
    }
}

impl ViewKeyDisclosure {
    /// Publish an account's view key.
    ///
    /// Takes the acknowledgement by type rather than by flag. See this
    /// module's docs for what is actually being given away — in particular
    /// that it is retroactive, irrevocable, and exposes the memos of people
    /// who never agreed to it.
    pub fn create(
        spend_public: impl Into<Vec<u8>>,
        view_public: impl Into<Vec<u8>>,
        view_secret: impl Into<Vec<u8>>,
        reason: impl Into<Vec<u8>>,
        disclosed_at_ms: u64,
        _acknowledged: &AcknowledgedIrreversibleDisclosure,
    ) -> Self {
        Self {
            spend_public: spend_public.into(),
            view_public: view_public.into(),
            view_secret: view_secret.into(),
            reason: reason.into(),
            disclosed_at_ms,
        }
    }

    /// The account's published spend public key — its stable identity as a
    /// payee, and what an auditor scans against.
    pub fn spend_public(&self) -> &[u8] {
        &self.spend_public
    }

    /// The account's published view public key.
    pub fn view_public(&self) -> &[u8] {
        &self.view_public
    }

    /// The view **secret**. This is the disclosure; everything else is
    /// context. Once these bytes are published they cannot be unpublished.
    pub fn view_secret(&self) -> &[u8] {
        &self.view_secret
    }

    /// The discloser's stated reason, in the clear. Not verified and not
    /// authoritative — a label for humans reading a disclosure, never a
    /// claim the protocol checks or a field to build policy on.
    pub fn reason(&self) -> &[u8] {
        &self.reason
    }

    /// When the discloser says it published. Self-reported, like every other
    /// timestamp in this tree that lacks an anchor — and specifically **not**
    /// a bound on what the disclosure reveals, since a view key is
    /// retroactive.
    pub fn disclosed_at_ms(&self) -> u64 {
        self.disclosed_at_ms
    }

    /// Canonical encoding.
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.raw(DISCLOSURE_DOMAIN);
        w.u8(DISCLOSURE_VERSION);
        w.bytes(&self.spend_public);
        w.bytes(&self.view_public);
        w.bytes(&self.view_secret);
        w.bytes(&self.reason);
        w.u64(self.disclosed_at_ms);
        w.finish()
    }

    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let mut r = Reader::new(bytes);
        let domain = r.array::<{ DISCLOSURE_DOMAIN.len() }>()?;
        if domain != DISCLOSURE_DOMAIN {
            return Err(DecodeFailure::UnsupportedVersion.into());
        }
        if r.u8()? != DISCLOSURE_VERSION {
            return Err(DecodeFailure::UnsupportedVersion.into());
        }
        let spend_public = r.field_element()?;
        let view_public = r.field_element()?;
        let view_secret = r.field_element()?;
        let reason = r.bytes()?;
        let disclosed_at_ms = r.u64()?;
        r.finish()?;
        Ok(Self {
            spend_public,
            view_public,
            view_secret,
            reason,
            disclosed_at_ms,
        })
    }

    /// BLAKE3 over [`Self::encode`].
    pub fn digest(&self) -> [u8; 32] {
        HashAlgorithm::Blake3.digest(&self.encode())
    }
}

/// Check that a disclosure's secret really is the account's view key.
///
/// The one thing worth verifying, and the reason this is not just a struct:
/// a disclosure whose secret is unrelated to its published view key would
/// produce an audit that finds nothing and looks exactly like an account
/// with no income. Refusing it means "audited and empty" and "not really
/// disclosed" cannot be confused.
///
/// Two limits, both real:
///
/// - **It binds the view keypair, not the account.** Pairing a genuine view
///   keypair with somebody else's *spend* key is internally consistent and
///   verifies here. The audit then finds nothing — indistinguishable from an
///   honest empty account. Nothing in a self-contained disclosure object can
///   close that; only an external statement about whose account this is
///   could, which is the caller's business (below).
/// - **It does not check who published it.** Anyone holding the view secret
///   can publish it, which is inherent to it being a secret they hold. No
///   check here changes that. Binding a disclosure to a `did:mini` root is
///   the caller's job, deliberately, so a value crate keeps no identity
///   dependency.
pub fn verify_disclosure(disclosure: &ViewKeyDisclosure) -> Result<VerifiedDisclosure> {
    if !mini_value::stealth_address_is_well_formed(
        disclosure.spend_public(),
        disclosure.view_public(),
    ) {
        return Err(PrivatePaymentError::MalformedDisclosureKey);
    }

    // The secret must be the discrete log of the published view public key.
    // Derived by the same operation the payment path uses, so a disclosure
    // that verifies here is a disclosure that will actually recognize
    // payments -- rather than one that merely looks well-formed.
    let expected = mini_value::view_public_from_secret(disclosure.view_secret())
        .ok_or(PrivatePaymentError::MalformedDisclosureKey)?;
    if expected.as_slice() != disclosure.view_public() {
        return Err(PrivatePaymentError::DisclosureKeyMismatch);
    }

    Ok(VerifiedDisclosure {
        disclosure: disclosure.clone(),
        digest: disclosure.digest(),
    })
}

/// Find the payments made to a disclosed account.
///
/// This is literally [`crate::scan`] with the published key, and returns
/// literally [`ScanOutcome`], because that identity *is* the design:
/// disclosure grants the public the holder's reading ability, no more and no
/// less. Anything an auditor could learn here, the holder could already
/// learn; anything the holder cannot learn, an auditor does not get either.
/// A separate, richer audit type would have been the beginning of "audited"
/// quietly meaning "more exposed than the owner", so there is not one.
///
/// Two limits carried over unchanged, both worth stating at the call site:
///
/// - **Amounts stay hidden.** A Pedersen commitment is not opened by a view
///   key, so an audit sees *which* payments arrived and what they were for,
///   never how much they were worth — unless the account also publishes
///   [`crate::AmountDisclosure`]s, which open chosen commitments. A view
///   key alone means the set of incoming payments is checkable, not the
///   sums.
/// - **A stranger can add noise.** Anyone can pay a published address, so
///   [`ScanOutcome::unreadable`] may hold payments an unrelated party sent to
///   make the audit look untidy. They cost the sender real value and reveal
///   nothing, but an auditor should not read a non-empty `unreadable` as
///   evidence the discloser hid something.
pub fn audit<'a>(
    disclosure: &VerifiedDisclosure,
    claims: impl IntoIterator<Item = &'a VerifiedPrivateClaim>,
) -> ScanOutcome {
    scan(
        disclosure.disclosure.view_secret(),
        disclosure.disclosure.spend_public(),
        claims,
    )
}
