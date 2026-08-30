#![forbid(unsafe_code)]

use std::env;
use std::ffi::OsString;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use slate_broadwebd::{
    DEFAULT_PROFILE_SYNC_PEER_ADVERTISEMENT_MEMBERSHIP_EPOCH,
    PROFILE_SYNC_PEER_DISCOVERY_CAPABILITY, ProfileSyncPeerAdvertisement,
};
use slate_profile_sync::sign_profile_sync_peer_advertisement;
use slate_storage::{SlateSyncSecret, SlateSyncSecretExport};

const USAGE: &str = "\
Generate a signed Slate profile-sync peer discovery advertisement.

Usage:
  slate-profile-sync-advertisement --key-file <path> --network-id <id> --device-id <id> --provider-id <id> --service-addr <addr> [--profile <profile>] [--membership-epoch <n>] [--sequence <n>] [--capability <name>...] [--output <path>]
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
    let args = AdvertisementArgs::parse(args)?;
    let key_bytes = fs::read(&args.key_file)
        .map_err(|error| format!("read key file {}: {error}", args.key_file.display()))?;
    let key_export = SlateSyncSecretExport::from_bytes(key_bytes.as_slice())
        .map_err(|error| format!("decode enrollment key: {error}"))?;
    let advertisement = signed_advertisement_from_key_export(&key_export, &args)?;
    let mut bytes = serde_json::to_vec_pretty(&advertisement)
        .map_err(|error| format!("encode signed advertisement JSON: {error}"))?;
    bytes.push(b'\n');

    if let Some(output) = &args.output {
        fs::write(output, bytes)
            .map_err(|error| format!("write advertisement {}: {error}", output.display()))?;
    } else {
        let mut stdout = io::stdout().lock();
        stdout
            .write_all(bytes.as_slice())
            .and_then(|_| stdout.flush())
            .map_err(|error| format!("write advertisement to stdout: {error}"))?;
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct AdvertisementArgs {
    key_file: PathBuf,
    profile: Option<String>,
    network_id: String,
    device_id: String,
    provider_id: String,
    service_addr: String,
    capabilities: Vec<String>,
    membership_epoch: i64,
    sequence: u64,
    output: Option<PathBuf>,
}

impl AdvertisementArgs {
    fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Self, String> {
        let mut parsed = ParsedAdvertisementArgs::default();
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            let arg = os_string_to_string(arg, "argument")?;
            match arg.as_str() {
                "--help" | "-h" => return Err(USAGE.to_string()),
                "--key-file" => parsed.key_file = Some(next_path(&mut args, "--key-file")?),
                "--profile" => parsed.profile = Some(next_string(&mut args, "--profile")?),
                "--network-id" => parsed.network_id = Some(next_string(&mut args, "--network-id")?),
                "--device-id" => parsed.device_id = Some(next_string(&mut args, "--device-id")?),
                "--provider-id" => {
                    parsed.provider_id = Some(next_string(&mut args, "--provider-id")?)
                }
                "--service-addr" => {
                    parsed.service_addr = Some(next_string(&mut args, "--service-addr")?)
                }
                "--capability" => parsed
                    .capabilities
                    .push(next_string(&mut args, "--capability")?),
                "--membership-epoch" => {
                    parsed.membership_epoch =
                        Some(parse_i64(next_string(&mut args, "--membership-epoch")?)?)
                }
                "--sequence" => {
                    parsed.sequence = Some(parse_u64(next_string(&mut args, "--sequence")?)?)
                }
                "--output" => parsed.output = Some(next_path(&mut args, "--output")?),
                _ => return Err(format!("unknown argument: {arg}\n\n{USAGE}")),
            }
        }

        Ok(Self {
            key_file: parsed
                .key_file
                .ok_or_else(|| format!("missing required --key-file\n\n{USAGE}"))?,
            profile: parsed.profile,
            network_id: parsed
                .network_id
                .ok_or_else(|| format!("missing required --network-id\n\n{USAGE}"))?,
            device_id: parsed
                .device_id
                .ok_or_else(|| format!("missing required --device-id\n\n{USAGE}"))?,
            provider_id: parsed
                .provider_id
                .ok_or_else(|| format!("missing required --provider-id\n\n{USAGE}"))?,
            service_addr: parsed
                .service_addr
                .ok_or_else(|| format!("missing required --service-addr\n\n{USAGE}"))?,
            capabilities: advertisement_capabilities(parsed.capabilities),
            membership_epoch: parsed
                .membership_epoch
                .unwrap_or(DEFAULT_PROFILE_SYNC_PEER_ADVERTISEMENT_MEMBERSHIP_EPOCH),
            sequence: parsed.sequence.unwrap_or_else(unix_sequence),
            output: parsed.output,
        })
    }
}

