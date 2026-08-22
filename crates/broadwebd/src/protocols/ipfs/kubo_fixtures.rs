use super::kubo::{
    IpfsKuboHttpContentExecutor, IpfsKuboProfileSyncRpcExecutor, IpfsKuboProfileSyncRpcRequest,
    IpfsKuboRpcResponse, profile_sync_object_id_from_ipfs_path,
    validate_kubo_profile_sync_name_token, validate_kubo_profile_sync_object_id,
};
use crate::http::{infer_content_type, parse_http_url};
use crate::{BroadwebdError, HttpFetchResponse, HttpHeader, ResourceBudget};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::{Mutex, OnceLock};
use url::Url;

const INTERNAL_KUBO_RPC_FIXTURE_SCHEME: &str = "slate-fixture-kubo";

/// Test-only socket substitute for Kubo-compatible HTTP RPC requests.
///
/// The shim only redirects Kubo-shaped requests to an in-process fixture
/// endpoint. The protocol client still builds and validates the same URLs and
/// response bodies it would use against a real Kubo node.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct InternalKuboRpcTransportShim;

pub type InternalKuboRpcResponse = IpfsKuboRpcResponse;

impl IpfsKuboProfileSyncRpcExecutor for InternalKuboRpcTransportShim {
    fn execute_profile_sync_request(
        &self,
        request: &IpfsKuboProfileSyncRpcRequest,
        budget: &ResourceBudget,
        body: Option<&[u8]>,
    ) -> Result<InternalKuboRpcResponse, BroadwebdError> {
        fetch_internal_kubo_profile_sync_fixture(request, budget, body)
    }

    fn execute_content_request(
        &self,
        url: &Url,
        max_response_bytes: usize,
    ) -> Result<InternalKuboRpcResponse, BroadwebdError> {
        fetch_internal_kubo_rpc_response(url, max_response_bytes, None)
    }
}

impl IpfsKuboHttpContentExecutor for InternalKuboRpcTransportShim {
    fn execute_http_content_request(
        &self,
        url: &Url,
        document_url: &str,
        budget: &ResourceBudget,
    ) -> Result<HttpFetchResponse, BroadwebdError> {
        fetch_internal_kubo_rpc_fixture(url, document_url, budget)
    }
}

pub(crate) fn register_internal_kubo_rpc_fixture_for_network(
    network_id: &str,
    responses: Vec<InternalKuboRpcResponse>,
) -> String {
    let base_url = next_internal_kubo_rpc_fixture_base_url(network_id);
    internal_kubo_rpc_fixtures()
        .lock()
        .expect("internal Kubo fixture registry should not be poisoned")
        .insert(
            base_url.clone(),
            InternalKuboRpcFixture::queued(responses.into()),
        );
    base_url
}

pub(crate) fn register_internal_kubo_profile_sync_model_for_network(network_id: &str) -> String {
    let base_url = next_internal_kubo_rpc_fixture_base_url(network_id);
    internal_kubo_rpc_fixtures()
        .lock()
        .expect("internal Kubo fixture registry should not be poisoned")
        .insert(
            base_url.clone(),
            InternalKuboRpcFixture::profile_sync_model(),
        );
    base_url
}

pub(crate) fn take_internal_kubo_rpc_fixture_requests(base_url: &str) -> Vec<String> {
    internal_kubo_rpc_fixtures()
        .lock()
        .expect("internal Kubo fixture registry should not be poisoned")
        .remove(base_url)
        .map(|fixture| fixture.requests)
        .unwrap_or_default()
}

