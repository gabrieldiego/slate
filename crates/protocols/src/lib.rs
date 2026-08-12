#![forbid(unsafe_code)]

use core::fmt;
use slate_routing::{Multiaddr, MultiaddrError, RoutingMode, RoutingPlan};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProtocolError {
    UnsupportedScheme(String),
    InvalidRoute(MultiaddrError),
}

impl From<MultiaddrError> for ProtocolError {
    fn from(error: MultiaddrError) -> Self {
        Self::InvalidRoute(error)
    }
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedScheme(scheme) => write!(formatter, "unsupported scheme: {scheme}"),
            Self::InvalidRoute(error) => write!(formatter, "invalid route: {error}"),
        }
    }
}

impl std::error::Error for ProtocolError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Scheme {
    Http,
    Https,
    Ipfs,
    Ipns,
    Onion,
    I2p,
    Slate,
    Unknown,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ProtocolRegistry;

impl ProtocolRegistry {
    pub fn classify(&self, target: &str) -> Scheme {
        let lower = target.to_ascii_lowercase();

        if lower.starts_with("slate://") {
            Scheme::Slate
        } else if lower.starts_with("ipfs://") {
            Scheme::Ipfs
        } else if lower.starts_with("ipns://") {
            Scheme::Ipns
        } else if lower.contains(".onion") {
            Scheme::Onion
        } else if lower.contains(".i2p") || lower.starts_with("i2p://") {
            Scheme::I2p
        } else if lower.starts_with("https://") {
            Scheme::Https
        } else if lower.starts_with("http://") {
            Scheme::Http
        } else {
            Scheme::Unknown
        }
    }

    pub fn plan_for(&self, target: &str) -> Result<RoutingPlan, ProtocolError> {
        match self.classify(target) {
            Scheme::Slate => Ok(RoutingPlan::new(
                target,
                Multiaddr::parse("/memory/slate/chrome")?,
                RoutingMode::Internal,
                "local-only browser state",
            )),
            Scheme::Ipfs | Scheme::Ipns => Ok(RoutingPlan::new(
                target,
                Multiaddr::parse("/ip4/127.0.0.1/tcp/8080/http")?,
                RoutingMode::Gateway,
                "local IPFS gateway; no public fallback",
            )),
            Scheme::Onion => Ok(RoutingPlan::new(
                target,
                Multiaddr::parse("/ip4/127.0.0.1/tcp/9050/socks5")?,
                RoutingMode::Proxy,
                "Tor SOCKS proxy; DNS must not escape",
            )),
            Scheme::I2p => Ok(RoutingPlan::new(
                target,
                Multiaddr::parse("/ip4/127.0.0.1/tcp/4444/http")?,
                RoutingMode::Proxy,
                "I2P local HTTP proxy",
            )),
            Scheme::Https => Ok(RoutingPlan::new(
                target,
                Multiaddr::parse("/dnsaddr/web/tcp/443/tls/http")?,
                RoutingMode::Direct,
                "ordinary web route",
            )),
            Scheme::Http => Ok(RoutingPlan::new(
                target,
                Multiaddr::parse("/dnsaddr/web/tcp/80/http")?,
                RoutingMode::Direct,
                "ordinary web route without transport encryption",
            )),
            Scheme::Unknown => Err(ProtocolError::UnsupportedScheme(target.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ProtocolRegistry, Scheme};
    use slate_routing::RoutingMode;

    #[test]
    fn classifies_broadweb_targets() {
        let registry = ProtocolRegistry;
        assert_eq!(registry.classify("ipfs://bafy..."), Scheme::Ipfs);
        assert_eq!(registry.classify("http://example.onion"), Scheme::Onion);
    }

    #[test]
    fn routes_ipfs_to_local_gateway() {
        let plan = ProtocolRegistry
            .plan_for("ipfs://bafy")
            .expect("ipfs route");
        assert_eq!(plan.mode, RoutingMode::Gateway);
        assert_eq!(plan.endpoint.as_str(), "/ip4/127.0.0.1/tcp/8080/http");
    }
}
