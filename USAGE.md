# sedocker Usage Guide

## Installation

### From Source
```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Clone and build
git clone <repo>
cd sedocker
make setup
make release

# Install
sudo make install
```

### Static Binary

The release binary is statically linked and has no dependencies:
```bash
# Check dependencies (should show "statically linked")
ldd sedocker

# Copy to any Linux system
scp sedocker user@remote:/usr/local/bin/
```

## Commands

### monitor - File Access Monitoring

Monitor file access in a directory in real-time.

**Basic Usage:**
```bash
# Monitor a directory
sudo sedocker monitor -d /docker/mysql/data
```

**With Container Information:**
```bash
# Show which container is accessing files
sudo sedocker monitor -d /docker/mysql/data --show-container
```

**JSON Output:**
```bash
# Output in JSON format for parsing
sudo sedocker monitor -d /docker/mysql/data -f json
```

**Output Example:**
```
EVENT   PID    UID   GID   PROCESS_PATH              CONTAINER       FILE_PATH
-------------------------------------------------------------------------------------------------------------------------
[OPEN]  12345  27    27    /usr/sbin/mysqld          a6c8a98ddebb    /docker/mysql/data/ibdata1
[WRITE] 12345  27    27    /usr/sbin/mysqld          a6c8a98ddebb    /docker/mysql/data/ib_logfile0
```

### check - Docker Information Collection

Collect comprehensive Docker container information.

**All Containers:**
```bash
# Check all containers
sudo sedocker check
```

**Specific Container:**
```bash
# Check one container
sudo sedocker check -c a6c8a98ddebb
# or by name
sudo sedocker check -c mysql_container
```

**Detailed Output:**
```bash
# Include process information
sudo sedocker check --verbose
```

**JSON Output:**
```bash
# Machine-readable format
sudo sedocker check -o json > containers.json
```

**Output Example:**
```
Container: a6c8a98ddebb
  Name:   mysql_prod
  Image:  mysql:8.0
  Status: running
  Created: 2026-02-10T10:30:00Z
  Ports:
    3306:tcp -> 3306/tcp
  Mounts:
    /docker/mysql/data -> /var/lib/mysql [rw, rw]
  Network:
    IP:      172.17.0.2
    Gateway: 172.17.0.1
    Mode:    bridge
  Process:
    PID: 12345
    UID: 999
    CMD: mysqld --datadir=/var/lib/mysql
```

## Use Cases

### Deployment Diagnostics
```bash
# Collect all container info for support
sedocker check -o json > deployment_info.json

# Monitor file access issues
sedocker monitor -d /data --show-container
```

### Security Auditing
```bash
# Monitor sensitive directories
sedocker monitor -d /etc --show-container -f json | \
  tee security_audit.log
```

### Troubleshooting
```bash
# Find which container is accessing files
sedocker monitor -d /shared/data --show-container

# Check container configuration
sedocker check -c problematic_container --verbose
```

## Requirements

- Linux kernel 2.6.36+ (for fanotify)
- Root privileges (for monitoring)
- Docker (for check command)

## Exit Codes

- 0: Success
- 1: Error occurred

## Environment Variables

None required. The tool is completely self-contained.