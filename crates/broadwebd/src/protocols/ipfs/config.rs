use super::IpfsKuboRpcEndpoint;
use crate::{
    BroadwebdError, DEFAULT_IPFS_GATEWAY, DEFAULT_IPFS_KUBO_RPC_API, DEFAULT_PUBLIC_IPFS_GATEWAY,
    DEFAULT_PUBLIC_IPFS_GATEWAYS, IPFS_GATEWAY_PLUGIN, IPFS_KUBO_RPC_PLUGIN,
    SLATE_IPFS_GATEWAY_ENV, SLATE_IPFS_GATEWAY_SCOPE_ENV, SLATE_IPFS_KUBO_RPC_ENV,
    SLATE_IPFS_TRANSPORT_ENV,
};
use url::Url;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IpfsGatewayScope {
    Local,
    Public,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IpfsTransportKind {
    Gateway,
    KuboRpc,
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
    public_gateway_fallbacks: Vec<IpfsGatewayEndpoint>,
    kubo_rpc: Option<IpfsKuboRpcEndpoint>,
    transport: IpfsTransportKind,
    allow_public_gateway_fallback: bool,
}

impl IpfsConfig {
    pub fn new(gateway_base: impl Into<String>) -> Result<Self, BroadwebdError> {
        Self::with_local_gateway(gateway_base)
    }

    pub fn from_environment() -> Result<Self, BroadwebdError> {
        let gateway = std::env::var(SLATE_IPFS_GATEWAY_ENV).ok();
        let scope = std::env::var(SLATE_IPFS_GATEWAY_SCOPE_ENV).ok();
        let transport = std::env::var(SLATE_IPFS_TRANSPORT_ENV).ok();
        let kubo_rpc = std::env::var(SLATE_IPFS_KUBO_RPC_ENV).ok();
        Self::from_runtime_options(
            gateway.as_deref(),
            scope.as_deref(),
            transport.as_deref(),
            kubo_rpc.as_deref(),
        )
    }

    pub fn from_options(
        gateway_base: Option<&str>,
        scope: Option<&str>,
    ) -> Result<Self, BroadwebdError> {
        Self::from_gateway_options(gateway_base, scope)
    }

    pub fn from_runtime_options(
        gateway_base: Option<&str>,
        scope: Option<&str>,
        transport: Option<&str>,
        kubo_rpc_api: Option<&str>,
    ) -> Result<Self, BroadwebdError> {
        let transport = parse_transport_kind(transport, kubo_rpc_api)?;
        match transport {
            IpfsTransportKind::Gateway => Self::from_gateway_options(gateway_base, scope),
            IpfsTransportKind::KuboRpc => {
                Self::from_kubo_rpc_options(gateway_base, scope, non_empty_trimmed(kubo_rpc_api))
            }
        }
    }

    fn from_gateway_options(
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
            (None, IpfsGatewayScope::Public) => Self::with_default_public_gateway(),
        }
    }

    fn from_kubo_rpc_options(
        gateway_base: Option<&str>,
        scope: Option<&str>,
        kubo_rpc_api: Option<&str>,
    ) -> Result<Self, BroadwebdError> {
        if non_empty_trimmed(gateway_base).is_some() || non_empty_trimmed(scope).is_some() {
            return Err(BroadwebdError::UnsupportedRequest(format!(
                "{SLATE_IPFS_TRANSPORT_ENV}=kubo-rpc cannot be combined with {SLATE_IPFS_GATEWAY_ENV} or {SLATE_IPFS_GATEWAY_SCOPE_ENV}"
            )));
        }
        Self::with_kubo_rpc(kubo_rpc_api.unwrap_or(DEFAULT_IPFS_KUBO_RPC_API))
    }

    pub fn with_local_gateway(gateway_base: impl Into<String>) -> Result<Self, BroadwebdError> {
        let gateway = IpfsGatewayEndpoint::local(gateway_base)?;
        let allow_public_gateway_fallback = !is_internal_fixture_gateway(gateway.base_url());
        let public_gateway_fallbacks = if allow_public_gateway_fallback {
            public_gateway_fallbacks_excluding(gateway.base_url())?
        } else {
            Vec::new()
        };
        Ok(Self {
            gateway,
            public_gateway_fallbacks,
            kubo_rpc: None,
            transport: IpfsTransportKind::Gateway,
            allow_public_gateway_fallback,
        })
    }

    pub fn with_public_gateway(gateway_base: impl Into<String>) -> Result<Self, BroadwebdError> {
        let gateway = IpfsGatewayEndpoint::public(gateway_base)?;
        Ok(Self {
            public_gateway_fallbacks: public_gateway_fallbacks_excluding(gateway.base_url())?,
            gateway,
            kubo_rpc: None,
            transport: IpfsTransportKind::Gateway,
            allow_public_gateway_fallback: true,
        })
    }

    pub fn with_default_public_gateway() -> Result<Self, BroadwebdError> {
        Self::with_public_gateway(DEFAULT_PUBLIC_IPFS_GATEWAY)
    }

    pub fn with_kubo_rpc(api_base_url: impl Into<String>) -> Result<Self, BroadwebdError> {
        Ok(Self {
            gateway: IpfsGatewayEndpoint::local(DEFAULT_IPFS_GATEWAY)?,
            public_gateway_fallbacks: public_gateway_fallbacks_excluding(DEFAULT_IPFS_GATEWAY)?,
            kubo_rpc: Some(IpfsKuboRpcEndpoint::local(api_base_url)?),
            transport: IpfsTransportKind::KuboRpc,
            allow_public_gateway_fallback: false,
        })
    }

    #[cfg(any(test, feature = "test-fixtures"))]
    pub(crate) fn with_prevalidated_kubo_rpc(
        api_base_url: impl Into<String>,
    ) -> Result<Self, BroadwebdError> {
        Ok(Self {
            gateway: IpfsGatewayEndpoint::local(DEFAULT_IPFS_GATEWAY)?,
            public_gateway_fallbacks: Vec::new(),
            kubo_rpc: Some(IpfsKuboRpcEndpoint::from_prevalidated_api_base_url(
                api_base_url,
            )),
            transport: IpfsTransportKind::KuboRpc,
            allow_public_gateway_fallback: false,
        })
    }

    pub fn gateway_endpoint(&self) -> &IpfsGatewayEndpoint {
        &self.gateway
    }

    pub fn gateway_candidates(&self) -> Vec<IpfsGatewayEndpoint> {
        let mut candidates = vec![self.gateway.clone()];
        if self.allow_public_gateway_fallback {
            candidates.extend(self.public_gateway_fallbacks.iter().cloned());
        }
        candidates
    }

    pub fn public_gateway_fallbacks(&self) -> &[IpfsGatewayEndpoint] {
        &self.public_gateway_fallbacks
    }

    pub fn gateway_base(&self) -> &str {
        self.gateway.base_url()
    }

    pub fn gateway_scope(&self) -> IpfsGatewayScope {
        self.gateway.scope()
    }

    pub fn kubo_rpc_endpoint(&self) -> Option<&IpfsKuboRpcEndpoint> {
        self.kubo_rpc.as_ref()
    }

    pub fn transport(&self) -> IpfsTransportKind {
        self.transport
    }

    pub fn http_transport_id(&self) -> &'static str {
        match self.transport {
            IpfsTransportKind::Gateway => IPFS_GATEWAY_PLUGIN,
            IpfsTransportKind::KuboRpc => IPFS_KUBO_RPC_PLUGIN,
        }
    }

    pub fn uses_public_gateway(&self) -> bool {
        matches!(self.transport, IpfsTransportKind::Gateway)
            && matches!(self.gateway_scope(), IpfsGatewayScope::Public)
    }

    pub fn uses_kubo_rpc(&self) -> bool {
        matches!(self.transport, IpfsTransportKind::KuboRpc)
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

fn parse_transport_kind(
    transport: Option<&str>,
    kubo_rpc_api: Option<&str>,
) -> Result<IpfsTransportKind, BroadwebdError> {
    match non_empty_trimmed(transport) {
        None if non_empty_trimmed(kubo_rpc_api).is_some() => Ok(IpfsTransportKind::KuboRpc),
        None => Ok(IpfsTransportKind::Gateway),
        Some(value) if value.eq_ignore_ascii_case("gateway") => Ok(IpfsTransportKind::Gateway),
        Some(value)
            if value.eq_ignore_ascii_case("kubo-rpc")
                || value.eq_ignore_ascii_case("kubo")
                || value.eq_ignore_ascii_case("local-kubo-rpc") =>
        {
            Ok(IpfsTransportKind::KuboRpc)
        }
        Some(value) => Err(BroadwebdError::UnsupportedRequest(format!(
            "unsupported {SLATE_IPFS_TRANSPORT_ENV}: {value}; expected gateway or kubo-rpc"
        ))),
    }
}

fn public_gateway_fallbacks_excluding(
    primary_gateway: &str,
) -> Result<Vec<IpfsGatewayEndpoint>, BroadwebdError> {
    DEFAULT_PUBLIC_IPFS_GATEWAYS
        .iter()
        .copied()
        .filter(|gateway| !gateway.eq_ignore_ascii_case(primary_gateway))
        .map(IpfsGatewayEndpoint::public)
        .collect()
}

impl IpfsConfig {
    fn default_local_gateway() -> Result<Self, BroadwebdError> {
        Self::new(DEFAULT_IPFS_GATEWAY)
    }
}

fn validate_gateway_url(gateway_base: &str, scope: IpfsGatewayScope) -> Result<(), BroadwebdError> {
    let url =
        Url::parse(gateway_base).map_err(|error| BroadwebdError::InvalidUrl(error.to_string()))?;
    #[cfg(any(test, feature = "test-fixtures"))]
    if crate::http::is_internal_fixture_http_url(&url) {
        return match scope {
            IpfsGatewayScope::Local => Ok(()),
            IpfsGatewayScope::Public => Err(BroadwebdError::UnsupportedRequest(format!(
                "public IPFS gateway mode cannot use an internal fixture gateway: {gateway_base}"
            ))),
        };
    }

    if !matches!(url.scheme(), "http" | "https") {
        return Err(BroadwebdError::UnsupportedRequest(format!(
            "unsupported IPFS gateway scheme: {}",
            url.scheme()
        )));
    }

    let host = url.host_str().ok_or_else(|| {
        BroadwebdError::InvalidUrl(format!("{gateway_base} is missing a gateway host"))
    })?;
    let is_loopback = is_numeric_loopback_host(host);
    match (scope, is_loopback) {
        (IpfsGatewayScope::Local, true) => Ok(()),
        (IpfsGatewayScope::Local, false) => Err(BroadwebdError::UnsupportedRequest(format!(
            "local IPFS gateway must use a numeric loopback address: {gateway_base}"
        ))),
        (IpfsGatewayScope::Public, false) => Ok(()),
        (IpfsGatewayScope::Public, true) => Err(BroadwebdError::UnsupportedRequest(format!(
            "public IPFS gateway mode requires a non-loopback gateway: {gateway_base}"
        ))),
    }
}

fn is_numeric_loopback_host(host: &str) -> bool {
    let host_for_parse = host
        .strip_prefix('[')
        .and_then(|host| host.strip_suffix(']'))
        .unwrap_or(host);
    host_for_parse
        .parse::<std::net::IpAddr>()
        .is_ok_and(|address| address.is_loopback())
}

#[cfg(any(test, feature = "test-fixtures"))]
fn is_internal_fixture_gateway(gateway_base: &str) -> bool {
    Url::parse(gateway_base)
        .ok()
        .is_some_and(|url| crate::http::is_internal_fixture_http_url(&url))
}

#[cfg(not(any(test, feature = "test-fixtures")))]
fn is_internal_fixture_gateway(_gateway_base: &str) -> bool {
    false
}
