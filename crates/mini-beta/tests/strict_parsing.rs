//! Issue #336 acceptance: malformed, oversized and unknown-link Beta objects
//! received through sync fail closed in every strict parser. Each case starts
//! from a valid object and changes exactly one thing, then re-signs it so the
//! signature is not what rejects it.

use did_mini::Controller;
use mini_beta::{
    create_campaign, create_contribution_receipt, create_finding, create_finding_disposition,
    parse_campaign_object, parse_contribution_object, parse_finding_disposition_object,
    parse_finding_object, BetaEpochId, BetaError, ClaimKey, ContributionKind, EvidenceClass,
    FindingSeverity, FindingState, SubmissionTag,
};
use mini_objects::{Object, ObjectBuilder, ObjectType, Payload};
use mini_store::{MemoryBackend, Store};

fn signer() -> Controller {
    Controller::incept_single_from_seeds(&[10; 32], &[11; 32]).unwrap()
}

struct Fixture {
    signer: Controller,
    campaign: Object,
    finding: Object,
    disposition: Object,
    contribution: Object,
}

fn fixture() -> Fixture {
    let signer = signer();
    let mut store = Store::new(MemoryBackend::new());
    let target = ObjectBuilder::new(ObjectType::Custom("mini/beta-target".to_string()))
        .payload(Payload::Public(b"exact beta build".to_vec()))
        .sign(&signer.did(), &signer)
        .unwrap();
    store.insert(&target).unwrap();
    let campaign = create_campaign(
        &mut store,
        &signer.did(),
        &signer,
        target.id(),
        BetaEpochId::new([7; 32]).unwrap(),
        "strictness",
        "exercise the strict parsers",
        &["parsers".to_string()],
        100,
        1_000,
        500,
        100,
        1,
    )
    .unwrap();
    let finding = create_finding(
        &mut store,
        &signer.did(),
        &signer,
        campaign.id(),
        SubmissionTag::new([8; 32]).unwrap(),
        EvidenceClass::PhysicalDevice,
        FindingSeverity::High,
        "ble",
        "relay does not recover",
        "two phones",
        "disconnect and reconnect",
        "message relays",
        "message queued",
        &["sha256:deadbeef".to_string()],
        "single vendor",
        true,
        200,
        2,
    )
    .unwrap();
    let disposition = create_finding_disposition(
        &mut store,
        &signer.did(),
        &signer,
        finding.id(),
        None,
        Some(target.id()),
        FindingState::Fixed,
        "fixed and retested",
        300,
        3,
    )
    .unwrap();
    let contribution = create_contribution_receipt(
        &mut store,
        &signer.did(),
        &signer,
        finding.id(),
        Some(campaign.id()),
        ClaimKey::from_secret(&[5; 32]).claim_tag().unwrap(),
        ContributionKind::Testing,
        "accepted work",
        &["evidence:accepted".to_string()],
        400,
        4,
    )
    .unwrap();
    Fixture {
        signer,
        campaign,
        finding,
        disposition,
        contribution,
    }
}

/// Rebuild `object` with its payload changed and/or extra links, re-signed.
fn rebuilt(
    f: &Fixture,
    object: &Object,
    payload: Payload,
    extra_links: &[(&str, &mini_objects::ObjectId)],
    drop_links: bool,
) -> Object {
    let mut builder = ObjectBuilder::new(object.object_type.clone())
        .timestamp_ms(object.timestamp_ms)
        .sequence(object.sequence)
        .payload(payload);
    if !drop_links {
        for link in &object.links {
            builder = builder.link(&link.rel, link.target.clone());
        }
    }
    for (rel, target) in extra_links {
        builder = builder.link(rel, (*target).clone());
    }
    builder.sign(&f.signer.did(), &f.signer).unwrap()
}

fn bytes_of(object: &Object) -> Vec<u8> {
    match &object.payload {
        Payload::Public(b) => b.clone(),
        Payload::Encrypted(_) => panic!("beta objects are public"),
    }
}

type Parser = fn(&Object) -> Result<(), BetaError>;

fn parsers(f: &Fixture) -> Vec<(&'static str, &Object, Parser)> {
    vec![
        ("campaign", &f.campaign, |o| {
            parse_campaign_object(o).map(|_| ())
        }),
        ("finding", &f.finding, |o| {
            parse_finding_object(o).map(|_| ())
        }),
        ("disposition", &f.disposition, |o| {
            parse_finding_disposition_object(o).map(|_| ())
        }),
        ("contribution", &f.contribution, |o| {
            parse_contribution_object(o).map(|_| ())
        }),
    ]
}

