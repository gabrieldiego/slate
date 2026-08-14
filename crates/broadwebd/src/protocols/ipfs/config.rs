use crate::{BroadwebdError, DEFAULT_IPFS_GATEWAY};
use url::Url;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IpfsGatewayScope {
    Local,
    Public,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IpfsGatewayEndpoint {
    base_url: String,
    scope: IpfsGatewayScope,
}

impl IpfsGatewayEndpoint {
    pub fn local(base_url: impl Into<String>) -> Result<Self, BroadwebdError> {
        let base_url = base_url.into();
        validate_gateway_url(&base_url, IpfsGatewayScope::Local)?;
        Ok(Self {
            base_url,
            scope: IpfsGatewayScope::Local,
        })
    }

    pub fn public(base_url: impl Into<String>) -> Result<Self, BroadwebdError> {
        let base_url = base_url.into();
        validate_gateway_url(&base_url, IpfsGatewayScope::Public)?;
        Ok(Self {
            base_url,
            scope: IpfsGatewayScope::Public,
        })
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub fn scope(&self) -> IpfsGatewayScope {
        self.scope
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IpfsConfig {
    gateway: IpfsGatewayEndpoint,
    allow_public_gateway_fallback: bool,
}

impl IpfsConfig {
    pub fn new(gateway_base: impl Into<String>) -> Result<Self, BroadwebdError> {
        Self::with_local_gateway(gateway_base)
    }

    pub fn with_local_gateway(gateway_base: impl Into<String>) -> Result<Self, BroadwebdError> {
        Ok(Self {
            gateway: IpfsGatewayEndpoint::local(gateway_base)?,
            allow_public_gateway_fallback: false,
        })
    }

    pub fn with_public_gateway(gateway_base: impl Into<String>) -> Result<Self, BroadwebdError> {
        Ok(Self {
            gateway: IpfsGatewayEndpoint::public(gateway_base)?,
            allow_public_gateway_fallback: false,
        })
    }

    pub fn gateway_endpoint(&self) -> &IpfsGatewayEndpoint {
        &self.gateway
    }

    pub fn gateway_base(&self) -> &str {
        self.gateway.base_url()
    }

    pub fn gateway_scope(&self) -> IpfsGatewayScope {
        self.gateway.scope()
    }

    pub fn uses_public_gateway(&self) -> bool {
        matches!(self.gateway_scope(), IpfsGatewayScope::Public)
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

fn validate_gateway_url(gateway_base: &str, scope: IpfsGatewayScope) -> Result<(), BroadwebdError> {
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
    let is_loopback = matches!(host, "localhost" | "127.0.0.1" | "::1");
    match (scope, is_loopback) {
        (IpfsGatewayScope::Local, true) => Ok(()),
        (IpfsGatewayScope::Local, false) => Err(BroadwebdError::UnsupportedRequest(format!(
            "public IPFS gateway requires explicit browser policy: {gateway_base}"
        ))),
        (IpfsGatewayScope::Public, false) => Ok(()),
        (IpfsGatewayScope::Public, true) => Err(BroadwebdError::UnsupportedRequest(format!(
            "public IPFS gateway mode requires a non-loopback gateway: {gateway_base}"
        ))),
    }
}
