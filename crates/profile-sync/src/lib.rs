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
    EncryptedSyncObject, IncomingSyncSettingText, PROFILE_SYNC_MANIFEST_OBJECT_KIND,
    PROFILE_SYNC_SETTING_CHANGE_OBJECT_KIND, ProfileSyncContentKey, ProfileSyncDeviceSigner,
    ProfileSyncManifest, ProfileSyncObjectBytes, ProfileSyncObjectSource,
    ProfileSyncRetentionPolicy, ProfileSyncRootCandidate as StorageProfileSyncRootCandidate,
    ProfileSyncSettingsTailChangePublication, SYNC_DOMAIN_SETTINGS, StorageError, SyncChangeRecord,
    SyncObjectError, settings_sync_manifest_for_tail_changes,
};

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
    for change in changes {
        if change.profile != profile {
            return Err(StorageError::InvalidProfileSyncManifest(format!(
                "tail change {} profile {} does not match manifest profile {}",
                change.id, change.profile, profile
            )));
        }
        if change.operation != "set_text" {
            return Err(StorageError::InvalidProfileSyncManifest(format!(
                "tail change {} operation {} is not supported by settings manifests",
                change.id, change.operation
            )));
        }
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
    use super::{BroadwebdProfileSyncObjectSource, BroadwebdProfileSyncPublisher};
    use slate_broadwebd::{
        BroadwebDaemon, LocalProfileSyncFixture, PluginRegistry, ResourceBudget,
    };
    use slate_storage::{
        DEFAULT_DATABASE_FILE_NAME, DEFAULT_PROFILE_ID, PROFILE_SYNC_CONTENT_KEY_BYTES,
        ProfileSyncContentKey, ProfileSyncDeviceSigner, ProfileSyncObjectSource,
        ProfileSyncRetentionPolicy, SYNC_DOMAIN_SETTINGS, SlateProfileDatabase,
        open_signed_profile_sync_manifest, open_signed_sync_setting_text,
    };

    const TEST_CONTENT_KEY_ID: &str = "content-key-epoch-1";

    #[test]
    fn broadwebd_bridge_publishes_and_reads_fixture_objects() {
        let fixture = LocalProfileSyncFixture::new();
        let mut registry = PluginRegistry::new();
        registry.register_service(fixture.service_for_device("runtime-a"));
        let state_root = test_state_root("broadwebd-source");
        let daemon =
            BroadwebDaemon::start_with_registry(&state_root, ResourceBudget::default(), registry)
                .expect("start local profile-sync daemon");
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
        let fixture = LocalProfileSyncFixture::new();
        let mut registry = PluginRegistry::new();
        registry.register_service(fixture.service_for_device("runtime-b"));
        let state_root = test_state_root("broadwebd-batch-publish");
        let daemon =
            BroadwebDaemon::start_with_registry(&state_root, ResourceBudget::default(), registry)
                .expect("start local profile-sync daemon");
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
        let fixture = LocalProfileSyncFixture::new();
        let mut registry = PluginRegistry::new();
        registry.register_service(fixture.service_for_device("runtime-c"));
        let state_root = test_state_root("signed-tail-publish");
        let db_root = test_state_root("signed-tail-db");
        let daemon =
            BroadwebDaemon::start_with_registry(&state_root, ResourceBudget::default(), registry)
                .expect("start local profile-sync daemon");
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

    fn assert_retained(publisher: &BroadwebdProfileSyncPublisher<'_>, object_id: &str) {
        let status = publisher
            .verify_retained_object("default", object_id)
            .expect("verify retained object");
        assert_eq!(status.object_id, object_id);
        assert!(status.retained);
        assert!(status.available);
    }

    fn test_state_root(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "slate-profile-sync-test-{}-{name}",
            std::process::id()
        ))
    }
}
