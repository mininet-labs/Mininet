use did_mini::{Capabilities, Controller, FreshnessPins};
use mini_bearer::{Initiator, Responder};
use mini_crypto::AgreementSecretKey;
use mini_transport_security::{
    PeerAdvertisement, ReplayCache, SessionAuthClaim, SessionRole, TransportPurpose,
    TransportSecurityError,
};

fn identity(seed: u8) -> (Controller, Controller) {
    let mut root = Controller::incept_single_from_seeds(&[seed; 32], &[seed + 1; 32]).unwrap();
    let device =
        Controller::incept_device_single_from_seeds(&root.did(), &[seed + 2; 32], &[seed + 3; 32])
            .unwrap();
    root.delegate_device(&device.did(), Capabilities::primary())
        .unwrap();
    (root, device)
}

fn channel_binding() -> [u8; 32] {
    let (initiator, hello) = Initiator::start().unwrap();
    let (responder, response) = Responder::respond(&hello).unwrap();
    let initiator = initiator.finish(&response).unwrap();
    assert_eq!(initiator.channel_binding(), responder.channel_binding());
    initiator.channel_binding()
}

#[test]
fn dialed_advertisement_and_live_session_must_name_the_same_endpoint() {
    let network_id = [7; 32];
    let binding = channel_binding();
    let (advertised_root, advertised_device) = identity(10);
    let advertised_routing = AgreementSecretKey::from_seed(&[20; 32]).public_key();
    let advertisement = PeerAdvertisement::issue(
        network_id,
        &advertised_root.did(),
        &advertised_device,
        advertised_routing,
        "127.0.0.1:9000".parse().unwrap(),
        1_000,
        2_000,
    )
    .unwrap();
    let mut advertisement_pins = FreshnessPins::new();
    let mut advertisement_replay = ReplayCache::new(8).unwrap();
    let advertisement = advertisement
        .verify(
            network_id,
            1_500,
            &advertised_root.kel(),
            &advertised_device.kel(),
            &mut advertisement_pins,
            &mut advertisement_replay,
        )
        .unwrap();

    // A second genuine endpoint can make a valid claim for the same CH1
    // transcript, but it is not the endpoint selected from the advertisement.
    // The combined verifier must reject it before the dial becomes accepted.
    let (redirect_root, redirect_device) = identity(40);
    let redirect_routing = AgreementSecretKey::from_seed(&[50; 32]).public_key();
    let redirect_claim = SessionAuthClaim::issue(
        &redirect_root.did(),
        &redirect_device,
        SessionRole::Responder,
        TransportPurpose::PeerExchange,
        redirect_routing,
        &binding,
        1_000,
        2_000,
    )
    .unwrap();
    let mut session_pins = FreshnessPins::new();
    let mut session_replay = ReplayCache::new(8).unwrap();
    assert_eq!(
        redirect_claim.verify_advertised(
            &advertisement,
            SessionRole::Responder,
            TransportPurpose::PeerExchange,
            &binding,
            1_500,
            &redirect_root.kel(),
            &redirect_device.kel(),
            &mut session_pins,
            &mut session_replay,
        ),
        Err(TransportSecurityError::IdentityMismatch)
    );

    let advertised_claim = SessionAuthClaim::issue(
        &advertised_root.did(),
        &advertised_device,
        SessionRole::Responder,
        TransportPurpose::PeerExchange,
        advertised_routing,
        &binding,
        1_000,
        2_000,
    )
    .unwrap();
    let accepted = advertised_claim
        .verify_advertised(
            &advertisement,
            SessionRole::Responder,
            TransportPurpose::PeerExchange,
            &binding,
            1_500,
            &advertised_root.kel(),
            &advertised_device.kel(),
            &mut session_pins,
            &mut session_replay,
        )
        .unwrap();
    assert_eq!(accepted.endpoint_id, advertisement.endpoint_id());
    assert_eq!(accepted.routing_key, advertisement.routing_key());
}
