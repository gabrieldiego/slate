#![forbid(unsafe_code)]
#![cfg(feature = "test-fixtures")]

use slate_broadwebd::{
    BroadwebDaemon, BroadwebdError, DEFAULT_PROFILE_SYNC_DISCOVERY_NAMESPACE, HttpFetchRequest,
    HttpHeader, IpnsProfileSyncPeerDiscoveryProvider, PluginRegistry, ProfileSyncObjectRequest,
    ProfileSyncPeerAdvertisement, ProfileSyncPeerAdvertisementSignature,
    ProfileSyncPeerDiscoveryProtocol, ProfileSyncPeerDiscoveryProvider,
    ProfileSyncPeerDiscoveryQuery, ProfileSyncPutObjectRequest, ProfileSyncRequest,
    ProfileSyncResponse, ProfileSyncRootRequest, ProfileSyncRootUpdate, ResourceBudget,
    test_fixtures::{
        InProcessBroadwebNetwork, InternalFixtureHttpResponse, InternalKuboRpcResponse,
        InternalKuboRpcTransportShim, ProfileSyncFixtureCapacity,
    },
};
use slate_storage::ProfileSyncDeviceSigner;

#[test]
fn daemon_fetches_fixture_http_without_loopback_transport() {
    let network = InProcessBroadwebNetwork::new();
    let fixture = network.http_response(InternalFixtureHttpResponse {
        status_code: 200,
        content_type: Some("text/html; charset=utf-8".to_string()),
        headers: vec![HttpHeader {
            name: "content-type".to_string(),
            value: "text/html; charset=utf-8".to_string(),
        }],
        body: b"<!doctype html><title>In Process Fixture</title><h1>Fixture body</h1>".to_vec(),
    });
    let fixture_url = fixture.base_url().to_string();
    let state_root = std::env::temp_dir().join(format!(
        "slate-broadwebd-in-process-fixture-{}",
        std::process::id()
    ));
    let daemon = BroadwebDaemon::start_with_registry(
        &state_root,
        ResourceBudget::default(),
        network.fixture_registry(),
    )
    .expect("daemon");

    let response = daemon
        .fetch_http(HttpFetchRequest::default_profile(&fixture_url))
        .expect("fetch synthetic fixture URL");
    let requests = fixture.finish();

    assert!(fixture_url.starts_with("slate-fixture-http://"));
    assert_eq!(response.status_code, 200);
    assert!(response.body_text_lossy().contains("Fixture body"));
    assert_eq!(requests, vec!["/"]);

    let _ = std::fs::remove_dir_all(state_root);
}

#[test]
fn fixture_registry_rejects_external_http_without_network_access() {
    let network = InProcessBroadwebNetwork::new();
    let state_root = std::env::temp_dir().join(format!(
        "slate-broadwebd-fixture-external-reject-{}",
        std::process::id()
    ));
    let daemon = BroadwebDaemon::start_with_registry(
        &state_root,
        ResourceBudget::default(),
        network.fixture_registry(),
    )
    .expect("daemon");

    let error = daemon
        .fetch_http(HttpFetchRequest::default_profile("https://example.com/"))
        .expect_err("fixture-only transport must reject external HTTP");

    assert!(matches!(
        error,
        BroadwebdError::UnsupportedRequest(message)
            if message.contains("cannot fetch external URL")
    ));

    let _ = std::fs::remove_dir_all(state_root);
}

