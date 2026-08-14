use crate::protocols::ipfs::IpfsGatewayTransport;
use crate::services::http_fetch::HttpFetchService;
use crate::transports::direct_http::DirectHttpTransport;
use crate::{
    BroadwebdError, HTTP_FETCH_PLUGIN, HttpFetchRequest, HttpFetchResponse, PluginHealth,
    PluginMetadata, PluginStatus, ResourceBudget, ServiceRequest, ServiceResponse,
    TransportHttpRequest,
};
use std::collections::BTreeMap;

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

pub struct PluginRegistry {
    transports: BTreeMap<String, Box<dyn TransportPlugin>>,
    services: BTreeMap<String, Box<dyn ApplicationServicePlugin>>,
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self {
            transports: BTreeMap::new(),
            services: BTreeMap::new(),
        }
    }

    pub fn with_default_http() -> Self {
        let mut registry = Self::new();
        registry.register_transport(DirectHttpTransport);
        registry.register_transport(IpfsGatewayTransport::default());
        registry.register_service(HttpFetchService);
        registry
    }

    pub fn register_transport(&mut self, plugin: impl TransportPlugin + 'static) {
        let metadata = plugin.metadata();
        self.transports.insert(metadata.id, Box::new(plugin));
    }

    pub fn register_service(&mut self, plugin: impl ApplicationServicePlugin + 'static) {
        let metadata = plugin.metadata();
        self.services.insert(metadata.id, Box::new(plugin));
    }

    pub fn list_plugins(&self) -> Vec<PluginMetadata> {
        self.transports
            .values()
            .map(|plugin| plugin.metadata())
            .chain(self.services.values().map(|plugin| plugin.metadata()))
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
        }
    }

    pub(crate) fn transport(&self, id: &str) -> Result<&dyn TransportPlugin, BroadwebdError> {
        self.transports
            .get(id)
            .map(|plugin| plugin.as_ref())
            .ok_or_else(|| BroadwebdError::MissingPlugin(id.to_string()))
    }

    fn service(&self, id: &str) -> Result<&dyn ApplicationServicePlugin, BroadwebdError> {
        self.services
            .get(id)
            .map(|plugin| plugin.as_ref())
            .ok_or_else(|| BroadwebdError::MissingPlugin(id.to_string()))
    }

    fn has_plugin(&self, id: &str) -> bool {
        self.transports.contains_key(id) || self.services.contains_key(id)
    }
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::with_default_http()
    }
}
