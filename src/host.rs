use crate::extension::{not_found_response, Extension, ExtensionMetadata, Route};
use crate::rest::ExtensionRestResponse;
use crate::transport::{
    decode_extension_rest_request, encode_extension_rest_response, AcknowledgedResponse,
    DiscoveryNode, DiscoveryNodeRole, ExtensionRequest, InitializeExtensionRequest,
    InitializeExtensionResponse, MessageFrame, RegisterRestActionsRequest, RequestType,
    ThreadContext, TransportAddress, TransportHandshakerHandshakeResponse,
    TransportServiceHandshakeResponse, Version,
};
use std::collections::BTreeMap;
use std::env;
use std::io::{self, ErrorKind};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;

const ACTION_TCP_HANDSHAKE: &str = "internal:tcp/handshake";
const ACTION_TRANSPORT_HANDSHAKE: &str = "internal:transport/handshake";
const ACTION_DISCOVERY_EXTENSIONS: &str = "internal:discovery/extensions";
const ACTION_REGISTER_REST_ACTIONS: &str = "internal:discovery/registerrestactions";
const ACTION_ENVIRONMENT_SETTINGS: &str = "internal:discovery/enviornmentsettings";
const ACTION_REST_EXECUTE_ON_EXTENSION: &str = "internal:extensions/restexecuteonextensiontaction";

#[derive(Debug, Clone)]
enum PendingRequest {
    RegisterRestActions {
        init_request_id: u64,
        thread_context: ThreadContext,
        features: Vec<String>,
    },
    EnvironmentSettings {
        init_request_id: u64,
        thread_context: ThreadContext,
    },
}

#[derive(Debug, Default)]
struct HostState {
    next_request_id: u64,
    pending_requests: BTreeMap<u64, PendingRequest>,
}

pub struct ExtensionHost {
    metadata: ExtensionMetadata,
    implemented_interfaces: Vec<String>,
    routes: Vec<Route>,
    state: Mutex<HostState>,
}

impl ExtensionHost {
    pub fn new<E: Extension>(extension: E) -> Self {
        let routes = extension.routes();
        let metadata = extension.metadata().clone();
        let implemented_interfaces = extension.implemented_interfaces();
        Self {
            metadata,
            implemented_interfaces,
            routes,
            state: Mutex::new(HostState {
                next_request_id: 1,
                pending_requests: BTreeMap::new(),
            }),
        }
    }

    pub fn serve(self) -> io::Result<()> {
        let listener = TcpListener::bind((self.metadata.host_address, self.metadata.port))?;
        let shared = Arc::new(self);

        for connection in listener.incoming() {
            let mut stream = connection?;
            let host = Arc::clone(&shared);
            thread::spawn(move || {
                if let Err(error) = host.serve_connection(&mut stream) {
                    eprintln!("connection error: {error}");
                }
            });
        }

        Ok(())
    }

    pub fn serve_connection(&self, stream: &mut TcpStream) -> io::Result<()> {
        loop {
            match MessageFrame::read_from(stream) {
                Ok(frame) => {
                    trace_frame("recv", &frame);
                    let outbound = self.handle_frame(frame)?;
                    for response in outbound {
                        trace_frame("send", &response);
                        response.write_to(stream)?;
                    }
                }
                Err(error) if error.kind() == ErrorKind::UnexpectedEof => return Ok(()),
                Err(error) => return Err(error),
            }
        }
    }

    pub fn handle_frame(&self, frame: MessageFrame) -> io::Result<Vec<MessageFrame>> {
        if frame.header.is_response() {
            return self.handle_response(frame);
        }

        match frame.action.as_deref() {
            Some(ACTION_TCP_HANDSHAKE) => Ok(vec![self.handle_tcp_handshake(frame)]),
            Some(ACTION_TRANSPORT_HANDSHAKE) => Ok(vec![self.handle_transport_handshake(frame)]),
            Some(ACTION_DISCOVERY_EXTENSIONS) => self.handle_initialize_extension(frame),
            Some(ACTION_REST_EXECUTE_ON_EXTENSION) => Ok(vec![self.handle_rest_execute(frame)?]),
            _ => Ok(Vec::new()),
        }
    }

