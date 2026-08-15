use crate::{BroadwebdError, DEFAULT_PROFILE, ResourceBudget};
use std::path::PathBuf;
use std::time::Duration;
use url::Url;

const USER_AGENT: &str = "Slate/0.0.1";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(12);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HttpFetchRequest {
    pub profile: String,
    pub url: String,
    pub transport_id: Option<String>,
}

impl HttpFetchRequest {
    pub fn new(profile: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            profile: profile.into(),
            url: url.into(),
            transport_id: None,
        }
    }

    pub fn default_profile(url: impl Into<String>) -> Self {
        Self::new(DEFAULT_PROFILE, url)
    }

    pub fn through_transport(mut self, transport_id: impl Into<String>) -> Self {
        self.transport_id = Some(transport_id.into());
        self
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransportHttpRequest {
    pub profile: String,
    pub url: String,
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
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FetchRouteInfo {
    pub profile: String,
    pub transport_id: String,
    pub privacy_boundary: String,
}

impl FetchRouteInfo {
    pub fn new(
        profile: impl Into<String>,
        transport_id: impl Into<String>,
        privacy_boundary: impl Into<String>,
    ) -> Self {
        Self {
            profile: profile.into(),
            transport_id: transport_id.into(),
            privacy_boundary: privacy_boundary.into(),
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
        let disposition = response_disposition(&final_url, content_type.as_deref());
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
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ServiceResponse {
    HttpFetch(HttpFetchResponse),
}

pub(crate) fn parse_http_url(input: &str) -> Result<Url, BroadwebdError> {
    Url::parse(input).map_err(|error| BroadwebdError::InvalidUrl(error.to_string()))
}

pub(crate) fn fetch_http_url(
    url: Url,
    budget: &ResourceBudget,
) -> Result<HttpFetchResponse, BroadwebdError> {
    let client = reqwest::blocking::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .user_agent(USER_AGENT)
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

fn response_disposition(final_url: &str, content_type: Option<&str>) -> FetchDisposition {
    if is_html_content_type(content_type) {
        return FetchDisposition::RenderHtml;
    }

    FetchDisposition::Download {
        suggested_filename: suggested_filename(final_url),
    }
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
