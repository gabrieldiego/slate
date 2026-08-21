use crate::{
    ApplicationServicePlugin, BroadwebdError, PROFILE_SYNC_PLUGIN, PluginKind, PluginMetadata,
    PluginRegistry, ProfileSyncObjectRequest, ProfileSyncProfileRequest, ProfileSyncProviderRecord,
    ProfileSyncPutObjectRequest, ProfileSyncRequest, ProfileSyncResponse, ProfileSyncRootRequest,
    ProfileSyncRootUpdate, ResourceBudget, ResourceProfile, ServiceRequest, ServiceResponse,
};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

const LOCAL_PROVIDER_ID: &str = "local-fake-profile-sync";
const LOCAL_PROVIDER_KIND: &str = "local-fake";
const LOCAL_PRIVACY_BOUNDARY: &str = "in-memory local test backend; no sockets or external network";

#[derive(Clone, Debug)]
pub struct ProfileSyncService {
    store: Arc<Mutex<ProfileSyncStore>>,
    provider_id: String,
    provider_kind: String,
    privacy_boundary: String,
}

#[derive(Clone, Debug, Default)]
struct ProfileSyncStore {
    objects: BTreeMap<(String, String), Vec<u8>>,
    retained: BTreeSet<(String, String)>,
    roots: BTreeMap<(String, String), String>,
    offline_providers: BTreeSet<String>,
}

#[derive(Clone, Debug, Default)]
pub struct LocalProfileSyncFixture {
    store: Arc<Mutex<ProfileSyncStore>>,
}

impl Default for ProfileSyncService {
    fn default() -> Self {
        Self {
            store: Arc::new(Mutex::new(ProfileSyncStore::default())),
            provider_id: LOCAL_PROVIDER_ID.to_string(),
            provider_kind: LOCAL_PROVIDER_KIND.to_string(),
            privacy_boundary: LOCAL_PRIVACY_BOUNDARY.to_string(),
        }
    }
}

impl ProfileSyncService {
    pub fn new() -> Self {
        Self::default()
    }

    fn local_fixture(store: Arc<Mutex<ProfileSyncStore>>, provider_id: impl Into<String>) -> Self {
        Self {
            store,
            provider_id: provider_id.into(),
            provider_kind: "local-fixture".to_string(),
            privacy_boundary: LOCAL_PRIVACY_BOUNDARY.to_string(),
        }
    }

    fn ensure_online(&self) -> Result<(), BroadwebdError> {
        let store = self.store()?;
        if store.offline_providers.contains(&self.provider_id) {
            Err(BroadwebdError::Request(format!(
                "profile sync provider is offline: {}",
                self.provider_id
            )))
        } else {
            Ok(())
        }
    }

