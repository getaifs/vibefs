# File State Matrix for VibeFS

This document systematically covers all possible file states across the three systems:
- **Git**: The source of truth for tracked files
- **Filesystem**: The actual repo directory on disk
- **NFS**: The virtual filesystem exposed to agents via VibeFS

## The Three Dimensions

### Git State
- **Tracked**: File is committed in Git at HEAD
- **Untracked**: File exists in repo but not in Git (e.g., Cargo.lock, .env)
- **Staged**: File is in Git index but not committed (not relevant for VibeFS)

### Filesystem State
- **Exists**: File is present in the actual repo directory
- **Missing**: File doesn't exist on disk

### Session State (NFS/VibeFS)
- **Clean**: File not modified in session (read from Git or passthrough)
- **Dirty**: File modified in session (read from session delta)
- **Created**: New file created in session
- **Deleted**: File removed in session (not yet implemented)

---

## Complete State Matrix

| # | Git | Filesystem | Session | Read From | Promotable? | Example |
|---|-----|------------|---------|-----------|-------------|---------|
| 1 | Tracked | Exists | Clean | Git blob | No | `src/main.rs` unchanged |
| 2 | Tracked | Exists | Dirty | Session delta | **Yes** | `src/main.rs` edited |
| 3 | Tracked | Missing | Clean | Git blob | No | File deleted from disk (Git still has it) |
| 4 | Tracked | Missing | Dirty | Session delta | **Yes** | File deleted from disk, recreated in session |
| 5 | Untracked | Exists | Clean | Repo filesystem | No* | `Cargo.lock` |
| 6 | Untracked | Exists | Dirty | Session delta | Depends | `.env` modified in session |
| 7 | Untracked | Missing | Created | Session delta | Depends | New file created |
| 8 | - | - | Created | Session delta | **Yes** | `src/new_feature.rs` |

*Untracked files in "Clean" state are read-only passthrough from repo.

---

## Detailed Scenarios

### Scenario 1: Tracked + Exists + Clean
**Most common case for reading files**
- File is in Git, exists on disk, not modified in session
- NFS reads blob directly from Git ODB
- File is NOT promotable (no changes)
- Example: Reading `README.md` without editing

### Scenario 2: Tracked + Exists + Dirty
**Primary work case - editing tracked files**
- File is in Git, exists on disk, modified in session
- NFS reads from session delta directory
- File IS promotable
- Example: Editing `src/main.rs`

### Scenario 3: Tracked + Missing + Clean
**Rare - disk corruption or manual deletion**
- File in Git but deleted from disk
- NFS still serves from Git blob (virtual view)
- File is NOT promotable (no session changes)
- Example: Someone ran `rm Cargo.toml` but Git still has it

### Scenario 4: Tracked + Missing + Dirty
**Recovering deleted file**
- File deleted from disk, but agent recreated it in session
- NFS reads from session delta
- File IS promotable
- Example: Deleted and recreated `config.yaml`

### Scenario 5: Untracked + Exists + Clean (NEW - Passthrough)
**Accessing gitignored files like Cargo.lock**
- File not in Git, exists on disk, not modified in session
- NFS reads directly from repo filesystem (passthrough)
- File is NOT promotable by default (gitignored)
- Example: `Cargo.lock`, `.env`

### Scenario 6: Untracked + Exists + Dirty
**Modifying a gitignored file**
- File not in Git, exists on disk, modified in session
- NFS reads from session delta
- Promotability depends on gitignore rules
- Example: Editing `.env` in session

### Scenario 7: Untracked + Missing + Created
**Creating file in gitignored directory**
- File doesn't exist anywhere, created in session
- NFS reads from session delta
- Promotability depends on gitignore rules
- Example: `node_modules/patch.js` (excluded) vs `src/new.rs` (included)

### Scenario 8: New File Created
**Primary work case - adding new files**
- File never existed, created by agent in session
- NFS reads from session delta
- File IS promotable (unless gitignored)
- Example: `src/auth/login.rs`

---

## Read Priority Order

When NFS receives a read request for a file:

