#![forbid(unsafe_code)]

use core::fmt;
use slate_broadwebd::{
    BroadwebDaemon, BroadwebdError, ProfileSyncObjectRequest as BroadwebdProfileSyncObjectRequest,
    ProfileSyncPutObjectRequest as BroadwebdProfileSyncPutObjectRequest,
    ProfileSyncRequest as BroadwebdProfileSyncRequest,
    ProfileSyncResponse as BroadwebdProfileSyncResponse,
    ProfileSyncRootRequest as BroadwebdProfileSyncRootRequest,
    ProfileSyncRootUpdate as BroadwebdProfileSyncRootUpdate,
};
use slate_storage::{
    DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH, EncryptedSyncObject, IncomingSyncSettingText,
    PROFILE_SYNC_CONTENT_KEY_ALGORITHM_CHACHA20_POLY1305, PROFILE_SYNC_DEVICE_HEAD_OBJECT_KIND,
    PROFILE_SYNC_DEVICE_HEAD_SCHEMA_VERSION, PROFILE_SYNC_MANIFEST_OBJECT_KIND,
    PROFILE_SYNC_SETTING_CHANGE_OBJECT_KIND, PROFILE_SYNC_SETTINGS_SNAPSHOT_OBJECT_KIND,
    PROFILE_SYNC_SETTINGS_SNAPSHOT_SCHEMA_VERSION, ProfileSyncContentKey, ProfileSyncDeviceHead,
    ProfileSyncDeviceHeadPullRecordStatus, ProfileSyncDeviceSigner, ProfileSyncManifest,
    ProfileSyncObjectBytes, ProfileSyncObjectSource, ProfileSyncRetentionPolicy,
    ProfileSyncRootCandidate as StorageProfileSyncRootCandidate, ProfileSyncRootRecord,
    ProfileSyncSettingsManifestApplication, ProfileSyncSettingsSnapshot,
    ProfileSyncSettingsSnapshotPublication, ProfileSyncSettingsTailChangePublication,
    ProfileSyncTrustedPullApplyError, SYNC_DOMAIN_SETTINGS, SlateProfileDatabase, StorageError,
    SyncChangeRecord, SyncCompactionTarget, SyncDevicePublicKeyRecord, SyncObjectError,
    SyncSnapshotRecord, SyncSnapshotRegistration, VerifiedProfileSyncDeviceHead,
    open_signed_profile_sync_device_head, settings_sync_manifest_for_snapshot_and_tail_changes,
    settings_sync_manifest_for_tail_changes, settings_sync_snapshot_id,
};
use std::collections::BTreeSet;

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

#[derive(Debug)]
pub enum ProfileSyncPublishError {
    Broadwebd(BroadwebdError),
    Storage(StorageError),
    SyncObject(SyncObjectError),
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
    Storage(StorageError),
    PullApply(ProfileSyncTrustedPullApplyError<BroadwebdError>),
    TrustedDeviceLimitExceeded {
        profile: String,
        trusted_device_count: usize,
        max_devices: u32,
    },
}

impl fmt::Display for ProfileSyncReceiveError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Storage(error) => write!(formatter, "profile sync storage error: {error}"),
            Self::PullApply(error) => write!(formatter, "profile sync receive error: {error}"),
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
            Self::Storage(error) => Some(error),
            Self::PullApply(error) => Some(error),
            Self::TrustedDeviceLimitExceeded { .. } => None,
        }
    }
}

