use crate::http::{fetch_http_url, parse_http_url};
use crate::{
    BroadwebdError, DIRECT_HTTP_PLUGIN, HttpFetchResponse, PluginKind, PluginMetadata,
    ResourceBudget, ResourceProfile, TransportHttpRequest, TransportPlugin,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DirectHttpTransport;

impl TransportPlugin for DirectHttpTransport {
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata::new(DIRECT_HTTP_PLUGIN, PluginKind::Transport)
            .with_capabilities(&["http", "https", "http-fetch"])
            .with_privacy_boundary("ordinary direct HTTP(S); uses normal DNS and network routing")
            .with_resource_profile(ResourceProfile::Low)
    }

    fn fetch_http(
        &self,
        request: &TransportHttpRequest,
        budget: &ResourceBudget,
    ) -> Result<HttpFetchResponse, BroadwebdError> {
        let url = parse_http_url(&request.url)?;
        if !matches!(url.scheme(), "http" | "https") {
            return Err(BroadwebdError::UnsupportedRequest(format!(
                "{} cannot fetch {}",
                DIRECT_HTTP_PLUGIN, request.url
            )));
        }

        fetch_http_url(url, budget)
    }
}
