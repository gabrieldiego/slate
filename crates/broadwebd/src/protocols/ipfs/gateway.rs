use super::{IpfsConfig, IpfsGatewayEndpoint, IpfsGatewayScope, address::ipfs_url_parts};
use crate::http::{fetch_http_url, parse_http_url};
use crate::{
    BroadwebdError, DEFAULT_IPFS_GATEWAY, FetchRouteInfo, HttpFetchResponse, IPFS_GATEWAY_PLUGIN,
    PluginKind, PluginMetadata, ResourceBudget, ResourceProfile, TransportHttpRequest,
    TransportPlugin,
};
use std::sync::Mutex;

pub struct IpfsGatewayTransport {
    gateways: Vec<IpfsGatewayEndpoint>,
    active_gateway: Mutex<usize>,
}

impl IpfsGatewayTransport {
    pub fn local(gateway_base: impl Into<String>) -> Result<Self, BroadwebdError> {
        Self::from_gateways(vec![IpfsGatewayEndpoint::local(gateway_base)?])
    }

    pub fn public(gateway_base: impl Into<String>) -> Result<Self, BroadwebdError> {
        Self::from_gateways(vec![IpfsGatewayEndpoint::public(gateway_base)?])
    }

    pub fn from_config(config: &IpfsConfig) -> Result<Self, BroadwebdError> {
        Self::from_gateways(config.gateway_candidates())
    }

    pub fn from_endpoint(gateway: IpfsGatewayEndpoint) -> Self {
        Self {
            gateways: vec![gateway],
            active_gateway: Mutex::new(0),
        }
    }

    pub fn from_gateways(gateways: Vec<IpfsGatewayEndpoint>) -> Result<Self, BroadwebdError> {
        if gateways.is_empty() {
            return Err(BroadwebdError::UnsupportedRequest(
                "IPFS gateway transport requires at least one gateway".to_string(),
            ));
        }
        Ok(Self {
            gateways,
            active_gateway: Mutex::new(0),
        })
    }

    pub fn gateway_endpoint(&self) -> &IpfsGatewayEndpoint {
        &self.gateways[0]
    }

    pub fn gateway_base(&self) -> &str {
        self.gateway_endpoint().base_url()
    }

    pub fn gateway_scope(&self) -> IpfsGatewayScope {
        self.gateway_endpoint().scope()
    }

    pub fn cached_gateway_base(&self) -> String {
        self.gateways[self.active_gateway_index()]
            .base_url()
            .to_string()
    }

    fn active_gateway_index(&self) -> usize {
        let index = *self
            .active_gateway
            .lock()
            .expect("IPFS gateway cache should not be poisoned");
        if index < self.gateways.len() {
            index
        } else {
            0
        }
    }

    fn set_active_gateway_index(&self, index: usize) {
        *self
            .active_gateway
            .lock()
            .expect("IPFS gateway cache should not be poisoned") = index;
    }

    fn gateway_attempt_order(&self) -> Vec<usize> {
        let start = self.active_gateway_index();
        (0..self.gateways.len())
            .map(|offset| (start + offset) % self.gateways.len())
            .collect()
    }

    fn has_public_gateway_fallback(&self) -> bool {
        self.gateways
            .iter()
            .skip(1)
            .any(|gateway| matches!(gateway.scope(), IpfsGatewayScope::Public))
    }
}

impl Default for IpfsGatewayTransport {
    fn default() -> Self {
        Self::local(DEFAULT_IPFS_GATEWAY).expect("default IPFS gateway should be loopback HTTP")
    }
}

