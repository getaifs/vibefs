#!/bin/bash
# Manual release script for VibeFS
# Builds and uploads release to GitHub
# Assumes: version in Cargo.toml is correct, tag exists, gh CLI installed

set -e

# Colors
GREEN='\033[0;32m'
BLUE='\033[0;34m'
RED='\033[0;31m'
NC='\033[0m'

# Detect script directory and repo root
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

# Get version from Cargo.toml
VERSION=$(grep '^version = ' Cargo.toml | head -1 | sed 's/version = "\(.*\)"/\1/')
TAG="v$VERSION"

echo -e "${BLUE}VibeFS Release Script${NC}"
echo "Version: $VERSION"
echo "Tag: $TAG"
echo ""

# Check gh CLI
if ! command -v gh &> /dev/null; then
    echo -e "${RED}Error: gh CLI not installed${NC}"
    echo "Install: https://cli.github.com/"
    exit 1
fi

# Check gh auth
if ! gh auth status &> /dev/null; then
    echo -e "${RED}Error: gh CLI not authenticated${NC}"
    echo "Run: gh auth login"
    exit 1
fi

# Check tag exists
if ! git rev-parse "$TAG" &> /dev/null; then
    echo -e "${RED}Error: Tag $TAG does not exist${NC}"
    echo "Create it with: git tag -a $TAG -m 'Release $TAG' && git push origin $TAG"
    exit 1
fi

# Detect platform and architecture
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

case "$OS" in
    darwin) PLATFORM="darwin" ;;
    linux) PLATFORM="linux" ;;
    *) echo -e "${RED}Unsupported OS: $OS${NC}"; exit 1 ;;
esac

case "$ARCH" in
    x86_64) ARCH_NAME="x86_64" ;;
    arm64|aarch64) ARCH_NAME="aarch64" ;;
    *) echo -e "${RED}Unsupported arch: $ARCH${NC}"; exit 1 ;;
esac

ARTIFACT_NAME="vibe-${PLATFORM}-${ARCH_NAME}.tar.gz"
echo "Building for: $PLATFORM-$ARCH_NAME"
echo "Artifact: $ARTIFACT_NAME"
echo ""

# Run tests
echo -e "${BLUE}Running tests...${NC}"
cargo test --quiet
echo -e "${GREEN}✓${NC} Tests passed"

# Build release
echo -e "${BLUE}Building release...${NC}"
cargo build --release --quiet
echo -e "${GREEN}✓${NC} Build complete"

# Create tarball
echo -e "${BLUE}Creating tarball...${NC}"
cd "$REPO_ROOT/target/release"
tar -czf "$ARTIFACT_NAME" vibe vibed
mv "$ARTIFACT_NAME" "$REPO_ROOT/"
cd "$REPO_ROOT"
echo -e "${GREEN}✓${NC} Created $ARTIFACT_NAME"

# Check if release exists
if gh release view "$TAG" &> /dev/null; then
    echo ""
    echo -e "${BLUE}Release $TAG exists. Uploading artifact...${NC}"
    gh release upload "$TAG" "$ARTIFACT_NAME" --clobber
else
    echo ""
    echo -e "${BLUE}Creating release $TAG...${NC}"
    gh release create "$TAG" "$ARTIFACT_NAME" \
        --title "Release $TAG" \
        --notes "See CHANGELOG.md for details"
fi

echo -e "${GREEN}✓${NC} Uploaded $ARTIFACT_NAME to release $TAG"

# Cleanup
rm "$ARTIFACT_NAME"

echo ""
echo -e "${GREEN}Release complete!${NC}"
echo "View at: https://github.com/getaifs/vibefs/releases/tag/$TAG"
