//! A real (CryptoNote-style) [`crate::stealth::StealthAddressScheme`]
//! implementation. Founder-overridden, AI-authored prototype — see
//! [`crate::stealth`]'s honest limit and D-0036. Do not treat this as
//! production-ready.
//!
//! ## The scheme
//!
//! A recipient publishes two public keys: a spend public key `A = a*G` and
//! a view public key `B = b*G`, keeping both secrets `a`/`b` private (`b`
//! can be kept "hotter" than `a`, since it is only needed to scan, not to
//! spend).
//!
//! To pay that recipient, a sender picks a fresh random scalar `r`,
//! publishes `R = r*G` alongside the payment, computes the Diffie-Hellman
//! shared point `r*B`, derives a shared scalar `s = H(r*B)`, and sends the
//! payment to the one-time address `P = s*G + A`. Nobody except the
//! recipient (via `b`) or the sender (via `r`) can compute `s*G`, so `P`
//! reveals nothing about `A` to anyone else.
//!
//! The recipient scans the ledger by recomputing the same shared point
//! from the *other* side of the same Diffie-Hellman exchange — `b*R`,
//! which equals `r*B` — deriving the same `s`, and checking whether
//! `s*G + A` matches a candidate output's address. Recognizing needs only
//! the view secret `b`; actually spending the output additionally needs
//! the spend secret `a`, via the one-time private key `x = s + a` (so that
//! `x*G == P`) — [`derive_spend_scalar`] computes it, kept separate from
//! the [`crate::stealth::StealthAddressScheme`] trait since recognition
//! and spending are deliberately different privilege levels.

use zeroize::Zeroize;

use crate::curve::{hash_to_scalar, random_scalar, CompressedRistretto, RistrettoPoint, Scalar};
use crate::error::Result;
use crate::stealth::{StealthAddressScheme, StealthOutput};

/// A recipient's stealth keypair: a spend keypair and a view keypair.
/// `spend_public`/`view_public` (via [`Self::spend_public_bytes`]/
/// [`Self::view_public_bytes`]) are what gets published as this
/// recipient's address; the two secrets never leave the device that
/// generated them.
#[derive(Clone)]
pub struct StealthKeypair {
    spend_secret: Scalar,
    spend_public: RistrettoPoint,
    view_secret: Scalar,
    view_public: RistrettoPoint,
}

impl core::fmt::Debug for StealthKeypair {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("StealthKeypair")
            .field(
                "spend_public",
                &hex(self.spend_public.compress().as_bytes()),
            )
            .field("view_public", &hex(self.view_public.compress().as_bytes()))
            .finish_non_exhaustive()
    }
}

impl StealthKeypair {
    /// Generate a fresh keypair from the OS CSPRNG.
    pub fn generate() -> Result<Self> {
        let spend_secret = random_scalar()?;
        let view_secret = random_scalar()?;
        Ok(StealthKeypair {
            spend_public: spend_secret * crate::curve::basepoint(),
            spend_secret,
            view_public: view_secret * crate::curve::basepoint(),
            view_secret,
        })
    }

    /// The public spend key to publish as part of this recipient's address.
    pub fn spend_public_bytes(&self) -> [u8; 32] {
        self.spend_public.compress().to_bytes()
    }

    /// The public view key to publish as part of this recipient's address.
    pub fn view_public_bytes(&self) -> [u8; 32] {
        self.view_public.compress().to_bytes()
    }

    /// The private view key, for scanning — see
    /// [`StealthAddressScheme::recognizes`].
    pub fn view_secret_bytes(&self) -> [u8; 32] {
        self.view_secret.to_bytes()
    }

    /// The private spend key, needed only to actually spend a recognized
    /// output — see [`derive_spend_scalar`]. Deliberately a separate,
    /// explicit accessor from [`Self::view_secret_bytes`]: recognizing a
    /// payment and being able to spend it are different privilege levels,
    /// and a real wallet should keep this one colder.
    pub fn spend_secret_bytes(&self) -> [u8; 32] {
        self.spend_secret.to_bytes()
    }
}

fn hex(bytes: &[u8]) -> String {
    use core::fmt::Write;
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(out, "{b:02x}");
    }
    out
}

