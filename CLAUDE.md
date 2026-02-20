# CLAUDE.md

Guidance for Claude Code when working with this repository.

## Project Overview

VibeFS is a Rust-based userspace NFS server that provides isolated Git workspaces for parallel AI agents. Each agent gets a virtual filesystem backed by Git's object database for reads and a session directory for writes.

## Architecture

### Core Principles
1. **Non-Invasiveness**: `.git` is source of truth; `.vibe/` is a sidecar overlay
2. **Locality**: All VibeFS data lives in `.vibe/`
3. **Virtualization**: NFS mounts serve files from Git (reads) or session deltas (writes)
4. **Automatic Tracking**: Daemon tracks dirty files via NFS write operations

### Storage Layout
```
.vibe/
├── metadata.db       # RocksDB: inode <-> {path, git_oid, is_dir, size}
├── sessions/
│   ├── <id>/         # CoW overlay - modified files written here
│   └── <id>.json     # Session metadata (port, mount point, spawn commit)
└── cache/            # Shared build artifacts
```

### Technology Stack
- **Rust** with Tokio async runtime
- **nfsserve**: Userspace NFSv3 server
- **gitoxide (gix)**: Git object database access
- **RocksDB**: Metadata persistence
- **Ratatui**: TUI dashboard

## Key Implementation Details

### NFS Read/Write Flow
- **Read**: Check session dir first (dirty files), then Git ODB, then repo filesystem (untracked)
- **Write**: Write to session directory, mark path as dirty in RocksDB
- **Symlinks**: Build artifact dirs (target/, node_modules/, etc.) symlink to local storage to avoid NFS xattr issues

### RocksDB Schema
- `inode:<id>` → `InodeMetadata {path, git_oid, is_dir, size, volatile}`
- `path:<path>` → `inode_id`
- `dirty:<id>:<path>` → `true` (marks file as modified in session)
- `counter:inode` → next available inode ID

### CLI vs Daemon
- Daemon holds write lock on RocksDB and serves NFS
- CLI commands use read-only RocksDB access to avoid lock contention
- Commands like `promote` scan session directory directly

## Development

```bash
cargo test              # Run all tests (58 total)
cargo build --release   # Build release binaries
./dev_scripts/install.sh  # Install to ~/.local/bin
```

* Always use dev_scripts/bump.sh to manage versioning, and releaes.sh to release.
* Always run workflow_tests to ensure everything works, before releasing.
* When running on local dev machine, make sure to install and run `vibe` to ensure the binary actually works.

See `dev_scripts/README.md` for release process.
