# Releasing VibeFS

This guide explains how to create new releases of VibeFS.

## Prerequisites

1. **GitHub Repository Setup**:
   - Repository must be at `github.com/getaifs/vibefs`
   - You must have write access to create releases
   - GitHub Actions must be enabled

2. **Required Tools**:
   - Git
   - GitHub CLI (`gh`) - optional but recommended

## Release Process

### Automated Release (Recommended)

The GitHub Actions workflow automatically builds binaries for all platforms and creates a release when you push a version tag.

#### Steps:

1. **Update Version Files**:
   ```bash
   # Update Cargo.toml version
   vim Cargo.toml  # Change version = "x.y.z"

   # Update CHANGELOG.md
   vim CHANGELOG.md  # Add release notes
   ```

2. **Commit Changes**:
   ```bash
   git add Cargo.toml CHANGELOG.md
   git commit -m "chore: bump version to vX.Y.Z"
   git push origin main
   ```

3. **Create and Push Tag**:
   ```bash
   # Replace X.Y.Z with your version
   git tag -a vX.Y.Z -m "Release vX.Y.Z"
   git push origin vX.Y.Z
   ```

4. **Monitor Build**:
   - Go to: https://github.com/getaifs/vibefs/actions
   - Watch the "Release" workflow
   - It will build binaries for Linux and macOS (Intel + ARM)
   - Automatically create a GitHub release with all binaries

5. **Verify Release**:
   - Go to: https://github.com/getaifs/vibefs/releases
   - Check that the release exists with all 3 binaries
   - Test the installation script:
     ```bash
     curl -sSfL https://raw.githubusercontent.com/getaifs/vibefs/HEAD/install.sh | bash
     ```

### Manual Release (If Needed)

If the automated workflow fails or you need to manually create a release:

#### On Linux:

```bash
# Install dependencies
sudo dnf install gcc-c++ clang-devel rocksdb-devel  # Fedora
# or
sudo apt install build-essential librocksdb-dev clang  # Ubuntu

# Build
cargo build --release --target x86_64-unknown-linux-gnu

# Create archive
cd target/x86_64-unknown-linux-gnu/release
tar -czf vibe-linux-x86_64.tar.gz vibe mark_dirty
```

#### On macOS:

```bash
# Install dependencies
brew install rocksdb

# Build for Intel
cargo build --release --target x86_64-apple-darwin
cd target/x86_64-apple-darwin/release
tar -czf vibe-darwin-x86_64.tar.gz vibe mark_dirty

# Build for Apple Silicon (if on ARM Mac)
cargo build --release --target aarch64-apple-darwin
cd target/aarch64-apple-darwin/release
tar -czf vibe-darwin-aarch64.tar.gz vibe mark_dirty
```

#### Cross-compilation on macOS:

```bash
# Add targets
rustup target add x86_64-apple-darwin
rustup target add aarch64-apple-darwin

# Build both architectures
cargo build --release --target x86_64-apple-darwin
cargo build --release --target aarch64-apple-darwin
```

#### Upload Manual Release:

```bash
# Using GitHub CLI
gh release create vX.Y.Z \
  vibe-linux-x86_64.tar.gz \
  vibe-darwin-x86_64.tar.gz \
  vibe-darwin-aarch64.tar.gz \
  --title "Release vX.Y.Z" \
  --notes "See CHANGELOG.md for details"
```

Or upload via GitHub web interface:
1. Go to https://github.com/getaifs/vibefs/releases/new
2. Create tag: `vX.Y.Z`
3. Add release title and notes
4. Upload the 3 `.tar.gz` files
5. Publish release

## Release Checklist

Before releasing:

- [ ] All tests pass: `cargo test`
- [ ] Code builds cleanly: `cargo build --release`
- [ ] Version updated in `Cargo.toml`
- [ ] `CHANGELOG.md` updated with release notes
- [ ] `README.md` reflects any new features
- [ ] Documentation is up to date

After releasing:

- [ ] Verify release appears on GitHub
- [ ] Test installation script works
- [ ] Test binaries on different platforms
- [ ] Announce release (if public)

## Version Numbering

VibeFS follows [Semantic Versioning](https://semver.org/):

- **MAJOR** version (X.0.0): Incompatible API changes
- **MINOR** version (0.X.0): New functionality, backwards compatible
- **PATCH** version (0.0.X): Backwards compatible bug fixes

Examples:
- `v0.1.0` - Initial release
- `v0.1.1` - Bug fix
- `v0.2.0` - New feature
- `v1.0.0` - Stable API

## Troubleshooting

### GitHub Actions Fails

1. Check the Actions tab for error logs
2. Common issues:
   - Missing dependencies (check apt/brew install steps)
   - RocksDB linking errors
   - Target architecture not installed

### Binary Not Working

1. Verify RocksDB is available on target system
2. Check architecture matches (x86_64 vs aarch64)
3. Ensure binary has execute permissions

### Installation Script Fails

1. Check GitHub release exists
2. Verify binary names match in release
3. Test download URL manually:
   ```bash
   curl -I https://github.com/getaifs/vibefs/releases/download/vX.Y.Z/vibe-linux-x86_64.tar.gz
   ```

## CI/CD Details

The `.github/workflows/release.yml` workflow:

1. **Triggers**: On pushing tags matching `v*.*.*`
2. **Builds**: Linux (x86_64), macOS (x86_64 + ARM64)
3. **Tests**: Runs `cargo test` before building
4. **Artifacts**: Creates `.tar.gz` archives with `vibe` and `mark_dirty`
5. **Release**: Automatically creates GitHub release with all binaries

## Platform-Specific Notes

### Linux (Fedora Immutable)

Users on immutable systems need RocksDB in a container:
```bash
distrobox create --name vibefs-dev --image fedora:latest
distrobox enter vibefs-dev
sudo dnf install rocksdb-devel
curl -sSfL https://raw.githubusercontent.com/getaifs/vibefs/HEAD/install.sh | bash
```

### macOS

RocksDB must be installed via Homebrew:
```bash
brew install rocksdb
curl -sSfL https://raw.githubusercontent.com/getaifs/vibefs/HEAD/install.sh | bash
```

### Future: Static Linking

Consider static linking to eliminate RocksDB dependency:
- Research `rocksdb-sys` static features
- May increase binary size significantly
- Simplifies installation greatly

## Questions?

Open an issue or discussion on GitHub if you need help with releases.
