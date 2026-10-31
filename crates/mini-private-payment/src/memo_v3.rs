//! `PrivatePaymentV3`'s sealed memo (Gate #72 audit, Section 10.1/10.2):
//! [`crate::memo`]'s same purpose — carry what a payment was for, and the
//! commitment opening that makes it spendable, to the recipient alone —
//! over the audit's exact plaintext layout and key-derivation context
//! instead of [`crate::memo::MEMO_KDF_INFO`]'s single fixed info string.
//!
//! # What changed from `crate::memo`, and why
//!
//! [`crate::memo::SealedMemo`] derives its AEAD key from a domain-
//! separated salt alone (`MEMO_KDF_INFO`) — the same key material for
//! every claim on every network. Section 10.2 binds the key derivation
//! to the specific network (via the salt) and to the specific claim,
//! output, and public output fields it seals (via the info string:
//! `memo_context_digest || output_index || R || P || amount_commitment`),
//! and additionally authenticates that same context as AEAD associated
//! data. A memo sealed this way cannot be replayed as though it belonged
//! to a different network, claim, or output even in combination with a
//! key-derivation implementation bug that `crate::memo`'s narrower
//! context would not by itself catch.
//!
//! The 256-byte plaintext layout adds an explicit version byte ahead of
//! [`crate::memo::PaymentNote`]'s fields (amount, blinding, bounded
//! reference, zero padding) so a future memo-format revision has
//! somewhere to signal itself without a new AEAD suite.
//!
//! [FREEZE reminder — D-0036/D-0037/D-0047] Founder-overridden,
//! AI-authored prototype. Unaudited.

use mini_crypto::{AeadKey, AeadNonce, AeadSuite, HashAlgorithm, KdfSuite};
use mini_value::StealthSharedSecret;

use crate::codec::{Reader, Writer};
use crate::error::{PrivatePaymentError, Result};
use crate::memo::{PaymentNote, PaymentPurpose};

/// Every V3 memo plaintext is padded to exactly this length before
/// sealing (Section 10.1) — matches [`crate::memo::MEMO_PADDED_BYTES`]
/// numerically, kept as its own constant so the two formats can diverge
/// later without a silent coupling.
pub const MEMO_V3_PADDED_BYTES: usize = 256;

/// Ciphertext length: plaintext plus the AEAD tag (Section 10.1: "272
/// bytes").
pub const MEMO_V3_SEALED_BYTES: usize = MEMO_V3_PADDED_BYTES + 16;

/// The only note version this module currently encodes/accepts.
pub const NOTE_VERSION_V3: u8 = 1;

/// Bytes the sealed note spends on everything but the reference: a
/// 1-byte version, a 4-byte length prefix, the 8-byte amount, and the
/// 32-byte blinding factor.
const NOTE_OVERHEAD_V3_BYTES: usize = 1 + 4 + 8 + 32;

/// Longest purpose payload a caller may seal, leaving room for the fixed
/// overhead inside the padded block.
pub const MAX_MEMO_V3_BYTES: usize = MEMO_V3_PADDED_BYTES - NOTE_OVERHEAD_V3_BYTES;

const MEMO_SALT_DOMAIN: &[u8] = b"mininet/private-payment/memo-salt/v3";
const MEMO_KEY_DOMAIN: &[u8] = b"mininet/private-payment/memo-key/v3";

fn encode_padded_v3(note: &PaymentNote) -> Result<Vec<u8>> {
    if note.purpose.reference.len() > MAX_MEMO_V3_BYTES {
        return Err(PrivatePaymentError::MemoTooLarge {
            got: note.purpose.reference.len(),
            max: MAX_MEMO_V3_BYTES,
        });
    }
    let mut w = Writer::new();
    w.u8(NOTE_VERSION_V3);
    w.u64(note.amount_micro);
    w.raw(&note.blinding);
    w.bytes(&note.purpose.reference);
    let mut block = w.finish();
    block.resize(MEMO_V3_PADDED_BYTES, 0);
    Ok(block)
}