    fn handle_response(&self, frame: MessageFrame) -> io::Result<Vec<MessageFrame>> {
        let mut state = self.lock_state()?;
        let Some(pending) = state.pending_requests.remove(&frame.header.request_id) else {
            return Ok(Vec::new());
        };

        match pending {
            PendingRequest::RegisterRestActions {
                init_request_id,
                thread_context,
                features,
            } => {
                let acknowledgement = AcknowledgedResponse::read_from(&frame.body)?;
                trace_acknowledgement(
                    "register_rest_actions",
                    frame.header.request_id,
                    &acknowledgement,
                );
                if !acknowledgement.acknowledged {
                    return Ok(Vec::new());
                }

                let env_request_id = next_request_id(&mut state);
                state.pending_requests.insert(
                    env_request_id,
                    PendingRequest::EnvironmentSettings {
                        init_request_id,
                        thread_context: thread_context.clone(),
                    },
                );

                let env_request = MessageFrame::request(
                    env_request_id,
                    Version::min_compat(),
                    thread_context,
                    features,
                    ACTION_ENVIRONMENT_SETTINGS.into(),
                    ExtensionRequest {
                        request_type: RequestType::EnvironmentSettings,
                        unique_id: None,
                    }
                    .to_bytes(),
                    false,
                );
                Ok(vec![env_request])
            }
            PendingRequest::EnvironmentSettings {
                init_request_id,
                thread_context,
            } => {
                trace_environment_settings(frame.header.request_id, frame.body.len());
                Ok(vec![MessageFrame::response(
                    init_request_id,
                    Version::min_compat(),
                    thread_context,
                    InitializeExtensionResponse {
                        name: self.metadata.name.clone(),
                        implemented_interfaces: self.implemented_interfaces.clone(),
                    }
                    .to_bytes(),
                    false,
                    false,
                )])
            }
        }
    }

    fn handle_tcp_handshake(&self, frame: MessageFrame) -> MessageFrame {
        MessageFrame::response(
            frame.header.request_id,
            frame.header.version,
            frame.thread_context,
            TransportHandshakerHandshakeResponse {
                version: Version::current(),
            }
            .to_bytes(),
            true,
            false,
        )
    }

    fn handle_transport_handshake(&self, frame: MessageFrame) -> MessageFrame {
        let response_version = frame.header.version;
        MessageFrame::response(
            frame.header.request_id,
            response_version,
            frame.thread_context,
            TransportServiceHandshakeResponse {
                discovery_node: Some(self.discovery_node()),
                cluster_name: String::new(),
                version: Version::current(),
            }
            .to_bytes_with_transport_version(response_version),
            false,
            false,
        )
    }

    fn handle_initialize_extension(&self, frame: MessageFrame) -> io::Result<Vec<MessageFrame>> {
        let mut input = crate::stream::StreamInput::new(&frame.body);
        let _task_id = crate::transport::TaskId::read_from(&mut input)?;
        let _request = InitializeExtensionRequest::read_from_with_transport_version(
            &mut input,
            frame.header.version,
        )?;

        let mut state = self.lock_state()?;
        let request_id = next_request_id(&mut state);
        state.pending_requests.insert(
            request_id,
            PendingRequest::RegisterRestActions {
                init_request_id: frame.header.request_id,
                thread_context: frame.thread_context.clone(),
                features: frame.features.clone(),
            },
        );

        let register_request = MessageFrame::request(
            request_id,
            Version::min_compat(),
            frame.thread_context,
            frame.features,
            ACTION_REGISTER_REST_ACTIONS.into(),
            RegisterRestActionsRequest {
                unique_id: self.metadata.unique_id.clone(),
                rest_actions: self
                    .routes
                    .iter()
                    .map(Route::registration_string)
                    .collect::<Vec<_>>(),
                deprecated_rest_actions: Vec::new(),
            }
            .to_bytes(),
            false,
        );

        Ok(vec![register_request])
    }

    fn handle_rest_execute(&self, frame: MessageFrame) -> io::Result<MessageFrame> {
        let request = decode_extension_rest_request(&frame.body)?;
        let response = self.dispatch_rest_request(request);
        Ok(MessageFrame::response(
            frame.header.request_id,
            Version::min_compat(),
            frame.thread_context,
            encode_extension_rest_response(&response),
            false,
            false,
        ))
    }

