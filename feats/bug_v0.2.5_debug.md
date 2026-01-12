[X] RPC is bad when actually read the file containt on the NFS mount point

╭─x on Erics-MacBook-Pro in getaifs.com on  master [?14]
╰─➜ vibe spawn test-v3
Spawning vibe workspace: test-v3
  Ensuring daemon is running...
  Session directory: /Users/x/src/getaifs.com/.vibe/sessions/test-v3
  NFS port: 51243
  Mount point: /Users/x/Library/Caches/vibe/mounts/test-v3

  Attempting NFS mount...
⚠ Auto-mount failed: mount_nfs failed: mount_nfs: can't mount / from localhost onto /Users/x/Library/Caches/vibe/mounts/test-v3: Operation not permitted


  To mount manually, run:
  mount_nfs -o vers=3,tcp,port=51243,mountport=51243,resvport,nolock,locallocks localhost:/ /Users/x/Library/Caches/vibe/mounts/test-v3

✓ Vibe workspace spawned successfully

╭─x on Erics-MacBook-Pro in getaifs.com on  master [?15]
╰─➜ mount_nfs -o vers=3,tcp,port=51243,mountport=51243,resvport,nolock,locallocks localhost:/ /Users/x/Library/Caches/vibe/mounts/test-v3
mount_nfs: can't mount / from localhost onto /Users/x/Library/Caches/vibe/mounts/test-v3: Operation not permitted

╭─x on Erics-MacBook-Pro in getaifs.com on  master [?15]
╰─➜ sudo mount_nfs -o vers=3,tcp,port=51243,mountport=51243,resvport,nolock,locallocks localhost:/ /Users/x/Library/Caches/vibe/mounts/test-v3
Password:

╭─x on Erics-MacBook-Pro in getaifs.com on  master [?15]
╰─➜ ls /Users/x/Library/Caches/vibe/mounts/test-v3
/Users/x/Library/Caches/vibe/mounts/test-v3: RPC struct is bad (os error 72)





[X] Not properly mounted folders 

╭─x on Erics-MacBook-Pro in getaifs.com on  master [?14]
╰─➜ vibe spawn vibe1
Spawning vibe workspace: vibe1
  Ensuring daemon is running...
  Session directory: /Users/x/src/getaifs.com/.vibe/sessions/vibe1
  NFS port: 50941
  Mount point: /Users/x/Library/Caches/vibe/mounts/vibe1

  Attempting NFS mount...
⚠ Auto-mount failed: mount_nfs failed: mount_nfs: can't mount / from localhost onto /Users/x/Library/Caches/vibe/mounts/vibe1: Invalid argument


  To mount manually, run:
  mount_nfs -o vers=4,tcp,port=50941,resvport,nolock,locallocks localhost:/vibe1 /Users/x/Library/Caches/vibe/mounts/vibe1

✓ Vibe workspace spawned successfully

╭─x on Erics-MacBook-Pro in getaifs.com on  master [?15]
╰─➜ mount_nfs -o vers=4,tcp,port=50941,resvport,nolock,locallocks localhost:/vibe1 /Users/x/Library/Caches/vibe/mounts/vibe1
mount_nfs: can't mount /vibe1 from localhost onto /Users/x/Library/Caches/vibe/mounts/vibe1: Invalid argument


[X] Unchanged files are not viewable, and new files are not tracked?

╭─x on Erics-MacBook-Pro in getaifs.com on  master [?19]
╰─➜ vibe status
VibeFS Status
=============
Repository: /Users/x/src/getaifs.com

Daemon: Running
  NFS Port: 49349
  Uptime: 2129s
  Active Sessions: 2

Sessions:
  - vibe-123 (port: 49349, uptime: 1421s)
    Mount: /Users/x/Library/Caches/vibe/mounts/vibe-123
  - default (port: 49349, uptime: 1290s)
    Mount: /Users/x/Library/Caches/vibe/mounts/default

Local Sessions:
  - default
  - vibe-123

╭─x on Erics-MacBook-Pro in getaifs.com on  master [?19]
╰─➜ ls /Users/x/Library/Caches/vibe/mounts/default
 hello.txt


╭─x on Erics-MacBook-Pro in getaifs.com on  master [?19]
╰─➜ git status
On branch master
Your branch is up to date with 'origin/master'.

Untracked files:
  (use "git add <file>..." to include in what will be committed)
        .vibe/
        AGENTS.md
        git-worktree-vs-vibefs.md
        hello.txt
        prd.md

Queation: how do I (or agent agent) even add file to vibefs?