#[test]
fn fixture_http_urls_are_scoped_to_creating_network() {
    let first_network = InProcessBroadwebNetwork::new();
    let second_network = InProcessBroadwebNetwork::new();
    let fixture = first_network.http_response(InternalFixtureHttpResponse {
        status_code: 200,
        content_type: Some("text/html; charset=utf-8".to_string()),
        headers: vec![HttpHeader {
            name: "content-type".to_string(),
            value: "text/html; charset=utf-8".to_string(),
        }],
        body: b"<!doctype html><title>Network Scoped Fixture</title>".to_vec(),
    });
    let fixture_url = fixture.base_url().to_string();
    let first_state_root = std::env::temp_dir().join(format!(
        "slate-broadwebd-fixture-scope-first-{}",
        std::process::id()
    ));
    let second_state_root = std::env::temp_dir().join(format!(
        "slate-broadwebd-fixture-scope-second-{}",
        std::process::id()
    ));
    let first_daemon = BroadwebDaemon::start_with_registry(
        &first_state_root,
        ResourceBudget::default(),
        first_network.fixture_registry(),
    )
    .expect("first network daemon");
    let second_daemon = BroadwebDaemon::start_with_registry(
        &second_state_root,
        ResourceBudget::default(),
        second_network.fixture_registry(),
    )
    .expect("second network daemon");

    assert!(fixture_url.starts_with("slate-fixture-http://"));
    assert!(fixture_url.contains(first_network.network_id()));
    assert!(!fixture_url.contains(second_network.network_id()));

    let cross_network_error = second_daemon
        .fetch_http(HttpFetchRequest::default_profile(&fixture_url))
        .expect_err("another in-process network must not consume this fixture URL");
    assert!(matches!(
        cross_network_error,
        BroadwebdError::UnsupportedRequest(message)
            if message.contains("does not belong to in-process network")
    ));

    let response = first_daemon
        .fetch_http(HttpFetchRequest::default_profile(&fixture_url))
        .expect("owning in-process network can consume this fixture URL");
    assert_eq!(
        response.body,
        b"<!doctype html><title>Network Scoped Fixture</title>".to_vec()
    );
    assert_eq!(fixture.finish(), vec!["/"]);

    let _ = std::fs::remove_dir_all(first_state_root);
    let _ = std::fs::remove_dir_all(second_state_root);
}

#[test]
fn default_registry_rejects_internal_http_fixture_urls() {
    let network = InProcessBroadwebNetwork::new();
    let fixture = network.http_response(InternalFixtureHttpResponse {
        status_code: 200,
        content_type: Some("text/plain".to_string()),
        headers: Vec::new(),
        body: b"default registry must not fetch this".to_vec(),
    });
    let state_root = std::env::temp_dir().join(format!(
        "slate-broadwebd-default-registry-fixture-reject-{}",
        std::process::id()
    ));
    let daemon = BroadwebDaemon::start_with_registry(
        &state_root,
        ResourceBudget::default(),
        PluginRegistry::with_default_http(),
    )
    .expect("default registry daemon");

    let error = daemon
        .fetch_http(HttpFetchRequest::default_profile(fixture.base_url()))
        .expect_err("default direct-http transport must not consume internal fixtures");
    assert!(matches!(
        error,
        BroadwebdError::UnsupportedRequest(message)
            if message.contains("cannot fetch internal fixture URL")
                || message.contains("direct-http cannot fetch")
                || message.contains("no HTTP transport for slate-fixture-http")
    ));

    assert!(fixture.finish().is_empty());
    let _ = std::fs::remove_dir_all(state_root);
}

#[test]
fn in_process_profile_sync_peer_discovery_models_p2p_networks_without_sockets() {
    let network = InProcessBroadwebNetwork::new();
    let first_device = network.profile_sync_peer_discovery_provider();
    let second_device = network.profile_sync_peer_discovery_provider();
    let requester = network.profile_sync_peer_discovery_provider();

    first_device
        .publish_profile_sync_peer(
            ProfileSyncPeerDiscoveryProtocol::Libp2pRendezvous,
            DEFAULT_PROFILE_SYNC_DISCOVERY_NAMESPACE,
            ProfileSyncPeerAdvertisement::new(
                "profile-a",
                "device-a",
                "provider-a",
                "/dnsaddr/rendezvous.slate.test/tcp/443/wss/p2p/12D3KooWDeviceA",
                1,
            )
            .expect("libp2p rendezvous-shaped advertisement"),
        )
        .expect("publish libp2p rendezvous-shaped advertisement");
    second_device
        .publish_profile_sync_peer(
            ProfileSyncPeerDiscoveryProtocol::Ipns,
            DEFAULT_PROFILE_SYNC_DISCOVERY_NAMESPACE,
            ProfileSyncPeerAdvertisement::new(
                "profile-a",
                "device-b",
                "provider-b",
                "/ipns/k51-profile-sync-root-device-b",
                2,
            )
            .expect("IPNS-shaped advertisement"),
        )
        .expect("publish IPNS-shaped advertisement");
    requester
        .publish_profile_sync_peer(
            ProfileSyncPeerDiscoveryProtocol::Libp2pRendezvous,
            DEFAULT_PROFILE_SYNC_DISCOVERY_NAMESPACE,
            ProfileSyncPeerAdvertisement::new(
                "profile-a",
                "device-local",
                "provider-local",
                "/dnsaddr/rendezvous.slate.test/tcp/443/wss/p2p/12D3KooWLocal",
                3,
            )
            .expect("requester's own advertisement"),
        )
        .expect("publish requester's own advertisement");

    let query = ProfileSyncPeerDiscoveryQuery::for_default_namespace(
        "profile-a",
        "device-local",
        [
            ProfileSyncPeerDiscoveryProtocol::Libp2pRendezvous,
            ProfileSyncPeerDiscoveryProtocol::Ipns,
        ],
        8,
    )
    .expect("discovery query");
    let discovered = requester
        .discover_profile_sync_peers(&query)
        .expect("discover peers through simulated p2p providers");

    assert_eq!(discovered.len(), 2);
    assert_eq!(
        discovered
            .iter()
            .map(|peer| peer.protocol)
            .collect::<Vec<_>>(),
        vec![
            ProfileSyncPeerDiscoveryProtocol::Libp2pRendezvous,
            ProfileSyncPeerDiscoveryProtocol::Ipns,
        ]
    );
    assert_eq!(discovered[0].advertisement.node_id, "device-a");
    assert_eq!(discovered[1].advertisement.node_id, "device-b");
    assert!(
        discovered
            .iter()
            .all(|peer| peer.advertisement.has_multiaddr_service_endpoint())
    );
    assert!(
        discovered
            .iter()
            .all(|peer| peer.advertisement.service_socket_addr().is_err())
    );
}

