use crate::{BroadwebdError, DEFAULT_PROFILE, ResourceBudget};
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
pub struct HttpFetchResponse {
    pub final_url: String,
    pub status_code: u16,
    pub content_type: Option<String>,
    pub headers: Vec<HttpHeader>,
    pub body: Vec<u8>,
    pub disposition: FetchDisposition,
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
        }
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
    let content_type = header_value(response.headers(), reqwest::header::CONTENT_TYPE);
    let headers = response_headers(response.headers());
    let body = response.bytes().map_err(request_error)?.to_vec();
    if body.len() > budget.max_http_response_bytes {
        return Err(BroadwebdError::ResponseTooLarge {
            limit: budget.max_http_response_bytes,
            actual: body.len(),
        });
    }

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
    let media_type = content_type
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    matches!(media_type.as_str(), "text/html" | "application/xhtml+xml")
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
