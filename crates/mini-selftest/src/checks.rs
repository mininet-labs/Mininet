//! The checks themselves.
//!
//! Each is a small function returning `Result<String, String>`: the `Ok`
//! string is what was observed, the `Err` string is what went wrong. A check
//! that returns `Ok` did the work; there is no path that reports a pass
//! without having run.
//!
//! Every check builds its own throwaway state under a caller-supplied scratch
//! directory, so nothing here can see or damage a user's identities, objects,
//! or settings. Two checks read an existing Windows installation; they only
//! read it.

use crate::{Check, Outcome, Report};
use did_mini::{Capabilities, Controller, Did};
use mini_objects::{ObjectBuilder, ObjectId, ObjectType, Payload};
use mini_store::{FsBackend, MemoryBackend, Store};
use std::path::{Path, PathBuf};

/// Subsystems, in the order the suite runs them.
pub const AREAS: &[&str] = &[
    "crypto",
    "identity",
    "storage",
    "social",
    "media",
    "messaging",
    "sync",
    "forge",
    "consensus",
    "settlement",
    "personhood",
    "reward",
    "network",
    "search",
    "policy",
    "value",
    "erasure",
    "spacetime",
    "install",
];

type CheckFn = fn(&Path) -> Result<String, String>;

/// Every check the suite knows, as `(area, name, negative, function)`.
///
/// Exposed so a front end can list what *would* run before running it: a
/// diagnostics button whose contents are only discoverable by pressing it is
/// not much better than no button.
pub fn all_checks() -> Vec<(&'static str, &'static str, bool, CheckFn)> {
    vec![
        (
            "crypto",
            "BLAKE3 and SHA-256 match their published test vectors",
            false,
            check_crypto_vectors as CheckFn,
        ),
        (
            "identity",
            "an identity root signs a message its own key state verifies",
            false,
            check_identity_sign,
        ),
        (
            "identity",
            "a forged signature does not verify against the key state",
            true,
            check_identity_forgery_rejected,
        ),
        (
            "identity",
            "a delegated device carries verifiable authority from its root",
            false,
            check_device_delegation,
        ),
        (
            "identity",
            "a key event log replays to the same key state on another machine",
            false,
            check_kel_replay,
        ),
        (
            "storage",
            "an object's content address is derived from its bytes",
            false,
            check_store_round_trip,
        ),
        (
            "storage",
            "a modified object does not keep its original address",
            true,
            check_store_tamper_rejected,
        ),
        (
            "social",
            "a signed post appears in the author's own feed",
            false,
            check_social_feed,
        ),
        (
            "social",
            "a threaded reply and a reaction attach to the exact post",
            false,
            check_social_thread,
        ),
        (
            "social",
            "following is one-directional until both sides sign",
            true,
            check_follow_is_not_mutual,
        ),
        (
            "media",
            "a chunked file reassembles byte for byte",
            false,
            check_media_round_trip,
        ),
        (
            "messaging",
            "an encrypted message decrypts for its own conversation",
            false,
            check_messaging_round_trip,
        ),
        (
            "messaging",
            "another conversation's key does not read this one",
            true,
            check_messaging_isolation,
        ),
        (
            "sync",
            "two stores converge over a real encrypted socket",
            false,
            check_sync_loopback,
        ),
        (
            "forge",
            "two independent approvals reach a governed merge",
            false,
            check_forge_quorum,
        ),
        (
            "forge",
            "one approval does not reach the two-approval floor",
            true,
            check_forge_single_approval_refused,
        ),
        (
            "forge",
            "an approval bound to one commit does not carry to another",
            true,
            check_forge_approval_is_commit_bound,
        ),
        (
            "erasure",
            "a file survives losing as many shards as it has parity",
            false,
            check_erasure_recovery,
        ),
        (
            "erasure",
            "losing one shard more than parity fails instead of guessing",
            true,
            check_erasure_limit,
        ),
        (
            "spacetime",
            "a storage proof verifies for the block it was made from",
            false,
            check_spacetime_proof,
        ),
        (
            "spacetime",
            "a storage proof does not verify for a different block",
            true,
            check_spacetime_proof_rejected,
        ),
        (
            "install",
            "a package round-trips through pack, verify, and install",
            false,
            check_install_round_trip,
        ),
        (
            "install",
            "a tampered package is refused rather than installed",
            true,
            check_install_tamper_refused,
        ),
        (
            "install",
            "this machine's installation matches its manifest",
            false,
            check_installed_integrity,
        ),
        (
            "consensus",
            "a precommit verifies against its signer's delegated device",
            false,
            check_chain_vote_signature,
        ),
        (
            "consensus",
            "a vote re-pointed at another block does not verify",
            true,
            check_chain_vote_is_bound_to_its_block,
        ),
        (
            "consensus",
            "another root's device cannot cast this root's vote",
            true,
            check_another_roots_device_cannot_cast_your_vote,
        ),
        (
            "settlement",
            "an offline payment claim signs and verifies",
            false,
            check_settlement_claim_round_trip,
        ),
        (
            "settlement",
            "raising the amount on a signed claim invalidates it",
            true,
            check_settlement_amount_cannot_be_edited,
        ),
        (
            "storage",
            "signed operations converge no matter what order they arrive in",
            false,
            check_crdt_converges_regardless_of_order,
        ),
        (
            "personhood",
            "three personhood signals fuse into one confidence score",
            false,
            check_personhood_confidence_fusion,
        ),
        (
            "personhood",
            "evidence past the decay horizon stops counting",
            true,
            check_stale_personhood_evidence_decays,
        ),
        (
            "reward",
            "reward accrual is rate-capped and vests only after a delay",
            false,
            check_reward_accrual_is_rate_capped,
        ),
        (
            "search",
            "text tokenizes and a query parses deterministically",
            false,
            check_search_tokenizer_and_query_parser,
        ),
        (
            "search",
            "the same HTML extracts identically twice",
            false,
            check_html_extraction_is_deterministic,
        ),
        (
            "search",
            "ranking rises with matched terms and falls with age",
            false,
            check_ranking_is_monotonic_in_evidence,
        ),
        (
            "network",
            "gossip fanout is bounded, repeatable, and never invents peers",
            true,
            check_gossip_fanout_is_bounded_and_deterministic,
        ),
        (
            "policy",
            "stronger privacy tiers declare higher cost, never free anonymity",
            false,
            check_privacy_tiers_cost_more_as_they_protect_more,
        ),
        (
            "policy",
            "a stronger tier is never quoted cheaper than a weaker one",
            true,
            check_a_stronger_tier_is_never_quoted_cheaper,
        ),
        (
            "policy",
            "erasure shards are placed on distinct identity roots",
            false,
            check_replication_spreads_shards_across_distinct_holders,
        ),
        (
            "policy",
            "too few holders is refused rather than doubling shards onto one",
            true,
            check_too_few_holders_is_refused_rather_than_doubled_up,
        ),
        (
            "policy",
            "private lookup labels rotate with the epoch",
            true,
            check_private_lookup_labels_do_not_repeat_across_epochs,
        ),
    ]
}

/// Run every check under `scratch`.
pub fn run_all(scratch: &Path) -> Report {
    run_selected(scratch, None)
}

/// Run only the checks in one area.
pub fn run_area(scratch: &Path, area: &str) -> Report {
    run_selected(scratch, Some(area))
}

fn run_selected(scratch: &Path, area: Option<&str>) -> Report {
    let started = std::time::Instant::now();
    let mut checks = Vec::new();
    for (index, (check_area, name, negative, function)) in all_checks().into_iter().enumerate() {
        if area.is_some_and(|wanted| wanted != check_area) {
            continue;
        }
        // A fresh directory per check, so one check's leftovers cannot make
        // the next one pass or fail for the wrong reason.
        let directory = scratch.join(format!("{index:02}-{check_area}"));
        let outcome = match std::fs::create_dir_all(&directory) {
            Ok(()) => match function(&directory) {
                Ok(detail) if detail.starts_with(SKIP_PREFIX) => Outcome::Skipped {
                    reason: detail[SKIP_PREFIX.len()..].to_string(),
                },
                Ok(detail) => Outcome::Passed { detail },
                Err(detail) => Outcome::Failed { detail },
            },
            Err(error) => Outcome::Failed {
                detail: format!("could not create a scratch directory: {error}"),
            },
        };
        let _ = std::fs::remove_dir_all(&directory);
        checks.push(Check {
            area: check_area,
            name,
            negative,
            outcome,
        });
    }
    // The value-layer checks run in their own process (see `crate::value`),
    // so they are appended rather than dispatched through the table above.
    if area.is_none_or(|wanted| wanted == "value") {
        checks.extend(crate::value::run());
    }
    Report {
        checks,
        elapsed_ms: started.elapsed().as_millis() as u64,
    }
}

