use crate::{
    daemon::BroadwebdClient,
    error::BroadwebdError,
    health::{DaemonHealth, DaemonLifecycle},
    http::{ServiceRequest, ServiceResponse},
    state::TemporaryDownloadRecord,
    status::BroadwebStatusSnapshot,
};
use serde::{Serialize, de::DeserializeOwned};
use slate_routing::Multiaddr;
use std::collections::BTreeMap;
use std::io::{self, Cursor, Read, Write};
use std::net::{IpAddr, SocketAddr, TcpStream};
use std::time::Duration;

pub const DEFAULT_SERVICE_FRAME_MAX_BYTES: usize = 32 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ServiceFrameCodec {
    max_frame_bytes: usize,
}

impl ServiceFrameCodec {
    pub fn new(max_frame_bytes: usize) -> Self {
        Self { max_frame_bytes }
    }

    pub fn max_frame_bytes(&self) -> usize {
        self.max_frame_bytes
    }

    pub fn encode_request(&self, request: &ServiceRequest) -> Result<Vec<u8>, BroadwebdError> {
        encode_json_frame(request, self.max_frame_bytes, "request")
    }

    pub fn decode_request(&self, frame: &[u8]) -> Result<ServiceRequest, BroadwebdError> {
        decode_json_frame(frame, self.max_frame_bytes, "request")
    }

    pub fn encode_response(&self, response: &ServiceResponse) -> Result<Vec<u8>, BroadwebdError> {
        encode_json_frame(response, self.max_frame_bytes, "response")
    }

    pub fn decode_response(&self, frame: &[u8]) -> Result<ServiceResponse, BroadwebdError> {
        decode_json_frame(frame, self.max_frame_bytes, "response")
    }

    pub fn write_request(
        &self,
        writer: &mut impl Write,
        request: &ServiceRequest,
    ) -> Result<(), BroadwebdError> {
        let frame = self.encode_request(request)?;
        write_len_prefixed_frame(writer, frame.as_slice())
    }

    pub fn read_request(&self, reader: &mut impl Read) -> Result<ServiceRequest, BroadwebdError> {
        let frame = read_len_prefixed_frame(reader, self.max_frame_bytes)?;
        self.decode_request(frame.as_slice())
    }

    pub fn write_response(
        &self,
        writer: &mut impl Write,
        response: &ServiceResponse,
    ) -> Result<(), BroadwebdError> {
        let frame = self.encode_response(response)?;
        write_len_prefixed_frame(writer, frame.as_slice())
    }

    pub fn read_response(&self, reader: &mut impl Read) -> Result<ServiceResponse, BroadwebdError> {
        let frame = read_len_prefixed_frame(reader, self.max_frame_bytes)?;
        self.decode_response(frame.as_slice())
    }
}

impl Default for ServiceFrameCodec {
    fn default() -> Self {
        Self::new(DEFAULT_SERVICE_FRAME_MAX_BYTES)
    }
}

pub fn dispatch_service_frame_request_over_stream<S: Read + Write>(
    codec: ServiceFrameCodec,
    stream: &mut S,
    request: &ServiceRequest,
) -> Result<ServiceResponse, BroadwebdError> {
    codec.write_request(stream, request)?;
    codec.read_response(stream)
}

pub trait ServiceFrameConnector {
    type Stream: Read + Write;

    fn connect(&self, codec: ServiceFrameCodec) -> Result<Self::Stream, BroadwebdError>;
}

pub trait ServiceFrameIoStream: Read + Write {}

impl<T: Read + Write> ServiceFrameIoStream for T {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServiceFrameConnectorKind {
    Tcp,
    Libp2p,
    Iroh,
    Ipns,
    Dnsaddr,
}

impl ServiceFrameConnectorKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Tcp => "tcp",
            Self::Libp2p => "libp2p",
            Self::Iroh => "iroh",
            Self::Ipns => "ipns",
            Self::Dnsaddr => "dnsaddr",
        }
    }

    pub fn is_deferred(self) -> bool {
        self != Self::Tcp
    }
}

pub fn dispatch_service_frame_request_with_connector<C: ServiceFrameConnector>(
    codec: ServiceFrameCodec,
    connector: &C,
    request: &ServiceRequest,
) -> Result<ServiceResponse, BroadwebdError> {
    let mut stream = connector.connect(codec)?;
    dispatch_service_frame_request_over_stream(codec, &mut stream, request)
}

pub fn service_frame_connector_kind_for_endpoint(
    endpoint: &str,
) -> Result<ServiceFrameConnectorKind, BroadwebdError> {
    let endpoint = endpoint.trim();
    if endpoint.is_empty() {
        return Err(BroadwebdError::Request(
            "service-frame endpoint is empty".to_string(),
        ));
    }
    if endpoint.parse::<SocketAddr>().is_ok() {
        return Ok(ServiceFrameConnectorKind::Tcp);
    }
    if let Some(target) = endpoint.strip_prefix("iroh-node:") {
        validate_deferred_service_frame_endpoint_target("iroh-node", target)?;
        return Ok(ServiceFrameConnectorKind::Iroh);
    }
    if let Some(target) = endpoint.strip_prefix("ipns:") {
        validate_deferred_service_frame_endpoint_target("ipns", target)?;
        return Ok(ServiceFrameConnectorKind::Ipns);
    }

    let multiaddr = Multiaddr::parse(endpoint).map_err(|error| {
        BroadwebdError::Request(format!(
            "service-frame endpoint must be a literal socket address, deferred protocol endpoint, or multiaddr: {error}"
        ))
    })?;
    service_frame_connector_kind_for_multiaddr(&multiaddr)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeferredServiceFrameConnector {
    endpoint: String,
    kind: ServiceFrameConnectorKind,
}

impl DeferredServiceFrameConnector {
    pub fn new(endpoint: impl Into<String>) -> Result<Self, BroadwebdError> {
        let endpoint = endpoint.into();
        let kind = service_frame_connector_kind_for_endpoint(endpoint.as_str())?;
        if !kind.is_deferred() {
            return Err(BroadwebdError::UnsupportedRequest(format!(
                "deferred service-frame connector cannot be created for TCP endpoint {endpoint}"
            )));
        }
        Ok(Self { endpoint, kind })
    }

    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    pub fn kind(&self) -> ServiceFrameConnectorKind {
        self.kind
    }
}

impl ServiceFrameConnector for DeferredServiceFrameConnector {
    type Stream = Cursor<Vec<u8>>;

    fn connect(&self, _codec: ServiceFrameCodec) -> Result<Self::Stream, BroadwebdError> {
        Err(BroadwebdError::UnsupportedRequest(format!(
            "service-frame {} connector is not implemented yet for endpoint {}; enable a protocol-specific connector before using this endpoint",
            self.kind.as_str(),
            self.endpoint
        )))
    }
}

pub trait ServiceFrameEndpointTransport {
    fn connector_kind(&self) -> ServiceFrameConnectorKind;

    fn supports_endpoint(&self, _endpoint: &str, kind: ServiceFrameConnectorKind) -> bool {
        kind == self.connector_kind()
    }

    fn connect_endpoint(
        &self,
        endpoint: &str,
        codec: ServiceFrameCodec,
    ) -> Result<Box<dyn ServiceFrameIoStream>, BroadwebdError>;
}

pub fn service_frame_tcp_socket_addr_from_endpoint(
    endpoint: &str,
) -> Result<SocketAddr, BroadwebdError> {
    if let Ok(socket_addr) = endpoint.parse::<SocketAddr>() {
        return Ok(socket_addr);
    }

    let multiaddr = Multiaddr::parse(endpoint).map_err(|error| {
        BroadwebdError::Request(format!(
            "service-frame TCP endpoint must be a literal socket address or /ip4|/ip6/.../tcp/... multiaddr: {error}"
        ))
    })?;
    service_frame_tcp_socket_addr_from_multiaddr(&multiaddr)
}

pub fn service_frame_tcp_endpoint_for_source(
    endpoint: &str,
    source_addr: SocketAddr,
) -> Result<String, BroadwebdError> {
    let socket_addr = service_frame_tcp_socket_addr_from_endpoint(endpoint)?;
    let ip = match socket_addr.ip() {
        IpAddr::V4(ip) if ip.is_unspecified() => source_addr.ip(),
        IpAddr::V6(ip) if ip.is_unspecified() => source_addr.ip(),
        ip => ip,
    };
    let socket_addr = SocketAddr::new(ip, socket_addr.port());
    if endpoint.trim_start().starts_with('/') {
        Ok(service_frame_tcp_multiaddr_from_socket_addr(socket_addr))
    } else {
        Ok(socket_addr.to_string())
    }
}

pub fn serve_one_service_frame_request_over_stream<S: Read + Write>(
    codec: ServiceFrameCodec,
    handler: &dyn BroadwebdClient,
    stream: &mut S,
) -> Result<(), BroadwebdError> {
    let request = codec.read_request(stream)?;
    let response = handler.dispatch_service_request(request)?;
    codec.write_response(stream, &response)
}

pub struct InProcessServiceFrameStream<'a> {
    handler: &'a dyn BroadwebdClient,
    codec: ServiceFrameCodec,
    request_bytes: Vec<u8>,
    response_bytes: Cursor<Vec<u8>>,
    response_ready: bool,
}

