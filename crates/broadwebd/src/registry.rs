use crate::protocols::{
    ipfs::{IpfsConfig, IpfsService},
    tor::TorService,
};
use crate::services::http_fetch::HttpFetchService;
use crate::services::profile_sync::ProfileSyncService;
use crate::transports::direct_http::DirectHttpTransport;
use crate::{
    BroadwebStatusReporter, BroadwebStatusSnapshot, BroadwebdError, DIRECT_HTTP_PLUGIN,
    HTTP_FETCH_PLUGIN, HttpFetchRequest, HttpFetchResponse, PROFILE_SYNC_PLUGIN, PluginHealth,
    PluginMetadata, PluginStatus, ProfileSyncRequest, ProfileSyncResponse, ResourceBudget,
    ServiceRequest, ServiceResponse, TransportHttpRequest,
};
use std::collections::BTreeMap;
use url::Url;

pub trait ProtocolService: Send + Sync {
    fn metadata(&self) -> PluginMetadata;

    fn install_plugins(&self, registry: &mut PluginRegistry) -> Vec<PluginInstallReport>;

    fn http_transport_for_url(&self, url: &Url) -> Option<Result<String, BroadwebdError>>;
}

pub trait TransportPlugin: Send + Sync {
    fn metadata(&self) -> PluginMetadata;

    fn fetch_http(
        &self,
        request: &TransportHttpRequest,
        budget: &ResourceBudget,
    ) -> Result<HttpFetchResponse, BroadwebdError>;
}

pub trait ApplicationServicePlugin: Send + Sync {
    fn metadata(&self) -> PluginMetadata;

