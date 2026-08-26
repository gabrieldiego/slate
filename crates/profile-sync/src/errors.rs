use crate::SettingsSyncHealthReport;
use core::fmt;
use slate_broadwebd::BroadwebdError;
use slate_storage::{ProfileSyncTrustedPullApplyError, StorageError, SyncObjectError};

#[derive(Debug)]
pub enum ProfileSyncPublishError {
    Broadwebd(BroadwebdError),
    Storage(StorageError),
    SyncObject(SyncObjectError),
    MembershipLogTooLarge {
        profile: String,
        max_records: usize,
        actual_records: usize,
    },
    LocalSyncLoopExhausted {
        profile: String,
        settings_root_id: String,
        max_steps: u32,
    },
}

impl fmt::Display for ProfileSyncPublishError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Broadwebd(error) => write!(formatter, "profile sync backend error: {error}"),
            Self::Storage(error) => write!(formatter, "profile sync storage error: {error}"),
            Self::SyncObject(error) => write!(formatter, "profile sync object error: {error}"),
            Self::MembershipLogTooLarge {
                profile,
                max_records,
                actual_records,
            } => write!(
                formatter,
                "profile sync membership log for profile {profile} has {actual_records} records, exceeding max {max_records}"
            ),
            Self::LocalSyncLoopExhausted {
                profile,
                settings_root_id,
                max_steps,
            } => write!(
                formatter,
                "local settings sync loop for profile {profile} root {settings_root_id} did not reach idle after {max_steps} steps"
            ),
        }
    }
}

impl std::error::Error for ProfileSyncPublishError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Broadwebd(error) => Some(error),
            Self::Storage(error) => Some(error),
            Self::SyncObject(error) => Some(error),
            Self::MembershipLogTooLarge { .. } => None,
            Self::LocalSyncLoopExhausted { .. } => None,
        }
    }
}

impl From<BroadwebdError> for ProfileSyncPublishError {
    fn from(error: BroadwebdError) -> Self {
        Self::Broadwebd(error)
    }
}

impl From<StorageError> for ProfileSyncPublishError {
    fn from(error: StorageError) -> Self {
        Self::Storage(error)
    }
}

impl From<SyncObjectError> for ProfileSyncPublishError {
    fn from(error: SyncObjectError) -> Self {
        Self::SyncObject(error)
    }
}

#[derive(Debug)]
pub enum ProfileSyncReceiveError {
    Broadwebd(BroadwebdError),
    Storage(StorageError),
    SyncObject(SyncObjectError),
    PullApply(ProfileSyncTrustedPullApplyError<BroadwebdError>),
    InvalidMembershipLog(String),
    TrustedDeviceLimitExceeded {
        profile: String,
        trusted_device_count: usize,
        max_devices: u32,
    },
}

impl fmt::Display for ProfileSyncReceiveError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Broadwebd(error) => write!(formatter, "profile sync backend error: {error}"),
            Self::Storage(error) => write!(formatter, "profile sync storage error: {error}"),
            Self::SyncObject(error) => write!(formatter, "profile sync object error: {error}"),
            Self::PullApply(error) => write!(formatter, "profile sync receive error: {error}"),
            Self::InvalidMembershipLog(reason) => {
                write!(formatter, "invalid profile sync membership log: {reason}")
            }
            Self::TrustedDeviceLimitExceeded {
                profile,
                trusted_device_count,
                max_devices,
            } => write!(
                formatter,
                "trusted settings sync for profile {profile} has {trusted_device_count} remote trusted devices, exceeding max {max_devices}"
            ),
        }
    }
}

impl std::error::Error for ProfileSyncReceiveError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Broadwebd(error) => Some(error),
            Self::Storage(error) => Some(error),
            Self::SyncObject(error) => Some(error),
            Self::PullApply(error) => Some(error),
            Self::InvalidMembershipLog(_) => None,
            Self::TrustedDeviceLimitExceeded { .. } => None,
        }
    }
}

impl From<StorageError> for ProfileSyncReceiveError {
    fn from(error: StorageError) -> Self {
        Self::Storage(error)
    }
}

impl From<BroadwebdError> for ProfileSyncReceiveError {
    fn from(error: BroadwebdError) -> Self {
        Self::Broadwebd(error)
    }
}

impl From<SyncObjectError> for ProfileSyncReceiveError {
    fn from(error: SyncObjectError) -> Self {
        Self::SyncObject(error)
    }
}

impl From<ProfileSyncTrustedPullApplyError<BroadwebdError>> for ProfileSyncReceiveError {
    fn from(error: ProfileSyncTrustedPullApplyError<BroadwebdError>) -> Self {
        Self::PullApply(error)
    }
}

#[derive(Debug)]
pub enum ProfileSyncCredentialError {
    Storage(StorageError),
    SyncObject(SyncObjectError),
    InactiveContentKey {
        profile: String,
        expected_key_id: String,
        active_key_id: String,
    },
    UntrustedLocalDevice {
        profile: String,
        device_id: String,
    },
    LocalDevicePublicKeyMismatch {
        profile: String,
        device_id: String,
    },
    LocalDeviceSignerMismatch {
        profile: String,
        local_device_id: String,
        signer_device_id: String,
    },
}

