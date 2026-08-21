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
#[cfg(any(test, feature = "test-fixtures"))]
const INTERNAL_KUBO_RPC_FIXTURE_SCHEME: &str = "slate-fixture-kubo";

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

    fn is_internal_fixture(&self) -> bool {
        #[cfg(any(test, feature = "test-fixtures"))]
        {
            Url::parse(self.api_base_url.as_str())
                .ok()
                .is_some_and(|url| is_internal_kubo_rpc_fixture_url(&url))
        }
        #[cfg(not(any(test, feature = "test-fixtures")))]
        {
            false
        }
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
        let (capabilities, privacy_boundary): (&[&str], &str) = if self
            .endpoint
            .is_internal_fixture()
        {
            (
                &[
                    "ipfs",
                    "ipns",
                    "http-fetch",
                    "in-process-fixture",
                    "socketless-fixture",
                ],
                "in-process Kubo RPC fixture; no sockets, DNS, loopback listener, or external network",
            )
        } else {
            (
                &["ipfs", "ipns", "http-fetch", "local-kubo-rpc"],
                "local Kubo RPC over HTTP; sends requested CIDs and IPNS names to the local node",
            )
        };
        PluginMetadata::new(IPFS_KUBO_RPC_PLUGIN, PluginKind::Transport)
            .with_capabilities(capabilities)
            .with_privacy_boundary(privacy_boundary)
            .with_resource_profile(ResourceProfile::Medium)
    }

    fn fetch_http(
        &self,
        request: &TransportHttpRequest,
        budget: &ResourceBudget,
    ) -> Result<HttpFetchResponse, BroadwebdError> {
        let client = if self.endpoint.is_internal_fixture() {
            None
        } else {
            Some(
                reqwest::blocking::Client::builder()
                    .timeout(REQUEST_TIMEOUT)
                    .user_agent(USER_AGENT)
                    .build()
                    .map_err(request_error)?,
            )
        };

        let mut last_response = None;
        for candidate in ipfs_content_path_candidates(&request.url)? {
            let cat_url =
                kubo_cat_url_for_path(&candidate.content_path, self.endpoint.api_base_url())?;
            let url = parse_http_url(&cat_url)?;

            #[cfg(any(test, feature = "test-fixtures"))]
            if is_internal_kubo_rpc_fixture_url(&url) {
                let fetch_response =
                    fetch_internal_kubo_rpc_fixture(&url, &candidate.document_url, budget)?;
                if (200..=299).contains(&fetch_response.status_code) {
                    return Ok(fetch_response);
                }
                last_response = Some(fetch_response);
                continue;
            }

            let response = client
                .as_ref()
                .expect("non-fixture Kubo RPC fetch should have an HTTP client")
                .post(url)
                .send()
                .map_err(request_error)?;
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
                &candidate.document_url,
                headers
                    .iter()
                    .find(|header| header.name.eq_ignore_ascii_case("content-type"))
                    .map(|header| header.value.as_str()),
                &body,
            );
            let fetch_response = HttpFetchResponse::new(
                candidate.document_url,
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
    kubo_cat_url_for_path(&content_path.content_path, api_base_url)
}

fn kubo_cat_url_for_path(content_path: &str, api_base_url: &str) -> Result<String, BroadwebdError> {
    let mut url = parse_http_url(api_base_url)?;
    let api_path = format!("{}/api/v0/cat", url.path().trim_end_matches('/'));
    url.set_path(&api_path);
    url.set_query(None);
    url.query_pairs_mut().append_pair("arg", &content_path);
    Ok(url.to_string())
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct IpfsContentPathCandidate {
    content_path: String,
    document_url: String,
}

fn ipfs_content_path_candidates(
    source: &str,
) -> Result<Vec<IpfsContentPathCandidate>, BroadwebdError> {
    let parts = ipfs_url_parts(source)?;
    let mut content_path = format!("/{}/{}", parts.namespace, parts.name);
    let mut document_url = format!("{}://{}", parts.namespace, parts.name);
    if parts.path != "/" {
        content_path.push_str(parts.path);
        document_url.push_str(parts.path);
    }
    if let Some(query) = parts.query {
        document_url.push('?');
        document_url.push_str(query);
    }
    let mut candidates = vec![IpfsContentPathCandidate {
        content_path: content_path.clone(),
        document_url,
    }];
    if should_try_directory_index(parts.path) {
        let index_content_path = format!("{}/index.html", content_path.trim_end_matches('/'));
        let mut index_document_url = format!(
            "{}://{}{}",
            parts.namespace,
            parts.name,
            index_document_path(parts.path)
        );
        if let Some(query) = parts.query {
            index_document_url.push('?');
            index_document_url.push_str(query);
        }
        candidates.push(IpfsContentPathCandidate {
            content_path: index_content_path,
            document_url: index_document_url,
        });
    }
    Ok(candidates)
}

fn index_document_path(path: &str) -> String {
    if path == "/" {
        "/index.html".to_string()
    } else {
        format!("{}/index.html", path.trim_end_matches('/'))
    }
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
    #[cfg(any(test, feature = "test-fixtures"))]
    if is_internal_kubo_rpc_fixture_url(&url) {
        return Ok(());
    }

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

#[cfg(any(test, feature = "test-fixtures"))]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InternalKuboRpcResponse {
    pub status_code: u16,
    pub content_type: String,
    pub body: Vec<u8>,
}

#[cfg(any(test, feature = "test-fixtures"))]
pub(crate) fn register_internal_kubo_rpc_fixture_for_network(
    network_id: &str,
    responses: Vec<InternalKuboRpcResponse>,
) -> String {
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT_FIXTURE_ID: AtomicUsize = AtomicUsize::new(1);

    let id = NEXT_FIXTURE_ID.fetch_add(1, Ordering::Relaxed);
    let base_url = if network_id == "global" {
        format!("{INTERNAL_KUBO_RPC_FIXTURE_SCHEME}://fixture-{id}")
    } else {
        format!("{INTERNAL_KUBO_RPC_FIXTURE_SCHEME}://{network_id}/fixture-{id}")
    };
    internal_kubo_rpc_fixtures()
        .lock()
        .expect("internal Kubo fixture registry should not be poisoned")
        .insert(
            base_url.clone(),
            InternalKuboRpcFixture {
                responses: responses.into(),
                requests: Vec::new(),
            },
        );
    base_url
}

#[cfg(any(test, feature = "test-fixtures"))]
pub(crate) fn take_internal_kubo_rpc_fixture_requests(base_url: &str) -> Vec<String> {
    internal_kubo_rpc_fixtures()
        .lock()
        .expect("internal Kubo fixture registry should not be poisoned")
        .remove(base_url)
        .map(|fixture| fixture.requests)
        .unwrap_or_default()
}

#[cfg(any(test, feature = "test-fixtures"))]
#[derive(Clone, Debug, Eq, PartialEq)]
struct InternalKuboRpcFixture {
    responses: std::collections::VecDeque<InternalKuboRpcResponse>,
    requests: Vec<String>,
}

#[cfg(any(test, feature = "test-fixtures"))]
fn is_internal_kubo_rpc_fixture_url(url: &Url) -> bool {
    if url.scheme() != INTERNAL_KUBO_RPC_FIXTURE_SCHEME {
        return false;
    }
    let Some(host) = url.host_str() else {
        return false;
    };
    if host.starts_with("fixture-") {
        return true;
    }
    internal_kubo_rpc_path_token(url)
        .as_deref()
        .is_some_and(|token| token.starts_with("fixture-"))
}

#[cfg(any(test, feature = "test-fixtures"))]
pub(crate) fn internal_kubo_rpc_url_belongs_to_network(url: &Url, network_id: &str) -> bool {
    if !is_internal_kubo_rpc_fixture_url(url) {
        return false;
    }
    let Some(url_network_id) = internal_kubo_rpc_network_id(url) else {
        return network_id == "global";
    };
    url_network_id == network_id
}

#[cfg(any(test, feature = "test-fixtures"))]
fn fetch_internal_kubo_rpc_fixture(
    url: &Url,
    document_url: &str,
    budget: &ResourceBudget,
) -> Result<HttpFetchResponse, BroadwebdError> {
    let base_url = internal_kubo_rpc_base_url(url)?;
    let request_target = match url.query() {
        Some(query) => format!("POST /api/v0/cat?{query} HTTP/1.1"),
        None => "POST /api/v0/cat HTTP/1.1".to_string(),
    };

    let mut fixtures = internal_kubo_rpc_fixtures()
        .lock()
        .expect("internal Kubo fixture registry should not be poisoned");
    let fixture = fixtures.get_mut(base_url.as_str()).ok_or_else(|| {
        BroadwebdError::Request(format!("missing internal Kubo fixture {base_url}"))
    })?;
    fixture.requests.push(request_target);
    let response = fixture.responses.pop_front().ok_or_else(|| {
        BroadwebdError::Request(format!("internal Kubo fixture {base_url} has no response"))
    })?;

    if response.body.len() > budget.max_http_response_bytes {
        return Err(BroadwebdError::ResponseTooLarge {
            limit: budget.max_http_response_bytes,
            actual: response.body.len(),
        });
    }

    let headers = vec![HttpHeader {
        name: "content-type".to_string(),
        value: response.content_type.clone(),
    }];
    let content_type = infer_content_type(
        document_url,
        Some(response.content_type.as_str()),
        response.body.as_slice(),
    );
    Ok(HttpFetchResponse::new(
        document_url.to_string(),
        response.status_code,
        content_type,
        headers,
        response.body,
    ))
}

#[cfg(any(test, feature = "test-fixtures"))]
fn internal_kubo_rpc_base_url(url: &Url) -> Result<String, BroadwebdError> {
    if url.scheme() == INTERNAL_KUBO_RPC_FIXTURE_SCHEME {
        let host = url.host_str().ok_or_else(|| {
            BroadwebdError::InvalidUrl(format!("invalid internal Kubo fixture URL: {url}"))
        })?;
        if host.starts_with("fixture-") {
            return Ok(format!("{}://{}", url.scheme(), host));
        }
    }

    let fixture_segment = url
        .path_segments()
        .and_then(|mut segments| segments.next().map(str::to_string))
        .ok_or_else(|| {
            BroadwebdError::InvalidUrl(format!("invalid internal Kubo fixture URL: {url}"))
        })?;
    Ok(format!(
        "{}://{}/{}",
        url.scheme(),
        url.host_str().unwrap_or_default(),
        fixture_segment
    ))
}

#[cfg(any(test, feature = "test-fixtures"))]
fn internal_kubo_rpc_network_id(url: &Url) -> Option<String> {
    let host = url.host_str()?;
    if host.starts_with("fixture-") {
        return None;
    }
    internal_kubo_rpc_path_token(url).map(|_| host.to_string())
}

#[cfg(any(test, feature = "test-fixtures"))]
fn internal_kubo_rpc_path_token(url: &Url) -> Option<String> {
    url.path_segments()
        .and_then(|mut segments| segments.next().map(str::to_string))
}

#[cfg(any(test, feature = "test-fixtures"))]
fn internal_kubo_rpc_fixtures()
-> &'static std::sync::Mutex<std::collections::BTreeMap<String, InternalKuboRpcFixture>> {
    use std::collections::BTreeMap;
    use std::sync::{Mutex, OnceLock};

    static FIXTURES: OnceLock<Mutex<BTreeMap<String, InternalKuboRpcFixture>>> = OnceLock::new();

    FIXTURES.get_or_init(|| Mutex::new(BTreeMap::new()))
}
