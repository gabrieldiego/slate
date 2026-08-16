use super::address::{
    TorHttpTarget, TorNetworkScheme, normalize_tor_navigation_url, tor_http_target,
};
use crate::http::infer_content_type;
use crate::{
    BroadwebStatusKind, BroadwebStatusReporter, BroadwebdError, FetchRouteInfo, HttpFetchResponse,
    HttpHeader, PluginKind, PluginMetadata, ResourceBudget, ResourceProfile, TOR_ARTI_HTTP_PLUGIN,
    TransportHttpRequest, TransportPlugin,
};
use arti_client::{TorClient, TorClientConfig};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::runtime::{Builder, Runtime};
use tor_rtcompat::PreferredRuntime;
use url::Url;

const MAX_HEADER_BYTES: usize = 64 * 1024;
const MAX_REDIRECTS: usize = 6;
const TOR_BOOTSTRAP_TIMEOUT: Duration = Duration::from_secs(90);
const TOR_REQUEST_TIMEOUT: Duration = Duration::from_secs(45);
const USER_AGENT: &str = "Slate/0.0.1";

pub struct TorArtiHttpTransport {
    state: Mutex<Option<TorRuntimeState>>,
    status: BroadwebStatusReporter,
}

struct TorRuntimeState {
    runtime: Runtime,
    client: Option<Arc<TorClient<PreferredRuntime>>>,
}

impl TorRuntimeState {
    fn new() -> Result<Self, BroadwebdError> {
        let runtime = Builder::new_multi_thread()
            .thread_name("slate-broadwebd-tor")
            .enable_all()
            .build()
            .map_err(BroadwebdError::from)?;
        Ok(Self {
            runtime,
            client: None,
        })
    }
}

impl TorArtiHttpTransport {
    pub fn new() -> Result<Self, BroadwebdError> {
        Self::with_status(BroadwebStatusReporter::new())
    }

    pub fn with_status(status: BroadwebStatusReporter) -> Result<Self, BroadwebdError> {
        Ok(Self {
            state: Mutex::new(None),
            status,
        })
    }

    fn client(&self, target: &str) -> Result<Arc<TorClient<PreferredRuntime>>, BroadwebdError> {
        let mut state = self
            .state
            .lock()
            .expect("Tor runtime cache should not be poisoned");
        let state = tor_runtime_state(&mut state)?;
        if let Some(client) = state.client.clone() {
            return Ok(client);
        }

        self.status.set(
            BroadwebStatusKind::Fetching,
            "Bootstrapping Tor",
            Some(target.to_string()),
            Some("arti".to_string()),
        );
        let client = match state
            .runtime
            .block_on(async {
                tokio::time::timeout(
                    TOR_BOOTSTRAP_TIMEOUT,
                    TorClient::create_bootstrapped(TorClientConfig::default()),
                )
                .await
            })
            .map_err(|_| BroadwebdError::Request("Tor bootstrap timed out".to_string()))
            .and_then(|result| result.map_err(tor_error))
        {
            Ok(client) => client,
            Err(error) => {
                self.status.set(
                    BroadwebStatusKind::Error,
                    "Tor bootstrap failed",
                    Some(target.to_string()),
                    Some("arti".to_string()),
                );
                return Err(error);
            }
        };
        state.client = Some(client.clone());
        Ok(client)
    }

    fn with_runtime<R>(
        &self,
        run: impl FnOnce(&Runtime) -> Result<R, BroadwebdError>,
    ) -> Result<R, BroadwebdError> {
        let mut state = self
            .state
            .lock()
            .expect("Tor runtime cache should not be poisoned");
        let state = tor_runtime_state(&mut state)?;
        run(&state.runtime)
    }
}

fn tor_runtime_state(
    state: &mut Option<TorRuntimeState>,
) -> Result<&mut TorRuntimeState, BroadwebdError> {
    if state.is_none() {
        *state = Some(TorRuntimeState::new()?);
    }
    Ok(state
        .as_mut()
        .expect("Tor runtime state should be initialized"))
}

impl Default for TorArtiHttpTransport {
    fn default() -> Self {
        Self::new().expect("Tor runtime should initialize")
    }
}