impl From<StorageError> for ProfileSyncReceiveError {
    fn from(error: StorageError) -> Self {
        Self::Storage(error)
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
            | Self::LocalDevicePublicKeyMismatch { .. } => None,
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublishedSettingsTailManifest {
    pub manifest_object_id: String,
    pub manifest: ProfileSyncManifest,
    pub tail_change_object_ids: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublishedSettingsSnapshotManifest {
    pub manifest_object_id: String,
    pub manifest: ProfileSyncManifest,
    pub snapshot_object_id: String,
    pub tail_change_object_ids: Vec<String>,
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublishedLocalSettingsSnapshotHead {
    pub publication: PublishedSettingsSnapshotManifest,
    pub device_head: PublishedProfileSyncDeviceHead,
    pub snapshot_record: SyncSnapshotRecord,
    pub settings_root: ProfileSyncRootRecord,
    pub device_head_root: ProfileSyncRootRecord,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublishedLocalSettingsTailHead {
    pub publication: PublishedSettingsSnapshotManifest,
    pub device_head: PublishedProfileSyncDeviceHead,
    pub snapshot_record: SyncSnapshotRecord,
    pub settings_root: ProfileSyncRootRecord,
    pub device_head_root: ProfileSyncRootRecord,
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
}

pub fn settings_device_head_root_id(device_id: &str) -> String {
    format!("settings/devices/{device_id}/head")
}

impl<'a> BroadwebdProfileSyncObjectSource<'a> {
    pub fn new(daemon: &'a BroadwebDaemon) -> Self {
        Self { daemon }
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
}

impl<'a> BroadwebdSettingsSyncRunner<'a> {
    pub fn new(daemon: &'a BroadwebDaemon) -> Self {
        Self { daemon }
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
        let Some(target) =
            database.settings_sync_compaction_target(profile, &retention_policy, now)?
        else {
            return Ok(None);
        };
        let events = database.sync_setting_text_events_after(
            profile,
            target.previous_snapshot_covers_revision,
            u32::MAX,
        )?;
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
        let events = database.sync_setting_text_events_after(profile, 0, u32::MAX)?;
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
            return Err(StorageError::InvalidProfileSyncManifest(format!(
                "no local settings change exists for device {}",
                database.local_sync_device_id()
            ))
            .into());
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
        let tail_events = database.sync_setting_text_events_after(
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
        let all_events = database.sync_setting_text_events_after(profile, 0, u32::MAX)?;
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
                let local_tail_events = database.sync_setting_text_events_after(
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

pub fn validate_settings_sync_cycle_credentials(
    database: &SlateProfileDatabase,
    profile: &str,
    key_id: &str,
    signer: &ProfileSyncDeviceSigner,
) -> Result<(), ProfileSyncCredentialError> {
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
    if active_key.key_id != key_id {
        return Err(ProfileSyncCredentialError::InactiveContentKey {
            profile: profile.to_string(),
            expected_key_id: key_id.to_string(),
            active_key_id: active_key.key_id,
        });
    }

    let public_key = signer.public_key()?;
    let Some(trusted_key) =
        database.sync_device_public_key(profile, public_key.device_id.as_str())?
    else {
        return Err(ProfileSyncCredentialError::UntrustedLocalDevice {
            profile: profile.to_string(),
            device_id: public_key.device_id,
        });
    };
    if trusted_key.public_key != public_key {
        return Err(ProfileSyncCredentialError::LocalDevicePublicKeyMismatch {
            profile: profile.to_string(),
            device_id: trusted_key.public_key.device_id,
        });
    }

    Ok(())
}

fn trusted_remote_device_public_keys(
    database: &SlateProfileDatabase,
    profile: &str,
    max_devices: u32,
) -> Result<Vec<SyncDevicePublicKeyRecord>, ProfileSyncReceiveError> {
    let trusted_devices = database
        .sync_device_public_keys(profile)?
        .into_iter()
        .filter(|record| record.public_key.device_id != database.local_sync_device_id())
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
        BroadwebdSettingsSyncRunner, BroadwebdTrustedDeviceHeadSyncStatus,
        LocalSettingsHeadPublishStatus, ProfileSyncCredentialError, ProfileSyncCycleError,
        ProfileSyncReceiveError, settings_device_head_root_id,
    };
    use slate_broadwebd::{ResourceBudget, test_fixtures::InProcessBroadwebNetwork};
    use slate_storage::{
        DEFAULT_DATABASE_FILE_NAME, DEFAULT_PROFILE_ID, DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
        IncomingSyncSettingText, PROFILE_SYNC_CONTENT_KEY_ALGORITHM_CHACHA20_POLY1305,
        PROFILE_SYNC_CONTENT_KEY_BYTES, PROFILE_SYNC_DEVICE_HEAD_SCHEMA_VERSION,
        ProfileSyncContentKey, ProfileSyncDeviceHead, ProfileSyncDeviceSigner,
        ProfileSyncObjectSource, ProfileSyncRetentionPolicy, SYNC_DOMAIN_CALENDAR,
        SYNC_DOMAIN_SETTINGS, SlateProfileDatabase, StorageError, SyncChangeRecord,
        SyncContentKeyEpochRegistration, SyncDevicePublicKeyRegistration,
        open_signed_profile_sync_device_head, open_signed_profile_sync_manifest,
        open_signed_profile_sync_settings_snapshot, open_signed_sync_setting_text,
        pull_signed_profile_sync_device_head, settings_sync_snapshot_id,
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
