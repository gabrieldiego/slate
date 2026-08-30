#![forbid(unsafe_code)]

use slate_broadwebd::{
    BroadwebDaemon, BroadwebdClient, DiscoveredProfileSyncPeer, IpfsConfig, PluginRegistry,
    ProfileSyncObjectRequest, ProfileSyncPeerAdvertisement, ProfileSyncProfileRequest,
    ProfileSyncPutObjectRequest, ProfileSyncRequest, ProfileSyncResponse,
    ProfileSyncRootHealthRequest, ProfileSyncRootRequest, ProfileSyncRootUpdate,
    ProfileSyncRuntimeConfig, ResourceBudget, ServiceFrameCodec, TcpServiceFrameBroadwebdClient,
    default_session_status_reporter, discover_profile_sync_peers,
    respond_to_profile_sync_peer_solicit, serve_one_service_frame_request_over_stream,
};
use std::env;
use std::fs;
use std::io;
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream, UdpSocket};
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
        Some("discover") => run_discover(DiscoverArgs::parse(args)?),
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
    discovery_advertisement_file: Option<PathBuf>,
    discovery_membership_epoch: i64,
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
        let mut discovery_advertisement_file = None;
        let mut discovery_membership_epoch = 1;
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
                "--discovery-advertisement-file" => {
                    discovery_advertisement_file = Some(PathBuf::from(next_value(
                        &mut args,
                        "--discovery-advertisement-file",
                    )?))
                }
                "--discovery-membership-epoch" => {
                    discovery_membership_epoch = parse_i64(
                        &next_value(&mut args, "--discovery-membership-epoch")?,
                        "--discovery-membership-epoch",
                    )?
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
            discovery_advertisement_file,
            discovery_membership_epoch,
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
                "probe requires --connect with a literal ip:port endpoint or /ip4|/ip6/.../tcp/... multiaddr\n\n{}",
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
struct DiscoverArgs {
    discovery_target: String,
    network_id: String,
    node_id: String,
    timeout: Duration,
    require_signed_discovery: bool,
    advertisement_output: Option<PathBuf>,
    connect_output: Option<PathBuf>,
}

impl DiscoverArgs {
    fn parse(args: impl Iterator<Item = String>) -> Result<Self, String> {
        let mut discovery_target = None;
        let mut network_id = DEFAULT_DISCOVERY_NETWORK_ID.to_string();
        let mut node_id = format!("requester_{}", std::process::id());
        let mut timeout_ms = DEFAULT_DISCOVERY_TIMEOUT_MS;
        let mut require_signed_discovery = false;
        let mut advertisement_output = None;
        let mut connect_output = None;
        let mut args = args.peekable();

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--discovery-target" => {
                    discovery_target = Some(next_value(&mut args, "--discovery-target")?)
                }
                "--network-id" => network_id = next_value(&mut args, "--network-id")?,
                "--node-id" => node_id = next_value(&mut args, "--node-id")?,
                "--timeout-ms" => {
                    timeout_ms = parse_u64(&next_value(&mut args, "--timeout-ms")?, "--timeout-ms")?
                }
                "--require-signed-discovery" => require_signed_discovery = true,
                "--advertisement-output" => {
                    advertisement_output = Some(PathBuf::from(next_value(
                        &mut args,
                        "--advertisement-output",
                    )?))
                }
                "--connect-output" => {
                    connect_output = Some(PathBuf::from(next_value(&mut args, "--connect-output")?))
                }
                "-h" | "--help" => return Err(usage()),
                _ => return Err(format!("unknown discover argument: {arg}\n\n{}", usage())),
            }
        }

        let discovery_target = discovery_target.ok_or_else(|| {
            format!(
                "discover requires --discovery-target with a host:port endpoint\n\n{}",
                usage()
            )
        })?;

        Ok(Self {
            discovery_target,
            network_id,
            node_id,
            timeout: Duration::from_millis(timeout_ms),
            require_signed_discovery,
            advertisement_output,
            connect_output,
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
    require_signed_discovery: bool,
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
        let mut require_signed_discovery = false;
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
                "--require-signed-discovery" => require_signed_discovery = true,
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
            require_signed_discovery,
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
            server_discovery_advertisement(&args, local_addr)?,
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

fn run_discover(args: DiscoverArgs) -> Result<(), String> {
    let peers = discover_profile_sync_peers(
        args.discovery_target.as_str(),
        args.network_id.as_str(),
        args.node_id.as_str(),
        args.timeout,
        8,
    )
    .map_err(|error| format!("discover profile-sync peers: {error}"))?;
    let peer = select_discovered_peer(peers, args.require_signed_discovery)?;
    let connect_addr = peer
        .connect_addr()
        .map_err(|error| format!("resolve discovered peer connect address: {error}"))?;
    eprintln!(
        "DISCOVERED_PROFILE_SYNC_PEER node_id={} provider_id={} membership_epoch={} signed={} source={} connect={}",
        peer.advertisement.node_id,
        peer.advertisement.provider_id,
        peer.advertisement.membership_epoch,
        peer.advertisement.identity_signature.is_some(),
        peer.source_addr,
        connect_addr
    );
    write_discovered_peer_outputs(
        &peer,
        connect_addr.as_str(),
        args.advertisement_output.as_ref(),
        args.connect_output.as_ref(),
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
    let peer = select_discovered_peer(peers, args.require_signed_discovery)?;
    let connect_addr = peer
        .connect_addr()
        .map_err(|error| format!("resolve discovered peer connect address: {error}"))?;
    println!(
        "DISCOVERED_PROFILE_SYNC_PEER node_id={} provider_id={} membership_epoch={} signed={} source={} connect={}",
        peer.advertisement.node_id,
        peer.advertisement.provider_id,
        peer.advertisement.membership_epoch,
        peer.advertisement.identity_signature.is_some(),
        peer.source_addr,
        connect_addr
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

fn write_discovered_peer_outputs(
    peer: &DiscoveredProfileSyncPeer,
    connect_addr: &str,
    advertisement_output: Option<&PathBuf>,
    connect_output: Option<&PathBuf>,
) -> Result<(), String> {
    let mut advertisement_json = serde_json::to_vec_pretty(&peer.advertisement)
        .map_err(|error| format!("encode discovered advertisement JSON: {error}"))?;
    advertisement_json.push(b'\n');
    if let Some(path) = advertisement_output {
        fs::write(path, advertisement_json).map_err(|error| {
            format!("write discovered advertisement {}: {error}", path.display())
        })?;
    } else {
        println!(
            "{}",
            String::from_utf8(advertisement_json)
                .map_err(|error| format!("encode discovered advertisement as UTF-8: {error}"))?
        );
    }

    if let Some(path) = connect_output {
        fs::write(path, format!("{connect_addr}\n")).map_err(|error| {
            format!(
                "write discovered connect address {}: {error}",
                path.display()
            )
        })?;
    }
    Ok(())
}

fn select_discovered_peer(
    peers: Vec<DiscoveredProfileSyncPeer>,
    require_signed_discovery: bool,
) -> Result<DiscoveredProfileSyncPeer, String> {
    peers
        .into_iter()
        .filter(|peer| !require_signed_discovery || peer.advertisement.identity_signature.is_some())
        .next()
        .ok_or_else(|| {
            if require_signed_discovery {
                "discover-probe found no signed profile-sync peers".to_string()
            } else {
                "discover-probe found no profile-sync peers".to_string()
            }
        })
}

fn server_discovery_advertisement(
    args: &ServerArgs,
    local_addr: SocketAddr,
) -> Result<ProfileSyncPeerAdvertisement, String> {
    if let Some(path) = &args.discovery_advertisement_file {
        return load_discovery_advertisement(path);
    }

    ProfileSyncPeerAdvertisement::new(
        args.discovery_network_id.as_str(),
        args.discovery_node_id.as_str(),
        args.discovery_provider_id.as_str(),
        local_addr.to_string(),
        1,
    )
    .and_then(|advertisement| advertisement.with_membership_epoch(args.discovery_membership_epoch))
    .map_err(|error| format!("create peer discovery advertisement: {error}"))
}

fn load_discovery_advertisement(path: &PathBuf) -> Result<ProfileSyncPeerAdvertisement, String> {
    let text = fs::read_to_string(path)
        .map_err(|error| format!("read discovery advertisement {}: {error}", path.display()))?;
    let advertisement = serde_json::from_str::<ProfileSyncPeerAdvertisement>(text.as_str())
        .map_err(|error| {
            format!(
                "decode discovery advertisement {} as JSON: {error}",
                path.display()
            )
        })?;
    advertisement.validate().map_err(|error| {
        format!(
            "validate discovery advertisement {}: {error}",
            path.display()
        )
    })?;
    Ok(advertisement)
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

fn parse_i64(value: &str, name: &str) -> Result<i64, String> {
    value
        .parse::<i64>()
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
  slate-broadwebd-net-probe serve --state-root <dir> [--bind <addr:port>] [--ready-file <path>] [--max-requests <n>] [--frame-max-bytes <bytes>] [--runtime-profile-sync] [--discovery-bind <addr:port>] [--discovery-ready-file <path>] [--discovery-network <id>] [--discovery-node <id>] [--discovery-provider <id>] [--discovery-membership-epoch <n>] [--discovery-advertisement-file <path>] [--discovery-multicast <ipv4>]
  slate-broadwebd-net-probe probe --connect <ip:port|/ip4/.../tcp/...|/ip6/.../tcp/...> [--profile <profile>] [--root-id <root>] [--payload <text>] [--frame-max-bytes <bytes>]
  slate-broadwebd-net-probe discover --discovery-target <host:port> [--network-id <id>] [--node-id <id>] [--timeout-ms <ms>] [--require-signed-discovery] [--advertisement-output <path>] [--connect-output <path>]
  slate-broadwebd-net-probe discover-probe --discovery-target <host:port> [--network-id <id>] [--node-id <id>] [--profile <profile>] [--root-id <root>] [--payload <text>] [--frame-max-bytes <bytes>] [--timeout-ms <ms>] [--require-signed-discovery]"
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use slate_broadwebd::ProfileSyncPeerAdvertisementSignature;

    #[test]
    fn server_args_accept_signed_discovery_controls() {
        let args = ServerArgs::parse(
            [
                "--state-root",
                "/tmp/slate-probe-state",
                "--discovery-bind",
                "127.0.0.1:0",
                "--discovery-membership-epoch",
                "9",
                "--discovery-advertisement-file",
                "/tmp/slate-peer-advertisement.json",
            ]
            .into_iter()
            .map(str::to_string),
        )
        .expect("parse server args");

        assert_eq!(args.discovery_membership_epoch, 9);
        assert_eq!(
            args.discovery_advertisement_file.as_deref(),
            Some(std::path::Path::new("/tmp/slate-peer-advertisement.json"))
        );
    }

    #[test]
    fn server_discovery_advertisement_applies_generated_membership_epoch() {
        let args = ServerArgs::parse(
            [
                "--state-root",
                "/tmp/slate-probe-state",
                "--discovery-network",
                "signednet",
                "--discovery-node",
                "node-a",
                "--discovery-provider",
                "provider-a",
                "--discovery-membership-epoch",
                "4",
            ]
            .into_iter()
            .map(str::to_string),
        )
        .expect("parse server args");

        let advertisement =
            server_discovery_advertisement(&args, "127.0.0.1:9443".parse().unwrap())
                .expect("create discovery advertisement");

        assert_eq!(advertisement.network_id, "signednet");
        assert_eq!(advertisement.node_id, "node-a");
        assert_eq!(advertisement.provider_id, "provider-a");
        assert_eq!(advertisement.service_addr, "127.0.0.1:9443");
        assert_eq!(advertisement.membership_epoch, 4);
        assert!(advertisement.identity_signature.is_none());
    }

    #[test]
    fn server_discovery_advertisement_loads_signed_file_without_resigning() {
        let path = std::env::temp_dir().join(format!(
            "slate-broadwebd-net-probe-advertisement-{}.json",
            std::process::id()
        ));
        let signature =
            ProfileSyncPeerAdvertisementSignature::ed25519("node-signed", vec![7; 32], vec![8; 64])
                .expect("signature envelope");
        let advertisement = ProfileSyncPeerAdvertisement::new(
            "signednet",
            "node-signed",
            "provider-signed",
            "127.0.0.1:9553",
            12,
        )
        .expect("advertisement")
        .with_membership_epoch(5)
        .expect("membership epoch")
        .with_identity_signature(signature)
        .expect("signed advertisement envelope");
        fs::write(&path, serde_json::to_string(&advertisement).unwrap())
            .expect("write advertisement fixture");
        let args = ServerArgs::parse(
            [
                "--state-root",
                "/tmp/slate-probe-state",
                "--discovery-advertisement-file",
                path.to_str().unwrap(),
            ]
            .into_iter()
            .map(str::to_string),
        )
        .expect("parse server args");

        let loaded = server_discovery_advertisement(&args, "127.0.0.1:9443".parse().unwrap())
            .expect("load discovery advertisement");

        assert_eq!(loaded, advertisement);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn discover_probe_args_accept_require_signed_discovery() {
        let args = DiscoverProbeArgs::parse(
            [
                "--discovery-target",
                "127.0.0.1:47883",
                "--require-signed-discovery",
            ]
            .into_iter()
            .map(str::to_string),
        )
        .expect("parse discover-probe args");

        assert!(args.require_signed_discovery);
    }

    #[test]
    fn discover_args_accept_capture_outputs() {
        let args = DiscoverArgs::parse(
            [
                "--discovery-target",
                "127.0.0.1:47883",
                "--network-id",
                "manual-net",
                "--node-id",
                "local-node",
                "--timeout-ms",
                "125",
                "--require-signed-discovery",
                "--advertisement-output",
                "advertisement.json",
                "--connect-output",
                "connect.txt",
            ]
            .into_iter()
            .map(str::to_string),
        )
        .expect("parse discover args");

        assert_eq!(args.discovery_target, "127.0.0.1:47883");
        assert_eq!(args.network_id, "manual-net");
        assert_eq!(args.node_id, "local-node");
        assert_eq!(args.timeout, Duration::from_millis(125));
        assert!(args.require_signed_discovery);
        assert_eq!(
            args.advertisement_output.as_deref(),
            Some(std::path::Path::new("advertisement.json"))
        );
        assert_eq!(
            args.connect_output.as_deref(),
            Some(std::path::Path::new("connect.txt"))
        );
    }

    #[test]
    fn discover_probe_signed_selection_skips_unsigned_candidates() {
        let unsigned = DiscoveredProfileSyncPeer {
            advertisement: ProfileSyncPeerAdvertisement::new(
                "signednet",
                "node-unsigned",
                "provider-unsigned",
                "127.0.0.1:9551",
                1,
            )
            .expect("unsigned advertisement"),
            source_addr: "127.0.0.1:41000".parse().unwrap(),
        };
        let signature =
            ProfileSyncPeerAdvertisementSignature::ed25519("node-signed", vec![7; 32], vec![8; 64])
                .expect("signature envelope");
        let signed = DiscoveredProfileSyncPeer {
            advertisement: ProfileSyncPeerAdvertisement::new(
                "signednet",
                "node-signed",
                "provider-signed",
                "127.0.0.1:9552",
                2,
            )
            .expect("signed advertisement")
            .with_identity_signature(signature)
            .expect("signed advertisement envelope"),
            source_addr: "127.0.0.1:41001".parse().unwrap(),
        };

        let selected =
            select_discovered_peer(vec![unsigned, signed], true).expect("select signed peer");

        assert_eq!(selected.advertisement.node_id, "node-signed");
        assert!(selected.advertisement.identity_signature.is_some());
    }

    #[test]
    fn discover_writes_selected_advertisement_and_connect_addr() {
        let output_dir = std::env::current_dir()
            .unwrap()
            .join("target/tmp/broadwebd-net-probe-tests");
        fs::create_dir_all(&output_dir).expect("create output dir");
        let advertisement_output = output_dir.join(format!(
            "selected-advertisement-{}.json",
            std::process::id()
        ));
        let connect_output =
            output_dir.join(format!("selected-connect-{}.txt", std::process::id()));
        let peer = DiscoveredProfileSyncPeer {
            advertisement: ProfileSyncPeerAdvertisement::new(
                "signednet",
                "node-signed",
                "provider-signed",
                "127.0.0.1:9552",
                2,
            )
            .expect("advertisement"),
            source_addr: "127.0.0.1:41001".parse().unwrap(),
        };

        write_discovered_peer_outputs(
            &peer,
            "127.0.0.1:9552",
            Some(&advertisement_output),
            Some(&connect_output),
        )
        .expect("write discover outputs");

        let loaded = serde_json::from_str::<ProfileSyncPeerAdvertisement>(
            fs::read_to_string(&advertisement_output)
                .expect("read advertisement")
                .as_str(),
        )
        .expect("decode advertisement");
        let connect = fs::read_to_string(&connect_output).expect("read connect output");

        assert_eq!(loaded, peer.advertisement);
        assert_eq!(connect, "127.0.0.1:9552\n");

        let _ = fs::remove_file(advertisement_output);
        let _ = fs::remove_file(connect_output);
    }
}
