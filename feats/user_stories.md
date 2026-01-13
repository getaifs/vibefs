# VibeFS User Stories

> **Document Purpose**: Comprehensive user stories for VibeFS, the session-based virtual filesystem for parallel agentic development. These stories inform workflow design, feature prioritization, and end-to-end test coverage.
>
> **Last Updated**: January 2026
> **Status**: Living document — update as product evolves

---

## Table of Contents

1. [Story Format & Prioritization](#story-format--prioritization)
2. [Persona Definitions](#persona-definitions)
3. [Epic 1: First-Time Setup & Onboarding](#epic-1-first-time-setup--onboarding)
4. [Epic 2: Single-Agent Development](#epic-2-single-agent-development)
5. [Epic 3: Multi-Agent Orchestration](#epic-3-multi-agent-orchestration)
6. [Epic 4: Session Lifecycle Management](#epic-4-session-lifecycle-management)
7. [Epic 5: Git Integration & Promotion](#epic-5-git-integration--promotion)
8. [Epic 6: Environment & Context Sharing](#epic-6-environment--context-sharing)
9. [Epic 7: Observability & Debugging](#epic-7-observability--debugging)
10. [Epic 8: Recovery & Edge Cases](#epic-8-recovery--edge-cases)
11. [Epic 9: Team & Enterprise Workflows](#epic-9-team--enterprise-workflows)
12. [Epic 10: Performance & Scale](#epic-10-performance--scale)
13. [Anti-Stories: What We Explicitly Don't Support](#anti-stories-what-we-explicitly-dont-support)
14. [Story → Workflow Mapping](#story--workflow-mapping)
15. [Story → E2E Test Mapping](#story--e2e-test-mapping)

---

## Story Format & Prioritization

Each story follows the format:

```
**[ID]** As a [persona], I want to [action], so that [outcome].

Priority: P0 (MVP) | P1 (Core) | P2 (Nice-to-have) | P3 (Future)
Complexity: S | M | L | XL
Workflow: [Workflow name for mapping]
```

**Priority Definitions:**
- **P0 (MVP)**: Must ship for the product to be viable
- **P1 (Core)**: Expected by users within first month
- **P2 (Nice-to-have)**: Differentiating features
- **P3 (Future)**: Roadmap items, not blocking launch

---

## Persona Definitions

### Primary Personas

| Persona | Description | Technical Level | Primary Goal |
|---------|-------------|-----------------|--------------|
| **Solo Vibe Coder** | Individual developer using AI agents (Claude, Cursor, Copilot) for parallel feature work | Mid-to-Senior | Ship features faster without context-switching overhead |
| **Agent Orchestrator** | Power user running 5-20 agents simultaneously on complex refactors | Senior/Staff | Maximize throughput on large codebase changes |
| **AI Agent** | The Claude/Gemini/Cursor process itself, operating within a session | N/A (machine) | Isolated, consistent environment with full repo access |
| **DevOps Engineer** | Person responsible for CI/CD, tooling, and developer experience | Senior | Integrate VibeFS into team workflows without breaking existing pipelines |

### Secondary Personas

| Persona | Description | Primary Goal |
|---------|-------------|--------------|
| **Open Source Maintainer** | Reviewing PRs from multiple AI-generated sources | Audit and merge agent work safely |
| **Tech Lead** | Overseeing team using VibeFS | Visibility into agent activity, prevent conflicts |
| **New Hire** | Onboarding to a VibeFS-enabled project | Quick ramp-up without breaking anything |

---

## Epic 1: First-Time Setup & Onboarding

### US-1.1: Initialize VibeFS on Existing Repository
**As a** Solo Vibe Coder,
**I want to** run a single command to initialize VibeFS on my existing Git repo,
**So that** I can start using parallel agent sessions without restructuring my project.

- **Priority**: P0 (MVP)
- **Complexity**: S
- **Workflow**: `init-workflow`
- **Acceptance Criteria**:
  - `vibe init` completes in <5 seconds for repos up to 50K files
  - Creates `.vibe/` directory with `sessions/`, `context/`, RocksDB store
  - Does NOT modify `.git/` or working tree
  - Populates metadata for all tracked files
  - Idempotent: running twice doesn't corrupt state

---

### US-1.2: Validate Repository Compatibility
**As a** Solo Vibe Coder,
**I want to** receive clear feedback if my repo isn't compatible with VibeFS,
**So that** I understand what to fix before proceeding.

- **Priority**: P0 (MVP)
- **Complexity**: S
- **Workflow**: `init-workflow`
- **Acceptance Criteria**:
  - Error if not in a Git repository
  - Error if Git repo is bare
  - Warning if repo has uncommitted changes (but allow proceeding)
  - Error if `.vibe/` exists but is corrupted

---

### US-1.3: First Session Walkthrough
**As a** New Hire,
**I want to** have a guided first-run experience,
**So that** I understand the VibeFS mental model without reading docs.

- **Priority**: P2 (Nice-to-have)
- **Complexity**: M
- **Workflow**: `onboarding-workflow`
- **Acceptance Criteria**:
  - First `vibe spawn` shows brief explanation of what's happening
  - Suggests next commands (e.g., "Try `vibe status` to see your session")
  - Can be disabled via `--quiet` or config

---

### US-1.4: Uninstall VibeFS Cleanly
**As a** Solo Vibe Coder,
**I want to** completely remove VibeFS from my project,
**So that** I can revert if VibeFS doesn't work for my workflow.

- **Priority**: P1 (Core)
- **Complexity**: S
- **Workflow**: `purge-workflow`
- **Acceptance Criteria**:
  - `vibe purge` unmounts all sessions, stops daemon, removes `.vibe/`
  - No orphaned mount points or processes
  - Git repo unchanged (no commits removed, refs intact)
  - Confirmation prompt with `--force` to skip

---

## Epic 2: Single-Agent Development

### US-2.1: Spawn Isolated Agent Session
**As a** Solo Vibe Coder,
**I want to** spawn a named session that gives an AI agent its own isolated view of my repo,
**So that** the agent can make changes without affecting my working directory.

- **Priority**: P0 (MVP)
- **Complexity**: M
- **Workflow**: `spawn-workflow`
- **Acceptance Criteria**:
  - `vibe spawn feature-auth` creates session named 'feature-auth' in <2 seconds
  - Session has read access to all Git-tracked files
  - Writes go to session-specific delta store
  - Returns mount path for agent to use
  - Session ID must be valid identifier (alphanumeric + hyphens)

---

### US-2.1.1: Spawn Isolated Agent Session with No Name
**As a** Solo Vibe Coder,
**I want to** spawn a session without a name that gives an AI agent its own isolated view of my repo,
**So that** the agent can make changes without affecting my working directory.

- **Priority**: P0 (MVP)
- **Complexity**: M
- **Workflow**: `spawn-workflow`
- **Acceptance Criteria**:
  - `vibe spawn` creates session in <2 seconds
  - We automatically assign a name (english readable, adj+noun format, e.g. "curious-ant")
  - Session has read access to all Git-tracked files
  - Writes go to session-specific delta store
  - Returns mount path for agent to use
  - Session ID must be valid identifier (alphanumeric + hyphens)

<!--
### US-2.2: Execute Command in Session Context
**As a** Solo Vibe Coder,
**I want to** run any shell command within a session's filesystem,
**So that** I can test, build, or run agents in the isolated environment.

- **Priority**: P0 (MVP)
- **Complexity**: S
- **Workflow**: `exec-workflow`
- **Acceptance Criteria**:
  - `vibe sh feature-auth` opens shell at session mount
  - `vibe sh feature-auth --command "npm test"` runs and exits
  - Exit code propagates correctly
  - Environment variables from context are available
-->
---

### US-2.3: Launch AI Agent in Session
**As a** Solo Vibe Coder,
**I want to** launch Claude/Cursor/Aider directly into a session,
**So that** the agent's file operations are automatically sandboxed.

- **Priority**: P0 (MVP)
- **Complexity**: M
- **Workflow**: `agent-launch-workflow`
- **Acceptance Criteria**:
  - `vibe launch <claude>` spawns session AND launches Claude with CWD at mount
  - Works with arbitrary binaries (claude, cursor, aider, code, etc.)
  - Agent sees complete repository structure
  - Agent can create, modify, delete files within session
  - Sessions are named in the same adj+noun format as `vibe spawn`, but the noun here is the agent name (e.g. "curious-claude")
  - Can be named with a optional --session parameter.

---

### US-2.4: Verify Session Reflects Git State
**As an** AI Agent,
**I want to** see the exact same file contents as the Git HEAD when I start,
**So that** I'm working from a known-good baseline.

- **Priority**: P0 (MVP)
- **Complexity**: M
- **Workflow**: `spawn-workflow`
- **Acceptance Criteria**:
  - `cat <file>` in session matches `git show HEAD:<file>`
  - Directory structure matches `git ls-tree -r HEAD`
  - Untracked files from working directory NOT visible (unless in context)
  - Submodules handled correctly (or explicitly unsupported with error)

---

### US-2.5: Track Modified Files Automatically
**As a** Solo Vibe Coder,
**I want to** see which files an agent has modified within a session,
**So that** I can review changes before promoting.

- **Priority**: P0 (MVP)
- **Complexity**: M
- **Workflow**: `status-workflow`
- **Acceptance Criteria**:
  - `vibe status` shows status per session
  - `vibe status <session>` shows (among others dirty files) for a specific session (note: not `vibe close`)
  - Tracks creates, modifies, deletes
  - Tracks snapshots
  - Distinguish binary vs text changes

---

## Epic 3: Multi-Agent Orchestration

### US-3.1: Run Multiple Agents in Parallel Sessions
**As an** Agent Orchestrator,
**I want to** spawn 10+ sessions simultaneously for different tasks,
**So that** I can parallelize a large refactor across multiple agents.

- **Priority**: P0 (MVP)
- **Complexity**: L
- **Workflow**: `parallel-spawn-workflow`
- **Acceptance Criteria**:
  - `for i in {1..10}; do vibe spawn task-$i &; done` completes without error
  - Each session has independent state
  - Daemon handles concurrent requests gracefully
  - Total memory overhead <500MB for 10 sessions on 1GB repo

---

### US-3.2: Assign Same Session to Multiple Agents (Collaboration Mode)
**As an** Agent Orchestrator,
**I want to** have two agents (e.g., Claude for code, Cursor for tests) share the same session,
**So that** they can collaborate on tightly coupled changes.

- **Priority**: P1 (Core)
- **Complexity**: M
- **Workflow**: `shared-session-workflow`
- **Acceptance Criteria**:
  - Second `vibe spawn same-session` returns existing mount (no error)
  - Both agents see each other's writes in real-time
  - No file locking conflicts (last-write-wins or warning)
  - Dirty tracking aggregates both agents' changes

---

### US-3.3: Isolate Agents with Disjoint File Sets
**As an** Agent Orchestrator,
**I want to** assign agents to specific subdirectories,
**So that** I prevent merge conflicts by ensuring non-overlapping changes.

- **Priority**: P2 (Nice-to-have)
- **Complexity**: L
- **Workflow**: `scoped-session-workflow`
- **Acceptance Criteria**:
  - `vibe spawn auth-agent --scopes src/auth/` limits writes to scope
  - Writes outside scope fail with clear error
  - Reads still work for entire repo (dependencies, configs)
  - Promotion only includes in-scope changes

---

### US-3.4: Batch Promote All Sessions
**As an** Agent Orchestrator,
**I want to** promote all active sessions in one command,
**So that** I can review all agent work as a set of commits.

- **Priority**: P1 (Core)
- **Complexity**: M
- **Workflow**: `batch-promote-workflow`
- **Acceptance Criteria**:
  - `vibe promote --all` creates phantom commits for each dirty session
  - Each commit references session ID
  - Commit messages auto-generated with session name
  - Report shows success/failure per session

---

### US-3.5: Detect Cross-Session Conflicts
**As an** Agent Orchestrator,
**I want to** know if two sessions modified the same file,
**So that** I can resolve conflicts before merging to main.

- **Priority**: P1 (Core)
- **Complexity**: L
- **Workflow**: `conflict-detection-workflow`
- **Acceptance Criteria**:
  - `vibe status --conflicts` shows overlapping dirty files
  - Shows which sessions conflict
  - Suggests resolution strategies
  - Blocks commit if conflicts exist (with `--force` override)

---

## Epic 4: Session Lifecycle Management

### US-4.1: List All Active Sessions
**As a** Solo Vibe Coder,
**I want to** see all my active sessions and their status,
**So that** I know what's running and can manage resources.

- **Priority**: P0 (MVP)
- **Complexity**: S
- **Workflow**: `status-workflow`
- **Acceptance Criteria**:
  - `vibe status` shows table: Session ID, Mount Path, Uptime, Dirty Count, Port
  - Distinguishes active (mounted) vs exported (ready to mount)
  - Shows daemon status (running/stopped, PID)

---

### US-4.2: Close Individual Session
**As a** Solo Vibe Coder,
**I want to** close a specific session when done,
**So that** I free resources without affecting other sessions.

- **Priority**: P0 (MVP)
- **Complexity**: M
- **Workflow**: `close-workflow`
- **Acceptance Criteria**:
  - `vibe close feature-auth` unmounts and cleans up
  - Warning if session has unpromoted changes
  - `--force` to close without promoting
  - Session directory removed from `.vibe/sessions/`

---

### US-4.3: Snapshot Session State
**As an** Agent Orchestrator,
**I want to** snapshot a session before a risky operation,
**So that** I can rollback if the agent breaks things.

- **Priority**: P1 (Core)
- **Complexity**: M
- **Workflow**: `snapshot-workflow`
- **Acceptance Criteria**:
  - `vibe snapshot feature-auth checkpoint-1` creates CoW snapshot
  - Completes in <1 second (APFS clonefile / btrfs reflink)
  - Snapshot is independent; original session continues
  - Can spawn new session from snapshot

---

### US-4.4: Restore Session from Snapshot
**As an** Agent Orchestrator,
**I want to** restore a session to a previous snapshot,
**So that** I can undo agent mistakes without losing the session.

- **Priority**: P1 (Core)
- **Complexity**: M
- **Workflow**: `restore-workflow`
- **Acceptance Criteria**:
  - `vibe restore feature-auth --snapshot checkpoint-1`
  - Current state replaced with snapshot state
  - Dirty tracking reset to snapshot's dirty state
  - Original snapshot preserved (can restore again)

---

### US-4.5: Session Timeout & Auto-Cleanup
**As a** DevOps Engineer,
**I want to** sessions to auto-close after idle timeout,
**So that** forgotten sessions don't consume resources indefinitely.

- **Priority**: P2 (Nice-to-have)
- **Complexity**: M
- **Workflow**: `cleanup-workflow`
- **Acceptance Criteria**:
  - Configurable idle timeout (default: 24 hours)
  - Warning notification before cleanup
  - Auto-promote option before closing
  - Daemon already has 20-min linger (extend to sessions)

---

## Epic 5: Git Integration & Promotion

### US-5.1: Promote Session to Phantom Commit
**As a** Solo Vibe Coder,
**I want to** promote session changes to a Git ref without touching HEAD,
**So that** I can review agent work before merging.

- **Priority**: P0 (MVP)
- **Complexity**: M
- **Workflow**: `promote-workflow`
- **Acceptance Criteria**:
  - `vibe promote feature-auth` creates commit at `refs/vibes/feature-auth`
  - Commit includes all dirty files from session
  - Commit message: "VibeFS: Promote session 'feature-auth'"
  - Parent is HEAD at time of spawn (or last promote)

---

### US-5.2: View Diff Before Promote
**As a** Solo Vibe Coder,
**I want to** see a diff of session changes before promoting,
**So that** I can verify the agent made correct changes.

- **Priority**: P0 (MVP)
- **Complexity**: S
- **Workflow**: `diff-workflow`
- **Acceptance Criteria**:
  - `vibe diff feature-auth` shows unified diff
  - Supports `--stat` for summary view
  - Pager support (less/more)
  - Color output with `--color=auto`

---

### US-5.3: Commit Promoted Work to HEAD
**As a** Solo Vibe Coder,
**I want to** merge a phantom commit into my main branch,
**So that** the agent's work becomes part of my project history.

- **Priority**: P0 (MVP)
- **Complexity**: M
- **Workflow**: `commit-workflow`
- **Acceptance Criteria**:
  - `vibe commit feature-auth` merges phantom commit to HEAD
  - Updates working tree with committed files
  - Closes session after successful commit
  - Creates merge commit if HEAD has diverged

---

### US-5.4: Cherry-Pick Specific Files from Session
**As a** Solo Vibe Coder,
**I want to** promote only specific files from a session,
**So that** I can accept partial work from an agent.

- **Priority**: P1 (Core)
- **Complexity**: M
- **Workflow**: `partial-promote-workflow`
- **Acceptance Criteria**:
  - `vibe promote feature-auth --only src/auth.rs src/tests/`
  - Only specified paths included in phantom commit
  - Remaining dirty files stay in session
  - Glob patterns supported

---

### US-5.5: Discard Session Changes
**As a** Solo Vibe Coder,
**I want to** throw away all changes in a session,
**So that** I can abandon failed agent attempts.

- **Priority**: P0 (MVP)
- **Complexity**: S
- **Workflow**: `discard-workflow`
- **Acceptance Criteria**:
  - `vibe close feature-auth --discard` removes session without promoting
  - Confirmation prompt (bypass with `--force`)
  - Delta store deleted
  - Phantom ref NOT created

---

### US-5.6: View Session History
**As an** Agent Orchestrator,
**I want to** see the promotion history of a session,
**So that** I can track iterative agent work.

- **Priority**: P2 (Nice-to-have)
- **Complexity**: M
- **Workflow**: `history-workflow`
- **Acceptance Criteria**:
  - `vibe log feature-auth` shows promote history
  - Each promote is a commit on `refs/vibes/feature-auth`
  - Shows timestamp, commit hash, files changed
  - Integrates with `git log refs/vibes/*`

---

## Epic 6: Environment & Context Sharing

### US-6.1: Share Environment Files Across Sessions
**As a** Solo Vibe Coder,
**I want to** have `.env` and other config files available in all sessions,
**So that** agents can run builds/tests without manual setup.

- **Priority**: P0 (MVP)
- **Complexity**: M
- **Workflow**: `context-injection-workflow`
- **Acceptance Criteria**:
  - Files in `.vibe/context/` appear in every session root
  - Session writes to context files are session-local (not shared)
  - Symlink from working dir supported: `ln -s ../.env .vibe/context/.env`
  - Context files override Git files of same name

---

### US-6.2: Inject node_modules or Vendor Dependencies
**As a** Solo Vibe Coder,
**I want to** share `node_modules/` across sessions without copying,
**So that** agents don't waste time on `npm install` per session.

- **Priority**: P1 (Core)
- **Complexity**: L
- **Workflow**: `dependency-injection-workflow`
- **Acceptance Criteria**:
  - `.vibe/context/node_modules` symlinked to main `node_modules/`
  - Read-only or CoW for agent writes
  - Works with npm, yarn, pnpm lockfiles
  - Same pattern for Python venv, Go modules, etc.

---

### US-6.3: Session-Specific Environment Overrides
**As an** Agent Orchestrator,
**I want to** set session-specific environment variables,
**So that** different agents can use different API keys or configs.

- **Priority**: P2 (Nice-to-have)
- **Complexity**: M
- **Workflow**: `env-override-workflow`
- **Acceptance Criteria**:
  - `vibe spawn feature-auth --env API_KEY=xxx`
  - Or `.vibe/sessions/feature-auth/.env` for persistence
  - Session env overrides global context
  - Shown in `vibe status --env`

---

## Epic 7: Observability & Debugging

### US-7.1: View Real-Time Session Activity
**As an** Agent Orchestrator,
**I want to** see which files agents are reading/writing in real-time,
**So that** I can monitor progress and catch runaway agents.

- **Priority**: P1 (Core)
- **Complexity**: L
- **Workflow**: `monitoring-workflow`
- **Acceptance Criteria**:
  - `vibe dashboard` shows TUI with live updates
  - File access log stream per session
  - Write operations highlighted
  - Pause/resume log streaming

---

### US-7.2: Debug Session Mount Issues
**As a** DevOps Engineer,
**I want to** diagnose NFS mount failures with clear error messages,
**So that** I can fix environment issues quickly.

- **Priority**: P0 (MVP)
- **Complexity**: M
- **Workflow**: `debug-workflow`
- **Acceptance Criteria**:
  - `vibe spawn --debug` enables verbose logging
  - Shows NFS negotiation, port binding, mount command
  - Common errors have troubleshooting hints
  - Logs written to `.vibe/logs/`

---

### US-7.3: Export Session Metrics
**As a** DevOps Engineer,
**I want to** export VibeFS metrics in Prometheus format,
**So that** I can integrate with our monitoring stack.

- **Priority**: P3 (Future)
- **Complexity**: L
- **Workflow**: `metrics-workflow`
- **Acceptance Criteria**:
  - `vibe daemon --metrics-port 9090` exposes `/metrics`
  - Metrics: sessions_active, files_dirty, nfs_ops_total, daemon_uptime
  - Labels for session_id, operation_type
  - Histogram for operation latency

---

### US-7.4: Inspect Session Metadata
**As a** Solo Vibe Coder,
**I want to** inspect the internal state of a session,
**So that** I can debug unexpected behavior.

- **Priority**: P1 (Core)
- **Complexity**: S
- **Workflow**: `inspect-workflow`
- **Acceptance Criteria**:
  - `vibe inspect feature-auth` dumps session metadata
  - Shows: spawn time, parent commit, dirty files, delta size
  - `--json` for scriptable output
  - RocksDB contents queryable

---

## Epic 8: Recovery & Edge Cases

### US-8.1: Recover from Daemon Crash
**As a** Solo Vibe Coder,
**I want to** recover my sessions after a daemon crash,
**So that** I don't lose unsaved agent work.

- **Priority**: P0 (MVP)
- **Complexity**: L
- **Workflow**: `recovery-workflow`
- **Acceptance Criteria**:
  - Delta store persists across daemon restarts
  - `vibe daemon start` recovers known sessions
  - Remounts sessions that were active
  - Dirty tracking reconstructed from delta files

---

### US-8.2: Handle Git Operations During Active Sessions
**As a** Solo Vibe Coder,
**I want to** run `git pull` in my main repo while sessions are active,
**So that** I can update baseline without closing sessions.

- **Priority**: P1 (Core)
- **Complexity**: L
- **Workflow**: `git-sync-workflow`
- **Acceptance Criteria**:
  - Sessions continue to work after `git pull`
  - Session shows files relative to their spawn commit (not new HEAD)
  - `vibe rebase feature-auth` updates session base to new HEAD
  - Conflict detection if session files changed upstream

---

### US-8.3: Handle Repository with Large Files
**As a** Solo Vibe Coder,
**I want to** VibeFS to work with repos containing large binary files,
**So that** I'm not blocked by game assets, ML models, etc.

- **Priority**: P1 (Core)
- **Complexity**: M
- **Workflow**: `large-file-workflow`
- **Acceptance Criteria**:
  - Git LFS pointers resolved to actual content
  - Large file reads don't block small file operations
  - Configurable max file size for NFS (streaming for >100MB)
  - Memory-efficient: don't load large files into memory

---

### US-8.4: Handle Submodules
**As a** Solo Vibe Coder,
**I want to** clear feedback on submodule support,
**So that** I know if my project structure is compatible.

- **Priority**: P2 (Nice-to-have)
- **Complexity**: L
- **Workflow**: `submodule-workflow`
- **Acceptance Criteria**:
  - Option A: Submodules appear as directories with their committed content
  - Option B: Clear error message that submodules aren't supported
  - Document recommended workaround (flatten or separate VibeFS instances)

---

### US-8.5: Survive System Sleep/Wake
**As a** Solo Vibe Coder,
**I want to** sessions to survive laptop sleep,
**So that** I can resume work the next day.

- **Priority**: P1 (Core)
- **Complexity**: M
- **Workflow**: `sleep-wake-workflow`
- **Acceptance Criteria**:
  - NFS mounts reconnect after wake
  - Daemon handles SIGCONT gracefully
  - No data loss in delta store
  - Timeout grace period for stale mounts

---

## Epic 9: Team & Enterprise Workflows

### US-9.1: Share Session with Teammate
**As a** Tech Lead,
**I want to** share a session's state with a teammate for review,
**So that** they can see exactly what an agent produced.

- **Priority**: P2 (Nice-to-have)
- **Complexity**: L
- **Workflow**: `share-workflow`
- **Acceptance Criteria**:
  - `vibe export feature-auth --bundle session.tar.gz`
  - Teammate runs `vibe import session.tar.gz`
  - Bundle includes delta store and metadata
  - Works across machines (portable format)

---

### US-9.2: Integrate with CI/CD Pipeline
**As a** DevOps Engineer,
**I want to** run VibeFS in CI for parallel test execution,
**So that** we can speed up our test suite.

- **Priority**: P2 (Nice-to-have)
- **Complexity**: L
- **Workflow**: `ci-workflow`
- **Acceptance Criteria**:
  - Runs in Docker/Linux CI environment
  - No sudo required (user-space NFS)
  - Parallel test sessions share read-only base
  - Fast teardown between jobs

---

### US-9.3: Audit Trail for Agent Changes
**As an** Open Source Maintainer,
**I want to** see which AI agent made which changes,
**So that** I can audit automated contributions.

- **Priority**: P2 (Nice-to-have)
- **Complexity**: M
- **Workflow**: `audit-workflow`
- **Acceptance Criteria**:
  - Commit message includes agent identifier
  - `refs/vibes/` namespace clearly marked as AI-generated
  - Session metadata logged: agent binary, spawn time, duration
  - Optional: sign commits with session key

---

### US-9.4: Limit Concurrent Sessions
**As a** DevOps Engineer,
**I want to** limit max concurrent sessions per repo,
**So that** runaway scripts don't exhaust system resources.

- **Priority**: P2 (Nice-to-have)
- **Complexity**: S
- **Workflow**: `limits-workflow`
- **Acceptance Criteria**:
  - Config: `max_sessions = 20` in `.vibe/config.toml`
  - Clear error when limit reached
  - `--force` to exceed limit
  - Show current usage in `vibe status`

---

## Epic 10: Performance & Scale

### US-10.1: Handle 100+ Concurrent Sessions
**As an** Agent Orchestrator,
**I want to** run 100 agents simultaneously on the same repo,
**So that** I can tackle massive codebases.

- **Priority**: P2 (Nice-to-have)
- **Complexity**: XL
- **Workflow**: `scale-workflow`
- **Acceptance Criteria**:
  - Daemon handles 100 concurrent NFS exports
  - <10 second spawn time at scale
  - Memory usage scales sub-linearly
  - Documented limits and recommendations

---

### US-10.2: Fast Initialization for Large Repos
**As a** Solo Vibe Coder,
**I want to** `vibe init` to complete quickly on monorepos,
**So that** I'm not blocked on large codebases.

- **Priority**: P1 (Core)
- **Complexity**: L
- **Workflow**: `init-workflow`
- **Acceptance Criteria**:
  - <30 seconds for 100K files
  - Progress indicator for long operations
  - Incremental update on reinit
  - Parallel metadata population

---

### US-10.3: Efficient Reads from Git Object Database
**As an** AI Agent,
**I want to** file reads to be as fast as native filesystem,
**So that** builds and tests run at normal speed.

- **Priority**: P1 (Core)
- **Complexity**: L
- **Workflow**: `performance-workflow`
- **Acceptance Criteria**:
  - Blob cache for hot files (LRU, configurable size)
  - Read latency <5ms for cached files
  - Batch reads for directory listings
  - Profiling data for optimization

---

---

## Anti-Stories: What We Explicitly Don't Support

These are intentional non-goals to maintain focus:

| ID | Anti-Story | Rationale |
|----|------------|-----------|
| AS-1 | Real-time sync between sessions | Complexity vs. value; use shared sessions instead |
| AS-2 | Windows native support (v1) | NFS challenges; WSL2 may work |
| AS-3 | Remote/distributed sessions | Out of scope; focus on local-first |
| AS-4 | Automatic conflict resolution | Too risky; human-in-the-loop required |
| AS-5 | Replace Git entirely | Git is source of truth; we're a sidecar |
| AS-6 | GUI application | CLI-first for agent compatibility |
| AS-7 | Sync with cloud storage | Local Git is source of truth |

---

## Story → Workflow Mapping

| Workflow Name | Stories | Key Commands |
|---------------|---------|--------------|
| `init-workflow` | US-1.1, US-1.2, US-10.2 | `vibe init` |
| `spawn-workflow` | US-2.1, US-2.4, US-3.1 | `vibe spawn <id>` |
| `exec-workflow` | US-2.2, US-2.3 | `vibe sh`, `vibe spawn <id> <binary>` |
| `agent-launch-workflow` | US-2.3 | `vibe spawn <id> claude` |
| `status-workflow` | US-2.5, US-4.1 | `vibe status` |
| `parallel-spawn-workflow` | US-3.1 | Multiple `vibe spawn` |
| `shared-session-workflow` | US-3.2 | `vibe spawn <existing>` |
| `conflict-detection-workflow` | US-3.5 | `vibe status --conflicts` |
| `close-workflow` | US-4.2, US-5.5 | `vibe close <id>` |
| `snapshot-workflow` | US-4.3 | `vibe snapshot <id> <name>` |
| `restore-workflow` | US-4.4 | `vibe restore <id> --snapshot <name>` |
| `promote-workflow` | US-5.1, US-5.4 | `vibe promote <id>` |
| `diff-workflow` | US-5.2 | `vibe diff <id>` |
| `commit-workflow` | US-5.3 | `vibe commit <id>` |
| `batch-promote-workflow` | US-3.4 | `vibe promote --all` |
| `context-injection-workflow` | US-6.1, US-6.2 | `.vibe/context/` |
| `monitoring-workflow` | US-7.1 | `vibe dashboard` |
| `debug-workflow` | US-7.2 | `vibe spawn --debug` |
| `recovery-workflow` | US-8.1 | `vibe daemon start` |
| `purge-workflow` | US-1.4 | `vibe purge` |

---

## Story → E2E Test Mapping

### Critical Path Tests (P0 - Must Pass for Release)

| Test ID | Workflow | Stories | Test Description |
|---------|----------|---------|------------------|
| E2E-001 | init-workflow | US-1.1, US-1.2 | Initialize on valid repo, verify `.vibe/` created |
| E2E-002 | spawn-workflow | US-2.1 | Spawn session, verify mount exists |
| E2E-003 | spawn-workflow | US-2.4 | Session file contents match Git HEAD |
| E2E-004 | exec-workflow | US-2.2 | Execute command in session, verify isolation |
| E2E-005 | agent-launch-workflow | US-2.3 | Launch agent binary in session context |
| E2E-006 | status-workflow | US-2.5, US-4.1 | Dirty files tracked, status accurate |
| E2E-007 | promote-workflow | US-5.1 | Promote creates phantom commit |
| E2E-008 | diff-workflow | US-5.2 | Diff shows correct changes |
| E2E-009 | commit-workflow | US-5.3 | Commit updates HEAD and working tree |
| E2E-010 | close-workflow | US-4.2 | Close session cleans up resources |
| E2E-011 | purge-workflow | US-1.4 | Purge removes all VibeFS state |
| E2E-012 | recovery-workflow | US-8.1 | Sessions survive daemon restart |

### Core Tests (P1 - Required for Production)

| Test ID | Workflow | Stories | Test Description |
|---------|----------|---------|------------------|
| E2E-101 | parallel-spawn-workflow | US-3.1 | 10 concurrent sessions work independently |
| E2E-102 | shared-session-workflow | US-3.2 | Two processes share session |
| E2E-103 | conflict-detection-workflow | US-3.5 | Detect cross-session file conflicts |
| E2E-104 | snapshot-workflow | US-4.3 | Snapshot creates CoW copy |
| E2E-105 | restore-workflow | US-4.4 | Restore reverts session state |
| E2E-106 | partial-promote-workflow | US-5.4 | Promote specific files only |
| E2E-107 | context-injection-workflow | US-6.1 | Context files visible in sessions |
| E2E-108 | monitoring-workflow | US-7.1 | Dashboard shows live session data |
| E2E-109 | debug-workflow | US-7.2 | Debug mode provides useful diagnostics |
| E2E-110 | git-sync-workflow | US-8.2 | Git pull doesn't break active sessions |
| E2E-111 | large-file-workflow | US-8.3 | Large files readable without crash |
| E2E-112 | sleep-wake-workflow | US-8.5 | Sessions survive system sleep |

### Extended Tests (P2/P3 - Nice to Have)

| Test ID | Workflow | Stories | Test Description |
|---------|----------|---------|------------------|
| E2E-201 | scoped-session-workflow | US-3.3 | Scoped session enforces boundaries |
| E2E-202 | batch-promote-workflow | US-3.4 | Batch promote all sessions |
| E2E-203 | cleanup-workflow | US-4.5 | Idle sessions auto-close |
| E2E-204 | env-override-workflow | US-6.3 | Session-specific env vars |
| E2E-205 | metrics-workflow | US-7.3 | Prometheus metrics exported |
| E2E-206 | share-workflow | US-9.1 | Export/import session bundle |
| E2E-207 | ci-workflow | US-9.2 | Run in Docker CI environment |
| E2E-208 | audit-workflow | US-9.3 | Agent changes have audit trail |
| E2E-209 | limits-workflow | US-9.4 | Session limits enforced |
| E2E-210 | scale-workflow | US-10.1 | 100 concurrent sessions stable |

---

## Appendix: Story Progression Roadmap

```
MVP (v1.0)                     Core (v1.1)                    Nice-to-Have (v1.2+)
─────────────────              ─────────────────              ─────────────────
US-1.1 Init                    US-3.2 Shared Sessions         US-3.3 Scoped Sessions
US-1.2 Validation              US-3.4 Batch Promote           US-4.5 Auto-Cleanup
US-2.1 Spawn        US-3.5 Conflict Detection      US-6.3 Env Overrides
US-2.1.1 Spawn-Names           US-4.3 Snapshot                US-7.3 Metrics
US-2.3 Agent Launch            US-4.4 Restore                 US-9.1 Share Session
US-2.4 Git State               US-5.4 Partial Promote         US-9.2 CI Integration
US-2.5 Track Changes           US-5.6 History                 US-9.3 Audit Trail
US-3.1 Parallel Sessions       US-6.2 Dependency Injection    US-9.4 Limits
US-4.1 List Sessions           US-7.1 Dashboard               US-10.1 Scale 100+
US-4.2 Close Session           US-7.4 Inspect                 US-8.4 Submodules
US-5.1 Promote                 US-8.2 Git Sync                US-1.3 Onboarding
US-5.2 Diff                    US-8.3 Large Files
US-5.3 Commit                  US-8.5 Sleep/Wake
US-5.5 Discard
US-6.1 Context Sharing
US-7.2 Debug
US-8.1 Recovery
US-1.4 Purge
```
