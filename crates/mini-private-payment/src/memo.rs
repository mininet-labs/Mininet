//! What a payment is *for*, sealed so only its recipient learns it.
//!
//! # Why this exists
//!
//! Paying a creator for a specific post is the natural thing to want, and
//! the naive way to express it is a `content_id` field on the payment. That
//! field would publish an engagement graph: which posts are being paid for,
//! how often, and — combined with timing — by whom. The network would learn
//! exactly what `mini_store::Store::note_view` refuses to record, having
//! been carefully built to take no viewer identity at all (PR2, D-0033). It
//! would be strange to protect viewing and then leak the same fact through
//! the payment.
//!
//! So the purpose travels sealed. The sender derives the stealth shared
//! secret it already computes for the one-time address, runs it through a
//! KDF, and AEAD-seals the purpose to that key. The recipient recovers the
//! same secret with their **view** key alone and opens it. Everyone else
//! sees an opaque blob of a fixed padded size.
//!
//! # The AAD binding, and the attack it stops
//!
//! The memo is sealed with the claim's transcript digest as additional
//! authenticated data. Without that binding a memo could be lifted from one
//! claim and pasted onto another: an attacker who saw "payment for post X"
//! could attach that memo to their own smaller payment, and the recipient
//! would credit the wrong payment to the wrong post. AAD makes the memo
//! openable only on the claim it was written for.
//!
//! # What is still visible
//!
//! The memo's *existence* and its padded length. Padding to
//! [`MEMO_PADDED_BYTES`] means a 4-byte purpose and a 200-byte one look
//! identical, but a payment with a memo and one without are distinguishable
//! — which is why [`SealedMemo::empty_for`] exists, so every claim carries
//! one and "no memo" is not itself a signal.

use mini_crypto::{AeadKey, AeadNonce, AeadSuite, HashAlgorithm, KdfSuite};
use mini_value::StealthSharedSecret;

use crate::codec::{Reader, Writer};
use crate::error::{PrivatePaymentError, Result};

/// KDF info string for the memo key. Domain-separated so this key can
/// never coincide with one derived from the same shared secret for another
/// purpose.
pub const MEMO_KDF_INFO: &[u8] = b"mininet/mini-private-payment/memo-key/v1";

/// Every memo plaintext is padded to exactly this length before sealing,
/// so the ciphertext length reveals nothing about the purpose.
pub const MEMO_PADDED_BYTES: usize = 256;

/// Longest purpose payload a caller may seal, leaving room for the length
/// prefix inside the padded block.
pub const MAX_MEMO_BYTES: usize = MEMO_PADDED_BYTES - 4;

/// What a payment is for, in the clear — only ever held by the sender
/// before sealing or the recipient after opening.
///
/// `reference` is deliberately opaque bytes rather than a typed content
/// id. This crate must not depend on the social layer (see the crate docs
/// on why), and a payment layer that knew what a post was would be a
/// payment layer that could be made to treat some posts differently.
#[derive(Clone, PartialEq, Eq)]
pub struct PaymentPurpose {
    /// Caller-defined: a content id, an invoice number, a note. Opaque
    /// here, meaningful to the two parties.
    pub reference: Vec<u8>,
}

impl PaymentPurpose {
    /// A purpose referring to `reference`.
    pub fn new(reference: impl Into<Vec<u8>>) -> Self {
        Self {
            reference: reference.into(),
        }
    }

    /// The empty purpose — a payment with nothing attached.
    pub fn none() -> Self {
        Self {
            reference: Vec::new(),
        }
    }

    fn encode_padded(&self) -> Result<Vec<u8>> {
        if self.reference.len() > MAX_MEMO_BYTES {
            return Err(PrivatePaymentError::MemoTooLarge {
                got: self.reference.len(),
                max: MAX_MEMO_BYTES,
            });
        }
        let mut writer = Writer::new();
        writer.bytes(&self.reference);
        let mut block = writer.finish();
        block.resize(MEMO_PADDED_BYTES, 0);
        Ok(block)
    }

