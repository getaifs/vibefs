#!/bin/bash
# Install VibeFS to host system from built binaries
# Works on both macOS and Linux, detects platform automatically

set -e

# Detect script directory and repo root
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Colors
GREEN='\033[0;32m'
BLUE='\033[0;34m'
RED='\033[0;31m'
NC='\033[0m'

echo -e "${BLUE}VibeFS Installation Script${NC}"
echo ""

# Check if we're in distrobox/container
IN_CONTAINER=false
if [ -f "/run/.containerenv" ] || [ -f "/.dockerenv" ]; then
    IN_CONTAINER=true
fi

# Detect platform
if [ "$(uname)" = "Darwin" ]; then
    PLATFORM="macos"
    INSTALL_DIR="${HOME}/.local/bin"
elif [ "$(uname)" = "Linux" ]; then
    PLATFORM="linux"
    INSTALL_DIR="${HOME}/.local/bin"
else
    echo -e "${RED}Unsupported platform: $(uname)${NC}"
    exit 1
fi

echo "Platform: $PLATFORM"
echo "Installing to: $INSTALL_DIR"
echo "Repo root: $REPO_ROOT"

# Check for built binaries
VIBE_BIN="$REPO_ROOT/target/release/vibe"
VIBED_BIN="$REPO_ROOT/target/release/vibed"
if [ ! -f "$VIBE_BIN" ]; then
    echo -e "${RED}Error: vibe binary not found at $VIBE_BIN${NC}"
    echo "Please build first: cargo build --release"
    exit 1
fi

# Create install directory
mkdir -p "$INSTALL_DIR"

# Install binaries
echo ""
echo "Installing binaries..."

if $IN_CONTAINER && command -v distrobox-host-exec &> /dev/null; then
    # We're in distrobox, install to host
    echo "Detected distrobox environment, installing to host..."

    # Create directory on host
    distrobox-host-exec mkdir -p "$INSTALL_DIR"

    # Copy binaries to host
    cp "$VIBE_BIN" "/tmp/vibe.tmp"
    cp "$VIBED_BIN" "/tmp/vibed.tmp"

    distrobox-host-exec cp /tmp/vibe.tmp "$INSTALL_DIR/vibe"
    distrobox-host-exec cp /tmp/vibed.tmp "$INSTALL_DIR/vibed"
    distrobox-host-exec chmod +x "$INSTALL_DIR/vibe" "$INSTALL_DIR/vibed"

    rm /tmp/vibe.tmp /tmp/vibed.tmp

    echo -e "${GREEN}✓${NC} Installed to host: $INSTALL_DIR/vibe"
    echo -e "${GREEN}✓${NC} Installed to host: $INSTALL_DIR/vibed"
else
    # Regular install
    cp "$VIBE_BIN" "$INSTALL_DIR/vibe"
    cp "$VIBED_BIN" "$INSTALL_DIR/vibed"
    chmod +x "$INSTALL_DIR/vibe" "$INSTALL_DIR/vibed"

    # Re-sign binaries on macOS (required after copy, prevents SIGKILL)
    if [ "$PLATFORM" = "macos" ]; then
        echo "Re-signing binaries for macOS..."
        codesign -s - --force "$INSTALL_DIR/vibe"
        codesign -s - --force "$INSTALL_DIR/vibed"
    fi

    echo -e "${GREEN}✓${NC} Installed: $INSTALL_DIR/vibe"
    echo -e "${GREEN}✓${NC} Installed: $INSTALL_DIR/vibed"
fi

# Check if install directory is in PATH
echo ""
if [[ ":$PATH:" == *":$INSTALL_DIR:"* ]]; then
    echo -e "${GREEN}✓${NC} $INSTALL_DIR is in your PATH"
else
    echo -e "${BLUE}ℹ${NC} Add to your PATH by adding this to ~/.bashrc or ~/.zshrc:"
    echo "  export PATH=\"$INSTALL_DIR:\$PATH\""
fi

# Verify installation
echo ""
echo "Verifying installation..."
if $IN_CONTAINER && command -v distrobox-host-exec &> /dev/null; then
    VERSION=$(distrobox-host-exec "$INSTALL_DIR/vibe" --version 2>&1 || echo "error")
else
    VERSION=$("$INSTALL_DIR/vibe" --version 2>&1 || echo "error")
fi

if [[ "$VERSION" != "error" ]]; then
    echo -e "${GREEN}✓${NC} Installation successful!"
    echo "  Version: $VERSION"
else
    echo -e "${RED}✗${NC} Installation verification failed"
    exit 1
fi

echo ""
echo -e "${GREEN}Installation complete!${NC}"
echo ""
echo "You can now use:"
echo "  vibe --help"
echo "  vibe init"
echo "  vibe spawn <session-name>"
