#![forbid(unsafe_code)]

use slate_broadwebd::{
    BroadwebDaemon, BroadwebdError, PluginRegistry, ProfileSyncObjectRequest,
    ProfileSyncPeerAdvertisement, ProfileSyncProfileRequest, ProfileSyncPutObjectRequest,
    ProfileSyncRequest, ProfileSyncResponse, ProfileSyncRootHealthRequest, ProfileSyncRootRequest,
    ProfileSyncRootUpdate, ResourceBudget, ServiceFrameCodec, TcpServiceFrameBroadwebdClient,
    discover_profile_sync_peers, respond_to_profile_sync_peer_solicit,
};
use std::env;
use std::fs;
use std::net::{TcpListener, TcpStream, UdpSocket};
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const PROFILE: &str = "default";
const ROOT_ID: &str = "settings/local-socket-discovery";
const NETWORK_ID: &str = "local_socket_fixture";
const PAYLOAD: &[u8] = b"local socket profile sync peer discovery";

#[test]
fn local_socket_peer_discovery_finds_service_frame_profile_sync_peer() {
    if env::var("SLATE_LOCAL_SOCKET_TESTS").ok().as_deref() != Some("1") {
        eprintln!("skipped: set SLATE_LOCAL_SOCKET_TESTS=1 to run local socket peer discovery");
        return;
    }

    let state_root = test_state_root("profile-sync-peer-discovery");
    let daemon = BroadwebDaemon::start_with_registry(
        &state_root,
        ResourceBudget::default(),
        PluginRegistry::with_default_http(),
    )
    .expect("start local socket fixture broadwebd");
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind local TCP service frame server");
    let service_addr = listener.local_addr().expect("read local TCP server addr");
    let tcp_server = thread::spawn(move || run_tcp_service_frame_server(listener, daemon, 8));

    let discovery_socket =
        UdpSocket::bind("127.0.0.1:0").expect("bind local UDP discovery responder");
    discovery_socket
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("set UDP discovery responder timeout");
    let discovery_addr = discovery_socket
        .local_addr()
        .expect("read UDP discovery responder addr");
    let advertisement = ProfileSyncPeerAdvertisement::new(
        NETWORK_ID,
        "fixture-node",
        "local-preview-profile-sync",
        format!("0.0.0.0:{}", service_addr.port()),
        1,
    )
    .expect("create local peer advertisement");
    let udp_responder = thread::spawn(move || {
        respond_to_profile_sync_peer_solicit(&discovery_socket, &advertisement)
            .map(|reply| reply.is_some())
            .map_err(|error| error.to_string())
    });

    let peers = discover_profile_sync_peers(
        discovery_addr,
        NETWORK_ID,
        "fixture-requester",
        Duration::from_secs(2),
        4,
    )
    .expect("discover local profile-sync peer");
    assert_eq!(peers.len(), 1);
    assert_eq!(peers[0].advertisement.node_id, "fixture-node");

    let connect_addr = peers[0].connect_addr().expect("peer connect addr");
    let client = TcpServiceFrameBroadwebdClient::with_codec(
        connect_addr,
        ServiceFrameCodec::new(1024 * 1024),
    )
    .with_timeout(Duration::from_secs(2));
    run_profile_sync_smoke(&client).expect("profile sync smoke through discovered peer");

    assert!(
        udp_responder
            .join()
            .expect("UDP discovery responder should not panic")
            .expect("UDP discovery responder should succeed")
    );
    tcp_server
        .join()
        .expect("TCP service frame server should not panic")
        .expect("TCP service frame server should succeed");
    let _ = fs::remove_dir_all(state_root);
}

fn run_tcp_service_frame_server(
    listener: TcpListener,
    daemon: BroadwebDaemon,
    max_requests: usize,
) -> Result<(), String> {
    let codec = ServiceFrameCodec::new(1024 * 1024);
    for _ in 0..max_requests {
        let (mut stream, _) = listener
            .accept()
            .map_err(|error| format!("accept local service-frame connection: {error}"))?;
        configure_stream(&stream)
            .map_err(|error| format!("configure local service-frame connection: {error}"))?;
        handle_connection(&daemon, codec, &mut stream)?;
    }
    Ok(())
}

fn handle_connection(
    daemon: &BroadwebDaemon,
    codec: ServiceFrameCodec,
    stream: &mut TcpStream,
) -> Result<(), String> {
    let request = codec
        .read_request(stream)
        .map_err(|error| format!("read local service request: {error}"))?;
    let response = daemon
        .dispatch_service_request(request)
        .map_err(|error| format!("dispatch local service request: {error}"))?;
    codec
        .write_response(stream, &response)
        .map_err(|error| format!("write local service response: {error}"))
}