impl<'a> InProcessServiceFrameStream<'a> {
    pub fn new(handler: &'a dyn BroadwebdClient, codec: ServiceFrameCodec) -> Self {
        Self {
            handler,
            codec,
            request_bytes: Vec::new(),
            response_bytes: Cursor::new(Vec::new()),
            response_ready: false,
        }
    }

    fn prepare_response(&mut self) -> io::Result<()> {
        if self.response_ready {
            return Ok(());
        }

        let mut request_reader = Cursor::new(self.request_bytes.as_slice());
        let request = self
            .codec
            .read_request(&mut request_reader)
            .map_err(service_frame_io_error)?;
        let response = self
            .handler
            .dispatch_service_request(request)
            .map_err(service_frame_io_error)?;
        let mut response_bytes = Vec::new();
        self.codec
            .write_response(&mut response_bytes, &response)
            .map_err(service_frame_io_error)?;
        self.response_bytes = Cursor::new(response_bytes);
        self.response_ready = true;
        Ok(())
    }
}

impl Read for InProcessServiceFrameStream<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.prepare_response()?;
        self.response_bytes.read(buffer)
    }
}

impl Write for InProcessServiceFrameStream<'_> {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        if self.response_ready {
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "in-process service-frame stream is one request per connection",
            ));
        }

        self.request_bytes.extend_from_slice(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.prepare_response()
    }
}

#[derive(Clone, Copy)]
pub struct InProcessServiceFrameConnector<'a> {
    handler: &'a dyn BroadwebdClient,
}

impl<'a> InProcessServiceFrameConnector<'a> {
    pub fn new(handler: &'a dyn BroadwebdClient) -> Self {
        Self { handler }
    }
}

impl<'a> ServiceFrameConnector for InProcessServiceFrameConnector<'a> {
    type Stream = InProcessServiceFrameStream<'a>;

    fn connect(&self, codec: ServiceFrameCodec) -> Result<Self::Stream, BroadwebdError> {
        Ok(InProcessServiceFrameStream::new(self.handler, codec))
    }
}

#[derive(Default)]
pub struct InProcessServiceFrameEndpointRegistry<'a> {
    endpoints: BTreeMap<String, InProcessServiceFrameEndpoint<'a>>,
}

#[derive(Clone, Copy)]
struct InProcessServiceFrameEndpoint<'a> {
    handler: &'a dyn BroadwebdClient,
    kind: ServiceFrameConnectorKind,
}

impl<'a> InProcessServiceFrameEndpointRegistry<'a> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(
        &mut self,
        endpoint: impl Into<String>,
        handler: &'a dyn BroadwebdClient,
    ) -> Result<(), BroadwebdError> {
        let endpoint = endpoint.into();
        let kind = service_frame_connector_kind_for_endpoint(endpoint.as_str())?;
        if !kind.is_deferred() {
            return Err(BroadwebdError::UnsupportedRequest(format!(
                "in-process service-frame endpoint registry does not model TCP endpoint {endpoint}; use TcpServiceFrameConnector for TCP"
            )));
        }
        if self.endpoints.contains_key(endpoint.as_str()) {
            return Err(BroadwebdError::Request(format!(
                "in-process service-frame endpoint is already registered: {endpoint}"
            )));
        }

        self.endpoints
            .insert(endpoint, InProcessServiceFrameEndpoint { handler, kind });
        Ok(())
    }

    pub fn contains_endpoint(&self, endpoint: &str) -> bool {
        self.endpoints.contains_key(endpoint)
    }

    pub fn connector(
        &self,
        endpoint: impl Into<String>,
    ) -> Result<InProcessServiceFrameEndpointConnector<'_, 'a>, BroadwebdError> {
        let endpoint = endpoint.into();
        self.endpoint(endpoint.as_str())?;
        Ok(InProcessServiceFrameEndpointConnector {
            registry: self,
            endpoint,
        })
    }

    fn endpoint(
        &self,
        endpoint: &str,
    ) -> Result<InProcessServiceFrameEndpoint<'a>, BroadwebdError> {
        let kind = service_frame_connector_kind_for_endpoint(endpoint)?;
        if !kind.is_deferred() {
            return Err(BroadwebdError::UnsupportedRequest(format!(
                "in-process service-frame endpoint registry cannot connect TCP endpoint {endpoint}; use TcpServiceFrameConnector for TCP"
            )));
        }

        self.endpoints.get(endpoint).copied().ok_or_else(|| {
            BroadwebdError::UnsupportedRequest(format!(
                "unregistered in-process service-frame endpoint: {endpoint}"
            ))
        })
    }
}

pub struct ServiceFrameEndpointConnectorFactory<'registry, 'handler> {
    registry: Option<&'registry InProcessServiceFrameEndpointRegistry<'handler>>,
    transports: Vec<&'registry dyn ServiceFrameEndpointTransport>,
    tcp_timeout: Duration,
}

