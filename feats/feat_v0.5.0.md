# VibeFS v0.5.0 Specification

> **Status**: Draft
> **Scope**: Focused subset of P0/P1 user stories
> **Target**: Improved single-agent and multi-agent CLI workflows

---

## 1. Overview

This spec defines the v0.5.0 milestone with a focused scope:

1. **Single-agent isolation** — spawn with auto-naming, diff, launch agents
2. **Multi-agent orchestration** — conflict detection, batch promote
3. **Session lifecycle** — restore from snapshots, inspect metadata
4. **Git integration** — diffs, partial promote
5. **Observability** — enhanced status, inspect command

---

## 2. Current State (v0.2.9 Baseline)

### Implemented
| Feature | Command | Status |
|---------|---------|--------|
| Initialize repo | `vibe init` | ✅ |
| Spawn session | `vibe spawn <id>` | ✅ |
| Snapshot | `vibe snapshot <id>` | ✅ |
| Promote to ref | `vibe promote <id>` | ✅ |
| Close session | `vibe close <id>` | ✅ |
| Purge all | `vibe purge` | ✅ |
| Status | `vibe status` | ✅ |
| Dashboard TUI | `vibe dashboard` | ✅ |
| Daemon lifecycle | `vibe daemon start/stop/status` | ✅ |

### This Spec Adds
| Feature | Command | Priority |
|---------|---------|----------|
| Auto-named spawn | `vibe spawn` (no args) | P0 |
| Launch agent | `vibe launch <agent>` | P0 |
| View diff | `vibe diff <session>` | P0 |
| Restore snapshot | `vibe restore <session> --snapshot <name>` | P1 |
| Conflict detection | `vibe status --conflicts` | P1 |
| Batch promote | `vibe promote --all` | P1 |
| Partial promote | `vibe promote <session> --only <paths>` | P1 |
| Inspect metadata | `vibe inspect <session>` | P1 |
| Per-session status | `vibe status <session>` | P1 |

---

## 3. CLI Specification

### 3.1 `vibe spawn` — Enhanced

**Current**: `vibe spawn <vibe-id>` (required name)

**New**: Support optional name with auto-generation

```
vibe spawn [<vibe-id>]

Options:
  --debug           Enable verbose NFS/mount logging
  --env KEY=VALUE   Set session-specific environment variable (P2, stub only)
```

**Auto-naming Algorithm** (when no `<vibe-id>` provided):
```
adjective + "-" + noun
```

