#![forbid(unsafe_code)]

use slate_broadwebd::{
    BroadwebDaemon, BroadwebdClient, IpfsConfig, PluginRegistry, ProfileSyncObjectRequest,
    ProfileSyncPeerAdvertisement, ProfileSyncProfileRequest, ProfileSyncPutObjectRequest,
    ProfileSyncRequest, ProfileSyncResponse, ProfileSyncRootHealthRequest, ProfileSyncRootRequest,
    ProfileSyncRootUpdate, ProfileSyncRuntimeConfig, ResourceBudget, ServiceFrameCodec,
    TcpServiceFrameBroadwebdClient, default_session_status_reporter, discover_profile_sync_peers,
    respond_to_profile_sync_peer_solicit, serve_one_service_frame_request_over_stream,
};
use std::env;
use std::fs;
use std::io;
use std::net::{Ipv4Addr, TcpListener, TcpStream, UdpSocket};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread;
use std::time::Duration;

const DEFAULT_FRAME_MAX_BYTES: usize = 1024 * 1024;
const DEFAULT_MAX_REQUESTS: usize = 16;
const DEFAULT_PROFILE: &str = "default";
const DEFAULT_ROOT_ID: &str = "settings/lan-smoke";
const DEFAULT_PAYLOAD: &str = "slate broadwebd LAN profile sync smoke";
const DEFAULT_DISCOVERY_NETWORK_ID: &str = "slate_lan_smoke";
const DEFAULT_DISCOVERY_PROVIDER_ID: &str = "local-preview-profile-sync";
const DEFAULT_DISCOVERY_TIMEOUT_MS: u64 = 2500;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        Some("serve") => run_server(ServerArgs::parse(args)?),
        Some("probe") => run_probe(ProbeArgs::parse(args)?),
        Some("discover-probe") => run_discover_probe(DiscoverProbeArgs::parse(args)?),
        _ => Err(usage()),
    }
}

#[derive(Debug)]
struct ServerArgs {
    bind: String,
    state_root: PathBuf,
    ready_file: Option<PathBuf>,
    max_requests: usize,
    frame_max_bytes: usize,
    discovery_bind: Option<String>,
    discovery_ready_file: Option<PathBuf>,
    discovery_network_id: String,
    discovery_node_id: String,
    discovery_provider_id: String,
    discovery_multicast: Option<Ipv4Addr>,
    runtime_profile_sync: bool,
}