pub(super) fn is_internal_kubo_rpc_fixture_url(url: &Url) -> bool {
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

pub(crate) fn internal_kubo_rpc_url_belongs_to_network(url: &Url, network_id: &str) -> bool {
    if !is_internal_kubo_rpc_fixture_url(url) {
        return false;
    }
    let Some(url_network_id) = internal_kubo_rpc_network_id(url) else {
        return network_id == "global";
    };
    url_network_id == network_id
}

fn fetch_internal_kubo_rpc_fixture(
    url: &Url,
    document_url: &str,
    budget: &ResourceBudget,
) -> Result<HttpFetchResponse, BroadwebdError> {
    let response = fetch_internal_kubo_rpc_response(url, budget.max_http_response_bytes, None)?;

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

fn fetch_internal_kubo_profile_sync_fixture(
    request: &IpfsKuboProfileSyncRpcRequest,
    budget: &ResourceBudget,
    body: Option<&[u8]>,
) -> Result<InternalKuboRpcResponse, BroadwebdError> {
    let url = parse_http_url(request.url())?;
    require_internal_kubo_rpc_fixture_url(&url)?;
    fetch_internal_kubo_rpc_response(&url, budget.max_profile_sync_object_bytes, body)
}

fn require_internal_kubo_rpc_fixture_url(url: &Url) -> Result<(), BroadwebdError> {
    if is_internal_kubo_rpc_fixture_url(url) {
        return Ok(());
    }

    Err(BroadwebdError::UnsupportedRequest(format!(
        "internal Kubo RPC transport requires an in-process fixture endpoint: {url}"
    )))
}

fn next_internal_kubo_rpc_fixture_base_url(network_id: &str) -> String {
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT_FIXTURE_ID: AtomicUsize = AtomicUsize::new(1);

    let id = NEXT_FIXTURE_ID.fetch_add(1, Ordering::Relaxed);
    if network_id == "global" {
        format!("{INTERNAL_KUBO_RPC_FIXTURE_SCHEME}://fixture-{id}")
    } else {
        format!("{INTERNAL_KUBO_RPC_FIXTURE_SCHEME}://{network_id}/fixture-{id}")
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct InternalKuboRpcFixture {
    behavior: InternalKuboRpcFixtureBehavior,
    requests: Vec<String>,
}

impl InternalKuboRpcFixture {
    fn queued(responses: VecDeque<InternalKuboRpcResponse>) -> Self {
        Self {
            behavior: InternalKuboRpcFixtureBehavior::Queued { responses },
            requests: Vec::new(),
        }
    }

    fn profile_sync_model() -> Self {
        Self {
            behavior: InternalKuboRpcFixtureBehavior::ProfileSyncModel(
                InternalKuboProfileSyncModel::default(),
            ),
            requests: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum InternalKuboRpcFixtureBehavior {
    Queued {
        responses: VecDeque<InternalKuboRpcResponse>,
    },
    ProfileSyncModel(InternalKuboProfileSyncModel),
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct InternalKuboProfileSyncModel {
    objects: BTreeMap<String, Vec<u8>>,
    pins: BTreeSet<String>,
    names: BTreeMap<String, String>,
}

impl InternalKuboProfileSyncModel {
    fn response_for(
        &mut self,
        url: &Url,
        body: Option<&[u8]>,
    ) -> Result<InternalKuboRpcResponse, BroadwebdError> {
        match internal_kubo_rpc_request_path(url).as_str() {
            "/api/v0/add" => self.add(body),
            "/api/v0/cat" => self.cat(url),
            "/api/v0/pin/add" => self.pin_add(url),
            "/api/v0/pin/rm" => self.pin_rm(url),
            "/api/v0/pin/ls" => self.pin_ls(url),
            "/api/v0/name/publish" => self.name_publish(url),
            "/api/v0/name/resolve" => self.name_resolve(url),
            path => Ok(internal_kubo_rpc_error_response(
                404,
                format!("unsupported Kubo fixture endpoint: {path}"),
            )),
        }
    }

    fn add(&mut self, body: Option<&[u8]>) -> Result<InternalKuboRpcResponse, BroadwebdError> {
        let bytes = body.ok_or_else(|| {
            BroadwebdError::Request(
                "internal Kubo profile-sync add requires request bytes".to_string(),
            )
        })?;
        let object_id = internal_kubo_profile_sync_model_object_id(bytes);
        self.objects.insert(object_id.clone(), bytes.to_vec());
        kubo_json_response(serde_json::json!({
            "Name": "profile-object",
            "Hash": object_id,
            "Size": bytes.len().to_string(),
        }))
    }

    fn cat(&self, url: &Url) -> Result<InternalKuboRpcResponse, BroadwebdError> {
        let object_id = object_id_query(url)?;
        let Some(bytes) = self.objects.get(object_id.as_str()) else {
            return Ok(internal_kubo_rpc_error_response(
                404,
                format!("missing IPFS object: {object_id}"),
            ));
        };
        Ok(InternalKuboRpcResponse {
            status_code: 200,
            content_type: "application/octet-stream".to_string(),
            body: bytes.clone(),
        })
    }

    fn pin_add(&mut self, url: &Url) -> Result<InternalKuboRpcResponse, BroadwebdError> {
        let object_id = required_query_value(url, "arg")?;
        validate_kubo_profile_sync_object_id(object_id.as_str())?;
        if !self.objects.contains_key(object_id.as_str()) {
            return Ok(internal_kubo_rpc_error_response(
                404,
                format!("cannot pin missing IPFS object: {object_id}"),
            ));
        }
        self.pins.insert(object_id.clone());
        kubo_json_response(serde_json::json!({ "Pins": [object_id] }))
    }

    fn pin_rm(&mut self, url: &Url) -> Result<InternalKuboRpcResponse, BroadwebdError> {
        let object_id = required_query_value(url, "arg")?;
        validate_kubo_profile_sync_object_id(object_id.as_str())?;
        self.pins.remove(object_id.as_str());
        kubo_json_response(serde_json::json!({ "Pins": [object_id] }))
    }

    fn pin_ls(&self, url: &Url) -> Result<InternalKuboRpcResponse, BroadwebdError> {
        let object_id = required_query_value(url, "arg")?;
        validate_kubo_profile_sync_object_id(object_id.as_str())?;
        let mut keys = serde_json::Map::new();
        if self.pins.contains(object_id.as_str()) {
            keys.insert(
                object_id,
                serde_json::json!({
                    "Type": "recursive",
                }),
            );
        }
        kubo_json_response(serde_json::json!({ "Keys": keys }))
    }

    fn name_publish(&mut self, url: &Url) -> Result<InternalKuboRpcResponse, BroadwebdError> {
        let object_id = object_id_query(url)?;
        let key_id = required_query_value(url, "key")?;
        validate_kubo_profile_sync_name_token("IPNS key id", key_id.as_str())?;
        if !self.objects.contains_key(object_id.as_str()) {
            return Ok(internal_kubo_rpc_error_response(
                404,
                format!("cannot publish missing IPFS object: {object_id}"),
            ));
        }
        self.names.insert(key_id.clone(), object_id.clone());
        kubo_json_response(serde_json::json!({
            "Name": key_id,
            "Value": format!("/ipfs/{object_id}"),
        }))
    }

    fn name_resolve(&self, url: &Url) -> Result<InternalKuboRpcResponse, BroadwebdError> {
        let name = ipns_name_query(url)?;
        let Some(object_id) = self.names.get(name.as_str()) else {
            return Ok(internal_kubo_rpc_error_response(
                404,
                format!("missing IPNS name: {name}"),
            ));
        };
        kubo_json_response(serde_json::json!({
            "Path": format!("/ipfs/{object_id}"),
        }))
    }
}

fn fetch_internal_kubo_rpc_response(
    url: &Url,
    max_response_bytes: usize,
    body: Option<&[u8]>,
) -> Result<InternalKuboRpcResponse, BroadwebdError> {
    require_internal_kubo_rpc_fixture_url(url)?;
    let base_url = internal_kubo_rpc_base_url(url)?;
    let request_target = internal_kubo_rpc_request_target(url);

    let mut fixtures = internal_kubo_rpc_fixtures()
        .lock()
        .expect("internal Kubo fixture registry should not be poisoned");
    let fixture = fixtures.get_mut(base_url.as_str()).ok_or_else(|| {
        BroadwebdError::Request(format!("missing internal Kubo fixture {base_url}"))
    })?;
    fixture.requests.push(request_target);
    let response = match &mut fixture.behavior {
        InternalKuboRpcFixtureBehavior::Queued { responses } => {
            responses.pop_front().ok_or_else(|| {
                BroadwebdError::Request(format!("internal Kubo fixture {base_url} has no response"))
            })?
        }
        InternalKuboRpcFixtureBehavior::ProfileSyncModel(model) => model.response_for(url, body)?,
    };

    if response.body.len() > max_response_bytes {
        return Err(BroadwebdError::ResponseTooLarge {
            limit: max_response_bytes,
            actual: response.body.len(),
        });
    }

    Ok(response)
}

fn internal_kubo_rpc_error_response(
    status_code: u16,
    message: impl Into<String>,
) -> InternalKuboRpcResponse {
    let message = message.into();
    let body = serde_json::to_vec(&serde_json::json!({ "Message": message }))
        .expect("Kubo fixture error JSON should encode");
    InternalKuboRpcResponse {
        status_code,
        content_type: "application/json".to_string(),
        body,
    }
}

fn internal_kubo_profile_sync_model_object_id(bytes: &[u8]) -> String {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("bafyfixture{:x}{hash:016x}", bytes.len())
}

fn object_id_query(url: &Url) -> Result<String, BroadwebdError> {
    profile_sync_object_id_from_ipfs_path(required_query_value(url, "arg")?.as_str())
}

fn ipns_name_query(url: &Url) -> Result<String, BroadwebdError> {
    let path = required_query_value(url, "arg")?;
    let name = path
        .strip_prefix("/ipns/")
        .ok_or_else(|| BroadwebdError::InvalidUrl(format!("expected /ipns/ path: {path}")))?;
    validate_kubo_profile_sync_name_token("IPNS name", name)?;
    Ok(name.to_string())
}

fn required_query_value(url: &Url, key: &str) -> Result<String, BroadwebdError> {
    url.query_pairs()
        .find(|(candidate, _)| candidate == key)
        .map(|(_, value)| value.into_owned())
        .ok_or_else(|| {
            BroadwebdError::InvalidUrl(format!("Kubo fixture request is missing {key}: {url}"))
        })
}

fn kubo_json_response(value: serde_json::Value) -> Result<InternalKuboRpcResponse, BroadwebdError> {
    let body = serde_json::to_vec(&value)
        .map_err(|error| BroadwebdError::Request(format!("encode Kubo fixture JSON: {error}")))?;
    Ok(InternalKuboRpcResponse {
        status_code: 200,
        content_type: "application/json".to_string(),
        body,
    })
}

fn internal_kubo_rpc_request_target(url: &Url) -> String {
    let path = internal_kubo_rpc_request_path(url);
    match url.query() {
        Some(query) => format!("POST {path}?{query} HTTP/1.1"),
        None => format!("POST {path} HTTP/1.1"),
    }
}

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

fn internal_kubo_rpc_network_id(url: &Url) -> Option<String> {
    let host = url.host_str()?;
    if host.starts_with("fixture-") {
        return None;
    }
    internal_kubo_rpc_path_token(url).map(|_| host.to_string())
}

fn internal_kubo_rpc_path_token(url: &Url) -> Option<String> {
    url.path_segments()
        .and_then(|mut segments| segments.next().map(str::to_string))
}

fn internal_kubo_rpc_fixtures() -> &'static Mutex<BTreeMap<String, InternalKuboRpcFixture>> {
    static FIXTURES: OnceLock<Mutex<BTreeMap<String, InternalKuboRpcFixture>>> = OnceLock::new();

    FIXTURES.get_or_init(|| Mutex::new(BTreeMap::new()))
}
