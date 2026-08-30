use crate::{
    ProfileSyncPeerDiscoveryTrustRejection, RejectedProfileSyncPeerDiscoveryCandidate,
    SettingsSyncCycleProviderRetentionIssue, SettingsSyncCycleProviderRetentionIssueKind,
    SettingsSyncHealthIssueComponent, SettingsSyncHealthReport,
    SettingsSyncRetentionProviderSelectionIssue, SettingsSyncRetentionProviderSelectionIssueKind,
    SettingsSyncRootObjectProviderIssue, SettingsSyncRootObjectProviderIssueKind,
    SettingsSyncStoredRetentionProviderMetadataIssue,
    SettingsSyncStoredRetentionProviderMetadataIssueKind, TrustedProfileSyncPeerDiscoveryReport,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalSettingsSyncRootObjectProviderIssueSummary {
    pub component: String,
    pub root_id: String,
    pub object_id: Option<String>,
    pub provider_id: String,
    pub kind: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalSettingsSyncProviderIssueSummary {
    pub category: String,
    pub provider_id: String,
    pub kind: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalSettingsSyncDiscoveryRejectionSummary {
    pub protocol: String,
    pub namespace: String,
    pub node_id: String,
    pub provider_id: String,
    pub reason: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LocalSettingsSyncDiscoverySummary {
    pub protocols: Vec<String>,
    pub trusted_peer_count: usize,
    pub rejected_peer_count: usize,
    pub selected_retention_provider_count: usize,
    pub endpoint_ready_provider_count: usize,
    pub endpoint_pending_protocol_provider_count: usize,
    pub endpoint_missing_provider_count: usize,
    pub endpoint_fail_closed_provider_count: usize,
    pub endpoint_requires_protocol_materializer: bool,
    pub retention_provider_selection_issue_count: usize,
    pub rejections: Vec<LocalSettingsSyncDiscoveryRejectionSummary>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalSettingsSyncRetentionIssueSummary {
    pub provider_index: usize,
    pub object_id: String,
    pub kind: String,
}

impl LocalSettingsSyncRootObjectProviderIssueSummary {
    fn from_issue(issue: SettingsSyncRootObjectProviderIssue) -> Self {
        Self {
            component: settings_sync_health_issue_component_name(issue.component).to_string(),
            root_id: issue.root_id,
            object_id: issue.object_id,
            provider_id: issue.provider_id,
            kind: settings_sync_root_object_provider_issue_kind_name(issue.kind).to_string(),
        }
    }
}

impl LocalSettingsSyncProviderIssueSummary {
    fn from_retention_provider_selection_issue(
        issue: SettingsSyncRetentionProviderSelectionIssue,
    ) -> Self {
        Self {
            category: "retention_provider_selection".to_string(),
            provider_id: issue.provider_id,
            kind: settings_sync_retention_provider_selection_issue_kind_name(issue.kind)
                .to_string(),
        }
    }

    fn from_stored_provider_metadata_issue(
        issue: SettingsSyncStoredRetentionProviderMetadataIssue,
    ) -> Self {
        Self {
            category: "stored_provider_metadata".to_string(),
            provider_id: issue.provider_id,
            kind: settings_sync_stored_provider_metadata_issue_kind_name(issue.kind).to_string(),
        }
    }
}

impl LocalSettingsSyncDiscoveryRejectionSummary {
    fn from_rejection(rejection: &RejectedProfileSyncPeerDiscoveryCandidate) -> Self {
        Self {
            protocol: rejection.protocol.as_str().to_string(),
            namespace: rejection.namespace.clone(),
            node_id: rejection.node_id.clone(),
            provider_id: rejection.provider_id.clone(),
            reason: profile_sync_peer_discovery_trust_rejection_name(rejection.reason).to_string(),
        }
    }
}

impl LocalSettingsSyncRetentionIssueSummary {
    fn from_issue(issue: SettingsSyncCycleProviderRetentionIssue) -> Self {
        Self {
            provider_index: issue.provider_index,
            object_id: issue.object_id,
            kind: settings_sync_cycle_provider_retention_issue_kind_name(issue.kind).to_string(),
        }
    }
}

pub(crate) fn local_settings_sync_discovery_rejection_summaries(
    report: &TrustedProfileSyncPeerDiscoveryReport,
) -> Vec<LocalSettingsSyncDiscoveryRejectionSummary> {
    report
        .rejected_peers
        .iter()
        .map(LocalSettingsSyncDiscoveryRejectionSummary::from_rejection)
        .collect()
}

pub(crate) fn local_settings_sync_root_object_provider_issue_summaries(
    health: &SettingsSyncHealthReport,
) -> Vec<LocalSettingsSyncRootObjectProviderIssueSummary> {
    health
        .root_object_provider_issues()
        .into_iter()
        .map(LocalSettingsSyncRootObjectProviderIssueSummary::from_issue)
        .collect()
}

pub(crate) fn local_settings_sync_retention_provider_selection_issue_summaries(
    issues: Vec<SettingsSyncRetentionProviderSelectionIssue>,
) -> Vec<LocalSettingsSyncProviderIssueSummary> {
    issues
        .into_iter()
        .map(LocalSettingsSyncProviderIssueSummary::from_retention_provider_selection_issue)
        .collect()
}

pub(crate) fn local_settings_sync_stored_provider_metadata_issue_summaries(
    issues: Vec<SettingsSyncStoredRetentionProviderMetadataIssue>,
) -> Vec<LocalSettingsSyncProviderIssueSummary> {
    issues
        .into_iter()
        .map(LocalSettingsSyncProviderIssueSummary::from_stored_provider_metadata_issue)
        .collect()
}

pub(crate) fn local_settings_sync_retention_issue_summaries(
    issues: Vec<SettingsSyncCycleProviderRetentionIssue>,
) -> Vec<LocalSettingsSyncRetentionIssueSummary> {
    issues
        .into_iter()
        .map(LocalSettingsSyncRetentionIssueSummary::from_issue)
        .collect()
}

fn settings_sync_health_issue_component_name(
    component: SettingsSyncHealthIssueComponent,
) -> &'static str {
    match component {
        SettingsSyncHealthIssueComponent::Providers => "providers",
        SettingsSyncHealthIssueComponent::SettingsRoot => "settings_root",
        SettingsSyncHealthIssueComponent::LocalDeviceHeadRoot => "local_device_head_root",
    }
}

fn settings_sync_root_object_provider_issue_kind_name(
    kind: SettingsSyncRootObjectProviderIssueKind,
) -> &'static str {
    match kind {
        SettingsSyncRootObjectProviderIssueKind::Delayed => "delayed",
        SettingsSyncRootObjectProviderIssueKind::Stale => "stale",
        SettingsSyncRootObjectProviderIssueKind::Offline => "offline",
        SettingsSyncRootObjectProviderIssueKind::RetainedUnavailable => "retained_unavailable",
    }
}

fn settings_sync_cycle_provider_retention_issue_kind_name(
    kind: SettingsSyncCycleProviderRetentionIssueKind,
) -> &'static str {
    match kind {
        SettingsSyncCycleProviderRetentionIssueKind::NotRetained => "not_retained",
        SettingsSyncCycleProviderRetentionIssueKind::NotAvailable => "not_available",
    }
}

fn settings_sync_retention_provider_selection_issue_kind_name(
    kind: SettingsSyncRetentionProviderSelectionIssueKind,
) -> &'static str {
    match kind {
        SettingsSyncRetentionProviderSelectionIssueKind::Stale => "stale",
        SettingsSyncRetentionProviderSelectionIssueKind::Offline => "offline",
        SettingsSyncRetentionProviderSelectionIssueKind::Ineligible => "ineligible",
        SettingsSyncRetentionProviderSelectionIssueKind::Undiscovered => "undiscovered",
        SettingsSyncRetentionProviderSelectionIssueKind::Duplicate => "duplicate",
    }
}

fn settings_sync_stored_provider_metadata_issue_kind_name(
    kind: SettingsSyncStoredRetentionProviderMetadataIssueKind,
) -> &'static str {
    match kind {
        SettingsSyncStoredRetentionProviderMetadataIssueKind::Disabled => "disabled",
        SettingsSyncStoredRetentionProviderMetadataIssueKind::StoredRoleIneligible => {
            "stored_role_ineligible"
        }
        SettingsSyncStoredRetentionProviderMetadataIssueKind::Unauthorized => "unauthorized",
    }
}

