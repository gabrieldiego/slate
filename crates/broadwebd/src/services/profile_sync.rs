use crate::{
    ApplicationServicePlugin, BroadwebdError, PROFILE_SYNC_PLUGIN, PluginKind, PluginMetadata,
    PluginRegistry, ProfileSyncObjectRequest, ProfileSyncProfileRequest, ProfileSyncProviderRecord,
    ProfileSyncPutObjectRequest, ProfileSyncRequest, ProfileSyncResponse, ProfileSyncRootRequest,
    ProfileSyncRootUpdate, ResourceBudget, ResourceProfile, ServiceRequest, ServiceResponse,
};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

const LOCAL_PROVIDER_ID: &str = "local-fake-profile-sync";

#[derive(Clone, Debug, Default)]
pub struct ProfileSyncService {
    store: Arc<Mutex<ProfileSyncStore>>,
}

#[derive(Clone, Debug, Default)]
struct ProfileSyncStore {
    objects: BTreeMap<(String, String), Vec<u8>>,
    retained: BTreeSet<(String, String)>,
    roots: BTreeMap<(String, String), String>,
}

impl ProfileSyncService {
    pub fn new() -> Self {
        Self::default()
    }

    fn put_object(
        &self,
        request: ProfileSyncPutObjectRequest,
    ) -> Result<ProfileSyncResponse, BroadwebdError> {
        validate_profile(&request.profile)?;
        let object_id = local_object_id(&request.bytes);
        let mut store = self.store()?;
        store
            .objects
            .insert((request.profile, object_id.clone()), request.bytes);
        Ok(ProfileSyncResponse::PutEncryptedObject { object_id })
    }

    fn get_object(
        &self,
        request: ProfileSyncObjectRequest,
    ) -> Result<ProfileSyncResponse, BroadwebdError> {
        validate_profile(&request.profile)?;
        let store = self.store()?;
        let bytes = store
            .objects
            .get(&(request.profile, request.object_id.clone()))
            .cloned()
            .ok_or_else(|| {
                BroadwebdError::UnsupportedRequest(format!(
                    "profile sync object not found: {}",
                    request.object_id
                ))
            })?;
        Ok(ProfileSyncResponse::GetEncryptedObject {
            object_id: request.object_id,
            bytes,
        })
    }

    fn retain_object(
        &self,
        request: ProfileSyncObjectRequest,
    ) -> Result<ProfileSyncResponse, BroadwebdError> {
        validate_profile(&request.profile)?;
        let mut store = self.store()?;
        if !store
            .objects
            .contains_key(&(request.profile.clone(), request.object_id.clone()))
        {
            return Err(BroadwebdError::UnsupportedRequest(format!(
                "cannot retain missing profile sync object: {}",
                request.object_id
            )));
        }
        store
            .retained
            .insert((request.profile, request.object_id.clone()));
        Ok(ProfileSyncResponse::RetainObject {
            object_id: request.object_id,
            retained: true,
        })
    }

    fn release_object(
        &self,
        request: ProfileSyncObjectRequest,
    ) -> Result<ProfileSyncResponse, BroadwebdError> {
        validate_profile(&request.profile)?;
        let mut store = self.store()?;
        store
            .retained
            .remove(&(request.profile, request.object_id.clone()));
        Ok(ProfileSyncResponse::ReleaseObject {
            object_id: request.object_id,
            retained: false,
        })
    }

    fn publish_root(
        &self,
        request: ProfileSyncRootUpdate,
    ) -> Result<ProfileSyncResponse, BroadwebdError> {
        validate_profile(&request.profile)?;
        let mut store = self.store()?;
        if !store
            .objects
            .contains_key(&(request.profile.clone(), request.object_id.clone()))
        {
            return Err(BroadwebdError::UnsupportedRequest(format!(
                "cannot publish missing profile sync object: {}",
                request.object_id
            )));
        }
        store.roots.insert(
            (request.profile, request.root_id.clone()),
            request.object_id.clone(),
        );
        Ok(ProfileSyncResponse::Root {
            root_id: request.root_id,
            object_id: Some(request.object_id),
        })
    }

