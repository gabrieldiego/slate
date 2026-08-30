use crate::{
    LocalSettingsSyncCurrentCycleReport, LocalSettingsSyncDiscoveryRejectionSummary,
    LocalSettingsSyncPreviewCycleReport, LocalSettingsSyncProviderIssueSummary,
    LocalSettingsSyncRetentionIssueSummary, LocalSettingsSyncRootObjectProviderIssueSummary,
    LocalSettingsSyncTwoDevicePreviewCycleReport,
};

pub fn local_settings_sync_current_cycle_report_json(
    report: &LocalSettingsSyncCurrentCycleReport,
    completed_at: i64,
) -> serde_json::Value {
    serde_json::json!({
        "completed_at": completed_at,
        "provider_id": report.provider_id.as_str(),
        "provider_endpoint_ref": report.provider_endpoint_ref.as_str(),
        "ready_for_manual_sync": report.ready_for_manual_sync,
        "pulled_membership_application_count": report.pulled_membership_application_count,
        "discovery_protocols": report.discovery_protocols.as_slice(),
        "discovery_trusted_peer_count": report.discovery_trusted_peer_count,
        "discovery_rejected_peer_count": report.discovery_rejected_peer_count,
        "discovery_selected_retention_provider_count": report.discovery_selected_retention_provider_count,
        "discovery_endpoint_ready_provider_count": report.discovery_endpoint_ready_provider_count,
        "discovery_endpoint_pending_protocol_provider_count": report.discovery_endpoint_pending_protocol_provider_count,
        "discovery_endpoint_missing_provider_count": report.discovery_endpoint_missing_provider_count,
        "discovery_endpoint_fail_closed_provider_count": report.discovery_endpoint_fail_closed_provider_count,
        "discovery_endpoint_requires_protocol_materializer": report.discovery_endpoint_requires_protocol_materializer,
        "discovery_retention_provider_selection_issue_count": report.discovery_retention_provider_selection_issue_count,
        "discovery_rejections": local_settings_sync_discovery_rejections_json(report.discovery_rejections.as_slice()),
        "selected_retention_provider_count": report.selected_retention_provider_count,
        "materialized_retention_provider_count": report.materialized_retention_provider_count,
        "retained_provider_count": report.retained_provider_count,
        "published_step_count": report.published_step_count,
        "published_object_count": report.published_object_count,
        "retained_object_count": report.retained_object_count,
        "retention_issue_count": report.retention_issue_count,
        "retention_issues": local_settings_sync_retention_issues_json(report.retention_issues.as_slice()),
        "fixture_materialization_issue_count": report.fixture_materialization_issue_count,
        "retention_provider_selection_issue_count": report.retention_provider_selection_issue_count,
        "retention_provider_selection_issues": local_settings_sync_provider_issues_json(report.retention_provider_selection_issues.as_slice()),
        "stored_provider_metadata_issue_count": report.stored_provider_metadata_issue_count,
        "stored_provider_metadata_issues": local_settings_sync_provider_issues_json(report.stored_provider_metadata_issues.as_slice()),
        "all_fixture_providers_materialized": report.all_fixture_providers_materialized,
        "selected_endpoint_ready_provider_count": report.selected_endpoint_ready_provider_count,
        "selected_endpoint_pending_protocol_provider_count": report.selected_endpoint_pending_protocol_provider_count,
        "selected_endpoint_missing_provider_count": report.selected_endpoint_missing_provider_count,
        "selected_endpoint_fail_closed_provider_count": report.selected_endpoint_fail_closed_provider_count,
        "selected_endpoint_requires_protocol_materializer": report.selected_endpoint_requires_protocol_materializer,
        "degraded_before": report.degraded_before,
        "degraded_after": report.degraded_after,
        "root_object_provider_issue_count": report.root_object_provider_issue_count,
        "root_object_provider_issues": local_settings_sync_root_object_provider_issues_json(report.root_object_provider_issues.as_slice()),
    })
}

