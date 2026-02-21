# VibeFS

Isolated Git workspaces for parallel AI agents. Each agent gets a virtual filesystem backed by Git's object database — reads come from Git, writes go to a session directory. No copies, no conflicts.

## Install

```bash
curl -sSfL https://raw.githubusercontent.com/getaifs/vibefs/HEAD/install.sh | bash
```

Installs `vibe` and `vibed` to `~/.local/bin`. Requires macOS (Linux support is experimental).

### Build from source

```bash
git clone https://github.com/getaifs/vibefs.git
cd vibefs
cargo build --release
./dev_scripts/install.sh
```

## Usage

```bash
cd /path/to/your/repo
vibe new agent-1          # create session, enter shell at NFS mount
# ... edit files, run builds ...
vibe diff                 # see what changed
vibe commit               # commit to refs/vibes/agent-1
exit                      # leave session
git merge refs/vibes/agent-1
```

Sessions auto-detect — run `vibe diff`, `vibe save`, `vibe commit` from inside the mount without specifying a session name.

## Commands

```
Sessions:
  new       Create a new session and enter shell
  attach    Attach to an existing session
  kill      Kill a session (unmount and clean up)

Versioning:
  save      Create a checkpoint of session state
  undo      Restore from checkpoint, or reset (--hard)
  commit    Commit session changes to a Git branch
  diff      Show unified diff of session changes

Info:
  ls        List sessions and show status

System:
  init      Initialize VibeFS for a Git repository
  rebase    Rebase session to current HEAD
  daemon    Daemon management commands
```

## Giving agents access

`vibe init` auto-appends a workflow guide to your `CLAUDE.md` (or creates `AGENTS.md`). You can also paste this into any agent's system prompt:

```
This repo uses VibeFS. You are working inside a VibeFS mount — an isolated
virtual filesystem backed by Git. Your changes are tracked automatically.

Key commands (run from inside the mount, no session name needed):
  vibe diff          Show what you changed
  vibe save          Checkpoint current state
  vibe undo --hard   Discard all changes, reset to base commit
  vibe commit        Commit changes to a Git ref

Do NOT use git commands — this mount has no .git directory.
Use vibe diff instead of git diff, and vibe commit instead of git commit.
```

## Launching agents

```bash
vibe new --agent claude       # launch Claude Code in a new session
vibe new --agent cursor       # launch Cursor
vibe new --agent aider        # launch aider
vibe new my-task --agent claude -- --model sonnet  # with extra args
```

## How it works

Each session is a lightweight overlay:

- **Reads**: Served from Git's object database (zero-copy)
- **Writes**: Go to `.vibe/sessions/<name>/` (the session delta)
- **Dirty tracking**: The NFS daemon records which files were written
- **Commit**: Hashes dirty files as Git blobs, creates a tree + commit at `refs/vibes/<name>`
- **Snapshots**: Uses APFS clonefile for instant, zero-cost checkpoints

```
.vibe/
├── metadata.db          # RocksDB: inode mappings
├── sessions/
│   ├── <name>/          # writable overlay (dirty files land here)
│   └── <name>.json      # session metadata (port, base commit)
└── cache/               # shared build artifacts (symlinked into mounts)
```

Build artifact directories (`target/`, `node_modules/`, etc.) are automatically symlinked to per-session local storage to avoid NFS performance issues and are excluded from commits.

## Requirements

- macOS with APFS (default on modern macOS)
- Git 2.30+
- Rust 1.70+ (build from source only)

## License

MIT
