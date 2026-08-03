//! One bounded, authenticated F1/F2/F2b pull from a single peer: advertise ->
//! generic verified retrieval -> a federation-specific post-check.
//!
//! The generic `mini_sync::request_retrieval`/`serve_retrieval` exchange
//! already proves every returned object is validly signed by *some* real,
//! KEL-verified identity (`mini-sync`'s own trust boundary, unmodified
//! here). That is necessary but not sufficient for federation: a peer that
//! is honest about its own signature can still relay someone else's
//! unrelated valid object, or an object of a type this session never asked
//! for. [`pull_source`] adds the two checks that make "this session's
//! objects" a meaningful claim: every returned object must decode as F1
//! ([`mini_search_federation::CRAWL_OBSERVATION_TYPE`]) or F2
//! ([`mini_search_federation::INDEX_SEGMENT_TYPE`]), and, when the caller
//! names an `expected_provider`, must be authored by exactly that identity.
//! Objects that fail either check were still validly ingested into the
//! local store by `mini-sync` (removing them would mean forking the
//! generic crate's trust boundary) but are excluded from
//! [`SourcePullReport::trusted`] -- callers must only treat `trusted` ids as
//! this source's contribution.

use did_mini::Did;
use mini_bearer::{Bearer, Channel};
use mini_objects::{Object, ObjectId, ObjectType};
use mini_search_federation::{CORPUS_BUNDLE_TYPE, CRAWL_OBSERVATION_TYPE, INDEX_SEGMENT_TYPE};
use mini_store::{Backend, Store};
use mini_sync::KelCache;

use crate::error::{NetError, Result};
use crate::message::{Msg, MAX_ADVERTISE_IDS};

const ADV_AAD: &[u8] = b"MINI/SEARCHFED-ADV1";

fn is_federation_object(t: &ObjectType) -> bool {
    matches!(
        t,
        ObjectType::Custom(name)
            if name == CRAWL_OBSERVATION_TYPE
                || name == INDEX_SEGMENT_TYPE
                || name == CORPUS_BUNDLE_TYPE
    )
}

fn send(bearer: &mut dyn Bearer, chan: &mut Channel, msg: &Msg) -> Result<()> {
    let ct = chan.seal(&msg.encode(), ADV_AAD)?;
    bearer.send(&ct)?;
    Ok(())
}

fn recv(bearer: &mut dyn Bearer, chan: &mut Channel) -> Result<Msg> {
    let ct = bearer.recv()?;
    let pt = chan.open(&ct, ADV_AAD)?;
    Msg::decode(&pt)
}

/// What one [`pull_source`] call learned about a single peer.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SourcePullReport {
    /// Ids the peer advertised (before the retrieval step).
    pub advertised: usize,
    /// The underlying `mini-sync` exact-retrieval counters.
    pub retrieval: mini_sync::RetrievalReport,
    /// Ids that passed both the generic ingest boundary and this crate's
    /// F1/F2/F2b-type + provider-identity check. Only these are this
    /// source's verified contribution.
    pub trusted: Vec<ObjectId>,
    /// Ingested successfully but not an F1/F2/F2b object type.
    pub wrong_type: usize,
    /// Ingested successfully, is F1/F2/F2b, but authored by someone other
    /// than `expected_provider`. Zero whenever the caller passes `None`.
    pub wrong_provider: usize,
}

/// Client side: pull up to `max_objects` F1/F2/F2b objects from one peer over an
/// already-established channel. `expected_provider`, when given, binds this
/// session to a specific identity -- objects from anyone else are excluded
/// from [`SourcePullReport::trusted`] even though `mini-sync` already
/// verified their own (different) authorship.
pub fn pull_source<B: Backend>(
    bearer: &mut dyn Bearer,
    chan: &mut Channel,
    store: &mut Store<B>,
    cache: &mut KelCache,
    expected_provider: Option<&Did>,
    max_objects: usize,
) -> Result<SourcePullReport> {
    if max_objects == 0 || max_objects > MAX_ADVERTISE_IDS {
        return Err(NetError::LimitExceeded);
    }

    send(
        bearer,
        chan,
        &Msg::AdvertiseRequest {
            max_ids: max_objects as u32,
        },
    )?;
    let offered = match recv(bearer, chan)? {
        Msg::AdvertiseResponse { ids } => ids,
        _ => return Err(NetError::Protocol),
    };
    if offered.len() > max_objects {
        return Err(NetError::LimitExceeded);
    }
    let mut candidates: Vec<ObjectId> = Vec::with_capacity(offered.len());
    for id in &offered {
        let oid = ObjectId::parse(id).map_err(|_| NetError::Protocol)?;
        if candidates.contains(&oid) {
            return Err(NetError::Protocol);
        }
        candidates.push(oid);
    }

    let mut report = SourcePullReport {
        advertised: candidates.len(),
        ..SourcePullReport::default()
    };
    if candidates.is_empty() {
        return Ok(report);
    }

    report.retrieval = mini_sync::request_retrieval(bearer, chan, store, cache, &candidates)?;

    for id in &candidates {
        let obj: Object = match store.get(id) {
            Ok(o) => o,
            // Rejected by the generic ingest boundary (unknown author,
            // invalid signature/provenance) -- already counted in
            // `report.retrieval.ingest`, nothing federation-specific to add.
            Err(_) => continue,
        };
        if !is_federation_object(&obj.object_type) {
            report.wrong_type += 1;
            continue;
        }
        if let Some(expected) = expected_provider {
            if &obj.author_human != expected {
                report.wrong_provider += 1;
                continue;
            }
        }
        report.trusted.push(id.clone());
    }

    Ok(report)
}

