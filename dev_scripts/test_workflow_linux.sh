#!/bin/bash
# VibeFS End-to-End Workflow Test for Linux
# Tests the complete workflow: init -> spawn -> modify -> promote -> commit

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Test configuration
TEST_DIR="/tmp/vibe-e2e-test-$$"
AGENT_ID="test-agent-$$"
VIBE_BIN="${PWD}/target/debug/vibe"
VIBED_BIN="${PWD}/target/debug/vibed"

echo_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

echo_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

echo_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

echo_step() {
    echo ""
    echo -e "${YELLOW}==== $1 ====${NC}"
}

cleanup() {
    echo_step "Cleanup"
    cd /tmp
    if [ -d "$TEST_DIR" ]; then
        # Kill any running vibed processes for this test
        pkill -f "vibed.*${TEST_DIR}" || true
        sleep 1

        # Unmount any NFS mounts (Linux-specific)
        if mount | grep -q "$TEST_DIR"; then
            echo_info "Unmounting NFS mounts..."
            find "${TEST_DIR}/.vibe/mounts" -type d -maxdepth 1 -mindepth 1 2>/dev/null | while read mount; do
                umount -l "$mount" 2>/dev/null || true
            done
        fi

        echo_info "Removing test directory: $TEST_DIR"
        rm -rf "$TEST_DIR"
    fi
}

trap cleanup EXIT

# Check for required tools
echo_step "Checking prerequisites"
if ! command -v cargo &> /dev/null; then
    echo_error "cargo not found. Please install Rust toolchain."
    exit 1
fi

# Check if we can mount NFS (might need permissions)
if ! command -v mount.nfs4 &> /dev/null && ! command -v mount.nfs &> /dev/null; then
    echo_error "NFS client tools not found. Please install nfs-common (Debian/Ubuntu) or nfs-utils (Fedora/RHEL)"
    exit 1
fi

echo_success "Prerequisites OK"

# Build VibeFS
echo_step "Building VibeFS (debug mode)"
cargo build
echo_success "Build complete"

# Create test repository
echo_step "Setting up test repository"
mkdir -p "$TEST_DIR"
cd "$TEST_DIR"

git init
git config user.name "Test User"
git config user.email "test@example.com"

# Create initial commit with some files
cat > README.md << 'EOF'
# Test Repository

This is a test repository for VibeFS.
EOF

cat > main.rs << 'EOF'
fn main() {
    println!("Hello, VibeFS!");
}
EOF

mkdir -p src
cat > src/lib.rs << 'EOF'
pub fn hello() -> &'static str {
    "Hello from lib"
}
EOF

git add .
git commit -m "Initial commit"
echo_success "Test repository created with initial commit"

# Test: vibe init
echo_step "Test 1: vibe init"
"$VIBE_BIN" init
if [ ! -d ".vibe" ]; then
    echo_error "Failed: .vibe directory not created"
    exit 1
fi
if [ ! -f ".vibe/metadata.db/CURRENT" ]; then
    echo_error "Failed: metadata.db not initialized"
    exit 1
fi
echo_success "vibe init completed successfully"

# Test: vibe spawn
echo_step "Test 2: vibe spawn"
"$VIBE_BIN" spawn "$AGENT_ID" &
SPAWN_PID=$!
sleep 3  # Give it time to mount

# Check if session directory exists
if [ ! -d ".vibe/sessions/$AGENT_ID" ]; then
    echo_error "Failed: session directory not created"
    exit 1
fi

# Check if mount point exists (optional - NFS mounting may not work in containers)
# On Linux, mounts are in ~/.cache/vibe/mounts/
MOUNT_BASE="$HOME/.cache/vibe/mounts"
MOUNT_POINT=$(find "$MOUNT_BASE" -name "*$AGENT_ID*" -type d 2>/dev/null | head -n1)
if [ -n "$MOUNT_POINT" ]; then
    echo_info "Mount point: $MOUNT_POINT"
    echo_success "NFS mount succeeded"
else
    echo_info "NFS mount not available (this is OK for testing - using session dir directly)"
fi

echo_success "vibe spawn completed successfully"

# Test: Make changes in session
echo_step "Test 3: Modify files in session"
SESSION_DIR=".vibe/sessions/$AGENT_ID"

