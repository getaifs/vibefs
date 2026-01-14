[X] Bug (if fixed, mark it as done with [X])

The folder structure in the mounted folder is not correct. It appears to be a flat structure with the files in the root of the mounted folder,
whereas the real git structure is properly hierarchically structured.

╭─x on Erics-MacBook-Pro in vibefs on  main [✘1⇡8] via  v1.92.0
╰─➜ vibe status
VibeFS Status for: /Users/x/src/vibefs
================================================================================
DAEMON: RUNNING (PID: 59674)
  Uptime:       17m
  Global Port:  0
  Sessions:     1

ACTIVE SESSIONS:
  SESSION              DIRTY    UPTIME       MOUNT
  -------------------- -------- ------------ ----------------------------------------
  test1                0        17m          /Users/x/Library/Caches/vibe/mounts/vibefs-test1
================================================================================


╰─➜ ls -l /Users/x/Library/Caches/vibe/mounts/vibefs-test1 | head -n 10
.rw-r--r--  2.2k x    13 Jan 08:23 bug_v0.2.0.md
.rw-r--r--  3.6k x    13 Jan 08:23 bug_v0.2.5_debug.md
.rw-r--r--   988 x    13 Jan 08:23 bug_v0.2.8.md
.rw-r--r--  1.8k x    13 Jan 08:23 bug_v0.2.9_debug.md
.rw-r--r--  1.1k x    13 Jan 08:23 Cargo.toml
.rw-r--r--  3.0k x    13 Jan 08:23 CHANGELOG.md
.rw-r--r--  6.2k x    13 Jan 08:23 CLAUDE.md
.rw-r--r--  6.6k x    13 Jan 08:23 close.rs
.rw-r--r--  5.3k x    13 Jan 08:23 commit.rs
.rw-r--r--  5.5k x    13 Jan 08:23 cwd_validation.rs

╰─➜ ls ~/src/vibefs
 Cargo.lock   CHANGELOG.md   dev_scripts   install.sh   RELEASING.md        󰣞 src      TEST_RESULTS.md   VIBEFS_WORKFLOW.md
 Cargo.toml   CLAUDE.md      feats        󰂺 README.md    spec.eric_local.md   target   tests