impl TransportPlugin for TorArtiHttpTransport {
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata::new(TOR_ARTI_HTTP_PLUGIN, PluginKind::Transport)
            .with_capabilities(&["tor", "onion", "http-fetch", "arti", "http-over-tor"])
            .with_privacy_boundary(
                "embedded Arti Tor client; .onion hosts are reached through Tor circuits and must not fall back to direct DNS",
            )
            .with_resource_profile(ResourceProfile::High)
    }

    fn fetch_http(
        &self,
        request: &TransportHttpRequest,
        budget: &ResourceBudget,
    ) -> Result<HttpFetchResponse, BroadwebdError> {
        let mut current = normalize_tor_navigation_url(&request.url)?;
        let mut client: Option<Arc<TorClient<PreferredRuntime>>> = None;

        for redirect_count in 0..=MAX_REDIRECTS {
            let target = tor_http_target(&current)?;
            if matches!(target.network_scheme, TorNetworkScheme::Https) {
                return Err(BroadwebdError::UnsupportedRequest(
                    "HTTPS over embedded Tor is not implemented yet".to_string(),
                ));
            }
            let tor_client = match &client {
                Some(client) => client.clone(),
                None => {
                    let tor_client = self.client(&current)?;
                    client = Some(tor_client.clone());
                    tor_client
                }
            };

            self.status.set(
                BroadwebStatusKind::Fetching,
                "Opening Tor circuit",
                Some(target.final_url.clone()),
                Some(target.host.clone()),
            );
            let raw_response = match self.with_runtime(|runtime| {
                runtime
                    .block_on(async {
                        tokio::time::timeout(
                            TOR_REQUEST_TIMEOUT,
                            fetch_plain_http_over_tor(tor_client, target.clone(), budget),
                        )
                        .await
                    })
                    .map_err(|_| {
                        BroadwebdError::Request("Tor HTTP request timed out".to_string())
                    })?
            }) {
                Ok(response) => response,
                Err(error) => {
                    self.publish_error_status(&target);
                    return Err(error);
                }
            };
            let response = match http_response_from_bytes(&target.final_url, raw_response, budget) {
                Ok(response) => response,
                Err(error) => {
                    self.publish_error_status(&target);
                    return Err(error);
                }
            };

            if let Some(redirect_url) = redirect_target(&response)? {
                if redirect_count == MAX_REDIRECTS {
                    return Err(BroadwebdError::UnsupportedRequest(format!(
                        "too many Tor redirects while fetching {}",
                        request.url
                    )));
                }
                current = redirect_url;
                self.status.set(
                    BroadwebStatusKind::Fetching,
                    "Following Tor redirect",
                    Some(current.clone()),
                    Some(target.host),
                );
                continue;
            }

            self.status.set(
                BroadwebStatusKind::Complete,
                "Loaded via Tor",
                Some(response.final_url.clone()),
                Some(target.host),
            );
            return Ok(response.with_route(FetchRouteInfo::new(
                request.profile.clone(),
                TOR_ARTI_HTTP_PLUGIN,
                "embedded Arti Tor client; onion URL and timing metadata stay inside the Tor routing boundary",
                request.purpose,
            )));
        }

        Err(BroadwebdError::UnsupportedRequest(format!(
            "too many Tor redirects while fetching {}",
            request.url
        )))
    }
}

impl TorArtiHttpTransport {
    fn publish_error_status(&self, target: &TorHttpTarget) {
        self.status.set(
            BroadwebStatusKind::Error,
            "Tor fetch failed",
            Some(target.final_url.clone()),
            Some(target.host.clone()),
        );
    }
}

async fn fetch_plain_http_over_tor(
    client: Arc<TorClient<PreferredRuntime>>,
    target: TorHttpTarget,
    budget: &ResourceBudget,
) -> Result<Vec<u8>, BroadwebdError> {
    let mut stream = client
        .connect((target.host.as_str(), target.port))
        .await
        .map_err(tor_error)?;
    let request = format!(
        "GET {} HTTP/1.1\r\nHost: {}\r\nUser-Agent: {}\r\nAccept: */*\r\nConnection: close\r\n\r\n",
        target.path_and_query, target.host_header, USER_AGENT
    );
    stream
        .write_all(request.as_bytes())
        .await
        .map_err(BroadwebdError::from)?;
    stream.flush().await.map_err(BroadwebdError::from)?;

    let mut response = Vec::new();
    let limit = budget
        .max_http_response_bytes
        .saturating_add(MAX_HEADER_BYTES);
    let mut chunk = [0_u8; 8192];
    loop {
        let read = stream
            .read(&mut chunk)
            .await
            .map_err(BroadwebdError::from)?;
        if read == 0 {
            break;
        }
        response.extend_from_slice(&chunk[..read]);
        if response.len() > limit {
            return Err(BroadwebdError::ResponseTooLarge {
                limit: budget.max_http_response_bytes,
                actual: response.len(),
            });
        }
    }
    Ok(response)
}

