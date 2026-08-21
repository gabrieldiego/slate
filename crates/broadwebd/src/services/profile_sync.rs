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
    can_publish_roots: bool,
}

#[derive(Clone, Debug, Default)]
struct ProfileSyncStore {
    objects: BTreeMap<(String, String, String), Vec<u8>>,
    retained: BTreeSet<(String, String, String)>,
    roots: BTreeMap<(String, String), ProfileSyncRootState>,
    providers: BTreeMap<String, ProfileSyncProviderState>,
    offline_providers: BTreeSet<String>,
    delayed_transfers: BTreeSet<(String, String)>,
    delayed_roots: BTreeSet<(String, String, String, String)>,
}

#[derive(Clone, Debug)]
struct ProfileSyncRootState {
    object_id: String,
    publisher_provider_id: String,
}

#[derive(Clone, Debug)]
struct ProfileSyncProviderState {
    provider_kind: String,
    privacy_boundary: String,
    can_publish_roots: bool,
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
            can_publish_roots: true,
        }
    }
}

impl ProfileSyncService {
    pub fn new() -> Self {
        Self::default()
    }

    fn local_fixture(store: Arc<Mutex<ProfileSyncStore>>, provider_id: impl Into<String>) -> Self {
        Self::local_fixture_provider(store, provider_id, "local-fixture", true)
    }

    fn local_fixture_availability_provider(
        store: Arc<Mutex<ProfileSyncStore>>,
        provider_id: impl Into<String>,
    ) -> Self {
        Self::local_fixture_provider(store, provider_id, "local-fixture-availability", false)
    }

