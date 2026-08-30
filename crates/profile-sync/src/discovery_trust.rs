use core::fmt;
use slate_broadwebd::{
    BroadwebdError, ProfileSyncPeerAdvertisement, ProfileSyncPeerAdvertisementSignature,
    ProfileSyncPeerDiscoveryProtocol, ProfileSyncPeerDiscoveryProvider,
    ProfileSyncPeerDiscoveryQuery, ProfileSyncPeerDiscoveryResult,
};
use slate_storage::{
    ProfileSyncDeviceSigner, SignedSyncObject, SlateProfileDatabase, StorageError, SyncObjectError,
};
use std::collections::BTreeMap;

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
    StaleDiscoverySequence,
    ReplayedDiscoverySequence,
    UnknownDevicePublicKey,
    UntrustedDevicePublicKey,
    MissingSignedIdentity,
    MissingRequiredCapability,
    SignatureDeviceMismatch,
    SignaturePublicKeyMismatch,
    SignerMembershipEpochTooNew,
    InvalidSignature,
}

impl ProfileSyncPeerDiscoveryTrustRejection {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::WrongNetwork => "wrong_network",
            Self::LocalDevice => "local_device",
            Self::MissingProfileSyncServiceFrameCapability => {
                "missing_profile_sync_service_frame_capability"
            }
            Self::StaleDiscoverySequence => "stale_discovery_sequence",
            Self::ReplayedDiscoverySequence => "replayed_discovery_sequence",
            Self::UnknownDevicePublicKey => "unknown_device_public_key",
            Self::UntrustedDevicePublicKey => "untrusted_device_public_key",
            Self::MissingSignedIdentity => "missing_signed_identity",
            Self::MissingRequiredCapability => "missing_required_capability",
            Self::SignatureDeviceMismatch => "signature_device_mismatch",
            Self::SignaturePublicKeyMismatch => "signature_public_key_mismatch",
            Self::SignerMembershipEpochTooNew => "signer_membership_epoch_too_new",
            Self::InvalidSignature => "invalid_signature",
        }
    }
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

#[derive(Debug)]
pub enum ProfileSyncPeerDiscoveryError {
    Broadwebd(BroadwebdError),
    Storage(StorageError),
}

impl fmt::Display for ProfileSyncPeerDiscoveryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Broadwebd(error) => {
                write!(formatter, "profile sync peer discovery failed: {error}")
            }
            Self::Storage(error) => {
                write!(
                    formatter,
                    "profile sync peer discovery trust failed: {error}"
                )
            }
        }
    }
}

impl std::error::Error for ProfileSyncPeerDiscoveryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Broadwebd(error) => Some(error),
            Self::Storage(error) => Some(error),
        }
    }
}

impl From<BroadwebdError> for ProfileSyncPeerDiscoveryError {
    fn from(error: BroadwebdError) -> Self {
        Self::Broadwebd(error)
    }
}

impl From<StorageError> for ProfileSyncPeerDiscoveryError {
    fn from(error: StorageError) -> Self {
        Self::Storage(error)
    }
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
    filter_trusted_profile_sync_peer_discovery_results_with_required_capabilities(
        database,
        profile,
        network_id,
        &[],
        candidates,
    )
}

pub fn filter_trusted_profile_sync_peer_discovery_results_with_required_capabilities(
    database: &SlateProfileDatabase,
    profile: &str,
    network_id: &str,
    required_capabilities: &[String],
    candidates: impl IntoIterator<Item = ProfileSyncPeerDiscoveryResult>,
) -> Result<TrustedProfileSyncPeerDiscoveryReport, StorageError> {
    let mut report = TrustedProfileSyncPeerDiscoveryReport::default();
    let mut trusted_candidates = Vec::new();
    for candidate in candidates {
        if let Some(reason) =
            profile_sync_peer_discovery_trust_rejection(database, profile, network_id, &candidate)?
        {
            report
                .rejected_peers
                .push(rejected_discovery_candidate(candidate, reason));
        } else {
            trusted_candidates.push(candidate);
        }
    }
    let fresh_report = TrustedProfileSyncPeerDiscoveryReport {
        trusted_peers: freshest_trusted_profile_sync_peer_discovery_results(
            trusted_candidates,
            &mut report,
        ),
        rejected_peers: report.rejected_peers,
    };
    Ok(require_profile_sync_peer_discovery_capabilities(
        fresh_report,
        required_capabilities,
    ))
}