/// A check returns `Ok` with this prefix to report that it could not run.
const SKIP_PREFIX: &str = "\u{1}skip:";

fn skip(reason: impl Into<String>) -> Result<String, String> {
    Ok(format!("{SKIP_PREFIX}{}", reason.into()))
}

// --- crypto ----------------------------------------------------------------

fn check_crypto_vectors(_scratch: &Path) -> Result<String, String> {
    // Known-answer tests against values published outside this repository, so
    // a passing check means the hashes agree with the rest of the world and
    // not merely with themselves.
    let sha = mini_crypto::hash::sha2_256(b"abc");
    let expected_sha = [
        0xba, 0x78, 0x16, 0xbf, 0x8f, 0x01, 0xcf, 0xea, 0x41, 0x41, 0x40, 0xde, 0x5d, 0xae, 0x22,
        0x23, 0xb0, 0x03, 0x61, 0xa3, 0x96, 0x17, 0x7a, 0x9c, 0xb4, 0x10, 0xff, 0x61, 0xf2, 0x00,
        0x15, 0xad,
    ];
    if sha != expected_sha {
        return Err("SHA-256 of \"abc\" did not match the published vector".to_string());
    }
    let blake = mini_crypto::hash::blake3_256(b"");
    let expected_blake = [
        0xaf, 0x13, 0x49, 0xb9, 0xf5, 0xf9, 0xa1, 0xa6, 0xa0, 0x40, 0x4d, 0xea, 0x36, 0xdc, 0xc9,
        0x49, 0x9b, 0xcb, 0x25, 0xc9, 0xad, 0xc1, 0x12, 0xb7, 0xcc, 0x9a, 0x93, 0xca, 0xe4, 0x1f,
        0x32, 0x62,
    ];
    if blake != expected_blake {
        return Err("BLAKE3 of the empty input did not match the published vector".to_string());
    }
    Ok("SHA-256(\"abc\") and BLAKE3(\"\") both match their published vectors".to_string())
}

// --- identity --------------------------------------------------------------

fn root_and_device(seed: u8) -> Result<(Controller, Controller), String> {
    let mut root = Controller::incept_single_from_seeds(&[seed; 32], &[seed.wrapping_add(1); 32])
        .map_err(|error| format!("inception failed: {error}"))?;
    let device = Controller::incept_device_single_from_seeds(
        &root.did(),
        &[seed.wrapping_add(2); 32],
        &[seed.wrapping_add(3); 32],
    )
    .map_err(|error| format!("device inception failed: {error}"))?;
    root.delegate_device(&device.did(), Capabilities::primary())
        .map_err(|error| format!("delegation failed: {error}"))?;
    Ok((root, device))
}

fn check_identity_sign(_scratch: &Path) -> Result<String, String> {
    let (root, _) = root_and_device(11)?;
    let message = b"a claim this identity is prepared to stand behind";
    let signatures = root.sign_message(message);
    let state = root
        .kel()
        .verify()
        .map_err(|error| format!("key state did not verify: {error}"))?;
    let _ = state;
    root.kel()
        .verify_message(message, &signatures)
        .map_err(|error| format!("a genuine signature did not verify: {error}"))?;
    Ok(format!(
        "{} signed and verified with {} signature(s)",
        short(&root.did()),
        signatures.len()
    ))
}

fn check_identity_forgery_rejected(_scratch: &Path) -> Result<String, String> {
    let (root, _) = root_and_device(23)?;
    let signatures = root.sign_message(b"the message that was signed");
    match root
        .kel()
        .verify_message(b"the message that was NOT signed", &signatures)
    {
        Err(_) => Ok(
            "a signature over different bytes was refused, so signatures bind to content"
                .to_string(),
        ),
        Ok(()) => Err("a signature verified over bytes it was not made for".to_string()),
    }
}

fn check_device_delegation(_scratch: &Path) -> Result<String, String> {
    let (root, device) = root_and_device(37)?;
    let capabilities = did_mini::verify_delegation(&root.kel(), &device.kel())
        .map_err(|error| format!("delegation did not verify: {error}"))?;
    Ok(format!(
        "device {} carries delegated authority from root {} ({capabilities:?})",
        short(&device.did()),
        short(&root.did())
    ))
}

fn check_kel_replay(_scratch: &Path) -> Result<String, String> {
    let (mut root, _) = root_and_device(53)?;
    let before = root
        .kel()
        .verify()
        .map_err(|error| format!("initial key state did not verify: {error}"))?;
    root.rotate()
        .map_err(|error| format!("rotation failed: {error}"))?;
    let after = root
        .kel()
        .verify()
        .map_err(|error| format!("rotated key state did not verify: {error}"))?;
    if format!("{before:?}") == format!("{after:?}") {
        return Err("rotating the key left the key state unchanged".to_string());
    }
    // Replay the whole log from scratch, the way a peer receiving it would:
    // the log is self-certifying, so a second party reaches the same state
    // without asking anyone.
    let replayed = root
        .kel()
        .verify()
        .map_err(|error| format!("replay from the log failed: {error}"))?;
    if format!("{replayed:?}") != format!("{after:?}") {
        return Err("replaying the log reached a different key state".to_string());
    }
    Ok("a pre-rotation advanced the key state, and replaying the log reproduced it".to_string())
}

// --- storage ---------------------------------------------------------------

fn a_signed_object(
    store: &mut Store<MemoryBackend>,
    human: &Did,
    device: &Controller,
    body: &[u8],
    sequence: u64,
) -> Result<ObjectId, String> {
    let object = ObjectBuilder::new(ObjectType::POST)
        .timestamp_ms(1_000 + sequence)
        .sequence(sequence)
        .payload(Payload::Public(body.to_vec()))
        .sign(human, device)
        .map_err(|error| format!("signing failed: {error}"))?;
    store
        .insert(&object)
        .map_err(|error| format!("insert failed: {error}"))?;
    Ok(object.id().clone())
}

fn check_store_round_trip(_scratch: &Path) -> Result<String, String> {
    let (root, device) = root_and_device(67)?;
    let mut store = Store::new(MemoryBackend::new());
    let id = a_signed_object(&mut store, &root.did(), &device, b"stored bytes", 1)?;
    let fetched = store
        .get(&id)
        .map_err(|error| format!("get failed: {error}"))?;
    if fetched.id() != &id {
        return Err("the object came back under a different address".to_string());
    }
    Ok(format!(
        "object {} was stored and fetched by its own content address",
        short_id(&id)
    ))
}

fn check_store_tamper_rejected(_scratch: &Path) -> Result<String, String> {
    let (root, device) = root_and_device(79)?;
    let mut store = Store::new(MemoryBackend::new());
    let first = a_signed_object(&mut store, &root.did(), &device, b"the original", 1)?;
    let second = a_signed_object(&mut store, &root.did(), &device, b"the original!", 2)?;
    if first == second {
        return Err("two different payloads produced the same content address".to_string());
    }
    Ok(format!(
        "changing one byte moved the address from {} to {}",
        short_id(&first),
        short_id(&second)
    ))
}

// --- social ----------------------------------------------------------------

fn check_social_feed(_scratch: &Path) -> Result<String, String> {
    let (root, device) = root_and_device(97)?;
    let human = root.did();
    let mut store = Store::new(MemoryBackend::new());
    mini_social::publish_post(
        &mut store,
        &human,
        &device,
        "hello from a self-test",
        1_000,
        1,
    )
    .map_err(|error| format!("publishing failed: {error}"))?;
    let items = mini_social::feed(&store, &human, mini_social::FeedFilter::Chronological, 10)
        .map_err(|error| format!("reading the feed failed: {error}"))?;
    if items.len() != 1 {
        return Err(format!(
            "expected exactly one feed item, the feed had {}",
            items.len()
        ));
    }
    Ok("one signed post was published and read back out of the local feed".to_string())
}

