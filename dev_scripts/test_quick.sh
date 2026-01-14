#!/bin/bash
# Quick smoke test for VibeFS - cross-platform
# Tests basic functionality without full workflow

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

TEST_DIR="/tmp/vibe-quick-test-$$"
VIBE_BIN="${PWD}/target/debug/vibe"

echo_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

echo_success() {
    echo -e "${GREEN}[✓]${NC} $1"
}

echo_error() {
    echo -e "${RED}[✗]${NC} $1"
}

cleanup() {
    cd /tmp
    if [ -d "$TEST_DIR" ]; then
        rm -rf "$TEST_DIR"
    fi
}

trap cleanup EXIT

echo_info "Running quick smoke test..."
echo ""

# Build
echo_info "Building VibeFS..."
cargo build --quiet
echo_success "Build successful"

# Setup test repo
mkdir -p "$TEST_DIR"
cd "$TEST_DIR"
git init -q
git config user.name "Test"
git config user.email "test@test.com"
echo "test" > test.txt
git add .
git commit -q -m "init"
echo_success "Test repo created"

# Test init
"$VIBE_BIN" init >/dev/null 2>&1
if [ -d ".vibe" ] && [ -f ".vibe/metadata.db/CURRENT" ]; then
    echo_success "vibe init works"
else
    echo_error "vibe init failed"
    exit 1
fi

# Test status (should work even without spawned session)
if "$VIBE_BIN" status >/dev/null 2>&1; then
    echo_success "vibe status works"
else
    echo_info "vibe status not yet implemented or errored (non-critical)"
fi

# Test help
if "$VIBE_BIN" --help >/dev/null 2>&1; then
    echo_success "vibe --help works"
else
    echo_error "vibe --help failed"
    exit 1
fi

echo ""
echo_success "Quick smoke test passed!"
echo_info "Run test_workflow_*.sh for full end-to-end testing"
