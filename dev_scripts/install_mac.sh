#!/bin/bash
# VibeFS v0.2 Installation Script for macOS
# This script builds and installs VibeFS locally

set -e

INSTALL_DIR="${HOME}/.local/bin"
CACHE_DIR="${HOME}/Library/Caches/vibe"
MOUNT_DIR="${CACHE_DIR}/mounts"

echo "==================================="
echo "  VibeFS v0.2 Installer for macOS"
echo "==================================="
echo ""

# Check for Rust toolchain
if ! command -v cargo &> /dev/null; then
    echo "Error: Rust toolchain not found."
    echo "Please install Rust from https://rustup.rs/"
    exit 1
fi

# Check for macOS
if [[ "$(uname)" != "Darwin" ]]; then
    echo "Warning: This installer is designed for macOS."
    echo "Continuing anyway..."
fi

# Build in release mode
echo "Building VibeFS in release mode..."
cargo build --release

# Create installation directories
echo "Creating directories..."
mkdir -p "${INSTALL_DIR}"
mkdir -p "${MOUNT_DIR}"

# Install binaries
echo "Installing binaries to ${INSTALL_DIR}..."
cp target/release/vibe "${INSTALL_DIR}/"
cp target/release/vibed "${INSTALL_DIR}/"
chmod +x "${INSTALL_DIR}/vibe"
chmod +x "${INSTALL_DIR}/vibed"

# Check if INSTALL_DIR is in PATH
if [[ ":$PATH:" != *":${INSTALL_DIR}:"* ]]; then
    echo ""
    echo "Warning: ${INSTALL_DIR} is not in your PATH."
    echo ""
    echo "Add one of these to your shell profile:"
    echo ""
    echo "  For bash (~/.bashrc or ~/.bash_profile):"
    echo "    export PATH=\"\${HOME}/.local/bin:\${PATH}\""
    echo ""
    echo "  For zsh (~/.zshrc):"
    echo "    export PATH=\"\${HOME}/.local/bin:\${PATH}\""
    echo ""
fi

echo ""
echo "==================================="
echo "  Installation Complete!"
echo "==================================="
echo ""
echo "Installed binaries:"
echo "  - ${INSTALL_DIR}/vibe"
echo "  - ${INSTALL_DIR}/vibed"
echo ""
echo "Cache directory: ${CACHE_DIR}"
echo "Mount directory: ${MOUNT_DIR}"
echo ""
echo "Quick Start:"
echo "  1. cd into a git repository"
echo "  2. Run: vibe init"
echo "  3. Run: vibe spawn my-session"
echo "  4. Run: vibe status"
echo ""
echo "For more info: vibe --help"