    fn call(
        &self,
        request: ServiceRequest,
        registry: &PluginRegistry,
        budget: &ResourceBudget,
    ) -> Result<ServiceResponse, BroadwebdError>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PluginInstallReport {
    pub metadata: PluginMetadata,
    pub replaced_existing: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProtocolInstallReport {
    pub metadata: PluginMetadata,
    pub replaced_existing: bool,
    pub installed_plugins: Vec<PluginInstallReport>,
}

pub struct PluginRegistry {
    protocol_services: BTreeMap<String, Box<dyn ProtocolService>>,
    transports: BTreeMap<String, Box<dyn TransportPlugin>>,
    services: BTreeMap<String, Box<dyn ApplicationServicePlugin>>,
    status: BroadwebStatusReporter,
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self {
            protocol_services: BTreeMap::new(),
            transports: BTreeMap::new(),
            services: BTreeMap::new(),
            status: BroadwebStatusReporter::new(),
        }
    }

    pub fn with_status(status: BroadwebStatusReporter) -> Self {
        Self {
            protocol_services: BTreeMap::new(),
            transports: BTreeMap::new(),
            services: BTreeMap::new(),
            status,
        }
    }

    pub fn with_default_http() -> Self {
        Self::with_default_http_and_ipfs_config(IpfsConfig::default())
    }

    pub fn with_default_http_and_ipfs_config(ipfs_config: IpfsConfig) -> Self {
        Self::with_default_http_and_ipfs_config_and_status(
            ipfs_config,
            BroadwebStatusReporter::new(),
        )
    }

    pub fn with_default_http_and_ipfs_config_and_status(
        ipfs_config: IpfsConfig,
        status: BroadwebStatusReporter,
    ) -> Self {
        let mut registry = Self::with_status(status);
        registry.register_transport(DirectHttpTransport);
        registry.register_protocol_service(IpfsService::new(ipfs_config));
        registry.register_protocol_service(TorService);
        registry.register_service(HttpFetchService);
        registry.register_service(ProfileSyncService::new());
        registry
    }

    pub fn status_reporter(&self) -> BroadwebStatusReporter {
        self.status.clone()
    }

    pub fn status_snapshot(&self) -> BroadwebStatusSnapshot {
        self.status.snapshot()
    }

    pub fn register_protocol_service(&mut self, service: impl ProtocolService + 'static) {
        let _ = self.install_protocol_service(service);
    }

    pub fn install_protocol_service(
        &mut self,
        service: impl ProtocolService + 'static,
    ) -> ProtocolInstallReport {
        let metadata = service.metadata();
        let installed_plugins = service.install_plugins(self);
        let replaced_existing = self
            .protocol_services
            .insert(metadata.id.clone(), Box::new(service))
            .is_some();
        ProtocolInstallReport {
            metadata,
            replaced_existing,
            installed_plugins,
        }
    }

    pub fn remove_protocol_service(&mut self, id: &str) -> Result<PluginMetadata, BroadwebdError> {
        self.protocol_services
            .remove(id)
            .map(|service| service.metadata())
            .ok_or_else(|| BroadwebdError::MissingPlugin(id.to_string()))
    }

    pub fn register_transport(&mut self, plugin: impl TransportPlugin + 'static) {
        let _ = self.install_transport(plugin);
    }

    pub fn install_transport(
        &mut self,
        plugin: impl TransportPlugin + 'static,
    ) -> PluginInstallReport {
        let metadata = plugin.metadata();
        let replaced_existing = self
            .transports
            .insert(metadata.id.clone(), Box::new(plugin))
            .is_some();
        PluginInstallReport {
            metadata,
            replaced_existing,
        }
    }

    pub fn register_service(&mut self, plugin: impl ApplicationServicePlugin + 'static) {
        let _ = self.install_service(plugin);
    }

    pub fn install_service(
        &mut self,
        plugin: impl ApplicationServicePlugin + 'static,
    ) -> PluginInstallReport {
        let metadata = plugin.metadata();
        let replaced_existing = self
            .services
            .insert(metadata.id.clone(), Box::new(plugin))
            .is_some();
        PluginInstallReport {
            metadata,
            replaced_existing,
        }
    }

    pub fn remove_transport(&mut self, id: &str) -> Result<PluginMetadata, BroadwebdError> {
        self.transports
            .remove(id)
            .map(|plugin| plugin.metadata())
            .ok_or_else(|| BroadwebdError::MissingPlugin(id.to_string()))
    }

    pub fn remove_service(&mut self, id: &str) -> Result<PluginMetadata, BroadwebdError> {
        self.services
            .remove(id)
            .map(|plugin| plugin.metadata())
            .ok_or_else(|| BroadwebdError::MissingPlugin(id.to_string()))
    }

    pub fn list_plugins(&self) -> Vec<PluginMetadata> {
        self.protocol_services
            .values()
            .map(|service| service.metadata())
            .chain(self.transports.values().map(|plugin| plugin.metadata()))
            .chain(self.services.values().map(|plugin| plugin.metadata()))
            .collect()
    }

    pub fn list_protocol_services(&self) -> Vec<PluginMetadata> {
        self.protocol_services
            .values()
            .map(|service| service.metadata())
            .collect()
    }

    pub fn list_transports(&self) -> Vec<PluginMetadata> {
        self.transports
            .values()
            .map(|plugin| plugin.metadata())
            .collect()
    }

    pub fn list_application_services(&self) -> Vec<PluginMetadata> {
        self.services
            .values()
            .map(|plugin| plugin.metadata())
            .collect()
    }

    pub fn plugin_statuses(&self) -> Vec<PluginStatus> {
        self.list_plugins()
            .into_iter()
            .map(|metadata| {
                let missing: Vec<String> = metadata
                    .dependencies
                    .iter()
                    .filter(|dependency| !self.has_plugin(dependency))
                    .cloned()
                    .collect();
                let health = if missing.is_empty() {
                    PluginHealth::Ready
                } else {
                    PluginHealth::Degraded(format!("missing dependencies: {}", missing.join(", ")))
                };
                PluginStatus { metadata, health }
            })
            .collect()
    }

    pub fn fetch_http(
        &self,
        request: HttpFetchRequest,
        budget: &ResourceBudget,
    ) -> Result<HttpFetchResponse, BroadwebdError> {
        let service = self.service(HTTP_FETCH_PLUGIN)?;
        match service.call(ServiceRequest::HttpFetch(request), self, budget)? {
            ServiceResponse::HttpFetch(response) => Ok(response),
            ServiceResponse::ProfileSync(_) => Err(BroadwebdError::UnsupportedRequest(
                "http-fetch returned a profile-sync response".to_string(),
            )),
        }
    }

    pub fn profile_sync(
        &self,
        request: ProfileSyncRequest,
        budget: &ResourceBudget,
    ) -> Result<ProfileSyncResponse, BroadwebdError> {
        match self.call_service(
            PROFILE_SYNC_PLUGIN,
            ServiceRequest::ProfileSync(request),
            budget,
        )? {
            ServiceResponse::ProfileSync(response) => Ok(response),
            ServiceResponse::HttpFetch(_) => Err(BroadwebdError::UnsupportedRequest(
                "profile-sync returned an HTTP response".to_string(),
            )),
        }
    }

    pub fn call_service(
        &self,
        service_id: &str,
        request: ServiceRequest,
        budget: &ResourceBudget,
    ) -> Result<ServiceResponse, BroadwebdError> {
        self.service(service_id)?.call(request, self, budget)
    }

    pub(crate) fn transport(&self, id: &str) -> Result<&dyn TransportPlugin, BroadwebdError> {
        self.transports
            .get(id)
            .map(|plugin| plugin.as_ref())
            .ok_or_else(|| BroadwebdError::MissingPlugin(id.to_string()))
    }

    pub(crate) fn resolve_http_transport(&self, target: &str) -> Result<String, BroadwebdError> {
        let url =
            Url::parse(target).map_err(|error| BroadwebdError::InvalidUrl(error.to_string()))?;

        for protocol in self.protocol_services.values() {
            if let Some(transport) = protocol.http_transport_for_url(&url) {
                return transport;
            }
        }

        if matches!(url.scheme(), "http" | "https") {
            return Ok(DIRECT_HTTP_PLUGIN.to_string());
        }

        Err(BroadwebdError::UnsupportedRequest(format!(
            "no HTTP transport for {}",
            url.scheme()
        )))
    }

    fn service(&self, id: &str) -> Result<&dyn ApplicationServicePlugin, BroadwebdError> {
        self.services
            .get(id)
            .map(|plugin| plugin.as_ref())
            .ok_or_else(|| BroadwebdError::MissingPlugin(id.to_string()))
    }

    fn has_plugin(&self, id: &str) -> bool {
        self.protocol_services.contains_key(id)
            || self.transports.contains_key(id)
            || self.services.contains_key(id)
    }
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::with_default_http()
    }
}
