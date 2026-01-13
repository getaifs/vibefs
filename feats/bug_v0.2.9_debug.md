[ ] Potential bug when using multiple sessions across multiple repos. 

I run vibe status in two different git repos.

╭─x on Erics-MacBook-Pro in getaifs.com on  master [?19]
╰─➜ vibe status
VibeFS Status for: /Users/x/src/getaifs.com
================================================================================
DAEMON: RUNNING (PID: 32044)
  Uptime:       32376s
  Global Port:  0
  Sessions:     1

ACTIVE SESSIONS:
  ID                   PORT       UPTIME     MOUNT POINT
  -------------------- ---------- ---------- ----------------------------------------
  t3                   56480      29294s     /Users/x/Library/Caches/vibe/mounts/getaifs.com-t3

OFFLINE SESSIONS (in storage):
  - t3
================================================================================

╭─x on Erics-MacBook-Pro in mfs on  main [?17] via  system
╰─➜ vibe init
Initializing VibeFS for repository at: /Users/x/src/mfs
Scanning Git repository...
Found 1 entries
✓ VibeFS initialized successfully
  Metadata store: /Users/x/src/mfs/.vibe/metadata.db
  Sessions dir: /Users/x/src/mfs/.vibe/sessions
  Cache dir: /Users/x/src/mfs/.vibe/cache
Bootstrapping VibeFS docs in AGENTS.md...
✓ Created AGENTS.md with VibeFS workflow documentation

╭─x on Erics-MacBook-Pro in mfs on  main [?20] via  system
╰─➜ vibe status
VibeFS Status for: /Users/x/src/mfs
================================================================================
DAEMON: NOT RUNNING
================================================================================

╭─x on Erics-MacBook-Pro in mfs on  main [?20] via  system
╰─➜ vibe spawn mfs-v1
Spawning vibe workspace: mfs-v1
  Ensuring daemon is running...
Error: Daemon failed to start within 5 seconds