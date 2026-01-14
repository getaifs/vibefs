# VibeFS Development Setup (Fedora Immutable)

## Overview

This project is developed on Fedora immutable Linux using **distrobox** for containerized development, avoiding direct installation of toolchains on the host system.

## Prerequisites

- Fedora immutable Linux (Silverblue/Kinoite/etc.)
- Distrobox installed
- Docker or Podman

## Development Environment Setup

### 1. Create Distrobox Container (Already Done)

```bash
distrobox create --name vibefs-dev --image fedora:latest
```

### 2. Enter Container and Install Dependencies

```bash
distrobox enter vibefs-dev

# Install build dependencies
sudo dnf install -y gcc-c++ clang-devel rocksdb-devel git

# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env
```

## Building the Project

### Inside Distrobox

```bash
distrobox enter vibefs-dev
source ~/.cargo/env
cd /var/home/x/src/vibefs

# Build release binary
ROCKSDB_LIB_DIR=/usr/lib64 cargo build --release

# Install binary inside container
cp target/release/vibe ~/.cargo/bin/
```

### Running Tests

```bash
# Inside distrobox
distrobox enter vibefs-dev
source ~/.cargo/env
cd /var/home/x/src/vibefs

# Run all tests
ROCKSDB_LIB_DIR=/usr/lib64 cargo test

# Run with output
ROCKSDB_LIB_DIR=/usr/lib64 cargo test -- --nocapture
```

## Using the Binary

### Option 1: Run from Distrobox (Recommended)

The binary uses shared libraries from the container, so it must be run from within the distrobox:

```bash
# Enter distrobox and use vibe
distrobox enter vibefs-dev -- vibe --help
distrobox enter vibefs-dev -- vibe init
distrobox enter vibefs-dev -- vibe spawn agent-1
```

### Option 2: Wrapper Script (Convenient)

Create a wrapper script on the host:

```bash
cat > ~/.local/bin/vibe << 'EOF'
#!/bin/bash
distrobox enter vibefs-dev -- vibe "$@"
EOF
chmod +x ~/.local/bin/vibe

# Now you can use it from host
vibe --help
vibe init
```

## Test Results

- **Build Status**: ✅ SUCCESS
- **Binary Size**: 3.7 MB
- **Unit Tests**: 8/8 passing ✅
- **Integration Tests**: 5/5 passing ✅
- **Total**: 13/13 tests passing ✅

## Project Structure

```
vibefs/
├── src/
│   ├── commands/       # Core commands (init, spawn, snapshot, promote, commit)
│   ├── db/            # RocksDB metadata store
│   ├── git/           # Git integration (CLI-based)
│   ├── nfs/           # NFS server (placeholder)
│   ├── tui/           # Ratatui dashboard
│   ├── bin/           # Helper utilities (mark_dirty)
│   ├── lib.rs         # Library entry point
│   └── main.rs        # CLI entry point
├── tests/
│   └── integration_tests.rs  # Full workflow tests
├── Cargo.toml
└── target/
    └── release/
        └── vibe       # Built binary (3.7 MB)
```

## Known Limitations

1. **Binary Portability**: The binary must run from within distrobox due to RocksDB shared library dependency
2. **NFS Server**: Not yet implemented - agents work directly in session directories
3. **Dirty File Tracking**: Requires manual marking via helper tool (automation pending)

## Development Workflow

1. Make changes to source code
2. Run tests in distrobox: `ROCKSDB_LIB_DIR=/usr/lib64 cargo test`
3. Build release: `ROCKSDB_LIB_DIR=/usr/lib64 cargo build --release`
4. Copy binary: `cp target/release/vibe ~/.cargo/bin/`
5. Test manually with sample repository

## Troubleshooting

### "cargo: command not found"

Make sure you're inside the distrobox and have sourced the Rust environment:

```bash
distrobox enter vibefs-dev
source ~/.cargo/env
```

### "librocksdb.so.10: cannot open shared object file"

This means you're trying to run the binary outside the distrobox. Use the wrapper script or run from within distrobox.

### Test failures related to RocksDB locks

The tests properly manage RocksDB locks with explicit scoping. If you see lock errors, ensure you're dropping the MetadataStore before opening it again.