#[test]
fn ipns_profile_sync_peer_discovery_round_trips_through_kubo_model_without_sockets() {
    let network = InProcessBroadwebNetwork::new();
    let kubo = network.kubo_profile_sync_model();
    let rpc = kubo
        .profile_sync_rpc()
        .expect("fixture Kubo profile-sync RPC");
    let provider = IpnsProfileSyncPeerDiscoveryProvider::new(
        rpc,
        InternalKuboRpcTransportShim,
        ResourceBudget::default(),
    )
    .with_publish_key_id("device-a")
    .with_resolve_name("device-a");
    let signer = ProfileSyncDeviceSigner::generate("device-a")
        .expect("generate IPNS discovery advertisement signer");

    provider
        .publish_profile_sync_peer(
            ProfileSyncPeerDiscoveryProtocol::Ipns,
            DEFAULT_PROFILE_SYNC_DISCOVERY_NAMESPACE,
            sign_test_profile_sync_peer_advertisement(
                ProfileSyncPeerAdvertisement::new(
                    "profile-a",
                    "device-a",
                    "provider-a",
                    "/dnsaddr/rendezvous.slate.test/tcp/443/wss/p2p/12D3KooWDeviceA",
                    1,
                )
                .expect("IPNS-discovered advertisement"),
                &signer,
            )
            .expect("signed IPNS-discovered advertisement"),
        )
        .expect("publish IPNS discovery record through Kubo fixture");

    let query = ProfileSyncPeerDiscoveryQuery::for_default_namespace(
        "profile-a",
        "device-local",
        [ProfileSyncPeerDiscoveryProtocol::Ipns],
        4,
    )
    .expect("IPNS discovery query");
    let discovered = provider
        .discover_profile_sync_peers(&query)
        .expect("discover IPNS record through Kubo fixture");

    assert_eq!(discovered.len(), 1);
    assert_eq!(
        discovered[0].protocol,
        ProfileSyncPeerDiscoveryProtocol::Ipns
    );
    assert_eq!(discovered[0].advertisement.node_id, "device-a");
    assert_eq!(discovered[0].advertisement.provider_id, "provider-a");
    assert!(discovered[0].advertisement.identity_signature.is_some());
    assert!(discovered[0].advertisement.has_multiaddr_service_endpoint());
    let requests = kubo.finish();
    assert_eq!(requests.len(), 5);
    assert_eq!(
        requests[0],
        "POST /api/v0/add?cid-version=1&raw-leaves=true&pin=false HTTP/1.1"
    );
    assert!(requests[1].starts_with("POST /api/v0/pin/add?arg=bafyfixture"));
    assert!(requests[1].ends_with("&recursive=true HTTP/1.1"));
    let object_id = requests[1]
        .strip_prefix("POST /api/v0/pin/add?arg=")
        .and_then(|request| request.strip_suffix("&recursive=true HTTP/1.1"))
        .expect("pin request carries object id");
    assert_eq!(
        requests[2],
        format!(
            "POST /api/v0/name/publish?arg=%2Fipfs%2F{object_id}&key=device-a&allow-offline=true HTTP/1.1"
        )
    );
    assert_eq!(
        requests[3],
        "POST /api/v0/name/resolve?arg=%2Fipns%2Fdevice-a&recursive=false HTTP/1.1"
    );
    assert_eq!(
        requests[4],
        format!("POST /api/v0/cat?arg=%2Fipfs%2F{object_id} HTTP/1.1")
    );
}

