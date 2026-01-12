# VibeFS

**A "Clean Slate" Virtual Filesystem for Massively Parallel AI Agent Workflows**

VibeFS is a Rust-based userspace NFS server that decouples Git worktrees from storage, enabling 100+ AI agents to work in isolated, zero-copy sandboxes while maintaining a single source of truth in Git.

## Overview

VibeFS enables massively parallel AI agent workflows by providing each agent with its own ephemeral workspace that appears as a full Git repository, but is actually a lightweight virtual filesystem backed by:

- **Git Object Database** for immutable, versioned content
- **RocksDB** for fast inode-to-OID mappings
- **Session directories** for Copy-on-Write overlays

This architecture allows agents to work independently without conflicts, while changes can be promoted and merged back into the main repository.

## System Requirements

### Required Dependencies

- **Rust**: 1.70+ (install via [rustup](https://rustup.rs/))
- **C++ Compiler**: Required for RocksDB compilation
  - Linux: `sudo dnf install gcc-c++` (Fedora) or `sudo apt install build-essential` (Ubuntu)
  - macOS: Xcode Command Line Tools (`xcode-select --install`)
- **Git**: 2.30+
- **libclang**: Required for bindgen
  - Linux: `sudo dnf install clang-devel` (Fedora) or `sudo apt install libclang-dev` (Ubuntu)
  - macOS: Included with Xcode Command Line Tools

### Optional Dependencies for CoW Snapshots

- **macOS**: APFS filesystem (default on modern macOS)
- **Linux**: Btrfs or XFS filesystem with reflink support

## Building

```bash
# Install system dependencies (Fedora example)
sudo dnf install gcc-c++ clang-devel

# Build the project
cargo build --release

# Run tests
cargo test

# Install globally
cargo install --path .
```

## Quick Start

### 1. Initialize VibeFS

Initialize VibeFS for an existing Git repository:

```bash
cd /path/to/your/repo
vibe init
```

This creates a `.vibe/` directory with:
- `metadata.db`: RocksDB store for inode mappings
- `sessions/`: Per-agent workspace directories
- `cache/`: Shared build artifact cache

### 2. Spawn a Vibe Workspace

Create an isolated workspace for an AI agent:

```bash
vibe spawn agent-1
```

This creates:
- A session directory at `.vibe/sessions/agent-1/`
- An NFS mount point at `/tmp/vibe/agent-1/`
- Metadata for tracking file changes

### 3. Work in the Workspace

Agent makes changes in the mounted workspace (simulated here):

```bash
# Agents would work in /tmp/vibe/agent-1/
# For testing, we can directly modify the session directory
echo "pub fn new_feature() -> bool { true }" > .vibe/sessions/agent-1/new_feature.rs
```

### 4. Create Snapshots (Optional)

Take zero-cost snapshots at any point:

```bash
vibe snapshot agent-1
```

### 5. Promote Changes

Serialize agent work into a Git commit:

```bash
vibe promote agent-1
```

This:
- Hashes modified files as Git blobs
- Creates a new Git tree
- Commits with HEAD as parent
- Points `refs/vibes/agent-1` to the new commit

### 6. Merge into Main

Finalize the vibe into main history:

```bash
vibe commit agent-1
```

This updates HEAD and cleans up the session.

### 7. Dashboard (Optional)

Launch the TUI dashboard to monitor all active vibes:

```bash
vibe dashboard
```

## Core Commands

- `vibe init` - Initialize VibeFS for a Git repository
- `vibe spawn <vibe-id>` - Create an isolated agent workspace
- `vibe snapshot <vibe-id>` - Take a zero-cost snapshot
- `vibe promote <vibe-id>` - Serialize changes into a Git commit
- `vibe commit <vibe-id>` - Merge vibe into main history
- `vibe dashboard` - Launch TUI monitoring dashboard

## Architecture

### Storage Layout

```
<project-root>/
├── .git/                 # Standard Git ODB (source of truth)
├── .vibe/                # VibeFS Sidecar
│   ├── metadata.db       # RocksDB: Inode <-> Oid mapping
│   ├── sessions/         # Writable deltas per vibe-id
│   │   └── <vibe-id>/    # CoW layer for specific agent
│   └── cache/            # Global CAS for build artifacts
```

### Key Concepts

1. **Non-Invasiveness**: `.git` is the source of truth; VibeFS is a sidecar overlay
2. **Locality**: All metadata lives in `.vibe/`
3. **Virtualization**: Workspaces are ephemeral NFS mounts
4. **Cheap Snapshots**: Uses APFS/Reflinks for zero-cost snapshots

### Technology Stack

- **Rust**: Core implementation language
- **gitoxide (gix)**: High-speed Git operations
- **RocksDB**: Metadata persistence
- **nfsserve**: Userspace NFSv4 server
- **Ratatui**: Terminal UI dashboard

## Development

### Project Structure

```
src/
├── db/         # RocksDB metadata store
├── git/        # Git integration via gitoxide
├── nfs/        # NFS server implementation
├── commands/   # CLI command implementations
├── tui/        # Dashboard UI
├── lib.rs      # Library root
└── main.rs     # CLI entry point

tests/
└── integration_tests.rs  # End-to-end workflow tests
```

### Running Tests

```bash
# Run all tests
cargo test

# Run specific test
cargo test test_full_workflow

# Run with output
cargo test -- --nocapture
```

### Code Style

```bash
# Format code
cargo fmt

# Lint
cargo clippy
```

## Limitations & Future Work

- **NFS Server**: Current implementation sets up infrastructure; full NFS server integration is WIP
- **Conflict Resolution**: Basic implementation; advanced merge strategies needed
- **Performance**: Optimizations needed for 100+ concurrent agents
- **Windows Support**: Currently focused on Unix-like systems

## License

[Specify your license here]

## Contributing

[Contribution guidelines here]

## References

- [Gitoxide](https://github.com/Byron/gitoxide)
- [RocksDB](https://rocksdb.org/)
- [NFSv4 RFC](https://tools.ietf.org/html/rfc7530)
- [APFS clonefile(2)](https://developer.apple.com/library/archive/documentation/System/Conceptual/ManPages_iPhoneOS/man2/clonefile.2.html)
