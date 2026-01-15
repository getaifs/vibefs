# VibeFS Development Scripts

This directory contains development scripts for VibeFS.

## Quick Reference

```bash
# Build, install, and quick test
./dev_scripts/test-local.sh

# Interactive testing session
./dev_scripts/test-local.sh --interactive

# Test agent launch flow
./dev_scripts/test-local.sh --agent

# Bump version and push
./dev_scripts/bump.sh patch   # 0.7.2 -> 0.7.3
./dev_scripts/bump.sh minor   # 0.7.2 -> 0.8.0
./dev_scripts/bump.sh 1.0.0   # Set explicit version

# Create GitHub release
./dev_scripts/release.sh
```

## Scripts

### `test-local.sh`

Quick local testing after code changes. Builds, installs, and verifies everything works.

```bash
# Quick smoke test (builds, installs, tests basic commands)
./dev_scripts/test-local.sh --quick

# Interactive mode (drops you in a test repo shell)
./dev_scripts/test-local.sh --interactive

# Test agent launch flow (requires mock-agent in PATH)
./dev_scripts/test-local.sh --agent

# Full workflow tests
./dev_scripts/test-local.sh --workflow
```

### `bump.sh`

Version management: updates Cargo.toml, runs tests, commits, tags, and pushes.

```bash
# Bump patch version (0.7.2 -> 0.7.3)
./dev_scripts/bump.sh patch

# Bump minor version (0.7.2 -> 0.8.0)
./dev_scripts/bump.sh minor

# Bump major version (0.7.2 -> 1.0.0)
./dev_scripts/bump.sh major

# Set explicit version
./dev_scripts/bump.sh 0.9.0
```

The script will:
1. Update `Cargo.toml` and `Cargo.lock`
2. Run tests (rolls back if they fail)
3. Build release binaries
4. Create commit and tag
5. Optionally push to origin

### `install.sh`

Installs pre-built VibeFS binaries to the host system.

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

### `release.sh`

Builds and uploads a release to GitHub.

```bash
# After bumping version with bump.sh:
./dev_scripts/release.sh
```

**Prerequisites**: `gh` CLI installed and authenticated (`gh auth login`)

### `mock-agent`

A fake agent for testing the `vibe <agent>` workflow. Use it to test:
- Agent argument passthrough
- Session creation for agents
- File changes in sessions

```bash
# Test agent launch
vibe mock-agent

# Test argument passthrough
vibe mock-agent --test-flag --another-flag

# Interactive mode
vibe mock-agent --interactive
```

## Typical Development Workflow

### After making code changes:

```bash
# 1. Quick test
./dev_scripts/test-local.sh

# 2. If tests pass, bump version and push
./dev_scripts/bump.sh patch

# 3. Create release (or let GitHub Actions do it)
./dev_scripts/release.sh
```

### Testing specific features:

```bash
# Test agent launch flow
PATH="./dev_scripts:$PATH" ./dev_scripts/test-local.sh --agent

# Or manually:
cargo build --release && ./dev_scripts/install.sh
cd /tmp && mkdir test-repo && cd test-repo && git init
vibe mock-agent --interactive
```

## Running Tests

```bash
# Unit and integration tests
cargo test

# Comprehensive workflow tests
./tests/workflow_tests.sh
```

## Platform Notes

### macOS vs Linux

**NFS requirements**:
- macOS: Built-in NFS client, no root needed
- Linux: Requires `nfs-common` package, root for mounting

**Code signing**:
- macOS: Binaries must be re-signed after copy (handled by install.sh)
- Linux: No signing required

### Fedora/RHEL

With system RocksDB, you may need:

```bash
ROCKSDB_LIB_DIR=/usr/lib64 cargo build --release
ROCKSDB_LIB_DIR=/usr/lib64 cargo test
```