impl ServerArgs {
    fn parse(args: impl Iterator<Item = String>) -> Result<Self, String> {
        let mut bind = "127.0.0.1:0".to_string();
        let mut state_root = None;
        let mut ready_file = None;
        let mut max_requests = DEFAULT_MAX_REQUESTS;
        let mut frame_max_bytes = DEFAULT_FRAME_MAX_BYTES;
        let mut discovery_bind = None;
        let mut discovery_ready_file = None;
        let mut discovery_network_id = DEFAULT_DISCOVERY_NETWORK_ID.to_string();
        let mut discovery_node_id = format!("probe_{}", std::process::id());
        let mut discovery_provider_id = DEFAULT_DISCOVERY_PROVIDER_ID.to_string();
        let mut discovery_multicast = None;
        let mut runtime_profile_sync = false;
        let mut args = args.peekable();

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--bind" => bind = next_value(&mut args, "--bind")?,
                "--state-root" => {
                    state_root = Some(PathBuf::from(next_value(&mut args, "--state-root")?))
                }
                "--ready-file" => {
                    ready_file = Some(PathBuf::from(next_value(&mut args, "--ready-file")?))
                }
                "--max-requests" => {
                    max_requests =
                        parse_usize(&next_value(&mut args, "--max-requests")?, "--max-requests")?
                }
                "--frame-max-bytes" => {
                    frame_max_bytes = parse_usize(
                        &next_value(&mut args, "--frame-max-bytes")?,
                        "--frame-max-bytes",
                    )?
                }
                "--discovery-bind" => {
                    discovery_bind = Some(next_value(&mut args, "--discovery-bind")?)
                }
                "--discovery-ready-file" => {
                    discovery_ready_file = Some(PathBuf::from(next_value(
                        &mut args,
                        "--discovery-ready-file",
                    )?))
                }
                "--discovery-network" => {
                    discovery_network_id = next_value(&mut args, "--discovery-network")?
                }
                "--discovery-node" => {
                    discovery_node_id = next_value(&mut args, "--discovery-node")?
                }
                "--discovery-provider" => {
                    discovery_provider_id = next_value(&mut args, "--discovery-provider")?
                }
                "--discovery-multicast" => {
                    let group = next_value(&mut args, "--discovery-multicast")?;
                    discovery_multicast = Some(
                        group
                            .parse()
                            .map_err(|error| format!("invalid multicast IPv4 address: {error}"))?,
                    );
                }
                "--runtime-profile-sync" => runtime_profile_sync = true,
                "-h" | "--help" => return Err(usage()),
                _ => return Err(format!("unknown serve argument: {arg}\n\n{}", usage())),
            }
        }

        let state_root = state_root.ok_or_else(|| {
            format!(
                "serve requires --state-root so test artifacts are explicit and removable\n\n{}",
                usage()
            )
        })?;

        Ok(Self {
            bind,
            state_root,
            ready_file,
            max_requests,
            frame_max_bytes,
            discovery_bind,
            discovery_ready_file,
            discovery_network_id,
            discovery_node_id,
            discovery_provider_id,
            discovery_multicast,
            runtime_profile_sync,
        })
    }
}

#[derive(Debug)]
struct ProbeArgs {
    connect: String,
    profile: String,
    root_id: String,
    payload: String,
    frame_max_bytes: usize,
}

impl ProbeArgs {
    fn parse(args: impl Iterator<Item = String>) -> Result<Self, String> {
        let mut connect = None;
        let mut profile = DEFAULT_PROFILE.to_string();
        let mut root_id = DEFAULT_ROOT_ID.to_string();
        let mut payload = DEFAULT_PAYLOAD.to_string();
        let mut frame_max_bytes = DEFAULT_FRAME_MAX_BYTES;
        let mut args = args.peekable();

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--connect" => connect = Some(next_value(&mut args, "--connect")?),
                "--profile" => profile = next_value(&mut args, "--profile")?,
                "--root-id" => root_id = next_value(&mut args, "--root-id")?,
                "--payload" => payload = next_value(&mut args, "--payload")?,
                "--frame-max-bytes" => {
                    frame_max_bytes = parse_usize(
                        &next_value(&mut args, "--frame-max-bytes")?,
                        "--frame-max-bytes",
                    )?
                }
                "-h" | "--help" => return Err(usage()),
                _ => return Err(format!("unknown probe argument: {arg}\n\n{}", usage())),
            }
        }

        let connect = connect.ok_or_else(|| {
            format!(
                "probe requires --connect with a host:port endpoint\n\n{}",
                usage()
            )
        })?;

        Ok(Self {
            connect,
            profile,
            root_id,
            payload,
            frame_max_bytes,
        })
    }
}

#[derive(Debug)]
struct DiscoverProbeArgs {
    discovery_target: String,
    network_id: String,
    node_id: String,
    profile: String,
    root_id: String,
    payload: String,
    frame_max_bytes: usize,
    timeout: Duration,
}

