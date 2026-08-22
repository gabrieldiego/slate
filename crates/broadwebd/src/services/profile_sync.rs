#[cfg(any(test, feature = "test-fixtures"))]
use crate::IpfsKuboProfileSyncRpc;
use crate::{
    ApplicationServicePlugin, BroadwebdError, PROFILE_SYNC_PLUGIN, PluginKind, PluginMetadata,
    PluginRegistry, ProfileSyncObjectRequest, ProfileSyncProfileRequest, ProfileSyncProviderHealth,
    ProfileSyncProviderRecord, ProfileSyncProviderRoles, ProfileSyncPutObjectRequest,
    ProfileSyncRequest, ProfileSyncResponse, ProfileSyncRootCandidate, ProfileSyncRootHealth,
    ProfileSyncRootHealthRequest, ProfileSyncRootRequest, ProfileSyncRootUpdate, ResourceBudget,
    ResourceProfile, ServiceRequest, ServiceResponse,
};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

const LOCAL_PROVIDER_ID: &str = "local-fake-profile-sync";
const LOCAL_PROVIDER_KIND: &str = "local-fake";
const LOCAL_PRIVACY_BOUNDARY: &str = "in-memory local test backend; no sockets or external network";
const MAX_PROFILE_SYNC_OBJECT_ID_BYTES: usize = 2048;
const MAX_PROFILE_SYNC_ROOT_ID_BYTES: usize = 256;

#[derive(Clone, Debug)]
pub struct ProfileSyncService {
    store: Arc<Mutex<ProfileSyncStore>>,
    provider_id: String,
    provider_kind: String,
    privacy_boundary: String,
    roles: ProfileSyncProviderRoles,
    #[cfg(any(test, feature = "test-fixtures"))]
    kubo_rpc: Option<IpfsKuboProfileSyncRpc>,
}

#[derive(Clone, Debug, Default)]
struct ProfileSyncStore {
    objects: BTreeMap<(String, String, String), Vec<u8>>,
    retained: BTreeSet<(String, String, String)>,
    roots: BTreeMap<(String, String, String), ProfileSyncRootState>,
    next_root_sequence: u64,
    providers: BTreeMap<String, ProfileSyncProviderState>,
    next_provider_seen_sequence: u64,
    minimum_provider_seen_sequence: u64,
    offline_providers: BTreeSet<String>,
    delayed_transfers: BTreeSet<(String, String)>,
    delayed_roots: BTreeSet<(String, String, String, String)>,
    retention_blocked_providers: BTreeSet<String>,
    retention_quota_by_provider: BTreeMap<String, usize>,
}

#[derive(Clone, Debug)]
struct ProfileSyncRootState {
    object_id: String,
    publisher_provider_id: String,
    publish_sequence: u64,
}

#[derive(Clone, Debug)]
struct ProfileSyncProviderState {
    provider_kind: String,
    privacy_boundary: String,
    roles: ProfileSyncProviderRoles,
    last_seen_sequence: u64,
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
            roles: ProfileSyncProviderRoles::logged_in_device(),
            #[cfg(any(test, feature = "test-fixtures"))]
            kubo_rpc: None,
        }
    }
}

impl ProfileSyncService {
    pub fn new() -> Self {
        Self::default()
    }

    fn local_fixture(store: Arc<Mutex<ProfileSyncStore>>, provider_id: impl Into<String>) -> Self {
        Self::local_fixture_provider(
            store,
            provider_id,
            "local-fixture",
            ProfileSyncProviderRoles::logged_in_device(),
        )
    }

    fn local_fixture_availability_provider(
        store: Arc<Mutex<ProfileSyncStore>>,
        provider_id: impl Into<String>,
    ) -> Self {
        Self::local_fixture_provider(
            store,
            provider_id,
            "local-fixture-availability",
            ProfileSyncProviderRoles::availability_provider(),
        )
    }

    fn local_fixture_provider(
        store: Arc<Mutex<ProfileSyncStore>>,
        provider_id: impl Into<String>,
        provider_kind: impl Into<String>,
        roles: ProfileSyncProviderRoles,
    ) -> Self {
        let provider_id = provider_id.into();
        let provider_kind = provider_kind.into();
        let privacy_boundary = LOCAL_PRIVACY_BOUNDARY.to_string();
        if let Ok(mut store) = store.lock() {
            let last_seen_sequence = next_provider_seen_sequence(&mut store);
            store.providers.insert(
                provider_id.clone(),
                ProfileSyncProviderState {
                    provider_kind: provider_kind.clone(),
                    privacy_boundary: privacy_boundary.clone(),
                    roles,
                    last_seen_sequence,
                },
            );
        }
        Self {
            store,
            provider_id,
            provider_kind,
            privacy_boundary,
            roles,
            #[cfg(any(test, feature = "test-fixtures"))]
            kubo_rpc: None,
        }
    }

    #[cfg(any(test, feature = "test-fixtures"))]
    pub fn kubo_fixture(
        api_base_url: impl Into<String>,
        provider_id: impl Into<String>,
    ) -> Result<Self, BroadwebdError> {
        Ok(Self {
            store: Arc::new(Mutex::new(ProfileSyncStore::default())),
            provider_id: provider_id.into(),
            provider_kind: "ipfs-kubo-fixture".to_string(),
            privacy_boundary:
                "in-process Kubo profile-sync fixture; no sockets, DNS, loopback listener, or external network"
                    .to_string(),
            roles: ProfileSyncProviderRoles::logged_in_device(),
            kubo_rpc: Some(IpfsKuboProfileSyncRpc::local(api_base_url)?),
        })
    }

    fn ensure_online(&self) -> Result<(), BroadwebdError> {
        let store = self.store()?;
        if store.offline_providers.contains(&self.provider_id) {
            Err(BroadwebdError::Request(format!(
                "profile sync provider is offline: {}",
                self.provider_id
            )))
        } else {
            self.require_role(self.roles.connectivity, "profile-sync/local-connectivity")
        }
    }

