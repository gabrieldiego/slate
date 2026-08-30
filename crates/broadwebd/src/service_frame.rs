use crate::{
    daemon::BroadwebdClient,
    error::BroadwebdError,
    health::{DaemonHealth, DaemonLifecycle},
    http::{ServiceRequest, ServiceResponse},
    state::TemporaryDownloadRecord,
    status::BroadwebStatusSnapshot,
};
use serde::{Serialize, de::DeserializeOwned};
use std::io::{self, Cursor, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
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

pub fn dispatch_service_frame_request_with_connector<C: ServiceFrameConnector>(
    codec: ServiceFrameCodec,
    connector: &C,
    request: &ServiceRequest,
) -> Result<ServiceResponse, BroadwebdError> {
    let mut stream = connector.connect(codec)?;
    dispatch_service_frame_request_over_stream(codec, &mut stream, request)
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
        let mut addresses = self.address.to_socket_addrs()?;
        let address = addresses.next().ok_or_else(|| {
            BroadwebdError::Request(format!(
                "TCP service-frame endpoint resolved no socket addresses: {}",
                self.address
            ))
        })?;
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
}
