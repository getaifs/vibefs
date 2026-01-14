# VibeFS Workflow Test Results v0.5

**Date**: 2026-01-13
**Version**: v0.5.1
**Platform**: macOS Darwin 25.2.0

## Summary

| Status | Count |
|--------|-------|
| PASSED | 5     |
| FAILED | 30    |
| Total  | 35    |

**Pass Rate**: 14.3%

## Test Results

### Passed Tests (5)

1. **Basic Initialization** - `vibe init` works correctly
2. **Close Nonexistent Session** - Correctly fails when session doesn't exist
3. **Daemon Status** - `vibe daemon status` works
4. **Init in Non-Git Dir** - Correctly fails in non-git directory
5. **Spawn Without Init** - Correctly fails without initialization

### Failed Tests (30)

Most failures are caused by **Bug #0.5.5** (Daemon fails to start in /tmp directories). Since tests run in /tmp, the daemon won't start, causing cascading failures.

#### Daemon Startup Failures (27 tests)
- Session Spawn (Local)
- Session Spawn Auto-name
- File Editing in Session
- Mark Dirty Files
- Session Status
- Session Status JSON
- Session Inspect
- Session Inspect JSON
- Session Diff
- Session Diff Stat
- Snapshot Creation
- Snapshot Preserves State
- Restore from Snapshot
- Promote Session
- Promote with Message
- Promote All Sessions
- Promote with --only
- Close Session
- Close with Dirty Check
- Get Session Path
- Daemon Stop
- Purge Specific Session
- Shell Command in Session
- Multiple Parallel Sessions
- Conflict Detection
- NFS Mount Structure
- Full E2E Workflow
- Double Spawn Same Session
- Promote Without Dirty Files

#### RocksDB Lock Issues (Subset of above)
When daemon does run, these tests fail due to lock contention:
- Session Inspect (Bug #0.5.6)
- Session Inspect JSON
- Session Diff
- Promote Session

## Bugs Identified

| Bug ID | Title | Severity | Status |
|--------|-------|----------|--------|
| 0.5.2 | NFS folder structure is flat | High | Open |
| 0.5.3 | RocksDB lock file error | High | Open |
| 0.5.5 | Daemon fails in /tmp directories | Critical | New |
| 0.5.6 | RocksDB lock contention daemon/CLI | High | New |
| 0.5.7 | Daemon stop is global, not per-repo | Medium | New |
| 0.5.8 | Launch uses wrong directory | High | New |
| 0.5.9 | Spawn without init unclear message | Low | New |

## Root Causes

1. **Daemon startup in /tmp** - The release build of vibed fails to start when the repository is in /tmp or /private/tmp. Debug builds work correctly, suggesting a release optimization or linking issue.

2. **RocksDB locking** - The daemon opens RocksDB in exclusive mode, preventing CLI commands from accessing metadata. Need to either route all metadata access through daemon IPC or use read-only mode for CLI.

3. **Path handling** - The launch command uses a hardcoded path instead of the actual mount point from spawn info.

## Recommendations

### Immediate Fixes
1. Fix daemon startup for /tmp directories (critical for CI)
2. Add IPC protocol for metadata queries to avoid RocksDB lock issues
3. Fix launch command to use correct mount path

### Testing Improvements
1. Run tests with debug build to verify workflows work
2. Add integration test that runs from non-/tmp directory
3. Add daemon health check before spawn operations

## Test Execution Log

```
============================================
VibeFS Comprehensive Workflow Tests
============================================
Started at: Tue Jan 13 19:27:37 CST 2026

PASSED: 5
FAILED: 30

Passed tests:
  - 1. Basic Initialization
  - 21. Close Nonexistent Session
  - 23. Daemon Status
  - 31. Init in Non-Git Dir
  - 32. Spawn Without Init
```

## Workflows Tested

1. Basic Initialization (`vibe init`)
2. Session Spawn (Local)
3. Session Spawn Auto-name
4. File Editing in Session
5. Mark Dirty Files (`mark_dirty`)
6. Session Status (`vibe status`)
7. Session Status JSON (`vibe status --json`)
8. Session Inspect (`vibe inspect`)
9. Session Inspect JSON (`vibe inspect --json`)
10. Session Diff (`vibe diff`)
11. Session Diff Stat (`vibe diff --stat`)
12. Snapshot Creation (`vibe snapshot`)
13. Snapshot Preserves State
14. Restore from Snapshot (`vibe restore`)
15. Promote Session (`vibe promote`)
16. Promote with Message (`vibe promote -m`)
17. Promote All Sessions (`vibe promote --all`)
18. Promote with --only patterns
19. Close Session (`vibe close`)
20. Close with Dirty Check (`vibe close --dirty`)
21. Close Nonexistent Session
22. Get Session Path (`vibe path`)
23. Daemon Status (`vibe daemon status`)
24. Daemon Stop (`vibe daemon stop`)
25. Purge Specific Session (`vibe purge -s`)
26. Shell Command in Session (`vibe sh -c`)
27. Multiple Parallel Sessions
28. Conflict Detection (`vibe status --conflicts`)
29. NFS Mount Structure verification
30. Full End-to-End Workflow
31. Init in Non-Git Directory
32. Spawn Without Init
33. Double Spawn Same Session
34. Promote Without Dirty Files
35. Launch Nonexistent Agent
