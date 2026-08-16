use crate::BroadwebdError;
use url::Url;

pub const TOR_HTTP_SCHEME: &str = "tor+http";
pub const TOR_HTTPS_SCHEME: &str = "tor+https";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TorHttpTarget {
    pub final_url: String,
    pub network_scheme: TorNetworkScheme,
    pub host: String,
    pub port: u16,
    pub host_header: String,
    pub path_and_query: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TorNetworkScheme {
    Http,
    Https,
}

impl TorNetworkScheme {
    pub fn from_url_scheme(scheme: &str) -> Option<Self> {
        match scheme {
            "http" | TOR_HTTP_SCHEME => Some(Self::Http),
            "https" | TOR_HTTPS_SCHEME => Some(Self::Https),
            _ => None,
        }
    }

    pub fn source_scheme(self) -> &'static str {
        match self {
            Self::Http => "http",
            Self::Https => "https",
        }
    }

    pub fn tor_scheme(self) -> &'static str {
        match self {
            Self::Http => TOR_HTTP_SCHEME,
            Self::Https => TOR_HTTPS_SCHEME,
        }
    }

    pub fn default_port(self) -> u16 {
        match self {
            Self::Http => 80,
            Self::Https => 443,
        }
    }
}

pub fn is_tor_http_scheme(scheme: &str) -> bool {
    matches!(scheme, TOR_HTTP_SCHEME | TOR_HTTPS_SCHEME)
}

pub fn is_onion_url(url: &Url) -> bool {
    url.host_str().is_some_and(is_onion_host)
}

pub fn is_onion_host(host: &str) -> bool {
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    let Some(name) = host.strip_suffix(".onion") else {
        return false;
    };
    !name.is_empty()
        && name.split('.').all(|label| {
            !label.is_empty()
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        })
}

pub fn tor_url_from_http_url(url: &Url) -> Result<Option<String>, BroadwebdError> {
    let Some(network_scheme) = TorNetworkScheme::from_url_scheme(url.scheme()) else {
        return Ok(None);
    };
    if is_tor_http_scheme(url.scheme()) {
        return Ok(Some(url_without_fragment(url)));
    }
    if !is_onion_url(url) {
        return Ok(None);
    }
    if !matches!(url.scheme(), "http" | "https") {
        return Ok(None);
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(BroadwebdError::UnsupportedRequest(
            "Tor onion URLs must not include userinfo".to_string(),
        ));
    }

    let host = url.host_str().ok_or_else(|| {
        BroadwebdError::InvalidUrl(format!("{} is missing an onion host", url.as_str()))
    })?;
    Ok(Some(build_tor_url(
        network_scheme,
        host,
        url.port(),
        url.path(),
        url.query(),
    )))
}

pub fn tor_http_target(source: &str) -> Result<TorHttpTarget, BroadwebdError> {
    let url = Url::parse(source).map_err(|error| BroadwebdError::InvalidUrl(error.to_string()))?;
    let network_scheme = TorNetworkScheme::from_url_scheme(url.scheme()).ok_or_else(|| {
        BroadwebdError::UnsupportedRequest(format!("unsupported Tor URL scheme: {}", url.scheme()))
    })?;
    if !matches!(
        url.scheme(),
        "http" | "https" | TOR_HTTP_SCHEME | TOR_HTTPS_SCHEME
    ) {
        return Err(BroadwebdError::UnsupportedRequest(format!(
            "unsupported Tor URL scheme: {}",
            url.scheme()
        )));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(BroadwebdError::UnsupportedRequest(
            "Tor onion URLs must not include userinfo".to_string(),
        ));
    }

    let host = url
        .host_str()
        .ok_or_else(|| BroadwebdError::InvalidUrl(format!("{source} is missing an onion host")))?
        .trim_end_matches('.')
        .to_ascii_lowercase();
    if !is_onion_host(&host) {
        return Err(BroadwebdError::UnsupportedRequest(format!(
            "Tor transport only accepts .onion hosts: {host}"
        )));
    }
    let port = url.port().unwrap_or_else(|| network_scheme.default_port());
    let host_header = if port == network_scheme.default_port() {
        host.clone()
    } else {
        format!("{host}:{port}")
    };
    let mut path_and_query = if url.path().is_empty() {
        "/".to_string()
    } else {
        url.path().to_string()
    };
    if let Some(query) = url.query() {
        path_and_query.push('?');
        path_and_query.push_str(query);
    }
    let final_url = build_tor_url(
        network_scheme,
        &host,
        (port != network_scheme.default_port()).then_some(port),
        url.path(),
        url.query(),
    );

    Ok(TorHttpTarget {
        final_url,
        network_scheme,
        host,
        port,
        host_header,
        path_and_query,
    })
}