fn sign_test_profile_sync_peer_advertisement(
    advertisement: ProfileSyncPeerAdvertisement,
    signer: &ProfileSyncDeviceSigner,
) -> Result<ProfileSyncPeerAdvertisement, BroadwebdError> {
    let payload = advertisement.signing_payload_bytes()?;
    let signed = signer
        .sign(payload.as_slice())
        .map_err(|error| BroadwebdError::Request(error.to_string()))?;
    let signature = ProfileSyncPeerAdvertisementSignature::ed25519(
        signed.device_id,
        signed.public_key,
        signed.signature,
    )?;
    advertisement.with_identity_signature(signature)
}

#[test]
fn device_daemons_share_profile_sync_without_loopback_transport() {
    let network = InProcessBroadwebNetwork::new();
    let first_state_root = std::env::temp_dir().join(format!(
        "slate-broadwebd-device-fixture-first-{}",
        std::process::id()
    ));
    let second_state_root = std::env::temp_dir().join(format!(
        "slate-broadwebd-device-fixture-second-{}",
        std::process::id()
    ));
    let first = network
        .daemon_for_device(&first_state_root, ResourceBudget::default(), "fixture-a")
        .expect("start first in-process device daemon");
    let second = network
        .daemon_for_device(&second_state_root, ResourceBudget::default(), "fixture-b")
        .expect("start second in-process device daemon");

    let ProfileSyncResponse::PutEncryptedObject { object_id } = first
        .profile_sync(ProfileSyncRequest::PutEncryptedObject(
            ProfileSyncPutObjectRequest::new("default", b"fixture profile bytes".to_vec()),
        ))
        .expect("put fixture profile-sync object")
    else {
        panic!("put object returned unexpected response");
    };
    let ProfileSyncResponse::RetainObject { retained: true, .. } = first
        .profile_sync(ProfileSyncRequest::RetainObject(
            ProfileSyncObjectRequest::new("default", object_id.as_str()),
        ))
        .expect("retain fixture profile-sync object")
    else {
        panic!("retain object returned unexpected response");
    };
    first
        .profile_sync(ProfileSyncRequest::PublishRoot(ProfileSyncRootUpdate::new(
            "default",
            "settings/latest",
            object_id.as_str(),
        )))
        .expect("publish fixture profile-sync root");

    let ProfileSyncResponse::Root {
        object_id: Some(resolved_object_id),
        ..
    } = second
        .profile_sync(ProfileSyncRequest::ResolveRoot(
            ProfileSyncRootRequest::new("default", "settings/latest"),
        ))
        .expect("resolve fixture profile-sync root from second daemon")
    else {
        panic!("resolve root returned unexpected response");
    };
    let ProfileSyncResponse::GetEncryptedObject { bytes, .. } = second
        .profile_sync(ProfileSyncRequest::GetEncryptedObject(
            ProfileSyncObjectRequest::new("default", resolved_object_id.as_str()),
        ))
        .expect("fetch fixture profile-sync object from second daemon")
    else {
        panic!("get object returned unexpected response");
    };
    assert_eq!(resolved_object_id, object_id);
    assert_eq!(bytes, b"fixture profile bytes".to_vec());

    let external_error = second
        .fetch_http(HttpFetchRequest::default_profile("https://example.com/"))
        .expect_err("per-device fixture registry must reject external HTTP");
    assert!(matches!(
        external_error,
        BroadwebdError::UnsupportedRequest(message)
            if message.contains("cannot fetch external URL")
    ));

    let _ = std::fs::remove_dir_all(first_state_root);
    let _ = std::fs::remove_dir_all(second_state_root);
}