impl DiscoverProbeArgs {
    fn parse(args: impl Iterator<Item = String>) -> Result<Self, String> {
        let mut discovery_target = None;
        let mut network_id = DEFAULT_DISCOVERY_NETWORK_ID.to_string();
        let mut node_id = format!("requester_{}", std::process::id());
        let mut profile = DEFAULT_PROFILE.to_string();
        let mut root_id = DEFAULT_ROOT_ID.to_string();
        let mut payload = DEFAULT_PAYLOAD.to_string();
        let mut frame_max_bytes = DEFAULT_FRAME_MAX_BYTES;
        let mut timeout_ms = DEFAULT_DISCOVERY_TIMEOUT_MS;
        let mut args = args.peekable();

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--discovery-target" => {
                    discovery_target = Some(next_value(&mut args, "--discovery-target")?)
                }
                "--network-id" => network_id = next_value(&mut args, "--network-id")?,
                "--node-id" => node_id = next_value(&mut args, "--node-id")?,
                "--profile" => profile = next_value(&mut args, "--profile")?,
                "--root-id" => root_id = next_value(&mut args, "--root-id")?,
                "--payload" => payload = next_value(&mut args, "--payload")?,
                "--frame-max-bytes" => {
                    frame_max_bytes = parse_usize(
                        &next_value(&mut args, "--frame-max-bytes")?,
                        "--frame-max-bytes",
                    )?
                }
                "--timeout-ms" => {
                    timeout_ms = parse_u64(&next_value(&mut args, "--timeout-ms")?, "--timeout-ms")?
                }
                "-h" | "--help" => return Err(usage()),
                _ => {
                    return Err(format!(
                        "unknown discover-probe argument: {arg}\n\n{}",
                        usage()
                    ));
                }
            }
        }

        let discovery_target = discovery_target.ok_or_else(|| {
            format!(
                "discover-probe requires --discovery-target with a host:port endpoint\n\n{}",
                usage()
            )
        })?;

        Ok(Self {
            discovery_target,
            network_id,
            node_id,
            profile,
            root_id,
            payload,
            frame_max_bytes,
            timeout: Duration::from_millis(timeout_ms),
        })
    }
}

fn run_server(args: ServerArgs) -> Result<(), String> {
    let codec = ServiceFrameCodec::new(args.frame_max_bytes);
    let listener = TcpListener::bind(args.bind.as_str())
        .map_err(|error| format!("bind {}: {error}", args.bind))?;
    let local_addr = listener
        .local_addr()
        .map_err(|error| format!("read listener address: {error}"))?;
    let registry = probe_registry(args.runtime_profile_sync)?;
    let daemon =
        BroadwebDaemon::start_with_registry(&args.state_root, ResourceBudget::default(), registry)
            .map_err(|error| format!("start broadwebd probe daemon: {error}"))?;

    if let Some(ready_file) = &args.ready_file {
        fs::write(ready_file, local_addr.to_string())
            .map_err(|error| format!("write ready file {}: {error}", ready_file.display()))?;
    }
    println!("LISTEN_ADDR={local_addr}");

    let discovery_responder = if let Some(discovery_bind) = args.discovery_bind.as_deref() {
        Some(start_discovery_responder(
            discovery_bind,
            args.discovery_ready_file.as_ref(),
            args.discovery_multicast,
            ProfileSyncPeerAdvertisement::new(
                args.discovery_network_id,
                args.discovery_node_id,
                args.discovery_provider_id,
                local_addr.to_string(),
                1,
            )
            .map_err(|error| format!("create peer discovery advertisement: {error}"))?,
        )?)
    } else {
        None
    };

    for _ in 0..args.max_requests {
        let (mut stream, _) = listener
            .accept()
            .map_err(|error| format!("accept service-frame connection: {error}"))?;
        configure_stream(&stream)
            .map_err(|error| format!("configure service-frame connection: {error}"))?;
        handle_connection(&daemon, codec, &mut stream)?;
    }

    if let Some(responder) = discovery_responder {
        responder.stop_and_join()?;
    }

    Ok(())
}

struct DiscoveryResponder {
    stop: Arc<AtomicBool>,
    thread: thread::JoinHandle<Result<(), String>>,
}

impl DiscoveryResponder {
    fn stop_and_join(self) -> Result<(), String> {
        self.stop.store(true, Ordering::Relaxed);
        self.thread
            .join()
            .map_err(|_| "peer discovery responder panicked".to_string())?
    }
}

