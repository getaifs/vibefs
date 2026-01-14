[X] NOT A BUG: `vibe daemon stop` is correctly per-repository

## Verification
Tested with two repos:
- `/tmp/vibefs_repo_a` - spawn session-a, daemon PID 97553
- `/tmp/vibefs_repo_b` - spawn session-b, daemon PID 97627

After running `vibe -r /tmp/vibefs_repo_b daemon stop`:
- Repo B's daemon stopped
- Repo A's daemon (PID 97553) still running

**The daemon IS per-repo. Each repo has its own vibed process and .vibe/vibed.sock socket.**

---

[ORIGINAL REPORT - INVALID]
[ ] Bug: `vibe daemon stop` kills daemon globally instead of per-repository

## Summary
The `vibe daemon stop` command stops the daemon regardless of which repository the user is currently in. This can unexpectedly kill NFS sessions for other repositories, disrupting work across multiple projects.

## Reproduction Steps
```bash
# Terminal 1: Start session in project A
cd ~/projects/projectA
vibe init && vibe spawn session-a
# Working in session-a...

# Terminal 2: Work in project B
cd ~/projects/projectB
vibe init && vibe spawn session-b
vibe daemon stop   # Intending to stop only projectB daemon

# Back in Terminal 1
vibe status   # Daemon is stopped! session-a is gone
```

## Expected Behavior
`vibe daemon stop` should only stop the daemon for the current repository. If the user wants to stop all daemons, there should be an explicit `--all` flag.

## Actual Behavior
Running `vibe daemon stop` in any directory kills the daemon process, which may be serving multiple repositories (or the wrong repository if invoked from a different project).

## Impact
- **Medium-High**: Disrupts multi-project workflows
- Can cause data loss if agent work is interrupted mid-operation
- Confusing behavior for users working on multiple projects

## Affected Use Cases
1. Developer with multiple VibeFS-enabled projects open
2. CI/CD running multiple parallel builds with VibeFS
3. User accidentally running `vibe daemon stop` in wrong directory

## Suggested Fix
1. The daemon should be per-repository (vibed.sock per .vibe directory)
2. `vibe daemon stop` should only stop the daemon for the current repo
3. Add `vibe daemon stop --all` to stop all running vibed processes
4. Consider `vibe daemon list` to show all running daemons across repos

## Current Architecture
- vibed creates a socket at `.vibe/vibed.sock` (per-repo)
- But the daemon process itself may be shared or there's only one global instance?
- Need to verify: Is there one daemon per repo or one global daemon?

## Workaround
Be very careful about which directory you're in when running daemon commands. Use `pwd` to verify before stopping.