#[test]
fn the_valid_objects_parse_so_every_failure_below_is_caused_by_its_mutation() {
    let f = fixture();
    for (name, object, parse) in parsers(&f) {
        assert!(parse(object).is_ok(), "{name} fixture must parse");
    }
}

#[test]
fn truncated_and_padded_payloads_fail_closed() {
    let f = fixture();
    for (name, object, parse) in parsers(&f) {
        let bytes = bytes_of(object);
        let truncated = rebuilt(
            &f,
            object,
            Payload::Public(bytes[..bytes.len() - 1].to_vec()),
            &[],
            false,
        );
        assert!(
            parse(&truncated).is_err(),
            "{name}: truncated payload parsed"
        );
        let mut padded = bytes.clone();
        padded.push(0);
        let padded = rebuilt(&f, object, Payload::Public(padded), &[], false);
        assert!(parse(&padded).is_err(), "{name}: padded payload parsed");
        let empty = rebuilt(&f, object, Payload::Public(Vec::new()), &[], false);
        assert!(parse(&empty).is_err(), "{name}: empty payload parsed");
    }
}

#[test]
fn an_unknown_payload_version_fails_closed() {
    let f = fixture();
    for (name, object, parse) in parsers(&f) {
        let mut bytes = bytes_of(object);
        bytes[0] ^= 0x7f;
        let bad = rebuilt(&f, object, Payload::Public(bytes), &[], false);
        assert!(parse(&bad).is_err(), "{name}: unknown version parsed");
    }
}

#[test]
fn unknown_duplicate_and_missing_links_fail_closed() {
    let f = fixture();
    let anchor = f.campaign.id().clone();
    for (name, object, parse) in parsers(&f) {
        let bytes = Payload::Public(bytes_of(object));
        let unknown = rebuilt(&f, object, bytes.clone(), &[("bogus", &anchor)], false);
        assert!(parse(&unknown).is_err(), "{name}: unknown link parsed");
        let first = object.links[0].rel.clone();
        let duplicate = rebuilt(
            &f,
            object,
            bytes.clone(),
            &[(first.as_str(), &anchor)],
            false,
        );
        assert!(parse(&duplicate).is_err(), "{name}: duplicate link parsed");
        let missing = rebuilt(&f, object, bytes, &[], true);
        assert!(parse(&missing).is_err(), "{name}: missing link parsed");
    }
}

#[test]
fn a_wrong_object_type_fails_closed() {
    let f = fixture();
    // Each object fed to every other parser is refused.
    let all = parsers(&f);
    for (name, _, parse) in &all {
        for (other, object, _) in &all {
            if name != other {
                assert!(parse(object).is_err(), "{name} parser accepted a {other}");
            }
        }
    }
}

#[test]
fn oversized_text_fields_fail_closed() {
    let f = fixture();
    // Offset of the first u32-length-prefixed text field in each payload:
    // campaign = version + epoch(32) + starts(8) + ends(8) + grant(8);
    // finding / contribution = version + 32-byte tag; disposition = version.
    let offsets = [
        ("campaign", 57usize),
        ("finding", 33),
        ("disposition", 1),
        ("contribution", 33),
    ];
    for (name, object, parse) in parsers(&f) {
        let at = offsets.iter().find(|(n, _)| *n == name).unwrap().1;
        let bytes = bytes_of(object);
        let old_len = u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
        let huge = 1_000_000usize;
        let mut inflated = bytes[..at].to_vec();
        inflated.extend_from_slice(&(huge as u32).to_be_bytes());
        inflated.extend(std::iter::repeat_n(b'a', huge));
        inflated.extend_from_slice(&bytes[at + 4 + old_len..]);
        let bad = rebuilt(&f, object, Payload::Public(inflated), &[], false);
        assert!(parse(&bad).is_err(), "{name}: oversized text parsed");

        let mut lying = bytes.clone();
        lying[at..at + 4].copy_from_slice(&u32::MAX.to_be_bytes());
        let bad = rebuilt(&f, object, Payload::Public(lying), &[], false);
        assert!(parse(&bad).is_err(), "{name}: lying length parsed");
    }
}
