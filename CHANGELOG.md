# Changelog

All notable changes to VibeFS will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Installation script for easy deployment via curl
- GitHub Actions workflow for automated releases
- Cross-platform binary releases (Linux, macOS Intel, macOS ARM)
- Bootstrap agent documentation on `vibe init`
- RELEASING.md guide for cutting releases

### Changed

### Fixed

### Removed

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

[Unreleased]: https://github.com/getaifs/vibefs/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/getaifs/vibefs/releases/tag/v0.1.0