/// Server side: answer one peer's advertisement request and serve exactly
/// what was advertised (never more). `candidate_ids` is the caller's own
/// record of ids it is willing to advertise as F1/F2/F2b sources (e.g. ids
/// it has itself published) -- this crate does not scan the store to build
/// that set, the same way `mini-sync`'s exact-retrieval keeps selection
/// policy outside the generic wire layer.
///
/// Returns the ids actually offered, for caller-side logging/tests.
pub fn serve_source<B: Backend>(
    bearer: &mut dyn Bearer,
    chan: &mut Channel,
    store: &Store<B>,
    candidate_ids: &[ObjectId],
) -> Result<Vec<ObjectId>> {
    let max_ids = match recv(bearer, chan)? {
        Msg::AdvertiseRequest { max_ids } => max_ids as usize,
        _ => return Err(NetError::Protocol),
    };
    let cap = max_ids.min(MAX_ADVERTISE_IDS);

    let mut selected: Vec<ObjectId> = Vec::new();
    for id in candidate_ids {
        if selected.len() >= cap {
            break;
        }
        match store.get(id) {
            Ok(obj) if is_federation_object(&obj.object_type) => selected.push(id.clone()),
            _ => continue,
        }
    }

    send(
        bearer,
        chan,
        &Msg::AdvertiseResponse {
            ids: selected.iter().map(|id| id.as_str().to_string()).collect(),
        },
    )?;

    let requested = mini_sync::receive_retrieval_request(bearer, chan)?;
    let to_serve: Vec<ObjectId> = requested
        .into_iter()
        .filter(|id| selected.contains(id))
        .collect();
    mini_sync::serve_retrieval(bearer, chan, store, &to_serve)?;

    Ok(selected)
}

#[cfg(test)]
mod tests {
    //! `serve_source`'s `is_federation_object` filter means a *compliant*
    //! server can never advertise a non-F1/F2/F2b object -- so proving
    //! `pull_source`'s own
    //! wrong-type defense-in-depth genuinely fires requires a server that
    //! does not go through `serve_source` at all. That server can only be
    //! built with access to the private `Msg`/`send`/`recv` this module
    //! already has, which is why this one check lives here as a unit test
    //! instead of in `tests/` alongside the rest of this crate's black-box
    //! (compliant-peer-only) coverage.
    use std::thread;

    use did_mini::{Capabilities, Controller};
    use mini_bearer::{pair, Bearer, Channel, Initiator, Responder};
    use mini_objects::{ObjectBuilder, ObjectType, Payload};
    use mini_search_federation::publish_crawl_observation;
    use mini_store::{MemoryBackend, Store};
    use mini_sync::{kel_carrier, KelCache};
    use mini_web_types::{
        CanonicalUrl, CrawlObservation, CrawlObservationId, FetchStatus, HttpStatus,
        NormalizedHost, ProviderPseudonym, Scheme,
    };

    use super::*;

    fn human(seed: u8) -> (Controller, Controller) {
        let mut root = Controller::incept_single_from_seeds(&[seed; 32], &[seed + 1; 32]).unwrap();
        let device = Controller::incept_device_single_from_seeds(
            &root.did(),
            &[seed + 2; 32],
            &[seed + 3; 32],
        )
        .unwrap();
        root.delegate_device(&device.did(), Capabilities::primary())
            .unwrap();
        (root, device)
    }

    fn channels(
        a: &mut mini_bearer::InProcessBearer,
        b: &mut mini_bearer::InProcessBearer,
    ) -> (Channel, Channel) {
        let (init, hello1) = Initiator::start().unwrap();
        a.send(&hello1).unwrap();
        let got1 = b.recv().unwrap();
        let (chan_b, hello2) = Responder::respond(&got1).unwrap();
        b.send(&hello2).unwrap();
        let got2 = a.recv().unwrap();
        (init.finish(&got2).unwrap(), chan_b)
    }

