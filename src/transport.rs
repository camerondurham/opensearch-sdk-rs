use core::fmt;
use std::collections::HashMap;
use std::io::{Error, ErrorKind, Read, Write};
use std::net::TcpStream;

const MARKER_BYTES: &[u8; 2] = b"ES";
const REQUEST_ID_SIZE: usize = 8;
const STATUS_SIZE: usize = 1;
const VERSION_ID_SIZE: usize = 4;
const VARIABLE_HEADER_SIZE_FIELD: usize = 4;

#[derive(Debug, Clone)]
struct OSTransportHeaderError;

impl fmt::Display for OSTransportHeaderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid header prefix bytes")
    }
}

// Variable headers contain thread context and feature information
// Reference: https://github.com/opensearch-project/opensearch-sdk-py/blob/main/src/opensearch_sdk_py/transport/tcp_header.py
#[derive(Debug, Default)]
pub struct VariableHeaders {
    pub thread_context: HashMap<String, String>,
    pub features: Vec<String>,
    pub action: Option<String>,
}

impl VariableHeaders {
    /// Parse variable headers from the stream
    /// Format: number_of_headers (VInt), then for each header: key (String), value (String)
    /// Then features (VInt count, String array), then action (String)
    pub fn from_stream(stream: &mut TcpStream, size: u32) -> Result<Self, Error> {
        if size == 0 {
            return Ok(Self::default());
        }

        let mut headers = VariableHeaders::default();
        let mut buffer = vec![0u8; size as usize];
        stream.read_exact(&mut buffer)?;

        // For now, store the raw bytes for future parsing
        // Full implementation requires OpenSearch VInt and String parsing
        // which follows Java DataInput format

        // TODO: Implement proper VInt and String parsing based on OpenSearch format
        // References:
        // - https://github.com/opensearch-project/OpenSearch/blob/main/server/src/main/java/org/opensearch/common/io/stream/StreamInput.java
        // - https://github.com/opensearch-project/opensearch-sdk-py/blob/main/src/opensearch_sdk_py/transport/stream_input.py

        Ok(headers)
    }
}

// Reference: https://github.com/opensearch-project/opensearch-sdk-py/blob/main/src/opensearch_sdk_py/transport/tcp_header.py
#[derive(Debug)]
pub struct TransportTcpHeader {
    pub message_length: u32,
    pub request_id: u64,
    pub status: u8,
    pub version: u32,
    pub variable_header_size: u32,
}

// Complete transport message with header, variable headers, and body
#[derive(Debug)]
pub struct TransportMessage {
    pub header: TransportTcpHeader,
    pub variable_headers: VariableHeaders,
    pub body: Vec<u8>,
}

impl TransportMessage {
    /// Parse a complete transport message from the stream
    pub fn from_stream(stream: &mut TcpStream) -> Result<Self, Error> {
        // Parse fixed header
        let header = TransportTcpHeader::from_stream(stream)?;

        // Parse variable headers
        let variable_headers = VariableHeaders::from_stream(stream, header.variable_header_size)?;

        // Calculate body size: message_length includes request_id, status, version, variable_header_size
        // but not the message_length field itself
        let body_size = header
            .message_length
            .saturating_sub(REQUEST_ID_SIZE as u32)
            .saturating_sub(STATUS_SIZE as u32)
            .saturating_sub(VERSION_ID_SIZE as u32)
            .saturating_sub(VARIABLE_HEADER_SIZE_FIELD as u32)
            .saturating_sub(header.variable_header_size);

        // Read message body
        let mut body = vec![0u8; body_size as usize];
        if body_size > 0 {
            stream.read_exact(&mut body)?;
        }

        Ok(TransportMessage {
            header,
            variable_headers,
            body,
        })
    }

    pub fn is_handshake(&self) -> bool {
        self.header.is_handshake()
    }

    pub fn is_request_response(&self) -> bool {
        self.header.is_request_response()
    }

    /// Write a transport message to the stream
    /// This writes the complete message including headers and body
    pub fn write_to_stream(&self, stream: &mut TcpStream) -> Result<(), Error> {
        // Write prefix
        stream.write_all(MARKER_BYTES)?;

        // Write message length (all bytes after the length field)
        let message_length = REQUEST_ID_SIZE as u32
            + STATUS_SIZE as u32
            + VERSION_ID_SIZE as u32
            + VARIABLE_HEADER_SIZE_FIELD as u32
            + self.header.variable_header_size
            + self.body.len() as u32;
        stream.write_all(&message_length.to_be_bytes())?;

        // Write request ID
        stream.write_all(&self.header.request_id.to_be_bytes())?;

        // Write status
        stream.write_all(&[self.header.status])?;

        // Write version
        stream.write_all(&self.header.version.to_be_bytes())?;

        // Write variable header size
        stream.write_all(&self.header.variable_header_size.to_be_bytes())?;

        // TODO: Write actual variable headers when implemented
        // For now, variable_header_size should be 0

        // Write body
        stream.write_all(&self.body)?;

        stream.flush()?;
        Ok(())
    }

    /// Create a handshake response message
    pub fn create_handshake_response(request_id: u64, version: u32) -> Self {
        TransportMessage {
            header: TransportTcpHeader {
                message_length: 0, // Will be calculated in write_to_stream
                request_id,
                status: transport_status::STATUS_HANDSHAKE,
                version,
                variable_header_size: 0,
            },
            variable_headers: VariableHeaders::default(),
            body: Vec::new(),
        }
    }
}