    fn resolve_root(
        &self,
        request: ProfileSyncRootRequest,
    ) -> Result<ProfileSyncResponse, BroadwebdError> {
        validate_profile(&request.profile)?;
        let store = self.store()?;
        Ok(ProfileSyncResponse::Root {
            root_id: request.root_id.clone(),
            object_id: store
                .roots
                .get(&(request.profile, request.root_id))
                .cloned(),
        })
    }

    fn discover_providers(
        &self,
        request: ProfileSyncProfileRequest,
    ) -> Result<ProfileSyncResponse, BroadwebdError> {
        validate_profile(&request.profile)?;
        let store = self.store()?;
        let retained_objects = store
            .retained
            .iter()
            .filter(|(profile, _)| profile == &request.profile)
            .count();
        Ok(ProfileSyncResponse::Providers {
            providers: vec![ProfileSyncProviderRecord {
                provider_id: LOCAL_PROVIDER_ID.to_string(),
                provider_kind: "local-fake".to_string(),
                privacy_boundary: "in-memory local test backend; no sockets or external network"
                    .to_string(),
                retained_objects,
            }],
        })
    }

    fn store(&self) -> Result<std::sync::MutexGuard<'_, ProfileSyncStore>, BroadwebdError> {
        self.store
            .lock()
            .map_err(|_| BroadwebdError::Request("profile sync store lock poisoned".to_string()))
    }
}

impl ApplicationServicePlugin for ProfileSyncService {
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata::new(PROFILE_SYNC_PLUGIN, PluginKind::ApplicationService)
            .with_capabilities(&[
                "profile-sync/fake",
                "profile-sync/object-transfer",
                "profile-sync/local-retention",
                "profile-sync/mutable-root",
                "profile-sync/provider-discovery",
            ])
            .with_privacy_boundary("local in-memory fake profile-sync backend for tests")
            .with_resource_profile(ResourceProfile::Low)
    }

    fn call(
        &self,
        request: ServiceRequest,
        _registry: &PluginRegistry,
        _budget: &ResourceBudget,
    ) -> Result<ServiceResponse, BroadwebdError> {
        let ServiceRequest::ProfileSync(request) = request else {
            return Err(BroadwebdError::UnsupportedRequest(
                "profile-sync cannot handle non-profile-sync requests".to_string(),
            ));
        };

        let response = match request {
            ProfileSyncRequest::PutEncryptedObject(request) => self.put_object(request)?,
            ProfileSyncRequest::GetEncryptedObject(request) => self.get_object(request)?,
            ProfileSyncRequest::RetainObject(request) => self.retain_object(request)?,
            ProfileSyncRequest::ReleaseObject(request) => self.release_object(request)?,
            ProfileSyncRequest::PublishRoot(request) => self.publish_root(request)?,
            ProfileSyncRequest::ResolveRoot(request) => self.resolve_root(request)?,
            ProfileSyncRequest::DiscoverProviders(request) => self.discover_providers(request)?,
        };
        Ok(ServiceResponse::ProfileSync(response))
    }
}

fn validate_profile(profile: &str) -> Result<(), BroadwebdError> {
    let is_valid = !profile.is_empty()
        && profile
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'));
    if is_valid {
        Ok(())
    } else {
        Err(BroadwebdError::InvalidProfile(profile.to_string()))
    }
}

fn local_object_id(bytes: &[u8]) -> String {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("local-broadweb-object:v1:{}:{hash:016x}", bytes.len())
}

#[cfg(test)]
mod tests {
    use super::local_object_id;

    #[test]
    fn local_object_ids_are_deterministic_for_test_backend() {
        assert_eq!(local_object_id(b"settings"), local_object_id(b"settings"));
        assert_ne!(local_object_id(b"settings"), local_object_id(b"calendar"));
    }
}
