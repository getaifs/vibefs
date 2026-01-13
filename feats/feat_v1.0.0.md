# ALL THE FOLLOWING IS OUT OF SCOPE (Marked as Section ##4- ##7 Below)

## 4. Context Injection System (P0/P1)

### 4.1 `.vibe/context/` Directory

Files placed in `.vibe/context/` appear in every session's root.

**Behavior**:
- Read from context if file exists there
- Session writes create session-local copy (CoW)
- Context files override Git files of same name

**Implementation** (NFS layer):
1. On `lookup(parent=root, name)`:
   - First check `.vibe/context/<name>`
   - Then check session delta
   - Then check Git tree
2. On `read(inode)`:
   - If inode maps to context file, read from `.vibe/context/`
3. On `write(inode)`:
   - Always write to session delta (never context)

<!-- ISSUE: The overlay order matters. User story US-6.1 says "Context files override Git files
     of same name" but what about session files? Proposed order:
     1. Session delta (highest priority - agent's changes)
     2. Context directory (user's shared config)
     3. Git HEAD (baseline)
     This means agent can override context files within their session. -->

### 4.2 Dependency Injection (P1)

For `node_modules/`, `venv/`, etc.

**Setup** (user action):
```bash
ln -s ../node_modules .vibe/context/node_modules
```

**Behavior**:
- Symlink followed for reads
- Writes to `node_modules/` go to session delta

<!-- ISSUE: Symlinks to directories outside .vibe/ may break if NFS tries to resolve them
     relative to mount point. Need to handle absolute vs relative symlinks correctly.
     May need to use bind mounts or special handling in NFS layer. -->

---

## 5. Debug & Observability (P0/P1)

### 5.1 Debug Logging

`vibe spawn --debug` enables verbose logging.

**Log destinations**:
- Stdout: Summary messages
- `.vibe/logs/<session>.log`: Full debug trace

**Log content**:
- NFS operation trace (lookup, read, write, readdir)
- RPC negotiation details
- Mount command and output
- Error stack traces

### 5.2 Dashboard Enhancements

Add to existing TUI:

1. **Conflict indicator**: Red badge on sessions with cross-session file conflicts
2. **Dirty file browser**: Press `d` to expand dirty file list (exists, enhance)
3. **Quick actions**: `p` to promote, `c` to close, `i` to inspect

---

## 6. Recovery & Resilience (P0/P1)

### 6.1 Daemon Crash Recovery (P0)

**Current state**: Daemon restarts work, sessions require manual re-export.

**Required behavior**:
1. On `vibe daemon start`, scan `.vibe/sessions/` for existing sessions
2. For each session with `.json` metadata, re-export NFS
3. Re-mount at original mount point
4. Restore dirty tracking from RocksDB

**Implementation**:
```rust
// In vibed.rs startup
fn recover_sessions(vibe_dir: &Path, db: &MetadataStore) -> Vec<Session> {
    let sessions_dir = vibe_dir.join("sessions");
    let mut recovered = vec![];

    for entry in fs::read_dir(sessions_dir) {
        if entry.path().extension() == Some("json") {
            let metadata: SessionMetadata = serde_json::from_reader(...);
            let session = export_session(metadata.id, ...);
            mount_session(&session);
            recovered.push(session);
        }
    }
    recovered
}
```

### 6.2 Git Sync During Active Sessions (P1)

**Scenario**: User runs `git pull` while sessions active.

**Behavior**:
- Sessions continue to see their spawn-time HEAD
- `vibe status` shows warning if HEAD has moved
- `vibe rebase <session>` updates session base (future, P2)

<!-- ISSUE: User story US-8.2 mentions `vibe rebase` but that's more complex. For v0.5.0,
     just warn about drift and require user to close/respawn if they want new baseline. -->

**Implementation**:
1. Store spawn commit in session metadata
2. On `vibe status`, compare spawn commit to current HEAD
3. If different, show warning:
   ```
   WARNING: Repository HEAD has moved since session was spawned.
   Session 'feature-auth' based on: abc1234 (3 commits behind)
   Consider closing and respawning for latest changes.
   ```

### 6.3 Sleep/Wake Resilience (P1)

**Problem**: NFS mounts may become stale after system sleep.

**Solution**:
1. Daemon handles SIGCONT (wake signal)
2. On wake, verify each mount is responsive
3. If stale, attempt remount
4. If remount fails, mark session as "stale" in status

<!-- ISSUE: macOS NFS mounts are notoriously flaky after sleep. May need to use
     soft mounts with timeout, or implement reconnect logic. This is complex and
     may need iteration. -->

---

## 7. Data Model Changes

### 7.1 Session Metadata Schema

Update `.vibe/sessions/<id>.json`:

```json
{
  "id": "feature-auth",
  "created_at": "2026-01-13T10:30:00Z",
  "spawn_commit": "abc1234def567890...",
  "spawn_branch": "main",
  "mount_point": "/tmp/vibe/feature-auth",
  "nfs_port": 52341,
  "agent": null,
  "env_vars": {}
}
```

**New fields**:
- `spawn_commit`: HEAD at spawn time (for diff, drift detection)
- `spawn_branch`: Branch at spawn time (informational)
- `agent`: Agent binary name if spawned via `vibe launch`
- `env_vars`: Session-specific environment (P2, stub for now)

### 7.2 RocksDB Schema

No changes required. Current schema supports:
- Inode → metadata mapping
- Path → inode mapping
- Dirty file tracking

---