impl<'registry, 'handler> ServiceFrameEndpointConnectorFactory<'registry, 'handler> {
    pub fn new() -> Self {
        Self {
            registry: None,
            transports: Vec::new(),
            tcp_timeout: Duration::from_secs(12),
        }
    }

    pub fn with_in_process_registry(
        registry: &'registry InProcessServiceFrameEndpointRegistry<'handler>,
    ) -> Self {
        Self {
            registry: Some(registry),
            transports: Vec::new(),
            tcp_timeout: Duration::from_secs(12),
        }
    }

    pub fn with_transport(
        mut self,
        transport: &'registry dyn ServiceFrameEndpointTransport,
    ) -> Self {
        self.transports.push(transport);
        self
    }

    pub fn with_tcp_timeout(mut self, timeout: Duration) -> Self {
        self.tcp_timeout = timeout;
        self
    }

    pub fn connector(
        &self,
        endpoint: impl Into<String>,
    ) -> Result<ServiceFrameEndpointConnector<'registry, 'handler>, BroadwebdError> {
        let endpoint = endpoint.into();
        let kind = service_frame_connector_kind_for_endpoint(endpoint.as_str())?;
        if kind == ServiceFrameConnectorKind::Tcp {
            return Ok(ServiceFrameEndpointConnector::Tcp(
                TcpServiceFrameConnector::new(endpoint).with_timeout(self.tcp_timeout),
            ));
        }

        if let Some(registry) = self.registry {
            if registry.contains_endpoint(endpoint.as_str()) {
                return Ok(ServiceFrameEndpointConnector::InProcess(
                    registry.connector(endpoint)?,
                ));
            }
        }

        if let Some(transport) = self
            .transports
            .iter()
            .copied()
            .find(|transport| transport.supports_endpoint(endpoint.as_str(), kind))
        {
            return Ok(ServiceFrameEndpointConnector::Transport {
                endpoint,
                kind,
                transport,
            });
        }

        Ok(ServiceFrameEndpointConnector::Deferred(
            DeferredServiceFrameConnector::new(endpoint)?,
        ))
    }
}

impl<'registry, 'handler> Default for ServiceFrameEndpointConnectorFactory<'registry, 'handler> {
    fn default() -> Self {
        Self::new()
    }
}

pub enum ServiceFrameEndpointConnector<'registry, 'handler> {
    Tcp(TcpServiceFrameConnector),
    Deferred(DeferredServiceFrameConnector),
    InProcess(InProcessServiceFrameEndpointConnector<'registry, 'handler>),
    Transport {
        endpoint: String,
        kind: ServiceFrameConnectorKind,
        transport: &'registry dyn ServiceFrameEndpointTransport,
    },
}

impl<'registry, 'handler> ServiceFrameEndpointConnector<'registry, 'handler> {
    pub fn endpoint(&self) -> &str {
        match self {
            Self::Tcp(connector) => connector.address(),
            Self::Deferred(connector) => connector.endpoint(),
            Self::InProcess(connector) => connector.endpoint(),
            Self::Transport { endpoint, .. } => endpoint,
        }
    }

    pub fn kind(&self) -> Result<ServiceFrameConnectorKind, BroadwebdError> {
        match self {
            Self::Tcp(_) => Ok(ServiceFrameConnectorKind::Tcp),
            Self::Deferred(connector) => Ok(connector.kind()),
            Self::InProcess(connector) => connector.kind(),
            Self::Transport { kind, .. } => Ok(*kind),
        }
    }
}

pub enum ServiceFrameEndpointStream<'handler> {
    Tcp(TcpStream),
    Deferred(Cursor<Vec<u8>>),
    InProcess(InProcessServiceFrameStream<'handler>),
    Transport(Box<dyn ServiceFrameIoStream>),
}

impl Read for ServiceFrameEndpointStream<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        match self {
            Self::Tcp(stream) => stream.read(buffer),
            Self::Deferred(stream) => stream.read(buffer),
            Self::InProcess(stream) => stream.read(buffer),
            Self::Transport(stream) => stream.read(buffer),
        }
    }
}

impl Write for ServiceFrameEndpointStream<'_> {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        match self {
            Self::Tcp(stream) => stream.write(buffer),
            Self::Deferred(stream) => stream.write(buffer),
            Self::InProcess(stream) => stream.write(buffer),
            Self::Transport(stream) => stream.write(buffer),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            Self::Tcp(stream) => stream.flush(),
            Self::Deferred(stream) => stream.flush(),
            Self::InProcess(stream) => stream.flush(),
            Self::Transport(stream) => stream.flush(),
        }
    }
}

impl<'handler> ServiceFrameConnector for ServiceFrameEndpointConnector<'_, 'handler> {
    type Stream = ServiceFrameEndpointStream<'handler>;

    fn connect(&self, codec: ServiceFrameCodec) -> Result<Self::Stream, BroadwebdError> {
        match self {
            Self::Tcp(connector) => connector
                .connect(codec)
                .map(ServiceFrameEndpointStream::Tcp),
            Self::Deferred(connector) => connector
                .connect(codec)
                .map(ServiceFrameEndpointStream::Deferred),
            Self::InProcess(connector) => connector
                .connect(codec)
                .map(ServiceFrameEndpointStream::InProcess),
            Self::Transport {
                endpoint,
                transport,
                ..
            } => transport
                .connect_endpoint(endpoint.as_str(), codec)
                .map(ServiceFrameEndpointStream::Transport),
        }
    }
}

pub struct InProcessServiceFrameEndpointConnector<'registry, 'handler> {
    registry: &'registry InProcessServiceFrameEndpointRegistry<'handler>,
    endpoint: String,
}

impl InProcessServiceFrameEndpointConnector<'_, '_> {
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    pub fn kind(&self) -> Result<ServiceFrameConnectorKind, BroadwebdError> {
        Ok(self.registry.endpoint(self.endpoint.as_str())?.kind)
    }
}

impl<'handler> ServiceFrameConnector for InProcessServiceFrameEndpointConnector<'_, 'handler> {
    type Stream = InProcessServiceFrameStream<'handler>;

    fn connect(&self, codec: ServiceFrameCodec) -> Result<Self::Stream, BroadwebdError> {
        let endpoint = self.registry.endpoint(self.endpoint.as_str())?;
        Ok(InProcessServiceFrameStream::new(endpoint.handler, codec))
    }
}

pub struct ServiceFrameBroadwebdClient<'a> {
    inner: &'a dyn BroadwebdClient,
    codec: ServiceFrameCodec,
}

impl<'a> ServiceFrameBroadwebdClient<'a> {
    pub fn new(inner: &'a dyn BroadwebdClient) -> Self {
        Self::with_codec(inner, ServiceFrameCodec::default())
    }

    pub fn with_codec(inner: &'a dyn BroadwebdClient, codec: ServiceFrameCodec) -> Self {
        Self { inner, codec }
    }

    pub fn codec(&self) -> ServiceFrameCodec {
        self.codec
    }
}