    fn decode_padded(block: &[u8]) -> Result<Self> {
        if block.len() != MEMO_PADDED_BYTES {
            return Err(PrivatePaymentError::MalformedMemo);
        }
        let mut reader = Reader::new(block);
        let reference = reader
            .bytes()
            .map_err(|_| PrivatePaymentError::MalformedMemo)?;
        // The remainder must be zero padding and nothing else: a memo with
        // a second message hidden in its tail would be a covert channel
        // that survives every check above.
        let consumed = 4 + reference.len();
        if block[consumed..].iter().any(|byte| *byte != 0) {
            return Err(PrivatePaymentError::MalformedMemo);
        }
        Ok(Self { reference })
    }
}

impl core::fmt::Debug for PaymentPurpose {
    /// Redacted: the whole point of a memo is that it is not public, and a
    /// debug log is public enough.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "PaymentPurpose(<{} bytes, redacted>)",
            self.reference.len()
        )
    }
}

/// A [`PaymentPurpose`] sealed to one recipient and one claim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SealedMemo {
    /// AEAD ciphertext over the padded purpose block.
    pub ciphertext: Vec<u8>,
}

impl SealedMemo {
    /// Seal `purpose` to the recipient who can recover `shared`, bound to
    /// `transcript_digest` so it cannot be moved onto another claim.
    pub fn seal(
        purpose: &PaymentPurpose,
        shared: &StealthSharedSecret,
        transcript_digest: &[u8; 32],
    ) -> Result<Self> {
        let block = purpose.encode_padded()?;
        let key = memo_key(shared)?;
        // A fixed all-zero nonce is correct here and nowhere else: the key
        // is derived from a shared secret that is fresh per payment (the
        // stealth `r` is drawn per call), so this key encrypts exactly one
        // message in its entire lifetime. Reusing a nonce under a reused
        // key is what breaks ChaCha20-Poly1305; neither happens here.
        let nonce = AeadNonce::from_bytes(&[0u8; 12])
            .map_err(|_| PrivatePaymentError::CryptoUnavailable)?;
        let ciphertext = key
            .encrypt(&nonce, &block, transcript_digest)
            .map_err(|_| PrivatePaymentError::CryptoUnavailable)?;
        Ok(Self { ciphertext })
    }

    /// A sealed empty purpose, so a claim carrying no message is
    /// byte-indistinguishable from one that does.
    pub fn empty_for(shared: &StealthSharedSecret, transcript_digest: &[u8; 32]) -> Result<Self> {
        Self::seal(&PaymentPurpose::none(), shared, transcript_digest)
    }

    /// Open this memo with a recovered shared secret.
    ///
    /// Returns [`PrivatePaymentError::MemoNotForYou`] when the AEAD tag
    /// fails, which is the expected outcome for every payment that is not
    /// yours — a scanning wallet calls this constantly and most calls fail.
    pub fn open(
        &self,
        shared: &StealthSharedSecret,
        transcript_digest: &[u8; 32],
    ) -> Result<PaymentPurpose> {
        let key = memo_key(shared)?;
        let nonce = AeadNonce::from_bytes(&[0u8; 12])
            .map_err(|_| PrivatePaymentError::CryptoUnavailable)?;
        let block = key
            .decrypt(&nonce, &self.ciphertext, transcript_digest)
            .map_err(|_| PrivatePaymentError::MemoNotForYou)?;
        PaymentPurpose::decode_padded(&block)
    }

    pub(crate) fn write_into(&self, writer: &mut Writer) {
        writer.bytes(&self.ciphertext);
    }

    pub(crate) fn read_from(reader: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            ciphertext: reader.bytes()?,
        })
    }
}

