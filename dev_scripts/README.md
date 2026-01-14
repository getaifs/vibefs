# VibeFS Development Scripts

This directory contains development scripts for VibeFS.

## Installation

### `install.sh`
Installs pre-built VibeFS binaries to the host system (cross-platform).

```bash
# Build first
cargo build --release

# Then install
./dev_scripts/install.sh
```

**Features**:
- Detects platform automatically (macOS/Linux)
- Installs to `~/.local/bin/` (vibe, vibed, mark_dirty)
- Re-signs binaries on macOS (required to prevent SIGKILL after copy)
- Detects distrobox containers and installs to host system
- Uses repo root detection (portable across developers)

## Testing

Run the test suite with:

```bash
cargo test
```

This runs all unit and integration tests (58 tests total).

## Platform Differences

### macOS vs Linux

**Unmount commands**:
- macOS: `umount -f` or `diskutil unmount force`
- Linux: `umount -l` (lazy unmount)

**NFS requirements**:
- macOS: Built-in NFS client
- Linux: Requires `nfs-common` or `nfs-utils` package

**Filesystem features**:
- macOS: Uses APFS `clonefile(2)` for CoW snapshots
- Linux: Uses Btrfs/XFS `ioctl_ficlonerange` for reflinks

**Code signing**:
- macOS: Binaries must be re-signed after copy (handled by install.sh)
- Linux: No signing required