impl BroadwebdClient for ServiceFrameBroadwebdClient<'_> {
    fn health(&self) -> DaemonHealth {
        self.inner.health()
    }

    fn status_snapshot(&self) -> BroadwebStatusSnapshot {
        self.inner.status_snapshot()
    }

    fn dispatch_service_request(
        &self,
        request: ServiceRequest,
    ) -> Result<ServiceResponse, BroadwebdError> {
        let connector = InProcessServiceFrameConnector::new(self.inner);
        dispatch_service_frame_request_with_connector(self.codec, &connector, &request)
    }

    fn temporary_downloads(
        &self,
        profile: &str,
    ) -> Result<Vec<TemporaryDownloadRecord>, BroadwebdError> {
        self.inner.temporary_downloads(profile)
    }

    fn downloads(&self, profile: &str) -> Result<Vec<TemporaryDownloadRecord>, BroadwebdError> {
        self.inner.downloads(profile)
    }
}

pub struct ConnectorServiceFrameBroadwebdClient<C> {
    connector: C,
    codec: ServiceFrameCodec,
}

impl<C> ConnectorServiceFrameBroadwebdClient<C> {
    pub fn new(connector: C) -> Self {
        Self::with_codec(connector, ServiceFrameCodec::default())
    }

    pub fn with_codec(connector: C, codec: ServiceFrameCodec) -> Self {
        Self { connector, codec }
    }

    pub fn connector(&self) -> &C {
        &self.connector
    }

    pub fn codec(&self) -> ServiceFrameCodec {
        self.codec
    }
}

impl<C: ServiceFrameConnector> BroadwebdClient for ConnectorServiceFrameBroadwebdClient<C> {
    fn health(&self) -> DaemonHealth {
        DaemonHealth {
            lifecycle: DaemonLifecycle::Ready,
            plugins: Vec::new(),
        }
    }

    fn status_snapshot(&self) -> BroadwebStatusSnapshot {
        BroadwebStatusSnapshot::idle()
    }

    fn dispatch_service_request(
        &self,
        request: ServiceRequest,
    ) -> Result<ServiceResponse, BroadwebdError> {
        dispatch_service_frame_request_with_connector(self.codec, &self.connector, &request)
    }

    fn temporary_downloads(
        &self,
        _profile: &str,
    ) -> Result<Vec<TemporaryDownloadRecord>, BroadwebdError> {
        Err(BroadwebdError::UnsupportedRequest(
            "temporary download listing is not exposed through service-frame connectors yet"
                .to_string(),
        ))
    }

    fn downloads(&self, _profile: &str) -> Result<Vec<TemporaryDownloadRecord>, BroadwebdError> {
        Err(BroadwebdError::UnsupportedRequest(
            "download listing is not exposed through service-frame connectors yet".to_string(),
        ))
    }
}

#[derive(Clone, Debug)]
pub struct TcpServiceFrameConnector {
    address: String,
    timeout: Duration,
}

impl TcpServiceFrameConnector {
    pub fn new(address: impl Into<String>) -> Self {
        Self {
            address: address.into(),
            timeout: Duration::from_secs(12),
        }
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn address(&self) -> &str {
        &self.address
    }

    pub fn timeout(&self) -> Duration {
        self.timeout
    }
}

impl ServiceFrameConnector for TcpServiceFrameConnector {
    type Stream = TcpStream;

    fn connect(&self, _codec: ServiceFrameCodec) -> Result<Self::Stream, BroadwebdError> {
        let address = service_frame_tcp_socket_addr_from_endpoint(self.address.as_str())?;
        let stream = TcpStream::connect_timeout(&address, self.timeout)?;
        stream.set_read_timeout(Some(self.timeout))?;
        stream.set_write_timeout(Some(self.timeout))?;
        Ok(stream)
    }
}

#[derive(Clone, Debug)]
pub struct TcpServiceFrameBroadwebdClient {
    address: String,
    codec: ServiceFrameCodec,
    timeout: Duration,
}

impl TcpServiceFrameBroadwebdClient {
    pub fn new(address: impl Into<String>) -> Self {
        Self::with_codec(address, ServiceFrameCodec::default())
    }

    pub fn with_codec(address: impl Into<String>, codec: ServiceFrameCodec) -> Self {
        Self {
            address: address.into(),
            codec,
            timeout: Duration::from_secs(12),
        }
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn address(&self) -> &str {
        &self.address
    }

    pub fn codec(&self) -> ServiceFrameCodec {
        self.codec
    }

    pub fn timeout(&self) -> Duration {
        self.timeout
    }
}

impl BroadwebdClient for TcpServiceFrameBroadwebdClient {
    fn health(&self) -> DaemonHealth {
        DaemonHealth {
            lifecycle: DaemonLifecycle::Ready,
            plugins: Vec::new(),
        }
    }

    fn status_snapshot(&self) -> BroadwebStatusSnapshot {
        BroadwebStatusSnapshot::idle()
    }

    fn dispatch_service_request(
        &self,
        request: ServiceRequest,
    ) -> Result<ServiceResponse, BroadwebdError> {
        let connector =
            TcpServiceFrameConnector::new(self.address.clone()).with_timeout(self.timeout);
        dispatch_service_frame_request_with_connector(self.codec, &connector, &request)
    }

    fn temporary_downloads(
        &self,
        _profile: &str,
    ) -> Result<Vec<TemporaryDownloadRecord>, BroadwebdError> {
        Err(BroadwebdError::UnsupportedRequest(
            "temporary download listing is not exposed through TCP service frames yet".to_string(),
        ))
    }