pub fn local_settings_sync_preview_cycle_report_json(
    report: &LocalSettingsSyncPreviewCycleReport,
    completed_at: i64,
) -> serde_json::Value {
    serde_json::json!({
        "completed_at": completed_at,
        "provider_id": report.provider_id.as_str(),
        "provider_endpoint_ref": report.provider_endpoint_ref.as_str(),
        "preview_setting_key": report.preview_setting_key.as_str(),
        "preview_setting_revision": report.preview_setting_revision,
        "ready_for_manual_sync": report.ready_for_manual_sync,
        "pulled_membership_application_count": report.pulled_membership_application_count,
        "discovery_protocols": report.discovery_protocols.as_slice(),
        "discovery_trusted_peer_count": report.discovery_trusted_peer_count,
        "discovery_rejected_peer_count": report.discovery_rejected_peer_count,
        "discovery_selected_retention_provider_count": report.discovery_selected_retention_provider_count,
        "discovery_endpoint_ready_provider_count": report.discovery_endpoint_ready_provider_count,
        "discovery_endpoint_pending_protocol_provider_count": report.discovery_endpoint_pending_protocol_provider_count,
        "discovery_endpoint_missing_provider_count": report.discovery_endpoint_missing_provider_count,
        "discovery_endpoint_fail_closed_provider_count": report.discovery_endpoint_fail_closed_provider_count,
        "discovery_endpoint_requires_protocol_materializer": report.discovery_endpoint_requires_protocol_materializer,
        "discovery_retention_provider_selection_issue_count": report.discovery_retention_provider_selection_issue_count,
        "discovery_rejections": local_settings_sync_discovery_rejections_json(report.discovery_rejections.as_slice()),
        "selected_retention_provider_count": report.selected_retention_provider_count,
        "materialized_retention_provider_count": report.materialized_retention_provider_count,
        "retained_provider_count": report.retained_provider_count,
        "published_step_count": report.published_step_count,
        "published_object_count": report.published_object_count,
        "retained_object_count": report.retained_object_count,
        "retention_issue_count": report.retention_issue_count,
        "retention_issues": local_settings_sync_retention_issues_json(report.retention_issues.as_slice()),
        "fixture_materialization_issue_count": report.fixture_materialization_issue_count,
        "retention_provider_selection_issue_count": report.retention_provider_selection_issue_count,
        "retention_provider_selection_issues": local_settings_sync_provider_issues_json(report.retention_provider_selection_issues.as_slice()),
        "stored_provider_metadata_issue_count": report.stored_provider_metadata_issue_count,
        "stored_provider_metadata_issues": local_settings_sync_provider_issues_json(report.stored_provider_metadata_issues.as_slice()),
        "all_fixture_providers_materialized": report.all_fixture_providers_materialized,
        "selected_endpoint_ready_provider_count": report.selected_endpoint_ready_provider_count,
        "selected_endpoint_pending_protocol_provider_count": report.selected_endpoint_pending_protocol_provider_count,
        "selected_endpoint_missing_provider_count": report.selected_endpoint_missing_provider_count,
        "selected_endpoint_fail_closed_provider_count": report.selected_endpoint_fail_closed_provider_count,
        "selected_endpoint_requires_protocol_materializer": report.selected_endpoint_requires_protocol_materializer,
        "degraded_before": report.degraded_before,
        "degraded_after": report.degraded_after,
        "root_object_provider_issue_count": report.root_object_provider_issue_count,
        "root_object_provider_issues": local_settings_sync_root_object_provider_issues_json(report.root_object_provider_issues.as_slice()),
    })
}

