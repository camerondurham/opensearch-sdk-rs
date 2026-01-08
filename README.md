# opensearch-sdk-rs

WIP attempt to implement an OpenSearch Extension SDK in Rust.

Inspired by https://github.com/opensearch-project/opensearch-sdk-py and https://www.youtube.com/watch?v=TZy7ViZbbHc

## Development Setup

This project uses Nix for managing development dependencies (Rust, protobuf compiler, JDK, etc.).

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

### Using Just

This project includes a `justfile` for common development tasks:

```bash
just build    # Build the project (with formatting)
just test     # Run tests
just run      # Run the server
just fmt      # Format code
just clippy   # Run clippy lints
```

### Without Nix

If you prefer not to use Nix, ensure you have the following installed:
- Rust toolchain (rustc, cargo, rustfmt)
- Protocol Buffers compiler (`protoc`)
- JDK 17 (for running OpenSearch)
- `just` command runner (optional)

## References

1. https://opensearch.org/blog/introducing-extensions-for-opensearch
2. https://github.com/opensearch-project/opensearch-sdk-py
3. https://github.com/opensearch-project/opensearch-sdk-java