    fn downloads(&self, _profile: &str) -> Result<Vec<TemporaryDownloadRecord>, BroadwebdError> {
        Err(BroadwebdError::UnsupportedRequest(
            "download listing is not exposed through TCP service frames yet".to_string(),
        ))
    }
}

fn encode_json_frame<T: Serialize>(
    value: &T,
    max_frame_bytes: usize,
    label: &str,
) -> Result<Vec<u8>, BroadwebdError> {
    let mut writer = BoundedFrameWriter::new(max_frame_bytes);
    let result = serde_json::to_writer(&mut writer, value);
    if let Err(error) = result {
        if let Some(actual) = writer.rejected_len() {
            return Err(BroadwebdError::FrameTooLarge {
                limit: max_frame_bytes,
                actual,
            });
        }

        return Err(BroadwebdError::Request(format!(
            "encode {label} service frame: {error}"
        )));
    }

    Ok(writer.into_bytes())
}

fn decode_json_frame<T: DeserializeOwned>(
    frame: &[u8],
    max_frame_bytes: usize,
    label: &str,
) -> Result<T, BroadwebdError> {
    if frame.len() > max_frame_bytes {
        return Err(BroadwebdError::FrameTooLarge {
            limit: max_frame_bytes,
            actual: frame.len(),
        });
    }

    serde_json::from_slice(frame)
        .map_err(|error| BroadwebdError::Request(format!("decode {label} service frame: {error}")))
}

fn write_len_prefixed_frame(writer: &mut impl Write, frame: &[u8]) -> Result<(), BroadwebdError> {
    let frame_len = u32::try_from(frame.len()).map_err(|_| {
        BroadwebdError::Request(format!(
            "service frame is too large for length-prefixed transport: {} bytes",
            frame.len()
        ))
    })?;
    writer.write_all(&frame_len.to_be_bytes())?;
    writer.write_all(frame)?;
    writer.flush()?;
    Ok(())
}

fn read_len_prefixed_frame(
    reader: &mut impl Read,
    max_frame_bytes: usize,
) -> Result<Vec<u8>, BroadwebdError> {
    let mut len_bytes = [0_u8; 4];
    reader.read_exact(&mut len_bytes)?;
    let frame_len = u32::from_be_bytes(len_bytes) as usize;
    if frame_len > max_frame_bytes {
        return Err(BroadwebdError::FrameTooLarge {
            limit: max_frame_bytes,
            actual: frame_len,
        });
    }

    let mut frame = vec![0_u8; frame_len];
    reader.read_exact(&mut frame)?;
    Ok(frame)
}

fn service_frame_connector_kind_for_multiaddr(
    endpoint: &Multiaddr,
) -> Result<ServiceFrameConnectorKind, BroadwebdError> {
    let segments = endpoint.segments().collect::<Vec<_>>();
    if service_frame_tcp_socket_addr_from_multiaddr(endpoint).is_ok() {
        return Ok(ServiceFrameConnectorKind::Tcp);
    }
    if segments.first() == Some(&"ipns") && segments.len() >= 2 {
        return Ok(ServiceFrameConnectorKind::Ipns);
    }
    if segments.iter().any(|segment| *segment == "p2p") {
        return Ok(ServiceFrameConnectorKind::Libp2p);
    }
    if segments.first() == Some(&"dnsaddr") && segments.len() >= 2 {
        return Ok(ServiceFrameConnectorKind::Dnsaddr);
    }
    if matches!(segments.first().copied(), Some("iroh" | "iroh-node")) && segments.len() >= 2 {
        return Ok(ServiceFrameConnectorKind::Iroh);
    }

    Err(BroadwebdError::UnsupportedRequest(format!(
        "service-frame endpoint {} needs an explicit protocol connector",
        endpoint.as_str()
    )))
}

fn service_frame_tcp_socket_addr_from_multiaddr(
    endpoint: &Multiaddr,
) -> Result<SocketAddr, BroadwebdError> {
    let segments = endpoint.segments().collect::<Vec<_>>();
    let [network, host, transport, port] = segments.as_slice() else {
        return Err(BroadwebdError::UnsupportedRequest(format!(
            "service-frame TCP connector cannot open multiaddr endpoint {}; use a connector for its transport stack",
            endpoint.as_str()
        )));
    };
    if *transport != "tcp" {
        return Err(BroadwebdError::UnsupportedRequest(format!(
            "service-frame TCP connector cannot open non-TCP multiaddr endpoint {}",
            endpoint.as_str()
        )));
    }

    let ip = match *network {
        "ip4" => {
            let ip = host.parse::<IpAddr>().map_err(|error| {
                BroadwebdError::Request(format!(
                    "invalid service-frame TCP /ip4 endpoint {}: {error}",
                    endpoint.as_str()
                ))
            })?;
            if !ip.is_ipv4() {
                return Err(BroadwebdError::Request(format!(
                    "service-frame TCP /ip4 endpoint contains non-IPv4 address: {}",
                    endpoint.as_str()
                )));
            }
            ip
        }
        "ip6" => {
            let ip = host.parse::<IpAddr>().map_err(|error| {
                BroadwebdError::Request(format!(
                    "invalid service-frame TCP /ip6 endpoint {}: {error}",
                    endpoint.as_str()
                ))
            })?;
            if !ip.is_ipv6() {
                return Err(BroadwebdError::Request(format!(
                    "service-frame TCP /ip6 endpoint contains non-IPv6 address: {}",
                    endpoint.as_str()
                )));
            }
            ip
        }
        _ => {
            return Err(BroadwebdError::UnsupportedRequest(format!(
                "service-frame TCP connector requires literal /ip4 or /ip6 multiaddr endpoint: {}",
                endpoint.as_str()
            )));
        }
    };
    let port = port.parse::<u16>().map_err(|error| {
        BroadwebdError::Request(format!(
            "invalid service-frame TCP port in multiaddr endpoint {}: {error}",
            endpoint.as_str()
        ))
    })?;
    Ok(SocketAddr::new(ip, port))
}

fn validate_deferred_service_frame_endpoint_target(
    protocol: &str,
    target: &str,
) -> Result<(), BroadwebdError> {
    if !target.is_empty()
        && target.len() <= 512
        && target.chars().all(|character| {
            character.is_ascii_alphanumeric()
                || matches!(character, '-' | '_' | '.' | ':' | '~' | '%' | '+' | '=')
        })
    {
        return Ok(());
    }

    Err(BroadwebdError::Request(format!(
        "invalid service-frame {protocol} endpoint target: {target:?}"
    )))
}

fn service_frame_tcp_multiaddr_from_socket_addr(socket_addr: SocketAddr) -> String {
    match socket_addr.ip() {
        IpAddr::V4(ip) => format!("/ip4/{ip}/tcp/{}", socket_addr.port()),
        IpAddr::V6(ip) => format!("/ip6/{ip}/tcp/{}", socket_addr.port()),
    }
}

fn service_frame_io_error(error: BroadwebdError) -> io::Error {
    io::Error::other(error)
}

struct BoundedFrameWriter {
    bytes: Vec<u8>,
    max_frame_bytes: usize,
    rejected_len: Option<usize>,
}

impl BoundedFrameWriter {
    fn new(max_frame_bytes: usize) -> Self {
        Self {
            bytes: Vec::with_capacity(max_frame_bytes.min(4096)),
            max_frame_bytes,
            rejected_len: None,
        }
    }

    fn rejected_len(&self) -> Option<usize> {
        self.rejected_len
    }

    fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

impl Write for BoundedFrameWriter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        let next_len = self.bytes.len().saturating_add(buffer.len());
        if next_len > self.max_frame_bytes {
            self.rejected_len = Some(next_len);
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "broadwebd service frame exceeds configured byte limit",
            ));
        }

        self.bytes.extend_from_slice(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ProfileSyncProfileRequest, ProfileSyncRequest, ProfileSyncResponse, ServiceRequest,
        ServiceResponse,
    };
    use std::io::Cursor;

    #[test]
    fn length_prefixed_service_frames_round_trip_request_and_response() {
        let codec = ServiceFrameCodec::new(4096);
        let request = ServiceRequest::ProfileSync(ProfileSyncRequest::DiscoverProviders(
            ProfileSyncProfileRequest::new("default"),
        ));
        let response = ServiceResponse::ProfileSync(ProfileSyncResponse::RetainedObjects {
            object_ids: vec!["object-a".to_string()],
        });
        let mut bytes = Vec::new();

        codec
            .write_request(&mut bytes, &request)
            .expect("write request frame");
        codec
            .write_response(&mut bytes, &response)
            .expect("write response frame");

        let mut cursor = Cursor::new(bytes);
        assert_eq!(
            codec.read_request(&mut cursor).expect("read request frame"),
            request
        );
        assert_eq!(
            codec
                .read_response(&mut cursor)
                .expect("read response frame"),
            response
        );
    }

