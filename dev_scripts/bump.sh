#!/bin/bash
# Version bump script for VibeFS
# Usage: ./dev_scripts/bump.sh [patch|minor|major|VERSION]
# Examples:
#   ./dev_scripts/bump.sh patch     # 0.7.2 -> 0.7.3
#   ./dev_scripts/bump.sh minor     # 0.7.2 -> 0.8.0
#   ./dev_scripts/bump.sh major     # 0.7.2 -> 1.0.0
#   ./dev_scripts/bump.sh 0.8.0     # Set specific version

set -e

# Colors
GREEN='\033[0;32m'
BLUE='\033[0;34m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m'

# Detect script directory and repo root
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

# Get current version from Cargo.toml
CURRENT_VERSION=$(grep '^version = ' Cargo.toml | head -1 | sed 's/version = "\(.*\)"/\1/')

echo -e "${BLUE}VibeFS Version Bump${NC}"
echo "Current version: $CURRENT_VERSION"
echo ""

# Parse bump type or explicit version
BUMP_TYPE="${1:-patch}"

calculate_new_version() {
    local current="$1"
    local bump="$2"

    # Parse semver
    IFS='.' read -r major minor patch <<< "$current"

    case "$bump" in
        patch)
            patch=$((patch + 1))
            ;;
        minor)
            minor=$((minor + 1))
            patch=0
            ;;
        major)
            major=$((major + 1))
            minor=0
            patch=0
            ;;
        *)
            # Assume explicit version
            if [[ "$bump" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
                echo "$bump"
                return 0
            else
                echo -e "${RED}Invalid version format: $bump${NC}" >&2
                echo "Use: patch, minor, major, or X.Y.Z" >&2
                exit 1
            fi
            ;;
    esac

    echo "${major}.${minor}.${patch}"
}

NEW_VERSION=$(calculate_new_version "$CURRENT_VERSION" "$BUMP_TYPE")

echo "New version: $NEW_VERSION"
echo ""

# Confirm
read -p "Proceed with bump to v$NEW_VERSION? [y/N] " -n 1 -r
echo ""
if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    echo "Aborted."
    exit 1
fi

# Update Cargo.toml
echo -e "${BLUE}Updating Cargo.toml...${NC}"
sed -i.bak "s/^version = \"$CURRENT_VERSION\"/version = \"$NEW_VERSION\"/" Cargo.toml
rm Cargo.toml.bak
echo -e "${GREEN}✓${NC} Updated Cargo.toml"

# Update Cargo.lock
echo -e "${BLUE}Updating Cargo.lock...${NC}"
cargo check --quiet 2>/dev/null || true
echo -e "${GREEN}✓${NC} Updated Cargo.lock"

# Run tests
echo -e "${BLUE}Running tests...${NC}"
if cargo test --quiet; then
    echo -e "${GREEN}✓${NC} Tests passed"
else
    echo -e "${RED}✗${NC} Tests failed! Rolling back..."
    git checkout Cargo.toml Cargo.lock
    exit 1
fi

# Build release
echo -e "${BLUE}Building release...${NC}"
cargo build --release --quiet
echo -e "${GREEN}✓${NC} Build successful"

# Git operations
echo -e "${BLUE}Creating commit...${NC}"
git add Cargo.toml Cargo.lock
git commit -m "chore: bump version to $NEW_VERSION"
echo -e "${GREEN}✓${NC} Committed"

echo -e "${BLUE}Creating tag v$NEW_VERSION...${NC}"
git tag -a "v$NEW_VERSION" -m "Release v$NEW_VERSION"
echo -e "${GREEN}✓${NC} Tagged"

# Push
echo ""
read -p "Push commit and tag to origin? [y/N] " -n 1 -r
echo ""
if [[ $REPLY =~ ^[Yy]$ ]]; then
    echo -e "${BLUE}Pushing...${NC}"
    git push && git push --tags
    echo -e "${GREEN}✓${NC} Pushed"
else
    echo -e "${YELLOW}Skipped push. Run manually:${NC}"
    echo "  git push && git push --tags"
fi

echo ""
echo -e "${GREEN}Version bumped to v$NEW_VERSION${NC}"
echo ""
echo "Next steps:"
echo "  1. Run ./dev_scripts/release.sh to upload artifacts"
echo "  2. Or wait for GitHub Actions to build automatically"
