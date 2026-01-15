#!/bin/bash
# Quick local test script for VibeFS
# Builds, installs, and runs through key commands
# Usage: ./dev_scripts/test-local.sh [--quick|--interactive|--agent]

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

# Test directory
TEST_DIR="/tmp/vibe-local-test"
VIBE_BIN="$HOME/.local/bin/vibe"

MODE="${1:---quick}"

echo -e "${BLUE}VibeFS Local Test${NC}"
echo "Mode: $MODE"
echo ""

# Step 1: Build
echo -e "${BLUE}[1/4] Building release...${NC}"
cargo build --release --quiet
echo -e "${GREEN}✓${NC} Build complete"

# Step 2: Install
echo -e "${BLUE}[2/4] Installing locally...${NC}"
"$SCRIPT_DIR/install.sh" > /dev/null
echo -e "${GREEN}✓${NC} Installed"

# Step 3: Show version
echo -e "${BLUE}[3/4] Verifying installation...${NC}"
VERSION=$("$VIBE_BIN" --version)
echo -e "${GREEN}✓${NC} $VERSION"

# Step 4: Cleanup old test directory
echo -e "${BLUE}[4/4] Preparing test environment...${NC}"
# Kill any existing daemon
pkill -9 vibed 2>/dev/null || true
sleep 0.5

# Unmount any stale mounts
mount_dir="$HOME/Library/Caches/vibe/mounts"
if [ -d "$mount_dir" ]; then
    for mount in $(mount | grep "$mount_dir" | awk '{print $3}'); do
        umount -f "$mount" 2>/dev/null || diskutil unmount force "$mount" 2>/dev/null || true
    done
fi

rm -rf "$TEST_DIR" 2>/dev/null || true
mkdir -p "$TEST_DIR"
cd "$TEST_DIR"

# Create test repo
git init --quiet
git config user.name "Test User"
git config user.email "test@example.com"
mkdir -p src
echo '# Test Project' > README.md
echo 'fn main() { println!("Hello"); }' > src/main.rs
git add .
git commit -m "Initial commit" --quiet

echo -e "${GREEN}✓${NC} Test repo created at $TEST_DIR"
echo ""

# Run mode-specific tests
case "$MODE" in
    --quick)
        echo -e "${BLUE}Running quick smoke test...${NC}"
        echo ""

        # Test basic commands
        echo "Testing: vibe init"
        "$VIBE_BIN" init
        echo ""

        echo "Testing: vibe new test-session -c 'ls'"
        "$VIBE_BIN" new test-session -c "ls -la"
        echo ""

        echo "Testing: vibe status"
        "$VIBE_BIN" status
        echo ""

        echo "Testing: vibe diff test-session"
        "$VIBE_BIN" diff test-session || true
        echo ""

        echo "Testing: vibe close test-session -f"
        "$VIBE_BIN" close test-session -f
        echo ""

        # Cleanup
        pkill -9 vibed 2>/dev/null || true
        rm -rf "$TEST_DIR"

        echo -e "${GREEN}Quick test passed!${NC}"
        ;;

    --interactive)
        echo -e "${BLUE}Starting interactive test session...${NC}"
        echo ""
        echo "Test repo: $TEST_DIR"
        echo ""
        echo "Try these commands:"
        echo "  vibe                    # Launch TUI dashboard"
        echo "  vibe new my-session     # Create session and enter shell"
        echo "  vibe status             # Show status"
        echo "  vibe diff               # Show changes"
        echo "  vibe close --all -f     # Clean up"
        echo ""
        echo "Press Enter to launch an interactive shell in the test repo..."
        read -r
        cd "$TEST_DIR"
        exec "$SHELL"
        ;;

    --agent)
        echo -e "${BLUE}Testing agent launch flow...${NC}"
        echo ""

        # Check if mock-agent exists
        MOCK_AGENT="$SCRIPT_DIR/mock-agent"
        if [ ! -x "$MOCK_AGENT" ]; then
            echo -e "${RED}mock-agent not found at $MOCK_AGENT${NC}"
            echo "Create it first or use a real agent."
            exit 1
        fi

        # Add mock-agent to PATH temporarily
        export PATH="$SCRIPT_DIR:$PATH"

        echo "Testing: vibe mock-agent"
        "$VIBE_BIN" mock-agent
        echo ""

        echo "Testing: vibe mock-agent --test-flag --another-flag"
        "$VIBE_BIN" mock-agent --test-flag --another-flag
        echo ""

        echo -e "${GREEN}Agent test passed!${NC}"

        # Cleanup
        "$VIBE_BIN" close --all -f 2>/dev/null || true
        pkill -9 vibed 2>/dev/null || true
        rm -rf "$TEST_DIR"
        ;;

    --workflow)
        echo -e "${BLUE}Running full workflow tests...${NC}"
        echo "(This runs tests/workflow_tests.sh)"
        echo ""
        cd "$REPO_ROOT"
        ./tests/workflow_tests.sh
        ;;

    *)
        echo -e "${RED}Unknown mode: $MODE${NC}"
        echo "Usage: $0 [--quick|--interactive|--agent|--workflow]"
        exit 1
        ;;
esac