/// Derive the memo's AEAD key from the stealth shared point.
///
/// The shared point is not uniform key material, so it never becomes a key
/// directly — HKDF with a domain-separated info string does that, the same
/// discipline `mini-bearer` applies to its own handshake secrets.
fn memo_key(shared: &StealthSharedSecret) -> Result<AeadKey> {
    let salt = HashAlgorithm::Blake3.digest(MEMO_KDF_INFO);
    KdfSuite::HkdfSha256
        .derive_aead_key(
            Some(&salt),
            shared.as_key_material(),
            MEMO_KDF_INFO,
            AeadSuite::DEFAULT,
        )
        .map_err(|_| PrivatePaymentError::CryptoUnavailable)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mini_value::{derive_output_with_secret, recover_shared_secret, StealthKeypair};

    fn secrets() -> (StealthSharedSecret, StealthSharedSecret) {
        let recipient = StealthKeypair::generate().unwrap();
        let (output, sender) = derive_output_with_secret(
            &recipient.spend_public_bytes(),
            &recipient.view_public_bytes(),
        )
        .unwrap();
        let received =
            recover_shared_secret(&recipient.view_secret_bytes(), &output.tx_public_key).unwrap();
        (sender, received)
    }

    #[test]
    fn the_recipient_opens_what_the_sender_sealed() {
        let (sender, received) = secrets();
        let digest = [9u8; 32];
        let purpose = PaymentPurpose::new(b"post:abc123".to_vec());
        let memo = SealedMemo::seal(&purpose, &sender, &digest).unwrap();
        assert_eq!(memo.open(&received, &digest).unwrap(), purpose);
    }

    #[test]
    fn a_stranger_cannot_open_it() {
        let (sender, _) = secrets();
        let (_, unrelated) = secrets();
        let digest = [1u8; 32];
        let memo =
            SealedMemo::seal(&PaymentPurpose::new(b"secret".to_vec()), &sender, &digest).unwrap();
        assert!(matches!(
            memo.open(&unrelated, &digest),
            Err(PrivatePaymentError::MemoNotForYou)
        ));
    }

    #[test]
    fn a_memo_cannot_be_moved_to_another_claim() {
        // The AAD binding: without it, an observer could lift "payment for
        // post X" off a large payment and staple it to a tiny one.
        let (sender, received) = secrets();
        let memo = SealedMemo::seal(
            &PaymentPurpose::new(b"post:x".to_vec()),
            &sender,
            &[7u8; 32],
        )
        .unwrap();
        assert!(matches!(
            memo.open(&received, &[8u8; 32]),
            Err(PrivatePaymentError::MemoNotForYou)
        ));
    }

    #[test]
    fn every_memo_is_the_same_size_regardless_of_purpose() {
        let (sender, _) = secrets();
        let digest = [3u8; 32];
        let empty = SealedMemo::empty_for(&sender, &digest).unwrap();
        let short =
            SealedMemo::seal(&PaymentPurpose::new(b"a".to_vec()), &sender, &digest).unwrap();
        let long = SealedMemo::seal(
            &PaymentPurpose::new(vec![0xab; MAX_MEMO_BYTES]),
            &sender,
            &digest,
        )
        .unwrap();
        assert_eq!(empty.ciphertext.len(), short.ciphertext.len());
        assert_eq!(short.ciphertext.len(), long.ciphertext.len());
    }

    #[test]
    fn an_oversized_purpose_is_refused_rather_than_truncated() {
        let (sender, _) = secrets();
        let too_big = PaymentPurpose::new(vec![0u8; MAX_MEMO_BYTES + 1]);
        assert!(matches!(
            SealedMemo::seal(&too_big, &sender, &[0u8; 32]),
            Err(PrivatePaymentError::MemoTooLarge { .. })
        ));
    }

    #[test]
    fn a_tampered_ciphertext_does_not_open() {
        let (sender, received) = secrets();
        let digest = [4u8; 32];
        let mut memo =
            SealedMemo::seal(&PaymentPurpose::new(b"hi".to_vec()), &sender, &digest).unwrap();
        memo.ciphertext[0] ^= 0x01;
        assert!(matches!(
            memo.open(&received, &digest),
            Err(PrivatePaymentError::MemoNotForYou)
        ));
    }

    #[test]
    fn nonzero_padding_is_refused_so_the_tail_is_not_a_covert_channel() {
        let mut block = PaymentPurpose::new(b"ok".to_vec()).encode_padded().unwrap();
        assert!(PaymentPurpose::decode_padded(&block).is_ok());
        *block.last_mut().unwrap() = 0x01;
        assert!(matches!(
            PaymentPurpose::decode_padded(&block),
            Err(PrivatePaymentError::MalformedMemo)
        ));
    }

    #[test]
    fn a_purpose_never_prints_its_contents() {
        let purpose = PaymentPurpose::new(b"who-paid-for-what".to_vec());
        let rendered = format!("{purpose:?}");
        assert!(!rendered.contains("who-paid-for-what"), "{rendered}");
        assert!(rendered.contains("redacted"), "{rendered}");
    }
}
