# VibeFS Architecture

## Overview

VibeFS provides isolated Git workspaces for parallel AI agents. Each agent sees a complete repository via an NFS mount, but writes are isolated to a per-session overlay.

```
┌─────────────────────────────────────────────────────────────┐
│                     Agent's View (NFS Mount)                │
│                                                             │
│   Appears as a normal Git repository                        │
│   All files readable, all writes isolated                   │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                    VibeFS Daemon (vibed)                    │
│                                                             │
│   NFSv3 server on localhost                                 │
│   Routes reads/writes through layered storage               │
│   Tracks dirty files in RocksDB                             │
└─────────────────────────────────────────────────────────────┘
                              │
         ┌────────────────────┼────────────────────┐
         ▼                    ▼                    ▼
┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐
│  Session Layer  │  │ Passthrough     │  │   Git Layer     │
│                 │  │                 │  │                 │
│ .vibe/sessions/ │  │ Repo filesystem │  │ .git/objects/   │
│ (dirty files)   │  │ (untracked)     │  │ (tracked)       │
└─────────────────┘  └─────────────────┘  └─────────────────┘
```

## Core Principles

### 1. Non-Invasiveness
- `.git/` is the source of truth—VibeFS never modifies it directly
- All VibeFS data lives in `.vibe/` sidecar directory
- Repository works normally without VibeFS

### 2. Layered Storage
Reads cascade through layers until content is found:
1. **Session Layer**: Check `.vibe/sessions/<id>/` for dirty files
2. **Passthrough Layer**: Check repo filesystem for untracked files
3. **Git Layer**: Read from Git object database

Writes always go to the session layer.

### 3. Copy-on-Write Isolation
- Each session has isolated storage in `.vibe/sessions/<id>/`
- Agents can modify any file without affecting other sessions
- Snapshots use filesystem reflinks for zero-cost copies

### 4. Automatic Dirty Tracking
- The daemon intercepts NFS writes and marks paths as dirty
- No manual marking required
- Dirty state persists in RocksDB

## Components

### Daemon (vibed)
Long-running process that:
- Serves NFSv3 on localhost (high port, no root required on macOS)
- Maintains RocksDB metadata store
- Tracks dirty files per session
- Handles multiple sessions concurrently

### CLI (vibe)
User interface for:
- Session lifecycle: `spawn`, `close`, `status`
- Git integration: `promote`, `diff`
- Recovery: `snapshot`, `restore`
- Monitoring: `dashboard`

CLI uses read-only RocksDB access to avoid lock contention with daemon.

### Storage Layout

```
<repo>/
├── .git/                    # Git repository (source of truth)
├── .vibe/                   # VibeFS sidecar
│   ├── metadata.db/         # RocksDB (inode mappings, dirty tracking)
│   ├── sessions/
│   │   ├── <session-id>/    # Session's modified files
│   │   └── <session-id>.json # Session metadata
│   └── cache/               # Shared artifacts (future)
└── ...                      # Normal repo files
```

## Platform Support

### macOS
- NFS auto-mounts using `noresvport` option (no root required)
- Mount point: `~/Library/Caches/vibe/mounts/<repo>-<session>/`
- CoW snapshots via APFS `clonefile(2)`

### Linux
- NFS mounting requires root (`sudo mount`)
- Mount point: `~/.cache/vibe/mounts/<repo>-<session>/`
- CoW snapshots via reflinks (Btrfs/XFS)
- Session directory mode works without root

### Both Platforms
- Session directories always work: `.vibe/sessions/<id>/`
- RocksDB metadata store
- Git integration via gitoxide

## Data Flow

### Read Path
```
NFS READ request for "src/main.rs"
    │
    ├─► Check dirty in session? ──YES──► Read .vibe/sessions/<id>/src/main.rs
    │         │
    │         NO
    │         ▼
    ├─► Has git_oid in metadata? ──YES──► Read blob from .git/objects/
    │         │
    │         NO
    │         ▼
    └─► Exists on filesystem? ──YES──► Read from repo (passthrough)
              │
              NO
              ▼
         Return ENOENT
```

### Write Path
```
NFS WRITE request for "src/main.rs"
    │
    ├─► Ensure parent dirs exist in session
    │
    ├─► Copy original content if first write (CoW)
    │
    ├─► Write to .vibe/sessions/<id>/src/main.rs
    │
    └─► Mark dirty: dirty:<session>:src/main.rs = true
```

### Promote Path
```
vibe promote <session>
    │
    ├─► Scan .vibe/sessions/<id>/ for files
    │
    ├─► Filter out gitignored paths
    │
    ├─► For each promotable file:
    │       ├─► Hash content as Git blob
    │       └─► Add to Git tree
    │
    ├─► Create commit with HEAD as parent
    │
    └─► Update refs/vibes/<session>
```

## Build Artifact Handling

Build tools (Cargo, npm, pip) don't work well with NFS due to:
- macOS `copyfile()` xattr issues
- NFS file locking limitations

Solution: Symlink artifact directories to local storage:
```
.vibe/sessions/<id>/
├── target -> /tmp/vibe-artifacts/<id>/target
├── node_modules -> /tmp/vibe-artifacts/<id>/node_modules
├── .venv -> /tmp/vibe-artifacts/<id>/.venv
└── ...
```

These symlinks are:
- Created automatically on session spawn
- Registered in NFS metadata (visible through mount)
- Cleaned up on session close

## Concurrency Model

- **Daemon**: Single process, async I/O via Tokio
- **NFS**: Multiple concurrent client connections supported
- **RocksDB**: Daemon holds write lock; CLI uses read-only access
- **Sessions**: Independent, no cross-session locking

## Failure Modes

### Daemon Crash
- Sessions become unmountable (stale NFS handles)
- Recovery: `vibe purge` or restart daemon
- Session data in `.vibe/sessions/` is preserved

### Stale Mounts
- NFS mounts can become stale if daemon restarts
- `vibe spawn` detects and cleans stale mounts
- Manual cleanup: `umount -f` (macOS) or `umount -l` (Linux)

### RocksDB Lock Contention
- Daemon holds write lock
- CLI operations use read-only access
- `vibe restore` requires stopping daemon (needs write access)
