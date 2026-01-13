[ ] Inconsistancy or inaccurate or unhelpful daemon status in (first?) spawn, and potentially unhelpful error messages in running vibed (killed).

    ╭─x on Erics-MacBook-Pro in mfs on  main [?11] via  system
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

    ╭─x on Erics-MacBook-Pro in mfs on  main [?18] via  system
    ╰─➜ vibe spawn mfs-v3
    Spawning vibe workspace: mfs-v3
    Ensuring daemon is running...
    Error: Daemon failed to start within 5 seconds.

    ╭─x on Erics-MacBook-Pro in mfs on  main [?18] via  system
    ╰─➜ vibed
    zsh: killed     vibed

    ╭─x on Erics-MacBook-Pro in mfs on  main [?18] via  system
    ╰─➜ ~/src/vibefs/target/debug/vibed

    ╭─x on Erics-MacBook-Pro in mfs on  main [?22] via  system
    ╰─➜ ~/src/vibefs/target/debug/vibed -f
    [vibed] Starting daemon for /Users/x/src/mfs
    [vibed] Vibe dir: /Users/x/src/mfs/.vibe
    [vibed] Socket path: /Users/x/src/mfs/.vibe/vibed.sock
    [vibed] PID path: /Users/x/src/mfs/.vibe/vibed.pid
    [vibed] Socket file exists, checking if daemon is alive...
    Error: Daemon already running for this repository

    ╭─x on Erics-MacBook-Pro in mfs on  main [?23] via  system
    ╰─➜ vibe spawn mfs-v4
    Spawning vibe workspace: mfs-v4
    Ensuring daemon is running...
    Session directory: /Users/x/src/mfs/.vibe/sessions/mfs-v4
    NFS port: 55928
    Mount point: /Users/x/Library/Caches/vibe/mounts/mfs-mfs-v4

    Attempting NFS mount...
    ✓ Vibe workspace mounted at: /Users/x/Library/Caches/vibe/mounts/mfs-mfs-v4

    ✓ Vibe workspace spawned successfully