pub fn local_settings_sync_two_device_preview_cycle_report_json(
    report: &LocalSettingsSyncTwoDevicePreviewCycleReport,
    completed_at: i64,
) -> serde_json::Value {
    let mut object = serde_json::Map::new();
    local_settings_sync_json_insert(&mut object, "completed_at", completed_at);
    local_settings_sync_json_insert(
        &mut object,
        "publisher_device_id",
        report.publisher_device_id.as_str(),
    );
    local_settings_sync_json_insert(
        &mut object,
        "receiver_device_id",
        report.receiver_device_id.as_str(),
    );
    local_settings_sync_json_insert(
        &mut object,
        "receiver_device_request_device_id",
        report.receiver_device_request_device_id.as_str(),
    );
    local_settings_sync_json_insert(&mut object, "provider_id", report.provider_id.as_str());
    local_settings_sync_json_insert(
        &mut object,
        "provider_endpoint_ref",
        report.provider_endpoint_ref.as_str(),
    );
    local_settings_sync_json_insert(
        &mut object,
        "preview_setting_key",
        report.preview_setting_key.as_str(),
    );
    local_settings_sync_json_insert(
        &mut object,
        "preview_setting_value",
        report.preview_setting_value.as_str(),
    );
    local_settings_sync_json_insert(
        &mut object,
        "publisher_published_step_count",
        report.publisher_published_step_count,
    );
    local_settings_sync_json_insert(
        &mut object,
        "publisher_published_object_count",
        report.publisher_published_object_count,
    );
    local_settings_sync_json_insert(
        &mut object,
        "publisher_retained_object_count",
        report.publisher_retained_object_count,
    );
    local_settings_sync_json_insert(
        &mut object,
        "publisher_retained_provider_count",
        report.publisher_retained_provider_count,
    );
    local_settings_sync_json_insert(
        &mut object,
        "receiver_enrollment_bundle_record_count",
        report.receiver_enrollment_bundle_record_count,
    );
    local_settings_sync_json_insert(
        &mut object,
        "receiver_pulled_membership_application_count",
        report.receiver_pulled_membership_application_count,
    );
    local_settings_sync_json_insert(
        &mut object,
        "discovery_protocols",
        report.discovery_protocols.as_slice(),
    );
    local_settings_sync_json_insert(
        &mut object,
        "discovery_trusted_peer_count",
        report.discovery_trusted_peer_count,
    );
    local_settings_sync_json_insert(
        &mut object,
        "discovery_rejected_peer_count",
        report.discovery_rejected_peer_count,
    );
    local_settings_sync_json_insert(
        &mut object,
        "discovery_selected_retention_provider_count",
        report.discovery_selected_retention_provider_count,
    );
    local_settings_sync_json_insert(
        &mut object,
        "discovery_endpoint_ready_provider_count",
        report.discovery_endpoint_ready_provider_count,
    );
    local_settings_sync_json_insert(
        &mut object,
        "discovery_endpoint_pending_protocol_provider_count",
        report.discovery_endpoint_pending_protocol_provider_count,
    );
    local_settings_sync_json_insert(
        &mut object,
        "discovery_endpoint_missing_provider_count",
        report.discovery_endpoint_missing_provider_count,
    );
    local_settings_sync_json_insert(
        &mut object,
        "discovery_endpoint_fail_closed_provider_count",
        report.discovery_endpoint_fail_closed_provider_count,
    );
    local_settings_sync_json_insert(
        &mut object,
        "discovery_endpoint_requires_protocol_materializer",
        report.discovery_endpoint_requires_protocol_materializer,
    );
    local_settings_sync_json_insert(
        &mut object,
        "discovery_retention_provider_selection_issue_count",
        report.discovery_retention_provider_selection_issue_count,
    );
    object.insert(
        "discovery_rejections".to_string(),
        local_settings_sync_discovery_rejections_json(report.discovery_rejections.as_slice()),
    );
    local_settings_sync_json_insert(
        &mut object,
        "receiver_applied_setting_count",
        report.receiver_applied_setting_count,
    );
    local_settings_sync_json_insert(
        &mut object,
        "receiver_published_step_count",
        report.receiver_published_step_count,
    );
    local_settings_sync_json_insert(
        &mut object,
        "receiver_received_value",
        report.receiver_received_value.as_deref(),
    );
    local_settings_sync_json_insert(
        &mut object,
        "receiver_membership_record_count",
        report.receiver_membership_record_count,
    );
    local_settings_sync_json_insert(
        &mut object,
        "receiver_trusted_device_count",
        report.receiver_trusted_device_count,
    );
    local_settings_sync_json_insert(
        &mut object,
        "retention_issue_count",
        report.retention_issue_count,
    );
    object.insert(
        "retention_issues".to_string(),
        local_settings_sync_retention_issues_json(report.retention_issues.as_slice()),
    );
    local_settings_sync_json_insert(
        &mut object,
        "fixture_materialization_issue_count",
        report.fixture_materialization_issue_count,
    );
    local_settings_sync_json_insert(
        &mut object,
        "retention_provider_selection_issue_count",
        report.retention_provider_selection_issue_count,
    );
    object.insert(
        "retention_provider_selection_issues".to_string(),
        local_settings_sync_provider_issues_json(
            report.retention_provider_selection_issues.as_slice(),
        ),
    );
    local_settings_sync_json_insert(
        &mut object,
        "stored_provider_metadata_issue_count",
        report.stored_provider_metadata_issue_count,
    );
    object.insert(
        "stored_provider_metadata_issues".to_string(),
        local_settings_sync_provider_issues_json(report.stored_provider_metadata_issues.as_slice()),
    );
    local_settings_sync_json_insert(
        &mut object,
        "all_fixture_providers_materialized",
        report.all_fixture_providers_materialized,
    );
    local_settings_sync_json_insert(
        &mut object,
        "selected_endpoint_ready_provider_count",
        report.selected_endpoint_ready_provider_count,
    );
    local_settings_sync_json_insert(
        &mut object,
        "selected_endpoint_pending_protocol_provider_count",
        report.selected_endpoint_pending_protocol_provider_count,
    );
    local_settings_sync_json_insert(
        &mut object,
        "selected_endpoint_missing_provider_count",
        report.selected_endpoint_missing_provider_count,
    );
    local_settings_sync_json_insert(
        &mut object,
        "selected_endpoint_fail_closed_provider_count",
        report.selected_endpoint_fail_closed_provider_count,
    );
    local_settings_sync_json_insert(
        &mut object,
        "selected_endpoint_requires_protocol_materializer",
        report.selected_endpoint_requires_protocol_materializer,
    );
    local_settings_sync_json_insert(&mut object, "degraded_before", report.degraded_before);
    local_settings_sync_json_insert(&mut object, "degraded_after", report.degraded_after);
    local_settings_sync_json_insert(
        &mut object,
        "root_object_provider_issue_count",
        report.root_object_provider_issue_count,
    );
    object.insert(
        "root_object_provider_issues".to_string(),
        local_settings_sync_root_object_provider_issues_json(
            report.root_object_provider_issues.as_slice(),
        ),
    );
    serde_json::Value::Object(object)
}