# Modify existing file
echo "// Modified by agent" >> "$SESSION_DIR/main.rs"

# Create new file
cat > "$SESSION_DIR/new_file.rs" << 'EOF'
pub fn new_function() {
    println!("This is a new file");
}
EOF

# Create directory with file
mkdir -p "$SESSION_DIR/tests"
cat > "$SESSION_DIR/tests/integration.rs" << 'EOF'
#[test]
fn test_example() {
    assert_eq!(2 + 2, 4);
}
EOF

echo_success "Made changes in session directory"

# Test: mark_dirty (if implemented)
echo_step "Test 4: Mark dirty files"
# Note: mark_dirty might be a separate command or part of vibe
# For now, we'll skip if it doesn't exist
if command -v mark_dirty &> /dev/null; then
    cd "$SESSION_DIR"
    mark_dirty . main.rs new_file.rs tests/integration.rs
    cd "$TEST_DIR"
    echo_success "Files marked as dirty"
else
    echo_info "mark_dirty command not found, skipping..."
fi

# Test: vibe promote
echo_step "Test 5: vibe promote"
"$VIBE_BIN" promote "$AGENT_ID" || {
    echo_error "Failed: vibe promote failed"
    kill $SPAWN_PID 2>/dev/null || true
    exit 1
}

# Check if ref was created
if ! git show-ref "refs/vibes/$AGENT_ID" &>/dev/null; then
    echo_error "Failed: refs/vibes/$AGENT_ID not created"
    kill $SPAWN_PID 2>/dev/null || true
    exit 1
fi

echo_success "vibe promote completed successfully"

# Verify the promoted commit contains our changes
echo_step "Test 6: Verify promoted changes"
PROMOTED_HASH=$(git rev-parse "refs/vibes/$AGENT_ID")
echo_info "Promoted commit: $PROMOTED_HASH"

# Check if new file exists in the commit
if ! git ls-tree -r "$PROMOTED_HASH" | grep -q "new_file.rs"; then
    echo_error "Failed: new_file.rs not in promoted commit"
    kill $SPAWN_PID 2>/dev/null || true
    exit 1
fi

echo_success "Promoted commit contains expected changes"

# Test: Merge promoted changes using standard Git
echo_step "Test 7: Merge promoted changes with git merge"
CURRENT_HEAD=$(git rev-parse HEAD)

# Use standard git merge to integrate the promoted changes
git merge --ff-only "refs/vibes/$AGENT_ID" -m "Merge vibe session $AGENT_ID" || {
    echo_error "Failed: git merge failed"
    kill $SPAWN_PID 2>/dev/null || true
    exit 1
}

# Verify HEAD moved
NEW_HEAD=$(git rev-parse HEAD)
if [ "$CURRENT_HEAD" = "$NEW_HEAD" ]; then
    echo_error "Failed: HEAD did not move after merge"
    exit 1
fi

if [ "$NEW_HEAD" != "$PROMOTED_HASH" ]; then
    echo_error "Failed: HEAD is not pointing to promoted commit"
    exit 1
fi

echo_success "Git merge completed successfully"

# Verify changes are in working tree
echo_step "Test 8: Verify final state"
if [ ! -f "new_file.rs" ]; then
    echo_error "Failed: new_file.rs not in working tree"
    exit 1
fi

if [ ! -f "tests/integration.rs" ]; then
    echo_error "Failed: tests/integration.rs not in working tree"
    exit 1
fi

if ! grep -q "Modified by agent" main.rs; then
    echo_error "Failed: main.rs modifications not present"
    exit 1
fi

echo_success "All changes present in working tree"

# Kill the spawn process
kill $SPAWN_PID 2>/dev/null || true
wait $SPAWN_PID 2>/dev/null || true

# Final summary
echo ""
echo_step "Test Summary"
echo_success "All workflow tests passed! ✓"
echo ""
echo "Tests completed:"
echo "  ✓ vibe init"
echo "  ✓ vibe spawn"
echo "  ✓ File modifications in session"
echo "  ✓ vibe promote"
echo "  ✓ Promoted commit verification"
echo "  ✓ Git merge of promoted changes"
echo "  ✓ Final state verification"
echo ""
echo_info "Test artifacts cleaned up"
