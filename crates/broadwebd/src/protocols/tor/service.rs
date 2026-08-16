use super::{TorArtiHttpTransport, address::is_onion_url, address::is_tor_http_scheme};
use crate::{
    BroadwebdError, PluginInstallReport, PluginKind, PluginMetadata, PluginRegistry,
    ProtocolService, ResourceProfile, TOR_ARTI_HTTP_PLUGIN, TOR_PROTOCOL_SERVICE,
};
use url::Url;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TorService;

impl TorService {
    pub fn install_adapter_plugins(
        &self,
        registry: &mut PluginRegistry,
    ) -> Vec<PluginInstallReport> {
        vec![
            registry.install_transport(
                TorArtiHttpTransport::with_status(registry.status_reporter())
                    .expect("Tor runtime should initialize"),
            ),
        ]
    }
}

impl ProtocolService for TorService {
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata::new(TOR_PROTOCOL_SERVICE, PluginKind::ProtocolService)
            .with_capabilities(&[
                "tor",
                "onion",
                "application/http-response",
                "arti",
                "http-over-tor",
            ])
            .with_dependencies(&[TOR_ARTI_HTTP_PLUGIN])
            .with_privacy_boundary(
                "routes .onion HTTP requests through embedded Arti; no direct DNS fallback is permitted",
            )
            .with_resource_profile(ResourceProfile::High)
    }

    fn install_plugins(&self, registry: &mut PluginRegistry) -> Vec<PluginInstallReport> {
        self.install_adapter_plugins(registry)
    }

    fn http_transport_for_url(&self, url: &Url) -> Option<Result<String, BroadwebdError>> {
        if is_tor_http_scheme(url.scheme()) {
            return Some(Ok(TOR_ARTI_HTTP_PLUGIN.to_string()));
        }
        if matches!(url.scheme(), "http" | "https") && is_onion_url(url) {
            return Some(Ok(TOR_ARTI_HTTP_PLUGIN.to_string()));
        }
        None
    }
}
