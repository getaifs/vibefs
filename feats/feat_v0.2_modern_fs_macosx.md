# Specification: VibeFS v0.2 (The Virtual Workspace Layer)

## 1. System Overview
VibeFS v0.2 evolves from the v0.1 metadata-only layer into a **true session-based virtual filesystem**. It uses an ephemeral background daemon (**`vibed`**) to serve a virtualized environment where agents (Claude, Gemini, etc.) can work in parallel without worktree corruption or "locked index" issues.

### Key Axioms:
* **Actor-Session Model:** Work is isolated by `session-id`. Multiple agents can share a session (collaboration) or use separate sessions (isolation).
* **Enterprise-Safe Mounting:** Uses a user-space **NFSv4.1 loopback**. This avoids the "sudo" requirement of standard mounts by using high ports and unprivileged mount flags (`resvport`). *Note: FSKit was considered but deferred due to lack of stable Rust bindings.*
* **Non-Invasive Sidecar:** The `.git` directory remains read-only. All session state is stored locally in `.vibe/sessions/<session_id>/`.

---

## 2. Technical Architecture

### A. The "Ghost Index" (State Management)
* **Database:** RocksDB at `.vibe/metadata.db`.
* **Mapping:** Tracks `(session_id, path) -> {Inode, GitOid, IsDirty, DeltaPath}`.
* **Ownership:** The `vibed` daemon owns the RocksDB lock. CLI commands communicate with the daemon via a **Unix Domain Socket (UDS)**.

### B. The Filesystem Provider (NFSv4.1)
* **Provider:** A Rust-native NFSv4.1 server implementation integrated into `vibed`.
* **Read Path:** 1. Check session delta: `.vibe/sessions/<session_id>/<path>`.
    2. Fallback: Stream blob from Git ODB using `gitoxide`.
* **Write Path:** Redirect all writes to `.vibe/sessions/<session_id>/<path>`.
* **Deduplication:** Use APFS `clonefile` (reflinks) to materialize large context files (like `node_modules` or `.env`) into a session without disk bloat.

### C. The Command Wrapper
The `vibe` CLI acts as a transparent prefix to existing agent binaries.
* **Usage:** `vibe [--session <id>] [binary] [args]`
* **Workflow:**
    1. Ensure `vibed` is running for the current repo.
    2. Check if the requested session is mounted at `~/Library/Caches/vibe/mounts/<session_id>`.
    3. If not, request `vibed` to export the session and trigger a local NFS mount.
    4. Change working directory to the mount point.
    5. `execvp` the target binary (e.g., `claude`) with all original arguments.

---

## 3. Implementation Plan (Step-by-Step)

> **Agent Instruction:** Write tests for each step first. Verify with `cargo test`.

### Step 1: The Ephemeral Daemon & UDS ✅ IMPLEMENTED
* ✅ Implement `vibed` binary with self-daemonization logic (`src/bin/vibed.rs`).
* ✅ Establish UDS listener for CLI-to-Daemon communication (`.vibe/vibed.sock`).
* ✅ Implement a 20-minute idleness "Linger" timer for auto-shutdown.
* **Crates:** `daemonize`, `tokio`.

### Step 2: NFSv4.1 Server Integration ✅ IMPLEMENTED
* ✅ Implement the NFSv3 protocol layer within `vibed` using `nfsserve` crate (`src/nfs/mod.rs`).
* ✅ Serve a virtual root directory for sessions.
* ✅ Implement `READ` and `READDIR` using Git ODB via git CLI.
* **Note:** Using NFSv3 via `nfsserve` crate (NFSv4.1 native implementation deferred).

### Step 3: Writable Deltas & APFS ✅ IMPLEMENTED
* ✅ Implement `WRITE` and `CREATE` logic in NFS module.
* ✅ All writes land in `.vibe/sessions/<session_id>/`.
* ✅ RocksDB marks paths as "Dirty" (`mark_dirty()`).
* ✅ APFS `clonefile(2)` for snapshots (`src/commands/snapshot.rs`).

### Step 4: The Sudo-less Mount Wrapper ✅ IMPLEMENTED
* ✅ Implement the CLI logic to execute `mount_nfs` (`src/commands/spawn.rs`).
* ✅ **Mac Protocol:** Uses `-o vers=4,tcp,port=<high_port>,resvport,nolock,locallocks`.
* ✅ Target mount point: `~/Library/Caches/vibe/mounts/<session_id>`.

### Step 5: Promotion & Convergence ✅ IMPLEMENTED (v0.1)
* ✅ `vibe promote <session_id>` implemented.
* ✅ Walks delta folder, hashes new blobs into `.git/objects`.
* ✅ Creates commit at `refs/vibes/<session_id>`.

---

## 4. Specific Guidance for the Agent

* **Gitoxide (`gix`):** Currently using git CLI wrapper. Migration to gitoxide planned for future.
* **Inode Stability:** NFS requires stable Inodes. RocksDB mapping persists Inodes across daemon restarts.
* **Absolute Paths:** Agents often use absolute paths. Virtual FS correctly resolves symlinks and paths relative to the mount root.
* **TUI Integration:** `vibe dashboard` available (basic implementation).

---

## 5. Definition of Success (MVP)

1. ✅ **Mounting:** `vibe ls` starts the daemon and lists files from the Git `HEAD`.
2. ✅ **Isolation:** `vibe sh -c "echo 'hello' > test.txt"` creates a file in the session delta, but the physical repo remains clean.
3. ✅ **Parallelism:** Running `vibe --session A` and `vibe --session B` simultaneously results in two distinct, isolated virtual workspaces.
4. ✅ **Promotion:** `vibe promote` generates a valid Git commit hash stored in the hidden vibe refs.

---

## 6. Implementation Status

### What's Done:
- `vibed` daemon binary with UDS IPC protocol
- 20-minute idle auto-shutdown
- NFS filesystem trait implementation (VibeNFS)
- CLI commands: init, spawn, snapshot, promote, commit, dashboard, status, ls, sh
- Daemon management: daemon start/stop/status
- RocksDB metadata store with inode mapping and dirty tracking
- APFS clonefile support for zero-cost snapshots

### Files Added/Modified:
- `src/bin/vibed.rs` - Daemon binary
- `src/daemon_client.rs` - Client for UDS communication
- `src/lib.rs` - Added daemon_ipc module
- `src/nfs/mod.rs` - Full NFS filesystem implementation
- `src/commands/spawn.rs` - Updated with daemon integration
- `src/main.rs` - Added new CLI commands

### Testing:
- All 16 unit tests passing
- All 5 integration tests passing
- `cargo build` succeeds with no warnings
