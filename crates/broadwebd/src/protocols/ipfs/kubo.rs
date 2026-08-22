use super::address::ipfs_url_parts;
use crate::http::{infer_content_type, parse_http_url};
use crate::{
    BroadwebdError, DEFAULT_IPFS_KUBO_RPC_API, HttpFetchResponse, HttpHeader, IPFS_KUBO_RPC_PLUGIN,
    PluginKind, PluginMetadata, ResourceBudget, ResourceProfile, TransportHttpRequest,
    TransportPlugin,
};
use serde::Deserialize;
use std::collections::BTreeMap;
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IpfsKuboProfileSyncRpc {
    endpoint: IpfsKuboRpcEndpoint,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IpfsKuboProfileSyncRpcRequest {
    operation: IpfsKuboProfileSyncOperation,
    url: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IpfsKuboProfileSyncOperation {
    PutEncryptedObject,
    RetainObject,
    ReleaseObject,
    VerifyRetainedObject,
    PublishRoot,
    ResolveRoot,
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

impl IpfsKuboProfileSyncRpc {
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

    pub fn put_encrypted_object_request(
        &self,
    ) -> Result<IpfsKuboProfileSyncRpcRequest, BroadwebdError> {
        Ok(IpfsKuboProfileSyncRpcRequest::new(
            IpfsKuboProfileSyncOperation::PutEncryptedObject,
            ipfs_kubo_profile_sync_add_url(self.endpoint.api_base_url())?,
        ))
    }

    pub fn retain_object_request(
        &self,
        object_id: &str,
    ) -> Result<IpfsKuboProfileSyncRpcRequest, BroadwebdError> {
        Ok(IpfsKuboProfileSyncRpcRequest::new(
            IpfsKuboProfileSyncOperation::RetainObject,
            ipfs_kubo_profile_sync_pin_add_url(object_id, self.endpoint.api_base_url())?,
        ))
    }

    pub fn release_object_request(
        &self,
        object_id: &str,
    ) -> Result<IpfsKuboProfileSyncRpcRequest, BroadwebdError> {
        Ok(IpfsKuboProfileSyncRpcRequest::new(
            IpfsKuboProfileSyncOperation::ReleaseObject,
            ipfs_kubo_profile_sync_pin_rm_url(object_id, self.endpoint.api_base_url())?,
        ))
    }

    pub fn verify_retained_object_request(
        &self,
        object_id: &str,
    ) -> Result<IpfsKuboProfileSyncRpcRequest, BroadwebdError> {
        Ok(IpfsKuboProfileSyncRpcRequest::new(
            IpfsKuboProfileSyncOperation::VerifyRetainedObject,
            ipfs_kubo_profile_sync_pin_ls_url(object_id, self.endpoint.api_base_url())?,
        ))
    }

    pub fn publish_root_request(
        &self,
        key_id: &str,
        object_id: &str,
    ) -> Result<IpfsKuboProfileSyncRpcRequest, BroadwebdError> {
        Ok(IpfsKuboProfileSyncRpcRequest::new(
            IpfsKuboProfileSyncOperation::PublishRoot,
            ipfs_kubo_profile_sync_name_publish_url(
                key_id,
                object_id,
                self.endpoint.api_base_url(),
            )?,
        ))
    }

    pub fn resolve_root_request(
        &self,
        name: &str,
    ) -> Result<IpfsKuboProfileSyncRpcRequest, BroadwebdError> {
        Ok(IpfsKuboProfileSyncRpcRequest::new(
            IpfsKuboProfileSyncOperation::ResolveRoot,
            ipfs_kubo_profile_sync_name_resolve_url(name, self.endpoint.api_base_url())?,
        ))
    }

    #[cfg(any(test, feature = "test-fixtures"))]
    pub fn put_encrypted_object_fixture(
        &self,
        object_bytes: &[u8],
        budget: &ResourceBudget,
    ) -> Result<String, BroadwebdError> {
        self.require_internal_fixture_endpoint()?;
        if object_bytes.len() > budget.max_profile_sync_object_bytes {
            return Err(BroadwebdError::ResponseTooLarge {
                limit: budget.max_profile_sync_object_bytes,
                actual: object_bytes.len(),
            });
        }

        let request = self.put_encrypted_object_request()?;
        let response = fetch_internal_kubo_profile_sync_fixture(&request, budget)?;
        require_kubo_profile_sync_success("add", response.status_code)?;
        ipfs_kubo_profile_sync_added_object_id(response.body.as_slice())
    }

    #[cfg(any(test, feature = "test-fixtures"))]
    pub fn retain_object_fixture(
        &self,
        object_id: &str,
        budget: &ResourceBudget,
    ) -> Result<(), BroadwebdError> {
        self.require_internal_fixture_endpoint()?;
        let request = self.retain_object_request(object_id)?;
        let response = fetch_internal_kubo_profile_sync_fixture(&request, budget)?;
        require_kubo_profile_sync_success("pin/add", response.status_code)
    }

    #[cfg(any(test, feature = "test-fixtures"))]
    pub fn release_object_fixture(
        &self,
        object_id: &str,
        budget: &ResourceBudget,
    ) -> Result<(), BroadwebdError> {
        self.require_internal_fixture_endpoint()?;
        let request = self.release_object_request(object_id)?;
        let response = fetch_internal_kubo_profile_sync_fixture(&request, budget)?;
        require_kubo_profile_sync_success("pin/rm", response.status_code)
    }

    #[cfg(any(test, feature = "test-fixtures"))]
    pub fn verify_retained_object_fixture(
        &self,
        object_id: &str,
        budget: &ResourceBudget,
    ) -> Result<bool, BroadwebdError> {
        self.require_internal_fixture_endpoint()?;
        let request = self.verify_retained_object_request(object_id)?;
        let response = fetch_internal_kubo_profile_sync_fixture(&request, budget)?;
        require_kubo_profile_sync_success("pin/ls", response.status_code)?;
        ipfs_kubo_profile_sync_pin_ls_has_recursive_pin(object_id, response.body.as_slice())
    }

    #[cfg(any(test, feature = "test-fixtures"))]
    pub fn publish_root_fixture(
        &self,
        key_id: &str,
        object_id: &str,
        budget: &ResourceBudget,
    ) -> Result<String, BroadwebdError> {
        self.require_internal_fixture_endpoint()?;
        let request = self.publish_root_request(key_id, object_id)?;
        let response = fetch_internal_kubo_profile_sync_fixture(&request, budget)?;
        require_kubo_profile_sync_success("name/publish", response.status_code)?;
        let published_object_id =
            ipfs_kubo_profile_sync_published_object_id(response.body.as_slice())?;
        if published_object_id == object_id {
            return Ok(published_object_id);
        }

        Err(BroadwebdError::Request(format!(
            "Kubo profile-sync name/publish returned {published_object_id}, expected {object_id}"
        )))
    }

    #[cfg(any(test, feature = "test-fixtures"))]
    pub fn resolve_root_fixture(
        &self,
        name: &str,
        budget: &ResourceBudget,
    ) -> Result<String, BroadwebdError> {
        self.require_internal_fixture_endpoint()?;
        let request = self.resolve_root_request(name)?;
        let response = fetch_internal_kubo_profile_sync_fixture(&request, budget)?;
        require_kubo_profile_sync_success("name/resolve", response.status_code)?;
        ipfs_kubo_profile_sync_resolved_object_id(response.body.as_slice())
    }

    #[cfg(any(test, feature = "test-fixtures"))]
    fn require_internal_fixture_endpoint(&self) -> Result<(), BroadwebdError> {
        if self.endpoint.is_internal_fixture() {
            return Ok(());
        }

        Err(BroadwebdError::UnsupportedRequest(
            "Kubo profile-sync fixture client requires an in-process fixture endpoint".to_string(),
        ))
    }
}

impl IpfsKuboProfileSyncRpcRequest {
    fn new(operation: IpfsKuboProfileSyncOperation, url: String) -> Self {
        Self { operation, url }
    }

    pub fn operation(&self) -> IpfsKuboProfileSyncOperation {
        self.operation
    }

    pub fn url(&self) -> &str {
        self.url.as_str()
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

pub fn ipfs_kubo_profile_sync_add_url(api_base_url: &str) -> Result<String, BroadwebdError> {
    let mut url = kubo_rpc_api_url(api_base_url, "add")?;
    url.query_pairs_mut()
        .append_pair("cid-version", "1")
        .append_pair("raw-leaves", "true")
        .append_pair("pin", "false");
    Ok(url.to_string())
}

pub fn ipfs_kubo_profile_sync_pin_add_url(
    object_id: &str,
    api_base_url: &str,
) -> Result<String, BroadwebdError> {
    validate_kubo_profile_sync_object_id(object_id)?;
    let mut url = kubo_rpc_api_url(api_base_url, "pin/add")?;
    url.query_pairs_mut()
        .append_pair("arg", object_id)
        .append_pair("recursive", "true");
    Ok(url.to_string())
}

pub fn ipfs_kubo_profile_sync_pin_rm_url(
    object_id: &str,
    api_base_url: &str,
) -> Result<String, BroadwebdError> {
    validate_kubo_profile_sync_object_id(object_id)?;
    let mut url = kubo_rpc_api_url(api_base_url, "pin/rm")?;
    url.query_pairs_mut()
        .append_pair("arg", object_id)
        .append_pair("recursive", "true");
    Ok(url.to_string())
}

pub fn ipfs_kubo_profile_sync_pin_ls_url(
    object_id: &str,
    api_base_url: &str,
) -> Result<String, BroadwebdError> {
    validate_kubo_profile_sync_object_id(object_id)?;
    let mut url = kubo_rpc_api_url(api_base_url, "pin/ls")?;
    url.query_pairs_mut()
        .append_pair("arg", object_id)
        .append_pair("type", "recursive");
    Ok(url.to_string())
}

pub fn ipfs_kubo_profile_sync_name_publish_url(
    key_id: &str,
    object_id: &str,
    api_base_url: &str,
) -> Result<String, BroadwebdError> {
    validate_kubo_profile_sync_name_token("IPNS key id", key_id)?;
    validate_kubo_profile_sync_object_id(object_id)?;
    let mut url = kubo_rpc_api_url(api_base_url, "name/publish")?;
    let object_path = format!("/ipfs/{object_id}");
    url.query_pairs_mut()
        .append_pair("arg", object_path.as_str())
        .append_pair("key", key_id)
        .append_pair("allow-offline", "true");
    Ok(url.to_string())
}

pub fn ipfs_kubo_profile_sync_name_resolve_url(
    name: &str,
    api_base_url: &str,
) -> Result<String, BroadwebdError> {
    validate_kubo_profile_sync_name_token("IPNS name", name)?;
    let mut url = kubo_rpc_api_url(api_base_url, "name/resolve")?;
    let name_path = if name.starts_with("/ipns/") {
        name.to_string()
    } else {
        format!("/ipns/{name}")
    };
    url.query_pairs_mut()
        .append_pair("arg", name_path.as_str())
        .append_pair("recursive", "false");
    Ok(url.to_string())
}

pub fn ipfs_kubo_profile_sync_added_object_id(
    response_body: &[u8],
) -> Result<String, BroadwebdError> {
    let response: KuboAddResponse = decode_kubo_json_response(response_body, "add")?;
    validate_kubo_profile_sync_object_id(response.hash.as_str())?;
    Ok(response.hash)
}

pub fn ipfs_kubo_profile_sync_pin_ls_has_recursive_pin(
    object_id: &str,
    response_body: &[u8],
) -> Result<bool, BroadwebdError> {
    validate_kubo_profile_sync_object_id(object_id)?;
    let response: KuboPinLsResponse = decode_kubo_json_response(response_body, "pin/ls")?;
    Ok(response
        .keys
        .get(object_id)
        .is_some_and(|pin| pin.kind == "recursive"))
}

pub fn ipfs_kubo_profile_sync_published_object_id(
    response_body: &[u8],
) -> Result<String, BroadwebdError> {
    let response: KuboNamePublishResponse =
        decode_kubo_json_response(response_body, "name/publish")?;
    let object_id = profile_sync_object_id_from_ipfs_path(response.value.as_str())?;
    validate_kubo_profile_sync_name_token("IPNS name", response.name.as_str())?;
    Ok(object_id)
}

pub fn ipfs_kubo_profile_sync_resolved_object_id(
    response_body: &[u8],
) -> Result<String, BroadwebdError> {
    let response: KuboNameResolveResponse =
        decode_kubo_json_response(response_body, "name/resolve")?;
    profile_sync_object_id_from_ipfs_path(response.path.as_str())
}

#[cfg(any(test, feature = "test-fixtures"))]
pub fn fetch_internal_kubo_profile_sync_fixture(
    request: &IpfsKuboProfileSyncRpcRequest,
    budget: &ResourceBudget,
) -> Result<InternalKuboRpcResponse, BroadwebdError> {
    let url = parse_http_url(request.url())?;
    fetch_internal_kubo_rpc_response(&url, budget.max_profile_sync_object_bytes)
}

#[cfg(any(test, feature = "test-fixtures"))]
fn require_kubo_profile_sync_success(
    operation: &str,
    status_code: u16,
) -> Result<(), BroadwebdError> {
    if (200..=299).contains(&status_code) {
        return Ok(());
    }

    Err(BroadwebdError::Request(format!(
        "Kubo profile-sync {operation} returned HTTP status {status_code}"
    )))
}

fn kubo_cat_url_for_path(content_path: &str, api_base_url: &str) -> Result<String, BroadwebdError> {
    let mut url = kubo_rpc_api_url(api_base_url, "cat")?;
    url.query_pairs_mut().append_pair("arg", &content_path);
    Ok(url.to_string())
}

#[derive(Debug, Deserialize)]
struct KuboAddResponse {
    #[serde(rename = "Hash")]
    hash: String,
}

#[derive(Debug, Deserialize)]
struct KuboPinLsResponse {
    #[serde(rename = "Keys")]
    keys: BTreeMap<String, KuboPinRecord>,
}

#[derive(Debug, Deserialize)]
struct KuboPinRecord {
    #[serde(rename = "Type")]
    kind: String,
}

#[derive(Debug, Deserialize)]
struct KuboNamePublishResponse {
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "Value")]
    value: String,
}

#[derive(Debug, Deserialize)]
struct KuboNameResolveResponse {
    #[serde(rename = "Path")]
    path: String,
}

fn decode_kubo_json_response<T>(response_body: &[u8], operation: &str) -> Result<T, BroadwebdError>
where
    T: for<'de> Deserialize<'de>,
{
    serde_json::from_slice(response_body).map_err(|error| {
        BroadwebdError::Request(format!("invalid Kubo {operation} response: {error}"))
    })
}

fn profile_sync_object_id_from_ipfs_path(path: &str) -> Result<String, BroadwebdError> {
    let object_id = path
        .strip_prefix("/ipfs/")
        .ok_or_else(|| BroadwebdError::InvalidUrl(format!("expected /ipfs/ path: {path}")))?;
    validate_kubo_profile_sync_object_id(object_id)?;
    Ok(object_id.to_string())
}

fn kubo_rpc_api_url(api_base_url: &str, endpoint: &str) -> Result<Url, BroadwebdError> {
    validate_kubo_rpc_url(api_base_url)?;
    let mut url = parse_http_url(api_base_url)?;
    let api_path = format!(
        "{}/api/v0/{}",
        url.path().trim_end_matches('/'),
        endpoint.trim_start_matches('/')
    );
    url.set_path(&api_path);
    url.set_query(None);
    Ok(url)
}

fn validate_kubo_profile_sync_object_id(object_id: &str) -> Result<(), BroadwebdError> {
    validate_kubo_profile_sync_name_token("profile sync object id", object_id)?;
    if object_id.contains('/') {
        return Err(BroadwebdError::InvalidUrl(format!(
            "profile sync object id must not contain a path separator: {object_id}"
        )));
    }
    Ok(())
}

fn validate_kubo_profile_sync_name_token(label: &str, value: &str) -> Result<(), BroadwebdError> {
    if value.is_empty() {
        return Err(BroadwebdError::InvalidUrl(format!("{label} is empty")));
    }
    if value.len() > 2048 {
        return Err(BroadwebdError::InvalidUrl(format!("{label} is too long")));
    }
    if value.chars().any(char::is_control) {
        return Err(BroadwebdError::InvalidUrl(format!(
            "{label} must not contain control characters"
        )));
    }
    Ok(())
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
    let host_for_parse = host
        .strip_prefix('[')
        .and_then(|host| host.strip_suffix(']'))
        .unwrap_or(host);
    if host_for_parse
        .parse::<std::net::IpAddr>()
        .is_ok_and(|address| address.is_loopback())
    {
        return Ok(());
    }

    Err(BroadwebdError::UnsupportedRequest(format!(
        "Kubo RPC endpoint must be a numeric loopback address: {api_base_url}"
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
    let response = fetch_internal_kubo_rpc_response(url, budget.max_http_response_bytes)?;

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
fn fetch_internal_kubo_rpc_response(
    url: &Url,
    max_response_bytes: usize,
) -> Result<InternalKuboRpcResponse, BroadwebdError> {
    let base_url = internal_kubo_rpc_base_url(url)?;
    let request_target = internal_kubo_rpc_request_target(url);

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

    if response.body.len() > max_response_bytes {
        return Err(BroadwebdError::ResponseTooLarge {
            limit: max_response_bytes,
            actual: response.body.len(),
        });
    }

    Ok(response)
}

#[cfg(any(test, feature = "test-fixtures"))]
fn internal_kubo_rpc_request_target(url: &Url) -> String {
    let path = internal_kubo_rpc_request_path(url);
    match url.query() {
        Some(query) => format!("POST {path}?{query} HTTP/1.1"),
        None => format!("POST {path} HTTP/1.1"),
    }
}

#[cfg(any(test, feature = "test-fixtures"))]
fn internal_kubo_rpc_request_path(url: &Url) -> String {
    let Some(host) = url.host_str() else {
        return url.path().to_string();
    };
    if host.starts_with("fixture-") {
        return url.path().to_string();
    }
    let Some(segments) = url.path_segments() else {
        return url.path().to_string();
    };
    let mut segments = segments.collect::<Vec<_>>();
    if segments
        .first()
        .is_some_and(|segment| segment.starts_with("fixture-"))
    {
        segments.remove(0);
        return format!("/{}", segments.join("/"));
    }
    url.path().to_string()
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
