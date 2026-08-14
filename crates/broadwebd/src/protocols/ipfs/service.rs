use super::{IpfsConfig, IpfsGatewayTransport};
use crate::{
    BroadwebdError, IPFS_GATEWAY_PLUGIN, IPFS_PROTOCOL_SERVICE, PluginInstallReport, PluginKind,
    PluginMetadata, PluginRegistry, ProtocolService, ResourceProfile,
};
use url::Url;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IpfsService {
    config: IpfsConfig,
}

impl IpfsService {
    pub fn new(config: IpfsConfig) -> Self {
        Self { config }
    }

    pub fn config(&self) -> &IpfsConfig {
        &self.config
    }

    pub fn install_adapter_plugins(
        &self,
        registry: &mut PluginRegistry,
    ) -> Vec<PluginInstallReport> {
        vec![registry.install_transport(IpfsGatewayTransport::new(self.config.gateway_base()))]
    }
}

impl ProtocolService for IpfsService {
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata::new(IPFS_PROTOCOL_SERVICE, PluginKind::ProtocolService)
            .with_capabilities(&[
                "ipfs",
                "ipns",
                "application/http-response",
                "local-gateway",
            ])
            .with_dependencies(&[IPFS_GATEWAY_PLUGIN])
            .with_privacy_boundary(
                "retrieves IPFS/IPNS through an explicitly configured local gateway; no public gateway fallback by default",
            )
            .with_resource_profile(ResourceProfile::Medium)
    }

    fn install_plugins(&self, registry: &mut PluginRegistry) -> Vec<PluginInstallReport> {
        self.install_adapter_plugins(registry)
    }

    fn http_transport_for_url(&self, url: &Url) -> Option<Result<String, BroadwebdError>> {
        match url.scheme() {
            "ipfs" | "ipns" => Some(Ok(IPFS_GATEWAY_PLUGIN.to_string())),
            _ => None,
        }
    }
}

impl Default for IpfsService {
    fn default() -> Self {
        Self::new(IpfsConfig::default())
    }
}
