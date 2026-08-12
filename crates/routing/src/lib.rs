#![forbid(unsafe_code)]

use core::fmt;
use core::str::FromStr;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Multiaddr(String);

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MultiaddrError {
    Empty,
    MissingLeadingSlash,
    EmptySegment,
}

impl Multiaddr {
    pub fn parse(input: &str) -> Result<Self, MultiaddrError> {
        input.parse()
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn segments(&self) -> impl Iterator<Item = &str> {
        self.0.split('/').filter(|segment| !segment.is_empty())
    }
}

impl FromStr for Multiaddr {
    type Err = MultiaddrError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        if input.is_empty() {
            return Err(MultiaddrError::Empty);
        }

        if !input.starts_with('/') {
            return Err(MultiaddrError::MissingLeadingSlash);
        }

        if input.split('/').skip(1).any(str::is_empty) {
            return Err(MultiaddrError::EmptySegment);
        }

        Ok(Self(input.to_string()))
    }
}

impl fmt::Display for Multiaddr {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl fmt::Display for MultiaddrError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("multiaddr is empty"),
            Self::MissingLeadingSlash => formatter.write_str("multiaddr must start with '/'"),
            Self::EmptySegment => formatter.write_str("multiaddr contains an empty segment"),
        }
    }
}

impl std::error::Error for MultiaddrError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RoutingMode {
    Direct,
    Gateway,
    Proxy,
    Internal,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoutingPlan {
    pub source: String,
    pub endpoint: Multiaddr,
    pub mode: RoutingMode,
    pub privacy_boundary: String,
}

impl RoutingPlan {
    pub fn new(
        source: impl Into<String>,
        endpoint: Multiaddr,
        mode: RoutingMode,
        privacy_boundary: impl Into<String>,
    ) -> Self {
        Self {
            source: source.into(),
            endpoint,
            mode,
            privacy_boundary: privacy_boundary.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Multiaddr, MultiaddrError};

    #[test]
    fn parses_valid_multiaddr() {
        let route = Multiaddr::parse("/ip4/127.0.0.1/tcp/8080/http").expect("valid route");
        let segments: Vec<&str> = route.segments().collect();
        assert_eq!(segments, ["ip4", "127.0.0.1", "tcp", "8080", "http"]);
    }

    #[test]
    fn rejects_malformed_multiaddr() {
        assert_eq!(
            Multiaddr::parse("ip4/127.0.0.1").unwrap_err(),
            MultiaddrError::MissingLeadingSlash
        );
    }
}
