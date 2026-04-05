# opensearch-sdk-rs

Rust proof of concept for OpenSearch 3.x extensions that run as a separate process instead of an in-process plugin.

The current implementation focuses on the REST-first extension lifecycle:
- transport frame parsing and encoding
- TCP and transport handshakes
- extension initialization
- REST route registration
- REST request dispatch for a hello-world extension

The design is intentionally closer to `opensearch-sdk-py` than the larger Java SDK surface.

## Development Setup

This project uses Nix for managing development dependencies (Rust and JDK for local OpenSearch work).

### Prerequisites

- [Nix package manager](https://nixos.org/download.html) installed with flakes enabled

### Getting Started

1. Enter the Nix development shell:
   ```bash
   nix develop
   ```

2. Build the project:
   ```bash
   cargo build
   ```

3. Run the server:
   ```bash
   cargo run
   ```

4. Run tests:
   ```bash
   cargo test
   ```

5. Run the live OpenSearch 3.x hello-world harness:
   ```bash
   ./scripts/live_hello.sh
   ```
   This builds the local `../OpenSearch` `no-jdk-linux-tar` archive if needed, starts the Rust extension,
   initializes it through `/_extensions/initialize`, and verifies
   `/_extensions/_hello-world-rs/hello`.

6. Run the cargo-visible ignored integration test for the same flow:
   ```bash
   cargo test --test live_hello -- --ignored --nocapture
   ```
   Use `OPENSEARCH_DIR=/path/to/OpenSearch` if your local checkout is not at `../OpenSearch`.

### Using Just

This project includes a `justfile` for common development tasks:

```bash
just build    # Build the project (with formatting)
just test     # Run tests
just run      # Run the server
just live_hello  # Run the end-to-end OpenSearch 3.x hello-world harness
just test_live_hello  # Run the ignored cargo integration test for the live harness
just fmt      # Format code
just clippy   # Run clippy lints
```

### Without Nix

If you prefer not to use Nix, ensure you have the following installed:
- Rust toolchain (rustc, cargo, rustfmt)
- JDK 17 (for running OpenSearch)
- `just` command runner (optional)

## What Works

- `internal:tcp/handshake`
- `internal:transport/handshake`
- `internal:discovery/extensions`
- `internal:extensions/restexecuteonextensiontaction`
- outbound `internal:discovery/registerrestactions`
- outbound `internal:discovery/enviornmentsettings`

## Current POC Route

The sample binary exposes one route:

```text
GET /hello
```

When registered through OpenSearch with the included `examples/hello/hello.json`, that route is expected to be reachable through:

```text
/_extensions/_hello-world-rs/hello
```

The sample server accepts optional runtime overrides for live harnessing:
- `OPENSEARCH_SDK_RS_HOST`
- `OPENSEARCH_SDK_RS_PORT`
- `OPENSEARCH_SDK_RS_TRACE`

## References

1. https://opensearch.org/blog/introducing-extensions-for-opensearch
2. https://github.com/opensearch-project/opensearch-sdk-py
3. https://github.com/opensearch-project/opensearch-sdk-java
