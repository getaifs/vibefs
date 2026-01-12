
[x] In the TUI (interactive part) it's better to also see the repo info in a nice way (grouped by repo). Or if it's only scoped to current repo then give a hint about it.
    - Dashboard now shows full repo path in title with "(Current Repo)" indicator
    - Details panel shows repository path for selected session
    - The 'c' close now actually closes the session via tokio::spawn

[x] Display `vibe status` in a nicer format.
    - Added table format for sessions with columns: ID, PORT, UPTIME, MOUNT POINT
    - Clear section headers (DAEMON, ACTIVE SESSIONS, OFFLINE SESSIONS)
    - Shows PID and uptime for daemon
    - Separator lines for readability

[x] Potential bug: any file copied to the folder seem to map to two dirty files. One as filename and the other as ._filename.
    - Fixed: Now filtering out macOS resource fork files (._filename) from dirty file lists
    - Also filtering out .DS_Store files
    - Applied to both TUI and close command dirty file collection
