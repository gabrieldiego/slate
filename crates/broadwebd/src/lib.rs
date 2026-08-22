#![forbid(unsafe_code)]

mod budget;
mod daemon;
mod error;
mod health;
mod http;
pub mod protocols;
mod registry;
pub mod services;
mod state;
mod status;
pub mod transports;

pub(crate) const DEFAULT_PROFILE: &str = "default";
pub const DIRECT_HTTP_PLUGIN: &str = "direct-http";
pub const HTTP_FETCH_PLUGIN: &str = "http-fetch";
pub const IPFS_PROTOCOL_SERVICE: &str = "ipfs";
pub const IPFS_GATEWAY_PLUGIN: &str = "ipfs-gateway";
pub const IPFS_KUBO_RPC_PLUGIN: &str = "ipfs-kubo-rpc";
pub const TOR_PROTOCOL_SERVICE: &str = "tor";
pub const TOR_ARTI_HTTP_PLUGIN: &str = "tor-arti-http";
pub const PROFILE_SYNC_PLUGIN: &str = "profile-sync";
pub const DEFAULT_IPFS_GATEWAY: &str = "http://127.0.0.1:8080";
pub const DEFAULT_IPFS_KUBO_RPC_API: &str = "http://127.0.0.1:5001";
pub const DEFAULT_PUBLIC_IPFS_GATEWAY: &str = "https://ipfs.filebase.io";
pub const DEFAULT_PUBLIC_IPFS_GATEWAYS: &[&str] = &[
    DEFAULT_PUBLIC_IPFS_GATEWAY,
    "https://w3s.link",
    "https://ipfs.io",
    "https://dweb.link",
];
pub const SLATE_IPFS_GATEWAY_ENV: &str = "SLATE_IPFS_GATEWAY";
pub const SLATE_IPFS_GATEWAY_SCOPE_ENV: &str = "SLATE_IPFS_GATEWAY_SCOPE";
pub const SLATE_IPFS_TRANSPORT_ENV: &str = "SLATE_IPFS_TRANSPORT";
pub const SLATE_IPFS_KUBO_RPC_ENV: &str = "SLATE_IPFS_KUBO_RPC";
pub const IN_PROCESS_PROFILE_SYNC_FIXTURE_ENDPOINT_SCHEME: &str = "slate-fixture-profile-sync";
pub const IN_PROCESS_PROFILE_SYNC_FIXTURE_ENDPOINT_PREFIX: &str = "slate-fixture-profile-sync://";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InProcessProfileSyncFixtureEndpointRef<'a> {
    network_id: &'a str,
    provider_id: &'a str,
}

impl<'a> InProcessProfileSyncFixtureEndpointRef<'a> {
    pub fn network_id(&self) -> &'a str {
        self.network_id
    }

    pub fn provider_id(&self) -> &'a str {
        self.provider_id
    }
}

pub fn parse_in_process_profile_sync_fixture_endpoint_ref(
    endpoint_ref: &str,
) -> Option<InProcessProfileSyncFixtureEndpointRef<'_>> {
    let endpoint = endpoint_ref.strip_prefix(IN_PROCESS_PROFILE_SYNC_FIXTURE_ENDPOINT_PREFIX)?;
    let (network_id, provider_id) = endpoint.split_once('/')?;
    if !is_in_process_profile_sync_fixture_endpoint_token(network_id)
        || !is_in_process_profile_sync_fixture_endpoint_token(provider_id)
    {
        return None;
    }

    Some(InProcessProfileSyncFixtureEndpointRef {
        network_id,
        provider_id,
    })
}

