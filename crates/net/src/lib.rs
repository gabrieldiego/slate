#![forbid(unsafe_code)]

use core::fmt;
use slate_routing::RoutingPlan;
use std::time::Duration;

pub const FIREFOX_COMPAT_VERSION: &str = "154.0";
/// Generic desktop Firefox UA for page fetches; avoids exposing Slate/Servo details to sites.
pub const BROWSER_USER_AGENT: &str =
    "Mozilla/5.0 (X11; Linux x86_64; rv:154.0) Gecko/20100101 Firefox/154.0";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(12);
const MAX_HTML_BYTES: usize = 2 * 1024 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RequestPolicy {
    pub allow_startup_network: bool,
}

impl Default for RequestPolicy {
    fn default() -> Self {
        Self {
            allow_startup_network: false,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlannedRequest {
    pub route: RoutingPlan,
    pub policy: RequestPolicy,
}

impl PlannedRequest {
    pub fn new(route: RoutingPlan, policy: RequestPolicy) -> Self {
        Self { route, policy }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FetchedPage {
    pub final_url: String,
    pub body: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RawFetchedPage {
    final_url: String,
    bytes: Vec<u8>,
}

trait WebPageTransport {
    fn get(&self, address: &str, user_agent: &str) -> Result<RawFetchedPage, FetchError>;
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct ReqwestWebPageTransport;

#[derive(Debug)]
pub enum FetchError {
    UnsupportedScheme(String),
    Request(String),
    TooLarge { limit: usize, actual: usize },
}

impl fmt::Display for FetchError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedScheme(address) => write!(formatter, "unsupported web URL: {address}"),
            Self::Request(error) => formatter.write_str(error),
            Self::TooLarge { limit, actual } => {
                write!(
                    formatter,
                    "response too large: {actual} bytes over {limit} byte limit"
                )
            }
        }
    }
}

impl std::error::Error for FetchError {}

pub fn fetch_web_page(address: &str) -> Result<FetchedPage, FetchError> {
    fetch_web_page_with_transport(address, &ReqwestWebPageTransport)
}

fn fetch_web_page_with_transport(
    address: &str,
    transport: &impl WebPageTransport,
) -> Result<FetchedPage, FetchError> {
    if !address.starts_with("http://") && !address.starts_with("https://") {
        return Err(FetchError::UnsupportedScheme(address.to_string()));
    }

    let response = transport.get(address, BROWSER_USER_AGENT)?;
    if response.bytes.len() > MAX_HTML_BYTES {
        return Err(FetchError::TooLarge {
            limit: MAX_HTML_BYTES,
            actual: response.bytes.len(),
        });
    }

    Ok(FetchedPage {
        final_url: response.final_url,
        body: String::from_utf8_lossy(&response.bytes).into_owned(),
    })
}

impl WebPageTransport for ReqwestWebPageTransport {
    fn get(&self, address: &str, user_agent: &str) -> Result<RawFetchedPage, FetchError> {
        let client = reqwest::blocking::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .user_agent(user_agent)
            .redirect(reqwest::redirect::Policy::limited(6))
            .build()
            .map_err(request_error)?;
        let response = client
            .get(address)
            .send()
            .and_then(reqwest::blocking::Response::error_for_status)
            .map_err(request_error)?;
        let final_url = response.url().to_string();
        let bytes = response.bytes().map_err(request_error)?;

        Ok(RawFetchedPage {
            final_url,
            bytes: bytes.to_vec(),
        })
    }
}

fn request_error(error: reqwest::Error) -> FetchError {
    FetchError::Request(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::{
        BROWSER_USER_AGENT, FIREFOX_COMPAT_VERSION, FetchError, RawFetchedPage, WebPageTransport,
        fetch_web_page, fetch_web_page_with_transport,
    };

    #[test]
    fn rejects_non_web_schemes() {
        assert!(matches!(
            fetch_web_page("file:///tmp/page.html"),
            Err(FetchError::UnsupportedScheme(_))
        ));
    }

    #[test]
    fn browser_user_agent_tracks_firefox_compat_version() {
        assert!(BROWSER_USER_AGENT.contains(&format!("rv:{FIREFOX_COMPAT_VERSION}")));
        assert!(BROWSER_USER_AGENT.contains(&format!("Firefox/{FIREFOX_COMPAT_VERSION}")));
        assert!(!BROWSER_USER_AGENT.contains("Slate/"));
    }

    #[test]
    fn fetches_html_from_internal_fixture_transport() {
        let transport = FixtureWebPageTransport {
            expected_user_agent: BROWSER_USER_AGENT,
            body: b"<!doctype html><title>Net Fixture</title><h1>Fetched</h1><p>Network body.</p>",
        };

        let page = fetch_web_page_with_transport("http://fixture.invalid/", &transport)
            .expect("fetch fixture page");

        assert_eq!(page.final_url, "http://fixture.invalid/");
        assert!(page.body.contains("Net Fixture"));
        assert!(page.body.contains("Network body."));
    }

    struct FixtureWebPageTransport {
        expected_user_agent: &'static str,
        body: &'static [u8],
    }

    impl WebPageTransport for FixtureWebPageTransport {
        fn get(&self, address: &str, user_agent: &str) -> Result<RawFetchedPage, FetchError> {
            assert_eq!(user_agent, self.expected_user_agent);
            Ok(RawFetchedPage {
                final_url: address.to_string(),
                bytes: self.body.to_vec(),
            })
        }
    }
}
