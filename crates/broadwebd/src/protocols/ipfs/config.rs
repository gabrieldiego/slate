use crate::{
    BroadwebdError, DEFAULT_IPFS_GATEWAY, SLATE_IPFS_GATEWAY_ENV, SLATE_IPFS_GATEWAY_SCOPE_ENV,
};
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

    pub fn from_environment() -> Result<Self, BroadwebdError> {
        let gateway = std::env::var(SLATE_IPFS_GATEWAY_ENV).ok();
        let scope = std::env::var(SLATE_IPFS_GATEWAY_SCOPE_ENV).ok();
        Self::from_options(gateway.as_deref(), scope.as_deref())
    }

    pub fn from_options(
        gateway_base: Option<&str>,
        scope: Option<&str>,
    ) -> Result<Self, BroadwebdError> {
        let gateway_base = non_empty_trimmed(gateway_base);
        let scope = parse_gateway_scope(scope)?;

        match (gateway_base, scope) {
            (Some(gateway_base), IpfsGatewayScope::Local) => Self::with_local_gateway(gateway_base),
            (Some(gateway_base), IpfsGatewayScope::Public) => {
                Self::with_public_gateway(gateway_base)
            }
            (None, IpfsGatewayScope::Local) => Self::default_local_gateway(),
            (None, IpfsGatewayScope::Public) => Err(BroadwebdError::UnsupportedRequest(format!(
                "{SLATE_IPFS_GATEWAY_SCOPE_ENV}=public requires {SLATE_IPFS_GATEWAY_ENV}"
            ))),
        }
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
        Self::default_local_gateway().expect("default IPFS gateway should be loopback HTTP")
    }
}

fn non_empty_trimmed(input: Option<&str>) -> Option<&str> {
    input.map(str::trim).filter(|value| !value.is_empty())
}

fn parse_gateway_scope(scope: Option<&str>) -> Result<IpfsGatewayScope, BroadwebdError> {
    match non_empty_trimmed(scope) {
        None => Ok(IpfsGatewayScope::Local),
        Some(scope) if scope.eq_ignore_ascii_case("local") => Ok(IpfsGatewayScope::Local),
        Some(scope) if scope.eq_ignore_ascii_case("public") => Ok(IpfsGatewayScope::Public),
        Some(scope) => Err(BroadwebdError::UnsupportedRequest(format!(
            "unsupported {SLATE_IPFS_GATEWAY_SCOPE_ENV}: {scope}; expected local or public"
        ))),
    }
}

impl IpfsConfig {
    fn default_local_gateway() -> Result<Self, BroadwebdError> {
        Self::new(DEFAULT_IPFS_GATEWAY)
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