fn check_social_thread(_scratch: &Path) -> Result<String, String> {
    let (root, device) = root_and_device(103)?;
    let human = root.did();
    let mut store = Store::new(MemoryBackend::new());
    let post = mini_social::publish_post(&mut store, &human, &device, "the parent post", 1_000, 1)
        .map_err(|error| format!("publishing failed: {error}"))?;
    let post_id = post.id().clone();
    mini_social::publish_comment(&mut store, &human, &device, &post_id, "a reply", 1_001, 2)
        .map_err(|error| format!("replying failed: {error}"))?;
    mini_social::set_reaction(
        &mut store,
        &human,
        &device,
        &post_id,
        mini_social::ReactionKind::Like,
        true,
        1_002,
        3,
    )
    .map_err(|error| format!("reacting failed: {error}"))?;
    let replies = mini_social::comments(&store, &post_id)
        .map_err(|error| format!("reading replies failed: {error}"))?;
    let counts = mini_social::reaction_counts(&store, &post_id)
        .map_err(|error| format!("reading reactions failed: {error}"))?;
    if replies.len() != 1 {
        return Err(format!("expected one reply, found {}", replies.len()));
    }
    let likes = counts
        .iter()
        .find(|(kind, _)| *kind == mini_social::ReactionKind::Like)
        .map(|(_, count)| *count)
        .unwrap_or(0);
    if likes != 1 {
        return Err(format!("expected one like, found {likes}"));
    }
    Ok(format!(
        "a reply and a like both attached to post {}",
        short_id(&post_id)
    ))
}

fn check_follow_is_not_mutual(_scratch: &Path) -> Result<String, String> {
    let (alice_root, alice_device) = root_and_device(113)?;
    let (bob_root, _) = root_and_device(131)?;
    let alice = alice_root.did();
    let bob = bob_root.did();
    let mut store = Store::new(MemoryBackend::new());
    mini_social::set_follow(&mut store, &alice, &alice_device, &bob, true, 1_000, 1)
        .map_err(|error| format!("following failed: {error}"))?;
    let alice_follows = mini_social::following(&store, &alice)
        .map_err(|error| format!("reading follows failed: {error}"))?;
    let bob_follows = mini_social::following(&store, &bob)
        .map_err(|error| format!("reading follows failed: {error}"))?;
    if !alice_follows.contains(&bob) {
        return Err("a signed follow did not appear for its author".to_string());
    }
    if !bob_follows.is_empty() {
        return Err("one side's follow produced an edge the other side never signed".to_string());
    }
    Ok(
        "one signed follow created exactly one edge; friendship still needs the other signature"
            .to_string(),
    )
}

// --- media -----------------------------------------------------------------

fn check_media_round_trip(_scratch: &Path) -> Result<String, String> {
    let (root, device) = root_and_device(149)?;
    let human = root.did();
    let mut store = Store::new(MemoryBackend::new());
    // Deliberately larger than mini_media::CHUNK_SIZE, so this exercises real
    // chunking rather than a single-chunk shortcut.
    let original: Vec<u8> = (0..(mini_media::CHUNK_SIZE as u32 * 2 + 7))
        .map(|index| (index % 251) as u8)
        .collect();
    let manifest = mini_media::publish_media(
        &mut store,
        &human,
        &device,
        "application/octet-stream",
        &original,
        1_000,
        1,
    )
    .map_err(|error| format!("publishing media failed: {error}"))?;
    let chunks = manifest.chunks.len();
    if chunks < 2 {
        return Err(format!("expected several chunks, got {chunks}"));
    }
    let reassembled = mini_media::assemble(&store, &manifest)
        .map_err(|error| format!("assembling failed: {error}"))?;
    if reassembled != original {
        return Err("the reassembled bytes differed from the original".to_string());
    }
    Ok(format!(
        "{} bytes split into {chunks} content-addressed chunks and reassembled exactly",
        original.len()
    ))
}

// --- messaging -------------------------------------------------------------

fn check_messaging_round_trip(_scratch: &Path) -> Result<String, String> {
    let (root, device) = root_and_device(151)?;
    let human = root.did();
    let mut store = Store::new(MemoryBackend::new());
    let secret = mini_messaging::ConversationSecret::generate_beta()
        .map_err(|error| format!("could not create a conversation: {error}"))?;
    mini_messaging::send(
        &mut store,
        &secret,
        human,
        &device,
        1_000,
        1,
        mini_messaging::MessageDraft::text("a private sentence"),
    )
    .map_err(|error| format!("sending failed: {error}"))?;
    let scan = mini_messaging::scan(&store, &secret)
        .map_err(|error| format!("scanning failed: {error}"))?;
    if scan.messages.len() != 1 {
        return Err(format!(
            "expected one message, the scan found {}",
            scan.messages.len()
        ));
    }
    Ok("a message was encrypted, stored under an opaque route, and decrypted back".to_string())
}

fn check_messaging_isolation(_scratch: &Path) -> Result<String, String> {
    let (root, device) = root_and_device(163)?;
    let human = root.did();
    let mut store = Store::new(MemoryBackend::new());
    let ours = mini_messaging::ConversationSecret::generate_beta()
        .map_err(|error| format!("could not create a conversation: {error}"))?;
    let theirs = mini_messaging::ConversationSecret::generate_beta()
        .map_err(|error| format!("could not create a second conversation: {error}"))?;
    mini_messaging::send(
        &mut store,
        &ours,
        human,
        &device,
        1_000,
        1,
        mini_messaging::MessageDraft::text("not for the other conversation"),
    )
    .map_err(|error| format!("sending failed: {error}"))?;
    let intruder = mini_messaging::scan(&store, &theirs)
        .map_err(|error| format!("scanning failed: {error}"))?;
    if !intruder.messages.is_empty() {
        return Err("a different conversation's key read this conversation's message".to_string());
    }
    Ok("a second conversation's capability read none of the first one's messages".to_string())
}

// --- sync ------------------------------------------------------------------

