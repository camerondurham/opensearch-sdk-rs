# Next Steps for OpenSearch SDK Rust

This document outlines the planned development steps to complete the OpenSearch Extension SDK implementation in Rust.

## Current Status

**Completed (Priority 1):**
- ✅ Development environment setup (Nix flake with all dependencies)
- ✅ Basic TCP header parsing for OpenSearch transport protocol
- ✅ Proto files for extension identity, requests, and REST actions
- ✅ CI/CD workflows (cargo check and test)
- ✅ Basic server listening on port 1234
- ✅ Handshake detection in transport layer
- ✅ Fixed build.rs duplicate proto file issue
- ✅ Added development documentation to README

**What Works:**
- Transport header parsing (prefix, size, request_id, status, version, variable_header_size)
- Handshake detection via status flags
- Basic server socket listening

**Known Issues:**
- ❌ Incomplete serialization/deserialization in `src/interface.rs:48` and `src/interface.rs:61`
- ❌ No handshake response implementation in `src/main.rs:64-66`
- ❌ No message body parsing beyond headers
- ❌ No variable header parsing
- ❌ No actual extension functionality (REST handlers, etc.)
- ❌ No tests implemented

## Priority 2: Complete Core Transport Protocol

### 2.1 Implement Variable Header Parsing
**Location:** `src/transport.rs`

Currently only fixed headers are parsed. Need to parse variable-length headers that follow the fixed header:
- Thread context headers
- Feature flags
- Any custom headers

