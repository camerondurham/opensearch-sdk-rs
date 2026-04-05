# Next Steps for OpenSearch SDK Rust

This crate now targets the OpenSearch 3.x extension protocol directly rather than the older header-only prototype.

## Current Status

Implemented now:
- std-only transport codec for OpenSearch extension frames
- thread-context and variable-header parsing
- TCP and transport handshake responses
- extension init flow modeled after `opensearch-sdk-py`
- outbound `registerrestactions` and `enviornmentsettings` requests
- minimal Rust extension API with route registration and handler dispatch
- hello-world standalone extension binary
- repeatable `scripts/live_hello.sh` harness for building the local OpenSearch `no-jdk-linux-tar`, starting the Rust extension, initializing it, and probing the hello-world route
- ignored cargo integration test that delegates to the live hello harness
- unit tests covering framing, request/response payloads, route matching, and the REST-first init sequence

Deliberately not implemented yet:
- custom settings registration
- transport actions
- extension-to-extension actions
- cargo-native source-backed OpenSearch integration test harness
- richer route extraction and request body/media-type coverage

## Near-Term Priorities

### 1. Broaden Wire Compatibility
- Validate request/response codecs against additional OpenSearch 3.x payloads beyond the hello-world GET path.
- Add support for request bodies with media types instead of assuming the GET-style no-body case.
- Add explicit error responses for unsupported actions and malformed payloads.

### 2. Fill Out the 3.x Extension Contract
- Add custom settings registration and settings update handling.
- Add environment settings response parsing instead of treating the payload as opaque.
- Add dependency lookup and cluster-state requests where the server contract already exists in `OpenSearch`.

### 3. Improve the Rust SDK Surface
- Expose a cleaner public API for extension metadata, route registration, and lifecycle hooks.
- Support path-param extraction and helper response builders.
- Add examples beyond hello-world, especially multi-route and settings-aware extensions.

## Next Agent Handoff

The next agent should assume the crate is at this state:
- `cargo test` passes locally.
- The crate no longer depends on `prost`, `tokio`, `byteorder`, or `nom`.
- `./scripts/live_hello.sh` succeeds against the local `/home/nixos/opensearch-project/OpenSearch` source tree.
- `cargo test --test live_hello -- --ignored --nocapture` is now the cargo-visible entrypoint for that same live harness.
- The old `justfile` OpenSearch Docker helpers are stale for this work because they still target 2.x images.

Primary code paths to understand first:
- `src/host.rs`: runtime state machine for handshakes, init flow, and REST dispatch
- `src/transport.rs`: wire codec, OpenSearch payload types, and protobuf byte builders
- `src/extension.rs`: minimal Rust extension API
- `src/main.rs`: hello-world standalone extension entrypoint

Recommended next execution sequence:
1. Confirm the current crate still passes with `cargo test`.
2. Run `cargo test --test live_hello -- --ignored --nocapture` from `opensearch-sdk-rs` when touching transport or host flow.
3. If that fails, inspect `.tmp/live-hello/logs/extension.log` and `.tmp/live-hello/logs/opensearch.log` and reconcile the Rust codec with the live bytes before broadening scope.
4. Extend coverage beyond the GET no-body path once the live harness remains stable.

Minimum acceptance criteria for the next agent:
- OpenSearch successfully initializes the Rust extension without timing out.
- The Rust process observes the expected sequence:
  `internal:tcp/handshake`
  `internal:transport/handshake`
  `internal:discovery/extensions`
  outbound `internal:discovery/registerrestactions`
  outbound `internal:discovery/enviornmentsettings`
  inbound `internal:extensions/restexecuteonextensiontaction`
- The hello-world endpoint returns the Rust response body through OpenSearch.

Most likely breakpoints during live testing:
- stream encoding mismatches in `DiscoveryNode` / `DiscoveryExtensionNode`
- request/response body layout mismatches around `InitializeExtensionRequest`
- `ExtensionRestRequest` field ordering or byte-array framing mismatches
- assumptions about `Version::min_compat()` versus what the running OpenSearch build actually sends
- missing handling for non-empty environment-settings payloads
- request-body/media-type handling for non-GET REST routes

If the live harness remains green, the next work item should be:
- broaden it beyond the GET no-body path with a request-body/media-type exercise and explicit unsupported-action/error coverage

If live initialization fails, the next work item should be:
- use the existing `OPENSEARCH_SDK_RS_TRACE=1` frame logging plus the saved harness logs to reconcile the Rust codec with the actual OpenSearch bytes before expanding feature scope

## Out of Scope for the First POC

These should not block the first fully decoupled extension proof:
- Java-SDK-style plugin migration interfaces such as analysis, search, mapper, repository, or ingest extension surfaces
- dependency injection or component factories
- async runtime migration

Those belong after the Rust SDK is proven against the current 3.x extension protocol.
