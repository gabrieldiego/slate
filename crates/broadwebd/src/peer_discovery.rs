use crate::BroadwebdError;
use serde::{Deserialize, Serialize};
use slate_routing::Multiaddr;
#[cfg(any(test, feature = "test-fixtures"))]
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::net::{IpAddr, SocketAddr, ToSocketAddrs, UdpSocket};
#[cfg(any(test, feature = "test-fixtures"))]
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const DEFAULT_PROFILE_SYNC_PEER_DISCOVERY_MAX_BYTES: usize = 8 * 1024;
pub const PROFILE_SYNC_PEER_DISCOVERY_CAPABILITY: &str = "profile-sync/service-frame-tcp";
pub const PROFILE_SYNC_DISCOVERY_PROTOCOL_LIBP2P_RENDEZVOUS: &str = "libp2p-rendezvous";
pub const PROFILE_SYNC_DISCOVERY_PROTOCOL_LIBP2P_KADEMLIA: &str = "libp2p-kademlia";
pub const PROFILE_SYNC_DISCOVERY_PROTOCOL_IPNS: &str = "ipns";
pub const PROFILE_SYNC_DISCOVERY_PROTOCOL_IROH_RENDEZVOUS: &str = "iroh-rendezvous";
pub const PROFILE_SYNC_DISCOVERY_PROTOCOL_LOCAL_SIMULATION: &str = "local-simulation";
pub const DEFAULT_PROFILE_SYNC_DISCOVERY_NAMESPACE: &str = "slate-profile-sync";

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct ProfileSyncPeerAdvertisement {
    pub network_id: String,
    pub node_id: String,
    pub provider_id: String,
    pub service_addr: String,
    pub capabilities: Vec<String>,
    pub sequence: u64,
}

impl ProfileSyncPeerAdvertisement {
    pub fn new(
        network_id: impl Into<String>,
        node_id: impl Into<String>,
        provider_id: impl Into<String>,
        service_addr: impl Into<String>,
        sequence: u64,
    ) -> Result<Self, BroadwebdError> {
        Self::with_capabilities(
            network_id,
            node_id,
            provider_id,
            service_addr,
            [PROFILE_SYNC_PEER_DISCOVERY_CAPABILITY],
            sequence,
        )
    }

    pub fn with_capabilities(
        network_id: impl Into<String>,
        node_id: impl Into<String>,
        provider_id: impl Into<String>,
        service_addr: impl Into<String>,
        capabilities: impl IntoIterator<Item = impl Into<String>>,
        sequence: u64,
    ) -> Result<Self, BroadwebdError> {
        let advertisement = Self {
            network_id: network_id.into(),
            node_id: node_id.into(),
            provider_id: provider_id.into(),
            service_addr: service_addr.into(),
            capabilities: capabilities.into_iter().map(Into::into).collect::<Vec<_>>(),
            sequence,
        };
        advertisement.validate()?;
        Ok(advertisement)
    }

    pub fn validate(&self) -> Result<(), BroadwebdError> {
        validate_discovery_token("network id", self.network_id.as_str())?;
        validate_discovery_token("node id", self.node_id.as_str())?;
        validate_discovery_token("provider id", self.provider_id.as_str())?;
        validate_service_addr(self.service_addr.as_str())?;
        if self.capabilities.is_empty() {
            return Err(BroadwebdError::Request(
                "profile-sync peer advertisement requires at least one capability".to_string(),
            ));
        }
        for capability in &self.capabilities {
            validate_capability(capability)?;
        }
        Ok(())
    }

    pub fn service_socket_addr(&self) -> Result<SocketAddr, BroadwebdError> {
        parse_one_socket_addr(self.service_addr.as_str(), "service address")
    }

    pub fn service_multiaddr(&self) -> Result<Multiaddr, BroadwebdError> {
        Multiaddr::parse(self.service_addr.as_str()).map_err(|error| {
            BroadwebdError::Request(format!(
                "invalid profile-sync peer discovery service multiaddr: {error}"
            ))
        })
    }

    pub fn has_multiaddr_service_endpoint(&self) -> bool {
        self.service_multiaddr().is_ok()
    }