fn decode_padded_v3(block: &[u8]) -> Result<PaymentNote> {
    if block.len() != MEMO_V3_PADDED_BYTES {
        return Err(PrivatePaymentError::MalformedMemo);
    }
    let mut r = Reader::new(block);
    let version = r.u8().map_err(|_| PrivatePaymentError::MalformedMemo)?;
    if version != NOTE_VERSION_V3 {
        return Err(PrivatePaymentError::MalformedMemo);
    }
    let amount_micro = r.u64().map_err(|_| PrivatePaymentError::MalformedMemo)?;
    let blinding = r
        .array::<32>()
        .map_err(|_| PrivatePaymentError::MalformedMemo)?;
    let reference = r.bytes().map_err(|_| PrivatePaymentError::MalformedMemo)?;
    let consumed = 1 + 8 + 32 + 4 + reference.len();
    if block[consumed..].iter().any(|byte| *byte != 0) {
        return Err(PrivatePaymentError::MalformedMemo);
    }
    Ok(PaymentNote::new(
        PaymentPurpose { reference },
        amount_micro,
        blinding,
    ))
}

/// Everything a V3 memo's AEAD is bound to: the network (via the key
/// derivation's salt) and, via both the derivation's info string and the
/// AEAD's associated data, the exact claim/output context this memo may
/// never be moved away from.
#[derive(Debug, Clone, Copy)]
pub struct MemoContextV3<'a> {
    pub network_id: &'a [u8; 32],
    pub memo_context_digest: &'a [u8; 32],
    pub output_index: u32,
    pub tx_public_key: &'a [u8],
    pub one_time_address: &'a [u8],
    pub amount_commitment: &'a [u8],
}

impl MemoContextV3<'_> {
    fn info(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.raw(MEMO_KEY_DOMAIN);
        w.raw(self.memo_context_digest);
        w.u32(self.output_index);
        w.bytes(self.tx_public_key);
        w.bytes(self.one_time_address);
        w.bytes(self.amount_commitment);
        w.finish()
    }

    fn aad(&self) -> Vec<u8> {
        // The exact same public output context, as the associated data
        // the AEAD tag itself authenticates -- belt and suspenders with
        // the key derivation's own info string binding the same fields.
        self.info()
    }

    fn key(&self, shared: &StealthSharedSecret) -> Result<AeadKey> {
        let salt =
            HashAlgorithm::Blake3.digest(&[MEMO_SALT_DOMAIN, self.network_id.as_slice()].concat());
        KdfSuite::HkdfSha256
            .derive_aead_key(
                Some(&salt),
                shared.as_key_material(),
                &self.info(),
                AeadSuite::DEFAULT,
            )
            .map_err(|_| PrivatePaymentError::CryptoUnavailable)
    }
}

/// A [`PaymentNote`] sealed under Section 10's V3 construction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SealedMemoV3 {
    pub ciphertext: Vec<u8>,
}

impl SealedMemoV3 {
    /// Seal `note` under `context`, to the recipient who can recover
    /// `shared`.
    pub fn seal(
        note: &PaymentNote,
        shared: &StealthSharedSecret,
        context: &MemoContextV3<'_>,
    ) -> Result<Self> {
        let block = encode_padded_v3(note)?;
        let key = context.key(shared)?;
        // A fixed all-zero nonce is correct here for the same reason
        // `crate::memo::SealedMemo::seal` uses one: the key is derived
        // from a shared secret that is fresh per payment (the stealth
        // `r` is drawn per call, per `mini_value::derive_output_v3`), so
        // this key encrypts exactly one message in its entire lifetime.
        let nonce = AeadNonce::from_bytes(&[0u8; 12])
            .map_err(|_| PrivatePaymentError::CryptoUnavailable)?;
        let ciphertext = key
            .encrypt(&nonce, &block, &context.aad())
            .map_err(|_| PrivatePaymentError::CryptoUnavailable)?;
        Ok(Self { ciphertext })
    }

