[ ] Bug: Spawn without init gives unclear error message

## Summary
When running `vibe spawn` without first running `vibe init`, the error message only says the spawn failed but doesn't clearly indicate that `vibe init` needs to be run first.

## Reproduction Steps
```bash
cd /path/to/git/repo  # Has .git but no .vibe
vibe spawn test-session
```

## Expected Behavior
Clear error message:
```
Error: VibeFS not initialized for this repository.
Run 'vibe init' first to initialize VibeFS.
```

## Actual Behavior
```
Spawning vibe workspace: test-session
Error: VibeFS not initialized. Run 'vibe init' first.
```

While the message does mention init, it could be clearer about what VibeFS is and why initialization is needed.

## Impact
- **Low**: Minor UX issue
- New users may be confused about what "VibeFS" means
- Error exits with failure which is correct behavior

## Suggested Improvements
1. Add more context to the error message
2. Consider auto-prompting to run init
3. Add a `--auto-init` flag to spawn that initializes if needed

## Current Behavior
The test passes (spawn correctly fails without init), but the error message could be more helpful.
