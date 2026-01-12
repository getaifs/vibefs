# Technical Specification: VibeFS (v1.0)

**Virtualized, Agent-Native Overlay for Concurrent Development**

---

## 1. Executive Summary & Axioms

VibeFS is a "Clean Slate" virtual filesystem designed to enable **Massively Parallel AI Agent workflows** on a single Git repository. It decouples the *worktree* from the *storage*, allowing 100+ agents to work in isolated, zero-copy sandboxes while maintaining a single source of truth in Git.

### The Core Axioms

1. **Axiom of Non-Invasiveness:** The primary Git repository and `.git` directory remain the source of truth. VibeFS is a sidecar, not a replacement.
2. **Axiom of Locality:** All metadata and session data live in the project root under `.vibe/`.
3. **Axiom of Virtualization:** Workspaces are ephemeral NFS mounts. Files are served from Git blobs (read) or session deltas (write).
4. **Axiom of Cheap Snapshots:** Every state transition must be instantaneous and space-efficient using APFS/Linux-Reflinks.

---

## 2. System Architecture

### 2.1 Storage Layout

```text
<project-root>/
├── .git/                 # Standard Git ODB
├── .vibe/                # VibeFS Sidecar
│   ├── metadata.db       # RocksDB: Inode <-> Oid mapping
│   ├── sessions/         # Writable deltas per Vibe ID
│   │   └── <vibe-id>/    # CoW layer for a specific agent
│   └── cache/            # Global CAS for build artifacts (node_modules)

```

### 2.2 The Tech Stack

* **Language:** Rust (Safety/Speed).
* **FS Interface:** Userspace NFSv4 Server (via `nfsserve` or custom).
* **Git Engine:** `gitoxide` (gix) for high-speed ODB access.
* **Database:** RocksDB for metadata persistence.
* **Management:** Ratatui for the TUI Dashboard.

---

## 3. The Core API (Axiomatic Commands)

### 3.1 `vibe init`

**Objective:** Hydrate the metadata.

1. Scans the current `.git` directory.
2. Populates RocksDB with a mapping of all files in the current `HEAD` to unique **u64 Inodes**.
3. Initializes the `.vibe/` directory structure.
4. **Note:** No source files are copied; this is a purely metadata-driven step.

### 3.2 `vibe spawn <vibe-id>`

**Objective:** Create an isolated workspace for an agent.

1. Starts a local NFS server instance on a random high port.
2. Creates a directory at `/tmp/vibe/<vibe-id>`.
3. Mounts the NFS share: `mount_nfs -o vers=4,tcp,port=<port> localhost:/ /tmp/vibe/<vibe-id>`.
4. Initializes a CoW directory in `.vibe/sessions/<vibe-id>/`.

### 3.3 `vibe snapshot`

**Objective:** Create a cheap recovery point.

1. Uses `clonefile(2)` (on Mac) or `ioctl_ficlonerange` (on Linux) to duplicate the `.vibe/sessions/<vibe-id>/` directory.
2. Stores the new directory pointer in RocksDB as a "Snapshot Version."
3. **Cost:** Constant time  and zero initial disk space.

### 3.4 `vibe promote`

**Objective:** Serialize agent work into Git logic.

1. **Diffing:** Walks the `.vibe/sessions/<vibe-id>/` directory to find modified files.
2. **Hashing:** Uses `gitoxide` to hash new blobs and write them into `.git/objects`.
3. **Tree Construction:** Recursively builds a new Git Tree object by merging the original `HEAD` tree with the new blobs.
4. **Draft Commit:** Creates a commit object with the current `HEAD` as the parent.
5. **Phantom Ref:** Points `refs/vibes/<vibe-id>` to this new commit.

### 3.5 `vibe commit`

**Objective:** Finalize the "Vibe" into the main history.

1. Validates the Promotion hash from `refs/vibes/<vibe-id>`.
2. Moves the current branch `HEAD` to this hash.
3. Synchronizes the physical root worktree to match this state (optional/on-demand).
4. Tears down the NFS mount and cleans up the session deltas.

---

## 4. Implementation Details for the AI Agent

### 4.1 The Inode-to-Git Mapping (RocksDB)

You must implement a bi-directional mapping to satisfy NFS requirements.

* **Key:** `inode_id` -> **Value:** `{path, git_oid, is_dir, size}`.
* **Key:** `path` -> **Value:** `inode_id`.
* During a `READ` request, if `is_dirty` is false, stream the blob directly from the `.git` ODB using its Oid.

### 4.2 Handling Untracked Files & Build Context

For files like `.env` or `node_modules`:

* VibeFS treats these as "Virtual Layers."
* On `spawn`, VibeFS can "inject" these files into the NFS mount by mapping their Inodes to files in the `.vibe/cache/` or the parent project root.
* These are marked as `volatile` in RocksDB and are **excluded** from `vibe promote` unless explicitly whitelisted.

---

## 5. TUI Dashboard (Management Layer)

The TUI (built with Ratatui) must provide "Air Traffic Control" for the parallel agents.

### Essential Views:

1. **Fleet Overview:** A list of active `vibe-ids`, their uptime, and "Drift" (number of dirty files).
2. **Diff Monitor:** A real-time stream of changes coming from the NFS write-buffer for a selected vibe.
3. **Conflict Matrix:** A heatmap showing if two vibes are modifying the same file paths.
4. **Promotion Queue:** A list of `refs/vibes/*` waiting for the human to "Commit" them to `main`.

---

## 6. Execution Guidance

* **Concurrency:** Use `tokio` for the NFS server to handle simultaneous I/O from multiple agents.
* **Lazy Loading:** Do not read blobs from Git until the first `READ` request for that Inode is received.
* **Safety:** Implement a "Lock" in RocksDB to prevent two `vibe commit` calls from updating the same branch ref simultaneously.

---

[Check out this intro to Gitoxide and Rust git](https://www.google.com/search?q=https://www.youtube.com/watch%3Fv%3Dw7l3p_I9XQo)

This video is relevant as it introduces the `gitoxide` (gix) library, which is a core component of our spec for high-performance, non-invasive Git operations in Rust.
