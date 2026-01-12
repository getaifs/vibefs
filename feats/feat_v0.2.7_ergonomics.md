Ergonomics improvements

[x] Command line options to "close" a session; such that we don't have to purge all when we accidentially create things and no use anymore. Plus a way to see what are the dirty files in a session.
    - Added `vibe close <session>` command
    - Supports `--force` flag to skip confirmation
    - Supports `--dirty` flag to only show dirty files without closing
    - Shows dirty files before closing and prompts for confirmation

[x] Not sure if `vibe ls` is that useful? If not, kill it, since now NFS is working and we can use `vibe path` to locate it.
    - Removed `vibe ls` command

[x] `vibe path` does too many things -- if there is no vibed or existing session, it creates one. It might be better to just enter an existing valid session.
    - Changed `vibe path` to only return path for existing, mounted sessions
    - No longer auto-creates sessions
    - Provides helpful error messages pointing users to `vibe spawn`

[x] TUI needs 1 - what are the repo names in addition to session name? Organize sessions by repo. 2 - potentially be able to see the dirty files in a session. 3 - Close a session or promote it.
    - Added repo name to dashboard title
    - Added dirty file count indicator per session (red [N] badge)
    - Added 'd' key to view dirty files popup
    - Added 'c' key to show close command hint
    - Added 'p' key to show promote command hint
    - Added session details panel with mount point and dirty file info
    - Added j/k vim-style navigation

[x] Commit might not be necessary since it is in the git domain. Consider killing it.
    - Removed `vibe commit` command (module still exists but not exported)

[x] Purge might be able to take a session name and just purge that session cleanly.
    - Added `--session <id>` option to `vibe purge`
    - `vibe purge -s <session>` closes a specific session
    - `vibe purge` (without -s) still purges all data

[x] NFS drive name might be able to have the repo name in it.
    - Mount points now use format: `~/Library/Caches/vibe/mounts/<repo>-<session>`
    - Added backwards compatibility for old mount point format in close/purge