fn decompress_point(bytes: &[u8]) -> Option<RistrettoPoint> {
    let arr: [u8; 32] = bytes.try_into().ok()?;
    CompressedRistretto(arr).decompress()
}

fn decompress_scalar(bytes: &[u8]) -> Option<Scalar> {
    let arr: [u8; 32] = bytes.try_into().ok()?;
    Some(Scalar::from_bytes_mod_order(arr))
}

/// The prototype [`StealthAddressScheme`] implementation (D-0036).
#[derive(Debug, Clone, Copy, Default)]
pub struct MininetStealthAddress;

impl StealthAddressScheme for MininetStealthAddress {
    fn derive_output(
        &mut self,
        recipient_spend_public: &[u8],
        recipient_view_public: &[u8],
    ) -> Option<StealthOutput> {
        let a = decompress_point(recipient_spend_public)?;
        let b = decompress_point(recipient_view_public)?;
        let r = random_scalar().ok()?;
        let tx_public_key = r * crate::curve::basepoint();
        let shared_point = r * b;
        let s = hash_to_scalar(&[shared_point.compress().as_bytes()]);
        let one_time_address = s * crate::curve::basepoint() + a;
        Some(StealthOutput {
            tx_public_key: tx_public_key.compress().to_bytes().to_vec(),
            one_time_address: one_time_address.compress().to_bytes().to_vec(),
        })
    }

    fn recognizes(
        &self,
        own_view_secret: &[u8],
        own_spend_public: &[u8],
        output: &StealthOutput,
    ) -> bool {
        let Some(b) = decompress_scalar(own_view_secret) else {
            return false;
        };
        let Some(a) = decompress_point(own_spend_public) else {
            return false;
        };
        let Some(r_pub) = decompress_point(&output.tx_public_key) else {
            return false;
        };
        let shared_point = b * r_pub;
        let s = hash_to_scalar(&[shared_point.compress().as_bytes()]);
        let expected = s * crate::curve::basepoint() + a;
        expected.compress().to_bytes().as_slice() == output.one_time_address.as_slice()
    }
}

/// A stealth payment's Diffie-Hellman shared point, in compressed form.
///
/// The sender computes `r*B` and the recipient computes `b*R`; these are
/// the same point. It is already computed on both sides to derive the
/// one-time address — it was simply discarded, which meant a sender had
/// no way to attach anything only the recipient could read. Returning it
/// introduces no new cryptography; it exposes a value the existing
/// CryptoNote-style derivation already produces.
///
/// **Never use these bytes directly as a key.** They are a curve point,
/// not uniform key material. Run them through a KDF with a domain
/// separator — `mini_crypto::KdfSuite::derive_aead_key` is what
/// `mini-private-payment` uses.
#[derive(Clone, PartialEq, Eq)]
pub struct StealthSharedSecret([u8; 32]);

impl StealthSharedSecret {
    /// The compressed shared point. Named to make the "this is not a key"
    /// point unmissable at every call site.
    pub fn as_key_material(&self) -> &[u8; 32] {
        &self.0
    }
}

impl core::fmt::Debug for StealthSharedSecret {
    /// Redacted: a shared secret in a log is a decryptable payment memo.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("StealthSharedSecret(<redacted>)")
    }
}