fn start_discovery_responder(
    bind_addr: &str,
    ready_file: Option<&PathBuf>,
    multicast_group: Option<Ipv4Addr>,
    advertisement: ProfileSyncPeerAdvertisement,
) -> Result<DiscoveryResponder, String> {
    let socket = UdpSocket::bind(bind_addr)
        .map_err(|error| format!("bind peer discovery UDP socket {bind_addr}: {error}"))?;
    if let Some(group) = multicast_group {
        socket
            .join_multicast_v4(&group, &Ipv4Addr::UNSPECIFIED)
            .map_err(|error| format!("join peer discovery multicast group {group}: {error}"))?;
    }
    socket
        .set_read_timeout(Some(Duration::from_millis(250)))
        .map_err(|error| format!("set peer discovery read timeout: {error}"))?;
    let local_addr = socket
        .local_addr()
        .map_err(|error| format!("read peer discovery socket addr: {error}"))?;
    if let Some(ready_file) = ready_file {
        fs::write(ready_file, local_addr.to_string()).map_err(|error| {
            format!(
                "write peer discovery ready file {}: {error}",
                ready_file.display()
            )
        })?;
    }
    println!("DISCOVERY_ADDR={local_addr}");

    let stop = Arc::new(AtomicBool::new(false));
    let thread_stop = Arc::clone(&stop);
    let thread = thread::spawn(move || {
        while !thread_stop.load(Ordering::Relaxed) {
            match respond_to_profile_sync_peer_solicit(&socket, &advertisement) {
                Ok(_) => {}
                Err(error) if is_udp_timeout_error(&error) => {}
                Err(error) => return Err(format!("peer discovery responder failed: {error}")),
            }
        }
        Ok(())
    });

    Ok(DiscoveryResponder { stop, thread })
}

fn handle_connection(
    daemon: &BroadwebDaemon,
    codec: ServiceFrameCodec,
    stream: &mut TcpStream,
) -> Result<(), String> {
    serve_one_service_frame_request_over_stream(codec, daemon, stream)
        .map_err(|error| format!("handle service-frame request: {error}"))
}

fn run_probe(args: ProbeArgs) -> Result<(), String> {
    let codec = ServiceFrameCodec::new(args.frame_max_bytes);
    let client = TcpServiceFrameBroadwebdClient::with_codec(args.connect.clone(), codec)
        .with_timeout(Duration::from_secs(12));
    run_profile_sync_probe(
        &client,
        &args.profile,
        &args.root_id,
        args.payload.into_bytes(),
    )
}

fn run_discover_probe(args: DiscoverProbeArgs) -> Result<(), String> {
    let peers = discover_profile_sync_peers(
        args.discovery_target.as_str(),
        args.network_id.as_str(),
        args.node_id.as_str(),
        args.timeout,
        8,
    )
    .map_err(|error| format!("discover profile-sync peers: {error}"))?;
    let peer = peers
        .first()
        .ok_or_else(|| "discover-probe found no profile-sync peers".to_string())?;
    let connect_addr = peer
        .connect_addr()
        .map_err(|error| format!("resolve discovered peer connect address: {error}"))?;
    println!(
        "DISCOVERED_PROFILE_SYNC_PEER node_id={} provider_id={} source={} connect={}",
        peer.advertisement.node_id, peer.advertisement.provider_id, peer.source_addr, connect_addr
    );
    let client = TcpServiceFrameBroadwebdClient::with_codec(
        connect_addr,
        ServiceFrameCodec::new(args.frame_max_bytes),
    )
    .with_timeout(Duration::from_secs(12));
    run_profile_sync_probe(
        &client,
        &args.profile,
        &args.root_id,
        args.payload.into_bytes(),
    )
}

