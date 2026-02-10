# sedock - Docker Monitoring and Inspection Tool

A lightweight, dependency-free tool for monitoring Docker containers and file access.

## Features

- **monitor**: Real-time file access monitoring using fanotify
- **check**: Comprehensive Docker container information collection

## Installation
```bash
# Build with musl for static linking (no glibc dependency)
cargo build --release --target x86_64-unknown-linux-musl

# The binary is completely standalone
./target/x86_64-unknown-linux-musl/release/sedock
```

## Usage

### Monitor file access
```bash
sedock monitor -d /docker/mysql/data
```

### Collect Docker information
```bash
sedock check
sedock check --container <container_id>
sedock check --output json
```

## Building

Requires Rust toolchain with musl target:
```bash
rustup target add x86_64-unknown-linux-musl
cargo build --release
```

## License

MIT