fn local_settings_sync_json_insert(
    object: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
    value: impl serde::Serialize,
) {
    object.insert(
        key.to_string(),
        serde_json::to_value(value).expect("serialize local settings sync preview JSON field"),
    );
}

fn local_settings_sync_root_object_provider_issues_json(
    issues: &[LocalSettingsSyncRootObjectProviderIssueSummary],
) -> serde_json::Value {
    serde_json::Value::Array(
        issues
            .iter()
            .map(local_settings_sync_root_object_provider_issue_json)
            .collect(),
    )
}

fn local_settings_sync_root_object_provider_issue_json(
    issue: &LocalSettingsSyncRootObjectProviderIssueSummary,
) -> serde_json::Value {
    serde_json::json!({
        "component": issue.component.as_str(),
        "root_id": issue.root_id.as_str(),
        "object_id": issue.object_id.as_deref(),
        "provider_id": issue.provider_id.as_str(),
        "kind": issue.kind.as_str(),
    })
}

fn local_settings_sync_provider_issues_json(
    issues: &[LocalSettingsSyncProviderIssueSummary],
) -> serde_json::Value {
    serde_json::Value::Array(
        issues
            .iter()
            .map(local_settings_sync_provider_issue_json)
            .collect(),
    )
}

fn local_settings_sync_provider_issue_json(
    issue: &LocalSettingsSyncProviderIssueSummary,
) -> serde_json::Value {
    serde_json::json!({
        "category": issue.category.as_str(),
        "provider_id": issue.provider_id.as_str(),
        "kind": issue.kind.as_str(),
    })
}

fn local_settings_sync_discovery_rejections_json(
    rejections: &[LocalSettingsSyncDiscoveryRejectionSummary],
) -> serde_json::Value {
    serde_json::Value::Array(
        rejections
            .iter()
            .map(local_settings_sync_discovery_rejection_json)
            .collect(),
    )
}

fn local_settings_sync_discovery_rejection_json(
    rejection: &LocalSettingsSyncDiscoveryRejectionSummary,
) -> serde_json::Value {
    serde_json::json!({
        "protocol": rejection.protocol.as_str(),
        "namespace": rejection.namespace.as_str(),
        "node_id": rejection.node_id.as_str(),
        "provider_id": rejection.provider_id.as_str(),
        "reason": rejection.reason.as_str(),
    })
}

