#![forbid(unsafe_code)]

use core::fmt;
use serde::{Deserialize, Serialize};
use slate_broadwebd::{
    BroadwebDaemon, BroadwebdError, ProfileSyncObjectRequest as BroadwebdProfileSyncObjectRequest,
    ProfileSyncProfileRequest as BroadwebdProfileSyncProfileRequest,
    ProfileSyncProviderHealth as BroadwebdProfileSyncProviderHealth,
    ProfileSyncProviderRecord as BroadwebdProfileSyncProviderRecord,
    ProfileSyncPutObjectRequest as BroadwebdProfileSyncPutObjectRequest,
    ProfileSyncRequest as BroadwebdProfileSyncRequest,
    ProfileSyncResponse as BroadwebdProfileSyncResponse,
    ProfileSyncRootHealth as BroadwebdProfileSyncRootHealth,
    ProfileSyncRootHealthRequest as BroadwebdProfileSyncRootHealthRequest,
    ProfileSyncRootRequest as BroadwebdProfileSyncRootRequest,
    ProfileSyncRootUpdate as BroadwebdProfileSyncRootUpdate,
};
use slate_storage::{
    DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH, EncryptedSyncObject, IncomingSyncSettingText,
    PROFILE_SYNC_CONTENT_KEY_ALGORITHM_CHACHA20_POLY1305, PROFILE_SYNC_DEVICE_HEAD_OBJECT_KIND,
    PROFILE_SYNC_DEVICE_HEAD_SCHEMA_VERSION, PROFILE_SYNC_MANIFEST_OBJECT_KIND,
    PROFILE_SYNC_SETTING_CHANGE_OBJECT_KIND, PROFILE_SYNC_SETTINGS_SNAPSHOT_OBJECT_KIND,
    PROFILE_SYNC_SETTINGS_SNAPSHOT_SCHEMA_VERSION, ProfileSyncContentKey, ProfileSyncDeviceHead,
    ProfileSyncDeviceHeadPullRecordStatus, ProfileSyncDevicePublicKey, ProfileSyncDeviceSigner,
    ProfileSyncManifest, ProfileSyncMembershipRecord, ProfileSyncObjectBytes,
    ProfileSyncObjectSource, ProfileSyncRetentionPolicy,
    ProfileSyncRootCandidate as StorageProfileSyncRootCandidate, ProfileSyncRootRecord,
    ProfileSyncSettingsCandidatePullApplyStatus, ProfileSyncSettingsManifestApplication,
    ProfileSyncSettingsSnapshot, ProfileSyncSettingsSnapshotPublication,
    ProfileSyncSettingsTailChangePublication, ProfileSyncTrustedPullApplyError,
    SYNC_DOMAIN_SETTINGS, SignedSyncObject, SlateProfileDatabase, StorageError,
    SyncAccountMembershipRecordApplication, SyncChangeRecord, SyncCompactionTarget,
    SyncDevicePublicKeyRecord, SyncObjectError, SyncSettingTextEvent, SyncSnapshotRecord,
    SyncSnapshotRegistration, VerifiedProfileSyncDeviceHead, open_signed_profile_sync_device_head,
    settings_sync_manifest_for_snapshot_and_tail_changes, settings_sync_manifest_for_tail_changes,
    settings_sync_snapshot_id,
};
use std::collections::BTreeSet;

pub const PROFILE_SYNC_MEMBERSHIP_LOG_SCHEMA_VERSION: u8 = 1;
pub const PROFILE_SYNC_MEMBERSHIP_LOG_ROOT_ID: &str = "account/membership/log";
pub const PROFILE_SYNC_MEMBERSHIP_LOG_MAX_RECORDS: usize = 512;

#[derive(Clone, Copy)]
pub struct BroadwebdProfileSyncObjectSource<'a> {
    daemon: &'a BroadwebDaemon,
}

#[derive(Clone, Copy)]
pub struct BroadwebdProfileSyncPublisher<'a> {
    daemon: &'a BroadwebDaemon,
}

#[derive(Clone, Copy)]
pub struct BroadwebdSettingsSyncRunner<'a> {
    daemon: &'a BroadwebDaemon,
}

#[derive(Clone, Copy)]
pub struct BroadwebdSettingsSyncScheduler<'a> {
    daemon: &'a BroadwebDaemon,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BroadwebdProfileSyncRetentionStatus {
    pub object_id: String,
    pub retained: bool,
    pub available: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BroadwebdProfileSyncRootPublication {
    pub profile: String,
    pub root_id: String,
    pub root_object_id: String,
    pub dependency_object_ids: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublishedProfileSyncMembershipRecord {
    pub profile: String,
    pub root_id: String,
    pub object_id: String,
    pub signed_record: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct ProfileSyncMembershipLog {
    pub profile: String,
    pub schema_version: u8,
    pub records: Vec<ProfileSyncMembershipLogEntry>,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct ProfileSyncMembershipLogEntry {
    pub record_id: String,
    pub root_id: String,
    pub object_id: String,
    pub membership_epoch: i64,
    pub record_kind: String,
    pub device_id: String,
    pub signer_device_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublishedProfileSyncMembershipLog {
    pub profile: String,
    pub root_id: String,
    pub object_id: String,
    pub log: ProfileSyncMembershipLog,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProfileSyncMembershipLogPublicationPlanStatus {
    Empty,
    Publishable,
    TooLarge,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileSyncMembershipLogPublicationPlan {
    pub profile: String,
    pub root_id: String,
    pub record_count: usize,
    pub max_records: usize,
    pub status: ProfileSyncMembershipLogPublicationPlanStatus,
}

impl ProfileSyncMembershipLogPublicationPlan {
    pub fn for_record_count(profile: &str, root_id: &str, record_count: usize) -> Self {
        let status = if record_count == 0 {
            ProfileSyncMembershipLogPublicationPlanStatus::Empty
        } else if record_count > PROFILE_SYNC_MEMBERSHIP_LOG_MAX_RECORDS {
            ProfileSyncMembershipLogPublicationPlanStatus::TooLarge
        } else {
            ProfileSyncMembershipLogPublicationPlanStatus::Publishable
        };
        Self {
            profile: profile.to_string(),
            root_id: root_id.to_string(),
            record_count,
            max_records: PROFILE_SYNC_MEMBERSHIP_LOG_MAX_RECORDS,
            status,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.status == ProfileSyncMembershipLogPublicationPlanStatus::Empty
    }

    pub fn is_publishable(&self) -> bool {
        self.status == ProfileSyncMembershipLogPublicationPlanStatus::Publishable
    }

    pub fn requires_compaction(&self) -> bool {
        self.status == ProfileSyncMembershipLogPublicationPlanStatus::TooLarge
    }
}

impl PublishedProfileSyncMembershipLog {
    pub fn published_object_ids(&self) -> Vec<String> {
        let mut object_ids = Vec::new();
        let mut seen = BTreeSet::new();
        for entry in &self.log.records {
            push_unique_object_id(&mut object_ids, &mut seen, entry.object_id.as_str());
        }
        push_unique_object_id(&mut object_ids, &mut seen, self.object_id.as_str());
        object_ids
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProfileSyncMembershipRecordPullStatus {
    NoPublishedRoot {
        profile: String,
        root_id: String,
    },
    Unchanged {
        profile: String,
        root_id: String,
        object_id: String,
    },
    Applied {
        root: ProfileSyncRootRecord,
        application: SyncAccountMembershipRecordApplication,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProfileSyncMembershipLogPullStatus {
    NoPublishedRoot {
        profile: String,
        root_id: String,
    },
    Unchanged {
        profile: String,
        root_id: String,
        object_id: String,
    },
    Applied {
        root: ProfileSyncRootRecord,
        log: ProfileSyncMembershipLog,
        applications: Vec<SyncAccountMembershipRecordApplication>,
    },
}

impl ProfileSyncMembershipLogPullStatus {
    pub fn applied_count(&self) -> usize {
        match self {
            Self::Applied { applications, .. } => applications.len(),
            Self::NoPublishedRoot { .. } | Self::Unchanged { .. } => 0,
        }
    }
}

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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublishedSettingsTailManifest {
    pub manifest_object_id: String,
    pub manifest: ProfileSyncManifest,
    pub tail_change_object_ids: Vec<String>,
}

impl PublishedSettingsTailManifest {
    pub fn published_object_ids(&self) -> Vec<String> {
        let mut object_ids = Vec::new();
        let mut seen = BTreeSet::new();
        for object_id in self.tail_change_object_ids.iter() {
            push_unique_object_id(&mut object_ids, &mut seen, object_id);
        }
        push_unique_object_id(&mut object_ids, &mut seen, &self.manifest_object_id);
        object_ids
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublishedSettingsSnapshotManifest {
    pub manifest_object_id: String,
    pub manifest: ProfileSyncManifest,
    pub snapshot_object_id: String,
    pub tail_change_object_ids: Vec<String>,
}

impl PublishedSettingsSnapshotManifest {
    pub fn published_object_ids(&self) -> Vec<String> {
        let mut object_ids = Vec::new();
        let mut seen = BTreeSet::new();
        push_unique_object_id(&mut object_ids, &mut seen, &self.snapshot_object_id);
        for object_id in self.tail_change_object_ids.iter() {
            push_unique_object_id(&mut object_ids, &mut seen, object_id);
        }
        push_unique_object_id(&mut object_ids, &mut seen, &self.manifest_object_id);
        object_ids
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublishedSettingsCompaction {
    pub target: SyncCompactionTarget,
    pub publication: PublishedSettingsSnapshotManifest,
    pub snapshot_record: SyncSnapshotRecord,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublishedProfileSyncDeviceHead {
    pub root_id: String,
    pub object_id: String,
    pub device_head: ProfileSyncDeviceHead,
}

impl PublishedProfileSyncDeviceHead {
    pub fn published_object_ids(&self) -> Vec<String> {
        vec![self.object_id.clone()]
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublishedLocalSettingsSnapshotHead {
    pub publication: PublishedSettingsSnapshotManifest,
    pub device_head: PublishedProfileSyncDeviceHead,
    pub snapshot_record: SyncSnapshotRecord,
    pub settings_root: ProfileSyncRootRecord,
    pub device_head_root: ProfileSyncRootRecord,
}

impl PublishedLocalSettingsSnapshotHead {
    pub fn published_object_ids(&self) -> Vec<String> {
        let mut object_ids = Vec::new();
        let mut seen = BTreeSet::new();
        extend_unique_object_ids(
            &mut object_ids,
            &mut seen,
            self.publication.published_object_ids(),
        );
        extend_unique_object_ids(
            &mut object_ids,
            &mut seen,
            self.device_head.published_object_ids(),
        );
        push_unique_object_id(&mut object_ids, &mut seen, &self.settings_root.object_id);
        push_unique_object_id(&mut object_ids, &mut seen, &self.device_head_root.object_id);
        object_ids
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublishedLocalSettingsTailHead {
    pub publication: PublishedSettingsSnapshotManifest,
    pub device_head: PublishedProfileSyncDeviceHead,
    pub snapshot_record: SyncSnapshotRecord,
    pub settings_root: ProfileSyncRootRecord,
    pub device_head_root: ProfileSyncRootRecord,
}

impl PublishedLocalSettingsTailHead {
    pub fn published_object_ids(&self) -> Vec<String> {
        let mut object_ids = Vec::new();
        let mut seen = BTreeSet::new();
        extend_unique_object_ids(
            &mut object_ids,
            &mut seen,
            self.publication.published_object_ids(),
        );
        extend_unique_object_ids(
            &mut object_ids,
            &mut seen,
            self.device_head.published_object_ids(),
        );
        push_unique_object_id(&mut object_ids, &mut seen, &self.settings_root.object_id);
        push_unique_object_id(&mut object_ids, &mut seen, &self.device_head_root.object_id);
        object_ids
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LocalSettingsHeadPublishStatus {
    PublishedFullSnapshot(PublishedLocalSettingsSnapshotHead),
    PublishedIncrementalTail(PublishedLocalSettingsTailHead),
    NoLocalSettingsChanges,
    UpToDate { snapshot_record: SyncSnapshotRecord },
}

impl LocalSettingsHeadPublishStatus {
    pub fn is_idle(&self) -> bool {
        matches!(self, Self::NoLocalSettingsChanges | Self::UpToDate { .. })
    }

    pub fn published_manifest_object_id(&self) -> Option<&str> {
        match self {
            Self::PublishedFullSnapshot(published) => {
                Some(published.publication.manifest_object_id.as_str())
            }
            Self::PublishedIncrementalTail(published) => {
                Some(published.publication.manifest_object_id.as_str())
            }
            Self::NoLocalSettingsChanges | Self::UpToDate { .. } => None,
        }
    }

    pub fn published_object_ids(&self) -> Vec<String> {
        match self {
            Self::PublishedFullSnapshot(published) => published.published_object_ids(),
            Self::PublishedIncrementalTail(published) => published.published_object_ids(),
            Self::NoLocalSettingsChanges | Self::UpToDate { .. } => Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalSettingsSyncRun {
    pub profile: String,
    pub settings_root_id: String,
    pub steps: Vec<LocalSettingsHeadPublishStatus>,
}

impl LocalSettingsSyncRun {
    pub fn is_idle(&self) -> bool {
        self.steps
            .last()
            .map(LocalSettingsHeadPublishStatus::is_idle)
            .unwrap_or(false)
    }

    pub fn published_step_count(&self) -> usize {
        self.steps
            .iter()
            .filter(|step| step.published_manifest_object_id().is_some())
            .count()
    }

    pub fn published_object_ids(&self) -> Vec<String> {
        let mut object_ids = Vec::new();
        let mut seen = BTreeSet::new();
        for step in self.steps.iter() {
            extend_unique_object_ids(&mut object_ids, &mut seen, step.published_object_ids());
        }
        object_ids
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BroadwebdTrustedDeviceHeadSyncStatus {
    NoPublishedRoot {
        profile: String,
        root_id: String,
    },
    Unchanged {
        profile: String,
        root_id: String,
        object_id: String,
    },
    Applied {
        device_head: VerifiedProfileSyncDeviceHead,
        root: ProfileSyncRootRecord,
        application: ProfileSyncSettingsManifestApplication,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TrustedSettingsDeviceSyncResult {
    pub device_id: String,
    pub root_id: String,
    pub status: BroadwebdTrustedDeviceHeadSyncStatus,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TrustedSettingsSyncRun {
    pub profile: String,
    pub devices: Vec<TrustedSettingsDeviceSyncResult>,
}

impl TrustedSettingsSyncRun {
    pub fn applied_count(&self) -> usize {
        self.devices
            .iter()
            .filter(|device| {
                matches!(
                    device.status,
                    BroadwebdTrustedDeviceHeadSyncStatus::Applied { .. }
                )
            })
            .count()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SettingsSyncCycleRun {
    pub profile: String,
    pub settings_root_id: String,
    pub publish: LocalSettingsSyncRun,
    pub receive: TrustedSettingsSyncRun,
}

impl SettingsSyncCycleRun {
    pub fn published_step_count(&self) -> usize {
        self.publish.published_step_count()
    }

    pub fn applied_count(&self) -> usize {
        self.receive.applied_count()
    }

    pub fn published_object_ids(&self) -> Vec<String> {
        self.publish.published_object_ids()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SettingsSyncCycleWithMembershipLogRun {
    pub pulled_membership_log: ProfileSyncMembershipLogPullStatus,
    pub cycle: SettingsSyncCycleRun,
    pub published_membership_log: Option<PublishedProfileSyncMembershipLog>,
}

impl SettingsSyncCycleWithMembershipLogRun {
    pub fn pulled_membership_application_count(&self) -> usize {
        self.pulled_membership_log.applied_count()
    }

    pub fn published_object_ids(&self) -> Vec<String> {
        let mut object_ids = self.cycle.published_object_ids();
        let mut seen = object_ids.iter().cloned().collect::<BTreeSet<_>>();
        if let Some(published_membership_log) = &self.published_membership_log {
            extend_unique_object_ids(
                &mut object_ids,
                &mut seen,
                published_membership_log.published_object_ids(),
            );
        }
        object_ids
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SettingsSyncCycleWithMembershipLogRetentionRun {
    pub cycle: SettingsSyncCycleWithMembershipLogRun,
    pub retained_object_ids: Vec<String>,
    pub retention: Vec<SettingsSyncCycleProviderRetentionRun>,
}

impl SettingsSyncCycleWithMembershipLogRetentionRun {
    pub fn retained_provider_count(&self) -> usize {
        self.retention
            .iter()
            .filter(|provider| {
                provider.object_count() > 0 && provider.object_count() == provider.retained_count()
            })
            .count()
    }
}

fn push_unique_object_id(
    object_ids: &mut Vec<String>,
    seen: &mut BTreeSet<String>,
    object_id: &str,
) {
    let object_id = object_id.to_string();
    if seen.insert(object_id.clone()) {
        object_ids.push(object_id);
    }
}

fn extend_unique_object_ids(
    object_ids: &mut Vec<String>,
    seen: &mut BTreeSet<String>,
    incoming: impl IntoIterator<Item = String>,
) {
    for object_id in incoming {
        push_unique_object_id(object_ids, seen, &object_id);
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SettingsSyncCycleWithHealthRun {
    pub before_health: SettingsSyncHealthReport,
    pub cycle: SettingsSyncCycleRun,
    pub after_health: SettingsSyncHealthReport,
}

impl SettingsSyncCycleWithHealthRun {
    pub fn degraded_before(&self) -> bool {
        self.before_health.degraded()
    }

    pub fn degraded_after(&self) -> bool {
        self.after_health.degraded()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SettingsSyncCycleProviderRetentionRun {
    pub provider_index: usize,
    pub object_statuses: Vec<BroadwebdProfileSyncRetentionStatus>,
}

impl SettingsSyncCycleProviderRetentionRun {
    pub fn object_count(&self) -> usize {
        self.object_statuses.len()
    }

    pub fn retained_count(&self) -> usize {
        self.object_statuses
            .iter()
            .filter(|status| status.retained)
            .count()
    }

    pub fn available_count(&self) -> usize {
        self.object_statuses
            .iter()
            .filter(|status| status.available)
            .count()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SettingsSyncCycleWithRetentionRun {
    pub before_health: SettingsSyncHealthReport,
    pub cycle: SettingsSyncCycleRun,
    pub retention: Vec<SettingsSyncCycleProviderRetentionRun>,
    pub after_health: SettingsSyncHealthReport,
}

impl SettingsSyncCycleWithRetentionRun {
    pub fn degraded_before(&self) -> bool {
        self.before_health.degraded()
    }

    pub fn degraded_after(&self) -> bool {
        self.after_health.degraded()
    }

    pub fn retained_provider_count(&self) -> usize {
        self.retention
            .iter()
            .filter(|provider| {
                provider.object_count() > 0 && provider.object_count() == provider.retained_count()
            })
            .count()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SettingsSyncCycleWithSharedRootCandidatesRun {
    pub before_health: SettingsSyncHealthReport,
    pub cycle: SettingsSyncCycleRun,
    pub shared_root_candidates: ProfileSyncSettingsCandidatePullApplyStatus,
    pub after_health: SettingsSyncHealthReport,
}

impl SettingsSyncCycleWithSharedRootCandidatesRun {
    pub fn degraded_before(&self) -> bool {
        self.before_health.degraded()
    }

    pub fn degraded_after(&self) -> bool {
        self.after_health.degraded()
    }

    pub fn shared_root_candidate_application_count(&self) -> usize {
        match &self.shared_root_candidates {
            ProfileSyncSettingsCandidatePullApplyStatus::Applied(applications) => {
                applications.len()
            }
            ProfileSyncSettingsCandidatePullApplyStatus::NoPublishedRoot { .. }
            | ProfileSyncSettingsCandidatePullApplyStatus::Unchanged { .. } => 0,
        }
    }

    pub fn shared_root_candidate_object_ids(&self) -> Vec<String> {
        shared_root_candidate_object_ids(&self.shared_root_candidates)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SettingsSyncCycleWithSharedRootRetentionRun {
    pub before_health: SettingsSyncHealthReport,
    pub cycle: SettingsSyncCycleRun,
    pub shared_root_candidates: ProfileSyncSettingsCandidatePullApplyStatus,
    pub retained_object_ids: Vec<String>,
    pub retention: Vec<SettingsSyncCycleProviderRetentionRun>,
    pub after_health: SettingsSyncHealthReport,
}

impl SettingsSyncCycleWithSharedRootRetentionRun {
    pub fn degraded_before(&self) -> bool {
        self.before_health.degraded()
    }

    pub fn degraded_after(&self) -> bool {
        self.after_health.degraded()
    }

    pub fn shared_root_candidate_application_count(&self) -> usize {
        match &self.shared_root_candidates {
            ProfileSyncSettingsCandidatePullApplyStatus::Applied(applications) => {
                applications.len()
            }
            ProfileSyncSettingsCandidatePullApplyStatus::NoPublishedRoot { .. }
            | ProfileSyncSettingsCandidatePullApplyStatus::Unchanged { .. } => 0,
        }
    }

    pub fn shared_root_candidate_object_ids(&self) -> Vec<String> {
        shared_root_candidate_object_ids(&self.shared_root_candidates)
    }

    pub fn retained_provider_count(&self) -> usize {
        self.retention
            .iter()
            .filter(|provider| {
                provider.object_count() > 0 && provider.object_count() == provider.retained_count()
            })
            .count()
    }
}

fn shared_root_candidate_object_ids(
    status: &ProfileSyncSettingsCandidatePullApplyStatus,
) -> Vec<String> {
    let mut object_ids = Vec::new();
    let mut seen = BTreeSet::new();
    if let ProfileSyncSettingsCandidatePullApplyStatus::Applied(applications) = status {
        for application in applications {
            extend_unique_object_ids(
                &mut object_ids,
                &mut seen,
                application.application.sync_object_ids.clone(),
            );
        }
    }
    object_ids
}

#[derive(Clone, Copy)]
pub struct SettingsSyncRetentionProviderHandle<'a> {
    pub provider_id: &'a str,
    pub daemon: &'a BroadwebDaemon,
}

impl<'a> SettingsSyncRetentionProviderHandle<'a> {
    pub fn new(provider_id: &'a str, daemon: &'a BroadwebDaemon) -> Self {
        Self {
            provider_id,
            daemon,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SettingsSyncCyclePreflight {
    pub profile: String,
    pub settings_root_id: String,
    pub local_device_id: String,
    pub signer_device_id: String,
    pub active_key_id: String,
    pub trusted_remote_device_count: usize,
    pub retention_provider_candidates: Vec<BroadwebdProfileSyncProviderRecord>,
    pub before_health: SettingsSyncHealthReport,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SettingsSyncCyclePreflightWithMembershipLog {
    pub pulled_membership_log: ProfileSyncMembershipLogPullStatus,
    pub preflight: SettingsSyncCyclePreflight,
}

impl SettingsSyncCyclePreflightWithMembershipLog {
    pub fn pulled_membership_application_count(&self) -> usize {
        self.pulled_membership_log.applied_count()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SettingsSyncCyclePolicy {
    pub retention_policy: ProfileSyncRetentionPolicy,
    pub max_publish_steps: u32,
    pub max_trusted_devices: u32,
    pub minimum_fresh_online_providers: usize,
    pub minimum_object_transfer_providers: usize,
    pub minimum_availability_providers: usize,
    pub minimum_mutable_root_providers: usize,
    pub minimum_online_retaining_providers: usize,
    pub maximum_stale_online_providers: Option<usize>,
    pub maximum_offline_providers: Option<usize>,
    pub require_healthy_providers: bool,
    pub require_healthy_roots_after_cycle: bool,
    pub require_healthy_settings_root_after_cycle: bool,
    pub require_healthy_local_device_head_root_after_cycle: bool,
}

impl SettingsSyncCyclePolicy {
    pub fn new(
        retention_policy: ProfileSyncRetentionPolicy,
        max_publish_steps: u32,
        max_trusted_devices: u32,
        minimum_online_retaining_providers: usize,
    ) -> Self {
        Self {
            retention_policy,
            max_publish_steps,
            max_trusted_devices,
            minimum_fresh_online_providers: 1,
            minimum_object_transfer_providers: 1,
            minimum_availability_providers: 1,
            minimum_mutable_root_providers: 1,
            minimum_online_retaining_providers,
            maximum_stale_online_providers: None,
            maximum_offline_providers: None,
            require_healthy_providers: true,
            require_healthy_roots_after_cycle: true,
            require_healthy_settings_root_after_cycle: true,
            require_healthy_local_device_head_root_after_cycle: true,
        }
    }

    pub fn with_minimum_fresh_online_providers(mut self, minimum: usize) -> Self {
        self.minimum_fresh_online_providers = minimum;
        self
    }

    pub fn with_minimum_object_transfer_providers(mut self, minimum: usize) -> Self {
        self.minimum_object_transfer_providers = minimum;
        self
    }

    pub fn with_minimum_availability_providers(mut self, minimum: usize) -> Self {
        self.minimum_availability_providers = minimum;
        self
    }

    pub fn with_minimum_mutable_root_providers(mut self, minimum: usize) -> Self {
        self.minimum_mutable_root_providers = minimum;
        self
    }

    pub fn with_maximum_stale_online_providers(mut self, maximum: usize) -> Self {
        self.maximum_stale_online_providers = Some(maximum);
        self
    }

    pub fn with_maximum_offline_providers(mut self, maximum: usize) -> Self {
        self.maximum_offline_providers = Some(maximum);
        self
    }

    pub fn with_provider_health_required(mut self, required: bool) -> Self {
        self.require_healthy_providers = required;
        self
    }

    pub fn with_root_health_required_after_cycle(mut self, required: bool) -> Self {
        self.require_healthy_roots_after_cycle = required;
        self.require_healthy_settings_root_after_cycle = required;
        self.require_healthy_local_device_head_root_after_cycle = required;
        self
    }

    pub fn with_settings_root_health_required_after_cycle(mut self, required: bool) -> Self {
        self.require_healthy_settings_root_after_cycle = required;
        self
    }

    pub fn with_local_device_head_root_health_required_after_cycle(
        mut self,
        required: bool,
    ) -> Self {
        self.require_healthy_local_device_head_root_after_cycle = required;
        self
    }

    pub fn minimum_selected_retention_provider_count(&self) -> usize {
        self.minimum_online_retaining_providers.saturating_sub(1)
    }

    pub fn check_before_cycle(
        &self,
        health: &SettingsSyncHealthReport,
    ) -> Result<(), ProfileSyncPolicyError> {
        if !self.require_healthy_providers {
            return Ok(());
        }
        if health.provider_health.degraded {
            return Err(ProfileSyncPolicyError::ProviderHealthDegraded {
                health: health.clone(),
            });
        }
        self.check_provider_minimum(
            "fresh online providers",
            self.minimum_fresh_online_providers,
            health.provider_health.fresh_online_providers,
            health,
        )?;
        self.check_provider_minimum(
            "object-transfer providers",
            self.minimum_object_transfer_providers,
            health.provider_health.object_transfer_providers,
            health,
        )?;
        self.check_provider_minimum(
            "availability providers",
            self.minimum_availability_providers,
            health.provider_health.availability_providers,
            health,
        )?;
        self.check_provider_minimum(
            "mutable-root providers",
            self.minimum_mutable_root_providers,
            health.provider_health.mutable_root_providers,
            health,
        )?;
        if let Some(maximum) = self.maximum_stale_online_providers {
            self.check_provider_maximum(
                "stale online providers",
                maximum,
                health.provider_health.stale_online_providers,
                health,
            )?;
        }
        if let Some(maximum) = self.maximum_offline_providers {
            self.check_provider_maximum(
                "offline providers",
                maximum,
                health.provider_health.offline_providers,
                health,
            )?;
        }
        Ok(())
    }

    pub fn check_selected_retention_provider_count(
        &self,
        actual: usize,
        health: &SettingsSyncHealthReport,
    ) -> Result<(), ProfileSyncPolicyError> {
        self.check_provider_minimum(
            "selected retention providers",
            self.minimum_selected_retention_provider_count(),
            actual,
            health,
        )
    }

    pub fn check_selected_retention_provider_freshness(
        &self,
        stale_selected: usize,
        offline_selected: usize,
        health: &SettingsSyncHealthReport,
    ) -> Result<(), ProfileSyncPolicyError> {
        self.check_provider_maximum(
            "stale selected retention providers",
            0,
            stale_selected,
            health,
        )?;
        self.check_provider_maximum(
            "offline selected retention providers",
            0,
            offline_selected,
            health,
        )
    }

    fn check_provider_minimum(
        &self,
        provider_role: &'static str,
        minimum: usize,
        actual: usize,
        health: &SettingsSyncHealthReport,
    ) -> Result<(), ProfileSyncPolicyError> {
        if actual < minimum {
            return Err(ProfileSyncPolicyError::ProviderMinimumUnmet {
                provider_role,
                minimum,
                actual,
                health: health.clone(),
            });
        }
        Ok(())
    }

    fn check_provider_maximum(
        &self,
        provider_role: &'static str,
        maximum: usize,
        actual: usize,
        health: &SettingsSyncHealthReport,
    ) -> Result<(), ProfileSyncPolicyError> {
        if actual > maximum {
            return Err(ProfileSyncPolicyError::ProviderMaximumExceeded {
                provider_role,
                maximum,
                actual,
                health: health.clone(),
            });
        }
        Ok(())
    }

    pub fn check_after_cycle(
        &self,
        health: &SettingsSyncHealthReport,
    ) -> Result<(), ProfileSyncPolicyError> {
        if !self.require_healthy_roots_after_cycle {
            return Ok(());
        }
        if self.require_healthy_settings_root_after_cycle && health.settings_root_health.degraded {
            return Err(ProfileSyncPolicyError::RootHealthDegraded {
                root_kind: "settings root",
                health: health.clone(),
            });
        }
        if self.require_healthy_local_device_head_root_after_cycle
            && health.local_device_head_root_health.degraded
        {
            return Err(ProfileSyncPolicyError::RootHealthDegraded {
                root_kind: "local device-head root",
                health: health.clone(),
            });
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SettingsSyncSchedulerConfig {
    pub profile: String,
    pub settings_root_id: String,
    pub policy: SettingsSyncCyclePolicy,
}

impl SettingsSyncSchedulerConfig {
    pub fn new(
        profile: impl Into<String>,
        settings_root_id: impl Into<String>,
        policy: SettingsSyncCyclePolicy,
    ) -> Self {
        Self {
            profile: profile.into(),
            settings_root_id: settings_root_id.into(),
            policy,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SettingsSyncScheduledCycleRun {
    pub preflight: SettingsSyncCyclePreflight,
    pub selected_retention_provider_ids: Vec<String>,
    pub stale_retention_provider_ids: Vec<String>,
    pub offline_retention_provider_ids: Vec<String>,
    pub undiscovered_retention_provider_ids: Vec<String>,
    pub duplicate_retention_provider_ids: Vec<String>,
    pub cycle: SettingsSyncCycleWithSharedRootRetentionRun,
}

impl SettingsSyncScheduledCycleRun {
    pub fn selected_retention_provider_count(&self) -> usize {
        self.selected_retention_provider_ids.len()
    }

    pub fn undiscovered_retention_provider_count(&self) -> usize {
        self.undiscovered_retention_provider_ids.len()
    }

    pub fn stale_retention_provider_count(&self) -> usize {
        self.stale_retention_provider_ids.len()
    }

    pub fn offline_retention_provider_count(&self) -> usize {
        self.offline_retention_provider_ids.len()
    }

    pub fn duplicate_retention_provider_count(&self) -> usize {
        self.duplicate_retention_provider_ids.len()
    }

    pub fn retained_provider_count(&self) -> usize {
        self.cycle.retained_provider_count()
    }

    pub fn degraded_before(&self) -> bool {
        self.cycle.degraded_before()
    }

    pub fn degraded_after(&self) -> bool {
        self.cycle.degraded_after()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SettingsSyncScheduledMembershipCycleRun {
    pub preflight: SettingsSyncCyclePreflightWithMembershipLog,
    pub selected_retention_provider_ids: Vec<String>,
    pub stale_retention_provider_ids: Vec<String>,
    pub offline_retention_provider_ids: Vec<String>,
    pub undiscovered_retention_provider_ids: Vec<String>,
    pub duplicate_retention_provider_ids: Vec<String>,
    pub cycle: SettingsSyncCycleWithMembershipLogRetentionRun,
}

impl SettingsSyncScheduledMembershipCycleRun {
    pub fn selected_retention_provider_count(&self) -> usize {
        self.selected_retention_provider_ids.len()
    }

    pub fn undiscovered_retention_provider_count(&self) -> usize {
        self.undiscovered_retention_provider_ids.len()
    }

    pub fn stale_retention_provider_count(&self) -> usize {
        self.stale_retention_provider_ids.len()
    }

    pub fn offline_retention_provider_count(&self) -> usize {
        self.offline_retention_provider_ids.len()
    }

    pub fn duplicate_retention_provider_count(&self) -> usize {
        self.duplicate_retention_provider_ids.len()
    }

    pub fn retained_provider_count(&self) -> usize {
        self.cycle.retained_provider_count()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SettingsSyncScheduledCyclePlan {
    pub preflight: SettingsSyncCyclePreflight,
    pub selected_retention_provider_ids: Vec<String>,
    pub stale_retention_provider_ids: Vec<String>,
    pub offline_retention_provider_ids: Vec<String>,
    pub undiscovered_retention_provider_ids: Vec<String>,
    pub duplicate_retention_provider_ids: Vec<String>,
}

impl SettingsSyncScheduledCyclePlan {
    pub fn retention_candidate_count(&self) -> usize {
        self.preflight.retention_provider_candidates.len()
    }

    pub fn selected_retention_provider_count(&self) -> usize {
        self.selected_retention_provider_ids.len()
    }

    pub fn stale_retention_provider_count(&self) -> usize {
        self.stale_retention_provider_ids.len()
    }

    pub fn offline_retention_provider_count(&self) -> usize {
        self.offline_retention_provider_ids.len()
    }

    pub fn undiscovered_retention_provider_count(&self) -> usize {
        self.undiscovered_retention_provider_ids.len()
    }

    pub fn duplicate_retention_provider_count(&self) -> usize {
        self.duplicate_retention_provider_ids.len()
    }

    pub fn degraded_before(&self) -> bool {
        self.preflight.before_health.degraded()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SettingsSyncScheduledMembershipCyclePlan {
    pub membership_log_publication: ProfileSyncMembershipLogPublicationPlan,
    pub cycle: SettingsSyncScheduledCyclePlan,
}

impl SettingsSyncScheduledMembershipCyclePlan {
    pub fn retention_candidate_count(&self) -> usize {
        self.cycle.retention_candidate_count()
    }

    pub fn selected_retention_provider_count(&self) -> usize {
        self.cycle.selected_retention_provider_count()
    }

    pub fn stale_retention_provider_count(&self) -> usize {
        self.cycle.stale_retention_provider_count()
    }

    pub fn offline_retention_provider_count(&self) -> usize {
        self.cycle.offline_retention_provider_count()
    }

    pub fn undiscovered_retention_provider_count(&self) -> usize {
        self.cycle.undiscovered_retention_provider_count()
    }

    pub fn duplicate_retention_provider_count(&self) -> usize {
        self.cycle.duplicate_retention_provider_count()
    }

    pub fn degraded_before(&self) -> bool {
        self.cycle.degraded_before()
    }
}

struct SelectedSettingsSyncRetentionProviders<'a> {
    plan: SettingsSyncScheduledCyclePlan,
    daemons: Vec<&'a BroadwebDaemon>,
}

fn select_settings_sync_retention_provider_handles<'a>(
    preflight: SettingsSyncCyclePreflight,
    retention_provider_handles: &[SettingsSyncRetentionProviderHandle<'a>],
) -> SelectedSettingsSyncRetentionProviders<'a> {
    let (
        selected_retention_provider_ids,
        stale_retention_provider_ids,
        offline_retention_provider_ids,
        undiscovered_retention_provider_ids,
        duplicate_retention_provider_ids,
        daemons,
    ) = {
        let candidate_provider_ids = preflight
            .retention_provider_candidates
            .iter()
            .map(|provider| provider.provider_id.as_str())
            .collect::<BTreeSet<_>>();
        let stale_provider_ids = preflight
            .before_health
            .provider_health
            .stale_online_provider_ids
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>();
        let offline_provider_ids = preflight
            .before_health
            .provider_health
            .offline_provider_ids
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>();
        let mut seen_selected_provider_ids = BTreeSet::new();
        let mut seen_stale_provider_ids = BTreeSet::new();
        let mut seen_offline_provider_ids = BTreeSet::new();
        let mut seen_undiscovered_provider_ids = BTreeSet::new();
        let mut seen_duplicate_provider_ids = BTreeSet::new();
        let mut selected_retention_provider_ids = Vec::new();
        let mut stale_retention_provider_ids = Vec::new();
        let mut offline_retention_provider_ids = Vec::new();
        let mut undiscovered_retention_provider_ids = Vec::new();
        let mut duplicate_retention_provider_ids = Vec::new();
        let mut daemons = Vec::new();

        for handle in retention_provider_handles {
            if !candidate_provider_ids.contains(handle.provider_id) {
                if stale_provider_ids.contains(handle.provider_id) {
                    if seen_stale_provider_ids.insert(handle.provider_id) {
                        stale_retention_provider_ids.push(handle.provider_id.to_string());
                    }
                } else if offline_provider_ids.contains(handle.provider_id) {
                    if seen_offline_provider_ids.insert(handle.provider_id) {
                        offline_retention_provider_ids.push(handle.provider_id.to_string());
                    }
                } else if seen_undiscovered_provider_ids.insert(handle.provider_id) {
                    undiscovered_retention_provider_ids.push(handle.provider_id.to_string());
                }
                continue;
            }
            if !seen_selected_provider_ids.insert(handle.provider_id) {
                if seen_duplicate_provider_ids.insert(handle.provider_id) {
                    duplicate_retention_provider_ids.push(handle.provider_id.to_string());
                }
                continue;
            }
            selected_retention_provider_ids.push(handle.provider_id.to_string());
            daemons.push(handle.daemon);
        }

        (
            selected_retention_provider_ids,
            stale_retention_provider_ids,
            offline_retention_provider_ids,
            undiscovered_retention_provider_ids,
            duplicate_retention_provider_ids,
            daemons,
        )
    };

    SelectedSettingsSyncRetentionProviders {
        plan: SettingsSyncScheduledCyclePlan {
            preflight,
            selected_retention_provider_ids,
            stale_retention_provider_ids,
            offline_retention_provider_ids,
            undiscovered_retention_provider_ids,
            duplicate_retention_provider_ids,
        },
        daemons,
    }
}

#[derive(Clone, Copy)]
pub struct SettingsSyncRuntimeSecrets<'a> {
    pub content_key: &'a ProfileSyncContentKey,
    pub signer: &'a ProfileSyncDeviceSigner,
}

impl<'a> SettingsSyncRuntimeSecrets<'a> {
    pub fn new(
        content_key: &'a ProfileSyncContentKey,
        signer: &'a ProfileSyncDeviceSigner,
    ) -> Self {
        Self {
            content_key,
            signer,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SettingsSyncHealthReport {
    pub profile: String,
    pub settings_root_id: String,
    pub local_device_head_root_id: String,
    pub provider_health: BroadwebdProfileSyncProviderHealth,
    pub settings_root_health: BroadwebdProfileSyncRootHealth,
    pub local_device_head_root_health: BroadwebdProfileSyncRootHealth,
}

impl SettingsSyncHealthReport {
    pub fn degraded(&self) -> bool {
        self.provider_health.degraded
            || self.settings_root_health.degraded
            || self.local_device_head_root_health.degraded
    }
}

pub fn settings_device_head_root_id(device_id: &str) -> String {
    format!("settings/devices/{device_id}/head")
}

pub fn sync_membership_record_root_id(record_id: &str) -> String {
    format!("account/membership/{record_id}")
}

impl<'a> BroadwebdProfileSyncObjectSource<'a> {
    pub fn new(daemon: &'a BroadwebDaemon) -> Self {
        Self { daemon }
    }

    pub fn pull_and_apply_sync_account_membership_record_if_changed(
        &self,
        database: &SlateProfileDatabase,
        profile: &str,
        root_id: &str,
    ) -> Result<ProfileSyncMembershipRecordPullStatus, ProfileSyncReceiveError> {
        let Some(object_id) = self.resolve_profile_sync_root(profile, root_id)? else {
            return Ok(ProfileSyncMembershipRecordPullStatus::NoPublishedRoot {
                profile: profile.to_string(),
                root_id: root_id.to_string(),
            });
        };
        if database
            .profile_sync_root(profile, root_id)?
            .is_some_and(|root| root.object_id == object_id)
        {
            return Ok(ProfileSyncMembershipRecordPullStatus::Unchanged {
                profile: profile.to_string(),
                root_id: root_id.to_string(),
                object_id,
            });
        }

        let signed_record = self.get_profile_sync_object(profile, object_id.as_str())?;
        let application =
            database.apply_signed_sync_account_membership_record(signed_record.bytes.as_slice())?;
        let root = database.set_profile_sync_root(profile, root_id, object_id.as_str())?;
        Ok(ProfileSyncMembershipRecordPullStatus::Applied { root, application })
    }

    pub fn pull_and_apply_sync_account_membership_log_if_changed(
        &self,
        database: &SlateProfileDatabase,
        profile: &str,
        root_id: &str,
    ) -> Result<ProfileSyncMembershipLogPullStatus, ProfileSyncReceiveError> {
        let Some(object_id) = self.resolve_profile_sync_root(profile, root_id)? else {
            return Ok(ProfileSyncMembershipLogPullStatus::NoPublishedRoot {
                profile: profile.to_string(),
                root_id: root_id.to_string(),
            });
        };
        if database
            .profile_sync_root(profile, root_id)?
            .is_some_and(|root| root.object_id == object_id)
        {
            return Ok(ProfileSyncMembershipLogPullStatus::Unchanged {
                profile: profile.to_string(),
                root_id: root_id.to_string(),
                object_id,
            });
        }

        let log_object = self.get_profile_sync_object(profile, object_id.as_str())?;
        let log = decode_profile_sync_membership_log(log_object.bytes.as_slice(), profile)?;
        let mut applications = Vec::with_capacity(log.records.len());
        for entry in &log.records {
            let signed_record = self.get_profile_sync_object(profile, entry.object_id.as_str())?;
            validate_membership_log_entry_object(profile, entry, signed_record.bytes.as_slice())?;
            applications.push(
                database
                    .apply_signed_sync_account_membership_record(signed_record.bytes.as_slice())?,
            );
        }
        let root = database.set_profile_sync_root(profile, root_id, object_id.as_str())?;
        Ok(ProfileSyncMembershipLogPullStatus::Applied {
            root,
            log,
            applications,
        })
    }

    pub fn pull_record_and_apply_trusted_settings_from_device_head(
        &self,
        database: &SlateProfileDatabase,
        profile: &str,
        root_id: &str,
        content_key: &ProfileSyncContentKey,
        key_id: &str,
    ) -> Result<
        BroadwebdTrustedDeviceHeadSyncStatus,
        ProfileSyncTrustedPullApplyError<BroadwebdError>,
    > {
        match database.pull_and_record_trusted_signed_profile_sync_device_head_if_changed(
            self,
            profile,
            root_id,
            content_key,
            key_id,
        )? {
            ProfileSyncDeviceHeadPullRecordStatus::NoPublishedRoot { profile, root_id } => {
                Ok(BroadwebdTrustedDeviceHeadSyncStatus::NoPublishedRoot { profile, root_id })
            }
            ProfileSyncDeviceHeadPullRecordStatus::Unchanged {
                profile,
                root_id,
                object_id,
            } => Ok(BroadwebdTrustedDeviceHeadSyncStatus::Unchanged {
                profile,
                root_id,
                object_id,
            }),
            ProfileSyncDeviceHeadPullRecordStatus::Updated { device_head, root } => {
                let application = database
                    .pull_and_apply_trusted_signed_settings_manifest_objects_from_device_head(
                        self,
                        profile,
                        &device_head,
                        content_key,
                        key_id,
                    )?;
                Ok(BroadwebdTrustedDeviceHeadSyncStatus::Applied {
                    device_head,
                    root,
                    application,
                })
            }
        }
    }

    pub fn pull_and_apply_trusted_settings_from_registered_devices(
        &self,
        database: &SlateProfileDatabase,
        profile: &str,
        content_key: &ProfileSyncContentKey,
        key_id: &str,
        max_devices: u32,
    ) -> Result<TrustedSettingsSyncRun, ProfileSyncReceiveError> {
        let trusted_devices = trusted_remote_device_public_keys(database, profile, max_devices)?;

        let mut devices = Vec::with_capacity(trusted_devices.len());
        for trusted_device in trusted_devices {
            let device_id = trusted_device.public_key.device_id;
            let root_id = settings_device_head_root_id(device_id.as_str());
            let status = self.pull_record_and_apply_trusted_settings_from_device_head(
                database,
                profile,
                root_id.as_str(),
                content_key,
                key_id,
            )?;
            devices.push(TrustedSettingsDeviceSyncResult {
                device_id,
                root_id,
                status,
            });
        }

        Ok(TrustedSettingsSyncRun {
            profile: profile.to_string(),
            devices,
        })
    }

    pub fn pull_and_apply_active_trusted_settings_manifest_candidates_if_changed(
        &self,
        database: &SlateProfileDatabase,
        profile: &str,
        root_id: &str,
        content_key: &ProfileSyncContentKey,
    ) -> Result<
        ProfileSyncSettingsCandidatePullApplyStatus,
        ProfileSyncTrustedPullApplyError<BroadwebdError>,
    > {
        database.pull_and_apply_active_trusted_signed_settings_manifest_candidates_if_changed(
            self,
            profile,
            root_id,
            content_key,
        )
    }
}

impl<'a> BroadwebdSettingsSyncRunner<'a> {
    pub fn new(daemon: &'a BroadwebDaemon) -> Self {
        Self { daemon }
    }

    pub fn settings_sync_health(
        &self,
        database: &SlateProfileDatabase,
        profile: &str,
        settings_root_id: &str,
        minimum_online_retaining_providers: usize,
    ) -> Result<SettingsSyncHealthReport, BroadwebdError> {
        let local_device_head_root_id =
            settings_device_head_root_id(database.local_sync_device_id());
        let provider_health = self.profile_sync_provider_health(profile)?;
        let settings_root_health = self.profile_sync_root_health(
            profile,
            settings_root_id,
            minimum_online_retaining_providers,
        )?;
        let local_device_head_root_health = self.profile_sync_root_health(
            profile,
            local_device_head_root_id.as_str(),
            minimum_online_retaining_providers,
        )?;

        Ok(SettingsSyncHealthReport {
            profile: profile.to_string(),
            settings_root_id: settings_root_id.to_string(),
            local_device_head_root_id,
            provider_health,
            settings_root_health,
            local_device_head_root_health,
        })
    }

    pub fn profile_sync_provider_health(
        &self,
        profile: &str,
    ) -> Result<BroadwebdProfileSyncProviderHealth, BroadwebdError> {
        let response = self
            .daemon
            .profile_sync(BroadwebdProfileSyncRequest::ProviderHealth(
                BroadwebdProfileSyncProfileRequest::new(profile),
            ))?;
        let BroadwebdProfileSyncResponse::ProviderHealth { health } = response else {
            return Err(BroadwebdError::UnsupportedRequest(
                "profile-sync provider health returned a non-health response".to_string(),
            ));
        };
        Ok(health)
    }

    pub fn profile_sync_providers(
        &self,
        profile: &str,
    ) -> Result<Vec<BroadwebdProfileSyncProviderRecord>, BroadwebdError> {
        let response = self
            .daemon
            .profile_sync(BroadwebdProfileSyncRequest::DiscoverProviders(
                BroadwebdProfileSyncProfileRequest::new(profile),
            ))?;
        let BroadwebdProfileSyncResponse::Providers { providers } = response else {
            return Err(BroadwebdError::UnsupportedRequest(
                "profile-sync provider discovery returned a non-provider response".to_string(),
            ));
        };
        Ok(providers)
    }

    pub fn profile_sync_retention_provider_candidates(
        &self,
        profile: &str,
    ) -> Result<Vec<BroadwebdProfileSyncProviderRecord>, BroadwebdError> {
        Ok(self
            .profile_sync_providers(profile)?
            .into_iter()
            .filter(|provider| provider.roles.availability && provider.roles.object_transfer)
            .collect())
    }

    pub fn profile_sync_root_health(
        &self,
        profile: &str,
        root_id: &str,
        minimum_online_retaining_providers: usize,
    ) -> Result<BroadwebdProfileSyncRootHealth, BroadwebdError> {
        let response = self
            .daemon
            .profile_sync(BroadwebdProfileSyncRequest::RootHealth(
                BroadwebdProfileSyncRootHealthRequest::with_minimum_online_retaining_providers(
                    profile,
                    root_id,
                    minimum_online_retaining_providers,
                ),
            ))?;
        let BroadwebdProfileSyncResponse::RootHealth { health } = response else {
            return Err(BroadwebdError::UnsupportedRequest(
                "profile-sync root health returned a non-health response".to_string(),
            ));
        };
        Ok(health)
    }

    pub fn run_settings_sync_cycle_with_health(
        &self,
        database: &SlateProfileDatabase,
        profile: &str,
        settings_root_id: &str,
        content_key: &ProfileSyncContentKey,
        key_id: &str,
        signer: &ProfileSyncDeviceSigner,
        retention_policy: ProfileSyncRetentionPolicy,
        max_publish_steps: u32,
        max_trusted_devices: u32,
        minimum_online_retaining_providers: usize,
    ) -> Result<SettingsSyncCycleWithHealthRun, ProfileSyncCycleWithHealthError> {
        let policy = SettingsSyncCyclePolicy::new(
            retention_policy,
            max_publish_steps,
            max_trusted_devices,
            minimum_online_retaining_providers,
        )
        .with_provider_health_required(false)
        .with_root_health_required_after_cycle(false);
        self.run_settings_sync_cycle_with_policy(
            database,
            profile,
            settings_root_id,
            content_key,
            key_id,
            signer,
            &policy,
        )
    }

    pub fn run_settings_sync_cycle_with_policy(
        &self,
        database: &SlateProfileDatabase,
        profile: &str,
        settings_root_id: &str,
        content_key: &ProfileSyncContentKey,
        key_id: &str,
        signer: &ProfileSyncDeviceSigner,
        policy: &SettingsSyncCyclePolicy,
    ) -> Result<SettingsSyncCycleWithHealthRun, ProfileSyncCycleWithHealthError> {
        let before_health = self.settings_sync_health(
            database,
            profile,
            settings_root_id,
            policy.minimum_online_retaining_providers,
        )?;
        policy.check_before_cycle(&before_health)?;
        let cycle = self.run_settings_sync_cycle(
            database,
            profile,
            settings_root_id,
            content_key,
            key_id,
            signer,
            policy.retention_policy.clone(),
            policy.max_publish_steps,
            policy.max_trusted_devices,
        )?;
        let after_health = self.settings_sync_health(
            database,
            profile,
            settings_root_id,
            policy.minimum_online_retaining_providers,
        )?;
        policy.check_after_cycle(&after_health)?;

        Ok(SettingsSyncCycleWithHealthRun {
            before_health,
            cycle,
            after_health,
        })
    }

    pub fn run_settings_sync_cycle_with_active_key_policy(
        &self,
        database: &SlateProfileDatabase,
        profile: &str,
        settings_root_id: &str,
        content_key: &ProfileSyncContentKey,
        signer: &ProfileSyncDeviceSigner,
        policy: &SettingsSyncCyclePolicy,
    ) -> Result<SettingsSyncCycleWithHealthRun, ProfileSyncCycleWithHealthError> {
        let preflight = self.settings_sync_cycle_preflight_with_active_key_policy(
            database,
            profile,
            settings_root_id,
            signer,
            policy,
        )?;
        let cycle = self.run_settings_sync_cycle(
            database,
            profile,
            settings_root_id,
            content_key,
            preflight.active_key_id.as_str(),
            signer,
            policy.retention_policy.clone(),
            policy.max_publish_steps,
            policy.max_trusted_devices,
        )?;
        let after_health = self.settings_sync_health(
            database,
            profile,
            settings_root_id,
            policy.minimum_online_retaining_providers,
        )?;
        policy.check_after_cycle(&after_health)?;

        Ok(SettingsSyncCycleWithHealthRun {
            before_health: preflight.before_health,
            cycle,
            after_health,
        })
    }

    pub fn run_settings_sync_cycle_with_active_key_policy_and_retention_providers(
        &self,
        database: &SlateProfileDatabase,
        profile: &str,
        settings_root_id: &str,
        content_key: &ProfileSyncContentKey,
        signer: &ProfileSyncDeviceSigner,
        policy: &SettingsSyncCyclePolicy,
        retention_provider_daemons: &[&BroadwebDaemon],
    ) -> Result<SettingsSyncCycleWithRetentionRun, ProfileSyncCycleWithHealthError> {
        let preflight = self.settings_sync_cycle_preflight_with_active_key_policy(
            database,
            profile,
            settings_root_id,
            signer,
            policy,
        )?;
        let cycle = self.run_settings_sync_cycle(
            database,
            profile,
            settings_root_id,
            content_key,
            preflight.active_key_id.as_str(),
            signer,
            policy.retention_policy.clone(),
            policy.max_publish_steps,
            policy.max_trusted_devices,
        )?;
        let mut retention = Vec::with_capacity(retention_provider_daemons.len());
        for (provider_index, provider_daemon) in retention_provider_daemons.iter().enumerate() {
            let object_statuses = BroadwebdProfileSyncPublisher::new(*provider_daemon)
                .retain_settings_sync_cycle_objects(&cycle)
                .map_err(ProfileSyncCycleWithHealthError::Retention)?;
            retention.push(SettingsSyncCycleProviderRetentionRun {
                provider_index,
                object_statuses,
            });
        }
        let after_health = self.settings_sync_health(
            database,
            profile,
            settings_root_id,
            policy.minimum_online_retaining_providers,
        )?;
        policy.check_after_cycle(&after_health)?;

        Ok(SettingsSyncCycleWithRetentionRun {
            before_health: preflight.before_health,
            cycle,
            retention,
            after_health,
        })
    }

    pub fn run_settings_sync_cycle_with_active_key_policy_and_shared_root_candidates(
        &self,
        database: &SlateProfileDatabase,
        profile: &str,
        settings_root_id: &str,
        content_key: &ProfileSyncContentKey,
        signer: &ProfileSyncDeviceSigner,
        policy: &SettingsSyncCyclePolicy,
    ) -> Result<SettingsSyncCycleWithSharedRootCandidatesRun, ProfileSyncCycleWithHealthError> {
        let preflight = self.settings_sync_cycle_preflight_with_active_key_policy(
            database,
            profile,
            settings_root_id,
            signer,
            policy,
        )?;
        let cycle = self.run_settings_sync_cycle(
            database,
            profile,
            settings_root_id,
            content_key,
            preflight.active_key_id.as_str(),
            signer,
            policy.retention_policy.clone(),
            policy.max_publish_steps,
            policy.max_trusted_devices,
        )?;
        let source = BroadwebdProfileSyncObjectSource::new(self.daemon);
        let shared_root_candidates = source
            .pull_and_apply_active_trusted_settings_manifest_candidates_if_changed(
                database,
                profile,
                settings_root_id,
                content_key,
            )
            .map_err(ProfileSyncReceiveError::from)
            .map_err(ProfileSyncCycleError::from)?;
        let after_health = self.settings_sync_health(
            database,
            profile,
            settings_root_id,
            policy.minimum_online_retaining_providers,
        )?;
        policy.check_after_cycle(&after_health)?;

        Ok(SettingsSyncCycleWithSharedRootCandidatesRun {
            before_health: preflight.before_health,
            cycle,
            shared_root_candidates,
            after_health,
        })
    }

    pub fn run_settings_sync_cycle_with_active_key_policy_shared_root_candidates_and_retention_providers(
        &self,
        database: &SlateProfileDatabase,
        profile: &str,
        settings_root_id: &str,
        content_key: &ProfileSyncContentKey,
        signer: &ProfileSyncDeviceSigner,
        policy: &SettingsSyncCyclePolicy,
        retention_provider_daemons: &[&BroadwebDaemon],
    ) -> Result<SettingsSyncCycleWithSharedRootRetentionRun, ProfileSyncCycleWithHealthError> {
        let preflight = self.settings_sync_cycle_preflight_with_active_key_policy(
            database,
            profile,
            settings_root_id,
            signer,
            policy,
        )?;
        self.run_settings_sync_cycle_with_active_key_policy_shared_root_candidates_and_retention_providers_after_preflight(
            database,
            content_key,
            signer,
            policy,
            retention_provider_daemons,
            &preflight,
        )
    }

    fn run_settings_sync_cycle_with_active_key_policy_shared_root_candidates_and_retention_providers_after_preflight(
        &self,
        database: &SlateProfileDatabase,
        content_key: &ProfileSyncContentKey,
        signer: &ProfileSyncDeviceSigner,
        policy: &SettingsSyncCyclePolicy,
        retention_provider_daemons: &[&BroadwebDaemon],
        preflight: &SettingsSyncCyclePreflight,
    ) -> Result<SettingsSyncCycleWithSharedRootRetentionRun, ProfileSyncCycleWithHealthError> {
        let cycle = self.run_settings_sync_cycle(
            database,
            preflight.profile.as_str(),
            preflight.settings_root_id.as_str(),
            content_key,
            preflight.active_key_id.as_str(),
            signer,
            policy.retention_policy.clone(),
            policy.max_publish_steps,
            policy.max_trusted_devices,
        )?;
        let source = BroadwebdProfileSyncObjectSource::new(self.daemon);
        let shared_root_candidates = source
            .pull_and_apply_active_trusted_settings_manifest_candidates_if_changed(
                database,
                preflight.profile.as_str(),
                preflight.settings_root_id.as_str(),
                content_key,
            )
            .map_err(ProfileSyncReceiveError::from)
            .map_err(ProfileSyncCycleError::from)?;

        let mut retained_object_ids = Vec::new();
        let mut seen = BTreeSet::new();
        extend_unique_object_ids(
            &mut retained_object_ids,
            &mut seen,
            cycle.published_object_ids(),
        );
        extend_unique_object_ids(
            &mut retained_object_ids,
            &mut seen,
            shared_root_candidate_object_ids(&shared_root_candidates),
        );

        let mut retention = Vec::with_capacity(retention_provider_daemons.len());
        for (provider_index, provider_daemon) in retention_provider_daemons.iter().enumerate() {
            let object_statuses = BroadwebdProfileSyncPublisher::new(*provider_daemon)
                .retain_profile_sync_objects(
                    preflight.profile.as_str(),
                    retained_object_ids.as_slice(),
                )
                .map_err(ProfileSyncCycleWithHealthError::Retention)?;
            retention.push(SettingsSyncCycleProviderRetentionRun {
                provider_index,
                object_statuses,
            });
        }
        let after_health = self.settings_sync_health(
            database,
            preflight.profile.as_str(),
            preflight.settings_root_id.as_str(),
            policy.minimum_online_retaining_providers,
        )?;
        policy.check_after_cycle(&after_health)?;

        Ok(SettingsSyncCycleWithSharedRootRetentionRun {
            before_health: preflight.before_health.clone(),
            cycle,
            shared_root_candidates,
            retained_object_ids,
            retention,
            after_health,
        })
    }

    pub fn settings_sync_cycle_preflight_with_active_key_policy(
        &self,
        database: &SlateProfileDatabase,
        profile: &str,
        settings_root_id: &str,
        signer: &ProfileSyncDeviceSigner,
        policy: &SettingsSyncCyclePolicy,
    ) -> Result<SettingsSyncCyclePreflight, ProfileSyncCycleWithHealthError> {
        let before_health = self.settings_sync_health(
            database,
            profile,
            settings_root_id,
            policy.minimum_online_retaining_providers,
        )?;
        policy.check_before_cycle(&before_health)?;
        let retention_provider_candidates =
            self.profile_sync_retention_provider_candidates(profile)?;
        let active_key_id = active_settings_sync_content_key_id(database, profile)
            .map_err(ProfileSyncCycleError::from)?;
        validate_settings_sync_cycle_credentials(database, profile, active_key_id.as_str(), signer)
            .map_err(ProfileSyncCycleError::from)?;
        let trusted_remote_device_count =
            trusted_remote_device_public_keys(database, profile, policy.max_trusted_devices)
                .map_err(ProfileSyncCycleError::from)?
                .len();

        Ok(SettingsSyncCyclePreflight {
            profile: profile.to_string(),
            settings_root_id: settings_root_id.to_string(),
            local_device_id: database.local_sync_device_id().to_string(),
            signer_device_id: signer.device_id().to_string(),
            active_key_id,
            trusted_remote_device_count,
            retention_provider_candidates,
            before_health,
        })
    }

    pub fn settings_sync_cycle_preflight_with_membership_log_and_active_key_policy(
        &self,
        database: &SlateProfileDatabase,
        profile: &str,
        settings_root_id: &str,
        membership_log_root_id: &str,
        signer: &ProfileSyncDeviceSigner,
        policy: &SettingsSyncCyclePolicy,
    ) -> Result<SettingsSyncCyclePreflightWithMembershipLog, ProfileSyncCycleWithHealthError> {
        let pulled_membership_log = BroadwebdProfileSyncObjectSource::new(self.daemon)
            .pull_and_apply_sync_account_membership_log_if_changed(
                database,
                profile,
                membership_log_root_id,
            )
            .map_err(ProfileSyncCycleError::from)?;
        let preflight = self.settings_sync_cycle_preflight_with_active_key_policy(
            database,
            profile,
            settings_root_id,
            signer,
            policy,
        )?;

        Ok(SettingsSyncCyclePreflightWithMembershipLog {
            pulled_membership_log,
            preflight,
        })
    }

    pub fn run_settings_sync_cycle(
        &self,
        database: &SlateProfileDatabase,
        profile: &str,
        settings_root_id: &str,
        content_key: &ProfileSyncContentKey,
        key_id: &str,
        signer: &ProfileSyncDeviceSigner,
        retention_policy: ProfileSyncRetentionPolicy,
        max_publish_steps: u32,
        max_trusted_devices: u32,
    ) -> Result<SettingsSyncCycleRun, ProfileSyncCycleError> {
        validate_settings_sync_cycle_credentials(database, profile, key_id, signer)?;
        trusted_remote_device_public_keys(database, profile, max_trusted_devices)?;
        let publisher = BroadwebdProfileSyncPublisher::new(self.daemon);
        let publish = publisher.run_pending_local_settings_sync(
            database,
            profile,
            settings_root_id,
            content_key,
            key_id,
            signer,
            retention_policy,
            max_publish_steps,
        )?;
        let source = BroadwebdProfileSyncObjectSource::new(self.daemon);
        let receive = source.pull_and_apply_trusted_settings_from_registered_devices(
            database,
            profile,
            content_key,
            key_id,
            max_trusted_devices,
        )?;

        Ok(SettingsSyncCycleRun {
            profile: profile.to_string(),
            settings_root_id: settings_root_id.to_string(),
            publish,
            receive,
        })
    }

    pub fn run_settings_sync_cycle_with_membership_log(
        &self,
        database: &SlateProfileDatabase,
        profile: &str,
        settings_root_id: &str,
        membership_log_root_id: &str,
        content_key: &ProfileSyncContentKey,
        key_id: &str,
        signer: &ProfileSyncDeviceSigner,
        retention_policy: ProfileSyncRetentionPolicy,
        max_publish_steps: u32,
        max_trusted_devices: u32,
    ) -> Result<SettingsSyncCycleWithMembershipLogRun, ProfileSyncCycleError> {
        let publisher = BroadwebdProfileSyncPublisher::new(self.daemon);
        let membership_log_publication = publisher.plan_local_sync_account_membership_log(
            database,
            profile,
            membership_log_root_id,
        )?;
        if membership_log_publication.requires_compaction() {
            return Err(ProfileSyncCycleError::from(
                ProfileSyncPublishError::MembershipLogTooLarge {
                    profile: membership_log_publication.profile,
                    max_records: membership_log_publication.max_records,
                    actual_records: membership_log_publication.record_count,
                },
            ));
        }

        let source = BroadwebdProfileSyncObjectSource::new(self.daemon);
        let pulled_membership_log = source
            .pull_and_apply_sync_account_membership_log_if_changed(
                database,
                profile,
                membership_log_root_id,
            )
            .map_err(ProfileSyncCycleError::from)?;
        let cycle = self.run_settings_sync_cycle(
            database,
            profile,
            settings_root_id,
            content_key,
            key_id,
            signer,
            retention_policy,
            max_publish_steps,
            max_trusted_devices,
        )?;
        let published_membership_log = publisher
            .publish_local_sync_account_membership_log(database, profile, membership_log_root_id)
            .map_err(ProfileSyncCycleError::from)?;

        Ok(SettingsSyncCycleWithMembershipLogRun {
            pulled_membership_log,
            cycle,
            published_membership_log,
        })
    }

    pub fn run_settings_sync_cycle_with_membership_log_and_retention_providers(
        &self,
        database: &SlateProfileDatabase,
        profile: &str,
        settings_root_id: &str,
        membership_log_root_id: &str,
        content_key: &ProfileSyncContentKey,
        key_id: &str,
        signer: &ProfileSyncDeviceSigner,
        retention_policy: ProfileSyncRetentionPolicy,
        max_publish_steps: u32,
        max_trusted_devices: u32,
        retention_provider_daemons: &[&BroadwebDaemon],
    ) -> Result<SettingsSyncCycleWithMembershipLogRetentionRun, ProfileSyncCycleWithHealthError>
    {
        let cycle = self
            .run_settings_sync_cycle_with_membership_log(
                database,
                profile,
                settings_root_id,
                membership_log_root_id,
                content_key,
                key_id,
                signer,
                retention_policy,
                max_publish_steps,
                max_trusted_devices,
            )
            .map_err(ProfileSyncCycleWithHealthError::Cycle)?;
        self.retain_membership_log_cycle_publications(profile, cycle, retention_provider_daemons)
    }

    fn run_settings_sync_cycle_with_membership_log_and_retention_providers_after_preflight(
        &self,
        database: &SlateProfileDatabase,
        content_key: &ProfileSyncContentKey,
        signer: &ProfileSyncDeviceSigner,
        policy: &SettingsSyncCyclePolicy,
        membership_log_root_id: &str,
        retention_provider_daemons: &[&BroadwebDaemon],
        preflight: &SettingsSyncCyclePreflightWithMembershipLog,
    ) -> Result<SettingsSyncCycleWithMembershipLogRetentionRun, ProfileSyncCycleWithHealthError>
    {
        let cycle = self
            .run_settings_sync_cycle(
                database,
                preflight.preflight.profile.as_str(),
                preflight.preflight.settings_root_id.as_str(),
                content_key,
                preflight.preflight.active_key_id.as_str(),
                signer,
                policy.retention_policy.clone(),
                policy.max_publish_steps,
                policy.max_trusted_devices,
            )
            .map_err(ProfileSyncCycleWithHealthError::Cycle)?;
        let published_membership_log = BroadwebdProfileSyncPublisher::new(self.daemon)
            .publish_local_sync_account_membership_log(
                database,
                preflight.preflight.profile.as_str(),
                membership_log_root_id,
            )
            .map_err(ProfileSyncCycleError::from)
            .map_err(ProfileSyncCycleWithHealthError::Cycle)?;
        let membership_cycle = SettingsSyncCycleWithMembershipLogRun {
            pulled_membership_log: preflight.pulled_membership_log.clone(),
            cycle,
            published_membership_log,
        };
        self.retain_membership_log_cycle_publications(
            preflight.preflight.profile.as_str(),
            membership_cycle,
            retention_provider_daemons,
        )
    }

    fn retain_membership_log_cycle_publications(
        &self,
        profile: &str,
        cycle: SettingsSyncCycleWithMembershipLogRun,
        retention_provider_daemons: &[&BroadwebDaemon],
    ) -> Result<SettingsSyncCycleWithMembershipLogRetentionRun, ProfileSyncCycleWithHealthError>
    {
        let retained_object_ids = cycle.published_object_ids();
        let mut retention = Vec::with_capacity(retention_provider_daemons.len());
        for (provider_index, provider_daemon) in retention_provider_daemons.iter().enumerate() {
            let object_statuses = BroadwebdProfileSyncPublisher::new(*provider_daemon)
                .retain_profile_sync_objects(profile, retained_object_ids.as_slice())
                .map_err(ProfileSyncCycleWithHealthError::Retention)?;
            retention.push(SettingsSyncCycleProviderRetentionRun {
                provider_index,
                object_statuses,
            });
        }

        Ok(SettingsSyncCycleWithMembershipLogRetentionRun {
            cycle,
            retained_object_ids,
            retention,
        })
    }
}

impl<'a> BroadwebdSettingsSyncScheduler<'a> {
    pub fn new(daemon: &'a BroadwebDaemon) -> Self {
        Self { daemon }
    }

    pub fn plan_once_selecting_retention_providers(
        &self,
        database: &SlateProfileDatabase,
        config: &SettingsSyncSchedulerConfig,
        signer: &ProfileSyncDeviceSigner,
        retention_provider_handles: &[SettingsSyncRetentionProviderHandle<'_>],
    ) -> Result<SettingsSyncScheduledCyclePlan, ProfileSyncCycleWithHealthError> {
        let runner = BroadwebdSettingsSyncRunner::new(self.daemon);
        let preflight = runner.settings_sync_cycle_preflight_with_active_key_policy(
            database,
            config.profile.as_str(),
            config.settings_root_id.as_str(),
            signer,
            &config.policy,
        )?;
        Ok(
            select_settings_sync_retention_provider_handles(preflight, retention_provider_handles)
                .plan,
        )
    }

    pub fn plan_once_with_membership_log_selecting_retention_providers(
        &self,
        database: &SlateProfileDatabase,
        config: &SettingsSyncSchedulerConfig,
        membership_log_root_id: &str,
        signer: &ProfileSyncDeviceSigner,
        retention_provider_handles: &[SettingsSyncRetentionProviderHandle<'_>],
    ) -> Result<SettingsSyncScheduledMembershipCyclePlan, ProfileSyncCycleWithHealthError> {
        let membership_log_publication = BroadwebdProfileSyncPublisher::new(self.daemon)
            .plan_local_sync_account_membership_log(
                database,
                config.profile.as_str(),
                membership_log_root_id,
            )
            .map_err(ProfileSyncCycleError::from)?;
        let cycle = self.plan_once_selecting_retention_providers(
            database,
            config,
            signer,
            retention_provider_handles,
        )?;

        Ok(SettingsSyncScheduledMembershipCyclePlan {
            membership_log_publication,
            cycle,
        })
    }

    pub fn run_once_selecting_retention_providers(
        &self,
        database: &SlateProfileDatabase,
        config: &SettingsSyncSchedulerConfig,
        secrets: SettingsSyncRuntimeSecrets<'_>,
        retention_provider_handles: &[SettingsSyncRetentionProviderHandle<'_>],
    ) -> Result<SettingsSyncScheduledCycleRun, ProfileSyncCycleWithHealthError> {
        let runner = BroadwebdSettingsSyncRunner::new(self.daemon);
        let preflight = runner.settings_sync_cycle_preflight_with_active_key_policy(
            database,
            config.profile.as_str(),
            config.settings_root_id.as_str(),
            secrets.signer,
            &config.policy,
        )?;
        let selection =
            select_settings_sync_retention_provider_handles(preflight, retention_provider_handles);
        let plan = selection.plan;
        config.policy.check_selected_retention_provider_freshness(
            plan.stale_retention_provider_count(),
            plan.offline_retention_provider_count(),
            &plan.preflight.before_health,
        )?;
        config.policy.check_selected_retention_provider_count(
            plan.selected_retention_provider_count(),
            &plan.preflight.before_health,
        )?;
        let cycle = runner
            .run_settings_sync_cycle_with_active_key_policy_shared_root_candidates_and_retention_providers_after_preflight(
                database,
                secrets.content_key,
                secrets.signer,
                &config.policy,
                selection.daemons.as_slice(),
                &plan.preflight,
            )?;

        Ok(SettingsSyncScheduledCycleRun {
            preflight: plan.preflight,
            selected_retention_provider_ids: plan.selected_retention_provider_ids,
            stale_retention_provider_ids: plan.stale_retention_provider_ids,
            offline_retention_provider_ids: plan.offline_retention_provider_ids,
            undiscovered_retention_provider_ids: plan.undiscovered_retention_provider_ids,
            duplicate_retention_provider_ids: plan.duplicate_retention_provider_ids,
            cycle,
        })
    }

    pub fn run_once_with_membership_log_selecting_retention_providers(
        &self,
        database: &SlateProfileDatabase,
        config: &SettingsSyncSchedulerConfig,
        membership_log_root_id: &str,
        secrets: SettingsSyncRuntimeSecrets<'_>,
        retention_provider_handles: &[SettingsSyncRetentionProviderHandle<'_>],
    ) -> Result<SettingsSyncScheduledMembershipCycleRun, ProfileSyncCycleWithHealthError> {
        let membership_log_publication = BroadwebdProfileSyncPublisher::new(self.daemon)
            .plan_local_sync_account_membership_log(
                database,
                config.profile.as_str(),
                membership_log_root_id,
            )
            .map_err(ProfileSyncCycleError::from)?;
        if membership_log_publication.requires_compaction() {
            return Err(ProfileSyncCycleWithHealthError::Cycle(
                ProfileSyncCycleError::from(ProfileSyncPublishError::MembershipLogTooLarge {
                    profile: membership_log_publication.profile,
                    max_records: membership_log_publication.max_records,
                    actual_records: membership_log_publication.record_count,
                }),
            ));
        }

        let runner = BroadwebdSettingsSyncRunner::new(self.daemon);
        let preflight = runner
            .settings_sync_cycle_preflight_with_membership_log_and_active_key_policy(
                database,
                config.profile.as_str(),
                config.settings_root_id.as_str(),
                membership_log_root_id,
                secrets.signer,
                &config.policy,
            )?;
        let selection = select_settings_sync_retention_provider_handles(
            preflight.preflight.clone(),
            retention_provider_handles,
        );
        let plan = selection.plan;
        config.policy.check_selected_retention_provider_freshness(
            plan.stale_retention_provider_count(),
            plan.offline_retention_provider_count(),
            &preflight.preflight.before_health,
        )?;
        config.policy.check_selected_retention_provider_count(
            plan.selected_retention_provider_count(),
            &preflight.preflight.before_health,
        )?;
        let cycle = runner
            .run_settings_sync_cycle_with_membership_log_and_retention_providers_after_preflight(
                database,
                secrets.content_key,
                secrets.signer,
                &config.policy,
                membership_log_root_id,
                selection.daemons.as_slice(),
                &preflight,
            )?;

        Ok(SettingsSyncScheduledMembershipCycleRun {
            preflight,
            selected_retention_provider_ids: plan.selected_retention_provider_ids,
            stale_retention_provider_ids: plan.stale_retention_provider_ids,
            offline_retention_provider_ids: plan.offline_retention_provider_ids,
            undiscovered_retention_provider_ids: plan.undiscovered_retention_provider_ids,
            duplicate_retention_provider_ids: plan.duplicate_retention_provider_ids,
            cycle,
        })
    }

    pub fn run_once(
        &self,
        database: &SlateProfileDatabase,
        config: &SettingsSyncSchedulerConfig,
        secrets: SettingsSyncRuntimeSecrets<'_>,
        retention_provider_daemons: &[&BroadwebDaemon],
    ) -> Result<SettingsSyncCycleWithSharedRootRetentionRun, ProfileSyncCycleWithHealthError> {
        BroadwebdSettingsSyncRunner::new(self.daemon)
            .run_settings_sync_cycle_with_active_key_policy_shared_root_candidates_and_retention_providers(
                database,
                config.profile.as_str(),
                config.settings_root_id.as_str(),
                secrets.content_key,
                secrets.signer,
                &config.policy,
                retention_provider_daemons,
            )
    }
}

impl<'a> BroadwebdProfileSyncPublisher<'a> {
    pub fn new(daemon: &'a BroadwebDaemon) -> Self {
        Self { daemon }
    }

    pub fn put_encrypted_object(
        &self,
        profile: &str,
        bytes: impl Into<Vec<u8>>,
    ) -> Result<String, BroadwebdError> {
        let response =
            self.daemon
                .profile_sync(BroadwebdProfileSyncRequest::PutEncryptedObject(
                    BroadwebdProfileSyncPutObjectRequest::new(profile, bytes),
                ))?;
        let BroadwebdProfileSyncResponse::PutEncryptedObject { object_id } = response else {
            return Err(BroadwebdError::UnsupportedRequest(
                "profile-sync put object returned a non-put response".to_string(),
            ));
        };
        Ok(object_id)
    }

    pub fn retain_object(&self, profile: &str, object_id: &str) -> Result<bool, BroadwebdError> {
        let response = self
            .daemon
            .profile_sync(BroadwebdProfileSyncRequest::RetainObject(
                BroadwebdProfileSyncObjectRequest::new(profile, object_id),
            ))?;
        let BroadwebdProfileSyncResponse::RetainObject { retained, .. } = response else {
            return Err(BroadwebdError::UnsupportedRequest(
                "profile-sync retain object returned a non-retain response".to_string(),
            ));
        };
        Ok(retained)
    }

    pub fn release_object(&self, profile: &str, object_id: &str) -> Result<bool, BroadwebdError> {
        let response = self
            .daemon
            .profile_sync(BroadwebdProfileSyncRequest::ReleaseObject(
                BroadwebdProfileSyncObjectRequest::new(profile, object_id),
            ))?;
        let BroadwebdProfileSyncResponse::ReleaseObject { retained, .. } = response else {
            return Err(BroadwebdError::UnsupportedRequest(
                "profile-sync release object returned a non-release response".to_string(),
            ));
        };
        Ok(retained)
    }

    pub fn verify_retained_object(
        &self,
        profile: &str,
        object_id: &str,
    ) -> Result<BroadwebdProfileSyncRetentionStatus, BroadwebdError> {
        let response =
            self.daemon
                .profile_sync(BroadwebdProfileSyncRequest::VerifyRetainedObject(
                    BroadwebdProfileSyncObjectRequest::new(profile, object_id),
                ))?;
        let BroadwebdProfileSyncResponse::RetainedObjectStatus {
            object_id,
            retained,
            available,
        } = response
        else {
            return Err(BroadwebdError::UnsupportedRequest(
                "profile-sync verify retained object returned a non-status response".to_string(),
            ));
        };
        Ok(BroadwebdProfileSyncRetentionStatus {
            object_id,
            retained,
            available,
        })
    }

    pub fn retain_profile_sync_objects(
        &self,
        profile: &str,
        object_ids: &[String],
    ) -> Result<Vec<BroadwebdProfileSyncRetentionStatus>, BroadwebdError> {
        let mut statuses = Vec::with_capacity(object_ids.len());
        for object_id in object_ids {
            self.retain_object(profile, object_id)?;
            statuses.push(self.verify_retained_object(profile, object_id)?);
        }
        Ok(statuses)
    }

    pub fn retain_published_objects(
        &self,
        profile: &str,
        object_ids: &[String],
    ) -> Result<Vec<BroadwebdProfileSyncRetentionStatus>, BroadwebdError> {
        self.retain_profile_sync_objects(profile, object_ids)
    }

    pub fn retain_settings_sync_cycle_objects(
        &self,
        cycle: &SettingsSyncCycleRun,
    ) -> Result<Vec<BroadwebdProfileSyncRetentionStatus>, BroadwebdError> {
        let object_ids = cycle.published_object_ids();
        self.retain_profile_sync_objects(cycle.profile.as_str(), object_ids.as_slice())
    }

    pub fn retain_profile_sync_membership_log_objects(
        &self,
        publication: &PublishedProfileSyncMembershipLog,
    ) -> Result<Vec<BroadwebdProfileSyncRetentionStatus>, BroadwebdError> {
        let object_ids = publication.published_object_ids();
        self.retain_profile_sync_objects(publication.profile.as_str(), object_ids.as_slice())
    }

    pub fn publish_root(
        &self,
        profile: &str,
        root_id: &str,
        object_id: &str,
    ) -> Result<String, BroadwebdError> {
        let response = self
            .daemon
            .profile_sync(BroadwebdProfileSyncRequest::PublishRoot(
                BroadwebdProfileSyncRootUpdate::new(profile, root_id, object_id),
            ))?;
        let BroadwebdProfileSyncResponse::Root {
            object_id: Some(published_object_id),
            ..
        } = response
        else {
            return Err(BroadwebdError::UnsupportedRequest(
                "profile-sync publish root returned a non-root response".to_string(),
            ));
        };
        Ok(published_object_id)
    }

    pub fn put_retained_object(
        &self,
        profile: &str,
        bytes: impl Into<Vec<u8>>,
    ) -> Result<String, BroadwebdError> {
        let object_id = self.put_encrypted_object(profile, bytes)?;
        self.retain_object(profile, object_id.as_str())?;
        Ok(object_id)
    }

    pub fn put_retained_root(
        &self,
        profile: &str,
        root_id: &str,
        bytes: impl Into<Vec<u8>>,
    ) -> Result<String, BroadwebdError> {
        let object_id = self.put_retained_object(profile, bytes)?;
        self.publish_root(profile, root_id, object_id.as_str())?;
        Ok(object_id)
    }

    pub fn publish_signed_sync_account_membership_record(
        &self,
        profile: &str,
        root_id: &str,
        signed_record: impl Into<Vec<u8>>,
    ) -> Result<PublishedProfileSyncMembershipRecord, ProfileSyncPublishError> {
        let signed_record = signed_record.into();
        let object_id = self.put_retained_root(profile, root_id, signed_record.clone())?;
        Ok(PublishedProfileSyncMembershipRecord {
            profile: profile.to_string(),
            root_id: root_id.to_string(),
            object_id,
            signed_record,
        })
    }

    pub fn plan_local_sync_account_membership_log(
        &self,
        database: &SlateProfileDatabase,
        profile: &str,
        root_id: &str,
    ) -> Result<ProfileSyncMembershipLogPublicationPlan, ProfileSyncPublishError> {
        let record_count = database.sync_account_membership_record_count(profile)?;
        Ok(ProfileSyncMembershipLogPublicationPlan::for_record_count(
            profile,
            root_id,
            record_count,
        ))
    }

    pub fn publish_local_sync_account_membership_log(
        &self,
        database: &SlateProfileDatabase,
        profile: &str,
        root_id: &str,
    ) -> Result<Option<PublishedProfileSyncMembershipLog>, ProfileSyncPublishError> {
        let plan = self.plan_local_sync_account_membership_log(database, profile, root_id)?;
        if plan.is_empty() {
            return Ok(None);
        }
        if plan.requires_compaction() {
            return Err(ProfileSyncPublishError::MembershipLogTooLarge {
                profile: plan.profile,
                max_records: plan.max_records,
                actual_records: plan.record_count,
            });
        }

        let records = database.sync_account_membership_records(profile)?;
        let loaded_plan = ProfileSyncMembershipLogPublicationPlan::for_record_count(
            profile,
            root_id,
            records.len(),
        );
        if loaded_plan.is_empty() {
            return Ok(None);
        }
        if loaded_plan.requires_compaction() {
            return Err(ProfileSyncPublishError::MembershipLogTooLarge {
                profile: loaded_plan.profile,
                max_records: loaded_plan.max_records,
                actual_records: loaded_plan.record_count,
            });
        }

        let mut entries = Vec::with_capacity(records.len());
        for record in records {
            let record_root_id = sync_membership_record_root_id(record.record_id.as_str());
            let publication = self.publish_signed_sync_account_membership_record(
                profile,
                record_root_id.as_str(),
                record.signed_record,
            )?;
            entries.push(ProfileSyncMembershipLogEntry {
                record_id: record.record_id,
                root_id: record_root_id,
                object_id: publication.object_id,
                membership_epoch: record.membership_epoch,
                record_kind: record.record_kind,
                device_id: record.device_id,
                signer_device_id: record.signer_device_id,
            });
        }

        let log = ProfileSyncMembershipLog {
            profile: profile.to_string(),
            schema_version: PROFILE_SYNC_MEMBERSHIP_LOG_SCHEMA_VERSION,
            records: entries,
        };
        let log_bytes = serde_json::to_vec(&log).map_err(SyncObjectError::Encode)?;
        let object_id = self.put_retained_root(profile, root_id, log_bytes)?;
        Ok(Some(PublishedProfileSyncMembershipLog {
            profile: profile.to_string(),
            root_id: root_id.to_string(),
            object_id,
            log,
        }))
    }

    pub fn put_retained_root_with_dependencies(
        &self,
        profile: &str,
        root_id: &str,
        dependency_objects: impl IntoIterator<Item = Vec<u8>>,
        root_bytes: impl Into<Vec<u8>>,
    ) -> Result<BroadwebdProfileSyncRootPublication, BroadwebdError> {
        let dependency_object_ids = dependency_objects
            .into_iter()
            .map(|bytes| self.put_retained_object(profile, bytes))
            .collect::<Result<Vec<_>, _>>()?;
        let root_object_id = self.put_retained_root(profile, root_id, root_bytes)?;
        Ok(BroadwebdProfileSyncRootPublication {
            profile: profile.to_string(),
            root_id: root_id.to_string(),
            root_object_id,
            dependency_object_ids,
        })
    }

    pub fn publish_signed_settings_tail_changes(
        &self,
        profile: &str,
        root_id: &str,
        changes: &[SyncChangeRecord],
        content_key: &ProfileSyncContentKey,
        key_id: &str,
        signer: &ProfileSyncDeviceSigner,
        retention_policy: ProfileSyncRetentionPolicy,
    ) -> Result<PublishedSettingsTailManifest, ProfileSyncPublishError> {
        validate_tail_changes_for_publish(profile, changes)?;
        let tail_publications = self.publish_settings_tail_change_publications(
            profile,
            changes,
            content_key,
            key_id,
            signer,
        )?;

        let manifest = settings_sync_manifest_for_tail_changes(
            profile,
            root_id,
            tail_publications.as_slice(),
            retention_policy,
        )?;
        let manifest_bytes = sign_encrypted_json_object(
            manifest.profile.as_str(),
            SYNC_DOMAIN_SETTINGS,
            PROFILE_SYNC_MANIFEST_OBJECT_KIND,
            key_id,
            &serde_json::to_vec(&manifest).map_err(SyncObjectError::Encode)?,
            content_key,
            signer,
        )?;
        let manifest_object_id = self.put_retained_root(profile, root_id, manifest_bytes)?;
        let tail_change_object_ids = manifest.tail_change_object_ids.clone();

        Ok(PublishedSettingsTailManifest {
            manifest_object_id,
            manifest,
            tail_change_object_ids,
        })
    }

    pub fn publish_signed_settings_snapshot_manifest(
        &self,
        profile: &str,
        root_id: &str,
        snapshot: &ProfileSyncSettingsSnapshot,
        covered_changes: &[SyncChangeRecord],
        tail_changes: &[SyncChangeRecord],
        content_key: &ProfileSyncContentKey,
        key_id: &str,
        signer: &ProfileSyncDeviceSigner,
        retention_policy: ProfileSyncRetentionPolicy,
    ) -> Result<PublishedSettingsSnapshotManifest, ProfileSyncPublishError> {
        validate_snapshot_for_publish(profile, snapshot)?;
        validate_snapshot_covered_changes_for_publish(profile, snapshot, covered_changes)?;
        validate_setting_changes_for_publish(profile, tail_changes, "tail change")?;

        let snapshot_bytes = sign_encrypted_json_object(
            snapshot.profile.as_str(),
            SYNC_DOMAIN_SETTINGS,
            PROFILE_SYNC_SETTINGS_SNAPSHOT_OBJECT_KIND,
            key_id,
            &serde_json::to_vec(snapshot).map_err(SyncObjectError::Encode)?,
            content_key,
            signer,
        )?;
        let snapshot_object_id = self.put_retained_object(profile, snapshot_bytes)?;
        let tail_publications = self.publish_settings_tail_change_publications(
            profile,
            tail_changes,
            content_key,
            key_id,
            signer,
        )?;
        let snapshot_publication = ProfileSyncSettingsSnapshotPublication {
            object_id: snapshot_object_id.clone(),
            snapshot: snapshot.clone(),
            covered_changes: covered_changes.to_vec(),
        };
        let manifest = settings_sync_manifest_for_snapshot_and_tail_changes(
            profile,
            root_id,
            &snapshot_publication,
            tail_publications.as_slice(),
            retention_policy,
        )?;
        let manifest_bytes = sign_encrypted_json_object(
            manifest.profile.as_str(),
            SYNC_DOMAIN_SETTINGS,
            PROFILE_SYNC_MANIFEST_OBJECT_KIND,
            key_id,
            &serde_json::to_vec(&manifest).map_err(SyncObjectError::Encode)?,
            content_key,
            signer,
        )?;
        let manifest_object_id = self.put_retained_root(profile, root_id, manifest_bytes)?;
        let tail_change_object_ids = manifest.tail_change_object_ids.clone();

        Ok(PublishedSettingsSnapshotManifest {
            manifest_object_id,
            manifest,
            snapshot_object_id,
            tail_change_object_ids,
        })
    }

    pub fn publish_signed_existing_settings_snapshot_manifest(
        &self,
        profile: &str,
        root_id: &str,
        snapshot_object_id: &str,
        snapshot: &ProfileSyncSettingsSnapshot,
        covered_changes: &[SyncChangeRecord],
        tail_changes: &[SyncChangeRecord],
        content_key: &ProfileSyncContentKey,
        key_id: &str,
        signer: &ProfileSyncDeviceSigner,
        retention_policy: ProfileSyncRetentionPolicy,
    ) -> Result<PublishedSettingsSnapshotManifest, ProfileSyncPublishError> {
        if snapshot_object_id.is_empty() {
            return Err(StorageError::InvalidProfileSyncManifest(
                "settings snapshot backend object id is empty".to_string(),
            )
            .into());
        }
        validate_snapshot_for_publish(profile, snapshot)?;
        validate_snapshot_covered_changes_for_publish(profile, snapshot, covered_changes)?;
        validate_setting_changes_for_publish(profile, tail_changes, "tail change")?;

        self.retain_object(profile, snapshot_object_id)?;
        let tail_publications = self.publish_settings_tail_change_publications(
            profile,
            tail_changes,
            content_key,
            key_id,
            signer,
        )?;
        let snapshot_publication = ProfileSyncSettingsSnapshotPublication {
            object_id: snapshot_object_id.to_string(),
            snapshot: snapshot.clone(),
            covered_changes: covered_changes.to_vec(),
        };
        let manifest = settings_sync_manifest_for_snapshot_and_tail_changes(
            profile,
            root_id,
            &snapshot_publication,
            tail_publications.as_slice(),
            retention_policy,
        )?;
        let manifest_bytes = sign_encrypted_json_object(
            manifest.profile.as_str(),
            SYNC_DOMAIN_SETTINGS,
            PROFILE_SYNC_MANIFEST_OBJECT_KIND,
            key_id,
            &serde_json::to_vec(&manifest).map_err(SyncObjectError::Encode)?,
            content_key,
            signer,
        )?;
        let manifest_object_id = self.put_retained_root(profile, root_id, manifest_bytes)?;
        let tail_change_object_ids = manifest.tail_change_object_ids.clone();

        Ok(PublishedSettingsSnapshotManifest {
            manifest_object_id,
            manifest,
            snapshot_object_id: snapshot_object_id.to_string(),
            tail_change_object_ids,
        })
    }

    pub fn compact_and_publish_settings(
        &self,
        database: &SlateProfileDatabase,
        profile: &str,
        root_id: &str,
        content_key: &ProfileSyncContentKey,
        key_id: &str,
        signer: &ProfileSyncDeviceSigner,
        retention_policy: ProfileSyncRetentionPolicy,
        now: i64,
    ) -> Result<Option<PublishedSettingsCompaction>, ProfileSyncPublishError> {
        let enabled_domain_ids = enabled_settings_sync_domain_ids(database, profile)?;
        let enabled_domains = enabled_domain_ids.iter().cloned().collect::<Vec<_>>();
        let Some(target) = database.settings_sync_compaction_target_for_domains(
            profile,
            &retention_policy,
            now,
            enabled_domains.as_slice(),
        )?
        else {
            return Ok(None);
        };
        let events = database
            .sync_setting_text_events_after(
                profile,
                target.previous_snapshot_covers_revision,
                u32::MAX,
            )?
            .into_iter()
            .filter(|event| enabled_domain_ids.contains(&event.change.domain))
            .collect::<Vec<_>>();
        let covered_changes = events
            .iter()
            .take(target.covered_change_count)
            .map(|event| event.change.clone())
            .collect::<Vec<_>>();
        let tail_changes = events
            .iter()
            .skip(target.covered_change_count)
            .map(|event| event.change.clone())
            .collect::<Vec<_>>();
        let included_domains = covered_changes
            .iter()
            .map(|change| change.domain.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let snapshot = database.settings_sync_snapshot_payload(
            profile,
            target.covers_revision,
            &included_domains,
        )?;
        let publication = self.publish_signed_settings_snapshot_manifest(
            profile,
            root_id,
            &snapshot,
            covered_changes.as_slice(),
            tail_changes.as_slice(),
            content_key,
            key_id,
            signer,
            retention_policy,
        )?;
        let snapshot_record = database.record_sync_snapshot(&SyncSnapshotRegistration {
            profile: profile.to_string(),
            snapshot_id: settings_sync_snapshot_id(target.covers_revision),
            backend_object_id: Some(publication.snapshot_object_id.clone()),
            covers_revision: target.covers_revision,
            included_domains: snapshot.included_domains,
        })?;

        Ok(Some(PublishedSettingsCompaction {
            target,
            publication,
            snapshot_record,
        }))
    }

    pub fn publish_signed_profile_sync_device_head(
        &self,
        profile: &str,
        root_id: &str,
        device_head: &ProfileSyncDeviceHead,
        content_key: &ProfileSyncContentKey,
        key_id: &str,
        signer: &ProfileSyncDeviceSigner,
    ) -> Result<PublishedProfileSyncDeviceHead, ProfileSyncPublishError> {
        validate_device_head_for_publish(profile, root_id, device_head, signer)?;
        let object_bytes = sign_encrypted_json_object(
            device_head.profile.as_str(),
            SYNC_DOMAIN_SETTINGS,
            PROFILE_SYNC_DEVICE_HEAD_OBJECT_KIND,
            key_id,
            &serde_json::to_vec(device_head).map_err(SyncObjectError::Encode)?,
            content_key,
            signer,
        )?;
        let object_id = self.put_retained_root(profile, root_id, object_bytes)?;

        Ok(PublishedProfileSyncDeviceHead {
            root_id: root_id.to_string(),
            object_id,
            device_head: device_head.clone(),
        })
    }

    pub fn publish_full_local_settings_snapshot_head(
        &self,
        database: &SlateProfileDatabase,
        profile: &str,
        settings_root_id: &str,
        content_key: &ProfileSyncContentKey,
        key_id: &str,
        signer: &ProfileSyncDeviceSigner,
        retention_policy: ProfileSyncRetentionPolicy,
    ) -> Result<Option<PublishedLocalSettingsSnapshotHead>, ProfileSyncPublishError> {
        let events = enabled_settings_sync_text_events_after(database, profile, 0, u32::MAX)?;
        if events.is_empty() {
            return Ok(None);
        }
        let covered_changes = events
            .iter()
            .map(|event| event.change.clone())
            .collect::<Vec<_>>();
        let Some(head_change) =
            latest_local_device_change_for_head(database.local_sync_device_id(), &covered_changes)
        else {
            return Ok(None);
        };
        let covers_revision = events
            .last()
            .expect("events emptiness was checked before reading latest revision")
            .revision
            .revision;
        let included_domains = covered_changes
            .iter()
            .map(|change| change.domain.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let snapshot =
            database.settings_sync_snapshot_payload(profile, covers_revision, &included_domains)?;
        let publication = self.publish_signed_settings_snapshot_manifest(
            profile,
            settings_root_id,
            &snapshot,
            covered_changes.as_slice(),
            &[],
            content_key,
            key_id,
            signer,
            retention_policy,
        )?;
        let snapshot_record = database.record_sync_snapshot(&SyncSnapshotRegistration {
            profile: profile.to_string(),
            snapshot_id: settings_sync_snapshot_id(covers_revision),
            backend_object_id: Some(publication.snapshot_object_id.clone()),
            covers_revision,
            included_domains: snapshot.included_domains,
        })?;
        let settings_root = database.set_profile_sync_root(
            profile,
            settings_root_id,
            publication.manifest_object_id.as_str(),
        )?;

        let device_head_root_id = settings_device_head_root_id(database.local_sync_device_id());
        let device_head = ProfileSyncDeviceHead {
            profile: profile.to_string(),
            device_id: database.local_sync_device_id().to_string(),
            root_id: device_head_root_id.clone(),
            schema_version: PROFILE_SYNC_DEVICE_HEAD_SCHEMA_VERSION,
            membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            latest_manifest_object_id: publication.manifest_object_id.clone(),
            latest_change_object_id: None,
            device_sequence: head_change.device_sequence,
            logical_clock: head_change.logical_clock,
            created_at: head_change.created_at,
        };
        let device_head = self.publish_signed_profile_sync_device_head(
            profile,
            device_head_root_id.as_str(),
            &device_head,
            content_key,
            key_id,
            signer,
        )?;
        let device_head_root = database.set_profile_sync_root(
            profile,
            device_head.root_id.as_str(),
            device_head.object_id.as_str(),
        )?;

        Ok(Some(PublishedLocalSettingsSnapshotHead {
            publication,
            device_head,
            snapshot_record,
            settings_root,
            device_head_root,
        }))
    }

    pub fn publish_local_settings_tail_head(
        &self,
        database: &SlateProfileDatabase,
        profile: &str,
        settings_root_id: &str,
        content_key: &ProfileSyncContentKey,
        key_id: &str,
        signer: &ProfileSyncDeviceSigner,
        retention_policy: ProfileSyncRetentionPolicy,
    ) -> Result<Option<PublishedLocalSettingsTailHead>, ProfileSyncPublishError> {
        let Some(snapshot_record) = database.latest_sync_snapshot(profile)? else {
            return Ok(None);
        };
        let Some(snapshot_object_id) = snapshot_record.backend_object_id.clone() else {
            return Err(StorageError::InvalidProfileSyncManifest(format!(
                "latest settings snapshot {} has no backend object id",
                snapshot_record.snapshot_id
            ))
            .into());
        };
        let tail_events = enabled_settings_sync_text_events_after(
            database,
            profile,
            snapshot_record.covers_revision,
            u32::MAX,
        )?;
        let tail_changes = tail_events
            .iter()
            .filter(|event| event.change.device_id == database.local_sync_device_id())
            .map(|event| event.change.clone())
            .collect::<Vec<_>>();
        let Some(head_change) =
            latest_local_device_change_for_head(database.local_sync_device_id(), &tail_changes)
        else {
            return Ok(None);
        };
        let all_events = enabled_settings_sync_text_events_after(database, profile, 0, u32::MAX)?;
        let covered_changes = all_events
            .iter()
            .take_while(|event| event.revision.revision <= snapshot_record.covers_revision)
            .map(|event| event.change.clone())
            .collect::<Vec<_>>();
        let snapshot = database.settings_sync_snapshot_payload(
            profile,
            snapshot_record.covers_revision,
            snapshot_record.included_domains.as_slice(),
        )?;
        let publication = self.publish_signed_existing_settings_snapshot_manifest(
            profile,
            settings_root_id,
            snapshot_object_id.as_str(),
            &snapshot,
            covered_changes.as_slice(),
            tail_changes.as_slice(),
            content_key,
            key_id,
            signer,
            retention_policy,
        )?;
        let settings_root = database.set_profile_sync_root(
            profile,
            settings_root_id,
            publication.manifest_object_id.as_str(),
        )?;
        let device_head_frontier = publication
            .manifest
            .device_frontiers
            .iter()
            .find(|frontier| frontier.device_id == database.local_sync_device_id())
            .ok_or_else(|| {
                StorageError::InvalidProfileSyncManifest(format!(
                    "settings manifest has no frontier for local device {}",
                    database.local_sync_device_id()
                ))
            })?;

        let device_head_root_id = settings_device_head_root_id(database.local_sync_device_id());
        let device_head = ProfileSyncDeviceHead {
            profile: profile.to_string(),
            device_id: database.local_sync_device_id().to_string(),
            root_id: device_head_root_id.clone(),
            schema_version: PROFILE_SYNC_DEVICE_HEAD_SCHEMA_VERSION,
            membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            latest_manifest_object_id: publication.manifest_object_id.clone(),
            latest_change_object_id: device_head_frontier.latest_change_object_id.clone(),
            device_sequence: device_head_frontier.latest_sequence,
            logical_clock: head_change.logical_clock,
            created_at: head_change.created_at,
        };
        let device_head = self.publish_signed_profile_sync_device_head(
            profile,
            device_head_root_id.as_str(),
            &device_head,
            content_key,
            key_id,
            signer,
        )?;
        let device_head_root = database.set_profile_sync_root(
            profile,
            device_head.root_id.as_str(),
            device_head.object_id.as_str(),
        )?;

        Ok(Some(PublishedLocalSettingsTailHead {
            publication,
            device_head,
            snapshot_record,
            settings_root,
            device_head_root,
        }))
    }

    pub fn publish_pending_local_settings_head(
        &self,
        database: &SlateProfileDatabase,
        profile: &str,
        settings_root_id: &str,
        content_key: &ProfileSyncContentKey,
        key_id: &str,
        signer: &ProfileSyncDeviceSigner,
        retention_policy: ProfileSyncRetentionPolicy,
    ) -> Result<LocalSettingsHeadPublishStatus, ProfileSyncPublishError> {
        let latest_snapshot = database.latest_sync_snapshot(profile)?;
        if let Some(snapshot_record) = latest_snapshot.as_ref() {
            if snapshot_record.backend_object_id.is_some() {
                let local_tail_events = enabled_settings_sync_text_events_after(
                    database,
                    profile,
                    snapshot_record.covers_revision,
                    u32::MAX,
                )?;
                let local_tail_changes = local_tail_events
                    .iter()
                    .filter(|event| event.change.device_id == database.local_sync_device_id())
                    .map(|event| event.change.clone())
                    .collect::<Vec<_>>();
                let Some(latest_local_tail_change) = latest_local_device_change_for_head(
                    database.local_sync_device_id(),
                    local_tail_changes.as_slice(),
                ) else {
                    return Ok(LocalSettingsHeadPublishStatus::UpToDate {
                        snapshot_record: snapshot_record.clone(),
                    });
                };
                if let Some(device_head) =
                    self.local_settings_device_head(database, profile, content_key, key_id, signer)?
                    && device_head.device_sequence >= latest_local_tail_change.device_sequence
                {
                    return Ok(LocalSettingsHeadPublishStatus::UpToDate {
                        snapshot_record: snapshot_record.clone(),
                    });
                }

                if let Some(publication) = self.publish_local_settings_tail_head(
                    database,
                    profile,
                    settings_root_id,
                    content_key,
                    key_id,
                    signer,
                    retention_policy,
                )? {
                    return Ok(LocalSettingsHeadPublishStatus::PublishedIncrementalTail(
                        publication,
                    ));
                }

                return Ok(LocalSettingsHeadPublishStatus::UpToDate {
                    snapshot_record: snapshot_record.clone(),
                });
            }
        }

        if let Some(publication) = self.publish_full_local_settings_snapshot_head(
            database,
            profile,
            settings_root_id,
            content_key,
            key_id,
            signer,
            retention_policy,
        )? {
            return Ok(LocalSettingsHeadPublishStatus::PublishedFullSnapshot(
                publication,
            ));
        }

        Ok(LocalSettingsHeadPublishStatus::NoLocalSettingsChanges)
    }

    fn local_settings_device_head(
        &self,
        database: &SlateProfileDatabase,
        profile: &str,
        content_key: &ProfileSyncContentKey,
        key_id: &str,
        signer: &ProfileSyncDeviceSigner,
    ) -> Result<Option<ProfileSyncDeviceHead>, ProfileSyncPublishError> {
        let root_id = settings_device_head_root_id(database.local_sync_device_id());
        let Some(root) = database.profile_sync_root(profile, root_id.as_str())? else {
            return Ok(None);
        };
        let source = BroadwebdProfileSyncObjectSource::new(self.daemon);
        let object = source.get_profile_sync_object(profile, root.object_id.as_str())?;
        if object.object_id != root.object_id {
            return Err(StorageError::InvalidProfileSyncManifest(format!(
                "stored local device head root {} resolved to mismatched object {}",
                root.object_id, object.object_id
            ))
            .into());
        }
        let public_key = signer.public_key()?;
        let device_head = open_signed_profile_sync_device_head(
            object.bytes.as_slice(),
            content_key,
            &public_key,
            profile,
            key_id,
        )?;
        validate_device_head_for_publish(profile, root_id.as_str(), &device_head, signer)?;
        Ok(Some(device_head))
    }

    pub fn run_pending_local_settings_sync(
        &self,
        database: &SlateProfileDatabase,
        profile: &str,
        settings_root_id: &str,
        content_key: &ProfileSyncContentKey,
        key_id: &str,
        signer: &ProfileSyncDeviceSigner,
        retention_policy: ProfileSyncRetentionPolicy,
        max_steps: u32,
    ) -> Result<LocalSettingsSyncRun, ProfileSyncPublishError> {
        let mut run = LocalSettingsSyncRun {
            profile: profile.to_string(),
            settings_root_id: settings_root_id.to_string(),
            steps: Vec::new(),
        };

        for _ in 0..max_steps {
            let step = self.publish_pending_local_settings_head(
                database,
                profile,
                settings_root_id,
                content_key,
                key_id,
                signer,
                retention_policy.clone(),
            )?;
            let is_idle = step.is_idle();
            run.steps.push(step);
            if is_idle {
                return Ok(run);
            }
        }

        Err(ProfileSyncPublishError::LocalSyncLoopExhausted {
            profile: profile.to_string(),
            settings_root_id: settings_root_id.to_string(),
            max_steps,
        })
    }

    fn publish_settings_tail_change_publications(
        &self,
        profile: &str,
        changes: &[SyncChangeRecord],
        content_key: &ProfileSyncContentKey,
        key_id: &str,
        signer: &ProfileSyncDeviceSigner,
    ) -> Result<Vec<ProfileSyncSettingsTailChangePublication>, ProfileSyncPublishError> {
        let mut tail_publications = Vec::with_capacity(changes.len());
        for change in changes {
            let incoming = incoming_setting_from_change(change);
            let object_bytes = sign_encrypted_json_object(
                incoming.profile.as_str(),
                incoming.domain.as_str(),
                PROFILE_SYNC_SETTING_CHANGE_OBJECT_KIND,
                key_id,
                &serde_json::to_vec(&incoming).map_err(SyncObjectError::Encode)?,
                content_key,
                signer,
            )?;
            let object_id = self.put_retained_object(profile, object_bytes)?;
            tail_publications.push(ProfileSyncSettingsTailChangePublication {
                object_id,
                change: change.clone(),
            });
        }
        Ok(tail_publications)
    }
}

fn enabled_settings_sync_domain_ids(
    database: &SlateProfileDatabase,
    profile: &str,
) -> Result<BTreeSet<String>, StorageError> {
    if database.app_sync_domains(profile)?.is_empty() {
        database.ensure_default_app_sync_domains(profile)?;
    }
    Ok(database
        .enabled_app_sync_domains(profile)?
        .into_iter()
        .map(|domain| domain.domain)
        .collect())
}

fn enabled_settings_sync_text_events_after(
    database: &SlateProfileDatabase,
    profile: &str,
    after_revision: i64,
    limit: u32,
) -> Result<Vec<SyncSettingTextEvent>, StorageError> {
    let enabled_domain_ids = enabled_settings_sync_domain_ids(database, profile)?;
    let mut events = Vec::new();
    for domain in enabled_domain_ids {
        events.extend(database.sync_setting_text_events_after_for_domain(
            profile,
            domain.as_str(),
            after_revision,
            limit,
        )?);
    }
    events.sort_by_key(|event| {
        (
            event.revision.revision,
            event.revision.created_at,
            event.revision.change_id,
        )
    });
    events.truncate(limit as usize);
    Ok(events)
}

pub fn validate_settings_sync_cycle_credentials(
    database: &SlateProfileDatabase,
    profile: &str,
    key_id: &str,
    signer: &ProfileSyncDeviceSigner,
) -> Result<(), ProfileSyncCredentialError> {
    let active_key_id = active_settings_sync_content_key_id(database, profile)?;
    if active_key_id != key_id {
        return Err(ProfileSyncCredentialError::InactiveContentKey {
            profile: profile.to_string(),
            expected_key_id: key_id.to_string(),
            active_key_id,
        });
    }

    let public_key = signer.public_key()?;
    if public_key.device_id != database.local_sync_device_id() {
        return Err(ProfileSyncCredentialError::LocalDeviceSignerMismatch {
            profile: profile.to_string(),
            local_device_id: database.local_sync_device_id().to_string(),
            signer_device_id: public_key.device_id,
        });
    }
    let Some(trusted_key) =
        database.sync_device_public_key(profile, public_key.device_id.as_str())?
    else {
        return Err(ProfileSyncCredentialError::UntrustedLocalDevice {
            profile: profile.to_string(),
            device_id: public_key.device_id,
        });
    };
    if !trusted_key.trusted {
        return Err(ProfileSyncCredentialError::UntrustedLocalDevice {
            profile: profile.to_string(),
            device_id: public_key.device_id,
        });
    }
    if trusted_key.public_key != public_key {
        return Err(ProfileSyncCredentialError::LocalDevicePublicKeyMismatch {
            profile: profile.to_string(),
            device_id: trusted_key.public_key.device_id,
        });
    }

    Ok(())
}

pub fn active_settings_sync_content_key_id(
    database: &SlateProfileDatabase,
    profile: &str,
) -> Result<String, ProfileSyncCredentialError> {
    let active_key = database
        .active_sync_content_key_epoch(profile)?
        .ok_or_else(|| StorageError::MissingActiveSyncContentKey(profile.to_string()))?;
    if active_key.algorithm != PROFILE_SYNC_CONTENT_KEY_ALGORITHM_CHACHA20_POLY1305 {
        return Err(StorageError::UnsupportedSyncContentKeyAlgorithm {
            key_id: active_key.key_id,
            algorithm: active_key.algorithm,
        }
        .into());
    }
    Ok(active_key.key_id)
}

fn trusted_remote_device_public_keys(
    database: &SlateProfileDatabase,
    profile: &str,
    max_devices: u32,
) -> Result<Vec<SyncDevicePublicKeyRecord>, ProfileSyncReceiveError> {
    let trusted_devices = database
        .sync_device_public_keys(profile)?
        .into_iter()
        .filter(|record| {
            record.trusted && record.public_key.device_id != database.local_sync_device_id()
        })
        .collect::<Vec<_>>();
    if trusted_devices.len() > max_devices as usize {
        return Err(ProfileSyncReceiveError::TrustedDeviceLimitExceeded {
            profile: profile.to_string(),
            trusted_device_count: trusted_devices.len(),
            max_devices,
        });
    }
    Ok(trusted_devices)
}

fn validate_tail_changes_for_publish(
    profile: &str,
    changes: &[SyncChangeRecord],
) -> Result<(), StorageError> {
    if changes.is_empty() {
        return Err(StorageError::InvalidProfileSyncManifest(
            "settings manifest tail is empty".to_string(),
        ));
    }
    validate_setting_changes_for_publish(profile, changes, "tail change")
}

fn validate_snapshot_for_publish(
    profile: &str,
    snapshot: &ProfileSyncSettingsSnapshot,
) -> Result<(), StorageError> {
    if snapshot.profile != profile {
        return Err(StorageError::InvalidProfileSyncManifest(format!(
            "snapshot profile {} does not match manifest profile {}",
            snapshot.profile, profile
        )));
    }
    if snapshot.schema_version != PROFILE_SYNC_SETTINGS_SNAPSHOT_SCHEMA_VERSION {
        return Err(StorageError::UnsupportedSyncSnapshotSchema(
            snapshot.schema_version,
        ));
    }
    Ok(())
}

fn validate_snapshot_covered_changes_for_publish(
    profile: &str,
    snapshot: &ProfileSyncSettingsSnapshot,
    changes: &[SyncChangeRecord],
) -> Result<(), StorageError> {
    validate_setting_changes_for_publish(profile, changes, "snapshot-covered change")?;
    for change in changes {
        if !snapshot
            .included_domains
            .iter()
            .any(|domain| domain == &change.domain)
        {
            return Err(StorageError::InvalidProfileSyncManifest(format!(
                "snapshot-covered change {} domain {} is not included in snapshot",
                change.id, change.domain
            )));
        }
    }
    Ok(())
}

fn validate_setting_changes_for_publish(
    profile: &str,
    changes: &[SyncChangeRecord],
    label: &str,
) -> Result<(), StorageError> {
    for change in changes {
        if change.profile != profile {
            return Err(StorageError::InvalidProfileSyncManifest(format!(
                "{label} {} profile {} does not match manifest profile {}",
                change.id, change.profile, profile
            )));
        }
        if change.operation != "set_text" {
            return Err(StorageError::InvalidProfileSyncManifest(format!(
                "{label} {} operation {} is not supported by settings manifests",
                change.id, change.operation
            )));
        }
    }
    Ok(())
}

fn validate_device_head_for_publish(
    profile: &str,
    root_id: &str,
    device_head: &ProfileSyncDeviceHead,
    signer: &ProfileSyncDeviceSigner,
) -> Result<(), ProfileSyncPublishError> {
    if root_id.is_empty() {
        return Err(StorageError::InvalidSyncRootId(root_id.to_string()).into());
    }
    if device_head.profile != profile {
        return Err(StorageError::InvalidProfileSyncManifest(format!(
            "device head profile {} does not match publish profile {}",
            device_head.profile, profile
        ))
        .into());
    }
    if device_head.root_id != root_id {
        return Err(StorageError::InvalidProfileSyncManifest(format!(
            "device head root {} does not match publish root {}",
            device_head.root_id, root_id
        ))
        .into());
    }
    if device_head.schema_version != PROFILE_SYNC_DEVICE_HEAD_SCHEMA_VERSION {
        return Err(SyncObjectError::UnsupportedSchema {
            object_kind: PROFILE_SYNC_DEVICE_HEAD_OBJECT_KIND.to_string(),
            schema_version: device_head.schema_version,
        }
        .into());
    }
    let public_key = signer.public_key()?;
    if device_head.device_id != public_key.device_id {
        return Err(SyncObjectError::DeviceKeyMismatch {
            expected_device_id: public_key.device_id,
            actual_device_id: device_head.device_id.clone(),
        }
        .into());
    }
    if device_head.latest_manifest_object_id.is_empty() {
        return Err(StorageError::InvalidProfileSyncManifest(
            "device head latest manifest object id is empty".to_string(),
        )
        .into());
    }
    Ok(())
}

fn latest_local_device_change_for_head<'a>(
    device_id: &str,
    changes: &'a [SyncChangeRecord],
) -> Option<&'a SyncChangeRecord> {
    changes
        .iter()
        .filter(|change| change.device_id == device_id)
        .max_by(|left, right| {
            (
                left.device_sequence,
                left.logical_clock,
                left.created_at,
                left.id,
            )
                .cmp(&(
                    right.device_sequence,
                    right.logical_clock,
                    right.created_at,
                    right.id,
                ))
        })
}

fn incoming_setting_from_change(change: &SyncChangeRecord) -> IncomingSyncSettingText {
    IncomingSyncSettingText::new(
        change.profile.clone(),
        change.domain.clone(),
        change.entity_key.clone(),
        change.payload.clone(),
        change.device_id.clone(),
        change.device_sequence,
        change.logical_clock,
    )
}

fn sign_encrypted_json_object(
    profile: &str,
    domain: &str,
    object_kind: &str,
    key_id: &str,
    payload: &[u8],
    content_key: &ProfileSyncContentKey,
    signer: &ProfileSyncDeviceSigner,
) -> Result<Vec<u8>, SyncObjectError> {
    let encrypted_object =
        EncryptedSyncObject::seal(profile, domain, object_kind, key_id, payload, content_key)?;
    let encrypted_bytes = encrypted_object.to_bytes()?;
    signer.sign(encrypted_bytes.as_slice())?.to_bytes()
}

fn decode_profile_sync_membership_log(
    bytes: &[u8],
    expected_profile: &str,
) -> Result<ProfileSyncMembershipLog, ProfileSyncReceiveError> {
    let log: ProfileSyncMembershipLog =
        serde_json::from_slice(bytes).map_err(SyncObjectError::Decode)?;
    validate_profile_sync_membership_log(&log, expected_profile)?;
    Ok(log)
}

fn validate_profile_sync_membership_log(
    log: &ProfileSyncMembershipLog,
    expected_profile: &str,
) -> Result<(), ProfileSyncReceiveError> {
    if log.schema_version != PROFILE_SYNC_MEMBERSHIP_LOG_SCHEMA_VERSION {
        return Err(ProfileSyncReceiveError::InvalidMembershipLog(format!(
            "unsupported schema version {}",
            log.schema_version
        )));
    }
    if log.profile != expected_profile {
        return Err(ProfileSyncReceiveError::InvalidMembershipLog(format!(
            "expected profile {expected_profile}, got {}",
            log.profile
        )));
    }
    if log.records.len() > PROFILE_SYNC_MEMBERSHIP_LOG_MAX_RECORDS {
        return Err(ProfileSyncReceiveError::InvalidMembershipLog(format!(
            "too many records: max {}, got {}",
            PROFILE_SYNC_MEMBERSHIP_LOG_MAX_RECORDS,
            log.records.len()
        )));
    }

    let mut seen_record_ids = BTreeSet::new();
    let mut previous_key: Option<(i64, String)> = None;
    for entry in &log.records {
        if entry.record_id.is_empty() {
            return Err(ProfileSyncReceiveError::InvalidMembershipLog(
                "record id must not be empty".to_string(),
            ));
        }
        if entry.root_id != sync_membership_record_root_id(entry.record_id.as_str()) {
            return Err(ProfileSyncReceiveError::InvalidMembershipLog(format!(
                "record {} has unexpected root {}",
                entry.record_id, entry.root_id
            )));
        }
        if entry.object_id.is_empty() {
            return Err(ProfileSyncReceiveError::InvalidMembershipLog(format!(
                "record {} has an empty object id",
                entry.record_id
            )));
        }
        if entry.membership_epoch < 1 {
            return Err(ProfileSyncReceiveError::InvalidMembershipLog(format!(
                "record {} has invalid membership epoch {}",
                entry.record_id, entry.membership_epoch
            )));
        }
        if !seen_record_ids.insert(entry.record_id.clone()) {
            return Err(ProfileSyncReceiveError::InvalidMembershipLog(format!(
                "record {} is duplicated",
                entry.record_id
            )));
        }

        let current_key = (entry.membership_epoch, entry.record_id.clone());
        if previous_key
            .as_ref()
            .is_some_and(|previous| previous > &current_key)
        {
            return Err(ProfileSyncReceiveError::InvalidMembershipLog(
                "records must be ordered by membership epoch and record id".to_string(),
            ));
        }
        previous_key = Some(current_key);
    }

    Ok(())
}

fn validate_membership_log_entry_object(
    expected_profile: &str,
    entry: &ProfileSyncMembershipLogEntry,
    signed_record: &[u8],
) -> Result<(), ProfileSyncReceiveError> {
    let (signer_device_id, membership_record) =
        decode_signed_membership_record_for_membership_log(signed_record)?;
    let expected_root_id = sync_membership_record_root_id(membership_record.record_id.as_str());
    if membership_record.profile != expected_profile
        || entry.record_id != membership_record.record_id
        || entry.root_id != expected_root_id
        || entry.membership_epoch != membership_record.membership_epoch
        || entry.record_kind != membership_record.record_kind
        || entry.device_id != membership_record.device_id
        || entry.signer_device_id != signer_device_id
    {
        return Err(ProfileSyncReceiveError::InvalidMembershipLog(format!(
            "entry {} does not match its signed membership record",
            entry.record_id
        )));
    }
    Ok(())
}

fn decode_signed_membership_record_for_membership_log(
    signed_record: &[u8],
) -> Result<(String, ProfileSyncMembershipRecord), ProfileSyncReceiveError> {
    let signed_object = SignedSyncObject::from_bytes(signed_record)?;
    let signer_key = ProfileSyncDevicePublicKey {
        device_id: signed_object.device_id.clone(),
        bytes: signed_object.public_key.clone(),
    };
    let payload = signed_object.verify_with(&signer_key)?;
    let membership_record = ProfileSyncMembershipRecord::from_bytes(payload)?;
    Ok((signed_object.device_id, membership_record))
}

impl ProfileSyncObjectSource for BroadwebdProfileSyncObjectSource<'_> {
    type Error = BroadwebdError;

    fn resolve_profile_sync_root(
        &self,
        profile: &str,
        root_id: &str,
    ) -> Result<Option<String>, Self::Error> {
        let response = self
            .daemon
            .profile_sync(BroadwebdProfileSyncRequest::ResolveRoot(
                BroadwebdProfileSyncRootRequest::new(profile, root_id),
            ))?;
        let BroadwebdProfileSyncResponse::Root { object_id, .. } = response else {
            return Err(BroadwebdError::UnsupportedRequest(
                "profile-sync resolve root returned a non-root response".to_string(),
            ));
        };
        Ok(object_id)
    }

    fn list_profile_sync_root_candidates(
        &self,
        profile: &str,
        root_id: &str,
    ) -> Result<Vec<StorageProfileSyncRootCandidate>, Self::Error> {
        let response =
            self.daemon
                .profile_sync(BroadwebdProfileSyncRequest::ListRootCandidates(
                    BroadwebdProfileSyncRootRequest::new(profile, root_id),
                ))?;
        let BroadwebdProfileSyncResponse::RootCandidates { candidates, .. } = response else {
            return Err(BroadwebdError::UnsupportedRequest(
                "profile-sync list root candidates returned a non-candidate response".to_string(),
            ));
        };
        Ok(candidates
            .into_iter()
            .map(|candidate| {
                StorageProfileSyncRootCandidate::new(
                    candidate.publisher_provider_id,
                    candidate.object_id,
                    candidate.publish_sequence,
                )
            })
            .collect())
    }

    fn get_profile_sync_object(
        &self,
        profile: &str,
        object_id: &str,
    ) -> Result<ProfileSyncObjectBytes, Self::Error> {
        let response =
            self.daemon
                .profile_sync(BroadwebdProfileSyncRequest::GetEncryptedObject(
                    BroadwebdProfileSyncObjectRequest::new(profile, object_id),
                ))?;
        let BroadwebdProfileSyncResponse::GetEncryptedObject { object_id, bytes } = response else {
            return Err(BroadwebdError::UnsupportedRequest(
                "profile-sync get object returned a non-object response".to_string(),
            ));
        };
        Ok(ProfileSyncObjectBytes { object_id, bytes })
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BroadwebdProfileSyncObjectSource, BroadwebdProfileSyncPublisher,
        BroadwebdSettingsSyncRunner, BroadwebdSettingsSyncScheduler,
        BroadwebdTrustedDeviceHeadSyncStatus, LocalSettingsHeadPublishStatus,
        PROFILE_SYNC_MEMBERSHIP_LOG_ROOT_ID, ProfileSyncCredentialError, ProfileSyncCycleError,
        ProfileSyncCycleWithHealthError, ProfileSyncMembershipLog, ProfileSyncMembershipLogEntry,
        ProfileSyncMembershipLogPublicationPlanStatus, ProfileSyncMembershipLogPullStatus,
        ProfileSyncMembershipRecordPullStatus, ProfileSyncPolicyError, ProfileSyncPublishError,
        ProfileSyncReceiveError, SettingsSyncCyclePolicy, SettingsSyncRetentionProviderHandle,
        SettingsSyncRuntimeSecrets, SettingsSyncSchedulerConfig, settings_device_head_root_id,
        sync_membership_record_root_id,
    };
    use slate_broadwebd::{
        BroadwebdError, ProfileSyncProfileRequest as BroadwebdProfileSyncProfileRequest,
        ProfileSyncRequest as BroadwebdProfileSyncRequest,
        ProfileSyncResponse as BroadwebdProfileSyncResponse, ResourceBudget,
        test_fixtures::InProcessBroadwebNetwork,
    };
    use slate_storage::{
        AppSyncDomainRegistration, BookmarkSlotSyncPayload, BookmarkUpdate,
        ChatConversationSyncPayload, ChatConversationUpdate, DEFAULT_DATABASE_FILE_NAME,
        DEFAULT_PROFILE_ID, DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH, FileEntrySyncPayload,
        FileEntryUpdate, IncomingSyncSettingText,
        PROFILE_SYNC_CONTENT_KEY_ALGORITHM_CHACHA20_POLY1305, PROFILE_SYNC_CONTENT_KEY_BYTES,
        PROFILE_SYNC_DEVICE_HEAD_SCHEMA_VERSION, PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_ENROLL_DEVICE,
        PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_REVOKE_DEVICE,
        PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_ROTATE_DEVICE_KEY,
        PROFILE_SYNC_MEMBERSHIP_RECORD_SCHEMA_VERSION, ProfileSyncContentKey,
        ProfileSyncDeviceHead, ProfileSyncDeviceSigner, ProfileSyncMembershipRecord,
        ProfileSyncObjectSource, ProfileSyncRetentionPolicy,
        ProfileSyncSettingsCandidatePullApplyStatus, SYNC_DOMAIN_BOOKMARKS, SYNC_DOMAIN_CALENDAR,
        SYNC_DOMAIN_CHAT, SYNC_DOMAIN_FILES, SYNC_DOMAIN_SETTINGS, SYNC_DOMAIN_STORAGE,
        SlateProfileDatabase, StorageError, StorageProviderSyncPayload, StorageProviderUpdate,
        SyncChangeRecord, SyncContentKeyEpochRegistration, SyncDevicePublicKeyRegistration,
        SyncSnapshotRegistration, TypedAppSyncDomainWatcher, open_signed_profile_sync_device_head,
        open_signed_profile_sync_manifest, open_signed_profile_sync_settings_snapshot,
        open_signed_sync_setting_text, pull_signed_profile_sync_device_head,
        settings_sync_snapshot_id,
    };
    use std::time::{SystemTime, UNIX_EPOCH};

    const TEST_CONTENT_KEY_ID: &str = "content-key-epoch-1";

    fn register_test_content_key_epoch(database: &SlateProfileDatabase, profile: &str) {
        database
            .register_sync_content_key_epoch(&SyncContentKeyEpochRegistration {
                profile: profile.to_string(),
                key_id: TEST_CONTENT_KEY_ID.to_string(),
                membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
                algorithm: PROFILE_SYNC_CONTENT_KEY_ALGORITHM_CHACHA20_POLY1305.to_string(),
                active: true,
            })
            .expect("register active test content key epoch");
    }

    fn signed_membership_record_bytes(
        signer: &ProfileSyncDeviceSigner,
        record: &ProfileSyncMembershipRecord,
    ) -> Vec<u8> {
        signer
            .sign(
                record
                    .to_bytes()
                    .expect("encode membership record")
                    .as_slice(),
            )
            .expect("sign membership record")
            .to_bytes()
            .expect("encode signed membership record")
    }

    #[test]
    fn membership_log_publication_plan_classifies_record_counts() {
        let empty =
            super::ProfileSyncMembershipLogPublicationPlan::for_record_count("default", "root", 0);
        assert_eq!(
            empty.status,
            ProfileSyncMembershipLogPublicationPlanStatus::Empty
        );
        assert!(empty.is_empty());
        assert!(!empty.is_publishable());
        assert!(!empty.requires_compaction());

        let capped = super::ProfileSyncMembershipLogPublicationPlan::for_record_count(
            "default",
            "root",
            super::PROFILE_SYNC_MEMBERSHIP_LOG_MAX_RECORDS,
        );
        assert_eq!(
            capped.status,
            ProfileSyncMembershipLogPublicationPlanStatus::Publishable
        );
        assert!(!capped.is_empty());
        assert!(capped.is_publishable());
        assert!(!capped.requires_compaction());

        let oversized = super::ProfileSyncMembershipLogPublicationPlan::for_record_count(
            "default",
            "root",
            super::PROFILE_SYNC_MEMBERSHIP_LOG_MAX_RECORDS + 1,
        );
        assert_eq!(
            oversized.status,
            ProfileSyncMembershipLogPublicationPlanStatus::TooLarge
        );
        assert!(!oversized.is_empty());
        assert!(!oversized.is_publishable());
        assert!(oversized.requires_compaction());
    }

    #[test]
    fn broadwebd_bridge_publishes_and_reads_fixture_objects() {
        let network = InProcessBroadwebNetwork::new();
        let state_root = test_state_root("broadwebd-source");
        let daemon = network
            .daemon_for_device(&state_root, ResourceBudget::default(), "runtime-a")
            .expect("start in-process profile-sync daemon");
        let object_bytes = b"encrypted runtime object".to_vec();
        let publisher = BroadwebdProfileSyncPublisher::new(&daemon);
        let object_id = publisher
            .put_retained_root("default", "settings/latest", object_bytes.clone())
            .expect("put, retain, and publish local profile-sync root");
        let retained = publisher
            .verify_retained_object("default", object_id.as_str())
            .expect("verify retained local profile-sync object");
        assert_eq!(retained.object_id, object_id);
        assert!(retained.retained);
        assert!(retained.available);

        let source = BroadwebdProfileSyncObjectSource::new(&daemon);
        assert_eq!(
            source
                .resolve_profile_sync_root("default", "settings/latest")
                .expect("resolve root")
                .as_deref(),
            Some(object_id.as_str())
        );
        let candidates = source
            .list_profile_sync_root_candidates("default", "settings/latest")
            .expect("list root candidates");
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].object_id, object_id);
        assert_eq!(candidates[0].publisher_id, "local-fixture-device-runtime-a");
        assert_eq!(candidates[0].publish_sequence, 1);

        let fetched = source
            .get_profile_sync_object("default", candidates[0].object_id.as_str())
            .expect("fetch object");
        assert_eq!(fetched.object_id, candidates[0].object_id);
        assert_eq!(fetched.bytes, object_bytes);

        assert!(
            !publisher
                .release_object("default", candidates[0].object_id.as_str())
                .expect("release local profile-sync object")
        );
        let released = publisher
            .verify_retained_object("default", candidates[0].object_id.as_str())
            .expect("verify released local profile-sync object");
        assert_eq!(released.object_id, candidates[0].object_id);
        assert!(!released.retained);
        assert!(released.available);

        let _ = std::fs::remove_dir_all(state_root);
    }

    #[test]
    fn broadwebd_bridge_publishes_and_applies_membership_records_without_loopback() {
        let network = InProcessBroadwebNetwork::new();
        let publisher_state_root = test_state_root("membership-publisher");
        let receiver_state_root = test_state_root("membership-receiver");
        let receiver_db_root = test_state_root("membership-receiver-db");
        let publisher_daemon = network
            .daemon_for_device(
                &publisher_state_root,
                ResourceBudget::default(),
                "membership-publisher-device",
            )
            .expect("start in-process membership publisher daemon");
        let receiver_daemon = network
            .daemon_for_device(
                &receiver_state_root,
                ResourceBudget::default(),
                "membership-receiver-device",
            )
            .expect("start in-process membership receiver daemon");
        let receiver_database =
            SlateProfileDatabase::open_resolved(receiver_db_root.join(DEFAULT_DATABASE_FILE_NAME))
                .expect("open receiver settings database");
        let signer = ProfileSyncDeviceSigner::generate("membership-device-a")
            .expect("generate membership signer");
        let record_id = "epoch-1-enroll-membership-device-a";
        let membership_record = ProfileSyncMembershipRecord {
            profile: DEFAULT_PROFILE_ID.to_string(),
            record_id: record_id.to_string(),
            schema_version: PROFILE_SYNC_MEMBERSHIP_RECORD_SCHEMA_VERSION,
            membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            record_kind: PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_ENROLL_DEVICE.to_string(),
            device_id: "membership-device-a".to_string(),
            device_public_key: Some(signer.public_key().expect("read signer public key")),
            created_at: 10,
        };
        let signed_record = signed_membership_record_bytes(&signer, &membership_record);
        let root_id = sync_membership_record_root_id(record_id);

        let publication = BroadwebdProfileSyncPublisher::new(&publisher_daemon)
            .publish_signed_sync_account_membership_record(
                DEFAULT_PROFILE_ID,
                root_id.as_str(),
                signed_record.clone(),
            )
            .expect("publish signed membership record through in-process broadwebd");
        assert_eq!(publication.profile, DEFAULT_PROFILE_ID);
        assert_eq!(publication.root_id, root_id);
        assert_eq!(publication.signed_record, signed_record);

        let applied = BroadwebdProfileSyncObjectSource::new(&receiver_daemon)
            .pull_and_apply_sync_account_membership_record_if_changed(
                &receiver_database,
                DEFAULT_PROFILE_ID,
                root_id.as_str(),
            )
            .expect("pull and apply signed membership record through in-process broadwebd");
        let ProfileSyncMembershipRecordPullStatus::Applied { root, application } = applied else {
            panic!("expected applied membership record from in-process broadwebd");
        };
        assert_eq!(root.profile, DEFAULT_PROFILE_ID);
        assert_eq!(root.root_id, root_id);
        assert_eq!(root.object_id, publication.object_id);
        assert!(application.bootstrapped);
        assert!(application.applied);
        assert_eq!(
            application.membership_record.record_id,
            "epoch-1-enroll-membership-device-a"
        );
        assert_eq!(
            application
                .device_key
                .as_ref()
                .expect("applied membership device key")
                .public_key
                .device_id,
            "membership-device-a"
        );

        let unchanged = BroadwebdProfileSyncObjectSource::new(&receiver_daemon)
            .pull_and_apply_sync_account_membership_record_if_changed(
                &receiver_database,
                DEFAULT_PROFILE_ID,
                root_id.as_str(),
            )
            .expect("skip unchanged membership root");
        assert!(matches!(
            unchanged,
            ProfileSyncMembershipRecordPullStatus::Unchanged {
                object_id,
                ..
            } if object_id == publication.object_id
        ));

        let _ = std::fs::remove_dir_all(publisher_state_root);
        let _ = std::fs::remove_dir_all(receiver_state_root);
        let _ = std::fs::remove_dir_all(receiver_db_root);
    }

    #[test]
    fn broadwebd_bridge_publishes_and_applies_membership_log_without_loopback() {
        let network = InProcessBroadwebNetwork::new();
        let publisher_state_root = test_state_root("membership-log-publisher");
        let receiver_state_root = test_state_root("membership-log-receiver");
        let publisher_db_root = test_state_root("membership-log-publisher-db");
        let receiver_db_root = test_state_root("membership-log-receiver-db");
        let publisher_daemon = network
            .daemon_for_device(
                &publisher_state_root,
                ResourceBudget::default(),
                "membership-log-publisher-device",
            )
            .expect("start in-process membership log publisher daemon");
        let receiver_daemon = network
            .daemon_for_device(
                &receiver_state_root,
                ResourceBudget::default(),
                "membership-log-receiver-device",
            )
            .expect("start in-process membership log receiver daemon");
        let publisher_database =
            SlateProfileDatabase::open_resolved(publisher_db_root.join(DEFAULT_DATABASE_FILE_NAME))
                .expect("open publisher settings database");
        let receiver_database =
            SlateProfileDatabase::open_resolved(receiver_db_root.join(DEFAULT_DATABASE_FILE_NAME))
                .expect("open receiver settings database");
        let signer_a = ProfileSyncDeviceSigner::generate("membership-log-device-a")
            .expect("generate membership log signer a");
        let signer_b = ProfileSyncDeviceSigner::generate("membership-log-device-b")
            .expect("generate membership log signer b");
        let enroll_a = ProfileSyncMembershipRecord {
            profile: DEFAULT_PROFILE_ID.to_string(),
            record_id: "epoch-1-enroll-membership-log-device-a".to_string(),
            schema_version: PROFILE_SYNC_MEMBERSHIP_RECORD_SCHEMA_VERSION,
            membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            record_kind: PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_ENROLL_DEVICE.to_string(),
            device_id: "membership-log-device-a".to_string(),
            device_public_key: Some(signer_a.public_key().expect("read signer a public key")),
            created_at: 10,
        };
        let signed_enroll_a = signed_membership_record_bytes(&signer_a, &enroll_a);
        publisher_database
            .apply_signed_sync_account_membership_record(signed_enroll_a.as_slice())
            .expect("publisher bootstraps signer a membership");
        let enroll_b = ProfileSyncMembershipRecord {
            profile: DEFAULT_PROFILE_ID.to_string(),
            record_id: "epoch-2-enroll-membership-log-device-b".to_string(),
            schema_version: PROFILE_SYNC_MEMBERSHIP_RECORD_SCHEMA_VERSION,
            membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH + 1,
            record_kind: PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_ENROLL_DEVICE.to_string(),
            device_id: "membership-log-device-b".to_string(),
            device_public_key: Some(signer_b.public_key().expect("read signer b public key")),
            created_at: 20,
        };
        let signed_enroll_b = signed_membership_record_bytes(&signer_a, &enroll_b);
        publisher_database
            .apply_signed_sync_account_membership_record(signed_enroll_b.as_slice())
            .expect("publisher applies signer b membership");

        let publisher = BroadwebdProfileSyncPublisher::new(&publisher_daemon);
        let plan = publisher
            .plan_local_sync_account_membership_log(
                &publisher_database,
                DEFAULT_PROFILE_ID,
                PROFILE_SYNC_MEMBERSHIP_LOG_ROOT_ID,
            )
            .expect("plan local membership log publication");
        assert_eq!(
            plan.status,
            ProfileSyncMembershipLogPublicationPlanStatus::Publishable
        );
        assert!(plan.is_publishable());
        assert!(!plan.requires_compaction());
        assert_eq!(plan.record_count, 2);

        let publication = publisher
            .publish_local_sync_account_membership_log(
                &publisher_database,
                DEFAULT_PROFILE_ID,
                PROFILE_SYNC_MEMBERSHIP_LOG_ROOT_ID,
            )
            .expect("publish local membership log through in-process broadwebd")
            .expect("membership log has records");
        assert_eq!(publication.profile, DEFAULT_PROFILE_ID);
        assert_eq!(publication.root_id, PROFILE_SYNC_MEMBERSHIP_LOG_ROOT_ID);
        assert_eq!(publication.log.records.len(), 2);
        assert_eq!(
            publication.log.records[0].root_id,
            sync_membership_record_root_id("epoch-1-enroll-membership-log-device-a")
        );
        assert_eq!(
            publication.log.records[1].root_id,
            sync_membership_record_root_id("epoch-2-enroll-membership-log-device-b")
        );

        let source = BroadwebdProfileSyncObjectSource::new(&receiver_daemon);
        assert_eq!(
            source
                .resolve_profile_sync_root(
                    DEFAULT_PROFILE_ID,
                    sync_membership_record_root_id("epoch-2-enroll-membership-log-device-b")
                        .as_str(),
                )
                .expect("resolve published membership record root")
                .as_deref(),
            Some(publication.log.records[1].object_id.as_str())
        );
        let applied = source
            .pull_and_apply_sync_account_membership_log_if_changed(
                &receiver_database,
                DEFAULT_PROFILE_ID,
                PROFILE_SYNC_MEMBERSHIP_LOG_ROOT_ID,
            )
            .expect("pull and apply membership log through in-process broadwebd");
        let ProfileSyncMembershipLogPullStatus::Applied {
            root,
            log,
            applications,
        } = applied
        else {
            panic!("expected applied membership log from in-process broadwebd");
        };
        assert_eq!(root.object_id, publication.object_id);
        assert_eq!(log, publication.log);
        assert_eq!(applications.len(), 2);
        assert!(applications[0].bootstrapped);
        assert!(applications[0].applied);
        assert!(!applications[1].bootstrapped);
        assert!(applications[1].applied);
        assert!(
            receiver_database
                .sync_device_public_key(DEFAULT_PROFILE_ID, "membership-log-device-a")
                .expect("read receiver signer a key")
                .expect("receiver has signer a key")
                .trusted
        );
        assert!(
            receiver_database
                .sync_device_public_key(DEFAULT_PROFILE_ID, "membership-log-device-b")
                .expect("read receiver signer b key")
                .expect("receiver has signer b key")
                .trusted
        );

        let unchanged = source
            .pull_and_apply_sync_account_membership_log_if_changed(
                &receiver_database,
                DEFAULT_PROFILE_ID,
                PROFILE_SYNC_MEMBERSHIP_LOG_ROOT_ID,
            )
            .expect("skip unchanged membership log root");
        assert!(matches!(
            unchanged,
            ProfileSyncMembershipLogPullStatus::Unchanged {
                object_id,
                ..
            } if object_id == publication.object_id
        ));

        let _ = std::fs::remove_dir_all(publisher_state_root);
        let _ = std::fs::remove_dir_all(receiver_state_root);
        let _ = std::fs::remove_dir_all(publisher_db_root);
        let _ = std::fs::remove_dir_all(receiver_db_root);
    }

    #[test]
    fn broadwebd_membership_log_objects_can_be_retained_by_provider_without_loopback() {
        let network = InProcessBroadwebNetwork::new();
        let device_state_root = test_state_root("membership-log-retention-device");
        let provider_state_root = test_state_root("membership-log-retention-provider");
        let db_root = test_state_root("membership-log-retention-db");
        let device_daemon = network
            .daemon_for_device(
                &device_state_root,
                ResourceBudget::default(),
                "membership-log-retention-device",
            )
            .expect("start in-process membership log device daemon");
        let provider_daemon = network
            .daemon_for_availability_provider(
                &provider_state_root,
                ResourceBudget::default(),
                "membership-log-retention-pinner",
            )
            .expect("start in-process membership log availability provider daemon");
        let database =
            SlateProfileDatabase::open_resolved(db_root.join(DEFAULT_DATABASE_FILE_NAME))
                .expect("open membership log retention database");
        let signer = ProfileSyncDeviceSigner::generate("membership-log-retention-signer")
            .expect("generate membership log retention signer");
        let enroll = ProfileSyncMembershipRecord {
            profile: DEFAULT_PROFILE_ID.to_string(),
            record_id: "epoch-1-enroll-membership-log-retention-signer".to_string(),
            schema_version: PROFILE_SYNC_MEMBERSHIP_RECORD_SCHEMA_VERSION,
            membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            record_kind: PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_ENROLL_DEVICE.to_string(),
            device_id: "membership-log-retention-signer".to_string(),
            device_public_key: Some(signer.public_key().expect("read signer public key")),
            created_at: 10,
        };
        database
            .apply_signed_sync_account_membership_record(
                signed_membership_record_bytes(&signer, &enroll).as_slice(),
            )
            .expect("bootstrap membership log retention signer");

        let publication = BroadwebdProfileSyncPublisher::new(&device_daemon)
            .publish_local_sync_account_membership_log(
                &database,
                DEFAULT_PROFILE_ID,
                PROFILE_SYNC_MEMBERSHIP_LOG_ROOT_ID,
            )
            .expect("publish membership log for retention")
            .expect("membership log has one record");
        let object_ids = publication.published_object_ids();
        assert_eq!(object_ids.len(), 2);

        let statuses = BroadwebdProfileSyncPublisher::new(&provider_daemon)
            .retain_profile_sync_membership_log_objects(&publication)
            .expect("provider retains membership log object set");
        assert_eq!(statuses.len(), object_ids.len());
        assert_eq!(
            statuses
                .iter()
                .map(|status| status.object_id.clone())
                .collect::<Vec<_>>(),
            object_ids
        );
        assert!(statuses.iter().all(|status| status.retained));
        assert!(statuses.iter().all(|status| status.available));

        let _ = std::fs::remove_dir_all(device_state_root);
        let _ = std::fs::remove_dir_all(provider_state_root);
        let _ = std::fs::remove_dir_all(db_root);
    }

    #[test]
    fn membership_cycle_retains_settings_and_membership_objects_without_loopback() {
        let network = InProcessBroadwebNetwork::new();
        let device_state_root = test_state_root("membership-cycle-retention-device");
        let provider_state_root = test_state_root("membership-cycle-retention-provider");
        let db_root = test_state_root("membership-cycle-retention-db");
        let device_daemon = network
            .daemon_for_device(
                &device_state_root,
                ResourceBudget::default(),
                "membership-cycle-retention-device",
            )
            .expect("start in-process membership cycle retention device daemon");
        let provider_daemon = network
            .daemon_for_availability_provider(
                &provider_state_root,
                ResourceBudget::default(),
                "membership-cycle-retention-pinner",
            )
            .expect("start in-process membership cycle retention provider daemon");
        let database = SlateProfileDatabase::open_resolved_with_device_id(
            db_root.join(DEFAULT_DATABASE_FILE_NAME),
            "membership-cycle-retention-device",
        )
        .expect("open membership cycle retention database");
        let content_key = ProfileSyncContentKey::from_bytes([77; PROFILE_SYNC_CONTENT_KEY_BYTES]);
        let signer = ProfileSyncDeviceSigner::generate("membership-cycle-retention-device")
            .expect("generate membership cycle retention signer");
        register_test_content_key_epoch(&database, DEFAULT_PROFILE_ID);
        let enroll = ProfileSyncMembershipRecord {
            profile: DEFAULT_PROFILE_ID.to_string(),
            record_id: "epoch-1-enroll-membership-cycle-retention-device".to_string(),
            schema_version: PROFILE_SYNC_MEMBERSHIP_RECORD_SCHEMA_VERSION,
            membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            record_kind: PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_ENROLL_DEVICE.to_string(),
            device_id: "membership-cycle-retention-device".to_string(),
            device_public_key: Some(signer.public_key().expect("read signer public key")),
            created_at: 10,
        };
        database
            .apply_signed_sync_account_membership_record(
                signed_membership_record_bytes(&signer, &enroll).as_slice(),
            )
            .expect("bootstrap membership cycle retention signer");
        database
            .set_sync_setting_text(DEFAULT_PROFILE_ID, SYNC_DOMAIN_SETTINGS, "ui.theme", "teal")
            .expect("write membership cycle retention setting");

        let run = BroadwebdSettingsSyncRunner::new(&device_daemon)
            .run_settings_sync_cycle_with_membership_log_and_retention_providers(
                &database,
                DEFAULT_PROFILE_ID,
                "settings/latest",
                PROFILE_SYNC_MEMBERSHIP_LOG_ROOT_ID,
                &content_key,
                TEST_CONTENT_KEY_ID,
                &signer,
                ProfileSyncRetentionPolicy::default(),
                4,
                4,
                &[&provider_daemon],
            )
            .expect("membership-aware cycle retains settings and membership objects");

        assert_eq!(run.cycle.cycle.published_step_count(), 1);
        assert!(run.cycle.published_membership_log.is_some());
        assert_eq!(run.retention.len(), 1);
        assert_eq!(
            run.retention[0].object_count(),
            run.retained_object_ids.len()
        );
        assert_eq!(
            run.retention[0].retained_count(),
            run.retained_object_ids.len()
        );
        assert_eq!(
            run.retention[0].available_count(),
            run.retained_object_ids.len()
        );
        assert_eq!(run.retained_provider_count(), 1);
        assert_eq!(run.retained_object_ids, run.cycle.published_object_ids());

        let _ = std::fs::remove_dir_all(device_state_root);
        let _ = std::fs::remove_dir_all(provider_state_root);
        let _ = std::fs::remove_dir_all(db_root);
    }

    #[test]
    fn membership_preflight_enrolls_local_device_before_credential_check_without_loopback() {
        let network = InProcessBroadwebNetwork::new();
        let publisher_state_root = test_state_root("membership-preflight-publisher");
        let receiver_state_root = test_state_root("membership-preflight-receiver");
        let publisher_db_root = test_state_root("membership-preflight-publisher-db");
        let receiver_db_root = test_state_root("membership-preflight-receiver-db");
        let publisher_daemon = network
            .daemon_for_device(
                &publisher_state_root,
                ResourceBudget::default(),
                "membership-preflight-device-a",
            )
            .expect("start in-process membership preflight publisher daemon");
        let receiver_daemon = network
            .daemon_for_device(
                &receiver_state_root,
                ResourceBudget::default(),
                "membership-preflight-device-b",
            )
            .expect("start in-process membership preflight receiver daemon");
        let publisher_database = SlateProfileDatabase::open_resolved_with_device_id(
            publisher_db_root.join(DEFAULT_DATABASE_FILE_NAME),
            "membership-preflight-device-a",
        )
        .expect("open membership preflight publisher database");
        let receiver_database = SlateProfileDatabase::open_resolved_with_device_id(
            receiver_db_root.join(DEFAULT_DATABASE_FILE_NAME),
            "membership-preflight-device-b",
        )
        .expect("open membership preflight receiver database");
        let signer_a = ProfileSyncDeviceSigner::generate("membership-preflight-device-a")
            .expect("generate membership preflight signer a");
        let signer_b = ProfileSyncDeviceSigner::generate("membership-preflight-device-b")
            .expect("generate membership preflight signer b");
        register_test_content_key_epoch(&receiver_database, DEFAULT_PROFILE_ID);
        let enroll_a = ProfileSyncMembershipRecord {
            profile: DEFAULT_PROFILE_ID.to_string(),
            record_id: "epoch-1-enroll-membership-preflight-device-a".to_string(),
            schema_version: PROFILE_SYNC_MEMBERSHIP_RECORD_SCHEMA_VERSION,
            membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            record_kind: PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_ENROLL_DEVICE.to_string(),
            device_id: "membership-preflight-device-a".to_string(),
            device_public_key: Some(signer_a.public_key().expect("read signer a public key")),
            created_at: 10,
        };
        publisher_database
            .apply_signed_sync_account_membership_record(
                signed_membership_record_bytes(&signer_a, &enroll_a).as_slice(),
            )
            .expect("publisher bootstraps membership preflight signer a");
        let enroll_b = ProfileSyncMembershipRecord {
            profile: DEFAULT_PROFILE_ID.to_string(),
            record_id: "epoch-2-enroll-membership-preflight-device-b".to_string(),
            schema_version: PROFILE_SYNC_MEMBERSHIP_RECORD_SCHEMA_VERSION,
            membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH + 1,
            record_kind: PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_ENROLL_DEVICE.to_string(),
            device_id: "membership-preflight-device-b".to_string(),
            device_public_key: Some(signer_b.public_key().expect("read signer b public key")),
            created_at: 20,
        };
        publisher_database
            .apply_signed_sync_account_membership_record(
                signed_membership_record_bytes(&signer_a, &enroll_b).as_slice(),
            )
            .expect("publisher applies membership preflight signer b enrollment");
        BroadwebdProfileSyncPublisher::new(&publisher_daemon)
            .publish_local_sync_account_membership_log(
                &publisher_database,
                DEFAULT_PROFILE_ID,
                PROFILE_SYNC_MEMBERSHIP_LOG_ROOT_ID,
            )
            .expect("publish membership preflight log")
            .expect("membership preflight log has records");

        let policy = SettingsSyncCyclePolicy::new(ProfileSyncRetentionPolicy::default(), 4, 4, 1)
            .with_provider_health_required(false);
        let runner = BroadwebdSettingsSyncRunner::new(&receiver_daemon);
        let direct_error = runner
            .settings_sync_cycle_preflight_with_active_key_policy(
                &receiver_database,
                DEFAULT_PROFILE_ID,
                "settings/latest",
                &signer_b,
                &policy,
            )
            .expect_err("plain preflight should reject missing local trusted key");
        assert!(matches!(
            direct_error,
            ProfileSyncCycleWithHealthError::Cycle(ProfileSyncCycleError::Credentials(
                ProfileSyncCredentialError::UntrustedLocalDevice { device_id, .. }
            )) if device_id == "membership-preflight-device-b"
        ));

        let preflight = runner
            .settings_sync_cycle_preflight_with_membership_log_and_active_key_policy(
                &receiver_database,
                DEFAULT_PROFILE_ID,
                "settings/latest",
                PROFILE_SYNC_MEMBERSHIP_LOG_ROOT_ID,
                &signer_b,
                &policy,
            )
            .expect("membership-aware preflight applies local enrollment before credentials");
        assert_eq!(preflight.pulled_membership_application_count(), 2);
        assert_eq!(
            preflight.preflight.local_device_id,
            "membership-preflight-device-b"
        );
        assert_eq!(
            preflight.preflight.signer_device_id,
            "membership-preflight-device-b"
        );
        assert!(
            receiver_database
                .sync_device_public_key(DEFAULT_PROFILE_ID, "membership-preflight-device-b")
                .expect("read receiver local trusted key")
                .expect("membership-aware preflight stores local key")
                .trusted
        );

        let _ = std::fs::remove_dir_all(publisher_state_root);
        let _ = std::fs::remove_dir_all(receiver_state_root);
        let _ = std::fs::remove_dir_all(publisher_db_root);
        let _ = std::fs::remove_dir_all(receiver_db_root);
    }

    #[test]
    fn scheduler_runs_membership_aware_cycle_with_selected_provider_without_loopback() {
        let network = InProcessBroadwebNetwork::new();
        let publisher_state_root = test_state_root("membership-scheduler-publisher");
        let receiver_state_root = test_state_root("membership-scheduler-receiver");
        let provider_state_root = test_state_root("membership-scheduler-provider");
        let publisher_db_root = test_state_root("membership-scheduler-publisher-db");
        let receiver_db_root = test_state_root("membership-scheduler-receiver-db");
        let publisher_daemon = network
            .daemon_for_device(
                &publisher_state_root,
                ResourceBudget::default(),
                "membership-scheduler-device-a",
            )
            .expect("start in-process membership scheduler publisher daemon");
        let receiver_daemon = network
            .daemon_for_device(
                &receiver_state_root,
                ResourceBudget::default(),
                "membership-scheduler-device-b",
            )
            .expect("start in-process membership scheduler receiver daemon");
        let provider_daemon = network
            .daemon_for_availability_provider(
                &provider_state_root,
                ResourceBudget::default(),
                "membership-scheduler-pinner",
            )
            .expect("start in-process membership scheduler provider daemon");
        let publisher_database = SlateProfileDatabase::open_resolved_with_device_id(
            publisher_db_root.join(DEFAULT_DATABASE_FILE_NAME),
            "membership-scheduler-device-a",
        )
        .expect("open membership scheduler publisher database");
        let receiver_database = SlateProfileDatabase::open_resolved_with_device_id(
            receiver_db_root.join(DEFAULT_DATABASE_FILE_NAME),
            "membership-scheduler-device-b",
        )
        .expect("open membership scheduler receiver database");
        let content_key = ProfileSyncContentKey::from_bytes([78; PROFILE_SYNC_CONTENT_KEY_BYTES]);
        let signer_a = ProfileSyncDeviceSigner::generate("membership-scheduler-device-a")
            .expect("generate membership scheduler signer a");
        let signer_b = ProfileSyncDeviceSigner::generate("membership-scheduler-device-b")
            .expect("generate membership scheduler signer b");
        register_test_content_key_epoch(&publisher_database, DEFAULT_PROFILE_ID);
        register_test_content_key_epoch(&receiver_database, DEFAULT_PROFILE_ID);
        let enroll_a = ProfileSyncMembershipRecord {
            profile: DEFAULT_PROFILE_ID.to_string(),
            record_id: "epoch-1-enroll-membership-scheduler-device-a".to_string(),
            schema_version: PROFILE_SYNC_MEMBERSHIP_RECORD_SCHEMA_VERSION,
            membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            record_kind: PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_ENROLL_DEVICE.to_string(),
            device_id: "membership-scheduler-device-a".to_string(),
            device_public_key: Some(signer_a.public_key().expect("read signer a public key")),
            created_at: 10,
        };
        publisher_database
            .apply_signed_sync_account_membership_record(
                signed_membership_record_bytes(&signer_a, &enroll_a).as_slice(),
            )
            .expect("publisher bootstraps membership scheduler signer a");
        let enroll_b = ProfileSyncMembershipRecord {
            profile: DEFAULT_PROFILE_ID.to_string(),
            record_id: "epoch-2-enroll-membership-scheduler-device-b".to_string(),
            schema_version: PROFILE_SYNC_MEMBERSHIP_RECORD_SCHEMA_VERSION,
            membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH + 1,
            record_kind: PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_ENROLL_DEVICE.to_string(),
            device_id: "membership-scheduler-device-b".to_string(),
            device_public_key: Some(signer_b.public_key().expect("read signer b public key")),
            created_at: 20,
        };
        publisher_database
            .apply_signed_sync_account_membership_record(
                signed_membership_record_bytes(&signer_a, &enroll_b).as_slice(),
            )
            .expect("publisher applies membership scheduler signer b enrollment");
        publisher_database
            .set_sync_setting_text(DEFAULT_PROFILE_ID, SYNC_DOMAIN_SETTINGS, "ui.theme", "teal")
            .expect("publisher writes membership scheduler setting");
        BroadwebdSettingsSyncRunner::new(&publisher_daemon)
            .run_settings_sync_cycle_with_membership_log(
                &publisher_database,
                DEFAULT_PROFILE_ID,
                "settings/latest",
                PROFILE_SYNC_MEMBERSHIP_LOG_ROOT_ID,
                &content_key,
                TEST_CONTENT_KEY_ID,
                &signer_a,
                ProfileSyncRetentionPolicy::default(),
                4,
                4,
            )
            .expect("publisher publishes membership and settings state");
        assert!(
            receiver_database
                .sync_device_public_key(DEFAULT_PROFILE_ID, "membership-scheduler-device-b")
                .expect("read receiver local key before scheduler")
                .is_none()
        );

        let selected_provider_id = "local-fixture-availability-membership-scheduler-pinner";
        let retention_provider_handles = [
            SettingsSyncRetentionProviderHandle::new(
                "not-discovered-membership-provider",
                &provider_daemon,
            ),
            SettingsSyncRetentionProviderHandle::new(selected_provider_id, &provider_daemon),
            SettingsSyncRetentionProviderHandle::new(selected_provider_id, &provider_daemon),
        ];
        let config = SettingsSyncSchedulerConfig::new(
            DEFAULT_PROFILE_ID,
            "settings/latest",
            SettingsSyncCyclePolicy::new(ProfileSyncRetentionPolicy::default(), 4, 4, 2)
                .with_provider_health_required(false)
                .with_root_health_required_after_cycle(false),
        );
        let run = BroadwebdSettingsSyncScheduler::new(&receiver_daemon)
            .run_once_with_membership_log_selecting_retention_providers(
                &receiver_database,
                &config,
                PROFILE_SYNC_MEMBERSHIP_LOG_ROOT_ID,
                SettingsSyncRuntimeSecrets::new(&content_key, &signer_b),
                &retention_provider_handles,
            )
            .expect("membership-aware scheduler applies and retains through selected provider");

        assert_eq!(run.preflight.pulled_membership_application_count(), 2);
        assert_eq!(
            run.selected_retention_provider_ids,
            vec![selected_provider_id.to_string()]
        );
        assert_eq!(
            run.undiscovered_retention_provider_ids,
            vec!["not-discovered-membership-provider".to_string()]
        );
        assert_eq!(
            run.duplicate_retention_provider_ids,
            vec![selected_provider_id.to_string()]
        );
        assert_eq!(run.cycle.cycle.cycle.applied_count(), 1);
        assert_eq!(
            receiver_database
                .get_setting_text("ui.theme")
                .expect("read synced membership scheduler setting")
                .as_deref(),
            Some("teal")
        );
        assert_eq!(run.cycle.retention.len(), 1);
        assert_eq!(
            run.cycle.retention[0].object_count(),
            run.cycle.retained_object_ids.len()
        );
        assert_eq!(
            run.cycle.retention[0].retained_count(),
            run.cycle.retained_object_ids.len()
        );
        assert_eq!(run.retained_provider_count(), 1);

        let _ = std::fs::remove_dir_all(publisher_state_root);
        let _ = std::fs::remove_dir_all(receiver_state_root);
        let _ = std::fs::remove_dir_all(provider_state_root);
        let _ = std::fs::remove_dir_all(publisher_db_root);
        let _ = std::fs::remove_dir_all(receiver_db_root);
    }

    #[test]
    fn broadwebd_membership_log_rejects_mismatched_record_objects_without_loopback() {
        let network = InProcessBroadwebNetwork::new();
        let publisher_state_root = test_state_root("membership-log-mismatch-publisher");
        let receiver_state_root = test_state_root("membership-log-mismatch-receiver");
        let receiver_db_root = test_state_root("membership-log-mismatch-receiver-db");
        let publisher_daemon = network
            .daemon_for_device(
                &publisher_state_root,
                ResourceBudget::default(),
                "membership-log-mismatch-publisher",
            )
            .expect("start in-process membership log mismatch publisher daemon");
        let receiver_daemon = network
            .daemon_for_device(
                &receiver_state_root,
                ResourceBudget::default(),
                "membership-log-mismatch-receiver",
            )
            .expect("start in-process membership log mismatch receiver daemon");
        let receiver_database =
            SlateProfileDatabase::open_resolved(receiver_db_root.join(DEFAULT_DATABASE_FILE_NAME))
                .expect("open membership log mismatch receiver database");
        let signer_a = ProfileSyncDeviceSigner::generate("membership-log-mismatch-a")
            .expect("generate mismatch signer a");
        let signer_b = ProfileSyncDeviceSigner::generate("membership-log-mismatch-b")
            .expect("generate mismatch signer b");
        let enroll_a = ProfileSyncMembershipRecord {
            profile: DEFAULT_PROFILE_ID.to_string(),
            record_id: "epoch-1-enroll-membership-log-mismatch-a".to_string(),
            schema_version: PROFILE_SYNC_MEMBERSHIP_RECORD_SCHEMA_VERSION,
            membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            record_kind: PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_ENROLL_DEVICE.to_string(),
            device_id: "membership-log-mismatch-a".to_string(),
            device_public_key: Some(signer_a.public_key().expect("read signer a public key")),
            created_at: 10,
        };
        let enroll_b = ProfileSyncMembershipRecord {
            profile: DEFAULT_PROFILE_ID.to_string(),
            record_id: "epoch-1-enroll-membership-log-mismatch-b".to_string(),
            schema_version: PROFILE_SYNC_MEMBERSHIP_RECORD_SCHEMA_VERSION,
            membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            record_kind: PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_ENROLL_DEVICE.to_string(),
            device_id: "membership-log-mismatch-b".to_string(),
            device_public_key: Some(signer_b.public_key().expect("read signer b public key")),
            created_at: 11,
        };
        let publisher = BroadwebdProfileSyncPublisher::new(&publisher_daemon);
        let mismatched_object_id = publisher
            .put_retained_object(
                DEFAULT_PROFILE_ID,
                signed_membership_record_bytes(&signer_b, &enroll_b),
            )
            .expect("put mismatched signed membership record object");
        let log = ProfileSyncMembershipLog {
            profile: DEFAULT_PROFILE_ID.to_string(),
            schema_version: super::PROFILE_SYNC_MEMBERSHIP_LOG_SCHEMA_VERSION,
            records: vec![ProfileSyncMembershipLogEntry {
                record_id: enroll_a.record_id.clone(),
                root_id: sync_membership_record_root_id(enroll_a.record_id.as_str()),
                object_id: mismatched_object_id,
                membership_epoch: enroll_a.membership_epoch,
                record_kind: enroll_a.record_kind.clone(),
                device_id: enroll_a.device_id.clone(),
                signer_device_id: signer_a.device_id().to_string(),
            }],
        };
        publisher
            .put_retained_root(
                DEFAULT_PROFILE_ID,
                PROFILE_SYNC_MEMBERSHIP_LOG_ROOT_ID,
                serde_json::to_vec(&log).expect("encode mismatched membership log"),
            )
            .expect("publish mismatched membership log");

        let error = BroadwebdProfileSyncObjectSource::new(&receiver_daemon)
            .pull_and_apply_sync_account_membership_log_if_changed(
                &receiver_database,
                DEFAULT_PROFILE_ID,
                PROFILE_SYNC_MEMBERSHIP_LOG_ROOT_ID,
            )
            .expect_err("mismatched membership log object should be rejected");
        assert!(matches!(
            error,
            ProfileSyncReceiveError::InvalidMembershipLog(reason)
                if reason.contains("does not match its signed membership record")
        ));
        assert!(
            receiver_database
                .sync_device_public_key(DEFAULT_PROFILE_ID, "membership-log-mismatch-a")
                .expect("read receiver key a")
                .is_none()
        );
        assert!(
            receiver_database
                .profile_sync_root(DEFAULT_PROFILE_ID, PROFILE_SYNC_MEMBERSHIP_LOG_ROOT_ID)
                .expect("read membership log root")
                .is_none()
        );

        let _ = std::fs::remove_dir_all(publisher_state_root);
        let _ = std::fs::remove_dir_all(receiver_state_root);
        let _ = std::fs::remove_dir_all(receiver_db_root);
    }

    #[test]
    fn broadwebd_membership_log_rejects_stale_epoch_records_without_loopback() {
        let network = InProcessBroadwebNetwork::new();
        let publisher_state_root = test_state_root("membership-log-stale-publisher");
        let receiver_state_root = test_state_root("membership-log-stale-receiver");
        let receiver_db_root = test_state_root("membership-log-stale-receiver-db");
        let publisher_daemon = network
            .daemon_for_device(
                &publisher_state_root,
                ResourceBudget::default(),
                "membership-log-stale-publisher",
            )
            .expect("start in-process stale membership log publisher daemon");
        let receiver_daemon = network
            .daemon_for_device(
                &receiver_state_root,
                ResourceBudget::default(),
                "membership-log-stale-receiver",
            )
            .expect("start in-process stale membership log receiver daemon");
        let receiver_database =
            SlateProfileDatabase::open_resolved(receiver_db_root.join(DEFAULT_DATABASE_FILE_NAME))
                .expect("open stale membership log receiver database");
        let signer_a = ProfileSyncDeviceSigner::generate("membership-log-stale-a")
            .expect("generate stale signer a");
        let signer_b = ProfileSyncDeviceSigner::generate("membership-log-stale-b")
            .expect("generate stale signer b");
        let enroll_a = ProfileSyncMembershipRecord {
            profile: DEFAULT_PROFILE_ID.to_string(),
            record_id: "epoch-1-enroll-membership-log-stale-a".to_string(),
            schema_version: PROFILE_SYNC_MEMBERSHIP_RECORD_SCHEMA_VERSION,
            membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            record_kind: PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_ENROLL_DEVICE.to_string(),
            device_id: "membership-log-stale-a".to_string(),
            device_public_key: Some(signer_a.public_key().expect("read signer a public key")),
            created_at: 10,
        };
        receiver_database
            .apply_signed_sync_account_membership_record(
                signed_membership_record_bytes(&signer_a, &enroll_a).as_slice(),
            )
            .expect("bootstrap stale log receiver signer a");
        let enroll_b = ProfileSyncMembershipRecord {
            profile: DEFAULT_PROFILE_ID.to_string(),
            record_id: "epoch-2-enroll-membership-log-stale-b".to_string(),
            schema_version: PROFILE_SYNC_MEMBERSHIP_RECORD_SCHEMA_VERSION,
            membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH + 1,
            record_kind: PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_ENROLL_DEVICE.to_string(),
            device_id: "membership-log-stale-b".to_string(),
            device_public_key: Some(signer_b.public_key().expect("read signer b public key")),
            created_at: 20,
        };
        receiver_database
            .apply_signed_sync_account_membership_record(
                signed_membership_record_bytes(&signer_a, &enroll_b).as_slice(),
            )
            .expect("apply stale log receiver signer b enrollment");
        let revoke_b = ProfileSyncMembershipRecord {
            profile: DEFAULT_PROFILE_ID.to_string(),
            record_id: "epoch-3-revoke-membership-log-stale-b".to_string(),
            schema_version: PROFILE_SYNC_MEMBERSHIP_RECORD_SCHEMA_VERSION,
            membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH + 2,
            record_kind: PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_REVOKE_DEVICE.to_string(),
            device_id: "membership-log-stale-b".to_string(),
            device_public_key: None,
            created_at: 30,
        };
        receiver_database
            .apply_signed_sync_account_membership_record(
                signed_membership_record_bytes(&signer_a, &revoke_b).as_slice(),
            )
            .expect("apply stale log receiver signer b revocation");

        let replacement_b = ProfileSyncDeviceSigner::generate("membership-log-stale-b")
            .expect("generate stale replacement signer b");
        let stale_rotate_b = ProfileSyncMembershipRecord {
            profile: DEFAULT_PROFILE_ID.to_string(),
            record_id: "epoch-2-rotate-membership-log-stale-b".to_string(),
            schema_version: PROFILE_SYNC_MEMBERSHIP_RECORD_SCHEMA_VERSION,
            membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH + 1,
            record_kind: PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_ROTATE_DEVICE_KEY.to_string(),
            device_id: "membership-log-stale-b".to_string(),
            device_public_key: Some(
                replacement_b
                    .public_key()
                    .expect("read replacement signer b public key"),
            ),
            created_at: 40,
        };
        let publisher = BroadwebdProfileSyncPublisher::new(&publisher_daemon);
        let stale_object_id = publisher
            .put_retained_object(
                DEFAULT_PROFILE_ID,
                signed_membership_record_bytes(&signer_a, &stale_rotate_b),
            )
            .expect("put stale signed membership record object");
        let log = ProfileSyncMembershipLog {
            profile: DEFAULT_PROFILE_ID.to_string(),
            schema_version: super::PROFILE_SYNC_MEMBERSHIP_LOG_SCHEMA_VERSION,
            records: vec![ProfileSyncMembershipLogEntry {
                record_id: stale_rotate_b.record_id.clone(),
                root_id: sync_membership_record_root_id(stale_rotate_b.record_id.as_str()),
                object_id: stale_object_id,
                membership_epoch: stale_rotate_b.membership_epoch,
                record_kind: stale_rotate_b.record_kind.clone(),
                device_id: stale_rotate_b.device_id.clone(),
                signer_device_id: signer_a.device_id().to_string(),
            }],
        };
        publisher
            .put_retained_root(
                DEFAULT_PROFILE_ID,
                PROFILE_SYNC_MEMBERSHIP_LOG_ROOT_ID,
                serde_json::to_vec(&log).expect("encode stale membership log"),
            )
            .expect("publish stale membership log");

        let error = BroadwebdProfileSyncObjectSource::new(&receiver_daemon)
            .pull_and_apply_sync_account_membership_log_if_changed(
                &receiver_database,
                DEFAULT_PROFILE_ID,
                PROFILE_SYNC_MEMBERSHIP_LOG_ROOT_ID,
            )
            .expect_err("stale membership log record should be rejected");
        assert!(matches!(
            error,
            ProfileSyncReceiveError::Storage(StorageError::InvalidProfileSyncMembershipRecord(reason))
                if reason.contains("older than latest applied epoch 3")
        ));
        assert!(
            receiver_database
                .sync_account_membership_record(
                    DEFAULT_PROFILE_ID,
                    "epoch-2-rotate-membership-log-stale-b",
                )
                .expect("read stale log record")
                .is_none()
        );
        assert!(
            !receiver_database
                .sync_device_public_key(DEFAULT_PROFILE_ID, "membership-log-stale-b")
                .expect("read stale log signer b key")
                .expect("stale log signer b key")
                .trusted
        );
        assert!(
            receiver_database
                .profile_sync_root(DEFAULT_PROFILE_ID, PROFILE_SYNC_MEMBERSHIP_LOG_ROOT_ID)
                .expect("read stale membership log root")
                .is_none()
        );

        let _ = std::fs::remove_dir_all(publisher_state_root);
        let _ = std::fs::remove_dir_all(receiver_state_root);
        let _ = std::fs::remove_dir_all(receiver_db_root);
    }

    #[test]
    fn broadwebd_membership_log_rejects_oversized_indexes_without_loopback() {
        let network = InProcessBroadwebNetwork::new();
        let publisher_state_root = test_state_root("membership-log-oversized-publisher");
        let receiver_state_root = test_state_root("membership-log-oversized-receiver");
        let receiver_db_root = test_state_root("membership-log-oversized-receiver-db");
        let publisher_daemon = network
            .daemon_for_device(
                &publisher_state_root,
                ResourceBudget::default(),
                "membership-log-oversized-publisher",
            )
            .expect("start in-process oversized membership log publisher daemon");
        let receiver_daemon = network
            .daemon_for_device(
                &receiver_state_root,
                ResourceBudget::default(),
                "membership-log-oversized-receiver",
            )
            .expect("start in-process oversized membership log receiver daemon");
        let receiver_database =
            SlateProfileDatabase::open_resolved(receiver_db_root.join(DEFAULT_DATABASE_FILE_NAME))
                .expect("open oversized membership log receiver database");
        let records = (0..=super::PROFILE_SYNC_MEMBERSHIP_LOG_MAX_RECORDS)
            .map(|index| {
                let record_id = format!("epoch-1-enroll-oversized-{index}");
                ProfileSyncMembershipLogEntry {
                    record_id: record_id.clone(),
                    root_id: sync_membership_record_root_id(record_id.as_str()),
                    object_id: format!("missing-membership-object-{index}"),
                    membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
                    record_kind: PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_ENROLL_DEVICE.to_string(),
                    device_id: format!("oversized-device-{index}"),
                    signer_device_id: "oversized-signer".to_string(),
                }
            })
            .collect::<Vec<_>>();
        let log = ProfileSyncMembershipLog {
            profile: DEFAULT_PROFILE_ID.to_string(),
            schema_version: super::PROFILE_SYNC_MEMBERSHIP_LOG_SCHEMA_VERSION,
            records,
        };
        BroadwebdProfileSyncPublisher::new(&publisher_daemon)
            .put_retained_root(
                DEFAULT_PROFILE_ID,
                PROFILE_SYNC_MEMBERSHIP_LOG_ROOT_ID,
                serde_json::to_vec(&log).expect("encode oversized membership log"),
            )
            .expect("publish oversized membership log");

        let error = BroadwebdProfileSyncObjectSource::new(&receiver_daemon)
            .pull_and_apply_sync_account_membership_log_if_changed(
                &receiver_database,
                DEFAULT_PROFILE_ID,
                PROFILE_SYNC_MEMBERSHIP_LOG_ROOT_ID,
            )
            .expect_err("oversized membership log should be rejected");
        assert!(matches!(
            error,
            ProfileSyncReceiveError::InvalidMembershipLog(reason)
                if reason.contains("too many records")
        ));
        assert!(
            receiver_database
                .profile_sync_root(DEFAULT_PROFILE_ID, PROFILE_SYNC_MEMBERSHIP_LOG_ROOT_ID)
                .expect("read oversized membership log root")
                .is_none()
        );

        let _ = std::fs::remove_dir_all(publisher_state_root);
        let _ = std::fs::remove_dir_all(receiver_state_root);
        let _ = std::fs::remove_dir_all(receiver_db_root);
    }

    #[test]
    fn broadwebd_membership_log_publisher_rejects_oversized_local_history_without_loopback() {
        let network = InProcessBroadwebNetwork::new();
        let publisher_state_root = test_state_root("membership-log-publish-oversized-publisher");
        let publisher_db_root = test_state_root("membership-log-publish-oversized-db");
        let publisher_daemon = network
            .daemon_for_device(
                &publisher_state_root,
                ResourceBudget::default(),
                "membership-log-publish-oversized-publisher",
            )
            .expect("start in-process oversized membership log publisher daemon");
        let publisher_database =
            SlateProfileDatabase::open_resolved(publisher_db_root.join(DEFAULT_DATABASE_FILE_NAME))
                .expect("open oversized membership log publisher database");
        let signer = ProfileSyncDeviceSigner::generate("membership-log-publish-oversized-signer")
            .expect("generate oversized membership log signer");
        let signer_public_key = signer.public_key().expect("read signer public key");
        for index in 0..=super::PROFILE_SYNC_MEMBERSHIP_LOG_MAX_RECORDS {
            let record = ProfileSyncMembershipRecord {
                profile: DEFAULT_PROFILE_ID.to_string(),
                record_id: format!("epoch-1-enroll-publish-oversized-{index:03}"),
                schema_version: PROFILE_SYNC_MEMBERSHIP_RECORD_SCHEMA_VERSION,
                membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
                record_kind: PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_ENROLL_DEVICE.to_string(),
                device_id: signer.device_id().to_string(),
                device_public_key: Some(signer_public_key.clone()),
                created_at: index as i64,
            };
            publisher_database
                .record_signed_sync_account_membership_record(
                    signed_membership_record_bytes(&signer, &record).as_slice(),
                )
                .expect("record oversized membership history entry");
        }

        let publisher = BroadwebdProfileSyncPublisher::new(&publisher_daemon);
        let plan = publisher
            .plan_local_sync_account_membership_log(
                &publisher_database,
                DEFAULT_PROFILE_ID,
                PROFILE_SYNC_MEMBERSHIP_LOG_ROOT_ID,
            )
            .expect("plan oversized local membership history");
        assert_eq!(
            plan.status,
            ProfileSyncMembershipLogPublicationPlanStatus::TooLarge
        );
        assert!(plan.requires_compaction());
        assert!(!plan.is_publishable());
        assert_eq!(
            plan.record_count,
            super::PROFILE_SYNC_MEMBERSHIP_LOG_MAX_RECORDS + 1
        );
        assert_eq!(
            plan.max_records,
            super::PROFILE_SYNC_MEMBERSHIP_LOG_MAX_RECORDS
        );
        assert_eq!(
            BroadwebdProfileSyncObjectSource::new(&publisher_daemon)
                .resolve_profile_sync_root(DEFAULT_PROFILE_ID, PROFILE_SYNC_MEMBERSHIP_LOG_ROOT_ID)
                .expect("read planned oversized publisher membership log root"),
            None
        );
        assert_eq!(
            BroadwebdProfileSyncObjectSource::new(&publisher_daemon)
                .resolve_profile_sync_root(
                    DEFAULT_PROFILE_ID,
                    sync_membership_record_root_id("epoch-1-enroll-publish-oversized-000").as_str(),
                )
                .expect("read planned oversized publisher first membership record root"),
            None
        );

        let runner_error = BroadwebdSettingsSyncRunner::new(&publisher_daemon)
            .run_settings_sync_cycle_with_membership_log(
                &publisher_database,
                DEFAULT_PROFILE_ID,
                "settings/latest",
                PROFILE_SYNC_MEMBERSHIP_LOG_ROOT_ID,
                &ProfileSyncContentKey::from_bytes([90; PROFILE_SYNC_CONTENT_KEY_BYTES]),
                TEST_CONTENT_KEY_ID,
                &signer,
                ProfileSyncRetentionPolicy::default(),
                4,
                4,
            )
            .expect_err("runner should reject oversized local membership history before sync");
        assert!(matches!(
            runner_error,
            ProfileSyncCycleError::Publish(ProfileSyncPublishError::MembershipLogTooLarge {
                max_records,
                actual_records,
                ..
            }) if max_records == super::PROFILE_SYNC_MEMBERSHIP_LOG_MAX_RECORDS
                && actual_records == super::PROFILE_SYNC_MEMBERSHIP_LOG_MAX_RECORDS + 1
        ));
        assert_eq!(
            BroadwebdProfileSyncObjectSource::new(&publisher_daemon)
                .resolve_profile_sync_root(DEFAULT_PROFILE_ID, PROFILE_SYNC_MEMBERSHIP_LOG_ROOT_ID)
                .expect("read runner-rejected oversized publisher membership log root"),
            None
        );

        let scheduler_error = BroadwebdSettingsSyncScheduler::new(&publisher_daemon)
            .run_once_with_membership_log_selecting_retention_providers(
                &publisher_database,
                &SettingsSyncSchedulerConfig::new(
                    DEFAULT_PROFILE_ID,
                    "settings/latest",
                    SettingsSyncCyclePolicy::new(ProfileSyncRetentionPolicy::default(), 4, 4, 1),
                ),
                PROFILE_SYNC_MEMBERSHIP_LOG_ROOT_ID,
                SettingsSyncRuntimeSecrets::new(
                    &ProfileSyncContentKey::from_bytes([91; PROFILE_SYNC_CONTENT_KEY_BYTES]),
                    &signer,
                ),
                &[],
            )
            .expect_err("scheduler should reject oversized local membership history before sync");
        assert!(matches!(
            scheduler_error,
            ProfileSyncCycleWithHealthError::Cycle(ProfileSyncCycleError::Publish(
                ProfileSyncPublishError::MembershipLogTooLarge {
                    max_records,
                    actual_records,
                    ..
                }
            )) if max_records == super::PROFILE_SYNC_MEMBERSHIP_LOG_MAX_RECORDS
                && actual_records == super::PROFILE_SYNC_MEMBERSHIP_LOG_MAX_RECORDS + 1
        ));
        assert_eq!(
            BroadwebdProfileSyncObjectSource::new(&publisher_daemon)
                .resolve_profile_sync_root(DEFAULT_PROFILE_ID, PROFILE_SYNC_MEMBERSHIP_LOG_ROOT_ID)
                .expect("read scheduler-rejected oversized publisher membership log root"),
            None
        );

        let error = publisher
            .publish_local_sync_account_membership_log(
                &publisher_database,
                DEFAULT_PROFILE_ID,
                PROFILE_SYNC_MEMBERSHIP_LOG_ROOT_ID,
            )
            .expect_err("publisher should reject oversized local membership history");
        assert!(matches!(
            error,
            ProfileSyncPublishError::MembershipLogTooLarge {
                max_records,
                actual_records,
                ..
            } if max_records == super::PROFILE_SYNC_MEMBERSHIP_LOG_MAX_RECORDS
                && actual_records == super::PROFILE_SYNC_MEMBERSHIP_LOG_MAX_RECORDS + 1
        ));
        assert_eq!(
            BroadwebdProfileSyncObjectSource::new(&publisher_daemon)
                .resolve_profile_sync_root(DEFAULT_PROFILE_ID, PROFILE_SYNC_MEMBERSHIP_LOG_ROOT_ID)
                .expect("read oversized publisher membership log root"),
            None
        );
        assert_eq!(
            BroadwebdProfileSyncObjectSource::new(&publisher_daemon)
                .resolve_profile_sync_root(
                    DEFAULT_PROFILE_ID,
                    sync_membership_record_root_id("epoch-1-enroll-publish-oversized-000").as_str(),
                )
                .expect("read oversized publisher first membership record root"),
            None
        );

        let _ = std::fs::remove_dir_all(publisher_state_root);
        let _ = std::fs::remove_dir_all(publisher_db_root);
    }

    #[test]
    fn settings_cycle_pulls_membership_log_before_trusted_settings_heads_without_loopback() {
        let network = InProcessBroadwebNetwork::new();
        let first_state_root = test_state_root("membership-cycle-first");
        let second_state_root = test_state_root("membership-cycle-second");
        let first_db_root = test_state_root("membership-cycle-first-db");
        let second_db_root = test_state_root("membership-cycle-second-db");
        let first_daemon = network
            .daemon_for_device(
                &first_state_root,
                ResourceBudget::default(),
                "membership-cycle-device-a",
            )
            .expect("start first membership cycle daemon");
        let second_daemon = network
            .daemon_for_device(
                &second_state_root,
                ResourceBudget::default(),
                "membership-cycle-device-b",
            )
            .expect("start second membership cycle daemon");
        let first_database = SlateProfileDatabase::open_resolved_with_device_id(
            first_db_root.join(DEFAULT_DATABASE_FILE_NAME),
            "membership-cycle-device-a",
        )
        .expect("open first membership cycle database");
        let second_database = SlateProfileDatabase::open_resolved_with_device_id(
            second_db_root.join(DEFAULT_DATABASE_FILE_NAME),
            "membership-cycle-device-b",
        )
        .expect("open second membership cycle database");
        let content_key = ProfileSyncContentKey::from_bytes([76; PROFILE_SYNC_CONTENT_KEY_BYTES]);
        let signer_a = ProfileSyncDeviceSigner::generate("membership-cycle-device-a")
            .expect("generate first membership cycle signer");
        let signer_b = ProfileSyncDeviceSigner::generate("membership-cycle-device-b")
            .expect("generate second membership cycle signer");
        register_test_content_key_epoch(&first_database, DEFAULT_PROFILE_ID);
        register_test_content_key_epoch(&second_database, DEFAULT_PROFILE_ID);

        let enroll_a = ProfileSyncMembershipRecord {
            profile: DEFAULT_PROFILE_ID.to_string(),
            record_id: "epoch-1-enroll-membership-cycle-device-a".to_string(),
            schema_version: PROFILE_SYNC_MEMBERSHIP_RECORD_SCHEMA_VERSION,
            membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            record_kind: PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_ENROLL_DEVICE.to_string(),
            device_id: "membership-cycle-device-a".to_string(),
            device_public_key: Some(signer_a.public_key().expect("read signer a public key")),
            created_at: 10,
        };
        let signed_enroll_a = signed_membership_record_bytes(&signer_a, &enroll_a);
        first_database
            .apply_signed_sync_account_membership_record(signed_enroll_a.as_slice())
            .expect("first device bootstraps signer a membership");
        let enroll_b = ProfileSyncMembershipRecord {
            profile: DEFAULT_PROFILE_ID.to_string(),
            record_id: "epoch-2-enroll-membership-cycle-device-b".to_string(),
            schema_version: PROFILE_SYNC_MEMBERSHIP_RECORD_SCHEMA_VERSION,
            membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH + 1,
            record_kind: PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_ENROLL_DEVICE.to_string(),
            device_id: "membership-cycle-device-b".to_string(),
            device_public_key: Some(signer_b.public_key().expect("read signer b public key")),
            created_at: 20,
        };
        let signed_enroll_b = signed_membership_record_bytes(&signer_a, &enroll_b);
        first_database
            .apply_signed_sync_account_membership_record(signed_enroll_b.as_slice())
            .expect("first device applies signer b membership");
        first_database
            .set_sync_setting_text(DEFAULT_PROFILE_ID, SYNC_DOMAIN_SETTINGS, "ui.theme", "teal")
            .expect("first device writes settings change");

        let first_run = BroadwebdSettingsSyncRunner::new(&first_daemon)
            .run_settings_sync_cycle_with_membership_log(
                &first_database,
                DEFAULT_PROFILE_ID,
                "settings/latest",
                PROFILE_SYNC_MEMBERSHIP_LOG_ROOT_ID,
                &content_key,
                TEST_CONTENT_KEY_ID,
                &signer_a,
                ProfileSyncRetentionPolicy::default(),
                4,
                4,
            )
            .expect("first device publishes settings and membership log");
        assert!(matches!(
            first_run.pulled_membership_log,
            ProfileSyncMembershipLogPullStatus::NoPublishedRoot { .. }
        ));
        assert_eq!(first_run.cycle.published_step_count(), 1);
        assert!(first_run.published_membership_log.is_some());
        assert!(
            second_database
                .sync_device_public_key(DEFAULT_PROFILE_ID, "membership-cycle-device-b")
                .expect("read second local key before membership pull")
                .is_none()
        );

        let second_run = BroadwebdSettingsSyncRunner::new(&second_daemon)
            .run_settings_sync_cycle_with_membership_log(
                &second_database,
                DEFAULT_PROFILE_ID,
                "settings/latest",
                PROFILE_SYNC_MEMBERSHIP_LOG_ROOT_ID,
                &content_key,
                TEST_CONTENT_KEY_ID,
                &signer_b,
                ProfileSyncRetentionPolicy::default(),
                4,
                4,
            )
            .expect("second device pulls membership before trusted settings heads");
        assert_eq!(second_run.pulled_membership_application_count(), 2);
        assert_eq!(second_run.cycle.applied_count(), 1);
        assert!(second_run.published_membership_log.is_some());
        assert_eq!(
            second_database
                .get_setting_text("ui.theme")
                .expect("read synced setting")
                .as_deref(),
            Some("teal")
        );
        assert!(
            second_database
                .sync_device_public_key(DEFAULT_PROFILE_ID, "membership-cycle-device-a")
                .expect("read second trusted key a")
                .expect("second device trusts key a")
                .trusted
        );
        assert!(
            second_database
                .sync_device_public_key(DEFAULT_PROFILE_ID, "membership-cycle-device-b")
                .expect("read second trusted key b")
                .expect("second device trusts key b")
                .trusted
        );

        let _ = std::fs::remove_dir_all(first_state_root);
        let _ = std::fs::remove_dir_all(second_state_root);
        let _ = std::fs::remove_dir_all(first_db_root);
        let _ = std::fs::remove_dir_all(second_db_root);
    }

    #[test]
    fn broadwebd_publisher_retains_dependencies_before_publishing_root() {
        let network = InProcessBroadwebNetwork::new();
        let state_root = test_state_root("broadwebd-batch-publish");
        let daemon = network
            .daemon_for_device(&state_root, ResourceBudget::default(), "runtime-b")
            .expect("start in-process profile-sync daemon");
        let publisher = BroadwebdProfileSyncPublisher::new(&daemon);
        let source = BroadwebdProfileSyncObjectSource::new(&daemon);

        let publication = publisher
            .put_retained_root_with_dependencies(
                "default",
                "settings/latest",
                vec![
                    b"encrypted dependency a".to_vec(),
                    b"encrypted dependency b".to_vec(),
                ],
                b"encrypted manifest root".to_vec(),
            )
            .expect("publish retained root object set");

        assert_eq!(publication.profile, "default");
        assert_eq!(publication.root_id, "settings/latest");
        assert_eq!(publication.dependency_object_ids.len(), 2);
        assert_eq!(
            source
                .resolve_profile_sync_root("default", "settings/latest")
                .expect("resolve published root")
                .as_deref(),
            Some(publication.root_object_id.as_str())
        );

        let root_object = source
            .get_profile_sync_object("default", publication.root_object_id.as_str())
            .expect("fetch published root object");
        assert_eq!(root_object.bytes, b"encrypted manifest root".to_vec());
        assert_retained(&publisher, &publication.root_object_id);
        for object_id in &publication.dependency_object_ids {
            assert_retained(&publisher, object_id);
            let object = source
                .get_profile_sync_object("default", object_id)
                .expect("fetch dependency object");
            assert!(object.bytes.starts_with(b"encrypted dependency "));
        }

        let _ = std::fs::remove_dir_all(state_root);
    }

    #[test]
    fn broadwebd_publisher_publishes_signed_settings_tail_manifest() {
        let network = InProcessBroadwebNetwork::new();
        let state_root = test_state_root("signed-tail-publish");
        let db_root = test_state_root("signed-tail-db");
        let daemon = network
            .daemon_for_device(&state_root, ResourceBudget::default(), "runtime-c")
            .expect("start in-process profile-sync daemon");
        let database = SlateProfileDatabase::open_resolved_with_device_id(
            db_root.join(DEFAULT_DATABASE_FILE_NAME),
            "runtime-c",
        )
        .expect("open local settings database");
        let change = database
            .set_sync_setting_text(DEFAULT_PROFILE_ID, SYNC_DOMAIN_SETTINGS, "ui.theme", "teal")
            .expect("write local sync setting");
        let content_key = ProfileSyncContentKey::from_bytes([41; PROFILE_SYNC_CONTENT_KEY_BYTES]);
        let signer = ProfileSyncDeviceSigner::generate("runtime-c").expect("generate signer");
        let public_key = signer.public_key().expect("read signer public key");
        let publisher = BroadwebdProfileSyncPublisher::new(&daemon);

        let publication = publisher
            .publish_signed_settings_tail_changes(
                DEFAULT_PROFILE_ID,
                "settings/latest",
                std::slice::from_ref(&change),
                &content_key,
                TEST_CONTENT_KEY_ID,
                &signer,
                ProfileSyncRetentionPolicy::default(),
            )
            .expect("publish signed settings tail manifest");

        assert_eq!(publication.manifest.profile, DEFAULT_PROFILE_ID);
        assert_eq!(publication.manifest.root_id, "settings/latest");
        assert_eq!(publication.tail_change_object_ids.len(), 1);
        assert_eq!(
            publication.manifest.tail_change_object_ids,
            publication.tail_change_object_ids
        );
        assert_eq!(publication.manifest.current_snapshot_object_id, None);

        let source = BroadwebdProfileSyncObjectSource::new(&daemon);
        assert_eq!(
            source
                .resolve_profile_sync_root(DEFAULT_PROFILE_ID, "settings/latest")
                .expect("resolve signed manifest root")
                .as_deref(),
            Some(publication.manifest_object_id.as_str())
        );
        let manifest_object = source
            .get_profile_sync_object(DEFAULT_PROFILE_ID, publication.manifest_object_id.as_str())
            .expect("fetch signed manifest object");
        let manifest = open_signed_profile_sync_manifest(
            manifest_object.bytes.as_slice(),
            &content_key,
            &public_key,
            DEFAULT_PROFILE_ID,
            TEST_CONTENT_KEY_ID,
        )
        .expect("verify signed manifest object");
        assert_eq!(manifest, publication.manifest);

        let tail_object_id = publication
            .tail_change_object_ids
            .first()
            .expect("tail object id");
        let tail_object = source
            .get_profile_sync_object(DEFAULT_PROFILE_ID, tail_object_id)
            .expect("fetch signed tail object");
        let incoming = open_signed_sync_setting_text(
            tail_object.bytes.as_slice(),
            &content_key,
            &public_key,
            DEFAULT_PROFILE_ID,
            SYNC_DOMAIN_SETTINGS,
            TEST_CONTENT_KEY_ID,
        )
        .expect("verify signed tail object");
        assert_eq!(incoming.key, "ui.theme");
        assert_eq!(incoming.value, "teal");
        assert_eq!(incoming.device_id, "runtime-c");

        let _ = std::fs::remove_dir_all(state_root);
        let _ = std::fs::remove_dir_all(db_root);
    }

    #[test]
    fn broadwebd_publisher_publishes_signed_profile_sync_device_head() {
        let network = InProcessBroadwebNetwork::new();
        let state_root = test_state_root("signed-device-head-publish");
        let db_root = test_state_root("signed-device-head-db");
        let daemon = network
            .daemon_for_device(&state_root, ResourceBudget::default(), "runtime-f")
            .expect("start in-process profile-sync daemon");
        let database = SlateProfileDatabase::open_resolved_with_device_id(
            db_root.join(DEFAULT_DATABASE_FILE_NAME),
            "runtime-f",
        )
        .expect("open local settings database");
        let change = database
            .set_sync_setting_text(DEFAULT_PROFILE_ID, SYNC_DOMAIN_SETTINGS, "ui.theme", "teal")
            .expect("write local sync setting");
        let content_key = ProfileSyncContentKey::from_bytes([44; PROFILE_SYNC_CONTENT_KEY_BYTES]);
        let signer = ProfileSyncDeviceSigner::generate("runtime-f").expect("generate signer");
        let public_key = signer.public_key().expect("read signer public key");
        let publisher = BroadwebdProfileSyncPublisher::new(&daemon);
        let manifest_publication = publisher
            .publish_signed_settings_tail_changes(
                DEFAULT_PROFILE_ID,
                "settings/latest",
                std::slice::from_ref(&change),
                &content_key,
                TEST_CONTENT_KEY_ID,
                &signer,
                ProfileSyncRetentionPolicy::default(),
            )
            .expect("publish signed settings manifest");
        let head_root_id = settings_device_head_root_id("runtime-f");
        let device_head = ProfileSyncDeviceHead {
            profile: DEFAULT_PROFILE_ID.to_string(),
            device_id: "runtime-f".to_string(),
            root_id: head_root_id.clone(),
            schema_version: PROFILE_SYNC_DEVICE_HEAD_SCHEMA_VERSION,
            membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            latest_manifest_object_id: manifest_publication.manifest_object_id.clone(),
            latest_change_object_id: manifest_publication.tail_change_object_ids.first().cloned(),
            device_sequence: change.device_sequence,
            logical_clock: change.logical_clock,
            created_at: change.created_at,
        };

        let head_publication = publisher
            .publish_signed_profile_sync_device_head(
                DEFAULT_PROFILE_ID,
                head_root_id.as_str(),
                &device_head,
                &content_key,
                TEST_CONTENT_KEY_ID,
                &signer,
            )
            .expect("publish signed device head");

        assert_eq!(head_publication.root_id, head_root_id);
        assert_eq!(head_publication.device_head, device_head);
        assert_retained(&publisher, head_publication.object_id.as_str());

        let source = BroadwebdProfileSyncObjectSource::new(&daemon);
        assert_eq!(
            source
                .resolve_profile_sync_root(DEFAULT_PROFILE_ID, head_publication.root_id.as_str())
                .expect("resolve signed device head root")
                .as_deref(),
            Some(head_publication.object_id.as_str())
        );
        let head_object = source
            .get_profile_sync_object(DEFAULT_PROFILE_ID, head_publication.object_id.as_str())
            .expect("fetch signed device head object");
        let decoded_head = open_signed_profile_sync_device_head(
            head_object.bytes.as_slice(),
            &content_key,
            &public_key,
            DEFAULT_PROFILE_ID,
            TEST_CONTENT_KEY_ID,
        )
        .expect("verify signed device head object");
        assert_eq!(decoded_head, device_head);
        let pulled_head = pull_signed_profile_sync_device_head(
            &source,
            DEFAULT_PROFILE_ID,
            head_publication.root_id.as_str(),
            &content_key,
            &public_key,
            TEST_CONTENT_KEY_ID,
        )
        .expect("pull signed device head")
        .expect("published device head");
        assert_eq!(pulled_head.object_id, head_publication.object_id);
        assert_eq!(pulled_head.device_head, device_head);

        let _ = std::fs::remove_dir_all(state_root);
        let _ = std::fs::remove_dir_all(db_root);
    }

    #[test]
    fn broadwebd_source_records_device_head_and_applies_referenced_manifest() {
        let network = InProcessBroadwebNetwork::new();
        let publisher_state_root = test_state_root("device-head-apply-publisher");
        let receiver_state_root = test_state_root("device-head-apply-receiver");
        let publisher_db_root = test_state_root("device-head-apply-publisher-db");
        let receiver_db_root = test_state_root("device-head-apply-receiver-db");
        let publisher_daemon = network
            .daemon_for_device(
                &publisher_state_root,
                ResourceBudget::default(),
                "runtime-g",
            )
            .expect("start publisher daemon");
        let receiver_daemon = network
            .daemon_for_device(&receiver_state_root, ResourceBudget::default(), "runtime-h")
            .expect("start receiver daemon");
        let publisher_database = SlateProfileDatabase::open_resolved_with_device_id(
            publisher_db_root.join(DEFAULT_DATABASE_FILE_NAME),
            "runtime-g",
        )
        .expect("open publisher settings database");
        let receiver_database = SlateProfileDatabase::open_resolved_with_device_id(
            receiver_db_root.join(DEFAULT_DATABASE_FILE_NAME),
            "runtime-h",
        )
        .expect("open receiver settings database");
        let change = publisher_database
            .set_sync_setting_text(DEFAULT_PROFILE_ID, SYNC_DOMAIN_SETTINGS, "ui.theme", "teal")
            .expect("publisher writes sync setting");
        let content_key = ProfileSyncContentKey::from_bytes([45; PROFILE_SYNC_CONTENT_KEY_BYTES]);
        let signer = ProfileSyncDeviceSigner::generate("runtime-g").expect("generate signer");
        let public_key = signer.public_key().expect("read signer public key");
        receiver_database
            .register_sync_device_public_key(&SyncDevicePublicKeyRegistration {
                profile: DEFAULT_PROFILE_ID.to_string(),
                public_key,
                membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            })
            .expect("receiver trusts publisher key");

        let publisher = BroadwebdProfileSyncPublisher::new(&publisher_daemon);
        let manifest_publication = publisher
            .publish_signed_settings_tail_changes(
                DEFAULT_PROFILE_ID,
                "settings/latest",
                std::slice::from_ref(&change),
                &content_key,
                TEST_CONTENT_KEY_ID,
                &signer,
                ProfileSyncRetentionPolicy::default(),
            )
            .expect("publish manifest");
        let head_root_id = settings_device_head_root_id("runtime-g");
        let device_head = ProfileSyncDeviceHead {
            profile: DEFAULT_PROFILE_ID.to_string(),
            device_id: "runtime-g".to_string(),
            root_id: head_root_id.clone(),
            schema_version: PROFILE_SYNC_DEVICE_HEAD_SCHEMA_VERSION,
            membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            latest_manifest_object_id: manifest_publication.manifest_object_id.clone(),
            latest_change_object_id: manifest_publication.tail_change_object_ids.first().cloned(),
            device_sequence: change.device_sequence,
            logical_clock: change.logical_clock,
            created_at: change.created_at,
        };
        let head_publication = publisher
            .publish_signed_profile_sync_device_head(
                DEFAULT_PROFILE_ID,
                head_root_id.as_str(),
                &device_head,
                &content_key,
                TEST_CONTENT_KEY_ID,
                &signer,
            )
            .expect("publish device head");

        let source = BroadwebdProfileSyncObjectSource::new(&receiver_daemon);
        let applied = source
            .pull_record_and_apply_trusted_settings_from_device_head(
                &receiver_database,
                DEFAULT_PROFILE_ID,
                head_root_id.as_str(),
                &content_key,
                TEST_CONTENT_KEY_ID,
            )
            .expect("pull and apply trusted device head");
        let BroadwebdTrustedDeviceHeadSyncStatus::Applied {
            device_head: applied_head,
            root,
            application,
        } = applied
        else {
            panic!("expected applied trusted device head, got {applied:?}");
        };
        assert_eq!(applied_head.object_id, head_publication.object_id);
        assert_eq!(applied_head.device_head, device_head);
        assert_eq!(root.object_id, head_publication.object_id);
        assert_eq!(
            application.manifest_object_id,
            manifest_publication.manifest_object_id
        );
        assert_eq!(
            receiver_database
                .get_setting_text("ui.theme")
                .expect("read synced receiver setting")
                .as_deref(),
            Some("teal")
        );
        assert_eq!(
            receiver_database
                .profile_sync_root(DEFAULT_PROFILE_ID, head_root_id.as_str())
                .expect("read stored device head root")
                .expect("stored device head root")
                .object_id,
            head_publication.object_id
        );

        let unchanged = source
            .pull_record_and_apply_trusted_settings_from_device_head(
                &receiver_database,
                DEFAULT_PROFILE_ID,
                head_root_id.as_str(),
                &content_key,
                TEST_CONTENT_KEY_ID,
            )
            .expect("pull unchanged trusted device head");
        assert!(matches!(
            unchanged,
            BroadwebdTrustedDeviceHeadSyncStatus::Unchanged { object_id, .. }
                if object_id == head_publication.object_id
        ));

        let _ = std::fs::remove_dir_all(publisher_state_root);
        let _ = std::fs::remove_dir_all(receiver_state_root);
        let _ = std::fs::remove_dir_all(publisher_db_root);
        let _ = std::fs::remove_dir_all(receiver_db_root);
    }

    #[test]
    fn broadwebd_source_applies_competing_trusted_settings_manifest_candidates() {
        let network = InProcessBroadwebNetwork::new();
        let first_state_root = test_state_root("candidate-bridge-first");
        let second_state_root = test_state_root("candidate-bridge-second");
        let receiver_state_root = test_state_root("candidate-bridge-receiver");
        let first_db_root = test_state_root("candidate-bridge-first-db");
        let second_db_root = test_state_root("candidate-bridge-second-db");
        let receiver_db_root = test_state_root("candidate-bridge-receiver-db");
        let first_daemon = network
            .daemon_for_device(
                &first_state_root,
                ResourceBudget::default(),
                "runtime-candidate-a",
            )
            .expect("start first candidate publisher daemon");
        let second_daemon = network
            .daemon_for_device(
                &second_state_root,
                ResourceBudget::default(),
                "runtime-candidate-b",
            )
            .expect("start second candidate publisher daemon");
        let receiver_daemon = network
            .daemon_for_device(
                &receiver_state_root,
                ResourceBudget::default(),
                "runtime-candidate-c",
            )
            .expect("start candidate receiver daemon");
        let first_database = SlateProfileDatabase::open_resolved_with_device_id(
            first_db_root.join(DEFAULT_DATABASE_FILE_NAME),
            "runtime-candidate-a",
        )
        .expect("open first candidate settings database");
        let second_database = SlateProfileDatabase::open_resolved_with_device_id(
            second_db_root.join(DEFAULT_DATABASE_FILE_NAME),
            "runtime-candidate-b",
        )
        .expect("open second candidate settings database");
        let receiver_database = SlateProfileDatabase::open_resolved_with_device_id(
            receiver_db_root.join(DEFAULT_DATABASE_FILE_NAME),
            "runtime-candidate-c",
        )
        .expect("open receiver candidate settings database");
        let profile = "candidateprofile";
        let settings_root_id = "settings/latest";
        let content_key = ProfileSyncContentKey::from_bytes([62; PROFILE_SYNC_CONTENT_KEY_BYTES]);
        let first_signer =
            ProfileSyncDeviceSigner::generate("runtime-candidate-a").expect("first signer");
        let second_signer =
            ProfileSyncDeviceSigner::generate("runtime-candidate-b").expect("second signer");
        register_test_content_key_epoch(&receiver_database, profile);
        for public_key in [
            first_signer.public_key().expect("first public key"),
            second_signer.public_key().expect("second public key"),
        ] {
            receiver_database
                .register_sync_device_public_key(&SyncDevicePublicKeyRegistration {
                    profile: profile.to_string(),
                    public_key,
                    membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
                })
                .expect("receiver trusts candidate publisher key");
        }

        let first_change = first_database
            .set_sync_setting_text(profile, SYNC_DOMAIN_SETTINGS, "ui.theme", "alpha")
            .expect("first candidate writes setting");
        let first_publication = BroadwebdProfileSyncPublisher::new(&first_daemon)
            .publish_signed_settings_tail_changes(
                profile,
                settings_root_id,
                std::slice::from_ref(&first_change),
                &content_key,
                TEST_CONTENT_KEY_ID,
                &first_signer,
                ProfileSyncRetentionPolicy::default(),
            )
            .expect("first candidate publishes shared settings root");
        let second_change = second_database
            .set_sync_setting_text(profile, SYNC_DOMAIN_SETTINGS, "ui.theme", "bravo")
            .expect("second candidate writes setting");
        let second_publication = BroadwebdProfileSyncPublisher::new(&second_daemon)
            .publish_signed_settings_tail_changes(
                profile,
                settings_root_id,
                std::slice::from_ref(&second_change),
                &content_key,
                TEST_CONTENT_KEY_ID,
                &second_signer,
                ProfileSyncRetentionPolicy::default(),
            )
            .expect("second candidate publishes shared settings root");

        let source = BroadwebdProfileSyncObjectSource::new(&receiver_daemon);
        let status = source
            .pull_and_apply_active_trusted_settings_manifest_candidates_if_changed(
                &receiver_database,
                profile,
                settings_root_id,
                &content_key,
            )
            .expect("receiver applies competing broadwebd settings candidates");
        let ProfileSyncSettingsCandidatePullApplyStatus::Applied(applications) = status else {
            panic!("expected candidate applications, got {status:?}");
        };
        assert_eq!(
            applications
                .iter()
                .map(|application| application.application.manifest_object_id.as_str())
                .collect::<Vec<_>>(),
            vec![
                first_publication.manifest_object_id.as_str(),
                second_publication.manifest_object_id.as_str(),
            ],
            "candidate application should run oldest publication first"
        );
        assert_eq!(
            receiver_database
                .get_sync_setting_text(profile, SYNC_DOMAIN_SETTINGS, "ui.theme")
                .expect("read receiver candidate value")
                .expect("receiver candidate value")
                .value,
            "bravo"
        );
        assert_eq!(
            receiver_database
                .profile_sync_root(profile, settings_root_id)
                .expect("read receiver shared settings root")
                .expect("receiver shared settings root")
                .object_id,
            second_publication.manifest_object_id.as_str()
        );

        let unchanged = source
            .pull_and_apply_active_trusted_settings_manifest_candidates_if_changed(
                &receiver_database,
                profile,
                settings_root_id,
                &content_key,
            )
            .expect("receiver checks unchanged candidate roots");
        assert_eq!(
            unchanged,
            ProfileSyncSettingsCandidatePullApplyStatus::Unchanged {
                profile: profile.to_string(),
                root_id: settings_root_id.to_string(),
                object_id: second_publication.manifest_object_id.clone(),
            }
        );

        let _ = std::fs::remove_dir_all(first_state_root);
        let _ = std::fs::remove_dir_all(second_state_root);
        let _ = std::fs::remove_dir_all(receiver_state_root);
        let _ = std::fs::remove_dir_all(first_db_root);
        let _ = std::fs::remove_dir_all(second_db_root);
        let _ = std::fs::remove_dir_all(receiver_db_root);
    }

    #[test]
    fn broadwebd_source_pulls_registered_trusted_device_heads_with_device_bound() {
        let network = InProcessBroadwebNetwork::new();
        let publisher_state_root = test_state_root("trusted-devices-publisher");
        let receiver_state_root = test_state_root("trusted-devices-receiver");
        let publisher_db_root = test_state_root("trusted-devices-publisher-db");
        let receiver_db_root = test_state_root("trusted-devices-receiver-db");
        let publisher_daemon = network
            .daemon_for_device(
                &publisher_state_root,
                ResourceBudget::default(),
                "runtime-u",
            )
            .expect("start publisher daemon");
        let receiver_daemon = network
            .daemon_for_device(&receiver_state_root, ResourceBudget::default(), "runtime-v")
            .expect("start receiver daemon");
        let publisher_database = SlateProfileDatabase::open_resolved_with_device_id(
            publisher_db_root.join(DEFAULT_DATABASE_FILE_NAME),
            "runtime-u",
        )
        .expect("open publisher settings database");
        let receiver_database = SlateProfileDatabase::open_resolved_with_device_id(
            receiver_db_root.join(DEFAULT_DATABASE_FILE_NAME),
            "runtime-v",
        )
        .expect("open receiver settings database");
        publisher_database
            .set_sync_setting_text(DEFAULT_PROFILE_ID, SYNC_DOMAIN_SETTINGS, "ui.theme", "teal")
            .expect("publisher writes sync setting");
        let content_key = ProfileSyncContentKey::from_bytes([50; PROFILE_SYNC_CONTENT_KEY_BYTES]);
        let publisher_signer =
            ProfileSyncDeviceSigner::generate("runtime-u").expect("generate publisher signer");
        let receiver_signer =
            ProfileSyncDeviceSigner::generate("runtime-v").expect("generate receiver signer");
        let missing_signer =
            ProfileSyncDeviceSigner::generate("runtime-w").expect("generate missing signer");
        for public_key in [
            publisher_signer.public_key().expect("publisher public key"),
            receiver_signer.public_key().expect("receiver public key"),
            missing_signer.public_key().expect("missing public key"),
        ] {
            receiver_database
                .register_sync_device_public_key(&SyncDevicePublicKeyRegistration {
                    profile: DEFAULT_PROFILE_ID.to_string(),
                    public_key,
                    membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
                })
                .expect("receiver trusts device key");
        }
        let publisher = BroadwebdProfileSyncPublisher::new(&publisher_daemon);
        let published = publisher
            .publish_full_local_settings_snapshot_head(
                &publisher_database,
                DEFAULT_PROFILE_ID,
                "settings/latest",
                &content_key,
                TEST_CONTENT_KEY_ID,
                &publisher_signer,
                ProfileSyncRetentionPolicy::default(),
            )
            .expect("publish trusted device settings head")
            .expect("publisher has settings changes");

        let source = BroadwebdProfileSyncObjectSource::new(&receiver_daemon);
        let limit_error = source
            .pull_and_apply_trusted_settings_from_registered_devices(
                &receiver_database,
                DEFAULT_PROFILE_ID,
                &content_key,
                TEST_CONTENT_KEY_ID,
                1,
            )
            .expect_err("device bound should reject two remote trusted devices");
        assert!(matches!(
            limit_error,
            ProfileSyncReceiveError::TrustedDeviceLimitExceeded {
                trusted_device_count: 2,
                max_devices: 1,
                ..
            }
        ));

        let run = source
            .pull_and_apply_trusted_settings_from_registered_devices(
                &receiver_database,
                DEFAULT_PROFILE_ID,
                &content_key,
                TEST_CONTENT_KEY_ID,
                4,
            )
            .expect("pull registered trusted devices");
        assert_eq!(run.profile, DEFAULT_PROFILE_ID);
        assert_eq!(run.devices.len(), 2);
        assert_eq!(run.applied_count(), 1);
        assert!(
            run.devices
                .iter()
                .all(|device| device.device_id != receiver_database.local_sync_device_id())
        );

        let published_device = run
            .devices
            .iter()
            .find(|device| device.device_id == "runtime-u")
            .expect("published trusted device result");
        let BroadwebdTrustedDeviceHeadSyncStatus::Applied { application, .. } =
            &published_device.status
        else {
            panic!("expected published trusted device to apply: {published_device:?}");
        };
        assert_eq!(
            application.manifest_object_id,
            published.publication.manifest_object_id
        );

        let missing_device = run
            .devices
            .iter()
            .find(|device| device.device_id == "runtime-w")
            .expect("missing trusted device result");
        assert!(matches!(
            &missing_device.status,
            BroadwebdTrustedDeviceHeadSyncStatus::NoPublishedRoot { root_id, .. }
                if root_id == "settings/devices/runtime-w/head"
        ));
        assert_eq!(
            receiver_database
                .get_setting_text("ui.theme")
                .expect("read synced receiver setting")
                .as_deref(),
            Some("teal")
        );

        let unchanged = source
            .pull_and_apply_trusted_settings_from_registered_devices(
                &receiver_database,
                DEFAULT_PROFILE_ID,
                &content_key,
                TEST_CONTENT_KEY_ID,
                4,
            )
            .expect("pull registered trusted devices again");
        assert_eq!(unchanged.applied_count(), 0);
        assert!(unchanged.devices.iter().any(|device| matches!(
            &device.status,
            BroadwebdTrustedDeviceHeadSyncStatus::Unchanged { object_id, .. }
                if device.device_id == "runtime-u" && object_id == &published.device_head.object_id
        )));

        let _ = std::fs::remove_dir_all(publisher_state_root);
        let _ = std::fs::remove_dir_all(receiver_state_root);
        let _ = std::fs::remove_dir_all(publisher_db_root);
        let _ = std::fs::remove_dir_all(receiver_db_root);
    }

    #[test]
    fn broadwebd_settings_sync_runner_publishes_then_receives_in_one_cycle() {
        let network = InProcessBroadwebNetwork::new();
        let first_state_root = test_state_root("cycle-first");
        let second_state_root = test_state_root("cycle-second");
        let first_db_root = test_state_root("cycle-first-db");
        let second_db_root = test_state_root("cycle-second-db");
        let first_daemon = network
            .daemon_for_device(&first_state_root, ResourceBudget::default(), "runtime-x")
            .expect("start first daemon");
        let second_daemon = network
            .daemon_for_device(&second_state_root, ResourceBudget::default(), "runtime-y")
            .expect("start second daemon");
        let first_database = SlateProfileDatabase::open_resolved_with_device_id(
            first_db_root.join(DEFAULT_DATABASE_FILE_NAME),
            "runtime-x",
        )
        .expect("open first settings database");
        let second_database = SlateProfileDatabase::open_resolved_with_device_id(
            second_db_root.join(DEFAULT_DATABASE_FILE_NAME),
            "runtime-y",
        )
        .expect("open second settings database");
        let profile = "cycleprofile";
        let settings_root_id = "settings/latest";
        let content_key = ProfileSyncContentKey::from_bytes([51; PROFILE_SYNC_CONTENT_KEY_BYTES]);
        let first_signer =
            ProfileSyncDeviceSigner::generate("runtime-x").expect("generate first signer");
        let second_signer =
            ProfileSyncDeviceSigner::generate("runtime-y").expect("generate second signer");
        register_test_content_key_epoch(&first_database, profile);
        register_test_content_key_epoch(&second_database, profile);
        first_database
            .register_sync_device_public_key(&SyncDevicePublicKeyRegistration {
                profile: profile.to_string(),
                public_key: first_signer.public_key().expect("first self public key"),
                membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            })
            .expect("first device trusts itself for retained dependencies");
        first_database
            .register_sync_device_public_key(&SyncDevicePublicKeyRegistration {
                profile: profile.to_string(),
                public_key: second_signer.public_key().expect("second public key"),
                membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            })
            .expect("first device trusts second");
        second_database
            .register_sync_device_public_key(&SyncDevicePublicKeyRegistration {
                profile: profile.to_string(),
                public_key: second_signer.public_key().expect("second self public key"),
                membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            })
            .expect("second device trusts itself for retained dependencies");
        second_database
            .register_sync_device_public_key(&SyncDevicePublicKeyRegistration {
                profile: profile.to_string(),
                public_key: first_signer.public_key().expect("first public key"),
                membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            })
            .expect("second device trusts first");

        first_database
            .set_sync_setting_text(profile, SYNC_DOMAIN_SETTINGS, "ui.theme", "teal")
            .expect("first device writes local setting");
        let first_cycle = BroadwebdSettingsSyncRunner::new(&first_daemon)
            .run_settings_sync_cycle(
                &first_database,
                profile,
                settings_root_id,
                &content_key,
                TEST_CONTENT_KEY_ID,
                &first_signer,
                ProfileSyncRetentionPolicy::default(),
                4,
                4,
            )
            .expect("first device runs sync cycle");
        assert_eq!(first_cycle.published_step_count(), 1);
        assert_eq!(first_cycle.applied_count(), 0);

        let second_pull_cycle = BroadwebdSettingsSyncRunner::new(&second_daemon)
            .run_settings_sync_cycle(
                &second_database,
                profile,
                settings_root_id,
                &content_key,
                TEST_CONTENT_KEY_ID,
                &second_signer,
                ProfileSyncRetentionPolicy::default(),
                4,
                4,
            )
            .expect("second device runs sync cycle");
        assert_eq!(second_pull_cycle.published_step_count(), 0);
        assert_eq!(second_pull_cycle.applied_count(), 1);
        assert_eq!(
            second_database
                .get_sync_setting_text(profile, SYNC_DOMAIN_SETTINGS, "ui.theme")
                .expect("read second synced theme")
                .expect("second synced theme")
                .value,
            "teal"
        );

        second_database
            .set_sync_setting_text(profile, SYNC_DOMAIN_SETTINGS, "ui.zoom", "110")
            .expect("second device writes local setting");
        let second_publish_cycle = BroadwebdSettingsSyncRunner::new(&second_daemon)
            .run_settings_sync_cycle(
                &second_database,
                profile,
                settings_root_id,
                &content_key,
                TEST_CONTENT_KEY_ID,
                &second_signer,
                ProfileSyncRetentionPolicy::default(),
                4,
                4,
            )
            .expect("second device publishes sync cycle");
        assert_eq!(second_publish_cycle.published_step_count(), 1);
        assert_eq!(second_publish_cycle.applied_count(), 0);

        let first_pull_cycle = BroadwebdSettingsSyncRunner::new(&first_daemon)
            .run_settings_sync_cycle(
                &first_database,
                profile,
                settings_root_id,
                &content_key,
                TEST_CONTENT_KEY_ID,
                &first_signer,
                ProfileSyncRetentionPolicy::default(),
                4,
                4,
            )
            .expect("first device pulls second device cycle");
        assert_eq!(first_pull_cycle.published_step_count(), 0);
        assert_eq!(first_pull_cycle.applied_count(), 1);
        assert_eq!(
            first_database
                .get_sync_setting_text(profile, SYNC_DOMAIN_SETTINGS, "ui.zoom")
                .expect("read first synced zoom")
                .expect("first synced zoom")
                .value,
            "110"
        );

        let _ = std::fs::remove_dir_all(first_state_root);
        let _ = std::fs::remove_dir_all(second_state_root);
        let _ = std::fs::remove_dir_all(first_db_root);
        let _ = std::fs::remove_dir_all(second_db_root);
    }

    #[test]
    fn broadwebd_settings_sync_cycle_can_apply_shared_root_candidates() {
        let network = InProcessBroadwebNetwork::new();
        let first_state_root = test_state_root("cycle-candidate-first");
        let second_state_root = test_state_root("cycle-candidate-second");
        let receiver_state_root = test_state_root("cycle-candidate-receiver");
        let provider_state_root = test_state_root("cycle-candidate-provider");
        let first_db_root = test_state_root("cycle-candidate-first-db");
        let second_db_root = test_state_root("cycle-candidate-second-db");
        let receiver_db_root = test_state_root("cycle-candidate-receiver-db");
        let first_daemon = network
            .daemon_for_device(
                &first_state_root,
                ResourceBudget::default(),
                "runtime-cycle-candidate-a",
            )
            .expect("start first candidate publisher daemon");
        let second_daemon = network
            .daemon_for_device(
                &second_state_root,
                ResourceBudget::default(),
                "runtime-cycle-candidate-b",
            )
            .expect("start second candidate publisher daemon");
        let receiver_daemon = network
            .daemon_for_device(
                &receiver_state_root,
                ResourceBudget::default(),
                "runtime-cycle-candidate-c",
            )
            .expect("start candidate receiver daemon");
        let provider_daemon = network
            .daemon_for_availability_provider(
                &provider_state_root,
                ResourceBudget::default(),
                "runtime-cycle-candidate-pinner",
            )
            .expect("start candidate availability provider daemon");
        let first_database = SlateProfileDatabase::open_resolved_with_device_id(
            first_db_root.join(DEFAULT_DATABASE_FILE_NAME),
            "runtime-cycle-candidate-a",
        )
        .expect("open first candidate database");
        let second_database = SlateProfileDatabase::open_resolved_with_device_id(
            second_db_root.join(DEFAULT_DATABASE_FILE_NAME),
            "runtime-cycle-candidate-b",
        )
        .expect("open second candidate database");
        let receiver_database = SlateProfileDatabase::open_resolved_with_device_id(
            receiver_db_root.join(DEFAULT_DATABASE_FILE_NAME),
            "runtime-cycle-candidate-c",
        )
        .expect("open receiver candidate database");
        let profile = "cyclecandidateprofile";
        let settings_root_id = "settings/latest";
        let content_key = ProfileSyncContentKey::from_bytes([63; PROFILE_SYNC_CONTENT_KEY_BYTES]);
        let first_signer =
            ProfileSyncDeviceSigner::generate("runtime-cycle-candidate-a").expect("first signer");
        let second_signer =
            ProfileSyncDeviceSigner::generate("runtime-cycle-candidate-b").expect("second signer");
        let receiver_signer = ProfileSyncDeviceSigner::generate("runtime-cycle-candidate-c")
            .expect("receiver signer");
        register_test_content_key_epoch(&receiver_database, profile);
        for public_key in [
            first_signer.public_key().expect("first public key"),
            second_signer.public_key().expect("second public key"),
            receiver_signer.public_key().expect("receiver public key"),
        ] {
            receiver_database
                .register_sync_device_public_key(&SyncDevicePublicKeyRegistration {
                    profile: profile.to_string(),
                    public_key,
                    membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
                })
                .expect("receiver trusts sync device key");
        }

        let first_change = first_database
            .set_sync_setting_text(profile, SYNC_DOMAIN_SETTINGS, "ui.theme", "alpha")
            .expect("first candidate writes setting");
        let first_publication = BroadwebdProfileSyncPublisher::new(&first_daemon)
            .publish_signed_settings_tail_changes(
                profile,
                settings_root_id,
                std::slice::from_ref(&first_change),
                &content_key,
                TEST_CONTENT_KEY_ID,
                &first_signer,
                ProfileSyncRetentionPolicy::default(),
            )
            .expect("first candidate publishes shared root");
        let second_change = second_database
            .set_sync_setting_text(profile, SYNC_DOMAIN_SETTINGS, "ui.theme", "bravo")
            .expect("second candidate writes setting");
        let second_publication = BroadwebdProfileSyncPublisher::new(&second_daemon)
            .publish_signed_settings_tail_changes(
                profile,
                settings_root_id,
                std::slice::from_ref(&second_change),
                &content_key,
                TEST_CONTENT_KEY_ID,
                &second_signer,
                ProfileSyncRetentionPolicy::default(),
            )
            .expect("second candidate publishes shared root");

        let receive_only_policy =
            SettingsSyncCyclePolicy::new(ProfileSyncRetentionPolicy::default(), 4, 4, 2)
                .with_local_device_head_root_health_required_after_cycle(false);
        let run = BroadwebdSettingsSyncRunner::new(&receiver_daemon)
            .run_settings_sync_cycle_with_active_key_policy_shared_root_candidates_and_retention_providers(
                &receiver_database,
                profile,
                settings_root_id,
                &content_key,
                &receiver_signer,
                &receive_only_policy,
                &[&provider_daemon],
            )
            .expect("receiver applies and retains shared-root candidates during active-key cycle");
        assert_eq!(run.cycle.published_step_count(), 0);
        assert_eq!(run.cycle.applied_count(), 0);
        assert_eq!(run.shared_root_candidate_application_count(), 2);
        let ProfileSyncSettingsCandidatePullApplyStatus::Applied(applications) =
            &run.shared_root_candidates
        else {
            panic!(
                "expected shared-root candidate applications, got {:?}",
                run.shared_root_candidates
            );
        };
        assert_eq!(
            applications
                .iter()
                .map(|application| application.application.manifest_object_id.as_str())
                .collect::<Vec<_>>(),
            vec![
                first_publication.manifest_object_id.as_str(),
                second_publication.manifest_object_id.as_str(),
            ]
        );
        let shared_object_ids = run.shared_root_candidate_object_ids();
        assert_eq!(
            shared_object_ids,
            vec![
                first_publication.manifest_object_id.clone(),
                first_publication.tail_change_object_ids[0].clone(),
                second_publication.manifest_object_id.clone(),
                second_publication.tail_change_object_ids[0].clone(),
            ]
        );
        assert_eq!(run.retained_object_ids, shared_object_ids);
        assert_eq!(run.retention.len(), 1);
        assert_eq!(run.retention[0].object_count(), shared_object_ids.len());
        assert_eq!(run.retention[0].retained_count(), shared_object_ids.len());
        assert_eq!(run.retained_provider_count(), 1);
        assert!(
            run.retention[0]
                .object_statuses
                .iter()
                .all(|status| status.retained)
        );
        assert!(
            run.retention[0]
                .object_statuses
                .iter()
                .all(|status| status.available)
        );
        assert!(!run.after_health.settings_root_health.degraded);
        assert_eq!(
            run.after_health
                .settings_root_health
                .online_retaining_providers,
            2
        );
        assert!(run.after_health.local_device_head_root_health.degraded);
        assert_eq!(
            receiver_database
                .get_sync_setting_text(profile, SYNC_DOMAIN_SETTINGS, "ui.theme")
                .expect("read receiver shared candidate value")
                .expect("receiver shared candidate value")
                .value,
            "bravo"
        );
        assert_eq!(
            receiver_database
                .profile_sync_root(profile, settings_root_id)
                .expect("read receiver shared root")
                .expect("receiver shared root")
                .object_id,
            second_publication.manifest_object_id.as_str()
        );
        let retained_shared_root_health = BroadwebdSettingsSyncRunner::new(&receiver_daemon)
            .profile_sync_root_health(profile, settings_root_id, 2)
            .expect("read retained shared-root health");
        assert!(!retained_shared_root_health.degraded);
        assert_eq!(retained_shared_root_health.online_retaining_providers, 2);

        let unchanged = BroadwebdSettingsSyncRunner::new(&receiver_daemon)
            .run_settings_sync_cycle_with_active_key_policy_shared_root_candidates_and_retention_providers(
                &receiver_database,
                profile,
                settings_root_id,
                &content_key,
                &receiver_signer,
                &receive_only_policy,
                &[&provider_daemon],
            )
            .expect("receiver checks unchanged shared-root candidates");
        assert_eq!(unchanged.shared_root_candidate_application_count(), 0);
        assert!(unchanged.retained_object_ids.is_empty());
        assert_eq!(unchanged.retention.len(), 1);
        assert_eq!(unchanged.retention[0].object_count(), 0);
        assert_eq!(unchanged.retained_provider_count(), 0);
        assert!(!unchanged.after_health.settings_root_health.degraded);
        assert_eq!(
            unchanged
                .after_health
                .settings_root_health
                .online_retaining_providers,
            2
        );
        assert!(
            unchanged
                .after_health
                .local_device_head_root_health
                .degraded
        );
        assert_eq!(
            unchanged.shared_root_candidates,
            ProfileSyncSettingsCandidatePullApplyStatus::Unchanged {
                profile: profile.to_string(),
                root_id: settings_root_id.to_string(),
                object_id: second_publication.manifest_object_id.clone(),
            }
        );

        let _ = std::fs::remove_dir_all(first_state_root);
        let _ = std::fs::remove_dir_all(second_state_root);
        let _ = std::fs::remove_dir_all(receiver_state_root);
        let _ = std::fs::remove_dir_all(provider_state_root);
        let _ = std::fs::remove_dir_all(first_db_root);
        let _ = std::fs::remove_dir_all(second_db_root);
        let _ = std::fs::remove_dir_all(receiver_db_root);
    }

    #[test]
    fn broadwebd_settings_sync_runner_rejects_invalid_cycle_credentials() {
        let network = InProcessBroadwebNetwork::new();
        let state_root = test_state_root("cycle-credentials");
        let db_root = test_state_root("cycle-credentials-db");
        let daemon = network
            .daemon_for_device(&state_root, ResourceBudget::default(), "runtime-z")
            .expect("start daemon");
        let database = SlateProfileDatabase::open_resolved_with_device_id(
            db_root.join(DEFAULT_DATABASE_FILE_NAME),
            "runtime-z",
        )
        .expect("open settings database");
        let profile = "credentialprofile";
        let content_key = ProfileSyncContentKey::from_bytes([52; PROFILE_SYNC_CONTENT_KEY_BYTES]);
        let signer = ProfileSyncDeviceSigner::generate("runtime-z").expect("generate signer");
        let runner = BroadwebdSettingsSyncRunner::new(&daemon);

        let missing_key_error = runner
            .run_settings_sync_cycle(
                &database,
                profile,
                "settings/latest",
                &content_key,
                TEST_CONTENT_KEY_ID,
                &signer,
                ProfileSyncRetentionPolicy::default(),
                4,
                4,
            )
            .expect_err("missing active content key should fail");
        assert!(matches!(
            missing_key_error,
            ProfileSyncCycleError::Credentials(ProfileSyncCredentialError::Storage(
                StorageError::MissingActiveSyncContentKey(profile)
            )) if profile == "credentialprofile"
        ));

        register_test_content_key_epoch(&database, profile);
        let wrong_signer =
            ProfileSyncDeviceSigner::generate("runtime-z").expect("generate wrong signer");
        database
            .register_sync_device_public_key(&SyncDevicePublicKeyRegistration {
                profile: profile.to_string(),
                public_key: wrong_signer.public_key().expect("wrong public key"),
                membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            })
            .expect("register mismatched local public key");
        let key_mismatch_error = runner
            .run_settings_sync_cycle(
                &database,
                profile,
                "settings/latest",
                &content_key,
                TEST_CONTENT_KEY_ID,
                &signer,
                ProfileSyncRetentionPolicy::default(),
                4,
                4,
            )
            .expect_err("mismatched local signer should fail");
        assert!(matches!(
            key_mismatch_error,
            ProfileSyncCycleError::Credentials(
                ProfileSyncCredentialError::LocalDevicePublicKeyMismatch {
                    profile,
                    device_id
                }
            ) if profile == "credentialprofile" && device_id == "runtime-z"
        ));

        database
            .register_sync_device_public_key(&SyncDevicePublicKeyRegistration {
                profile: profile.to_string(),
                public_key: signer.public_key().expect("local public key"),
                membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            })
            .expect("register matching local public key");
        let remote_signer =
            ProfileSyncDeviceSigner::generate("runtime-z-remote").expect("generate remote signer");
        database
            .register_sync_device_public_key(&SyncDevicePublicKeyRegistration {
                profile: profile.to_string(),
                public_key: remote_signer.public_key().expect("remote public key"),
                membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            })
            .expect("register trusted remote public key");
        let remote_signer_error = runner
            .run_settings_sync_cycle(
                &database,
                profile,
                "settings/latest",
                &content_key,
                TEST_CONTENT_KEY_ID,
                &remote_signer,
                ProfileSyncRetentionPolicy::default(),
                4,
                4,
            )
            .expect_err("trusted remote signer should not publish local device state");
        assert!(matches!(
            remote_signer_error,
            ProfileSyncCycleError::Credentials(
                ProfileSyncCredentialError::LocalDeviceSignerMismatch {
                    profile,
                    local_device_id,
                    signer_device_id
                }
            ) if profile == "credentialprofile"
                && local_device_id == "runtime-z"
                && signer_device_id == "runtime-z-remote"
        ));
        let inactive_key_error = runner
            .run_settings_sync_cycle(
                &database,
                profile,
                "settings/latest",
                &content_key,
                "content-key-epoch-2",
                &signer,
                ProfileSyncRetentionPolicy::default(),
                4,
                4,
            )
            .expect_err("inactive requested content key should fail");
        assert!(matches!(
            inactive_key_error,
            ProfileSyncCycleError::Credentials(ProfileSyncCredentialError::InactiveContentKey {
                profile,
                expected_key_id,
                active_key_id
            }) if profile == "credentialprofile"
                && expected_key_id == "content-key-epoch-2"
                && active_key_id == TEST_CONTENT_KEY_ID
        ));

        let _ = std::fs::remove_dir_all(state_root);
        let _ = std::fs::remove_dir_all(db_root);
    }

    #[test]
    fn broadwebd_settings_sync_runner_reports_in_process_fixture_health() {
        let network = InProcessBroadwebNetwork::new();
        let state_root = test_state_root("cycle-health");
        let db_root = test_state_root("cycle-health-db");
        let daemon = network
            .daemon_for_device(&state_root, ResourceBudget::default(), "runtime-health")
            .expect("start in-process profile-sync daemon");
        let database = SlateProfileDatabase::open_resolved_with_device_id(
            db_root.join(DEFAULT_DATABASE_FILE_NAME),
            "runtime-health",
        )
        .expect("open local settings database");
        let profile = "healthprofile";
        let settings_root_id = "settings/latest";
        let runner = BroadwebdSettingsSyncRunner::new(&daemon);

        let empty_health = runner
            .settings_sync_health(&database, profile, settings_root_id, 1)
            .expect("read empty in-process settings sync health");
        assert_eq!(empty_health.profile, profile);
        assert_eq!(empty_health.settings_root_id, settings_root_id);
        assert_eq!(
            empty_health.local_device_head_root_id,
            "settings/devices/runtime-health/head"
        );
        assert_eq!(empty_health.provider_health.online_providers, 1);
        assert_eq!(empty_health.provider_health.object_transfer_providers, 1);
        assert!(!empty_health.provider_health.degraded);
        assert!(empty_health.settings_root_health.degraded);
        assert!(empty_health.local_device_head_root_health.degraded);
        assert!(empty_health.degraded());
        assert!(
            empty_health
                .settings_root_health
                .message
                .contains("no visible candidates")
        );

        let content_key = ProfileSyncContentKey::from_bytes([53; PROFILE_SYNC_CONTENT_KEY_BYTES]);
        let signer = ProfileSyncDeviceSigner::generate("runtime-health").expect("generate signer");
        register_test_content_key_epoch(&database, profile);
        database
            .register_sync_device_public_key(&SyncDevicePublicKeyRegistration {
                profile: profile.to_string(),
                public_key: signer.public_key().expect("local public key"),
                membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            })
            .expect("register local trusted public key");
        database
            .set_sync_setting_text(profile, SYNC_DOMAIN_SETTINGS, "ui.theme", "teal")
            .expect("write local setting");
        let cycle = runner
            .run_settings_sync_cycle(
                &database,
                profile,
                settings_root_id,
                &content_key,
                TEST_CONTENT_KEY_ID,
                &signer,
                ProfileSyncRetentionPolicy::default(),
                4,
                4,
            )
            .expect("publish local settings through in-process fixture");
        assert_eq!(cycle.published_step_count(), 1);

        let published_health = runner
            .settings_sync_health(&database, profile, settings_root_id, 1)
            .expect("read published in-process settings sync health");
        assert!(!published_health.degraded());
        assert_eq!(published_health.provider_health.online_providers, 1);
        assert_eq!(published_health.provider_health.retained_objects, 3);
        assert_eq!(published_health.settings_root_health.visible_candidates, 1);
        assert!(
            published_health
                .settings_root_health
                .latest_object_available
        );
        assert_eq!(
            published_health
                .settings_root_health
                .online_retaining_providers,
            1
        );
        assert_eq!(
            published_health
                .local_device_head_root_health
                .visible_candidates,
            1
        );
        assert!(
            published_health
                .local_device_head_root_health
                .latest_object_available
        );
        assert_eq!(
            published_health
                .local_device_head_root_health
                .online_retaining_providers,
            1
        );

        let _ = std::fs::remove_dir_all(state_root);
        let _ = std::fs::remove_dir_all(db_root);
    }

    #[test]
    fn broadwebd_settings_sync_runner_wraps_cycle_with_health() {
        let network = InProcessBroadwebNetwork::new();
        let state_root = test_state_root("cycle-with-health");
        let db_root = test_state_root("cycle-with-health-db");
        let daemon = network
            .daemon_for_device(
                &state_root,
                ResourceBudget::default(),
                "runtime-health-cycle",
            )
            .expect("start in-process profile-sync daemon");
        let database = SlateProfileDatabase::open_resolved_with_device_id(
            db_root.join(DEFAULT_DATABASE_FILE_NAME),
            "runtime-health-cycle",
        )
        .expect("open local settings database");
        let profile = "healthcycleprofile";
        let settings_root_id = "settings/latest";
        let content_key = ProfileSyncContentKey::from_bytes([54; PROFILE_SYNC_CONTENT_KEY_BYTES]);
        let signer =
            ProfileSyncDeviceSigner::generate("runtime-health-cycle").expect("generate signer");
        register_test_content_key_epoch(&database, profile);
        database
            .register_sync_device_public_key(&SyncDevicePublicKeyRegistration {
                profile: profile.to_string(),
                public_key: signer.public_key().expect("local public key"),
                membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            })
            .expect("register local trusted public key");
        database
            .set_sync_setting_text(profile, SYNC_DOMAIN_SETTINGS, "ui.theme", "teal")
            .expect("write local setting");

        let run = BroadwebdSettingsSyncRunner::new(&daemon)
            .run_settings_sync_cycle_with_health(
                &database,
                profile,
                settings_root_id,
                &content_key,
                TEST_CONTENT_KEY_ID,
                &signer,
                ProfileSyncRetentionPolicy::default(),
                4,
                4,
                1,
            )
            .expect("run bounded settings sync cycle with health");

        assert!(run.degraded_before());
        assert!(!run.before_health.provider_health.degraded);
        assert!(run.before_health.settings_root_health.degraded);
        assert!(run.before_health.local_device_head_root_health.degraded);
        assert_eq!(run.cycle.published_step_count(), 1);
        assert_eq!(run.cycle.applied_count(), 0);
        assert!(!run.degraded_after());
        assert_eq!(run.after_health.settings_root_health.visible_candidates, 1);
        assert_eq!(
            run.after_health
                .local_device_head_root_health
                .visible_candidates,
            1
        );

        let policy_run = BroadwebdSettingsSyncRunner::new(&daemon)
            .run_settings_sync_cycle_with_policy(
                &database,
                profile,
                settings_root_id,
                &content_key,
                TEST_CONTENT_KEY_ID,
                &signer,
                &SettingsSyncCyclePolicy::new(ProfileSyncRetentionPolicy::default(), 4, 4, 1),
            )
            .expect("strict policy allows cycle when providers are healthy");
        assert!(!policy_run.degraded_before());
        assert_eq!(policy_run.cycle.published_step_count(), 0);
        assert!(!policy_run.degraded_after());

        let _ = std::fs::remove_dir_all(state_root);
        let _ = std::fs::remove_dir_all(db_root);
    }

    #[test]
    fn broadwebd_settings_sync_policy_rejects_degraded_provider_health() {
        let network = InProcessBroadwebNetwork::new();
        let state_root = test_state_root("cycle-policy-degraded-provider");
        let db_root = test_state_root("cycle-policy-degraded-provider-db");
        let daemon = network
            .daemon_for_availability_provider(
                &state_root,
                ResourceBudget::default(),
                "runtime-policy-provider",
            )
            .expect("start availability-only in-process provider daemon");
        let database = SlateProfileDatabase::open_resolved_with_device_id(
            db_root.join(DEFAULT_DATABASE_FILE_NAME),
            "runtime-policy",
        )
        .expect("open local settings database");
        let profile = "policyprofile";
        let content_key = ProfileSyncContentKey::from_bytes([55; PROFILE_SYNC_CONTENT_KEY_BYTES]);
        let signer = ProfileSyncDeviceSigner::generate("runtime-policy").expect("generate signer");
        let policy = SettingsSyncCyclePolicy::new(ProfileSyncRetentionPolicy::default(), 4, 4, 1);

        let error = BroadwebdSettingsSyncRunner::new(&daemon)
            .run_settings_sync_cycle_with_policy(
                &database,
                profile,
                "settings/latest",
                &content_key,
                TEST_CONTENT_KEY_ID,
                &signer,
                &policy,
            )
            .expect_err("availability-only provider health should fail runtime policy");

        let ProfileSyncCycleWithHealthError::Policy(
            ProfileSyncPolicyError::ProviderHealthDegraded { health },
        ) = error
        else {
            panic!("expected degraded provider policy error, got {error:?}");
        };
        assert_eq!(health.profile, profile);
        assert!(health.provider_health.degraded);
        assert_eq!(health.provider_health.online_providers, 1);
        assert_eq!(health.provider_health.mutable_root_providers, 0);
        assert!(
            health
                .provider_health
                .message
                .contains("mutable-root provider")
        );

        let _ = std::fs::remove_dir_all(state_root);
        let _ = std::fs::remove_dir_all(db_root);
    }

    #[test]
    fn broadwebd_settings_sync_policy_requires_fresh_provider_quorum() {
        let network = InProcessBroadwebNetwork::new();
        let first_state_root = test_state_root("cycle-policy-quorum-first");
        let second_state_root = test_state_root("cycle-policy-quorum-second");
        let db_root = test_state_root("cycle-policy-quorum-db");
        let first_daemon = network
            .daemon_for_device(
                &first_state_root,
                ResourceBudget::default(),
                "runtime-quorum-a",
            )
            .expect("start first in-process profile-sync daemon");
        let database = SlateProfileDatabase::open_resolved_with_device_id(
            db_root.join(DEFAULT_DATABASE_FILE_NAME),
            "runtime-quorum-a",
        )
        .expect("open local settings database");
        let profile = "quorumprofile";
        let settings_root_id = "settings/latest";
        let content_key = ProfileSyncContentKey::from_bytes([58; PROFILE_SYNC_CONTENT_KEY_BYTES]);
        let signer =
            ProfileSyncDeviceSigner::generate("runtime-quorum-a").expect("generate signer");
        let policy = SettingsSyncCyclePolicy::new(ProfileSyncRetentionPolicy::default(), 4, 4, 1)
            .with_minimum_fresh_online_providers(2);

        let one_provider_error = BroadwebdSettingsSyncRunner::new(&first_daemon)
            .run_settings_sync_cycle_with_active_key_policy(
                &database,
                profile,
                settings_root_id,
                &content_key,
                &signer,
                &policy,
            )
            .expect_err("one fresh provider should not satisfy the policy");
        let ProfileSyncCycleWithHealthError::Policy(ProfileSyncPolicyError::ProviderMinimumUnmet {
            provider_role,
            minimum,
            actual,
            health,
        }) = one_provider_error
        else {
            panic!("expected provider quorum policy error, got {one_provider_error:?}");
        };
        assert_eq!(provider_role, "fresh online providers");
        assert_eq!(minimum, 2);
        assert_eq!(actual, 1);
        assert_eq!(health.provider_health.fresh_online_providers, 1);
        assert!(!health.provider_health.degraded);

        let _second_daemon = network
            .daemon_for_device(
                &second_state_root,
                ResourceBudget::default(),
                "runtime-quorum-b",
            )
            .expect("start second in-process profile-sync daemon");
        register_test_content_key_epoch(&database, profile);
        database
            .register_sync_device_public_key(&SyncDevicePublicKeyRegistration {
                profile: profile.to_string(),
                public_key: signer.public_key().expect("local public key"),
                membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            })
            .expect("register local trusted public key");
        database
            .set_sync_setting_text(profile, SYNC_DOMAIN_SETTINGS, "ui.theme", "teal")
            .expect("write local setting");

        let run = BroadwebdSettingsSyncRunner::new(&first_daemon)
            .run_settings_sync_cycle_with_active_key_policy(
                &database,
                profile,
                settings_root_id,
                &content_key,
                &signer,
                &policy,
            )
            .expect("two fresh providers satisfy the policy");
        assert_eq!(run.before_health.provider_health.fresh_online_providers, 2);
        assert_eq!(run.cycle.published_step_count(), 1);
        assert!(!run.degraded_after());

        let _ = std::fs::remove_dir_all(first_state_root);
        let _ = std::fs::remove_dir_all(second_state_root);
        let _ = std::fs::remove_dir_all(db_root);
    }

    #[test]
    fn broadwebd_settings_sync_policy_can_reject_stale_online_providers() {
        let network = InProcessBroadwebNetwork::new();
        let fixture = network.profile_sync();
        let first_state_root = test_state_root("cycle-policy-stale-first");
        let second_state_root = test_state_root("cycle-policy-stale-second");
        let stale_state_root = test_state_root("cycle-policy-stale-third");
        let db_root = test_state_root("cycle-policy-stale-db");
        let first_daemon = network
            .daemon_for_device(
                &first_state_root,
                ResourceBudget::default(),
                "runtime-stale-a",
            )
            .expect("start first in-process profile-sync daemon");
        let _second_daemon = network
            .daemon_for_device(
                &second_state_root,
                ResourceBudget::default(),
                "runtime-stale-b",
            )
            .expect("start second in-process profile-sync daemon");
        let _stale_daemon = network
            .daemon_for_device(
                &stale_state_root,
                ResourceBudget::default(),
                "runtime-stale-c",
            )
            .expect("start stale in-process profile-sync daemon");
        fixture
            .expire_current_provider_freshness()
            .expect("expire all provider freshness");
        fixture
            .mark_device_seen("runtime-stale-a")
            .expect("mark first provider fresh");
        fixture
            .mark_device_seen("runtime-stale-b")
            .expect("mark second provider fresh");

        let database = SlateProfileDatabase::open_resolved_with_device_id(
            db_root.join(DEFAULT_DATABASE_FILE_NAME),
            "runtime-stale-a",
        )
        .expect("open local settings database");
        let profile = "staleproviderprofile";
        let signer =
            ProfileSyncDeviceSigner::generate("runtime-stale-a").expect("generate local signer");
        register_test_content_key_epoch(&database, profile);
        database
            .register_sync_device_public_key(&SyncDevicePublicKeyRegistration {
                profile: profile.to_string(),
                public_key: signer.public_key().expect("local public key"),
                membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            })
            .expect("register local trusted public key");
        let policy = SettingsSyncCyclePolicy::new(ProfileSyncRetentionPolicy::default(), 4, 4, 1)
            .with_minimum_fresh_online_providers(2)
            .with_maximum_stale_online_providers(0);

        let error = BroadwebdSettingsSyncRunner::new(&first_daemon)
            .settings_sync_cycle_preflight_with_active_key_policy(
                &database,
                profile,
                "settings/latest",
                &signer,
                &policy,
            )
            .expect_err("strict stale-provider policy should reject stale online provider");
        let ProfileSyncCycleWithHealthError::Policy(
            ProfileSyncPolicyError::ProviderMaximumExceeded {
                provider_role,
                maximum,
                actual,
                health,
            },
        ) = error
        else {
            panic!("expected stale provider policy error, got {error:?}");
        };
        assert_eq!(provider_role, "stale online providers");
        assert_eq!(maximum, 0);
        assert_eq!(actual, 1);
        assert!(!health.provider_health.degraded);
        assert_eq!(health.provider_health.fresh_online_providers, 2);
        assert_eq!(health.provider_health.stale_online_providers, 1);

        fixture
            .mark_device_seen("runtime-stale-c")
            .expect("refresh stale provider");
        let preflight = BroadwebdSettingsSyncRunner::new(&first_daemon)
            .settings_sync_cycle_preflight_with_active_key_policy(
                &database,
                profile,
                "settings/latest",
                &signer,
                &policy,
            )
            .expect("fresh providers satisfy strict stale-provider policy");
        assert_eq!(
            preflight
                .before_health
                .provider_health
                .fresh_online_providers,
            3
        );
        assert_eq!(
            preflight
                .before_health
                .provider_health
                .stale_online_providers,
            0
        );

        let _ = std::fs::remove_dir_all(first_state_root);
        let _ = std::fs::remove_dir_all(second_state_root);
        let _ = std::fs::remove_dir_all(stale_state_root);
        let _ = std::fs::remove_dir_all(db_root);
    }

    #[test]
    fn broadwebd_settings_sync_policy_can_reject_offline_providers() {
        let network = InProcessBroadwebNetwork::new();
        let fixture = network.profile_sync();
        let first_state_root = test_state_root("cycle-policy-offline-first");
        let second_state_root = test_state_root("cycle-policy-offline-second");
        let offline_state_root = test_state_root("cycle-policy-offline-third");
        let db_root = test_state_root("cycle-policy-offline-db");
        let first_daemon = network
            .daemon_for_device(
                &first_state_root,
                ResourceBudget::default(),
                "runtime-offline-a",
            )
            .expect("start first in-process profile-sync daemon");
        let _second_daemon = network
            .daemon_for_device(
                &second_state_root,
                ResourceBudget::default(),
                "runtime-offline-b",
            )
            .expect("start second in-process profile-sync daemon");
        let offline_daemon = network
            .daemon_for_device(
                &offline_state_root,
                ResourceBudget::default(),
                "runtime-offline-c",
            )
            .expect("start offline in-process profile-sync daemon");
        fixture
            .set_device_online("runtime-offline-c", false)
            .expect("mark third provider offline");

        let database = SlateProfileDatabase::open_resolved_with_device_id(
            db_root.join(DEFAULT_DATABASE_FILE_NAME),
            "runtime-offline-a",
        )
        .expect("open local settings database");
        let profile = "offlineproviderprofile";
        let content_key = ProfileSyncContentKey::from_bytes([73; PROFILE_SYNC_CONTENT_KEY_BYTES]);
        let signer =
            ProfileSyncDeviceSigner::generate("runtime-offline-a").expect("generate local signer");
        register_test_content_key_epoch(&database, profile);
        database
            .register_sync_device_public_key(&SyncDevicePublicKeyRegistration {
                profile: profile.to_string(),
                public_key: signer.public_key().expect("local public key"),
                membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            })
            .expect("register local trusted public key");
        let policy = SettingsSyncCyclePolicy::new(ProfileSyncRetentionPolicy::default(), 4, 4, 1)
            .with_minimum_fresh_online_providers(2)
            .with_maximum_offline_providers(0);

        let error = BroadwebdSettingsSyncRunner::new(&first_daemon)
            .settings_sync_cycle_preflight_with_active_key_policy(
                &database,
                profile,
                "settings/latest",
                &signer,
                &policy,
            )
            .expect_err("strict offline-provider policy should reject offline provider");
        let ProfileSyncCycleWithHealthError::Policy(
            ProfileSyncPolicyError::ProviderMaximumExceeded {
                provider_role,
                maximum,
                actual,
                health,
            },
        ) = error
        else {
            panic!("expected offline provider policy error, got {error:?}");
        };
        assert_eq!(provider_role, "offline providers");
        assert_eq!(maximum, 0);
        assert_eq!(actual, 1);
        assert!(!health.provider_health.degraded);
        assert_eq!(health.provider_health.fresh_online_providers, 2);
        assert_eq!(health.provider_health.offline_providers, 1);
        assert_eq!(
            health.provider_health.offline_provider_ids,
            vec!["local-fixture-device-runtime-offline-c".to_string()]
        );

        let selected_provider_id = "local-fixture-device-runtime-offline-c";
        let selection_policy =
            SettingsSyncCyclePolicy::new(ProfileSyncRetentionPolicy::default(), 4, 4, 2)
                .with_minimum_fresh_online_providers(2);
        let plan = BroadwebdSettingsSyncScheduler::new(&first_daemon)
            .plan_once_selecting_retention_providers(
                &database,
                &SettingsSyncSchedulerConfig::new(profile, "settings/latest", selection_policy),
                &signer,
                &[SettingsSyncRetentionProviderHandle::new(
                    selected_provider_id,
                    &offline_daemon,
                )],
            )
            .expect("scheduler plan classifies offline selected retention provider");
        assert_eq!(
            plan.offline_retention_provider_ids,
            vec![selected_provider_id.to_string()]
        );
        assert_eq!(plan.offline_retention_provider_count(), 1);
        assert!(plan.stale_retention_provider_ids.is_empty());
        assert!(plan.undiscovered_retention_provider_ids.is_empty());

        let run_error = BroadwebdSettingsSyncScheduler::new(&first_daemon)
            .run_once_selecting_retention_providers(
                &database,
                &SettingsSyncSchedulerConfig::new(
                    profile,
                    "settings/latest",
                    SettingsSyncCyclePolicy::new(ProfileSyncRetentionPolicy::default(), 4, 4, 2)
                        .with_minimum_fresh_online_providers(2),
                ),
                SettingsSyncRuntimeSecrets::new(&content_key, &signer),
                &[SettingsSyncRetentionProviderHandle::new(
                    selected_provider_id,
                    &offline_daemon,
                )],
            )
            .expect_err("scheduler run should reject offline selected retention provider");
        let ProfileSyncCycleWithHealthError::Policy(
            ProfileSyncPolicyError::ProviderMaximumExceeded {
                provider_role,
                maximum,
                actual,
                health,
            },
        ) = run_error
        else {
            panic!("expected offline selected retention provider error, got {run_error:?}");
        };
        assert_eq!(provider_role, "offline selected retention providers");
        assert_eq!(maximum, 0);
        assert_eq!(actual, 1);
        assert_eq!(
            health.provider_health.offline_provider_ids,
            vec![selected_provider_id.to_string()]
        );
        assert!(
            database
                .profile_sync_roots(profile)
                .expect("read roots after rejected offline run")
                .is_empty()
        );

        fixture
            .set_device_online("runtime-offline-c", true)
            .expect("bring third provider online");
        let retained = offline_daemon
            .profile_sync(BroadwebdProfileSyncRequest::ListRetainedObjects(
                BroadwebdProfileSyncProfileRequest::new(profile),
            ))
            .expect("recovered provider can list retained objects after rejected scheduler run");
        assert_eq!(
            retained,
            BroadwebdProfileSyncResponse::RetainedObjects {
                object_ids: Vec::new(),
            }
        );
        let preflight = BroadwebdSettingsSyncRunner::new(&first_daemon)
            .settings_sync_cycle_preflight_with_active_key_policy(
                &database,
                profile,
                "settings/latest",
                &signer,
                &policy,
            )
            .expect("online providers satisfy strict offline-provider policy");
        assert_eq!(
            preflight
                .before_health
                .provider_health
                .fresh_online_providers,
            3
        );
        assert_eq!(preflight.before_health.provider_health.offline_providers, 0);

        let _ = std::fs::remove_dir_all(first_state_root);
        let _ = std::fs::remove_dir_all(second_state_root);
        let _ = std::fs::remove_dir_all(offline_state_root);
        let _ = std::fs::remove_dir_all(db_root);
    }

    #[test]
    fn broadwebd_settings_sync_policy_checks_root_health_after_cycle() {
        let network = InProcessBroadwebNetwork::new();
        let state_root = test_state_root("cycle-policy-root-quorum");
        let db_root = test_state_root("cycle-policy-root-quorum-db");
        let daemon = network
            .daemon_for_device(
                &state_root,
                ResourceBudget::default(),
                "runtime-root-quorum",
            )
            .expect("start in-process profile-sync daemon");
        let database = SlateProfileDatabase::open_resolved_with_device_id(
            db_root.join(DEFAULT_DATABASE_FILE_NAME),
            "runtime-root-quorum",
        )
        .expect("open local settings database");
        let profile = "rootquorumprofile";
        let settings_root_id = "settings/latest";
        let content_key = ProfileSyncContentKey::from_bytes([59; PROFILE_SYNC_CONTENT_KEY_BYTES]);
        let signer =
            ProfileSyncDeviceSigner::generate("runtime-root-quorum").expect("generate signer");
        let policy = SettingsSyncCyclePolicy::new(ProfileSyncRetentionPolicy::default(), 4, 4, 2);
        register_test_content_key_epoch(&database, profile);
        database
            .register_sync_device_public_key(&SyncDevicePublicKeyRegistration {
                profile: profile.to_string(),
                public_key: signer.public_key().expect("local public key"),
                membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            })
            .expect("register local trusted public key");
        database
            .set_sync_setting_text(profile, SYNC_DOMAIN_SETTINGS, "ui.theme", "teal")
            .expect("write local setting");

        let error = BroadwebdSettingsSyncRunner::new(&daemon)
            .run_settings_sync_cycle_with_active_key_policy(
                &database,
                profile,
                settings_root_id,
                &content_key,
                &signer,
                &policy,
            )
            .expect_err("one retaining provider should fail the after-cycle root policy");

        let ProfileSyncCycleWithHealthError::Policy(ProfileSyncPolicyError::RootHealthDegraded {
            root_kind,
            health,
        }) = error
        else {
            panic!("expected root health policy error, got {error:?}");
        };
        assert_eq!(root_kind, "settings root");
        assert_eq!(health.settings_root_health.visible_candidates, 1);
        assert!(health.settings_root_health.latest_object_available);
        assert_eq!(health.settings_root_health.online_retaining_providers, 1);
        assert_eq!(
            health
                .settings_root_health
                .minimum_online_retaining_providers,
            2
        );
        assert!(health.settings_root_health.degraded);
        assert!(
            database
                .profile_sync_root(profile, settings_root_id)
                .expect("read local settings root")
                .is_some()
        );
        assert!(
            database
                .profile_sync_root(
                    profile,
                    settings_device_head_root_id("runtime-root-quorum").as_str()
                )
                .expect("read local device-head root")
                .is_some()
        );

        let _ = std::fs::remove_dir_all(state_root);
        let _ = std::fs::remove_dir_all(db_root);
    }

    #[test]
    fn broadwebd_settings_sync_availability_provider_retains_cycle_objects_for_root_quorum() {
        let network = InProcessBroadwebNetwork::new();
        let device_state_root = test_state_root("cycle-retention-device");
        let provider_state_root = test_state_root("cycle-retention-provider");
        let db_root = test_state_root("cycle-retention-db");
        let device_daemon = network
            .daemon_for_device(
                &device_state_root,
                ResourceBudget::default(),
                "runtime-retain-a",
            )
            .expect("start in-process profile-sync device daemon");
        let provider_daemon = network
            .daemon_for_availability_provider(
                &provider_state_root,
                ResourceBudget::default(),
                "runtime-retain-pinner",
            )
            .expect("start in-process availability-provider daemon");
        let database = SlateProfileDatabase::open_resolved_with_device_id(
            db_root.join(DEFAULT_DATABASE_FILE_NAME),
            "runtime-retain-a",
        )
        .expect("open local settings database");
        let profile = "retentionprofile";
        let settings_root_id = "settings/latest";
        let content_key = ProfileSyncContentKey::from_bytes([60; PROFILE_SYNC_CONTENT_KEY_BYTES]);
        let signer = ProfileSyncDeviceSigner::generate("runtime-retain-a")
            .expect("generate local device signer");
        register_test_content_key_epoch(&database, profile);
        database
            .register_sync_device_public_key(&SyncDevicePublicKeyRegistration {
                profile: profile.to_string(),
                public_key: signer.public_key().expect("local public key"),
                membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            })
            .expect("register local trusted public key");
        database
            .set_sync_setting_text(profile, SYNC_DOMAIN_SETTINGS, "ui.theme", "teal")
            .expect("write local setting");

        let run = BroadwebdSettingsSyncRunner::new(&device_daemon)
            .run_settings_sync_cycle_with_health(
                &database,
                profile,
                settings_root_id,
                &content_key,
                TEST_CONTENT_KEY_ID,
                &signer,
                ProfileSyncRetentionPolicy::default(),
                4,
                4,
                2,
            )
            .expect("publish through in-process fixture with report-only health");
        assert_eq!(run.cycle.published_step_count(), 1);
        assert!(run.degraded_after());
        assert_eq!(
            run.after_health
                .settings_root_health
                .online_retaining_providers,
            1
        );

        let object_ids = run.cycle.published_object_ids();
        assert_eq!(object_ids.len(), 3);
        let LocalSettingsHeadPublishStatus::PublishedFullSnapshot(published) =
            &run.cycle.publish.steps[0]
        else {
            panic!("first local publish step should publish a full snapshot");
        };
        assert!(object_ids.contains(&published.publication.snapshot_object_id));
        assert!(object_ids.contains(&published.publication.manifest_object_id));
        assert!(object_ids.contains(&published.device_head.object_id));

        let statuses = BroadwebdProfileSyncPublisher::new(&provider_daemon)
            .retain_settings_sync_cycle_objects(&run.cycle)
            .expect("in-process provider retains all cycle objects");
        assert_eq!(statuses.len(), object_ids.len());
        assert!(statuses.iter().all(|status| status.retained));
        assert!(statuses.iter().all(|status| status.available));

        let health = BroadwebdSettingsSyncRunner::new(&device_daemon)
            .settings_sync_health(&database, profile, settings_root_id, 2)
            .expect("read retained-root health from in-process fixture");
        assert!(!health.degraded());
        assert_eq!(health.settings_root_health.online_retaining_providers, 2);
        assert_eq!(
            health
                .local_device_head_root_health
                .online_retaining_providers,
            2
        );

        let _ = std::fs::remove_dir_all(device_state_root);
        let _ = std::fs::remove_dir_all(provider_state_root);
        let _ = std::fs::remove_dir_all(db_root);
    }

    #[test]
    fn broadwebd_settings_sync_cycle_retains_objects_before_strict_root_policy_check() {
        let network = InProcessBroadwebNetwork::new();
        let device_state_root = test_state_root("cycle-policy-retention-device");
        let provider_state_root = test_state_root("cycle-policy-retention-provider");
        let db_root = test_state_root("cycle-policy-retention-db");
        let device_daemon = network
            .daemon_for_device(
                &device_state_root,
                ResourceBudget::default(),
                "runtime-policy-retain-a",
            )
            .expect("start in-process profile-sync device daemon");
        let provider_daemon = network
            .daemon_for_availability_provider(
                &provider_state_root,
                ResourceBudget::default(),
                "runtime-policy-retain-pinner",
            )
            .expect("start in-process availability-provider daemon");
        let database = SlateProfileDatabase::open_resolved_with_device_id(
            db_root.join(DEFAULT_DATABASE_FILE_NAME),
            "runtime-policy-retain-a",
        )
        .expect("open local settings database");
        let profile = "policyretentionprofile";
        let settings_root_id = "settings/latest";
        let content_key = ProfileSyncContentKey::from_bytes([61; PROFILE_SYNC_CONTENT_KEY_BYTES]);
        let signer = ProfileSyncDeviceSigner::generate("runtime-policy-retain-a")
            .expect("generate local device signer");
        let policy = SettingsSyncCyclePolicy::new(ProfileSyncRetentionPolicy::default(), 4, 4, 2);
        register_test_content_key_epoch(&database, profile);
        database
            .register_sync_device_public_key(&SyncDevicePublicKeyRegistration {
                profile: profile.to_string(),
                public_key: signer.public_key().expect("local public key"),
                membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            })
            .expect("register local trusted public key");
        database
            .set_sync_setting_text(profile, SYNC_DOMAIN_SETTINGS, "ui.theme", "teal")
            .expect("write local setting");

        let run = BroadwebdSettingsSyncRunner::new(&device_daemon)
            .run_settings_sync_cycle_with_active_key_policy_and_retention_providers(
                &database,
                profile,
                settings_root_id,
                &content_key,
                &signer,
                &policy,
                &[&provider_daemon],
            )
            .expect("retention provider satisfies strict root policy after cycle");

        assert!(run.degraded_before());
        assert!(!run.before_health.provider_health.degraded);
        assert_eq!(run.cycle.published_step_count(), 1);
        assert_eq!(run.retention.len(), 1);
        assert_eq!(run.retention[0].provider_index, 0);
        assert_eq!(
            run.retention[0].object_count(),
            run.cycle.published_object_ids().len()
        );
        assert_eq!(
            run.retention[0].object_count(),
            run.retention[0].retained_count()
        );
        assert_eq!(
            run.retention[0].object_count(),
            run.retention[0].available_count()
        );
        assert_eq!(run.retained_provider_count(), 1);
        assert!(!run.degraded_after());
        assert_eq!(
            run.after_health
                .settings_root_health
                .online_retaining_providers,
            2
        );
        assert_eq!(
            run.after_health
                .local_device_head_root_health
                .online_retaining_providers,
            2
        );

        let _ = std::fs::remove_dir_all(device_state_root);
        let _ = std::fs::remove_dir_all(provider_state_root);
        let _ = std::fs::remove_dir_all(db_root);
    }

    #[test]
    fn broadwebd_settings_sync_scheduler_runs_one_retained_fixture_tick() {
        let network = InProcessBroadwebNetwork::new();
        let device_state_root = test_state_root("scheduler-retention-device");
        let provider_state_root = test_state_root("scheduler-retention-provider");
        let db_root = test_state_root("scheduler-retention-db");
        let device_daemon = network
            .daemon_for_device(
                &device_state_root,
                ResourceBudget::default(),
                "runtime-scheduler-a",
            )
            .expect("start in-process profile-sync scheduler device daemon");
        let provider_daemon = network
            .daemon_for_availability_provider(
                &provider_state_root,
                ResourceBudget::default(),
                "runtime-scheduler-pinner",
            )
            .expect("start in-process scheduler availability provider daemon");
        let database = SlateProfileDatabase::open_resolved_with_device_id(
            db_root.join(DEFAULT_DATABASE_FILE_NAME),
            "runtime-scheduler-a",
        )
        .expect("open scheduler local settings database");
        let profile = "schedulerprofile";
        let settings_root_id = "settings/latest";
        let content_key = ProfileSyncContentKey::from_bytes([64; PROFILE_SYNC_CONTENT_KEY_BYTES]);
        let signer = ProfileSyncDeviceSigner::generate("runtime-scheduler-a")
            .expect("generate scheduler local device signer");
        register_test_content_key_epoch(&database, profile);
        database
            .register_sync_device_public_key(&SyncDevicePublicKeyRegistration {
                profile: profile.to_string(),
                public_key: signer.public_key().expect("local public key"),
                membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            })
            .expect("register scheduler local trusted public key");
        database
            .set_sync_setting_text(profile, SYNC_DOMAIN_SETTINGS, "ui.theme", "teal")
            .expect("write scheduler local setting");

        let config = SettingsSyncSchedulerConfig::new(
            profile,
            settings_root_id,
            SettingsSyncCyclePolicy::new(ProfileSyncRetentionPolicy::default(), 4, 4, 2),
        );
        let run = BroadwebdSettingsSyncScheduler::new(&device_daemon)
            .run_once(
                &database,
                &config,
                SettingsSyncRuntimeSecrets::new(&content_key, &signer),
                &[&provider_daemon],
            )
            .expect("scheduler tick publishes and retains settings state");

        assert!(run.degraded_before());
        assert!(!run.before_health.provider_health.degraded);
        assert_eq!(run.cycle.published_step_count(), 1);
        assert_eq!(run.cycle.applied_count(), 0);
        assert_eq!(run.shared_root_candidate_application_count(), 0);
        assert_eq!(run.retained_object_ids, run.cycle.published_object_ids());
        assert_eq!(run.retention.len(), 1);
        assert_eq!(
            run.retention[0].object_count(),
            run.retained_object_ids.len()
        );
        assert_eq!(
            run.retention[0].retained_count(),
            run.retained_object_ids.len()
        );
        assert_eq!(
            run.retention[0].available_count(),
            run.retained_object_ids.len()
        );
        assert_eq!(run.retained_provider_count(), 1);
        assert!(!run.degraded_after());
        assert_eq!(
            run.after_health
                .settings_root_health
                .online_retaining_providers,
            2
        );
        assert_eq!(
            run.after_health
                .local_device_head_root_health
                .online_retaining_providers,
            2
        );

        let _ = std::fs::remove_dir_all(device_state_root);
        let _ = std::fs::remove_dir_all(provider_state_root);
        let _ = std::fs::remove_dir_all(db_root);
    }

    #[test]
    fn broadwebd_settings_sync_scheduler_selects_retention_provider_handles() {
        let network = InProcessBroadwebNetwork::new();
        let device_state_root = test_state_root("scheduler-select-device");
        let provider_state_root = test_state_root("scheduler-select-provider");
        let db_root = test_state_root("scheduler-select-db");
        let device_daemon = network
            .daemon_for_device(
                &device_state_root,
                ResourceBudget::default(),
                "runtime-scheduler-select-a",
            )
            .expect("start in-process profile-sync scheduler device daemon");
        let provider_daemon = network
            .daemon_for_availability_provider(
                &provider_state_root,
                ResourceBudget::default(),
                "runtime-scheduler-select-pinner",
            )
            .expect("start in-process scheduler availability provider daemon");
        let database = SlateProfileDatabase::open_resolved_with_device_id(
            db_root.join(DEFAULT_DATABASE_FILE_NAME),
            "runtime-scheduler-select-a",
        )
        .expect("open scheduler local settings database");
        let profile = "schedulerselectprofile";
        let settings_root_id = "settings/latest";
        let content_key = ProfileSyncContentKey::from_bytes([65; PROFILE_SYNC_CONTENT_KEY_BYTES]);
        let signer = ProfileSyncDeviceSigner::generate("runtime-scheduler-select-a")
            .expect("generate scheduler local device signer");
        register_test_content_key_epoch(&database, profile);
        database
            .register_sync_device_public_key(&SyncDevicePublicKeyRegistration {
                profile: profile.to_string(),
                public_key: signer.public_key().expect("local public key"),
                membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            })
            .expect("register scheduler local trusted public key");
        let local_membership_record = ProfileSyncMembershipRecord {
            profile: profile.to_string(),
            record_id: "epoch-1-enroll-runtime-scheduler-select-a".to_string(),
            schema_version: PROFILE_SYNC_MEMBERSHIP_RECORD_SCHEMA_VERSION,
            membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            record_kind: PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_ENROLL_DEVICE.to_string(),
            device_id: "runtime-scheduler-select-a".to_string(),
            device_public_key: Some(signer.public_key().expect("local membership public key")),
            created_at: 10,
        };
        database
            .record_signed_sync_account_membership_record(
                signed_membership_record_bytes(&signer, &local_membership_record).as_slice(),
            )
            .expect("record scheduler local membership history");
        database
            .set_sync_setting_text(profile, SYNC_DOMAIN_SETTINGS, "ui.theme", "teal")
            .expect("write scheduler local setting");

        let config = SettingsSyncSchedulerConfig::new(
            profile,
            settings_root_id,
            SettingsSyncCyclePolicy::new(ProfileSyncRetentionPolicy::default(), 4, 4, 2),
        );
        let selected_provider_id = "local-fixture-availability-runtime-scheduler-select-pinner";
        let retention_provider_handles = [
            SettingsSyncRetentionProviderHandle::new("not-a-discovered-provider", &provider_daemon),
            SettingsSyncRetentionProviderHandle::new(selected_provider_id, &provider_daemon),
            SettingsSyncRetentionProviderHandle::new(selected_provider_id, &provider_daemon),
        ];
        let scheduler = BroadwebdSettingsSyncScheduler::new(&device_daemon);
        let latest_revision_before_plan = database
            .latest_sync_revision(profile)
            .expect("read latest scheduler revision");
        assert!(
            database
                .profile_sync_roots(profile)
                .expect("read roots before plan")
                .is_empty()
        );

        let plan = scheduler
            .plan_once_selecting_retention_providers(
                &database,
                &config,
                &signer,
                &retention_provider_handles,
            )
            .expect("scheduler plan selects discovered retention provider handles");

        assert_eq!(plan.retention_candidate_count(), 2);
        assert_eq!(
            plan.selected_retention_provider_ids,
            vec![selected_provider_id.to_string()]
        );
        assert_eq!(
            plan.undiscovered_retention_provider_ids,
            vec!["not-a-discovered-provider".to_string()]
        );
        assert_eq!(
            plan.duplicate_retention_provider_ids,
            vec![selected_provider_id.to_string()]
        );
        assert_eq!(plan.selected_retention_provider_count(), 1);
        assert_eq!(plan.undiscovered_retention_provider_count(), 1);
        assert_eq!(plan.duplicate_retention_provider_count(), 1);
        assert!(plan.degraded_before());
        assert!(!plan.preflight.before_health.provider_health.degraded);
        assert_eq!(
            database
                .latest_sync_revision(profile)
                .expect("read latest revision after plan"),
            latest_revision_before_plan
        );
        assert!(
            database
                .profile_sync_roots(profile)
                .expect("read roots after plan")
                .is_empty()
        );

        let membership_plan = scheduler
            .plan_once_with_membership_log_selecting_retention_providers(
                &database,
                &config,
                PROFILE_SYNC_MEMBERSHIP_LOG_ROOT_ID,
                &signer,
                &retention_provider_handles,
            )
            .expect("scheduler membership plan previews local membership publication");

        assert_eq!(
            membership_plan.membership_log_publication.status,
            ProfileSyncMembershipLogPublicationPlanStatus::Publishable
        );
        assert_eq!(membership_plan.membership_log_publication.record_count, 1);
        assert_eq!(
            membership_plan.cycle.selected_retention_provider_ids,
            vec![selected_provider_id.to_string()]
        );
        assert_eq!(
            membership_plan.cycle.undiscovered_retention_provider_ids,
            vec!["not-a-discovered-provider".to_string()]
        );
        assert_eq!(
            membership_plan.cycle.duplicate_retention_provider_ids,
            vec![selected_provider_id.to_string()]
        );
        assert_eq!(membership_plan.selected_retention_provider_count(), 1);
        assert_eq!(membership_plan.undiscovered_retention_provider_count(), 1);
        assert_eq!(membership_plan.duplicate_retention_provider_count(), 1);
        assert!(membership_plan.membership_log_publication.is_publishable());
        assert_eq!(
            database
                .latest_sync_revision(profile)
                .expect("read latest revision after membership plan"),
            latest_revision_before_plan
        );
        assert!(
            database
                .profile_sync_roots(profile)
                .expect("read roots after membership plan")
                .is_empty()
        );

        let run = scheduler
            .run_once_selecting_retention_providers(
                &database,
                &config,
                SettingsSyncRuntimeSecrets::new(&content_key, &signer),
                &retention_provider_handles,
            )
            .expect("scheduler tick selects discovered retention provider handles");

        assert!(
            run.preflight
                .retention_provider_candidates
                .iter()
                .any(|provider| {
                    provider.provider_id == "local-fixture-device-runtime-scheduler-select-a"
                })
        );
        assert!(
            run.preflight
                .retention_provider_candidates
                .iter()
                .any(|provider| provider.provider_id == selected_provider_id)
        );
        assert_eq!(
            run.selected_retention_provider_ids,
            vec![selected_provider_id.to_string()]
        );
        assert_eq!(
            run.undiscovered_retention_provider_ids,
            vec!["not-a-discovered-provider".to_string()]
        );
        assert_eq!(
            run.duplicate_retention_provider_ids,
            vec![selected_provider_id.to_string()]
        );
        assert_eq!(run.selected_retention_provider_count(), 1);
        assert_eq!(run.undiscovered_retention_provider_count(), 1);
        assert_eq!(run.duplicate_retention_provider_count(), 1);
        assert!(run.degraded_before());
        assert_eq!(run.cycle.cycle.published_step_count(), 1);
        assert_eq!(run.cycle.retention.len(), 1);
        assert_eq!(run.retained_provider_count(), 1);
        assert!(!run.degraded_after());
        assert_eq!(
            run.cycle
                .after_health
                .settings_root_health
                .online_retaining_providers,
            2
        );
        assert_eq!(
            run.cycle
                .after_health
                .local_device_head_root_health
                .online_retaining_providers,
            2
        );

        let _ = std::fs::remove_dir_all(device_state_root);
        let _ = std::fs::remove_dir_all(provider_state_root);
        let _ = std::fs::remove_dir_all(db_root);
    }

    #[test]
    fn broadwebd_settings_sync_scheduler_plan_excludes_stale_retention_provider_handles() {
        let network = InProcessBroadwebNetwork::new();
        let device_state_root = test_state_root("scheduler-stale-select-device");
        let provider_state_root = test_state_root("scheduler-stale-select-provider");
        let db_root = test_state_root("scheduler-stale-select-db");
        let device_daemon = network
            .daemon_for_device(
                &device_state_root,
                ResourceBudget::default(),
                "runtime-scheduler-stale-select-a",
            )
            .expect("start in-process profile-sync scheduler device daemon");
        let provider_daemon = network
            .daemon_for_availability_provider(
                &provider_state_root,
                ResourceBudget::default(),
                "runtime-scheduler-stale-select-pinner",
            )
            .expect("start in-process scheduler stale availability provider daemon");
        network
            .profile_sync()
            .expire_current_provider_freshness()
            .expect("expire current provider freshness");
        network
            .profile_sync()
            .mark_device_seen("runtime-scheduler-stale-select-a")
            .expect("keep the local scheduler device fresh");
        let database = SlateProfileDatabase::open_resolved_with_device_id(
            db_root.join(DEFAULT_DATABASE_FILE_NAME),
            "runtime-scheduler-stale-select-a",
        )
        .expect("open scheduler local settings database");
        let profile = "schedulerstaleselectprofile";
        let settings_root_id = "settings/latest";
        let content_key = ProfileSyncContentKey::from_bytes([72; PROFILE_SYNC_CONTENT_KEY_BYTES]);
        let signer = ProfileSyncDeviceSigner::generate("runtime-scheduler-stale-select-a")
            .expect("generate scheduler local device signer");
        register_test_content_key_epoch(&database, profile);
        database
            .register_sync_device_public_key(&SyncDevicePublicKeyRegistration {
                profile: profile.to_string(),
                public_key: signer.public_key().expect("local public key"),
                membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            })
            .expect("register scheduler local trusted public key");
        database
            .set_sync_setting_text(profile, SYNC_DOMAIN_SETTINGS, "ui.theme", "teal")
            .expect("write scheduler local setting");

        let selected_provider_id =
            "local-fixture-availability-runtime-scheduler-stale-select-pinner";
        let latest_revision_before_plan = database
            .latest_sync_revision(profile)
            .expect("read latest scheduler revision");
        let plan = BroadwebdSettingsSyncScheduler::new(&device_daemon)
            .plan_once_selecting_retention_providers(
                &database,
                &SettingsSyncSchedulerConfig::new(
                    profile,
                    settings_root_id,
                    SettingsSyncCyclePolicy::new(ProfileSyncRetentionPolicy::default(), 4, 4, 2),
                ),
                &signer,
                &[SettingsSyncRetentionProviderHandle::new(
                    selected_provider_id,
                    &provider_daemon,
                )],
            )
            .expect("scheduler plan filters stale selected retention provider handles");

        assert_eq!(plan.retention_candidate_count(), 1);
        assert!(
            plan.preflight
                .retention_provider_candidates
                .iter()
                .any(|provider| {
                    provider.provider_id == "local-fixture-device-runtime-scheduler-stale-select-a"
                })
        );
        assert_eq!(plan.selected_retention_provider_count(), 0);
        assert_eq!(
            plan.stale_retention_provider_ids,
            vec![selected_provider_id.to_string()]
        );
        assert_eq!(plan.stale_retention_provider_count(), 1);
        assert!(plan.offline_retention_provider_ids.is_empty());
        assert_eq!(plan.offline_retention_provider_count(), 0);
        assert!(plan.undiscovered_retention_provider_ids.is_empty());
        assert_eq!(
            plan.preflight
                .before_health
                .provider_health
                .stale_online_provider_ids,
            vec![selected_provider_id.to_string()]
        );
        assert_eq!(plan.duplicate_retention_provider_count(), 0);
        assert!(plan.degraded_before());
        assert!(!plan.preflight.before_health.provider_health.degraded);
        assert_eq!(
            plan.preflight
                .before_health
                .provider_health
                .stale_online_providers,
            1
        );
        assert_eq!(
            database
                .latest_sync_revision(profile)
                .expect("read latest revision after plan"),
            latest_revision_before_plan
        );
        assert!(
            database
                .profile_sync_roots(profile)
                .expect("read roots after plan")
                .is_empty()
        );

        let run_error = BroadwebdSettingsSyncScheduler::new(&device_daemon)
            .run_once_selecting_retention_providers(
                &database,
                &SettingsSyncSchedulerConfig::new(
                    profile,
                    settings_root_id,
                    SettingsSyncCyclePolicy::new(ProfileSyncRetentionPolicy::default(), 4, 4, 2),
                ),
                SettingsSyncRuntimeSecrets::new(&content_key, &signer),
                &[SettingsSyncRetentionProviderHandle::new(
                    selected_provider_id,
                    &provider_daemon,
                )],
            )
            .expect_err("scheduler run should reject stale selected retention provider");
        let ProfileSyncCycleWithHealthError::Policy(
            ProfileSyncPolicyError::ProviderMaximumExceeded {
                provider_role,
                maximum,
                actual,
                health,
            },
        ) = run_error
        else {
            panic!("expected stale selected retention provider error, got {run_error:?}");
        };
        assert_eq!(provider_role, "stale selected retention providers");
        assert_eq!(maximum, 0);
        assert_eq!(actual, 1);
        assert_eq!(
            health.provider_health.stale_online_provider_ids,
            vec![selected_provider_id.to_string()]
        );
        assert_eq!(
            database
                .latest_sync_revision(profile)
                .expect("read latest revision after rejected stale run"),
            latest_revision_before_plan
        );
        assert!(
            database
                .profile_sync_roots(profile)
                .expect("read roots after rejected stale run")
                .is_empty()
        );
        let retained = provider_daemon
            .profile_sync(BroadwebdProfileSyncRequest::ListRetainedObjects(
                BroadwebdProfileSyncProfileRequest::new(profile),
            ))
            .expect("stale provider can list retained objects after rejected scheduler run");
        assert_eq!(
            retained,
            BroadwebdProfileSyncResponse::RetainedObjects {
                object_ids: Vec::new(),
            }
        );

        let _ = std::fs::remove_dir_all(device_state_root);
        let _ = std::fs::remove_dir_all(provider_state_root);
        let _ = std::fs::remove_dir_all(db_root);
    }

    #[test]
    fn broadwebd_settings_sync_scheduler_rejects_insufficient_selected_retention_before_mutation() {
        let network = InProcessBroadwebNetwork::new();
        let device_state_root = test_state_root("scheduler-selected-quorum-device");
        let provider_state_root = test_state_root("scheduler-selected-quorum-provider");
        let db_root = test_state_root("scheduler-selected-quorum-db");
        let device_daemon = network
            .daemon_for_device(
                &device_state_root,
                ResourceBudget::default(),
                "runtime-scheduler-selected-quorum-a",
            )
            .expect("start in-process profile-sync scheduler device daemon");
        let provider_daemon = network
            .daemon_for_availability_provider(
                &provider_state_root,
                ResourceBudget::default(),
                "runtime-scheduler-selected-quorum-pinner",
            )
            .expect("start in-process scheduler availability provider daemon");
        let database = SlateProfileDatabase::open_resolved_with_device_id(
            db_root.join(DEFAULT_DATABASE_FILE_NAME),
            "runtime-scheduler-selected-quorum-a",
        )
        .expect("open scheduler local settings database");
        let profile = "schedulerselectedquorumprofile";
        let settings_root_id = "settings/latest";
        let content_key = ProfileSyncContentKey::from_bytes([70; PROFILE_SYNC_CONTENT_KEY_BYTES]);
        let signer = ProfileSyncDeviceSigner::generate("runtime-scheduler-selected-quorum-a")
            .expect("generate scheduler local device signer");
        register_test_content_key_epoch(&database, profile);
        database
            .register_sync_device_public_key(&SyncDevicePublicKeyRegistration {
                profile: profile.to_string(),
                public_key: signer.public_key().expect("local public key"),
                membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            })
            .expect("register scheduler local trusted public key");
        database
            .set_sync_setting_text(profile, SYNC_DOMAIN_SETTINGS, "ui.theme", "teal")
            .expect("write scheduler local setting");
        let latest_revision_before_run = database
            .latest_sync_revision(profile)
            .expect("read latest scheduler revision");

        let error = BroadwebdSettingsSyncScheduler::new(&device_daemon)
            .run_once_selecting_retention_providers(
                &database,
                &SettingsSyncSchedulerConfig::new(
                    profile,
                    settings_root_id,
                    SettingsSyncCyclePolicy::new(ProfileSyncRetentionPolicy::default(), 4, 4, 2),
                ),
                SettingsSyncRuntimeSecrets::new(&content_key, &signer),
                &[],
            )
            .expect_err("scheduler should reject a selected-provider set that cannot meet quorum");

        let ProfileSyncCycleWithHealthError::Policy(ProfileSyncPolicyError::ProviderMinimumUnmet {
            provider_role,
            minimum,
            actual,
            health,
        }) = error
        else {
            panic!("expected selected retention provider policy error, got {error:?}");
        };
        assert_eq!(provider_role, "selected retention providers");
        assert_eq!(minimum, 1);
        assert_eq!(actual, 0);
        assert!(!health.provider_health.degraded);
        assert_eq!(
            database
                .latest_sync_revision(profile)
                .expect("read latest revision after rejected run"),
            latest_revision_before_run
        );
        assert!(
            database
                .profile_sync_roots(profile)
                .expect("read roots after rejected run")
                .is_empty()
        );
        let retained = provider_daemon
            .profile_sync(BroadwebdProfileSyncRequest::ListRetainedObjects(
                BroadwebdProfileSyncProfileRequest::new(profile),
            ))
            .expect("provider can list retained objects after rejected scheduler run");
        assert_eq!(
            retained,
            BroadwebdProfileSyncResponse::RetainedObjects {
                object_ids: Vec::new(),
            }
        );

        let _ = std::fs::remove_dir_all(device_state_root);
        let _ = std::fs::remove_dir_all(provider_state_root);
        let _ = std::fs::remove_dir_all(db_root);
    }

    #[test]
    fn broadwebd_settings_sync_scheduler_surfaces_retention_quota_failure() {
        let network = InProcessBroadwebNetwork::new();
        let device_state_root = test_state_root("scheduler-quota-device");
        let provider_state_root = test_state_root("scheduler-quota-provider");
        let db_root = test_state_root("scheduler-quota-db");
        let device_daemon = network
            .daemon_for_device(
                &device_state_root,
                ResourceBudget::default(),
                "runtime-scheduler-quota-a",
            )
            .expect("start in-process profile-sync scheduler device daemon");
        let provider_daemon = network
            .daemon_for_availability_provider(
                &provider_state_root,
                ResourceBudget::default(),
                "runtime-scheduler-quota-pinner",
            )
            .expect("start in-process scheduler availability provider daemon");
        network
            .profile_sync()
            .set_availability_provider_retention_quota("runtime-scheduler-quota-pinner", Some(0))
            .expect("force provider quota failure");
        let database = SlateProfileDatabase::open_resolved_with_device_id(
            db_root.join(DEFAULT_DATABASE_FILE_NAME),
            "runtime-scheduler-quota-a",
        )
        .expect("open scheduler local settings database");
        let profile = "schedulerquotaprofile";
        let settings_root_id = "settings/latest";
        let content_key = ProfileSyncContentKey::from_bytes([68; PROFILE_SYNC_CONTENT_KEY_BYTES]);
        let signer = ProfileSyncDeviceSigner::generate("runtime-scheduler-quota-a")
            .expect("generate scheduler local device signer");
        register_test_content_key_epoch(&database, profile);
        database
            .register_sync_device_public_key(&SyncDevicePublicKeyRegistration {
                profile: profile.to_string(),
                public_key: signer.public_key().expect("local public key"),
                membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            })
            .expect("register scheduler local trusted public key");
        database
            .set_sync_setting_text(profile, SYNC_DOMAIN_SETTINGS, "ui.theme", "teal")
            .expect("write scheduler local setting");

        let selected_provider_id = "local-fixture-availability-runtime-scheduler-quota-pinner";
        let error = BroadwebdSettingsSyncScheduler::new(&device_daemon)
            .run_once_selecting_retention_providers(
                &database,
                &SettingsSyncSchedulerConfig::new(
                    profile,
                    settings_root_id,
                    SettingsSyncCyclePolicy::new(ProfileSyncRetentionPolicy::default(), 4, 4, 2),
                ),
                SettingsSyncRuntimeSecrets::new(&content_key, &signer),
                &[SettingsSyncRetentionProviderHandle::new(
                    selected_provider_id,
                    &provider_daemon,
                )],
            )
            .expect_err("scheduler should surface selected provider quota failure");

        assert!(matches!(
            error,
            ProfileSyncCycleWithHealthError::Retention(BroadwebdError::UnsupportedRequest(message))
                if message.contains("retention quota exceeded")
        ));
        let retained = provider_daemon
            .profile_sync(BroadwebdProfileSyncRequest::ListRetainedObjects(
                BroadwebdProfileSyncProfileRequest::new(profile),
            ))
            .expect("quota-constrained provider can list retained objects");
        assert_eq!(
            retained,
            BroadwebdProfileSyncResponse::RetainedObjects {
                object_ids: Vec::new(),
            }
        );

        let _ = std::fs::remove_dir_all(device_state_root);
        let _ = std::fs::remove_dir_all(provider_state_root);
        let _ = std::fs::remove_dir_all(db_root);
    }

    #[test]
    fn broadwebd_settings_sync_scheduler_surfaces_pinning_policy_failure() {
        let network = InProcessBroadwebNetwork::new();
        let device_state_root = test_state_root("scheduler-pinning-device");
        let provider_state_root = test_state_root("scheduler-pinning-provider");
        let db_root = test_state_root("scheduler-pinning-db");
        let device_daemon = network
            .daemon_for_device(
                &device_state_root,
                ResourceBudget::default(),
                "runtime-scheduler-pinning-a",
            )
            .expect("start in-process profile-sync scheduler device daemon");
        let provider_daemon = network
            .daemon_for_availability_provider(
                &provider_state_root,
                ResourceBudget::default(),
                "runtime-scheduler-pinning-pinner",
            )
            .expect("start in-process scheduler availability provider daemon");
        network
            .profile_sync()
            .set_availability_provider_retention_available(
                "runtime-scheduler-pinning-pinner",
                false,
            )
            .expect("force provider pinning-policy failure");
        let database = SlateProfileDatabase::open_resolved_with_device_id(
            db_root.join(DEFAULT_DATABASE_FILE_NAME),
            "runtime-scheduler-pinning-a",
        )
        .expect("open scheduler local settings database");
        let profile = "schedulerpinningprofile";
        let settings_root_id = "settings/latest";
        let content_key = ProfileSyncContentKey::from_bytes([69; PROFILE_SYNC_CONTENT_KEY_BYTES]);
        let signer = ProfileSyncDeviceSigner::generate("runtime-scheduler-pinning-a")
            .expect("generate scheduler local device signer");
        register_test_content_key_epoch(&database, profile);
        database
            .register_sync_device_public_key(&SyncDevicePublicKeyRegistration {
                profile: profile.to_string(),
                public_key: signer.public_key().expect("local public key"),
                membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            })
            .expect("register scheduler local trusted public key");
        database
            .set_sync_setting_text(profile, SYNC_DOMAIN_SETTINGS, "ui.theme", "teal")
            .expect("write scheduler local setting");

        let selected_provider_id = "local-fixture-availability-runtime-scheduler-pinning-pinner";
        let error = BroadwebdSettingsSyncScheduler::new(&device_daemon)
            .run_once_selecting_retention_providers(
                &database,
                &SettingsSyncSchedulerConfig::new(
                    profile,
                    settings_root_id,
                    SettingsSyncCyclePolicy::new(ProfileSyncRetentionPolicy::default(), 4, 4, 2),
                ),
                SettingsSyncRuntimeSecrets::new(&content_key, &signer),
                &[SettingsSyncRetentionProviderHandle::new(
                    selected_provider_id,
                    &provider_daemon,
                )],
            )
            .expect_err("scheduler should surface selected provider pinning-policy failure");

        assert!(matches!(
            error,
            ProfileSyncCycleWithHealthError::Retention(BroadwebdError::UnsupportedRequest(message))
                if message.contains("pinning policy")
        ));
        let retained = provider_daemon
            .profile_sync(BroadwebdProfileSyncRequest::ListRetainedObjects(
                BroadwebdProfileSyncProfileRequest::new(profile),
            ))
            .expect("policy-constrained provider can list retained objects");
        assert_eq!(
            retained,
            BroadwebdProfileSyncResponse::RetainedObjects {
                object_ids: Vec::new(),
            }
        );

        let _ = std::fs::remove_dir_all(device_state_root);
        let _ = std::fs::remove_dir_all(provider_state_root);
        let _ = std::fs::remove_dir_all(db_root);
    }

    #[test]
    fn broadwebd_settings_sync_policy_uses_active_content_key_metadata() {
        let network = InProcessBroadwebNetwork::new();
        let state_root = test_state_root("cycle-active-key-policy");
        let db_root = test_state_root("cycle-active-key-policy-db");
        let daemon = network
            .daemon_for_device(&state_root, ResourceBudget::default(), "runtime-active-key")
            .expect("start in-process profile-sync daemon");
        let database = SlateProfileDatabase::open_resolved_with_device_id(
            db_root.join(DEFAULT_DATABASE_FILE_NAME),
            "runtime-active-key",
        )
        .expect("open local settings database");
        let profile = "activekeyprofile";
        let settings_root_id = "settings/latest";
        let content_key = ProfileSyncContentKey::from_bytes([56; PROFILE_SYNC_CONTENT_KEY_BYTES]);
        let signer = ProfileSyncDeviceSigner::generate("runtime-active-key")
            .expect("generate active key signer");
        register_test_content_key_epoch(&database, profile);
        database
            .register_sync_device_public_key(&SyncDevicePublicKeyRegistration {
                profile: profile.to_string(),
                public_key: signer.public_key().expect("local public key"),
                membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            })
            .expect("register local trusted public key");
        database
            .set_sync_setting_text(profile, SYNC_DOMAIN_SETTINGS, "ui.theme", "teal")
            .expect("write local setting");

        let run = BroadwebdSettingsSyncRunner::new(&daemon)
            .run_settings_sync_cycle_with_active_key_policy(
                &database,
                profile,
                settings_root_id,
                &content_key,
                &signer,
                &SettingsSyncCyclePolicy::new(ProfileSyncRetentionPolicy::default(), 4, 4, 1),
            )
            .expect("active key metadata drives settings sync cycle");

        assert!(!run.before_health.provider_health.degraded);
        assert!(run.before_health.settings_root_health.degraded);
        assert_eq!(run.cycle.published_step_count(), 1);
        assert_eq!(run.cycle.applied_count(), 0);
        assert!(!run.degraded_after());

        let _ = std::fs::remove_dir_all(state_root);
        let _ = std::fs::remove_dir_all(db_root);
    }

    #[test]
    fn broadwebd_settings_sync_policy_reports_missing_active_content_key() {
        let network = InProcessBroadwebNetwork::new();
        let state_root = test_state_root("cycle-active-key-missing");
        let db_root = test_state_root("cycle-active-key-missing-db");
        let daemon = network
            .daemon_for_device(
                &state_root,
                ResourceBudget::default(),
                "runtime-missing-active-key",
            )
            .expect("start in-process profile-sync daemon");
        let database = SlateProfileDatabase::open_resolved_with_device_id(
            db_root.join(DEFAULT_DATABASE_FILE_NAME),
            "runtime-missing-active-key",
        )
        .expect("open local settings database");
        let profile = "missingactivekeyprofile";
        let content_key = ProfileSyncContentKey::from_bytes([57; PROFILE_SYNC_CONTENT_KEY_BYTES]);
        let signer = ProfileSyncDeviceSigner::generate("runtime-missing-active-key")
            .expect("generate missing active key signer");

        let error = BroadwebdSettingsSyncRunner::new(&daemon)
            .run_settings_sync_cycle_with_active_key_policy(
                &database,
                profile,
                "settings/latest",
                &content_key,
                &signer,
                &SettingsSyncCyclePolicy::new(ProfileSyncRetentionPolicy::default(), 4, 4, 1),
            )
            .expect_err("missing active content-key metadata should fail");

        assert!(matches!(
            error,
            ProfileSyncCycleWithHealthError::Cycle(ProfileSyncCycleError::Credentials(
                ProfileSyncCredentialError::Storage(
                    StorageError::MissingActiveSyncContentKey(missing_profile)
                )
            )) if missing_profile == "missingactivekeyprofile"
        ));

        let _ = std::fs::remove_dir_all(state_root);
        let _ = std::fs::remove_dir_all(db_root);
    }

    #[test]
    fn broadwebd_settings_sync_policy_rejects_revoked_local_device_key() {
        let network = InProcessBroadwebNetwork::new();
        let state_root = test_state_root("cycle-local-key-revoked");
        let db_root = test_state_root("cycle-local-key-revoked-db");
        let daemon = network
            .daemon_for_device(
                &state_root,
                ResourceBudget::default(),
                "runtime-revoked-local-key",
            )
            .expect("start in-process profile-sync daemon");
        let database = SlateProfileDatabase::open_resolved_with_device_id(
            db_root.join(DEFAULT_DATABASE_FILE_NAME),
            "runtime-revoked-local-key",
        )
        .expect("open local settings database");
        let profile = "revokedlocalkeyprofile";
        let content_key = ProfileSyncContentKey::from_bytes([68; PROFILE_SYNC_CONTENT_KEY_BYTES]);
        let signer = ProfileSyncDeviceSigner::generate("runtime-revoked-local-key")
            .expect("generate revoked local key signer");
        register_test_content_key_epoch(&database, profile);
        database
            .register_sync_device_public_key(&SyncDevicePublicKeyRegistration {
                profile: profile.to_string(),
                public_key: signer.public_key().expect("local public key"),
                membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            })
            .expect("register local trusted public key");
        database
            .set_sync_device_public_key_trusted(profile, signer.device_id(), false)
            .expect("revoke local public key")
            .expect("revoked local public key");

        let error = BroadwebdSettingsSyncRunner::new(&daemon)
            .run_settings_sync_cycle_with_active_key_policy(
                &database,
                profile,
                "settings/latest",
                &content_key,
                &signer,
                &SettingsSyncCyclePolicy::new(ProfileSyncRetentionPolicy::default(), 4, 4, 1),
            )
            .expect_err("revoked local device key should fail credential preflight");
        assert!(matches!(
            error,
            ProfileSyncCycleWithHealthError::Cycle(ProfileSyncCycleError::Credentials(
                ProfileSyncCredentialError::UntrustedLocalDevice {
                    profile,
                    device_id
                }
            )) if profile == "revokedlocalkeyprofile"
                && device_id == "runtime-revoked-local-key"
        ));

        let _ = std::fs::remove_dir_all(state_root);
        let _ = std::fs::remove_dir_all(db_root);
    }

    #[test]
    fn broadwebd_settings_sync_preflight_reports_runtime_inputs_without_publishing() {
        let network = InProcessBroadwebNetwork::new();
        let state_root = test_state_root("cycle-preflight");
        let provider_state_root = test_state_root("cycle-preflight-provider");
        let db_root = test_state_root("cycle-preflight-db");
        let daemon = network
            .daemon_for_device(&state_root, ResourceBudget::default(), "runtime-preflight")
            .expect("start in-process profile-sync daemon");
        let _provider_daemon = network
            .daemon_for_availability_provider(
                &provider_state_root,
                ResourceBudget::default(),
                "runtime-preflight-pinner",
            )
            .expect("start in-process availability-provider daemon");
        let database = SlateProfileDatabase::open_resolved_with_device_id(
            db_root.join(DEFAULT_DATABASE_FILE_NAME),
            "runtime-preflight",
        )
        .expect("open local settings database");
        let profile = "preflightprofile";
        let settings_root_id = "settings/latest";
        let signer =
            ProfileSyncDeviceSigner::generate("runtime-preflight").expect("generate signer");
        let remote_signer =
            ProfileSyncDeviceSigner::generate("runtime-preflight-remote").expect("remote signer");
        register_test_content_key_epoch(&database, profile);
        database
            .register_sync_device_public_key(&SyncDevicePublicKeyRegistration {
                profile: profile.to_string(),
                public_key: signer.public_key().expect("local public key"),
                membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            })
            .expect("register local public key");
        database
            .register_sync_device_public_key(&SyncDevicePublicKeyRegistration {
                profile: profile.to_string(),
                public_key: remote_signer.public_key().expect("remote public key"),
                membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            })
            .expect("register remote public key");

        let preflight = BroadwebdSettingsSyncRunner::new(&daemon)
            .settings_sync_cycle_preflight_with_active_key_policy(
                &database,
                profile,
                settings_root_id,
                &signer,
                &SettingsSyncCyclePolicy::new(ProfileSyncRetentionPolicy::default(), 4, 4, 1),
            )
            .expect("settings sync preflight");

        assert_eq!(preflight.profile, profile);
        assert_eq!(preflight.settings_root_id, settings_root_id);
        assert_eq!(preflight.local_device_id, "runtime-preflight");
        assert_eq!(preflight.signer_device_id, "runtime-preflight");
        assert_eq!(preflight.active_key_id, TEST_CONTENT_KEY_ID);
        assert_eq!(preflight.trusted_remote_device_count, 1);
        assert_eq!(preflight.retention_provider_candidates.len(), 2);
        assert!(
            preflight
                .retention_provider_candidates
                .iter()
                .any(|provider| {
                    provider.provider_id == "local-fixture-device-runtime-preflight"
                        && provider.roles.mutable_roots
                        && provider.roles.availability
                })
        );
        assert!(
            preflight
                .retention_provider_candidates
                .iter()
                .any(|provider| {
                    provider.provider_id == "local-fixture-availability-runtime-preflight-pinner"
                        && !provider.roles.mutable_roots
                        && provider.roles.availability
                })
        );
        assert!(!preflight.before_health.provider_health.degraded);
        assert_eq!(
            preflight
                .before_health
                .provider_health
                .availability_providers,
            2
        );
        assert!(preflight.before_health.settings_root_health.degraded);
        assert!(
            database
                .profile_sync_root(profile, settings_root_id)
                .expect("read settings root")
                .is_none()
        );
        assert!(
            database
                .profile_sync_root(
                    profile,
                    settings_device_head_root_id("runtime-preflight").as_str()
                )
                .expect("read local device-head root")
                .is_none()
        );

        database
            .set_sync_device_public_key_trusted(profile, remote_signer.device_id(), false)
            .expect("revoke remote public key")
            .expect("revoked remote public key");
        let revoked_preflight = BroadwebdSettingsSyncRunner::new(&daemon)
            .settings_sync_cycle_preflight_with_active_key_policy(
                &database,
                profile,
                settings_root_id,
                &signer,
                &SettingsSyncCyclePolicy::new(ProfileSyncRetentionPolicy::default(), 4, 4, 1),
            )
            .expect("settings sync preflight after remote key revocation");
        assert_eq!(revoked_preflight.trusted_remote_device_count, 0);

        let _ = std::fs::remove_dir_all(state_root);
        let _ = std::fs::remove_dir_all(provider_state_root);
        let _ = std::fs::remove_dir_all(db_root);
    }

    #[test]
    fn broadwebd_settings_sync_preflight_enforces_trusted_device_limit() {
        let network = InProcessBroadwebNetwork::new();
        let state_root = test_state_root("cycle-preflight-device-limit");
        let db_root = test_state_root("cycle-preflight-device-limit-db");
        let daemon = network
            .daemon_for_device(
                &state_root,
                ResourceBudget::default(),
                "runtime-preflight-limit",
            )
            .expect("start in-process profile-sync daemon");
        let database = SlateProfileDatabase::open_resolved_with_device_id(
            db_root.join(DEFAULT_DATABASE_FILE_NAME),
            "runtime-preflight-limit",
        )
        .expect("open local settings database");
        let profile = "preflightlimitprofile";
        let settings_root_id = "settings/latest";
        let signer =
            ProfileSyncDeviceSigner::generate("runtime-preflight-limit").expect("generate signer");
        let first_remote =
            ProfileSyncDeviceSigner::generate("runtime-preflight-limit-a").expect("remote a");
        let second_remote =
            ProfileSyncDeviceSigner::generate("runtime-preflight-limit-b").expect("remote b");
        register_test_content_key_epoch(&database, profile);
        for public_key in [
            signer.public_key().expect("local public key"),
            first_remote.public_key().expect("first remote public key"),
            second_remote
                .public_key()
                .expect("second remote public key"),
        ] {
            database
                .register_sync_device_public_key(&SyncDevicePublicKeyRegistration {
                    profile: profile.to_string(),
                    public_key,
                    membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
                })
                .expect("register public key");
        }

        let error = BroadwebdSettingsSyncRunner::new(&daemon)
            .settings_sync_cycle_preflight_with_active_key_policy(
                &database,
                profile,
                settings_root_id,
                &signer,
                &SettingsSyncCyclePolicy::new(ProfileSyncRetentionPolicy::default(), 4, 1, 1),
            )
            .expect_err("trusted device limit should fail in preflight");
        assert!(matches!(
            error,
            ProfileSyncCycleWithHealthError::Cycle(ProfileSyncCycleError::Receive(
                ProfileSyncReceiveError::TrustedDeviceLimitExceeded {
                    profile,
                    trusted_device_count: 2,
                    max_devices: 1
                }
            )) if profile == "preflightlimitprofile"
        ));
        assert!(
            database
                .profile_sync_root(profile, settings_root_id)
                .expect("read settings root")
                .is_none()
        );

        let _ = std::fs::remove_dir_all(state_root);
        let _ = std::fs::remove_dir_all(db_root);
    }

    #[test]
    fn broadwebd_publisher_publishes_full_local_settings_snapshot_head() {
        let network = InProcessBroadwebNetwork::new();
        let publisher_state_root = test_state_root("full-snapshot-head-publisher");
        let receiver_state_root = test_state_root("full-snapshot-head-receiver");
        let publisher_db_root = test_state_root("full-snapshot-head-publisher-db");
        let receiver_db_root = test_state_root("full-snapshot-head-receiver-db");
        let publisher_daemon = network
            .daemon_for_device(
                &publisher_state_root,
                ResourceBudget::default(),
                "runtime-i",
            )
            .expect("start publisher daemon");
        let receiver_daemon = network
            .daemon_for_device(&receiver_state_root, ResourceBudget::default(), "runtime-j")
            .expect("start receiver daemon");
        let publisher_database = SlateProfileDatabase::open_resolved_with_device_id(
            publisher_db_root.join(DEFAULT_DATABASE_FILE_NAME),
            "runtime-i",
        )
        .expect("open publisher settings database");
        let receiver_database = SlateProfileDatabase::open_resolved_with_device_id(
            receiver_db_root.join(DEFAULT_DATABASE_FILE_NAME),
            "runtime-j",
        )
        .expect("open receiver settings database");
        publisher_database
            .register_app_sync_domain(&AppSyncDomainRegistration {
                profile: DEFAULT_PROFILE_ID.to_string(),
                domain: SYNC_DOMAIN_CALENDAR.to_string(),
                schema_version: 1,
                enabled: true,
                privacy_classification: "sensitive".to_string(),
                sync_content: false,
            })
            .expect("enable calendar sync for publisher test profile");
        publisher_database
            .set_sync_setting_text(DEFAULT_PROFILE_ID, SYNC_DOMAIN_SETTINGS, "ui.theme", "teal")
            .expect("publisher writes theme setting");
        publisher_database
            .set_sync_setting_text(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_CALENDAR,
                "default_view",
                "month",
            )
            .expect("publisher writes calendar setting");
        publisher_database
            .set_bookmark_slot(
                &BookmarkUpdate {
                    profile: DEFAULT_PROFILE_ID.to_string(),
                    url: "https://example.com/".to_string(),
                    title: Some("Example".to_string()),
                    folder: None,
                    position: 2,
                    favicon_key: Some("favicon:https://example.com/".to_string()),
                },
                None,
            )
            .expect("publisher writes bookmark slot");
        publisher_database
            .set_bookmark_slot(
                &BookmarkUpdate {
                    profile: DEFAULT_PROFILE_ID.to_string(),
                    url: "https://old.example/".to_string(),
                    title: Some("Old".to_string()),
                    folder: None,
                    position: 3,
                    favicon_key: None,
                },
                None,
            )
            .expect("publisher writes removable bookmark slot");
        publisher_database
            .remove_bookmark(DEFAULT_PROFILE_ID, "https://old.example/")
            .expect("publisher removes bookmark slot");
        receiver_database
            .upsert_bookmark(&BookmarkUpdate {
                profile: DEFAULT_PROFILE_ID.to_string(),
                url: "https://old.example/".to_string(),
                title: Some("Old".to_string()),
                folder: None,
                position: 3,
                favicon_key: None,
            })
            .expect("receiver has stale bookmark slot");
        let content_key = ProfileSyncContentKey::from_bytes([46; PROFILE_SYNC_CONTENT_KEY_BYTES]);
        let signer = ProfileSyncDeviceSigner::generate("runtime-i").expect("generate signer");
        let public_key = signer.public_key().expect("read signer public key");
        receiver_database
            .register_sync_device_public_key(&SyncDevicePublicKeyRegistration {
                profile: DEFAULT_PROFILE_ID.to_string(),
                public_key,
                membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            })
            .expect("receiver trusts publisher key");
        let publisher = BroadwebdProfileSyncPublisher::new(&publisher_daemon);

        let published = publisher
            .publish_full_local_settings_snapshot_head(
                &publisher_database,
                DEFAULT_PROFILE_ID,
                "settings/latest",
                &content_key,
                TEST_CONTENT_KEY_ID,
                &signer,
                ProfileSyncRetentionPolicy::default(),
            )
            .expect("publish full local snapshot head")
            .expect("local settings changes exist");

        assert_eq!(published.publication.manifest.root_id, "settings/latest");
        assert_eq!(
            published
                .publication
                .manifest
                .current_snapshot_object_id
                .as_deref(),
            Some(published.publication.snapshot_object_id.as_str())
        );
        assert_eq!(
            published.publication.tail_change_object_ids,
            Vec::<String>::new()
        );
        assert_eq!(
            published.publication.manifest.included_domains,
            vec![
                SYNC_DOMAIN_BOOKMARKS.to_string(),
                SYNC_DOMAIN_CALENDAR.to_string(),
                SYNC_DOMAIN_SETTINGS.to_string()
            ]
        );
        assert_eq!(
            published.settings_root.object_id,
            published.publication.manifest_object_id
        );
        assert_eq!(
            published.device_head_root.object_id,
            published.device_head.object_id
        );
        assert_eq!(
            published.snapshot_record.backend_object_id.as_deref(),
            Some(published.publication.snapshot_object_id.as_str())
        );
        assert_eq!(
            publisher_database
                .profile_sync_root(DEFAULT_PROFILE_ID, "settings/latest")
                .expect("read local published settings root")
                .expect("local settings root")
                .object_id,
            published.publication.manifest_object_id
        );

        let source = BroadwebdProfileSyncObjectSource::new(&receiver_daemon);
        let applied = source
            .pull_record_and_apply_trusted_settings_from_device_head(
                &receiver_database,
                DEFAULT_PROFILE_ID,
                published.device_head.root_id.as_str(),
                &content_key,
                TEST_CONTENT_KEY_ID,
            )
            .expect("receiver applies full snapshot from trusted head");
        let BroadwebdTrustedDeviceHeadSyncStatus::Applied { application, .. } = applied else {
            panic!("expected full snapshot application, got {applied:?}");
        };
        assert_eq!(
            application.manifest_object_id,
            published.publication.manifest_object_id
        );
        assert!(application.snapshot.is_some());
        assert_eq!(application.tail_changes, Vec::<SyncChangeRecord>::new());
        assert_eq!(
            receiver_database
                .get_setting_text("ui.theme")
                .expect("read receiver theme")
                .as_deref(),
            Some("teal")
        );
        assert_eq!(
            receiver_database
                .get_sync_setting_text(DEFAULT_PROFILE_ID, SYNC_DOMAIN_CALENDAR, "default_view")
                .expect("read receiver calendar sync setting")
                .expect("receiver calendar sync setting")
                .value,
            "month"
        );
        let bookmark_value = receiver_database
            .get_sync_setting_text(DEFAULT_PROFILE_ID, SYNC_DOMAIN_BOOKMARKS, "home.slot.2")
            .expect("read receiver bookmark sync setting")
            .expect("receiver bookmark sync setting")
            .value;
        let bookmark_payload: BookmarkSlotSyncPayload =
            serde_json::from_str(bookmark_value.as_str()).expect("decode bookmark payload");
        assert_eq!(bookmark_payload.url, "https://example.com/");
        assert_eq!(bookmark_payload.title.as_deref(), Some("Example"));
        assert_eq!(bookmark_payload.position, 2);
        assert!(!bookmark_payload.deleted);
        assert_eq!(
            bookmark_payload.favicon_key.as_deref(),
            Some("favicon:https://example.com/")
        );
        assert_eq!(bookmark_payload.replaced_url, None);
        let bookmarks = receiver_database
            .bookmarks(DEFAULT_PROFILE_ID)
            .expect("read receiver bookmarks");
        assert!(bookmarks.iter().any(|bookmark| {
            bookmark.url == "https://example.com/"
                && bookmark.title.as_deref() == Some("Example")
                && bookmark.position == 2
        }));
        assert!(
            !bookmarks
                .iter()
                .any(|bookmark| bookmark.url == "https://old.example/")
        );
        let deleted_bookmark_value = receiver_database
            .get_sync_setting_text(DEFAULT_PROFILE_ID, SYNC_DOMAIN_BOOKMARKS, "home.slot.3")
            .expect("read receiver deleted bookmark sync setting")
            .expect("receiver deleted bookmark sync setting")
            .value;
        let deleted_bookmark_payload: BookmarkSlotSyncPayload =
            serde_json::from_str(deleted_bookmark_value.as_str())
                .expect("decode deleted bookmark payload");
        assert!(deleted_bookmark_payload.deleted);
        assert_eq!(deleted_bookmark_payload.url, "https://old.example/");
        assert_eq!(deleted_bookmark_payload.position, 3);

        let _ = std::fs::remove_dir_all(publisher_state_root);
        let _ = std::fs::remove_dir_all(receiver_state_root);
        let _ = std::fs::remove_dir_all(publisher_db_root);
        let _ = std::fs::remove_dir_all(receiver_db_root);
    }

    #[test]
    fn broadwebd_publisher_syncs_typed_app_metadata_snapshot_head() {
        let network = InProcessBroadwebNetwork::new();
        let publisher_state_root = test_state_root("typed-app-snapshot-head-publisher");
        let receiver_state_root = test_state_root("typed-app-snapshot-head-receiver");
        let publisher_db_root = test_state_root("typed-app-snapshot-head-publisher-db");
        let receiver_db_root = test_state_root("typed-app-snapshot-head-receiver-db");
        let publisher_daemon = network
            .daemon_for_device(
                &publisher_state_root,
                ResourceBudget::default(),
                "runtime-typed-app-publisher",
            )
            .expect("start typed app publisher daemon");
        let receiver_daemon = network
            .daemon_for_device(
                &receiver_state_root,
                ResourceBudget::default(),
                "runtime-typed-app-receiver",
            )
            .expect("start typed app receiver daemon");
        let publisher_database = SlateProfileDatabase::open_resolved_with_device_id(
            publisher_db_root.join(DEFAULT_DATABASE_FILE_NAME),
            "runtime-typed-app-publisher",
        )
        .expect("open typed app publisher settings database");
        let receiver_database = SlateProfileDatabase::open_resolved_with_device_id(
            receiver_db_root.join(DEFAULT_DATABASE_FILE_NAME),
            "runtime-typed-app-receiver",
        )
        .expect("open typed app receiver settings database");
        for (domain, privacy_classification, sync_content) in [
            (SYNC_DOMAIN_CHAT, "sensitive", false),
            (SYNC_DOMAIN_FILES, "content", true),
            (SYNC_DOMAIN_STORAGE, "sensitive", false),
        ] {
            publisher_database
                .register_app_sync_domain(&AppSyncDomainRegistration {
                    profile: DEFAULT_PROFILE_ID.to_string(),
                    domain: domain.to_string(),
                    schema_version: 1,
                    enabled: true,
                    privacy_classification: privacy_classification.to_string(),
                    sync_content,
                })
                .expect("enable typed app sync domain for publisher test profile");
        }
        publisher_database
            .upsert_chat_conversation(&ChatConversationUpdate {
                profile: DEFAULT_PROFILE_ID.to_string(),
                conversation_id: "runtime-chat-1".to_string(),
                provider_id: Some("whatsapp".to_string()),
                external_thread_id: Some("team@example.test".to_string()),
                display_name: "Runtime Team".to_string(),
                avatar_key: Some("chat-avatar:runtime-chat-1".to_string()),
                last_message_at: Some(1_789_010_000),
                unread_count: 4,
                archived: false,
                muted: true,
            })
            .expect("publisher writes typed chat metadata");
        publisher_database
            .upsert_file_entry(&FileEntryUpdate {
                profile: DEFAULT_PROFILE_ID.to_string(),
                entry_id: "runtime-file-1".to_string(),
                sync_set_id: Some("runtime-set".to_string()),
                parent_id: None,
                name: "runtime.txt".to_string(),
                entry_kind: "file".to_string(),
                content_ref: Some("bafy-runtime-file".to_string()),
                mime_type: Some("text/plain".to_string()),
                size_bytes: Some(512),
                modified_at: Some(1_789_010_100),
                integrity: Some("sha256-runtime-file".to_string()),
                retention_policy: Some("keep-latest".to_string()),
            })
            .expect("publisher writes typed file metadata");
        publisher_database
            .upsert_storage_provider(&StorageProviderUpdate {
                profile: DEFAULT_PROFILE_ID.to_string(),
                provider_id: "runtime-provider-1".to_string(),
                provider_kind: "ipfs".to_string(),
                display_name: "Runtime IPFS".to_string(),
                endpoint_ref: Some(
                    "/dnsaddr/runtime.example.test/p2p/runtime-provider-1".to_string(),
                ),
                discovery: true,
                connectivity: true,
                object_transfer: true,
                availability: true,
                mutable_roots: false,
                quota_bytes: Some(8_192),
                max_retained_objects: Some(16),
                pinning_policy: Some("manual".to_string()),
                enabled: true,
            })
            .expect("publisher writes typed storage provider metadata");

        let content_key = ProfileSyncContentKey::from_bytes([71; PROFILE_SYNC_CONTENT_KEY_BYTES]);
        let signer = ProfileSyncDeviceSigner::generate("runtime-typed-app-publisher")
            .expect("generate typed app publisher signer");
        let public_key = signer.public_key().expect("read signer public key");
        receiver_database
            .register_sync_device_public_key(&SyncDevicePublicKeyRegistration {
                profile: DEFAULT_PROFILE_ID.to_string(),
                public_key,
                membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            })
            .expect("receiver trusts typed app publisher key");
        let chat_watcher = TypedAppSyncDomainWatcher::<ChatConversationSyncPayload>::new(
            receiver_database.clone(),
            DEFAULT_PROFILE_ID,
            SYNC_DOMAIN_CHAT,
            8,
        )
        .expect("initialize receiver chat watcher cursor");
        let file_watcher = TypedAppSyncDomainWatcher::<FileEntrySyncPayload>::new(
            receiver_database.clone(),
            DEFAULT_PROFILE_ID,
            SYNC_DOMAIN_FILES,
            8,
        )
        .expect("initialize receiver files watcher cursor");
        let storage_watcher = TypedAppSyncDomainWatcher::<StorageProviderSyncPayload>::new(
            receiver_database.clone(),
            DEFAULT_PROFILE_ID,
            SYNC_DOMAIN_STORAGE,
            8,
        )
        .expect("initialize receiver storage watcher cursor");
        let initial_chat_revision = chat_watcher
            .current_revision()
            .expect("read initial receiver chat watcher cursor");
        let initial_file_revision = file_watcher
            .current_revision()
            .expect("read initial receiver files watcher cursor");
        let initial_storage_revision = storage_watcher
            .current_revision()
            .expect("read initial receiver storage watcher cursor");
        assert_eq!(
            chat_watcher
                .poll_once()
                .expect("poll idle chat")
                .event_count(),
            0
        );
        assert_eq!(
            file_watcher
                .poll_once()
                .expect("poll idle files")
                .event_count(),
            0
        );
        assert_eq!(
            storage_watcher
                .poll_once()
                .expect("poll idle storage")
                .event_count(),
            0
        );
        let publisher = BroadwebdProfileSyncPublisher::new(&publisher_daemon);

        let published = publisher
            .publish_full_local_settings_snapshot_head(
                &publisher_database,
                DEFAULT_PROFILE_ID,
                "settings/latest",
                &content_key,
                TEST_CONTENT_KEY_ID,
                &signer,
                ProfileSyncRetentionPolicy::default(),
            )
            .expect("publish typed app metadata snapshot head")
            .expect("typed app metadata changes exist");

        assert_eq!(
            published.publication.manifest.included_domains,
            vec![
                SYNC_DOMAIN_CHAT.to_string(),
                SYNC_DOMAIN_FILES.to_string(),
                SYNC_DOMAIN_SETTINGS.to_string(),
                SYNC_DOMAIN_STORAGE.to_string()
            ]
        );
        assert_eq!(
            published.snapshot_record.included_domains,
            published.publication.manifest.included_domains
        );
        assert_eq!(
            published.publication.tail_change_object_ids,
            Vec::<String>::new()
        );

        let source = BroadwebdProfileSyncObjectSource::new(&receiver_daemon);
        let applied = source
            .pull_record_and_apply_trusted_settings_from_device_head(
                &receiver_database,
                DEFAULT_PROFILE_ID,
                published.device_head.root_id.as_str(),
                &content_key,
                TEST_CONTENT_KEY_ID,
            )
            .expect("receiver applies typed app snapshot from trusted head");
        let BroadwebdTrustedDeviceHeadSyncStatus::Applied { application, .. } = applied else {
            panic!("expected typed app snapshot application, got {applied:?}");
        };
        assert_eq!(
            application.manifest_object_id,
            published.publication.manifest_object_id
        );
        assert!(application.snapshot.is_some());
        assert_eq!(application.tail_changes, Vec::<SyncChangeRecord>::new());

        let conversations = receiver_database
            .chat_conversations(DEFAULT_PROFILE_ID, 10)
            .expect("read receiver typed chat metadata");
        assert_eq!(conversations.len(), 1);
        assert_eq!(conversations[0].conversation_id, "runtime-chat-1");
        assert_eq!(conversations[0].display_name, "Runtime Team");
        assert_eq!(conversations[0].unread_count, 4);
        assert!(conversations[0].muted);

        let files = receiver_database
            .file_entries(DEFAULT_PROFILE_ID, 10)
            .expect("read receiver typed file metadata");
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].entry_id, "runtime-file-1");
        assert_eq!(files[0].content_ref.as_deref(), Some("bafy-runtime-file"));
        assert_eq!(files[0].size_bytes, Some(512));

        let providers = receiver_database
            .storage_providers(DEFAULT_PROFILE_ID, 10)
            .expect("read receiver typed storage provider metadata");
        assert_eq!(providers.len(), 1);
        assert_eq!(providers[0].provider_id, "runtime-provider-1");
        assert_eq!(providers[0].provider_kind, "ipfs");
        assert_eq!(providers[0].quota_bytes, Some(8_192));
        assert!(providers[0].availability);
        assert_eq!(providers[0].pinning_policy.as_deref(), Some("manual"));

        let chat_applied = chat_watcher
            .poll_apply_and_acknowledge(|chat_poll| {
                assert!(chat_poll.advanced());
                assert_eq!(chat_poll.previous_revision, initial_chat_revision);
                assert_eq!(chat_poll.event_count(), 1);
                assert_eq!(
                    chat_poll.events[0].change.entity_key,
                    "conversation.runtime-chat-1"
                );
                assert_eq!(chat_poll.events[0].value.conversation_id, "runtime-chat-1");
                assert_eq!(chat_poll.events[0].value.display_name, "Runtime Team");
                assert_eq!(chat_poll.events[0].value.unread_count, 4);
                assert!(chat_poll.events[0].value.muted);
                assert!(!chat_poll.events[0].value.deleted);
                Ok::<(), &'static str>(())
            })
            .expect("apply and acknowledge receiver chat typed app events after sync apply");
        let chat_poll = chat_applied.poll;
        assert!(chat_poll.advanced());
        assert_eq!(chat_poll.previous_revision, initial_chat_revision);
        assert_eq!(chat_poll.event_count(), 1);
        assert_eq!(
            chat_poll.events[0].change.entity_key,
            "conversation.runtime-chat-1"
        );
        assert_eq!(chat_poll.events[0].value.conversation_id, "runtime-chat-1");
        assert_eq!(chat_poll.events[0].value.display_name, "Runtime Team");
        assert_eq!(chat_poll.events[0].value.unread_count, 4);
        assert!(chat_poll.events[0].value.muted);
        assert!(!chat_poll.events[0].value.deleted);
        assert_eq!(
            receiver_database
                .app_sync_domain_cursor(DEFAULT_PROFILE_ID, SYNC_DOMAIN_CHAT)
                .expect("read receiver chat watcher cursor")
                .map(|cursor| cursor.latest_revision),
            Some(chat_poll.latest_revision)
        );
        assert_eq!(
            chat_applied.cursor.latest_revision,
            chat_poll.latest_revision
        );

        let file_applied = file_watcher
            .poll_apply_and_acknowledge(|file_poll| {
                assert!(file_poll.advanced());
                assert_eq!(file_poll.previous_revision, initial_file_revision);
                assert_eq!(file_poll.event_count(), 1);
                assert_eq!(
                    file_poll.events[0].change.entity_key,
                    "entry.runtime-file-1"
                );
                assert_eq!(file_poll.events[0].value.entry_id, "runtime-file-1");
                assert_eq!(
                    file_poll.events[0].value.content_ref.as_deref(),
                    Some("bafy-runtime-file")
                );
                assert_eq!(file_poll.events[0].value.size_bytes, Some(512));
                assert!(!file_poll.events[0].value.deleted);
                Ok::<(), &'static str>(())
            })
            .expect("apply and acknowledge receiver files typed app events after sync apply");
        let file_poll = file_applied.poll;
        assert!(file_poll.advanced());
        assert_eq!(file_poll.previous_revision, initial_file_revision);
        assert_eq!(file_poll.event_count(), 1);
        assert_eq!(
            file_poll.events[0].change.entity_key,
            "entry.runtime-file-1"
        );
        assert_eq!(file_poll.events[0].value.entry_id, "runtime-file-1");
        assert_eq!(
            file_poll.events[0].value.content_ref.as_deref(),
            Some("bafy-runtime-file")
        );
        assert_eq!(file_poll.events[0].value.size_bytes, Some(512));
        assert!(!file_poll.events[0].value.deleted);
        assert_eq!(
            file_applied.cursor.latest_revision,
            file_poll.latest_revision
        );

        let storage_applied = storage_watcher
            .poll_apply_and_acknowledge(|storage_poll| {
                assert!(storage_poll.advanced());
                assert_eq!(storage_poll.previous_revision, initial_storage_revision);
                assert_eq!(storage_poll.event_count(), 1);
                assert_eq!(
                    storage_poll.events[0].change.entity_key,
                    "provider.runtime-provider-1"
                );
                assert_eq!(
                    storage_poll.events[0].value.provider_id,
                    "runtime-provider-1"
                );
                assert_eq!(storage_poll.events[0].value.provider_kind, "ipfs");
                assert_eq!(storage_poll.events[0].value.quota_bytes, Some(8_192));
                assert!(storage_poll.events[0].value.availability);
                assert!(!storage_poll.events[0].value.deleted);
                Ok::<(), &'static str>(())
            })
            .expect("apply and acknowledge receiver storage typed app events after sync apply");
        let storage_poll = storage_applied.poll;
        assert!(storage_poll.advanced());
        assert_eq!(storage_poll.previous_revision, initial_storage_revision);
        assert_eq!(storage_poll.event_count(), 1);
        assert_eq!(
            storage_poll.events[0].change.entity_key,
            "provider.runtime-provider-1"
        );
        assert_eq!(
            storage_poll.events[0].value.provider_id,
            "runtime-provider-1"
        );
        assert_eq!(storage_poll.events[0].value.provider_kind, "ipfs");
        assert_eq!(storage_poll.events[0].value.quota_bytes, Some(8_192));
        assert!(storage_poll.events[0].value.availability);
        assert!(!storage_poll.events[0].value.deleted);
        assert_eq!(
            storage_applied.cursor.latest_revision,
            storage_poll.latest_revision
        );

        let _ = std::fs::remove_dir_all(publisher_state_root);
        let _ = std::fs::remove_dir_all(receiver_state_root);
        let _ = std::fs::remove_dir_all(publisher_db_root);
        let _ = std::fs::remove_dir_all(receiver_db_root);
    }

    #[test]
    fn broadwebd_publisher_syncs_typed_app_metadata_tail_head() {
        let network = InProcessBroadwebNetwork::new();
        let publisher_state_root = test_state_root("typed-app-tail-head-publisher");
        let receiver_state_root = test_state_root("typed-app-tail-head-receiver");
        let publisher_db_root = test_state_root("typed-app-tail-head-publisher-db");
        let receiver_db_root = test_state_root("typed-app-tail-head-receiver-db");
        let publisher_daemon = network
            .daemon_for_device(
                &publisher_state_root,
                ResourceBudget::default(),
                "runtime-typed-app-upsert-tail-publisher",
            )
            .expect("start typed app upsert tail publisher daemon");
        let receiver_daemon = network
            .daemon_for_device(
                &receiver_state_root,
                ResourceBudget::default(),
                "runtime-typed-app-upsert-tail-receiver",
            )
            .expect("start typed app upsert tail receiver daemon");
        let publisher_database = SlateProfileDatabase::open_resolved_with_device_id(
            publisher_db_root.join(DEFAULT_DATABASE_FILE_NAME),
            "runtime-typed-app-upsert-tail-publisher",
        )
        .expect("open typed app upsert tail publisher settings database");
        let receiver_database = SlateProfileDatabase::open_resolved_with_device_id(
            receiver_db_root.join(DEFAULT_DATABASE_FILE_NAME),
            "runtime-typed-app-upsert-tail-receiver",
        )
        .expect("open typed app upsert tail receiver settings database");
        for (domain, privacy_classification, sync_content) in [
            (SYNC_DOMAIN_CHAT, "sensitive", false),
            (SYNC_DOMAIN_FILES, "content", true),
            (SYNC_DOMAIN_STORAGE, "sensitive", false),
        ] {
            publisher_database
                .register_app_sync_domain(&AppSyncDomainRegistration {
                    profile: DEFAULT_PROFILE_ID.to_string(),
                    domain: domain.to_string(),
                    schema_version: 1,
                    enabled: true,
                    privacy_classification: privacy_classification.to_string(),
                    sync_content,
                })
                .expect("enable typed app upsert tail sync domain for publisher test profile");
        }

        publisher_database
            .upsert_chat_conversation(&ChatConversationUpdate {
                profile: DEFAULT_PROFILE_ID.to_string(),
                conversation_id: "runtime-chat-tail-1".to_string(),
                provider_id: Some("whatsapp".to_string()),
                external_thread_id: Some("tail-team@example.test".to_string()),
                display_name: "Runtime Team".to_string(),
                avatar_key: Some("chat-avatar:runtime-chat-tail-1".to_string()),
                last_message_at: Some(1_789_040_000),
                unread_count: 1,
                archived: false,
                muted: false,
            })
            .expect("publisher writes initial typed chat metadata");
        publisher_database
            .upsert_file_entry(&FileEntryUpdate {
                profile: DEFAULT_PROFILE_ID.to_string(),
                entry_id: "runtime-file-tail-1".to_string(),
                sync_set_id: Some("runtime-set".to_string()),
                parent_id: None,
                name: "runtime-initial.txt".to_string(),
                entry_kind: "file".to_string(),
                content_ref: Some("bafy-runtime-file-initial".to_string()),
                mime_type: Some("text/plain".to_string()),
                size_bytes: Some(128),
                modified_at: Some(1_789_040_100),
                integrity: Some("sha256-runtime-file-initial".to_string()),
                retention_policy: Some("keep-latest".to_string()),
            })
            .expect("publisher writes initial typed file metadata");
        publisher_database
            .upsert_storage_provider(&StorageProviderUpdate {
                profile: DEFAULT_PROFILE_ID.to_string(),
                provider_id: "runtime-provider-tail-1".to_string(),
                provider_kind: "ipfs".to_string(),
                display_name: "Runtime IPFS".to_string(),
                endpoint_ref: Some(
                    "/dnsaddr/runtime-tail.example.test/p2p/runtime-provider-tail-1".to_string(),
                ),
                discovery: true,
                connectivity: true,
                object_transfer: true,
                availability: false,
                mutable_roots: false,
                quota_bytes: Some(4_096),
                max_retained_objects: Some(8),
                pinning_policy: Some("manual".to_string()),
                enabled: true,
            })
            .expect("publisher writes initial typed storage provider metadata");

        let content_key = ProfileSyncContentKey::from_bytes([74; PROFILE_SYNC_CONTENT_KEY_BYTES]);
        let signer = ProfileSyncDeviceSigner::generate("runtime-typed-app-upsert-tail-publisher")
            .expect("generate typed app upsert tail publisher signer");
        receiver_database
            .register_sync_device_public_key(&SyncDevicePublicKeyRegistration {
                profile: DEFAULT_PROFILE_ID.to_string(),
                public_key: signer.public_key().expect("read signer public key"),
                membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            })
            .expect("receiver trusts typed app upsert tail publisher key");
        let publisher = BroadwebdProfileSyncPublisher::new(&publisher_daemon);
        let source = BroadwebdProfileSyncObjectSource::new(&receiver_daemon);
        let chat_watcher = TypedAppSyncDomainWatcher::<ChatConversationSyncPayload>::new(
            receiver_database.clone(),
            DEFAULT_PROFILE_ID,
            SYNC_DOMAIN_CHAT,
            8,
        )
        .expect("initialize receiver chat cursor before typed app snapshot");
        let file_watcher = TypedAppSyncDomainWatcher::<FileEntrySyncPayload>::new(
            receiver_database.clone(),
            DEFAULT_PROFILE_ID,
            SYNC_DOMAIN_FILES,
            8,
        )
        .expect("initialize receiver files cursor before typed app snapshot");
        let storage_watcher = TypedAppSyncDomainWatcher::<StorageProviderSyncPayload>::new(
            receiver_database.clone(),
            DEFAULT_PROFILE_ID,
            SYNC_DOMAIN_STORAGE,
            8,
        )
        .expect("initialize receiver storage cursor before typed app snapshot");
        let initial_chat_revision = chat_watcher
            .current_revision()
            .expect("read initial chat cursor before typed app snapshot");
        let initial_file_revision = file_watcher
            .current_revision()
            .expect("read initial files cursor before typed app snapshot");
        let initial_storage_revision = storage_watcher
            .current_revision()
            .expect("read initial storage cursor before typed app snapshot");
        assert_eq!(
            chat_watcher
                .poll_once()
                .expect("poll idle chat before typed app snapshot")
                .event_count(),
            0
        );
        assert_eq!(
            file_watcher
                .poll_once()
                .expect("poll idle files before typed app snapshot")
                .event_count(),
            0
        );
        assert_eq!(
            storage_watcher
                .poll_once()
                .expect("poll idle storage before typed app snapshot")
                .event_count(),
            0
        );

        let full = publisher
            .publish_full_local_settings_snapshot_head(
                &publisher_database,
                DEFAULT_PROFILE_ID,
                "settings/latest",
                &content_key,
                TEST_CONTENT_KEY_ID,
                &signer,
                ProfileSyncRetentionPolicy::default(),
            )
            .expect("publish initial typed app metadata snapshot head")
            .expect("initial typed app metadata changes exist");
        let full_applied = source
            .pull_record_and_apply_trusted_settings_from_device_head(
                &receiver_database,
                DEFAULT_PROFILE_ID,
                full.device_head.root_id.as_str(),
                &content_key,
                TEST_CONTENT_KEY_ID,
            )
            .expect("receiver applies initial typed app snapshot");
        assert!(matches!(
            full_applied,
            BroadwebdTrustedDeviceHeadSyncStatus::Applied { .. }
        ));
        assert_eq!(
            receiver_database
                .chat_conversations(DEFAULT_PROFILE_ID, 10)
                .expect("read receiver initial typed chat metadata")[0]
                .display_name,
            "Runtime Team"
        );
        assert_eq!(
            receiver_database
                .file_entries(DEFAULT_PROFILE_ID, 10)
                .expect("read receiver initial typed file metadata")[0]
                .name,
            "runtime-initial.txt"
        );
        assert_eq!(
            receiver_database
                .storage_providers(DEFAULT_PROFILE_ID, 10)
                .expect("read receiver initial typed storage provider metadata")[0]
                .availability,
            false
        );
        let snapshot_chat_applied = chat_watcher
            .poll_apply_and_acknowledge(|snapshot_chat_poll| {
                assert!(snapshot_chat_poll.advanced());
                assert_eq!(snapshot_chat_poll.previous_revision, initial_chat_revision);
                assert_eq!(snapshot_chat_poll.event_count(), 1);
                assert_eq!(
                    snapshot_chat_poll.events[0].value.conversation_id,
                    "runtime-chat-tail-1"
                );
                assert_eq!(
                    snapshot_chat_poll.events[0].value.display_name,
                    "Runtime Team"
                );
                Ok::<(), &'static str>(())
            })
            .expect("receiver applies initial typed chat snapshot payload");
        let snapshot_chat_poll = snapshot_chat_applied.poll;
        assert!(snapshot_chat_poll.advanced());
        assert_eq!(snapshot_chat_poll.previous_revision, initial_chat_revision);
        assert_eq!(snapshot_chat_poll.event_count(), 1);
        assert_eq!(
            snapshot_chat_poll.events[0].value.conversation_id,
            "runtime-chat-tail-1"
        );
        assert_eq!(
            snapshot_chat_poll.events[0].value.display_name,
            "Runtime Team"
        );
        assert_eq!(
            snapshot_chat_applied.cursor.latest_revision,
            snapshot_chat_poll.latest_revision
        );

        let snapshot_file_applied = file_watcher
            .poll_apply_and_acknowledge(|snapshot_file_poll| {
                assert!(snapshot_file_poll.advanced());
                assert_eq!(snapshot_file_poll.previous_revision, initial_file_revision);
                assert_eq!(snapshot_file_poll.event_count(), 1);
                assert_eq!(
                    snapshot_file_poll.events[0].value.entry_id,
                    "runtime-file-tail-1"
                );
                assert_eq!(
                    snapshot_file_poll.events[0].value.name,
                    "runtime-initial.txt"
                );
                Ok::<(), &'static str>(())
            })
            .expect("receiver applies initial typed files snapshot payload");
        let snapshot_file_poll = snapshot_file_applied.poll;
        assert!(snapshot_file_poll.advanced());
        assert_eq!(snapshot_file_poll.previous_revision, initial_file_revision);
        assert_eq!(snapshot_file_poll.event_count(), 1);
        assert_eq!(
            snapshot_file_poll.events[0].value.entry_id,
            "runtime-file-tail-1"
        );
        assert_eq!(
            snapshot_file_poll.events[0].value.name,
            "runtime-initial.txt"
        );
        assert_eq!(
            snapshot_file_applied.cursor.latest_revision,
            snapshot_file_poll.latest_revision
        );

        let snapshot_storage_applied = storage_watcher
            .poll_apply_and_acknowledge(|snapshot_storage_poll| {
                assert!(snapshot_storage_poll.advanced());
                assert_eq!(
                    snapshot_storage_poll.previous_revision,
                    initial_storage_revision
                );
                assert_eq!(snapshot_storage_poll.event_count(), 1);
                assert_eq!(
                    snapshot_storage_poll.events[0].value.provider_id,
                    "runtime-provider-tail-1"
                );
                assert!(!snapshot_storage_poll.events[0].value.availability);
                Ok::<(), &'static str>(())
            })
            .expect("receiver applies initial typed storage snapshot payload");
        let snapshot_storage_poll = snapshot_storage_applied.poll;
        assert!(snapshot_storage_poll.advanced());
        assert_eq!(
            snapshot_storage_poll.previous_revision,
            initial_storage_revision
        );
        assert_eq!(snapshot_storage_poll.event_count(), 1);
        assert_eq!(
            snapshot_storage_poll.events[0].value.provider_id,
            "runtime-provider-tail-1"
        );
        assert!(!snapshot_storage_poll.events[0].value.availability);
        assert_eq!(
            snapshot_storage_applied.cursor.latest_revision,
            snapshot_storage_poll.latest_revision
        );

        publisher_database
            .upsert_chat_conversation(&ChatConversationUpdate {
                profile: DEFAULT_PROFILE_ID.to_string(),
                conversation_id: "runtime-chat-tail-1".to_string(),
                provider_id: Some("whatsapp".to_string()),
                external_thread_id: Some("tail-team@example.test".to_string()),
                display_name: "Runtime Team Updated".to_string(),
                avatar_key: Some("chat-avatar:runtime-chat-tail-1-updated".to_string()),
                last_message_at: Some(1_789_040_500),
                unread_count: 7,
                archived: true,
                muted: true,
            })
            .expect("publisher writes typed chat metadata tail update");
        publisher_database
            .upsert_file_entry(&FileEntryUpdate {
                profile: DEFAULT_PROFILE_ID.to_string(),
                entry_id: "runtime-file-tail-1".to_string(),
                sync_set_id: Some("runtime-set".to_string()),
                parent_id: Some("runtime-folder-tail".to_string()),
                name: "runtime-updated.txt".to_string(),
                entry_kind: "file".to_string(),
                content_ref: Some("bafy-runtime-file-updated".to_string()),
                mime_type: Some("text/plain".to_string()),
                size_bytes: Some(2_048),
                modified_at: Some(1_789_040_600),
                integrity: Some("sha256-runtime-file-updated".to_string()),
                retention_policy: Some("keep-pinned".to_string()),
            })
            .expect("publisher writes typed file metadata tail update");
        publisher_database
            .upsert_storage_provider(&StorageProviderUpdate {
                profile: DEFAULT_PROFILE_ID.to_string(),
                provider_id: "runtime-provider-tail-1".to_string(),
                provider_kind: "ipfs".to_string(),
                display_name: "Runtime IPFS Updated".to_string(),
                endpoint_ref: Some(
                    "/dnsaddr/runtime-tail-updated.example.test/p2p/runtime-provider-tail-1"
                        .to_string(),
                ),
                discovery: true,
                connectivity: true,
                object_transfer: true,
                availability: true,
                mutable_roots: true,
                quota_bytes: Some(16_384),
                max_retained_objects: Some(32),
                pinning_policy: Some("auto".to_string()),
                enabled: false,
            })
            .expect("publisher writes typed storage provider metadata tail update");

        let tail = publisher
            .publish_local_settings_tail_head(
                &publisher_database,
                DEFAULT_PROFILE_ID,
                "settings/latest",
                &content_key,
                TEST_CONTENT_KEY_ID,
                &signer,
                ProfileSyncRetentionPolicy::default(),
            )
            .expect("publish typed app metadata tail head")
            .expect("typed app metadata tail changes exist");

        assert_eq!(
            tail.publication.snapshot_object_id,
            full.publication.snapshot_object_id
        );
        assert_eq!(tail.publication.tail_change_object_ids.len(), 3);
        assert_eq!(
            tail.device_head.device_head.latest_change_object_id,
            tail.publication.tail_change_object_ids.last().cloned()
        );

        let applied = source
            .pull_record_and_apply_trusted_settings_from_device_head(
                &receiver_database,
                DEFAULT_PROFILE_ID,
                tail.device_head.root_id.as_str(),
                &content_key,
                TEST_CONTENT_KEY_ID,
            )
            .expect("receiver applies typed app metadata tail from trusted head");
        let BroadwebdTrustedDeviceHeadSyncStatus::Applied { application, .. } = applied else {
            panic!("expected typed app metadata tail application, got {applied:?}");
        };
        assert_eq!(
            application.manifest_object_id,
            tail.publication.manifest_object_id
        );
        assert!(application.snapshot.is_some());
        assert_eq!(application.tail_changes.len(), 3);

        let conversations = receiver_database
            .chat_conversations(DEFAULT_PROFILE_ID, 10)
            .expect("read receiver updated typed chat metadata");
        assert_eq!(conversations.len(), 1);
        assert_eq!(conversations[0].conversation_id, "runtime-chat-tail-1");
        assert_eq!(conversations[0].display_name, "Runtime Team Updated");
        assert_eq!(conversations[0].unread_count, 7);
        assert!(conversations[0].archived);
        assert!(conversations[0].muted);

        let files = receiver_database
            .file_entries(DEFAULT_PROFILE_ID, 10)
            .expect("read receiver updated typed file metadata");
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].entry_id, "runtime-file-tail-1");
        assert_eq!(files[0].parent_id.as_deref(), Some("runtime-folder-tail"));
        assert_eq!(files[0].name, "runtime-updated.txt");
        assert_eq!(
            files[0].content_ref.as_deref(),
            Some("bafy-runtime-file-updated")
        );
        assert_eq!(files[0].size_bytes, Some(2_048));
        assert_eq!(files[0].retention_policy.as_deref(), Some("keep-pinned"));

        let providers = receiver_database
            .storage_providers(DEFAULT_PROFILE_ID, 10)
            .expect("read receiver updated typed storage provider metadata");
        assert_eq!(providers.len(), 1);
        assert_eq!(providers[0].provider_id, "runtime-provider-tail-1");
        assert_eq!(providers[0].display_name, "Runtime IPFS Updated");
        assert!(providers[0].availability);
        assert!(providers[0].mutable_roots);
        assert_eq!(providers[0].quota_bytes, Some(16_384));
        assert_eq!(providers[0].max_retained_objects, Some(32));
        assert_eq!(providers[0].pinning_policy.as_deref(), Some("auto"));
        assert!(!providers[0].enabled);

        let tail_chat_applied = chat_watcher
            .poll_apply_and_acknowledge(|tail_chat_poll| {
                assert!(tail_chat_poll.advanced());
                assert_eq!(
                    tail_chat_poll.previous_revision,
                    snapshot_chat_poll.latest_revision
                );
                assert_eq!(tail_chat_poll.event_count(), 1);
                assert_eq!(
                    tail_chat_poll.events[0].value.conversation_id,
                    "runtime-chat-tail-1"
                );
                assert_eq!(
                    tail_chat_poll.events[0].value.display_name,
                    "Runtime Team Updated"
                );
                assert_eq!(tail_chat_poll.events[0].value.unread_count, 7);
                assert!(tail_chat_poll.events[0].value.archived);
                assert!(tail_chat_poll.events[0].value.muted);
                Ok::<(), &'static str>(())
            })
            .expect("receiver applies typed chat tail payload");
        let tail_chat_poll = tail_chat_applied.poll;
        assert!(tail_chat_poll.advanced());
        assert_eq!(
            tail_chat_poll.previous_revision,
            snapshot_chat_poll.latest_revision
        );
        assert_eq!(tail_chat_poll.event_count(), 1);
        assert_eq!(
            tail_chat_poll.events[0].value.conversation_id,
            "runtime-chat-tail-1"
        );
        assert_eq!(
            tail_chat_poll.events[0].value.display_name,
            "Runtime Team Updated"
        );
        assert_eq!(tail_chat_poll.events[0].value.unread_count, 7);
        assert!(tail_chat_poll.events[0].value.archived);
        assert!(tail_chat_poll.events[0].value.muted);
        assert_eq!(
            tail_chat_applied.cursor.latest_revision,
            tail_chat_poll.latest_revision
        );

        let tail_file_applied = file_watcher
            .poll_apply_and_acknowledge(|tail_file_poll| {
                assert!(tail_file_poll.advanced());
                assert_eq!(
                    tail_file_poll.previous_revision,
                    snapshot_file_poll.latest_revision
                );
                assert_eq!(tail_file_poll.event_count(), 1);
                assert_eq!(
                    tail_file_poll.events[0].value.entry_id,
                    "runtime-file-tail-1"
                );
                assert_eq!(tail_file_poll.events[0].value.name, "runtime-updated.txt");
                assert_eq!(tail_file_poll.events[0].value.size_bytes, Some(2_048));
                assert_eq!(
                    tail_file_poll.events[0].value.retention_policy.as_deref(),
                    Some("keep-pinned")
                );
                Ok::<(), &'static str>(())
            })
            .expect("receiver applies typed files tail payload");
        let tail_file_poll = tail_file_applied.poll;
        assert!(tail_file_poll.advanced());
        assert_eq!(
            tail_file_poll.previous_revision,
            snapshot_file_poll.latest_revision
        );
        assert_eq!(tail_file_poll.event_count(), 1);
        assert_eq!(
            tail_file_poll.events[0].value.entry_id,
            "runtime-file-tail-1"
        );
        assert_eq!(tail_file_poll.events[0].value.name, "runtime-updated.txt");
        assert_eq!(tail_file_poll.events[0].value.size_bytes, Some(2_048));
        assert_eq!(
            tail_file_poll.events[0].value.retention_policy.as_deref(),
            Some("keep-pinned")
        );
        assert_eq!(
            tail_file_applied.cursor.latest_revision,
            tail_file_poll.latest_revision
        );

        let tail_storage_applied = storage_watcher
            .poll_apply_and_acknowledge(|tail_storage_poll| {
                assert!(tail_storage_poll.advanced());
                assert_eq!(
                    tail_storage_poll.previous_revision,
                    snapshot_storage_poll.latest_revision
                );
                assert_eq!(tail_storage_poll.event_count(), 1);
                assert_eq!(
                    tail_storage_poll.events[0].value.provider_id,
                    "runtime-provider-tail-1"
                );
                assert_eq!(
                    tail_storage_poll.events[0].value.display_name,
                    "Runtime IPFS Updated"
                );
                assert!(tail_storage_poll.events[0].value.availability);
                assert!(tail_storage_poll.events[0].value.mutable_roots);
                assert!(!tail_storage_poll.events[0].value.enabled);
                Ok::<(), &'static str>(())
            })
            .expect("receiver applies typed storage tail payload");
        let tail_storage_poll = tail_storage_applied.poll;
        assert!(tail_storage_poll.advanced());
        assert_eq!(
            tail_storage_poll.previous_revision,
            snapshot_storage_poll.latest_revision
        );
        assert_eq!(tail_storage_poll.event_count(), 1);
        assert_eq!(
            tail_storage_poll.events[0].value.provider_id,
            "runtime-provider-tail-1"
        );
        assert_eq!(
            tail_storage_poll.events[0].value.display_name,
            "Runtime IPFS Updated"
        );
        assert!(tail_storage_poll.events[0].value.availability);
        assert!(tail_storage_poll.events[0].value.mutable_roots);
        assert!(!tail_storage_poll.events[0].value.enabled);
        assert_eq!(
            tail_storage_applied.cursor.latest_revision,
            tail_storage_poll.latest_revision
        );

        let _ = std::fs::remove_dir_all(publisher_state_root);
        let _ = std::fs::remove_dir_all(receiver_state_root);
        let _ = std::fs::remove_dir_all(publisher_db_root);
        let _ = std::fs::remove_dir_all(receiver_db_root);
    }

    #[test]
    fn broadwebd_publisher_syncs_typed_app_metadata_tombstone_snapshot_head() {
        let network = InProcessBroadwebNetwork::new();
        let publisher_state_root = test_state_root("typed-app-tombstone-head-publisher");
        let receiver_state_root = test_state_root("typed-app-tombstone-head-receiver");
        let publisher_db_root = test_state_root("typed-app-tombstone-head-publisher-db");
        let receiver_db_root = test_state_root("typed-app-tombstone-head-receiver-db");
        let publisher_daemon = network
            .daemon_for_device(
                &publisher_state_root,
                ResourceBudget::default(),
                "runtime-typed-app-delete-publisher",
            )
            .expect("start typed app tombstone publisher daemon");
        let receiver_daemon = network
            .daemon_for_device(
                &receiver_state_root,
                ResourceBudget::default(),
                "runtime-typed-app-delete-receiver",
            )
            .expect("start typed app tombstone receiver daemon");
        let publisher_database = SlateProfileDatabase::open_resolved_with_device_id(
            publisher_db_root.join(DEFAULT_DATABASE_FILE_NAME),
            "runtime-typed-app-delete-publisher",
        )
        .expect("open typed app tombstone publisher settings database");
        let receiver_database = SlateProfileDatabase::open_resolved_with_device_id(
            receiver_db_root.join(DEFAULT_DATABASE_FILE_NAME),
            "runtime-typed-app-delete-receiver",
        )
        .expect("open typed app tombstone receiver settings database");
        for (domain, privacy_classification, sync_content) in [
            (SYNC_DOMAIN_CHAT, "sensitive", false),
            (SYNC_DOMAIN_FILES, "content", true),
            (SYNC_DOMAIN_STORAGE, "sensitive", false),
        ] {
            publisher_database
                .register_app_sync_domain(&AppSyncDomainRegistration {
                    profile: DEFAULT_PROFILE_ID.to_string(),
                    domain: domain.to_string(),
                    schema_version: 1,
                    enabled: true,
                    privacy_classification: privacy_classification.to_string(),
                    sync_content,
                })
                .expect("enable typed app tombstone sync domain for publisher test profile");
        }

        let chat_update = ChatConversationUpdate {
            profile: DEFAULT_PROFILE_ID.to_string(),
            conversation_id: "runtime-chat-delete".to_string(),
            provider_id: Some("whatsapp".to_string()),
            external_thread_id: Some("old-team@example.test".to_string()),
            display_name: "Old Runtime Team".to_string(),
            avatar_key: Some("chat-avatar:runtime-chat-delete".to_string()),
            last_message_at: Some(1_789_020_000),
            unread_count: 2,
            archived: false,
            muted: false,
        };
        let file_update = FileEntryUpdate {
            profile: DEFAULT_PROFILE_ID.to_string(),
            entry_id: "runtime-file-delete".to_string(),
            sync_set_id: Some("runtime-set".to_string()),
            parent_id: None,
            name: "old-runtime.txt".to_string(),
            entry_kind: "file".to_string(),
            content_ref: Some("bafy-runtime-delete".to_string()),
            mime_type: Some("text/plain".to_string()),
            size_bytes: Some(1_024),
            modified_at: Some(1_789_020_100),
            integrity: Some("sha256-runtime-delete".to_string()),
            retention_policy: Some("keep-latest".to_string()),
        };
        let provider_update = StorageProviderUpdate {
            profile: DEFAULT_PROFILE_ID.to_string(),
            provider_id: "runtime-provider-delete".to_string(),
            provider_kind: "ipfs".to_string(),
            display_name: "Old Runtime IPFS".to_string(),
            endpoint_ref: Some(
                "/dnsaddr/old-runtime.example.test/p2p/runtime-provider-delete".to_string(),
            ),
            discovery: true,
            connectivity: true,
            object_transfer: true,
            availability: true,
            mutable_roots: false,
            quota_bytes: Some(4_096),
            max_retained_objects: Some(8),
            pinning_policy: Some("manual".to_string()),
            enabled: true,
        };
        for database in [&publisher_database, &receiver_database] {
            database
                .upsert_chat_conversation(&chat_update)
                .expect("seed typed chat metadata");
            database
                .upsert_file_entry(&file_update)
                .expect("seed typed file metadata");
            database
                .upsert_storage_provider(&provider_update)
                .expect("seed typed storage provider metadata");
        }
        publisher_database
            .remove_chat_conversation(DEFAULT_PROFILE_ID, chat_update.conversation_id.as_str())
            .expect("publisher tombstones typed chat metadata");
        publisher_database
            .remove_file_entry(DEFAULT_PROFILE_ID, file_update.entry_id.as_str())
            .expect("publisher tombstones typed file metadata");
        publisher_database
            .remove_storage_provider(DEFAULT_PROFILE_ID, provider_update.provider_id.as_str())
            .expect("publisher tombstones typed storage provider metadata");

        let content_key = ProfileSyncContentKey::from_bytes([72; PROFILE_SYNC_CONTENT_KEY_BYTES]);
        let signer = ProfileSyncDeviceSigner::generate("runtime-typed-app-delete-publisher")
            .expect("generate typed app tombstone publisher signer");
        receiver_database
            .register_sync_device_public_key(&SyncDevicePublicKeyRegistration {
                profile: DEFAULT_PROFILE_ID.to_string(),
                public_key: signer.public_key().expect("read signer public key"),
                membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            })
            .expect("receiver trusts typed app tombstone publisher key");

        let published = BroadwebdProfileSyncPublisher::new(&publisher_daemon)
            .publish_full_local_settings_snapshot_head(
                &publisher_database,
                DEFAULT_PROFILE_ID,
                "settings/latest",
                &content_key,
                TEST_CONTENT_KEY_ID,
                &signer,
                ProfileSyncRetentionPolicy::default(),
            )
            .expect("publish typed app tombstone snapshot head")
            .expect("typed app tombstone metadata changes exist");

        assert_eq!(
            published.publication.manifest.included_domains,
            vec![
                SYNC_DOMAIN_CHAT.to_string(),
                SYNC_DOMAIN_FILES.to_string(),
                SYNC_DOMAIN_SETTINGS.to_string(),
                SYNC_DOMAIN_STORAGE.to_string()
            ]
        );
        assert_eq!(
            published.publication.tail_change_object_ids,
            Vec::<String>::new()
        );

        let applied = BroadwebdProfileSyncObjectSource::new(&receiver_daemon)
            .pull_record_and_apply_trusted_settings_from_device_head(
                &receiver_database,
                DEFAULT_PROFILE_ID,
                published.device_head.root_id.as_str(),
                &content_key,
                TEST_CONTENT_KEY_ID,
            )
            .expect("receiver applies typed app tombstone snapshot from trusted head");
        let BroadwebdTrustedDeviceHeadSyncStatus::Applied { application, .. } = applied else {
            panic!("expected typed app tombstone snapshot application, got {applied:?}");
        };
        assert_eq!(
            application.manifest_object_id,
            published.publication.manifest_object_id
        );
        assert!(application.snapshot.is_some());
        assert_eq!(application.tail_changes, Vec::<SyncChangeRecord>::new());

        assert!(
            receiver_database
                .chat_conversations(DEFAULT_PROFILE_ID, 10)
                .expect("read receiver typed chat metadata")
                .is_empty()
        );
        assert!(
            receiver_database
                .file_entries(DEFAULT_PROFILE_ID, 10)
                .expect("read receiver typed file metadata")
                .is_empty()
        );
        assert!(
            receiver_database
                .storage_providers(DEFAULT_PROFILE_ID, 10)
                .expect("read receiver typed storage provider metadata")
                .is_empty()
        );

        let chat_value = receiver_database
            .get_sync_setting_text(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_CHAT,
                "conversation.runtime-chat-delete",
            )
            .expect("read receiver chat tombstone sync setting")
            .expect("receiver chat tombstone sync setting")
            .value;
        let chat_payload: ChatConversationSyncPayload =
            serde_json::from_str(chat_value.as_str()).expect("decode chat tombstone payload");
        assert!(chat_payload.deleted);
        assert_eq!(chat_payload.conversation_id, "runtime-chat-delete");

        let file_value = receiver_database
            .get_sync_setting_text(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_FILES,
                "entry.runtime-file-delete",
            )
            .expect("read receiver file tombstone sync setting")
            .expect("receiver file tombstone sync setting")
            .value;
        let file_payload: FileEntrySyncPayload =
            serde_json::from_str(file_value.as_str()).expect("decode file tombstone payload");
        assert!(file_payload.deleted);
        assert_eq!(file_payload.entry_id, "runtime-file-delete");

        let provider_value = receiver_database
            .get_sync_setting_text(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_STORAGE,
                "provider.runtime-provider-delete",
            )
            .expect("read receiver storage provider tombstone sync setting")
            .expect("receiver storage provider tombstone sync setting")
            .value;
        let provider_payload: StorageProviderSyncPayload =
            serde_json::from_str(provider_value.as_str())
                .expect("decode storage provider tombstone payload");
        assert!(provider_payload.deleted);
        assert_eq!(provider_payload.provider_id, "runtime-provider-delete");

        let _ = std::fs::remove_dir_all(publisher_state_root);
        let _ = std::fs::remove_dir_all(receiver_state_root);
        let _ = std::fs::remove_dir_all(publisher_db_root);
        let _ = std::fs::remove_dir_all(receiver_db_root);
    }

    #[test]
    fn broadwebd_publisher_syncs_typed_app_metadata_tombstone_tail_head() {
        let network = InProcessBroadwebNetwork::new();
        let publisher_state_root = test_state_root("typed-app-tombstone-tail-publisher");
        let receiver_state_root = test_state_root("typed-app-tombstone-tail-receiver");
        let publisher_db_root = test_state_root("typed-app-tombstone-tail-publisher-db");
        let receiver_db_root = test_state_root("typed-app-tombstone-tail-receiver-db");
        let publisher_daemon = network
            .daemon_for_device(
                &publisher_state_root,
                ResourceBudget::default(),
                "runtime-typed-app-tail-publisher",
            )
            .expect("start typed app tombstone tail publisher daemon");
        let receiver_daemon = network
            .daemon_for_device(
                &receiver_state_root,
                ResourceBudget::default(),
                "runtime-typed-app-tail-receiver",
            )
            .expect("start typed app tombstone tail receiver daemon");
        let publisher_database = SlateProfileDatabase::open_resolved_with_device_id(
            publisher_db_root.join(DEFAULT_DATABASE_FILE_NAME),
            "runtime-typed-app-tail-publisher",
        )
        .expect("open typed app tombstone tail publisher settings database");
        let receiver_database = SlateProfileDatabase::open_resolved_with_device_id(
            receiver_db_root.join(DEFAULT_DATABASE_FILE_NAME),
            "runtime-typed-app-tail-receiver",
        )
        .expect("open typed app tombstone tail receiver settings database");
        publisher_database
            .register_app_sync_domain(&AppSyncDomainRegistration {
                profile: DEFAULT_PROFILE_ID.to_string(),
                domain: SYNC_DOMAIN_CHAT.to_string(),
                schema_version: 1,
                enabled: true,
                privacy_classification: "sensitive".to_string(),
                sync_content: false,
            })
            .expect("enable chat sync domain for tombstone tail test profile");
        let chat_update = ChatConversationUpdate {
            profile: DEFAULT_PROFILE_ID.to_string(),
            conversation_id: "runtime-chat-tail-delete".to_string(),
            provider_id: Some("whatsapp".to_string()),
            external_thread_id: Some("tail-team@example.test".to_string()),
            display_name: "Tail Runtime Team".to_string(),
            avatar_key: Some("chat-avatar:runtime-chat-tail-delete".to_string()),
            last_message_at: Some(1_789_030_000),
            unread_count: 1,
            archived: false,
            muted: false,
        };
        publisher_database
            .upsert_chat_conversation(&chat_update)
            .expect("publisher writes typed chat metadata before snapshot");

        let content_key = ProfileSyncContentKey::from_bytes([73; PROFILE_SYNC_CONTENT_KEY_BYTES]);
        let signer = ProfileSyncDeviceSigner::generate("runtime-typed-app-tail-publisher")
            .expect("generate typed app tombstone tail publisher signer");
        receiver_database
            .register_sync_device_public_key(&SyncDevicePublicKeyRegistration {
                profile: DEFAULT_PROFILE_ID.to_string(),
                public_key: signer.public_key().expect("read signer public key"),
                membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            })
            .expect("receiver trusts typed app tombstone tail publisher key");
        let publisher = BroadwebdProfileSyncPublisher::new(&publisher_daemon);
        let source = BroadwebdProfileSyncObjectSource::new(&receiver_daemon);
        let chat_watcher = TypedAppSyncDomainWatcher::<ChatConversationSyncPayload>::new(
            receiver_database.clone(),
            DEFAULT_PROFILE_ID,
            SYNC_DOMAIN_CHAT,
            8,
        )
        .expect("initialize receiver chat cursor before tombstone tail snapshot");
        let initial_chat_revision = chat_watcher
            .current_revision()
            .expect("read initial receiver chat cursor before tombstone tail snapshot");
        assert_eq!(
            chat_watcher
                .poll_once()
                .expect("poll idle chat before tombstone tail snapshot")
                .event_count(),
            0
        );

        let full = publisher
            .publish_full_local_settings_snapshot_head(
                &publisher_database,
                DEFAULT_PROFILE_ID,
                "settings/latest",
                &content_key,
                TEST_CONTENT_KEY_ID,
                &signer,
                ProfileSyncRetentionPolicy::default(),
            )
            .expect("publish typed app metadata snapshot head")
            .expect("typed app metadata changes exist before deletion");
        let full_applied = source
            .pull_record_and_apply_trusted_settings_from_device_head(
                &receiver_database,
                DEFAULT_PROFILE_ID,
                full.device_head.root_id.as_str(),
                &content_key,
                TEST_CONTENT_KEY_ID,
            )
            .expect("receiver applies typed app snapshot before tombstone tail");
        assert!(matches!(
            full_applied,
            BroadwebdTrustedDeviceHeadSyncStatus::Applied { .. }
        ));
        assert_eq!(
            receiver_database
                .chat_conversations(DEFAULT_PROFILE_ID, 10)
                .expect("read receiver typed chat metadata")
                .len(),
            1
        );
        let snapshot_chat_applied = chat_watcher
            .poll_apply_and_acknowledge(|snapshot_chat_poll| {
                assert!(snapshot_chat_poll.advanced());
                assert_eq!(snapshot_chat_poll.previous_revision, initial_chat_revision);
                assert_eq!(snapshot_chat_poll.event_count(), 1);
                assert_eq!(
                    snapshot_chat_poll.events[0].value.conversation_id,
                    "runtime-chat-tail-delete"
                );
                assert_eq!(
                    snapshot_chat_poll.events[0].value.display_name,
                    "Tail Runtime Team"
                );
                assert!(!snapshot_chat_poll.events[0].value.deleted);
                Ok::<(), &'static str>(())
            })
            .expect("receiver applies typed chat snapshot before tombstone tail");
        let snapshot_chat_poll = snapshot_chat_applied.poll;
        assert!(snapshot_chat_poll.advanced());
        assert_eq!(snapshot_chat_poll.previous_revision, initial_chat_revision);
        assert_eq!(snapshot_chat_poll.event_count(), 1);
        assert_eq!(
            snapshot_chat_poll.events[0].value.conversation_id,
            "runtime-chat-tail-delete"
        );
        assert_eq!(
            snapshot_chat_poll.events[0].value.display_name,
            "Tail Runtime Team"
        );
        assert!(!snapshot_chat_poll.events[0].value.deleted);
        assert_eq!(
            snapshot_chat_applied.cursor.latest_revision,
            snapshot_chat_poll.latest_revision
        );

        publisher_database
            .remove_chat_conversation(DEFAULT_PROFILE_ID, chat_update.conversation_id.as_str())
            .expect("publisher tombstones typed chat metadata after snapshot");
        let tail = publisher
            .publish_local_settings_tail_head(
                &publisher_database,
                DEFAULT_PROFILE_ID,
                "settings/latest",
                &content_key,
                TEST_CONTENT_KEY_ID,
                &signer,
                ProfileSyncRetentionPolicy::default(),
            )
            .expect("publish typed app metadata tombstone tail head")
            .expect("typed app tombstone tail changes exist");

        assert_eq!(
            tail.publication.snapshot_object_id,
            full.publication.snapshot_object_id
        );
        assert_eq!(tail.publication.tail_change_object_ids.len(), 1);
        assert_eq!(
            tail.device_head.device_head.latest_change_object_id,
            tail.publication.tail_change_object_ids.first().cloned()
        );

        let applied = source
            .pull_record_and_apply_trusted_settings_from_device_head(
                &receiver_database,
                DEFAULT_PROFILE_ID,
                tail.device_head.root_id.as_str(),
                &content_key,
                TEST_CONTENT_KEY_ID,
            )
            .expect("receiver applies typed app tombstone tail from trusted head");
        let BroadwebdTrustedDeviceHeadSyncStatus::Applied { application, .. } = applied else {
            panic!("expected typed app tombstone tail application, got {applied:?}");
        };
        assert_eq!(
            application.manifest_object_id,
            tail.publication.manifest_object_id
        );
        assert!(application.snapshot.is_some());
        assert_eq!(application.tail_changes.len(), 1);
        assert!(
            receiver_database
                .chat_conversations(DEFAULT_PROFILE_ID, 10)
                .expect("read receiver typed chat metadata after tombstone tail")
                .is_empty()
        );

        let chat_value = receiver_database
            .get_sync_setting_text(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_CHAT,
                "conversation.runtime-chat-tail-delete",
            )
            .expect("read receiver chat tombstone tail sync setting")
            .expect("receiver chat tombstone tail sync setting")
            .value;
        let chat_payload: ChatConversationSyncPayload =
            serde_json::from_str(chat_value.as_str()).expect("decode chat tombstone tail payload");
        assert!(chat_payload.deleted);
        assert_eq!(chat_payload.conversation_id, "runtime-chat-tail-delete");

        let tombstone_chat_applied = chat_watcher
            .poll_apply_and_acknowledge(|tombstone_chat_poll| {
                assert!(tombstone_chat_poll.advanced());
                assert_eq!(
                    tombstone_chat_poll.previous_revision,
                    snapshot_chat_poll.latest_revision
                );
                assert_eq!(tombstone_chat_poll.event_count(), 1);
                assert_eq!(
                    tombstone_chat_poll.events[0].change.entity_key,
                    "conversation.runtime-chat-tail-delete"
                );
                assert!(tombstone_chat_poll.events[0].value.deleted);
                assert_eq!(
                    tombstone_chat_poll.events[0].value.conversation_id,
                    "runtime-chat-tail-delete"
                );
                Ok::<(), &'static str>(())
            })
            .expect("receiver applies typed chat tombstone tail payload");
        let tombstone_chat_poll = tombstone_chat_applied.poll;
        assert!(tombstone_chat_poll.advanced());
        assert_eq!(
            tombstone_chat_poll.previous_revision,
            snapshot_chat_poll.latest_revision
        );
        assert_eq!(tombstone_chat_poll.event_count(), 1);
        assert_eq!(
            tombstone_chat_poll.events[0].change.entity_key,
            "conversation.runtime-chat-tail-delete"
        );
        assert!(tombstone_chat_poll.events[0].value.deleted);
        assert_eq!(
            tombstone_chat_poll.events[0].value.conversation_id,
            "runtime-chat-tail-delete"
        );
        assert_eq!(
            tombstone_chat_applied.cursor.latest_revision,
            tombstone_chat_poll.latest_revision
        );

        let _ = std::fs::remove_dir_all(publisher_state_root);
        let _ = std::fs::remove_dir_all(receiver_state_root);
        let _ = std::fs::remove_dir_all(publisher_db_root);
        let _ = std::fs::remove_dir_all(receiver_db_root);
    }

    #[test]
    fn enabled_settings_sync_events_preserve_order_without_disabled_domains() {
        let db_root = test_state_root("enabled-domain-event-feed-db");
        let database =
            SlateProfileDatabase::open_resolved(db_root.join(DEFAULT_DATABASE_FILE_NAME))
                .expect("open settings database");
        let baseline_revision = database
            .latest_sync_revision(DEFAULT_PROFILE_ID)
            .expect("read baseline revision");
        database
            .register_app_sync_domain(&AppSyncDomainRegistration {
                profile: DEFAULT_PROFILE_ID.to_string(),
                domain: SYNC_DOMAIN_CALENDAR.to_string(),
                schema_version: 1,
                enabled: true,
                privacy_classification: "sensitive".to_string(),
                sync_content: false,
            })
            .expect("enable calendar sync for test profile");

        let first_settings = database
            .set_sync_setting_text(DEFAULT_PROFILE_ID, SYNC_DOMAIN_SETTINGS, "ui.theme", "teal")
            .expect("write first enabled settings event");
        database
            .set_sync_setting_text(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_CHAT,
                "last_thread",
                "thread-1",
            )
            .expect("write disabled chat event");
        let calendar = database
            .set_sync_setting_text(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_CALENDAR,
                "default_view",
                "month",
            )
            .expect("write enabled calendar event");
        let second_settings = database
            .set_sync_setting_text(DEFAULT_PROFILE_ID, SYNC_DOMAIN_SETTINGS, "ui.zoom", "110")
            .expect("write second enabled settings event");

        let first_batch = super::enabled_settings_sync_text_events_after(
            &database,
            DEFAULT_PROFILE_ID,
            baseline_revision,
            2,
        )
        .expect("read first enabled event batch");
        assert_eq!(
            first_batch
                .iter()
                .map(|event| event.change.clone())
                .collect::<Vec<_>>(),
            vec![first_settings.clone(), calendar.clone()]
        );

        let next_batch = super::enabled_settings_sync_text_events_after(
            &database,
            DEFAULT_PROFILE_ID,
            first_batch[0].revision.revision,
            10,
        )
        .expect("read next enabled event batch");
        assert_eq!(
            next_batch
                .iter()
                .map(|event| event.change.clone())
                .collect::<Vec<_>>(),
            vec![calendar, second_settings]
        );

        let _ = std::fs::remove_dir_all(db_root);
    }

    #[test]
    fn broadwebd_publisher_skips_disabled_app_domains_from_local_publish() {
        let network = InProcessBroadwebNetwork::new();
        let publisher_state_root = test_state_root("disabled-domain-publisher");
        let receiver_state_root = test_state_root("disabled-domain-receiver");
        let publisher_db_root = test_state_root("disabled-domain-publisher-db");
        let receiver_db_root = test_state_root("disabled-domain-receiver-db");
        let publisher_daemon = network
            .daemon_for_device(
                &publisher_state_root,
                ResourceBudget::default(),
                "runtime-disabled-domain-publisher",
            )
            .expect("start publisher daemon");
        let receiver_daemon = network
            .daemon_for_device(
                &receiver_state_root,
                ResourceBudget::default(),
                "runtime-disabled-domain-receiver",
            )
            .expect("start receiver daemon");
        let publisher_database = SlateProfileDatabase::open_resolved_with_device_id(
            publisher_db_root.join(DEFAULT_DATABASE_FILE_NAME),
            "runtime-disabled-domain-publisher",
        )
        .expect("open publisher settings database");
        let receiver_database = SlateProfileDatabase::open_resolved_with_device_id(
            receiver_db_root.join(DEFAULT_DATABASE_FILE_NAME),
            "runtime-disabled-domain-receiver",
        )
        .expect("open receiver settings database");
        publisher_database
            .set_sync_setting_text(DEFAULT_PROFILE_ID, SYNC_DOMAIN_SETTINGS, "ui.theme", "teal")
            .expect("publisher writes enabled setting");
        publisher_database
            .set_sync_setting_text(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_CALENDAR,
                "default_view",
                "month",
            )
            .expect("publisher writes disabled calendar setting");
        let content_key = ProfileSyncContentKey::from_bytes([66; PROFILE_SYNC_CONTENT_KEY_BYTES]);
        let signer = ProfileSyncDeviceSigner::generate("runtime-disabled-domain-publisher")
            .expect("generate signer");
        let public_key = signer.public_key().expect("read signer public key");
        receiver_database
            .register_sync_device_public_key(&SyncDevicePublicKeyRegistration {
                profile: DEFAULT_PROFILE_ID.to_string(),
                public_key,
                membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            })
            .expect("receiver trusts publisher key");
        let publisher = BroadwebdProfileSyncPublisher::new(&publisher_daemon);

        let published = publisher
            .publish_full_local_settings_snapshot_head(
                &publisher_database,
                DEFAULT_PROFILE_ID,
                "settings/latest",
                &content_key,
                TEST_CONTENT_KEY_ID,
                &signer,
                ProfileSyncRetentionPolicy::default(),
            )
            .expect("publish enabled app-domain snapshot head")
            .expect("enabled settings changes exist");

        assert_eq!(
            published.publication.manifest.included_domains,
            vec![SYNC_DOMAIN_SETTINGS.to_string()]
        );
        assert_eq!(
            published.snapshot_record.included_domains,
            vec![SYNC_DOMAIN_SETTINGS.to_string()]
        );

        let source = BroadwebdProfileSyncObjectSource::new(&receiver_daemon);
        let applied = source
            .pull_record_and_apply_trusted_settings_from_device_head(
                &receiver_database,
                DEFAULT_PROFILE_ID,
                published.device_head.root_id.as_str(),
                &content_key,
                TEST_CONTENT_KEY_ID,
            )
            .expect("receiver applies full snapshot from trusted head");
        assert!(matches!(
            applied,
            BroadwebdTrustedDeviceHeadSyncStatus::Applied { .. }
        ));
        assert_eq!(
            receiver_database
                .get_setting_text("ui.theme")
                .expect("read receiver theme")
                .as_deref(),
            Some("teal")
        );
        assert!(
            receiver_database
                .get_sync_setting_text(DEFAULT_PROFILE_ID, SYNC_DOMAIN_CALENDAR, "default_view")
                .expect("read receiver calendar sync setting")
                .is_none()
        );

        publisher_database
            .set_sync_setting_text(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_CALENDAR,
                "default_view",
                "week",
            )
            .expect("publisher writes disabled calendar tail setting");
        let pending = publisher
            .publish_pending_local_settings_head(
                &publisher_database,
                DEFAULT_PROFILE_ID,
                "settings/latest",
                &content_key,
                TEST_CONTENT_KEY_ID,
                &signer,
                ProfileSyncRetentionPolicy::default(),
            )
            .expect("disabled calendar tail should not publish");
        assert!(matches!(
            pending,
            LocalSettingsHeadPublishStatus::UpToDate { .. }
        ));

        let _ = std::fs::remove_dir_all(publisher_state_root);
        let _ = std::fs::remove_dir_all(receiver_state_root);
        let _ = std::fs::remove_dir_all(publisher_db_root);
        let _ = std::fs::remove_dir_all(receiver_db_root);
    }

    #[test]
    fn broadwebd_publisher_skips_disabled_typed_app_metadata_from_local_publish() {
        let network = InProcessBroadwebNetwork::new();
        let publisher_state_root = test_state_root("disabled-typed-domain-publisher");
        let receiver_state_root = test_state_root("disabled-typed-domain-receiver");
        let publisher_db_root = test_state_root("disabled-typed-domain-publisher-db");
        let receiver_db_root = test_state_root("disabled-typed-domain-receiver-db");
        let publisher_daemon = network
            .daemon_for_device(
                &publisher_state_root,
                ResourceBudget::default(),
                "runtime-disabled-typed-domain-publisher",
            )
            .expect("start disabled typed domain publisher daemon");
        let receiver_daemon = network
            .daemon_for_device(
                &receiver_state_root,
                ResourceBudget::default(),
                "runtime-disabled-typed-domain-receiver",
            )
            .expect("start disabled typed domain receiver daemon");
        let publisher_database = SlateProfileDatabase::open_resolved_with_device_id(
            publisher_db_root.join(DEFAULT_DATABASE_FILE_NAME),
            "runtime-disabled-typed-domain-publisher",
        )
        .expect("open disabled typed domain publisher settings database");
        let receiver_database = SlateProfileDatabase::open_resolved_with_device_id(
            receiver_db_root.join(DEFAULT_DATABASE_FILE_NAME),
            "runtime-disabled-typed-domain-receiver",
        )
        .expect("open disabled typed domain receiver settings database");
        publisher_database
            .set_sync_setting_text(DEFAULT_PROFILE_ID, SYNC_DOMAIN_SETTINGS, "ui.theme", "teal")
            .expect("publisher writes enabled setting");
        publisher_database
            .upsert_chat_conversation(&ChatConversationUpdate {
                profile: DEFAULT_PROFILE_ID.to_string(),
                conversation_id: "runtime-disabled-chat-1".to_string(),
                provider_id: Some("whatsapp".to_string()),
                external_thread_id: Some("private-thread@example.test".to_string()),
                display_name: "Private Team".to_string(),
                avatar_key: Some("chat-avatar:runtime-disabled-chat-1".to_string()),
                last_message_at: Some(1_789_050_000),
                unread_count: 3,
                archived: false,
                muted: false,
            })
            .expect("publisher writes disabled typed chat metadata");
        let content_key = ProfileSyncContentKey::from_bytes([75; PROFILE_SYNC_CONTENT_KEY_BYTES]);
        let signer = ProfileSyncDeviceSigner::generate("runtime-disabled-typed-domain-publisher")
            .expect("generate disabled typed domain publisher signer");
        receiver_database
            .register_sync_device_public_key(&SyncDevicePublicKeyRegistration {
                profile: DEFAULT_PROFILE_ID.to_string(),
                public_key: signer.public_key().expect("read signer public key"),
                membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            })
            .expect("receiver trusts disabled typed domain publisher key");
        let publisher = BroadwebdProfileSyncPublisher::new(&publisher_daemon);

        let published = publisher
            .publish_full_local_settings_snapshot_head(
                &publisher_database,
                DEFAULT_PROFILE_ID,
                "settings/latest",
                &content_key,
                TEST_CONTENT_KEY_ID,
                &signer,
                ProfileSyncRetentionPolicy::default(),
            )
            .expect("publish enabled-only snapshot head")
            .expect("enabled settings changes exist");

        assert_eq!(
            published.publication.manifest.included_domains,
            vec![SYNC_DOMAIN_SETTINGS.to_string()]
        );
        assert_eq!(
            published.publication.tail_change_object_ids,
            Vec::<String>::new()
        );

        let applied = BroadwebdProfileSyncObjectSource::new(&receiver_daemon)
            .pull_record_and_apply_trusted_settings_from_device_head(
                &receiver_database,
                DEFAULT_PROFILE_ID,
                published.device_head.root_id.as_str(),
                &content_key,
                TEST_CONTENT_KEY_ID,
            )
            .expect("receiver applies enabled-only snapshot from trusted head");
        assert!(matches!(
            applied,
            BroadwebdTrustedDeviceHeadSyncStatus::Applied { .. }
        ));
        assert_eq!(
            receiver_database
                .get_setting_text("ui.theme")
                .expect("read receiver theme")
                .as_deref(),
            Some("teal")
        );
        assert!(
            receiver_database
                .get_sync_setting_text(
                    DEFAULT_PROFILE_ID,
                    SYNC_DOMAIN_CHAT,
                    "conversation.runtime-disabled-chat-1"
                )
                .expect("read receiver disabled chat sync setting")
                .is_none()
        );
        assert!(
            receiver_database
                .chat_conversations(DEFAULT_PROFILE_ID, 10)
                .expect("read receiver disabled typed chat metadata")
                .is_empty()
        );

        publisher_database
            .upsert_chat_conversation(&ChatConversationUpdate {
                profile: DEFAULT_PROFILE_ID.to_string(),
                conversation_id: "runtime-disabled-chat-2".to_string(),
                provider_id: Some("sms".to_string()),
                external_thread_id: Some("+15550101010".to_string()),
                display_name: "Private SMS".to_string(),
                avatar_key: None,
                last_message_at: Some(1_789_050_100),
                unread_count: 1,
                archived: false,
                muted: true,
            })
            .expect("publisher writes disabled typed chat metadata after snapshot");
        let pending = publisher
            .publish_pending_local_settings_head(
                &publisher_database,
                DEFAULT_PROFILE_ID,
                "settings/latest",
                &content_key,
                TEST_CONTENT_KEY_ID,
                &signer,
                ProfileSyncRetentionPolicy::default(),
            )
            .expect("disabled typed chat tail should not publish");
        assert!(matches!(
            pending,
            LocalSettingsHeadPublishStatus::UpToDate { .. }
        ));

        let _ = std::fs::remove_dir_all(publisher_state_root);
        let _ = std::fs::remove_dir_all(receiver_state_root);
        let _ = std::fs::remove_dir_all(publisher_db_root);
        let _ = std::fs::remove_dir_all(receiver_db_root);
    }

    #[test]
    fn broadwebd_publisher_publishes_local_settings_tail_head_from_latest_snapshot() {
        let network = InProcessBroadwebNetwork::new();
        let publisher_state_root = test_state_root("tail-head-publisher");
        let receiver_state_root = test_state_root("tail-head-receiver");
        let publisher_db_root = test_state_root("tail-head-publisher-db");
        let receiver_db_root = test_state_root("tail-head-receiver-db");
        let publisher_daemon = network
            .daemon_for_device(
                &publisher_state_root,
                ResourceBudget::default(),
                "runtime-k",
            )
            .expect("start publisher daemon");
        let receiver_daemon = network
            .daemon_for_device(&receiver_state_root, ResourceBudget::default(), "runtime-l")
            .expect("start receiver daemon");
        let publisher_database = SlateProfileDatabase::open_resolved_with_device_id(
            publisher_db_root.join(DEFAULT_DATABASE_FILE_NAME),
            "runtime-k",
        )
        .expect("open publisher settings database");
        let receiver_database = SlateProfileDatabase::open_resolved_with_device_id(
            receiver_db_root.join(DEFAULT_DATABASE_FILE_NAME),
            "runtime-l",
        )
        .expect("open receiver settings database");
        publisher_database
            .set_sync_setting_text(DEFAULT_PROFILE_ID, SYNC_DOMAIN_SETTINGS, "ui.theme", "teal")
            .expect("publisher writes initial setting");
        let content_key = ProfileSyncContentKey::from_bytes([47; PROFILE_SYNC_CONTENT_KEY_BYTES]);
        let signer = ProfileSyncDeviceSigner::generate("runtime-k").expect("generate signer");
        let public_key = signer.public_key().expect("read signer public key");
        receiver_database
            .register_sync_device_public_key(&SyncDevicePublicKeyRegistration {
                profile: DEFAULT_PROFILE_ID.to_string(),
                public_key,
                membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            })
            .expect("receiver trusts publisher key");
        let publisher = BroadwebdProfileSyncPublisher::new(&publisher_daemon);

        let full = publisher
            .publish_full_local_settings_snapshot_head(
                &publisher_database,
                DEFAULT_PROFILE_ID,
                "settings/latest",
                &content_key,
                TEST_CONTENT_KEY_ID,
                &signer,
                ProfileSyncRetentionPolicy::default(),
            )
            .expect("publish full local snapshot head")
            .expect("initial settings changes exist");
        publisher_database
            .set_sync_setting_text(DEFAULT_PROFILE_ID, SYNC_DOMAIN_SETTINGS, "ui.zoom", "110")
            .expect("publisher writes post-snapshot setting");

        let incremental = publisher
            .publish_local_settings_tail_head(
                &publisher_database,
                DEFAULT_PROFILE_ID,
                "settings/latest",
                &content_key,
                TEST_CONTENT_KEY_ID,
                &signer,
                ProfileSyncRetentionPolicy::default(),
            )
            .expect("publish local settings tail head")
            .expect("post-snapshot tail changes exist");

        assert_eq!(
            incremental.publication.snapshot_object_id,
            full.publication.snapshot_object_id
        );
        assert_eq!(incremental.publication.tail_change_object_ids.len(), 1);
        assert_eq!(
            incremental.device_head.device_head.latest_change_object_id,
            incremental
                .publication
                .tail_change_object_ids
                .first()
                .cloned()
        );
        assert_eq!(
            incremental.settings_root.object_id,
            incremental.publication.manifest_object_id
        );
        assert_eq!(
            incremental.device_head_root.object_id,
            incremental.device_head.object_id
        );

        let source = BroadwebdProfileSyncObjectSource::new(&receiver_daemon);
        let applied = source
            .pull_record_and_apply_trusted_settings_from_device_head(
                &receiver_database,
                DEFAULT_PROFILE_ID,
                incremental.device_head.root_id.as_str(),
                &content_key,
                TEST_CONTENT_KEY_ID,
            )
            .expect("receiver applies incremental tail head");
        let BroadwebdTrustedDeviceHeadSyncStatus::Applied { application, .. } = applied else {
            panic!("expected incremental tail application, got {applied:?}");
        };
        assert_eq!(
            application.manifest_object_id,
            incremental.publication.manifest_object_id
        );
        assert!(application.snapshot.is_some());
        assert_eq!(application.tail_changes.len(), 1);
        assert_eq!(
            receiver_database
                .get_setting_text("ui.theme")
                .expect("read receiver theme")
                .as_deref(),
            Some("teal")
        );
        assert_eq!(
            receiver_database
                .get_setting_text("ui.zoom")
                .expect("read receiver zoom")
                .as_deref(),
            Some("110")
        );

        let _ = std::fs::remove_dir_all(publisher_state_root);
        let _ = std::fs::remove_dir_all(receiver_state_root);
        let _ = std::fs::remove_dir_all(publisher_db_root);
        let _ = std::fs::remove_dir_all(receiver_db_root);
    }

    #[test]
    fn broadwebd_publisher_selects_pending_local_settings_head_publish_step() {
        let network = InProcessBroadwebNetwork::new();
        let publisher_state_root = test_state_root("pending-head-publisher");
        let receiver_state_root = test_state_root("pending-head-receiver");
        let publisher_db_root = test_state_root("pending-head-publisher-db");
        let receiver_db_root = test_state_root("pending-head-receiver-db");
        let publisher_daemon = network
            .daemon_for_device(
                &publisher_state_root,
                ResourceBudget::default(),
                "runtime-m",
            )
            .expect("start publisher daemon");
        let receiver_daemon = network
            .daemon_for_device(&receiver_state_root, ResourceBudget::default(), "runtime-n")
            .expect("start receiver daemon");
        let publisher_database = SlateProfileDatabase::open_resolved_with_device_id(
            publisher_db_root.join(DEFAULT_DATABASE_FILE_NAME),
            "runtime-m",
        )
        .expect("open publisher settings database");
        let receiver_database = SlateProfileDatabase::open_resolved_with_device_id(
            receiver_db_root.join(DEFAULT_DATABASE_FILE_NAME),
            "runtime-n",
        )
        .expect("open receiver settings database");
        let content_key = ProfileSyncContentKey::from_bytes([48; PROFILE_SYNC_CONTENT_KEY_BYTES]);
        let signer = ProfileSyncDeviceSigner::generate("runtime-m").expect("generate signer");
        let public_key = signer.public_key().expect("read signer public key");
        receiver_database
            .register_sync_device_public_key(&SyncDevicePublicKeyRegistration {
                profile: DEFAULT_PROFILE_ID.to_string(),
                public_key,
                membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            })
            .expect("receiver trusts publisher key");
        let publisher = BroadwebdProfileSyncPublisher::new(&publisher_daemon);

        let empty = publisher
            .publish_pending_local_settings_head(
                &publisher_database,
                "empty-profile",
                "settings/latest",
                &content_key,
                TEST_CONTENT_KEY_ID,
                &signer,
                ProfileSyncRetentionPolicy::default(),
            )
            .expect("publish empty pending local settings head");
        assert_eq!(
            empty,
            LocalSettingsHeadPublishStatus::NoLocalSettingsChanges
        );

        publisher_database
            .set_sync_setting_text(DEFAULT_PROFILE_ID, SYNC_DOMAIN_SETTINGS, "ui.theme", "teal")
            .expect("publisher writes initial setting");
        let first = publisher
            .publish_pending_local_settings_head(
                &publisher_database,
                DEFAULT_PROFILE_ID,
                "settings/latest",
                &content_key,
                TEST_CONTENT_KEY_ID,
                &signer,
                ProfileSyncRetentionPolicy::default(),
            )
            .expect("publish first pending local settings head");
        let LocalSettingsHeadPublishStatus::PublishedFullSnapshot(first) = first else {
            panic!("expected first pending publish to create a full snapshot, got {first:?}");
        };
        assert_eq!(first.publication.tail_change_object_ids.len(), 0);

        let unchanged = publisher
            .publish_pending_local_settings_head(
                &publisher_database,
                DEFAULT_PROFILE_ID,
                "settings/latest",
                &content_key,
                TEST_CONTENT_KEY_ID,
                &signer,
                ProfileSyncRetentionPolicy::default(),
            )
            .expect("publish unchanged pending local settings head");
        let LocalSettingsHeadPublishStatus::UpToDate { snapshot_record } = unchanged else {
            panic!("expected unchanged pending publish to be up to date, got {unchanged:?}");
        };
        assert_eq!(
            snapshot_record.snapshot_id,
            first.snapshot_record.snapshot_id
        );

        publisher_database
            .set_sync_setting_text(DEFAULT_PROFILE_ID, SYNC_DOMAIN_SETTINGS, "ui.zoom", "110")
            .expect("publisher writes post-snapshot setting");
        let second = publisher
            .publish_pending_local_settings_head(
                &publisher_database,
                DEFAULT_PROFILE_ID,
                "settings/latest",
                &content_key,
                TEST_CONTENT_KEY_ID,
                &signer,
                ProfileSyncRetentionPolicy::default(),
            )
            .expect("publish second pending local settings head");
        let LocalSettingsHeadPublishStatus::PublishedIncrementalTail(second) = second else {
            panic!("expected second pending publish to create a tail manifest, got {second:?}");
        };
        assert_eq!(
            second.publication.snapshot_object_id,
            first.publication.snapshot_object_id
        );
        assert_eq!(second.publication.tail_change_object_ids.len(), 1);

        let source = BroadwebdProfileSyncObjectSource::new(&receiver_daemon);
        let applied = source
            .pull_record_and_apply_trusted_settings_from_device_head(
                &receiver_database,
                DEFAULT_PROFILE_ID,
                second.device_head.root_id.as_str(),
                &content_key,
                TEST_CONTENT_KEY_ID,
            )
            .expect("receiver applies scheduler-selected tail head");
        let BroadwebdTrustedDeviceHeadSyncStatus::Applied { application, .. } = applied else {
            panic!("expected scheduler-selected tail application, got {applied:?}");
        };
        assert_eq!(application.tail_changes.len(), 1);
        assert_eq!(
            receiver_database
                .get_setting_text("ui.theme")
                .expect("read receiver theme")
                .as_deref(),
            Some("teal")
        );
        assert_eq!(
            receiver_database
                .get_setting_text("ui.zoom")
                .expect("read receiver zoom")
                .as_deref(),
            Some("110")
        );

        let _ = std::fs::remove_dir_all(publisher_state_root);
        let _ = std::fs::remove_dir_all(receiver_state_root);
        let _ = std::fs::remove_dir_all(publisher_db_root);
        let _ = std::fs::remove_dir_all(receiver_db_root);
    }

    #[test]
    fn broadwebd_publisher_runs_pending_local_settings_sync_until_idle() {
        let network = InProcessBroadwebNetwork::new();
        let publisher_state_root = test_state_root("sync-loop-publisher");
        let receiver_state_root = test_state_root("sync-loop-receiver");
        let publisher_db_root = test_state_root("sync-loop-publisher-db");
        let receiver_db_root = test_state_root("sync-loop-receiver-db");
        let publisher_daemon = network
            .daemon_for_device(
                &publisher_state_root,
                ResourceBudget::default(),
                "runtime-o",
            )
            .expect("start publisher daemon");
        let receiver_daemon = network
            .daemon_for_device(&receiver_state_root, ResourceBudget::default(), "runtime-p")
            .expect("start receiver daemon");
        let publisher_database = SlateProfileDatabase::open_resolved_with_device_id(
            publisher_db_root.join(DEFAULT_DATABASE_FILE_NAME),
            "runtime-o",
        )
        .expect("open publisher settings database");
        let receiver_database = SlateProfileDatabase::open_resolved_with_device_id(
            receiver_db_root.join(DEFAULT_DATABASE_FILE_NAME),
            "runtime-p",
        )
        .expect("open receiver settings database");
        let content_key = ProfileSyncContentKey::from_bytes([49; PROFILE_SYNC_CONTENT_KEY_BYTES]);
        let signer = ProfileSyncDeviceSigner::generate("runtime-o").expect("generate signer");
        let public_key = signer.public_key().expect("read signer public key");
        receiver_database
            .register_sync_device_public_key(&SyncDevicePublicKeyRegistration {
                profile: DEFAULT_PROFILE_ID.to_string(),
                public_key,
                membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            })
            .expect("receiver trusts publisher key");
        let publisher = BroadwebdProfileSyncPublisher::new(&publisher_daemon);

        publisher_database
            .set_sync_setting_text(DEFAULT_PROFILE_ID, SYNC_DOMAIN_SETTINGS, "ui.theme", "teal")
            .expect("publisher writes initial setting");
        let first_run = publisher
            .run_pending_local_settings_sync(
                &publisher_database,
                DEFAULT_PROFILE_ID,
                "settings/latest",
                &content_key,
                TEST_CONTENT_KEY_ID,
                &signer,
                ProfileSyncRetentionPolicy::default(),
                4,
            )
            .expect("run pending local settings sync");
        assert!(first_run.is_idle());
        assert_eq!(first_run.published_step_count(), 1);
        assert_eq!(first_run.steps.len(), 2);
        let LocalSettingsHeadPublishStatus::PublishedFullSnapshot(first_publish) =
            &first_run.steps[0]
        else {
            panic!("expected first loop step to publish a full snapshot: {first_run:?}");
        };
        let LocalSettingsHeadPublishStatus::UpToDate { snapshot_record } = &first_run.steps[1]
        else {
            panic!("expected first loop to settle as up to date: {first_run:?}");
        };
        assert_eq!(
            snapshot_record.snapshot_id,
            first_publish.snapshot_record.snapshot_id
        );

        publisher_database
            .apply_sync_setting_text(&IncomingSyncSettingText::new(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_SETTINGS,
                "ui.remote",
                "from-remote",
                "runtime-q",
                1,
                50,
            ))
            .expect("publisher applies remote post-snapshot setting");
        let remote_only_run = publisher
            .run_pending_local_settings_sync(
                &publisher_database,
                DEFAULT_PROFILE_ID,
                "settings/latest",
                &content_key,
                TEST_CONTENT_KEY_ID,
                &signer,
                ProfileSyncRetentionPolicy::default(),
                4,
            )
            .expect("run pending local settings sync after remote-only update");
        assert!(remote_only_run.is_idle());
        assert_eq!(remote_only_run.published_step_count(), 0);

        publisher_database
            .set_sync_setting_text(DEFAULT_PROFILE_ID, SYNC_DOMAIN_SETTINGS, "ui.zoom", "110")
            .expect("publisher writes post-snapshot setting");
        let second_run = publisher
            .run_pending_local_settings_sync(
                &publisher_database,
                DEFAULT_PROFILE_ID,
                "settings/latest",
                &content_key,
                TEST_CONTENT_KEY_ID,
                &signer,
                ProfileSyncRetentionPolicy::default(),
                4,
            )
            .expect("run pending local settings sync after update");
        assert!(second_run.is_idle());
        assert_eq!(second_run.published_step_count(), 1);
        assert_eq!(second_run.steps.len(), 2);
        let LocalSettingsHeadPublishStatus::PublishedIncrementalTail(second_publish) =
            &second_run.steps[0]
        else {
            panic!("expected second loop step to publish an incremental tail: {second_run:?}");
        };
        assert_eq!(
            second_publish.publication.snapshot_object_id,
            first_publish.publication.snapshot_object_id
        );
        assert_eq!(second_publish.publication.tail_change_object_ids.len(), 1);
        assert!(second_run.steps[1].is_idle());

        let source = BroadwebdProfileSyncObjectSource::new(&receiver_daemon);
        let applied = source
            .pull_record_and_apply_trusted_settings_from_device_head(
                &receiver_database,
                DEFAULT_PROFILE_ID,
                second_publish.device_head.root_id.as_str(),
                &content_key,
                TEST_CONTENT_KEY_ID,
            )
            .expect("receiver applies sync-loop-selected tail head");
        let BroadwebdTrustedDeviceHeadSyncStatus::Applied { application, .. } = applied else {
            panic!("expected sync-loop-selected tail application, got {applied:?}");
        };
        assert_eq!(
            application.manifest_object_id,
            second_publish.publication.manifest_object_id
        );
        assert_eq!(application.tail_changes.len(), 1);
        assert_eq!(
            receiver_database
                .get_setting_text("ui.theme")
                .expect("read receiver theme")
                .as_deref(),
            Some("teal")
        );
        assert_eq!(
            receiver_database
                .get_setting_text("ui.zoom")
                .expect("read receiver zoom")
                .as_deref(),
            Some("110")
        );
        assert_eq!(
            receiver_database
                .get_setting_text("ui.remote")
                .expect("read receiver remote setting")
                .as_deref(),
            None
        );

        let _ = std::fs::remove_dir_all(publisher_state_root);
        let _ = std::fs::remove_dir_all(receiver_state_root);
        let _ = std::fs::remove_dir_all(publisher_db_root);
        let _ = std::fs::remove_dir_all(receiver_db_root);
    }

    #[test]
    fn broadwebd_publisher_publishes_signed_settings_snapshot_manifest() {
        let network = InProcessBroadwebNetwork::new();
        let state_root = test_state_root("signed-snapshot-publish");
        let db_root = test_state_root("signed-snapshot-db");
        let daemon = network
            .daemon_for_device(&state_root, ResourceBudget::default(), "runtime-d")
            .expect("start in-process profile-sync daemon");
        let database = SlateProfileDatabase::open_resolved_with_device_id(
            db_root.join(DEFAULT_DATABASE_FILE_NAME),
            "runtime-d",
        )
        .expect("open local settings database");
        let covered_change = database
            .set_sync_setting_text(DEFAULT_PROFILE_ID, SYNC_DOMAIN_SETTINGS, "ui.theme", "teal")
            .expect("write compacted sync setting");
        let covers_revision = database
            .latest_sync_revision(DEFAULT_PROFILE_ID)
            .expect("read snapshot revision");
        let snapshot = database
            .settings_sync_snapshot_payload(
                DEFAULT_PROFILE_ID,
                covers_revision,
                &[SYNC_DOMAIN_SETTINGS.to_string()],
            )
            .expect("build settings snapshot payload");
        let tail_change = database
            .set_sync_setting_text(DEFAULT_PROFILE_ID, SYNC_DOMAIN_SETTINGS, "ui.zoom", "110")
            .expect("write retained tail sync setting");
        let content_key = ProfileSyncContentKey::from_bytes([42; PROFILE_SYNC_CONTENT_KEY_BYTES]);
        let signer = ProfileSyncDeviceSigner::generate("runtime-d").expect("generate signer");
        let public_key = signer.public_key().expect("read signer public key");
        let publisher = BroadwebdProfileSyncPublisher::new(&daemon);

        let publication = publisher
            .publish_signed_settings_snapshot_manifest(
                DEFAULT_PROFILE_ID,
                "settings/latest",
                &snapshot,
                std::slice::from_ref(&covered_change),
                std::slice::from_ref(&tail_change),
                &content_key,
                TEST_CONTENT_KEY_ID,
                &signer,
                ProfileSyncRetentionPolicy::default(),
            )
            .expect("publish signed settings snapshot manifest");

        assert_eq!(publication.manifest.profile, DEFAULT_PROFILE_ID);
        assert_eq!(publication.manifest.root_id, "settings/latest");
        assert_eq!(
            publication.manifest.current_snapshot_object_id.as_deref(),
            Some(publication.snapshot_object_id.as_str())
        );
        assert_eq!(publication.tail_change_object_ids.len(), 1);
        assert_eq!(
            publication.manifest.tail_change_object_ids,
            publication.tail_change_object_ids
        );

        let source = BroadwebdProfileSyncObjectSource::new(&daemon);
        assert_eq!(
            source
                .resolve_profile_sync_root(DEFAULT_PROFILE_ID, "settings/latest")
                .expect("resolve signed snapshot manifest root")
                .as_deref(),
            Some(publication.manifest_object_id.as_str())
        );

        let manifest_object = source
            .get_profile_sync_object(DEFAULT_PROFILE_ID, publication.manifest_object_id.as_str())
            .expect("fetch signed manifest object");
        let manifest = open_signed_profile_sync_manifest(
            manifest_object.bytes.as_slice(),
            &content_key,
            &public_key,
            DEFAULT_PROFILE_ID,
            TEST_CONTENT_KEY_ID,
        )
        .expect("verify signed manifest object");
        assert_eq!(manifest, publication.manifest);

        let snapshot_object = source
            .get_profile_sync_object(DEFAULT_PROFILE_ID, publication.snapshot_object_id.as_str())
            .expect("fetch signed snapshot object");
        let decoded_snapshot = open_signed_profile_sync_settings_snapshot(
            snapshot_object.bytes.as_slice(),
            &content_key,
            &public_key,
            DEFAULT_PROFILE_ID,
            TEST_CONTENT_KEY_ID,
        )
        .expect("verify signed snapshot object");
        assert_eq!(decoded_snapshot, snapshot);

        let tail_object_id = publication
            .tail_change_object_ids
            .first()
            .expect("tail object id");
        let tail_object = source
            .get_profile_sync_object(DEFAULT_PROFILE_ID, tail_object_id)
            .expect("fetch signed tail object");
        let incoming = open_signed_sync_setting_text(
            tail_object.bytes.as_slice(),
            &content_key,
            &public_key,
            DEFAULT_PROFILE_ID,
            SYNC_DOMAIN_SETTINGS,
            TEST_CONTENT_KEY_ID,
        )
        .expect("verify signed tail object");
        assert_eq!(incoming.key, "ui.zoom");
        assert_eq!(incoming.value, "110");
        assert_eq!(incoming.device_id, "runtime-d");
        assert_retained(&publisher, publication.snapshot_object_id.as_str());
        assert_retained(&publisher, publication.manifest_object_id.as_str());
        assert_retained(&publisher, tail_object_id);

        let _ = std::fs::remove_dir_all(state_root);
        let _ = std::fs::remove_dir_all(db_root);
    }

    #[test]
    fn broadwebd_publisher_compacts_and_publishes_signed_settings_snapshot_manifest() {
        let network = InProcessBroadwebNetwork::new();
        let state_root = test_state_root("compact-snapshot-publish");
        let db_root = test_state_root("compact-snapshot-db");
        let daemon = network
            .daemon_for_device(&state_root, ResourceBudget::default(), "runtime-e")
            .expect("start in-process profile-sync daemon");
        let database = SlateProfileDatabase::open_resolved_with_device_id(
            db_root.join(DEFAULT_DATABASE_FILE_NAME),
            "runtime-e",
        )
        .expect("open local settings database");
        let baseline_revision = database
            .latest_sync_revision(DEFAULT_PROFILE_ID)
            .expect("read baseline revision");
        database
            .register_app_sync_domain(&AppSyncDomainRegistration {
                profile: DEFAULT_PROFILE_ID.to_string(),
                domain: SYNC_DOMAIN_CALENDAR.to_string(),
                schema_version: 1,
                enabled: true,
                privacy_classification: "sensitive".to_string(),
                sync_content: false,
            })
            .expect("enable calendar sync for compaction test profile");
        database
            .set_sync_setting_text(DEFAULT_PROFILE_ID, SYNC_DOMAIN_SETTINGS, "ui.theme", "teal")
            .expect("write first compacted setting");
        database
            .set_sync_setting_text(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_CALENDAR,
                "default_view",
                "month",
            )
            .expect("write second compacted setting");
        database
            .set_sync_setting_text(DEFAULT_PROFILE_ID, SYNC_DOMAIN_SETTINGS, "ui.zoom", "110")
            .expect("write retained tail setting");
        let content_key = ProfileSyncContentKey::from_bytes([43; PROFILE_SYNC_CONTENT_KEY_BYTES]);
        let signer = ProfileSyncDeviceSigner::generate("runtime-e").expect("generate signer");
        let public_key = signer.public_key().expect("read signer public key");
        let publisher = BroadwebdProfileSyncPublisher::new(&daemon);
        let retention_policy = ProfileSyncRetentionPolicy {
            min_tail_change_count: 1,
            change_retention_seconds: 0,
            ..ProfileSyncRetentionPolicy::default()
        };

        let compaction = publisher
            .compact_and_publish_settings(
                &database,
                DEFAULT_PROFILE_ID,
                "settings/latest",
                &content_key,
                TEST_CONTENT_KEY_ID,
                &signer,
                retention_policy,
                i64::MAX,
            )
            .expect("publish compacted settings")
            .expect("compaction target");

        let expected_covered_change_count = if baseline_revision > 0 { 3 } else { 2 };
        assert_eq!(
            compaction.target.covered_change_count,
            expected_covered_change_count
        );
        assert_eq!(compaction.target.retained_tail_change_count, 1);
        assert_eq!(
            compaction.snapshot_record.snapshot_id,
            settings_sync_snapshot_id(compaction.target.covers_revision)
        );
        assert_eq!(
            compaction.snapshot_record.backend_object_id.as_deref(),
            Some(compaction.publication.snapshot_object_id.as_str())
        );
        assert_eq!(
            database
                .latest_sync_snapshot(DEFAULT_PROFILE_ID)
                .expect("read latest sync snapshot")
                .as_ref(),
            Some(&compaction.snapshot_record)
        );
        assert_eq!(
            database
                .settings_sync_compaction_target(
                    DEFAULT_PROFILE_ID,
                    &ProfileSyncRetentionPolicy {
                        min_tail_change_count: 1,
                        change_retention_seconds: 0,
                        ..ProfileSyncRetentionPolicy::default()
                    },
                    i64::MAX,
                )
                .expect("read compaction target after publish"),
            None
        );

        let source = BroadwebdProfileSyncObjectSource::new(&daemon);
        let manifest_object = source
            .get_profile_sync_object(
                DEFAULT_PROFILE_ID,
                compaction.publication.manifest_object_id.as_str(),
            )
            .expect("fetch compacted manifest object");
        let manifest = open_signed_profile_sync_manifest(
            manifest_object.bytes.as_slice(),
            &content_key,
            &public_key,
            DEFAULT_PROFILE_ID,
            TEST_CONTENT_KEY_ID,
        )
        .expect("verify compacted manifest object");
        assert_eq!(manifest, compaction.publication.manifest);
        assert_eq!(
            manifest.current_snapshot_object_id.as_deref(),
            Some(compaction.publication.snapshot_object_id.as_str())
        );
        assert_eq!(manifest.tail_change_object_ids.len(), 1);

        let snapshot_object = source
            .get_profile_sync_object(
                DEFAULT_PROFILE_ID,
                compaction.publication.snapshot_object_id.as_str(),
            )
            .expect("fetch compacted snapshot object");
        let snapshot = open_signed_profile_sync_settings_snapshot(
            snapshot_object.bytes.as_slice(),
            &content_key,
            &public_key,
            DEFAULT_PROFILE_ID,
            TEST_CONTENT_KEY_ID,
        )
        .expect("verify compacted snapshot object");
        assert_eq!(
            snapshot.included_domains,
            vec![
                SYNC_DOMAIN_CALENDAR.to_string(),
                SYNC_DOMAIN_SETTINGS.to_string()
            ]
        );
        assert!(snapshot.values.iter().any(|value| {
            value.domain == SYNC_DOMAIN_SETTINGS && value.key == "ui.theme" && value.value == "teal"
        }));
        assert!(snapshot.values.iter().any(|value| {
            value.domain == SYNC_DOMAIN_CALENDAR
                && value.key == "default_view"
                && value.value == "month"
        }));
        assert!(snapshot.values.iter().all(|value| value.key != "ui.zoom"));

        let tail_object_id = compaction
            .publication
            .tail_change_object_ids
            .first()
            .expect("tail object id");
        let tail_object = source
            .get_profile_sync_object(DEFAULT_PROFILE_ID, tail_object_id)
            .expect("fetch compacted tail object");
        let incoming = open_signed_sync_setting_text(
            tail_object.bytes.as_slice(),
            &content_key,
            &public_key,
            DEFAULT_PROFILE_ID,
            SYNC_DOMAIN_SETTINGS,
            TEST_CONTENT_KEY_ID,
        )
        .expect("verify compacted tail object");
        assert_eq!(incoming.key, "ui.zoom");
        assert_eq!(incoming.value, "110");

        let _ = std::fs::remove_dir_all(state_root);
        let _ = std::fs::remove_dir_all(db_root);
    }

    #[test]
    fn broadwebd_publisher_does_not_compact_disabled_app_domains() {
        let network = InProcessBroadwebNetwork::new();
        let state_root = test_state_root("disabled-domain-compaction");
        let db_root = test_state_root("disabled-domain-compaction-db");
        let daemon = network
            .daemon_for_device(
                &state_root,
                ResourceBudget::default(),
                "runtime-disabled-domain-compaction",
            )
            .expect("start in-process profile-sync daemon");
        let database = SlateProfileDatabase::open_resolved_with_device_id(
            db_root.join(DEFAULT_DATABASE_FILE_NAME),
            "runtime-disabled-domain-compaction",
        )
        .expect("open local settings database");
        let baseline_revision = database
            .latest_sync_revision(DEFAULT_PROFILE_ID)
            .expect("read baseline revision");
        if baseline_revision > 0 {
            database
                .record_sync_snapshot(&SyncSnapshotRegistration {
                    profile: DEFAULT_PROFILE_ID.to_string(),
                    snapshot_id: "snapshot-baseline".to_string(),
                    backend_object_id: Some("snapshot-object-baseline".to_string()),
                    covers_revision: baseline_revision,
                    included_domains: vec![SYNC_DOMAIN_SETTINGS.to_string()],
                })
                .expect("record baseline snapshot");
        }
        database
            .set_sync_setting_text(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_CALENDAR,
                "default_view",
                "month",
            )
            .expect("write disabled calendar setting");
        let content_key = ProfileSyncContentKey::from_bytes([67; PROFILE_SYNC_CONTENT_KEY_BYTES]);
        let signer = ProfileSyncDeviceSigner::generate("runtime-disabled-domain-compaction")
            .expect("generate signer");
        let publisher = BroadwebdProfileSyncPublisher::new(&daemon);

        let compaction = publisher
            .compact_and_publish_settings(
                &database,
                DEFAULT_PROFILE_ID,
                "settings/latest",
                &content_key,
                TEST_CONTENT_KEY_ID,
                &signer,
                ProfileSyncRetentionPolicy {
                    min_tail_change_count: 0,
                    change_retention_seconds: 0,
                    ..ProfileSyncRetentionPolicy::default()
                },
                i64::MAX,
            )
            .expect("disabled app domain compaction is evaluated locally");

        assert_eq!(compaction, None);
        assert!(
            database
                .profile_sync_root(DEFAULT_PROFILE_ID, "settings/latest")
                .expect("read settings root")
                .is_none()
        );

        let _ = std::fs::remove_dir_all(state_root);
        let _ = std::fs::remove_dir_all(db_root);
    }

    fn assert_retained(publisher: &BroadwebdProfileSyncPublisher<'_>, object_id: &str) {
        let status = publisher
            .verify_retained_object("default", object_id)
            .expect("verify retained object");
        assert_eq!(status.object_id, object_id);
        assert!(status.retained);
        assert!(status.available);
    }

    fn test_state_root(name: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time should move forward")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "slate-profile-sync-test-{}-{nanos}-{name}",
            std::process::id()
        ))
    }
}