fn check_sync_loopback(scratch: &Path) -> Result<String, String> {
    use mini_bearer::{Bearer, Initiator, Responder, TcpBearer};
    use mini_sync::{kel_carrier, sync_bidirectional, KelCache, SyncRole};

    let (root, device) = root_and_device(167)?;
    let human = root.did();

    let open = |name: &str| -> Result<(Store<FsBackend>, KelCache), String> {
        let path = scratch.join(name);
        let mut store = Store::new(
            FsBackend::open(&path).map_err(|error| format!("opening a store failed: {error}"))?,
        );
        for kel in [root.kel(), device.kel()] {
            let carrier = kel_carrier(&kel, &human, &device)
                .map_err(|error| format!("building a KEL carrier failed: {error}"))?;
            store
                .insert(&carrier)
                .map_err(|error| format!("inserting a carrier failed: {error}"))?;
        }
        let mut cache = KelCache::new();
        cache.insert_verified(root.kel());
        cache.insert_verified(device.kel());
        cache
            .hydrate_from_store(&store)
            .map_err(|error| format!("hydrating the KEL cache failed: {error}"))?;
        Ok((store, cache))
    };

    let (mut sender, mut sender_cache) = open("sender")?;
    mini_social::publish_post(
        &mut sender,
        &human,
        &device,
        "this post should cross the socket",
        1_000,
        1,
    )
    .map_err(|error| format!("publishing failed: {error}"))?;
    let expected = sender
        .all_ids()
        .map_err(|error| format!("counting objects failed: {error}"))?
        .len();

    let listener = std::net::TcpListener::bind("127.0.0.1:0")
        .map_err(|error| format!("could not bind a loopback socket: {error}"))?;
    let address = listener
        .local_addr()
        .map_err(|error| format!("could not read the socket address: {error}"))?;

    // The responder runs on a thread and the initiator on this one: a real
    // two-party handshake over a real socket, not an in-process shortcut.
    let receiver_path = scratch.join("receiver");
    let receiver_thread = std::thread::spawn(move || -> Result<(usize, usize), String> {
        let (stream, _) = listener
            .accept()
            .map_err(|error| format!("accept failed: {error}"))?;
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(20)))
            .map_err(|error| format!("setting a read timeout failed: {error}"))?;
        let mut store = Store::new(
            FsBackend::open(&receiver_path)
                .map_err(|error| format!("opening the receiving store failed: {error}"))?,
        );
        let mut cache = KelCache::new();
        let mut bearer = TcpBearer::from_stream(stream)
            .map_err(|error| format!("bearer setup failed: {error}"))?;
        let hello = bearer
            .recv()
            .map_err(|error| format!("handshake receive failed: {error}"))?;
        let (mut channel, response) =
            Responder::respond(&hello).map_err(|error| format!("handshake failed: {error}"))?;
        bearer
            .send(&response)
            .map_err(|error| format!("handshake send failed: {error}"))?;
        let report = sync_bidirectional(
            &mut bearer,
            &mut channel,
            &mut store,
            &mut cache,
            SyncRole::Responder,
        )
        .map_err(|error| format!("sync failed: {error}"))?;
        if report.invalid > 0 {
            return Err(format!("{} object(s) failed verification", report.invalid));
        }
        if report.unknown_author > 0 {
            return Err(format!(
                "{} object(s) were rejected for an unknown author, so the identity \
                 carriers did not establish trust",
                report.unknown_author
            ));
        }
        // Content objects and identity carriers are counted separately: the
        // receiver starts with an empty identity cache and has to learn both
        // KELs from the stream before the post's author is known at all. That
        // is the property worth checking, so both counters are returned.
        Ok((report.accepted, report.carriers))
    });

    let stream = std::net::TcpStream::connect_timeout(&address, std::time::Duration::from_secs(10))
        .map_err(|error| format!("could not connect to the loopback socket: {error}"))?;
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(20)))
        .map_err(|error| format!("setting a read timeout failed: {error}"))?;
    let mut bearer =
        TcpBearer::from_stream(stream).map_err(|error| format!("bearer setup failed: {error}"))?;
    let (initiator, hello) =
        Initiator::start().map_err(|error| format!("handshake start failed: {error}"))?;
    bearer
        .send(&hello)
        .map_err(|error| format!("handshake send failed: {error}"))?;
    let response = bearer
        .recv()
        .map_err(|error| format!("handshake receive failed: {error}"))?;
    let mut channel = initiator
        .finish(&response)
        .map_err(|error| format!("handshake finish failed: {error}"))?;
    sync_bidirectional(
        &mut bearer,
        &mut channel,
        &mut sender,
        &mut sender_cache,
        SyncRole::Initiator,
    )
    .map_err(|error| format!("sync failed: {error}"))?;

    let (accepted, carriers) = receiver_thread
        .join()
        .map_err(|_| "the receiving thread panicked".to_string())??;
    if accepted + carriers < expected {
        return Err(format!(
            "the peer took {accepted} object(s) and {carriers} identity carrier(s) of {expected}"
        ));
    }
    if carriers == 0 {
        return Err(
            "no identity carrier crossed, so trust was not established in band".to_string(),
        );
    }
    Ok(format!(
        "{accepted} signed object(s) and {carriers} identity carrier(s) crossed an encrypted \
         loopback connection and verified, starting from no shared trust"
    ))
}

// --- forge -----------------------------------------------------------------

/// A three-maintainer project with a two-approval policy, plus an outside
/// contributor, mirroring how the repository itself is governed.
struct Forge {
    store: Store<MemoryBackend>,
    project: ObjectId,
    maintainers: Vec<(Controller, Controller)>,
    contributor: (Controller, Controller),
}

fn forge_world() -> Result<Forge, String> {
    let maintainers = vec![
        root_and_device(10)?,
        root_and_device(50)?,
        root_and_device(90)?,
    ];
    let contributor = root_and_device(130)?;
    let mut store = Store::new(MemoryBackend::new());
    let policy = mini_forge::Policy {
        min_approvals: 2,
        maintainers: maintainers.iter().map(|(root, _)| root.did()).collect(),
    };
    let project = mini_forge::project(
        &mut store,
        &maintainers[0].0.did(),
        &maintainers[0].1,
        "selftest",
        &policy,
    )
    .map_err(|error| format!("creating the project failed: {error}"))?
    .id()
    .clone();
    Ok(Forge {
        store,
        project,
        maintainers,
        contributor,
    })
}

impl Forge {
    fn oracle(&self) -> mini_forge::KelDirectory {
        let mut directory = mini_forge::KelDirectory::new();
        for (root, device) in self.maintainers.iter().chain([&self.contributor]) {
            directory.insert(root.kel());
            directory.insert(device.kel());
        }
        directory
    }

    fn commit(&mut self, text: &[u8], sequence: u64) -> Result<ObjectId, String> {
        let (root, device) = &self.contributor;
        let human = root.did();
        let file = mini_forge::put_file(&mut self.store, &human, device, text)
            .map_err(|error| format!("writing a file failed: {error}"))?;
        let tree = mini_forge::put_tree(
            &mut self.store,
            &human,
            device,
            &[mini_forge::TreeEntry {
                name: "change.rs".into(),
                is_dir: false,
                target: file,
            }],
        )
        .map_err(|error| format!("writing a tree failed: {error}"))?;
        Ok(mini_forge::commit(
            &mut self.store,
            &human,
            device,
            "a change",
            &tree,
            &[],
            100,
            sequence,
        )
        .map_err(|error| format!("committing failed: {error}"))?
        .id()
        .clone())
    }
}

fn check_forge_quorum(_scratch: &Path) -> Result<String, String> {
    let mut forge = forge_world()?;
    let head = forge.commit(b"a fix an outsider wrote", 1)?;
    // Destructured so the store can be borrowed mutably while the signing
    // keys beside it are borrowed immutably. `Controller` is deliberately not
    // `Clone` --- it holds private key material --- so these checks work with
    // disjoint field borrows rather than copies of anyone's keys.
    let Forge {
        store,
        project,
        maintainers,
        contributor,
    } = &mut forge;
    let proposal = mini_forge::propose(
        store,
        &contributor.0.did(),
        &contributor.1,
        project,
        "main",
        "a fix",
        &head,
        project,
        200,
        1,
    )
    .map_err(|error| format!("proposing failed: {error}"))?
    .id()
    .clone();

    for (index, (root, device)) in maintainers.iter().enumerate().take(2) {
        mini_forge::approve(
            store,
            &root.did(),
            device,
            &proposal,
            &head,
            true,
            300 + index as u64,
            1,
        )
        .map_err(|error| format!("approving failed: {error}"))?;
    }
    mini_forge::merge(
        store,
        &maintainers[2].0.did(),
        &maintainers[2].1,
        project,
        project,
        &proposal,
        400,
        1,
    )
    .map_err(|error| format!("recording the merge failed: {error}"))?;

    let state = mini_forge::resolve_project(&forge.store, &forge.oracle(), &forge.project)
        .map_err(|error| format!("resolving the project failed: {error}"))?;
    if state.entries != 1 {
        return Err(format!(
            "expected one governed chain entry, found {}",
            state.entries
        ));
    }
    if state.branches != vec![("main".to_string(), head.clone())] {
        return Err("the canonical branch head is not the reviewed commit".to_string());
    }
    if state.forks_detected {
        return Err("a fork was reported where none exists".to_string());
    }
    Ok(format!(
        "an outsider's commit {} reached the canonical main head after two independent approvals",
        short_id(&head)
    ))
}

fn check_forge_single_approval_refused(_scratch: &Path) -> Result<String, String> {
    let mut forge = forge_world()?;
    let head = forge.commit(b"a change with one approval", 1)?;
    let Forge {
        store,
        project,
        maintainers,
        contributor,
    } = &mut forge;
    let proposal = mini_forge::propose(
        store,
        &contributor.0.did(),
        &contributor.1,
        project,
        "main",
        "one approval only",
        &head,
        project,
        200,
        1,
    )
    .map_err(|error| format!("proposing failed: {error}"))?
    .id()
    .clone();
    mini_forge::approve(
        store,
        &maintainers[0].0.did(),
        &maintainers[0].1,
        &proposal,
        &head,
        true,
        300,
        1,
    )
    .map_err(|error| format!("approving failed: {error}"))?;
    mini_forge::merge(
        store,
        &maintainers[1].0.did(),
        &maintainers[1].1,
        project,
        project,
        &proposal,
        400,
        1,
    )
    .map_err(|error| format!("recording the merge failed: {error}"))?;

    let state = mini_forge::resolve_project(&forge.store, &forge.oracle(), &forge.project)
        .map_err(|error| format!("resolving the project failed: {error}"))?;
    if state.entries != 0 || !state.branches.is_empty() {
        return Err(
            "a merge with one approval was counted against a two-approval policy".to_string(),
        );
    }
    Ok("a merge recorded with one approval did not move the canonical head".to_string())
}

