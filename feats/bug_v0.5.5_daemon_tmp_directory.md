[X] RESOLVED: Daemon fails to start when repository is in /tmp directory

## Resolution
This was NOT a code bug. The issue was caused by stale/outdated installed binaries at `~/.local/bin/vibe` and `~/.local/bin/vibed`.

After testing, both debug AND release builds from `target/` work correctly in /tmp directories. The installed binaries were from an older version that had issues.

### Fix:
Copy fresh binaries from target/release to ~/.local/bin:
```bash
cp target/release/vibe ~/.local/bin/
cp target/release/vibed ~/.local/bin/
```

---

[ORIGINAL REPORT - RESOLVED]
## Summary
The vibed daemon fails to start with exit code -1 when the repository is located in /tmp or /private/tmp directories on macOS. This affects the vast majority of automated tests and any real usage where the repo is in a temp location.

## Reproduction Steps
```bash
cd /tmp
mkdir test-repo && cd test-repo
git init
git config user.name "Test"
git config user.email "test@test.com"
echo "test" > file.txt
git add . && git commit -m "init"
vibe init
vibe spawn test-session  # FAILS
```

## Expected Behavior
Daemon should start successfully regardless of the repository location.

## Actual Behavior
```
Spawning vibe workspace: test-session
  Ensuring daemon is running...
  Starting daemon: /Users/x/.local/bin/vibed
Error: Daemon process exited immediately with code -1.
Binary: /Users/x/.local/bin/vibed
This may indicate the binary was killed by macOS security.
Try running 'vibed -f' manually to see the error.
```

## Impact
- **Critical**: Blocks all spawn/mount operations in temp directories
- Affects 30 out of 35 workflow tests
- Makes automated testing unreliable
- May affect CI/CD pipelines using temp directories

## Workflows Affected
- Session Spawn (Local)
- Session Spawn Auto-name
- File Editing in Session
- Mark Dirty Files
- Session Status
- Session Status JSON
- Snapshot Creation
- Snapshot Preserves State
- Restore from Snapshot
- Promote Session
- Close Session
- Purge Session
- Shell Command in Session
- Multiple Parallel Sessions
- Conflict Detection
- NFS Mount Structure
- Full E2E Workflow

## Possible Causes
1. macOS sandbox/security restrictions on /tmp
2. Path canonicalization issue (/tmp vs /private/tmp on macOS)
3. Daemon process working directory issue
4. Socket path creation failing in /tmp

## Workaround
Run vibed in foreground mode with explicit repo path:
```bash
vibed -r /private/tmp/test-repo -f
```

This appears to work, suggesting the issue is in how the daemon is spawned as a background process.

## Additional Data Point
The debug build (non-release) of vibed typically starts without issues, but the release build has problems. This suggests:
1. Release optimizations might be causing undefined behavior
2. Different library linking between debug/release
3. Timing-sensitive code that only fails when optimized
4. Signal handling differences in optimized code

## Debugging Steps
Try building with debug symbols in release:
```bash
RUSTFLAGS="-C debuginfo=2" cargo build --release
```

Or compare behavior:
```bash
# Debug build
cargo build
./target/debug/vibed -r /tmp/test-repo -f

# Release build
cargo build --release
./target/release/vibed -r /tmp/test-repo -f
```

## Environment
- macOS (Darwin 25.2.0)
- vibe v0.5.1
- Shell: zsh
- Issue specific to release builds
