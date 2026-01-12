# VibeFS Workflow Guide

This document explains the correct workflow for using VibeFS, especially for AI agents working on features.

## Directory Structure

```
<repo-root>/
├── .vibe/                    # VibeFS metadata (created by vibe init)
│   ├── metadata.db/          # RocksDB database
│   ├── sessions/             # Active agent sessions
│   │   └── <agent-id>/       # Individual session workspace
│   └── cache/                # Shared cache
├── src/                      # Your source code
└── ...                       # Other repo files
```

## Correct Workflow Steps

### 1. Initialize VibeFS (One Time)

**IMPORTANT**: Always run from repository root!

```bash
cd /path/to/your/repo
vibe init
```

**Do NOT run `vibe init` from:**
- Inside `.vibe/` directory
- Inside a session directory
- From a subdirectory of your repo (unless you know what you're doing)

### 2. Spawn an Agent Workspace

```bash
# From repository root
vibe spawn <agent-id>
```

This creates: `.vibe/sessions/<agent-id>/`

### 3. Work in the Session Directory

**Option A: Use absolute paths (Recommended for AI agents)**

```bash
# Create/edit files with absolute paths
/path/to/repo/.vibe/sessions/<agent-id>/newfile.rs
/path/to/repo/.vibe/sessions/<agent-id>/src/main.rs
```

**Option B: Change directory and use relative paths**

```bash
cd .vibe/sessions/<agent-id>/
# Now work with relative paths
vim newfile.rs
mkdir src && vim src/main.rs
```

**CRITICAL**: If using Option B, never run `vibe init` or `vibe spawn` from here!

### 4. Mark Files as Dirty

From repository root:

```bash
mark_dirty . file1.rs src/file2.rs
```

The path is relative to repository root (the `.` is the repo root).

### 5. Promote Session

From repository root:

```bash
vibe promote <agent-id>
```

This reads files from `.vibe/sessions/<agent-id>/` and creates a Git commit.

### 6. Commit to Main

From repository root:

```bash
vibe commit <agent-id>
```

This:
- Updates HEAD to the promoted commit
- Updates working tree with `git reset --hard`
- **Deletes the session directory** `.vibe/sessions/<agent-id>/`

## Common Pitfalls

### ❌ Pitfall 1: Running `vibe init` from Wrong Directory

```bash
# WRONG
cd .vibe/sessions/my-agent/
vibe init  # Creates nested .vibe/ directory!
```

**Result**: Creates `.vibe/sessions/my-agent/.vibe/` - double nesting!

**Solution**: Always `cd` back to repository root before running `vibe init`.

### ❌ Pitfall 2: Session Directory Disappears

**Symptom**: Files you created in `.vibe/sessions/<id>/` are gone.

**Cause**: Someone ran `vibe commit <id>`, which deletes the session directory.

**Solution**:
- Don't run `vibe commit` until you're done with the session
- If multiple agents are working, use different session IDs
- Consider using `vibe snapshot <id>` to backup before risky operations

### ❌ Pitfall 3: Shell CWD Confusion

**Symptom**: Commands create files in unexpected locations.

**Cause**: The shell's current working directory changed without notice.

**Solution**:
- Always use absolute paths when creating files programmatically
- Check `pwd` before running vibe commands
- Stay in repository root for all vibe commands

### ❌ Pitfall 4: Forgetting to Mark Dirty

**Symptom**: `vibe promote` says "No changes to promote" even though you changed files.

**Cause**: Files weren't marked as dirty in the metadata database.

**Solution**: Always run `mark_dirty . <files>` after modifying files in the session.

## Best Practices for AI Agents

### 1. Always Use Absolute Paths

```python
# Good
repo_root = "/var/home/x/src/vibefs"
session_file = f"{repo_root}/.vibe/sessions/agent-1/newfile.rs"
```

### 2. Verify Repository Root First

```bash
# Start of every workflow
cd /var/home/x/src/vibefs  # Or wherever repo root is
pwd  # Verify you're in the right place
```

### 3. Never Nest Vibe Commands

```bash
# Don't do this
cd .vibe/sessions/agent-1/
vibe spawn another-agent  # Wrong!
vibe init  # Wrong!
```

### 4. Clean Up Orphaned Sessions

If you discover a nested structure:

```bash
# From repository root
rm -rf .vibe/sessions/<agent-id>/.vibe/
```

## Debugging Session Issues

### Check Session Structure

```bash
# From repository root
find .vibe/sessions/<agent-id> -type d
```

**Expected output**:
```
.vibe/sessions/<agent-id>
.vibe/sessions/<agent-id>/src
.vibe/sessions/<agent-id>/tests
# etc - should mirror your repo structure
```

**Bad output (nested .vibe)**:
```
.vibe/sessions/<agent-id>
.vibe/sessions/<agent-id>/.vibe          # ← WRONG
.vibe/sessions/<agent-id>/.vibe/sessions # ← WRONG
```

### Check Dirty Files

```bash
# This command doesn't exist yet, but it should
# For now, check RocksDB directly (advanced)
```

### List Active Sessions

```bash
ls -la .vibe/sessions/
```

## Advanced: Multiple Agents

```bash
# Agent 1 works on feature A
vibe spawn agent-1
# Edit files in .vibe/sessions/agent-1/
mark_dirty . feature_a.rs
vibe promote agent-1

# Agent 2 works on feature B (parallel)
vibe spawn agent-2
# Edit files in .vibe/sessions/agent-2/
mark_dirty . feature_b.rs
vibe promote agent-2

# Commit agent-1 first
vibe commit agent-1

# Now agent-2
vibe commit agent-2
```

**Important**: Don't commit one agent while another is still working on dependent files!

## Summary: The Golden Rules

1. ✅ Always run vibe commands from repository root
2. ✅ Use absolute paths when creating files programmatically
3. ✅ Mark files dirty after editing
4. ✅ Promote before commit
5. ✅ Session directories are deleted on commit - save your work first!

## Questions?

If you discover new pitfalls or workflow issues, document them here or open an issue!
