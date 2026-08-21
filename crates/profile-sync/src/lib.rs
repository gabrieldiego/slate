#![forbid(unsafe_code)]

use slate_broadwebd::{
    BroadwebDaemon, BroadwebdError, ProfileSyncObjectRequest as BroadwebdProfileSyncObjectRequest,
    ProfileSyncRequest as BroadwebdProfileSyncRequest,
    ProfileSyncResponse as BroadwebdProfileSyncResponse,
    ProfileSyncRootRequest as BroadwebdProfileSyncRootRequest,
};
use slate_storage::{
    ProfileSyncObjectBytes, ProfileSyncObjectSource,
    ProfileSyncRootCandidate as StorageProfileSyncRootCandidate,
};

#[derive(Clone, Copy)]
pub struct BroadwebdProfileSyncObjectSource<'a> {
    daemon: &'a BroadwebDaemon,
}

impl<'a> BroadwebdProfileSyncObjectSource<'a> {
    pub fn new(daemon: &'a BroadwebDaemon) -> Self {
        Self { daemon }
    }
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
    use super::BroadwebdProfileSyncObjectSource;
    use slate_broadwebd::{
        BroadwebDaemon, LocalProfileSyncFixture, PluginRegistry, ProfileSyncObjectRequest,
        ProfileSyncPutObjectRequest, ProfileSyncRequest, ProfileSyncResponse,
        ProfileSyncRootUpdate, ResourceBudget,
    };
    use slate_storage::ProfileSyncObjectSource;

    #[test]
    fn broadwebd_source_reads_fixture_roots_candidates_and_objects() {
        let fixture = LocalProfileSyncFixture::new();
        let mut registry = PluginRegistry::new();
        registry.register_service(fixture.service_for_device("runtime-a"));
        let state_root = test_state_root("broadwebd-source");
        let daemon =
            BroadwebDaemon::start_with_registry(&state_root, ResourceBudget::default(), registry)
                .expect("start local profile-sync daemon");
        let object_bytes = b"encrypted runtime object".to_vec();
        let put = daemon
            .profile_sync(ProfileSyncRequest::PutEncryptedObject(
                ProfileSyncPutObjectRequest::new("default", object_bytes.clone()),
            ))
            .expect("put local profile-sync object");
        let ProfileSyncResponse::PutEncryptedObject { object_id } = put else {
            panic!("unexpected put response");
        };
        daemon
            .profile_sync(ProfileSyncRequest::RetainObject(
                ProfileSyncObjectRequest::new("default", object_id.clone()),
            ))
            .expect("retain local profile-sync object");
        daemon
            .profile_sync(ProfileSyncRequest::PublishRoot(ProfileSyncRootUpdate::new(
                "default",
                "settings/latest",
                object_id.clone(),
            )))
            .expect("publish local profile-sync root");

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

        let _ = std::fs::remove_dir_all(state_root);
    }

    fn test_state_root(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "slate-profile-sync-test-{}-{name}",
            std::process::id()
        ))
    }
}
