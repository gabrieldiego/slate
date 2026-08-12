#![forbid(unsafe_code)]

use core::fmt;
use slate_routing::RoutingPlan;
use std::time::Duration;

const USER_AGENT: &str = "Slate/0.1";
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
    if !address.starts_with("http://") && !address.starts_with("https://") {
        return Err(FetchError::UnsupportedScheme(address.to_string()));
    }

    let client = reqwest::blocking::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .user_agent(USER_AGENT)
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
    if bytes.len() > MAX_HTML_BYTES {
        return Err(FetchError::TooLarge {
            limit: MAX_HTML_BYTES,
            actual: bytes.len(),
        });
    }

    Ok(FetchedPage {
        final_url,
        body: String::from_utf8_lossy(&bytes).into_owned(),
    })
}

fn request_error(error: reqwest::Error) -> FetchError {
    FetchError::Request(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::{FetchError, fetch_web_page};
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    #[test]
    fn rejects_non_web_schemes() {
        assert!(matches!(
            fetch_web_page("file:///tmp/page.html"),
            Err(FetchError::UnsupportedScheme(_))
        ));
    }

    #[test]
    fn fetches_html_from_local_http_server() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind local server");
        let address = format!("http://{}", listener.local_addr().expect("local address"));
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept request");
            let mut request = [0_u8; 1024];
            let _ = stream.read(&mut request).expect("read request");
            let body =
                "<!doctype html><title>Net Fixture</title><h1>Fetched</h1><p>Network body.</p>";
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream
                .write_all(response.as_bytes())
                .expect("write response");
        });

        let page = fetch_web_page(&address).expect("fetch local page");
        server.join().expect("server thread");

        assert_eq!(page.final_url, format!("{address}/"));
        assert!(page.body.contains("Net Fixture"));
        assert!(page.body.contains("Network body."));
    }
}
