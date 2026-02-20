#!/bin/bash
# Daemon scenario tests for VibeFS
# Run from the vibefs directory with: ./tests/daemon_scenarios.sh

SCRIPT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
VIBE="$SCRIPT_DIR/target/debug/vibe"
VIBED="$SCRIPT_DIR/target/debug/vibed"
TEST_DIR=$(mktemp -d)

# Cleanup function
cleanup() {
    # Stop any daemons we started
    for repo in "$TEST_DIR"/repo*; do
        if [ -d "$repo/.vibe" ]; then
            "$VIBE" -r "$repo" daemon stop 2>/dev/null || true
        fi
    done
    sleep 1
    rm -rf "$TEST_DIR"
}
trap cleanup EXIT

echo "=== VibeFS Daemon Scenario Tests ==="
echo "Test directory: $TEST_DIR"
echo ""

# Build first
echo "Building..."
cd "$SCRIPT_DIR"
cargo build --quiet
echo "Using vibe: $VIBE"
echo "Using vibed: $VIBED"
echo ""

# Helper function to create a test repo
setup_test_repo() {
    local repo_dir="$1"
    mkdir -p "$repo_dir"
    cd "$repo_dir"
    git init --quiet
    git config user.name "Test"
    git config user.email "test@test.com"
    echo "test" > README.md
    git add .
    git commit -m "init" --quiet
    cd "$SCRIPT_DIR"
}

PASS=0
FAIL=0

check() {
    if [ "$1" = "0" ]; then
        echo "  PASS: $2"
        PASS=$((PASS + 1))
    else
        echo "  FAIL: $2"
        FAIL=$((FAIL + 1))
    fi
}

# Test 1: Clean startup
echo "Test 1: Clean daemon startup"
REPO1="$TEST_DIR/repo1"
setup_test_repo "$REPO1"

OUTPUT=$("$VIBE" -r "$REPO1" init 2>&1)
echo "$OUTPUT" | grep -q "initialized successfully"
check $? "init succeeded"

OUTPUT=$("$VIBE" -r "$REPO1" new test-session -c "true" 2>&1)
echo "$OUTPUT" | grep -q "spawned successfully"
check $? "spawn succeeded"

OUTPUT=$("$VIBE" -r "$REPO1" daemon status 2>&1)
echo "$OUTPUT" | grep -q "Repository"
check $? "daemon running"

OUTPUT=$("$VIBE" -r "$REPO1" daemon stop 2>&1)
echo "$OUTPUT" | grep -q "shutdown"
check $? "daemon stopped"
sleep 1
echo ""

# Test 2: Stale socket cleanup
echo "Test 2: Stale socket cleanup"
REPO2="$TEST_DIR/repo2"
setup_test_repo "$REPO2"
"$VIBE" -r "$REPO2" init >/dev/null 2>&1

# Create a stale socket file (regular file, not actual socket)
mkdir -p "$REPO2/.vibe"
touch "$REPO2/.vibe/vibed.sock"
echo "  Created stale socket file"

# Try to spawn - should clean up stale socket and succeed
OUTPUT=$("$VIBE" -r "$REPO2" new stale-test -c "true" 2>&1)
if echo "$OUTPUT" | grep -q "Cleaning up stale socket"; then
    echo "  PASS: Detected and cleaned stale socket"
    PASS=$((PASS + 1))
elif echo "$OUTPUT" | grep -q "spawned successfully"; then
    echo "  PASS: Spawn succeeded (socket was cleaned)"
    PASS=$((PASS + 1))
else
    echo "  FAIL: Did not handle stale socket"
    echo "    Output: $OUTPUT"
    FAIL=$((FAIL + 1))
fi

"$VIBE" -r "$REPO2" daemon stop 2>/dev/null || true
sleep 1
echo ""

# Test 3: Stale PID file cleanup
echo "Test 3: Stale PID file cleanup"
REPO3="$TEST_DIR/repo3"
setup_test_repo "$REPO3"
"$VIBE" -r "$REPO3" init >/dev/null 2>&1

# Create a stale PID file with a non-existent PID
mkdir -p "$REPO3/.vibe"
echo "999999" > "$REPO3/.vibe/vibed.pid"
echo "  Created stale PID file (pid 999999)"

# Try to spawn - should clean up stale PID and succeed
OUTPUT=$("$VIBE" -r "$REPO3" new pid-test -c "true" 2>&1)
if echo "$OUTPUT" | grep -q "Cleaning up stale PID"; then
    echo "  PASS: Detected and cleaned stale PID file"
    PASS=$((PASS + 1))
elif echo "$OUTPUT" | grep -q "spawned successfully"; then
    echo "  PASS: Spawn succeeded (PID file was cleaned)"
    PASS=$((PASS + 1))
else
    echo "  FAIL: Did not handle stale PID file"
    echo "    Output: $OUTPUT"
    FAIL=$((FAIL + 1))
fi

"$VIBE" -r "$REPO3" daemon stop 2>/dev/null || true
sleep 1
echo ""

# Test 4: Multiple repos (independent daemons)
echo "Test 4: Multiple repos with independent daemons"
REPO4A="$TEST_DIR/repo4a"
REPO4B="$TEST_DIR/repo4b"
setup_test_repo "$REPO4A"
setup_test_repo "$REPO4B"
"$VIBE" -r "$REPO4A" init >/dev/null 2>&1
"$VIBE" -r "$REPO4B" init >/dev/null 2>&1

# Start daemon in repo4a
OUTPUT=$("$VIBE" -r "$REPO4A" new session-a -c "true" 2>&1)
echo "$OUTPUT" | grep -q "spawned successfully"
check $? "repo4a daemon started"

# Start daemon in repo4b (should be independent)
OUTPUT=$("$VIBE" -r "$REPO4B" new session-b -c "true" 2>&1)
echo "$OUTPUT" | grep -q "spawned successfully"
check $? "repo4b daemon started"

# Both should be running
OUTPUT=$("$VIBE" -r "$REPO4A" daemon status 2>&1)
echo "$OUTPUT" | grep -q "Repository"
check $? "repo4a daemon still running"

OUTPUT=$("$VIBE" -r "$REPO4B" daemon status 2>&1)
echo "$OUTPUT" | grep -q "Repository"
check $? "repo4b daemon still running"

"$VIBE" -r "$REPO4A" daemon stop 2>/dev/null || true
"$VIBE" -r "$REPO4B" daemon stop 2>/dev/null || true
sleep 1
echo ""

# Test 5: Error message includes binary path
echo "Test 5: Error messages include diagnostic info"
REPO5="$TEST_DIR/repo5"
setup_test_repo "$REPO5"
"$VIBE" -r "$REPO5" init >/dev/null 2>&1

# Create both stale socket and PID so we can see the cleanup messages
mkdir -p "$REPO5/.vibe"
touch "$REPO5/.vibe/vibed.sock"
echo "999999" > "$REPO5/.vibe/vibed.pid"

OUTPUT=$("$VIBE" -r "$REPO5" new diagnostic-test -c "true" 2>&1)
if echo "$OUTPUT" | grep -q "Starting daemon:"; then
    echo "  PASS: Shows which binary is being used"
    PASS=$((PASS + 1))
else
    echo "  FAIL: Missing binary path in output"
    echo "    Output: $OUTPUT"
    FAIL=$((FAIL + 1))
fi

"$VIBE" -r "$REPO5" daemon stop 2>/dev/null || true
sleep 1
echo ""

echo "=== Test Results ==="
echo "Passed: $PASS"
echo "Failed: $FAIL"
echo ""

if [ "$FAIL" -gt 0 ]; then
    exit 1
fi
exit 0