    fn dispatch_rest_request(
        &self,
        request: crate::rest::ExtensionRestRequest,
    ) -> ExtensionRestResponse {
        if let Some(route) = self
            .routes
            .iter()
            .find(|route| route.matches(request.method, &request.path))
        {
            return (route.handler)(request);
        }

        not_found_response(request)
    }

    fn discovery_node(&self) -> DiscoveryNode {
        let address = TransportAddress::new(self.metadata.host_address, self.metadata.port as i32);

        DiscoveryNode {
            node_name: self.metadata.unique_id.clone(),
            node_id: self.metadata.unique_id.clone(),
            ephemeral_id: format!("{}-ephemeral", self.metadata.unique_id),
            host_name: self.metadata.host_address.to_string(),
            host_address: self.metadata.host_address.to_string(),
            address,
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

    fn lock_state(&self) -> io::Result<MutexGuard<'_, HostState>> {
        self.state
            .lock()
            .map_err(|_| io::Error::other("host state mutex poisoned"))
    }
}

fn next_request_id(state: &mut HostState) -> u64 {
    let request_id = state.next_request_id;
    state.next_request_id += 1;
    request_id
}

fn trace_enabled() -> bool {
    env::var_os("OPENSEARCH_SDK_RS_TRACE").is_some()
}

fn trace_frame(direction: &str, frame: &MessageFrame) {
    if !trace_enabled() {
        return;
    }

    let action = frame.action.as_deref().unwrap_or("<response>");
    eprintln!(
        "[trace] {direction} request_id={} action={} version={} status=0x{:02x} handshake={} error={} body_bytes={} features={}",
        frame.header.request_id,
        action,
        frame.header.version,
        frame.header.status,
        frame.header.is_handshake(),
        frame.header.is_error(),
        frame.body.len(),
        frame.features.len(),
    );
}

fn trace_acknowledgement(kind: &str, request_id: u64, acknowledgement: &AcknowledgedResponse) {
    if !trace_enabled() {
        return;
    }

    eprintln!(
        "[trace] recv request_id={} pending={} acknowledged={}",
        request_id, kind, acknowledgement.acknowledged
    );
}

fn trace_environment_settings(request_id: u64, body_len: usize) {
    if !trace_enabled() {
        return;
    }

    eprintln!(
        "[trace] recv request_id={} pending=environment_settings body_bytes={}",
        request_id, body_len
    );
}

#[cfg(test)]
mod tests {
    use super::{
        ExtensionHost, ACTION_DISCOVERY_EXTENSIONS, ACTION_ENVIRONMENT_SETTINGS,
        ACTION_REGISTER_REST_ACTIONS,
    };
    use crate::extension::{Extension, ExtensionMetadata, Route};
    use crate::rest::{ExtensionRestResponse, HttpVersion, RestMethod, RestStatus};
    use crate::stream::{StreamInput, StreamOutput};
    use crate::transport::{
        decode_extension_rest_request, AcknowledgedResponse, DiscoveryExtensionNode, DiscoveryNode,
        DiscoveryNodeRole, MessageFrame, TaskId, ThreadContext, TransportAddress, Version,
    };
    use std::collections::BTreeMap;
    use std::net::{IpAddr, Ipv4Addr};

    struct TestExtension {
        metadata: ExtensionMetadata,
    }

    impl TestExtension {
        fn new() -> Self {
            Self {
                metadata: ExtensionMetadata::new("Hello World", "hello-world-rs"),
            }
        }
    }

    impl Extension for TestExtension {
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

    fn discovery_node() -> DiscoveryNode {
        DiscoveryNode {
            node_name: "source".into(),
            node_id: "source".into(),
            ephemeral_id: "source-ephemeral".into(),
            host_name: "127.0.0.1".into(),
            host_address: "127.0.0.1".into(),
            address: TransportAddress::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 9300),
            stream_address: None,
            attributes: BTreeMap::new(),
            roles: vec![DiscoveryNodeRole::cluster_manager()],
            version: Version::current(),
        }
    }