fn run_profile_sync_probe(
    client: &TcpServiceFrameBroadwebdClient,
    profile: &str,
    root_id: &str,
    payload: Vec<u8>,
) -> Result<(), String> {
    let providers = discover_providers(client, profile)?;
    if providers == 0 {
        return Err("profile-sync probe discovered no providers".to_string());
    }

    let object_id = put_object(client, profile, payload.as_slice())?;
    publish_root(client, profile, root_id, object_id.as_str())?;
    let resolved_object_id = resolve_root(client, profile, root_id)?
        .ok_or_else(|| "profile-sync probe root did not resolve after publish".to_string())?;
    if resolved_object_id != object_id {
        return Err(format!(
            "profile-sync probe resolved {resolved_object_id}, expected {object_id}"
        ));
    }

    let fetched = get_object(client, profile, object_id.as_str())?;
    if fetched != payload {
        return Err(
            "profile-sync probe fetched payload does not match published payload".to_string(),
        );
    }

    retain_object(client, profile, object_id.as_str())?;
    verify_retained_object(client, profile, object_id.as_str())?;
    verify_root_health(client, profile, root_id)?;

    println!(
        "PROFILE_SYNC_LAN_PROBE_OK providers={providers} object_id={object_id} payload_bytes={}",
        fetched.len()
    );
    Ok(())
}

fn discover_providers(client: &dyn BroadwebdClient, profile: &str) -> Result<usize, String> {
    match profile_sync(
        client,
        ProfileSyncRequest::DiscoverProviders(ProfileSyncProfileRequest::new(profile)),
    )? {
        ProfileSyncResponse::Providers { providers } => Ok(providers.len()),
        response => Err(format!(
            "profile-sync probe expected Providers response, got {response:?}"
        )),
    }
}

fn put_object(client: &dyn BroadwebdClient, profile: &str, bytes: &[u8]) -> Result<String, String> {
    match profile_sync(
        client,
        ProfileSyncRequest::PutEncryptedObject(ProfileSyncPutObjectRequest::new(
            profile,
            bytes.to_vec(),
        )),
    )? {
        ProfileSyncResponse::PutEncryptedObject { object_id } => Ok(object_id),
        response => Err(format!(
            "profile-sync probe expected PutEncryptedObject response, got {response:?}"
        )),
    }
}

fn publish_root(
    client: &dyn BroadwebdClient,
    profile: &str,
    root_id: &str,
    object_id: &str,
) -> Result<(), String> {
    match profile_sync(
        client,
        ProfileSyncRequest::PublishRoot(ProfileSyncRootUpdate::new(profile, root_id, object_id)),
    )? {
        ProfileSyncResponse::Root {
            object_id: Some(published_object_id),
            ..
        } if published_object_id == object_id => Ok(()),
        response => Err(format!(
            "profile-sync probe expected published Root response, got {response:?}"
        )),
    }
}

fn resolve_root(
    client: &dyn BroadwebdClient,
    profile: &str,
    root_id: &str,
) -> Result<Option<String>, String> {
    match profile_sync(
        client,
        ProfileSyncRequest::ResolveRoot(ProfileSyncRootRequest::new(profile, root_id)),
    )? {
        ProfileSyncResponse::Root { object_id, .. } => Ok(object_id),
        response => Err(format!(
            "profile-sync probe expected Root response, got {response:?}"
        )),
    }
}

fn get_object(
    client: &dyn BroadwebdClient,
    profile: &str,
    object_id: &str,
) -> Result<Vec<u8>, String> {
    match profile_sync(
        client,
        ProfileSyncRequest::GetEncryptedObject(ProfileSyncObjectRequest::new(profile, object_id)),
    )? {
        ProfileSyncResponse::GetEncryptedObject { bytes, .. } => Ok(bytes),
        response => Err(format!(
            "profile-sync probe expected GetEncryptedObject response, got {response:?}"
        )),
    }
}

fn retain_object(
    client: &dyn BroadwebdClient,
    profile: &str,
    object_id: &str,
) -> Result<(), String> {
    match profile_sync(
        client,
        ProfileSyncRequest::RetainObject(ProfileSyncObjectRequest::new(profile, object_id)),
    )? {
        ProfileSyncResponse::RetainObject { retained: true, .. } => Ok(()),
        response => Err(format!(
            "profile-sync probe expected retained object response, got {response:?}"
        )),
    }
}

