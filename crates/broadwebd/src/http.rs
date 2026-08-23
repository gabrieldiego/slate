use crate::{BroadwebdError, DEFAULT_PROFILE, ResourceBudget};
use slate_net::BROWSER_USER_AGENT;
use std::path::PathBuf;
use std::time::Duration;
use url::Url;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(12);
#[cfg(any(test, feature = "test-fixtures"))]
const INTERNAL_HTTP_FIXTURE_SCHEME: &str = "slate-fixture-http";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HttpFetchRequest {
    pub profile: String,
    pub url: String,
    pub transport_id: Option<String>,
    pub purpose: FetchPurpose,
    pub suggested_download_filename: Option<String>,
}

impl HttpFetchRequest {
    pub fn new(profile: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            profile: profile.into(),
            url: url.into(),
            transport_id: None,
            purpose: FetchPurpose::Navigation,
            suggested_download_filename: None,
        }
    }

    pub fn default_profile(url: impl Into<String>) -> Self {
        Self::new(DEFAULT_PROFILE, url)
    }

    pub fn through_transport(mut self, transport_id: impl Into<String>) -> Self {
        self.transport_id = Some(transport_id.into());
        self
    }

    pub fn for_subresource(mut self) -> Self {
        self.purpose = FetchPurpose::Subresource;
        self
    }

    pub fn download_as(mut self, filename: impl Into<String>) -> Self {
        self.suggested_download_filename = Some(filename.into());
        self
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FetchPurpose {
    Navigation,
    Subresource,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransportHttpRequest {
    pub profile: String,
    pub url: String,
    pub purpose: FetchPurpose,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HttpHeader {
    pub name: String,
    pub value: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FetchDisposition {
    RenderHtml,
    Download { suggested_filename: String },
    ErrorPage { status_code: u16 },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FetchRouteInfo {
    pub profile: String,
    pub transport_id: String,
    pub privacy_boundary: String,
    pub purpose: FetchPurpose,
}

impl FetchRouteInfo {
    pub fn new(
        profile: impl Into<String>,
        transport_id: impl Into<String>,
        privacy_boundary: impl Into<String>,
        purpose: FetchPurpose,
    ) -> Self {
        Self {
            profile: profile.into(),
            transport_id: transport_id.into(),
            privacy_boundary: privacy_boundary.into(),
            purpose,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DownloadRecord {
    pub profile: String,
    pub filename: String,
    pub path: PathBuf,
    pub size_bytes: usize,
    pub content_type: Option<String>,
}

impl DownloadRecord {
    pub fn new(
        profile: impl Into<String>,
        filename: impl Into<String>,
        path: impl Into<PathBuf>,
        size_bytes: usize,
        content_type: Option<String>,
    ) -> Self {
        Self {
            profile: profile.into(),
            filename: filename.into(),
            path: path.into(),
            size_bytes,
            content_type,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HttpFetchResponse {
    pub final_url: String,
    pub status_code: u16,
    pub content_type: Option<String>,
    pub headers: Vec<HttpHeader>,
    pub body: Vec<u8>,
    pub disposition: FetchDisposition,
    pub route: Option<FetchRouteInfo>,
    pub download: Option<DownloadRecord>,
}

impl HttpFetchResponse {
    pub fn new(
        final_url: impl Into<String>,
        status_code: u16,
        content_type: Option<String>,
        headers: Vec<HttpHeader>,
        body: Vec<u8>,
    ) -> Self {
        let final_url = final_url.into();
        let disposition =
            response_disposition(status_code, &final_url, content_type.as_deref(), &headers);
        Self {
            final_url,
            status_code,
            content_type,
            headers,
            body,
            disposition,
            route: None,
            download: None,
        }
    }

    pub fn with_route(mut self, route: FetchRouteInfo) -> Self {
        self.route = Some(route);
        self
    }

    pub fn with_download(mut self, download: DownloadRecord) -> Self {
        self.download = Some(download);
        self
    }

    pub fn with_download_disposition(mut self, suggested_filename: impl Into<String>) -> Self {
        self.disposition = FetchDisposition::Download {
            suggested_filename: suggested_filename.into(),
        };
        self
    }

    pub fn body_text_lossy(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    pub fn is_html(&self) -> bool {
        matches!(self.disposition, FetchDisposition::RenderHtml)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ServiceRequest {
    HttpFetch(HttpFetchRequest),
    ProfileSync(ProfileSyncRequest),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ServiceResponse {
    HttpFetch(HttpFetchResponse),
    ProfileSync(ProfileSyncResponse),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProfileSyncRequest {
    PutEncryptedObject(ProfileSyncPutObjectRequest),
    GetEncryptedObject(ProfileSyncObjectRequest),
    RetainObject(ProfileSyncObjectRequest),
    ReleaseObject(ProfileSyncObjectRequest),
    ListRetainedObjects(ProfileSyncProfileRequest),
    VerifyRetainedObject(ProfileSyncObjectRequest),
    PublishRoot(ProfileSyncRootUpdate),
    ResolveRoot(ProfileSyncRootRequest),
    ListRootCandidates(ProfileSyncRootRequest),
    DiscoverProviders(ProfileSyncProfileRequest),
    ProviderHealth(ProfileSyncProfileRequest),
    RootHealth(ProfileSyncRootHealthRequest),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileSyncPutObjectRequest {
    pub profile: String,
    pub bytes: Vec<u8>,
}

impl ProfileSyncPutObjectRequest {
    pub fn new(profile: impl Into<String>, bytes: impl Into<Vec<u8>>) -> Self {
        Self {
            profile: profile.into(),
            bytes: bytes.into(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileSyncObjectRequest {
    pub profile: String,
    pub object_id: String,
}

impl ProfileSyncObjectRequest {
    pub fn new(profile: impl Into<String>, object_id: impl Into<String>) -> Self {
        Self {
            profile: profile.into(),
            object_id: object_id.into(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileSyncRootRequest {
    pub profile: String,
    pub root_id: String,
}

impl ProfileSyncRootRequest {
    pub fn new(profile: impl Into<String>, root_id: impl Into<String>) -> Self {
        Self {
            profile: profile.into(),
            root_id: root_id.into(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileSyncRootUpdate {
    pub profile: String,
    pub root_id: String,
    pub object_id: String,
}

impl ProfileSyncRootUpdate {
    pub fn new(
        profile: impl Into<String>,
        root_id: impl Into<String>,
        object_id: impl Into<String>,
    ) -> Self {
        Self {
            profile: profile.into(),
            root_id: root_id.into(),
            object_id: object_id.into(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileSyncRootHealthRequest {
    pub profile: String,
    pub root_id: String,
    pub minimum_online_retaining_providers: usize,
}

impl ProfileSyncRootHealthRequest {
    pub fn new(profile: impl Into<String>, root_id: impl Into<String>) -> Self {
        Self::with_minimum_online_retaining_providers(profile, root_id, 1)
    }

    pub fn with_minimum_online_retaining_providers(
        profile: impl Into<String>,
        root_id: impl Into<String>,
        minimum_online_retaining_providers: usize,
    ) -> Self {
        Self {
            profile: profile.into(),
            root_id: root_id.into(),
            minimum_online_retaining_providers,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileSyncProfileRequest {
    pub profile: String,
}

impl ProfileSyncProfileRequest {
    pub fn new(profile: impl Into<String>) -> Self {
        Self {
            profile: profile.into(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProfileSyncResponse {
    PutEncryptedObject {
        object_id: String,
    },
    GetEncryptedObject {
        object_id: String,
        bytes: Vec<u8>,
    },
    RetainObject {
        object_id: String,
        retained: bool,
    },
    ReleaseObject {
        object_id: String,
        retained: bool,
    },
    RetainedObjects {
        object_ids: Vec<String>,
    },
    RetainedObjectStatus {
        object_id: String,
        retained: bool,
        available: bool,
    },
    Root {
        root_id: String,
        object_id: Option<String>,
    },
    RootCandidates {
        root_id: String,
        candidates: Vec<ProfileSyncRootCandidate>,
    },
    Providers {
        providers: Vec<ProfileSyncProviderRecord>,
    },
    ProviderHealth {
        health: ProfileSyncProviderHealth,
    },
    RootHealth {
        health: ProfileSyncRootHealth,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileSyncRootCandidate {
    pub publisher_provider_id: String,
    pub object_id: String,
    pub publish_sequence: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileSyncProviderRecord {
    pub provider_id: String,
    pub provider_kind: String,
    pub privacy_boundary: String,
    pub retained_objects: usize,
    pub roles: ProfileSyncProviderRoles,
    pub can_publish_roots: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileSyncProviderHealth {
    pub profile: String,
    pub known_providers: usize,
    pub online_providers: usize,
    pub offline_providers: usize,
    pub fresh_online_providers: usize,
    pub stale_online_providers: usize,
    pub fresh_online_provider_ids: Vec<String>,
    pub stale_online_provider_ids: Vec<String>,
    pub offline_provider_ids: Vec<String>,
    pub minimum_provider_seen_sequence: u64,
    pub object_transfer_providers: usize,
    pub availability_providers: usize,
    pub mutable_root_providers: usize,
    pub retained_objects: usize,
    pub degraded: bool,
    pub message: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileSyncRootHealth {
    pub profile: String,
    pub root_id: String,
    pub visible_candidates: usize,
    pub delayed_candidates: usize,
    pub delayed_publisher_provider_ids: Vec<String>,
    pub latest_object_id: Option<String>,
    pub latest_object_available: bool,
    pub latest_object_available_provider_ids: Vec<String>,
    pub latest_object_stale_provider_ids: Vec<String>,
    pub latest_object_offline_provider_ids: Vec<String>,
    pub delayed_object_provider_ids: Vec<String>,
    pub online_retaining_providers: usize,
    pub minimum_online_retaining_providers: usize,
    pub degraded: bool,
    pub message: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProfileSyncProviderRoles {
    pub discovery: bool,
    pub connectivity: bool,
    pub object_transfer: bool,
    pub availability: bool,
    pub mutable_roots: bool,
}

impl ProfileSyncProviderRoles {
    pub const fn logged_in_device() -> Self {
        Self {
            discovery: true,
            connectivity: true,
            object_transfer: true,
            availability: true,
            mutable_roots: true,
        }
    }

    pub const fn availability_provider() -> Self {
        Self {
            discovery: true,
            connectivity: true,
            object_transfer: true,
            availability: true,
            mutable_roots: false,
        }
    }
}

pub(crate) fn parse_http_url(input: &str) -> Result<Url, BroadwebdError> {
    Url::parse(input).map_err(|error| BroadwebdError::InvalidUrl(error.to_string()))
}

pub(crate) fn fetch_http_url_over_network(
    url: Url,
    budget: &ResourceBudget,
) -> Result<HttpFetchResponse, BroadwebdError> {
    if !matches!(url.scheme(), "http" | "https") {
        return Err(BroadwebdError::UnsupportedRequest(format!(
            "unsupported HTTP fetch scheme: {}",
            url.scheme()
        )));
    }

    let client = reqwest::blocking::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .user_agent(BROWSER_USER_AGENT)
        .redirect(reqwest::redirect::Policy::limited(6))
        .build()
        .map_err(request_error)?;
    let response = client.get(url).send().map_err(request_error)?;
    let final_url = response.url().to_string();
    let status_code = response.status().as_u16();
    let header_content_type = header_value(response.headers(), reqwest::header::CONTENT_TYPE);
    let headers = response_headers(response.headers());
    let body = response.bytes().map_err(request_error)?.to_vec();
    if body.len() > budget.max_http_response_bytes {
        return Err(BroadwebdError::ResponseTooLarge {
            limit: budget.max_http_response_bytes,
            actual: body.len(),
        });
    }
    let content_type = infer_content_type(&final_url, header_content_type.as_deref(), &body);

    Ok(HttpFetchResponse::new(
        final_url,
        status_code,
        content_type,
        headers,
        body,
    ))
}

fn request_error(error: reqwest::Error) -> BroadwebdError {
    BroadwebdError::Request(error.to_string())
}

fn header_value(
    headers: &reqwest::header::HeaderMap,
    name: reqwest::header::HeaderName,
) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
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
pub struct InternalFixtureHttpResponse {
    pub status_code: u16,
    pub content_type: Option<String>,
    pub headers: Vec<HttpHeader>,
    pub body: Vec<u8>,
}

#[cfg(any(test, feature = "test-fixtures"))]
pub(crate) fn register_internal_fixture_http_response_for_network(
    network_id: &str,
    response: InternalFixtureHttpResponse,
) -> String {
    register_internal_fixture_http_sequence_for_network(network_id, vec![response])
}

#[cfg(any(test, feature = "test-fixtures"))]
pub(crate) fn register_internal_fixture_http_sequence_for_network(
    network_id: &str,
    responses: Vec<InternalFixtureHttpResponse>,
) -> String {
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT_FIXTURE_ID: AtomicUsize = AtomicUsize::new(1);

    let id = NEXT_FIXTURE_ID.fetch_add(1, Ordering::Relaxed);
    let base_url = if network_id == "global" {
        format!("{INTERNAL_HTTP_FIXTURE_SCHEME}://fixture-{id}/")
    } else {
        format!("{INTERNAL_HTTP_FIXTURE_SCHEME}://{network_id}/fixture-{id}/")
    };
    internal_fixture_http_responses()
        .lock()
        .expect("internal HTTP fixture registry should not be poisoned")
        .insert(
            base_url.clone(),
            InternalFixtureHttp {
                responses: responses.into(),
                requests: Vec::new(),
            },
        );
    base_url
}

#[cfg(any(test, feature = "test-fixtures"))]
pub fn take_internal_fixture_http_requests(base_url: &str) -> Vec<String> {
    internal_fixture_http_responses()
        .lock()
        .expect("internal HTTP fixture registry should not be poisoned")
        .remove(base_url)
        .map(|fixture| fixture.requests)
        .unwrap_or_default()
}

#[cfg(any(test, feature = "test-fixtures"))]
pub(crate) fn unregistered_internal_fixture_http_url_for_network(network_id: &str) -> String {
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT_MISSING_FIXTURE_ID: AtomicUsize = AtomicUsize::new(1);

    let id = NEXT_MISSING_FIXTURE_ID.fetch_add(1, Ordering::Relaxed);
    if network_id == "global" {
        format!("{INTERNAL_HTTP_FIXTURE_SCHEME}://missing-{id}")
    } else {
        format!("{INTERNAL_HTTP_FIXTURE_SCHEME}://{network_id}/missing-{id}")
    }
}

#[cfg(any(test, feature = "test-fixtures"))]
pub(crate) fn is_internal_fixture_http_url(url: &Url) -> bool {
    if url.scheme() != INTERNAL_HTTP_FIXTURE_SCHEME {
        return false;
    }
    let Some(host) = url.host_str() else {
        return false;
    };
    if host.starts_with("fixture-") || host.starts_with("missing-") {
        return true;
    }
    internal_fixture_http_path_token(url)
        .as_deref()
        .is_some_and(|token| token.starts_with("fixture-") || token.starts_with("missing-"))
}

#[cfg(any(test, feature = "test-fixtures"))]
pub(crate) fn internal_fixture_http_url_belongs_to_network(url: &Url, network_id: &str) -> bool {
    if !is_internal_fixture_http_url(url) {
        return false;
    }
    let Some(url_network_id) = internal_fixture_http_network_id(url) else {
        return network_id == "global";
    };
    url_network_id == network_id
}

#[cfg(any(test, feature = "test-fixtures"))]
pub(crate) fn fetch_internal_fixture_http_url(
    url: &Url,
    budget: &ResourceBudget,
) -> Result<HttpFetchResponse, BroadwebdError> {
    let url_text = url.as_str();
    let mut fixtures = internal_fixture_http_responses()
        .lock()
        .expect("internal HTTP fixture registry should not be poisoned");
    let Some(base_url) = fixtures
        .iter()
        .filter(|(base_url, _)| url_text.starts_with(base_url.as_str()))
        .max_by_key(|(base_url, _)| base_url.len())
        .map(|(base_url, _)| base_url.clone())
    else {
        return Err(BroadwebdError::Request(format!(
            "internal HTTP fixture has no response for {url_text}"
        )));
    };
    let fixture = fixtures
        .get_mut(base_url.as_str())
        .expect("matched internal HTTP fixture should exist");
    fixture.requests.push(http_fixture_request_target(url));
    let response = fixture.responses.pop_front().ok_or_else(|| {
        BroadwebdError::Request(format!("internal HTTP fixture {base_url} has no response"))
    })?;

    if response.body.len() > budget.max_http_response_bytes {
        return Err(BroadwebdError::ResponseTooLarge {
            limit: budget.max_http_response_bytes,
            actual: response.body.len(),
        });
    }

    let content_type = infer_content_type(
        url_text,
        response.content_type.as_deref(),
        response.body.as_slice(),
    );
    Ok(HttpFetchResponse::new(
        url_text,
        response.status_code,
        content_type,
        response.headers.clone(),
        response.body.clone(),
    ))
}

#[cfg(any(test, feature = "test-fixtures"))]
#[derive(Clone, Debug, Eq, PartialEq)]
struct InternalFixtureHttp {
    responses: std::collections::VecDeque<InternalFixtureHttpResponse>,
    requests: Vec<String>,
}

#[cfg(any(test, feature = "test-fixtures"))]
fn http_fixture_request_target(url: &Url) -> String {
    let mut target = url.path().to_string();
    if let Some(host) = url.host_str()
        && !host.starts_with("fixture-")
        && !host.starts_with("missing-")
        && let Some(token) = internal_fixture_http_path_token(url)
    {
        let prefix = format!("/{token}");
        if target == prefix {
            target = "/".to_string();
        } else if let Some(stripped) = target.strip_prefix(&prefix) {
            target = stripped.to_string();
        }
    }
    if target.is_empty() {
        target.push('/');
    }
    if let Some(query) = url.query() {
        target.push('?');
        target.push_str(query);
    }
    target
}

#[cfg(any(test, feature = "test-fixtures"))]
fn internal_fixture_http_network_id(url: &Url) -> Option<String> {
    let host = url.host_str()?;
    if host.starts_with("fixture-") || host.starts_with("missing-") {
        return None;
    }
    internal_fixture_http_path_token(url).map(|_| host.to_string())
}

#[cfg(any(test, feature = "test-fixtures"))]
fn internal_fixture_http_path_token(url: &Url) -> Option<String> {
    url.path_segments()
        .and_then(|mut segments| segments.next().map(str::to_string))
}

#[cfg(any(test, feature = "test-fixtures"))]
fn internal_fixture_http_responses()
-> &'static std::sync::Mutex<std::collections::BTreeMap<String, InternalFixtureHttp>> {
    use std::collections::BTreeMap;
    use std::sync::{Mutex, OnceLock};

    static RESPONSES: OnceLock<Mutex<BTreeMap<String, InternalFixtureHttp>>> = OnceLock::new();

    RESPONSES.get_or_init(|| Mutex::new(BTreeMap::new()))
}

pub(crate) fn infer_content_type(
    final_url: &str,
    header_content_type: Option<&str>,
    body: &[u8],
) -> Option<String> {
    let header_content_type = header_content_type
        .map(str::trim)
        .filter(|content_type| !content_type.is_empty());
    header_content_type
        .filter(|content_type| !is_generic_binary_content_type(content_type))
        .map(str::to_string)
        .or_else(|| content_type_from_html_body(body))
        .or_else(|| content_type_from_path(final_url))
        .or_else(|| header_content_type.map(str::to_string))
}

fn response_disposition(
    status_code: u16,
    final_url: &str,
    content_type: Option<&str>,
    headers: &[HttpHeader],
) -> FetchDisposition {
    if !(200..=299).contains(&status_code) {
        return FetchDisposition::ErrorPage { status_code };
    }

    if is_attachment_response(headers) {
        return FetchDisposition::Download {
            suggested_filename: content_disposition_filename(headers)
                .unwrap_or_else(|| suggested_filename(final_url)),
        };
    }

    if is_html_content_type(content_type) {
        return FetchDisposition::RenderHtml;
    }

    FetchDisposition::Download {
        suggested_filename: suggested_filename(final_url),
    }
}

fn is_attachment_response(headers: &[HttpHeader]) -> bool {
    headers
        .iter()
        .find(|header| header.name.eq_ignore_ascii_case("content-disposition"))
        .is_some_and(|header| {
            header
                .value
                .split(';')
                .next()
                .unwrap_or_default()
                .trim()
                .eq_ignore_ascii_case("attachment")
        })
}

fn content_disposition_filename(headers: &[HttpHeader]) -> Option<String> {
    let value = headers
        .iter()
        .find(|header| header.name.eq_ignore_ascii_case("content-disposition"))?
        .value
        .as_str();
    value
        .split(';')
        .skip(1)
        .filter_map(|parameter| parameter.split_once('='))
        .find_map(|(name, value)| {
            if name.trim().eq_ignore_ascii_case("filename") {
                let filename = unquote_header_value(value.trim());
                if filename.is_empty() {
                    None
                } else {
                    Some(filename.to_string())
                }
            } else {
                None
            }
        })
}

fn unquote_header_value(value: &str) -> &str {
    value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .unwrap_or(value)
}

fn is_html_content_type(content_type: Option<&str>) -> bool {
    let Some(content_type) = content_type else {
        return false;
    };
    matches!(
        media_type(content_type).as_str(),
        "text/html" | "application/xhtml+xml"
    )
}

fn is_generic_binary_content_type(content_type: &str) -> bool {
    matches!(
        media_type(content_type).as_str(),
        "application/octet-stream" | "binary/octet-stream"
    )
}

fn media_type(content_type: &str) -> String {
    content_type
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
}

fn content_type_from_path(url: &str) -> Option<String> {
    let path = Url::parse(url).ok()?.path().to_ascii_lowercase();
    let content_type = match path.rsplit('.').next()? {
        "html" | "htm" => "text/html; charset=utf-8",
        "xhtml" => "application/xhtml+xml",
        "css" => "text/css",
        "js" | "mjs" => "application/javascript",
        "json" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "wasm" => "application/wasm",
        "txt" => "text/plain; charset=utf-8",
        _ => return None,
    };
    Some(content_type.to_string())
}

fn content_type_from_html_body(body: &[u8]) -> Option<String> {
    let prefix = std::str::from_utf8(body.get(..body.len().min(256))?)
        .ok()?
        .trim_start()
        .to_ascii_lowercase();
    if is_html_body_prefix(&prefix) {
        return Some("text/html; charset=utf-8".to_string());
    }
    None
}

fn is_html_body_prefix(prefix: &str) -> bool {
    const HTML_PREFIXES: &[&str] = &[
        "<!doctype html",
        "<html",
        "<head",
        "<body",
        "<title",
        "<main",
        "<section",
        "<article",
        "<header",
        "<nav",
        "<div",
        "<p",
        "<h1",
        "<h2",
        "<h3",
        "<h4",
        "<h5",
        "<h6",
        "<style",
        "<script",
    ];
    HTML_PREFIXES
        .iter()
        .any(|html_prefix| prefix.starts_with(html_prefix))
}

fn suggested_filename(final_url: &str) -> String {
    Url::parse(final_url)
        .ok()
        .and_then(|url| {
            url.path_segments()
                .and_then(|mut segments| segments.next_back().map(str::to_string))
        })
        .filter(|segment| !segment.is_empty())
        .unwrap_or_else(|| "download".to_string())
}
