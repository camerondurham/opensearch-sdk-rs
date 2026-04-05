use std::collections::{BTreeMap, BTreeSet};
use std::io;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RestMethod {
    Get,
    Post,
    Put,
    Delete,
    Options,
    Head,
    Patch,
    Trace,
    Connect,
}

impl RestMethod {
    pub fn from_wire(value: u32) -> io::Result<Self> {
        match value {
            0 => Ok(Self::Get),
            1 => Ok(Self::Post),
            2 => Ok(Self::Put),
            3 => Ok(Self::Delete),
            4 => Ok(Self::Options),
            5 => Ok(Self::Head),
            6 => Ok(Self::Patch),
            7 => Ok(Self::Trace),
            8 => Ok(Self::Connect),
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("unknown REST method ordinal {value}"),
            )),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Delete => "DELETE",
            Self::Options => "OPTIONS",
            Self::Head => "HEAD",
            Self::Patch => "PATCH",
            Self::Trace => "TRACE",
            Self::Connect => "CONNECT",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpVersion {
    Http10,
    Http11,
}

impl HttpVersion {
    pub fn from_wire(value: u32) -> io::Result<Self> {
        match value {
            0 => Ok(Self::Http10),
            1 => Ok(Self::Http11),
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("unknown HTTP version ordinal {value}"),
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestStatus {
    Ok,
    Accepted,
    NotFound,
    InternalServerError,
}

impl RestStatus {
    pub fn to_wire(self) -> u32 {
        match self {
            Self::Ok => 2,
            Self::Accepted => 4,
            Self::NotFound => 21,
            Self::InternalServerError => 40,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtensionRestRequest {
    pub method: RestMethod,
    pub uri: String,
    pub path: String,
    pub params: BTreeMap<String, String>,
    pub headers: BTreeMap<String, Vec<String>>,
    pub media_type: Option<String>,
    pub content: Vec<u8>,
    pub principal_identifier_token: String,
    pub http_version: HttpVersion,
    pub consumed_params: BTreeSet<String>,
    pub content_consumed: bool,
}

impl ExtensionRestRequest {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        method: RestMethod,
        uri: String,
        path: String,
        params: BTreeMap<String, String>,
        headers: BTreeMap<String, Vec<String>>,
        media_type: Option<String>,
        content: Vec<u8>,
        principal_identifier_token: String,
        http_version: HttpVersion,
    ) -> Self {
        Self {
            method,
            uri,
            path,
            params,
            headers,
            media_type,
            content,
            principal_identifier_token,
            http_version,
            consumed_params: BTreeSet::new(),
            content_consumed: false,
        }
    }

    pub fn route_key(&self) -> String {
        format!("{} {}", self.method.as_str(), self.path)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtensionRestResponse {
    pub status: RestStatus,
    pub content_type: String,
    pub content: Vec<u8>,
    pub headers: BTreeMap<String, Vec<String>>,
    pub consumed_params: BTreeSet<String>,
    pub content_consumed: bool,
}

impl ExtensionRestResponse {
    pub const TEXT_CONTENT_TYPE: &'static str = "text/plain; charset=UTF-8";

    pub fn text(status: RestStatus, content: impl Into<String>) -> Self {
        Self {
            status,
            content_type: Self::TEXT_CONTENT_TYPE.into(),
            content: content.into().into_bytes(),
            headers: BTreeMap::new(),
            consumed_params: BTreeSet::new(),
            content_consumed: false,
        }
    }

    pub fn from_request(
        request: ExtensionRestRequest,
        status: RestStatus,
        content_type: impl Into<String>,
        content: Vec<u8>,
    ) -> Self {
        Self {
            status,
            content_type: content_type.into(),
            content,
            headers: BTreeMap::new(),
            consumed_params: request.consumed_params,
            content_consumed: request.content_consumed,
        }
    }
}
