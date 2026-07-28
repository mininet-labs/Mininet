//! Real challenge-response round-trip ranging over an already-bound
//! encrypted channel (D-0368; `docs/BETA_STATUS.md` item 2, "active range
//! measurement").
//!
//! ## What this replaces
//!
//! Before this module, [`crate::AttestationFields::rtt_samples_ms`] was
//! just a `Vec<u32>` a caller assembled by hand -- nothing in this crate
//! produced those numbers, so whichever side built the fields could put any
//! values it liked in that field, and there was no protocol event a
//! verifier could point to that the numbers actually came from. Verification
//! only checked the *signature* over the transcript, never that the claimed
//! timings happened. This module gives a caller a real measurement to put
//! there instead of an arbitrary one.
//!
//! ## The protocol
//!
//! Three steps, one round trip:
//!
//! 1. [`send_range_challenge`] (measuring side): seal and send a fresh
//!    random 32-byte challenge over `chan`, and start a wall-clock timer.
//! 2. [`respond_to_range_challenge`] (responding side): receive one
//!    challenge and echo it straight back, with no processing delay beyond
//!    the AEAD open/seal itself.
//! 3. [`recv_range_response`] (measuring side): block until the echo
//!    arrives, verify it matches the challenge that was sent, and return the
//!    elapsed wall-clock time.
//!
//! Splitting the round trip into three steps (rather than one function that
//! internally sends and blocks on the reply) is deliberate: a real two-phone
//! deployment runs the measuring and responding sides as two independent
//! processes with no coordination needed beyond the three steps above, but
//! a single-process caller driving both ends of an in-process bearer pair
//! (this crate's own tests, and `mini-keystone`'s demo) has only one thread
//! and must interleave the steps itself -- exactly like
//! `mini-keystone::run_demo` already does for its channel handshake and KEL
//! exchange, which is why this mirrors that shape instead of introducing
//! threads.
//!
//! Only one side's clock is ever trusted: the measuring side times its own
//! send-to-receive interval around a message that necessarily crossed the
//! physical medium twice, so no clock coordination between the two devices
//! is needed for the bound to mean something (the same principle classical
//! distance-bounding protocols use).
//!
//! ## Honest limits
//!
//! This is round-trip application-layer timing over whatever transport
//! `Bearer` sits on, not a formal cryptographic distance-bounding protocol
//! (no pre-committed challenge/response bits exchanged before the round
//! begins) and not hardware ranging (see [`crate::ranging`]). It does not
//! defeat a wormhole/relay attack that forwards bytes between two real
//! devices with near-zero added latency -- no software-only RTT bound can.
//! What it closes is the specific gap `docs/BETA_STATUS.md` item 2 named: a
//! claimed proximity number backed by a real, independently-timed protocol
//! exchange, not an arbitrary self-reported value nobody else could check.

use std::time::Instant;

use mini_bearer::{Bearer, Channel};
use mini_crypto::random_32;

use crate::error::{PresenceError, Result};

/// AAD labels for ranging messages, distinct from KEL/application messages
/// sealed over the same channel.
const AAD_RANGE_CHALLENGE: &[u8] = b"MINI/PRESENCE range-challenge";
const AAD_RANGE_RESPONSE: &[u8] = b"MINI/PRESENCE range-response";

/// The state carried from [`send_range_challenge`] to [`recv_range_response`]
/// across one round trip: the exact challenge sent, and when.
#[derive(Debug)]
pub struct PendingRangeChallenge {
    challenge: [u8; 32],
    started: Instant,
}

/// Step 1 (measuring side): seal and send a fresh random challenge over
/// `chan`, and start timing. Pass the returned [`PendingRangeChallenge`] to
/// [`recv_range_response`] once the peer's echo has been requested via
/// [`respond_to_range_challenge`].
pub fn send_range_challenge(
    bearer: &mut dyn Bearer,
    chan: &mut Channel,
) -> Result<PendingRangeChallenge> {
    let challenge = random_32().map_err(PresenceError::Crypto)?;
    let ct = chan
        .seal(&challenge, AAD_RANGE_CHALLENGE)
        .map_err(PresenceError::Bearer)?;
    let started = Instant::now();
    bearer.send(&ct).map_err(PresenceError::Bearer)?;
    Ok(PendingRangeChallenge { challenge, started })
}

/// Step 2 (responding side): receive one challenge and echo it straight
/// back, with no processing delay beyond the AEAD open/seal itself, so the
/// round trip the measuring side times is real.
pub fn respond_to_range_challenge(bearer: &mut dyn Bearer, chan: &mut Channel) -> Result<()> {
    let ct = bearer.recv().map_err(PresenceError::Bearer)?;
    let challenge = chan
        .open(&ct, AAD_RANGE_CHALLENGE)
        .map_err(PresenceError::Bearer)?;
    let resp_ct = chan
        .seal(&challenge, AAD_RANGE_RESPONSE)
        .map_err(PresenceError::Bearer)?;
    bearer.send(&resp_ct).map_err(PresenceError::Bearer)?;
    Ok(())
}