impl Drop for StealthSharedSecret {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

/// Derive a stealth output **and** return the shared secret used to build
/// it, so the sender can seal a memo only this recipient can open.
///
/// Identical derivation to
/// [`StealthAddressScheme::derive_output`](crate::StealthAddressScheme::derive_output)
/// — same fresh `r` per call, same unlinkability — differing only in what
/// it hands back.
pub fn derive_output_with_secret(
    recipient_spend_public: &[u8],
    recipient_view_public: &[u8],
) -> Option<(StealthOutput, StealthSharedSecret)> {
    let a = decompress_point(recipient_spend_public)?;
    let b = decompress_point(recipient_view_public)?;
    let r = random_scalar().ok()?;
    let tx_public_key = r * crate::curve::basepoint();
    let shared_point = r * b;
    let shared_bytes = shared_point.compress().to_bytes();
    let s = hash_to_scalar(&[&shared_bytes]);
    let one_time_address = s * crate::curve::basepoint() + a;
    Some((
        StealthOutput {
            tx_public_key: tx_public_key.compress().to_bytes().to_vec(),
            one_time_address: one_time_address.compress().to_bytes().to_vec(),
        },
        StealthSharedSecret(shared_bytes),
    ))
}

/// Recover the same shared secret on the recipient's side, from their view
/// secret and the sender's published `tx_public_key`.
///
/// The **view** secret alone suffices, matching
/// [`StealthAddressScheme::recognizes`](crate::StealthAddressScheme::recognizes):
/// reading a payment's memo is a view-key privilege, spending it is not,
/// so the spend key stays offline.
pub fn recover_shared_secret(
    own_view_secret: &[u8],
    tx_public_key: &[u8],
) -> Option<StealthSharedSecret> {
    let b = decompress_scalar(own_view_secret)?;
    let r_pub = decompress_point(tx_public_key)?;
    Some(StealthSharedSecret((b * r_pub).compress().to_bytes()))
}

/// Derive the one-time private scalar `x = s + a` needed to spend
/// `output`, given the recipient's own view and spend secrets. Kept
/// separate from [`StealthAddressScheme`]: recognizing a payment (view
/// secret only) and being able to spend it (view + spend secret) are
/// deliberately different privilege levels, the same "spend key stays
/// colder" principle real wallets rely on.
pub fn derive_spend_scalar(
    own_view_secret: &[u8],
    own_spend_secret: &[u8],
    output: &StealthOutput,
) -> Option<Scalar> {
    let b = decompress_scalar(own_view_secret)?;
    let a = decompress_scalar(own_spend_secret)?;
    let r_pub = decompress_point(&output.tx_public_key)?;
    let shared_point = b * r_pub;
    let s = hash_to_scalar(&[shared_point.compress().as_bytes()]);
    Some(s + a)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recipient_recognizes_their_own_output() {
        let recipient = StealthKeypair::generate().unwrap();
        let mut scheme = MininetStealthAddress;
        let output = scheme
            .derive_output(
                &recipient.spend_public_bytes(),
                &recipient.view_public_bytes(),
            )
            .unwrap();

        assert!(scheme.recognizes(
            &recipient.view_secret_bytes(),
            &recipient.spend_public_bytes(),
            &output,
        ));
    }

    #[test]
    fn a_different_recipient_does_not_recognize_the_output() {
        let recipient = StealthKeypair::generate().unwrap();
        let outsider = StealthKeypair::generate().unwrap();
        let mut scheme = MininetStealthAddress;
        let output = scheme
            .derive_output(
                &recipient.spend_public_bytes(),
                &recipient.view_public_bytes(),
            )
            .unwrap();

        assert!(!scheme.recognizes(
            &outsider.view_secret_bytes(),
            &outsider.spend_public_bytes(),
            &output,
        ));
    }

    #[test]
    fn two_outputs_to_the_same_recipient_are_unlinkable() {
        let recipient = StealthKeypair::generate().unwrap();
        let mut scheme = MininetStealthAddress;
        let output_a = scheme
            .derive_output(
                &recipient.spend_public_bytes(),
                &recipient.view_public_bytes(),
            )
            .unwrap();
        let output_b = scheme
            .derive_output(
                &recipient.spend_public_bytes(),
                &recipient.view_public_bytes(),
            )
            .unwrap();

        assert_ne!(output_a.tx_public_key, output_b.tx_public_key);
        assert_ne!(output_a.one_time_address, output_b.one_time_address);
        // Both are still recognized as the same recipient's, despite looking
        // completely unrelated on the wire.
        assert!(scheme.recognizes(
            &recipient.view_secret_bytes(),
            &recipient.spend_public_bytes(),
            &output_a,
        ));
        assert!(scheme.recognizes(
            &recipient.view_secret_bytes(),
            &recipient.spend_public_bytes(),
            &output_b,
        ));
    }

    #[test]
    fn derived_spend_scalar_actually_opens_the_one_time_address() {
        let recipient = StealthKeypair::generate().unwrap();
        let mut scheme = MininetStealthAddress;
        let output = scheme
            .derive_output(
                &recipient.spend_public_bytes(),
                &recipient.view_public_bytes(),
            )
            .unwrap();

        let x = derive_spend_scalar(
            &recipient.view_secret_bytes(),
            &recipient.spend_secret_bytes(),
            &output,
        )
        .unwrap();

        let reconstructed = (x * crate::curve::basepoint()).compress().to_bytes();
        assert_eq!(reconstructed.as_slice(), output.one_time_address.as_slice());
    }

    #[test]
    fn malformed_keys_are_rejected_without_panicking() {
        let mut scheme = MininetStealthAddress;
        assert_eq!(scheme.derive_output(b"too-short", b"also-too-short"), None);

        let fake_output = StealthOutput {
            tx_public_key: vec![0u8; 4],
            one_time_address: vec![0u8; 32],
        };
        assert!(!scheme.recognizes(b"view", b"spend", &fake_output));
    }

    #[test]
    fn sender_and_recipient_derive_the_same_shared_secret() {
        let recipient = StealthKeypair::generate().unwrap();
        let (output, sender_secret) = derive_output_with_secret(
            &recipient.spend_public_bytes(),
            &recipient.view_public_bytes(),
        )
        .unwrap();

        let recipient_secret =
            recover_shared_secret(&recipient.view_secret_bytes(), &output.tx_public_key).unwrap();
        assert_eq!(
            sender_secret.as_key_material(),
            recipient_secret.as_key_material()
        );
    }

    #[test]
    fn derive_output_with_secret_produces_a_recognizable_output() {
        // The secret-returning variant must derive *the same way* as the
        // trait method, or a memo would be readable by someone who cannot
        // recognize the payment it is attached to.
        let recipient = StealthKeypair::generate().unwrap();
        let (output, _) = derive_output_with_secret(
            &recipient.spend_public_bytes(),
            &recipient.view_public_bytes(),
        )
        .unwrap();

        let scheme = MininetStealthAddress;
        assert!(scheme.recognizes(
            &recipient.view_secret_bytes(),
            &recipient.spend_public_bytes(),
            &output,
        ));
    }

    #[test]
    fn a_stranger_derives_a_different_shared_secret() {
        let recipient = StealthKeypair::generate().unwrap();
        let stranger = StealthKeypair::generate().unwrap();
        let (output, sender_secret) = derive_output_with_secret(
            &recipient.spend_public_bytes(),
            &recipient.view_public_bytes(),
        )
        .unwrap();

        let wrong =
            recover_shared_secret(&stranger.view_secret_bytes(), &output.tx_public_key).unwrap();
        assert_ne!(sender_secret.as_key_material(), wrong.as_key_material());
    }

    #[test]
    fn two_payments_to_one_recipient_share_no_secret() {
        // Fresh randomness per call is what makes outputs unlinkable; if
        // two payments derived the same secret they would also derive the
        // same one-time address, defeating the entire scheme.
        let recipient = StealthKeypair::generate().unwrap();
        let (first_out, first) = derive_output_with_secret(
            &recipient.spend_public_bytes(),
            &recipient.view_public_bytes(),
        )
        .unwrap();
        let (second_out, second) = derive_output_with_secret(
            &recipient.spend_public_bytes(),
            &recipient.view_public_bytes(),
        )
        .unwrap();
        assert_ne!(first.as_key_material(), second.as_key_material());
        assert_ne!(first_out.one_time_address, second_out.one_time_address);
    }

    #[test]
    fn a_shared_secret_never_prints_its_bytes() {
        let recipient = StealthKeypair::generate().unwrap();
        let secret = recover_shared_secret(&recipient.view_secret_bytes(), &{
            let (o, _) = derive_output_with_secret(
                &recipient.spend_public_bytes(),
                &recipient.view_public_bytes(),
            )
            .unwrap();
            o.tx_public_key
        })
        .unwrap();
        let rendered = format!("{secret:?}");
        assert!(rendered.contains("redacted"), "{rendered}");
        assert!(!rendered.contains(&hex(secret.as_key_material())));
    }

    #[test]
    fn malformed_inputs_to_the_shared_secret_helpers_are_rejected() {
        assert!(derive_output_with_secret(b"short", b"short").is_none());
        assert!(recover_shared_secret(b"short", b"short").is_none());
    }
}