```
1. Is file dirty/modified in session?
   YES → Read from session delta (.vibe/sessions/<id>/<path>)
   NO  → Continue

2. Does file have a git_oid in metadata?
   YES → Read from Git ODB (git cat-file blob <oid>)
   NO  → Continue

3. Does file exist in repo filesystem?
   YES → Read from repo filesystem (passthrough)
   NO  → Return empty or error
```

---

## Write Behavior

When NFS receives a write request:

```
1. File exists in metadata?
   YES → Update in session delta, mark dirty
   NO  → Create new inode, write to session delta, mark dirty

2. Parent directory exists in metadata?
   YES → Add child to parent
   NO  → Create parent directories recursively
```

---

## Promotion Rules

When `vibe promote` runs:

```
For each dirty file:
1. Check against .gitignore (session or repo)
   IGNORED → Exclude from promotion
   NOT IGNORED → Continue

2. Check if file exists in session delta
   EXISTS → Hash blob, add to Git tree
   MISSING → Skip (was created then deleted)

3. Build new commit with modified tree
```

---

## Current Limitations / Known Issues

### Not Yet Implemented
1. **File Deletion Tracking**: Deleting a file in session doesn't mark it as "should be removed in promotion"
2. **Hard Links**: NFSv3 LINK operation not implemented (nfsserve limitation)

### Edge Cases Needing Work
1. **Hardlinks/Symlinks**: Partial support, may not work correctly
2. **File Permissions**: Mode changes not tracked
3. **Empty Directories**: Not persisted in Git (Git only tracks files)
4. **Atomic Renames**: May not be atomic across session boundary

### NFS Performance Considerations
1. **File Locking**: NFS doesn't support the locking semantics that Rust's incremental compilation requires. For Cargo builds, set `CARGO_INCREMENTAL=0`:
   ```bash
   CARGO_INCREMENTAL=0 cargo build
   ```
2. **Build Artifacts**: Building large projects in NFS mounts is slower than local filesystems due to network overhead. Consider using `CARGO_TARGET_DIR` to place build artifacts on local storage.

---

## User Visibility Recommendations

### In `vibe status`
Show for each session:
- Promotable files count
- Excluded (gitignored) files count
- Behind HEAD warning

### In `vibe inspect`
Show:
- List of promotable files with status (new/modified)
- List of excluded files (collapsed by directory)
- Session base commit vs current HEAD

### In TUI Dashboard
Show:
- `5 files (+127 excluded)` format
- Color coding: Green (clean), Yellow (dirty), Blue (promoted)
- Separate popup for promotable vs excluded files

---

## Testing Checklist

- [ ] Read tracked file (clean) → Git blob
- [ ] Read tracked file (dirty) → Session delta
- [ ] Read untracked file (Cargo.lock) → Repo passthrough
- [ ] Write to tracked file → Session delta + mark dirty
- [ ] Create new file → New inode + session delta + mark dirty
- [ ] Promote excludes gitignored files
- [ ] Promote includes new non-gitignored files
- [ ] Status shows correct counts
- [ ] TUI shows promotable vs excluded

---

## Summary

The key insight is: **VibeFS virtualizes the repo at a point in time, with a CoW (Copy-on-Write) layer for modifications.**

```
┌─────────────────────────────────────────────────────────────┐
│                      NFS MOUNT VIEW                          │
│  (What the agent sees)                                       │
└─────────────────────────────────────────────────────────────┘
                           │
                           ▼
          ┌────────────────────────────────────┐
          │        Session Delta Layer         │
          │  (.vibe/sessions/<id>/)            │
          │  - Modified files (dirty)          │
          │  - New files (created)             │
          └────────────────────────────────────┘
                           │
                           ▼
          ┌────────────────────────────────────┐
          │        Passthrough Layer           │
          │  (Repo filesystem)                 │
          │  - Untracked files (Cargo.lock)    │
          └────────────────────────────────────┘
                           │
                           ▼
          ┌────────────────────────────────────┐
          │          Git ODB Layer             │
          │  (.git/objects/)                   │
          │  - Tracked files (blobs)           │
          └────────────────────────────────────┘
```

This layered approach allows:
1. **Zero-copy reads** for unchanged files
2. **Isolated writes** per session
3. **Clean promotion** of intentional changes
4. **Access to build artifacts** via passthrough
