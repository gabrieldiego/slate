use crate::DEFAULT_IPFS_GATEWAY;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IpfsConfig {
    gateway_base: String,
    allow_public_gateway_fallback: bool,
}

impl IpfsConfig {
    pub fn new(gateway_base: impl Into<String>) -> Self {
        Self {
            gateway_base: gateway_base.into(),
            allow_public_gateway_fallback: false,
        }
    }

    pub fn gateway_base(&self) -> &str {
        &self.gateway_base
    }

    pub fn allow_public_gateway_fallback(&self) -> bool {
        self.allow_public_gateway_fallback
    }
}

impl Default for IpfsConfig {
    fn default() -> Self {
        Self::new(DEFAULT_IPFS_GATEWAY)
    }
}
