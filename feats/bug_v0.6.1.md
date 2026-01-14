[ ] Inconsistent (potentially dirty) tracking of files


╭─x on Erics-MacBook-Pro in vibefs on  main via  v1.92.0
╰─➜ vibe -V
vibe 0.6.1 (73c228c)

╭─x on Erics-MacBook-Pro in vibefs on  main via  v1.92.0
╰─➜ ls
 build.rs   Cargo.lock   Cargo.toml   CHANGELOG.md   CLAUDE.md   dev_scripts   feats   install.sh  󰂺 README.md   RELEASING.md  󰣞 src   target   tests

╭─x on Erics-MacBook-Pro in vibefs on  main via  v1.92.0
╰─➜ vibe status
VibeFS Status for: /Users/x/src/vibefs
================================================================================
DAEMON: RUNNING (PID: 57198)
  Uptime:       1m
  Global Port:  0
  Sessions:     1

ACTIVE SESSIONS:
  SESSION              DIRTY    UPTIME       MOUNT
  -------------------- -------- ------------ ----------------------------------------
  kind-varahamihira    0        1m           /Users/x/Library/Caches/vibe/mounts/vibefs-kind-varahamihira
================================================================================

╭─x on Erics-MacBook-Pro in vibefs on  main via  v1.92.0
╰─➜ ls /Users/x/Library/Caches/vibe/mounts/vibefs-kind-varahamihira
 Cargo.lock   CHANGELOG.md   dev_scripts   index.ts    󰂺 README.md      ROUGH_EDGES.md      󰣞 src      TEST_RESULTS.md   VIBEFS_WORKFLOW.md
 Cargo.toml   CLAUDE.md      feats         install.sh   RELEASING.md   spec.eric_local.md   target   tests