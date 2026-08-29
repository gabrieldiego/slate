use core::fmt;
use slate_broadwebd::{
    BroadwebdError, ProfileSyncPeerAdvertisement, ProfileSyncPeerAdvertisementSignature,
    ProfileSyncPeerDiscoveryProtocol, ProfileSyncPeerDiscoveryResult,
};
use slate_storage::{
    ProfileSyncDeviceSigner, SignedSyncObject, SlateProfileDatabase, StorageError, SyncObjectError,
};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TrustedProfileSyncPeerDiscoveryReport {
    pub trusted_peers: Vec<ProfileSyncPeerDiscoveryResult>,
    pub rejected_peers: Vec<RejectedProfileSyncPeerDiscoveryCandidate>,
}

impl TrustedProfileSyncPeerDiscoveryReport {
    pub fn trusted_peer_count(&self) -> usize {
        self.trusted_peers.len()
    }

    pub fn rejected_peer_count(&self) -> usize {
        self.rejected_peers.len()
    }

    pub fn has_trusted_peer(&self) -> bool {
        !self.trusted_peers.is_empty()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RejectedProfileSyncPeerDiscoveryCandidate {
    pub protocol: ProfileSyncPeerDiscoveryProtocol,
    pub namespace: String,
    pub network_id: String,
    pub node_id: String,
    pub provider_id: String,
    pub reason: ProfileSyncPeerDiscoveryTrustRejection,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProfileSyncPeerDiscoveryTrustRejection {
    WrongNetwork,
    LocalDevice,
    MissingProfileSyncServiceFrameCapability,
    UnknownDevicePublicKey,
    UntrustedDevicePublicKey,
    MissingSignedIdentity,
    SignatureDeviceMismatch,
    SignaturePublicKeyMismatch,
    InvalidSignature,
}

#[derive(Debug)]
pub enum ProfileSyncPeerAdvertisementSignatureError {
    Broadwebd(BroadwebdError),
    SyncObject(SyncObjectError),
    SignerNodeMismatch {
        node_id: String,
        signer_device_id: String,
    },
}

impl fmt::Display for ProfileSyncPeerAdvertisementSignatureError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Broadwebd(error) => write!(
                formatter,
                "profile sync peer advertisement serialization failed: {error}"
            ),
            Self::SyncObject(error) => {
                write!(
                    formatter,
                    "profile sync peer advertisement signing failed: {error}"
                )
            }
            Self::SignerNodeMismatch {
                node_id,
                signer_device_id,
            } => write!(
                formatter,
                "profile sync peer advertisement node {node_id} cannot be signed by device {signer_device_id}"
            ),
        }
    }
}

impl std::error::Error for ProfileSyncPeerAdvertisementSignatureError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Broadwebd(error) => Some(error),
            Self::SyncObject(error) => Some(error),
            Self::SignerNodeMismatch { .. } => None,
        }
    }
}

impl From<BroadwebdError> for ProfileSyncPeerAdvertisementSignatureError {
    fn from(error: BroadwebdError) -> Self {
        Self::Broadwebd(error)
    }
}

impl From<SyncObjectError> for ProfileSyncPeerAdvertisementSignatureError {
    fn from(error: SyncObjectError) -> Self {
        Self::SyncObject(error)
    }
}

pub fn sign_profile_sync_peer_advertisement(
    advertisement: ProfileSyncPeerAdvertisement,
    signer: &ProfileSyncDeviceSigner,
) -> Result<ProfileSyncPeerAdvertisement, ProfileSyncPeerAdvertisementSignatureError> {
    if advertisement.node_id != signer.device_id() {
        return Err(
            ProfileSyncPeerAdvertisementSignatureError::SignerNodeMismatch {
                node_id: advertisement.node_id,
                signer_device_id: signer.device_id().to_string(),
            },
        );
    }

    let payload = advertisement.signing_payload_bytes()?;
    let signed = signer.sign(payload.as_slice())?;
    let signature = ProfileSyncPeerAdvertisementSignature::ed25519(
        signed.device_id,
        signed.public_key,
        signed.signature,
    )?;
    Ok(advertisement.with_identity_signature(signature)?)
}