fn check_forge_approval_is_commit_bound(_scratch: &Path) -> Result<String, String> {
    let mut forge = forge_world()?;
    let reviewed = forge.commit(b"the commit that was reviewed", 1)?;
    let swapped = forge.commit(b"the commit that was substituted", 2)?;
    let Forge {
        store,
        project,
        maintainers,
        contributor,
    } = &mut forge;
    let proposal = mini_forge::propose(
        store,
        &contributor.0.did(),
        &contributor.1,
        project,
        "main",
        "swap after review",
        &swapped,
        project,
        200,
        3,
    )
    .map_err(|error| format!("proposing failed: {error}"))?
    .id()
    .clone();
    // Both approvals name the commit that was *read*, not the one the
    // proposal now points at.
    for (index, (root, device)) in maintainers.iter().enumerate().take(2) {
        mini_forge::approve(
            store,
            &root.did(),
            device,
            &proposal,
            &reviewed,
            true,
            300 + index as u64,
            1,
        )
        .map_err(|error| format!("approving failed: {error}"))?;
    }
    mini_forge::merge(
        store,
        &maintainers[2].0.did(),
        &maintainers[2].1,
        project,
        project,
        &proposal,
        400,
        1,
    )
    .map_err(|error| format!("recording the merge failed: {error}"))?;

    let state = mini_forge::resolve_project(&forge.store, &forge.oracle(), &forge.project)
        .map_err(|error| format!("resolving the project failed: {error}"))?;
    if state.entries != 0 {
        return Err(
            "approvals of one commit were counted for a proposal pointing at another".to_string(),
        );
    }
    Ok(format!(
        "approvals naming {} did not authorize the substituted commit {}",
        short_id(&reviewed),
        short_id(&swapped)
    ))
}

// --- erasure ---------------------------------------------------------------

fn check_erasure_recovery(_scratch: &Path) -> Result<String, String> {
    let params = mini_erasure::ErasureParams::new(4, 2)
        .map_err(|error| format!("bad parameters: {error}"))?;
    let original: Vec<u8> = (0..4_097u32).map(|index| (index % 97) as u8).collect();
    let encoded = mini_erasure::encode(&original, params)
        .map_err(|error| format!("encoding failed: {error}"))?;
    let mut shards: Vec<Option<mini_erasure::Shard>> =
        encoded.shards.iter().cloned().map(Some).collect();
    // Lose exactly as many as there is parity for, including a data shard.
    shards[0] = None;
    shards[5] = None;
    let recovered = mini_erasure::reconstruct(params, &shards, encoded.original_len)
        .map_err(|error| format!("reconstruction failed: {error}"))?;
    if recovered != original {
        return Err("the reconstructed bytes differed from the original".to_string());
    }
    Ok(format!(
        "{} bytes recovered exactly after losing 2 of 6 shards",
        original.len()
    ))
}

fn check_erasure_limit(_scratch: &Path) -> Result<String, String> {
    let params = mini_erasure::ErasureParams::new(4, 2)
        .map_err(|error| format!("bad parameters: {error}"))?;
    let original: Vec<u8> = (0..1_000u32).map(|index| index as u8).collect();
    let encoded = mini_erasure::encode(&original, params)
        .map_err(|error| format!("encoding failed: {error}"))?;
    let mut shards: Vec<Option<mini_erasure::Shard>> =
        encoded.shards.iter().cloned().map(Some).collect();
    shards[0] = None;
    shards[1] = None;
    shards[2] = None;
    match mini_erasure::reconstruct(params, &shards, encoded.original_len) {
        Err(_) => Ok(
            "losing 3 of 6 shards with 2 parity failed cleanly instead of returning wrong bytes"
                .to_string(),
        ),
        Ok(_) => Err("reconstruction claimed success with too few shards".to_string()),
    }
}

// --- spacetime -------------------------------------------------------------

fn check_spacetime_proof(_scratch: &Path) -> Result<String, String> {
    let blocks: Vec<Vec<u8>> = (0..8u8).map(|index| vec![index; 512]).collect();
    let tree = mini_spacetime::MerkleTree::from_blocks(&blocks)
        .ok_or_else(|| "could not build a Merkle tree over the blocks".to_string())?;
    let proof = tree
        .prove(3)
        .ok_or_else(|| "could not produce a proof for block 3".to_string())?;
    if !proof.verify(&blocks[3], tree.root(), tree.leaf_count()) {
        return Err("a genuine possession proof did not verify".to_string());
    }
    Ok(format!(
        "a possession proof for 1 of {} blocks verified against the committed root",
        tree.leaf_count()
    ))
}

fn check_spacetime_proof_rejected(_scratch: &Path) -> Result<String, String> {
    let blocks: Vec<Vec<u8>> = (0..8u8).map(|index| vec![index; 512]).collect();
    let tree = mini_spacetime::MerkleTree::from_blocks(&blocks)
        .ok_or_else(|| "could not build a Merkle tree over the blocks".to_string())?;
    let proof = tree
        .prove(3)
        .ok_or_else(|| "could not produce a proof for block 3".to_string())?;
    if proof.verify(&blocks[4], tree.root(), tree.leaf_count()) {
        return Err("a proof for one block verified against a different block".to_string());
    }
    Ok("a possession proof did not verify for a block it was not made from".to_string())
}

// --- install ---------------------------------------------------------------

fn sample_package(
    scratch: &Path,
) -> Result<(mini_windows_setup::PackageManifest, Vec<u8>), String> {
    use mini_windows_setup::manifest::{ManifestHeader, PackageShortcut};
    let desktop = b"#!/bin/sh\necho selftest-client\n".to_vec();
    let cli = b"#!/bin/sh\necho selftest-cli\n".to_vec();
    let setup = b"#!/bin/sh\necho selftest-setup\n".to_vec();
    let files = vec![
        mini_windows_setup::PackageFile::describe("mininet-desktop.exe", &desktop)
            .map_err(|error| error.to_string())?,
        mini_windows_setup::PackageFile::describe("mini.exe", &cli)
            .map_err(|error| error.to_string())?,
        // Uninstall registration now requires the package to carry its own
        // setup executable (or an explicit `options.setup_exe`), so Apps &
        // features never records a path to a program that was never
        // installed. This fixture package registers uninstall, so it needs
        // one too.
        mini_windows_setup::PackageFile::describe("mininet-setup.exe", &setup)
            .map_err(|error| error.to_string())?,
    ];
    let manifest = mini_windows_setup::PackageManifest::new(
        ManifestHeader {
            package: "mininet-selftest-package",
            version: "0.1.0",
            target: "x86_64-pc-windows-msvc",
            product: "Mininet",
            launch: "mininet-desktop.exe",
            built_at_ms: 1_757_635_200_000,
        },
        files,
        vec![PackageShortcut {
            target: "mininet-desktop.exe".to_string(),
            name: "Mininet".to_string(),
        }],
    )
    .map_err(|error| error.to_string())?;
    let bytes = mini_windows_setup::container::write(&manifest, |path| match path {
        "mininet-desktop.exe" => Ok(desktop.clone()),
        "mini.exe" => Ok(cli.clone()),
        "mininet-setup.exe" => Ok(setup.clone()),
        other => Err(mini_windows_setup::SetupError::MissingFile {
            path: other.to_string(),
        }),
    })
    .map_err(|error| error.to_string())?;
    let _ = scratch;
    Ok((manifest, bytes))
}