    fn put_object(
        &self,
        request: ProfileSyncPutObjectRequest,
        budget: &ResourceBudget,
    ) -> Result<ProfileSyncResponse, BroadwebdError> {
        validate_profile(&request.profile)?;
        self.require_role(self.roles.object_transfer, "profile-sync/object-transfer")?;
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
        validate_profile_sync_object_id(&request.object_id)?;
        self.require_role(self.roles.object_transfer, "profile-sync/object-transfer")?;
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
        validate_profile_sync_object_id(&request.object_id)?;
        self.require_role(self.roles.availability, "profile-sync/availability")?;
        self.require_role(self.roles.object_transfer, "profile-sync/object-transfer")?;
        let mut store = self.store()?;
        if store
            .retention_blocked_providers
            .contains(&self.provider_id)
        {
            return Err(BroadwebdError::UnsupportedRequest(format!(
                "profile sync retention is blocked by local pinning policy for provider: {}",
                self.provider_id
            )));
        }
        let retained_key = (
            self.provider_id.clone(),
            request.profile.clone(),
            request.object_id.clone(),
        );
        if !store.retained.contains(&retained_key)
            && let Some(max_retained_objects) =
                store.retention_quota_by_provider.get(&self.provider_id)
            && retained_provider_object_count(&store, &self.provider_id) >= *max_retained_objects
        {
            return Err(BroadwebdError::UnsupportedRequest(format!(
                "profile sync retention quota exceeded for provider {}: max {} retained objects",
                self.provider_id, max_retained_objects
            )));
        }
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
        store.retained.insert(retained_key);
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
        validate_profile_sync_object_id(&request.object_id)?;
        self.require_role(self.roles.availability, "profile-sync/availability")?;
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
        self.require_role(self.roles.availability, "profile-sync/availability")?;
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
        validate_profile_sync_object_id(&request.object_id)?;
        self.require_role(self.roles.availability, "profile-sync/availability")?;
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
        validate_profile_sync_root_id(&request.root_id)?;
        validate_profile_sync_object_id(&request.object_id)?;
        self.require_role(self.roles.mutable_roots, "profile-sync/mutable-root")?;

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
        store.next_root_sequence += 1;
        let publish_sequence = store.next_root_sequence;
        store.roots.insert(
            (
                request.profile,
                request.root_id.clone(),
                self.provider_id.clone(),
            ),
            ProfileSyncRootState {
                object_id: request.object_id.clone(),
                publisher_provider_id: self.provider_id.clone(),
                publish_sequence,
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
        validate_profile_sync_root_id(&request.root_id)?;
        self.require_role(self.roles.discovery, "profile-sync/provider-discovery")?;
        let store = self.store()?;
        let object_id = latest_visible_root_candidate(
            &store,
            self.provider_id.as_str(),
            request.profile.as_str(),
            request.root_id.as_str(),
        )
        .map(|candidate| candidate.object_id);
        Ok(ProfileSyncResponse::Root {
            root_id: request.root_id.clone(),
            object_id,
        })
    }

    fn list_root_candidates(
        &self,
        request: ProfileSyncRootRequest,
    ) -> Result<ProfileSyncResponse, BroadwebdError> {
        validate_profile(&request.profile)?;
        validate_profile_sync_root_id(&request.root_id)?;
        self.require_role(self.roles.discovery, "profile-sync/provider-discovery")?;
        let store = self.store()?;
        Ok(ProfileSyncResponse::RootCandidates {
            root_id: request.root_id.clone(),
            candidates: visible_root_candidates(
                &store,
                self.provider_id.as_str(),
                request.profile.as_str(),
                request.root_id.as_str(),
            ),
        })
    }

    fn root_health(
        &self,
        request: ProfileSyncRootHealthRequest,
    ) -> Result<ProfileSyncResponse, BroadwebdError> {
        validate_profile(&request.profile)?;
        validate_profile_sync_root_id(&request.root_id)?;
        self.require_role(self.roles.discovery, "profile-sync/provider-discovery")?;
        let store = self.store()?;
        let candidates = visible_root_candidates(
            &store,
            self.provider_id.as_str(),
            request.profile.as_str(),
            request.root_id.as_str(),
        );
        let delayed_candidates = delayed_root_candidates(
            &store,
            self.provider_id.as_str(),
            request.profile.as_str(),
            request.root_id.as_str(),
        );
        let delayed_publisher_provider_ids = delayed_candidates
            .iter()
            .map(|candidate| candidate.publisher_provider_id.clone())
            .collect::<Vec<_>>();
        let latest_object_id = candidates
            .first()
            .map(|candidate| candidate.object_id.clone());
        let latest_object_available = latest_object_id.as_deref().is_some_and(|object_id| {
            find_online_object(
                &store,
                self.provider_id.as_str(),
                request.profile.as_str(),
                object_id,
            )
            .is_some()
        });
        let delayed_object_provider_ids = latest_object_id
            .as_deref()
            .map(|object_id| {
                delayed_object_provider_ids(
                    &store,
                    self.provider_id.as_str(),
                    request.profile.as_str(),
                    object_id,
                )
            })
            .unwrap_or_default();
        let online_retaining_providers = latest_object_id
            .as_deref()
            .map(|object_id| {
                online_retaining_provider_count(&store, request.profile.as_str(), object_id)
            })
            .unwrap_or_default();
        let (degraded, message) = profile_sync_root_health_message(
            candidates.len(),
            delayed_candidates.len(),
            latest_object_available,
            delayed_object_provider_ids.len(),
            online_retaining_providers,
            request.minimum_online_retaining_providers,
        );

        Ok(ProfileSyncResponse::RootHealth {
            health: ProfileSyncRootHealth {
                profile: request.profile,
                root_id: request.root_id,
                visible_candidates: candidates.len(),
                delayed_candidates: delayed_candidates.len(),
                delayed_publisher_provider_ids,
                latest_object_id,
                latest_object_available,
                delayed_object_provider_ids,
                online_retaining_providers,
                minimum_online_retaining_providers: request.minimum_online_retaining_providers,
                degraded,
                message,
            },
        })
    }

    fn discover_providers(
        &self,
        request: ProfileSyncProfileRequest,
    ) -> Result<ProfileSyncResponse, BroadwebdError> {
        validate_profile(&request.profile)?;
        self.require_role(self.roles.discovery, "profile-sync/provider-discovery")?;
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
                roles: self.roles,
                can_publish_roots: self.roles.mutable_roots,
            }]
        } else {
            store
                .providers
                .iter()
                .filter(|(provider_id, state)| {
                    provider_is_online(&store, provider_id, state.roles)
                        && provider_is_fresh(&store, state.last_seen_sequence)
                })
                .map(|(provider_id, state)| ProfileSyncProviderRecord {
                    provider_id: provider_id.clone(),
                    provider_kind: state.provider_kind.clone(),
                    privacy_boundary: state.privacy_boundary.clone(),
                    retained_objects: retained_object_count(&store, provider_id, &request.profile),
                    roles: state.roles,
                    can_publish_roots: state.roles.mutable_roots,
                })
                .collect()
        };
        Ok(ProfileSyncResponse::Providers { providers })
    }

    fn provider_health(
        &self,
        request: ProfileSyncProfileRequest,
    ) -> Result<ProfileSyncResponse, BroadwebdError> {
        validate_profile(&request.profile)?;
        self.require_role(self.roles.discovery, "profile-sync/provider-discovery")?;
        let store = self.store()?;
        let providers = provider_health_entries(&store, self.provider_id.as_str(), self.roles);
        let known_providers = providers.len();
        let mut online_providers = 0;
        let mut fresh_online_providers = 0;
        let mut stale_online_providers = 0;
        let mut fresh_online_provider_ids = Vec::new();
        let mut stale_online_provider_ids = Vec::new();
        let mut offline_provider_ids = Vec::new();
        let mut object_transfer_providers = 0;
        let mut availability_providers = 0;
        let mut mutable_root_providers = 0;
        let mut retained_objects = 0;
        for provider in providers {
            if provider.online {
                online_providers += 1;
                if provider.fresh {
                    fresh_online_providers += 1;
                    fresh_online_provider_ids.push(provider.provider_id.to_string());
                    retained_objects +=
                        retained_object_count(&store, provider.provider_id, &request.profile);
                    if provider.roles.object_transfer {
                        object_transfer_providers += 1;
                    }
                    if provider.roles.availability {
                        availability_providers += 1;
                    }
                    if provider.roles.mutable_roots {
                        mutable_root_providers += 1;
                    }
                } else {
                    stale_online_providers += 1;
                    stale_online_provider_ids.push(provider.provider_id.to_string());
                }
            } else {
                offline_provider_ids.push(provider.provider_id.to_string());
            }
        }
        let offline_providers = known_providers.saturating_sub(online_providers);
        let (degraded, message) = profile_sync_health_message(
            online_providers,
            fresh_online_providers,
            object_transfer_providers,
            availability_providers,
            mutable_root_providers,
        );

        Ok(ProfileSyncResponse::ProviderHealth {
            health: ProfileSyncProviderHealth {
                profile: request.profile,
                known_providers,
                online_providers,
                offline_providers,
                fresh_online_providers,
                stale_online_providers,
                fresh_online_provider_ids,
                stale_online_provider_ids,
                offline_provider_ids,
                minimum_provider_seen_sequence: store.minimum_provider_seen_sequence,
                object_transfer_providers,
                availability_providers,
                mutable_root_providers,
                retained_objects,
                degraded,
                message,
            },
        })
    }

    fn store(&self) -> Result<std::sync::MutexGuard<'_, ProfileSyncStore>, BroadwebdError> {
        self.store
            .lock()
            .map_err(|_| BroadwebdError::Request("profile sync store lock poisoned".to_string()))
    }

    fn require_role(&self, enabled: bool, role: &str) -> Result<(), BroadwebdError> {
        if enabled {
            Ok(())
        } else {
            Err(BroadwebdError::UnsupportedRequest(format!(
                "profile sync provider lacks {role} role: {}",
                self.provider_id
            )))
        }
    }

    #[cfg(any(test, feature = "test-fixtures"))]
    fn kubo_fixture_profile_sync(
        &self,
        kubo_rpc: &IpfsKuboProfileSyncRpc,
        request: ProfileSyncRequest,
        budget: &ResourceBudget,
    ) -> Result<ProfileSyncResponse, BroadwebdError> {
        match request {
            ProfileSyncRequest::PutEncryptedObject(request) => {
                validate_profile(&request.profile)?;
                self.require_role(self.roles.object_transfer, "profile-sync/object-transfer")?;
                let object_id = kubo_rpc.put_encrypted_object_fixture(&request.bytes, budget)?;
                Ok(ProfileSyncResponse::PutEncryptedObject { object_id })
            }
            ProfileSyncRequest::GetEncryptedObject(request) => {
                validate_profile(&request.profile)?;
                validate_profile_sync_object_id(&request.object_id)?;
                self.require_role(self.roles.object_transfer, "profile-sync/object-transfer")?;
                Err(BroadwebdError::UnsupportedRequest(
                    "Kubo profile-sync fixture backend does not fetch encrypted objects yet"
                        .to_string(),
                ))
            }
            ProfileSyncRequest::RetainObject(request) => {
                validate_profile(&request.profile)?;
                validate_profile_sync_object_id(&request.object_id)?;
                self.require_role(self.roles.availability, "profile-sync/availability")?;
                self.require_role(self.roles.object_transfer, "profile-sync/object-transfer")?;
                kubo_rpc.retain_object_fixture(&request.object_id, budget)?;
                Ok(ProfileSyncResponse::RetainObject {
                    object_id: request.object_id,
                    retained: true,
                })
            }
            ProfileSyncRequest::ReleaseObject(request) => {
                validate_profile(&request.profile)?;
                validate_profile_sync_object_id(&request.object_id)?;
                self.require_role(self.roles.availability, "profile-sync/availability")?;
                kubo_rpc.release_object_fixture(&request.object_id, budget)?;
                Ok(ProfileSyncResponse::ReleaseObject {
                    object_id: request.object_id,
                    retained: false,
                })
            }
            ProfileSyncRequest::ListRetainedObjects(request) => {
                validate_profile(&request.profile)?;
                self.require_role(self.roles.availability, "profile-sync/availability")?;
                Err(BroadwebdError::UnsupportedRequest(
                    "Kubo profile-sync fixture backend does not list retained objects yet"
                        .to_string(),
                ))
            }
            ProfileSyncRequest::VerifyRetainedObject(request) => {
                validate_profile(&request.profile)?;
                validate_profile_sync_object_id(&request.object_id)?;
                self.require_role(self.roles.availability, "profile-sync/availability")?;
                let retained =
                    kubo_rpc.verify_retained_object_fixture(&request.object_id, budget)?;
                Ok(ProfileSyncResponse::RetainedObjectStatus {
                    object_id: request.object_id,
                    retained,
                    available: retained,
                })
            }
            ProfileSyncRequest::PublishRoot(request) => {
                validate_profile(&request.profile)?;
                validate_profile_sync_root_id(&request.root_id)?;
                validate_profile_sync_object_id(&request.object_id)?;
                self.require_role(self.roles.mutable_roots, "profile-sync/mutable-root")?;
                let object_id =
                    kubo_rpc.publish_root_fixture(&request.root_id, &request.object_id, budget)?;
                Ok(ProfileSyncResponse::Root {
                    root_id: request.root_id,
                    object_id: Some(object_id),
                })
            }
            ProfileSyncRequest::ResolveRoot(request) => {
                validate_profile(&request.profile)?;
                validate_profile_sync_root_id(&request.root_id)?;
                self.require_role(self.roles.discovery, "profile-sync/provider-discovery")?;
                let object_id = kubo_rpc.resolve_root_fixture(&request.root_id, budget)?;
                Ok(ProfileSyncResponse::Root {
                    root_id: request.root_id,
                    object_id: Some(object_id),
                })
            }
            ProfileSyncRequest::ListRootCandidates(request) => {
                validate_profile(&request.profile)?;
                validate_profile_sync_root_id(&request.root_id)?;
                self.require_role(self.roles.discovery, "profile-sync/provider-discovery")?;
                Err(BroadwebdError::UnsupportedRequest(
                    "Kubo profile-sync fixture backend does not list root candidates yet"
                        .to_string(),
                ))
            }
            ProfileSyncRequest::DiscoverProviders(request) => {
                validate_profile(&request.profile)?;
                self.require_role(self.roles.discovery, "profile-sync/provider-discovery")?;
                Ok(ProfileSyncResponse::Providers {
                    providers: vec![ProfileSyncProviderRecord {
                        provider_id: self.provider_id.clone(),
                        provider_kind: self.provider_kind.clone(),
                        privacy_boundary: self.privacy_boundary.clone(),
                        retained_objects: 0,
                        roles: self.roles,
                        can_publish_roots: self.roles.mutable_roots,
                    }],
                })
            }
            ProfileSyncRequest::ProviderHealth(request) => {
                validate_profile(&request.profile)?;
                self.require_role(self.roles.discovery, "profile-sync/provider-discovery")?;
                Ok(ProfileSyncResponse::ProviderHealth {
                    health: ProfileSyncProviderHealth {
                        profile: request.profile,
                        known_providers: 1,
                        online_providers: 1,
                        offline_providers: 0,
                        fresh_online_providers: 1,
                        stale_online_providers: 0,
                        fresh_online_provider_ids: vec![self.provider_id.clone()],
                        stale_online_provider_ids: Vec::new(),
                        offline_provider_ids: Vec::new(),
                        minimum_provider_seen_sequence: 0,
                        object_transfer_providers: usize::from(self.roles.object_transfer),
                        availability_providers: usize::from(self.roles.availability),
                        mutable_root_providers: usize::from(self.roles.mutable_roots),
                        retained_objects: 0,
                        degraded: false,
                        message: "Kubo profile-sync fixture provider is ready".to_string(),
                    },
                })
            }
            ProfileSyncRequest::RootHealth(request) => {
                validate_profile(&request.profile)?;
                validate_profile_sync_root_id(&request.root_id)?;
                self.require_role(self.roles.discovery, "profile-sync/provider-discovery")?;
                Err(BroadwebdError::UnsupportedRequest(
                    "Kubo profile-sync fixture backend does not inspect root health yet"
                        .to_string(),
                ))
            }
        }
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

    pub fn service_for_provider_with_roles(
        &self,
        provider_id: impl Into<String>,
        provider_kind: impl Into<String>,
        roles: ProfileSyncProviderRoles,
    ) -> ProfileSyncService {
        ProfileSyncService::local_fixture_provider(
            self.store.clone(),
            provider_id,
            provider_kind,
            roles,
        )
    }

    pub fn expire_current_provider_freshness(&self) -> Result<u64, BroadwebdError> {
        let mut store = self
            .store
            .lock()
            .map_err(|_| BroadwebdError::Request("profile sync store lock poisoned".to_string()))?;
        store.minimum_provider_seen_sequence = store.next_provider_seen_sequence.saturating_add(1);
        Ok(store.minimum_provider_seen_sequence)
    }

    pub fn mark_device_seen(&self, device_id: impl AsRef<str>) -> Result<u64, BroadwebdError> {
        self.mark_provider_seen(local_fixture_provider_id(device_id))
    }

    pub fn mark_availability_provider_seen(
        &self,
        provider_id: impl AsRef<str>,
    ) -> Result<u64, BroadwebdError> {
        self.mark_provider_seen(local_fixture_availability_provider_id(provider_id))
    }

    pub fn mark_provider_seen(&self, provider_id: impl AsRef<str>) -> Result<u64, BroadwebdError> {
        let provider_id = provider_id.as_ref();
        let mut store = self
            .store
            .lock()
            .map_err(|_| BroadwebdError::Request("profile sync store lock poisoned".to_string()))?;
        if !store.providers.contains_key(provider_id) {
            return Err(BroadwebdError::UnsupportedRequest(format!(
                "unknown profile sync provider: {provider_id}"
            )));
        }
        let last_seen_sequence = next_provider_seen_sequence(&mut store);
        store
            .providers
            .get_mut(provider_id)
            .expect("provider existence was checked before marking freshness")
            .last_seen_sequence = last_seen_sequence;
        Ok(last_seen_sequence)
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

    pub fn set_device_retention_available(
        &self,
        device_id: impl AsRef<str>,
        available: bool,
    ) -> Result<(), BroadwebdError> {
        self.set_provider_retention_available(local_fixture_provider_id(device_id), available)
    }

    pub fn set_availability_provider_retention_available(
        &self,
        provider_id: impl AsRef<str>,
        available: bool,
    ) -> Result<(), BroadwebdError> {
        self.set_provider_retention_available(
            local_fixture_availability_provider_id(provider_id),
            available,
        )
    }

    pub fn set_provider_retention_available(
        &self,
        provider_id: impl AsRef<str>,
        available: bool,
    ) -> Result<(), BroadwebdError> {
        let provider_id = provider_id.as_ref();
        let mut store = self
            .store
            .lock()
            .map_err(|_| BroadwebdError::Request("profile sync store lock poisoned".to_string()))?;
        if !store.providers.contains_key(provider_id) {
            return Err(BroadwebdError::UnsupportedRequest(format!(
                "unknown profile sync provider: {provider_id}"
            )));
        }
        if available {
            store.retention_blocked_providers.remove(provider_id);
        } else {
            store
                .retention_blocked_providers
                .insert(provider_id.to_string());
        }
        Ok(())
    }

    pub fn set_device_retention_quota(
        &self,
        device_id: impl AsRef<str>,
        max_retained_objects: Option<usize>,
    ) -> Result<(), BroadwebdError> {
        self.set_provider_retention_quota(
            local_fixture_provider_id(device_id),
            max_retained_objects,
        )
    }

    pub fn set_availability_provider_retention_quota(
        &self,
        provider_id: impl AsRef<str>,
        max_retained_objects: Option<usize>,
    ) -> Result<(), BroadwebdError> {
        self.set_provider_retention_quota(
            local_fixture_availability_provider_id(provider_id),
            max_retained_objects,
        )
    }

    pub fn set_provider_retention_quota(
        &self,
        provider_id: impl AsRef<str>,
        max_retained_objects: Option<usize>,
    ) -> Result<(), BroadwebdError> {
        let provider_id = provider_id.as_ref();
        let mut store = self
            .store
            .lock()
            .map_err(|_| BroadwebdError::Request("profile sync store lock poisoned".to_string()))?;
        if !store.providers.contains_key(provider_id) {
            return Err(BroadwebdError::UnsupportedRequest(format!(
                "unknown profile sync provider: {provider_id}"
            )));
        }
        if let Some(max_retained_objects) = max_retained_objects {
            store
                .retention_quota_by_provider
                .insert(provider_id.to_string(), max_retained_objects);
        } else {
            store.retention_quota_by_provider.remove(provider_id);
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
        let root_id = root_id.as_ref();
        validate_profile(profile)?;
        validate_profile_sync_root_id(root_id)?;
        let source_provider_id = local_fixture_provider_id(source_device_id);
        let target_provider_id = local_fixture_provider_id(target_device_id);
        let link = (
            source_provider_id,
            target_provider_id,
            profile.to_string(),
            root_id.to_string(),
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
        #[cfg(any(test, feature = "test-fixtures"))]
        if self.kubo_rpc.is_some() {
            let capabilities = [
                "profile-sync/kubo-fixture",
                "profile-sync/object-transfer",
                "profile-sync/local-retention",
                "profile-sync/mutable-root",
                "profile-sync/provider-discovery",
                "socketless-fixture",
            ];
            return PluginMetadata::new(PROFILE_SYNC_PLUGIN, PluginKind::ApplicationService)
                .with_capabilities(&capabilities)
                .with_privacy_boundary(self.privacy_boundary.as_str())
                .with_resource_profile(ResourceProfile::Low);
        }

        let mut capabilities = vec!["profile-sync/fake"];
        if self.roles.discovery {
            capabilities.push("profile-sync/provider-discovery");
        }
        if self.roles.connectivity {
            capabilities.push("profile-sync/local-connectivity");
        }
        if self.roles.object_transfer {
            capabilities.push("profile-sync/object-transfer");
        }
        if self.roles.availability {
            capabilities.push("profile-sync/local-retention");
        }
        if self.roles.mutable_roots {
            capabilities.push("profile-sync/mutable-root");
        } else if self.roles.availability {
            capabilities.push("profile-sync/availability-provider");
        }
        PluginMetadata::new(PROFILE_SYNC_PLUGIN, PluginKind::ApplicationService)
            .with_capabilities(capabilities.as_slice())
            .with_privacy_boundary(
                "local in-memory fake profile-sync backend for tests; no sockets or external network",
            )
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

        #[cfg(any(test, feature = "test-fixtures"))]
        if let Some(kubo_rpc) = &self.kubo_rpc {
            return Ok(ServiceResponse::ProfileSync(
                self.kubo_fixture_profile_sync(kubo_rpc, request, budget)?,
            ));
        }

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
            ProfileSyncRequest::ListRootCandidates(request) => {
                self.list_root_candidates(request)?
            }
            ProfileSyncRequest::DiscoverProviders(request) => self.discover_providers(request)?,
            ProfileSyncRequest::ProviderHealth(request) => self.provider_health(request)?,
            ProfileSyncRequest::RootHealth(request) => self.root_health(request)?,
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

fn validate_profile_sync_root_id(root_id: &str) -> Result<(), BroadwebdError> {
    let is_valid = !root_id.is_empty()
        && root_id.len() <= MAX_PROFILE_SYNC_ROOT_ID_BYTES
        && root_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'/'))
        && root_id.split('/').all(|segment| {
            !segment.is_empty() && segment != "." && segment != ".." && !segment.starts_with('.')
        });
    if is_valid {
        Ok(())
    } else {
        Err(BroadwebdError::UnsupportedRequest(
            "invalid profile sync root id".to_string(),
        ))
    }
}

fn validate_profile_sync_object_id(object_id: &str) -> Result<(), BroadwebdError> {
    let is_valid = !object_id.is_empty()
        && object_id.len() <= MAX_PROFILE_SYNC_OBJECT_ID_BYTES
        && object_id.bytes().all(|byte| {
            byte.is_ascii_graphic() && !matches!(byte, b'\\' | b'\'' | b'"' | b'<' | b'>' | b'`')
        });
    if is_valid {
        Ok(())
    } else {
        Err(BroadwebdError::UnsupportedRequest(
            "invalid profile sync object id".to_string(),
        ))
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

#[derive(Clone, Copy)]
struct ProfileSyncProviderHealthEntry<'a> {
    provider_id: &'a str,
    roles: ProfileSyncProviderRoles,
    online: bool,
    fresh: bool,
}

fn provider_health_entries<'a>(
    store: &'a ProfileSyncStore,
    default_provider_id: &'a str,
    default_roles: ProfileSyncProviderRoles,
) -> Vec<ProfileSyncProviderHealthEntry<'a>> {
    if store.providers.is_empty() {
        return vec![ProfileSyncProviderHealthEntry {
            provider_id: default_provider_id,
            roles: default_roles,
            online: default_roles.connectivity,
            fresh: provider_is_fresh(store, 0),
        }];
    }

    store
        .providers
        .iter()
        .map(|(provider_id, state)| ProfileSyncProviderHealthEntry {
            provider_id: provider_id.as_str(),
            roles: state.roles,
            online: provider_is_online(store, provider_id, state.roles),
            fresh: provider_is_fresh(store, state.last_seen_sequence),
        })
        .collect()
}

fn profile_sync_health_message(
    online_providers: usize,
    fresh_online_providers: usize,
    object_transfer_providers: usize,
    availability_providers: usize,
    mutable_root_providers: usize,
) -> (bool, String) {
    if online_providers == 0 {
        (
            true,
            "profile sync has no online providers in the local fixture".to_string(),
        )
    } else if fresh_online_providers == 0 {
        (
            true,
            "profile sync has no fresh online providers in the local fixture".to_string(),
        )
    } else if object_transfer_providers == 0 {
        (
            true,
            "profile sync has no fresh online object-transfer provider in the local fixture"
                .to_string(),
        )
    } else if availability_providers == 0 {
        (
            true,
            "profile sync has no fresh online availability provider in the local fixture"
                .to_string(),
        )
    } else if mutable_root_providers == 0 {
        (
            true,
            "profile sync has no fresh online mutable-root provider in the local fixture"
                .to_string(),
        )
    } else {
        (
            false,
            "profile sync providers are ready in the local fixture".to_string(),
        )
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

fn retained_provider_object_count(store: &ProfileSyncStore, provider_id: &str) -> usize {
    store
        .retained
        .iter()
        .filter(|(retained_provider_id, _, _)| retained_provider_id == provider_id)
        .count()
}

fn online_retaining_provider_count(
    store: &ProfileSyncStore,
    profile: &str,
    object_id: &str,
) -> usize {
    store
        .retained
        .iter()
        .filter(|(provider_id, retained_profile, retained_object_id)| {
            retained_profile == profile
                && retained_object_id == object_id
                && provider_is_fresh_online_for_role(store, provider_id, |roles| {
                    roles.availability && roles.object_transfer
                })
        })
        .count()
}

fn profile_sync_root_health_message(
    visible_candidates: usize,
    delayed_candidates: usize,
    latest_object_available: bool,
    delayed_object_providers: usize,
    online_retaining_providers: usize,
    minimum_online_retaining_providers: usize,
) -> (bool, String) {
    if visible_candidates == 0 {
        if delayed_candidates > 0 {
            return (
                true,
                format!(
                    "profile sync root has no visible candidates in the local fixture; {delayed_candidates} candidate(s) are delayed"
                ),
            );
        }
        (
            true,
            "profile sync root has no visible candidates in the local fixture".to_string(),
        )
    } else if !latest_object_available {
        if delayed_object_providers > 0 {
            return (
                true,
                format!(
                    "profile sync root object is blocked by {delayed_object_providers} delayed object-transfer provider(s) in the local fixture"
                ),
            );
        }
        (
            true,
            "profile sync root object is not available from a fresh online provider in the local fixture"
                .to_string(),
        )
    } else if online_retaining_providers == 0 {
        (
            true,
            "profile sync root object is not retained by a fresh online provider in the local fixture"
                .to_string(),
        )
    } else if online_retaining_providers < minimum_online_retaining_providers {
        (
            true,
            format!(
                "profile sync root has {online_retaining_providers} fresh online retaining providers, below the requested quorum of {minimum_online_retaining_providers}"
            ),
        )
    } else {
        (
            false,
            "profile sync root is available and retained by fresh online providers in the local fixture"
                .to_string(),
        )
    }
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
                && provider_is_fresh_online_for_role(store, provider_id, |roles| {
                    roles.object_transfer
                })
                && transfer_available(store, provider_id, requester_provider_id)
        })
        .map(|(_, bytes)| bytes)
}

fn delayed_object_provider_ids(
    store: &ProfileSyncStore,
    requester_provider_id: &str,
    profile: &str,
    object_id: &str,
) -> Vec<String> {
    store
        .objects
        .keys()
        .filter_map(|(provider_id, stored_profile, stored_object_id)| {
            if stored_profile == profile
                && stored_object_id == object_id
                && provider_is_fresh_online_for_role(store, provider_id, |roles| {
                    roles.object_transfer
                })
                && !transfer_available(store, provider_id, requester_provider_id)
            {
                Some(provider_id.clone())
            } else {
                None
            }
        })
        .collect()
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

fn latest_visible_root_candidate(
    store: &ProfileSyncStore,
    requester_provider_id: &str,
    profile: &str,
    root_id: &str,
) -> Option<ProfileSyncRootCandidate> {
    visible_root_candidates(store, requester_provider_id, profile, root_id)
        .into_iter()
        .next()
}

fn visible_root_candidates(
    store: &ProfileSyncStore,
    requester_provider_id: &str,
    profile: &str,
    root_id: &str,
) -> Vec<ProfileSyncRootCandidate> {
    let mut candidates = store
        .roots
        .iter()
        .filter_map(
            |((stored_profile, stored_root_id, publisher_provider_id), root)| {
                if stored_profile != profile
                    || stored_root_id != root_id
                    || !provider_has_role(store, publisher_provider_id, |roles| roles.mutable_roots)
                    || !root_available(
                        store,
                        publisher_provider_id,
                        requester_provider_id,
                        profile,
                        root_id,
                    )
                {
                    return None;
                }
                Some(ProfileSyncRootCandidate {
                    publisher_provider_id: root.publisher_provider_id.clone(),
                    object_id: root.object_id.clone(),
                    publish_sequence: root.publish_sequence,
                })
            },
        )
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        right
            .publish_sequence
            .cmp(&left.publish_sequence)
            .then_with(|| left.publisher_provider_id.cmp(&right.publisher_provider_id))
            .then_with(|| left.object_id.cmp(&right.object_id))
    });
    candidates
}

fn delayed_root_candidates(
    store: &ProfileSyncStore,
    requester_provider_id: &str,
    profile: &str,
    root_id: &str,
) -> Vec<ProfileSyncRootCandidate> {
    let mut candidates = store
        .roots
        .iter()
        .filter_map(
            |((stored_profile, stored_root_id, publisher_provider_id), root)| {
                if stored_profile != profile
                    || stored_root_id != root_id
                    || !provider_has_role(store, publisher_provider_id, |roles| roles.mutable_roots)
                    || root_available(
                        store,
                        publisher_provider_id,
                        requester_provider_id,
                        profile,
                        root_id,
                    )
                {
                    return None;
                }
                Some(ProfileSyncRootCandidate {
                    publisher_provider_id: root.publisher_provider_id.clone(),
                    object_id: root.object_id.clone(),
                    publish_sequence: root.publish_sequence,
                })
            },
        )
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        right
            .publish_sequence
            .cmp(&left.publish_sequence)
            .then_with(|| left.publisher_provider_id.cmp(&right.publisher_provider_id))
            .then_with(|| left.object_id.cmp(&right.object_id))
    });
    candidates
}

