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
    EncryptedSyncObject, IncomingSyncSettingText, PROFILE_SYNC_DEVICE_HEAD_OBJECT_KIND,
    PROFILE_SYNC_DEVICE_HEAD_SCHEMA_VERSION, PROFILE_SYNC_MANIFEST_OBJECT_KIND,
    PROFILE_SYNC_SETTING_CHANGE_OBJECT_KIND, PROFILE_SYNC_SETTINGS_SNAPSHOT_OBJECT_KIND,
    PROFILE_SYNC_SETTINGS_SNAPSHOT_SCHEMA_VERSION, ProfileSyncContentKey, ProfileSyncDeviceHead,
    ProfileSyncDeviceSigner, ProfileSyncManifest, ProfileSyncObjectBytes, ProfileSyncObjectSource,
    ProfileSyncRetentionPolicy, ProfileSyncRootCandidate as StorageProfileSyncRootCandidate,
    ProfileSyncSettingsSnapshot, ProfileSyncSettingsSnapshotPublication,
    ProfileSyncSettingsTailChangePublication, SYNC_DOMAIN_SETTINGS, SlateProfileDatabase,
    StorageError, SyncChangeRecord, SyncCompactionTarget, SyncObjectError, SyncSnapshotRecord,
    SyncSnapshotRegistration, settings_sync_manifest_for_snapshot_and_tail_changes,
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
}

impl fmt::Display for ProfileSyncPublishError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Broadwebd(error) => write!(formatter, "profile sync backend error: {error}"),
            Self::Storage(error) => write!(formatter, "profile sync storage error: {error}"),
            Self::SyncObject(error) => write!(formatter, "profile sync object error: {error}"),
        }
    }
}

impl std::error::Error for ProfileSyncPublishError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Broadwebd(error) => Some(error),
            Self::Storage(error) => Some(error),
            Self::SyncObject(error) => Some(error),
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

pub fn settings_device_head_root_id(device_id: &str) -> String {
    format!("settings/devices/{device_id}/head")
}

impl<'a> BroadwebdProfileSyncObjectSource<'a> {
    pub fn new(daemon: &'a BroadwebDaemon) -> Self {
        Self { daemon }
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
        settings_device_head_root_id,
    };
    use slate_broadwebd::{ResourceBudget, test_fixtures::InProcessBroadwebNetwork};
    use slate_storage::{
        DEFAULT_DATABASE_FILE_NAME, DEFAULT_PROFILE_ID, DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
        PROFILE_SYNC_CONTENT_KEY_BYTES, PROFILE_SYNC_DEVICE_HEAD_SCHEMA_VERSION,
        ProfileSyncContentKey, ProfileSyncDeviceHead, ProfileSyncDeviceSigner,
        ProfileSyncObjectSource, ProfileSyncRetentionPolicy, SYNC_DOMAIN_CALENDAR,
        SYNC_DOMAIN_SETTINGS, SlateProfileDatabase, open_signed_profile_sync_device_head,
        open_signed_profile_sync_manifest, open_signed_profile_sync_settings_snapshot,
        open_signed_sync_setting_text, pull_signed_profile_sync_device_head,
        settings_sync_snapshot_id,
    };
    use std::time::{SystemTime, UNIX_EPOCH};

    const TEST_CONTENT_KEY_ID: &str = "content-key-epoch-1";

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
