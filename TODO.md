# VibeFS Post-Launch TODO

Tracked issues and polish items for v0.9.4+.

## CLI Ergonomics

- [ ] **Session arg inconsistency**: Some commands use positional (`vibe rebase <session>`), others use `-s` flag (`vibe save -s <session>`). All commands should accept session as either positional or `-s` for consistency.
- [ ] **`vibe new <existing>` silently reuses session**: Should error with "session already exists" or explicitly say it's attaching to an existing session.
- [ ] **`vibe init` CLAUDE.md not idempotent**: Running `vibe init` twice appends duplicate workflow docs to CLAUDE.md. Should detect existing content and skip.
- [ ] **`vibe ls` outside repo exits code 0**: "VibeFS not initialized" message should use non-zero exit code for scripting.
- [ ] **No `vibe export/mount` command**: Users can't remount offline sessions without `attach` (which opens a shell). Need a `vibe mount <session>` or `vibe export <session>` for scripting and automation.

## Safety

- [ ] **`kill --purge --all --force` can destroy own mount**: Should detect if CWD is inside a vibe mount and warn/refuse. An agent running from a mount gets completely bricked.
- [ ] **`vibe rebase` on offline session silently re-activates**: Should warn or require an explicit flag instead of printing "unexport failed" then mounting anyway.

## Cleanup

- [ ] **Leftover snapshot dirs on `kill`**: `vibe kill <session>` leaves `<session>_snapshot_*` directories behind. Should clean them up or offer `vibe clean`.
- [ ] **`vibe daemon start` when already running says "started"**: Should say "Daemon already running (PID: N)" instead.
- [ ] **`vibe kill` output says "Unmounted" twice**: Redundant "Unmounted session" then "Unmounting..." messages.
- [ ] **`vibe daemon status` uptime format**: Shows raw seconds (`601s`) while `vibe ls` uses human format (`6m`). Should be consistent.
