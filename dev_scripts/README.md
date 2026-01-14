# VibeFS Development Scripts

This directory contains development and testing scripts for VibeFS.

## Installation Scripts

### `install_mac.sh`
Builds and installs VibeFS locally on macOS.

```bash
./dev_scripts/install_mac.sh
```

Installs to `~/.local/bin/` and creates cache directories.

## Testing Scripts

### Quick Smoke Test

**`test_quick.sh`** - Fast smoke test (cross-platform)

Runs basic sanity checks without full workflow:
- Builds VibeFS in debug mode
- Tests `vibe init` in a temp repo
- Verifies basic commands work

```bash
./dev_scripts/test_quick.sh
```

**Runtime**: ~30 seconds

### Full Workflow Tests

**`test_workflow_mac.sh`** - Complete end-to-end test for macOS

**`test_workflow_linux.sh`** - Complete end-to-end test for Linux

These scripts test the entire VibeFS workflow:

1. Build VibeFS (debug mode)
2. Create temporary Git repository
3. Run `vibe init`
4. Spawn session with `vibe spawn`
5. Make modifications (edit files, add new files, create directories)
6. Mark files as dirty (if command exists)
7. Promote changes with `vibe promote`
8. Verify promoted commit contains changes
9. Finalize with `vibe commit`
10. Verify final state in working tree

```bash
# On macOS
./dev_scripts/test_workflow_mac.sh

# On Linux
./dev_scripts/test_workflow_linux.sh
```

**Runtime**: ~1-2 minutes

**Requirements**:
- Mac: Xcode Command Line Tools, Rust toolchain
- Linux: NFS client tools (`nfs-common` on Debian/Ubuntu, `nfs-utils` on Fedora/RHEL), Rust toolchain

### Test Output

All tests use color-coded output:
- **Blue [INFO]**: Informational messages
- **Yellow headings**: Test step headers
- **Green [SUCCESS]**: Passed tests
- **Red [ERROR]**: Failed tests

### Cleanup

All test scripts automatically clean up temporary files and processes on exit (including when interrupted with Ctrl+C).

## Platform Differences

### macOS vs Linux

**Unmount commands**:
- macOS: `umount -f`
- Linux: `umount -l` (lazy unmount)

**NFS requirements**:
- macOS: Built-in NFS client
- Linux: Requires `nfs-common` or `nfs-utils` package

**Filesystem features**:
- macOS: Uses APFS `clonefile(2)` for CoW snapshots
- Linux: Uses Btrfs/XFS `ioctl_ficlonerange` for reflinks

## CI/CD Integration

These scripts can be integrated into CI pipelines:

```yaml
# GitHub Actions example
- name: Run workflow tests
  run: |
    if [[ "$OSTYPE" == "darwin"* ]]; then
      ./dev_scripts/test_workflow_mac.sh
    else
      ./dev_scripts/test_workflow_linux.sh
    fi
```

## Adding New Tests

When adding new workflow tests:

1. Use the existing test structure as a template
2. Add color-coded output for readability
3. Include proper cleanup in trap handler
4. Verify each step before proceeding
5. Provide clear error messages on failure
6. Document platform-specific behavior

## Debugging Test Failures

If a test fails:

1. Check the error message for the specific step that failed
2. Look in `/tmp/vibe-e2e-test-*` directories (if cleanup didn't run)
3. Check for running `vibed` processes: `ps aux | grep vibed`
4. Verify NFS mounts: `mount | grep vibe`
5. Check logs in `.vibe/` directory of test repo

To preserve test artifacts for debugging, comment out the `trap cleanup EXIT` line.
