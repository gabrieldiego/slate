use super::{IpfsGatewayEndpoint, IpfsGatewayScope};
use crate::http::{fetch_http_url, parse_http_url};
use crate::{
    BroadwebdError, DEFAULT_IPFS_GATEWAY, HttpFetchResponse, IPFS_GATEWAY_PLUGIN, PluginKind,
    PluginMetadata, ResourceBudget, ResourceProfile, TransportHttpRequest, TransportPlugin,
};
use url::Url;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IpfsGatewayTransport {
    gateway: IpfsGatewayEndpoint,
}

impl IpfsGatewayTransport {
    pub fn local(gateway_base: impl Into<String>) -> Result<Self, BroadwebdError> {
        Ok(Self::from_endpoint(IpfsGatewayEndpoint::local(
            gateway_base,
        )?))
    }

    pub fn public(gateway_base: impl Into<String>) -> Result<Self, BroadwebdError> {
        Ok(Self::from_endpoint(IpfsGatewayEndpoint::public(
            gateway_base,
        )?))
    }

    pub fn from_endpoint(gateway: IpfsGatewayEndpoint) -> Self {
        Self { gateway }
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
}

impl Default for IpfsGatewayTransport {
    fn default() -> Self {
        Self::local(DEFAULT_IPFS_GATEWAY).expect("default IPFS gateway should be loopback HTTP")
    }
}

impl TransportPlugin for IpfsGatewayTransport {
    fn metadata(&self) -> PluginMetadata {
        let capabilities = match self.gateway_scope() {
            IpfsGatewayScope::Local => ["ipfs", "ipns", "http-fetch", "local-gateway"],
            IpfsGatewayScope::Public => ["ipfs", "ipns", "http-fetch", "public-gateway"],
        };
        let privacy_boundary = match self.gateway_scope() {
            IpfsGatewayScope::Local => {
                "local IPFS gateway over HTTP; no public gateway fallback by default"
            }
            IpfsGatewayScope::Public => {
                "explicit public IPFS gateway; requested CIDs, IPNS names, and client network metadata leave the machine"
            }
        };
        PluginMetadata::new(IPFS_GATEWAY_PLUGIN, PluginKind::Transport)
            .with_capabilities(&capabilities)
            .with_privacy_boundary(privacy_boundary)
            .with_resource_profile(ResourceProfile::Medium)
    }

    fn fetch_http(
        &self,
        request: &TransportHttpRequest,
        budget: &ResourceBudget,
    ) -> Result<HttpFetchResponse, BroadwebdError> {
        let gateway_url = ipfs_gateway_http_url(&request.url, self.gateway_base())?;
        let url = parse_http_url(&gateway_url)?;
        fetch_http_url(url, budget)
    }
}

pub fn ipfs_gateway_http_url(source: &str, gateway_base: &str) -> Result<String, BroadwebdError> {
    let parsed =
        Url::parse(source).map_err(|error| BroadwebdError::InvalidUrl(error.to_string()))?;
    let namespace = match parsed.scheme() {
        "ipfs" => "ipfs",
        "ipns" => "ipns",
        scheme => {
            return Err(BroadwebdError::UnsupportedRequest(format!(
                "unsupported IPFS gateway scheme: {scheme}"
            )));
        }
    };
    let name = parsed
        .host_str()
        .ok_or_else(|| BroadwebdError::InvalidUrl(format!("{source} is missing a content name")))?;
    let mut output = format!(
        "{}/{}/{}",
        gateway_base.trim_end_matches('/'),
        namespace,
        name
    );
    if parsed.path() != "/" {
        output.push_str(parsed.path());
    }
    if let Some(query) = parsed.query() {
        output.push('?');
        output.push_str(query);
    }
    Ok(output)
}