    /// Open this memo with a recovered shared secret and the same
    /// context it was sealed under.
    pub fn open(
        &self,
        shared: &StealthSharedSecret,
        context: &MemoContextV3<'_>,
    ) -> Result<PaymentNote> {
        let key = context.key(shared)?;
        let nonce = AeadNonce::from_bytes(&[0u8; 12])
            .map_err(|_| PrivatePaymentError::CryptoUnavailable)?;
        let block = key
            .decrypt(&nonce, &self.ciphertext, &context.aad())
            .map_err(|_| PrivatePaymentError::MemoNotForYou)?;
        decode_padded_v3(&block)
    }

    pub(crate) fn write_into(&self, w: &mut Writer) {
        w.raw(&self.ciphertext);
    }

    pub(crate) fn read_from(r: &mut Reader<'_>) -> Result<Self> {
        let ciphertext = r.array::<MEMO_V3_SEALED_BYTES>()?.to_vec();
        Ok(Self { ciphertext })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mini_value::{derive_output_v3, recover_and_verify_v3, StealthKeypair};

    const SUITE_ID: u16 = 0x0003;
    const NETWORK: [u8; 32] = [4u8; 32];

    fn secrets_and_output() -> (StealthSharedSecret, StealthSharedSecret, Vec<u8>, Vec<u8>) {
        let recipient = StealthKeypair::generate().unwrap();
        let (output, sender) = derive_output_v3(
            SUITE_ID,
            &NETWORK,
            &recipient.spend_public_bytes(),
            &recipient.view_public_bytes(),
        )
        .unwrap();
        let received = recover_and_verify_v3(
            SUITE_ID,
            &NETWORK,
            &recipient.view_secret_bytes(),
            &recipient.spend_public_bytes(),
            &output,
        )
        .unwrap();
        (
            sender,
            received,
            output.tx_public_key,
            output.one_time_address,
        )
    }

    fn context<'a>(
        network_id: &'a [u8; 32],
        digest: &'a [u8; 32],
        r: &'a [u8],
        p: &'a [u8],
        commitment: &'a [u8],
    ) -> MemoContextV3<'a> {
        MemoContextV3 {
            network_id,
            memo_context_digest: digest,
            output_index: 0,
            tx_public_key: r,
            one_time_address: p,
            amount_commitment: commitment,
        }
    }

    #[test]
    fn the_recipient_opens_what_the_sender_sealed() {
        let (sender, received, r, p) = secrets_and_output();
        let digest = [1u8; 32];
        let commitment = vec![9u8; 32];
        let ctx = context(&NETWORK, &digest, &r, &p, &commitment);
        let note = PaymentNote::new(PaymentPurpose::new(b"invoice-1".to_vec()), 1_000, [7u8; 32]);
        let memo = SealedMemoV3::seal(&note, &sender, &ctx).unwrap();
        assert_eq!(memo.open(&received, &ctx).unwrap(), note);
    }

    #[test]
    fn a_stranger_cannot_open_it() {
        let (sender, _, r, p) = secrets_and_output();
        let (_, unrelated, _, _) = secrets_and_output();
        let digest = [2u8; 32];
        let commitment = vec![1u8; 32];
        let ctx = context(&NETWORK, &digest, &r, &p, &commitment);
        let memo = SealedMemoV3::seal(
            &PaymentNote::new(PaymentPurpose::new(b"secret".to_vec()), 1, [0u8; 32]),
            &sender,
            &ctx,
        )
        .unwrap();
        assert!(matches!(
            memo.open(&unrelated, &ctx),
            Err(PrivatePaymentError::MemoNotForYou)
        ));
    }

    #[test]
    fn moving_a_memo_to_a_different_output_index_fails_to_open() {
        let (sender, received, r, p) = secrets_and_output();
        let digest = [3u8; 32];
        let commitment = vec![2u8; 32];
        let mut ctx = context(&NETWORK, &digest, &r, &p, &commitment);
        let memo = SealedMemoV3::seal(
            &PaymentNote::new(PaymentPurpose::new(b"x".to_vec()), 5, [1u8; 32]),
            &sender,
            &ctx,
        )
        .unwrap();
        ctx.output_index = 1;
        assert!(matches!(
            memo.open(&received, &ctx),
            Err(PrivatePaymentError::MemoNotForYou)
        ));
    }

    #[test]
    fn a_different_network_salt_fails_to_open() {
        let (sender, received, r, p) = secrets_and_output();
        let digest = [5u8; 32];
        let commitment = vec![3u8; 32];
        let other_network = [8u8; 32];
        let seal_ctx = context(&NETWORK, &digest, &r, &p, &commitment);
        let open_ctx = context(&other_network, &digest, &r, &p, &commitment);
        let memo = SealedMemoV3::seal(
            &PaymentNote::new(PaymentPurpose::new(b"y".to_vec()), 1, [0u8; 32]),
            &sender,
            &seal_ctx,
        )
        .unwrap();
        assert!(matches!(
            memo.open(&received, &open_ctx),
            Err(PrivatePaymentError::MemoNotForYou)
        ));
    }

    #[test]
    fn every_memo_is_the_same_size_regardless_of_purpose() {
        let (sender, _, r, p) = secrets_and_output();
        let digest = [6u8; 32];
        let commitment = vec![4u8; 32];
        let ctx = context(&NETWORK, &digest, &r, &p, &commitment);
        let empty = SealedMemoV3::seal(
            &PaymentNote::new(PaymentPurpose::none(), 0, [0u8; 32]),
            &sender,
            &ctx,
        )
        .unwrap();
        let long = SealedMemoV3::seal(
            &PaymentNote::new(
                PaymentPurpose::new(vec![0xab; MAX_MEMO_V3_BYTES]),
                1,
                [0u8; 32],
            ),
            &sender,
            &ctx,
        )
        .unwrap();
        assert_eq!(empty.ciphertext.len(), MEMO_V3_SEALED_BYTES);
        assert_eq!(long.ciphertext.len(), MEMO_V3_SEALED_BYTES);
    }

    #[test]
    fn an_oversized_purpose_is_refused_rather_than_truncated() {
        let (sender, _, r, p) = secrets_and_output();
        let digest = [7u8; 32];
        let commitment = vec![5u8; 32];
        let ctx = context(&NETWORK, &digest, &r, &p, &commitment);
        let too_big = PaymentPurpose::new(vec![0u8; MAX_MEMO_V3_BYTES + 1]);
        assert!(matches!(
            SealedMemoV3::seal(&PaymentNote::new(too_big, 1, [0u8; 32]), &sender, &ctx),
            Err(PrivatePaymentError::MemoTooLarge { .. })
        ));
    }

    #[test]
    fn a_tampered_ciphertext_does_not_open() {
        let (sender, received, r, p) = secrets_and_output();
        let digest = [8u8; 32];
        let commitment = vec![6u8; 32];
        let ctx = context(&NETWORK, &digest, &r, &p, &commitment);
        let mut memo = SealedMemoV3::seal(
            &PaymentNote::new(PaymentPurpose::new(b"hi".to_vec()), 1, [0u8; 32]),
            &sender,
            &ctx,
        )
        .unwrap();
        memo.ciphertext[0] ^= 0x01;
        assert!(matches!(
            memo.open(&received, &ctx),
            Err(PrivatePaymentError::MemoNotForYou)
        ));
    }

    #[test]
    fn wire_round_trip_preserves_the_ciphertext() {
        let (sender, _, r, p) = secrets_and_output();
        let digest = [9u8; 32];
        let commitment = vec![7u8; 32];
        let ctx = context(&NETWORK, &digest, &r, &p, &commitment);
        let memo = SealedMemoV3::seal(
            &PaymentNote::new(PaymentPurpose::new(b"z".to_vec()), 1, [0u8; 32]),
            &sender,
            &ctx,
        )
        .unwrap();
        let mut w = Writer::new();
        memo.write_into(&mut w);
        let bytes = w.finish();
        let mut r_reader = Reader::new(&bytes);
        let decoded = SealedMemoV3::read_from(&mut r_reader).unwrap();
        assert_eq!(memo, decoded);
    }
}