fn profile_sync_peer_discovery_trust_rejection_name(
    reason: ProfileSyncPeerDiscoveryTrustRejection,
) -> &'static str {
    match reason {
        ProfileSyncPeerDiscoveryTrustRejection::WrongNetwork => "wrong_network",
        ProfileSyncPeerDiscoveryTrustRejection::LocalDevice => "local_device",
        ProfileSyncPeerDiscoveryTrustRejection::MissingProfileSyncServiceFrameCapability => {
            "missing_profile_sync_service_frame_capability"
        }
        ProfileSyncPeerDiscoveryTrustRejection::StaleDiscoverySequence => {
            "stale_discovery_sequence"
        }
        ProfileSyncPeerDiscoveryTrustRejection::ReplayedDiscoverySequence => {
            "replayed_discovery_sequence"
        }
        ProfileSyncPeerDiscoveryTrustRejection::UnknownDevicePublicKey => {
            "unknown_device_public_key"
        }
        ProfileSyncPeerDiscoveryTrustRejection::UntrustedDevicePublicKey => {
            "untrusted_device_public_key"
        }
        ProfileSyncPeerDiscoveryTrustRejection::MissingSignedIdentity => "missing_signed_identity",
        ProfileSyncPeerDiscoveryTrustRejection::SignatureDeviceMismatch => {
            "signature_device_mismatch"
        }
        ProfileSyncPeerDiscoveryTrustRejection::SignaturePublicKeyMismatch => {
            "signature_public_key_mismatch"
        }
        ProfileSyncPeerDiscoveryTrustRejection::SignerMembershipEpochTooNew => {
            "signer_membership_epoch_too_new"
        }
        ProfileSyncPeerDiscoveryTrustRejection::InvalidSignature => "invalid_signature",
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalSettingsSyncPreviewCycleReport {
    pub profile: String,
    pub local_device_id: String,
    pub provider_id: String,
    pub provider_endpoint_ref: String,
    pub preview_setting_key: String,
    pub preview_setting_revision: i64,
    pub ready_for_manual_sync: bool,
    pub blocked_reason: Option<String>,
    pub pulled_membership_application_count: usize,
    pub discovery_protocols: Vec<String>,
    pub discovery_trusted_peer_count: usize,
    pub discovery_rejected_peer_count: usize,
    pub discovery_selected_retention_provider_count: usize,
    pub discovery_endpoint_ready_provider_count: usize,
    pub discovery_endpoint_pending_protocol_provider_count: usize,
    pub discovery_endpoint_missing_provider_count: usize,
    pub discovery_endpoint_fail_closed_provider_count: usize,
    pub discovery_endpoint_requires_protocol_materializer: bool,
    pub discovery_retention_provider_selection_issue_count: usize,
    pub discovery_rejections: Vec<LocalSettingsSyncDiscoveryRejectionSummary>,
    pub selected_retention_provider_count: usize,
    pub materialized_retention_provider_count: usize,
    pub retained_provider_count: usize,
    pub published_step_count: usize,
    pub published_object_count: usize,
    pub retained_object_count: usize,
    pub retention_issue_count: usize,
    pub retention_issues: Vec<LocalSettingsSyncRetentionIssueSummary>,
    pub fixture_materialization_issue_count: usize,
    pub retention_provider_selection_issue_count: usize,
    pub retention_provider_selection_issues: Vec<LocalSettingsSyncProviderIssueSummary>,
    pub stored_provider_metadata_issue_count: usize,
    pub stored_provider_metadata_issues: Vec<LocalSettingsSyncProviderIssueSummary>,
    pub all_fixture_providers_materialized: bool,
    pub selected_endpoint_ready_provider_count: usize,
    pub selected_endpoint_pending_protocol_provider_count: usize,
    pub selected_endpoint_missing_provider_count: usize,
    pub selected_endpoint_fail_closed_provider_count: usize,
    pub selected_endpoint_requires_protocol_materializer: bool,
    pub degraded_before: bool,
    pub degraded_after: bool,
    pub root_object_provider_issue_count: usize,
    pub root_object_provider_issues: Vec<LocalSettingsSyncRootObjectProviderIssueSummary>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalSettingsSyncCurrentCycleReport {
    pub profile: String,
    pub local_device_id: String,
    pub provider_id: String,
    pub provider_endpoint_ref: String,
    pub ready_for_manual_sync: bool,
    pub blocked_reason: Option<String>,
    pub pulled_membership_application_count: usize,
    pub discovery_protocols: Vec<String>,
    pub discovery_trusted_peer_count: usize,
    pub discovery_rejected_peer_count: usize,
    pub discovery_selected_retention_provider_count: usize,
    pub discovery_endpoint_ready_provider_count: usize,
    pub discovery_endpoint_pending_protocol_provider_count: usize,
    pub discovery_endpoint_missing_provider_count: usize,
    pub discovery_endpoint_fail_closed_provider_count: usize,
    pub discovery_endpoint_requires_protocol_materializer: bool,
    pub discovery_retention_provider_selection_issue_count: usize,
    pub discovery_rejections: Vec<LocalSettingsSyncDiscoveryRejectionSummary>,
    pub selected_retention_provider_count: usize,
    pub materialized_retention_provider_count: usize,
    pub retained_provider_count: usize,
    pub published_step_count: usize,
    pub published_object_count: usize,
    pub retained_object_count: usize,
    pub retention_issue_count: usize,
    pub retention_issues: Vec<LocalSettingsSyncRetentionIssueSummary>,
    pub fixture_materialization_issue_count: usize,
    pub retention_provider_selection_issue_count: usize,
    pub retention_provider_selection_issues: Vec<LocalSettingsSyncProviderIssueSummary>,
    pub stored_provider_metadata_issue_count: usize,
    pub stored_provider_metadata_issues: Vec<LocalSettingsSyncProviderIssueSummary>,
    pub all_fixture_providers_materialized: bool,
    pub selected_endpoint_ready_provider_count: usize,
    pub selected_endpoint_pending_protocol_provider_count: usize,
    pub selected_endpoint_missing_provider_count: usize,
    pub selected_endpoint_fail_closed_provider_count: usize,
    pub selected_endpoint_requires_protocol_materializer: bool,
    pub degraded_before: bool,
    pub degraded_after: bool,
    pub root_object_provider_issue_count: usize,
    pub root_object_provider_issues: Vec<LocalSettingsSyncRootObjectProviderIssueSummary>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalSettingsSyncTwoDevicePreviewCycleReport {
    pub profile: String,
    pub publisher_device_id: String,
    pub receiver_device_id: String,
    pub provider_id: String,
    pub provider_endpoint_ref: String,
    pub preview_setting_key: String,
    pub preview_setting_value: String,
    pub publisher_published_step_count: usize,
    pub publisher_published_object_count: usize,
    pub publisher_retained_object_count: usize,
    pub publisher_retained_provider_count: usize,
    pub receiver_device_request_device_id: String,
    pub receiver_enrollment_bundle_record_count: usize,
    pub receiver_pulled_membership_application_count: usize,
    pub discovery_protocols: Vec<String>,
    pub discovery_trusted_peer_count: usize,
    pub discovery_rejected_peer_count: usize,
    pub discovery_selected_retention_provider_count: usize,
    pub discovery_endpoint_ready_provider_count: usize,
    pub discovery_endpoint_pending_protocol_provider_count: usize,
    pub discovery_endpoint_missing_provider_count: usize,
    pub discovery_endpoint_fail_closed_provider_count: usize,
    pub discovery_endpoint_requires_protocol_materializer: bool,
    pub discovery_retention_provider_selection_issue_count: usize,
    pub discovery_rejections: Vec<LocalSettingsSyncDiscoveryRejectionSummary>,
    pub receiver_applied_setting_count: usize,
    pub receiver_published_step_count: usize,
    pub receiver_received_value: Option<String>,
    pub receiver_membership_record_count: usize,
    pub receiver_trusted_device_count: usize,
    pub retention_issue_count: usize,
    pub retention_issues: Vec<LocalSettingsSyncRetentionIssueSummary>,
    pub fixture_materialization_issue_count: usize,
    pub retention_provider_selection_issue_count: usize,
    pub retention_provider_selection_issues: Vec<LocalSettingsSyncProviderIssueSummary>,
    pub stored_provider_metadata_issue_count: usize,
    pub stored_provider_metadata_issues: Vec<LocalSettingsSyncProviderIssueSummary>,
    pub all_fixture_providers_materialized: bool,
    pub selected_endpoint_ready_provider_count: usize,
    pub selected_endpoint_pending_protocol_provider_count: usize,
    pub selected_endpoint_missing_provider_count: usize,
    pub selected_endpoint_fail_closed_provider_count: usize,
    pub selected_endpoint_requires_protocol_materializer: bool,
    pub degraded_before: bool,
    pub degraded_after: bool,
    pub root_object_provider_issue_count: usize,
    pub root_object_provider_issues: Vec<LocalSettingsSyncRootObjectProviderIssueSummary>,
}

#[cfg(test)]
mod tests {
    use super::{
        LocalSettingsSyncProviderIssueSummary, LocalSettingsSyncRetentionIssueSummary,
        LocalSettingsSyncRootObjectProviderIssueSummary,
    };
    use crate::{
        SettingsSyncCycleProviderRetentionIssue, SettingsSyncCycleProviderRetentionIssueKind,
        SettingsSyncHealthIssueComponent, SettingsSyncRetentionProviderSelectionIssue,
        SettingsSyncRetentionProviderSelectionIssueKind, SettingsSyncRootObjectProviderIssue,
        SettingsSyncRootObjectProviderIssueKind, SettingsSyncStoredRetentionProviderMetadataIssue,
        SettingsSyncStoredRetentionProviderMetadataIssueKind,
    };

    #[test]
    fn local_settings_sync_root_object_provider_issue_summary_names_states() {
        let summary = LocalSettingsSyncRootObjectProviderIssueSummary::from_issue(
            SettingsSyncRootObjectProviderIssue {
                component: SettingsSyncHealthIssueComponent::SettingsRoot,
                root_id: "settings/latest".to_string(),
                object_id: Some("bafyfixture123".to_string()),
                provider_id: "provider-a".to_string(),
                kind: SettingsSyncRootObjectProviderIssueKind::Offline,
            },
        );

        assert_eq!(summary.component, "settings_root");
        assert_eq!(summary.root_id, "settings/latest");
        assert_eq!(summary.object_id.as_deref(), Some("bafyfixture123"));
        assert_eq!(summary.provider_id, "provider-a");
        assert_eq!(summary.kind, "offline");

        let retained_unavailable = LocalSettingsSyncRootObjectProviderIssueSummary::from_issue(
            SettingsSyncRootObjectProviderIssue {
                component: SettingsSyncHealthIssueComponent::SettingsRoot,
                root_id: "settings/latest".to_string(),
                object_id: Some("bafyfixture456".to_string()),
                provider_id: "provider-b".to_string(),
                kind: SettingsSyncRootObjectProviderIssueKind::RetainedUnavailable,
            },
        );
        assert_eq!(retained_unavailable.kind, "retained_unavailable");
    }

    #[test]
    fn local_settings_sync_retention_issue_summary_names_states() {
        let not_retained = LocalSettingsSyncRetentionIssueSummary::from_issue(
            SettingsSyncCycleProviderRetentionIssue {
                provider_index: 2,
                object_id: "bafyfixture123".to_string(),
                kind: SettingsSyncCycleProviderRetentionIssueKind::NotRetained,
            },
        );
        let not_available = LocalSettingsSyncRetentionIssueSummary::from_issue(
            SettingsSyncCycleProviderRetentionIssue {
                provider_index: 3,
                object_id: "bafyfixture456".to_string(),
                kind: SettingsSyncCycleProviderRetentionIssueKind::NotAvailable,
            },
        );

        assert_eq!(not_retained.provider_index, 2);
        assert_eq!(not_retained.object_id, "bafyfixture123");
        assert_eq!(not_retained.kind, "not_retained");
        assert_eq!(not_available.provider_index, 3);
        assert_eq!(not_available.object_id, "bafyfixture456");
        assert_eq!(not_available.kind, "not_available");
    }

    #[test]
    fn local_settings_sync_provider_issue_summary_names_states() {
        let selection =
            LocalSettingsSyncProviderIssueSummary::from_retention_provider_selection_issue(
                SettingsSyncRetentionProviderSelectionIssue {
                    provider_id: "provider-a".to_string(),
                    kind: SettingsSyncRetentionProviderSelectionIssueKind::Undiscovered,
                },
            );
        let metadata = LocalSettingsSyncProviderIssueSummary::from_stored_provider_metadata_issue(
            SettingsSyncStoredRetentionProviderMetadataIssue {
                provider_id: "provider-b".to_string(),
                kind: SettingsSyncStoredRetentionProviderMetadataIssueKind::Unauthorized,
            },
        );

        assert_eq!(selection.category, "retention_provider_selection");
        assert_eq!(selection.provider_id, "provider-a");
        assert_eq!(selection.kind, "undiscovered");
        assert_eq!(metadata.category, "stored_provider_metadata");
        assert_eq!(metadata.provider_id, "provider-b");
        assert_eq!(metadata.kind, "unauthorized");
    }
}
