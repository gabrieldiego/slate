use super::{IpfsConfig, IpfsGatewayTransport};
use crate::{PluginInstallReport, PluginRegistry};

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

    pub fn install_plugins(&self, registry: &mut PluginRegistry) -> Vec<PluginInstallReport> {
        vec![registry.install_transport(IpfsGatewayTransport::new(self.config.gateway_base()))]
    }
}

impl Default for IpfsService {
    fn default() -> Self {
        Self::new(IpfsConfig::default())
    }
}