pub fn filter_trusted_profile_sync_peer_discovery_results(
    database: &SlateProfileDatabase,
    profile: &str,
    network_id: &str,
    candidates: impl IntoIterator<Item = ProfileSyncPeerDiscoveryResult>,
) -> Result<TrustedProfileSyncPeerDiscoveryReport, StorageError> {
    let mut report = TrustedProfileSyncPeerDiscoveryReport::default();
    for candidate in candidates {
        if let Some(reason) =
            profile_sync_peer_discovery_trust_rejection(database, profile, network_id, &candidate)?
        {
            report
                .rejected_peers
                .push(RejectedProfileSyncPeerDiscoveryCandidate {
                    protocol: candidate.protocol,
                    namespace: candidate.namespace,
                    network_id: candidate.advertisement.network_id,
                    node_id: candidate.advertisement.node_id,
                    provider_id: candidate.advertisement.provider_id,
                    reason,
                });
        } else {
            report.trusted_peers.push(candidate);
        }
    }
    Ok(report)
}

fn profile_sync_peer_discovery_trust_rejection(
    database: &SlateProfileDatabase,
    profile: &str,
    network_id: &str,
    candidate: &ProfileSyncPeerDiscoveryResult,
) -> Result<Option<ProfileSyncPeerDiscoveryTrustRejection>, StorageError> {
    let advertisement = &candidate.advertisement;
    if advertisement.network_id != network_id {
        return Ok(Some(ProfileSyncPeerDiscoveryTrustRejection::WrongNetwork));
    }
    if advertisement.node_id == database.local_sync_device_id() {
        return Ok(Some(ProfileSyncPeerDiscoveryTrustRejection::LocalDevice));
    }
    if !advertisement.supports_profile_sync_service_frames() {
        return Ok(Some(
            ProfileSyncPeerDiscoveryTrustRejection::MissingProfileSyncServiceFrameCapability,
        ));
    }

    let Some(record) = database.sync_device_public_key(profile, advertisement.node_id.as_str())?
    else {
        return Ok(Some(
            ProfileSyncPeerDiscoveryTrustRejection::UnknownDevicePublicKey,
        ));
    };
    if !record.trusted {
        return Ok(Some(
            ProfileSyncPeerDiscoveryTrustRejection::UntrustedDevicePublicKey,
        ));
    }
    let Some(signature) = advertisement.identity_signature.as_ref() else {
        return Ok(Some(
            ProfileSyncPeerDiscoveryTrustRejection::MissingSignedIdentity,
        ));
    };
    if signature.device_id != advertisement.node_id {
        return Ok(Some(
            ProfileSyncPeerDiscoveryTrustRejection::SignatureDeviceMismatch,
        ));
    }
    if signature.public_key != record.public_key.bytes {
        return Ok(Some(
            ProfileSyncPeerDiscoveryTrustRejection::SignaturePublicKeyMismatch,
        ));
    }

    let Ok(payload) = advertisement.signing_payload_bytes() else {
        return Ok(Some(
            ProfileSyncPeerDiscoveryTrustRejection::InvalidSignature,
        ));
    };
    let signed = SignedSyncObject {
        version: signature.version,
        device_id: signature.device_id.clone(),
        public_key: signature.public_key.clone(),
        payload,
        signature: signature.signature.clone(),
    };
    if signed.verify_with(&record.public_key).is_err() {
        return Ok(Some(
            ProfileSyncPeerDiscoveryTrustRejection::InvalidSignature,
        ));
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::{
        ProfileSyncPeerAdvertisementSignatureError, ProfileSyncPeerDiscoveryTrustRejection,
        filter_trusted_profile_sync_peer_discovery_results, sign_profile_sync_peer_advertisement,
    };
    use slate_broadwebd::{
        ProfileSyncPeerAdvertisement, ProfileSyncPeerDiscoveryProtocol,
        ProfileSyncPeerDiscoveryResult,
    };
    use slate_storage::{
        DEFAULT_DATABASE_FILE_NAME, DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH, ProfileSyncDeviceSigner,
        SlateProfileDatabase, SyncDevicePublicKeyRegistration,
    };
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn trusted_profile_sync_peer_discovery_filters_by_local_trust_state() {
        let db_root = test_state_root("trusted-peer-discovery-db");
        let database = SlateProfileDatabase::open_resolved_with_device_id(
            db_root.join(DEFAULT_DATABASE_FILE_NAME),
            "peer-trust-local-device",
        )
        .expect("open peer trust database");
        let profile = "peertrustprofile";
        let network_id = "peertrustnetwork";
        let trusted_signer =
            ProfileSyncDeviceSigner::generate("peer-trust-remote").expect("trusted signer");
        let revoked_signer =
            ProfileSyncDeviceSigner::generate("peer-trust-revoked").expect("revoked signer");
        let conflicting_key_signer = ProfileSyncDeviceSigner::generate(trusted_signer.device_id())
            .expect("conflicting signer for trusted device id");

        for public_key in [
            trusted_signer.public_key().expect("trusted public key"),
            revoked_signer.public_key().expect("revoked public key"),
        ] {
            database
                .register_sync_device_public_key(&SyncDevicePublicKeyRegistration {
                    profile: profile.to_string(),
                    public_key,
                    membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
                })
                .expect("register peer public key");
        }
        database
            .set_sync_device_public_key_trusted(profile, revoked_signer.device_id(), false)
            .expect("revoke remote peer public key")
            .expect("revoked remote peer public key");

        let mut invalid_signature = test_signed_discovery_result(
            network_id,
            &trusted_signer,
            "peer-trust-provider-g",
            "/ip4/127.0.0.1/tcp/9407",
        );
        invalid_signature.advertisement.service_addr = "/ip4/127.0.0.1/tcp/9499".to_string();

        let candidates = vec![
            test_signed_discovery_result(
                network_id,
                &trusted_signer,
                "peer-trust-provider-a",
                "/ip4/127.0.0.1/tcp/9401",
            ),
            test_discovery_result(
                network_id,
                revoked_signer.device_id(),
                "peer-trust-provider-b",
                "/ip4/127.0.0.1/tcp/9402",
            ),
            test_discovery_result(
                network_id,
                "peer-trust-unknown",
                "peer-trust-provider-c",
                "/ip4/127.0.0.1/tcp/9403",
            ),
            test_discovery_result(
                "peertrustothernetwork",
                trusted_signer.device_id(),
                "peer-trust-provider-d",
                "/ip4/127.0.0.1/tcp/9404",
            ),
            test_discovery_result(
                network_id,
                "peer-trust-local-device",
                "peer-trust-provider-e",
                "/ip4/127.0.0.1/tcp/9405",
            ),
            test_discovery_result_with_capabilities(
                network_id,
                trusted_signer.device_id(),
                "peer-trust-provider-f",
                "/ip4/127.0.0.1/tcp/9406",
                ["profile-sync/metadata-only"],
            ),
            test_discovery_result(
                network_id,
                trusted_signer.device_id(),
                "peer-trust-provider-g",
                "/ip4/127.0.0.1/tcp/9407",
            ),
            invalid_signature,
            test_signed_discovery_result(
                network_id,
                &conflicting_key_signer,
                "peer-trust-provider-h",
                "/ip4/127.0.0.1/tcp/9408",
            ),
        ];

        let report = filter_trusted_profile_sync_peer_discovery_results(
            &database, profile, network_id, candidates,
        )
        .expect("filter trusted peer discovery candidates");

        assert_eq!(report.trusted_peer_count(), 1);
        assert!(report.has_trusted_peer());
        assert_eq!(
            report.trusted_peers[0].advertisement.node_id.as_str(),
            trusted_signer.device_id()
        );
        assert_eq!(report.rejected_peer_count(), 8);
        assert_eq!(
            report
                .rejected_peers
                .iter()
                .map(|rejection| {
                    (
                        rejection.node_id.as_str(),
                        rejection.provider_id.as_str(),
                        rejection.reason,
                    )
                })
                .collect::<Vec<_>>(),
            vec![
                (
                    revoked_signer.device_id(),
                    "peer-trust-provider-b",
                    ProfileSyncPeerDiscoveryTrustRejection::UntrustedDevicePublicKey,
                ),
                (
                    "peer-trust-unknown",
                    "peer-trust-provider-c",
                    ProfileSyncPeerDiscoveryTrustRejection::UnknownDevicePublicKey,
                ),
                (
                    trusted_signer.device_id(),
                    "peer-trust-provider-d",
                    ProfileSyncPeerDiscoveryTrustRejection::WrongNetwork,
                ),
                (
                    "peer-trust-local-device",
                    "peer-trust-provider-e",
                    ProfileSyncPeerDiscoveryTrustRejection::LocalDevice,
                ),
                (
                    trusted_signer.device_id(),
                    "peer-trust-provider-f",
                    ProfileSyncPeerDiscoveryTrustRejection::MissingProfileSyncServiceFrameCapability,
                ),
                (
                    trusted_signer.device_id(),
                    "peer-trust-provider-g",
                    ProfileSyncPeerDiscoveryTrustRejection::MissingSignedIdentity,
                ),
                (
                    trusted_signer.device_id(),
                    "peer-trust-provider-g",
                    ProfileSyncPeerDiscoveryTrustRejection::InvalidSignature,
                ),
                (
                    trusted_signer.device_id(),
                    "peer-trust-provider-h",
                    ProfileSyncPeerDiscoveryTrustRejection::SignaturePublicKeyMismatch,
                ),
            ]
        );
        assert!(
            report
                .rejected_peers
                .iter()
                .all(|rejection| rejection.protocol
                    == ProfileSyncPeerDiscoveryProtocol::Libp2pRendezvous
                    && rejection.namespace == "slate-profile-sync")
        );

        let _ = std::fs::remove_dir_all(db_root);
    }

    #[test]
    fn profile_sync_peer_advertisement_signing_requires_matching_node_id() {
        let signer =
            ProfileSyncDeviceSigner::generate("peer-trust-signer").expect("generate signer");
        let error = sign_profile_sync_peer_advertisement(
            ProfileSyncPeerAdvertisement::new(
                "peertrustnetwork",
                "peer-trust-other-node",
                "peer-trust-provider",
                "/ip4/127.0.0.1/tcp/9401",
                1,
            )
            .expect("advertisement"),
            &signer,
        )
        .expect_err("mismatched node id should not be signed");

        assert!(matches!(
            error,
            ProfileSyncPeerAdvertisementSignatureError::SignerNodeMismatch {
                node_id,
                signer_device_id,
            } if node_id == "peer-trust-other-node"
                && signer_device_id == signer.device_id()
        ));
    }

    fn test_discovery_result(
        network_id: &str,
        node_id: &str,
        provider_id: &str,
        service_addr: &str,
    ) -> ProfileSyncPeerDiscoveryResult {
        test_discovery_result_with_capabilities(
            network_id,
            node_id,
            provider_id,
            service_addr,
            ["profile-sync/service-frame-tcp"],
        )
    }

    fn test_discovery_result_with_capabilities(
        network_id: &str,
        node_id: &str,
        provider_id: &str,
        service_addr: &str,
        capabilities: impl IntoIterator<Item = &'static str>,
    ) -> ProfileSyncPeerDiscoveryResult {
        ProfileSyncPeerDiscoveryResult {
            protocol: ProfileSyncPeerDiscoveryProtocol::Libp2pRendezvous,
            namespace: "slate-profile-sync".to_string(),
            advertisement: ProfileSyncPeerAdvertisement::with_capabilities(
                network_id,
                node_id,
                provider_id,
                service_addr,
                capabilities,
                1,
            )
            .expect("build profile-sync discovery test advertisement"),
        }
    }

    fn test_signed_discovery_result(
        network_id: &str,
        signer: &ProfileSyncDeviceSigner,
        provider_id: &str,
        service_addr: &str,
    ) -> ProfileSyncPeerDiscoveryResult {
        ProfileSyncPeerDiscoveryResult {
            protocol: ProfileSyncPeerDiscoveryProtocol::Libp2pRendezvous,
            namespace: "slate-profile-sync".to_string(),
            advertisement: sign_profile_sync_peer_advertisement(
                ProfileSyncPeerAdvertisement::new(
                    network_id,
                    signer.device_id(),
                    provider_id,
                    service_addr,
                    1,
                )
                .expect("build profile-sync discovery test advertisement"),
                signer,
            )
            .expect("sign profile-sync discovery test advertisement"),
        }
    }

    fn test_state_root(name: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time should move forward")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "slate-profile-sync-discovery-trust-test-{}-{nanos}-{name}",
            std::process::id()
        ))
    }
}