#[test]
fn in_process_profile_sync_fixture_enforces_provider_capacity() {
    let network =
        InProcessBroadwebNetwork::with_profile_sync_capacity(ProfileSyncFixtureCapacity {
            max_providers: Some(1),
            max_objects: Some(4),
            max_roots: Some(4),
        });
    let first_state_root = std::env::temp_dir().join(format!(
        "slate-broadwebd-provider-capacity-first-{}",
        std::process::id()
    ));
    let second_state_root = std::env::temp_dir().join(format!(
        "slate-broadwebd-provider-capacity-second-{}",
        std::process::id()
    ));
    let first = network
        .daemon_for_device(&first_state_root, ResourceBudget::default(), "fixture-a")
        .expect("start first capacity-bounded device daemon");
    let second = network
        .daemon_for_device(&second_state_root, ResourceBudget::default(), "fixture-b")
        .expect("start second capacity-bounded device daemon");

    first
        .profile_sync(ProfileSyncRequest::PutEncryptedObject(
            ProfileSyncPutObjectRequest::new("default", b"first fixture object".to_vec()),
        ))
        .expect("first provider fits fixture capacity");
    let error = second
        .profile_sync(ProfileSyncRequest::PutEncryptedObject(
            ProfileSyncPutObjectRequest::new("default", b"second fixture object".to_vec()),
        ))
        .expect_err("second provider should exceed fixture capacity");

    assert!(matches!(
        error,
        BroadwebdError::UnsupportedRequest(message)
            if message.contains("provider capacity exceeded")
    ));

    let _ = std::fs::remove_dir_all(first_state_root);
    let _ = std::fs::remove_dir_all(second_state_root);
}

#[test]
fn in_process_profile_sync_fixture_enforces_object_and_root_capacity() {
    let network =
        InProcessBroadwebNetwork::with_profile_sync_capacity(ProfileSyncFixtureCapacity {
            max_providers: Some(1),
            max_objects: Some(1),
            max_roots: Some(1),
        });
    let state_root = std::env::temp_dir().join(format!(
        "slate-broadwebd-object-root-capacity-{}",
        std::process::id()
    ));
    let daemon = network
        .daemon_for_device(&state_root, ResourceBudget::default(), "fixture-a")
        .expect("start capacity-bounded device daemon");

    let ProfileSyncResponse::PutEncryptedObject { object_id } = daemon
        .profile_sync(ProfileSyncRequest::PutEncryptedObject(
            ProfileSyncPutObjectRequest::new("default", b"first fixture object".to_vec()),
        ))
        .expect("first fixture object fits capacity")
    else {
        panic!("put object returned unexpected response");
    };
    let object_error = daemon
        .profile_sync(ProfileSyncRequest::PutEncryptedObject(
            ProfileSyncPutObjectRequest::new("default", b"second fixture object".to_vec()),
        ))
        .expect_err("second unique object should exceed fixture capacity");
    assert!(matches!(
        object_error,
        BroadwebdError::UnsupportedRequest(message)
            if message.contains("object capacity exceeded")
    ));

    daemon
        .profile_sync(ProfileSyncRequest::PublishRoot(ProfileSyncRootUpdate::new(
            "default",
            "settings/latest",
            object_id.as_str(),
        )))
        .expect("first fixture root fits capacity");
    let root_error = daemon
        .profile_sync(ProfileSyncRequest::PublishRoot(ProfileSyncRootUpdate::new(
            "default",
            "settings/alternate",
            object_id.as_str(),
        )))
        .expect_err("second root should exceed fixture capacity");
    assert!(matches!(
        root_error,
        BroadwebdError::UnsupportedRequest(message)
            if message.contains("root capacity exceeded")
    ));

    let _ = std::fs::remove_dir_all(state_root);
}

