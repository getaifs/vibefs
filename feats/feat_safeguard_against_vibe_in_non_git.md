# Feature: Safeguard Against Running Vibe Commands in Wrong Directory

## Problem Statement

AI agents (and humans) can accidentally run vibe commands from incorrect directories, leading to:

1. **Nested .vibe directories**: Running `vibe init` from inside `.vibe/sessions/<id>/` creates `.vibe/sessions/<id>/.vibe/`
2. **Confusion about repository root**: Commands executed from subdirectories may create incorrect paths
3. **Silent failures**: Commands may fail without clear indication that the working directory is wrong

This happened in practice when an agent's shell cwd drifted to a session directory and ran `vibe init`, creating a nested structure.

## Requirements

### Detection Criteria

All vibe commands should check:

1. **Is current directory a Git repository?**
   - Check for `.git` directory walking up the tree
   - If not found, return error with hint

2. **Is current directory the Git repository root?**
   - Verify cwd matches the directory containing `.git`
   - If in a subdirectory, suggest the correct path

3. **Is current directory inside a vibe session?**
   - Detect if cwd contains `.vibe/sessions/` in its path
   - Special error message for this case since it's the most confusing

### Error Messages

Error messages should be clear and actionable, hinting at the cwd issue:

#### Not in Git Repository
```
Error: Current directory is not a Git repository

Current directory: /home/user/random/path

VibeFS requires a Git repository to operate.

Hint: Navigate to your Git repository root before running vibe commands:
  cd /path/to/your/repo
  vibe init
```

#### In Git Subdirectory
```
Error: Must run vibe commands from repository root

Current directory: /home/user/repo/src/commands
Repository root:  /home/user/repo

Hint: Navigate to the repository root:
  cd /home/user/repo
  vibe <command>
```

#### Inside Session Directory (Most Critical)
```
Error: Cannot run vibe commands from inside a session directory

Current directory: /home/user/repo/.vibe/sessions/agent-1
Repository root:  /home/user/repo

This creates nested .vibe directories and breaks the workflow!

Hint: Always run vibe commands from the repository root:
  cd /home/user/repo
  vibe <command>

Reminder: Session directories are workspaces for editing files.
Use absolute paths when working in sessions, but run vibe
commands from the repository root.
```

### Implementation Requirements

1. **Create utility function** `validate_cwd()` that:
   - Returns `Result<PathBuf>` with repository root on success
   - Returns error with appropriate message on failure
   - Used by all command entry points

2. **Add to all commands**:
   - `vibe init`: Check not in session directory
   - `vibe spawn`: Check in repository root
   - `vibe snapshot`: Check in repository root
   - `vibe promote`: Check in repository root
   - `vibe commit`: Check in repository root
   - `vibe status` (if exists): Check in repository root

3. **Test coverage**:
   - Test running commands from non-git directory
   - Test running commands from git subdirectory
   - Test running commands from session directory
   - Test running commands from repository root (should succeed)

## Success Criteria

1. Running any vibe command from a non-git directory shows clear error
2. Running any vibe command from a subdirectory shows path to root
3. Running any vibe command from inside `.vibe/sessions/` shows special warning
4. Running vibe commands from repository root works as before
5. Error messages include the problematic cwd path for debugging
6. All tests pass with new validation logic

## Implementation Notes

### Git Repository Detection

Use existing git CLI integration:
```bash
git rev-parse --show-toplevel
```

Returns:
- Repository root path on success
- Error if not in git repository

### Path Comparison

Compare canonicalized paths:
- Current working directory (canonicalized)
- Repository root from `git rev-parse`
- Check if cwd path contains `.vibe/sessions/`

### Error Context

Use `anyhow::Context` to add helpful hints:
```rust
git rev-parse --show-toplevel
    .context("Not in a Git repository")
    .with_context(|| format!("Current directory: {:?}", env::current_dir()))?
```

## Future Enhancements

1. **Auto-correction flag**: `vibe --from-root <command>` that auto-cd's
2. **Session-aware commands**: `vibe-session promote` that detects session from cwd
3. **Config override**: Allow running from subdirectories if `.vibe/config.toml` permits
4. **Shell integration**: Export `VIBE_ROOT` environment variable on spawn

## Related Documentation

- `VIBEFS_WORKFLOW.md`: Workflow best practices
- `CLAUDE.md`: Architecture axioms (Locality principle)