Word lists (embed in binary) [see docker names: https://raw.githubusercontent.com/bearjaws/docker-names/refs/heads/master/index.js ]:

**Collision handling**: If generated name exists, append `-2`, `-3`, etc.

**Output**:
```
Session 'clever-fox' spawned at /tmp/vibe/clever-fox
```

---

### 3.2 `vibe launch <agent>` — New Command

Combines spawn + agent execution in one step.

```
vibe launch <agent> [--session <name>]

Arguments:
  <agent>           Binary name: claude, cursor, aider, code, etc.

Options:
  --session <name>  Use specific session name (default: adj-<agent>, e.g. "clever-claude")
```

**Implementation**:
1. Generate session name: `<adjective>-<agent>` (e.g., "calm-claude")
2. Call `vibe spawn <session-name>`
3. Set `CWD` to mount path
4. `exec` the agent binary (replaces current process)

**Error handling**:
What if the agent binary doesn't exist? We should check PATH and fail gracefully
with a helpful message listing common agent binaries and (do you mean X if editing distance allows).
```
Error: Binary 'cluade' not found in PATH.
Did you mean: claude, cursor, code, codex, amp, aider?
```

---

### 3.3 `vibe diff <session>` — New Command

Show unified diff of session changes against base commit.

```
vibe diff <session> [options]

Options:
  --stat            Show diffstat summary only
  --color <when>    Color output: auto|always|never (default: auto)
  --no-pager        Disable pager (less)
```

**Implementation**:
1. Get dirty files from RocksDB: `db.get_dirty_paths()`
2. For each dirty file:
   - Get base content: `git show <spawn-commit>:<path>`
   - Get current content: read from `.vibe/sessions/<session>/<path>`
   - Generate unified diff with `similar` crate or shell out to `diff`
3. Handle new files (show as all `+` lines)
4. Handle deleted files (show as all `-` lines)

Need to track the spawn commit (HEAD at spawn time) in session metadata.
Currently `.vibe/sessions/<id>.json` exists but may not contain parent commit.
Verify and add if missing.

**Output format**:
```diff
diff --vibe a/src/auth.rs b/src/auth.rs
--- a/src/auth.rs
+++ b/src/auth.rs
@@ -10,6 +10,7 @@
 fn authenticate() {
+    validate_token();
     // ...
 }
```

---

### 3.4 `vibe restore <session>` — New Command

Restore session state from a snapshot.

```
vibe restore <session> --snapshot <name>

Options:
  --snapshot <name>  Snapshot name to restore from (required)
```

**Implementation**:
1. Verify snapshot exists: `.vibe/sessions/<session>_snapshot_<name>/`
2. Delete current session delta: `rm -rf .vibe/sessions/<session>/`
3. Copy snapshot to session: `cp -c` (clonefile) or `cp -r --reflink=auto`
4. Clear dirty tracking in RocksDB for this session
5. Re-scan restored files and mark as dirty

**Auto-backup behavior**: Before restore, auto-snapshot current state as `<session>_snapshot_pre-restore-<timestamp>`. Print message: `Backed up current state to snapshot 'pre-restore-<timestamp>'`

Add `--no-backup` flag to skip this.

---

### 3.5 `vibe status` — Enhanced

Add conflict detection and per-session details.

```
vibe status [<session>] [options]

Options:
  --conflicts       Show cross-session file conflicts
  --json            Output as JSON
```

**Without arguments**: Show overview (current behavior + enhancements)

```
DAEMON: RUNNING (PID: 12345, uptime: 2h 15m)

ACTIVE SESSIONS (3):
  SESSION          DIRTY   UPTIME   MOUNT
  feature-auth     5       45m      /tmp/vibe/feature-auth
  refactor-ui      12      30m      /tmp/vibe/refactor-ui
  test-suite       0       15m      /tmp/vibe/test-suite

OFFLINE SESSIONS (2):
  backup-v1, experiment-2
```

**With `<session>` argument**: Show details for specific session

```
SESSION: feature-auth
  Mount:     /tmp/vibe/feature-auth
  Uptime:    45m
  Base:      abc1234 (main, 2 hours ago)
  Dirty:     5 files
  Snapshots: checkpoint-1, before-refactor

DIRTY FILES:
  M src/auth.rs
  M src/auth/token.rs
  A src/auth/oauth.rs
  D src/legacy_auth.rs
  M tests/auth_test.rs
```

**With `--conflicts`**: Show overlapping dirty files

```
CROSS-SESSION CONFLICTS:

  src/auth.rs
    Modified by: feature-auth, refactor-ui

  src/utils.rs
    Modified by: feature-auth, test-suite, refactor-ui

RECOMMENDATION: Review conflicts before promoting. Use 'vibe diff <session>' to inspect.
```

---

### 3.6 `vibe promote` — Enhanced

Add batch and partial promotion.

```
vibe promote <session> [options]
vibe promote --all

Options:
  --all             Promote all dirty sessions
  --only <paths>    Only promote specified paths (glob patterns supported)
  --message <msg>   Custom commit message
```

**Partial promote** (`--only`):
1. Filter dirty files by provided glob patterns
2. Only include matched files in phantom commit
3. Keep unmatched files as dirty in session

<!-- ISSUE: User story US-5.4 says "Glob patterns supported" but doesn't specify glob syntax.
     Use standard shell glob (*, **, ?) via the `glob` crate. Document in help text. -->

**Batch promote** (`--all`):
1. Get all sessions with dirty files
2. Promote each in sequence
3. Report success/failure per session

```
Promoting 3 sessions...
  feature-auth: ✓ refs/vibes/feature-auth -> abc1234
  refactor-ui:  ✓ refs/vibes/refactor-ui -> def5678
  test-suite:   ✗ No dirty files to promote
Done. 2 promoted, 1 skipped.
```

---

### 3.7 `vibe inspect <session>` — New Command

Dump session metadata for debugging.

```
vibe inspect <session> [options]

Options:
  --json            Output as JSON (for scripting)
```

**Output**:
```
SESSION: feature-auth

Metadata:
  ID:           feature-auth
  Created:      2026-01-13 10:30:00 UTC
  Mount Point:  /tmp/vibe/feature-auth
  NFS Port:     52341

Git State:
  Base Commit:  abc1234def567890...
  Base Branch:  main
  Phantom Ref:  refs/vibes/feature-auth (exists)

Storage:
  Delta Path:   .vibe/sessions/feature-auth/
  Delta Size:   1.2 MB (5 files)
  Snapshots:    2 (checkpoint-1, before-refactor)

Dirty Files (5):
  src/auth.rs              (modified, 2.4 KB)
  src/auth/token.rs        (modified, 1.1 KB)
  src/auth/oauth.rs        (new, 3.2 KB)
  src/legacy_auth.rs       (deleted)
  tests/auth_test.rs       (modified, 0.8 KB)
```

---

## 4. Error Messages

### 4.1 Mount Failures

Current errors are cryptic. Improve with actionable messages.

| Error | Message |
|-------|---------|
| NFS port in use | `Error: Port 52341 already in use. Another VibeFS daemon may be running. Run 'vibe daemon status' to check.` |
| Mount failed | `Error: Failed to mount NFS share. Ensure NFS is enabled: 'sudo nfsd enable' (macOS) or install nfs-common (Linux).` |
| Permission denied | `Error: Mount permission denied. On macOS, grant Full Disk Access to Terminal in System Preferences > Privacy.` |
| Timeout | `Error: Mount timed out after 30s. The NFS server may be unresponsive. Run 'vibe daemon status' for diagnostics.` |

### 4.2 Session Errors

| Error | Message |
|-------|---------|
| Session not found | `Error: Session 'foo' not found. Run 'vibe status' to see active sessions.` |
| Session has dirty files | `Warning: Session 'foo' has 5 uncommitted files. Use '--force' to close anyway, or 'vibe promote foo' first.` |
| Snapshot not found | `Error: Snapshot 'bar' not found for session 'foo'. Run 'vibe status foo' to see available snapshots.` |

---

## 5. Implementation Order

### Phase 1: Foundation
1. Session metadata enhancement (add `spawn_commit` to `.json`)
2. `vibe spawn` auto-naming
3. `vibe diff`

### Phase 2: Session Lifecycle
4. `vibe restore`
5. `vibe inspect`
6. Enhanced `vibe status` (per-session details)

### Phase 3: Multi-Agent
7. `vibe status --conflicts`
8. `vibe promote --all`
9. `vibe promote --only`

### Phase 4: Agent Launch
10. `vibe launch`

---

## 6. Testing Requirements

### Unit Tests

| Module | Tests |
|--------|-------|
| `commands/spawn.rs` | Auto-naming generation, collision handling |
| `commands/diff.rs` | Unified diff generation, binary files, new/deleted files |
| `commands/restore.rs` | Snapshot restore, auto-backup, dirty tracking reset |
| `commands/launch.rs` | Binary lookup, "did you mean" suggestions |

### Integration Tests (E2E)

| Test | Covers |
|------|--------|
| `test_spawn_auto_name` | US-2.1.1: Spawn without name |
| `test_launch_agent` | US-2.3: Launch agent in session |
| `test_diff_output` | US-5.2: View diff before promote |
| `test_restore_snapshot` | US-4.4: Restore from snapshot |
| `test_conflict_detection` | US-3.5: Cross-session conflicts |
| `test_batch_promote` | US-3.4: Promote all sessions |
| `test_partial_promote` | US-5.4: Cherry-pick files |
| `test_session_status` | US-2.5: Per-session dirty file listing |

---

## 7. Out of Scope (Deferred to v0.6+)

| Feature | User Story | Reason |
|---------|------------|--------|
| `vibe commit` (merge to HEAD) | US-5.3 | Requires careful working-tree handling |
| Context injection (`.vibe/context/`) | US-6.1 | NFS complexity |
| Daemon crash recovery | US-8.1 | Requires daemon changes |
| Git drift detection | US-8.2 | Requires daemon changes |
| Scoped sessions (`--scopes`) | US-3.3 | P2 |
| Session timeout/auto-cleanup | US-4.5 | P2 |
| Env var overrides (`--env`) | US-6.3 | P2 |
| Prometheus metrics | US-7.3 | P3 |

---

## Appendix A: Word Lists for Auto-Naming

Use docker-names word lists: https://github.com/bearjaws/docker-names

Extract adjectives and nouns arrays, embed in `src/names.rs`.

---

## Appendix B: Command Quick Reference (v0.5.0)

```
# Sessions (new/enhanced in v0.5.0)
vibe spawn [<name>]                       # Auto-name if omitted ★
vibe launch <agent> [--session <name>]    # Spawn + exec agent ★
vibe status [<session>]                   # Per-session details ★
vibe status --conflicts                   # Cross-session conflicts ★

# Snapshots (new in v0.5.0)
vibe restore <session> --snapshot <name>  # Restore from snapshot ★

# Git Integration (new/enhanced in v0.5.0)
vibe diff <session> [--stat]              # View changes ★
vibe promote --all                        # Batch promote ★
vibe promote <session> --only <paths>     # Partial promote ★

# Observability (new in v0.5.0)
vibe inspect <session> [--json]           # Session metadata ★

# Existing (unchanged)
vibe init
vibe spawn <name>
vibe snapshot <session> [<name>]
vibe promote <session>
vibe close <session> [--force]
vibe dashboard
vibe purge [--force]
vibe daemon start|stop|status
```

★ = New or enhanced in v0.5.0
