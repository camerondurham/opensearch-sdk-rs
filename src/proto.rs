// Generated protobuf code will be included here via build.rs
// The prost-build crate generates Rust code from .proto files during the build process

// Include the generated protobuf modules
// These are generated in OUT_DIR during build
pub mod extension_identity {
    include!(concat!(
        env!("OUT_DIR"),
        "/org.opensearch.extensions.proto.rs"
    ));
}

// Re-export commonly used types for convenience
pub use extension_identity::{
    ExtensionIdentity, ExtensionRequest, RegisterRestActions, RequestType,
};

use prost::Message;
use std::io::{Error, ErrorKind};

/// Parse an ExtensionIdentity message from bytes
pub fn parse_extension_identity(bytes: &[u8]) -> Result<ExtensionIdentity, Error> {
    ExtensionIdentity::decode(bytes).map_err(|e| {
        Error::new(
            ErrorKind::InvalidData,
            format!("Failed to decode ExtensionIdentity: {}", e),
        )
    })
}

/// Parse an ExtensionRequest message from bytes
pub fn parse_extension_request(bytes: &[u8]) -> Result<ExtensionRequest, Error> {
    ExtensionRequest::decode(bytes).map_err(|e| {
        Error::new(
            ErrorKind::InvalidData,
            format!("Failed to decode ExtensionRequest: {}", e),
        )
    })
}

/// Parse a RegisterRestActions message from bytes
pub fn parse_register_rest_actions(bytes: &[u8]) -> Result<RegisterRestActions, Error> {
    RegisterRestActions::decode(bytes).map_err(|e| {
        Error::new(
            ErrorKind::InvalidData,
            format!("Failed to decode RegisterRestActions: {}", e),
        )
    })
}

/// Serialize an ExtensionIdentity message to bytes
pub fn serialize_extension_identity(msg: &ExtensionIdentity) -> Vec<u8> {
    msg.encode_to_vec()
}

/// Serialize an ExtensionRequest message to bytes
pub fn serialize_extension_request(msg: &ExtensionRequest) -> Vec<u8> {
    msg.encode_to_vec()
}

/// Serialize a RegisterRestActions message to bytes
pub fn serialize_register_rest_actions(msg: &RegisterRestActions) -> Vec<u8> {
    msg.encode_to_vec()
}
