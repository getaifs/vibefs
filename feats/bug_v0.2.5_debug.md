[ ] Unchanged files are not viewable, and new files are not tracked?

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