pub fn normalize_tor_navigation_url(source: &str) -> Result<String, BroadwebdError> {
    let url = Url::parse(source).map_err(|error| BroadwebdError::InvalidUrl(error.to_string()))?;
    if let Some(url) = tor_url_from_http_url(&url)? {
        return Ok(url);
    }
    tor_http_target(source).map(|target| target.final_url)
}

fn build_tor_url(
    network_scheme: TorNetworkScheme,
    host: &str,
    port: Option<u16>,
    path: &str,
    query: Option<&str>,
) -> String {
    let mut output = format!("{}://{}", network_scheme.tor_scheme(), host);
    if let Some(port) = port {
        output.push(':');
        output.push_str(&port.to_string());
    }
    if path.is_empty() {
        output.push('/');
    } else {
        output.push_str(path);
    }
    if let Some(query) = query {
        output.push('?');
        output.push_str(query);
    }
    output
}

fn url_without_fragment(url: &Url) -> String {
    let mut url = url.clone();
    url.set_fragment(None);
    url.to_string()
}

#[cfg(test)]
mod tests {
    use super::{
        TOR_HTTP_SCHEME, TOR_HTTPS_SCHEME, TorNetworkScheme, is_onion_host,
        normalize_tor_navigation_url, tor_http_target, tor_url_from_http_url,
    };
    use url::Url;

    #[test]
    fn onion_host_validation_is_suffix_based() {
        assert!(is_onion_host("example.onion"));
        assert!(is_onion_host("sub.example.onion"));
        assert!(is_onion_host("EXAMPLE.ONION"));
        assert!(!is_onion_host("example.com"));
        assert!(!is_onion_host(".onion"));
        assert!(!is_onion_host("bad label.onion"));
    }

    #[test]
    fn http_onion_url_maps_to_tor_scheme() {
        let url = Url::parse("http://Example.Onion:8080/docs?a=1#ignored").unwrap();

        assert_eq!(
            tor_url_from_http_url(&url).unwrap().as_deref(),
            Some("tor+http://example.onion:8080/docs?a=1")
        );
    }

    #[test]
    fn tor_target_extracts_plain_http_request_parts() {
        let target = tor_http_target("tor+http://example.onion/docs?a=1").unwrap();

        assert_eq!(target.final_url, "tor+http://example.onion/docs?a=1");
        assert_eq!(target.network_scheme, TorNetworkScheme::Http);
        assert_eq!(target.host, "example.onion");
        assert_eq!(target.port, 80);
        assert_eq!(target.host_header, "example.onion");
        assert_eq!(target.path_and_query, "/docs?a=1");
    }

    #[test]
    fn tor_target_rejects_non_onion_hosts() {
        assert!(tor_http_target("tor+http://example.com/").is_err());
    }

    #[test]
    fn normalizer_accepts_tor_schemes_and_http_onion_urls() {
        assert_eq!(
            normalize_tor_navigation_url("http://example.onion/").unwrap(),
            "tor+http://example.onion/"
        );
        assert_eq!(
            normalize_tor_navigation_url("tor+https://example.onion/").unwrap(),
            "tor+https://example.onion/"
        );
        assert!(normalize_tor_navigation_url("https://example.com/").is_err());
        assert_eq!(TOR_HTTP_SCHEME, "tor+http");
        assert_eq!(TOR_HTTPS_SCHEME, "tor+https");
    }
}
