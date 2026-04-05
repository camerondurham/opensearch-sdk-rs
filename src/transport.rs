use crate::rest::{ExtensionRestRequest, ExtensionRestResponse, HttpVersion, RestMethod};
use crate::stream::{StreamInput, StreamOutput};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::io::{self, Read, Write};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

const MARKER_BYTES: &[u8; 2] = b"ES";
const MASK: u32 = 0x0800_0000;
const FIXED_HEADER_MESSAGE_BYTES: usize = 8 + 1 + 4 + 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Version {
    encoded_id: u32,
}

impl Version {
    pub const CURRENT_RELEASE_ID: u32 = 3_060_099;
    pub const MIN_COMPAT_RELEASE_ID: u32 = 2_190_099;
    pub const STREAM_ADDRESS_RELEASE_ID: u32 = 3_020_099;

    pub fn current() -> Self {
        Self::from_release_id(Self::CURRENT_RELEASE_ID)
    }

    pub fn min_compat() -> Self {
        Self::from_release_id(Self::MIN_COMPAT_RELEASE_ID)
    }

    pub fn from_release_id(release_id: u32) -> Self {
        Self {
            encoded_id: release_id ^ MASK,
        }
    }

    pub fn from_encoded_id(encoded_id: u32) -> Self {
        Self { encoded_id }
    }

    pub fn encoded_id(self) -> u32 {
        self.encoded_id
    }

    pub fn write_header_bytes(self, output: &mut StreamOutput) {
        output.write_u32(self.encoded_id);
    }

    pub fn write_to_stream(self, output: &mut StreamOutput) {
        output.write_vint(self.encoded_id);
    }

    pub fn read_from_stream(input: &mut StreamInput<'_>) -> io::Result<Self> {
        Ok(Self::from_encoded_id(input.read_vint()?))
    }

    pub fn release_id(self) -> u32 {
        self.encoded_id & !MASK
    }

    pub fn on_or_after_release_id(self, release_id: u32) -> bool {
        self.release_id() >= release_id
    }
}

impl fmt::Display for Version {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let release_id = self.release_id();
        let major = (release_id / 1_000_000) % 100;
        let minor = (release_id / 10_000) % 100;
        let revision = (release_id / 100) % 100;
        let build = release_id % 100;
        write!(formatter, "{major}.{minor}.{revision}.{build}")
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ThreadContext {
    pub request_headers: BTreeMap<String, String>,
    pub response_headers: BTreeMap<String, BTreeSet<String>>,
}

impl ThreadContext {
    pub fn read_from(input: &mut StreamInput<'_>) -> io::Result<Self> {
        Ok(Self {
            request_headers: input.read_string_map()?,
            response_headers: input.read_string_set_map()?,
        })
    }

    pub fn write_to(&self, output: &mut StreamOutput) {
        output.write_string_map(&self.request_headers);
        output.write_string_set_map(&self.response_headers);
    }
}

pub mod transport_status {
    pub const STATUS_REQRES: u8 = 1 << 0;
    pub const STATUS_ERROR: u8 = 1 << 1;
    pub const STATUS_COMPRESS: u8 = 1 << 2;
    pub const STATUS_HANDSHAKE: u8 = 1 << 3;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransportTcpHeader {
    pub message_length: u32,
    pub request_id: u64,
    pub status: u8,
    pub version: Version,
    pub variable_header_size: u32,
}

impl TransportTcpHeader {
    pub fn is_request(&self) -> bool {
        (self.status & transport_status::STATUS_REQRES) == 0
    }

    pub fn is_response(&self) -> bool {
        !self.is_request()
    }

    pub fn is_handshake(&self) -> bool {
        (self.status & transport_status::STATUS_HANDSHAKE) != 0
    }

    pub fn is_error(&self) -> bool {
        (self.status & transport_status::STATUS_ERROR) != 0
    }

    pub fn is_compressed(&self) -> bool {
        (self.status & transport_status::STATUS_COMPRESS) != 0
    }

    pub fn read_from<R: Read>(reader: &mut R) -> io::Result<Self> {
        let mut prefix = [0u8; 2];
        reader.read_exact(&mut prefix)?;
        if &prefix != MARKER_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid transport header prefix",
            ));
        }

