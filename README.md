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

### Runtime Requirements

- **Git**: 2.30+
- **macOS**: Built-in NFS client (default)
- **Linux**: NFS client tools (`nfs-utils` on Fedora/RHEL, `nfs-common` on Debian/Ubuntu)

### Optional Dependencies for CoW Snapshots

- **macOS**: APFS filesystem (default on modern macOS)
- **Linux**: Btrfs or XFS filesystem with reflink support

### Build Requirements (Source Builds Only)

- **Rust**: 1.70+ (install via [rustup](https://rustup.rs/))
- **C++ Compiler**: Required for RocksDB compilation
  - Linux: `sudo dnf install gcc-c++` (Fedora) or `sudo apt install build-essential` (Ubuntu)
  - macOS: Xcode Command Line Tools (`xcode-select --install`)
- **libclang**: Required for bindgen
  - Linux: `sudo dnf install clang-devel` (Fedora) or `sudo apt install libclang-dev` (Ubuntu)
  - macOS: Included with Xcode Command Line Tools

## Installation

### Quick Install (Prebuilt Releases)

```bash
curl -sSfL https://raw.githubusercontent.com/getaifs/vibefs/HEAD/install.sh | bash
```

This installs `vibe`, `vibed`, and `mark_dirty` to `~/.local/bin`.

### Build From Source (macOS/Linux)

```bash
# Clone and build
git clone https://github.com/getaifs/vibefs.git
cd vibefs
cargo build --release

# Install from local build (cross-platform)
./dev_scripts/install.sh
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
- An NFS mount (macOS automatically; Linux requires a manual `sudo mount` step shown in output)
- Automatic dirty file tracking when working through the NFS mount

### 3. Work in the Workspace

Work directly in the NFS mount point (printed by `vibe spawn`):

```bash
# Use vibe sh to run commands in the session
vibe sh -s agent-1 -c "echo 'hello' > newfile.txt"

# Or launch an agent directly
vibe launch claude --session agent-1
```

Changes are automatically tracked by the daemon.

**Linux note**: `vibe spawn` prints the exact `sudo mount` command you need. If you skip mounting, you can still work directly in `.vibe/sessions/<id>` and `vibe promote` will scan the session directory for changes.

### 4. Create Snapshots (Optional)

Take zero-cost snapshots at any point:

```bash
vibe snapshot agent-1
```

### 5. Promote Changes to Git

Serialize agent work into a Git commit:

```bash
vibe promote agent-1
```

This:
- Hashes modified files as Git blobs
- Creates a new Git tree
- Commits with HEAD as parent
- Points `refs/vibes/agent-1` to the new commit

### 6. Merge to Main (Manual)

After promotion, use standard Git commands to merge:

```bash
git merge refs/vibes/agent-1
# or
git cherry-pick refs/vibes/agent-1
```

### 7. Close Session

When done, close the session:

```bash
vibe close agent-1
```

## Core Commands

| Command | Description |
|---------|-------------|
| `vibe init` | Initialize VibeFS for a Git repository |
| `vibe spawn <id>` | Create an isolated agent workspace with NFS mount |
| `vibe sh -s <id>` | Execute commands in a session's mount point |
| `vibe launch <agent>` | Spawn session and launch an agent (claude, cursor, etc.) |
| `vibe snapshot <id>` | Take a zero-cost CoW snapshot |
| `vibe restore <id>` | Restore session from a snapshot |
| `vibe promote <id>` | Serialize changes into a Git commit |
| `vibe rebase <id>` | Move a session's base commit to current HEAD |
| `vibe close <id>` | Unmount and clean up a session |
| `vibe status` | Show daemon and session status |
| `vibe diff <id>` | Show unified diff of session changes |
| `vibe inspect <id>` | Inspect session metadata for debugging |
| `vibe purge` | Stop daemon and delete all `.vibe` data for the repo |

## Architecture

### Storage Layout

```
<project-root>/
├── .git/                 # Standard Git ODB (source of truth)
├── .vibe/                # VibeFS Sidecar
│   ├── metadata.db       # RocksDB: Inode <-> Oid mapping
│   ├── sessions/         # Writable deltas per vibe-id
│   │   ├── <vibe-id>/    # CoW layer for specific agent
│   │   └── <vibe-id>.json # Session metadata
│   └── cache/            # Global CAS for build artifacts
```

### Mount Points

```
macOS:  ~/Library/Caches/vibe/mounts/<repo>-<session>/
Linux:  ~/.cache/vibe/mounts/<repo>-<session>/
```

### Key Concepts

1. **Non-Invasiveness**: `.git` is the source of truth; VibeFS is a sidecar overlay
2. **Locality**: All metadata lives in `.vibe/`
3. **Virtualization**: Workspaces are ephemeral NFS mounts
4. **Cheap Snapshots**: Uses APFS clonefile / Linux reflinks for zero-cost snapshots
5. **Automatic Tracking**: Daemon tracks dirty files via NFS write operations

### Technology Stack

- **Rust**: Core implementation language
- **gitoxide (gix)**: High-speed Git operations
- **RocksDB**: Metadata persistence
- **nfsserve**: Userspace NFSv3 server
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
├── bin/        # vibed daemon
├── lib.rs      # Library root
└── main.rs     # CLI entry point

tests/
├── integration_tests.rs  # Rust integration tests
└── workflow_tests.sh     # Bash workflow tests
```

### Running Tests

```bash
# Run Rust tests
cargo test

# Run workflow tests (requires built binaries)
./tests/workflow_tests.sh
```