#[test]
fn in_process_kubo_profile_sync_fixture_enforces_service_capacity() {
    let network =
        InProcessBroadwebNetwork::with_profile_sync_capacity(ProfileSyncFixtureCapacity {
            max_providers: Some(1),
            max_objects: Some(1),
            max_roots: Some(1),
        });
    let kubo_fixture = network.kubo_profile_sync_model();
    let state_root = std::env::temp_dir().join(format!(
        "slate-broadwebd-kubo-profile-capacity-{}",
        std::process::id()
    ));
    let daemon = network
        .daemon_for_kubo_profile_sync(
            &state_root,
            ResourceBudget::default(),
            kubo_fixture.base_url().to_string(),
            "kubo-capacity",
        )
        .expect("start capacity-bounded Kubo profile-sync daemon");

    let ProfileSyncResponse::PutEncryptedObject { object_id } = daemon
        .profile_sync(ProfileSyncRequest::PutEncryptedObject(
            ProfileSyncPutObjectRequest::new("default", b"first Kubo fixture object".to_vec()),
        ))
        .expect("first Kubo object fits service capacity")
    else {
        panic!("put object returned unexpected response");
    };
    let object_error = daemon
        .profile_sync(ProfileSyncRequest::PutEncryptedObject(
            ProfileSyncPutObjectRequest::new("default", b"second Kubo fixture object".to_vec()),
        ))
        .expect_err("second unique Kubo object should exceed service capacity");
    assert!(matches!(
        object_error,
        BroadwebdError::UnsupportedRequest(message)
            if message.contains("object capacity exceeded")
    ));

    daemon
        .profile_sync(ProfileSyncRequest::PublishRoot(ProfileSyncRootUpdate::new(
            "default",
            "settings/latest",
            object_id.as_str(),
        )))
        .expect("first Kubo root fits service capacity");
    let root_error = daemon
        .profile_sync(ProfileSyncRequest::PublishRoot(ProfileSyncRootUpdate::new(
            "default",
            "settings/alternate",
            object_id.as_str(),
        )))
        .expect_err("second Kubo root should exceed service capacity");
    assert!(matches!(
        root_error,
        BroadwebdError::UnsupportedRequest(message)
            if message.contains("root capacity exceeded")
    ));
    assert_eq!(
        kubo_fixture.finish(),
        vec![
            "POST /api/v0/add?cid-version=1&raw-leaves=true&pin=false HTTP/1.1".to_string(),
            format!(
                "POST /api/v0/name/publish?arg=%2Fipfs%2F{}&key=settings%2Flatest&allow-offline=true HTTP/1.1",
                object_id
            ),
        ]
    );

    let _ = std::fs::remove_dir_all(state_root);
}

#[test]
fn in_process_kubo_profile_sync_model_enforces_capacity_before_sockets() {
    let network =
        InProcessBroadwebNetwork::with_profile_sync_capacity(ProfileSyncFixtureCapacity {
            max_providers: None,
            max_objects: Some(1),
            max_roots: Some(1),
        });
    let kubo_fixture = network.kubo_profile_sync_model();
    let rpc = kubo_fixture
        .profile_sync_rpc()
        .expect("build fixture Kubo profile-sync RPC");
    let executor = InternalKuboRpcTransportShim;
    let budget = ResourceBudget::default();

    let object_id = rpc
        .put_encrypted_object(&executor, b"first Kubo model object", &budget)
        .expect("first Kubo model object fits capacity");
    let object_error = rpc
        .put_encrypted_object(&executor, b"second Kubo model object", &budget)
        .expect_err("second Kubo model object should exceed capacity");
    assert!(matches!(
        object_error,
        BroadwebdError::Request(message)
            if message.contains("Kubo profile-sync add returned HTTP status 507")
    ));

    rpc.publish_root(&executor, "settings/latest", object_id.as_str(), &budget)
        .expect("first Kubo model IPNS name fits capacity");
    let name_error = rpc
        .publish_root(&executor, "settings/alternate", object_id.as_str(), &budget)
        .expect_err("second Kubo model IPNS name should exceed capacity");
    assert!(matches!(
        name_error,
        BroadwebdError::Request(message)
            if message.contains("Kubo profile-sync name/publish returned HTTP status 507")
    ));

    let requests = kubo_fixture.finish();
    assert_eq!(requests.len(), 4);
    assert_eq!(
        requests[0],
        "POST /api/v0/add?cid-version=1&raw-leaves=true&pin=false HTTP/1.1"
    );
    assert_eq!(
        requests[1],
        "POST /api/v0/add?cid-version=1&raw-leaves=true&pin=false HTTP/1.1"
    );
    assert!(
        requests[2].starts_with("POST /api/v0/name/publish?"),
        "first publish request should be Kubo-shaped: {:?}",
        requests
    );
    assert!(
        requests[3].starts_with("POST /api/v0/name/publish?"),
        "second publish request should be Kubo-shaped: {:?}",
        requests
    );
}

#[test]
fn in_process_protocol_daemons_reject_loopback_endpoints() {
    let network = InProcessBroadwebNetwork::new();
    let gateway_state_root = std::env::temp_dir().join(format!(
        "slate-broadwebd-fixture-reject-loopback-gateway-{}",
        std::process::id()
    ));
    let kubo_state_root = std::env::temp_dir().join(format!(
        "slate-broadwebd-fixture-reject-loopback-kubo-{}",
        std::process::id()
    ));

    let gateway_error = network.daemon_for_ipfs_gateway(
        &gateway_state_root,
        ResourceBudget::default(),
        "http://127.0.0.1:8080",
    );
    let kubo_error = network.daemon_for_kubo_rpc(
        &kubo_state_root,
        ResourceBudget::default(),
        "http://127.0.0.1:5001",
    );
    let gateway_error = expect_daemon_error(
        gateway_error,
        "in-process fixture daemon must reject loopback IPFS gateways",
    );
    let kubo_error = expect_daemon_error(
        kubo_error,
        "in-process fixture daemon must reject loopback Kubo RPC endpoints",
    );

    assert!(matches!(
        gateway_error,
        BroadwebdError::UnsupportedRequest(message)
            if message.contains("must use a URL created by network")
    ));
    assert!(matches!(
        kubo_error,
        BroadwebdError::UnsupportedRequest(message)
            if message.contains("must use a URL created by network")
    ));
}

