Bugs 

[X] `vibe ls` doesn't start the daemon and hangs
[X] `vibe dashboard` fail to open RocksDB
    ╰─➜ vibe dashboard
        Error: Failed to open RocksDB

        Caused by:
            IO error: While lock file: /Users/x/src/getaifs.com/.vibe/metadata.db/LOCK: Resource temporarily unavailable

[X] `vibe spawn` bug

    ╭─x on Erics-MacBook-Pro in getaifs.com on  master [?15]
    ╰─➜ vibe spawn vibe-123
    Spawning vibe workspace: vibe-123
    Ensuring daemon is running...
    Session directory: /Users/x/src/getaifs.com/.vibe/sessions/vibe-123
    NFS port: 49349
    Mount point: /Users/x/Library/Caches/vibe/mounts/vibe-123

    Attempting NFS mount...
    ⚠ Auto-mount failed: mount_nfs failed: mount_nfs: can't mount / from localhost onto /Users/x/Library/Caches/vibe/mounts/vibe-123: Invalid argument


    To mount manually, run:
    mount_nfs -o vers=4,tcp,port=49349,resvport,nolock,locallocks localhost:/vibe-123 /Users/x/Library/Caches/vibe/mounts/vibe-123

    ✓ Vibe workspace spawned successfully

    ╭─x on Erics-MacBook-Pro in getaifs.com on  master [?16]
    ╰─➜ mount_nfs -o vers=4,tcp,port=49349,resvport,nolock,locallocks localhost:/vibe-123 /Users/x/Library/Caches/vibe/mounts/vibe-123
    mount_nfs: can't mount /vibe-123 from localhost onto /Users/x/Library/Caches/vibe/mounts/vibe-123: Invalid argument

[X] Potential duplicate bug -- Mount failed

    ╭─x on Erics-MacBook-Pro in getaifs.com on  master [?17]
╰─➜ vibe status
    VibeFS Status
    =============
    Repository: /Users/x/src/getaifs.com

    Daemon: Running
    NFS Port: 49349
    Uptime: 894s
    Active Sessions: 2

    Sessions:
    - vibe-123 (port: 49349, uptime: 187s)
        Mount: /Users/x/Library/Caches/vibe/mounts/vibe-123
    - default (port: 49349, uptime: 55s)
        Mount: /Users/x/Library/Caches/vibe/mounts/default

    Local Sessions:
    - default
    - vibe-123

    ╭─x on Erics-MacBook-Pro in getaifs.com on  master [?17]
    ╰─➜ ls /Users/x/Library/Caches/vibe/mounts/vibe-123

    ╭─x on Erics-MacBook-Pro in getaifs.com on  master [?17]
    ╰─➜ vibe ls
    .firebaserc
    .gitignore
    firebase.json
    public