// https://github.com/opensearch-project/opensearch-sdk-py/blob/main/src/opensearch_sdk_py/transport/transport_status.py#L9
pub mod transport_status {
    pub static STATUS_REQRES: u8 = 1 << 0;
    pub static STATUS_ERROR: u8 = 1 << 1;
    pub static STATUS_COMPRESS: u8 = 1 << 2;
    pub static STATUS_HANDSHAKE: u8 = 1 << 3;
}

// https://github.com/opensearch-project/opensearch-sdk-java/blob/main/DEVELOPER_GUIDE.md

// https://github.com/opensearch-project/opensearch-rs/blob/main/opensearch/src/http/transport.rs
impl TransportTcpHeader {
    pub fn new(
        request_id: u64,
        status: u8,
        version: u32,
        content_size: u32,
        variable_header_size: u32,
    ) -> Self {
        let message_length = content_size as usize
            + REQUEST_ID_SIZE
            + STATUS_SIZE
            + VERSION_ID_SIZE
            + variable_header_size as usize;
        Self {
            message_length: message_length
                .try_into()
                .expect("unable to convert into u32"),
            request_id,
            status,
            version,
            variable_header_size,
        }
    }

    pub fn is_handshake(&self) -> bool {
        self.status == transport_status::STATUS_HANDSHAKE
    }

    pub fn is_request_response(&self) -> bool {
        self.status == transport_status::STATUS_REQRES
    }

    pub fn is_error(&self) -> bool {
        self.status == transport_status::STATUS_ERROR
    }

    pub fn is_compressed(&self) -> bool {
        self.status == transport_status::STATUS_COMPRESS
    }

    // TODO: proper serialization/deserialization
    // https://github.com/thepacketgeek/rust-tcpstream-demo/tree/master/protocol

    // TODO: find how to simplify the byte reading (with nom??)
    pub fn from_stream(stream: &mut TcpStream) -> Result<Self, Error> {
        let mut prefix = [0u8; 2];
        match stream.read_exact(&mut prefix) {
            Ok(_) => {
                if &prefix != MARKER_BYTES {
                    return Err(Error::new(ErrorKind::InvalidData, "invalid header prefix"));
                }
            }
            Err(e) => {
                eprintln!("Unable to parse prefix");
                return Err(e);
            }
        }
        let mut size = [0u8; 4]; // parse bytes for an integer
        match stream.read_exact(&mut size) {
            Ok(_) => {}
            Err(e) => {
                eprintln!("Unable to parse size");
                return Err(e);
            }
        }
        let mut request_id = [0u8; 8];
        match stream.read_exact(&mut request_id) {
            Ok(_) => {}
            Err(e) => {
                eprintln!("Cannot parse request_id");
                return Err(e);
            }
        }
        let mut status = [0u8; 1];
        match stream.read_exact(&mut status) {
            Ok(_) => {}
            Err(e) => {
                eprintln!("Unable to parse status");
                return Err(e);
            }
        }
        let mut version = [0u8; 4];
        match stream.read_exact(&mut version) {
            Ok(_) => {}
            Err(e) => {
                eprintln!("Unbale to parse version");
                return Err(e);
            }
        }
        let mut variable_header_size = [0u8; 4];
        match stream.read_exact(&mut variable_header_size) {
            Ok(_) => {}
            Err(e) => {
                eprintln!("Unable to parse variable_header_size");
                return Err(e);
            }
        }

        let message_length = REQUEST_ID_SIZE as u32
            + STATUS_SIZE as u32
            + VERSION_ID_SIZE as u32
            + u32::from_be_bytes(variable_header_size);

        Ok(Self {
            request_id: u64::from_be_bytes(request_id),
            status: status[0],
            variable_header_size: u32::from_be_bytes(variable_header_size),
            version: u32::from_be_bytes(version),
            message_length,
        })
    }

    // TODO: rewrite for OpenSearch header purposes
    pub fn write(stream: &mut TcpStream) -> Result<(), Error> {
        // Create a buffer for the TCP header

        let mut header = [0; 20];

        // Set source and destination ports

        header[0..2].copy_from_slice(&0u16.to_be_bytes());

        header[2..4].copy_from_slice(&80u16.to_be_bytes());

        // Set sequence number

        header[4..8].copy_from_slice(&0u32.to_be_bytes());

        // Set acknowledgement number

        header[8..12].copy_from_slice(&0u32.to_be_bytes());

        // Set header length, flags, etc

        header[12] = 0x50; // Header length = 5 words, flags = ACK set

        // Calculate and set checksum (skipped for brevity)

        // Write the header to the stream

        stream.write_all(&header)?;

        Ok(())
    }

    /*
        public static void writeHeader(
        StreamOutput output,
        long requestId,
        byte status,
        Version version,
        int contentSize,
        int variableHeaderSize
    ) throws IOException {
        output.writeBytes(PREFIX);
        // write the size, the size indicates the remaining message size, not including the size int
        output.writeInt(contentSize + REQUEST_ID_SIZE + STATUS_SIZE + VERSION_ID_SIZE + VARIABLE_HEADER_SIZE);
        output.writeLong(requestId);
        output.writeByte(status);
        output.writeInt(version.id);
        assert variableHeaderSize != -1 : "Variable header size not set";
        output.writeInt(variableHeaderSize);
    }
    */
}

mod test {
    #[cfg(test)]
    fn test_from_tcp_stream() {
        use std::net::TcpStream;
    }
}