pub fn http_response_from_bytes(
    final_url: &str,
    response: Vec<u8>,
    budget: &ResourceBudget,
) -> Result<HttpFetchResponse, BroadwebdError> {
    let header_end = find_header_end(&response).ok_or_else(|| {
        BroadwebdError::Request("Tor HTTP response ended before headers completed".to_string())
    })?;
    if header_end > MAX_HEADER_BYTES {
        return Err(BroadwebdError::ResponseTooLarge {
            limit: budget.max_http_response_bytes,
            actual: header_end,
        });
    }

    let header_text = std::str::from_utf8(&response[..header_end])
        .map_err(|error| BroadwebdError::Request(format!("invalid HTTP headers: {error}")))?;
    let mut lines = header_text.split("\r\n");
    let status_line = lines
        .next()
        .ok_or_else(|| BroadwebdError::Request("missing HTTP status line".to_string()))?;
    let status_code = status_code(status_line)?;
    let headers = lines.filter_map(http_header).collect::<Vec<_>>();
    let mut body = response[header_end + 4..].to_vec();

    if header_value(&headers, "transfer-encoding")
        .is_some_and(|value| value.to_ascii_lowercase().contains("chunked"))
    {
        body = decode_chunked_body(&body, budget.max_http_response_bytes)?;
    }

    if body.len() > budget.max_http_response_bytes {
        return Err(BroadwebdError::ResponseTooLarge {
            limit: budget.max_http_response_bytes,
            actual: body.len(),
        });
    }
    let content_type = infer_content_type(
        final_url,
        header_value(&headers, "content-type").as_deref(),
        &body,
    );
    Ok(HttpFetchResponse::new(
        final_url.to_string(),
        status_code,
        content_type,
        headers,
        body,
    ))
}

fn redirect_target(response: &HttpFetchResponse) -> Result<Option<String>, BroadwebdError> {
    if !matches!(response.status_code, 301 | 302 | 303 | 307 | 308) {
        return Ok(None);
    }
    let Some(location) = header_value(&response.headers, "location") else {
        return Ok(None);
    };
    let base = Url::parse(&response.final_url)
        .map_err(|error| BroadwebdError::InvalidUrl(error.to_string()))?;
    let joined = base
        .join(location.trim())
        .map_err(|error| BroadwebdError::InvalidUrl(error.to_string()))?;
    normalize_tor_navigation_url(joined.as_str()).map(Some)
}

fn find_header_end(response: &[u8]) -> Option<usize> {
    response.windows(4).position(|window| window == b"\r\n\r\n")
}

fn status_code(status_line: &str) -> Result<u16, BroadwebdError> {
    let mut parts = status_line.split_whitespace();
    let version = parts
        .next()
        .ok_or_else(|| BroadwebdError::Request("missing HTTP version".to_string()))?;
    if !version.starts_with("HTTP/") {
        return Err(BroadwebdError::Request(format!(
            "invalid HTTP status line: {status_line}"
        )));
    }
    parts
        .next()
        .ok_or_else(|| BroadwebdError::Request("missing HTTP status code".to_string()))?
        .parse::<u16>()
        .map_err(|error| BroadwebdError::Request(format!("invalid HTTP status code: {error}")))
}

fn http_header(line: &str) -> Option<HttpHeader> {
    let (name, value) = line.split_once(':')?;
    let name = name.trim();
    let value = value.trim();
    (!name.is_empty()).then(|| HttpHeader {
        name: name.to_string(),
        value: value.to_string(),
    })
}

fn header_value(headers: &[HttpHeader], name: &str) -> Option<String> {
    headers
        .iter()
        .find(|header| header.name.eq_ignore_ascii_case(name))
        .map(|header| header.value.clone())
}

