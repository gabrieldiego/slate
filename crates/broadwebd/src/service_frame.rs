use crate::{BroadwebdError, ServiceRequest, ServiceResponse};
use serde::{Serialize, de::DeserializeOwned};
use std::io::{self, Write};

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
}

impl Default for ServiceFrameCodec {
    fn default() -> Self {
        Self::new(DEFAULT_SERVICE_FRAME_MAX_BYTES)
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