    fn put_object(
        &self,
        request: ProfileSyncPutObjectRequest,
        budget: &ResourceBudget,
    ) -> Result<ProfileSyncResponse, BroadwebdError> {
        validate_profile(&request.profile)?;
        validate_object_budget(request.bytes.len(), budget)?;
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
        budget: &ResourceBudget,
    ) -> Result<ProfileSyncResponse, BroadwebdError> {
        validate_profile(&request.profile)?;
        let store = self.store()?;
        let object_id = request.object_id;
        let bytes = store
            .objects
            .get(&(request.profile, object_id.clone()))
            .ok_or_else(|| {
                BroadwebdError::UnsupportedRequest(format!(
                    "profile sync object not found: {}",
                    object_id
                ))
            })?;
        validate_object_budget(bytes.len(), budget)?;
        Ok(ProfileSyncResponse::GetEncryptedObject {
            object_id,
            bytes: bytes.clone(),
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

    fn list_retained_objects(
        &self,
        request: ProfileSyncProfileRequest,
    ) -> Result<ProfileSyncResponse, BroadwebdError> {
        validate_profile(&request.profile)?;
        let store = self.store()?;
        let object_ids = store
            .retained
            .iter()
            .filter(|(profile, _)| profile == &request.profile)
            .map(|(_, object_id)| object_id.clone())
            .collect();
        Ok(ProfileSyncResponse::RetainedObjects { object_ids })
    }

    fn verify_retained_object(
        &self,
        request: ProfileSyncObjectRequest,
    ) -> Result<ProfileSyncResponse, BroadwebdError> {
        validate_profile(&request.profile)?;
        let store = self.store()?;
        let key = (request.profile, request.object_id.clone());
        let retained = store.retained.contains(&key);
        let available = store.objects.contains_key(&key);
        Ok(ProfileSyncResponse::RetainedObjectStatus {
            object_id: request.object_id,
            retained,
            available,
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
                provider_id: self.provider_id.clone(),
                provider_kind: self.provider_kind.clone(),
                privacy_boundary: self.privacy_boundary.clone(),
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

impl LocalProfileSyncFixture {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn service_for_device(&self, device_id: impl AsRef<str>) -> ProfileSyncService {
        ProfileSyncService::local_fixture(self.store.clone(), local_fixture_provider_id(device_id))
    }

    pub fn set_device_online(
        &self,
        device_id: impl AsRef<str>,
        online: bool,
    ) -> Result<(), BroadwebdError> {
        let provider_id = local_fixture_provider_id(device_id);
        let mut store = self
            .store
            .lock()
            .map_err(|_| BroadwebdError::Request("profile sync store lock poisoned".to_string()))?;
        if online {
            store.offline_providers.remove(provider_id.as_str());
        } else {
            store.offline_providers.insert(provider_id);
        }
        Ok(())
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
        budget: &ResourceBudget,
    ) -> Result<ServiceResponse, BroadwebdError> {
        let ServiceRequest::ProfileSync(request) = request else {
            return Err(BroadwebdError::UnsupportedRequest(
                "profile-sync cannot handle non-profile-sync requests".to_string(),
            ));
        };

        self.ensure_online()?;

        let response = match request {
            ProfileSyncRequest::PutEncryptedObject(request) => self.put_object(request, budget)?,
            ProfileSyncRequest::GetEncryptedObject(request) => self.get_object(request, budget)?,
            ProfileSyncRequest::RetainObject(request) => self.retain_object(request)?,
            ProfileSyncRequest::ReleaseObject(request) => self.release_object(request)?,
            ProfileSyncRequest::ListRetainedObjects(request) => {
                self.list_retained_objects(request)?
            }
            ProfileSyncRequest::VerifyRetainedObject(request) => {
                self.verify_retained_object(request)?
            }
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

fn validate_object_budget(size: usize, budget: &ResourceBudget) -> Result<(), BroadwebdError> {
    if size > budget.max_profile_sync_object_bytes {
        Err(BroadwebdError::ResponseTooLarge {
            limit: budget.max_profile_sync_object_bytes,
            actual: size,
        })
    } else {
        Ok(())
    }
}

fn local_fixture_provider_id(device_id: impl AsRef<str>) -> String {
    format!("local-fixture-device-{}", device_id.as_ref())
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
    use super::{LocalProfileSyncFixture, local_object_id};
    use crate::{
        BroadwebdError, PluginRegistry, ProfileSyncObjectRequest, ProfileSyncPutObjectRequest,
        ProfileSyncRequest, ProfileSyncResponse, ProfileSyncRootRequest, ProfileSyncRootUpdate,
        ResourceBudget,
    };

    #[test]
    fn local_object_ids_are_deterministic_for_test_backend() {
        assert_eq!(local_object_id(b"settings"), local_object_id(b"settings"));
        assert_ne!(local_object_id(b"settings"), local_object_id(b"calendar"));
    }

    #[test]
    fn local_fixture_devices_share_simulated_protocol_state() {
        let fixture = LocalProfileSyncFixture::new();
        let mut device_a = PluginRegistry::new();
        let mut device_b = PluginRegistry::new();
        let budget = ResourceBudget::default();

        device_a.register_service(fixture.service_for_device("a"));
        device_b.register_service(fixture.service_for_device("b"));

        let put = device_a
            .profile_sync(
                ProfileSyncRequest::PutEncryptedObject(ProfileSyncPutObjectRequest::new(
                    "default",
                    b"encrypted manifest from device a".to_vec(),
                )),
                &budget,
            )
            .expect("device a can put object into fixture");
        let ProfileSyncResponse::PutEncryptedObject { object_id } = put else {
            panic!("unexpected put response");
        };

        device_a
            .profile_sync(
                ProfileSyncRequest::RetainObject(ProfileSyncObjectRequest::new(
                    "default",
                    object_id.clone(),
                )),
                &budget,
            )
            .expect("device a can retain fixture object");
        device_a
            .profile_sync(
                ProfileSyncRequest::PublishRoot(ProfileSyncRootUpdate::new(
                    "default",
                    "profile-root",
                    object_id.clone(),
                )),
                &budget,
            )
            .expect("device a can publish fixture root");

        let resolved = device_b
            .profile_sync(
                ProfileSyncRequest::ResolveRoot(ProfileSyncRootRequest::new(
                    "default",
                    "profile-root",
                )),
                &budget,
            )
            .expect("device b can resolve fixture root");
        assert_eq!(
            resolved,
            ProfileSyncResponse::Root {
                root_id: "profile-root".to_string(),
                object_id: Some(object_id.clone())
            }
        );

        let fetched = device_b
            .profile_sync(
                ProfileSyncRequest::GetEncryptedObject(ProfileSyncObjectRequest::new(
                    "default", object_id,
                )),
                &budget,
            )
            .expect("device b can fetch fixture object");
        let ProfileSyncResponse::GetEncryptedObject { bytes, .. } = fetched else {
            panic!("unexpected get response");
        };
        assert_eq!(bytes, b"encrypted manifest from device a");
    }

    #[test]
    fn local_fixture_enforces_profile_sync_object_budget() {
        let fixture = LocalProfileSyncFixture::new();
        let mut device_a = PluginRegistry::new();
        let mut device_b = PluginRegistry::new();
        let budget = ResourceBudget::default();
        let constrained_budget = ResourceBudget {
            max_profile_sync_object_bytes: 4,
            ..ResourceBudget::default()
        };

        device_a.register_service(fixture.service_for_device("a"));
        device_b.register_service(fixture.service_for_device("b"));

        let put_error = device_a
            .profile_sync(
                ProfileSyncRequest::PutEncryptedObject(ProfileSyncPutObjectRequest::new(
                    "default",
                    b"12345".to_vec(),
                )),
                &constrained_budget,
            )
            .expect_err("oversized fixture object should be rejected on put");
        assert_eq!(
            put_error,
            BroadwebdError::ResponseTooLarge {
                limit: 4,
                actual: 5
            }
        );

        let put = device_a
            .profile_sync(
                ProfileSyncRequest::PutEncryptedObject(ProfileSyncPutObjectRequest::new(
                    "default",
                    b"12345".to_vec(),
                )),
                &budget,
            )
            .expect("device a can put object with default budget");
        let ProfileSyncResponse::PutEncryptedObject { object_id } = put else {
            panic!("unexpected put response");
        };

        let get_error = device_b
            .profile_sync(
                ProfileSyncRequest::GetEncryptedObject(ProfileSyncObjectRequest::new(
                    "default", object_id,
                )),
                &constrained_budget,
            )
            .expect_err("oversized fixture object should be rejected on get");
        assert_eq!(
            get_error,
            BroadwebdError::ResponseTooLarge {
                limit: 4,
                actual: 5
            }
        );
    }

    #[test]
    fn local_fixture_can_model_offline_devices() {
        let fixture = LocalProfileSyncFixture::new();
        let mut device_a = PluginRegistry::new();
        let mut device_b = PluginRegistry::new();
        let budget = ResourceBudget::default();

        device_a.register_service(fixture.service_for_device("a"));
        device_b.register_service(fixture.service_for_device("b"));

        let put = device_a
            .profile_sync(
                ProfileSyncRequest::PutEncryptedObject(ProfileSyncPutObjectRequest::new(
                    "default",
                    b"encrypted manifest from device a".to_vec(),
                )),
                &budget,
            )
            .expect("device a can put object into fixture");
        let ProfileSyncResponse::PutEncryptedObject { object_id } = put else {
            panic!("unexpected put response");
        };
        device_a
            .profile_sync(
                ProfileSyncRequest::PublishRoot(ProfileSyncRootUpdate::new(
                    "default",
                    "profile-root",
                    object_id.clone(),
                )),
                &budget,
            )
            .expect("device a can publish fixture root");

        fixture
            .set_device_online("b", false)
            .expect("mark fixture device offline");
        let offline_error = device_b
            .profile_sync(
                ProfileSyncRequest::ResolveRoot(ProfileSyncRootRequest::new(
                    "default",
                    "profile-root",
                )),
                &budget,
            )
            .unwrap_err();
        assert!(matches!(
            offline_error,
            BroadwebdError::Request(message)
                if message.contains("local-fixture-device-b")
                    && message.contains("offline")
        ));

        fixture
            .set_device_online("b", true)
            .expect("mark fixture device online");
        let resolved = device_b
            .profile_sync(
                ProfileSyncRequest::ResolveRoot(ProfileSyncRootRequest::new(
                    "default",
                    "profile-root",
                )),
                &budget,
            )
            .expect("device b can resolve after returning online");
        assert_eq!(
            resolved,
            ProfileSyncResponse::Root {
                root_id: "profile-root".to_string(),
                object_id: Some(object_id)
            }
        );
    }
}