    pub fn connect_addr_for_source(
        &self,
        source_addr: SocketAddr,
    ) -> Result<String, BroadwebdError> {
        let service_addr = self.service_socket_addr()?;
        let ip = match service_addr.ip() {
            IpAddr::V4(ip) if ip.is_unspecified() => source_addr.ip(),
            IpAddr::V6(ip) if ip.is_unspecified() => source_addr.ip(),
            ip => ip,
        };
        Ok(SocketAddr::new(ip, service_addr.port()).to_string())
    }

    pub fn supports_profile_sync_service_frames(&self) -> bool {
        self.capabilities
            .iter()
            .any(|capability| capability == PROFILE_SYNC_PEER_DISCOVERY_CAPABILITY)
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ProfileSyncPeerDiscoveryProtocol {
    Libp2pRendezvous,
    Libp2pKademlia,
    Ipns,
    IrohRendezvous,
    LocalSimulation,
}

impl ProfileSyncPeerDiscoveryProtocol {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Libp2pRendezvous => PROFILE_SYNC_DISCOVERY_PROTOCOL_LIBP2P_RENDEZVOUS,
            Self::Libp2pKademlia => PROFILE_SYNC_DISCOVERY_PROTOCOL_LIBP2P_KADEMLIA,
            Self::Ipns => PROFILE_SYNC_DISCOVERY_PROTOCOL_IPNS,
            Self::IrohRendezvous => PROFILE_SYNC_DISCOVERY_PROTOCOL_IROH_RENDEZVOUS,
            Self::LocalSimulation => PROFILE_SYNC_DISCOVERY_PROTOCOL_LOCAL_SIMULATION,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileSyncPeerDiscoveryQuery {
    pub network_id: String,
    pub requester_node_id: String,
    pub namespace: String,
    pub protocols: Vec<ProfileSyncPeerDiscoveryProtocol>,
    pub max_peers: usize,
}

impl ProfileSyncPeerDiscoveryQuery {
    pub fn new(
        network_id: impl Into<String>,
        requester_node_id: impl Into<String>,
        namespace: impl Into<String>,
        protocols: impl IntoIterator<Item = ProfileSyncPeerDiscoveryProtocol>,
        max_peers: usize,
    ) -> Result<Self, BroadwebdError> {
        let query = Self {
            network_id: network_id.into(),
            requester_node_id: requester_node_id.into(),
            namespace: namespace.into(),
            protocols: protocols.into_iter().collect(),
            max_peers,
        };
        query.validate()?;
        Ok(query)
    }

    pub fn for_default_namespace(
        network_id: impl Into<String>,
        requester_node_id: impl Into<String>,
        protocols: impl IntoIterator<Item = ProfileSyncPeerDiscoveryProtocol>,
        max_peers: usize,
    ) -> Result<Self, BroadwebdError> {
        Self::new(
            network_id,
            requester_node_id,
            DEFAULT_PROFILE_SYNC_DISCOVERY_NAMESPACE,
            protocols,
            max_peers,
        )
    }

    pub fn validate(&self) -> Result<(), BroadwebdError> {
        validate_discovery_token("network id", self.network_id.as_str())?;
        validate_discovery_token("requester node id", self.requester_node_id.as_str())?;
        validate_namespace(self.namespace.as_str())?;
        if self.protocols.is_empty() {
            return Err(BroadwebdError::Request(
                "profile-sync peer discovery query requires at least one protocol".to_string(),
            ));
        }
        if self.max_peers == 0 {
            return Err(BroadwebdError::Request(
                "profile-sync peer discovery query max_peers must be greater than zero".to_string(),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileSyncPeerDiscoveryPublication {
    pub protocol: ProfileSyncPeerDiscoveryProtocol,
    pub namespace: String,
    pub advertisement: ProfileSyncPeerAdvertisement,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileSyncPeerDiscoveryResult {
    pub protocol: ProfileSyncPeerDiscoveryProtocol,
    pub namespace: String,
    pub advertisement: ProfileSyncPeerAdvertisement,
}

pub trait ProfileSyncPeerDiscoveryProvider: Send + Sync {
    fn publish_profile_sync_peer(
        &self,
        protocol: ProfileSyncPeerDiscoveryProtocol,
        namespace: &str,
        advertisement: ProfileSyncPeerAdvertisement,
    ) -> Result<ProfileSyncPeerDiscoveryPublication, BroadwebdError>;

    fn discover_profile_sync_peers(
        &self,
        query: &ProfileSyncPeerDiscoveryQuery,
    ) -> Result<Vec<ProfileSyncPeerDiscoveryResult>, BroadwebdError>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiscoveredProfileSyncPeer {
    pub advertisement: ProfileSyncPeerAdvertisement,
    pub source_addr: SocketAddr,
}

impl DiscoveredProfileSyncPeer {
    pub fn connect_addr(&self) -> Result<String, BroadwebdError> {
        self.advertisement.connect_addr_for_source(self.source_addr)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProfileSyncPeerDiscoveryMessage {
    Solicit {
        network_id: String,
        requester_node_id: String,
    },
    Advertisement {
        advertisement: ProfileSyncPeerAdvertisement,
    },
}

impl ProfileSyncPeerDiscoveryMessage {
    pub fn solicit(
        network_id: impl Into<String>,
        requester_node_id: impl Into<String>,
    ) -> Result<Self, BroadwebdError> {
        let network_id = network_id.into();
        let requester_node_id = requester_node_id.into();
        validate_discovery_token("network id", network_id.as_str())?;
        validate_discovery_token("requester node id", requester_node_id.as_str())?;
        Ok(Self::Solicit {
            network_id,
            requester_node_id,
        })
    }

    pub fn advertisement(
        advertisement: ProfileSyncPeerAdvertisement,
    ) -> Result<Self, BroadwebdError> {
        advertisement.validate()?;
        Ok(Self::Advertisement { advertisement })
    }
}

pub fn encode_profile_sync_peer_discovery_message(
    message: &ProfileSyncPeerDiscoveryMessage,
) -> Result<Vec<u8>, BroadwebdError> {
    let bytes = serde_json::to_vec(message).map_err(|error| {
        BroadwebdError::Request(format!(
            "encode profile-sync peer discovery message: {error}"
        ))
    })?;
    if bytes.len() > DEFAULT_PROFILE_SYNC_PEER_DISCOVERY_MAX_BYTES {
        return Err(BroadwebdError::FrameTooLarge {
            limit: DEFAULT_PROFILE_SYNC_PEER_DISCOVERY_MAX_BYTES,
            actual: bytes.len(),
        });
    }
    Ok(bytes)
}

pub fn decode_profile_sync_peer_discovery_message(
    bytes: &[u8],
) -> Result<ProfileSyncPeerDiscoveryMessage, BroadwebdError> {
    if bytes.len() > DEFAULT_PROFILE_SYNC_PEER_DISCOVERY_MAX_BYTES {
        return Err(BroadwebdError::FrameTooLarge {
            limit: DEFAULT_PROFILE_SYNC_PEER_DISCOVERY_MAX_BYTES,
            actual: bytes.len(),
        });
    }
    let message =
        serde_json::from_slice::<ProfileSyncPeerDiscoveryMessage>(bytes).map_err(|error| {
            BroadwebdError::Request(format!(
                "decode profile-sync peer discovery message: {error}"
            ))
        })?;
    validate_profile_sync_peer_discovery_message(&message)?;
    Ok(message)
}

pub fn send_profile_sync_peer_discovery_message(
    socket: &UdpSocket,
    target: SocketAddr,
    message: &ProfileSyncPeerDiscoveryMessage,
) -> Result<usize, BroadwebdError> {
    let bytes = encode_profile_sync_peer_discovery_message(message)?;
    Ok(socket.send_to(bytes.as_slice(), target)?)
}

pub fn recv_profile_sync_peer_discovery_message(
    socket: &UdpSocket,
) -> Result<(ProfileSyncPeerDiscoveryMessage, SocketAddr), BroadwebdError> {
    let mut bytes = vec![0_u8; DEFAULT_PROFILE_SYNC_PEER_DISCOVERY_MAX_BYTES];
    let (len, source_addr) = socket.recv_from(bytes.as_mut_slice())?;
    let message = decode_profile_sync_peer_discovery_message(&bytes[..len])?;
    Ok((message, source_addr))
}

pub fn respond_to_profile_sync_peer_solicit(
    socket: &UdpSocket,
    advertisement: &ProfileSyncPeerAdvertisement,
) -> Result<Option<SocketAddr>, BroadwebdError> {
    let (message, source_addr) = recv_profile_sync_peer_discovery_message(socket)?;
    let ProfileSyncPeerDiscoveryMessage::Solicit { network_id, .. } = message else {
        return Ok(None);
    };
    if network_id != advertisement.network_id {
        return Ok(None);
    }
    let response = ProfileSyncPeerDiscoveryMessage::advertisement(advertisement.clone())?;
    send_profile_sync_peer_discovery_message(socket, source_addr, &response)?;
    Ok(Some(source_addr))
}

pub fn discover_profile_sync_peers(
    discovery_target: impl ToSocketAddrs,
    network_id: &str,
    requester_node_id: &str,
    timeout: Duration,
    max_peers: usize,
) -> Result<Vec<DiscoveredProfileSyncPeer>, BroadwebdError> {
    validate_discovery_token("network id", network_id)?;
    validate_discovery_token("requester node id", requester_node_id)?;
    let discovery_target = first_socket_addr(discovery_target, "discovery target")?;
    let bind_addr = if discovery_target.is_ipv4() {
        "0.0.0.0:0"
    } else {
        "[::]:0"
    };
    let socket = UdpSocket::bind(bind_addr)?;
    socket.set_read_timeout(Some(timeout))?;

    let solicit = ProfileSyncPeerDiscoveryMessage::solicit(network_id, requester_node_id)?;
    send_profile_sync_peer_discovery_message(&socket, discovery_target, &solicit)?;
    collect_profile_sync_peer_advertisements(&socket, network_id, timeout, max_peers)
}

pub fn collect_profile_sync_peer_advertisements(
    socket: &UdpSocket,
    network_id: &str,
    timeout: Duration,
    max_peers: usize,
) -> Result<Vec<DiscoveredProfileSyncPeer>, BroadwebdError> {
    validate_discovery_token("network id", network_id)?;
    let deadline = Instant::now() + timeout;
    let mut peers = Vec::new();
    let mut seen = BTreeSet::new();

    while peers.len() < max_peers {
        let now = Instant::now();
        if now >= deadline {
            break;
        }
        socket.set_read_timeout(Some(deadline.saturating_duration_since(now)))?;
        match recv_profile_sync_peer_discovery_message(socket) {
            Ok((ProfileSyncPeerDiscoveryMessage::Advertisement { advertisement }, source_addr))
                if advertisement.network_id == network_id
                    && advertisement.supports_profile_sync_service_frames() =>
            {
                let key = (
                    advertisement.node_id.clone(),
                    advertisement.provider_id.clone(),
                    source_addr,
                );
                if seen.insert(key) {
                    peers.push(DiscoveredProfileSyncPeer {
                        advertisement,
                        source_addr,
                    });
                }
            }
            Ok(_) => {}
            Err(BroadwebdError::Io(error)) if is_udp_timeout(&error) => break,
            Err(error) => return Err(error),
        }
    }

    Ok(peers)
}

fn validate_profile_sync_peer_discovery_message(
    message: &ProfileSyncPeerDiscoveryMessage,
) -> Result<(), BroadwebdError> {
    match message {
        ProfileSyncPeerDiscoveryMessage::Solicit {
            network_id,
            requester_node_id,
        } => {
            validate_discovery_token("network id", network_id)?;
            validate_discovery_token("requester node id", requester_node_id)
        }
        ProfileSyncPeerDiscoveryMessage::Advertisement { advertisement } => {
            advertisement.validate()
        }
    }
}

fn validate_discovery_token(label: &str, token: &str) -> Result<(), BroadwebdError> {
    if token.is_empty()
        || token.len() > 128
        || !token
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(BroadwebdError::Request(format!(
            "invalid profile-sync peer discovery {label}: {token:?}"
        )));
    }
    Ok(())
}

fn validate_capability(capability: &str) -> Result<(), BroadwebdError> {
    if capability.is_empty()
        || capability.len() > 128
        || !capability
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'/'))
    {
        return Err(BroadwebdError::Request(format!(
            "invalid profile-sync peer discovery capability: {capability:?}"
        )));
    }
    Ok(())
}

fn validate_namespace(namespace: &str) -> Result<(), BroadwebdError> {
    if namespace.is_empty()
        || namespace.len() > 256
        || !namespace
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'/'))
    {
        return Err(BroadwebdError::Request(format!(
            "invalid profile-sync peer discovery namespace: {namespace:?}"
        )));
    }
    Ok(())
}

fn validate_service_addr(service_addr: &str) -> Result<(), BroadwebdError> {
    if parse_one_socket_addr(service_addr, "service address").is_ok()
        || Multiaddr::parse(service_addr).is_ok()
    {
        return Ok(());
    }
    Err(BroadwebdError::Request(format!(
        "profile-sync peer discovery service endpoint must be a socket address or multiaddr: {service_addr:?}"
    )))
}

fn parse_one_socket_addr(input: &str, label: &str) -> Result<SocketAddr, BroadwebdError> {
    input
        .parse::<SocketAddr>()
        .map_err(|error| BroadwebdError::Request(format!("parse {label}: {error}")))
}

fn first_socket_addr(input: impl ToSocketAddrs, label: &str) -> Result<SocketAddr, BroadwebdError> {
    input
        .to_socket_addrs()
        .map_err(|error| BroadwebdError::Request(format!("resolve {label}: {error}")))?
        .next()
        .ok_or_else(|| BroadwebdError::Request(format!("{label} resolved no socket addresses")))
}

fn is_udp_timeout(error: &str) -> bool {
    error.contains("timed out")
        || error.contains("would block")
        || error.contains("Resource temporarily unavailable")
}

#[cfg(any(test, feature = "test-fixtures"))]
#[derive(Clone, Debug, Default)]
pub struct SimulatedProfileSyncPeerDiscoveryNetwork {
    records: Arc<Mutex<BTreeMap<SimulatedDiscoveryKey, ProfileSyncPeerAdvertisement>>>,
}

#[cfg(any(test, feature = "test-fixtures"))]
impl SimulatedProfileSyncPeerDiscoveryNetwork {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn provider(&self) -> SimulatedProfileSyncPeerDiscoveryProvider {
        SimulatedProfileSyncPeerDiscoveryProvider {
            network: self.clone(),
        }
    }
}

#[cfg(any(test, feature = "test-fixtures"))]
#[derive(Clone, Debug)]
pub struct SimulatedProfileSyncPeerDiscoveryProvider {
    network: SimulatedProfileSyncPeerDiscoveryNetwork,
}

#[cfg(any(test, feature = "test-fixtures"))]
impl ProfileSyncPeerDiscoveryProvider for SimulatedProfileSyncPeerDiscoveryProvider {
    fn publish_profile_sync_peer(
        &self,
        protocol: ProfileSyncPeerDiscoveryProtocol,
        namespace: &str,
        advertisement: ProfileSyncPeerAdvertisement,
    ) -> Result<ProfileSyncPeerDiscoveryPublication, BroadwebdError> {
        validate_namespace(namespace)?;
        advertisement.validate()?;
        let key = SimulatedDiscoveryKey::new(
            protocol,
            advertisement.network_id.as_str(),
            namespace,
            advertisement.node_id.as_str(),
            advertisement.provider_id.as_str(),
        )?;
        self.network
            .records
            .lock()
            .map_err(|_| {
                BroadwebdError::Request("peer discovery simulation lock poisoned".to_string())
            })?
            .insert(key, advertisement.clone());
        Ok(ProfileSyncPeerDiscoveryPublication {
            protocol,
            namespace: namespace.to_string(),
            advertisement,
        })
    }

    fn discover_profile_sync_peers(
        &self,
        query: &ProfileSyncPeerDiscoveryQuery,
    ) -> Result<Vec<ProfileSyncPeerDiscoveryResult>, BroadwebdError> {
        query.validate()?;
        let records = self.network.records.lock().map_err(|_| {
            BroadwebdError::Request("peer discovery simulation lock poisoned".to_string())
        })?;
        let mut results = Vec::new();
        let mut seen = BTreeSet::new();
        for protocol in &query.protocols {
            for (key, advertisement) in records.iter() {
                if results.len() >= query.max_peers {
                    return Ok(results);
                }
                if key.protocol != *protocol
                    || key.network_id != query.network_id
                    || key.namespace != query.namespace
                    || advertisement.node_id == query.requester_node_id
                    || !advertisement.supports_profile_sync_service_frames()
                {
                    continue;
                }
                let seen_key = (
                    advertisement.node_id.clone(),
                    advertisement.provider_id.clone(),
                );
                if seen.insert(seen_key) {
                    results.push(ProfileSyncPeerDiscoveryResult {
                        protocol: *protocol,
                        namespace: key.namespace.clone(),
                        advertisement: advertisement.clone(),
                    });
                }
            }
        }
        Ok(results)
    }
}

#[cfg(any(test, feature = "test-fixtures"))]
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct SimulatedDiscoveryKey {
    protocol: ProfileSyncPeerDiscoveryProtocol,
    network_id: String,
    namespace: String,
    node_id: String,
    provider_id: String,
}

#[cfg(any(test, feature = "test-fixtures"))]
impl SimulatedDiscoveryKey {
    fn new(
        protocol: ProfileSyncPeerDiscoveryProtocol,
        network_id: &str,
        namespace: &str,
        node_id: &str,
        provider_id: &str,
    ) -> Result<Self, BroadwebdError> {
        validate_discovery_token("network id", network_id)?;
        validate_namespace(namespace)?;
        validate_discovery_token("node id", node_id)?;
        validate_discovery_token("provider id", provider_id)?;
        Ok(Self {
            protocol,
            network_id: network_id.to_string(),
            namespace: namespace.to_string(),
            node_id: node_id.to_string(),
            provider_id: provider_id.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peer_discovery_message_round_trips_profile_sync_advertisement() {
        let advertisement =
            ProfileSyncPeerAdvertisement::new("local", "node-a", "provider-a", "0.0.0.0:9000", 7)
                .expect("valid advertisement");
        let message = ProfileSyncPeerDiscoveryMessage::advertisement(advertisement.clone())
            .expect("valid discovery message");
        let bytes =
            encode_profile_sync_peer_discovery_message(&message).expect("encode discovery message");

        assert_eq!(
            decode_profile_sync_peer_discovery_message(bytes.as_slice())
                .expect("decode discovery message"),
            message
        );
        assert!(advertisement.supports_profile_sync_service_frames());
    }

    #[test]
    fn peer_discovery_rejects_oversized_datagrams_before_json_parse() {
        let bytes = vec![b'{'; DEFAULT_PROFILE_SYNC_PEER_DISCOVERY_MAX_BYTES + 1];
        let error = decode_profile_sync_peer_discovery_message(bytes.as_slice())
            .expect_err("oversized discovery datagram should fail");
        assert_eq!(
            error,
            BroadwebdError::FrameTooLarge {
                limit: DEFAULT_PROFILE_SYNC_PEER_DISCOVERY_MAX_BYTES,
                actual: DEFAULT_PROFILE_SYNC_PEER_DISCOVERY_MAX_BYTES + 1,
            }
        );
    }

    #[test]
    fn peer_discovery_connect_addr_uses_source_ip_for_unspecified_service_host() {
        let advertisement =
            ProfileSyncPeerAdvertisement::new("local", "node-a", "provider-a", "0.0.0.0:9000", 1)
                .expect("valid advertisement");
        let source_addr = "192.0.2.55:41000".parse().expect("source addr");

        assert_eq!(
            advertisement
                .connect_addr_for_source(source_addr)
                .expect("connect address"),
            "192.0.2.55:9000"
        );
    }

    #[test]
    fn peer_discovery_rejects_invalid_tokens() {
        let error = ProfileSyncPeerAdvertisement::new(
            "bad network",
            "node-a",
            "provider-a",
            "127.0.0.1:9000",
            1,
        )
        .expect_err("space in network id should fail");
        assert!(error.to_string().contains("invalid profile-sync"));
    }

    #[test]
    fn peer_discovery_accepts_multiaddr_service_endpoints_for_p2p_adapters() {
        let advertisement = ProfileSyncPeerAdvertisement::new(
            "broadweb",
            "node-a",
            "provider-a",
            "/dnsaddr/rendezvous.local/tcp/443/wss/p2p/peer-a",
            1,
        )
        .expect("multiaddr service endpoint");

        assert!(advertisement.has_multiaddr_service_endpoint());
        assert_eq!(
            advertisement
                .service_multiaddr()
                .expect("service multiaddr")
                .as_str(),
            "/dnsaddr/rendezvous.local/tcp/443/wss/p2p/peer-a"
        );
        assert!(
            advertisement
                .service_socket_addr()
                .expect_err("multiaddr endpoint should not parse as socket")
                .to_string()
                .contains("parse service address")
        );
    }

    #[test]
    fn simulated_peer_discovery_provider_finds_protocol_records_without_sockets() {
        let network = SimulatedProfileSyncPeerDiscoveryNetwork::new();
        let provider = network.provider();
        provider
            .publish_profile_sync_peer(
                ProfileSyncPeerDiscoveryProtocol::Libp2pRendezvous,
                DEFAULT_PROFILE_SYNC_DISCOVERY_NAMESPACE,
                ProfileSyncPeerAdvertisement::new(
                    "account-a",
                    "node-a",
                    "provider-a",
                    "/dnsaddr/rendezvous.local/tcp/443/wss/p2p/peer-a",
                    1,
                )
                .expect("libp2p-shaped advertisement"),
            )
            .expect("publish libp2p-shaped advertisement");
        provider
            .publish_profile_sync_peer(
                ProfileSyncPeerDiscoveryProtocol::Ipns,
                DEFAULT_PROFILE_SYNC_DISCOVERY_NAMESPACE,
                ProfileSyncPeerAdvertisement::new(
                    "account-a",
                    "node-b",
                    "provider-b",
                    "/ipns/k51qzi5uqu5dh-example-profile-sync-root",
                    1,
                )
                .expect("ipns-shaped advertisement"),
            )
            .expect("publish ipns-shaped advertisement");

        let query = ProfileSyncPeerDiscoveryQuery::for_default_namespace(
            "account-a",
            "node-local",
            [
                ProfileSyncPeerDiscoveryProtocol::Libp2pRendezvous,
                ProfileSyncPeerDiscoveryProtocol::Ipns,
            ],
            8,
        )
        .expect("discovery query");
        let discovered = provider
            .discover_profile_sync_peers(&query)
            .expect("discover simulated peers");

        assert_eq!(discovered.len(), 2);
        assert_eq!(
            discovered
                .iter()
                .map(|peer| peer.protocol)
                .collect::<Vec<_>>(),
            vec![
                ProfileSyncPeerDiscoveryProtocol::Libp2pRendezvous,
                ProfileSyncPeerDiscoveryProtocol::Ipns,
            ]
        );
        assert_eq!(discovered[0].advertisement.node_id, "node-a");
        assert_eq!(discovered[1].advertisement.node_id, "node-b");
    }

    #[test]
    fn simulated_peer_discovery_filters_network_namespace_protocol_and_self() {
        let network = SimulatedProfileSyncPeerDiscoveryNetwork::new();
        let provider = network.provider();
        for (protocol, network_id, namespace, node_id, provider_id) in [
            (
                ProfileSyncPeerDiscoveryProtocol::Libp2pKademlia,
                "account-a",
                DEFAULT_PROFILE_SYNC_DISCOVERY_NAMESPACE,
                "node-a",
                "provider-a",
            ),
            (
                ProfileSyncPeerDiscoveryProtocol::IrohRendezvous,
                "account-a",
                DEFAULT_PROFILE_SYNC_DISCOVERY_NAMESPACE,
                "node-b",
                "provider-b",
            ),
            (
                ProfileSyncPeerDiscoveryProtocol::Libp2pKademlia,
                "account-b",
                DEFAULT_PROFILE_SYNC_DISCOVERY_NAMESPACE,
                "node-c",
                "provider-c",
            ),
            (
                ProfileSyncPeerDiscoveryProtocol::Libp2pKademlia,
                "account-a",
                "other-namespace",
                "node-d",
                "provider-d",
            ),
            (
                ProfileSyncPeerDiscoveryProtocol::Libp2pKademlia,
                "account-a",
                DEFAULT_PROFILE_SYNC_DISCOVERY_NAMESPACE,
                "node-local",
                "provider-local",
            ),
        ] {
            provider
                .publish_profile_sync_peer(
                    protocol,
                    namespace,
                    ProfileSyncPeerAdvertisement::new(
                        network_id,
                        node_id,
                        provider_id,
                        format!("/p2p/{node_id}/x/slate-profile-sync"),
                        1,
                    )
                    .expect("simulated p2p advertisement"),
                )
                .expect("publish simulated record");
        }

        let query = ProfileSyncPeerDiscoveryQuery::for_default_namespace(
            "account-a",
            "node-local",
            [ProfileSyncPeerDiscoveryProtocol::Libp2pKademlia],
            8,
        )
        .expect("discovery query");
        let discovered = provider
            .discover_profile_sync_peers(&query)
            .expect("discover filtered peers");

        assert_eq!(discovered.len(), 1);
        assert_eq!(
            discovered[0].protocol,
            ProfileSyncPeerDiscoveryProtocol::Libp2pKademlia
        );
        assert_eq!(discovered[0].advertisement.node_id, "node-a");
        assert_eq!(discovered[0].advertisement.provider_id, "provider-a");
    }
}
