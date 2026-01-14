# VibeFS Dirty & Untracked File Tracking Design

## Mental Model

### Three File Categories

In VibeFS, files fall into three distinct categories based on their relationship to Git:

```
┌─────────────────────────────────────────────────────────────────────────┐
│                         FILE CATEGORIES                                  │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│  1. TRACKED FILES (in Git index at spawn_commit)                        │
│     ├── CLEAN: unchanged from spawn_commit                              │
│     └── DIRTY: modified in session, SHOULD be promoted                  │
│                                                                          │
│  2. GITIGNORED FILES (matched by .gitignore)                            │
│     ├── Pre-existing: in repo but not committed (e.g., node_modules)    │
│     └── Session-created: generated during session (e.g., build output)  │
│     └── Status: NEVER promoted (excluded)                               │
│                                                                          │
│  3. NEW FILES (not in Git, not in .gitignore)                          │
│     └── Created by agent intentionally                                  │
│     └── Status: SHOULD be promoted                                      │
│                                                                          │
└─────────────────────────────────────────────────────────────────────────┘
```

### The Core Principle

**Promote = Intent to Commit**

When an agent creates or modifies a file, the question is: "Should this be committed to Git?"

- **YES**: Tracked files (modified) + New files (not gitignored)
- **NO**: Gitignored files (build artifacts, dependencies, local config)

### File Lifecycle

```
Git HEAD (spawn_commit)          Session Layer                   Promote?
──────────────────────          ─────────────                   ────────
src/main.rs (tracked)    ──→    MODIFIED                   ──→  YES
src/lib.rs (tracked)     ──→    UNCHANGED (not in session) ──→  NO (skip)
(not exists)             ──→    CREATED: src/new.rs        ──→  YES
(not exists)             ──→    CREATED: node_modules/...  ──→  NO (gitignored)
.env.example (tracked)   ──→    MODIFIED                   ──→  YES
(not exists)             ──→    CREATED: .env              ──→  NO (gitignored)
```

## Scenarios & Edge Cases

### Scenario 1: Agent Modifies Tracked File
```
Before: src/main.rs exists in Git at spawn_commit
Action: Agent writes to src/main.rs via NFS
Result: File copied to session dir, marked dirty
Promote: YES - include in commit
```

### Scenario 2: Agent Creates New Source File
```
Before: src/new_feature.rs does NOT exist
Action: Agent creates src/new_feature.rs via NFS
Result: File created in session dir, marked dirty
Promote: YES - include in commit (not gitignored)
```

### Scenario 3: Agent Runs npm install
```
Before: node_modules/ does NOT exist (or is gitignored)
Action: Agent runs npm install, creates node_modules/
Result: Files created in session dir, marked dirty
Promote: NO - excluded by .gitignore
```

### Scenario 4: Agent Creates Build Output
```
Before: dist/ or target/ does NOT exist
Action: Agent runs build, creates dist/bundle.js
Result: File created in session dir, marked dirty
Promote: NO - excluded by .gitignore (typically)
```

### Scenario 5: Pre-existing Untracked Files in Repo
```
Before: node_modules/ exists in repo (untracked)
Setup: At init time, these are NOT in metadata (Git only)
       Session can READ from actual filesystem (passthrough)
Action: Agent modifies node_modules/foo/bar.js
Result: File copied to session, marked dirty
Promote: NO - excluded by .gitignore
```

### Scenario 6: Agent Creates .env from .env.example
```
Before: .env.example tracked, .env gitignored
Action: Agent copies .env.example to .env, modifies it
Result: .env in session, marked dirty
Promote: NO - excluded by .gitignore
```

### Scenario 7: Agent Adds New Entry to .gitignore
```
Before: .gitignore tracked
Action: Agent modifies .gitignore (adds new pattern)
Result: .gitignore modified in session
Promote: YES - .gitignore itself is tracked
Note: New patterns apply AFTER this promotion
```

