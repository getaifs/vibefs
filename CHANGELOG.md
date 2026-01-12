# Changelog

All notable changes to VibeFS will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

### Changed

### Fixed

### Removed

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

[Unreleased]: https://github.com/getaifs/vibefs/compare/v0.2.9...HEAD
[0.2.9]: https://github.com/getaifs/vibefs/compare/v0.1.0...v0.2.9
[0.1.0]: https://github.com/getaifs/vibefs/releases/tag/v0.1.0
