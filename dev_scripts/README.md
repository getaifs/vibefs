# VibeFS Development Scripts

This directory contains development scripts for VibeFS.

## Scripts

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

### `release.sh`
Builds and uploads a release to GitHub (cross-platform).

```bash
# 1. Update version in Cargo.toml and CHANGELOG.md
# 2. Commit and push
# 3. Create and push tag
git tag -a v0.7.0 -m "Release v0.7.0"
git push origin v0.7.0

# 4. Run release script (builds and uploads for current platform)
./dev_scripts/release.sh
```

**Prerequisites**: `gh` CLI installed and authenticated (`gh auth login`)

**Note**: GitHub Actions automatically builds all platforms on tag push. Use this script for manual releases or to add artifacts to existing releases.

## Testing

Run the test suite with:

```bash
cargo test
```

This runs all unit and integration tests (58 tests total).

## Fedora/RHEL Notes

On Fedora with system RocksDB, you may need to set:

```bash
ROCKSDB_LIB_DIR=/usr/lib64 cargo build --release
ROCKSDB_LIB_DIR=/usr/lib64 cargo test
```

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