#[test]
fn in_process_kubo_rpc_urls_are_scoped_to_creating_network() {
    let first_network = InProcessBroadwebNetwork::new();
    let second_network = InProcessBroadwebNetwork::new();
    let kubo_fixture = first_network.kubo_rpc_response(InternalKuboRpcResponse {
        status_code: 200,
        content_type: "text/html; charset=utf-8".to_string(),
        body: b"<!doctype html><title>Kubo Fixture</title>".to_vec(),
    });
    let first_state_root = std::env::temp_dir().join(format!(
        "slate-broadwebd-kubo-scope-first-{}",
        std::process::id()
    ));
    let second_state_root = std::env::temp_dir().join(format!(
        "slate-broadwebd-kubo-scope-second-{}",
        std::process::id()
    ));

    assert!(kubo_fixture.base_url().starts_with("slate-fixture-kubo://"));
    assert!(kubo_fixture.base_url().contains(first_network.network_id()));
    assert!(
        !kubo_fixture
            .base_url()
            .contains(second_network.network_id())
    );

    let cross_network_error = second_network.daemon_for_kubo_rpc(
        &second_state_root,
        ResourceBudget::default(),
        kubo_fixture.base_url().to_string(),
    );
    let cross_network_error = expect_daemon_error(
        cross_network_error,
        "another in-process network must not use this Kubo fixture URL",
    );
    assert!(matches!(
        cross_network_error,
        BroadwebdError::UnsupportedRequest(message)
            if message.contains("must use a URL created by network")
    ));

    let daemon = first_network
        .daemon_for_kubo_rpc(
            &first_state_root,
            ResourceBudget::default(),
            kubo_fixture.base_url().to_string(),
        )
        .expect("owning in-process network can use this Kubo fixture URL");
    let response = daemon
        .fetch_http(HttpFetchRequest::default_profile(
            "ipfs://bafybeigdyrzt/index.html",
        ))
        .expect("owning network can fetch from Kubo fixture");
    assert_eq!(
        response.body,
        b"<!doctype html><title>Kubo Fixture</title>".to_vec()
    );
    assert_eq!(
        kubo_fixture.finish(),
        vec!["POST /api/v0/cat?arg=%2Fipfs%2Fbafybeigdyrzt%2Findex.html HTTP/1.1"]
    );

    let _ = std::fs::remove_dir_all(first_state_root);
    let _ = std::fs::remove_dir_all(second_state_root);
}

#[test]
fn ipfs_fixture_daemons_use_internal_protocol_endpoints() {
    let network = InProcessBroadwebNetwork::new();
    let gateway_fixture = network.http_response(InternalFixtureHttpResponse {
        status_code: 200,
        content_type: Some("text/html; charset=utf-8".to_string()),
        headers: vec![HttpHeader {
            name: "content-type".to_string(),
            value: "text/html; charset=utf-8".to_string(),
        }],
        body: b"<!doctype html><title>Fixture IPFS</title>".to_vec(),
    });
    let kubo_fixture = network.kubo_rpc_response(InternalKuboRpcResponse {
        status_code: 200,
        content_type: "text/html; charset=utf-8".to_string(),
        body: b"<!doctype html><title>Fixture Kubo</title>".to_vec(),
    });
    let gateway_state_root = std::env::temp_dir().join(format!(
        "slate-broadwebd-fixture-ipfs-gateway-{}",
        std::process::id()
    ));
    let kubo_state_root = std::env::temp_dir().join(format!(
        "slate-broadwebd-fixture-ipfs-kubo-{}",
        std::process::id()
    ));
    let gateway_daemon = network
        .daemon_for_ipfs_gateway(
            &gateway_state_root,
            ResourceBudget::default(),
            gateway_fixture.base_url().to_string(),
        )
        .expect("start fixture gateway daemon");
    let kubo_daemon = network
        .daemon_for_kubo_rpc(
            &kubo_state_root,
            ResourceBudget::default(),
            kubo_fixture.base_url().to_string(),
        )
        .expect("start fixture Kubo daemon");

    let gateway_response = gateway_daemon
        .fetch_http(HttpFetchRequest::default_profile(
            "ipfs://bafybeigdyrzt/index.html",
        ))
        .expect("fetch synthetic IPFS gateway fixture");
    let kubo_response = kubo_daemon
        .fetch_http(HttpFetchRequest::default_profile(
            "ipfs://bafybeigdyrzt/index.html",
        ))
        .expect("fetch synthetic Kubo fixture");

    assert_eq!(
        gateway_response
            .route
            .as_ref()
            .expect("gateway route")
            .transport_id,
        "ipfs-gateway"
    );
    assert_eq!(
        kubo_response
            .route
            .as_ref()
            .expect("Kubo route")
            .transport_id,
        "ipfs-kubo-rpc"
    );
    assert_eq!(
        gateway_fixture.finish(),
        vec!["/ipfs/bafybeigdyrzt/index.html"]
    );
    assert_eq!(
        kubo_fixture.finish(),
        vec!["POST /api/v0/cat?arg=%2Fipfs%2Fbafybeigdyrzt%2Findex.html HTTP/1.1"]
    );

    let _ = std::fs::remove_dir_all(gateway_state_root);
    let _ = std::fs::remove_dir_all(kubo_state_root);
}