    fn init_frame() -> MessageFrame {
        let source_node = discovery_node();
        let extension_node = DiscoveryExtensionNode {
            discovery_node: source_node.clone(),
            minimum_compatible_version: Version::from_release_id(3_000_000),
            dependencies: Vec::new(),
        };

        let mut body = StreamOutput::new();
        TaskId::default().write_to(&mut body);
        source_node.write_to(&mut body);
        extension_node.write_to(&mut body);
        body.write_string("service-token");

        MessageFrame::request(
            9,
            Version::current(),
            ThreadContext::default(),
            Vec::new(),
            ACTION_DISCOVERY_EXTENSIONS.into(),
            body.into_bytes(),
            false,
        )
    }

    #[test]
    fn init_flow_matches_python_sequence() {
        let host = ExtensionHost::new(TestExtension::new());

        let register = host.handle_frame(init_frame()).unwrap();
        assert_eq!(register.len(), 1);
        assert_eq!(
            register[0].action.as_deref(),
            Some(ACTION_REGISTER_REST_ACTIONS)
        );

        let register_ack = MessageFrame::response(
            register[0].header.request_id,
            Version::min_compat(),
            ThreadContext::default(),
            {
                let mut out = StreamOutput::new();
                out.write_bool(true);
                out.into_bytes()
            },
            false,
            false,
        );

        let env_request = host.handle_frame(register_ack).unwrap();
        assert_eq!(env_request.len(), 1);
        assert_eq!(
            env_request[0].action.as_deref(),
            Some(ACTION_ENVIRONMENT_SETTINGS)
        );

        let env_response = MessageFrame::response(
            env_request[0].header.request_id,
            Version::min_compat(),
            ThreadContext::default(),
            Vec::new(),
            false,
            false,
        );

        let init_response = host.handle_frame(env_response).unwrap();
        assert_eq!(init_response.len(), 1);
        assert!(init_response[0].header.is_response());
        assert_eq!(init_response[0].header.request_id, 9);
    }

    #[test]
    fn rest_request_dispatches_to_registered_route() {
        let mut body = StreamOutput::new();
        TaskId::default().write_to(&mut body);
        body.write_vint(0);
        body.write_string("/hello");
        body.write_string("/hello");
        body.write_string_map(&BTreeMap::new());
        body.write_string_list_map(&BTreeMap::new());
        body.write_bool(false);
        body.write_byte_array(&[]);
        body.write_string("");
        body.write_vint(1);

        let frame = MessageFrame::request(
            11,
            Version::min_compat(),
            ThreadContext::default(),
            Vec::new(),
            "internal:extensions/restexecuteonextensiontaction".into(),
            body.into_bytes(),
            false,
        );

        let host = ExtensionHost::new(TestExtension::new());
        let response = host.handle_frame(frame).unwrap();
        assert_eq!(response.len(), 1);

        let payload = response[0].body.clone();
        let mut input = StreamInput::new(&payload);
        assert_eq!(input.read_vint().unwrap(), RestStatus::Ok.to_wire());
        assert_eq!(
            input.read_string().unwrap(),
            ExtensionRestResponse::TEXT_CONTENT_TYPE
        );
        assert_eq!(
            String::from_utf8(input.read_byte_array().unwrap()).unwrap(),
            "Hello from Rust!"
        );
    }

    #[test]
    fn rest_request_payload_decodes() {
        let mut body = StreamOutput::new();
        TaskId::default().write_to(&mut body);
        body.write_vint(0);
        body.write_string("/hello");
        body.write_string("/hello");
        body.write_string_map(&BTreeMap::new());
        body.write_string_list_map(&BTreeMap::new());
        body.write_bool(false);
        body.write_byte_array(&[]);
        body.write_string("");
        body.write_vint(1);

        let request = decode_extension_rest_request(&body.into_bytes()).unwrap();
        assert_eq!(request.method, RestMethod::Get);
        assert_eq!(request.http_version, HttpVersion::Http11);
    }

    #[test]
    fn acknowledged_response_round_trips() {
        let mut out = StreamOutput::new();
        out.write_bool(true);
        let ack = AcknowledgedResponse::read_from(&out.into_bytes()).unwrap();
        assert!(ack.acknowledged);
    }
}