fn run_profile_sync_smoke(client: &TcpServiceFrameBroadwebdClient) -> Result<(), String> {
    let providers = discover_providers(client)?;
    if providers == 0 {
        return Err("local socket discovery smoke found no providers".to_string());
    }

    let object_id = put_object(client, PAYLOAD)?;
    publish_root(client, object_id.as_str())?;
    let resolved_object_id = resolve_root(client)?
        .ok_or_else(|| "local socket discovery smoke root did not resolve".to_string())?;
    if resolved_object_id != object_id {
        return Err(format!(
            "local socket discovery smoke resolved {resolved_object_id}, expected {object_id}"
        ));
    }

    let fetched = get_object(client, object_id.as_str())?;
    if fetched != PAYLOAD {
        return Err("local socket discovery smoke fetched payload mismatch".to_string());
    }

    retain_object(client, object_id.as_str())?;
    verify_retained_object(client, object_id.as_str())?;
    verify_root_health(client)?;
    Ok(())
}

fn discover_providers(client: &TcpServiceFrameBroadwebdClient) -> Result<usize, String> {
    match profile_sync(
        client,
        ProfileSyncRequest::DiscoverProviders(ProfileSyncProfileRequest::new(PROFILE)),
    )? {
        ProfileSyncResponse::Providers { providers } => Ok(providers.len()),
        response => Err(format!("expected Providers response, got {response:?}")),
    }
}

fn put_object(client: &TcpServiceFrameBroadwebdClient, bytes: &[u8]) -> Result<String, String> {
    match profile_sync(
        client,
        ProfileSyncRequest::PutEncryptedObject(ProfileSyncPutObjectRequest::new(
            PROFILE,
            bytes.to_vec(),
        )),
    )? {
        ProfileSyncResponse::PutEncryptedObject { object_id } => Ok(object_id),
        response => Err(format!(
            "expected PutEncryptedObject response, got {response:?}"
        )),
    }
}

fn publish_root(client: &TcpServiceFrameBroadwebdClient, object_id: &str) -> Result<(), String> {
    match profile_sync(
        client,
        ProfileSyncRequest::PublishRoot(ProfileSyncRootUpdate::new(PROFILE, ROOT_ID, object_id)),
    )? {
        ProfileSyncResponse::Root {
            object_id: Some(published_object_id),
            ..
        } if published_object_id == object_id => Ok(()),
        response => Err(format!(
            "expected published Root response, got {response:?}"
        )),
    }
}

fn resolve_root(client: &TcpServiceFrameBroadwebdClient) -> Result<Option<String>, String> {
    match profile_sync(
        client,
        ProfileSyncRequest::ResolveRoot(ProfileSyncRootRequest::new(PROFILE, ROOT_ID)),
    )? {
        ProfileSyncResponse::Root { object_id, .. } => Ok(object_id),
        response => Err(format!("expected Root response, got {response:?}")),
    }
}

fn get_object(client: &TcpServiceFrameBroadwebdClient, object_id: &str) -> Result<Vec<u8>, String> {
    match profile_sync(
        client,
        ProfileSyncRequest::GetEncryptedObject(ProfileSyncObjectRequest::new(PROFILE, object_id)),
    )? {
        ProfileSyncResponse::GetEncryptedObject { bytes, .. } => Ok(bytes),
        response => Err(format!(
            "expected GetEncryptedObject response, got {response:?}"
        )),
    }
}

fn retain_object(client: &TcpServiceFrameBroadwebdClient, object_id: &str) -> Result<(), String> {
    match profile_sync(
        client,
        ProfileSyncRequest::RetainObject(ProfileSyncObjectRequest::new(PROFILE, object_id)),
    )? {
        ProfileSyncResponse::RetainObject { retained: true, .. } => Ok(()),
        response => Err(format!("expected RetainObject response, got {response:?}")),
    }
}

fn verify_retained_object(
    client: &TcpServiceFrameBroadwebdClient,
    object_id: &str,
) -> Result<(), String> {
    match profile_sync(
        client,
        ProfileSyncRequest::VerifyRetainedObject(ProfileSyncObjectRequest::new(PROFILE, object_id)),
    )? {
        ProfileSyncResponse::RetainedObjectStatus {
            retained: true,
            available: true,
            ..
        } => Ok(()),
        response => Err(format!("expected retained object status, got {response:?}")),
    }
}

fn verify_root_health(client: &TcpServiceFrameBroadwebdClient) -> Result<(), String> {
    match profile_sync(
        client,
        ProfileSyncRequest::RootHealth(ProfileSyncRootHealthRequest::new(PROFILE, ROOT_ID)),
    )? {
        ProfileSyncResponse::RootHealth { health }
            if !health.degraded && health.latest_object_available =>
        {
            Ok(())
        }
        response => Err(format!("expected healthy root response, got {response:?}")),
    }
}

fn profile_sync(
    client: &TcpServiceFrameBroadwebdClient,
    request: ProfileSyncRequest,
) -> Result<ProfileSyncResponse, String> {
    slate_broadwebd::BroadwebdClient::profile_sync(client, request)
        .map_err(|error| format!("profile-sync request failed: {error}"))
}

fn configure_stream(stream: &TcpStream) -> Result<(), BroadwebdError> {
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    Ok(())
}

fn test_state_root(name: &str) -> PathBuf {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time after UNIX_EPOCH")
        .as_nanos();
    env::temp_dir().join(format!(
        "slate-broadwebd-{name}-{}-{now}",
        std::process::id()
    ))
}
