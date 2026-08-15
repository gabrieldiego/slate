use super::{IpfsConfig, IpfsGatewayTransport, IpfsKuboRpcTransport, IpfsTransportKind};
use crate::{
    BroadwebdError, IPFS_GATEWAY_PLUGIN, IPFS_KUBO_RPC_PLUGIN, IPFS_PROTOCOL_SERVICE,
    PluginInstallReport, PluginKind, PluginMetadata, PluginRegistry, ProtocolService,
    ResourceProfile,
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
        match self.config.transport() {
            IpfsTransportKind::Gateway => {
                vec![
                    registry.install_transport(
                        IpfsGatewayTransport::from_config(&self.config)
                            .expect("validated IPFS config should provide gateway candidates"),
                    ),
                ]
            }
            IpfsTransportKind::KuboRpc => vec![
                registry.install_transport(IpfsKuboRpcTransport::from_endpoint(
                    self.config
                        .kubo_rpc_endpoint()
                        .expect("Kubo RPC config should include an endpoint")
                        .clone(),
                )),
            ],
        }
    }

    fn dependency(&self) -> &'static str {
        match self.config.transport() {
            IpfsTransportKind::Gateway => IPFS_GATEWAY_PLUGIN,
            IpfsTransportKind::KuboRpc => IPFS_KUBO_RPC_PLUGIN,
        }
    }
}

impl ProtocolService for IpfsService {
    fn metadata(&self) -> PluginMetadata {
        let capabilities = match self.config.transport() {
            IpfsTransportKind::Gateway if self.config.uses_public_gateway() => [
                "ipfs",
                "ipns",
                "application/http-response",
                "public-gateway",
                "public-gateway-fallback",
            ],
            IpfsTransportKind::Gateway => [
                "ipfs",
                "ipns",
                "application/http-response",
                "local-gateway",
                "public-gateway-fallback",
            ],
            IpfsTransportKind::KuboRpc => [
                "ipfs",
                "ipns",
                "application/http-response",
                "local-kubo-rpc",
                "local-only",
            ],
        };
        let privacy_boundary = match self.config.transport() {
            IpfsTransportKind::Gateway if self.config.uses_public_gateway() => {
                "retrieves IPFS/IPNS through an explicitly configured public gateway list"
            }
            IpfsTransportKind::Gateway => {
                "retrieves IPFS/IPNS through a local gateway first, with configured public gateway fallback when local retrieval fails"
            }
            IpfsTransportKind::KuboRpc => {
                "retrieves IPFS/IPNS through an explicitly configured local Kubo RPC endpoint; no public gateway fallback by default"
            }
        };
        PluginMetadata::new(IPFS_PROTOCOL_SERVICE, PluginKind::ProtocolService)
            .with_capabilities(&capabilities)
            .with_dependencies(&[self.dependency()])
            .with_privacy_boundary(privacy_boundary)
            .with_resource_profile(ResourceProfile::Medium)
    }

    fn install_plugins(&self, registry: &mut PluginRegistry) -> Vec<PluginInstallReport> {
        self.install_adapter_plugins(registry)
    }

    fn http_transport_for_url(&self, url: &Url) -> Option<Result<String, BroadwebdError>> {
        match url.scheme() {
            "ipfs" | "ipns" => Some(Ok(self.config.http_transport_id().to_string())),
            _ => None,
        }
    }
}

impl Default for IpfsService {
    fn default() -> Self {
        Self::new(IpfsConfig::default())
    }
}