/// Step 3 (measuring side): block until the peer's echo arrives, verify it
/// matches the challenge [`send_range_challenge`] sent, and return the
/// measured wall-clock round-trip time in milliseconds.
pub fn recv_range_response(
    bearer: &mut dyn Bearer,
    chan: &mut Channel,
    pending: PendingRangeChallenge,
) -> Result<u32> {
    let resp_ct = bearer.recv().map_err(PresenceError::Bearer)?;
    let elapsed = pending.started.elapsed();
    let echoed = chan
        .open(&resp_ct, AAD_RANGE_RESPONSE)
        .map_err(PresenceError::Bearer)?;
    if echoed != pending.challenge {
        return Err(PresenceError::RangingEchoMismatch);
    }
    Ok(u32::try_from(elapsed.as_millis()).unwrap_or(u32::MAX))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mini_bearer::{pair, Initiator, Responder};

    fn connected_channels() -> (
        mini_bearer::InProcessBearer,
        Channel,
        mini_bearer::InProcessBearer,
        Channel,
    ) {
        let (mut bearer_a, mut bearer_b) = pair();
        let (initiator, hello1) = Initiator::start().unwrap();
        bearer_a.send(&hello1).unwrap();
        let got1 = bearer_b.recv().unwrap();
        let (chan_b, hello2) = Responder::respond(&got1).unwrap();
        bearer_b.send(&hello2).unwrap();
        let got2 = bearer_a.recv().unwrap();
        let chan_a = initiator.finish(&got2).unwrap();
        (bearer_a, chan_a, bearer_b, chan_b)
    }

    #[test]
    fn one_round_trip_measures_a_real_elapsed_time() {
        let (mut bearer_a, mut chan_a, mut bearer_b, mut chan_b) = connected_channels();

        let pending = send_range_challenge(&mut bearer_a, &mut chan_a).unwrap();
        respond_to_range_challenge(&mut bearer_b, &mut chan_b).unwrap();
        let ms = recv_range_response(&mut bearer_a, &mut chan_a, pending).unwrap();

        // In-process, near-instant, but genuinely measured -- not a fixed
        // literal the caller supplied.
        assert!(ms < 1_000);
    }

    #[test]
    fn several_round_trips_can_be_run_in_sequence() {
        let (mut bearer_a, mut chan_a, mut bearer_b, mut chan_b) = connected_channels();

        let mut samples = Vec::new();
        for _ in 0..4 {
            let pending = send_range_challenge(&mut bearer_a, &mut chan_a).unwrap();
            respond_to_range_challenge(&mut bearer_b, &mut chan_b).unwrap();
            samples.push(recv_range_response(&mut bearer_a, &mut chan_a, pending).unwrap());
        }
        assert_eq!(samples.len(), 4);
    }

    #[test]
    fn a_response_over_the_wrong_channel_is_rejected() {
        // A response encrypted under a *different* channel's keys cannot be
        // opened as this session's response -- simulates an attacker (or a
        // bug) trying to splice in an answer from elsewhere.
        let (mut bearer_a, mut chan_a, mut bearer_b, mut chan_b) = connected_channels();
        let (_bearer_c, mut chan_c, _bearer_d, _chan_d) = connected_channels();

        let pending = send_range_challenge(&mut bearer_a, &mut chan_a).unwrap();
        // Drain the real challenge B received, but reply using an unrelated
        // channel's keys instead of echoing it back honestly.
        let ct = bearer_b.recv().unwrap();
        let challenge = chan_b.open(&ct, AAD_RANGE_CHALLENGE).unwrap();
        let forged = chan_c.seal(&challenge, AAD_RANGE_RESPONSE).unwrap();
        bearer_b.send(&forged).unwrap();

        let err = recv_range_response(&mut bearer_a, &mut chan_a, pending).unwrap_err();
        assert!(matches!(err, PresenceError::Bearer(_)));
    }

    #[test]
    fn an_echo_of_the_wrong_challenge_is_rejected() {
        // B receives the real challenge but (maliciously or buggily) echoes
        // a different value back under valid channel keys.
        let (mut bearer_a, mut chan_a, mut bearer_b, mut chan_b) = connected_channels();

        let pending = send_range_challenge(&mut bearer_a, &mut chan_a).unwrap();
        let ct = bearer_b.recv().unwrap();
        let _real_challenge = chan_b.open(&ct, AAD_RANGE_CHALLENGE).unwrap();
        let wrong = [0xAAu8; 32];
        let resp_ct = chan_b.seal(&wrong, AAD_RANGE_RESPONSE).unwrap();
        bearer_b.send(&resp_ct).unwrap();

        let err = recv_range_response(&mut bearer_a, &mut chan_a, pending).unwrap_err();
        assert_eq!(err, PresenceError::RangingEchoMismatch);
    }
}