        let mut bytes = [0u8; 4];
        reader.read_exact(&mut bytes)?;
        let message_length = u32::from_be_bytes(bytes);

        let mut bytes = [0u8; 8];
        reader.read_exact(&mut bytes)?;
        let request_id = u64::from_be_bytes(bytes);

        let mut status = [0u8; 1];
        reader.read_exact(&mut status)?;

        let mut bytes = [0u8; 4];
        reader.read_exact(&mut bytes)?;
        let version = Version::from_encoded_id(u32::from_be_bytes(bytes));

        let mut bytes = [0u8; 4];
        reader.read_exact(&mut bytes)?;
        let variable_header_size = u32::from_be_bytes(bytes);

        Ok(Self {
            message_length,
            request_id,
            status: status[0],
            version,
            variable_header_size,
        })
    }

    pub fn write_to(&self, output: &mut StreamOutput) {
        output.write_bytes(MARKER_BYTES);
        output.write_u32(self.message_length);
        output.write_u64(self.request_id);
        output.write_u8(self.status);
        self.version.write_header_bytes(output);
        output.write_u32(self.variable_header_size);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageFrame {
    pub header: TransportTcpHeader,
    pub thread_context: ThreadContext,
    pub features: Vec<String>,
    pub action: Option<String>,
    pub body: Vec<u8>,
}

impl MessageFrame {
    pub fn request(
        request_id: u64,
        version: Version,
        thread_context: ThreadContext,
        features: Vec<String>,
        action: String,
        body: Vec<u8>,
        is_handshake: bool,
    ) -> Self {
        let mut variable = StreamOutput::new();
        thread_context.write_to(&mut variable);
        variable.write_string_array(&features);
        variable.write_string(&action);
        let variable_bytes = variable.into_bytes();

        Self {
            header: TransportTcpHeader {
                message_length: (FIXED_HEADER_MESSAGE_BYTES + variable_bytes.len() + body.len())
                    as u32,
                request_id,
                status: if is_handshake {
                    transport_status::STATUS_HANDSHAKE
                } else {
                    0
                },
                version,
                variable_header_size: variable_bytes.len() as u32,
            },
            thread_context,
            features,
            action: Some(action),
            body,
        }
    }

    pub fn response(
        request_id: u64,
        version: Version,
        thread_context: ThreadContext,
        body: Vec<u8>,
        is_handshake: bool,
        is_error: bool,
    ) -> Self {
        let mut variable = StreamOutput::new();
        thread_context.write_to(&mut variable);
        let variable_bytes = variable.into_bytes();

        let mut status = transport_status::STATUS_REQRES;
        if is_handshake {
            status |= transport_status::STATUS_HANDSHAKE;
        }
        if is_error {
            status |= transport_status::STATUS_ERROR;
        }

        Self {
            header: TransportTcpHeader {
                message_length: (FIXED_HEADER_MESSAGE_BYTES + variable_bytes.len() + body.len())
                    as u32,
                request_id,
                status,
                version,
                variable_header_size: variable_bytes.len() as u32,
            },
            thread_context,
            features: Vec::new(),
            action: None,
            body,
        }
    }

    pub fn read_from<R: Read>(reader: &mut R) -> io::Result<Self> {
        let header = TransportTcpHeader::read_from(reader)?;
        let remaining = header.message_length as usize - FIXED_HEADER_MESSAGE_BYTES;
        let mut remaining_bytes = vec![0u8; remaining];
        reader.read_exact(&mut remaining_bytes)?;

        let variable_len = header.variable_header_size as usize;
        if variable_len > remaining_bytes.len() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "variable header is longer than the frame body",
            ));
        }

        let (variable_bytes, body) = remaining_bytes.split_at(variable_len);
        let mut variable_input = StreamInput::new(variable_bytes);
        let thread_context = ThreadContext::read_from(&mut variable_input)?;
        let mut features = Vec::new();
        let mut action = None;

        if header.is_request() && variable_input.remaining() > 0 {
            features = variable_input.read_string_array()?;
            action = Some(variable_input.read_string()?);
        }

        Ok(Self {
            header,
            thread_context,
            features,
            action,
            body: body.to_vec(),
        })
    }

    pub fn write_to<W: Write>(&self, writer: &mut W) -> io::Result<()> {
        let bytes = self.to_bytes();
        writer.write_all(&bytes)
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut variable = StreamOutput::new();
        self.thread_context.write_to(&mut variable);
        if self.header.is_request() {
            variable.write_string_array(&self.features);
            variable.write_string(self.action.as_deref().unwrap_or_default());
        }
        let variable_bytes = variable.into_bytes();

        let mut header = self.header.clone();
        header.variable_header_size = variable_bytes.len() as u32;
        header.message_length =
            (FIXED_HEADER_MESSAGE_BYTES + variable_bytes.len() + self.body.len()) as u32;

        let mut output = StreamOutput::new();
        header.write_to(&mut output);
        output.write_bytes(&variable_bytes);
        output.write_bytes(&self.body);
        output.into_bytes()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskId {
    pub node_id: String,
    pub id: Option<i64>,
}

impl Default for TaskId {
    fn default() -> Self {
        Self {
            node_id: String::new(),
            id: None,
        }
    }
}

impl TaskId {
    pub fn read_from(input: &mut StreamInput<'_>) -> io::Result<Self> {
        let node_id = input.read_string()?;
        let id = if node_id.is_empty() {
            None
        } else {
            Some(input.read_i64()?)
        };
        Ok(Self { node_id, id })
    }

    pub fn write_to(&self, output: &mut StreamOutput) {
        output.write_string(&self.node_id);
        if let Some(id) = self.id {
            output.write_i64(id);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransportAddress {
    pub address: IpAddr,
    pub host_name: String,
    pub port: i32,
}

impl TransportAddress {
    pub fn new(address: IpAddr, port: i32) -> Self {
        Self {
            host_name: address.to_string(),
            address,
            port,
        }
    }

    pub fn read_from(input: &mut StreamInput<'_>) -> io::Result<Self> {
        let len = input.read_u8()? as usize;
        let bytes = input.read_bytes(len)?;
        let host_name = input.read_string()?;
        let port = input.read_i32()?;
        let address = match len {
            4 => IpAddr::V4(Ipv4Addr::new(bytes[0], bytes[1], bytes[2], bytes[3])),
            16 => {
                let mut octets = [0u8; 16];
                octets.copy_from_slice(&bytes);
                IpAddr::V6(Ipv6Addr::from(octets))
            }
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "unsupported transport address length",
                ))
            }
        };

        Ok(Self {
            address,
            host_name,
            port,
        })
    }

    pub fn write_to(&self, output: &mut StreamOutput) {
        let bytes = match self.address {
            IpAddr::V4(address) => address.octets().to_vec(),
            IpAddr::V6(address) => address.octets().to_vec(),
        };
        output.write_u8(bytes.len() as u8);
        output.write_bytes(&bytes);
        output.write_string(&self.host_name);
        output.write_i32(self.port);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveryNodeRole {
    pub name: String,
    pub abbreviation: String,
    pub can_contain_data: bool,
}

impl DiscoveryNodeRole {
    pub fn cluster_manager() -> Self {
        Self {
            name: "cluster_manager".into(),
            abbreviation: "m".into(),
            can_contain_data: false,
        }
    }

    pub fn data() -> Self {
        Self {
            name: "data".into(),
            abbreviation: "d".into(),
            can_contain_data: true,
        }
    }

    pub fn ingest() -> Self {
        Self {
            name: "ingest".into(),
            abbreviation: "i".into(),
            can_contain_data: false,
        }
    }

    pub fn remote_cluster_client() -> Self {
        Self {
            name: "remote_cluster_client".into(),
            abbreviation: "r".into(),
            can_contain_data: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveryNode {
    pub node_name: String,
    pub node_id: String,
    pub ephemeral_id: String,
    pub host_name: String,
    pub host_address: String,
    pub address: TransportAddress,
    pub stream_address: Option<TransportAddress>,
    pub attributes: BTreeMap<String, String>,
    pub roles: Vec<DiscoveryNodeRole>,
    pub version: Version,
}

impl DiscoveryNode {
    pub fn read_from(input: &mut StreamInput<'_>) -> io::Result<Self> {
        Self::read_from_with_transport_version(input, Version::min_compat())
    }

    pub fn read_from_with_transport_version(
        input: &mut StreamInput<'_>,
        transport_version: Version,
    ) -> io::Result<Self> {
        let node_name = input.read_string()?;
        let node_id = input.read_string()?;
        let ephemeral_id = input.read_string()?;
        let host_name = input.read_string()?;
        let host_address = input.read_string()?;
        let address = TransportAddress::read_from(input)?;
        let stream_address =
            if transport_version.on_or_after_release_id(Version::STREAM_ADDRESS_RELEASE_ID) {
                if input.read_bool()? {
                    Some(TransportAddress::read_from(input)?)
                } else {
                    None
                }
            } else {
                None
            };
        let attributes = input.read_string_map()?;
        let roles_len = input.read_vint()? as usize;
        let mut roles = Vec::with_capacity(roles_len);
        for _ in 0..roles_len {
            roles.push(DiscoveryNodeRole {
                name: input.read_string()?,
                abbreviation: input.read_string()?,
                can_contain_data: input.read_bool()?,
            });
        }

        Ok(Self {
            node_name,
            node_id,
            ephemeral_id,
            host_name,
            host_address,
            address,
            stream_address,
            attributes,
            roles,
            version: Version::read_from_stream(input)?,
        })
    }

    pub fn write_to(&self, output: &mut StreamOutput) {
        self.write_to_with_transport_version(output, Version::current());
    }

    pub fn write_to_with_transport_version(
        &self,
        output: &mut StreamOutput,
        transport_version: Version,
    ) {
        output.write_string(&self.node_name);
        output.write_string(&self.node_id);
        output.write_string(&self.ephemeral_id);
        output.write_string(&self.host_name);
        output.write_string(&self.host_address);
        self.address.write_to(output);
        if transport_version.on_or_after_release_id(Version::STREAM_ADDRESS_RELEASE_ID) {
            output.write_bool(self.stream_address.is_some());
            if let Some(stream_address) = &self.stream_address {
                stream_address.write_to(output);
            }
        }
        output.write_string_map(&self.attributes);
        output.write_vint(self.roles.len() as u32);
        for role in &self.roles {
            output.write_string(&role.name);
            output.write_string(&role.abbreviation);
            output.write_bool(role.can_contain_data);
        }
        self.version.write_to_stream(output);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtensionDependency {
    pub unique_id: String,
    pub version: Version,
}

impl ExtensionDependency {
    pub fn read_from(input: &mut StreamInput<'_>) -> io::Result<Self> {
        Ok(Self {
            unique_id: input.read_string()?,
            version: Version::read_from_stream(input)?,
        })
    }

    pub fn write_to(&self, output: &mut StreamOutput) {
        output.write_string(&self.unique_id);
        self.version.write_to_stream(output);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveryExtensionNode {
    pub discovery_node: DiscoveryNode,
    pub minimum_compatible_version: Version,
    pub dependencies: Vec<ExtensionDependency>,
}

impl DiscoveryExtensionNode {
    pub fn read_from(input: &mut StreamInput<'_>) -> io::Result<Self> {
        Self::read_from_with_transport_version(input, Version::min_compat())
    }

    pub fn read_from_with_transport_version(
        input: &mut StreamInput<'_>,
        transport_version: Version,
    ) -> io::Result<Self> {
        let discovery_node =
            DiscoveryNode::read_from_with_transport_version(input, transport_version)?;
        let minimum_compatible_version = Version::read_from_stream(input)?;
        let dependency_len = input.read_vint()? as usize;
        let mut dependencies = Vec::with_capacity(dependency_len);
        for _ in 0..dependency_len {
            dependencies.push(ExtensionDependency::read_from(input)?);
        }
        Ok(Self {
            discovery_node,
            minimum_compatible_version,
            dependencies,
        })
    }

    pub fn write_to(&self, output: &mut StreamOutput) {
        self.write_to_with_transport_version(output, Version::current());
    }

    pub fn write_to_with_transport_version(
        &self,
        output: &mut StreamOutput,
        transport_version: Version,
    ) {
        self.discovery_node
            .write_to_with_transport_version(output, transport_version);
        self.minimum_compatible_version.write_to_stream(output);
        output.write_vint(self.dependencies.len() as u32);
        for dependency in &self.dependencies {
            dependency.write_to(output);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitializeExtensionRequest {
    pub source_node: DiscoveryNode,
    pub extension: DiscoveryExtensionNode,
    pub service_account_header: String,
}

impl InitializeExtensionRequest {
    pub fn read_from(input: &mut StreamInput<'_>) -> io::Result<Self> {
        Self::read_from_with_transport_version(input, Version::min_compat())
    }

    pub fn read_from_with_transport_version(
        input: &mut StreamInput<'_>,
        transport_version: Version,
    ) -> io::Result<Self> {
        Ok(Self {
            source_node: DiscoveryNode::read_from_with_transport_version(input, transport_version)?,
            extension: DiscoveryExtensionNode::read_from_with_transport_version(
                input,
                transport_version,
            )?,
            service_account_header: input.read_string()?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitializeExtensionResponse {
    pub name: String,
    pub implemented_interfaces: Vec<String>,
}

impl InitializeExtensionResponse {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut output = StreamOutput::new();
        output.write_string(&self.name);
        output.write_string_array(&self.implemented_interfaces);
        output.into_bytes()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestType {
    ClusterState = 0,
    ClusterSettings = 1,
    RegisterRestActions = 2,
    RegisterSettings = 3,
    EnvironmentSettings = 4,
    DependencyInformation = 5,
    CreateComponent = 6,
    OnIndexModule = 7,
    GetSettings = 8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtensionRequest {
    pub request_type: RequestType,
    pub unique_id: Option<String>,
}

impl ExtensionRequest {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut output = StreamOutput::new();
        TaskId::default().write_to(&mut output);
        let proto = encode_extension_request(self.unique_id.as_deref(), self.request_type as u32);
        output.write_byte_array(&proto);
        output.into_bytes()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisterRestActionsRequest {
    pub unique_id: String,
    pub rest_actions: Vec<String>,
    pub deprecated_rest_actions: Vec<String>,
}

impl RegisterRestActionsRequest {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut output = StreamOutput::new();
        TaskId::default().write_to(&mut output);
        let proto = encode_register_rest_actions(
            &self.unique_id,
            &self.rest_actions,
            &self.deprecated_rest_actions,
        );
        output.write_byte_array(&proto);
        output.into_bytes()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransportHandshakerHandshakeResponse {
    pub version: Version,
}

impl TransportHandshakerHandshakeResponse {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut output = StreamOutput::new();
        self.version.write_to_stream(&mut output);
        output.into_bytes()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransportServiceHandshakeResponse {
    pub discovery_node: Option<DiscoveryNode>,
    pub cluster_name: String,
    pub version: Version,
}

impl TransportServiceHandshakeResponse {
    pub fn to_bytes(&self) -> Vec<u8> {
        self.to_bytes_with_transport_version(Version::current())
    }

    pub fn to_bytes_with_transport_version(&self, transport_version: Version) -> Vec<u8> {
        let mut output = StreamOutput::new();
        output.write_bool(self.discovery_node.is_some());
        if let Some(node) = &self.discovery_node {
            node.write_to_with_transport_version(&mut output, transport_version);
        }
        output.write_string(&self.cluster_name);
        self.version.write_to_stream(&mut output);
        output.into_bytes()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcknowledgedResponse {
    pub acknowledged: bool,
}

impl AcknowledgedResponse {
    pub fn read_from(bytes: &[u8]) -> io::Result<Self> {
        let mut input = StreamInput::new(bytes);
        Ok(Self {
            acknowledged: input.read_bool()?,
        })
    }
}

pub fn decode_extension_rest_request(bytes: &[u8]) -> io::Result<ExtensionRestRequest> {
    let mut input = StreamInput::new(bytes);
    let _task_id = TaskId::read_from(&mut input)?;
    let method = RestMethod::from_wire(input.read_vint()?)?;
    let uri = input.read_string()?;
    let path = input.read_string()?;
    let params = input.read_string_map()?;
    let headers = input.read_string_list_map()?;
    let media_type = if input.read_bool()? {
        Some(input.read_string()?)
    } else {
        None
    };
    let content = input.read_byte_array()?;
    let principal_identifier_token = input.read_string()?;
    let http_version = HttpVersion::from_wire(input.read_vint()?)?;

    Ok(ExtensionRestRequest::new(
        method,
        uri,
        path,
        params,
        headers,
        media_type,
        content,
        principal_identifier_token,
        http_version,
    ))
}

pub fn encode_extension_rest_response(response: &ExtensionRestResponse) -> Vec<u8> {
    let mut output = StreamOutput::new();
    output.write_vint(response.status.to_wire());
    output.write_string(&response.content_type);
    output.write_byte_array(&response.content);
    output.write_string_list_map(&response.headers);
    let consumed = response
        .consumed_params
        .iter()
        .cloned()
        .collect::<Vec<String>>();
    output.write_string_array(&consumed);
    output.write_bool(response.content_consumed);
    output.into_bytes()
}

fn encode_extension_identity(unique_id: &str) -> Vec<u8> {
    let mut out = Vec::new();
    encode_protobuf_field(
        1,
        2,
        &encode_length_delimited(unique_id.as_bytes()),
        &mut out,
    );
    out
}

fn encode_extension_request(unique_id: Option<&str>, request_type: u32) -> Vec<u8> {
    let mut out = Vec::new();
    if let Some(unique_id) = unique_id {
        let identity = encode_extension_identity(unique_id);
        encode_protobuf_field(1, 2, &encode_length_delimited(&identity), &mut out);
    }
    encode_protobuf_field(2, 0, &encode_varint(request_type as u64), &mut out);
    out
}

fn encode_register_rest_actions(
    unique_id: &str,
    rest_actions: &[String],
    deprecated_rest_actions: &[String],
) -> Vec<u8> {
    let mut out = Vec::new();
    let identity = encode_extension_identity(unique_id);
    encode_protobuf_field(1, 2, &encode_length_delimited(&identity), &mut out);
    for action in rest_actions {
        encode_protobuf_field(2, 2, &encode_length_delimited(action.as_bytes()), &mut out);
    }
    for action in deprecated_rest_actions {
        encode_protobuf_field(3, 2, &encode_length_delimited(action.as_bytes()), &mut out);
    }
    out
}

fn encode_protobuf_field(tag: u32, wire_type: u8, value: &[u8], out: &mut Vec<u8>) {
    out.extend_from_slice(&encode_varint(((tag << 3) | wire_type as u32) as u64));
    out.extend_from_slice(value);
}

fn encode_length_delimited(value: &[u8]) -> Vec<u8> {
    let mut out = encode_varint(value.len() as u64);
    out.extend_from_slice(value);
    out
}

fn encode_varint(mut value: u64) -> Vec<u8> {
    let mut out = Vec::new();
    loop {
        if (value & !0x7F) == 0 {
            out.push(value as u8);
            return out;
        }
        out.push(((value & 0x7F) as u8) | 0x80);
        value >>= 7;
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AcknowledgedResponse, DiscoveryNode, DiscoveryNodeRole, ExtensionRequest,
        InitializeExtensionRequest, InitializeExtensionResponse, MessageFrame,
        RegisterRestActionsRequest, RequestType, TaskId, ThreadContext, TransportAddress,
        TransportHandshakerHandshakeResponse, TransportServiceHandshakeResponse, Version,
    };
    use crate::rest::{ExtensionRestResponse, RestStatus};
    use std::collections::{BTreeMap, BTreeSet};
    use std::net::{IpAddr, Ipv4Addr};

    fn test_node() -> DiscoveryNode {
        DiscoveryNode {
            node_name: "hello-world-rs".into(),
            node_id: "hello-world-rs".into(),
            ephemeral_id: "hello-world-rs-ephemeral".into(),
            host_name: "127.0.0.1".into(),
            host_address: "127.0.0.1".into(),
            address: TransportAddress::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 1234),
            stream_address: None,
            attributes: BTreeMap::new(),
            roles: vec![
                DiscoveryNodeRole::cluster_manager(),
                DiscoveryNodeRole::data(),
                DiscoveryNodeRole::ingest(),
                DiscoveryNodeRole::remote_cluster_client(),
            ],
            version: Version::current(),
        }
    }

    #[test]
    fn message_frame_round_trips() {
        let mut thread_context = ThreadContext::default();
        thread_context
            .request_headers
            .insert("_system_index_access_allowed".into(), "false".into());
        thread_context.response_headers.insert(
            "warnings".into(),
            BTreeSet::from(["be careful".to_string()]),
        );

        let frame = MessageFrame::request(
            42,
            Version::min_compat(),
            thread_context,
            vec!["feature-a".into()],
            "internal:tcp/handshake".into(),
            vec![1, 2, 3],
            true,
        );
        let bytes = frame.to_bytes();
        let parsed = MessageFrame::read_from(&mut bytes.as_slice()).unwrap();

        assert_eq!(parsed.header.request_id, 42);
        assert!(parsed.header.is_handshake());
        assert_eq!(parsed.action.as_deref(), Some("internal:tcp/handshake"));
        assert_eq!(parsed.features, vec!["feature-a".to_string()]);
        assert_eq!(parsed.body, vec![1, 2, 3]);
    }

    #[test]
    fn protobuf_backed_requests_are_framed_as_transport_messages() {
        let register = RegisterRestActionsRequest {
            unique_id: "hello-world-rs".into(),
            rest_actions: vec!["GET /hello hello_world_rs:hello".into()],
            deprecated_rest_actions: Vec::new(),
        };
        let register_bytes = register.to_bytes();
        let mut input = crate::stream::StreamInput::new(&register_bytes);
        let task_id = TaskId::read_from(&mut input).unwrap();
        assert!(task_id.node_id.is_empty());
        assert!(input.read_byte_array().unwrap().starts_with(&[10]));

        let env_request = ExtensionRequest {
            request_type: RequestType::EnvironmentSettings,
            unique_id: None,
        };
        let env_bytes = env_request.to_bytes();
        let mut input = crate::stream::StreamInput::new(&env_bytes);
        let task_id = TaskId::read_from(&mut input).unwrap();
        assert!(task_id.node_id.is_empty());
        assert_eq!(input.read_byte_array().unwrap()[0], 16);
    }

    #[test]
    fn initialize_extension_request_round_trips() {
        let node = test_node();
        let extension_node = super::DiscoveryExtensionNode {
            discovery_node: node.clone(),
            minimum_compatible_version: Version::from_release_id(3_000_000),
            dependencies: Vec::new(),
        };

        let mut output = crate::stream::StreamOutput::new();
        TaskId::default().write_to(&mut output);
        node.write_to(&mut output);
        extension_node.write_to(&mut output);
        output.write_string("service-token");

        let bytes = output.into_bytes();
        let mut input = crate::stream::StreamInput::new(&bytes);
        let _task_id = TaskId::read_from(&mut input).unwrap();
        let parsed = InitializeExtensionRequest::read_from_with_transport_version(
            &mut input,
            Version::current(),
        )
        .unwrap();

        assert_eq!(parsed.source_node.node_id, "hello-world-rs");
        assert_eq!(parsed.service_account_header, "service-token");
    }

    #[test]
    fn transport_responses_encode_expected_payloads() {
        let tcp = TransportHandshakerHandshakeResponse {
            version: Version::current(),
        };
        assert!(!tcp.to_bytes().is_empty());

        let transport = TransportServiceHandshakeResponse {
            discovery_node: Some(test_node()),
            cluster_name: String::new(),
            version: Version::current(),
        };
        assert!(transport.to_bytes().len() > tcp.to_bytes().len());

        let init = InitializeExtensionResponse {
            name: "Hello World".into(),
            implemented_interfaces: vec!["ActionExtension".into()],
        };
        assert!(!init.to_bytes().is_empty());
    }

    #[test]
    fn acknowledged_response_reads_single_boolean() {
        let bytes = vec![1u8];
        assert!(
            AcknowledgedResponse::read_from(&bytes)
                .unwrap()
                .acknowledged
        );
    }

    #[test]
    fn rest_response_payload_starts_with_status_ordinal() {
        let response = ExtensionRestResponse::text(RestStatus::Ok, "hello");
        let bytes = super::encode_extension_rest_response(&response);
        let mut input = crate::stream::StreamInput::new(&bytes);
        assert_eq!(input.read_vint().unwrap(), 2);
    }
}
