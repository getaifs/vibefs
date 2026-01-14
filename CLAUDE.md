# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

VibeFS is a "Clean Slate" virtual filesystem designed to enable **Massively Parallel AI Agent workflows** on a single Git repository. It's a Rust-based userspace NFS server that decouples the worktree from storage, allowing 100+ agents to work in isolated, zero-copy sandboxes while maintaining a single source of truth in Git.

## Core Architecture

### Fundamental Axioms
1. **Non-Invasiveness:** The primary `.git` directory is the source of truth. VibeFS is a sidecar overlay.
2. **Locality:** All metadata and session data live in `.vibe/` at project root.
3. **Virtualization:** Workspaces are ephemeral NFS mounts serving files from Git blobs (read) or session deltas (write).
4. **Cheap Snapshots:** State transitions use APFS/Linux-Reflinks for instant, zero-cost snapshots.

### Storage Layout
```
<project-root>/
├── .git/                 # Standard Git ODB (source of truth)
├── .vibe/                # VibeFS Sidecar
│   ├── metadata.db       # RocksDB: Inode <-> Oid mapping
│   ├── sessions/         # Writable deltas per vibe-id
│   │   └── <vibe-id>/    # CoW layer for specific agent
│   └── cache/            # Global CAS for build artifacts (node_modules)
```

### Technology Stack
- **Language:** Rust (for safety and speed)
- **FS Interface:** Userspace NFSv3 Server (via `nfsserve` crate)
- **Git Engine:** `gitoxide` (gix) for high-speed ODB access
- **Database:** RocksDB for metadata persistence
- **TUI:** Ratatui for the dashboard

## Core Commands (Target API)

### `vibe init`
Hydrates metadata by scanning `.git`, populating RocksDB with inode mappings for all files at current HEAD. No file copying—purely metadata-driven.

### `vibe spawn <vibe-id>`
Creates isolated NFS workspace:
1. Ensures daemon is running (starts if needed)
2. Creates session directory in `.vibe/sessions/<vibe-id>/`
3. Creates mount point at `~/Library/Caches/vibe/mounts/<repo>-<vibe-id>/` (macOS)
4. Mounts NFS share via the daemon's NFSv3 server

### `vibe snapshot`
Creates zero-cost recovery point using `clonefile(2)` (macOS) or `ioctl_ficlonerange` (Linux) to duplicate session directory.

### `vibe promote`
Serializes agent work into Git:
1. Diffs `.vibe/sessions/<vibe-id>/` to find modifications
2. Hashes new blobs via gitoxide into `.git/objects`
3. Builds new Git tree by merging HEAD tree with new blobs
4. Creates draft commit with current HEAD as parent
5. Points `refs/vibes/<vibe-id>` to new commit

### `vibe close <vibe-id>`
Closes a session:
1. Unmounts the NFS share
2. Cleans up session directory (optionally preserving dirty files)
3. Use `git merge refs/vibes/<vibe-id>` to merge promoted changes into main

## Implementation Requirements

### Inode-to-Git Mapping (RocksDB)
Bi-directional mapping for NFS:
- `inode_id` → `{path, git_oid, is_dir, size}`
- `path` → `inode_id`
- On READ, if not dirty, stream blob directly from `.git` ODB using Oid

### Untracked Files & Build Context
Files like `.env` or `node_modules`:
- Treated as "Virtual Layers"
- Injected into NFS mount via inodes mapped to `.vibe/cache/` or parent root
- Marked as `volatile` in RocksDB
- Excluded from `vibe promote` unless explicitly whitelisted

### TUI Dashboard Views (Ratatui)
"Air Traffic Control" for parallel agents:
1. **Fleet Overview:** Active vibe-ids, uptime, drift (dirty file count)
2. **Diff Monitor:** Real-time change stream from NFS write-buffer
3. **Conflict Matrix:** Heatmap of concurrent file modifications
4. **Promotion Queue:** `refs/vibes/*` awaiting commit to main

### Concurrency & Performance
- Use `tokio` for async NFS server handling simultaneous I/O
- Lazy load blobs from Git only on first READ request per inode
- CLI commands use read-only RocksDB access to avoid lock contention with daemon

## Development Notes

This is a greenfield project implementing a novel filesystem abstraction. When developing:

1. The Git ODB is read-only from VibeFS's perspective—never write to `.git` except through proper gitoxide APIs during `promote`.

2. All write operations from agents go to `.vibe/sessions/<vibe-id>/`, which acts as a CoW overlay layer.

3. The NFSv3 implementation maps POSIX file operations to this hybrid read-from-git/write-to-session architecture.

4. RocksDB metadata is the performance-critical path—design schema carefully for fast inode lookups during NFS operations.

5. Reflink/CoW snapshot support is OS-dependent—abstract this behind a platform-specific trait for macOS (APFS) vs Linux (Btrfs/XFS).



## VibeFS Workflow

This repository uses VibeFS for managing parallel AI agent workflows on Git.

### Core Workflow

When working on features, follow this workflow:

1. **Initialize** (first time only):
   ```bash
   vibe init
   ```

2. **Spawn your workspace**:
   ```bash
   vibe spawn <agent-id>
   ```
   Creates an isolated session with NFS mount at `~/Library/Caches/vibe/mounts/<repo>-<agent-id>/`

3. **Work in the mount point**:
   ```bash
   # Get the mount path
   vibe path <agent-id>

   # Or use vibe sh to run commands
   vibe sh -s <agent-id> -c "your command here"

   # Or launch an agent directly
   vibe launch claude --session <agent-id>
   ```
   Changes are automatically tracked by the daemon.

4. **Promote to Git commit**:
   ```bash
   vibe promote <agent-id>
   ```
   Creates a commit at `refs/vibes/<agent-id>` with your changes

5. **Merge to main** (when ready):
   ```bash
   git merge refs/vibes/<agent-id>
   ```

6. **Close session**:
   ```bash
   vibe close <agent-id>
   ```

### Key Concepts

- **Sessions**: Isolated NFS-mounted workspaces
- **Automatic dirty tracking**: Daemon tracks file modifications via NFS writes
- **Zero-cost snapshots**: `vibe snapshot` creates instant backups
- **Git integration**: All changes flow through proper Git commits
- **Parallel work**: Multiple agents can work simultaneously in separate sessions

### Example Session

```bash
# Start working on a feature
vibe spawn feature-auth

# Launch Claude in the workspace
vibe launch claude --session feature-auth

# Or work manually
vibe sh -s feature-auth -c "echo 'impl auth' > auth.rs"

# Promote changes to a Git ref
vibe promote feature-auth

# Merge to main
git merge refs/vibes/feature-auth

# Clean up
vibe close feature-auth
```
