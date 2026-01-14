[X] FIXED: `vibe launch` executes agent in wrong directory (/tmp/vibe instead of mount point)

## Resolution
Fixed by loading the actual mount point from SpawnInfo after spawning the session, instead of using a hardcoded `/tmp/vibe/<session>` path.

### Fix in `src/commands/launch.rs`:
```rust
// Load spawn info to get the actual mount point
let spawn_info = SpawnInfo::load(repo_path, &session)
    .with_context(|| "Failed to load session info after spawn")?;

let mount_point = spawn_info.mount_point;

println!("Executing {} in {}", agent, mount_point.display());
```

---

[ORIGINAL REPORT - RESOLVED]
## Summary
The `vibe launch` command spawns a session correctly but then tries to execute the agent in `/tmp/vibe/<session>` instead of the actual NFS mount point at `~/Library/Caches/vibe/mounts/<repo>-<session>`. This causes the agent to fail or operate in the wrong directory.

## Reproduction Steps
```bash
vibe init
vibe launch claude
```

## Expected Behavior
The agent should be executed in the NFS mount point where files are accessible:
```
Executing claude in /Users/x/Library/Caches/vibe/mounts/myrepo-vigilant-claude
```

## Actual Behavior
```
Spawning vibe workspace: vigilant-claude
  NFS port: 60078
  Mount point: /Users/x/Library/Caches/vibe/mounts/vibefs-vigilant-claude

  Attempting NFS mount...
✓ Vibe workspace mounted at: /Users/x/Library/Caches/vibe/mounts/vibefs-vigilant-claude

Executing claude in /tmp/vibe/vigilant-claude   # WRONG PATH!
Error: Failed to exec claude: No such file or directory (os error 2)
```

## Impact
- **High**: `vibe launch` command is essentially broken
- Agents can't access files in the correct working directory
- Even if agent binary exists, it operates in empty/wrong directory

## Root Cause
The launch command uses a hardcoded `/tmp/vibe/<session>` path instead of reading the actual mount point from the spawn response or session info JSON.

## Affected Code
Likely in `src/commands/launch.rs` - the working directory is set to `/tmp/vibe/<vibe_id>` instead of using:
1. The mount_point from daemon response
2. The spawn_info.json's mount_point field

## Suggested Fix
```rust
// Instead of:
let work_dir = PathBuf::from("/tmp/vibe").join(&vibe_id);

// Use:
let spawn_info_path = vibe_dir.join("sessions").join(format!("{}.json", vibe_id));
let spawn_info: SpawnInfo = serde_json::from_str(&std::fs::read_to_string(&spawn_info_path)?)?;
let work_dir = spawn_info.mount_point;
```

## Workaround
Manually spawn and then change to the correct directory:
```bash
vibe spawn my-session
cd $(vibe path my-session)
claude  # or whatever agent
```
