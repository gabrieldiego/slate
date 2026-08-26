use slate_broadwebd::{ProfileSyncPeerDiscoveryProtocol, ProfileSyncPeerDiscoveryResult};
use slate_storage::{SlateProfileDatabase, StorageError};

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
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::{
        ProfileSyncPeerDiscoveryTrustRejection, filter_trusted_profile_sync_peer_discovery_results,
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

        let candidates = vec![
            test_discovery_result(
                network_id,
                trusted_signer.device_id(),
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
        assert_eq!(report.rejected_peer_count(), 5);
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
