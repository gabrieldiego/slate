use super::address::ipfs_url_parts;
use crate::http::{infer_content_type, parse_http_url};
use crate::{
    BroadwebdError, DEFAULT_IPFS_KUBO_RPC_API, HttpFetchResponse, HttpHeader, IPFS_KUBO_RPC_PLUGIN,
    PluginKind, PluginMetadata, ResourceBudget, ResourceProfile, TransportHttpRequest,
    TransportPlugin,
};
use std::time::Duration;
use url::Url;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(12);
const USER_AGENT: &str = "Slate/0.0.1";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IpfsKuboRpcEndpoint {
    api_base_url: String,
}

impl IpfsKuboRpcEndpoint {
    pub fn local(api_base_url: impl Into<String>) -> Result<Self, BroadwebdError> {
        let api_base_url = api_base_url.into();
        validate_kubo_rpc_url(&api_base_url)?;
        Ok(Self { api_base_url })
    }

    pub fn api_base_url(&self) -> &str {
        &self.api_base_url
    }
}

impl Default for IpfsKuboRpcEndpoint {
    fn default() -> Self {
        Self::local(DEFAULT_IPFS_KUBO_RPC_API)
            .expect("default Kubo RPC API should be loopback HTTP")
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IpfsKuboRpcTransport {
    endpoint: IpfsKuboRpcEndpoint,
}

impl IpfsKuboRpcTransport {
    pub fn local(api_base_url: impl Into<String>) -> Result<Self, BroadwebdError> {
        Ok(Self::from_endpoint(IpfsKuboRpcEndpoint::local(
            api_base_url,
        )?))
    }

    pub fn from_endpoint(endpoint: IpfsKuboRpcEndpoint) -> Self {
        Self { endpoint }
    }

    pub fn endpoint(&self) -> &IpfsKuboRpcEndpoint {
        &self.endpoint
    }
}

impl Default for IpfsKuboRpcTransport {
    fn default() -> Self {
        Self::from_endpoint(IpfsKuboRpcEndpoint::default())
    }
}

impl TransportPlugin for IpfsKuboRpcTransport {
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata::new(IPFS_KUBO_RPC_PLUGIN, PluginKind::Transport)
            .with_capabilities(&["ipfs", "ipns", "http-fetch", "local-kubo-rpc"])
            .with_privacy_boundary(
                "local Kubo RPC over HTTP; sends requested CIDs and IPNS names to the local node",
            )
            .with_resource_profile(ResourceProfile::Medium)
    }

    fn fetch_http(
        &self,
        request: &TransportHttpRequest,
        budget: &ResourceBudget,
    ) -> Result<HttpFetchResponse, BroadwebdError> {
        let client = reqwest::blocking::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .user_agent(USER_AGENT)
            .build()
            .map_err(request_error)?;

        let mut last_response = None;
        for content_path in ipfs_content_path_candidates(&request.url)? {
            let cat_url = kubo_cat_url_for_path(&content_path, self.endpoint.api_base_url())?;
            let url = parse_http_url(&cat_url)?;
            let response = client.post(url).send().map_err(request_error)?;
            let status_code = response.status().as_u16();
            let headers = response_headers(response.headers());
            let body = response.bytes().map_err(request_error)?.to_vec();
            if body.len() > budget.max_http_response_bytes {
                return Err(BroadwebdError::ResponseTooLarge {
                    limit: budget.max_http_response_bytes,
                    actual: body.len(),
                });
            }
            let content_type = infer_content_type(
                &request.url,
                headers
                    .iter()
                    .find(|header| header.name.eq_ignore_ascii_case("content-type"))
                    .map(|header| header.value.as_str()),
                &body,
            );
            let fetch_response = HttpFetchResponse::new(
                request.url.clone(),
                status_code,
                content_type,
                headers,
                body,
            );
            if (200..=299).contains(&fetch_response.status_code) {
                return Ok(fetch_response);
            }
            last_response = Some(fetch_response);
        }

        last_response.ok_or_else(|| {
            BroadwebdError::UnsupportedRequest(format!(
                "no Kubo RPC content path candidates for {}",
                request.url
            ))
        })
    }
}

pub fn ipfs_kubo_cat_url(source: &str, api_base_url: &str) -> Result<String, BroadwebdError> {
    let content_path = ipfs_content_path_candidates(source)?
        .into_iter()
        .next()
        .ok_or_else(|| BroadwebdError::InvalidUrl(format!("{source} is missing a content path")))?;
    kubo_cat_url_for_path(&content_path, api_base_url)
}

fn kubo_cat_url_for_path(content_path: &str, api_base_url: &str) -> Result<String, BroadwebdError> {
    let mut url = parse_http_url(api_base_url)?;
    let api_path = format!("{}/api/v0/cat", url.path().trim_end_matches('/'));
    url.set_path(&api_path);
    url.set_query(None);
    url.query_pairs_mut().append_pair("arg", &content_path);
    Ok(url.to_string())
}

fn ipfs_content_path_candidates(source: &str) -> Result<Vec<String>, BroadwebdError> {
    let parts = ipfs_url_parts(source)?;
    let mut content_path = format!("/{}/{}", parts.namespace, parts.name);
    if parts.path != "/" {
        content_path.push_str(parts.path);
    }
    let mut candidates = vec![content_path.clone()];
    if should_try_directory_index(parts.path) {
        candidates.push(format!("{}/index.html", content_path.trim_end_matches('/')));
    }
    Ok(candidates)
}

fn should_try_directory_index(path: &str) -> bool {
    path == "/"
        || path.ends_with('/')
        || path
            .rsplit('/')
            .next()
            .is_some_and(|last| !last.contains('.'))
}

fn validate_kubo_rpc_url(api_base_url: &str) -> Result<(), BroadwebdError> {
    let url =
        Url::parse(api_base_url).map_err(|error| BroadwebdError::InvalidUrl(error.to_string()))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(BroadwebdError::UnsupportedRequest(format!(
            "unsupported Kubo RPC scheme: {}",
            url.scheme()
        )));
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err(BroadwebdError::InvalidUrl(format!(
            "{api_base_url} must not include a query or fragment"
        )));
    }
    let host = url.host_str().ok_or_else(|| {
        BroadwebdError::InvalidUrl(format!("{api_base_url} is missing a Kubo RPC host"))
    })?;
    if matches!(host, "localhost" | "127.0.0.1" | "::1") {
        return Ok(());
    }

    Err(BroadwebdError::UnsupportedRequest(format!(
        "Kubo RPC endpoint must be loopback: {api_base_url}"
    )))
}

fn request_error(error: reqwest::Error) -> BroadwebdError {
    BroadwebdError::Request(error.to_string())
}

fn response_headers(headers: &reqwest::header::HeaderMap) -> Vec<HttpHeader> {
    headers
        .iter()
        .filter_map(|(name, value)| {
            Some(HttpHeader {
                name: name.as_str().to_string(),
                value: value.to_str().ok()?.to_string(),
            })
        })
        .collect()
}
