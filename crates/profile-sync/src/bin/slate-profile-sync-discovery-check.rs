#![forbid(unsafe_code)]

use std::env;
use std::ffi::OsString;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use serde_json::json;
use slate_broadwebd::{
    DEFAULT_PROFILE_SYNC_DISCOVERY_NAMESPACE, PROFILE_SYNC_DISCOVERY_PROTOCOL_IPNS,
    PROFILE_SYNC_DISCOVERY_PROTOCOL_IROH_RENDEZVOUS,
    PROFILE_SYNC_DISCOVERY_PROTOCOL_LIBP2P_KADEMLIA,
    PROFILE_SYNC_DISCOVERY_PROTOCOL_LIBP2P_RENDEZVOUS,
    PROFILE_SYNC_DISCOVERY_PROTOCOL_LOCAL_SIMULATION, ProfileSyncPeerAdvertisement,
    ProfileSyncPeerDiscoveryProtocol, ProfileSyncPeerDiscoveryResult,
    validate_profile_sync_peer_discovery_capability,
};
use slate_profile_sync::{
    ProfileSyncPeerDiscoveryTrustRejection, RejectedProfileSyncPeerDiscoveryCandidate,
    TrustedProfileSyncPeerDiscoveryReport, filter_trusted_profile_sync_peer_discovery_results,
};
use slate_storage::{DEFAULT_PROFILE_ID, SlateProfileDatabase};

const USAGE: &str = "\
Check Slate profile-sync discovery advertisements against local profile trust.

Usage:
  slate-profile-sync-discovery-check --settings-db <path> --network-id <id> --advertisement-file <path>... [--profile <profile>] [--local-device-id <id>] [--protocol <name>] [--namespace <name>] [--require-capability <name>...] [--require-trusted] [--output <path>]
";