fn local_settings_sync_retention_issues_json(
    issues: &[LocalSettingsSyncRetentionIssueSummary],
) -> serde_json::Value {
    serde_json::Value::Array(
        issues
            .iter()
            .map(local_settings_sync_retention_issue_json)
            .collect(),
    )
}

fn local_settings_sync_retention_issue_json(
    issue: &LocalSettingsSyncRetentionIssueSummary,
) -> serde_json::Value {
    serde_json::json!({
        "provider_index": issue.provider_index,
        "object_id": issue.object_id.as_str(),
        "kind": issue.kind.as_str(),
    })
}

#[cfg(test)]
mod tests {
    use super::local_settings_sync_preview_cycle_report_json;
    use crate::{
        LocalSettingsSyncDiscoveryRejectionSummary, LocalSettingsSyncPreviewCycleReport,
        LocalSettingsSyncProviderIssueSummary, LocalSettingsSyncRetentionIssueSummary,
        LocalSettingsSyncRootObjectProviderIssueSummary,
    };
    use slate_storage::DEFAULT_PROFILE_ID;

    #[test]
    fn local_settings_sync_preview_cycle_report_json_carries_issue_and_endpoint_fields() {
        let report = LocalSettingsSyncPreviewCycleReport {
            profile: DEFAULT_PROFILE_ID.to_string(),
            local_device_id: "device-a".to_string(),
            provider_id: "provider-a".to_string(),
            provider_endpoint_ref: "provider:provider-a".to_string(),
            preview_setting_key: "preview.key".to_string(),
            preview_setting_revision: 7,
            ready_for_manual_sync: true,
            blocked_reason: None,
            pulled_membership_application_count: 0,
            discovery_protocols: vec![
                "ipns".to_string(),
                "libp2p-rendezvous".to_string(),
                "local-simulation".to_string(),
            ],
            discovery_trusted_peer_count: 1,
            discovery_rejected_peer_count: 1,
            discovery_selected_retention_provider_count: 1,
            discovery_endpoint_ready_provider_count: 0,
            discovery_endpoint_pending_protocol_provider_count: 1,
            discovery_endpoint_missing_provider_count: 0,
            discovery_endpoint_fail_closed_provider_count: 0,
            discovery_endpoint_requires_protocol_materializer: true,
            discovery_retention_provider_selection_issue_count: 0,
            discovery_rejections: vec![LocalSettingsSyncDiscoveryRejectionSummary {
                protocol: "local-simulation".to_string(),
                namespace: "slate-profile-sync".to_string(),
                node_id: "node-b".to_string(),
                provider_id: "provider-d".to_string(),
                reason: "signer_membership_epoch_too_new".to_string(),
            }],
            selected_retention_provider_count: 1,
            materialized_retention_provider_count: 1,
            retained_provider_count: 1,
            published_step_count: 2,
            published_object_count: 3,
            retained_object_count: 2,
            retention_issue_count: 1,
            retention_issues: vec![LocalSettingsSyncRetentionIssueSummary {
                provider_index: 0,
                object_id: "bafyfixture-retention".to_string(),
                kind: "not_available".to_string(),
            }],
            fixture_materialization_issue_count: 0,
            retention_provider_selection_issue_count: 1,
            retention_provider_selection_issues: vec![LocalSettingsSyncProviderIssueSummary {
                category: "retention_provider_selection".to_string(),
                provider_id: "provider-b".to_string(),
                kind: "undiscovered".to_string(),
            }],
            stored_provider_metadata_issue_count: 1,
            stored_provider_metadata_issues: vec![LocalSettingsSyncProviderIssueSummary {
                category: "stored_provider_metadata".to_string(),
                provider_id: "provider-c".to_string(),
                kind: "unauthorized".to_string(),
            }],
            all_fixture_providers_materialized: true,
            selected_endpoint_ready_provider_count: 1,
            selected_endpoint_pending_protocol_provider_count: 2,
            selected_endpoint_missing_provider_count: 1,
            selected_endpoint_fail_closed_provider_count: 1,
            selected_endpoint_requires_protocol_materializer: true,
            degraded_before: true,
            degraded_after: true,
            root_object_provider_issue_count: 1,
            root_object_provider_issues: vec![LocalSettingsSyncRootObjectProviderIssueSummary {
                component: "settings_root".to_string(),
                root_id: "settings/latest".to_string(),
                object_id: Some("bafyfixture123".to_string()),
                provider_id: "provider-a".to_string(),
                kind: "offline".to_string(),
            }],
        };
        let json = local_settings_sync_preview_cycle_report_json(&report, 12);

        assert_eq!(json["completed_at"], 12);
        assert_eq!(json["preview_setting_key"], "preview.key");
        assert_eq!(json["root_object_provider_issue_count"], 1);
        assert_eq!(
            json["root_object_provider_issues"][0]["component"],
            "settings_root"
        );
        assert_eq!(
            json["root_object_provider_issues"][0]["root_id"],
            "settings/latest"
        );
        assert_eq!(
            json["root_object_provider_issues"][0]["object_id"],
            "bafyfixture123"
        );
        assert_eq!(
            json["root_object_provider_issues"][0]["provider_id"],
            "provider-a"
        );
        assert_eq!(json["root_object_provider_issues"][0]["kind"], "offline");
        assert_eq!(json["retention_provider_selection_issue_count"], 1);
        assert_eq!(
            json["retention_provider_selection_issues"][0]["category"],
            "retention_provider_selection"
        );
        assert_eq!(
            json["retention_provider_selection_issues"][0]["provider_id"],
            "provider-b"
        );
        assert_eq!(
            json["retention_provider_selection_issues"][0]["kind"],
            "undiscovered"
        );
        assert_eq!(json["stored_provider_metadata_issue_count"], 1);
        assert_eq!(
            json["stored_provider_metadata_issues"][0]["category"],
            "stored_provider_metadata"
        );
        assert_eq!(
            json["stored_provider_metadata_issues"][0]["provider_id"],
            "provider-c"
        );
        assert_eq!(
            json["stored_provider_metadata_issues"][0]["kind"],
            "unauthorized"
        );
        assert_eq!(json["retention_issue_count"], 1);
        assert_eq!(json["retention_issues"][0]["provider_index"], 0);
        assert_eq!(
            json["retention_issues"][0]["object_id"],
            "bafyfixture-retention"
        );
        assert_eq!(json["retention_issues"][0]["kind"], "not_available");
        assert_eq!(json["selected_endpoint_ready_provider_count"], 1);
        assert_eq!(json["selected_endpoint_pending_protocol_provider_count"], 2);
        assert_eq!(json["selected_endpoint_missing_provider_count"], 1);
        assert_eq!(json["selected_endpoint_fail_closed_provider_count"], 1);
        assert_eq!(
            json["selected_endpoint_requires_protocol_materializer"],
            true
        );
        assert_eq!(json["discovery_trusted_peer_count"], 1);
        assert_eq!(json["discovery_protocols"][0], "ipns");
        assert_eq!(json["discovery_protocols"][1], "libp2p-rendezvous");
        assert_eq!(json["discovery_protocols"][2], "local-simulation");
        assert_eq!(json["discovery_rejected_peer_count"], 1);
        assert_eq!(json["discovery_selected_retention_provider_count"], 1);
        assert_eq!(json["discovery_endpoint_ready_provider_count"], 0);
        assert_eq!(
            json["discovery_endpoint_pending_protocol_provider_count"],
            1
        );
        assert_eq!(json["discovery_endpoint_missing_provider_count"], 0);
        assert_eq!(json["discovery_endpoint_fail_closed_provider_count"], 0);
        assert_eq!(
            json["discovery_endpoint_requires_protocol_materializer"],
            true
        );
        assert_eq!(
            json["discovery_retention_provider_selection_issue_count"],
            0
        );
        assert_eq!(
            json["discovery_rejections"][0]["protocol"],
            "local-simulation"
        );
        assert_eq!(json["discovery_rejections"][0]["node_id"], "node-b");
        assert_eq!(json["discovery_rejections"][0]["provider_id"], "provider-d");
        assert_eq!(
            json["discovery_rejections"][0]["reason"],
            "signer_membership_epoch_too_new"
        );
    }
}