impl fmt::Display for ProfileSyncCredentialError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Storage(error) => {
                write!(formatter, "profile sync credential storage error: {error}")
            }
            Self::SyncObject(error) => {
                write!(formatter, "profile sync credential object error: {error}")
            }
            Self::InactiveContentKey {
                profile,
                expected_key_id,
                active_key_id,
            } => write!(
                formatter,
                "profile {profile} active sync content key is {active_key_id}, not requested key {expected_key_id}"
            ),
            Self::UntrustedLocalDevice { profile, device_id } => write!(
                formatter,
                "profile {profile} has no trusted public key for local sync device {device_id}"
            ),
            Self::LocalDevicePublicKeyMismatch { profile, device_id } => write!(
                formatter,
                "profile {profile} trusted public key for local sync device {device_id} does not match the supplied signer"
            ),
            Self::LocalDeviceSignerMismatch {
                profile,
                local_device_id,
                signer_device_id,
            } => write!(
                formatter,
                "profile {profile} local sync device is {local_device_id}, but supplied signer is for {signer_device_id}"
            ),
        }
    }
}

impl std::error::Error for ProfileSyncCredentialError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Storage(error) => Some(error),
            Self::SyncObject(error) => Some(error),
            Self::InactiveContentKey { .. }
            | Self::UntrustedLocalDevice { .. }
            | Self::LocalDevicePublicKeyMismatch { .. }
            | Self::LocalDeviceSignerMismatch { .. } => None,
        }
    }
}

impl From<StorageError> for ProfileSyncCredentialError {
    fn from(error: StorageError) -> Self {
        Self::Storage(error)
    }
}

impl From<SyncObjectError> for ProfileSyncCredentialError {
    fn from(error: SyncObjectError) -> Self {
        Self::SyncObject(error)
    }
}

#[derive(Debug)]
pub enum ProfileSyncCycleError {
    Credentials(ProfileSyncCredentialError),
    Publish(ProfileSyncPublishError),
    Receive(ProfileSyncReceiveError),
}

impl fmt::Display for ProfileSyncCycleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Credentials(error) => {
                write!(
                    formatter,
                    "profile sync credential preflight failed: {error}"
                )
            }
            Self::Publish(error) => write!(formatter, "profile sync publish cycle failed: {error}"),
            Self::Receive(error) => write!(formatter, "profile sync receive cycle failed: {error}"),
        }
    }
}

impl std::error::Error for ProfileSyncCycleError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Credentials(error) => Some(error),
            Self::Publish(error) => Some(error),
            Self::Receive(error) => Some(error),
        }
    }
}

impl From<ProfileSyncPublishError> for ProfileSyncCycleError {
    fn from(error: ProfileSyncPublishError) -> Self {
        Self::Publish(error)
    }
}

impl From<ProfileSyncCredentialError> for ProfileSyncCycleError {
    fn from(error: ProfileSyncCredentialError) -> Self {
        Self::Credentials(error)
    }
}

impl From<ProfileSyncReceiveError> for ProfileSyncCycleError {
    fn from(error: ProfileSyncReceiveError) -> Self {
        Self::Receive(error)
    }
}

#[derive(Debug)]
pub enum ProfileSyncCycleWithHealthError {
    Health(BroadwebdError),
    Retention(BroadwebdError),
    Policy(ProfileSyncPolicyError),
    Cycle(ProfileSyncCycleError),
}

impl fmt::Display for ProfileSyncCycleWithHealthError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Health(error) => {
                write!(formatter, "profile sync health check failed: {error}")
            }
            Self::Retention(error) => write!(formatter, "profile sync retention failed: {error}"),
            Self::Policy(error) => write!(formatter, "profile sync policy check failed: {error}"),
            Self::Cycle(error) => write!(formatter, "profile sync cycle failed: {error}"),
        }
    }
}

impl std::error::Error for ProfileSyncCycleWithHealthError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Health(error) => Some(error),
            Self::Retention(error) => Some(error),
            Self::Policy(error) => Some(error),
            Self::Cycle(error) => Some(error),
        }
    }
}

impl From<BroadwebdError> for ProfileSyncCycleWithHealthError {
    fn from(error: BroadwebdError) -> Self {
        Self::Health(error)
    }
}

impl From<ProfileSyncCycleError> for ProfileSyncCycleWithHealthError {
    fn from(error: ProfileSyncCycleError) -> Self {
        Self::Cycle(error)
    }
}