    #[test]
    fn length_prefixed_service_frames_reject_oversized_payloads_before_reading_json() {
        let codec = ServiceFrameCodec::new(2);
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&3_u32.to_be_bytes());
        bytes.extend_from_slice(b"{}!");

        let error = codec
            .read_request(&mut Cursor::new(bytes))
            .expect_err("oversized frame should fail before JSON parsing");
        assert_eq!(
            error,
            BroadwebdError::FrameTooLarge {
                limit: 2,
                actual: 3,
            }
        );
    }

    #[test]
    fn service_frame_tcp_endpoint_accepts_literal_ip_socket_and_multiaddr() {
        let source_addr = "192.168.50.55:47883"
            .parse::<SocketAddr>()
            .expect("source socket");

        assert_eq!(
            service_frame_tcp_socket_addr_from_endpoint("127.0.0.1:9443").expect("literal socket"),
            "127.0.0.1:9443".parse::<SocketAddr>().unwrap()
        );
        assert_eq!(
            service_frame_tcp_socket_addr_from_endpoint("/ip4/127.0.0.1/tcp/9443")
                .expect("literal /ip4 tcp multiaddr"),
            "127.0.0.1:9443".parse::<SocketAddr>().unwrap()
        );
        assert_eq!(
            service_frame_tcp_endpoint_for_source("0.0.0.0:9443", source_addr)
                .expect("rewrite unspecified socket address"),
            "192.168.50.55:9443"
        );
        assert_eq!(
            service_frame_tcp_endpoint_for_source("/ip4/0.0.0.0/tcp/9443", source_addr)
                .expect("rewrite unspecified /ip4 multiaddr"),
            "/ip4/192.168.50.55/tcp/9443"
        );
    }

    #[test]
    fn service_frame_endpoint_classifier_separates_tcp_and_deferred_connectors() {
        assert_eq!(
            service_frame_connector_kind_for_endpoint("127.0.0.1:9443")
                .expect("literal TCP endpoint"),
            ServiceFrameConnectorKind::Tcp
        );
        assert_eq!(
            service_frame_connector_kind_for_endpoint("/ip4/127.0.0.1/tcp/9443")
                .expect("literal TCP multiaddr endpoint"),
            ServiceFrameConnectorKind::Tcp
        );
        assert_eq!(
            service_frame_connector_kind_for_endpoint("/ip4/127.0.0.1/tcp/9443/p2p/peer-a")
                .expect("libp2p multiaddr endpoint"),
            ServiceFrameConnectorKind::Libp2p
        );
        assert_eq!(
            service_frame_connector_kind_for_endpoint("/dnsaddr/bootstrap.libp2p.io/tcp/443")
                .expect("dnsaddr endpoint"),
            ServiceFrameConnectorKind::Dnsaddr
        );
        assert_eq!(
            service_frame_connector_kind_for_endpoint("/dnsaddr/bootstrap.libp2p.io/p2p/peer-a")
                .expect("dnsaddr p2p endpoint"),
            ServiceFrameConnectorKind::Libp2p
        );
        assert_eq!(
            service_frame_connector_kind_for_endpoint("/ipns/k51-profile-root")
                .expect("IPNS endpoint"),
            ServiceFrameConnectorKind::Ipns
        );
        assert_eq!(
            service_frame_connector_kind_for_endpoint("iroh-node:node-a")
                .expect("Iroh node endpoint"),
            ServiceFrameConnectorKind::Iroh
        );
    }

    #[test]
    fn deferred_service_frame_connector_fails_closed_until_transport_exists() {
        let connector =
            DeferredServiceFrameConnector::new("/dnsaddr/bootstrap.libp2p.io/p2p/peer-a")
                .expect("deferred p2p connector");
        assert_eq!(connector.kind(), ServiceFrameConnectorKind::Libp2p);
        assert_eq!(
            connector.endpoint(),
            "/dnsaddr/bootstrap.libp2p.io/p2p/peer-a"
        );

        let error = connector
            .connect(ServiceFrameCodec::new(4096))
            .expect_err("deferred connector must not open sockets");
        assert!(
            error
                .to_string()
                .contains("service-frame libp2p connector is not implemented yet")
        );

        let tcp = DeferredServiceFrameConnector::new("/ip4/127.0.0.1/tcp/9443")
            .expect_err("TCP endpoints must use the TCP connector");
        assert!(
            tcp.to_string()
                .contains("cannot be created for TCP endpoint")
        );
    }

    #[test]
    fn service_frame_tcp_endpoint_rejects_dns_and_p2p_multiaddrs_without_connector() {
        let hostname = service_frame_tcp_socket_addr_from_endpoint("localhost:9443")
            .expect_err("TCP connector must not perform DNS resolution");
        assert!(
            hostname
                .to_string()
                .contains("literal socket address or /ip4|/ip6")
        );

        let dns_multiaddr =
            service_frame_tcp_socket_addr_from_endpoint("/dnsaddr/bootstrap.libp2p.io/tcp/443")
                .expect_err("TCP connector must not resolve dnsaddr multiaddrs");
        assert!(
            dns_multiaddr
                .to_string()
                .contains("requires literal /ip4 or /ip6")
        );

        let p2p_multiaddr =
            service_frame_tcp_socket_addr_from_endpoint("/ip4/127.0.0.1/tcp/9443/p2p/peer")
                .expect_err("TCP connector must not consume p2p multiaddrs");
        assert!(p2p_multiaddr.to_string().contains("use a connector"));
    }

    #[test]
    fn in_process_service_frame_stream_uses_same_exchange_helper_as_tcp() {
        struct EchoClient;

        impl BroadwebdClient for EchoClient {
            fn health(&self) -> DaemonHealth {
                DaemonHealth {
                    lifecycle: DaemonLifecycle::Ready,
                    plugins: Vec::new(),
                }
            }

            fn status_snapshot(&self) -> BroadwebStatusSnapshot {
                BroadwebStatusSnapshot::idle()
            }

            fn dispatch_service_request(
                &self,
                request: ServiceRequest,
            ) -> Result<ServiceResponse, BroadwebdError> {
                assert_eq!(
                    request,
                    ServiceRequest::ProfileSync(ProfileSyncRequest::DiscoverProviders(
                        ProfileSyncProfileRequest::new("default")
                    ))
                );
                Ok(ServiceResponse::ProfileSync(
                    ProfileSyncResponse::RetainedObjects {
                        object_ids: vec!["socket-shim-object".to_string()],
                    },
                ))
            }

            fn temporary_downloads(
                &self,
                _profile: &str,
            ) -> Result<Vec<TemporaryDownloadRecord>, BroadwebdError> {
                Ok(Vec::new())
            }

            fn downloads(
                &self,
                _profile: &str,
            ) -> Result<Vec<TemporaryDownloadRecord>, BroadwebdError> {
                Ok(Vec::new())
            }
        }

        let codec = ServiceFrameCodec::new(4096);
        let request = ServiceRequest::ProfileSync(ProfileSyncRequest::DiscoverProviders(
            ProfileSyncProfileRequest::new("default"),
        ));
        let handler = EchoClient;
        let mut stream = InProcessServiceFrameStream::new(&handler, codec);

        let response = dispatch_service_frame_request_over_stream(codec, &mut stream, &request)
            .expect("exchange request through in-process service-frame stream");
        assert_eq!(
            response,
            ServiceResponse::ProfileSync(ProfileSyncResponse::RetainedObjects {
                object_ids: vec!["socket-shim-object".to_string()],
            })
        );
    }

    #[test]
    fn connector_service_frame_client_uses_swappable_stream_boundary() {
        struct EchoClient;

        impl BroadwebdClient for EchoClient {
            fn health(&self) -> DaemonHealth {
                DaemonHealth {
                    lifecycle: DaemonLifecycle::Ready,
                    plugins: Vec::new(),
                }
            }

            fn status_snapshot(&self) -> BroadwebStatusSnapshot {
                BroadwebStatusSnapshot::idle()
            }

            fn dispatch_service_request(
                &self,
                request: ServiceRequest,
            ) -> Result<ServiceResponse, BroadwebdError> {
                assert_eq!(
                    request,
                    ServiceRequest::ProfileSync(ProfileSyncRequest::DiscoverProviders(
                        ProfileSyncProfileRequest::new("default")
                    ))
                );
                Ok(ServiceResponse::ProfileSync(
                    ProfileSyncResponse::RetainedObjects {
                        object_ids: vec!["connector-object".to_string()],
                    },
                ))
            }

            fn temporary_downloads(
                &self,
                _profile: &str,
            ) -> Result<Vec<TemporaryDownloadRecord>, BroadwebdError> {
                Ok(Vec::new())
            }

            fn downloads(
                &self,
                _profile: &str,
            ) -> Result<Vec<TemporaryDownloadRecord>, BroadwebdError> {
                Ok(Vec::new())
            }
        }

        let handler = EchoClient;
        let connector = InProcessServiceFrameConnector::new(&handler);
        let client = ConnectorServiceFrameBroadwebdClient::with_codec(
            connector,
            ServiceFrameCodec::new(4096),
        );
        let response = client
            .profile_sync(ProfileSyncRequest::DiscoverProviders(
                ProfileSyncProfileRequest::new("default"),
            ))
            .expect("exchange request through connector-backed service-frame client");

        assert_eq!(
            response,
            ProfileSyncResponse::RetainedObjects {
                object_ids: vec!["connector-object".to_string()],
            }
        );
    }

    #[test]
    fn in_process_service_frame_endpoint_registry_models_deferred_connectors_without_sockets() {
        struct EchoClient;

        impl BroadwebdClient for EchoClient {
            fn health(&self) -> DaemonHealth {
                DaemonHealth {
                    lifecycle: DaemonLifecycle::Ready,
                    plugins: Vec::new(),
                }
            }

            fn status_snapshot(&self) -> BroadwebStatusSnapshot {
                BroadwebStatusSnapshot::idle()
            }

            fn dispatch_service_request(
                &self,
                request: ServiceRequest,
            ) -> Result<ServiceResponse, BroadwebdError> {
                assert_eq!(
                    request,
                    ServiceRequest::ProfileSync(ProfileSyncRequest::DiscoverProviders(
                        ProfileSyncProfileRequest::new("default")
                    ))
                );
                Ok(ServiceResponse::ProfileSync(
                    ProfileSyncResponse::RetainedObjects {
                        object_ids: vec!["registered-endpoint-object".to_string()],
                    },
                ))
            }

            fn temporary_downloads(
                &self,
                _profile: &str,
            ) -> Result<Vec<TemporaryDownloadRecord>, BroadwebdError> {
                Ok(Vec::new())
            }

            fn downloads(
                &self,
                _profile: &str,
            ) -> Result<Vec<TemporaryDownloadRecord>, BroadwebdError> {
                Ok(Vec::new())
            }
        }

        let handler = EchoClient;
        let mut registry = InProcessServiceFrameEndpointRegistry::new();
        let p2p_endpoint = "/dnsaddr/rendezvous.slate.test/tcp/443/wss/p2p/12D3KooWDeviceA";
        let ipns_endpoint = "/ipns/k51-profile-sync-root";
        let iroh_endpoint = "iroh-node:node-a";
        registry
            .register(p2p_endpoint, &handler)
            .expect("register p2p-shaped endpoint");
        registry
            .register(ipns_endpoint, &handler)
            .expect("register IPNS-shaped endpoint");
        registry
            .register(iroh_endpoint, &handler)
            .expect("register Iroh-shaped endpoint");

        let connector = registry
            .connector(p2p_endpoint)
            .expect("build connector for registered endpoint");
        assert_eq!(connector.endpoint(), p2p_endpoint);
        assert_eq!(
            connector.kind().expect("registered endpoint kind"),
            ServiceFrameConnectorKind::Libp2p
        );

        let client = ConnectorServiceFrameBroadwebdClient::with_codec(
            connector,
            ServiceFrameCodec::new(4096),
        );
        let response = client
            .profile_sync(ProfileSyncRequest::DiscoverProviders(
                ProfileSyncProfileRequest::new("default"),
            ))
            .expect("exchange request through registered socketless endpoint");

        assert_eq!(
            response,
            ProfileSyncResponse::RetainedObjects {
                object_ids: vec!["registered-endpoint-object".to_string()],
            }
        );
    }

    #[test]
    fn service_frame_endpoint_connector_factory_selects_tcp_and_deferred_boundaries() {
        let factory = ServiceFrameEndpointConnectorFactory::new();
        let tcp_endpoint = "/ip4/127.0.0.1/tcp/9443";
        let tcp_connector = factory
            .connector(tcp_endpoint)
            .expect("build TCP endpoint connector");
        assert_eq!(tcp_connector.endpoint(), tcp_endpoint);
        assert_eq!(
            tcp_connector.kind().expect("TCP connector kind"),
            ServiceFrameConnectorKind::Tcp
        );

        let deferred_endpoint = "/dnsaddr/rendezvous.slate.test/tcp/443/wss/p2p/12D3KooWDeviceA";
        let deferred_connector = factory
            .connector(deferred_endpoint)
            .expect("build deferred endpoint connector");
        assert_eq!(deferred_connector.endpoint(), deferred_endpoint);
        assert_eq!(
            deferred_connector.kind().expect("deferred connector kind"),
            ServiceFrameConnectorKind::Libp2p
        );
        let error = match deferred_connector.connect(ServiceFrameCodec::new(4096)) {
            Ok(_) => panic!("unregistered deferred endpoint must fail closed"),
            Err(error) => error,
        };
        assert!(
            error
                .to_string()
                .contains("service-frame libp2p connector is not implemented yet")
        );
    }

    #[test]
    fn service_frame_endpoint_connector_factory_uses_registered_transport_adapter() {
        struct EchoClient;

        impl BroadwebdClient for EchoClient {
            fn health(&self) -> DaemonHealth {
                DaemonHealth {
                    lifecycle: DaemonLifecycle::Ready,
                    plugins: Vec::new(),
                }
            }

            fn status_snapshot(&self) -> BroadwebStatusSnapshot {
                BroadwebStatusSnapshot::idle()
            }

            fn dispatch_service_request(
                &self,
                request: ServiceRequest,
            ) -> Result<ServiceResponse, BroadwebdError> {
                assert_eq!(
                    request,
                    ServiceRequest::ProfileSync(ProfileSyncRequest::DiscoverProviders(
                        ProfileSyncProfileRequest::new("default")
                    ))
                );
                Ok(ServiceResponse::ProfileSync(
                    ProfileSyncResponse::RetainedObjects {
                        object_ids: vec!["transport-adapter-object".to_string()],
                    },
                ))
            }

            fn temporary_downloads(
                &self,
                _profile: &str,
            ) -> Result<Vec<TemporaryDownloadRecord>, BroadwebdError> {
                Ok(Vec::new())
            }

            fn downloads(
                &self,
                _profile: &str,
            ) -> Result<Vec<TemporaryDownloadRecord>, BroadwebdError> {
                Ok(Vec::new())
            }
        }

        static HANDLER: EchoClient = EchoClient;

        struct IrohAdapter;

        impl ServiceFrameEndpointTransport for IrohAdapter {
            fn connector_kind(&self) -> ServiceFrameConnectorKind {
                ServiceFrameConnectorKind::Iroh
            }

            fn connect_endpoint(
                &self,
                endpoint: &str,
                codec: ServiceFrameCodec,
            ) -> Result<Box<dyn ServiceFrameIoStream>, BroadwebdError> {
                assert_eq!(endpoint, "iroh-node:adapter-node-a");
                Ok(Box::new(InProcessServiceFrameStream::new(&HANDLER, codec)))
            }
        }

        let adapter = IrohAdapter;
        let factory = ServiceFrameEndpointConnectorFactory::new().with_transport(&adapter);
        let connector = factory
            .connector("iroh-node:adapter-node-a")
            .expect("factory selects registered Iroh adapter");
        assert_eq!(connector.endpoint(), "iroh-node:adapter-node-a");
        assert_eq!(
            connector.kind().expect("transport adapter kind"),
            ServiceFrameConnectorKind::Iroh
        );
        let client = ConnectorServiceFrameBroadwebdClient::with_codec(
            connector,
            ServiceFrameCodec::new(4096),
        );

        let response = client
            .profile_sync(ProfileSyncRequest::DiscoverProviders(
                ProfileSyncProfileRequest::new("default"),
            ))
            .expect("exchange request through registered transport adapter");

        assert_eq!(
            response,
            ProfileSyncResponse::RetainedObjects {
                object_ids: vec!["transport-adapter-object".to_string()],
            }
        );
    }

    #[test]
    fn service_frame_endpoint_connector_factory_uses_socketless_registered_endpoint() {
        struct EchoClient;

        impl BroadwebdClient for EchoClient {
            fn health(&self) -> DaemonHealth {
                DaemonHealth {
                    lifecycle: DaemonLifecycle::Ready,
                    plugins: Vec::new(),
                }
            }

            fn status_snapshot(&self) -> BroadwebStatusSnapshot {
                BroadwebStatusSnapshot::idle()
            }

            fn dispatch_service_request(
                &self,
                request: ServiceRequest,
            ) -> Result<ServiceResponse, BroadwebdError> {
                assert_eq!(
                    request,
                    ServiceRequest::ProfileSync(ProfileSyncRequest::DiscoverProviders(
                        ProfileSyncProfileRequest::new("default")
                    ))
                );
                Ok(ServiceResponse::ProfileSync(
                    ProfileSyncResponse::RetainedObjects {
                        object_ids: vec!["factory-endpoint-object".to_string()],
                    },
                ))
            }

            fn temporary_downloads(
                &self,
                _profile: &str,
            ) -> Result<Vec<TemporaryDownloadRecord>, BroadwebdError> {
                Ok(Vec::new())
            }

            fn downloads(
                &self,
                _profile: &str,
            ) -> Result<Vec<TemporaryDownloadRecord>, BroadwebdError> {
                Ok(Vec::new())
            }
        }

        let handler = EchoClient;
        let endpoint = "iroh-node:factory-node-a";
        let mut registry = InProcessServiceFrameEndpointRegistry::new();
        registry
            .register(endpoint, &handler)
            .expect("register socketless Iroh-shaped endpoint");
        let factory = ServiceFrameEndpointConnectorFactory::with_in_process_registry(&registry);
        let connector = factory
            .connector(endpoint)
            .expect("build connector through endpoint factory");
        assert_eq!(connector.endpoint(), endpoint);
        assert_eq!(
            connector.kind().expect("factory connector kind"),
            ServiceFrameConnectorKind::Iroh
        );
        let client = ConnectorServiceFrameBroadwebdClient::with_codec(
            connector,
            ServiceFrameCodec::new(4096),
        );

        let response = client
            .profile_sync(ProfileSyncRequest::DiscoverProviders(
                ProfileSyncProfileRequest::new("default"),
            ))
            .expect("exchange request through factory-selected socketless endpoint");

        assert_eq!(
            response,
            ProfileSyncResponse::RetainedObjects {
                object_ids: vec!["factory-endpoint-object".to_string()],
            }
        );
    }

    #[test]
    fn in_process_service_frame_endpoint_registry_rejects_tcp_and_missing_endpoints() {
        struct EmptyClient;

        impl BroadwebdClient for EmptyClient {
            fn health(&self) -> DaemonHealth {
                DaemonHealth {
                    lifecycle: DaemonLifecycle::Ready,
                    plugins: Vec::new(),
                }
            }

            fn status_snapshot(&self) -> BroadwebStatusSnapshot {
                BroadwebStatusSnapshot::idle()
            }

            fn dispatch_service_request(
                &self,
                _request: ServiceRequest,
            ) -> Result<ServiceResponse, BroadwebdError> {
                Err(BroadwebdError::UnsupportedRequest(
                    "empty client has no services".to_string(),
                ))
            }

            fn temporary_downloads(
                &self,
                _profile: &str,
            ) -> Result<Vec<TemporaryDownloadRecord>, BroadwebdError> {
                Ok(Vec::new())
            }

            fn downloads(
                &self,
                _profile: &str,
            ) -> Result<Vec<TemporaryDownloadRecord>, BroadwebdError> {
                Ok(Vec::new())
            }
        }

        let handler = EmptyClient;
        let mut registry = InProcessServiceFrameEndpointRegistry::new();
        let tcp_error = registry
            .register("/ip4/127.0.0.1/tcp/9443", &handler)
            .expect_err("TCP endpoints should stay on the TCP connector");
        assert!(
            tcp_error
                .to_string()
                .contains("does not model TCP endpoint")
        );

        let endpoint = "/ip4/127.0.0.1/tcp/9443/p2p/peer-a";
        let missing_error = match registry.connector(endpoint) {
            Ok(_) => panic!("unregistered endpoint should fail closed"),
            Err(error) => error,
        };
        assert!(
            missing_error
                .to_string()
                .contains("unregistered in-process service-frame endpoint")
        );

        registry
            .register(endpoint, &handler)
            .expect("register p2p endpoint");
        let duplicate_error = registry
            .register(endpoint, &handler)
            .expect_err("duplicate endpoints should be rejected");
        assert!(
            duplicate_error
                .to_string()
                .contains("endpoint is already registered")
        );
    }
}