fn decode_chunked_body(body: &[u8], limit: usize) -> Result<Vec<u8>, BroadwebdError> {
    let mut output = Vec::new();
    let mut offset = 0;
    loop {
        let line_end = find_crlf(&body[offset..])
            .ok_or_else(|| BroadwebdError::Request("truncated chunk size".to_string()))?;
        let size_line = std::str::from_utf8(&body[offset..offset + line_end])
            .map_err(|error| BroadwebdError::Request(format!("invalid chunk size: {error}")))?;
        let size =
            usize::from_str_radix(size_line.split(';').next().unwrap_or_default().trim(), 16)
                .map_err(|error| BroadwebdError::Request(format!("invalid chunk size: {error}")))?;
        offset += line_end + 2;
        if size == 0 {
            break;
        }
        let chunk_end = offset.saturating_add(size);
        if chunk_end > body.len() {
            return Err(BroadwebdError::Request("truncated chunk body".to_string()));
        }
        output.extend_from_slice(&body[offset..chunk_end]);
        if output.len() > limit {
            return Err(BroadwebdError::ResponseTooLarge {
                limit,
                actual: output.len(),
            });
        }
        offset = chunk_end;
        if body.get(offset..offset + 2) != Some(b"\r\n") {
            return Err(BroadwebdError::Request(
                "chunk body missing CRLF terminator".to_string(),
            ));
        }
        offset += 2;
    }
    Ok(output)
}

fn find_crlf(input: &[u8]) -> Option<usize> {
    input.windows(2).position(|window| window == b"\r\n")
}

fn tor_error(error: arti_client::Error) -> BroadwebdError {
    BroadwebdError::Request(format!("Tor request failed: {error}"))
}

#[cfg(test)]
mod tests {
    use super::{http_response_from_bytes, redirect_target};
    use crate::{
        BroadwebdError, FetchDisposition, FetchPurpose, ResourceBudget, TOR_ARTI_HTTP_PLUGIN,
        TorArtiHttpTransport, TransportHttpRequest, TransportPlugin,
    };

    #[test]
    fn tor_http_response_parser_decodes_content_type_and_body() {
        let response = http_response_from_bytes(
            "tor+http://example.onion/index.html",
            b"HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: 31\r\n\r\n<!doctype html><title>Tor</title>"
                .to_vec(),
            &ResourceBudget::default(),
        )
        .unwrap();

        assert_eq!(response.status_code, 200);
        assert_eq!(
            response.content_type.as_deref(),
            Some("text/html; charset=utf-8")
        );
        assert_eq!(response.disposition, FetchDisposition::RenderHtml);
        assert!(response.body_text_lossy().contains("Tor"));
    }

    #[test]
    fn tor_http_response_parser_decodes_chunked_body() {
        let response = http_response_from_bytes(
            "tor+http://example.onion/file.txt",
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n6\r\n world\r\n0\r\n\r\n"
                .to_vec(),
            &ResourceBudget::default(),
        )
        .unwrap();

        assert_eq!(response.body, b"hello world");
    }

    #[test]
    fn tor_redirects_are_normalized_to_tor_schemes() {
        let response = http_response_from_bytes(
            "tor+http://example.onion/start",
            b"HTTP/1.1 302 Found\r\nLocation: http://target.onion/next\r\n\r\n".to_vec(),
            &ResourceBudget::default(),
        )
        .unwrap();

        assert_eq!(
            redirect_target(&response).unwrap().as_deref(),
            Some("tor+http://target.onion/next")
        );
    }

    #[test]
    fn tor_transport_rejects_https_before_bootstrapping() {
        let transport = TorArtiHttpTransport::new().unwrap();
        let request = TransportHttpRequest {
            profile: "default".to_string(),
            url: "tor+https://example.onion/".to_string(),
            purpose: FetchPurpose::Navigation,
        };

        assert!(matches!(
            transport.fetch_http(&request, &ResourceBudget::default()),
            Err(BroadwebdError::UnsupportedRequest(_))
        ));
    }

    #[test]
    fn tor_transport_reports_metadata() {
        let transport = TorArtiHttpTransport::new().unwrap();
        let metadata = transport.metadata();

        assert_eq!(metadata.id, TOR_ARTI_HTTP_PLUGIN);
        assert!(metadata.capabilities.iter().any(|item| item == "arti"));
    }
}
