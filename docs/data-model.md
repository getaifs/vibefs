# VibeFS Data Model

This document describes the core data structures and storage schema used by VibeFS.

## Overview

VibeFS uses a hybrid storage approach:
- **RocksDB** for metadata (inode mappings, dirty tracking)
- **Filesystem** for session data (modified files)
- **Git ODB** for tracked file content (read-only)

## RocksDB Schema

The metadata store uses a flat key-value schema with prefixed keys.

### Key Patterns

| Prefix | Format | Value Type | Description |
|--------|--------|------------|-------------|
| `inode:` | `inode:{id}` | JSON | Inode metadata |
| `path:` | `path:{path}` | u64 (LE) | Path to inode mapping |
| `dirty:` | `dirty:{path}` | `"1"` | Dirty file marker |
| `counter:` | `counter:inode` | u64 (LE) | Next inode ID |

### InodeMetadata Structure

```rust
struct InodeMetadata {
    path: String,           // Relative path from repo root
    git_oid: Option<String>,// Git blob OID (None for new files)
    is_dir: bool,           // Directory flag
    size: u64,              // File size in bytes
    volatile: bool,         // Exclude from promotion
}
```

**Field semantics:**

- `path` - Relative path without leading slash (e.g., `src/main.rs`)
- `git_oid` - For tracked files, the Git blob SHA. For symlinks, `symlink:{target}`. For new files, `None`
- `is_dir` - True for directories, determines NFS file type
- `size` - Used for NFS getattr responses
- `volatile` - True for gitignored files; these are tracked but excluded from `vibe promote`

### Example Data

```
inode:100 → {"path":"src","git_oid":null,"is_dir":true,"size":0,"volatile":false}
inode:101 → {"path":"src/main.rs","git_oid":"a1b2c3...","is_dir":false,"size":1024,"volatile":false}
inode:102 → {"path":"target","git_oid":"symlink:/tmp/vibe-artifacts/abc/target","is_dir":false,"size":42,"volatile":true}

path:src → 100 (as u64 little-endian)
path:src/main.rs → 101
path:target → 102

dirty:src/main.rs → "1"

counter:inode → 102
```

## Inode ID Allocation

- IDs start at 100 to avoid reserved numbers
- ID 1 is used for the root directory
- Monotonically increasing counter stored in RocksDB

## Daemon IPC Protocol

The CLI communicates with the daemon via Unix Domain Socket using JSON messages.

### Socket Location

```
.vibe/vibed.sock
```

### Request Types

```rust
enum DaemonRequest {
    Ping,                           // Health check
    Status,                         // Get daemon status
    ExportSession { vibe_id },      // Export session to NFS
    UnexportSession { vibe_id },    // Remove session from NFS
    ListSessions,                   // List active sessions
    Shutdown,                       // Graceful shutdown
}
```

### Response Types

```rust
enum DaemonResponse {
    Pong { version },
    Status { repo_path, nfs_port, session_count, uptime_secs, version },
    SessionExported { vibe_id, nfs_port, mount_point },
    SessionUnexported { vibe_id },
    Sessions { sessions: Vec<SessionInfo> },
    ShuttingDown,
    Error { message },
}
```

### SessionInfo Structure

```rust
struct SessionInfo {
    vibe_id: String,      // Session identifier
    mount_point: String,  // NFS mount path
    nfs_port: u16,        // NFS server port
    uptime_secs: u64,     // Session uptime
}
```

## Session Directory Layout

Each session stores modified files in a directory structure that mirrors the repo:

```
.vibe/sessions/<vibe-id>/
├── src/
│   └── main.rs           # Modified file
├── new_file.rs           # New file created in session
├── target -> /tmp/vibe-artifacts/<vibe-id>/target  # Symlink (not promoted)
└── ...
```

**Symlinks for build artifacts:**

To work around NFS xattr issues with build tools (Cargo, npm), artifact directories are symlinked to local storage:

```
.vibe/sessions/<id>/target → /tmp/vibe-artifacts/<id>/target
.vibe/sessions/<id>/node_modules → /tmp/vibe-artifacts/<id>/node_modules
```

These symlinks are:
1. Created when the session is exported to NFS
2. Registered in metadata with `git_oid: "symlink:{target}"`
3. Exposed through NFS as symbolic links
4. Excluded from promotion (volatile flag)

## Git Integration

### Reading from Git ODB

When a file is not dirty and has a `git_oid`:
1. Use gitoxide to look up blob by OID
2. Stream content directly from `.git/objects`
3. Zero-copy operation - no data stored in session

### Promoting to Git

When `vibe promote` runs:
1. Scan session directory for files
2. Filter out gitignored paths
3. For each promotable file:
   - Read content from session directory
   - Hash as Git blob via gitoxide
   - Add to new tree
4. Create commit with HEAD as parent
5. Update `refs/vibes/<vibe-id>`

## Concurrency Model

| Component | Lock Strategy |
|-----------|--------------|
| Daemon | Single writer to RocksDB |
| CLI | Read-only RocksDB access |
| NFS | Async I/O via Tokio |
| Sessions | Independent, no cross-session locks |

The daemon holds the exclusive write lock on RocksDB. CLI commands that only read metadata can run concurrently. Commands that need write access (e.g., `vibe restore`) require stopping the daemon.