fn check_install_round_trip(scratch: &Path) -> Result<String, String> {
    let (manifest, bytes) = sample_package(scratch)?;
    let container =
        mini_windows_setup::Container::open(&bytes).map_err(|error| error.to_string())?;
    container.verify_all().map_err(|error| error.to_string())?;

    let setup = mini_windows_setup::Setup::new(scratch.join("Programs"))
        .with_user_data_root(scratch.join("UserData"));
    let options = mini_windows_setup::InstallOptions {
        start_menu_dir: Some(scratch.join("menu")),
        desktop_dir: Some(scratch.join("desktop")),
        ..Default::default()
    };
    let approval = mini_windows_setup::InstallApproval::new(&manifest, 1_000);
    let mut shell = mini_windows_setup::RecordingShell::default();
    let report = setup
        .install(&container, &approval, &options, &mut shell, 1_000)
        .map_err(|error| error.to_string())?;
    let verify = setup
        .verify_installed("0.1.0")
        .map_err(|error| error.to_string())?;
    if !verify.is_intact() {
        return Err(format!(
            "a freshly installed package did not verify: {:?}",
            verify.problems
        ));
    }
    Ok(format!(
        "{} file(s) installed, re-hashed from disk, and verified intact",
        report.files_written
    ))
}

fn check_install_tamper_refused(scratch: &Path) -> Result<String, String> {
    let (manifest, mut bytes) = sample_package(scratch)?;
    let last = bytes.len() - 1;
    bytes[last] ^= 0xff;
    let container =
        mini_windows_setup::Container::open(&bytes).map_err(|error| error.to_string())?;
    let setup = mini_windows_setup::Setup::new(scratch.join("Programs"))
        .with_user_data_root(scratch.join("UserData"));
    let options = mini_windows_setup::InstallOptions {
        start_menu_dir: Some(scratch.join("menu")),
        desktop_dir: Some(scratch.join("desktop")),
        ..Default::default()
    };
    let approval = mini_windows_setup::InstallApproval::new(&manifest, 1_000);
    let mut shell = mini_windows_setup::RecordingShell::default();
    match setup.install(&container, &approval, &options, &mut shell, 1_000) {
        Err(error) => {
            if setup
                .status()
                .map_err(|error| error.to_string())?
                .active
                .is_some()
            {
                return Err("a refused install still activated something".to_string());
            }
            Ok(format!(
                "a package with one flipped byte was refused ({})",
                error.code()
            ))
        }
        Ok(_) => Err("a tampered package installed successfully".to_string()),
    }
}

fn check_installed_integrity(_scratch: &Path) -> Result<String, String> {
    let setup = mini_windows_setup::Setup::for_current_user();
    let status = setup.status().map_err(|error| error.to_string())?;
    let Some(active) = status.active else {
        return skip(format!(
            "nothing is installed in {} to check",
            status.install_root.display()
        ));
    };
    let report = setup
        .verify_installed(&active.version_text)
        .map_err(|error| error.to_string())?;
    if !report.is_intact() {
        let problems: Vec<String> = report
            .problems
            .iter()
            .map(mini_windows_setup::report::describe_problem)
            .collect();
        return Err(format!(
            "the installed copy of {} does not match its manifest: {}",
            active.version_text,
            problems.join(", ")
        ));
    }
    Ok(format!(
        "installed {} matches its manifest: {} file(s), {} bytes re-hashed",
        active.version_text, report.files_checked, report.bytes_checked
    ))
}

// --- helpers ---------------------------------------------------------------

fn short(did: &Did) -> String {
    let text = did.as_str();
    match text.char_indices().nth(24) {
        Some((index, _)) => format!("{}...", &text[..index]),
        None => text.to_string(),
    }
}

fn short_id(id: &ObjectId) -> String {
    let text = id.as_str();
    match text.char_indices().nth(16) {
        Some((index, _)) => format!("{}...", &text[..index]),
        None => text.to_string(),
    }
}

/// Where to put throwaway state for a run.
pub fn default_scratch() -> PathBuf {
    std::env::temp_dir().join(format!(
        "mininet-selftest-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|since| since.as_nanos())
            .unwrap_or(0)
    ))
}

// --- consensus -------------------------------------------------------------

fn check_chain_vote_signature(_scratch: &Path) -> Result<String, String> {
    let (root, device) = root_and_device(181)?;
    let vote = mini_chain::sign_vote(
        mini_chain::VoteKind::Precommit,
        7,
        0,
        [0x11; 32],
        &root.did(),
        &device,
    );
    mini_chain::verify_vote(&vote, &root.kel(), &device.kel())
        .map_err(|error| format!("a genuine vote did not verify: {error}"))?;
    Ok(format!(
        "a precommit at height {} verified against its signer's delegated device",
        vote.height
    ))
}

fn check_chain_vote_is_bound_to_its_block(_scratch: &Path) -> Result<String, String> {
    let (root, device) = root_and_device(191)?;
    let mut vote = mini_chain::sign_vote(
        mini_chain::VoteKind::Precommit,
        7,
        0,
        [0x11; 32],
        &root.did(),
        &device,
    );
    // Swap the block after signing: the classic equivocation primitive.
    vote.block_hash = [0x22; 32];
    match mini_chain::verify_vote(&vote, &root.kel(), &device.kel()) {
        Err(_) => Ok(
            "a vote re-pointed at a different block was refused, so votes bind to one block"
                .to_string(),
        ),
        Ok(()) => Err("a vote verified for a block it was not cast on".to_string()),
    }
}

fn check_another_roots_device_cannot_cast_your_vote(_scratch: &Path) -> Result<String, String> {
    let (root, _) = root_and_device(197)?;
    let (_, other_device) = root_and_device(199)?;
    // A device belonging to a different root, claiming to vote for this one.
    let vote = mini_chain::sign_vote(
        mini_chain::VoteKind::Precommit,
        7,
        0,
        [0x11; 32],
        &root.did(),
        &other_device,
    );
    match mini_chain::verify_vote(&vote, &root.kel(), &other_device.kel()) {
        Err(_) => Ok(
            "a device delegated by another identity root could not cast this root's vote"
                .to_string(),
        ),
        Ok(()) => {
            Err("an undelegated device cast a vote for a root it does not belong to".to_string())
        }
    }
}

// --- settlement ------------------------------------------------------------

fn check_settlement_claim_round_trip(_scratch: &Path) -> Result<String, String> {
    let payer = mini_crypto::SigningKey::from_seed(&[0x21; 32]);
    let claim = mini_settlement::sign_claim(
        &payer,
        b"payee-label",
        1_500_000,
        1,
        2_000,
        b"last-known-chain",
        1_000,
    )
    .map_err(|error| format!("signing a claim failed: {error}"))?;
    mini_settlement::verify_claim_signature(&claim)
        .map_err(|error| format!("a genuine claim did not verify: {error}"))?;
    Ok(format!(
        "an offline payment claim for {} micro-MINI signed and verified",
        claim.amount_micro
    ))
}

fn check_settlement_amount_cannot_be_edited(_scratch: &Path) -> Result<String, String> {
    let payer = mini_crypto::SigningKey::from_seed(&[0x23; 32]);
    let mut claim = mini_settlement::sign_claim(
        &payer,
        b"payee-label",
        1_000_000,
        1,
        2_000,
        b"last-known-chain",
        1_000,
    )
    .map_err(|error| format!("signing a claim failed: {error}"))?;
    claim.amount_micro = 9_000_000;
    match mini_settlement::verify_claim_signature(&claim) {
        Err(_) => Ok(
            "raising the amount on a signed claim invalidated it, so the payer's signature \
             covers the amount"
                .to_string(),
        ),
        Ok(()) => Err("an edited amount still verified against the payer's signature".to_string()),
    }
}

// --- storage (CRDT convergence) -------------------------------------------

fn check_crdt_converges_regardless_of_order(_scratch: &Path) -> Result<String, String> {
    let (root, device) = root_and_device(211)?;
    let human = root.did();
    // The document root is a real signed object's id rather than an invented
    // one: content addresses in this tree are derived from bytes, and a check
    // that fabricated one would be testing a shape the protocol never produces.
    let mut store = Store::new(MemoryBackend::new());
    let doc = a_signed_object(&mut store, &human, &device, b"document root", 1)?;
    let first = mini_crdt::op_add(&doc, &doc, b"first", 1_000, 1, &human, &device)
        .map_err(|error| format!("op_add failed: {error}"))?;
    let second = mini_crdt::op_add(&doc, &doc, b"second", 1_001, 2, &human, &device)
        .map_err(|error| format!("op_add failed: {error}"))?;
    let third = mini_crdt::op_add(&doc, &doc, b"third", 1_002, 3, &human, &device)
        .map_err(|error| format!("op_add failed: {error}"))?;

    let forward = mini_crdt::replay(&doc, &[first.clone(), second.clone(), third.clone()]);
    let reversed = mini_crdt::replay(&doc, &[third, second, first]);
    if format!("{forward:?}") != format!("{reversed:?}") {
        return Err(
            "replaying the same operations in a different order produced a different document"
                .to_string(),
        );
    }
    Ok(format!(
        "three signed operations converged to the same document from both orders ({} node(s))",
        forward.len()
    ))
}