fn main() -> ExitCode {
    match run(env::args_os().skip(1)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: impl IntoIterator<Item = OsString>) -> Result<(), String> {
    let args = DiscoveryCheckArgs::parse(args)?;
    let report = trusted_discovery_report_from_advertisement_files(&args)?;
    if args.require_trusted && !report.has_trusted_peer() {
        return Err("profile-sync discovery check found no trusted peers".to_string());
    }

    let mut bytes = serde_json::to_vec_pretty(&discovery_check_report_json(
        &args.profile,
        &args.network_id,
        args.required_capabilities.as_slice(),
        &report,
    ))
    .map_err(|error| format!("encode discovery check report JSON: {error}"))?;
    bytes.push(b'\n');
    if let Some(output) = &args.output {
        fs::write(output, bytes)
            .map_err(|error| format!("write discovery check report {}: {error}", output.display()))
    } else {
        let mut stdout = io::stdout().lock();
        stdout
            .write_all(bytes.as_slice())
            .and_then(|_| stdout.flush())
            .map_err(|error| format!("write discovery check report to stdout: {error}"))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct DiscoveryCheckArgs {
    settings_db: PathBuf,
    profile: String,
    local_device_id: Option<String>,
    network_id: String,
    protocol: ProfileSyncPeerDiscoveryProtocol,
    namespace: String,
    advertisement_files: Vec<PathBuf>,
    required_capabilities: Vec<String>,
    require_trusted: bool,
    output: Option<PathBuf>,
}

impl DiscoveryCheckArgs {
    fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Self, String> {
        let mut parsed = ParsedDiscoveryCheckArgs::default();
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            let arg = os_string_to_string(arg, "argument")?;
            match arg.as_str() {
                "--help" | "-h" => return Err(USAGE.to_string()),
                "--settings-db" => {
                    parsed.settings_db = Some(next_path(&mut args, "--settings-db")?)
                }
                "--profile" => parsed.profile = Some(next_string(&mut args, "--profile")?),
                "--local-device-id" => {
                    parsed.local_device_id = Some(next_string(&mut args, "--local-device-id")?)
                }
                "--network-id" => parsed.network_id = Some(next_string(&mut args, "--network-id")?),
                "--protocol" => {
                    parsed.protocol = Some(parse_protocol(
                        next_string(&mut args, "--protocol")?.as_str(),
                    )?)
                }
                "--namespace" => parsed.namespace = Some(next_string(&mut args, "--namespace")?),
                "--advertisement-file" => parsed
                    .advertisement_files
                    .push(next_path(&mut args, "--advertisement-file")?),
                "--require-capability" => parsed
                    .required_capabilities
                    .push(next_string(&mut args, "--require-capability")?),
                "--require-trusted" => parsed.require_trusted = true,
                "--output" => parsed.output = Some(next_path(&mut args, "--output")?),
                _ => return Err(format!("unknown argument: {arg}\n\n{USAGE}")),
            }
        }

        if parsed.advertisement_files.is_empty() {
            return Err(format!("missing required --advertisement-file\n\n{USAGE}"));
        }
        for capability in &parsed.required_capabilities {
            validate_profile_sync_peer_discovery_capability(capability.as_str()).map_err(
                |error| format!("invalid required profile-sync discovery capability: {error}"),
            )?;
        }

        Ok(Self {
            settings_db: parsed
                .settings_db
                .ok_or_else(|| format!("missing required --settings-db\n\n{USAGE}"))?,
            profile: parsed
                .profile
                .unwrap_or_else(|| DEFAULT_PROFILE_ID.to_string()),
            local_device_id: parsed.local_device_id,
            network_id: parsed
                .network_id
                .ok_or_else(|| format!("missing required --network-id\n\n{USAGE}"))?,
            protocol: parsed
                .protocol
                .unwrap_or(ProfileSyncPeerDiscoveryProtocol::LocalSimulation),
            namespace: parsed
                .namespace
                .unwrap_or_else(|| DEFAULT_PROFILE_SYNC_DISCOVERY_NAMESPACE.to_string()),
            advertisement_files: parsed.advertisement_files,
            required_capabilities: parsed.required_capabilities,
            require_trusted: parsed.require_trusted,
            output: parsed.output,
        })
    }
}

#[derive(Debug, Default)]
struct ParsedDiscoveryCheckArgs {
    settings_db: Option<PathBuf>,
    profile: Option<String>,
    local_device_id: Option<String>,
    network_id: Option<String>,
    protocol: Option<ProfileSyncPeerDiscoveryProtocol>,
    namespace: Option<String>,
    advertisement_files: Vec<PathBuf>,
    required_capabilities: Vec<String>,
    require_trusted: bool,
    output: Option<PathBuf>,
}

fn trusted_discovery_report_from_advertisement_files(
    args: &DiscoveryCheckArgs,
) -> Result<TrustedProfileSyncPeerDiscoveryReport, String> {
    let database = if let Some(local_device_id) = &args.local_device_id {
        SlateProfileDatabase::open_resolved_with_device_id(
            args.settings_db.clone(),
            local_device_id.as_str(),
        )
    } else {
        SlateProfileDatabase::open_resolved_with_persistent_device_id(args.settings_db.clone())
    }
    .map_err(|error| {
        format!(
            "open settings database {}: {error}",
            args.settings_db.display()
        )
    })?;

    let candidates = advertisement_file_candidates(args)?;
    let report = filter_trusted_profile_sync_peer_discovery_results(
        &database,
        args.profile.as_str(),
        args.network_id.as_str(),
        candidates,
    )
    .map_err(|error| format!("check discovery candidates against local trust: {error}"))?;
    Ok(require_advertised_capabilities(
        report,
        args.required_capabilities.as_slice(),
    ))
}

fn require_advertised_capabilities(
    report: TrustedProfileSyncPeerDiscoveryReport,
    required_capabilities: &[String],
) -> TrustedProfileSyncPeerDiscoveryReport {
    if required_capabilities.is_empty() {
        return report;
    }

    let mut filtered = TrustedProfileSyncPeerDiscoveryReport {
        trusted_peers: Vec::new(),
        rejected_peers: report.rejected_peers,
    };
    for peer in report.trusted_peers {
        if required_capabilities
            .iter()
            .all(|capability| peer.advertisement.has_capability(capability))
        {
            filtered.trusted_peers.push(peer);
        } else {
            filtered
                .rejected_peers
                .push(rejected_missing_required_capability_peer(peer));
        }
    }
    filtered
}

fn rejected_missing_required_capability_peer(
    peer: ProfileSyncPeerDiscoveryResult,
) -> RejectedProfileSyncPeerDiscoveryCandidate {
    RejectedProfileSyncPeerDiscoveryCandidate {
        protocol: peer.protocol,
        namespace: peer.namespace,
        network_id: peer.advertisement.network_id,
        node_id: peer.advertisement.node_id,
        provider_id: peer.advertisement.provider_id,
        reason: ProfileSyncPeerDiscoveryTrustRejection::MissingRequiredCapability,
    }
}

fn advertisement_file_candidates(
    args: &DiscoveryCheckArgs,
) -> Result<Vec<ProfileSyncPeerDiscoveryResult>, String> {
    args.advertisement_files
        .iter()
        .map(|path| {
            let text = fs::read_to_string(path)
                .map_err(|error| format!("read advertisement {}: {error}", path.display()))?;
            let advertisement = serde_json::from_str::<ProfileSyncPeerAdvertisement>(text.as_str())
                .map_err(|error| {
                    format!("decode advertisement {} as JSON: {error}", path.display())
                })?;
            advertisement
                .validate()
                .map_err(|error| format!("validate advertisement {}: {error}", path.display()))?;
            Ok(ProfileSyncPeerDiscoveryResult {
                protocol: args.protocol,
                namespace: args.namespace.clone(),
                advertisement,
            })
        })
        .collect()
}

fn discovery_check_report_json(
    profile: &str,
    network_id: &str,
    required_capabilities: &[String],
    report: &TrustedProfileSyncPeerDiscoveryReport,
) -> serde_json::Value {
    json!({
        "profile": profile,
        "network_id": network_id,
        "required_capabilities": required_capabilities,
        "trusted_peer_count": report.trusted_peer_count(),
        "rejected_peer_count": report.rejected_peer_count(),
        "trusted_peers": report.trusted_peers.iter().map(trusted_peer_json).collect::<Vec<_>>(),
        "rejected_peers": report.rejected_peers.iter().map(rejected_peer_json).collect::<Vec<_>>(),
    })
}

fn trusted_peer_json(peer: &ProfileSyncPeerDiscoveryResult) -> serde_json::Value {
    let advertisement = &peer.advertisement;
    json!({
        "protocol": peer.protocol.as_str(),
        "namespace": peer.namespace.as_str(),
        "network_id": advertisement.network_id.as_str(),
        "node_id": advertisement.node_id.as_str(),
        "provider_id": advertisement.provider_id.as_str(),
        "service_addr": advertisement.service_addr.as_str(),
        "capabilities": advertisement.capabilities.as_slice(),
        "membership_epoch": advertisement.membership_epoch,
        "sequence": advertisement.sequence,
        "signed": advertisement.identity_signature.is_some(),
    })
}

fn rejected_peer_json(candidate: &RejectedProfileSyncPeerDiscoveryCandidate) -> serde_json::Value {
    json!({
        "protocol": candidate.protocol.as_str(),
        "namespace": candidate.namespace.as_str(),
        "network_id": candidate.network_id.as_str(),
        "node_id": candidate.node_id.as_str(),
        "provider_id": candidate.provider_id.as_str(),
        "reason": candidate.reason.as_str(),
    })
}

fn next_string(args: &mut impl Iterator<Item = OsString>, name: &str) -> Result<String, String> {
    let value = args
        .next()
        .ok_or_else(|| format!("missing value for {name}\n\n{USAGE}"))?;
    os_string_to_string(value, name)
}

fn next_path(args: &mut impl Iterator<Item = OsString>, name: &str) -> Result<PathBuf, String> {
    args.next()
        .map(PathBuf::from)
        .ok_or_else(|| format!("missing value for {name}\n\n{USAGE}"))
}

fn os_string_to_string(value: OsString, name: &str) -> Result<String, String> {
    value
        .into_string()
        .map_err(|_| format!("{name} must be valid UTF-8"))
}

fn parse_protocol(value: &str) -> Result<ProfileSyncPeerDiscoveryProtocol, String> {
    match value {
        PROFILE_SYNC_DISCOVERY_PROTOCOL_LIBP2P_RENDEZVOUS => {
            Ok(ProfileSyncPeerDiscoveryProtocol::Libp2pRendezvous)
        }
        PROFILE_SYNC_DISCOVERY_PROTOCOL_LIBP2P_KADEMLIA => {
            Ok(ProfileSyncPeerDiscoveryProtocol::Libp2pKademlia)
        }
        PROFILE_SYNC_DISCOVERY_PROTOCOL_IPNS => Ok(ProfileSyncPeerDiscoveryProtocol::Ipns),
        PROFILE_SYNC_DISCOVERY_PROTOCOL_IROH_RENDEZVOUS => {
            Ok(ProfileSyncPeerDiscoveryProtocol::IrohRendezvous)
        }
        PROFILE_SYNC_DISCOVERY_PROTOCOL_LOCAL_SIMULATION => {
            Ok(ProfileSyncPeerDiscoveryProtocol::LocalSimulation)
        }
        _ => Err(format!(
            "unsupported profile-sync discovery protocol: {value}"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DiscoveryCheckArgs, discovery_check_report_json,
        trusted_discovery_report_from_advertisement_files,
    };
    use slate_broadwebd::{
        PROFILE_SYNC_DISCOVERY_PROTOCOL_IPNS, PROFILE_SYNC_PEER_DISCOVERY_CAPABILITY,
        PROFILE_SYNC_PEER_DISCOVERY_CAPABILITY_LOCAL_RETENTION,
        PROFILE_SYNC_PEER_DISCOVERY_CAPABILITY_OBJECT_TRANSFER, ProfileSyncPeerAdvertisement,
        ProfileSyncPeerDiscoveryProtocol,
    };
    use slate_profile_sync::sign_profile_sync_peer_advertisement;
    use slate_storage::{
        DEFAULT_PROFILE_ID, DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH, ProfileSyncDeviceSigner,
        SlateProfileDatabase, SyncDevicePublicKeyRegistration,
    };
    use std::ffi::OsString;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    #[test]
    fn parse_accepts_multiple_advertisement_files() {
        let args = DiscoveryCheckArgs::parse(os_args([
            "--settings-db",
            "slate-settings.db",
            "--network-id",
            "manual-net",
            "--protocol",
            PROFILE_SYNC_DISCOVERY_PROTOCOL_IPNS,
            "--advertisement-file",
            "a.json",
            "--advertisement-file",
            "b.json",
            "--require-capability",
            PROFILE_SYNC_PEER_DISCOVERY_CAPABILITY_OBJECT_TRANSFER,
            "--require-capability",
            PROFILE_SYNC_PEER_DISCOVERY_CAPABILITY_LOCAL_RETENTION,
            "--require-trusted",
            "--output",
            "report.json",
        ]))
        .expect("parse args");

        assert_eq!(args.settings_db, PathBuf::from("slate-settings.db"));
        assert_eq!(args.profile, DEFAULT_PROFILE_ID);
        assert_eq!(args.network_id, "manual-net");
        assert_eq!(args.protocol, ProfileSyncPeerDiscoveryProtocol::Ipns);
        assert_eq!(args.advertisement_files.len(), 2);
        assert_eq!(
            args.required_capabilities,
            vec![
                PROFILE_SYNC_PEER_DISCOVERY_CAPABILITY_OBJECT_TRANSFER.to_string(),
                PROFILE_SYNC_PEER_DISCOVERY_CAPABILITY_LOCAL_RETENTION.to_string(),
            ]
        );
        assert!(args.require_trusted);
        assert_eq!(args.output, Some(PathBuf::from("report.json")));
    }

    #[test]
    fn checks_signed_discovery_advertisement_against_settings_db() {
        let root = test_root("manual-discovery-check-trusted");
        let settings_db = root.join("slate-settings.db");
        let advertisement_file = root.join("advertisement.json");
        let database = SlateProfileDatabase::open_resolved_with_device_id(
            settings_db.clone(),
            "local-checker",
        )
        .expect("open settings db");
        let signer =
            ProfileSyncDeviceSigner::generate("remote-checker").expect("generate remote signer");
        database
            .register_sync_device_public_key(&SyncDevicePublicKeyRegistration {
                profile: DEFAULT_PROFILE_ID.to_string(),
                public_key: signer.public_key().expect("remote public key"),
                membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            })
            .expect("register remote public key");
        let advertisement = sign_profile_sync_peer_advertisement(
            ProfileSyncPeerAdvertisement::with_capabilities(
                "manual-net",
                signer.device_id(),
                "remote-provider",
                "/ip4/127.0.0.1/tcp/39000",
                [
                    PROFILE_SYNC_PEER_DISCOVERY_CAPABILITY,
                    PROFILE_SYNC_PEER_DISCOVERY_CAPABILITY_OBJECT_TRANSFER,
                    PROFILE_SYNC_PEER_DISCOVERY_CAPABILITY_LOCAL_RETENTION,
                ],
                7,
            )
            .expect("advertisement"),
            &signer,
        )
        .expect("sign advertisement");
        write_advertisement(&advertisement_file, &advertisement);

        let args = DiscoveryCheckArgs::parse(os_args([
            "--settings-db",
            path_str(&settings_db),
            "--local-device-id",
            "local-checker",
            "--network-id",
            "manual-net",
            "--advertisement-file",
            path_str(&advertisement_file),
            "--require-capability",
            PROFILE_SYNC_PEER_DISCOVERY_CAPABILITY_OBJECT_TRANSFER,
            "--require-capability",
            PROFILE_SYNC_PEER_DISCOVERY_CAPABILITY_LOCAL_RETENTION,
            "--require-trusted",
        ]))
        .expect("parse args");
        let report =
            trusted_discovery_report_from_advertisement_files(&args).expect("check discovery");
        let json = discovery_check_report_json(
            DEFAULT_PROFILE_ID,
            "manual-net",
            args.required_capabilities.as_slice(),
            &report,
        );

        assert_eq!(report.trusted_peer_count(), 1);
        assert_eq!(report.rejected_peer_count(), 0);
        assert_eq!(json["trusted_peer_count"], 1);
        assert_eq!(
            json["required_capabilities"][0],
            "profile-sync/object-transfer"
        );
        assert_eq!(
            json["required_capabilities"][1],
            "profile-sync/local-retention"
        );
        assert_eq!(json["trusted_peers"][0]["node_id"], "remote-checker");
        assert_eq!(json["trusted_peers"][0]["signed"], true);
        assert_eq!(
            json["trusted_peers"][0]["capabilities"][2],
            "profile-sync/local-retention"
        );
    }

    #[test]
    fn reports_unknown_signed_discovery_advertisement_as_rejected() {
        let root = test_root("manual-discovery-check-unknown");
        let settings_db = root.join("slate-settings.db");
        let advertisement_file = root.join("advertisement.json");
        SlateProfileDatabase::open_resolved_with_device_id(settings_db.clone(), "local-checker")
            .expect("open settings db");
        let signer =
            ProfileSyncDeviceSigner::generate("unknown-checker").expect("generate unknown signer");
        let advertisement = sign_profile_sync_peer_advertisement(
            ProfileSyncPeerAdvertisement::new(
                "manual-net",
                signer.device_id(),
                "unknown-provider",
                "127.0.0.1:39000",
                7,
            )
            .expect("advertisement"),
            &signer,
        )
        .expect("sign advertisement");
        write_advertisement(&advertisement_file, &advertisement);

        let args = DiscoveryCheckArgs::parse(os_args([
            "--settings-db",
            path_str(&settings_db),
            "--local-device-id",
            "local-checker",
            "--network-id",
            "manual-net",
            "--advertisement-file",
            path_str(&advertisement_file),
        ]))
        .expect("parse args");
        let report =
            trusted_discovery_report_from_advertisement_files(&args).expect("check discovery");
        let json = discovery_check_report_json(
            DEFAULT_PROFILE_ID,
            "manual-net",
            args.required_capabilities.as_slice(),
            &report,
        );

        assert_eq!(report.trusted_peer_count(), 0);
        assert_eq!(report.rejected_peer_count(), 1);
        assert_eq!(
            json["rejected_peers"][0]["reason"],
            "unknown_device_public_key"
        );
    }

    #[test]
    fn rejects_trusted_discovery_peer_missing_required_capability() {
        let root = test_root("manual-discovery-check-missing-capability");
        let settings_db = root.join("slate-settings.db");
        let advertisement_file = root.join("advertisement.json");
        let database = SlateProfileDatabase::open_resolved_with_device_id(
            settings_db.clone(),
            "local-checker",
        )
        .expect("open settings db");
        let signer =
            ProfileSyncDeviceSigner::generate("remote-checker").expect("generate remote signer");
        database
            .register_sync_device_public_key(&SyncDevicePublicKeyRegistration {
                profile: DEFAULT_PROFILE_ID.to_string(),
                public_key: signer.public_key().expect("remote public key"),
                membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            })
            .expect("register remote public key");
        let advertisement = sign_profile_sync_peer_advertisement(
            ProfileSyncPeerAdvertisement::new(
                "manual-net",
                signer.device_id(),
                "remote-provider",
                "/ip4/127.0.0.1/tcp/39000",
                7,
            )
            .expect("advertisement"),
            &signer,
        )
        .expect("sign advertisement");
        write_advertisement(&advertisement_file, &advertisement);

        let args = DiscoveryCheckArgs::parse(os_args([
            "--settings-db",
            path_str(&settings_db),
            "--local-device-id",
            "local-checker",
            "--network-id",
            "manual-net",
            "--advertisement-file",
            path_str(&advertisement_file),
            "--require-capability",
            PROFILE_SYNC_PEER_DISCOVERY_CAPABILITY_LOCAL_RETENTION,
        ]))
        .expect("parse args");
        let report =
            trusted_discovery_report_from_advertisement_files(&args).expect("check discovery");
        let json = discovery_check_report_json(
            DEFAULT_PROFILE_ID,
            "manual-net",
            args.required_capabilities.as_slice(),
            &report,
        );

        assert_eq!(report.trusted_peer_count(), 0);
        assert_eq!(report.rejected_peer_count(), 1);
        assert_eq!(
            json["rejected_peers"][0]["reason"],
            "missing_required_capability"
        );
    }

    fn os_args(args: impl IntoIterator<Item = &'static str>) -> impl Iterator<Item = OsString> {
        args.into_iter().map(OsString::from)
    }

    fn test_root(name: &str) -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let suffix = COUNTER.fetch_add(1, Ordering::Relaxed);
        let root = std::env::current_dir()
            .expect("current directory")
            .join("target/tmp/profile-sync-discovery-check-tests")
            .join(format!("{name}-{suffix}"));
        fs::create_dir_all(&root).expect("create test root");
        root
    }

    fn write_advertisement(path: &Path, advertisement: &ProfileSyncPeerAdvertisement) {
        let text = serde_json::to_string(advertisement).expect("encode advertisement");
        fs::write(path, text).expect("write advertisement");
    }

    fn path_str(path: &Path) -> &'static str {
        Box::leak(path.to_string_lossy().into_owned().into_boxed_str())
    }
}