## Implementation Strategy

### Phase 1: Gitignore Integration

1. **Parse .gitignore at promote time**
   - Load `.gitignore` from session (if modified) or Git HEAD
   - Also check parent directories for nested `.gitignore` files
   - Use standard gitignore globbing rules

2. **Filter dirty paths before promotion**
   ```rust
   let dirty_paths = get_all_dirty_paths();
   let promotable_paths = dirty_paths
       .filter(|path| !is_gitignored(path))
       .collect();
   ```

### Phase 2: Untracked File Handling for Reads

For files that exist in the repo but aren't in Git (e.g., node_modules):

1. **Passthrough reads**: If file not in metadata but exists on disk
   - Check if path matches gitignored pattern
   - If so, read directly from filesystem (not Git)
   - Do NOT create inode in metadata (volatile)

2. **Mark volatile on write**: If agent writes to gitignored path
   - Create inode with `volatile: true`
   - Mark dirty (for session consistency)
   - Exclude from promote

### Phase 3: Clear User Feedback

1. **On promote, show what's excluded**:
   ```
   Promoting 5 files:
     - src/main.rs
     - src/new_feature.rs
     - README.md

   Excluded (gitignored): 3 files
     - node_modules/... (1247 files)
     - dist/bundle.js
     - .env
   ```

2. **Provide --include-ignored flag** for edge cases

## Data Flow Diagram

```
                    ┌─────────────────────┐
                    │   NFS Write Request │
                    └──────────┬──────────┘
                               │
                               ▼
                    ┌─────────────────────┐
                    │ Write to session/   │
                    │ Create inode if new │
                    └──────────┬──────────┘
                               │
                               ▼
                    ┌─────────────────────┐
                    │   Mark as dirty     │
                    │ (all writes tracked)│
                    └──────────┬──────────┘
                               │
                               │  (later)
                               ▼
                    ┌─────────────────────┐
                    │   vibe promote      │
                    └──────────┬──────────┘
                               │
                               ▼
                    ┌─────────────────────┐
                    │ Get all dirty paths │
                    └──────────┬──────────┘
                               │
                               ▼
                    ┌─────────────────────┐
                    │ Filter by .gitignore│◄──── .gitignore rules
                    └──────────┬──────────┘
                               │
                               ▼
                    ┌─────────────────────┐
                    │ Hash & commit only  │
                    │ non-ignored files   │
                    └─────────────────────┘
```

## Key Design Decisions

### Decision 1: Mark All Writes as Dirty
**Rationale**: Track everything at write time, filter at promote time.
- Simpler NFS implementation
- User can see ALL changes (dirty list shows everything)
- Filtering happens once at the decision point (promote)

### Decision 2: Load .gitignore at Promote Time
**Rationale**: gitignore can change during session.
- Agent might modify .gitignore
- Using session's .gitignore (or HEAD if not modified) ensures consistency
- Avoids stale gitignore patterns

### Decision 3: Use `volatile` Flag for Runtime Tracking
**Rationale**: Distinguish between "promotable dirty" and "volatile dirty"
- `dirty` = file was written in session
- `volatile` = file matches gitignore pattern
- `promotable` = dirty AND NOT volatile

### Decision 4: Passthrough Reads for Untracked Files
**Rationale**: node_modules etc. must be readable for build tools
- If path not in metadata + not in Git + exists on disk → passthrough
- Don't pollute metadata with thousands of node_modules inodes
- Keep metadata focused on tracked files

## Testing Strategy

### Unit Tests
1. Gitignore pattern matching
2. Dirty path filtering
3. Volatile flag handling

### Integration Tests
1. Promote excludes gitignored files
2. New files are promoted (when not gitignored)
3. Modified tracked files are promoted
4. Passthrough reads work for untracked files

### Workflow Tests
1. Full npm install → build → promote cycle
2. Create .env from .env.example (shouldn't promote .env)
3. Modify .gitignore then create matching file (test order)