impl From<ProfileSyncPolicyError> for ProfileSyncCycleWithHealthError {
    fn from(error: ProfileSyncPolicyError) -> Self {
        Self::Policy(error)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProfileSyncPolicyError {
    ProviderHealthDegraded {
        health: SettingsSyncHealthReport,
    },
    RootHealthDegraded {
        root_kind: &'static str,
        health: SettingsSyncHealthReport,
    },
    ProviderMinimumUnmet {
        provider_role: &'static str,
        minimum: usize,
        actual: usize,
        health: SettingsSyncHealthReport,
    },
    ProviderMaximumExceeded {
        provider_role: &'static str,
        maximum: usize,
        actual: usize,
        health: SettingsSyncHealthReport,
    },
}

impl fmt::Display for ProfileSyncPolicyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ProviderHealthDegraded { health } => write!(
                formatter,
                "profile {} provider health is degraded: {}",
                health.profile, health.provider_health.message
            ),
            Self::RootHealthDegraded { root_kind, health } => write!(
                formatter,
                "profile {} {} health is degraded",
                health.profile, root_kind
            ),
            Self::ProviderMinimumUnmet {
                provider_role,
                minimum,
                actual,
                health,
            } => write!(
                formatter,
                "profile {} requires at least {} {}, but health reported {}",
                health.profile, minimum, provider_role, actual
            ),
            Self::ProviderMaximumExceeded {
                provider_role,
                maximum,
                actual,
                health,
            } => write!(
                formatter,
                "profile {} allows at most {} {}, but health reported {}",
                health.profile, maximum, provider_role, actual
            ),
        }
    }
}

impl std::error::Error for ProfileSyncPolicyError {}

#[cfg(test)]
mod tests {
    use super::{
        ProfileSyncCredentialError, ProfileSyncCycleError, ProfileSyncPolicyError,
        ProfileSyncPublishError, ProfileSyncReceiveError,
    };
    use crate::SettingsSyncHealthReport;
    use slate_broadwebd::{ProfileSyncProviderHealth, ProfileSyncRootHealth};
    use slate_storage::StorageError;
    use std::error::Error;

    #[test]
    fn publish_error_reports_membership_log_limit_without_source() {
        let error = ProfileSyncPublishError::MembershipLogTooLarge {
            profile: "default".to_string(),
            max_records: 512,
            actual_records: 513,
        };

        assert!(error.to_string().contains("513 records"));
        assert!(error.source().is_none());
    }

    #[test]
    fn receive_error_keeps_storage_source() {
        let error = ProfileSyncReceiveError::from(StorageError::InvalidSyncDeviceId(
            "bad-device".to_string(),
        ));

        assert!(error.to_string().contains("profile sync storage error"));
        assert!(error.source().is_some());
    }

    #[test]
    fn credential_error_converts_into_cycle_error() {
        let credential_error = ProfileSyncCredentialError::UntrustedLocalDevice {
            profile: "default".to_string(),
            device_id: "device-a".to_string(),
        };
        let cycle_error = ProfileSyncCycleError::from(credential_error);

        assert!(matches!(
            cycle_error,
            ProfileSyncCycleError::Credentials(
                ProfileSyncCredentialError::UntrustedLocalDevice { .. }
            )
        ));
        assert!(
            cycle_error
                .to_string()
                .contains("credential preflight failed")
        );
    }

    #[test]
    fn policy_error_display_includes_profile_role_and_counts() {
        let error = ProfileSyncPolicyError::ProviderMaximumExceeded {
            provider_role: "retention providers",
            maximum: 1,
            actual: 2,
            health: test_health_report(),
        };

        assert_eq!(
            error.to_string(),
            "profile default allows at most 1 retention providers, but health reported 2"
        );
    }

    fn test_health_report() -> SettingsSyncHealthReport {
        SettingsSyncHealthReport {
            profile: "default".to_string(),
            settings_root_id: "settings/latest".to_string(),
            local_device_head_root_id: "settings/devices/device-a/head".to_string(),
            provider_health: ProfileSyncProviderHealth {
                profile: "default".to_string(),
                known_providers: 1,
                online_providers: 1,
                offline_providers: 0,
                fresh_online_providers: 1,
                stale_online_providers: 0,
                fresh_online_provider_ids: vec!["provider-a".to_string()],
                stale_online_provider_ids: Vec::new(),
                offline_provider_ids: Vec::new(),
                minimum_provider_seen_sequence: 1,
                object_transfer_providers: 1,
                availability_providers: 1,
                mutable_root_providers: 1,
                retained_objects: 0,
                degraded: false,
                message: "healthy".to_string(),
            },
            settings_root_health: healthy_root("settings/latest"),
            local_device_head_root_health: healthy_root("settings/devices/device-a/head"),
        }
    }

    fn healthy_root(root_id: &str) -> ProfileSyncRootHealth {
        ProfileSyncRootHealth {
            profile: "default".to_string(),
            root_id: root_id.to_string(),
            visible_candidates: 1,
            delayed_candidates: 0,
            delayed_publisher_provider_ids: Vec::new(),
            latest_object_id: Some("object-a".to_string()),
            latest_object_available: true,
            latest_object_available_provider_ids: vec!["provider-a".to_string()],
            latest_object_stale_provider_ids: Vec::new(),
            latest_object_offline_provider_ids: Vec::new(),
            delayed_object_provider_ids: Vec::new(),
            unavailable_retaining_provider_ids: Vec::new(),
            online_retaining_providers: 1,
            minimum_online_retaining_providers: 1,
            degraded: false,
            message: "healthy".to_string(),
        }
    }
}