impl TransportPlugin for IpfsGatewayTransport {
    fn metadata(&self) -> PluginMetadata {
        let capabilities = match (self.gateway_scope(), self.has_public_gateway_fallback()) {
            (IpfsGatewayScope::Local, true) => [
                "ipfs",
                "ipns",
                "http-fetch",
                "local-gateway",
                "public-gateway-fallback",
            ],
            (IpfsGatewayScope::Local, false) => [
                "ipfs",
                "ipns",
                "http-fetch",
                "local-gateway",
                "local-gateway-only",
            ],
            (IpfsGatewayScope::Public, true) => [
                "ipfs",
                "ipns",
                "http-fetch",
                "public-gateway",
                "public-gateway-fallback",
            ],
            (IpfsGatewayScope::Public, false) => [
                "ipfs",
                "ipns",
                "http-fetch",
                "public-gateway",
                "public-gateway-only",
            ],
        };
        let privacy_boundary = match (self.gateway_scope(), self.has_public_gateway_fallback()) {
            (IpfsGatewayScope::Local, true) => {
                "local IPFS gateway first; falls back to configured public IPFS gateways for IPFS/IPNS requests when local retrieval fails"
            }
            (IpfsGatewayScope::Local, false) => "local IPFS gateway over HTTP",
            (IpfsGatewayScope::Public, _) => {
                "explicit public IPFS gateway list; requested CIDs, IPNS names, and client network metadata leave the machine"
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
        let mut last_error = None;
        let mut last_response = None;

        for index in self.gateway_attempt_order() {
            let gateway = &self.gateways[index];
            let gateway_url = ipfs_gateway_http_url(&request.url, gateway.base_url())?;
            let url = parse_http_url(&gateway_url)?;
            match fetch_http_url(url, budget) {
                Ok(response) if is_usable_gateway_response(&response) => {
                    self.set_active_gateway_index(index);
                    return Ok(response.with_route(selected_gateway_route(request, gateway)));
                }
                Ok(response) => {
                    last_response =
                        Some(response.with_route(selected_gateway_route(request, gateway)));
                }
                Err(error) => {
                    last_error = Some(error);
                }
            }
        }

        self.set_active_gateway_index(0);
        if let Some(response) = last_response {
            return Ok(response);
        }
        Err(last_error.unwrap_or_else(|| {
            BroadwebdError::UnsupportedRequest("no IPFS gateways are configured".to_string())
        }))
    }
}

fn is_usable_gateway_response(response: &HttpFetchResponse) -> bool {
    response.status_code == 200 && !is_service_worker_gateway_bootstrap(response)
}

fn is_service_worker_gateway_bootstrap(response: &HttpFetchResponse) -> bool {
    let final_url = response.final_url.to_ascii_lowercase();
    if final_url.contains("://inbrowser.link/") || final_url.contains(".ipfs.inbrowser.link") {
        return true;
    }

    let is_html = response
        .content_type
        .as_deref()
        .map(|content_type| content_type.to_ascii_lowercase().contains("text/html"))
        .unwrap_or(false);
    if !is_html {
        return false;
    }

    let prefix_len = response.body.len().min(4096);
    let prefix = String::from_utf8_lossy(&response.body[..prefix_len]);
    prefix.contains("IPFS Service Worker Gateway") || prefix.contains("Service Worker Required")
}

fn selected_gateway_route(
    request: &TransportHttpRequest,
    gateway: &IpfsGatewayEndpoint,
) -> FetchRouteInfo {
    FetchRouteInfo::new(
        request.profile.clone(),
        IPFS_GATEWAY_PLUGIN,
        selected_gateway_privacy_boundary(gateway),
        request.purpose,
    )
}

fn selected_gateway_privacy_boundary(gateway: &IpfsGatewayEndpoint) -> String {
    match gateway.scope() {
        IpfsGatewayScope::Local => format!("local IPFS gateway over HTTP: {}", gateway.base_url()),
        IpfsGatewayScope::Public => format!(
            "public IPFS gateway: {}; requested CIDs, IPNS names, and client network metadata leave the machine",
            gateway.base_url()
        ),
    }
}

pub fn ipfs_gateway_http_url(source: &str, gateway_base: &str) -> Result<String, BroadwebdError> {
    let parts = ipfs_url_parts(source)?;
    let mut output = format!(
        "{}/{}/{}",
        gateway_base.trim_end_matches('/'),
        parts.namespace,
        parts.name
    );
    if parts.path != "/" {
        output.push_str(parts.path);
    }
    if let Some(query) = parts.query {
        output.push('?');
        output.push_str(query);
    }
    Ok(output)
}
