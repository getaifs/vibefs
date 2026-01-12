This specification defines **VibeFS v0.2**, an ephemeral, session-based **virtual filesystem** designed to allow multiple AI agents to work concurrently on a Git repository without physical worktree conflicts.

# Specification: VibeFS v0.2 (The Virtual Workspace Layer)

## 1. System Overview

We have implemented in v0.1 a basic virtual workspace layer that allowed multiple agents to work on the same repository without physical worktree conflicts. However, it was not a true virtual filesystem and was not session-based.

In VibeFS v0.2, we implement a true virtual filesystem that allows multiple agents to work on the same repository without physical worktree conflicts. It uses a background process (**`vibed`**) to serve a virtual filesystem. Coding agents (Claude, Gemini, etc.) are wrapped by a CLI that redirects their working directory to this virtual space.

### Key Axioms:

* **Actor-Session Model:** Work is isolated by `session-id`. Multiple agents can inhabit the same session to collaborate or different sessions to remain isolated.
* **Enterprise-Safe Mounting:** Use **Apple FSKit** (macOS 15+) to provide unprivileged, user-space filesystem mounting. If FSKit is unavailable, fallback to a user-space NFSv4 loopback.
* **Non-Invasive:** The `.git` directory is read-only. All changes are stored in `.vibe/sessions/<session_id>/`.

---

## 2. Technical Architecture

### A. The "Ghost Index" (State Management)

* **Database:** RocksDB located at `.vibe/metadata.db`.
* **Mapping:** Tracks `(session_id, path) -> {Inode, GitOid, IsDirty, DeltaPath}`.
* **Axiom:** The background process (`vibed`) owns the DB lock. All CLI commands communicate via a Unix Domain Socket (UDS).

### B. The Filesystem Provider (For now, focus on MacOS: FSKit)

* **Provider:** Implement a `VFS` provider using Apple's `FSKit`.
* **Logic:**
* **Lookup/Read:** Check the session's delta directory. If the file is missing, stream the blob from the Git ODB using `gitoxide`.
* **Write:** Redirect all writes to `.vibe/sessions/<session_id>/<path>`.
* **Deduplication:** Use APFS `clonefile` (reflinks) when materializing files from the global context to a session to save space.



### C. The Handy Command Wrapper

Regardless of the vibe coding tool (claude, codex, gemini, etc), we provide a simple wrapper to run the tool such that they can work with the virtual filesystem.

* **Usage:** `vibe [binary] [args]` (e.g., `vibe gemini --edit "fix bug"`)
* **Flow:**
1. Find/Start `vibed` for the current repo.
2. Check if the session mount (default or specified via `--session`) is active.
3. If not, request `vibed` to mount the session via FSKit to `/tmp/vibe/<session_id>`.
4. Change current directory to the mount point.
5. `execvp` the target binary with original arguments.



---

## 3. Implementation Plan (Step-by-Step)

In each step, write tests first and verify your work with `cargo test` or other means before you move to the next step. Write down the expectation and outcome.

### Step 1: The Ephemeral Daemon Skeleton

* Implement a Rust binary `vibed` that self-daemonizes (forks to background).
* Implement a Unix Domain Socket (UDS) listener for IPC.
* Implement a "Linger" timer: exit if no filesystem activity is detected for 20 minutes.
* **Library Suggestion:** `daemonize`, `interprocess` (for UDS).

### Step 2: FSKit / Userspace FS Integration

* Integrate a crate for userspace filesystems (search for `fskit-rust` bindings or use a FUSE-to-FSKit bridge).
* Implement the `read` logic:
* Primary: `.vibe/sessions/<id>/<path>`
* Secondary (Fallback): Git Object Database.


* Implement the `write` logic:
* Always write to `.vibe/sessions/<id>/<path>`.
* Update the RocksDB "Dirty" bit for that path.



### Step 3: Enterprise-Safe Mounting

* Ensure the mount point is within the user's home directory (e.g., `~/Library/Caches/vibe/mounts/<id>`) to avoid permissions issues.
* Verify that `vibed` can mount and unmount without requiring `sudo`.

### Step 4: The "Vibe Prefix" CLI

* Implement the command wrapper logic.
* Add logic to parse a `--session <name>` flag before the binary name.
* Example: `vibe --session refactor-auth gemini` should result in a mount specifically for that session.

### Step 5: Promotion & Convergence

* Implement `vibe promote [session_id]`.
* Scan the session's delta folder.
* Use `gitoxide` to hash and write blobs to `.git/objects`.
* Create a Git Tree and a "Phantom Commit" at `refs/vibes/<session_id>`.

---

## 4. Specific Guidance for the Agent

* **Git Operations:** Use the `gix` (gitoxide) crate. It is faster and more memory-safe than `libgit2`.
* **Concurrency:** Use `tokio` for the UDS listener and the FS request handling.
* **Path Mapping:** When an agent runs a tool, it expects absolute paths to work. Ensure the virtual FS correctly handles absolute path translations between the physical repo and the mount point.
* **Agency Needed:** * Decide on the best FSKit bindings for Rust (the ecosystem is moving fast; use the most stable FUSE-compatible layer if direct FSKit is too raw).
* Design the UDS protocol for "Heartbeat" checks between the CLI and the Daemon.
* Make sure the TUI (`vibe dashboard`) works with the newly added features.



---

### Definition of Success For Filesystem Operations (MVP)

1. Run `vibe ls`. `vibed` starts, mounts a virtual folder, and `ls` lists files from the latest Git commit. It only works under a .git repository for now.
2. Run `vibe sh -c "echo 'hello' > test.txt"`.
3. The file `test.txt` exists in `.vibe/sessions/default/test.txt` but **not** in the physical repo.
4. Run `vibe promote`. A new commit appears in `git log refs/vibes/default`.