fn verify_retained_object(
    client: &dyn BroadwebdClient,
    profile: &str,
    object_id: &str,
) -> Result<(), String> {
    match profile_sync(
        client,
        ProfileSyncRequest::VerifyRetainedObject(ProfileSyncObjectRequest::new(profile, object_id)),
    )? {
        ProfileSyncResponse::RetainedObjectStatus {
            retained: true,
            available: true,
            ..
        } => Ok(()),
        response => Err(format!(
            "profile-sync probe expected retained and available object, got {response:?}"
        )),
    }
}

fn verify_root_health(
    client: &dyn BroadwebdClient,
    profile: &str,
    root_id: &str,
) -> Result<(), String> {
    match profile_sync(
        client,
        ProfileSyncRequest::RootHealth(ProfileSyncRootHealthRequest::new(profile, root_id)),
    )? {
        ProfileSyncResponse::RootHealth { health }
            if !health.degraded && health.latest_object_available =>
        {
            Ok(())
        }
        response => Err(format!(
            "profile-sync probe expected healthy root response, got {response:?}"
        )),
    }
}

fn profile_sync(
    client: &dyn BroadwebdClient,
    request: ProfileSyncRequest,
) -> Result<ProfileSyncResponse, String> {
    client
        .profile_sync(request)
        .map_err(|error| format!("profile-sync request failed: {error}"))
}

fn configure_stream(stream: &TcpStream) -> io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(12)))?;
    stream.set_write_timeout(Some(Duration::from_secs(12)))?;
    Ok(())
}

fn probe_registry(runtime_profile_sync: bool) -> Result<PluginRegistry, String> {
    if !runtime_profile_sync {
        return Ok(PluginRegistry::with_default_http());
    }

    PluginRegistry::with_default_http_and_runtime_profile_sync_config(
        IpfsConfig::from_environment()
            .map_err(|error| format!("read IPFS runtime config: {error}"))?,
        default_session_status_reporter(),
        ProfileSyncRuntimeConfig::from_environment()
            .map_err(|error| format!("read profile-sync runtime config: {error}"))?,
    )
    .map_err(|error| format!("build runtime profile-sync registry: {error}"))
}

fn next_value(args: &mut impl Iterator<Item = String>, name: &str) -> Result<String, String> {
    args.next()
        .ok_or_else(|| format!("{name} requires a value\n\n{}", usage()))
}

fn parse_usize(value: &str, name: &str) -> Result<usize, String> {
    value
        .parse::<usize>()
        .map_err(|error| format!("invalid {name} value {value:?}: {error}"))
}

fn parse_u64(value: &str, name: &str) -> Result<u64, String> {
    value
        .parse::<u64>()
        .map_err(|error| format!("invalid {name} value {value:?}: {error}"))
}

fn is_udp_timeout_error(error: &slate_broadwebd::BroadwebdError) -> bool {
    let message = error.to_string();
    message.contains("timed out")
        || message.contains("would block")
        || message.contains("Resource temporarily unavailable")
}

fn usage() -> String {
    "usage:
  slate-broadwebd-net-probe serve --state-root <dir> [--bind <addr:port>] [--ready-file <path>] [--max-requests <n>] [--frame-max-bytes <bytes>] [--runtime-profile-sync] [--discovery-bind <addr:port>] [--discovery-ready-file <path>] [--discovery-network <id>] [--discovery-node <id>] [--discovery-provider <id>] [--discovery-multicast <ipv4>]
  slate-broadwebd-net-probe probe --connect <host:port> [--profile <profile>] [--root-id <root>] [--payload <text>] [--frame-max-bytes <bytes>]
  slate-broadwebd-net-probe discover-probe --discovery-target <host:port> [--network-id <id>] [--node-id <id>] [--profile <profile>] [--root-id <root>] [--payload <text>] [--frame-max-bytes <bytes>] [--timeout-ms <ms>]"
        .to_string()
}