fn is_in_process_profile_sync_fixture_endpoint_token(token: &str) -> bool {
    !token.is_empty()
        && token.len() <= 512
        && token
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

pub use budget::ResourceBudget;
pub use daemon::{
    BroadwebDaemon, default_session_state_root, default_session_status_reporter,
    default_session_status_snapshot,
};
pub use error::BroadwebdError;
pub use health::{
    DaemonHealth, DaemonLifecycle, PluginHealth, PluginKind, PluginMetadata, PluginStatus,
    ResourceProfile,
};
pub use http::{
    DownloadRecord, FetchDisposition, FetchPurpose, FetchRouteInfo, HttpFetchRequest,
    HttpFetchResponse, HttpHeader, ProfileSyncObjectRequest, ProfileSyncProfileRequest,
    ProfileSyncProviderHealth, ProfileSyncProviderRecord, ProfileSyncProviderRoles,
    ProfileSyncPutObjectRequest, ProfileSyncRequest, ProfileSyncResponse, ProfileSyncRootCandidate,
    ProfileSyncRootHealth, ProfileSyncRootHealthRequest, ProfileSyncRootRequest,
    ProfileSyncRootUpdate, ServiceRequest, ServiceResponse, TransportHttpRequest,
};
pub use protocols::ipfs::{
    IpfsConfig, IpfsGatewayEndpoint, IpfsGatewayScope, IpfsGatewayTransport,
    IpfsKuboProfileSyncOperation, IpfsKuboProfileSyncRpc, IpfsKuboProfileSyncRpcRequest,
    IpfsKuboRpcEndpoint, IpfsKuboRpcTransport, IpfsService, IpfsTransportKind,
    ipfs_gateway_http_url, ipfs_kubo_cat_url, ipfs_kubo_profile_sync_add_url,
    ipfs_kubo_profile_sync_added_object_id, ipfs_kubo_profile_sync_name_publish_url,
    ipfs_kubo_profile_sync_name_resolve_url, ipfs_kubo_profile_sync_pin_add_url,
    ipfs_kubo_profile_sync_pin_ls_has_recursive_pin, ipfs_kubo_profile_sync_pin_ls_url,
    ipfs_kubo_profile_sync_pin_rm_url, ipfs_kubo_profile_sync_published_object_id,
    ipfs_kubo_profile_sync_resolved_object_id,
};
pub use protocols::tor::{
    TOR_HTTP_SCHEME, TOR_HTTPS_SCHEME, TorArtiHttpTransport, TorHttpTarget, TorNetworkScheme,
    TorService, http_response_from_bytes, is_onion_host, is_onion_url, is_tor_http_scheme,
    normalize_tor_navigation_url, tor_http_target, tor_url_from_http_url,
};
pub use registry::{
    ApplicationServicePlugin, PluginInstallReport, PluginRegistry, ProtocolInstallReport,
    ProtocolService, TransportPlugin,
};
pub use services::{
    http_fetch::HttpFetchService,
    profile_sync::{LocalProfileSyncFixture, ProfileSyncService},
};
pub use state::{StateRoot, TemporaryDownloadRecord};
pub use status::{BroadwebStatusKind, BroadwebStatusReporter, BroadwebStatusSnapshot};
pub use transports::direct_http::DirectHttpTransport;

#[cfg(any(test, feature = "test-fixtures"))]
pub mod test_fixtures {
    use crate::http::{
        fetch_http_url, internal_fixture_http_url_belongs_to_network, is_internal_fixture_http_url,
        parse_http_url, register_internal_fixture_http_response_for_network,
        register_internal_fixture_http_sequence_for_network, take_internal_fixture_http_requests,
        unregistered_internal_fixture_http_url_for_network,
    };
    use crate::protocols::ipfs::{
        internal_kubo_rpc_url_belongs_to_network, register_internal_kubo_rpc_fixture_for_network,
        take_internal_kubo_rpc_fixture_requests,
    };
    use crate::services::{http_fetch::HttpFetchService, profile_sync::ProfileSyncService};
    use crate::{
        BroadwebDaemon, BroadwebdError, DIRECT_HTTP_PLUGIN,
        IN_PROCESS_PROFILE_SYNC_FIXTURE_ENDPOINT_PREFIX, IpfsConfig, IpfsService, PluginKind,
        PluginMetadata, PluginRegistry, ProfileSyncProviderRoles, ResourceBudget, ResourceProfile,
        TransportHttpRequest, TransportPlugin,
    };
    use std::path::PathBuf;

    pub use crate::http::InternalFixtureHttpResponse;
    pub use crate::protocols::ipfs::{InternalKuboRpcResponse, InternalKuboRpcTransportShim};
    pub use crate::services::profile_sync::LocalProfileSyncFixture;

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct InProcessFixtureHttpTransport {
        network_id: String,
    }

    impl InProcessFixtureHttpTransport {
        fn new(network_id: impl Into<String>) -> Self {
            Self {
                network_id: network_id.into(),
            }
        }
    }

    impl Default for InProcessFixtureHttpTransport {
        fn default() -> Self {
            Self::new("global")
        }
    }

    impl TransportPlugin for InProcessFixtureHttpTransport {
        fn metadata(&self) -> PluginMetadata {
            PluginMetadata::new(DIRECT_HTTP_PLUGIN, PluginKind::Transport)
                .with_capabilities(&[
                    "http-fixture",
                    "http-fetch",
                    "in-process",
                    "socketless-fixture",
                ])
                .with_privacy_boundary(
                    "in-process HTTP fixture transport; no sockets, DNS, or external network",
                )
                .with_resource_profile(ResourceProfile::Low)
        }

        fn fetch_http(
            &self,
            request: &TransportHttpRequest,
            budget: &ResourceBudget,
        ) -> Result<crate::HttpFetchResponse, BroadwebdError> {
            let url = parse_http_url(&request.url)?;
            if is_internal_fixture_http_url(&url) {
                if !internal_fixture_http_url_belongs_to_network(&url, self.network_id.as_str()) {
                    return Err(BroadwebdError::UnsupportedRequest(format!(
                        "internal HTTP fixture URL does not belong to in-process network {}: {}",
                        self.network_id, request.url
                    )));
                }
                return fetch_http_url(url, budget);
            }

            Err(BroadwebdError::UnsupportedRequest(format!(
                "in-process fixture HTTP transport cannot fetch external URL: {}",
                request.url
            )))
        }
    }

    /// In-process broadweb network fixture.
    ///
    /// Endpoints returned by this fixture use Slate-only synthetic URL schemes
    /// such as `slate-fixture-http://`, `slate-fixture-kubo://`, and
    /// `slate-fixture-profile-sync://`. They are
    /// resolved through process-local registries and never bind loopback
    /// sockets, start listeners, or contact external networks.
    #[derive(Clone, Debug)]
    pub struct InProcessBroadwebNetwork {
        network_id: String,
        profile_sync: LocalProfileSyncFixture,
    }

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct InProcessProfileSyncProviderEndpoint {
        provider_id: String,
        endpoint_ref: String,
    }

    impl InProcessProfileSyncProviderEndpoint {
        fn new(network_id: &str, provider_id: impl Into<String>) -> Self {
            let provider_id = provider_id.into();
            Self {
                endpoint_ref: format!(
                    "{IN_PROCESS_PROFILE_SYNC_FIXTURE_ENDPOINT_PREFIX}{network_id}/{provider_id}"
                ),
                provider_id,
            }
        }

        pub fn provider_id(&self) -> &str {
            self.provider_id.as_str()
        }

        pub fn endpoint_ref(&self) -> &str {
            self.endpoint_ref.as_str()
        }

        pub fn into_endpoint_ref(self) -> String {
            self.endpoint_ref
        }
    }

    impl Default for InProcessBroadwebNetwork {
        fn default() -> Self {
            Self {
                network_id: next_in_process_network_id(),
                profile_sync: LocalProfileSyncFixture::new(),
            }
        }
    }

    impl InProcessBroadwebNetwork {
        pub fn new() -> Self {
            Self::default()
        }

        pub fn fixture_registry(&self) -> PluginRegistry {
            let mut registry = PluginRegistry::new();
            registry
                .register_transport(InProcessFixtureHttpTransport::new(self.network_id.clone()));
            registry.register_service(HttpFetchService);
            registry.register_service(ProfileSyncService::new());
            registry
        }

        pub fn network_id(&self) -> &str {
            self.network_id.as_str()
        }

        pub fn profile_sync_provider_endpoint(
            &self,
            provider_id: impl Into<String>,
        ) -> InProcessProfileSyncProviderEndpoint {
            InProcessProfileSyncProviderEndpoint::new(self.network_id.as_str(), provider_id)
        }

        pub fn profile_sync_provider_endpoint_ref(&self, provider_id: impl Into<String>) -> String {
            self.profile_sync_provider_endpoint(provider_id)
                .into_endpoint_ref()
        }

        pub fn registry_for_device(&self, device_id: impl AsRef<str>) -> PluginRegistry {
            let mut registry = self.fixture_registry();
            registry.register_service(self.profile_sync.service_for_device(device_id));
            registry
        }

        pub fn registry_for_availability_provider(
            &self,
            provider_id: impl AsRef<str>,
        ) -> PluginRegistry {
            let mut registry = self.fixture_registry();
            registry.register_service(
                self.profile_sync
                    .service_for_availability_provider(provider_id),
            );
            registry
        }

        pub fn registry_for_provider_with_roles(
            &self,
            provider_id: impl Into<String>,
            provider_kind: impl Into<String>,
            roles: ProfileSyncProviderRoles,
        ) -> PluginRegistry {
            let mut registry = self.fixture_registry();
            registry.register_service(self.profile_sync.service_for_provider_with_roles(
                provider_id,
                provider_kind,
                roles,
            ));
            registry
        }

        pub fn registry_for_ipfs_gateway(
            &self,
            gateway_base: impl Into<String>,
        ) -> Result<PluginRegistry, BroadwebdError> {
            let gateway_base = gateway_base.into();
            let gateway_url = parse_http_url(&gateway_base)?;
            if !internal_fixture_http_url_belongs_to_network(&gateway_url, self.network_id.as_str())
            {
                return Err(BroadwebdError::UnsupportedRequest(format!(
                    "in-process IPFS gateway fixtures must use a URL created by network {}: {}",
                    self.network_id, gateway_base
                )));
            }
            let mut registry = self.fixture_registry();
            registry.register_protocol_service(IpfsService::new(IpfsConfig::new(gateway_base)?));
            Ok(registry)
        }

        pub fn registry_for_kubo_rpc(
            &self,
            api_base_url: impl Into<String>,
        ) -> Result<PluginRegistry, BroadwebdError> {
            let api_base_url = api_base_url.into();
            let api_url = parse_http_url(&api_base_url)?;
            if !internal_kubo_rpc_url_belongs_to_network(&api_url, self.network_id.as_str()) {
                return Err(BroadwebdError::UnsupportedRequest(format!(
                    "in-process Kubo RPC fixtures must use a URL created by network {}: {}",
                    self.network_id, api_base_url
                )));
            }
            let mut registry = self.fixture_registry();
            registry.register_protocol_service(IpfsService::new(IpfsConfig::with_kubo_rpc(
                api_base_url,
            )?));
            Ok(registry)
        }

        pub fn registry_for_kubo_profile_sync(
            &self,
            api_base_url: impl Into<String>,
            provider_id: impl Into<String>,
        ) -> Result<PluginRegistry, BroadwebdError> {
            let api_base_url = api_base_url.into();
            let api_url = parse_http_url(&api_base_url)?;
            if !internal_kubo_rpc_url_belongs_to_network(&api_url, self.network_id.as_str()) {
                return Err(BroadwebdError::UnsupportedRequest(format!(
                    "in-process Kubo profile-sync fixtures must use a URL created by network {}: {}",
                    self.network_id, api_base_url
                )));
            }
            let mut registry = self.fixture_registry();
            registry.register_service(ProfileSyncService::kubo_fixture(api_base_url, provider_id)?);
            Ok(registry)
        }

        pub fn daemon_for_device(
            &self,
            state_root: impl Into<PathBuf>,
            budget: ResourceBudget,
            device_id: impl AsRef<str>,
        ) -> Result<BroadwebDaemon, BroadwebdError> {
            BroadwebDaemon::start_with_registry(
                state_root,
                budget,
                self.registry_for_device(device_id),
            )
        }

        pub fn daemon_for_availability_provider(
            &self,
            state_root: impl Into<PathBuf>,
            budget: ResourceBudget,
            provider_id: impl AsRef<str>,
        ) -> Result<BroadwebDaemon, BroadwebdError> {
            BroadwebDaemon::start_with_registry(
                state_root,
                budget,
                self.registry_for_availability_provider(provider_id),
            )
        }

        pub fn daemon_for_ipfs_gateway(
            &self,
            state_root: impl Into<PathBuf>,
            budget: ResourceBudget,
            gateway_base: impl Into<String>,
        ) -> Result<BroadwebDaemon, BroadwebdError> {
            BroadwebDaemon::start_with_registry(
                state_root,
                budget,
                self.registry_for_ipfs_gateway(gateway_base)?,
            )
        }

        pub fn daemon_for_kubo_rpc(
            &self,
            state_root: impl Into<PathBuf>,
            budget: ResourceBudget,
            api_base_url: impl Into<String>,
        ) -> Result<BroadwebDaemon, BroadwebdError> {
            BroadwebDaemon::start_with_registry(
                state_root,
                budget,
                self.registry_for_kubo_rpc(api_base_url)?,
            )
        }

        pub fn daemon_for_kubo_profile_sync(
            &self,
            state_root: impl Into<PathBuf>,
            budget: ResourceBudget,
            api_base_url: impl Into<String>,
            provider_id: impl Into<String>,
        ) -> Result<BroadwebDaemon, BroadwebdError> {
            BroadwebDaemon::start_with_registry(
                state_root,
                budget,
                self.registry_for_kubo_profile_sync(api_base_url, provider_id)?,
            )
        }

        pub fn daemon_for_provider_with_roles(
            &self,
            state_root: impl Into<PathBuf>,
            budget: ResourceBudget,
            provider_id: impl Into<String>,
            provider_kind: impl Into<String>,
            roles: ProfileSyncProviderRoles,
        ) -> Result<BroadwebDaemon, BroadwebdError> {
            BroadwebDaemon::start_with_registry(
                state_root,
                budget,
                self.registry_for_provider_with_roles(provider_id, provider_kind, roles),
            )
        }

        pub fn http_response(&self, response: InternalFixtureHttpResponse) -> InProcessHttpFixture {
            InProcessHttpFixture::new(register_internal_fixture_http_response_for_network(
                self.network_id.as_str(),
                response,
            ))
        }

        pub fn http_sequence(
            &self,
            responses: Vec<InternalFixtureHttpResponse>,
        ) -> InProcessHttpFixture {
            InProcessHttpFixture::new(register_internal_fixture_http_sequence_for_network(
                self.network_id.as_str(),
                responses,
            ))
        }

        pub fn missing_http_url(&self) -> String {
            unregistered_internal_fixture_http_url_for_network(self.network_id.as_str())
        }

        pub fn kubo_rpc_response(
            &self,
            response: InternalKuboRpcResponse,
        ) -> InProcessKuboRpcFixture {
            InProcessKuboRpcFixture::new(register_internal_kubo_rpc_fixture_for_network(
                self.network_id.as_str(),
                vec![response],
            ))
        }

        pub fn kubo_rpc_sequence(
            &self,
            responses: Vec<InternalKuboRpcResponse>,
        ) -> InProcessKuboRpcFixture {
            InProcessKuboRpcFixture::new(register_internal_kubo_rpc_fixture_for_network(
                self.network_id.as_str(),
                responses,
            ))
        }

        pub fn profile_sync(&self) -> LocalProfileSyncFixture {
            self.profile_sync.clone()
        }
    }

    fn next_in_process_network_id() -> String {
        use std::sync::atomic::{AtomicUsize, Ordering};

        static NEXT_NETWORK_ID: AtomicUsize = AtomicUsize::new(1);

        let id = NEXT_NETWORK_ID.fetch_add(1, Ordering::Relaxed);
        format!("network-{id}")
    }

    #[derive(Debug)]
    pub struct InProcessHttpFixture {
        base_url: String,
    }

    impl InProcessHttpFixture {
        fn new(base_url: String) -> Self {
            Self { base_url }
        }

        pub fn base_url(&self) -> &str {
            self.base_url.as_str()
        }

        pub fn finish(mut self) -> Vec<String> {
            self.take_requests()
        }

        fn take_requests(&mut self) -> Vec<String> {
            if self.base_url.is_empty() {
                return Vec::new();
            }
            let base_url = std::mem::take(&mut self.base_url);
            take_internal_fixture_http_requests(base_url.as_str())
        }
    }

    impl Drop for InProcessHttpFixture {
        fn drop(&mut self) {
            let _ = self.take_requests();
        }
    }

    #[derive(Debug)]
    pub struct InProcessKuboRpcFixture {
        base_url: String,
    }

    impl InProcessKuboRpcFixture {
        fn new(base_url: String) -> Self {
            Self { base_url }
        }

        pub fn base_url(&self) -> &str {
            self.base_url.as_str()
        }

        pub fn finish(mut self) -> Vec<String> {
            self.take_requests()
        }

        fn take_requests(&mut self) -> Vec<String> {
            if self.base_url.is_empty() {
                return Vec::new();
            }
            let base_url = std::mem::take(&mut self.base_url);
            take_internal_kubo_rpc_fixture_requests(base_url.as_str())
        }
    }

    impl Drop for InProcessKuboRpcFixture {
        fn drop(&mut self) {
            let _ = self.take_requests();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_fixtures::{
        InProcessBroadwebNetwork, InProcessHttpFixture, InProcessKuboRpcFixture,
        InternalFixtureHttpResponse, InternalKuboRpcResponse, InternalKuboRpcTransportShim,
    };
    use super::{
        BroadwebDaemon, BroadwebStatusKind, BroadwebStatusReporter, BroadwebdError,
        DEFAULT_IPFS_KUBO_RPC_API, DIRECT_HTTP_PLUGIN, FetchDisposition, FetchPurpose,
        HttpFetchRequest, HttpFetchResponse, IPFS_GATEWAY_PLUGIN, IPFS_KUBO_RPC_PLUGIN, IpfsConfig,
        IpfsGatewayEndpoint, IpfsGatewayScope, IpfsGatewayTransport, IpfsKuboProfileSyncOperation,
        IpfsKuboProfileSyncRpc, IpfsKuboRpcEndpoint, IpfsService, IpfsTransportKind,
        PROFILE_SYNC_PLUGIN, PluginHealth, PluginKind, PluginMetadata, PluginRegistry,
        ProfileSyncObjectRequest, ProfileSyncProfileRequest, ProfileSyncProviderRoles,
        ProfileSyncPutObjectRequest, ProfileSyncRequest, ProfileSyncResponse,
        ProfileSyncRootHealthRequest, ProfileSyncRootRequest, ProfileSyncRootUpdate,
        ProfileSyncService, ProtocolService, ResourceBudget, ResourceProfile,
        SLATE_IPFS_TRANSPORT_ENV, StateRoot, TOR_ARTI_HTTP_PLUGIN, TOR_PROTOCOL_SERVICE,
        TorService, TransportHttpRequest, TransportPlugin, ipfs_gateway_http_url,
        ipfs_kubo_cat_url, ipfs_kubo_profile_sync_add_url, ipfs_kubo_profile_sync_added_object_id,
        ipfs_kubo_profile_sync_name_publish_url, ipfs_kubo_profile_sync_name_resolve_url,
        ipfs_kubo_profile_sync_pin_add_url, ipfs_kubo_profile_sync_pin_ls_has_recursive_pin,
        ipfs_kubo_profile_sync_pin_ls_url, ipfs_kubo_profile_sync_pin_rm_url,
        ipfs_kubo_profile_sync_published_object_id, ipfs_kubo_profile_sync_resolved_object_id,
        tor_http_target, tor_url_from_http_url,
    };
    use std::fs;
    use std::path::PathBuf;
    use url::Url;

    #[test]
    fn state_root_prepares_profile_directories() {
        let root = test_state_root("state-root");
        let state = StateRoot::prepare(&root).expect("prepare state root");
        let profile_root = state.prepare_profile("default").expect("prepare profile");

        assert!(profile_root.join("protocol-state").is_dir());
        assert!(profile_root.join("temporary").is_dir());
        assert!(state.prepare_profile("../escape").is_err());

        let download_path = state
            .store_temporary_download("default", "../ipfs image?.png", b"download bytes")
            .expect("store temporary download");
        assert!(download_path.starts_with(profile_root.join("temporary").join("downloads")));
        assert_eq!(
            download_path.file_name().and_then(|name| name.to_str()),
            Some("_ipfs_image_.png")
        );
        assert_eq!(
            fs::read(&download_path).expect("read temporary download"),
            b"download bytes"
        );
        let second_download_path = state
            .store_temporary_download("default", "../ipfs image?.png", b"second")
            .expect("store colliding temporary download");
        assert_ne!(download_path, second_download_path);
        assert_eq!(
            second_download_path
                .file_name()
                .and_then(|name| name.to_str()),
            Some("_ipfs_image_-1.png")
        );

        let downloads = state
            .temporary_downloads("default")
            .expect("list temporary downloads");
        assert_eq!(downloads.len(), 2);
        assert_eq!(
            downloads
                .iter()
                .find(|download| download.filename == "_ipfs_image_.png")
                .map(|download| download.size_bytes),
            Some("download bytes".len() as u64)
        );
        assert_eq!(
            downloads
                .iter()
                .find(|download| download.filename == "_ipfs_image_-1.png")
                .map(|download| download.size_bytes),
            Some("second".len() as u64)
        );
        assert!(state.temporary_downloads("../escape").is_err());

        let empty_root = test_state_root("empty-downloads");
        let empty_state = StateRoot::prepare(&empty_root).expect("prepare empty state root");
        assert!(
            empty_state
                .temporary_downloads("default")
                .expect("list empty temporary downloads")
                .is_empty()
        );

        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(empty_root);
    }

    #[test]
    fn state_root_stores_indexed_downloads_in_configured_download_root() {
        let root = test_state_root("indexed-downloads");
        let download_root = test_download_root("indexed-downloads");
        let state = StateRoot::prepare_with_download_root(&root, &download_root)
            .expect("prepare state root");

        let download_path = state
            .store_download("default", "../ipfs image?.png", b"download bytes")
            .expect("store indexed download");
        assert!(download_path.starts_with(&download_root));
        assert_eq!(
            download_path.file_name().and_then(|name| name.to_str()),
            Some("_ipfs_image_.png")
        );
        assert_eq!(
            fs::read(&download_path).expect("read indexed download"),
            b"download bytes"
        );

        let second_download_path = state
            .store_download("default", "../ipfs image?.png", b"second")
            .expect("store colliding indexed download");
        assert_eq!(
            second_download_path
                .file_name()
                .and_then(|name| name.to_str()),
            Some("_ipfs_image_-1.png")
        );

        let downloads = state.downloads("default").expect("list indexed downloads");
        assert_eq!(downloads.len(), 2);
        assert_eq!(
            downloads
                .iter()
                .find(|download| download.filename == "_ipfs_image_.png")
                .map(|download| download.path.as_path()),
            Some(download_path.as_path())
        );
        assert_eq!(
            downloads
                .iter()
                .find(|download| download.filename == "_ipfs_image_-1.png")
                .map(|download| download.size_bytes),
            Some("second".len() as u64)
        );
        assert!(state.downloads("../escape").is_err());

        let empty_root = test_state_root("empty-indexed-downloads");
        let empty_download_root = test_download_root("empty-indexed-downloads");
        let empty_state = StateRoot::prepare_with_download_root(&empty_root, &empty_download_root)
            .expect("prepare empty state root");
        assert!(
            empty_state
                .downloads("default")
                .expect("list empty indexed downloads")
                .is_empty()
        );

        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(download_root);
        let _ = fs::remove_dir_all(empty_root);
        let _ = fs::remove_dir_all(empty_download_root);
    }

    #[test]
    fn default_registry_reports_ready_http_plugins() {
        let daemon = BroadwebDaemon::start(test_state_root("ready-plugins")).expect("daemon");
        let health = daemon.health();

        assert!(health.plugins.iter().any(|status| {
            status.metadata.id == "direct-http" && matches!(status.health, PluginHealth::Ready)
        }));
        assert!(health.plugins.iter().any(|status| {
            status.metadata.id == "http-fetch" && matches!(status.health, PluginHealth::Ready)
        }));
        assert!(health.plugins.iter().any(|status| {
            status.metadata.id == "ipfs" && matches!(status.health, PluginHealth::Ready)
        }));
        assert!(health.plugins.iter().any(|status| {
            status.metadata.id == PROFILE_SYNC_PLUGIN
                && matches!(status.health, PluginHealth::Ready)
        }));

        let _ = fs::remove_dir_all(daemon.state_root().path());
    }

    #[test]
    fn fake_profile_sync_service_stores_retains_and_resolves_local_objects() {
        let daemon = BroadwebDaemon::start(test_state_root("profile-sync")).expect("daemon");
        let put = daemon
            .profile_sync(ProfileSyncRequest::PutEncryptedObject(
                ProfileSyncPutObjectRequest::new("default", b"encrypted manifest".to_vec()),
            ))
            .expect("put profile sync object");
        let ProfileSyncResponse::PutEncryptedObject { object_id } = put else {
            panic!("unexpected put response");
        };

        let fetched = daemon
            .profile_sync(ProfileSyncRequest::GetEncryptedObject(
                ProfileSyncObjectRequest::new("default", object_id.clone()),
            ))
            .expect("fetch profile sync object");
        assert_eq!(
            fetched,
            ProfileSyncResponse::GetEncryptedObject {
                object_id: object_id.clone(),
                bytes: b"encrypted manifest".to_vec()
            }
        );

        let retained = daemon
            .profile_sync(ProfileSyncRequest::RetainObject(
                ProfileSyncObjectRequest::new("default", object_id.clone()),
            ))
            .expect("retain profile sync object");
        assert_eq!(
            retained,
            ProfileSyncResponse::RetainObject {
                object_id: object_id.clone(),
                retained: true
            }
        );

        let retained_objects = daemon
            .profile_sync(ProfileSyncRequest::ListRetainedObjects(
                ProfileSyncProfileRequest::new("default"),
            ))
            .expect("list retained profile sync objects");
        assert_eq!(
            retained_objects,
            ProfileSyncResponse::RetainedObjects {
                object_ids: vec![object_id.clone()]
            }
        );

        let verified = daemon
            .profile_sync(ProfileSyncRequest::VerifyRetainedObject(
                ProfileSyncObjectRequest::new("default", object_id.clone()),
            ))
            .expect("verify retained profile sync object");
        assert_eq!(
            verified,
            ProfileSyncResponse::RetainedObjectStatus {
                object_id: object_id.clone(),
                retained: true,
                available: true
            }
        );

        let published = daemon
            .profile_sync(ProfileSyncRequest::PublishRoot(ProfileSyncRootUpdate::new(
                "default",
                "profile-root",
                object_id.clone(),
            )))
            .expect("publish profile root");
        assert_eq!(
            published,
            ProfileSyncResponse::Root {
                root_id: "profile-root".to_string(),
                object_id: Some(object_id.clone())
            }
        );

        let resolved = daemon
            .profile_sync(ProfileSyncRequest::ResolveRoot(
                ProfileSyncRootRequest::new("default", "profile-root"),
            ))
            .expect("resolve profile root");
        assert_eq!(
            resolved,
            ProfileSyncResponse::Root {
                root_id: "profile-root".to_string(),
                object_id: Some(object_id.clone())
            }
        );

        let providers = daemon
            .profile_sync(ProfileSyncRequest::DiscoverProviders(
                ProfileSyncProfileRequest::new("default"),
            ))
            .expect("discover providers");
        let ProfileSyncResponse::Providers { providers } = providers else {
            panic!("unexpected providers response");
        };
        assert_eq!(providers.len(), 1);
        assert_eq!(providers[0].provider_kind, "local-fake");
        assert_eq!(providers[0].retained_objects, 1);
        assert_eq!(
            providers[0].roles,
            ProfileSyncProviderRoles::logged_in_device()
        );
        assert_eq!(
            providers[0].can_publish_roots,
            providers[0].roles.mutable_roots
        );

        let released = daemon
            .profile_sync(ProfileSyncRequest::ReleaseObject(
                ProfileSyncObjectRequest::new("default", object_id.clone()),
            ))
            .expect("release profile sync object");
        assert_eq!(
            released,
            ProfileSyncResponse::ReleaseObject {
                object_id: object_id.clone(),
                retained: false
            }
        );

        let verified_after_release = daemon
            .profile_sync(ProfileSyncRequest::VerifyRetainedObject(
                ProfileSyncObjectRequest::new("default", object_id.clone()),
            ))
            .expect("verify released profile sync object");
        assert_eq!(
            verified_after_release,
            ProfileSyncResponse::RetainedObjectStatus {
                object_id,
                retained: false,
                available: true
            }
        );

        let _ = fs::remove_dir_all(daemon.state_root().path());
    }

    #[test]
    fn default_registry_can_use_explicit_ipfs_config() {
        let registry = PluginRegistry::with_default_http_and_ipfs_config(
            IpfsConfig::with_public_gateway("https://ipfs.io").expect("public gateway config"),
        );
        let statuses = registry.plugin_statuses();

        assert!(statuses.iter().any(|status| {
            status.metadata.id == "ipfs"
                && status
                    .metadata
                    .capabilities
                    .iter()
                    .any(|capability| capability == "public-gateway")
                && matches!(status.health, PluginHealth::Ready)
        }));
    }

    #[test]
    fn registry_reports_missing_dependencies_as_degraded() {
        let mut registry = PluginRegistry::new();
        registry.register_service(super::HttpFetchService);

        let statuses = registry.plugin_statuses();
        assert!(statuses.iter().any(|status| {
            status.metadata.id == "http-fetch" && matches!(status.health, PluginHealth::Degraded(_))
        }));
    }

    #[test]
    fn daemon_can_install_replace_and_remove_plugins_without_restart() {
        let mut daemon = BroadwebDaemon::start_with_registry(
            test_state_root("hot-plugins"),
            ResourceBudget::default(),
            PluginRegistry::new(),
        )
        .expect("daemon");

        assert!(matches!(
            daemon.fetch_http(HttpFetchRequest::default_profile("http://example.test/")),
            Err(BroadwebdError::MissingPlugin(plugin)) if plugin == "http-fetch"
        ));

        let service_install = daemon.install_service(super::HttpFetchService);
        assert_eq!(service_install.metadata.id, "http-fetch");
        assert!(!service_install.replaced_existing);

        let transport_install =
            daemon.install_transport(FixtureTransport::new(DIRECT_HTTP_PLUGIN, "first body"));
        assert_eq!(transport_install.metadata.id, DIRECT_HTTP_PLUGIN);
        assert!(!transport_install.replaced_existing);

        let first_response = daemon
            .fetch_http(HttpFetchRequest::default_profile("http://example.test/"))
            .expect("fetch with installed plugins");
        assert!(first_response.body_text_lossy().contains("first body"));

        let transport_replace =
            daemon.install_transport(FixtureTransport::new(DIRECT_HTTP_PLUGIN, "second body"));
        assert_eq!(transport_replace.metadata.id, DIRECT_HTTP_PLUGIN);
        assert!(transport_replace.replaced_existing);

        let second_response = daemon
            .fetch_http(HttpFetchRequest::default_profile("http://example.test/"))
            .expect("fetch with replaced transport");
        assert!(second_response.body_text_lossy().contains("second body"));

        let removed = daemon.remove_service("http-fetch").expect("remove service");
        assert_eq!(removed.id, "http-fetch");
        assert!(matches!(
            daemon.fetch_http(HttpFetchRequest::default_profile("http://example.test/")),
            Err(BroadwebdError::MissingPlugin(plugin)) if plugin == "http-fetch"
        ));

        let _ = fs::remove_dir_all(daemon.state_root().path());
    }

    #[test]
    fn http_fetch_uses_in_process_http_transport() {
        let network = InProcessBroadwebNetwork::new();
        let (address, fixture) = in_process_http_fixture_for_network(
            &network,
            "text/html; charset=utf-8",
            "<!doctype html><title>Broadwebd Fixture</title><h1>Fetched</h1>",
        );
        let daemon = BroadwebDaemon::start_with_registry(
            test_state_root("http-fetch"),
            Default::default(),
            network.fixture_registry(),
        )
        .expect("daemon");
        let response = daemon
            .fetch_http(HttpFetchRequest::default_profile(&address))
            .expect("fetch fixture");
        fixture.finish();

        assert_eq!(response.status_code, 200);
        assert_eq!(response.disposition, FetchDisposition::RenderHtml);
        assert!(response.body_text_lossy().contains("Broadwebd Fixture"));

        let _ = fs::remove_dir_all(daemon.state_root().path());
    }

    #[test]
    fn http_fetch_can_use_ipfs_gateway_transport_for_html() {
        let (gateway, fixture) = in_process_http_fixture(
            "text/html; charset=utf-8",
            "<!doctype html><title>IPFS Fixture</title><h1>Fetched From IPFS</h1>",
        );
        let mut registry = PluginRegistry::new();
        registry.register_transport(IpfsGatewayTransport::local(&gateway).expect("local gateway"));
        registry.register_service(super::HttpFetchService);
        let daemon = BroadwebDaemon::start_with_registry(
            test_state_root("ipfs-html"),
            Default::default(),
            registry,
        )
        .expect("daemon");
        let response = daemon
            .fetch_http(
                HttpFetchRequest::default_profile("ipfs://bafybeigdyrzt/index.html")
                    .through_transport("ipfs-gateway"),
            )
            .expect("fetch IPFS fixture");
        fixture.finish();

        assert_eq!(response.disposition, FetchDisposition::RenderHtml);
        assert!(response.body_text_lossy().contains("IPFS Fixture"));

        let _ = fs::remove_dir_all(daemon.state_root().path());
    }

    #[test]
    fn http_fetch_routes_ipfs_through_protocol_service() {
        let (gateway, fixture) = in_process_http_fixture(
            "text/html; charset=utf-8",
            "<!doctype html><title>IPFS Service Fixture</title><h1>Fetched From IPFS Service</h1>",
        );
        let mut registry = PluginRegistry::new();
        registry.register_protocol_service(IpfsService::new(
            IpfsConfig::new(&gateway).expect("local gateway config"),
        ));
        registry.register_service(super::HttpFetchService);
        let daemon = BroadwebDaemon::start_with_registry(
            test_state_root("ipfs-service-html"),
            Default::default(),
            registry,
        )
        .expect("daemon");
        let response = daemon
            .fetch_http(HttpFetchRequest::default_profile(
                "ipfs://bafybeigdyrzt/index.html",
            ))
            .expect("fetch IPFS fixture");
        fixture.finish();

        assert_eq!(response.disposition, FetchDisposition::RenderHtml);
        assert!(response.body_text_lossy().contains("IPFS Service Fixture"));

        let _ = fs::remove_dir_all(daemon.state_root().path());
    }

    #[test]
    fn http_fetch_annotates_ipfs_gateway_profile_and_privacy_context() {
        let (gateway, fixture) = in_process_http_fixture(
            "text/html; charset=utf-8",
            "<!doctype html><title>IPFS Profile Fixture</title>",
        );
        let mut registry = PluginRegistry::new();
        registry.register_protocol_service(IpfsService::new(
            IpfsConfig::new(&gateway).expect("local gateway config"),
        ));
        registry.register_service(super::HttpFetchService);
        let daemon = BroadwebDaemon::start_with_registry(
            test_state_root("ipfs-route-context"),
            Default::default(),
            registry,
        )
        .expect("daemon");
        let response = daemon
            .fetch_http(HttpFetchRequest::new(
                "research",
                "ipfs://bafybeigdyrzt/index.html",
            ))
            .expect("fetch IPFS fixture");
        fixture.finish();

        let route = response.route.expect("route info");
        assert_eq!(route.profile, "research");
        assert_eq!(route.transport_id, IPFS_GATEWAY_PLUGIN);
        assert!(route.privacy_boundary.contains("local IPFS gateway"));
        assert_eq!(route.purpose, FetchPurpose::Navigation);
        assert!(
            daemon
                .state_root()
                .profile_root("research")
                .expect("research profile root")
                .join("protocol-state")
                .is_dir()
        );

        let _ = fs::remove_dir_all(daemon.state_root().path());
    }

    #[test]
    fn http_fetch_can_use_ipfs_gateway_transport_for_downloads() {
        let (gateway, fixture) = in_process_http_fixture("image/png", "png-ish");
        let mut registry = PluginRegistry::new();
        registry.register_transport(IpfsGatewayTransport::local(&gateway).expect("local gateway"));
        registry.register_service(super::HttpFetchService);
        let state_root = test_state_root("ipfs-download");
        let download_root = test_download_root("ipfs-download");
        let daemon = BroadwebDaemon::start_with_registry_and_download_root(
            &state_root,
            &download_root,
            Default::default(),
            registry,
        )
        .expect("daemon");
        let response = daemon
            .fetch_http(
                HttpFetchRequest::default_profile("ipfs://bafybeigdyrzt/image.png")
                    .through_transport("ipfs-gateway"),
            )
            .expect("fetch IPFS fixture");
        fixture.finish();

        assert_eq!(
            response.disposition,
            FetchDisposition::Download {
                suggested_filename: "image.png".to_string()
            }
        );
        let download = response.download.expect("download record");
        assert_eq!(download.profile, "default");
        assert_eq!(download.filename, "image.png");
        assert_eq!(download.size_bytes, "png-ish".len());
        assert_eq!(fs::read(&download.path).expect("read download"), b"png-ish");
        assert_eq!(download.path, download_root.join("image.png"));

        let _ = fs::remove_dir_all(state_root);
        let _ = fs::remove_dir_all(download_root);
    }

    #[test]
    fn http_fetch_does_not_record_subresource_downloads() {
        let (gateway, fixture) = in_process_http_fixture("text/css", "body{color:#123}");
        let mut registry = PluginRegistry::new();
        registry.register_transport(IpfsGatewayTransport::local(&gateway).expect("local gateway"));
        registry.register_service(super::HttpFetchService);
        let daemon = BroadwebDaemon::start_with_registry(
            test_state_root("ipfs-subresource"),
            Default::default(),
            registry,
        )
        .expect("daemon");
        let response = daemon
            .fetch_http(
                HttpFetchRequest::default_profile("ipfs://bafybeigdyrzt/style.css")
                    .for_subresource()
                    .through_transport("ipfs-gateway"),
            )
            .expect("fetch IPFS subresource fixture");
        fixture.finish();

        assert_eq!(
            response.disposition,
            FetchDisposition::Download {
                suggested_filename: "style.css".to_string()
            }
        );
        assert_eq!(response.download, None);
        let route = response.route.expect("route info");
        assert_eq!(route.purpose, FetchPurpose::Subresource);
        assert!(
            !daemon
                .state_root()
                .profile_root("default")
                .expect("default profile root")
                .join("temporary/downloads/style.css")
                .exists()
        );

        let _ = fs::remove_dir_all(daemon.state_root().path());
    }

    #[test]
    fn http_fetch_marks_ipfs_gateway_failures_as_error_pages() {
        let (gateway, fixture) = in_process_http_status_fixture(
            "404 Not Found",
            "text/plain; charset=utf-8",
            "missing IPFS content",
        );
        let mut registry = PluginRegistry::new();
        registry.register_transport(IpfsGatewayTransport::local(&gateway).expect("local gateway"));
        registry.register_service(super::HttpFetchService);
        let daemon = BroadwebDaemon::start_with_registry(
            test_state_root("ipfs-gateway-error"),
            Default::default(),
            registry,
        )
        .expect("daemon");
        let response = daemon
            .fetch_http(
                HttpFetchRequest::default_profile("ipfs://bafybeigdyrzt/missing.txt")
                    .through_transport("ipfs-gateway"),
            )
            .expect("fetch IPFS error fixture");
        fixture.finish();

        assert_eq!(response.status_code, 404);
        assert_eq!(
            response.disposition,
            FetchDisposition::ErrorPage { status_code: 404 }
        );
        assert_eq!(response.download, None);
        assert!(response.body_text_lossy().contains("missing IPFS content"));

        let _ = fs::remove_dir_all(daemon.state_root().path());
    }

    #[test]
    fn ipfs_gateway_transport_falls_back_from_unavailable_local_gateway() {
        let missing_gateway = missing_in_process_http_fixture_url();
        let (fallback_gateway, fixture) = in_process_http_fixture(
            "text/html; charset=utf-8",
            "<!doctype html><title>Fallback Gateway</title>",
        );
        let transport = IpfsGatewayTransport::from_gateways(vec![
            IpfsGatewayEndpoint::local(&missing_gateway).expect("missing local gateway"),
            IpfsGatewayEndpoint::local(&fallback_gateway).expect("fallback gateway"),
        ])
        .expect("transport");
        let request = TransportHttpRequest {
            profile: "default".to_string(),
            url: "ipfs://bafybeigdyrzt/index.html".to_string(),
            purpose: FetchPurpose::Navigation,
        };

        let response = transport
            .fetch_http(&request, &ResourceBudget::default())
            .expect("fetch through fallback gateway");
        fixture.finish();

        assert_eq!(response.status_code, 200);
        assert!(response.body_text_lossy().contains("Fallback Gateway"));
        assert_eq!(transport.cached_gateway_base(), fallback_gateway);
    }

    #[test]
    fn ipfs_gateway_transport_reports_fallback_status() {
        let missing_gateway = missing_in_process_http_fixture_url();
        let (fallback_gateway, fixture) = in_process_http_fixture(
            "text/html; charset=utf-8",
            "<!doctype html><title>Status Gateway</title>",
        );
        let status = BroadwebStatusReporter::new();
        let transport = IpfsGatewayTransport::from_gateways_with_status(
            vec![
                IpfsGatewayEndpoint::local(&missing_gateway).expect("missing local gateway"),
                IpfsGatewayEndpoint::local(&fallback_gateway).expect("fallback gateway"),
            ],
            status.clone(),
        )
        .expect("transport");
        let request = TransportHttpRequest {
            profile: "default".to_string(),
            url: "ipfs://bafybeigdyrzt/index.html".to_string(),
            purpose: FetchPurpose::Navigation,
        };

        let response = transport
            .fetch_http(&request, &ResourceBudget::default())
            .expect("fetch through fallback gateway");
        fixture.finish();

        let snapshot = status.snapshot();
        assert_eq!(response.status_code, 200);
        assert_eq!(snapshot.kind, BroadwebStatusKind::Complete);
        assert!(snapshot.message.contains("Loaded via"));
        assert_eq!(snapshot.target.as_deref(), Some(request.url.as_str()));
        assert_eq!(snapshot.gateway.as_deref(), Some(fallback_gateway.as_str()));
        assert!(snapshot.sequence >= 3);
    }

    #[test]
    fn ipfs_gateway_transport_skips_service_worker_gateway_bootstrap() {
        let (service_worker_gateway, service_worker_fixture) = in_process_http_fixture(
            "text/html; charset=utf-8",
            "<!doctype html><title>IPFS Service Worker Gateway</title><h1>Service Worker Required</h1>",
        );
        let (fallback_gateway, fallback_fixture) = in_process_http_fixture(
            "text/html; charset=utf-8",
            "<!doctype html><title>Actual IPFS Page</title>",
        );
        let transport = IpfsGatewayTransport::from_gateways(vec![
            IpfsGatewayEndpoint::local(&service_worker_gateway).expect("service worker gateway"),
            IpfsGatewayEndpoint::local(&fallback_gateway).expect("fallback gateway"),
        ])
        .expect("transport");
        let request = TransportHttpRequest {
            profile: "default".to_string(),
            url: "ipfs://bafybeigdyrzt/index.html".to_string(),
            purpose: FetchPurpose::Navigation,
        };

        let response = transport
            .fetch_http(&request, &ResourceBudget::default())
            .expect("fetch through fallback gateway");
        service_worker_fixture.finish();
        fallback_fixture.finish();

        assert_eq!(response.status_code, 200);
        assert!(response.body_text_lossy().contains("Actual IPFS Page"));
        assert_eq!(transport.cached_gateway_base(), fallback_gateway);
    }

    #[test]
    fn ipfs_gateway_transport_caches_success_and_resets_after_bounded_failure() {
        let missing_gateway = missing_in_process_http_fixture_url();
        let (fallback_gateway, fixture) = in_process_http_fixture(
            "text/html; charset=utf-8",
            "<!doctype html><title>Cached Gateway</title>",
        );
        let transport = IpfsGatewayTransport::from_gateways(vec![
            IpfsGatewayEndpoint::local(&missing_gateway).expect("missing local gateway"),
            IpfsGatewayEndpoint::local(&fallback_gateway).expect("fallback gateway"),
        ])
        .expect("transport");
        let request = TransportHttpRequest {
            profile: "default".to_string(),
            url: "ipfs://bafybeigdyrzt/index.html".to_string(),
            purpose: FetchPurpose::Navigation,
        };

        let response = transport
            .fetch_http(&request, &ResourceBudget::default())
            .expect("fetch through fallback gateway");
        fixture.finish();

        assert_eq!(response.status_code, 200);
        assert_eq!(transport.cached_gateway_base(), fallback_gateway);

        let error = transport
            .fetch_http(&request, &ResourceBudget::default())
            .expect_err("all gateways should be tried once and fail");
        assert!(matches!(error, BroadwebdError::Request(_)));
        assert_eq!(transport.cached_gateway_base(), missing_gateway);
    }

    #[test]
    fn http_fetch_infers_html_from_generic_content_type_and_body() {
        let network = InProcessBroadwebNetwork::new();
        let (address, fixture) = in_process_http_fixture_for_network(
            &network,
            "application/octet-stream",
            "<!doctype html><title>Sniffed HTML Fixture</title><h1>Fetched</h1>",
        );
        let daemon = BroadwebDaemon::start_with_registry(
            test_state_root("sniff-html-body"),
            Default::default(),
            network.fixture_registry(),
        )
        .expect("daemon");
        let response = daemon
            .fetch_http(HttpFetchRequest::default_profile(&address))
            .expect("fetch fixture");
        fixture.finish();

        assert_eq!(
            response.content_type,
            Some("text/html; charset=utf-8".to_string())
        );
        assert_eq!(response.disposition, FetchDisposition::RenderHtml);

        let _ = fs::remove_dir_all(daemon.state_root().path());
    }

    #[test]
    fn http_fetch_infers_html_fragment_from_generic_content_type() {
        let network = InProcessBroadwebNetwork::new();
        let (address, fixture) = in_process_http_fixture_for_network(
            &network,
            "application/octet-stream",
            "<h2>Simple IPFS Fixture</h2>",
        );
        let daemon = BroadwebDaemon::start_with_registry(
            test_state_root("sniff-html-fragment"),
            Default::default(),
            network.fixture_registry(),
        )
        .expect("daemon");
        let response = daemon
            .fetch_http(HttpFetchRequest::default_profile(&address))
            .expect("fetch fixture");
        fixture.finish();

        assert_eq!(
            response.content_type,
            Some("text/html; charset=utf-8".to_string())
        );
        assert_eq!(response.disposition, FetchDisposition::RenderHtml);

        let _ = fs::remove_dir_all(daemon.state_root().path());
    }

    #[test]
    fn http_fetch_infers_html_from_generic_content_type_and_path() {
        let network = InProcessBroadwebNetwork::new();
        let (address, fixture) = in_process_http_fixture_for_network(
            &network,
            "application/octet-stream",
            "<h1>IPFS HTML Path</h1>",
        );
        let address = format!("{address}/index.html");
        let daemon = BroadwebDaemon::start_with_registry(
            test_state_root("sniff-html-path"),
            Default::default(),
            network.fixture_registry(),
        )
        .expect("daemon");
        let response = daemon
            .fetch_http(HttpFetchRequest::default_profile(&address))
            .expect("fetch fixture");
        fixture.finish();

        assert_eq!(
            response.content_type,
            Some("text/html; charset=utf-8".to_string())
        );
        assert_eq!(response.disposition, FetchDisposition::RenderHtml);

        let _ = fs::remove_dir_all(daemon.state_root().path());
    }

    #[test]
    fn non_html_http_fetch_is_marked_as_download() {
        let network = InProcessBroadwebNetwork::new();
        let (address, fixture) =
            in_process_http_fixture_for_network(&network, "application/octet-stream", "binary-ish");
        let state_root = test_state_root("download");
        let download_root = test_download_root("download");
        let daemon = BroadwebDaemon::start_with_registry_and_download_root(
            &state_root,
            &download_root,
            Default::default(),
            network.fixture_registry(),
        )
        .expect("daemon");
        let response = daemon
            .fetch_http(HttpFetchRequest::default_profile(&address))
            .expect("fetch fixture");
        fixture.finish();

        assert_eq!(
            response.disposition,
            FetchDisposition::Download {
                suggested_filename: "download".to_string()
            }
        );
        let download = response.download.expect("download record");
        assert_eq!(download.path, download_root.join("download"));
        assert_eq!(
            fs::read(&download.path).expect("read download"),
            b"binary-ish"
        );

        let _ = fs::remove_dir_all(state_root);
        let _ = fs::remove_dir_all(download_root);
    }

    #[test]
    fn content_disposition_attachment_sets_download_filename() {
        let network = InProcessBroadwebNetwork::new();
        let (address, fixture) = in_process_http_fixture_with_headers_for_network(
            &network,
            "text/html; charset=utf-8",
            &[r#"Content-Disposition: attachment; filename="report.html""#],
            "<!doctype html><title>Attachment</title>",
        );
        let state_root = test_state_root("attachment-download");
        let download_root = test_download_root("attachment-download");
        let daemon = BroadwebDaemon::start_with_registry_and_download_root(
            &state_root,
            &download_root,
            Default::default(),
            network.fixture_registry(),
        )
        .expect("daemon");
        let response = daemon
            .fetch_http(HttpFetchRequest::default_profile(&address))
            .expect("fetch attachment fixture");
        fixture.finish();

        assert_eq!(
            response.disposition,
            FetchDisposition::Download {
                suggested_filename: "report.html".to_string()
            }
        );
        let download = response.download.expect("download record");
        assert_eq!(download.filename, "report.html");
        assert_eq!(download.path, download_root.join("report.html"));

        let _ = fs::remove_dir_all(state_root);
        let _ = fs::remove_dir_all(download_root);
    }

    #[test]
    fn explicit_download_request_saves_html_with_requested_filename() {
        let network = InProcessBroadwebNetwork::new();
        let (address, fixture) = in_process_http_fixture_for_network(
            &network,
            "text/html; charset=utf-8",
            "<!doctype html><title>Download Me</title>",
        );
        let state_root = test_state_root("explicit-download");
        let download_root = test_download_root("explicit-download");
        let daemon = BroadwebDaemon::start_with_registry_and_download_root(
            &state_root,
            &download_root,
            Default::default(),
            network.fixture_registry(),
        )
        .expect("daemon");
        let response = daemon
            .fetch_http(HttpFetchRequest::default_profile(&address).download_as("page.html"))
            .expect("fetch explicit download fixture");
        fixture.finish();

        assert_eq!(
            response.disposition,
            FetchDisposition::Download {
                suggested_filename: "page.html".to_string()
            }
        );
        let download = response.download.expect("download record");
        assert_eq!(download.filename, "page.html");
        assert_eq!(download.path, download_root.join("page.html"));
        assert!(
            fs::read_to_string(&download.path)
                .expect("read explicit download")
                .contains("Download Me")
        );

        let _ = fs::remove_dir_all(state_root);
        let _ = fs::remove_dir_all(download_root);
    }

    #[test]
    fn response_size_budget_is_enforced() {
        let network = InProcessBroadwebNetwork::new();
        let (address, fixture) =
            in_process_http_fixture_for_network(&network, "text/html", "0123456789");
        let budget = ResourceBudget {
            max_http_response_bytes: 4,
            ..ResourceBudget::default()
        };
        let daemon = BroadwebDaemon::start_with_registry(
            test_state_root("budget"),
            budget,
            network.fixture_registry(),
        )
        .expect("daemon");
        let error = daemon
            .fetch_http(HttpFetchRequest::default_profile(&address))
            .expect_err("budget exceeded");
        fixture.finish();

        assert!(matches!(error, BroadwebdError::ResponseTooLarge { .. }));

        let _ = fs::remove_dir_all(daemon.state_root().path());
    }

    #[test]
    fn direct_http_rejects_non_http_schemes() {
        let transport = super::DirectHttpTransport;
        let request = TransportHttpRequest {
            profile: "default".to_string(),
            url: "ipfs://bafybeigdyrzt".to_string(),
            purpose: FetchPurpose::Navigation,
        };

        assert!(matches!(
            transport.fetch_http(&request, &ResourceBudget::default()),
            Err(BroadwebdError::UnsupportedRequest(_))
        ));
    }

    #[test]
    fn default_registry_routes_onion_hosts_to_tor_before_direct_http() {
        let registry = PluginRegistry::with_default_http();

        assert_eq!(
            registry
                .resolve_http_transport("http://example.onion/")
                .expect("resolve onion HTTP"),
            TOR_ARTI_HTTP_PLUGIN
        );
        assert_eq!(
            registry
                .resolve_http_transport("tor+http://example.onion/")
                .expect("resolve Tor HTTP"),
            TOR_ARTI_HTTP_PLUGIN
        );
        assert_eq!(
            registry
                .resolve_http_transport("https://example.onion/")
                .expect("resolve onion HTTPS"),
            TOR_ARTI_HTTP_PLUGIN
        );
        assert_eq!(
            registry
                .resolve_http_transport("tor+https://example.onion/")
                .expect("resolve Tor HTTPS"),
            TOR_ARTI_HTTP_PLUGIN
        );
        assert_eq!(
            registry
                .resolve_http_transport("https://example.com/")
                .expect("resolve ordinary HTTPS"),
            DIRECT_HTTP_PLUGIN
        );
    }

    #[test]
    fn tor_service_registers_arti_transport() {
        let service = TorService;
        let mut registry = PluginRegistry::new();
        let installs = service.install_adapter_plugins(&mut registry);
        let metadata = service.metadata();

        assert_eq!(installs.len(), 1);
        assert_eq!(installs[0].metadata.id, TOR_ARTI_HTTP_PLUGIN);
        assert_eq!(metadata.id, TOR_PROTOCOL_SERVICE);
        assert_eq!(
            metadata.dependencies,
            vec![TOR_ARTI_HTTP_PLUGIN.to_string()]
        );
        assert!(
            metadata
                .capabilities
                .iter()
                .any(|capability| capability == "onion")
        );
    }

    #[test]
    fn tor_http_urls_are_normalized_without_direct_dns() {
        let url = Url::parse("http://Example.Onion/docs?a=1#client-only").unwrap();
        assert_eq!(
            tor_url_from_http_url(&url).expect("normalize onion URL"),
            Some("tor+http://example.onion/docs?a=1".to_string())
        );

        let target = tor_http_target("tor+http://example.onion/docs?a=1").expect("Tor target");
        assert_eq!(target.host, "example.onion");
        assert_eq!(target.port, 80);
        assert_eq!(target.path_and_query, "/docs?a=1");
    }

    #[test]
    fn http_fetch_annotates_tor_route_metadata_without_live_tor() {
        let mut registry = PluginRegistry::new();
        registry.register_protocol_service(TorService);
        registry.register_transport(FixtureTransport::new(TOR_ARTI_HTTP_PLUGIN, "Tor fixture"));
        registry.register_service(super::HttpFetchService);
        let state_root = test_state_root("tor-route-context");
        let download_root = test_download_root("tor-route-context");
        let daemon = BroadwebDaemon::start_with_registry_and_download_root(
            &state_root,
            &download_root,
            Default::default(),
            registry,
        )
        .expect("daemon");

        let response = daemon
            .fetch_http(
                HttpFetchRequest::new("research", "http://example.onion/docs")
                    .download_as("docs.html"),
            )
            .expect("fetch Tor fixture");

        assert_eq!(
            response.disposition,
            FetchDisposition::Download {
                suggested_filename: "docs.html".to_string()
            }
        );
        assert!(response.body_text_lossy().contains("Tor fixture"));
        let route = response.route.expect("route info");
        assert_eq!(route.profile, "research");
        assert_eq!(route.transport_id, TOR_ARTI_HTTP_PLUGIN);
        assert_eq!(route.privacy_boundary, "test fixture transport");
        assert_eq!(route.purpose, FetchPurpose::Navigation);

        let _ = fs::remove_dir_all(state_root);
        let _ = fs::remove_dir_all(download_root);
    }

    #[test]
    fn http_fetch_fails_closed_when_tor_transport_is_unavailable() {
        let mut registry = PluginRegistry::new();
        registry.register_transport(super::DirectHttpTransport);
        registry.register_protocol_service(TorService);
        registry
            .remove_transport(TOR_ARTI_HTTP_PLUGIN)
            .expect("remove Tor transport");
        registry.register_service(super::HttpFetchService);

        assert_eq!(
            registry
                .resolve_http_transport("https://example.onion/")
                .expect("resolve onion HTTPS"),
            TOR_ARTI_HTTP_PLUGIN
        );
        assert!(registry.plugin_statuses().iter().any(|status| {
            status.metadata.id == TOR_PROTOCOL_SERVICE
                && matches!(status.health, PluginHealth::Degraded(_))
        }));

        let daemon = BroadwebDaemon::start_with_registry(
            test_state_root("tor-fail-closed"),
            Default::default(),
            registry,
        )
        .expect("daemon");
        let error = daemon
            .fetch_http(HttpFetchRequest::default_profile("http://example.onion/"))
            .expect_err("missing Tor transport should fail closed");

        assert!(matches!(
            error,
            BroadwebdError::MissingPlugin(plugin) if plugin == TOR_ARTI_HTTP_PLUGIN
        ));

        let _ = fs::remove_dir_all(daemon.state_root().path());
    }

    #[test]
    fn ipfs_urls_can_be_mapped_to_local_gateway_http() {
        assert_eq!(
            ipfs_gateway_http_url("ipfs://bafybeigdyrzt/index.html", "http://127.0.0.1:8080")
                .expect("gateway url"),
            "http://127.0.0.1:8080/ipfs/bafybeigdyrzt/index.html"
        );
        assert_eq!(
            ipfs_gateway_http_url("ipns://example.net/docs?a=1", "http://127.0.0.1:8080/")
                .expect("gateway url"),
            "http://127.0.0.1:8080/ipns/example.net/docs?a=1"
        );
    }

    #[test]
    fn ipfs_gateway_urls_preserve_cidv0_case() {
        assert_eq!(
            ipfs_gateway_http_url(
                "ipfs://QmUKwop8CmB4ictvQyCJQru97NRVakJFVWpV74guJ89tcb/index.html?filename=Index.html#top",
                "http://127.0.0.1:8080",
            )
            .expect("CIDv0 gateway URL"),
            "http://127.0.0.1:8080/ipfs/QmUKwop8CmB4ictvQyCJQru97NRVakJFVWpV74guJ89tcb/index.html?filename=Index.html"
        );
    }

    #[test]
    fn ipfs_service_registers_gateway_transport_from_config() {
        let service = IpfsService::new(
            IpfsConfig::new("http://127.0.0.1:9090").expect("local gateway config"),
        );
        let mut registry = PluginRegistry::new();
        let installs = service.install_adapter_plugins(&mut registry);

        assert!(service.config().allow_public_gateway_fallback());
        assert!(!service.config().public_gateway_fallbacks().is_empty());
        assert_eq!(installs.len(), 1);
        assert_eq!(installs[0].metadata.id, IPFS_GATEWAY_PLUGIN);
        assert!(!installs[0].replaced_existing);
        assert!(
            registry
                .list_transports()
                .iter()
                .any(|metadata| metadata.id == IPFS_GATEWAY_PLUGIN)
        );
    }

    #[test]
    fn ipfs_config_rejects_public_gateway_without_policy() {
        assert!(matches!(
            IpfsConfig::new("https://ipfs.io"),
            Err(BroadwebdError::UnsupportedRequest(_))
        ));
    }

    #[test]
    fn ipfs_config_requires_numeric_loopback_for_local_gateway() {
        assert!(IpfsConfig::new("http://127.0.0.1:8080").is_ok());
        assert!(IpfsConfig::new("http://[::1]:8080").is_ok());
        assert!(matches!(
            IpfsConfig::new("http://localhost:8080"),
            Err(BroadwebdError::UnsupportedRequest(_))
        ));
    }

    #[test]
    fn ipfs_config_accepts_explicit_public_gateway() {
        let config = IpfsConfig::with_public_gateway("https://ipfs.io")
            .expect("explicit public gateway config");

        assert_eq!(config.gateway_base(), "https://ipfs.io");
        assert_eq!(config.gateway_scope(), IpfsGatewayScope::Public);
        assert!(config.uses_public_gateway());
        assert!(config.allow_public_gateway_fallback());
        assert!(
            config
                .public_gateway_fallbacks()
                .iter()
                .all(|gateway| gateway.base_url() != "https://ipfs.io")
        );
    }

    #[test]
    fn ipfs_config_options_default_to_local_gateway() {
        let config = IpfsConfig::from_options(None, None).expect("default IPFS config");

        assert_eq!(config.gateway_base(), super::DEFAULT_IPFS_GATEWAY);
        assert_eq!(config.gateway_scope(), IpfsGatewayScope::Local);
    }

    #[test]
    fn ipfs_config_options_reject_public_gateway_without_public_scope() {
        assert!(matches!(
            IpfsConfig::from_options(Some("https://ipfs.io"), None),
            Err(BroadwebdError::UnsupportedRequest(_))
        ));
    }

    #[test]
    fn ipfs_config_options_accept_public_gateway_with_public_scope() {
        let config = IpfsConfig::from_options(Some("https://ipfs.io"), Some("public"))
            .expect("public IPFS gateway config");

        assert_eq!(config.gateway_base(), "https://ipfs.io");
        assert_eq!(config.gateway_scope(), IpfsGatewayScope::Public);
    }

    #[test]
    fn ipfs_config_options_use_default_public_gateway_for_public_scope_without_gateway() {
        let config = IpfsConfig::from_options(None, Some("public"))
            .expect("default public IPFS gateway config");

        assert_eq!(config.gateway_base(), super::DEFAULT_PUBLIC_IPFS_GATEWAY);
        assert_eq!(config.gateway_scope(), IpfsGatewayScope::Public);
        assert!(config.uses_public_gateway());
    }

    #[test]
    fn ipfs_runtime_options_default_to_gateway_transport() {
        let config =
            IpfsConfig::from_runtime_options(None, None, None, None).expect("default IPFS config");

        assert_eq!(config.transport(), IpfsTransportKind::Gateway);
        assert_eq!(config.gateway_base(), super::DEFAULT_IPFS_GATEWAY);
        assert_eq!(config.http_transport_id(), IPFS_GATEWAY_PLUGIN);
    }

    #[test]
    fn ipfs_runtime_options_accept_explicit_kubo_transport() {
        let config = IpfsConfig::from_runtime_options(None, None, Some("kubo-rpc"), None)
            .expect("Kubo RPC config");

        assert_eq!(config.transport(), IpfsTransportKind::KuboRpc);
        assert_eq!(config.http_transport_id(), IPFS_KUBO_RPC_PLUGIN);
        assert_eq!(
            config
                .kubo_rpc_endpoint()
                .expect("Kubo RPC endpoint")
                .api_base_url(),
            DEFAULT_IPFS_KUBO_RPC_API
        );
    }

    #[test]
    fn ipfs_runtime_options_accept_kubo_rpc_endpoint_override() {
        let config = IpfsConfig::from_runtime_options(
            None,
            None,
            Some("local-kubo-rpc"),
            Some("http://127.0.0.1:5050"),
        )
        .expect("Kubo RPC config");

        assert_eq!(config.transport(), IpfsTransportKind::KuboRpc);
        assert_eq!(
            config
                .kubo_rpc_endpoint()
                .expect("Kubo RPC endpoint")
                .api_base_url(),
            "http://127.0.0.1:5050"
        );
    }

    #[test]
    fn ipfs_runtime_options_select_kubo_when_rpc_endpoint_is_set() {
        let config =
            IpfsConfig::from_runtime_options(None, None, None, Some("http://127.0.0.1:5050"))
                .expect("Kubo RPC config");

        assert_eq!(config.transport(), IpfsTransportKind::KuboRpc);
        assert_eq!(config.http_transport_id(), IPFS_KUBO_RPC_PLUGIN);
    }

    #[test]
    fn ipfs_runtime_options_reject_kubo_mixed_with_gateway_policy() {
        assert!(matches!(
            IpfsConfig::from_runtime_options(
                Some("http://127.0.0.1:8080"),
                None,
                Some("kubo-rpc"),
                None
            ),
            Err(BroadwebdError::UnsupportedRequest(error))
                if error.contains(SLATE_IPFS_TRANSPORT_ENV)
        ));
    }

    #[test]
    fn ipfs_public_gateway_transport_exposes_public_privacy_boundary() {
        let transport =
            IpfsGatewayTransport::public("https://ipfs.io").expect("public gateway transport");
        let metadata = transport.metadata();

        assert_eq!(transport.gateway_scope(), IpfsGatewayScope::Public);
        assert!(
            metadata
                .capabilities
                .iter()
                .any(|capability| capability == "public-gateway")
        );
        assert!(metadata.privacy_boundary.contains("public IPFS gateway"));
        assert_eq!(
            ipfs_gateway_http_url("ipfs://bafybeigdyrzt/index.html", transport.gateway_base())
                .expect("gateway url"),
            "https://ipfs.io/ipfs/bafybeigdyrzt/index.html"
        );
    }

    #[test]
    fn ipfs_service_registers_public_gateway_transport_from_explicit_config() {
        let service = IpfsService::new(
            IpfsConfig::with_public_gateway("https://ipfs.io")
                .expect("explicit public gateway config"),
        );
        let mut registry = PluginRegistry::new();
        let installs = service.install_adapter_plugins(&mut registry);
        let metadata = service.metadata();

        assert!(service.config().uses_public_gateway());
        assert_eq!(installs.len(), 1);
        assert_eq!(installs[0].metadata.id, IPFS_GATEWAY_PLUGIN);
        assert!(
            installs[0]
                .metadata
                .capabilities
                .iter()
                .any(|capability| capability == "public-gateway")
        );
        assert!(
            metadata
                .capabilities
                .iter()
                .any(|capability| capability == "public-gateway")
        );
        assert!(
            registry
                .list_transports()
                .iter()
                .any(|metadata| metadata.id == IPFS_GATEWAY_PLUGIN)
        );
    }

    #[test]
    fn ipfs_kubo_urls_map_to_local_rpc_cat_endpoint() {
        assert_eq!(
            ipfs_kubo_cat_url(
                "ipfs://bafybeigdyrzt/index.html?a=1",
                "http://127.0.0.1:5001"
            )
            .expect("Kubo cat url"),
            "http://127.0.0.1:5001/api/v0/cat?arg=%2Fipfs%2Fbafybeigdyrzt%2Findex.html"
        );
        assert_eq!(
            ipfs_kubo_cat_url("ipns://example.net/docs/app.js", "http://127.0.0.1:5001/")
                .expect("Kubo cat url"),
            "http://127.0.0.1:5001/api/v0/cat?arg=%2Fipns%2Fexample.net%2Fdocs%2Fapp.js"
        );
    }

    #[test]
    fn ipfs_kubo_urls_preserve_cidv0_case() {
        assert_eq!(
            ipfs_kubo_cat_url(
                "ipfs://QmUKwop8CmB4ictvQyCJQru97NRVakJFVWpV74guJ89tcb/index.html?a=1",
                "http://127.0.0.1:5001",
            )
            .expect("CIDv0 Kubo cat URL"),
            "http://127.0.0.1:5001/api/v0/cat?arg=%2Fipfs%2FQmUKwop8CmB4ictvQyCJQru97NRVakJFVWpV74guJ89tcb%2Findex.html"
        );
    }

    #[test]
    fn ipfs_kubo_profile_sync_rpc_urls_map_to_local_endpoints() {
        let object_id = "bafybeigdyrztprofileobject";

        assert_eq!(
            ipfs_kubo_profile_sync_add_url("http://127.0.0.1:5001").expect("profile sync add URL"),
            "http://127.0.0.1:5001/api/v0/add?cid-version=1&raw-leaves=true&pin=false"
        );
        assert_eq!(
            ipfs_kubo_profile_sync_pin_add_url(object_id, "http://127.0.0.1:5001")
                .expect("profile sync pin add URL"),
            "http://127.0.0.1:5001/api/v0/pin/add?arg=bafybeigdyrztprofileobject&recursive=true"
        );
        assert_eq!(
            ipfs_kubo_profile_sync_pin_rm_url(object_id, "http://127.0.0.1:5001")
                .expect("profile sync pin remove URL"),
            "http://127.0.0.1:5001/api/v0/pin/rm?arg=bafybeigdyrztprofileobject&recursive=true"
        );
        assert_eq!(
            ipfs_kubo_profile_sync_pin_ls_url(object_id, "http://127.0.0.1:5001")
                .expect("profile sync pin status URL"),
            "http://127.0.0.1:5001/api/v0/pin/ls?arg=bafybeigdyrztprofileobject&type=recursive"
        );
        assert_eq!(
            ipfs_kubo_profile_sync_name_publish_url(
                "settings-latest",
                object_id,
                "http://127.0.0.1:5001"
            )
            .expect("profile sync IPNS publish URL"),
            "http://127.0.0.1:5001/api/v0/name/publish?arg=%2Fipfs%2Fbafybeigdyrztprofileobject&key=settings-latest&allow-offline=true"
        );
        assert_eq!(
            ipfs_kubo_profile_sync_name_resolve_url("k51syncroot", "http://127.0.0.1:5001")
                .expect("profile sync IPNS resolve URL"),
            "http://127.0.0.1:5001/api/v0/name/resolve?arg=%2Fipns%2Fk51syncroot&recursive=false"
        );
    }

    #[test]
    fn ipfs_kubo_profile_sync_rpc_urls_reject_path_shaped_object_ids() {
        assert!(matches!(
            ipfs_kubo_profile_sync_pin_add_url("../settings", "http://127.0.0.1:5001"),
            Err(BroadwebdError::InvalidUrl(_))
        ));
        assert!(matches!(
            ipfs_kubo_profile_sync_name_publish_url(
                "settings-latest",
                "bafy/object",
                "http://127.0.0.1:5001"
            ),
            Err(BroadwebdError::InvalidUrl(_))
        ));
        assert!(matches!(
            ipfs_kubo_profile_sync_add_url("https://ipfs.example.test:5001"),
            Err(BroadwebdError::UnsupportedRequest(_))
        ));
    }

    #[test]
    fn ipfs_kubo_profile_sync_rpc_plans_profile_sync_requests() {
        let rpc =
            IpfsKuboProfileSyncRpc::local("http://127.0.0.1:5001").expect("Kubo profile sync RPC");
        let object_id = "bafybeigdyrztprofileobject";

        let put = rpc
            .put_encrypted_object_request()
            .expect("plan Kubo profile sync object add");
        assert_eq!(
            put.operation(),
            IpfsKuboProfileSyncOperation::PutEncryptedObject
        );
        assert_eq!(
            put.url(),
            "http://127.0.0.1:5001/api/v0/add?cid-version=1&raw-leaves=true&pin=false"
        );

        let retain = rpc
            .retain_object_request(object_id)
            .expect("plan Kubo profile sync pin");
        assert_eq!(
            retain.operation(),
            IpfsKuboProfileSyncOperation::RetainObject
        );
        assert_eq!(
            retain.url(),
            "http://127.0.0.1:5001/api/v0/pin/add?arg=bafybeigdyrztprofileobject&recursive=true"
        );

        let release = rpc
            .release_object_request(object_id)
            .expect("plan Kubo profile sync unpin");
        assert_eq!(
            release.operation(),
            IpfsKuboProfileSyncOperation::ReleaseObject
        );
        assert_eq!(
            release.url(),
            "http://127.0.0.1:5001/api/v0/pin/rm?arg=bafybeigdyrztprofileobject&recursive=true"
        );

        let verify = rpc
            .verify_retained_object_request(object_id)
            .expect("plan Kubo profile sync pin status");
        assert_eq!(
            verify.operation(),
            IpfsKuboProfileSyncOperation::VerifyRetainedObject
        );
        assert_eq!(
            verify.url(),
            "http://127.0.0.1:5001/api/v0/pin/ls?arg=bafybeigdyrztprofileobject&type=recursive"
        );

        let publish = rpc
            .publish_root_request("settings-latest", object_id)
            .expect("plan Kubo profile sync IPNS publish");
        assert_eq!(
            publish.operation(),
            IpfsKuboProfileSyncOperation::PublishRoot
        );
        assert_eq!(
            publish.url(),
            "http://127.0.0.1:5001/api/v0/name/publish?arg=%2Fipfs%2Fbafybeigdyrztprofileobject&key=settings-latest&allow-offline=true"
        );

        let resolve = rpc
            .resolve_root_request("k51syncroot")
            .expect("plan Kubo profile sync IPNS resolve");
        assert_eq!(
            resolve.operation(),
            IpfsKuboProfileSyncOperation::ResolveRoot
        );
        assert_eq!(
            resolve.url(),
            "http://127.0.0.1:5001/api/v0/name/resolve?arg=%2Fipns%2Fk51syncroot&recursive=false"
        );
    }

    #[test]
    fn ipfs_kubo_profile_sync_rpc_executes_over_internal_transport_shim() {
        let object_id = "bafybeigdyrztprofileobject";
        let fixture = InProcessBroadwebNetwork::new().kubo_rpc_response(InternalKuboRpcResponse {
            status_code: 200,
            content_type: "application/json".to_string(),
            body: br#"{"Name":"profile-object","Hash":"bafybeigdyrztprofileobject","Size":"128"}"#
                .to_vec(),
        });
        let rpc = IpfsKuboProfileSyncRpc::local(fixture.base_url())
            .expect("fixture Kubo profile sync RPC");
        let request = rpc
            .put_encrypted_object_request()
            .expect("plan fixture Kubo object add");
        let response = InternalKuboRpcTransportShim::execute_profile_sync_request(
            &request,
            &ResourceBudget::default(),
        )
        .expect("execute Kubo profile sync request through internal transport");

        assert_eq!(
            ipfs_kubo_profile_sync_added_object_id(response.body.as_slice())
                .expect("parse fixture Kubo add response"),
            object_id
        );
        assert_eq!(
            fixture.finish(),
            vec!["POST /api/v0/add?cid-version=1&raw-leaves=true&pin=false HTTP/1.1"]
        );
    }

    #[test]
    fn ipfs_kubo_profile_sync_fixture_puts_encrypted_object() {
        let object_id = "bafybeigdyrztprofileobject";
        let fixture = InProcessBroadwebNetwork::new().kubo_rpc_response(InternalKuboRpcResponse {
            status_code: 200,
            content_type: "application/json".to_string(),
            body: br#"{"Name":"profile-object","Hash":"bafybeigdyrztprofileobject","Size":"128"}"#
                .to_vec(),
        });
        let rpc = IpfsKuboProfileSyncRpc::local(fixture.base_url())
            .expect("fixture Kubo profile sync RPC");

        assert_eq!(
            rpc.put_encrypted_object_via_internal_transport(
                b"encrypted slate-settings snapshot",
                &ResourceBudget::default()
            )
            .expect("put encrypted object through fixture Kubo RPC"),
            object_id
        );
        assert_eq!(
            fixture.finish(),
            vec!["POST /api/v0/add?cid-version=1&raw-leaves=true&pin=false HTTP/1.1"]
        );
    }

    #[test]
    fn ipfs_kubo_profile_sync_fixture_rejects_oversized_object_before_request() {
        let fixture = InProcessBroadwebNetwork::new().kubo_rpc_response(InternalKuboRpcResponse {
            status_code: 200,
            content_type: "application/json".to_string(),
            body: br#"{"Name":"profile-object","Hash":"bafybeigdyrztprofileobject","Size":"128"}"#
                .to_vec(),
        });
        let rpc = IpfsKuboProfileSyncRpc::local(fixture.base_url())
            .expect("fixture Kubo profile sync RPC");
        let budget = ResourceBudget {
            max_profile_sync_object_bytes: 4,
            ..ResourceBudget::default()
        };

        assert!(matches!(
            rpc.put_encrypted_object_via_internal_transport(b"encrypted object", &budget),
            Err(BroadwebdError::ResponseTooLarge {
                limit: 4,
                actual: 16
            })
        ));
        assert!(fixture.finish().is_empty());
    }

    #[test]
    fn ipfs_kubo_profile_sync_fixture_rejects_kubo_error_status() {
        let fixture = InProcessBroadwebNetwork::new().kubo_rpc_response(InternalKuboRpcResponse {
            status_code: 500,
            content_type: "application/json".to_string(),
            body: br#"{"Message":"add failed"}"#.to_vec(),
        });
        let rpc = IpfsKuboProfileSyncRpc::local(fixture.base_url())
            .expect("fixture Kubo profile sync RPC");

        assert!(matches!(
            rpc.put_encrypted_object_via_internal_transport(b"encrypted object", &ResourceBudget::default()),
            Err(BroadwebdError::Request(message))
                if message == "Kubo profile-sync add returned HTTP status 500"
        ));
        assert_eq!(
            fixture.finish(),
            vec!["POST /api/v0/add?cid-version=1&raw-leaves=true&pin=false HTTP/1.1"]
        );
    }

    #[test]
    fn ipfs_kubo_profile_sync_fixture_gets_encrypted_object() {
        let object_id = "bafybeigdyrztprofileobject";
        let fixture = InProcessBroadwebNetwork::new().kubo_rpc_response(InternalKuboRpcResponse {
            status_code: 200,
            content_type: "application/octet-stream".to_string(),
            body: b"encrypted slate-settings snapshot".to_vec(),
        });
        let rpc = IpfsKuboProfileSyncRpc::local(fixture.base_url())
            .expect("fixture Kubo profile sync RPC");

        assert_eq!(
            rpc.get_encrypted_object_via_internal_transport(object_id, &ResourceBudget::default())
                .expect("get encrypted object through fixture Kubo RPC"),
            b"encrypted slate-settings snapshot".to_vec()
        );
        assert_eq!(
            fixture.finish(),
            vec!["POST /api/v0/cat?arg=%2Fipfs%2Fbafybeigdyrztprofileobject HTTP/1.1"]
        );
    }

    #[test]
    fn ipfs_kubo_profile_sync_fixture_retains_verifies_and_releases_object() {
        let object_id = "bafybeigdyrztprofileobject";
        let fixture = InProcessBroadwebNetwork::new().kubo_rpc_sequence(vec![
            InternalKuboRpcResponse {
                status_code: 200,
                content_type: "application/json".to_string(),
                body: br#"{"Pins":["bafybeigdyrztprofileobject"]}"#.to_vec(),
            },
            InternalKuboRpcResponse {
                status_code: 200,
                content_type: "application/json".to_string(),
                body: br#"{"Keys":{"bafybeigdyrztprofileobject":{"Type":"recursive"}}}"#.to_vec(),
            },
            InternalKuboRpcResponse {
                status_code: 200,
                content_type: "application/json".to_string(),
                body: br#"{"Pins":["bafybeigdyrztprofileobject"]}"#.to_vec(),
            },
        ]);
        let rpc = IpfsKuboProfileSyncRpc::local(fixture.base_url())
            .expect("fixture Kubo profile sync RPC");

        rpc.retain_object_via_internal_transport(object_id, &ResourceBudget::default())
            .expect("retain profile object through fixture Kubo RPC");
        assert!(
            rpc.verify_retained_object_via_internal_transport(
                object_id,
                &ResourceBudget::default()
            )
            .expect("verify retained profile object through fixture Kubo RPC")
        );
        rpc.release_object_via_internal_transport(object_id, &ResourceBudget::default())
            .expect("release profile object through fixture Kubo RPC");

        assert_eq!(
            fixture.finish(),
            vec![
                "POST /api/v0/pin/add?arg=bafybeigdyrztprofileobject&recursive=true HTTP/1.1",
                "POST /api/v0/pin/ls?arg=bafybeigdyrztprofileobject&type=recursive HTTP/1.1",
                "POST /api/v0/pin/rm?arg=bafybeigdyrztprofileobject&recursive=true HTTP/1.1",
            ]
        );
    }

    #[test]
    fn ipfs_kubo_profile_sync_fixture_verify_reports_unretained_object() {
        let object_id = "bafybeigdyrztprofileobject";
        let fixture = InProcessBroadwebNetwork::new().kubo_rpc_response(InternalKuboRpcResponse {
            status_code: 200,
            content_type: "application/json".to_string(),
            body: br#"{"Keys":{}}"#.to_vec(),
        });
        let rpc = IpfsKuboProfileSyncRpc::local(fixture.base_url())
            .expect("fixture Kubo profile sync RPC");

        assert!(
            !rpc.verify_retained_object_via_internal_transport(
                object_id,
                &ResourceBudget::default()
            )
            .expect("verify missing retained profile object through fixture Kubo RPC")
        );
        assert_eq!(
            fixture.finish(),
            vec!["POST /api/v0/pin/ls?arg=bafybeigdyrztprofileobject&type=recursive HTTP/1.1"]
        );
    }

    #[test]
    fn ipfs_kubo_profile_sync_fixture_publishes_and_resolves_root() {
        let object_id = "bafybeigdyrztprofileobject";
        let fixture = InProcessBroadwebNetwork::new().kubo_rpc_sequence(vec![
            InternalKuboRpcResponse {
                status_code: 200,
                content_type: "application/json".to_string(),
                body: br#"{"Name":"k51syncroot","Value":"/ipfs/bafybeigdyrztprofileobject"}"#
                    .to_vec(),
            },
            InternalKuboRpcResponse {
                status_code: 200,
                content_type: "application/json".to_string(),
                body: br#"{"Path":"/ipfs/bafybeigdyrztprofileobject"}"#.to_vec(),
            },
        ]);
        let rpc = IpfsKuboProfileSyncRpc::local(fixture.base_url())
            .expect("fixture Kubo profile sync RPC");

        assert_eq!(
            rpc.publish_root_via_internal_transport(
                "settings-latest",
                object_id,
                &ResourceBudget::default()
            )
            .expect("publish profile root through fixture Kubo RPC"),
            object_id
        );
        assert_eq!(
            rpc.resolve_root_via_internal_transport("k51syncroot", &ResourceBudget::default())
                .expect("resolve profile root through fixture Kubo RPC"),
            object_id
        );
        assert_eq!(
            fixture.finish(),
            vec![
                "POST /api/v0/name/publish?arg=%2Fipfs%2Fbafybeigdyrztprofileobject&key=settings-latest&allow-offline=true HTTP/1.1",
                "POST /api/v0/name/resolve?arg=%2Fipns%2Fk51syncroot&recursive=false HTTP/1.1",
            ]
        );
    }

    #[test]
    fn ipfs_kubo_profile_sync_fixture_rejects_mismatched_published_root() {
        let object_id = "bafybeigdyrztprofileobject";
        let fixture = InProcessBroadwebNetwork::new().kubo_rpc_response(InternalKuboRpcResponse {
            status_code: 200,
            content_type: "application/json".to_string(),
            body: br#"{"Name":"k51syncroot","Value":"/ipfs/bafybeigdyrztdifferentobject"}"#
                .to_vec(),
        });
        let rpc = IpfsKuboProfileSyncRpc::local(fixture.base_url())
            .expect("fixture Kubo profile sync RPC");

        assert!(matches!(
            rpc.publish_root_via_internal_transport("settings-latest", object_id, &ResourceBudget::default()),
            Err(BroadwebdError::Request(message))
                if message == "Kubo profile-sync name/publish returned bafybeigdyrztdifferentobject, expected bafybeigdyrztprofileobject"
        ));
        assert_eq!(
            fixture.finish(),
            vec![
                "POST /api/v0/name/publish?arg=%2Fipfs%2Fbafybeigdyrztprofileobject&key=settings-latest&allow-offline=true HTTP/1.1"
            ]
        );
    }

    #[test]
    fn kubo_profile_sync_fixture_reports_protocol_semantics_over_internal_transport() {
        let network = InProcessBroadwebNetwork::new();
        let fixture = network.kubo_rpc_response(InternalKuboRpcResponse {
            status_code: 200,
            content_type: "application/json".to_string(),
            body: br#"{"Name":"profile-object","Hash":"bafybeigdyrztprofileobject","Size":"128"}"#
                .to_vec(),
        });
        let state_root = std::env::temp_dir().join(format!(
            "slate-broadwebd-fixture-metadata-profile-sync-{}",
            std::process::id()
        ));
        let daemon = network
            .daemon_for_kubo_profile_sync(
                &state_root,
                ResourceBudget::default(),
                fixture.base_url().to_string(),
                "kubo-profile-provider",
            )
            .expect("start Kubo profile-sync fixture daemon");
        let profile_sync_status = daemon
            .health()
            .plugins
            .into_iter()
            .find(|status| status.metadata.id == PROFILE_SYNC_PLUGIN)
            .expect("profile-sync plugin status");

        for capability in [
            "profile-sync/kubo-rpc",
            "profile-sync/internal-transport-shim",
            "socketless-fixture",
        ] {
            assert!(
                profile_sync_status
                    .metadata
                    .capabilities
                    .iter()
                    .any(|candidate| candidate == capability),
                "profile-sync should advertise {capability}"
            );
        }
        assert!(
            profile_sync_status
                .metadata
                .privacy_boundary
                .contains("no sockets, DNS, loopback listener, or external network")
        );

        let ProfileSyncResponse::Providers { providers } = daemon
            .profile_sync(ProfileSyncRequest::DiscoverProviders(
                ProfileSyncProfileRequest::new("default"),
            ))
            .expect("discover fixture Kubo profile-sync provider")
        else {
            panic!("expected provider discovery response");
        };
        assert_eq!(providers.len(), 1);
        assert_eq!(providers[0].provider_id, "kubo-profile-provider");
        assert_eq!(providers[0].provider_kind, "ipfs-kubo-fixture");
        assert!(
            providers[0]
                .privacy_boundary
                .contains("no sockets, DNS, loopback listener, or external network")
        );
        assert!(fixture.finish().is_empty());
        let _ = std::fs::remove_dir_all(state_root);
    }

    #[test]
    fn profile_sync_service_uses_socketless_kubo_fixture_backend() {
        let object_id = "bafybeigdyrztprofileobject";
        let fixture = InProcessBroadwebNetwork::new().kubo_rpc_sequence(vec![
            InternalKuboRpcResponse {
                status_code: 200,
                content_type: "application/json".to_string(),
                body:
                    br#"{"Name":"profile-object","Hash":"bafybeigdyrztprofileobject","Size":"128"}"#
                        .to_vec(),
            },
            InternalKuboRpcResponse {
                status_code: 200,
                content_type: "application/octet-stream".to_string(),
                body: Vec::new(),
            },
            InternalKuboRpcResponse {
                status_code: 200,
                content_type: "application/json".to_string(),
                body: br#"{"Pins":["bafybeigdyrztprofileobject"]}"#.to_vec(),
            },
            InternalKuboRpcResponse {
                status_code: 200,
                content_type: "application/json".to_string(),
                body: br#"{"Keys":{"bafybeigdyrztprofileobject":{"Type":"recursive"}}}"#.to_vec(),
            },
            InternalKuboRpcResponse {
                status_code: 200,
                content_type: "application/json".to_string(),
                body: br#"{"Name":"settings-latest","Value":"/ipfs/bafybeigdyrztprofileobject"}"#
                    .to_vec(),
            },
            InternalKuboRpcResponse {
                status_code: 200,
                content_type: "application/json".to_string(),
                body: br#"{"Path":"/ipfs/bafybeigdyrztprofileobject"}"#.to_vec(),
            },
            InternalKuboRpcResponse {
                status_code: 200,
                content_type: "application/json".to_string(),
                body: br#"{"Pins":["bafybeigdyrztprofileobject"]}"#.to_vec(),
            },
        ]);
        let mut registry = PluginRegistry::new();
        registry.register_service(
            ProfileSyncService::kubo_fixture(fixture.base_url(), "kubo-fixture-provider")
                .expect("Kubo fixture profile-sync service"),
        );
        let budget = ResourceBudget::default();

        let ProfileSyncResponse::Providers { providers } = registry
            .profile_sync(
                ProfileSyncRequest::DiscoverProviders(ProfileSyncProfileRequest::new("default")),
                &budget,
            )
            .expect("discover Kubo fixture profile-sync provider")
        else {
            panic!("expected provider discovery response");
        };
        assert_eq!(providers.len(), 1);
        assert_eq!(providers[0].provider_id, "kubo-fixture-provider");
        assert_eq!(providers[0].provider_kind, "ipfs-kubo-fixture");
        assert!(providers[0].can_publish_roots);

        let ProfileSyncResponse::PutEncryptedObject {
            object_id: put_object_id,
        } = registry
            .profile_sync(
                ProfileSyncRequest::PutEncryptedObject(ProfileSyncPutObjectRequest::new(
                    "default",
                    b"encrypted slate-settings snapshot".to_vec(),
                )),
                &budget,
            )
            .expect("put profile sync object through Kubo fixture service")
        else {
            panic!("expected put object response");
        };
        assert_eq!(put_object_id, object_id);

        assert_eq!(
            registry
                .profile_sync(
                    ProfileSyncRequest::GetEncryptedObject(ProfileSyncObjectRequest::new(
                        "default", object_id,
                    )),
                    &budget,
                )
                .expect("get profile sync object through Kubo fixture service"),
            ProfileSyncResponse::GetEncryptedObject {
                object_id: object_id.to_string(),
                bytes: b"encrypted slate-settings snapshot".to_vec(),
            }
        );
        assert_eq!(
            registry
                .profile_sync(
                    ProfileSyncRequest::RetainObject(ProfileSyncObjectRequest::new(
                        "default", object_id,
                    )),
                    &budget,
                )
                .expect("retain object through Kubo fixture service"),
            ProfileSyncResponse::RetainObject {
                object_id: object_id.to_string(),
                retained: true,
            }
        );
        assert_eq!(
            registry
                .profile_sync(
                    ProfileSyncRequest::VerifyRetainedObject(ProfileSyncObjectRequest::new(
                        "default", object_id,
                    )),
                    &budget,
                )
                .expect("verify object through Kubo fixture service"),
            ProfileSyncResponse::RetainedObjectStatus {
                object_id: object_id.to_string(),
                retained: true,
                available: true,
            }
        );
        assert_eq!(
            registry
                .profile_sync(
                    ProfileSyncRequest::ListRetainedObjects(ProfileSyncProfileRequest::new(
                        "default",
                    )),
                    &budget,
                )
                .expect("list retained objects through Kubo fixture service"),
            ProfileSyncResponse::RetainedObjects {
                object_ids: vec![object_id.to_string()],
            }
        );
        let ProfileSyncResponse::ProviderHealth { health } = registry
            .profile_sync(
                ProfileSyncRequest::ProviderHealth(ProfileSyncProfileRequest::new("default")),
                &budget,
            )
            .expect("read Kubo fixture provider health")
        else {
            panic!("expected provider health response");
        };
        assert_eq!(health.retained_objects, 1);
        assert_eq!(health.object_transfer_providers, 1);
        assert_eq!(health.availability_providers, 1);
        assert_eq!(health.mutable_root_providers, 1);
        assert!(!health.degraded);
        assert_eq!(
            registry
                .profile_sync(
                    ProfileSyncRequest::PublishRoot(ProfileSyncRootUpdate::new(
                        "default",
                        "settings-latest",
                        object_id,
                    )),
                    &budget,
                )
                .expect("publish root through Kubo fixture service"),
            ProfileSyncResponse::Root {
                root_id: "settings-latest".to_string(),
                object_id: Some(object_id.to_string()),
            }
        );
        let ProfileSyncResponse::RootCandidates { candidates, .. } = registry
            .profile_sync(
                ProfileSyncRequest::ListRootCandidates(ProfileSyncRootRequest::new(
                    "default",
                    "settings-latest",
                )),
                &budget,
            )
            .expect("list Kubo fixture root candidates")
        else {
            panic!("expected root candidates response");
        };
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].publisher_provider_id, "kubo-fixture-provider");
        assert_eq!(candidates[0].object_id, object_id);
        let ProfileSyncResponse::RootHealth { health } = registry
            .profile_sync(
                ProfileSyncRequest::RootHealth(
                    ProfileSyncRootHealthRequest::with_minimum_online_retaining_providers(
                        "default",
                        "settings-latest",
                        1,
                    ),
                ),
                &budget,
            )
            .expect("read Kubo fixture root health")
        else {
            panic!("expected root health response");
        };
        assert_eq!(health.latest_object_id.as_deref(), Some(object_id));
        assert!(health.latest_object_available);
        assert_eq!(health.online_retaining_providers, 1);
        assert!(!health.degraded);
        assert_eq!(
            registry
                .profile_sync(
                    ProfileSyncRequest::ResolveRoot(ProfileSyncRootRequest::new(
                        "default",
                        "settings-latest",
                    )),
                    &budget,
                )
                .expect("resolve root through Kubo fixture service"),
            ProfileSyncResponse::Root {
                root_id: "settings-latest".to_string(),
                object_id: Some(object_id.to_string()),
            }
        );
        assert_eq!(
            registry
                .profile_sync(
                    ProfileSyncRequest::ReleaseObject(ProfileSyncObjectRequest::new(
                        "default", object_id,
                    )),
                    &budget,
                )
                .expect("release object through Kubo fixture service"),
            ProfileSyncResponse::ReleaseObject {
                object_id: object_id.to_string(),
                retained: false,
            }
        );
        assert_eq!(
            registry
                .profile_sync(
                    ProfileSyncRequest::ListRetainedObjects(ProfileSyncProfileRequest::new(
                        "default",
                    )),
                    &budget,
                )
                .expect("list retained objects after Kubo fixture release"),
            ProfileSyncResponse::RetainedObjects {
                object_ids: Vec::new(),
            }
        );
        let ProfileSyncResponse::RootHealth { health } = registry
            .profile_sync(
                ProfileSyncRequest::RootHealth(
                    ProfileSyncRootHealthRequest::with_minimum_online_retaining_providers(
                        "default",
                        "settings-latest",
                        1,
                    ),
                ),
                &budget,
            )
            .expect("read Kubo fixture root health after release")
        else {
            panic!("expected root health response after release");
        };
        assert_eq!(health.latest_object_id.as_deref(), Some(object_id));
        assert!(health.latest_object_available);
        assert_eq!(health.online_retaining_providers, 0);
        assert!(health.degraded);

        assert_eq!(
            fixture.finish(),
            vec![
                "POST /api/v0/add?cid-version=1&raw-leaves=true&pin=false HTTP/1.1",
                "POST /api/v0/cat?arg=%2Fipfs%2Fbafybeigdyrztprofileobject HTTP/1.1",
                "POST /api/v0/pin/add?arg=bafybeigdyrztprofileobject&recursive=true HTTP/1.1",
                "POST /api/v0/pin/ls?arg=bafybeigdyrztprofileobject&type=recursive HTTP/1.1",
                "POST /api/v0/name/publish?arg=%2Fipfs%2Fbafybeigdyrztprofileobject&key=settings-latest&allow-offline=true HTTP/1.1",
                "POST /api/v0/name/resolve?arg=%2Fipns%2Fsettings-latest&recursive=false HTTP/1.1",
                "POST /api/v0/pin/rm?arg=bafybeigdyrztprofileobject&recursive=true HTTP/1.1",
            ]
        );
    }

    #[test]
    fn in_process_network_builds_kubo_profile_sync_daemon_without_sockets() {
        let object_id = "bafybeigdyrztprofileobject";
        let network = InProcessBroadwebNetwork::new();
        let fixture = network.kubo_rpc_sequence(vec![
            InternalKuboRpcResponse {
                status_code: 200,
                content_type: "application/json".to_string(),
                body:
                    br#"{"Name":"profile-object","Hash":"bafybeigdyrztprofileobject","Size":"128"}"#
                        .to_vec(),
            },
            InternalKuboRpcResponse {
                status_code: 200,
                content_type: "application/octet-stream".to_string(),
                body: Vec::new(),
            },
        ]);
        let state_root = test_state_root("kubo-profile-sync-daemon");
        let daemon = network
            .daemon_for_kubo_profile_sync(
                &state_root,
                ResourceBudget::default(),
                fixture.base_url(),
                "kubo-profile-sync-provider",
            )
            .expect("start Kubo profile-sync fixture daemon");

        let ProfileSyncResponse::PutEncryptedObject {
            object_id: put_object_id,
        } = daemon
            .profile_sync(ProfileSyncRequest::PutEncryptedObject(
                ProfileSyncPutObjectRequest::new(
                    "default",
                    b"encrypted slate-settings snapshot".to_vec(),
                ),
            ))
            .expect("put through Kubo profile-sync fixture daemon")
        else {
            panic!("expected put response");
        };
        assert_eq!(put_object_id, object_id);
        assert_eq!(
            daemon
                .profile_sync(ProfileSyncRequest::GetEncryptedObject(
                    ProfileSyncObjectRequest::new("default", object_id),
                ))
                .expect("get through Kubo profile-sync fixture daemon"),
            ProfileSyncResponse::GetEncryptedObject {
                object_id: object_id.to_string(),
                bytes: b"encrypted slate-settings snapshot".to_vec(),
            }
        );
        assert_eq!(
            fixture.finish(),
            vec![
                "POST /api/v0/add?cid-version=1&raw-leaves=true&pin=false HTTP/1.1",
                "POST /api/v0/cat?arg=%2Fipfs%2Fbafybeigdyrztprofileobject HTTP/1.1",
            ]
        );

        let _ = std::fs::remove_dir_all(state_root);
    }

    #[test]
    fn in_process_network_rejects_foreign_kubo_profile_sync_fixture() {
        let source_network = InProcessBroadwebNetwork::new();
        let target_network = InProcessBroadwebNetwork::new();
        let fixture = source_network.kubo_rpc_response(InternalKuboRpcResponse {
            status_code: 200,
            content_type: "application/json".to_string(),
            body: br#"{"Name":"profile-object","Hash":"bafybeigdyrztprofileobject","Size":"128"}"#
                .to_vec(),
        });

        let error = match target_network
            .registry_for_kubo_profile_sync(fixture.base_url(), "foreign-provider")
        {
            Ok(_) => panic!("foreign Kubo fixture URL should be rejected"),
            Err(error) => error,
        };
        assert!(matches!(
            error,
            BroadwebdError::UnsupportedRequest(message)
                if message.contains("in-process Kubo profile-sync fixtures must use a URL created by network")
        ));
    }

    #[test]
    fn ipfs_kubo_profile_sync_response_parsers_extract_object_ids() {
        let object_id = "bafybeigdyrztprofileobject";

        assert_eq!(
            ipfs_kubo_profile_sync_added_object_id(
                br#"{"Name":"profile-object","Hash":"bafybeigdyrztprofileobject","Size":"128"}"#
            )
            .expect("parse Kubo add response"),
            object_id
        );
        assert!(
            ipfs_kubo_profile_sync_pin_ls_has_recursive_pin(
                object_id,
                br#"{"Keys":{"bafybeigdyrztprofileobject":{"Type":"recursive"}}}"#
            )
            .expect("parse recursive Kubo pin status")
        );
        assert!(
            !ipfs_kubo_profile_sync_pin_ls_has_recursive_pin(
                object_id,
                br#"{"Keys":{"bafybeigdyrztprofileobject":{"Type":"indirect"}}}"#
            )
            .expect("parse non-recursive Kubo pin status")
        );
        assert_eq!(
            ipfs_kubo_profile_sync_published_object_id(
                br#"{"Name":"k51profilelatest","Value":"/ipfs/bafybeigdyrztprofileobject"}"#
            )
            .expect("parse Kubo IPNS publish response"),
            object_id
        );
        assert_eq!(
            ipfs_kubo_profile_sync_resolved_object_id(
                br#"{"Path":"/ipfs/bafybeigdyrztprofileobject"}"#
            )
            .expect("parse Kubo IPNS resolve response"),
            object_id
        );
    }

    #[test]
    fn ipfs_kubo_profile_sync_response_parsers_reject_malformed_kubo_data() {
        assert!(matches!(
            ipfs_kubo_profile_sync_added_object_id(br#"{"Name":"missing-hash"}"#),
            Err(BroadwebdError::Request(_))
        ));
        assert!(matches!(
            ipfs_kubo_profile_sync_published_object_id(
                br#"{"Name":"k51profilelatest","Value":"/ipns/not-an-object"}"#
            ),
            Err(BroadwebdError::InvalidUrl(_))
        ));
        assert!(matches!(
            ipfs_kubo_profile_sync_resolved_object_id(
                br#"{"Path":"/ipfs/bafybeigdyrztprofileobject/extra"}"#
            ),
            Err(BroadwebdError::InvalidUrl(_))
        ));
    }

    #[test]
    fn ipfs_kubo_config_requires_loopback_rpc_endpoint() {
        assert!(IpfsKuboRpcEndpoint::local("http://127.0.0.1:5001").is_ok());
        assert!(IpfsKuboRpcEndpoint::local("http://[::1]:5001").is_ok());
        assert!(matches!(
            IpfsKuboRpcEndpoint::local("http://localhost:5001"),
            Err(BroadwebdError::UnsupportedRequest(_))
        ));
        assert!(matches!(
            IpfsKuboRpcEndpoint::local("https://ipfs.example.test:5001"),
            Err(BroadwebdError::UnsupportedRequest(_))
        ));
    }

    #[test]
    fn ipfs_service_registers_kubo_rpc_transport_from_config() {
        let service = IpfsService::new(
            IpfsConfig::with_kubo_rpc("http://127.0.0.1:5001").expect("Kubo RPC config"),
        );
        let mut registry = PluginRegistry::new();
        let installs = service.install_adapter_plugins(&mut registry);
        let metadata = service.metadata();

        assert_eq!(service.config().transport(), IpfsTransportKind::KuboRpc);
        assert!(service.config().uses_kubo_rpc());
        assert_eq!(service.config().http_transport_id(), IPFS_KUBO_RPC_PLUGIN);
        assert_eq!(installs.len(), 1);
        assert_eq!(installs[0].metadata.id, IPFS_KUBO_RPC_PLUGIN);
        assert!(
            metadata
                .capabilities
                .iter()
                .any(|capability| capability == "local-kubo-rpc")
        );
        assert_eq!(
            metadata.dependencies,
            vec![IPFS_KUBO_RPC_PLUGIN.to_string()]
        );
    }

    #[test]
    fn http_fetch_routes_ipfs_through_kubo_rpc_transport_for_html() {
        let (rpc, fixture) = in_process_kubo_rpc_fixture(
            "application/octet-stream",
            "<!doctype html><title>Kubo Fixture</title><h1>Fetched From Kubo</h1>",
        );
        let mut registry = PluginRegistry::new();
        registry.register_protocol_service(IpfsService::new(
            IpfsConfig::with_kubo_rpc(&rpc).expect("Kubo RPC config"),
        ));
        registry.register_service(super::HttpFetchService);
        let daemon = BroadwebDaemon::start_with_registry(
            test_state_root("ipfs-kubo-html"),
            Default::default(),
            registry,
        )
        .expect("daemon");
        let response = daemon
            .fetch_http(HttpFetchRequest::default_profile(
                "ipfs://bafybeigdyrzt/index.html",
            ))
            .expect("fetch Kubo fixture");
        let request = finish_single_kubo_request(fixture);

        assert!(
            request.contains("POST /api/v0/cat?arg=%2Fipfs%2Fbafybeigdyrzt%2Findex.html HTTP/1.1")
        );
        assert_eq!(response.disposition, FetchDisposition::RenderHtml);
        assert_eq!(
            response.content_type,
            Some("text/html; charset=utf-8".to_string())
        );
        assert!(response.body_text_lossy().contains("Kubo Fixture"));

        let _ = fs::remove_dir_all(daemon.state_root().path());
    }

    #[test]
    fn http_fetch_annotates_kubo_rpc_profile_and_privacy_context() {
        let (rpc, fixture) = in_process_kubo_rpc_fixture(
            "text/html; charset=utf-8",
            "<!doctype html><title>Kubo Profile Fixture</title>",
        );
        let mut registry = PluginRegistry::new();
        registry.register_protocol_service(IpfsService::new(
            IpfsConfig::with_kubo_rpc(&rpc).expect("Kubo RPC config"),
        ));
        registry.register_service(super::HttpFetchService);
        let daemon = BroadwebDaemon::start_with_registry(
            test_state_root("kubo-route-context"),
            Default::default(),
            registry,
        )
        .expect("daemon");
        let response = daemon
            .fetch_http(HttpFetchRequest::new(
                "research",
                "ipfs://bafybeigdyrzt/index.html",
            ))
            .expect("fetch Kubo fixture");
        fixture.finish();

        let route = response.route.expect("route info");
        assert_eq!(route.profile, "research");
        assert_eq!(route.transport_id, IPFS_KUBO_RPC_PLUGIN);
        assert!(
            route
                .privacy_boundary
                .contains("in-process Kubo RPC fixture")
        );
        assert_eq!(route.purpose, FetchPurpose::Navigation);

        let _ = fs::remove_dir_all(daemon.state_root().path());
    }

    #[test]
    fn http_fetch_does_not_record_kubo_rpc_subresource_downloads() {
        let (rpc, fixture) = in_process_kubo_rpc_fixture("text/css", "body{color:#123}");
        let mut registry = PluginRegistry::new();
        registry.register_protocol_service(IpfsService::new(
            IpfsConfig::with_kubo_rpc(&rpc).expect("Kubo RPC config"),
        ));
        registry.register_service(super::HttpFetchService);
        let daemon = BroadwebDaemon::start_with_registry(
            test_state_root("kubo-subresource"),
            Default::default(),
            registry,
        )
        .expect("daemon");
        let response = daemon
            .fetch_http(
                HttpFetchRequest::default_profile("ipfs://bafybeigdyrzt/style.css")
                    .for_subresource(),
            )
            .expect("fetch Kubo subresource fixture");
        let request = finish_single_kubo_request(fixture);

        assert!(
            request.contains("POST /api/v0/cat?arg=%2Fipfs%2Fbafybeigdyrzt%2Fstyle.css HTTP/1.1")
        );
        assert_eq!(
            response.disposition,
            FetchDisposition::Download {
                suggested_filename: "style.css".to_string()
            }
        );
        assert_eq!(response.download, None);
        let route = response.route.expect("route info");
        assert_eq!(route.profile, "default");
        assert_eq!(route.transport_id, IPFS_KUBO_RPC_PLUGIN);
        assert_eq!(route.purpose, FetchPurpose::Subresource);
        assert!(
            !daemon
                .state_root()
                .profile_root("default")
                .expect("default profile root")
                .join("temporary/downloads/style.css")
                .exists()
        );

        let _ = fs::remove_dir_all(daemon.state_root().path());
    }

    #[test]
    fn http_fetch_routes_kubo_directory_to_index_after_failed_cat() {
        let (rpc, fixture) = in_process_kubo_rpc_sequence_fixture(vec![
            ("500 Internal Server Error", "text/plain", "directory"),
            (
                "200 OK",
                "application/octet-stream",
                "<!doctype html><title>Kubo Directory Fixture</title>",
            ),
        ]);
        let mut registry = PluginRegistry::new();
        registry.register_protocol_service(IpfsService::new(
            IpfsConfig::with_kubo_rpc(&rpc).expect("Kubo RPC config"),
        ));
        registry.register_service(super::HttpFetchService);
        let daemon = BroadwebDaemon::start_with_registry(
            test_state_root("ipfs-kubo-directory"),
            Default::default(),
            registry,
        )
        .expect("daemon");
        let response = daemon
            .fetch_http(HttpFetchRequest::default_profile(
                "ipfs://bafybeigdyrzt/docs/",
            ))
            .expect("fetch Kubo directory fixture");
        let requests = fixture.finish();

        assert_eq!(requests.len(), 2);
        assert!(
            requests[0].contains("POST /api/v0/cat?arg=%2Fipfs%2Fbafybeigdyrzt%2Fdocs%2F HTTP/1.1")
        );
        assert!(
            requests[1].contains(
                "POST /api/v0/cat?arg=%2Fipfs%2Fbafybeigdyrzt%2Fdocs%2Findex.html HTTP/1.1"
            )
        );
        assert_eq!(response.status_code, 200);
        assert_eq!(response.final_url, "ipfs://bafybeigdyrzt/docs/index.html");
        assert_eq!(response.disposition, FetchDisposition::RenderHtml);
        assert!(
            response
                .body_text_lossy()
                .contains("Kubo Directory Fixture")
        );

        let _ = fs::remove_dir_all(daemon.state_root().path());
    }

    #[test]
    fn http_fetch_routes_kubo_ipns_root_to_index_after_failed_cat() {
        let (rpc, fixture) = in_process_kubo_rpc_sequence_fixture(vec![
            ("500 Internal Server Error", "text/plain", "directory"),
            (
                "200 OK",
                "application/octet-stream",
                "<!doctype html><title>Kubo IPNS Fixture</title>",
            ),
        ]);
        let mut registry = PluginRegistry::new();
        registry.register_protocol_service(IpfsService::new(
            IpfsConfig::with_kubo_rpc(&rpc).expect("Kubo RPC config"),
        ));
        registry.register_service(super::HttpFetchService);
        let daemon = BroadwebDaemon::start_with_registry(
            test_state_root("ipns-kubo-root"),
            Default::default(),
            registry,
        )
        .expect("daemon");
        let response = daemon
            .fetch_http(HttpFetchRequest::default_profile("ipns://example.net"))
            .expect("fetch Kubo IPNS root fixture");
        let requests = fixture.finish();

        assert_eq!(requests.len(), 2);
        assert!(requests[0].contains("POST /api/v0/cat?arg=%2Fipns%2Fexample.net HTTP/1.1"));
        assert!(
            requests[1]
                .contains("POST /api/v0/cat?arg=%2Fipns%2Fexample.net%2Findex.html HTTP/1.1")
        );
        assert_eq!(response.status_code, 200);
        assert_eq!(response.disposition, FetchDisposition::RenderHtml);
        assert!(response.body_text_lossy().contains("Kubo IPNS Fixture"));
        assert_eq!(response.final_url, "ipns://example.net/index.html");

        let _ = fs::remove_dir_all(daemon.state_root().path());
    }

    #[test]
    #[ignore = "external internet smoke test; run through `make test-external-network`"]
    fn external_direct_http_fetches_example_domain() {
        if std::env::var_os("SLATE_EXTERNAL_NETWORK_TESTS").is_none() {
            eprintln!("set SLATE_EXTERNAL_NETWORK_TESTS=1 to run external network tests");
            return;
        }

        let daemon = BroadwebDaemon::start(test_state_root("external-http")).expect("daemon");
        let response = daemon
            .fetch_http(HttpFetchRequest::default_profile("http://example.com/"))
            .expect("fetch example.com");

        assert_eq!(response.status_code, 200);
        assert_eq!(response.disposition, FetchDisposition::RenderHtml);
        assert!(response.body_text_lossy().contains("Example Domain"));

        let _ = fs::remove_dir_all(daemon.state_root().path());
    }

    #[test]
    #[ignore = "external internet smoke test; run through `make test-external-network`"]
    fn external_public_ipfs_gateway_fetches_cid() {
        if std::env::var_os("SLATE_EXTERNAL_NETWORK_TESTS").is_none() {
            eprintln!("set SLATE_EXTERNAL_NETWORK_TESTS=1 to run external network tests");
            return;
        }

        let mut registry = PluginRegistry::new();
        registry.register_protocol_service(IpfsService::new(
            IpfsConfig::with_public_gateway("https://ipfs.io")
                .expect("explicit public gateway config"),
        ));
        registry.register_service(super::HttpFetchService);
        let daemon = BroadwebDaemon::start_with_registry(
            test_state_root("external-public-ipfs"),
            ResourceBudget::default(),
            registry,
        )
        .expect("daemon");
        let response = daemon
            .fetch_http(HttpFetchRequest::default_profile(
                "ipfs://bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi",
            ))
            .expect("fetch public gateway CID");

        assert_eq!(response.status_code, 200);
        assert!(!response.body.is_empty());
        assert!(response.final_url.starts_with("https://ipfs.io/ipfs/"));

        let _ = fs::remove_dir_all(daemon.state_root().path());
    }

    fn in_process_http_fixture(
        content_type: &'static str,
        body: &'static str,
    ) -> (String, InProcessHttpFixture) {
        let network = InProcessBroadwebNetwork::new();
        in_process_http_fixture_for_network(&network, content_type, body)
    }

    fn in_process_http_fixture_for_network(
        network: &InProcessBroadwebNetwork,
        content_type: &'static str,
        body: &'static str,
    ) -> (String, InProcessHttpFixture) {
        in_process_http_status_fixture_for_network(network, "200 OK", content_type, body)
    }

    fn in_process_http_fixture_with_headers_for_network(
        network: &InProcessBroadwebNetwork,
        content_type: &'static str,
        extra_headers: &'static [&'static str],
        body: &'static str,
    ) -> (String, InProcessHttpFixture) {
        in_process_http_status_fixture_with_headers_for_network(
            network,
            "200 OK",
            content_type,
            extra_headers,
            body,
        )
    }

    fn in_process_http_status_fixture(
        status: &'static str,
        content_type: &'static str,
        body: &'static str,
    ) -> (String, InProcessHttpFixture) {
        let network = InProcessBroadwebNetwork::new();
        in_process_http_status_fixture_for_network(&network, status, content_type, body)
    }

    fn in_process_http_status_fixture_for_network(
        network: &InProcessBroadwebNetwork,
        status: &'static str,
        content_type: &'static str,
        body: &'static str,
    ) -> (String, InProcessHttpFixture) {
        in_process_http_status_fixture_with_headers_for_network(
            network,
            status,
            content_type,
            &[],
            body,
        )
    }

    fn in_process_http_status_fixture_with_headers_for_network(
        network: &InProcessBroadwebNetwork,
        status: &'static str,
        content_type: &'static str,
        extra_headers: &'static [&'static str],
        body: &'static str,
    ) -> (String, InProcessHttpFixture) {
        let mut headers = vec![super::HttpHeader {
            name: "content-type".to_string(),
            value: content_type.to_string(),
        }];
        headers.extend(extra_headers.iter().map(|header| {
            let (name, value) = header
                .split_once(':')
                .expect("fixture header should use name: value syntax");
            super::HttpHeader {
                name: name.trim().to_string(),
                value: value.trim().to_string(),
            }
        }));
        let fixture = network.http_response(InternalFixtureHttpResponse {
            status_code: status_code(status),
            content_type: Some(content_type.to_string()),
            headers,
            body: body.as_bytes().to_vec(),
        });
        (fixture.base_url().to_string(), fixture)
    }

    fn missing_in_process_http_fixture_url() -> String {
        InProcessBroadwebNetwork::new().missing_http_url()
    }

    fn in_process_kubo_rpc_fixture(
        content_type: &'static str,
        body: &'static str,
    ) -> (String, InProcessKuboRpcFixture) {
        in_process_kubo_rpc_sequence_fixture(vec![("200 OK", content_type, body)])
    }

    fn in_process_kubo_rpc_sequence_fixture(
        responses: Vec<(&'static str, &'static str, &'static str)>,
    ) -> (String, InProcessKuboRpcFixture) {
        let fixture = InProcessBroadwebNetwork::new().kubo_rpc_sequence(
            responses
                .into_iter()
                .map(|(status, content_type, body)| InternalKuboRpcResponse {
                    status_code: status_code(status),
                    content_type: content_type.to_string(),
                    body: body.as_bytes().to_vec(),
                })
                .collect(),
        );
        (fixture.base_url().to_string(), fixture)
    }

    fn finish_single_kubo_request(fixture: InProcessKuboRpcFixture) -> String {
        fixture
            .finish()
            .into_iter()
            .next()
            .expect("expected one Kubo fixture request")
    }

    #[test]
    fn in_process_fixture_layer_uses_synthetic_urls() {
        let (http_url, http_fixture) =
            in_process_http_fixture("text/plain", "synthetic HTTP fixture");
        assert!(http_url.starts_with("slate-fixture-http://"));
        http_fixture.finish();

        let (kubo_url, kubo_fixture) =
            in_process_kubo_rpc_fixture("text/plain", "synthetic Kubo fixture");
        assert!(kubo_url.starts_with("slate-fixture-kubo://"));
        kubo_fixture.finish();

        let network = InProcessBroadwebNetwork::new();
        let profile_sync_endpoint = network.profile_sync_provider_endpoint("provider-a");
        assert_eq!(profile_sync_endpoint.provider_id(), "provider-a");
        assert!(
            profile_sync_endpoint
                .endpoint_ref()
                .starts_with(crate::IN_PROCESS_PROFILE_SYNC_FIXTURE_ENDPOINT_PREFIX)
        );
        assert_eq!(
            profile_sync_endpoint
                .endpoint_ref()
                .split_once("://")
                .unwrap()
                .0,
            crate::IN_PROCESS_PROFILE_SYNC_FIXTURE_ENDPOINT_SCHEME
        );
        let parsed_endpoint = crate::parse_in_process_profile_sync_fixture_endpoint_ref(
            profile_sync_endpoint.endpoint_ref(),
        )
        .expect("profile-sync fixture endpoint should parse");
        assert_eq!(parsed_endpoint.network_id(), network.network_id());
        assert_eq!(parsed_endpoint.provider_id(), "provider-a");
        assert!(
            crate::parse_in_process_profile_sync_fixture_endpoint_ref(
                "slate-fixture-profile-sync://network-1/provider-a/extra",
            )
            .is_none()
        );
        assert!(
            crate::parse_in_process_profile_sync_fixture_endpoint_ref("http://127.0.0.1:5001")
                .is_none()
        );
    }

    fn status_code(status: &str) -> u16 {
        status
            .split_whitespace()
            .next()
            .expect("fixture status should include numeric code")
            .parse()
            .expect("fixture status code should be numeric")
    }

    fn test_state_root(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "slate-broadwebd-test-{}-{name}",
            std::process::id()
        ))
    }

    fn test_download_root(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "slate-broadwebd-downloads-{}-{name}",
            std::process::id()
        ))
    }

    struct FixtureTransport {
        id: &'static str,
        body: &'static str,
    }

    impl FixtureTransport {
        fn new(id: &'static str, body: &'static str) -> Self {
            Self { id, body }
        }
    }

    impl TransportPlugin for FixtureTransport {
        fn metadata(&self) -> PluginMetadata {
            PluginMetadata::new(self.id, PluginKind::Transport)
                .with_capabilities(&["http", "https", "http-fetch"])
                .with_privacy_boundary("test fixture transport")
                .with_resource_profile(ResourceProfile::Low)
        }

        fn fetch_http(
            &self,
            request: &TransportHttpRequest,
            _budget: &ResourceBudget,
        ) -> Result<HttpFetchResponse, BroadwebdError> {
            Ok(HttpFetchResponse::new(
                &request.url,
                200,
                Some("text/html; charset=utf-8".to_string()),
                Vec::new(),
                format!("<!doctype html><title>Fixture</title>{}", self.body).into_bytes(),
            ))
        }
    }
}