fn next_provider_seen_sequence(store: &mut ProfileSyncStore) -> u64 {
    store.next_provider_seen_sequence = store.next_provider_seen_sequence.saturating_add(1);
    store.next_provider_seen_sequence
}

fn provider_is_online(
    store: &ProfileSyncStore,
    provider_id: &str,
    roles: ProfileSyncProviderRoles,
) -> bool {
    roles.connectivity && !store.offline_providers.contains(provider_id)
}

fn provider_is_fresh(store: &ProfileSyncStore, last_seen_sequence: u64) -> bool {
    last_seen_sequence >= store.minimum_provider_seen_sequence
}

fn provider_is_fresh_online_for_role(
    store: &ProfileSyncStore,
    provider_id: &str,
    role: impl FnOnce(ProfileSyncProviderRoles) -> bool,
) -> bool {
    store.providers.get(provider_id).map_or(true, |state| {
        provider_is_online(store, provider_id, state.roles)
            && provider_is_fresh(store, state.last_seen_sequence)
            && role(state.roles)
    })
}

fn provider_has_role(
    store: &ProfileSyncStore,
    provider_id: &str,
    role: impl FnOnce(ProfileSyncProviderRoles) -> bool,
) -> bool {
    store
        .providers
        .get(provider_id)
        .map_or(true, |state| role(state.roles))
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
        ProfileSyncProviderRoles, ProfileSyncPutObjectRequest, ProfileSyncRequest,
        ProfileSyncResponse, ProfileSyncRootHealthRequest, ProfileSyncRootRequest,
        ProfileSyncRootUpdate, ResourceBudget,
    };

    #[test]
    fn local_object_ids_are_deterministic_for_test_backend() {
        assert_eq!(local_object_id(b"settings"), local_object_id(b"settings"));
        assert_ne!(local_object_id(b"settings"), local_object_id(b"calendar"));
    }

    #[test]
    fn local_fixture_rejects_invalid_profile_sync_identifiers() {
        let fixture = LocalProfileSyncFixture::new();
        let mut device = PluginRegistry::new();
        let budget = ResourceBudget::default();

        device.register_service(fixture.service_for_device("a"));

        let empty_root = device
            .profile_sync(
                ProfileSyncRequest::ResolveRoot(ProfileSyncRootRequest::new("default", "")),
                &budget,
            )
            .expect_err("empty root id should fail before backend lookup");
        assert!(matches!(
            empty_root,
            BroadwebdError::UnsupportedRequest(message)
                if message.contains("invalid profile sync root id")
        ));

        let escaped_root = device
            .profile_sync(
                ProfileSyncRequest::ListRootCandidates(ProfileSyncRootRequest::new(
                    "default",
                    "settings/../latest",
                )),
                &budget,
            )
            .expect_err("path-like root escape should fail before backend lookup");
        assert!(matches!(
            escaped_root,
            BroadwebdError::UnsupportedRequest(message)
                if message.contains("invalid profile sync root id")
        ));

        let empty_object = device
            .profile_sync(
                ProfileSyncRequest::GetEncryptedObject(ProfileSyncObjectRequest::new(
                    "default", "",
                )),
                &budget,
            )
            .expect_err("empty object id should fail before backend lookup");
        assert!(matches!(
            empty_object,
            BroadwebdError::UnsupportedRequest(message)
                if message.contains("invalid profile sync object id")
        ));

        let malformed_object = device
            .profile_sync(
                ProfileSyncRequest::PublishRoot(ProfileSyncRootUpdate::new(
                    "default",
                    "settings/latest",
                    "not valid",
                )),
                &budget,
            )
            .expect_err("malformed object id should fail before publish lookup");
        assert!(matches!(
            malformed_object,
            BroadwebdError::UnsupportedRequest(message)
                if message.contains("invalid profile sync object id")
        ));
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
    fn local_fixture_lists_competing_mutable_root_candidates() {
        let fixture = LocalProfileSyncFixture::new();
        let mut device_a = PluginRegistry::new();
        let mut device_b = PluginRegistry::new();
        let mut device_c = PluginRegistry::new();
        let budget = ResourceBudget::default();

        device_a.register_service(fixture.service_for_device("a"));
        device_b.register_service(fixture.service_for_device("b"));
        device_c.register_service(fixture.service_for_device("c"));

        let put_a = device_a
            .profile_sync(
                ProfileSyncRequest::PutEncryptedObject(ProfileSyncPutObjectRequest::new(
                    "default",
                    b"encrypted root candidate from device a".to_vec(),
                )),
                &budget,
            )
            .expect("device a can put root candidate into fixture");
        let ProfileSyncResponse::PutEncryptedObject {
            object_id: object_a,
        } = put_a
        else {
            panic!("unexpected put response");
        };
        device_a
            .profile_sync(
                ProfileSyncRequest::PublishRoot(ProfileSyncRootUpdate::new(
                    "default",
                    "profile-root",
                    object_a.clone(),
                )),
                &budget,
            )
            .expect("device a can publish root candidate");

        let put_b = device_b
            .profile_sync(
                ProfileSyncRequest::PutEncryptedObject(ProfileSyncPutObjectRequest::new(
                    "default",
                    b"encrypted root candidate from device b".to_vec(),
                )),
                &budget,
            )
            .expect("device b can put competing root candidate into fixture");
        let ProfileSyncResponse::PutEncryptedObject {
            object_id: object_b,
        } = put_b
        else {
            panic!("unexpected put response");
        };
        device_b
            .profile_sync(
                ProfileSyncRequest::PublishRoot(ProfileSyncRootUpdate::new(
                    "default",
                    "profile-root",
                    object_b.clone(),
                )),
                &budget,
            )
            .expect("device b can publish competing root candidate");

        let candidates = device_c
            .profile_sync(
                ProfileSyncRequest::ListRootCandidates(ProfileSyncRootRequest::new(
                    "default",
                    "profile-root",
                )),
                &budget,
            )
            .expect("device c can list visible root candidates");
        let ProfileSyncResponse::RootCandidates {
            root_id,
            candidates,
        } = candidates
        else {
            panic!("unexpected candidates response");
        };
        assert_eq!(root_id, "profile-root");
        assert_eq!(candidates.len(), 2);
        assert_eq!(
            candidates[0].publisher_provider_id,
            "local-fixture-device-b"
        );
        assert_eq!(candidates[0].object_id, object_b);
        assert_eq!(candidates[0].publish_sequence, 2);
        assert_eq!(
            candidates[1].publisher_provider_id,
            "local-fixture-device-a"
        );
        assert_eq!(candidates[1].object_id, object_a);
        assert_eq!(candidates[1].publish_sequence, 1);

        let resolved = device_c
            .profile_sync(
                ProfileSyncRequest::ResolveRoot(ProfileSyncRootRequest::new(
                    "default",
                    "profile-root",
                )),
                &budget,
            )
            .expect("device c resolves latest visible candidate");
        assert_eq!(
            resolved,
            ProfileSyncResponse::Root {
                root_id: "profile-root".to_string(),
                object_id: Some(candidates[0].object_id.clone()),
            }
        );
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
        let delayed_health = device_b
            .profile_sync(
                ProfileSyncRequest::RootHealth(ProfileSyncRootHealthRequest::new(
                    "default",
                    "profile-root",
                )),
                &budget,
            )
            .expect("device b can inspect delayed fixture root health");
        let ProfileSyncResponse::RootHealth { health } = delayed_health else {
            panic!("unexpected root health response");
        };
        assert_eq!(health.visible_candidates, 0);
        assert_eq!(health.delayed_candidates, 1);
        assert_eq!(
            health.delayed_publisher_provider_ids,
            vec!["local-fixture-device-a".to_string()]
        );
        assert!(health.degraded);
        assert!(health.message.contains("delayed"));
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
        let released_health = device_b
            .profile_sync(
                ProfileSyncRequest::RootHealth(ProfileSyncRootHealthRequest::new(
                    "default",
                    "profile-root",
                )),
                &budget,
            )
            .expect("device b can inspect released fixture root health");
        let ProfileSyncResponse::RootHealth { health } = released_health else {
            panic!("unexpected root health response");
        };
        assert_eq!(health.visible_candidates, 1);
        assert_eq!(health.delayed_candidates, 0);
        assert!(health.delayed_publisher_provider_ids.is_empty());
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
                && provider.roles == ProfileSyncProviderRoles::logged_in_device()
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
    fn local_fixture_enforces_provider_roles() {
        let fixture = LocalProfileSyncFixture::new();
        let mut device_a = PluginRegistry::new();
        let mut no_transfer = PluginRegistry::new();
        let mut no_availability = PluginRegistry::new();
        let mut no_discovery = PluginRegistry::new();
        let mut no_connectivity = PluginRegistry::new();
        let budget = ResourceBudget::default();

        device_a.register_service(fixture.service_for_device("a"));
        no_transfer.register_service(fixture.service_for_provider_with_roles(
            "local-fixture-no-transfer",
            "local-fixture-custom",
            ProfileSyncProviderRoles {
                object_transfer: false,
                mutable_roots: false,
                ..ProfileSyncProviderRoles::logged_in_device()
            },
        ));
        no_availability.register_service(fixture.service_for_provider_with_roles(
            "local-fixture-no-availability",
            "local-fixture-custom",
            ProfileSyncProviderRoles {
                availability: false,
                mutable_roots: false,
                ..ProfileSyncProviderRoles::logged_in_device()
            },
        ));
        no_discovery.register_service(fixture.service_for_provider_with_roles(
            "local-fixture-no-discovery",
            "local-fixture-custom",
            ProfileSyncProviderRoles {
                discovery: false,
                mutable_roots: false,
                ..ProfileSyncProviderRoles::logged_in_device()
            },
        ));
        no_connectivity.register_service(fixture.service_for_provider_with_roles(
            "local-fixture-no-connectivity",
            "local-fixture-custom",
            ProfileSyncProviderRoles {
                connectivity: false,
                mutable_roots: false,
                ..ProfileSyncProviderRoles::logged_in_device()
            },
        ));

        let put = device_a
            .profile_sync(
                ProfileSyncRequest::PutEncryptedObject(ProfileSyncPutObjectRequest::new(
                    "default",
                    b"encrypted role policy object".to_vec(),
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
                    "settings/latest",
                    object_id.clone(),
                )),
                &budget,
            )
            .expect("device a can publish fixture root");

        let get_without_transfer = no_transfer
            .profile_sync(
                ProfileSyncRequest::GetEncryptedObject(ProfileSyncObjectRequest::new(
                    "default",
                    object_id.clone(),
                )),
                &budget,
            )
            .expect_err("provider without transfer must not fetch objects");
        assert!(matches!(
            get_without_transfer,
            BroadwebdError::UnsupportedRequest(message)
                if message.contains("profile-sync/object-transfer")
                    && message.contains("local-fixture-no-transfer")
        ));

        let retain_without_transfer = no_transfer
            .profile_sync(
                ProfileSyncRequest::RetainObject(ProfileSyncObjectRequest::new(
                    "default",
                    object_id.clone(),
                )),
                &budget,
            )
            .expect_err("provider without transfer must not retain remote objects");
        assert!(matches!(
            retain_without_transfer,
            BroadwebdError::UnsupportedRequest(message)
                if message.contains("profile-sync/object-transfer")
                    && message.contains("local-fixture-no-transfer")
        ));

        let retain_without_availability = no_availability
            .profile_sync(
                ProfileSyncRequest::RetainObject(ProfileSyncObjectRequest::new(
                    "default",
                    object_id.clone(),
                )),
                &budget,
            )
            .expect_err("provider without availability must not retain objects");
        assert!(matches!(
            retain_without_availability,
            BroadwebdError::UnsupportedRequest(message)
                if message.contains("profile-sync/availability")
                    && message.contains("local-fixture-no-availability")
        ));

        let list_without_availability = no_availability
            .profile_sync(
                ProfileSyncRequest::ListRetainedObjects(ProfileSyncProfileRequest::new("default")),
                &budget,
            )
            .expect_err("provider without availability must not list retained objects");
        assert!(matches!(
            list_without_availability,
            BroadwebdError::UnsupportedRequest(message)
                if message.contains("profile-sync/availability")
                    && message.contains("local-fixture-no-availability")
        ));

        let discover_without_discovery = no_discovery
            .profile_sync(
                ProfileSyncRequest::DiscoverProviders(ProfileSyncProfileRequest::new("default")),
                &budget,
            )
            .expect_err("provider without discovery must not discover providers");
        assert!(matches!(
            discover_without_discovery,
            BroadwebdError::UnsupportedRequest(message)
                if message.contains("profile-sync/provider-discovery")
                    && message.contains("local-fixture-no-discovery")
        ));

        let resolve_without_discovery = no_discovery
            .profile_sync(
                ProfileSyncRequest::ResolveRoot(ProfileSyncRootRequest::new(
                    "default",
                    "settings/latest",
                )),
                &budget,
            )
            .expect_err("provider without discovery must not resolve roots");
        assert!(matches!(
            resolve_without_discovery,
            BroadwebdError::UnsupportedRequest(message)
                if message.contains("profile-sync/provider-discovery")
                    && message.contains("local-fixture-no-discovery")
        ));

        let discover_without_connectivity = no_connectivity
            .profile_sync(
                ProfileSyncRequest::DiscoverProviders(ProfileSyncProfileRequest::new("default")),
                &budget,
            )
            .expect_err("provider without connectivity must not answer service calls");
        assert!(matches!(
            discover_without_connectivity,
            BroadwebdError::UnsupportedRequest(message)
                if message.contains("profile-sync/local-connectivity")
                    && message.contains("local-fixture-no-connectivity")
        ));

        let mut custom_source = PluginRegistry::new();
        let mut requester = PluginRegistry::new();
        custom_source.register_service(fixture.service_for_provider_with_roles(
            "local-fixture-source-role",
            "local-fixture-custom",
            ProfileSyncProviderRoles {
                mutable_roots: false,
                ..ProfileSyncProviderRoles::logged_in_device()
            },
        ));
        requester.register_service(fixture.service_for_device("requester"));
        let put_from_source = custom_source
            .profile_sync(
                ProfileSyncRequest::PutEncryptedObject(ProfileSyncPutObjectRequest::new(
                    "default",
                    b"source object that later loses transfer".to_vec(),
                )),
                &budget,
            )
            .expect("source can put before losing transfer role");
        let ProfileSyncResponse::PutEncryptedObject {
            object_id: source_object_id,
        } = put_from_source
        else {
            panic!("unexpected put response");
        };
        let mut source_without_transfer = PluginRegistry::new();
        source_without_transfer.register_service(fixture.service_for_provider_with_roles(
            "local-fixture-source-role",
            "local-fixture-custom",
            ProfileSyncProviderRoles {
                object_transfer: false,
                mutable_roots: false,
                ..ProfileSyncProviderRoles::logged_in_device()
            },
        ));
        let unavailable_from_source_without_transfer = requester
            .profile_sync(
                ProfileSyncRequest::GetEncryptedObject(ProfileSyncObjectRequest::new(
                    "default",
                    source_object_id,
                )),
                &budget,
            )
            .expect_err("objects held by a no-transfer provider should be unavailable");
        assert!(matches!(
            unavailable_from_source_without_transfer,
            BroadwebdError::UnsupportedRequest(message)
                if message.contains("not available")
        ));
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
                if message.contains("profile-sync/mutable-root")
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
        assert_eq!(
            provider.roles,
            ProfileSyncProviderRoles::availability_provider()
        );
        assert!(!provider.can_publish_roots);
        assert_eq!(provider.can_publish_roots, provider.roles.mutable_roots);

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
    fn local_fixture_reports_degraded_provider_health() {
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
                    b"encrypted health object".to_vec(),
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
                    "default", object_id,
                )),
                &budget,
            )
            .expect("availability provider can retain health object");

        let healthy = device_a
            .profile_sync(
                ProfileSyncRequest::ProviderHealth(ProfileSyncProfileRequest::new("default")),
                &budget,
            )
            .expect("device a can inspect provider health");
        let ProfileSyncResponse::ProviderHealth { health } = healthy else {
            panic!("unexpected health response");
        };
        assert_eq!(health.profile, "default");
        assert_eq!(health.known_providers, 2);
        assert_eq!(health.online_providers, 2);
        assert_eq!(health.offline_providers, 0);
        assert_eq!(health.fresh_online_providers, 2);
        assert_eq!(health.stale_online_providers, 0);
        assert_eq!(
            health.fresh_online_provider_ids,
            vec![
                "local-fixture-availability-pin-1".to_string(),
                "local-fixture-device-a".to_string()
            ]
        );
        assert!(health.stale_online_provider_ids.is_empty());
        assert!(health.offline_provider_ids.is_empty());
        assert_eq!(health.minimum_provider_seen_sequence, 0);
        assert_eq!(health.object_transfer_providers, 2);
        assert_eq!(health.availability_providers, 2);
        assert_eq!(health.mutable_root_providers, 1);
        assert_eq!(health.retained_objects, 1);
        assert!(!health.degraded);

        fixture
            .set_device_online("a", false)
            .expect("mark only mutable-root provider offline");

        let degraded = availability_provider
            .profile_sync(
                ProfileSyncRequest::ProviderHealth(ProfileSyncProfileRequest::new("default")),
                &budget,
            )
            .expect("availability provider can inspect degraded health");
        let ProfileSyncResponse::ProviderHealth { health } = degraded else {
            panic!("unexpected health response");
        };
        assert_eq!(health.known_providers, 2);
        assert_eq!(health.online_providers, 1);
        assert_eq!(health.offline_providers, 1);
        assert_eq!(health.fresh_online_providers, 1);
        assert_eq!(health.stale_online_providers, 0);
        assert_eq!(
            health.fresh_online_provider_ids,
            vec!["local-fixture-availability-pin-1".to_string()]
        );
        assert!(health.stale_online_provider_ids.is_empty());
        assert_eq!(
            health.offline_provider_ids,
            vec!["local-fixture-device-a".to_string()]
        );
        assert_eq!(health.minimum_provider_seen_sequence, 0);
        assert_eq!(health.object_transfer_providers, 1);
        assert_eq!(health.availability_providers, 1);
        assert_eq!(health.mutable_root_providers, 0);
        assert_eq!(health.retained_objects, 1);
        assert!(health.degraded);
        assert!(health.message.contains("mutable-root provider"));
    }

    #[test]
    fn local_fixture_reports_stale_provider_health() {
        let fixture = LocalProfileSyncFixture::new();
        let mut device_a = PluginRegistry::new();
        let mut device_b = PluginRegistry::new();
        let budget = ResourceBudget::default();

        device_a.register_service(fixture.service_for_device("a"));
        device_b.register_service(fixture.service_for_device("b"));

        let minimum_seen = fixture
            .expire_current_provider_freshness()
            .expect("expire current provider freshness");
        let stale = device_b
            .profile_sync(
                ProfileSyncRequest::ProviderHealth(ProfileSyncProfileRequest::new("default")),
                &budget,
            )
            .expect("device b can inspect stale provider health");
        let ProfileSyncResponse::ProviderHealth { health } = stale else {
            panic!("unexpected health response");
        };
        assert_eq!(health.known_providers, 2);
        assert_eq!(health.online_providers, 2);
        assert_eq!(health.offline_providers, 0);
        assert_eq!(health.fresh_online_providers, 0);
        assert_eq!(health.stale_online_providers, 2);
        assert!(health.fresh_online_provider_ids.is_empty());
        assert_eq!(
            health.stale_online_provider_ids,
            vec![
                "local-fixture-device-a".to_string(),
                "local-fixture-device-b".to_string()
            ]
        );
        assert!(health.offline_provider_ids.is_empty());
        assert_eq!(health.minimum_provider_seen_sequence, minimum_seen);
        assert_eq!(health.object_transfer_providers, 0);
        assert_eq!(health.availability_providers, 0);
        assert_eq!(health.mutable_root_providers, 0);
        assert!(health.degraded);
        assert!(health.message.contains("fresh online providers"));

        fixture.mark_device_seen("b").expect("mark device b fresh");
        let recovering = device_b
            .profile_sync(
                ProfileSyncRequest::ProviderHealth(ProfileSyncProfileRequest::new("default")),
                &budget,
            )
            .expect("device b can inspect recovering provider health");
        let ProfileSyncResponse::ProviderHealth { health } = recovering else {
            panic!("unexpected health response");
        };
        assert_eq!(health.known_providers, 2);
        assert_eq!(health.online_providers, 2);
        assert_eq!(health.offline_providers, 0);
        assert_eq!(health.fresh_online_providers, 1);
        assert_eq!(health.stale_online_providers, 1);
        assert_eq!(
            health.fresh_online_provider_ids,
            vec!["local-fixture-device-b".to_string()]
        );
        assert_eq!(
            health.stale_online_provider_ids,
            vec!["local-fixture-device-a".to_string()]
        );
        assert!(health.offline_provider_ids.is_empty());
        assert_eq!(health.object_transfer_providers, 1);
        assert_eq!(health.availability_providers, 1);
        assert_eq!(health.mutable_root_providers, 1);
        assert!(!health.degraded);
    }

    #[test]
    fn local_fixture_reports_root_health_for_retained_objects() {
        let fixture = LocalProfileSyncFixture::new();
        let mut device_a = PluginRegistry::new();
        let mut availability_provider = PluginRegistry::new();
        let budget = ResourceBudget::default();

        device_a.register_service(fixture.service_for_device("a"));
        availability_provider.register_service(fixture.service_for_availability_provider("pin-1"));

        let missing = device_a
            .profile_sync(
                ProfileSyncRequest::RootHealth(ProfileSyncRootHealthRequest::new(
                    "default",
                    "settings/latest",
                )),
                &budget,
            )
            .expect("missing root health is reported locally");
        let ProfileSyncResponse::RootHealth { health } = missing else {
            panic!("unexpected root health response");
        };
        assert_eq!(health.root_id, "settings/latest");
        assert_eq!(health.visible_candidates, 0);
        assert_eq!(health.latest_object_id, None);
        assert!(!health.latest_object_available);
        assert_eq!(health.online_retaining_providers, 0);
        assert!(health.degraded);
        assert!(health.message.contains("no visible candidates"));

        let put = device_a
            .profile_sync(
                ProfileSyncRequest::PutEncryptedObject(ProfileSyncPutObjectRequest::new(
                    "default",
                    b"encrypted root health object".to_vec(),
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
                    "settings/latest",
                    object_id.clone(),
                )),
                &budget,
            )
            .expect("device a can publish root for health");
        availability_provider
            .profile_sync(
                ProfileSyncRequest::RetainObject(ProfileSyncObjectRequest::new(
                    "default",
                    object_id.clone(),
                )),
                &budget,
            )
            .expect("availability provider can retain root object");

        let healthy = device_a
            .profile_sync(
                ProfileSyncRequest::RootHealth(ProfileSyncRootHealthRequest::new(
                    "default",
                    "settings/latest",
                )),
                &budget,
            )
            .expect("retained root health is reported locally");
        let ProfileSyncResponse::RootHealth { health } = healthy else {
            panic!("unexpected root health response");
        };
        assert_eq!(health.profile, "default");
        assert_eq!(health.root_id, "settings/latest");
        assert_eq!(health.visible_candidates, 1);
        assert_eq!(health.latest_object_id.as_deref(), Some(object_id.as_str()));
        assert!(health.latest_object_available);
        assert_eq!(health.online_retaining_providers, 1);
        assert_eq!(health.minimum_online_retaining_providers, 1);
        assert!(!health.degraded);

        let quorum_degraded = device_a
            .profile_sync(
                ProfileSyncRequest::RootHealth(
                    ProfileSyncRootHealthRequest::with_minimum_online_retaining_providers(
                        "default",
                        "settings/latest",
                        2,
                    ),
                ),
                &budget,
            )
            .expect("retained root health can apply a local quorum policy");
        let ProfileSyncResponse::RootHealth { health } = quorum_degraded else {
            panic!("unexpected root health response");
        };
        assert_eq!(health.online_retaining_providers, 1);
        assert_eq!(health.minimum_online_retaining_providers, 2);
        assert!(health.degraded);
        assert!(health.message.contains("requested quorum"));

        availability_provider
            .profile_sync(
                ProfileSyncRequest::ReleaseObject(ProfileSyncObjectRequest::new(
                    "default",
                    object_id.clone(),
                )),
                &budget,
            )
            .expect("availability provider can release root object");
        let unretained = device_a
            .profile_sync(
                ProfileSyncRequest::RootHealth(ProfileSyncRootHealthRequest::new(
                    "default",
                    "settings/latest",
                )),
                &budget,
            )
            .expect("unretained root health is reported locally");
        let ProfileSyncResponse::RootHealth { health } = unretained else {
            panic!("unexpected root health response");
        };
        assert_eq!(health.latest_object_id.as_deref(), Some(object_id.as_str()));
        assert!(health.latest_object_available);
        assert_eq!(health.online_retaining_providers, 0);
        assert!(health.degraded);
        assert!(health.message.contains("not retained"));
    }

    #[test]
    fn local_fixture_root_health_ignores_stale_retaining_provider() {
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
                    b"encrypted root freshness object".to_vec(),
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
                    "settings/latest",
                    object_id.clone(),
                )),
                &budget,
            )
            .expect("device a can publish root for health");
        availability_provider
            .profile_sync(
                ProfileSyncRequest::RetainObject(ProfileSyncObjectRequest::new(
                    "default",
                    object_id.clone(),
                )),
                &budget,
            )
            .expect("availability provider can retain root object");

        fixture
            .expire_current_provider_freshness()
            .expect("expire current provider freshness");
        fixture.mark_device_seen("a").expect("mark device a fresh");

        let stale_retention = device_a
            .profile_sync(
                ProfileSyncRequest::RootHealth(ProfileSyncRootHealthRequest::new(
                    "default",
                    "settings/latest",
                )),
                &budget,
            )
            .expect("stale retained root health is reported locally");
        let ProfileSyncResponse::RootHealth { health } = stale_retention else {
            panic!("unexpected root health response");
        };
        assert_eq!(health.latest_object_id.as_deref(), Some(object_id.as_str()));
        assert!(health.latest_object_available);
        assert_eq!(health.online_retaining_providers, 0);
        assert!(health.degraded);
        assert!(health.message.contains("not retained"));

        fixture
            .mark_availability_provider_seen("pin-1")
            .expect("mark availability provider fresh");
        let healthy = device_a
            .profile_sync(
                ProfileSyncRequest::RootHealth(ProfileSyncRootHealthRequest::new(
                    "default",
                    "settings/latest",
                )),
                &budget,
            )
            .expect("fresh retained root health is reported locally");
        let ProfileSyncResponse::RootHealth { health } = healthy else {
            panic!("unexpected root health response");
        };
        assert_eq!(health.online_retaining_providers, 1);
        assert!(!health.degraded);
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
        device_a
            .profile_sync(
                ProfileSyncRequest::PublishRoot(ProfileSyncRootUpdate::new(
                    "default",
                    "settings/latest",
                    object_id.clone(),
                )),
                &budget,
            )
            .expect("device a can publish root for delayed object transfer");
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
        let delayed_health = device_b
            .profile_sync(
                ProfileSyncRequest::RootHealth(ProfileSyncRootHealthRequest::new(
                    "default",
                    "settings/latest",
                )),
                &budget,
            )
            .expect("device b can inspect delayed object-transfer root health");
        let ProfileSyncResponse::RootHealth { health } = delayed_health else {
            panic!("unexpected root health response");
        };
        assert_eq!(health.visible_candidates, 1);
        assert_eq!(health.delayed_candidates, 0);
        assert_eq!(health.latest_object_id.as_deref(), Some(object_id.as_str()));
        assert!(!health.latest_object_available);
        assert_eq!(
            health.delayed_object_provider_ids,
            vec!["local-fixture-device-a".to_string()]
        );
        assert!(health.degraded);
        assert!(health.message.contains("delayed object-transfer"));
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
        let released_health = device_b
            .profile_sync(
                ProfileSyncRequest::RootHealth(ProfileSyncRootHealthRequest::new(
                    "default",
                    "settings/latest",
                )),
                &budget,
            )
            .expect("device b can inspect released object-transfer root health");
        let ProfileSyncResponse::RootHealth { health } = released_health else {
            panic!("unexpected root health response");
        };
        assert_eq!(health.visible_candidates, 1);
        assert!(health.latest_object_available);
        assert!(health.delayed_object_provider_ids.is_empty());
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
    fn local_fixture_can_block_retention_by_pinning_policy() {
        let fixture = LocalProfileSyncFixture::new();
        let mut device = PluginRegistry::new();
        let mut provider = PluginRegistry::new();
        let budget = ResourceBudget::default();

        device.register_service(fixture.service_for_device("publisher"));
        provider.register_service(fixture.service_for_availability_provider("pinner"));

        let put = device
            .profile_sync(
                ProfileSyncRequest::PutEncryptedObject(ProfileSyncPutObjectRequest::new(
                    "default",
                    b"encrypted object for policy-gated pinning".to_vec(),
                )),
                &budget,
            )
            .expect("device can put object into fixture");
        let ProfileSyncResponse::PutEncryptedObject { object_id } = put else {
            panic!("unexpected put response");
        };
        fixture
            .set_availability_provider_retention_available("pinner", false)
            .expect("block pinner retention policy");

        let fetch = provider
            .profile_sync(
                ProfileSyncRequest::GetEncryptedObject(ProfileSyncObjectRequest::new(
                    "default",
                    object_id.clone(),
                )),
                &budget,
            )
            .expect("provider can still transfer object while retention is blocked");
        assert_eq!(
            fetch,
            ProfileSyncResponse::GetEncryptedObject {
                object_id: object_id.clone(),
                bytes: b"encrypted object for policy-gated pinning".to_vec(),
            }
        );
        let retain_error = provider
            .profile_sync(
                ProfileSyncRequest::RetainObject(ProfileSyncObjectRequest::new(
                    "default",
                    object_id.clone(),
                )),
                &budget,
            )
            .expect_err("provider retention policy blocks pinning");
        assert!(matches!(
            retain_error,
            BroadwebdError::UnsupportedRequest(message)
                if message.contains("pinning policy")
        ));
        let retained_status = provider
            .profile_sync(
                ProfileSyncRequest::VerifyRetainedObject(ProfileSyncObjectRequest::new(
                    "default",
                    object_id.clone(),
                )),
                &budget,
            )
            .expect("provider can still verify retention state");
        assert_eq!(
            retained_status,
            ProfileSyncResponse::RetainedObjectStatus {
                object_id: object_id.clone(),
                retained: false,
                available: true,
            }
        );

        fixture
            .set_availability_provider_retention_available("pinner", true)
            .expect("allow pinner retention policy");
        let retained = provider
            .profile_sync(
                ProfileSyncRequest::RetainObject(ProfileSyncObjectRequest::new(
                    "default",
                    object_id.clone(),
                )),
                &budget,
            )
            .expect("provider can retain after pinning policy allows it");
        assert_eq!(
            retained,
            ProfileSyncResponse::RetainObject {
                object_id: object_id.clone(),
                retained: true,
            }
        );
        let retained_status = provider
            .profile_sync(
                ProfileSyncRequest::VerifyRetainedObject(ProfileSyncObjectRequest::new(
                    "default", object_id,
                )),
                &budget,
            )
            .expect("provider can verify retained object");
        assert_eq!(
            retained_status,
            ProfileSyncResponse::RetainedObjectStatus {
                object_id: local_object_id(b"encrypted object for policy-gated pinning"),
                retained: true,
                available: true,
            }
        );
    }

    #[test]
    fn local_fixture_can_limit_retention_quota() {
        let fixture = LocalProfileSyncFixture::new();
        let mut device = PluginRegistry::new();
        let mut provider = PluginRegistry::new();
        let budget = ResourceBudget::default();

        device.register_service(fixture.service_for_device("publisher"));
        provider.register_service(fixture.service_for_availability_provider("quota-pinner"));
        fixture
            .set_availability_provider_retention_quota("quota-pinner", Some(1))
            .expect("limit pinner quota");

        let first = device
            .profile_sync(
                ProfileSyncRequest::PutEncryptedObject(ProfileSyncPutObjectRequest::new(
                    "default",
                    b"first encrypted object under quota".to_vec(),
                )),
                &budget,
            )
            .expect("device can put first fixture object");
        let ProfileSyncResponse::PutEncryptedObject {
            object_id: first_object_id,
        } = first
        else {
            panic!("unexpected first put response");
        };
        let second = device
            .profile_sync(
                ProfileSyncRequest::PutEncryptedObject(ProfileSyncPutObjectRequest::new(
                    "default",
                    b"second encrypted object over quota".to_vec(),
                )),
                &budget,
            )
            .expect("device can put second fixture object");
        let ProfileSyncResponse::PutEncryptedObject {
            object_id: second_object_id,
        } = second
        else {
            panic!("unexpected second put response");
        };

        provider
            .profile_sync(
                ProfileSyncRequest::RetainObject(ProfileSyncObjectRequest::new(
                    "default",
                    first_object_id.clone(),
                )),
                &budget,
            )
            .expect("provider can retain first object inside quota");
        provider
            .profile_sync(
                ProfileSyncRequest::RetainObject(ProfileSyncObjectRequest::new(
                    "default",
                    first_object_id.clone(),
                )),
                &budget,
            )
            .expect("provider can idempotently retain existing object at quota");

        let quota_error = provider
            .profile_sync(
                ProfileSyncRequest::RetainObject(ProfileSyncObjectRequest::new(
                    "default",
                    second_object_id.clone(),
                )),
                &budget,
            )
            .expect_err("provider quota blocks second retained object");
        assert!(matches!(
            quota_error,
            BroadwebdError::UnsupportedRequest(message)
                if message.contains("retention quota exceeded")
        ));
        let second_status = provider
            .profile_sync(
                ProfileSyncRequest::VerifyRetainedObject(ProfileSyncObjectRequest::new(
                    "default",
                    second_object_id.clone(),
                )),
                &budget,
            )
            .expect("provider can verify over-quota object status");
        assert_eq!(
            second_status,
            ProfileSyncResponse::RetainedObjectStatus {
                object_id: second_object_id.clone(),
                retained: false,
                available: true,
            }
        );

        provider
            .profile_sync(
                ProfileSyncRequest::ReleaseObject(ProfileSyncObjectRequest::new(
                    "default",
                    first_object_id,
                )),
                &budget,
            )
            .expect("provider can release retained quota slot");
        let second_retained = provider
            .profile_sync(
                ProfileSyncRequest::RetainObject(ProfileSyncObjectRequest::new(
                    "default",
                    second_object_id.clone(),
                )),
                &budget,
            )
            .expect("provider can retain second object after releasing quota slot");
        assert_eq!(
            second_retained,
            ProfileSyncResponse::RetainObject {
                object_id: second_object_id,
                retained: true,
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
