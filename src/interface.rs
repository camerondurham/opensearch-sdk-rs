use byteorder::{NetworkEndian, ReadBytesExt, WriteBytesExt};
use std::io::{self, BufRead, BufReader, Read, Write};
// TODO: rewrite using Serde

const TAG_BYTES: &[u8; 2] = b"ES";

pub trait Serialize {
    /// Serialize to a `Write`able buffer
    fn serialize(&self, buf: &mut impl Write) -> io::Result<usize>;
}

pub trait Deserialize {
    type Output;
    /// Deserialize from a `Read`able buffer
    fn deserialize(buf: &mut impl Read) -> io::Result<Self::Output>;
}

/// Request object (client -> server)
/// Reference: https://github.com/opensearch-project/opensearch-sdk-py/blob/main/src/opensearch_sdk_py/transport/transport_status.py#L9
#[derive(Debug)]
pub enum Request {
    RequestResponse(String),
    TransportError(String),
    Compress(String),
    Handshake(String),
}

/// Encode the request type as a single byte (as long as we don't exceed 255 types)
///
/// We use `&Request` since we don't actually need to own or mutat the request fields
impl From<&Request> for u8 {
    fn from(req: &Request) -> Self {
        match req {
            Request::RequestResponse(_) => 1 << 0,
            Request::TransportError(_) => 1 << 1,
            Request::Compress(_) => 1 << 2,
            Request::Handshake(_) => 1 << 3,
        }
    }
}

impl Serialize for Request {
    /// Serialize Request to bytes to send to OpenSearch server
    fn serialize(&self, buf: &mut impl Write) -> io::Result<usize> {
        let mut bytes_written = 0;

        // Write request type byte
        buf.write_u8(self.into())?;
        bytes_written += 1;

        // Write the message content (length-prefixed string)
        let content = match self {
            Request::RequestResponse(s) => s,
            Request::TransportError(s) => s,
            Request::Compress(s) => s,
            Request::Handshake(s) => s,
        };

        // Write string length as 4-byte big-endian integer
        let len = content.len() as u32;
        buf.write_u32::<NetworkEndian>(len)?;
        bytes_written += 4;

        // Write string bytes
        buf.write_all(content.as_bytes())?;
        bytes_written += content.len();

        Ok(bytes_written)
    }
}

impl Deserialize for Request {
    type Output = Request;

    /// Deserialize Request from bytes (to receive from TcpStream)
    fn deserialize(buf: &mut impl Read) -> io::Result<Self::Output> {
        // Read request type byte
        let request_type = buf.read_u8()?;

        // Read string length (4-byte big-endian)
        let length = buf.read_u32::<NetworkEndian>()?;

        // Read string content
        let mut content_bytes = vec![0u8; length as usize];
        buf.read_exact(&mut content_bytes)?;

        let content = String::from_utf8(content_bytes).map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Invalid UTF-8 in request content: {}", e),
            )
        })?;

        // Match request type byte to enum variant
        let request = match request_type {
            t if t == (1 << 0) => Request::RequestResponse(content),
            t if t == (1 << 1) => Request::TransportError(content),
            t if t == (1 << 2) => Request::Compress(content),
            t if t == (1 << 3) => Request::Handshake(content),
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("Invalid request type byte: {}", request_type),
                ))
            }
        };

        Ok(request)
    }
}
