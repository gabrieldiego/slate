use crate::{
    ApplicationServicePlugin, BroadwebdError, DIRECT_HTTP_PLUGIN, HTTP_FETCH_PLUGIN, PluginKind,
    PluginMetadata, PluginRegistry, ResourceBudget, ResourceProfile, ServiceRequest,
    ServiceResponse, TransportHttpRequest,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct HttpFetchService;

impl ApplicationServicePlugin for HttpFetchService {
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata::new(HTTP_FETCH_PLUGIN, PluginKind::ApplicationService)
            .with_capabilities(&[
                "application/http-response",
                "html-render-boundary",
                "download-boundary",
            ])
            .with_dependencies(&[DIRECT_HTTP_PLUGIN])
            .with_privacy_boundary(
                "uses an approved transport plugin to produce HTTP-like responses",
            )
            .with_resource_profile(ResourceProfile::Low)
    }

    fn call(
        &self,
        request: ServiceRequest,
        registry: &PluginRegistry,
        budget: &ResourceBudget,
    ) -> Result<ServiceResponse, BroadwebdError> {
        match request {
            ServiceRequest::HttpFetch(request) => {
                let transport = registry.transport(&request.transport_id)?;
                let transport_request = TransportHttpRequest {
                    profile: request.profile,
                    url: request.url,
                };
                transport
                    .fetch_http(&transport_request, budget)
                    .map(ServiceResponse::HttpFetch)
            }
        }
    }
}