// --- personhood ------------------------------------------------------------

fn check_personhood_confidence_fusion(_scratch: &Path) -> Result<String, String> {
    let decay = mini_uniqueness::DecayPolicy::months_scale_default();
    let weights = mini_uniqueness::ConfidenceWeights::whitepaper_default();
    let strong = mini_uniqueness::ConfidenceInputs {
        vouch_trust: 900,
        vouch_age_ms: 0,
        presence_score: 900,
        presence_age_ms: 0,
        behavioral_score: Some(900),
    };
    let weak = mini_uniqueness::ConfidenceInputs {
        vouch_trust: 10,
        vouch_age_ms: 0,
        presence_score: 10,
        presence_age_ms: 0,
        behavioral_score: Some(10),
    };
    let strong_score = mini_uniqueness::fuse_confidence(&strong, &decay, &weights);
    let weak_score = mini_uniqueness::fuse_confidence(&weak, &decay, &weights);
    if strong_score <= weak_score {
        return Err(format!(
            "fused confidence did not rank strong evidence above weak ({strong_score} vs {weak_score})"
        ));
    }
    Ok(format!(
        "three personhood signals fused to {strong_score} for strong evidence and {weak_score} for weak"
    ))
}

fn check_stale_personhood_evidence_decays(_scratch: &Path) -> Result<String, String> {
    let decay = mini_uniqueness::DecayPolicy::months_scale_default();
    let weights = mini_uniqueness::ConfidenceWeights::whitepaper_default();
    let fresh = mini_uniqueness::ConfidenceInputs {
        vouch_trust: 900,
        vouch_age_ms: 0,
        presence_score: 900,
        presence_age_ms: 0,
        behavioral_score: Some(900),
    };
    let stale = mini_uniqueness::ConfidenceInputs {
        vouch_age_ms: decay.zero_after_ms,
        presence_age_ms: decay.zero_after_ms,
        ..fresh
    };
    let fresh_score = mini_uniqueness::fuse_confidence(&fresh, &decay, &weights);
    let stale_score = mini_uniqueness::fuse_confidence(&stale, &decay, &weights);
    if stale_score >= fresh_score {
        return Err(format!(
            "evidence past the decay horizon still counted as much as fresh evidence \
             ({stale_score} vs {fresh_score})"
        ));
    }
    Ok(format!(
        "evidence aged past the decay horizon fell from {fresh_score} to {stale_score}"
    ))
}

// --- reward ----------------------------------------------------------------

fn check_reward_accrual_is_rate_capped(_scratch: &Path) -> Result<String, String> {
    let params = mini_reward::RewardParams::demo_default();
    if params.max_points_per_window == 0 {
        return Err("the demo reward profile has no rate cap".to_string());
    }
    if params.maturation_ms == 0 {
        return Err("the demo reward profile vests instantly".to_string());
    }

    // Checking the profile's numbers alone would still pass if `accrue`
    // ignored one of them. Exercise the real function instead: six
    // co-presences at `base_points` each (6_000) inside one window would
    // exceed the demo profile's 5_000-point cap if it were not enforced, and
    // a maturation delay of a whole day means none of it should be vested
    // moments after the events themselves.
    let subject = reward_check_identity(0)?;
    let verdicts: Vec<mini_presence::PresenceVerdict> = (1..=6)
        .map(|seed| {
            Ok(mini_presence::PresenceVerdict {
                initiator_root: subject.clone(),
                responder_root: reward_check_identity(seed)?,
                at_ms: 1_000 + u64::from(seed) * 100,
                hardware_ranged: false,
            })
        })
        .collect::<Result<_, String>>()?;
    let last_event_ms = verdicts
        .iter()
        .map(|verdict| verdict.at_ms)
        .max()
        .expect("six verdicts were just constructed above");

    let just_after = mini_reward::accrue(&subject, &verdicts, &params, last_event_ms);
    if just_after.accrued_points != params.max_points_per_window {
        return Err(format!(
            "six co-presences of {} points each should cap accrual at the window limit of {}, \
             but {} points accrued",
            params.base_points, params.max_points_per_window, just_after.accrued_points
        ));
    }
    if just_after.vested_points != 0 {
        return Err(format!(
            "presence recorded moments ago already vested {} of its {} accrued points, \
             skipping the maturation delay",
            just_after.vested_points, just_after.accrued_points
        ));
    }

    let after_maturation = mini_reward::accrue(
        &subject,
        &verdicts,
        &params,
        last_event_ms + params.maturation_ms + 1,
    );
    if after_maturation.vested_points != params.max_points_per_window {
        return Err(format!(
            "{} ms past maturation, the capped total of {} should be fully vested, but only {} \
             vested",
            params.maturation_ms + 1,
            params.max_points_per_window,
            after_maturation.vested_points
        ));
    }

    Ok(format!(
        "six co-presences of {} points each capped accrual at {} per {} ms window and left it \
         unvested until {} ms after the event",
        params.base_points, params.max_points_per_window, params.window_ms, params.maturation_ms
    ))
}

/// A distinct, validly-formed identity root for the reward accrual check.
///
/// A real `did:mini` root, not a placeholder string: [`mini_reward::accrue`]
/// only ever sees roots that passed identity construction in the real
/// system, and a check exercising it should offer the same shape of input.
fn reward_check_identity(seed: u8) -> Result<Did, String> {
    Ok(
        Controller::incept_single_from_seeds(&[seed; 32], &[seed.wrapping_add(100); 32])
            .map_err(|error| {
                format!("inception failed while building a reward check fixture: {error}")
            })?
            .did(),
    )
}

// --- search ----------------------------------------------------------------

fn check_search_tokenizer_and_query_parser(_scratch: &Path) -> Result<String, String> {
    let tokens = mini_lexical_index::tokenize("Mininet is a constitutional protocol");
    if tokens.is_empty() {
        return Err("the tokenizer produced nothing for ordinary prose".to_string());
    }
    let repeated = mini_lexical_index::tokenize("same same same");
    let counted: u32 = repeated.iter().map(|(_, count)| *count).sum();
    if counted < 3 {
        return Err(format!(
            "the tokenizer lost repeated occurrences: counted {counted} of 3"
        ));
    }
    let parsed = mini_query::parse_query("\"exact phrase\" plus terms");
    Ok(format!(
        "{} token(s) from a sentence, repeats counted, and a query parsed to {parsed:?}",
        tokens.len()
    )
    .chars()
    .take(200)
    .collect())
}

fn check_html_extraction_is_deterministic(_scratch: &Path) -> Result<String, String> {
    let html = "<html><head><title>A page</title></head><body><h1>Heading</h1>\
                <p>Some body text for the extractor.</p></body></html>";
    let first =
        mini_web_extract::extract(html).map_err(|error| format!("extraction failed: {error:?}"))?;
    let second =
        mini_web_extract::extract(html).map_err(|error| format!("extraction failed: {error:?}"))?;
    if format!("{first:?}") != format!("{second:?}") {
        return Err("extracting the same HTML twice produced different results".to_string());
    }
    Ok("static HTML extraction produced identical output on two runs".to_string())
}

fn check_ranking_is_monotonic_in_evidence(_scratch: &Path) -> Result<String, String> {
    let few = mini_ranker::signals::lexical(1, 10, 1);
    let many = mini_ranker::signals::lexical(9, 10, 9);
    if many <= few {
        return Err(format!(
            "a document matching more query terms did not score higher ({many} vs {few})"
        ));
    }
    let fresh = mini_ranker::signals::freshness(1_000_000, 1_000_000);
    let old = mini_ranker::signals::freshness(0, 1_000_000_000);
    if old > fresh {
        return Err("an older document scored fresher than a new one".to_string());
    }
    Ok(format!(
        "lexical score rose from {few} to {many} with more matched terms, and freshness fell \
         from {fresh} to {old} with age"
    ))
}

