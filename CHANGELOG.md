# Changelog

All notable changes to VibeFS will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

### Changed

### Fixed

### Removed

## [0.6.4] - 2026-01-14

### Fixed
- **NFS write corruption**: Replaced read-modify-write pattern with proper file seeking in write operations
  - Prevents data corruption during concurrent writes
  - Uses file handles with seek+write_all instead of read-modify-rewrite
- **setattr truncation**: Implemented proper file truncation via setattr (set_size3)
  - Previously setattr was a no-op, causing truncation operations to fail
- **Session file reads**: Fixed reading files created in session but not marked dirty
  - AppleDouble (`._`) files and other metadata files now read correctly
  - Resolves ENOATTR errors during `fs::copy()` and `cp` operations
- **Rename path mapping**: Added proper path mapping updates during rename operations
  - Old path mapping is now deleted when renaming files
  - Dirty flag is properly migrated to new path

### Documentation
- Updated FILE_STATE_MATRIX.md with NFS performance considerations
  - Documented `CARGO_INCREMENTAL=0` workaround for Cargo builds
  - Removed outdated "file corruption" and "rmdir" known issues

## [0.6.1] - 2026-01-14

### Changed
- **CLI naming consistency**: Standardized on `session` terminology everywhere (was mix of `vibe_id`/`session`)
- **`vibe sh` no longer auto-creates sessions**: Now requires `--create` flag to create new sessions
  - `vibe sh -s foo` errors if session doesn't exist
  - `vibe sh --create -s foo` creates session if needed
- **Removed `vibe purge -s` option**: Use `vibe close <session>` instead for closing specific sessions
  - `vibe purge` now only cleans up all VibeFS data (no session-specific option)
  - This removes lifecycle confusion between `purge -s` and `close`

### Documentation
- Cleaned up outdated documentation files
- Removed `VIBEFS_WORKFLOW.md` (redundant with `CLAUDE.md`)
- Removed `TEST_RESULTS.md` (outdated, `CHANGELOG.md` is authoritative)
- Updated `RELEASING.md` to reference `vibed` instead of `mark_dirty`

## [0.6.0] - 2026-01-14

### Fixed
- **Stale NFS mount recovery**: When daemon crashes/restarts, mounts are now properly cleaned up and remounted
  - `mount_nfs` detects existing mounts and unmounts stale ones before remounting
  - `vibe sh` now calls `mount_nfs` to handle stale mounts (like `vibe spawn` does)
  - No more hanging commands after daemon restart
- **Test cleanup**: Workflow tests now properly unmount NFS and kill daemon processes on cleanup
- **macOS code signing**: Dev install script re-signs binaries after copy to prevent SIGKILL

### Changed
- `mount_nfs` is now a public function for use by other commands
- Improved mount handling to prevent stacked/duplicate mounts

## [0.5.2] - 2026-01-14

### Fixed
- **RocksDB lock contention**: CLI commands (inspect, diff, status, promote) now use read-only mode to avoid lock conflicts with daemon
  - `vibe inspect`, `vibe diff`, `vibe status` work while daemon is running
  - `vibe promote` reads dirty paths in read-only mode
  - `vibe restore` provides helpful error message (requires daemon stop for write access)
- **Launch wrong directory**: `vibe launch` now correctly uses actual NFS mount point from SpawnInfo instead of hardcoded `/tmp/vibe/<session>` path
- **Test suite improvements**: Workflow test for restore now correctly stops daemon before restore operation

### Documentation
- Updated bug report files with fix resolutions
- Marked bug_v0.5.5 (daemon /tmp issue) as resolved - was stale binaries, not code bug
- Marked bug_v0.5.6 (RocksDB lock) as fixed
- Marked bug_v0.5.7 (daemon stop global) as NOT A BUG - daemon is correctly per-repo
- Marked bug_v0.5.8 (launch wrong directory) as fixed

## [0.5.1] - 2026-01-13

### Fixed
- NFS folder structure now correctly shows hierarchical directory structure instead of flat file list

## [0.2.9] - 2026-01-12

### Added
- `vibe close <session>` command to close individual sessions without purging all data
  - `--force` flag to skip confirmation even with dirty files
  - `--dirty` flag to only show dirty files without closing
- Session-specific purge via `vibe purge -s <session>`
- TUI dashboard improvements:
  - Full repo path in title with "(Current Repo)" indicator
  - Repository path shown in session details panel
  - Dirty file count badges per session (red [N] indicator)
  - 'd' key to view dirty files popup
  - 'c' key to close sessions directly from dashboard
  - 'p' key to show promote command hint
  - j/k vim-style navigation
- Repo name included in NFS mount point format (`<repo>-<session>`)
- `vibe status` improved formatting:
  - Table format for sessions with columns: ID, PORT, UPTIME, MOUNT POINT
  - Clear section headers (DAEMON, ACTIVE SESSIONS, OFFLINE SESSIONS)
  - PID and uptime display for daemon

### Changed
- `vibe path` no longer auto-creates sessions; only returns path for existing mounted sessions
- Mount points now use format `~/Library/Caches/vibe/mounts/<repo>-<session>`

### Fixed
- Filter out macOS resource fork files (`._filename`) from dirty file lists
- Filter out `.DS_Store` files from dirty file lists
- Backwards compatibility for old mount point format in close/purge commands

### Removed
- `vibe ls` command (use `vibe path` + standard ls instead)
- `vibe commit` command (use git directly after promote)

## [0.1.0] - 2026-01-12

### Added
- Initial release of VibeFS
- Core commands: `init`, `spawn`, `snapshot`, `promote`, `commit`
- RocksDB-based metadata store for inode tracking
- Git integration for managing parallel agent workflows
- Session-based isolated workspaces
- Copy-on-write snapshot support (Linux reflinks, macOS clonefile)
- Comprehensive test suite (16 tests passing)
- TUI dashboard placeholder
- Helper tool: `mark_dirty` for file tracking

### Architecture
- Rust implementation for safety and performance
- Git CLI integration for repository operations
- Async runtime with Tokio
- Platform-specific CoW snapshot implementations

### Documentation
- CLAUDE.md with project architecture
- DEV_SETUP.md for development environment
- TEST_RESULTS.md with comprehensive test results
- HOST_SETUP_VERIFIED.md for host configuration

### Known Limitations
- NFS server not yet implemented (planned)
- Dirty file tracking requires manual marking
- Requires RocksDB system library

[Unreleased]: https://github.com/getaifs/vibefs/compare/v0.6.1...HEAD
[0.6.1]: https://github.com/getaifs/vibefs/compare/v0.6.0...v0.6.1
[0.6.0]: https://github.com/getaifs/vibefs/compare/v0.5.2...v0.6.0
[0.5.2]: https://github.com/getaifs/vibefs/compare/v0.5.1...v0.5.2
[0.5.1]: https://github.com/getaifs/vibefs/compare/v0.2.9...v0.5.1
[0.2.9]: https://github.com/getaifs/vibefs/compare/v0.1.0...v0.2.9
[0.1.0]: https://github.com/getaifs/vibefs/releases/tag/v0.1.0