#[derive(Debug, Default)]
struct ParsedAdvertisementArgs {
    key_file: Option<PathBuf>,
    profile: Option<String>,
    network_id: Option<String>,
    device_id: Option<String>,
    provider_id: Option<String>,
    service_addr: Option<String>,
    capabilities: Vec<String>,
    membership_epoch: Option<i64>,
    sequence: Option<u64>,
    output: Option<PathBuf>,
}

fn signed_advertisement_from_key_export(
    key_export: &SlateSyncSecretExport,
    args: &AdvertisementArgs,
) -> Result<ProfileSyncPeerAdvertisement, String> {
    let profile = args
        .profile
        .as_deref()
        .unwrap_or(key_export.profile.as_str());
    let sync_secret = SlateSyncSecret::from_export_for_profile(key_export, profile)
        .map_err(|error| format!("load enrollment key for profile {profile}: {error}"))?;
    let signer = sync_secret
        .derive_profile_sync_device_signer(profile, args.device_id.as_str(), args.membership_epoch)
        .map_err(|error| format!("derive device signer: {error}"))?;
    let advertisement = ProfileSyncPeerAdvertisement::with_capabilities(
        args.network_id.as_str(),
        args.device_id.as_str(),
        args.provider_id.as_str(),
        args.service_addr.as_str(),
        args.capabilities.iter().map(String::as_str),
        args.sequence,
    )
    .and_then(|advertisement| advertisement.with_membership_epoch(args.membership_epoch))
    .map_err(|error| format!("create peer discovery advertisement: {error}"))?;
    sign_profile_sync_peer_advertisement(advertisement, &signer)
        .map_err(|error| format!("sign peer discovery advertisement: {error}"))
}

fn advertisement_capabilities(extra_capabilities: Vec<String>) -> Vec<String> {
    let mut capabilities = vec![PROFILE_SYNC_PEER_DISCOVERY_CAPABILITY.to_string()];
    for capability in extra_capabilities {
        if !capabilities.iter().any(|stored| stored == &capability) {
            capabilities.push(capability);
        }
    }
    capabilities
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

fn parse_i64(value: String) -> Result<i64, String> {
    value
        .parse::<i64>()
        .map_err(|error| format!("invalid integer {value}: {error}"))
}

fn parse_u64(value: String) -> Result<u64, String> {
    value
        .parse::<u64>()
        .map_err(|error| format!("invalid unsigned integer {value}: {error}"))
}

fn unix_sequence() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs().max(1))
        .unwrap_or(1)
}

#[cfg(test)]
mod tests {
    use super::{AdvertisementArgs, signed_advertisement_from_key_export};
    use slate_broadwebd::{
        PROFILE_SYNC_PEER_DISCOVERY_CAPABILITY,
        PROFILE_SYNC_PEER_DISCOVERY_CAPABILITY_LOCAL_RETENTION,
        PROFILE_SYNC_PEER_DISCOVERY_CAPABILITY_OBJECT_TRANSFER,
    };
    use slate_storage::{DEFAULT_PROFILE_ID, SLATE_SYNC_SECRET_BYTES, SlateSyncSecret};
    use std::ffi::OsString;
    use std::path::PathBuf;