    #[test]
    fn a_non_f1_f2_object_advertised_by_a_noncompliant_peer_is_excluded_from_trusted() {
        let (root, device) = human(90);
        let mut server_store: Store<MemoryBackend> = Store::new(MemoryBackend::new());
        server_store
            .insert(&kel_carrier(&root.kel(), &root.did(), &device).unwrap())
            .unwrap();
        server_store
            .insert(&kel_carrier(&device.kel(), &root.did(), &device).unwrap())
            .unwrap();
        // A perfectly valid, validly signed object -- just not F1/F2.
        let post = ObjectBuilder::new(ObjectType::POST)
            .timestamp_ms(1)
            .sequence(0)
            .payload(Payload::Public(b"not a search object".to_vec()))
            .sign(&root.did(), &device)
            .unwrap();
        let post_id = post.id().clone();
        server_store.insert(&post).unwrap();

        let mut client_store: Store<MemoryBackend> = Store::new(MemoryBackend::new());
        let mut client_cache = KelCache::new();
        client_cache.insert_verified(root.kel());
        client_cache.insert_verified(device.kel());

        let (mut client_bearer, mut server_bearer) = pair();
        let (mut client_chan, mut server_chan) = channels(&mut client_bearer, &mut server_bearer);

        let advertised_id = post_id.as_str().to_string();
        let server_thread = thread::spawn(move || {
            // A noncompliant server: skip `serve_source`'s type filter and
            // advertise the POST object directly.
            let _req = recv(&mut server_bearer, &mut server_chan).unwrap();
            send(
                &mut server_bearer,
                &mut server_chan,
                &Msg::AdvertiseResponse {
                    ids: vec![advertised_id],
                },
            )
            .unwrap();
            let requested =
                mini_sync::receive_retrieval_request(&mut server_bearer, &mut server_chan).unwrap();
            mini_sync::serve_retrieval(
                &mut server_bearer,
                &mut server_chan,
                &server_store,
                &requested,
            )
            .unwrap();
        });

        let report = pull_source(
            &mut client_bearer,
            &mut client_chan,
            &mut client_store,
            &mut client_cache,
            None,
            16,
        )
        .unwrap();
        server_thread.join().unwrap();

        assert_eq!(report.advertised, 1);
        assert_eq!(report.retrieval.ingest.accepted, 1);
        assert_eq!(report.wrong_type, 1);
        assert!(report.trusted.is_empty());
        // The object is still present in the local store (mini-sync's own
        // trust boundary is not forked), just excluded from `trusted`.
        assert!(client_store.contains(&post_id).unwrap());
    }

    fn url(host: &str, path: &str) -> CanonicalUrl {
        CanonicalUrl::new(
            Scheme::Https,
            NormalizedHost::new(host).unwrap(),
            None,
            path,
            None,
        )
        .unwrap()
    }

    #[test]
    fn objects_from_an_unexpected_provider_are_excluded_from_trusted() {
        let (a_root, a_device) = human(91);
        let (b_root, b_device) = human(92);
        let mut server_store: Store<MemoryBackend> = Store::new(MemoryBackend::new());
        for (root, device) in [(&a_root, &a_device), (&b_root, &b_device)] {
            server_store
                .insert(&kel_carrier(&root.kel(), &root.did(), device).unwrap())
                .unwrap();
            server_store
                .insert(&kel_carrier(&device.kel(), &root.did(), device).unwrap())
                .unwrap();
        }
        let obs_a = CrawlObservation {
            id: CrawlObservationId(mini_crypto::Multihash::of(
                mini_crypto::HashAlgorithm::Blake3,
                b"a",
            )),
            requested_url: url("a.example", "/"),
            final_url: url("a.example", "/"),
            observed_at_ms: 1,
            status: FetchStatus::Success(HttpStatus::new(200).unwrap()),
            content_digest: None,
            media_type: None,
            byte_length: None,
            redirect_chain: Vec::new(),
            crawler: ProviderPseudonym(mini_crypto::Multihash::of(
                mini_crypto::HashAlgorithm::Blake3,
                b"crawler-a",
            )),
        };
        let id_a =
            publish_crawl_observation(&mut server_store, &a_root.did(), &a_device, &obs_a).unwrap();
        let mut obs_b = obs_a.clone();
        obs_b.id = CrawlObservationId(mini_crypto::Multihash::of(
            mini_crypto::HashAlgorithm::Blake3,
            b"b",
        ));
        obs_b.requested_url = url("b.example", "/");
        obs_b.final_url = url("b.example", "/");
        let id_b =
            publish_crawl_observation(&mut server_store, &b_root.did(), &b_device, &obs_b).unwrap();

        let mut client_store: Store<MemoryBackend> = Store::new(MemoryBackend::new());
        let mut client_cache = KelCache::new();
        client_cache.insert_verified(a_root.kel());
        client_cache.insert_verified(a_device.kel());
        client_cache.insert_verified(b_root.kel());
        client_cache.insert_verified(b_device.kel());

        let (mut client_bearer, mut server_bearer) = pair();
        let (mut client_chan, mut server_chan) = channels(&mut client_bearer, &mut server_bearer);
        let expected_a = a_root.did();
        let server_thread = thread::spawn(move || {
            serve_source(
                &mut server_bearer,
                &mut server_chan,
                &server_store,
                &[id_a, id_b],
            )
            .unwrap();
        });

        let report = pull_source(
            &mut client_bearer,
            &mut client_chan,
            &mut client_store,
            &mut client_cache,
            Some(&expected_a),
            16,
        )
        .unwrap();
        server_thread.join().unwrap();

        assert_eq!(report.advertised, 2);
        assert_eq!(report.retrieval.ingest.accepted, 2);
        assert_eq!(report.wrong_provider, 1);
        assert_eq!(report.trusted.len(), 1);
    }
}
