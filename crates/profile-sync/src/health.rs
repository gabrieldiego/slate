use slate_broadwebd::{
    ProfileSyncProviderHealth as BroadwebdProfileSyncProviderHealth,
    ProfileSyncRootHealth as BroadwebdProfileSyncRootHealth,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SettingsSyncHealthReport {
    pub profile: String,
    pub settings_root_id: String,
    pub local_device_head_root_id: String,
    pub provider_health: BroadwebdProfileSyncProviderHealth,
    pub settings_root_health: BroadwebdProfileSyncRootHealth,
    pub local_device_head_root_health: BroadwebdProfileSyncRootHealth,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SettingsSyncHealthIssueComponent {
    Providers,
    SettingsRoot,
    LocalDeviceHeadRoot,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SettingsSyncHealthIssue {
    pub component: SettingsSyncHealthIssueComponent,
    pub root_id: Option<String>,
    pub message: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SettingsSyncRootObjectProviderIssueKind {
    Delayed,
    Stale,
    Offline,
    RetainedUnavailable,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SettingsSyncRootObjectProviderIssue {
    pub component: SettingsSyncHealthIssueComponent,
    pub root_id: String,
    pub object_id: Option<String>,
    pub provider_id: String,
    pub kind: SettingsSyncRootObjectProviderIssueKind,
}

impl SettingsSyncHealthReport {
    pub fn degraded(&self) -> bool {
        self.provider_health.degraded
            || self.settings_root_health.degraded
            || self.local_device_head_root_health.degraded
    }

    pub fn healthy(&self) -> bool {
        !self.degraded()
    }

    pub fn provider_degraded(&self) -> bool {
        self.provider_health.degraded
    }

    pub fn settings_root_degraded(&self) -> bool {
        self.settings_root_health.degraded
    }

    pub fn local_device_head_root_degraded(&self) -> bool {
        self.local_device_head_root_health.degraded
    }

    pub fn degraded_root_count(&self) -> usize {
        usize::from(self.settings_root_degraded())
            + usize::from(self.local_device_head_root_degraded())
    }

    pub fn degradation_issue_count(&self) -> usize {
        self.degradation_issues().len()
    }

    pub fn degradation_issues(&self) -> Vec<SettingsSyncHealthIssue> {
        let mut issues = Vec::new();
        if self.provider_degraded() {
            issues.push(SettingsSyncHealthIssue {
                component: SettingsSyncHealthIssueComponent::Providers,
                root_id: None,
                message: self.provider_health.message.clone(),
            });
        }
        if self.settings_root_degraded() {
            issues.push(SettingsSyncHealthIssue {
                component: SettingsSyncHealthIssueComponent::SettingsRoot,
                root_id: Some(self.settings_root_id.clone()),
                message: self.settings_root_health.message.clone(),
            });
        }
        if self.local_device_head_root_degraded() {
            issues.push(SettingsSyncHealthIssue {
                component: SettingsSyncHealthIssueComponent::LocalDeviceHeadRoot,
                root_id: Some(self.local_device_head_root_id.clone()),
                message: self.local_device_head_root_health.message.clone(),
            });
        }
        issues
    }

    pub fn root_object_provider_issue_count(&self) -> usize {
        self.root_object_provider_issues().len()
    }

    pub fn root_object_provider_issues(&self) -> Vec<SettingsSyncRootObjectProviderIssue> {
        let mut issues = Vec::new();
        append_settings_sync_root_object_provider_issues(
            &mut issues,
            SettingsSyncHealthIssueComponent::SettingsRoot,
            self.settings_root_id.as_str(),
            &self.settings_root_health,
        );
        append_settings_sync_root_object_provider_issues(
            &mut issues,
            SettingsSyncHealthIssueComponent::LocalDeviceHeadRoot,
            self.local_device_head_root_id.as_str(),
            &self.local_device_head_root_health,
        );
        issues
    }
}

fn append_settings_sync_root_object_provider_issues(
    issues: &mut Vec<SettingsSyncRootObjectProviderIssue>,
    component: SettingsSyncHealthIssueComponent,
    root_id: &str,
    health: &BroadwebdProfileSyncRootHealth,
) {
    append_settings_sync_root_object_provider_issue_kind(
        issues,
        component.clone(),
        root_id,
        health.latest_object_id.clone(),
        SettingsSyncRootObjectProviderIssueKind::Delayed,
        health.delayed_object_provider_ids.as_slice(),
    );
    append_settings_sync_root_object_provider_issue_kind(
        issues,
        component.clone(),
        root_id,
        health.latest_object_id.clone(),
        SettingsSyncRootObjectProviderIssueKind::Stale,
        health.latest_object_stale_provider_ids.as_slice(),
    );
    append_settings_sync_root_object_provider_issue_kind(
        issues,
        component.clone(),
        root_id,
        health.latest_object_id.clone(),
        SettingsSyncRootObjectProviderIssueKind::Offline,
        health.latest_object_offline_provider_ids.as_slice(),
    );
    append_settings_sync_root_object_provider_issue_kind(
        issues,
        component,
        root_id,
        health.latest_object_id.clone(),
        SettingsSyncRootObjectProviderIssueKind::RetainedUnavailable,
        health.unavailable_retaining_provider_ids.as_slice(),
    );
}

fn append_settings_sync_root_object_provider_issue_kind(
    issues: &mut Vec<SettingsSyncRootObjectProviderIssue>,
    component: SettingsSyncHealthIssueComponent,
    root_id: &str,
    object_id: Option<String>,
    kind: SettingsSyncRootObjectProviderIssueKind,
    provider_ids: &[String],
) {
    issues.extend(provider_ids.iter().cloned().map(|provider_id| {
        SettingsSyncRootObjectProviderIssue {
            component: component.clone(),
            root_id: root_id.to_string(),
            object_id: object_id.clone(),
            provider_id,
            kind,
        }
    }));
}

#[cfg(test)]
mod tests {
    use super::{
        BroadwebdProfileSyncProviderHealth, BroadwebdProfileSyncRootHealth,
        SettingsSyncHealthIssue, SettingsSyncHealthIssueComponent, SettingsSyncHealthReport,
        SettingsSyncRootObjectProviderIssue, SettingsSyncRootObjectProviderIssueKind,
    };

    fn provider_health(degraded: bool, message: &str) -> BroadwebdProfileSyncProviderHealth {
        BroadwebdProfileSyncProviderHealth {
            profile: "profile-a".to_string(),
            known_providers: 1,
            online_providers: usize::from(!degraded),
            offline_providers: usize::from(degraded),
            fresh_online_providers: usize::from(!degraded),
            stale_online_providers: 0,
            fresh_online_provider_ids: if degraded {
                Vec::new()
            } else {
                vec!["provider-a".to_string()]
            },
            stale_online_provider_ids: Vec::new(),
            offline_provider_ids: if degraded {
                vec!["provider-a".to_string()]
            } else {
                Vec::new()
            },
            minimum_provider_seen_sequence: 1,
            object_transfer_providers: usize::from(!degraded),
            availability_providers: usize::from(!degraded),
            mutable_root_providers: usize::from(!degraded),
            retained_objects: 0,
            degraded,
            message: message.to_string(),
        }
    }

    fn root_health(root_id: &str, degraded: bool, message: &str) -> BroadwebdProfileSyncRootHealth {
        BroadwebdProfileSyncRootHealth {
            profile: "profile-a".to_string(),
            root_id: root_id.to_string(),
            visible_candidates: usize::from(!degraded),
            delayed_candidates: 0,
            delayed_publisher_provider_ids: Vec::new(),
            latest_object_id: Some("object-a".to_string()),
            latest_object_available: !degraded,
            latest_object_available_provider_ids: if degraded {
                Vec::new()
            } else {
                vec!["provider-a".to_string()]
            },
            latest_object_stale_provider_ids: Vec::new(),
            latest_object_offline_provider_ids: Vec::new(),
            delayed_object_provider_ids: Vec::new(),
            unavailable_retaining_provider_ids: Vec::new(),
            online_retaining_providers: usize::from(!degraded),
            minimum_online_retaining_providers: 1,
            degraded,
            message: message.to_string(),
        }
    }

    fn healthy_report() -> SettingsSyncHealthReport {
        SettingsSyncHealthReport {
            profile: "profile-a".to_string(),
            settings_root_id: "settings/latest".to_string(),
            local_device_head_root_id: "settings/devices/device-a/head".to_string(),
            provider_health: provider_health(false, "providers healthy"),
            settings_root_health: root_health("settings/latest", false, "settings root healthy"),
            local_device_head_root_health: root_health(
                "settings/devices/device-a/head",
                false,
                "device head healthy",
            ),
        }
    }

    #[test]
    fn health_report_reports_healthy_components() {
        let report = healthy_report();

        assert!(report.healthy());
        assert!(!report.degraded());
        assert!(!report.provider_degraded());
        assert!(!report.settings_root_degraded());
        assert!(!report.local_device_head_root_degraded());
        assert_eq!(report.degraded_root_count(), 0);
        assert_eq!(report.degradation_issue_count(), 0);
        assert_eq!(report.degradation_issues(), Vec::new());
        assert_eq!(report.root_object_provider_issue_count(), 0);
        assert_eq!(report.root_object_provider_issues(), Vec::new());
    }

    #[test]
    fn health_report_collects_component_degradation_issues() {
        let mut report = healthy_report();
        report.provider_health = provider_health(true, "provider quorum degraded");
        report.settings_root_health =
            root_health("settings/latest", true, "settings root unavailable");

        assert!(report.degraded());
        assert_eq!(report.degraded_root_count(), 1);
        assert_eq!(report.degradation_issue_count(), 2);
        assert_eq!(
            report.degradation_issues(),
            vec![
                SettingsSyncHealthIssue {
                    component: SettingsSyncHealthIssueComponent::Providers,
                    root_id: None,
                    message: "provider quorum degraded".to_string(),
                },
                SettingsSyncHealthIssue {
                    component: SettingsSyncHealthIssueComponent::SettingsRoot,
                    root_id: Some("settings/latest".to_string()),
                    message: "settings root unavailable".to_string(),
                },
            ]
        );
    }

    #[test]
    fn health_report_collects_root_object_provider_issues() {
        let mut report = healthy_report();
        report.settings_root_health.latest_object_id = Some("settings-object".to_string());
        report.settings_root_health.delayed_object_provider_ids = vec!["provider-delayed".into()];
        report.settings_root_health.latest_object_stale_provider_ids =
            vec!["provider-stale".into()];
        report
            .settings_root_health
            .latest_object_offline_provider_ids = vec!["provider-offline".into()];
        report
            .settings_root_health
            .unavailable_retaining_provider_ids = vec!["provider-unavailable".into()];
        report.local_device_head_root_health.latest_object_id = None;
        report
            .local_device_head_root_health
            .delayed_object_provider_ids = vec!["provider-head-delayed".into()];

        assert_eq!(report.root_object_provider_issue_count(), 5);
        assert_eq!(
            report.root_object_provider_issues(),
            vec![
                SettingsSyncRootObjectProviderIssue {
                    component: SettingsSyncHealthIssueComponent::SettingsRoot,
                    root_id: "settings/latest".to_string(),
                    object_id: Some("settings-object".to_string()),
                    provider_id: "provider-delayed".to_string(),
                    kind: SettingsSyncRootObjectProviderIssueKind::Delayed,
                },
                SettingsSyncRootObjectProviderIssue {
                    component: SettingsSyncHealthIssueComponent::SettingsRoot,
                    root_id: "settings/latest".to_string(),
                    object_id: Some("settings-object".to_string()),
                    provider_id: "provider-stale".to_string(),
                    kind: SettingsSyncRootObjectProviderIssueKind::Stale,
                },
                SettingsSyncRootObjectProviderIssue {
                    component: SettingsSyncHealthIssueComponent::SettingsRoot,
                    root_id: "settings/latest".to_string(),
                    object_id: Some("settings-object".to_string()),
                    provider_id: "provider-offline".to_string(),
                    kind: SettingsSyncRootObjectProviderIssueKind::Offline,
                },
                SettingsSyncRootObjectProviderIssue {
                    component: SettingsSyncHealthIssueComponent::SettingsRoot,
                    root_id: "settings/latest".to_string(),
                    object_id: Some("settings-object".to_string()),
                    provider_id: "provider-unavailable".to_string(),
                    kind: SettingsSyncRootObjectProviderIssueKind::RetainedUnavailable,
                },
                SettingsSyncRootObjectProviderIssue {
                    component: SettingsSyncHealthIssueComponent::LocalDeviceHeadRoot,
                    root_id: "settings/devices/device-a/head".to_string(),
                    object_id: None,
                    provider_id: "provider-head-delayed".to_string(),
                    kind: SettingsSyncRootObjectProviderIssueKind::Delayed,
                },
            ]
        );
    }
}