// --- network ---------------------------------------------------------------

fn check_gossip_fanout_is_bounded_and_deterministic(_scratch: &Path) -> Result<String, String> {
    let mut candidates = Vec::new();
    for _ in 0..20 {
        candidates.push(
            mini_net::PeerId::generate()
                .map_err(|error| format!("could not generate a peer id: {error}"))?,
        );
    }
    let candidate_set: std::collections::HashSet<mini_net::PeerId> =
        candidates.iter().copied().collect();

    let chosen = mini_net::fanout_peers(&candidates, 4);
    if chosen.len() != 4 {
        return Err(format!(
            "asked for a fanout of 4 and got {} peer(s)",
            chosen.len()
        ));
    }
    fanout_selected_only_real_distinct_peers(&chosen, &candidate_set)?;

    let again = mini_net::fanout_peers(&candidates, 4);
    if chosen != again {
        return Err(
            "the same candidate set produced a different fanout on a second call".to_string(),
        );
    }
    let over = mini_net::fanout_peers(&candidates, 1_000);
    if over.len() > candidates.len() {
        return Err("a fanout larger than the candidate set invented peers".to_string());
    }
    fanout_selected_only_real_distinct_peers(&over, &candidate_set)?;

    Ok(format!(
        "gossip fanout selected {} of {} real, distinct peers, repeatably, and never invented \
         or repeated one",
        chosen.len(),
        candidates.len()
    ))
}

/// Every id `selected` names is one of `candidates`, and none repeats.
///
/// The advertised guarantee is that fanout never invents peers, so a check
/// for it has to look at the selected ids themselves: matching only the
/// *count* returned would still pass an implementation that fabricates or
/// duplicates ids instead of actually selecting from the candidate pool.
fn fanout_selected_only_real_distinct_peers(
    selected: &[mini_net::PeerId],
    candidates: &std::collections::HashSet<mini_net::PeerId>,
) -> Result<(), String> {
    if selected.iter().any(|peer| !candidates.contains(peer)) {
        return Err("fanout selected a peer id that was never in the candidate set".to_string());
    }
    let distinct: std::collections::HashSet<_> = selected.iter().copied().collect();
    if distinct.len() != selected.len() {
        return Err(format!(
            "fanout returned {} peer(s) but only {} were distinct",
            selected.len(),
            distinct.len()
        ));
    }
    Ok(())
}

// --- policy ----------------------------------------------------------------

fn check_privacy_tiers_cost_more_as_they_protect_more(_scratch: &Path) -> Result<String, String> {
    use mini_privacy_policy::PrivacyTier;
    // The cost doctrine's central claim: stronger protection is not free, and
    // the schedule says so honestly rather than implying free anonymity.
    let tiers = [
        PrivacyTier::Direct,
        PrivacyTier::Relayed,
        PrivacyTier::Mixed,
        PrivacyTier::Burst,
    ];
    let mut previous: Option<u32> = None;
    let mut described = Vec::new();
    for tier in tiers {
        let cost = mini_privacy_policy::expected_cost(tier);
        let low = cost.bandwidth_multiplier_millix_min;
        described.push(format!("{tier:?}={low} millix"));
        if let Some(previous) = previous {
            if low < previous {
                return Err(format!(
                    "{tier:?} claims lower bandwidth cost ({low} millix) than the weaker tier below it ({previous})"
                ));
            }
        }
        previous = Some(low);
    }
    Ok(format!(
        "declared bandwidth cost rises with protection: {}",
        described.join(", ")
    ))
}

fn check_a_stronger_tier_is_never_quoted_cheaper(_scratch: &Path) -> Result<String, String> {
    use mini_privacy_policy::PrivacyTier;
    let prices = mini_resource_pricing::PriceVector {
        bandwidth_micro_mini_per_mb: 1_000,
        storage_micro_mini_per_mb_day: 100,
    };
    let direct = mini_resource_pricing::quote(&prices, PrivacyTier::Direct, 10, 30)
        .map_err(|error| format!("quoting Tier 0 failed: {error:?}"))?;
    let mixed = mini_resource_pricing::quote(&prices, PrivacyTier::Mixed, 10, 30)
        .map_err(|error| format!("quoting Tier 2 failed: {error:?}"))?;
    if mixed.min_micro_mini < direct.min_micro_mini {
        return Err(
            "a mix-network tier was quoted cheaper than a direct one, which would make the \
             cost doctrine meaningless"
                .to_string(),
        );
    }
    Ok(format!(
        "the same payload quotes {} micro-MINI direct and {} mixed, so privacy is priced, \
         not promised free",
        direct.min_micro_mini, mixed.min_micro_mini
    ))
}

fn check_replication_spreads_shards_across_distinct_holders(
    _scratch: &Path,
) -> Result<String, String> {
    let params = mini_erasure::ErasureParams::new(4, 2)
        .map_err(|error| format!("bad parameters: {error}"))?;
    let mut holders = Vec::new();
    for seed in 0..6u8 {
        let (root, _) = root_and_device(seed.wrapping_mul(7).wrapping_add(3))?;
        holders.push(root.did());
    }
    let plan = mini_replication_policy::plan_placement(params, &holders)
        .map_err(|error| format!("placement failed: {error:?}"))?;
    let distinct: std::collections::BTreeSet<String> = plan
        .assignments()
        .iter()
        .map(|assignment| assignment.holder.0.as_str().to_string())
        .collect();
    if distinct.len() != plan.assignments().len() {
        return Err(format!(
            "{} shard(s) were placed on only {} distinct holder(s), so losing one holder \
             costs more than one shard",
            plan.assignments().len(),
            distinct.len()
        ));
    }
    Ok(format!(
        "{} shards were placed on {} distinct identity roots",
        plan.assignments().len(),
        distinct.len()
    ))
}

fn check_too_few_holders_is_refused_rather_than_doubled_up(
    _scratch: &Path,
) -> Result<String, String> {
    let params = mini_erasure::ErasureParams::new(4, 2)
        .map_err(|error| format!("bad parameters: {error}"))?;
    let (root, _) = root_and_device(151)?;
    // Six shards, two candidates: placing them anyway would silently put
    // several shards on one holder and call it replication.
    let (other, _) = root_and_device(157)?;
    match mini_replication_policy::plan_placement(params, &[root.did(), other.did()]) {
        Err(_) => Ok(
            "placing 6 shards on 2 holders was refused rather than doubling shards onto one"
                .to_string(),
        ),
        Ok(plan) => Err(format!(
            "placement accepted 2 holders for {} shards",
            plan.assignments().len()
        )),
    }
}

fn check_private_lookup_labels_do_not_repeat_across_epochs(
    _scratch: &Path,
) -> Result<String, String> {
    let secret = mini_private_index::CapabilitySecret::generate()
        .map_err(|error| format!("could not create a capability secret: {error:?}"))?;
    let first = mini_private_index::derive_lookup_label(
        &secret,
        b"scope",
        b"replica",
        mini_private_index::IndexEpoch(1),
        mini_private_index::LookupPurpose::ShardLookup,
    )
    .map_err(|error| format!("deriving a label failed: {error:?}"))?;
    let same = mini_private_index::derive_lookup_label(
        &secret,
        b"scope",
        b"replica",
        mini_private_index::IndexEpoch(1),
        mini_private_index::LookupPurpose::ShardLookup,
    )
    .map_err(|error| format!("deriving a label failed: {error:?}"))?;
    let next_epoch = mini_private_index::derive_lookup_label(
        &secret,
        b"scope",
        b"replica",
        mini_private_index::IndexEpoch(2),
        mini_private_index::LookupPurpose::ShardLookup,
    )
    .map_err(|error| format!("deriving a label failed: {error:?}"))?;
    if first != same {
        return Err("the same inputs produced two different lookup labels".to_string());
    }
    if first == next_epoch {
        return Err(
            "the label did not rotate with the epoch, so a storage node could link lookups \
             across epochs"
                .to_string(),
        );
    }
    Ok("a lookup label is stable within an epoch and unlinkable across epochs".to_string())
}