**References:**
- [Python SDK: async_extension_host.py#L48](https://github.com/opensearch-project/opensearch-sdk-py/blob/fbfbeef4d0dffbd6ecea32959ab4df5c1bf34431/src/opensearch_sdk_py/server/async_extension_host.py#L48)
- [Python SDK: tcp_header.py](https://github.com/opensearch-project/opensearch-sdk-py/blob/main/src/opensearch_sdk_py/transport/tcp_header.py)

**Tasks:**
- [ ] Define structure for variable headers
- [ ] Implement parsing after fixed header
- [ ] Handle thread context headers
- [ ] Add tests for variable header parsing

### 2.2 Implement Handshake Response
**Location:** `src/main.rs:64-66`

Complete the TODO for handshake handling:
```rust
if h.is_handshake() {
    // TODO: actually handle this case
}
```

**Tasks:**
- [ ] Study OpenSearch handshake protocol
- [ ] Create handshake response message
- [ ] Serialize and send response back to OpenSearch
- [ ] Parse handshake request body (extension identity)
- [ ] Add handshake state tracking

**References:**
- [Java SDK: CREATE_YOUR_FIRST_EXTENSION.md](https://github.com/opensearch-project/opensearch-sdk-java/blob/main/CREATE_YOUR_FIRST_EXTENSION.md)
- [Python SDK: Extension class](https://github.com/opensearch-project/opensearch-sdk-py/blob/main/src/opensearch_sdk_py/extension.py)

### 2.3 Complete Serialization/Deserialization
**Location:** `src/interface.rs`

Finish implementing the `Serialize` and `Deserialize` traits:
- Complete `Request::serialize` at line 48
- Complete `Request::deserialize` at line 61

**Tasks:**
- [ ] Implement full serialization for Request types
- [ ] Implement full deserialization for Request types
- [ ] Add proper error handling
- [ ] Consider using serde instead of custom traits (see TODO at line 3)
- [ ] Write unit tests for serialization round-trips

### 2.4 Add Message Body Parsing
**Location:** New module or extend `src/transport.rs`

After headers are parsed, parse the protobuf message bodies:
- ExtensionIdentity messages
- ExtensionRequest messages
- RegisterRestActions messages

**Tasks:**
- [ ] Use generated protobuf code from build.rs
- [ ] Parse message bodies after headers
- [ ] Create message routing based on request type
- [ ] Handle different message types appropriately

## Priority 3: Basic Extension Functionality

### 3.1 Implement Extension Initialization Handshake
**Location:** New module `src/handshake.rs` or extend `src/main.rs`

Implement full bidirectional handshake protocol:
1. Receive handshake request from OpenSearch
2. Parse extension identity
3. Send handshake response
4. Wait for acknowledgment

**Tasks:**
- [ ] Create handshake state machine
- [ ] Implement request/response flow
- [ ] Handle handshake errors
- [ ] Add timeout handling
- [ ] Log handshake progress

### 3.2 Add REST Action Registration
**Location:** New module `src/rest.rs`

Use `RegisterRestActionsProto.proto` to register REST endpoints with OpenSearch:

**Tasks:**
- [ ] Create REST action registry
- [ ] Build RegisterRestActions message
- [ ] Send registration request to OpenSearch
- [ ] Handle registration response
- [ ] Map REST routes to handlers

**References:**
- `examples/hello/hello.json` - example extension config
- [Java SDK REST examples](https://github.com/opensearch-project/opensearch-sdk-java/blob/main/CREATE_YOUR_FIRST_EXTENSION.md)

### 3.3 Create "Hello World" REST Handler
**Location:** New module `src/handlers.rs`

Implement the endpoint referenced in `examples/hello/hello.json`:

**Tasks:**
- [ ] Create handler trait/interface
- [ ] Implement basic "hello world" handler
- [ ] Parse incoming REST requests
- [ ] Build REST responses
- [ ] Connect handler to REST action registration
- [ ] Test with: `curl -ku "admin:$PASS" -XGET "https://localhost:9200/_extensions/_hello-world-rs/hello"`

### 3.4 Async/Tokio Integration
**Location:** Throughout codebase

Currently uses blocking I/O. Integrate Tokio properly:

**Tasks:**
- [ ] Convert main server loop to async
- [ ] Use tokio::net::TcpListener instead of std::net::TcpListener
- [ ] Add async handlers
- [ ] Handle concurrent connections
- [ ] Add connection pooling if needed

## Priority 4: Testing & Documentation

### 4.1 Write Unit Tests
**Location:** Test modules in each source file

**Tasks:**
- [ ] Test header parsing with various inputs
- [ ] Test serialization/deserialization round-trips
- [ ] Test handshake state machine
- [ ] Test message routing
- [ ] Test error conditions
- [ ] Add property-based tests for serialization

### 4.2 Write Integration Tests
**Location:** `tests/` directory

**Tasks:**
- [ ] Create test fixtures with mock OpenSearch messages
- [ ] Test full handshake flow
- [ ] Test REST action registration
- [ ] Test extension initialization
- [ ] Set up test against actual OpenSearch instance (using Docker)
- [ ] Add CI integration test job

### 4.3 Add Examples
**Location:** `examples/` directory

**Tasks:**
- [ ] Complete hello-world example
- [ ] Add example with multiple REST endpoints
- [ ] Add example with custom transport actions
- [ ] Add example with settings registration
- [ ] Document how to run each example

### 4.4 Expand Documentation

**Tasks:**
- [ ] Expand `DEVELOPMENT_NOTES.md` with protocol learnings
- [ ] Document architecture decisions
- [ ] Add API documentation (cargo doc)
- [ ] Create architecture diagrams
- [ ] Document OpenSearch setup for development
- [ ] Add troubleshooting guide
- [ ] Document extension deployment

## Future Enhancements (Post-MVP)

### Error Handling
- [ ] Comprehensive error types
- [ ] Error recovery strategies
- [ ] Graceful shutdown handling

### Advanced Features
- [ ] Settings registration (REQUEST_EXTENSION_REGISTER_SETTINGS)
- [ ] Cluster state access (REQUEST_EXTENSION_CLUSTER_STATE)
- [ ] Environment settings (REQUEST_EXTENSION_ENVIRONMENT_SETTINGS)
- [ ] Dependency information (REQUEST_EXTENSION_DEPENDENCY_INFORMATION)
- [ ] Component creation (CREATE_COMPONENT)
- [ ] Index module hooks (ON_INDEX_MODULE)

### Performance
- [ ] Connection pooling
- [ ] Message batching
- [ ] Zero-copy optimizations
- [ ] Benchmarking suite

### Observability
- [ ] Structured logging
- [ ] Metrics collection
- [ ] Tracing integration
- [ ] Health check endpoint

## References

- [OpenSearch Extensions Blog](https://opensearch.org/blog/introducing-extensions-for-opensearch)
- [Python SDK](https://github.com/opensearch-project/opensearch-sdk-py)
- [Java SDK](https://github.com/opensearch-project/opensearch-sdk-java)
- [Java SDK Developer Guide](https://github.com/opensearch-project/opensearch-sdk-java/blob/main/DEVELOPER_GUIDE.md)
- [Creating Your First Extension](https://github.com/opensearch-project/opensearch-sdk-java/blob/main/CREATE_YOUR_FIRST_EXTENSION.md)
- [CRUD Extension Example](https://github.com/dbwiddis/CRUDExtension)
- [OpenSearch Extensions Video](https://www.youtube.com/watch?v=TZy7ViZbbHc)