    fn local_fixture_provider(
        store: Arc<Mutex<ProfileSyncStore>>,
        provider_id: impl Into<String>,
        provider_kind: impl Into<String>,
        can_publish_roots: bool,
    ) -> Self {
        let provider_id = provider_id.into();
        let provider_kind = provider_kind.into();
        let privacy_boundary = LOCAL_PRIVACY_BOUNDARY.to_string();
        if let Ok(mut store) = store.lock() {
            store.providers.insert(
                provider_id.clone(),
                ProfileSyncProviderState {
                    provider_kind: provider_kind.clone(),
                    privacy_boundary: privacy_boundary.clone(),
                    can_publish_roots,
                },
            );
        }
        Self {
            store,
            provider_id,
            provider_kind,
            privacy_boundary,
            can_publish_roots,
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
        store.objects.insert(
            (self.provider_id.clone(), request.profile, object_id.clone()),
            request.bytes,
        );
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
        let bytes = find_online_object(&store, &self.provider_id, &request.profile, &object_id)
            .ok_or_else(|| {
                BroadwebdError::UnsupportedRequest(format!(
                    "profile sync object not available from an online provider: {}",
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
        let Some(bytes) = find_online_object(
            &store,
            &self.provider_id,
            &request.profile,
            &request.object_id,
        )
        .cloned() else {
            return Err(BroadwebdError::UnsupportedRequest(format!(
                "cannot retain unavailable profile sync object: {}",
                request.object_id
            )));
        };
        store.objects.insert(
            (
                self.provider_id.clone(),
                request.profile.clone(),
                request.object_id.clone(),
            ),
            bytes,
        );
        store.retained.insert((
            self.provider_id.clone(),
            request.profile,
            request.object_id.clone(),
        ));
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
        store.retained.remove(&(
            self.provider_id.clone(),
            request.profile,
            request.object_id.clone(),
        ));
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
            .filter(|(provider_id, profile, _)| {
                provider_id == &self.provider_id && profile == &request.profile
            })
            .map(|(_, _, object_id)| object_id.clone())
            .collect();
        Ok(ProfileSyncResponse::RetainedObjects { object_ids })
    }

    fn verify_retained_object(
        &self,
        request: ProfileSyncObjectRequest,
    ) -> Result<ProfileSyncResponse, BroadwebdError> {
        validate_profile(&request.profile)?;
        let store = self.store()?;
        let retained_key = (
            self.provider_id.clone(),
            request.profile.clone(),
            request.object_id.clone(),
        );
        let retained = store.retained.contains(&retained_key);
        let available = find_online_object(
            &store,
            &self.provider_id,
            &request.profile,
            &request.object_id,
        )
        .is_some();
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
        if !self.can_publish_roots {
            return Err(BroadwebdError::UnsupportedRequest(format!(
                "profile sync provider cannot publish mutable roots: {}",
                self.provider_id
            )));
        }

        let mut store = self.store()?;
        if !provider_has_object(
            &store,
            &self.provider_id,
            &request.profile,
            &request.object_id,
        ) {
            return Err(BroadwebdError::UnsupportedRequest(format!(
                "cannot publish missing local profile sync object: {}",
                request.object_id
            )));
        }
        store.roots.insert(
            (request.profile, request.root_id.clone()),
            ProfileSyncRootState {
                object_id: request.object_id.clone(),
                publisher_provider_id: self.provider_id.clone(),
            },
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
        let object_id = store
            .roots
            .get(&(request.profile.clone(), request.root_id.clone()))
            .filter(|root| {
                root_available(
                    &store,
                    root.publisher_provider_id.as_str(),
                    self.provider_id.as_str(),
                    request.profile.as_str(),
                    request.root_id.as_str(),
                )
            })
            .map(|root| root.object_id.clone());
        Ok(ProfileSyncResponse::Root {
            root_id: request.root_id.clone(),
            object_id,
        })
    }

    fn discover_providers(
        &self,
        request: ProfileSyncProfileRequest,
    ) -> Result<ProfileSyncResponse, BroadwebdError> {
        validate_profile(&request.profile)?;
        let store = self.store()?;
        let providers = if store.providers.is_empty() {
            vec![ProfileSyncProviderRecord {
                provider_id: self.provider_id.clone(),
                provider_kind: self.provider_kind.clone(),
                privacy_boundary: self.privacy_boundary.clone(),
                retained_objects: retained_object_count(
                    &store,
                    &self.provider_id,
                    &request.profile,
                ),
                can_publish_roots: self.can_publish_roots,
            }]
        } else {
            store
                .providers
                .iter()
                .filter(|(provider_id, _)| !store.offline_providers.contains(provider_id.as_str()))
                .map(|(provider_id, state)| ProfileSyncProviderRecord {
                    provider_id: provider_id.clone(),
                    provider_kind: state.provider_kind.clone(),
                    privacy_boundary: state.privacy_boundary.clone(),
                    retained_objects: retained_object_count(&store, provider_id, &request.profile),
                    can_publish_roots: state.can_publish_roots,
                })
                .collect()
        };
        Ok(ProfileSyncResponse::Providers { providers })
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

    pub fn service_for_availability_provider(
        &self,
        provider_id: impl AsRef<str>,
    ) -> ProfileSyncService {
        ProfileSyncService::local_fixture_availability_provider(
            self.store.clone(),
            local_fixture_availability_provider_id(provider_id),
        )
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

    pub fn set_device_transfer_available(
        &self,
        source_device_id: impl AsRef<str>,
        target_device_id: impl AsRef<str>,
        available: bool,
    ) -> Result<(), BroadwebdError> {
        let source_provider_id = local_fixture_provider_id(source_device_id);
        let target_provider_id = local_fixture_provider_id(target_device_id);
        let mut store = self
            .store
            .lock()
            .map_err(|_| BroadwebdError::Request("profile sync store lock poisoned".to_string()))?;
        let link = (source_provider_id, target_provider_id);
        if available {
            store.delayed_transfers.remove(&link);
        } else {
            store.delayed_transfers.insert(link);
        }
        Ok(())
    }

    pub fn set_device_root_available(
        &self,
        source_device_id: impl AsRef<str>,
        target_device_id: impl AsRef<str>,
        profile: impl AsRef<str>,
        root_id: impl AsRef<str>,
        available: bool,
    ) -> Result<(), BroadwebdError> {
        let profile = profile.as_ref();
        validate_profile(profile)?;
        let source_provider_id = local_fixture_provider_id(source_device_id);
        let target_provider_id = local_fixture_provider_id(target_device_id);
        let link = (
            source_provider_id,
            target_provider_id,
            profile.to_string(),
            root_id.as_ref().to_string(),
        );
        let mut store = self
            .store
            .lock()
            .map_err(|_| BroadwebdError::Request("profile sync store lock poisoned".to_string()))?;
        if available {
            store.delayed_roots.remove(&link);
        } else {
            store.delayed_roots.insert(link);
        }
        Ok(())
    }
}

impl ApplicationServicePlugin for ProfileSyncService {
    fn metadata(&self) -> PluginMetadata {
        let capabilities: &[&str] = if self.can_publish_roots {
            &[
                "profile-sync/fake",
                "profile-sync/object-transfer",
                "profile-sync/local-retention",
                "profile-sync/mutable-root",
                "profile-sync/provider-discovery",
            ]
        } else {
            &[
                "profile-sync/fake",
                "profile-sync/object-transfer",
                "profile-sync/local-retention",
                "profile-sync/availability-provider",
                "profile-sync/provider-discovery",
            ]
        };
        PluginMetadata::new(PROFILE_SYNC_PLUGIN, PluginKind::ApplicationService)
            .with_capabilities(capabilities)
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

fn retained_object_count(store: &ProfileSyncStore, provider_id: &str, profile: &str) -> usize {
    store
        .retained
        .iter()
        .filter(|(retained_provider_id, retained_profile, _)| {
            retained_provider_id == provider_id && retained_profile == profile
        })
        .count()
}

fn provider_has_object(
    store: &ProfileSyncStore,
    provider_id: &str,
    profile: &str,
    object_id: &str,
) -> bool {
    store.objects.contains_key(&(
        provider_id.to_string(),
        profile.to_string(),
        object_id.to_string(),
    ))
}

fn find_online_object<'a>(
    store: &'a ProfileSyncStore,
    requester_provider_id: &str,
    profile: &str,
    object_id: &str,
) -> Option<&'a Vec<u8>> {
    store
        .objects
        .iter()
        .find(|((provider_id, stored_profile, stored_object_id), _)| {
            stored_profile == profile
                && stored_object_id == object_id
                && !store.offline_providers.contains(provider_id.as_str())
                && transfer_available(store, provider_id, requester_provider_id)
        })
        .map(|(_, bytes)| bytes)
}

fn transfer_available(
    store: &ProfileSyncStore,
    source_provider_id: &str,
    requester_provider_id: &str,
) -> bool {
    source_provider_id == requester_provider_id
        || !store.delayed_transfers.contains(&(
            source_provider_id.to_string(),
            requester_provider_id.to_string(),
        ))
}

fn root_available(
    store: &ProfileSyncStore,
    source_provider_id: &str,
    requester_provider_id: &str,
    profile: &str,
    root_id: &str,
) -> bool {
    source_provider_id == requester_provider_id
        || !store.delayed_roots.contains(&(
            source_provider_id.to_string(),
            requester_provider_id.to_string(),
            profile.to_string(),
            root_id.to_string(),
        ))
}

fn local_fixture_provider_id(device_id: impl AsRef<str>) -> String {
    format!("local-fixture-device-{}", device_id.as_ref())
}

fn local_fixture_availability_provider_id(provider_id: impl AsRef<str>) -> String {
    format!("local-fixture-availability-{}", provider_id.as_ref())
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
        BroadwebdError, PluginRegistry, ProfileSyncObjectRequest, ProfileSyncProfileRequest,
        ProfileSyncPutObjectRequest, ProfileSyncRequest, ProfileSyncResponse,
        ProfileSyncRootRequest, ProfileSyncRootUpdate, ResourceBudget,
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
    fn local_fixture_can_delay_mutable_root_propagation_between_devices() {
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
                    b"encrypted root target from device a".to_vec(),
                )),
                &budget,
            )
            .expect("device a can put root target object into fixture");
        let ProfileSyncResponse::PutEncryptedObject { object_id } = put else {
            panic!("unexpected put response");
        };
        fixture
            .set_device_root_available("a", "b", "default", "profile-root", false)
            .expect("delay root propagation from device a to device b");
        device_a
            .profile_sync(
                ProfileSyncRequest::PublishRoot(ProfileSyncRootUpdate::new(
                    "default",
                    "profile-root",
                    object_id.clone(),
                )),
                &budget,
            )
            .expect("device a can publish fixture root while propagation is delayed");

        let unresolved = device_b
            .profile_sync(
                ProfileSyncRequest::ResolveRoot(ProfileSyncRootRequest::new(
                    "default",
                    "profile-root",
                )),
                &budget,
            )
            .expect("device b can resolve delayed fixture root");
        assert_eq!(
            unresolved,
            ProfileSyncResponse::Root {
                root_id: "profile-root".to_string(),
                object_id: None,
            }
        );
        let source_resolved = device_a
            .profile_sync(
                ProfileSyncRequest::ResolveRoot(ProfileSyncRootRequest::new(
                    "default",
                    "profile-root",
                )),
                &budget,
            )
            .expect("device a can resolve its own delayed fixture root");
        assert_eq!(
            source_resolved,
            ProfileSyncResponse::Root {
                root_id: "profile-root".to_string(),
                object_id: Some(object_id.clone()),
            }
        );
        let direct_fetch = device_b
            .profile_sync(
                ProfileSyncRequest::GetEncryptedObject(ProfileSyncObjectRequest::new(
                    "default",
                    object_id.clone(),
                )),
                &budget,
            )
            .expect("root delay does not imply object transfer delay");
        assert_eq!(
            direct_fetch,
            ProfileSyncResponse::GetEncryptedObject {
                object_id: object_id.clone(),
                bytes: b"encrypted root target from device a".to_vec(),
            }
        );

        fixture
            .set_device_root_available("a", "b", "default", "profile-root", true)
            .expect("release root propagation from device a to device b");
        let resolved = device_b
            .profile_sync(
                ProfileSyncRequest::ResolveRoot(ProfileSyncRootRequest::new(
                    "default",
                    "profile-root",
                )),
                &budget,
            )
            .expect("device b can resolve root after propagation release");
        assert_eq!(
            resolved,
            ProfileSyncResponse::Root {
                root_id: "profile-root".to_string(),
                object_id: Some(object_id),
            }
        );
    }

    #[test]
    fn local_fixture_discovers_online_providers_and_scopes_retention() {
        let fixture = LocalProfileSyncFixture::new();
        let mut device_a = PluginRegistry::new();
        let mut device_b = PluginRegistry::new();
        let mut device_c = PluginRegistry::new();
        let budget = ResourceBudget::default();

        device_a.register_service(fixture.service_for_device("a"));
        device_b.register_service(fixture.service_for_device("b"));
        device_c.register_service(fixture.service_for_device("c"));

        let put = device_a
            .profile_sync(
                ProfileSyncRequest::PutEncryptedObject(ProfileSyncPutObjectRequest::new(
                    "default",
                    b"encrypted profile object".to_vec(),
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

        let verified_from_b = device_b
            .profile_sync(
                ProfileSyncRequest::VerifyRetainedObject(ProfileSyncObjectRequest::new(
                    "default",
                    object_id.clone(),
                )),
                &budget,
            )
            .expect("device b can verify fixture object availability");
        assert_eq!(
            verified_from_b,
            ProfileSyncResponse::RetainedObjectStatus {
                object_id: object_id.clone(),
                retained: false,
                available: true,
            }
        );

        let retained_by_b = device_b
            .profile_sync(
                ProfileSyncRequest::ListRetainedObjects(ProfileSyncProfileRequest::new("default")),
                &budget,
            )
            .expect("device b can list retained objects");
        assert_eq!(
            retained_by_b,
            ProfileSyncResponse::RetainedObjects {
                object_ids: Vec::new(),
            }
        );

        let providers = device_b
            .profile_sync(
                ProfileSyncRequest::DiscoverProviders(ProfileSyncProfileRequest::new("default")),
                &budget,
            )
            .expect("device b can discover local fixture providers");
        let ProfileSyncResponse::Providers { providers } = providers else {
            panic!("unexpected providers response");
        };
        let provider_statuses = providers
            .iter()
            .map(|provider| (provider.provider_id.as_str(), provider.retained_objects))
            .collect::<Vec<_>>();
        assert_eq!(
            provider_statuses,
            vec![
                ("local-fixture-device-a", 1),
                ("local-fixture-device-b", 0),
                ("local-fixture-device-c", 0),
            ]
        );
        assert!(providers.iter().all(|provider| {
            provider.provider_kind == "local-fixture"
                && provider.privacy_boundary.contains("no sockets")
                && provider.can_publish_roots
        }));

        fixture
            .set_device_online("c", false)
            .expect("mark fixture device c offline");
        let providers = device_b
            .profile_sync(
                ProfileSyncRequest::DiscoverProviders(ProfileSyncProfileRequest::new("default")),
                &budget,
            )
            .expect("device b can rediscover local fixture providers");
        let ProfileSyncResponse::Providers { providers } = providers else {
            panic!("unexpected providers response");
        };
        let provider_ids = providers
            .iter()
            .map(|provider| provider.provider_id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            provider_ids,
            vec!["local-fixture-device-a", "local-fixture-device-b"]
        );
    }

    #[test]
    fn local_fixture_availability_provider_cannot_publish_roots() {
        let fixture = LocalProfileSyncFixture::new();
        let mut device_a = PluginRegistry::new();
        let mut availability_provider = PluginRegistry::new();
        let budget = ResourceBudget::default();

        device_a.register_service(fixture.service_for_device("a"));
        availability_provider.register_service(fixture.service_for_availability_provider("pin-1"));

        let put = device_a
            .profile_sync(
                ProfileSyncRequest::PutEncryptedObject(ProfileSyncPutObjectRequest::new(
                    "default",
                    b"encrypted object for availability provider".to_vec(),
                )),
                &budget,
            )
            .expect("device a can put object into fixture");
        let ProfileSyncResponse::PutEncryptedObject { object_id } = put else {
            panic!("unexpected put response");
        };

        availability_provider
            .profile_sync(
                ProfileSyncRequest::RetainObject(ProfileSyncObjectRequest::new(
                    "default",
                    object_id.clone(),
                )),
                &budget,
            )
            .expect("availability provider can retain encrypted bytes");

        let publish_error = availability_provider
            .profile_sync(
                ProfileSyncRequest::PublishRoot(ProfileSyncRootUpdate::new(
                    "default",
                    "settings/latest",
                    object_id.clone(),
                )),
                &budget,
            )
            .expect_err("availability provider must not publish mutable roots");
        assert!(matches!(
            publish_error,
            BroadwebdError::UnsupportedRequest(message)
                if message.contains("cannot publish mutable roots")
                    && message.contains("local-fixture-availability-pin-1")
        ));

        let providers = device_a
            .profile_sync(
                ProfileSyncRequest::DiscoverProviders(ProfileSyncProfileRequest::new("default")),
                &budget,
            )
            .expect("device a can discover availability provider policy");
        let ProfileSyncResponse::Providers { providers } = providers else {
            panic!("unexpected providers response");
        };
        let provider = providers
            .iter()
            .find(|provider| provider.provider_id == "local-fixture-availability-pin-1")
            .expect("availability provider is discoverable");
        assert_eq!(provider.provider_kind, "local-fixture-availability");
        assert_eq!(provider.retained_objects, 1);
        assert!(!provider.can_publish_roots);

        device_a
            .profile_sync(
                ProfileSyncRequest::PublishRoot(ProfileSyncRootUpdate::new(
                    "default",
                    "settings/latest",
                    object_id.clone(),
                )),
                &budget,
            )
            .expect("logged-in device can still publish mutable root");
        let resolved = availability_provider
            .profile_sync(
                ProfileSyncRequest::ResolveRoot(ProfileSyncRootRequest::new(
                    "default",
                    "settings/latest",
                )),
                &budget,
            )
            .expect("availability provider can resolve mutable root");
        assert_eq!(
            resolved,
            ProfileSyncResponse::Root {
                root_id: "settings/latest".to_string(),
                object_id: Some(object_id),
            }
        );
    }

    #[test]
    fn local_fixture_object_availability_follows_online_providers() {
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
                    b"encrypted object only on device a".to_vec(),
                )),
                &budget,
            )
            .expect("device a can put object into fixture");
        let ProfileSyncResponse::PutEncryptedObject { object_id } = put else {
            panic!("unexpected put response");
        };

        fixture
            .set_device_online("a", false)
            .expect("mark fixture device a offline");
        let unavailable = device_b
            .profile_sync(
                ProfileSyncRequest::GetEncryptedObject(ProfileSyncObjectRequest::new(
                    "default",
                    object_id.clone(),
                )),
                &budget,
            )
            .expect_err("device b cannot fetch when only provider is offline");
        assert!(matches!(
            unavailable,
            BroadwebdError::UnsupportedRequest(message)
                if message.contains("not available")
        ));

        fixture
            .set_device_online("a", true)
            .expect("mark fixture device a online");
        device_b
            .profile_sync(
                ProfileSyncRequest::RetainObject(ProfileSyncObjectRequest::new(
                    "default",
                    object_id.clone(),
                )),
                &budget,
            )
            .expect("device b can retain object while device a is online");
        fixture
            .set_device_online("a", false)
            .expect("mark fixture device a offline again");

        let fetched = device_b
            .profile_sync(
                ProfileSyncRequest::GetEncryptedObject(ProfileSyncObjectRequest::new(
                    "default",
                    object_id.clone(),
                )),
                &budget,
            )
            .expect("device b can fetch retained object after device a goes offline");
        assert_eq!(
            fetched,
            ProfileSyncResponse::GetEncryptedObject {
                object_id: object_id.clone(),
                bytes: b"encrypted object only on device a".to_vec(),
            }
        );
        let verified = device_b
            .profile_sync(
                ProfileSyncRequest::VerifyRetainedObject(ProfileSyncObjectRequest::new(
                    "default", object_id,
                )),
                &budget,
            )
            .expect("device b can verify retained local availability");
        assert_eq!(
            verified,
            ProfileSyncResponse::RetainedObjectStatus {
                object_id: local_object_id(b"encrypted object only on device a"),
                retained: true,
                available: true,
            }
        );
    }

    #[test]
    fn local_fixture_can_delay_object_transfer_between_devices() {
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
                    b"encrypted object waiting for delayed transfer".to_vec(),
                )),
                &budget,
            )
            .expect("device a can put object into fixture");
        let ProfileSyncResponse::PutEncryptedObject { object_id } = put else {
            panic!("unexpected put response");
        };
        fixture
            .set_device_transfer_available("a", "b", false)
            .expect("delay transfer from device a to device b");

        let unavailable = device_b
            .profile_sync(
                ProfileSyncRequest::GetEncryptedObject(ProfileSyncObjectRequest::new(
                    "default",
                    object_id.clone(),
                )),
                &budget,
            )
            .expect_err("device b cannot fetch while transfer is delayed");
        assert!(matches!(
            unavailable,
            BroadwebdError::UnsupportedRequest(message)
                if message.contains("not available")
        ));
        let retained_status = device_b
            .profile_sync(
                ProfileSyncRequest::VerifyRetainedObject(ProfileSyncObjectRequest::new(
                    "default",
                    object_id.clone(),
                )),
                &budget,
            )
            .expect("device b can verify delayed object status");
        assert_eq!(
            retained_status,
            ProfileSyncResponse::RetainedObjectStatus {
                object_id: object_id.clone(),
                retained: false,
                available: false,
            }
        );
        let retain_error = device_b
            .profile_sync(
                ProfileSyncRequest::RetainObject(ProfileSyncObjectRequest::new(
                    "default",
                    object_id.clone(),
                )),
                &budget,
            )
            .expect_err("device b cannot retain while transfer is delayed");
        assert!(matches!(
            retain_error,
            BroadwebdError::UnsupportedRequest(message)
                if message.contains("cannot retain unavailable")
        ));

        let own_fetch = device_a
            .profile_sync(
                ProfileSyncRequest::GetEncryptedObject(ProfileSyncObjectRequest::new(
                    "default",
                    object_id.clone(),
                )),
                &budget,
            )
            .expect("device a can still fetch its own object");
        assert_eq!(
            own_fetch,
            ProfileSyncResponse::GetEncryptedObject {
                object_id: object_id.clone(),
                bytes: b"encrypted object waiting for delayed transfer".to_vec(),
            }
        );

        fixture
            .set_device_transfer_available("a", "b", true)
            .expect("release delayed transfer from device a to device b");
        device_b
            .profile_sync(
                ProfileSyncRequest::RetainObject(ProfileSyncObjectRequest::new(
                    "default",
                    object_id.clone(),
                )),
                &budget,
            )
            .expect("device b can retain after delayed transfer is released");
        let fetched = device_b
            .profile_sync(
                ProfileSyncRequest::GetEncryptedObject(ProfileSyncObjectRequest::new(
                    "default",
                    object_id.clone(),
                )),
                &budget,
            )
            .expect("device b can fetch after retaining delayed object");
        assert_eq!(
            fetched,
            ProfileSyncResponse::GetEncryptedObject {
                object_id,
                bytes: b"encrypted object waiting for delayed transfer".to_vec(),
            }
        );
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
