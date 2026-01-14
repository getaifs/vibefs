[X] FIXED: RocksDB lock contention between daemon and CLI commands

## Resolution
Fixed by using `MetadataStore::open_readonly()` for all CLI commands that only need to read data. The daemon continues to hold the exclusive write lock, but CLI commands can now read the metadata concurrently.

### Files Modified:
- `src/commands/inspect.rs` - Changed to `open_readonly()`
- `src/commands/diff.rs` - Changed to `open_readonly()`
- `src/commands/status.rs` - Changed all three usages to `open_readonly()`
- `src/commands/promote.rs` - Changed to `open_readonly()` (only reads dirty paths)
- `src/commands/restore.rs` - Added helpful error message (requires write access, so daemon must be stopped)

### Note on restore
The `vibe restore` command still requires exclusive access because it clears and rebuilds dirty tracking. Users must stop the daemon first with `vibe daemon stop`. This is documented in the error message.

---

[ORIGINAL REPORT - RESOLVED]
## Summary
When the vibed daemon is running, CLI commands that need to access the metadata store (inspect, diff, promote) fail with a RocksDB lock error. The daemon holds an exclusive lock on the database, preventing other processes from accessing it.

## Reproduction Steps
```bash
cd /path/to/repo
vibe init
vibe spawn test-session   # Starts daemon, which holds RocksDB lock
vibe inspect test-session # FAILS with lock error
```

## Expected Behavior
CLI commands should be able to read metadata while the daemon is running, either through:
1. The daemon proxying metadata requests via IPC
2. RocksDB read-only mode for CLI commands
3. Shared lock mode for concurrent access

## Actual Behavior
```
Error: Failed to open RocksDB

Caused by:
    IO error: While lock file: /path/to/repo/.vibe/metadata.db/LOCK: Resource temporarily unavailable
```

## Impact
- **High**: Many operations require stopping the daemon first
- User must run `vibe daemon stop` before running inspect, diff, or promote
- Disrupts workflow and can kill ongoing NFS sessions

## Workflows Affected
- Session Inspect
- Session Inspect JSON
- Session Diff
- Session Diff Stat
- Promote Session
- Promote with Message
- Promote All Sessions
- Promote with --only

## Current Workaround
```bash
vibe daemon stop   # Stops daemon (releases lock)
sleep 1            # Wait for cleanup
vibe inspect test  # Now works
# But NFS mount is no longer active!
```

## Suggested Fixes
1. **Preferred**: Route metadata queries through daemon via IPC protocol
   - Add new IPC commands: `GetDirtyFiles`, `GetInodeInfo`, etc.
   - CLI commands send requests to daemon instead of opening RocksDB directly

2. **Alternative**: Open RocksDB in read-only mode for read operations
   - Use `rocksdb::DB::open_for_read_only()` for inspect/diff
   - Only daemon uses read-write mode

3. **Quick fix**: Add `--no-lock` flag for read-only operations that accepts stale reads

## Technical Details
- RocksDB: 0.22
- Lock location: `.vibe/metadata.db/LOCK`
- Daemon opens DB at startup and holds it open for the session lifecycle