pub fn require_profile_sync_peer_discovery_capabilities(
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
            filtered.rejected_peers.push(rejected_discovery_candidate(
                peer,
                ProfileSyncPeerDiscoveryTrustRejection::MissingRequiredCapability,
            ));
        }
    }
    filtered
}

pub fn discover_trusted_profile_sync_peers_with_required_capabilities(
    database: &SlateProfileDatabase,
    profile: &str,
    provider: &(impl ProfileSyncPeerDiscoveryProvider + ?Sized),
    query: &ProfileSyncPeerDiscoveryQuery,
    required_capabilities: &[String],
) -> Result<TrustedProfileSyncPeerDiscoveryReport, ProfileSyncPeerDiscoveryError> {
    let candidates = provider.discover_profile_sync_peers(query)?;
    Ok(
        filter_trusted_profile_sync_peer_discovery_results_with_required_capabilities(
            database,
            profile,
            query.network_id.as_str(),
            required_capabilities,
            candidates,
        )?,
    )
}

pub fn discover_trusted_profile_sync_peers(
    database: &SlateProfileDatabase,
    profile: &str,
    provider: &(impl ProfileSyncPeerDiscoveryProvider + ?Sized),
    query: &ProfileSyncPeerDiscoveryQuery,
) -> Result<TrustedProfileSyncPeerDiscoveryReport, ProfileSyncPeerDiscoveryError> {
    discover_trusted_profile_sync_peers_with_required_capabilities(
        database,
        profile,
        provider,
        query,
        &[],
    )
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
    if record.membership_epoch > advertisement.membership_epoch {
        return Ok(Some(
            ProfileSyncPeerDiscoveryTrustRejection::SignerMembershipEpochTooNew,
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

fn freshest_trusted_profile_sync_peer_discovery_results(
    candidates: Vec<ProfileSyncPeerDiscoveryResult>,
    report: &mut TrustedProfileSyncPeerDiscoveryReport,
) -> Vec<ProfileSyncPeerDiscoveryResult> {
    let mut freshest_by_key = BTreeMap::<ProfileSyncPeerDiscoveryFreshnessKey, usize>::new();
    let mut freshest = Vec::<Option<ProfileSyncPeerDiscoveryResult>>::new();

    for candidate in candidates {
        let key = ProfileSyncPeerDiscoveryFreshnessKey::from_result(&candidate);
        let candidate_sequence = candidate.advertisement.sequence;
        if let Some(existing_index) = freshest_by_key.get(&key).copied() {
            let existing = freshest[existing_index]
                .as_ref()
                .expect("freshness key should point at an accepted discovery candidate");
            let existing_sequence = existing.advertisement.sequence;
            if candidate_sequence > existing_sequence {
                let stale = freshest[existing_index]
                    .replace(candidate)
                    .expect("freshness key should point at an accepted discovery candidate");
                report.rejected_peers.push(rejected_discovery_candidate(
                    stale,
                    ProfileSyncPeerDiscoveryTrustRejection::StaleDiscoverySequence,
                ));
            } else if candidate_sequence == existing_sequence {
                report.rejected_peers.push(rejected_discovery_candidate(
                    candidate,
                    ProfileSyncPeerDiscoveryTrustRejection::ReplayedDiscoverySequence,
                ));
            } else {
                report.rejected_peers.push(rejected_discovery_candidate(
                    candidate,
                    ProfileSyncPeerDiscoveryTrustRejection::StaleDiscoverySequence,
                ));
            }
        } else {
            freshest_by_key.insert(key, freshest.len());
            freshest.push(Some(candidate));
        }
    }

    freshest.into_iter().flatten().collect()
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct ProfileSyncPeerDiscoveryFreshnessKey {
    protocol: ProfileSyncPeerDiscoveryProtocol,
    namespace: String,
    network_id: String,
    node_id: String,
    provider_id: String,
}

impl ProfileSyncPeerDiscoveryFreshnessKey {
    fn from_result(result: &ProfileSyncPeerDiscoveryResult) -> Self {
        Self {
            protocol: result.protocol,
            namespace: result.namespace.clone(),
            network_id: result.advertisement.network_id.clone(),
            node_id: result.advertisement.node_id.clone(),
            provider_id: result.advertisement.provider_id.clone(),
        }
    }
}

fn rejected_discovery_candidate(
    candidate: ProfileSyncPeerDiscoveryResult,
    reason: ProfileSyncPeerDiscoveryTrustRejection,
) -> RejectedProfileSyncPeerDiscoveryCandidate {
    RejectedProfileSyncPeerDiscoveryCandidate {
        protocol: candidate.protocol,
        namespace: candidate.namespace,
        network_id: candidate.advertisement.network_id,
        node_id: candidate.advertisement.node_id,
        provider_id: candidate.advertisement.provider_id,
        reason,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ProfileSyncPeerAdvertisementSignatureError, ProfileSyncPeerDiscoveryTrustRejection,
        discover_trusted_profile_sync_peers,
        discover_trusted_profile_sync_peers_with_required_capabilities,
        filter_trusted_profile_sync_peer_discovery_results, sign_profile_sync_peer_advertisement,
    };
    use slate_broadwebd::{
        DEFAULT_PROFILE_SYNC_DISCOVERY_NAMESPACE,
        DEFAULT_PROFILE_SYNC_PEER_ADVERTISEMENT_MEMBERSHIP_EPOCH,
        PROFILE_SYNC_PEER_DISCOVERY_CAPABILITY,
        PROFILE_SYNC_PEER_DISCOVERY_CAPABILITY_LOCAL_RETENTION,
        PROFILE_SYNC_PEER_DISCOVERY_CAPABILITY_OBJECT_TRANSFER, ProfileSyncPeerAdvertisement,
        ProfileSyncPeerDiscoveryProtocol, ProfileSyncPeerDiscoveryProvider,
        ProfileSyncPeerDiscoveryQuery, ProfileSyncPeerDiscoveryResult,
        test_fixtures::SimulatedProfileSyncPeerDiscoveryNetwork,
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
        let future_epoch_signer =
            ProfileSyncDeviceSigner::generate("peer-trust-future-epoch").expect("future signer");
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
            .register_sync_device_public_key(&SyncDevicePublicKeyRegistration {
                profile: profile.to_string(),
                public_key: future_epoch_signer
                    .public_key()
                    .expect("future-epoch public key"),
                membership_epoch: DEFAULT_PROFILE_SYNC_PEER_ADVERTISEMENT_MEMBERSHIP_EPOCH + 1,
            })
            .expect("register future-epoch peer public key");
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
            test_signed_discovery_result(
                network_id,
                &future_epoch_signer,
                "peer-trust-provider-i",
                "/ip4/127.0.0.1/tcp/9409",
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
        assert_eq!(report.rejected_peer_count(), 9);
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
                (
                    future_epoch_signer.device_id(),
                    "peer-trust-provider-i",
                    ProfileSyncPeerDiscoveryTrustRejection::SignerMembershipEpochTooNew,
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

    #[test]
    fn trusted_profile_sync_peer_discovery_prefers_fresh_sequence_and_rejects_replays() {
        let db_root = test_state_root("trusted-peer-discovery-freshness-db");
        let database = SlateProfileDatabase::open_resolved_with_device_id(
            db_root.join(DEFAULT_DATABASE_FILE_NAME),
            "peer-trust-freshness-local-device",
        )
        .expect("open peer freshness database");
        let profile = "peerfreshnessprofile";
        let network_id = "peerfreshnessnetwork";
        let trusted_signer =
            ProfileSyncDeviceSigner::generate("peer-trust-freshness-remote").expect("trusted");
        database
            .register_sync_device_public_key(&SyncDevicePublicKeyRegistration {
                profile: profile.to_string(),
                public_key: trusted_signer.public_key().expect("trusted public key"),
                membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            })
            .expect("register trusted discovery signer");

        let candidates = vec![
            test_signed_discovery_result_with_sequence(
                network_id,
                &trusted_signer,
                "peer-trust-freshness-provider",
                "/dnsaddr/rendezvous.local/tcp/443/wss/p2p/stale-peer",
                1,
            ),
            test_signed_discovery_result_with_sequence(
                network_id,
                &trusted_signer,
                "peer-trust-freshness-provider",
                "/dnsaddr/rendezvous.local/tcp/443/wss/p2p/fresh-peer",
                3,
            ),
            test_signed_discovery_result_with_sequence(
                network_id,
                &trusted_signer,
                "peer-trust-freshness-provider",
                "/dnsaddr/rendezvous.local/tcp/443/wss/p2p/replayed-peer",
                3,
            ),
            test_signed_discovery_result_with_sequence(
                network_id,
                &trusted_signer,
                "peer-trust-freshness-provider",
                "/dnsaddr/rendezvous.local/tcp/443/wss/p2p/stale-peer-again",
                2,
            ),
        ];

        let report = filter_trusted_profile_sync_peer_discovery_results(
            &database, profile, network_id, candidates,
        )
        .expect("filter trusted peer discovery freshness");

        assert_eq!(report.trusted_peer_count(), 1);
        assert_eq!(report.rejected_peer_count(), 3);
        assert_eq!(report.trusted_peers[0].advertisement.sequence, 3);
        assert_eq!(
            report.trusted_peers[0].advertisement.service_addr,
            "/dnsaddr/rendezvous.local/tcp/443/wss/p2p/fresh-peer"
        );
        assert_eq!(
            report
                .rejected_peers
                .iter()
                .map(|rejection| rejection.reason)
                .collect::<Vec<_>>(),
            vec![
                ProfileSyncPeerDiscoveryTrustRejection::StaleDiscoverySequence,
                ProfileSyncPeerDiscoveryTrustRejection::ReplayedDiscoverySequence,
                ProfileSyncPeerDiscoveryTrustRejection::StaleDiscoverySequence,
            ]
        );

        let _ = std::fs::remove_dir_all(db_root);
    }

    #[test]
    fn trusted_profile_sync_peer_discovery_runs_provider_then_filters_signed_results() {
        let db_root = test_state_root("trusted-peer-discovery-provider-db");
        let database = SlateProfileDatabase::open_resolved_with_device_id(
            db_root.join(DEFAULT_DATABASE_FILE_NAME),
            "peer-trust-provider-local-device",
        )
        .expect("open peer discovery provider database");
        let profile = "peertrustproviderprofile";
        let network_id = "peertrustprovidernetwork";
        let trusted_signer =
            ProfileSyncDeviceSigner::generate("peer-trust-provider-remote").expect("trusted");
        let unknown_signer =
            ProfileSyncDeviceSigner::generate("peer-trust-provider-unknown").expect("unknown");
        database
            .register_sync_device_public_key(&SyncDevicePublicKeyRegistration {
                profile: profile.to_string(),
                public_key: trusted_signer.public_key().expect("trusted public key"),
                membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            })
            .expect("register trusted discovery signer");

        let network = SimulatedProfileSyncPeerDiscoveryNetwork::new();
        let provider = network.provider();
        provider
            .publish_profile_sync_peer(
                ProfileSyncPeerDiscoveryProtocol::IrohRendezvous,
                DEFAULT_PROFILE_SYNC_DISCOVERY_NAMESPACE,
                sign_profile_sync_peer_advertisement(
                    ProfileSyncPeerAdvertisement::new(
                        network_id,
                        trusted_signer.device_id(),
                        "peer-trust-provider-retention-a",
                        "/dnsaddr/iroh-rendezvous.local/tcp/443/wss/p2p/trusted-peer",
                        1,
                    )
                    .expect("trusted advertisement"),
                    &trusted_signer,
                )
                .expect("sign trusted advertisement"),
            )
            .expect("publish trusted advertisement");
        provider
            .publish_profile_sync_peer(
                ProfileSyncPeerDiscoveryProtocol::IrohRendezvous,
                DEFAULT_PROFILE_SYNC_DISCOVERY_NAMESPACE,
                sign_profile_sync_peer_advertisement(
                    ProfileSyncPeerAdvertisement::new(
                        network_id,
                        unknown_signer.device_id(),
                        "peer-trust-provider-retention-b",
                        "/dnsaddr/iroh-rendezvous.local/tcp/443/wss/p2p/unknown-peer",
                        1,
                    )
                    .expect("unknown advertisement"),
                    &unknown_signer,
                )
                .expect("sign unknown advertisement"),
            )
            .expect("publish unknown advertisement");

        let query = ProfileSyncPeerDiscoveryQuery::for_default_namespace(
            network_id,
            "peer-trust-provider-local-device",
            [ProfileSyncPeerDiscoveryProtocol::IrohRendezvous],
            8,
        )
        .expect("discovery query");
        let report = discover_trusted_profile_sync_peers(&database, profile, &provider, &query)
            .expect("discover and filter trusted peers");

        assert_eq!(report.trusted_peer_count(), 1);
        assert_eq!(report.rejected_peer_count(), 1);
        assert_eq!(
            report.trusted_peers[0].advertisement.provider_id,
            "peer-trust-provider-retention-a"
        );
        assert_eq!(
            report.rejected_peers[0].reason,
            ProfileSyncPeerDiscoveryTrustRejection::UnknownDevicePublicKey
        );

        let _ = std::fs::remove_dir_all(db_root);
    }

    #[test]
    fn trusted_profile_sync_peer_discovery_can_require_role_capabilities() {
        let db_root = test_state_root("trusted-peer-discovery-provider-roles-db");
        let database = SlateProfileDatabase::open_resolved_with_device_id(
            db_root.join(DEFAULT_DATABASE_FILE_NAME),
            "peer-trust-provider-roles-local-device",
        )
        .expect("open peer discovery roles database");
        let profile = "peertrustproviderrolesprofile";
        let network_id = "peertrustproviderrolesnetwork";
        let trusted_signer = ProfileSyncDeviceSigner::generate("peer-trust-provider-roles-remote")
            .expect("trusted role signer");
        database
            .register_sync_device_public_key(&SyncDevicePublicKeyRegistration {
                profile: profile.to_string(),
                public_key: trusted_signer.public_key().expect("trusted public key"),
                membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            })
            .expect("register trusted discovery role signer");

        let network = SimulatedProfileSyncPeerDiscoveryNetwork::new();
        let provider = network.provider();
        provider
            .publish_profile_sync_peer(
                ProfileSyncPeerDiscoveryProtocol::IrohRendezvous,
                DEFAULT_PROFILE_SYNC_DISCOVERY_NAMESPACE,
                test_signed_discovery_advertisement_with_capabilities(
                    network_id,
                    &trusted_signer,
                    "peer-trust-provider-roles-transfer-only",
                    "/dnsaddr/iroh-rendezvous.local/tcp/443/wss/p2p/transfer-only",
                    [
                        PROFILE_SYNC_PEER_DISCOVERY_CAPABILITY,
                        PROFILE_SYNC_PEER_DISCOVERY_CAPABILITY_OBJECT_TRANSFER,
                    ],
                ),
            )
            .expect("publish transfer-only advertisement");
        provider
            .publish_profile_sync_peer(
                ProfileSyncPeerDiscoveryProtocol::IrohRendezvous,
                DEFAULT_PROFILE_SYNC_DISCOVERY_NAMESPACE,
                test_signed_discovery_advertisement_with_capabilities(
                    network_id,
                    &trusted_signer,
                    "peer-trust-provider-roles-retention",
                    "/dnsaddr/iroh-rendezvous.local/tcp/443/wss/p2p/retention",
                    [
                        PROFILE_SYNC_PEER_DISCOVERY_CAPABILITY,
                        PROFILE_SYNC_PEER_DISCOVERY_CAPABILITY_OBJECT_TRANSFER,
                        PROFILE_SYNC_PEER_DISCOVERY_CAPABILITY_LOCAL_RETENTION,
                    ],
                ),
            )
            .expect("publish retention advertisement");

        let query = ProfileSyncPeerDiscoveryQuery::for_default_namespace(
            network_id,
            "peer-trust-provider-roles-local-device",
            [ProfileSyncPeerDiscoveryProtocol::IrohRendezvous],
            8,
        )
        .expect("discovery query");
        let required_capabilities = vec![
            PROFILE_SYNC_PEER_DISCOVERY_CAPABILITY_OBJECT_TRANSFER.to_string(),
            PROFILE_SYNC_PEER_DISCOVERY_CAPABILITY_LOCAL_RETENTION.to_string(),
        ];
        let report = discover_trusted_profile_sync_peers_with_required_capabilities(
            &database,
            profile,
            &provider,
            &query,
            required_capabilities.as_slice(),
        )
        .expect("discover and filter trusted peers with required roles");

        assert_eq!(report.trusted_peer_count(), 1);
        assert_eq!(report.rejected_peer_count(), 1);
        assert_eq!(
            report.trusted_peers[0].advertisement.provider_id,
            "peer-trust-provider-roles-retention"
        );
        assert_eq!(
            report.rejected_peers[0].provider_id,
            "peer-trust-provider-roles-transfer-only"
        );
        assert_eq!(
            report.rejected_peers[0].reason,
            ProfileSyncPeerDiscoveryTrustRejection::MissingRequiredCapability
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

    fn test_signed_discovery_result(
        network_id: &str,
        signer: &ProfileSyncDeviceSigner,
        provider_id: &str,
        service_addr: &str,
    ) -> ProfileSyncPeerDiscoveryResult {
        test_signed_discovery_result_with_sequence(network_id, signer, provider_id, service_addr, 1)
    }

    fn test_signed_discovery_result_with_sequence(
        network_id: &str,
        signer: &ProfileSyncDeviceSigner,
        provider_id: &str,
        service_addr: &str,
        sequence: u64,
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
                    sequence,
                )
                .expect("build profile-sync discovery test advertisement"),
                signer,
            )
            .expect("sign profile-sync discovery test advertisement"),
        }
    }

    fn test_signed_discovery_advertisement_with_capabilities(
        network_id: &str,
        signer: &ProfileSyncDeviceSigner,
        provider_id: &str,
        service_addr: &str,
        capabilities: impl IntoIterator<Item = &'static str>,
    ) -> ProfileSyncPeerAdvertisement {
        sign_profile_sync_peer_advertisement(
            ProfileSyncPeerAdvertisement::with_capabilities(
                network_id,
                signer.device_id(),
                provider_id,
                service_addr,
                capabilities,
                1,
            )
            .expect("build profile-sync discovery test advertisement"),
            signer,
        )
        .expect("sign profile-sync discovery test advertisement")
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
