use crate::{BroadwebdError, DEFAULT_IPFS_GATEWAY};
use url::Url;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IpfsConfig {
    gateway_base: String,
    allow_public_gateway_fallback: bool,
}

impl IpfsConfig {
    pub fn new(gateway_base: impl Into<String>) -> Result<Self, BroadwebdError> {
        let gateway_base = gateway_base.into();
        validate_local_gateway(&gateway_base)?;
        Ok(Self {
            gateway_base,
            allow_public_gateway_fallback: false,
        })
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
        Self::new(DEFAULT_IPFS_GATEWAY).expect("default IPFS gateway should be loopback HTTP")
    }
}

fn validate_local_gateway(gateway_base: &str) -> Result<(), BroadwebdError> {
    let url =
        Url::parse(gateway_base).map_err(|error| BroadwebdError::InvalidUrl(error.to_string()))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(BroadwebdError::UnsupportedRequest(format!(
            "unsupported IPFS gateway scheme: {}",
            url.scheme()
        )));
    }

    let host = url.host_str().ok_or_else(|| {
        BroadwebdError::InvalidUrl(format!("{gateway_base} is missing a gateway host"))
    })?;
    if matches!(host, "localhost" | "127.0.0.1" | "::1") {
        return Ok(());
    }

    Err(BroadwebdError::UnsupportedRequest(format!(
        "public IPFS gateway requires explicit browser policy: {gateway_base}"
    )))
}
