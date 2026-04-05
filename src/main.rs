use opensearch_sdk_rs::extension::{Extension, ExtensionMetadata, Route};
use opensearch_sdk_rs::host::ExtensionHost;
use opensearch_sdk_rs::rest::{ExtensionRestResponse, RestMethod, RestStatus};
use std::env;
use std::io::{self, ErrorKind};

struct HelloWorldExtension {
    metadata: ExtensionMetadata,
}

impl HelloWorldExtension {
    fn new() -> io::Result<Self> {
        let mut metadata = ExtensionMetadata::new("Hello World", "hello-world-rs");

        if let Some(host) = env::var_os("OPENSEARCH_SDK_RS_HOST") {
            metadata.host_address = host.to_string_lossy().parse().map_err(|error| {
                io::Error::new(
                    ErrorKind::InvalidInput,
                    format!("invalid OPENSEARCH_SDK_RS_HOST: {error}"),
                )
            })?;
        }

        if let Some(port) = env::var_os("OPENSEARCH_SDK_RS_PORT") {
            metadata.port = port.to_string_lossy().parse().map_err(|error| {
                io::Error::new(
                    ErrorKind::InvalidInput,
                    format!("invalid OPENSEARCH_SDK_RS_PORT: {error}"),
                )
            })?;
        }

        Ok(Self { metadata })
    }
}

impl Extension for HelloWorldExtension {
    fn metadata(&self) -> &ExtensionMetadata {
        &self.metadata
    }

    fn routes(&self) -> Vec<Route> {
        vec![Route::new(
            RestMethod::Get,
            "/hello",
            "hello_world_rs:hello",
            |request| {
                ExtensionRestResponse::from_request(
                    request,
                    RestStatus::Ok,
                    ExtensionRestResponse::TEXT_CONTENT_TYPE,
                    b"Hello from Rust!".to_vec(),
                )
            },
        )]
    }
}

fn main() -> std::io::Result<()> {
    let host = ExtensionHost::new(HelloWorldExtension::new()?);
    host.serve()
}
