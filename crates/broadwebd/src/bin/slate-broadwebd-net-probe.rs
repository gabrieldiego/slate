#![forbid(unsafe_code)]

use slate_broadwebd::{
    BroadwebDaemon, BroadwebdClient, PluginRegistry, ProfileSyncObjectRequest,
    ProfileSyncProfileRequest, ProfileSyncPutObjectRequest, ProfileSyncRequest,
    ProfileSyncResponse, ProfileSyncRootHealthRequest, ProfileSyncRootRequest,
    ProfileSyncRootUpdate, ResourceBudget, ServiceFrameCodec, TcpServiceFrameBroadwebdClient,
};
use std::env;
use std::fs;
use std::io;
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

const DEFAULT_FRAME_MAX_BYTES: usize = 1024 * 1024;
const DEFAULT_MAX_REQUESTS: usize = 16;
const DEFAULT_PROFILE: &str = "default";
const DEFAULT_ROOT_ID: &str = "settings/lan-smoke";
const DEFAULT_PAYLOAD: &str = "slate broadwebd LAN profile sync smoke";

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
}

impl ServerArgs {
    fn parse(args: impl Iterator<Item = String>) -> Result<Self, String> {
        let mut bind = "127.0.0.1:0".to_string();
        let mut state_root = None;
        let mut ready_file = None;
        let mut max_requests = DEFAULT_MAX_REQUESTS;
        let mut frame_max_bytes = DEFAULT_FRAME_MAX_BYTES;
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

fn run_server(args: ServerArgs) -> Result<(), String> {
    let codec = ServiceFrameCodec::new(args.frame_max_bytes);
    let listener = TcpListener::bind(args.bind.as_str())
        .map_err(|error| format!("bind {}: {error}", args.bind))?;
    let local_addr = listener
        .local_addr()
        .map_err(|error| format!("read listener address: {error}"))?;
    let daemon = BroadwebDaemon::start_with_registry(
        &args.state_root,
        ResourceBudget::default(),
        PluginRegistry::with_default_http(),
    )
    .map_err(|error| format!("start broadwebd probe daemon: {error}"))?;

    if let Some(ready_file) = &args.ready_file {
        fs::write(ready_file, local_addr.to_string())
            .map_err(|error| format!("write ready file {}: {error}", ready_file.display()))?;
    }
    println!("LISTEN_ADDR={local_addr}");

    for _ in 0..args.max_requests {
        let (mut stream, _) = listener
            .accept()
            .map_err(|error| format!("accept service-frame connection: {error}"))?;
        configure_stream(&stream)
            .map_err(|error| format!("configure service-frame connection: {error}"))?;
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
        .map_err(|error| format!("read service request: {error}"))?;
    let response = daemon
        .dispatch_service_request(request)
        .map_err(|error| format!("dispatch service request: {error}"))?;
    codec
        .write_response(stream, &response)
        .map_err(|error| format!("write service response: {error}"))
}

fn run_probe(args: ProbeArgs) -> Result<(), String> {
    let codec = ServiceFrameCodec::new(args.frame_max_bytes);
    let client = TcpServiceFrameBroadwebdClient::with_codec(args.connect.clone(), codec)
        .with_timeout(Duration::from_secs(12));

    let providers = discover_providers(&client, args.profile.as_str())?;
    if providers == 0 {
        return Err("profile-sync probe discovered no providers".to_string());
    }

    let payload = args.payload.into_bytes();
    let object_id = put_object(&client, args.profile.as_str(), payload.as_slice())?;
    publish_root(
        &client,
        args.profile.as_str(),
        args.root_id.as_str(),
        object_id.as_str(),
    )?;
    let resolved_object_id =
        resolve_root(&client, args.profile.as_str(), args.root_id.as_str())?
            .ok_or_else(|| "profile-sync probe root did not resolve after publish".to_string())?;
    if resolved_object_id != object_id {
        return Err(format!(
            "profile-sync probe resolved {resolved_object_id}, expected {object_id}"
        ));
    }

    let fetched = get_object(&client, args.profile.as_str(), object_id.as_str())?;
    if fetched != payload {
        return Err(
            "profile-sync probe fetched payload does not match published payload".to_string(),
        );
    }

    retain_object(&client, args.profile.as_str(), object_id.as_str())?;
    verify_retained_object(&client, args.profile.as_str(), object_id.as_str())?;
    verify_root_health(&client, args.profile.as_str(), args.root_id.as_str())?;

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

fn next_value(args: &mut impl Iterator<Item = String>, name: &str) -> Result<String, String> {
    args.next()
        .ok_or_else(|| format!("{name} requires a value\n\n{}", usage()))
}

fn parse_usize(value: &str, name: &str) -> Result<usize, String> {
    value
        .parse::<usize>()
        .map_err(|error| format!("invalid {name} value {value:?}: {error}"))
}

fn usage() -> String {
    "usage:
  slate-broadwebd-net-probe serve --state-root <dir> [--bind <addr:port>] [--ready-file <path>] [--max-requests <n>] [--frame-max-bytes <bytes>]
  slate-broadwebd-net-probe probe --connect <host:port> [--profile <profile>] [--root-id <root>] [--payload <text>] [--frame-max-bytes <bytes>]"
        .to_string()
}
