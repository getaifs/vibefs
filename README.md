# VibeFS

Isolated Git workspaces for parallel AI agents. Each agent gets an NFS-mounted sandbox backed by Git, with automatic dirty tracking and zero-copy snapshots.

## Install

```bash
git clone https://github.com/getaifs/vibefs.git && cd vibefs
cargo build --release
./dev_scripts/install.sh
```

**Dependencies**: Rust 1.70+, C++ compiler, libclang. See [dev_scripts/README.md](dev_scripts/README.md) for platform-specific details.

## Quick Start

```bash
cd /path/to/your/repo
vibe init                    # Initialize VibeFS
vibe spawn my-session        # Create isolated workspace
vibe path my-session         # Get mount path
# ... work in the mount point ...
vibe promote my-session      # Create Git commit from changes
git merge refs/vibes/my-session  # Merge to main
vibe close my-session        # Clean up
```

## Commands

| Command | Description |
|---------|-------------|
| `vibe init` | Initialize VibeFS for a Git repo |
| `vibe spawn <id>` | Create isolated workspace with NFS mount |
| `vibe path <id>` | Print mount path for a session |
| `vibe sh -s <id>` | Run commands in session's mount |
| `vibe launch <agent>` | Spawn session and launch agent (claude, cursor, etc.) |
| `vibe promote <id>` | Commit session changes to `refs/vibes/<id>` |
| `vibe close <id>` | Unmount and clean up session |
| `vibe status` | Show daemon and session status |
| `vibe diff <id>` | Show changes in session |
| `vibe snapshot <id>` | Create zero-cost CoW snapshot |
| `vibe restore <id>` | Restore from snapshot |
| `vibe dashboard` | TUI monitoring dashboard |

## How It Works

```
.vibe/
├── metadata.db      # Inode mappings (RocksDB)
├── sessions/        # Per-session CoW overlays
│   └── <id>/        # Modified files go here
└── cache/           # Shared build artifacts
```

- **Reads**: Served from Git object database (zero-copy)
- **Writes**: Go to session directory (automatic tracking)
- **Promote**: Hashes dirty files into Git commit

See [docs/file-states.md](docs/file-states.md) for details.

## License

MIT