#[test]
fn in_process_protocol_fixture_metadata_reports_socketless_boundaries() {
    let network = InProcessBroadwebNetwork::new();
    let gateway_fixture = network.http_response(InternalFixtureHttpResponse {
        status_code: 200,
        content_type: Some("text/html; charset=utf-8".to_string()),
        headers: vec![HttpHeader {
            name: "content-type".to_string(),
            value: "text/html; charset=utf-8".to_string(),
        }],
        body: b"<!doctype html><title>Fixture IPFS</title>".to_vec(),
    });
    let kubo_fixture = network.kubo_rpc_response(InternalKuboRpcResponse {
        status_code: 200,
        content_type: "text/html; charset=utf-8".to_string(),
        body: b"<!doctype html><title>Fixture Kubo</title>".to_vec(),
    });
    let gateway_state_root = std::env::temp_dir().join(format!(
        "slate-broadwebd-fixture-metadata-gateway-{}",
        std::process::id()
    ));
    let kubo_state_root = std::env::temp_dir().join(format!(
        "slate-broadwebd-fixture-metadata-kubo-{}",
        std::process::id()
    ));
    let gateway_daemon = network
        .daemon_for_ipfs_gateway(
            &gateway_state_root,
            ResourceBudget::default(),
            gateway_fixture.base_url().to_string(),
        )
        .expect("start fixture gateway daemon");
    let kubo_daemon = network
        .daemon_for_kubo_rpc(
            &kubo_state_root,
            ResourceBudget::default(),
            kubo_fixture.base_url().to_string(),
        )
        .expect("start fixture Kubo daemon");

    assert_socketless_fixture_plugin(&gateway_daemon, "direct-http");
    assert_socketless_fixture_plugin(&gateway_daemon, "ipfs-gateway");
    assert_socketless_fixture_plugin(&kubo_daemon, "direct-http");
    assert_socketless_fixture_plugin(&kubo_daemon, "ipfs-kubo-rpc");

    assert!(gateway_fixture.finish().is_empty());
    assert!(kubo_fixture.finish().is_empty());
    let _ = std::fs::remove_dir_all(gateway_state_root);
    let _ = std::fs::remove_dir_all(kubo_state_root);
}

fn expect_daemon_error(
    result: Result<BroadwebDaemon, BroadwebdError>,
    message: &str,
) -> BroadwebdError {
    match result {
        Ok(_) => panic!("{message}"),
        Err(error) => error,
    }
}

fn assert_socketless_fixture_plugin(daemon: &BroadwebDaemon, plugin_id: &str) {
    let status = daemon
        .health()
        .plugins
        .into_iter()
        .find(|status| status.metadata.id == plugin_id)
        .unwrap_or_else(|| panic!("missing fixture plugin {plugin_id}"));

    assert!(
        status
            .metadata
            .capabilities
            .iter()
            .any(|capability| capability == "socketless-fixture"),
        "{plugin_id} should advertise socketless fixture behavior"
    );
    assert!(
        status.metadata.privacy_boundary.contains("no sockets"),
        "{plugin_id} should report a socketless privacy boundary"
    );
}
