#!/usr/bin/env bash
# VibeFS Installation Script
# Usage: curl -sSfL https://raw.githubusercontent.com/getaifs/vibefs/HEAD/install.sh | bash

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Configuration
REPO="getaifs/vibefs"
BINARY_NAME="vibe"
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"

# Print functions
info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

error() {
    echo -e "${RED}[ERROR]${NC} $1"
    exit 1
}

# Detect OS and architecture
detect_platform() {
    local os arch

    # Detect OS
    case "$(uname -s)" in
        Linux*)     os="linux" ;;
        Darwin*)    os="darwin" ;;
        *)          error "Unsupported operating system: $(uname -s)" ;;
    esac

    # Detect architecture
    case "$(uname -m)" in
        x86_64)     arch="x86_64" ;;
        aarch64)    arch="aarch64" ;;
        arm64)      arch="aarch64" ;;
        *)          error "Unsupported architecture: $(uname -m)" ;;
    esac

    echo "${os}-${arch}"
}

# Get latest release version from GitHub
get_latest_version() {
    local version
    version=$(curl -sSf "https://api.github.com/repos/${REPO}/releases/latest" | grep '"tag_name"' | sed -E 's/.*"([^"]+)".*/\1/')

    if [ -z "$version" ]; then
        error "Failed to fetch latest version from GitHub"
    fi

    echo "$version"
}

# Download and install binary
install_binary() {
    local version="$1"
    local platform="$2"
    local download_url="https://github.com/${REPO}/releases/download/${version}/${BINARY_NAME}-${platform}.tar.gz"
    local temp_dir

    info "Downloading VibeFS ${version} for ${platform}..."

    temp_dir=$(mktemp -d)
    trap "rm -rf $temp_dir" EXIT

    # Download tarball
    if ! curl -sSfL "$download_url" -o "$temp_dir/${BINARY_NAME}.tar.gz"; then
        error "Failed to download binary from ${download_url}"
    fi

    # Extract binary
    info "Extracting binary..."
    tar -xzf "$temp_dir/${BINARY_NAME}.tar.gz" -C "$temp_dir"

    # Create install directory if it doesn't exist
    mkdir -p "$INSTALL_DIR"

    # Install binary
    info "Installing to ${INSTALL_DIR}/${BINARY_NAME}..."
    mv "$temp_dir/${BINARY_NAME}" "$INSTALL_DIR/${BINARY_NAME}"
    chmod +x "$INSTALL_DIR/${BINARY_NAME}"

    # Install helper tools if present
    if [ -f "$temp_dir/mark_dirty" ]; then
        mv "$temp_dir/mark_dirty" "$INSTALL_DIR/mark_dirty"
        chmod +x "$INSTALL_DIR/mark_dirty"
        info "Installed mark_dirty helper"
    fi
}

# Setup wrapper for immutable Linux systems
setup_distrobox_wrapper() {
    if [ ! -f /etc/os-release ]; then
        return
    fi

    # Check if running on an immutable Linux distro
    if grep -qi "silverblue\|kinoite\|fedora.*immutable" /etc/os-release 2>/dev/null; then
        info "Detected immutable Linux system"
        warn "VibeFS requires RocksDB libraries. Consider running in distrobox:"
        warn "  distrobox create --name vibefs-dev --image fedora:latest"
        warn "  distrobox enter vibefs-dev"
        warn "  Inside container: sudo dnf install rocksdb-devel && curl ... | bash"
    fi
}

# Check if install directory is in PATH
check_path() {
    if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
        warn "${INSTALL_DIR} is not in your PATH"
        warn "Add this to your shell profile (~/.bashrc, ~/.zshrc, etc.):"
        warn "  export PATH=\"\$PATH:${INSTALL_DIR}\""
    fi
}

# Main installation flow
main() {
    info "VibeFS Installer"
    info "================"
    echo

    # Detect platform
    local platform
    platform=$(detect_platform)
    info "Detected platform: ${platform}"

    # Get latest version
    local version
    version=$(get_latest_version)
    info "Latest version: ${version}"
    echo

    # Install binary
    install_binary "$version" "$platform"
    echo

    # Setup for special cases
    setup_distrobox_wrapper
    echo

    # Check PATH
    check_path
    echo

    # Success message
    info "✓ VibeFS installed successfully!"
    info "Run 'vibe --help' to get started"
    echo
    info "For more information, visit: https://github.com/${REPO}"
}

# Run main function
main "$@"