    #[test]
    fn parse_requires_manual_discovery_inputs() {
        let args = AdvertisementArgs::parse(os_args([
            "--key-file",
            "enrollment.json",
            "--network-id",
            "manual-net",
            "--device-id",
            "device-a",
            "--provider-id",
            "provider-a",
            "--service-addr",
            "127.0.0.1:39000",
            "--membership-epoch",
            "3",
            "--sequence",
            "7",
            "--capability",
            PROFILE_SYNC_PEER_DISCOVERY_CAPABILITY_OBJECT_TRANSFER,
            "--capability",
            PROFILE_SYNC_PEER_DISCOVERY_CAPABILITY_LOCAL_RETENTION,
            "--output",
            "advertisement.json",
        ]))
        .expect("parse args");

        assert_eq!(args.key_file, PathBuf::from("enrollment.json"));
        assert_eq!(args.profile, None);
        assert_eq!(args.network_id, "manual-net");
        assert_eq!(args.device_id, "device-a");
        assert_eq!(args.provider_id, "provider-a");
        assert_eq!(args.service_addr, "127.0.0.1:39000");
        assert_eq!(
            args.capabilities,
            vec![
                PROFILE_SYNC_PEER_DISCOVERY_CAPABILITY.to_string(),
                PROFILE_SYNC_PEER_DISCOVERY_CAPABILITY_OBJECT_TRANSFER.to_string(),
                PROFILE_SYNC_PEER_DISCOVERY_CAPABILITY_LOCAL_RETENTION.to_string(),
            ]
        );
        assert_eq!(args.membership_epoch, 3);
        assert_eq!(args.sequence, 7);
        assert_eq!(args.output, Some(PathBuf::from("advertisement.json")));
    }

    #[test]
    fn signs_discovery_advertisement_from_enrollment_key() {
        let secret = SlateSyncSecret::from_bytes([17; SLATE_SYNC_SECRET_BYTES]);
        let key_export = secret.export_for_profile(DEFAULT_PROFILE_ID, 123);
        let args = AdvertisementArgs::parse(os_args([
            "--key-file",
            "ignored.json",
            "--network-id",
            "manual-net",
            "--device-id",
            "device-a",
            "--provider-id",
            "provider-a",
            "--service-addr",
            "/ip4/127.0.0.1/tcp/39000",
            "--membership-epoch",
            "5",
            "--sequence",
            "9",
            "--capability",
            PROFILE_SYNC_PEER_DISCOVERY_CAPABILITY_OBJECT_TRANSFER,
            "--capability",
            PROFILE_SYNC_PEER_DISCOVERY_CAPABILITY_LOCAL_RETENTION,
        ]))
        .expect("parse args");

        let advertisement =
            signed_advertisement_from_key_export(&key_export, &args).expect("sign advertisement");

        assert_eq!(advertisement.network_id, "manual-net");
        assert_eq!(advertisement.node_id, "device-a");
        assert_eq!(advertisement.provider_id, "provider-a");
        assert_eq!(advertisement.service_addr, "/ip4/127.0.0.1/tcp/39000");
        assert!(advertisement.supports_profile_sync_service_frames());
        assert!(advertisement.supports_durable_profile_sync_retention());
        assert_eq!(advertisement.membership_epoch, 5);
        assert_eq!(advertisement.sequence, 9);
        assert!(advertisement.identity_signature.is_some());
    }

    #[test]
    fn rejects_profile_mismatch_before_signing() {
        let secret = SlateSyncSecret::from_bytes([19; SLATE_SYNC_SECRET_BYTES]);
        let key_export = secret.export_for_profile(DEFAULT_PROFILE_ID, 123);
        let args = AdvertisementArgs::parse(os_args([
            "--key-file",
            "ignored.json",
            "--profile",
            "work",
            "--network-id",
            "manual-net",
            "--device-id",
            "device-a",
            "--provider-id",
            "provider-a",
            "--service-addr",
            "127.0.0.1:39000",
            "--sequence",
            "9",
        ]))
        .expect("parse args");

        let error = signed_advertisement_from_key_export(&key_export, &args)
            .expect_err("profile mismatch should fail");

        assert!(error.contains("profile"));
    }

    fn os_args(args: impl IntoIterator<Item = &'static str>) -> impl Iterator<Item = OsString> {
        args.into_iter().map(OsString::from)
    }
}
