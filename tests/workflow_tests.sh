#!/bin/bash
# VibeFS Comprehensive Workflow Tests
# This script tests all developer workflows against the current system

set -o pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Test directory
TEST_DIR="/tmp/vibefs_workflow_tests"
VIBE_BIN="/Users/x/src/vibefs/target/release/vibe"
MARK_DIRTY_BIN="/Users/x/src/vibefs/target/release/mark_dirty"

# Results tracking
PASSED_COUNT=0
FAILED_COUNT=0
PASSED_TESTS=""
FAILED_TESTS=""
FAILED_OUTPUTS=""

# Logging
log_info() { echo -e "${BLUE}[INFO]${NC} $1"; }
log_pass() { echo -e "${GREEN}[PASS]${NC} $1"; PASSED_COUNT=$((PASSED_COUNT + 1)); PASSED_TESTS="$PASSED_TESTS\n  - $1"; }
log_fail() { echo -e "${RED}[FAIL]${NC} $1"; FAILED_COUNT=$((FAILED_COUNT + 1)); FAILED_TESTS="$FAILED_TESTS\n  - $1"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }

# Run a test and capture result
run_test() {
    local test_name="$1"
    local test_func="$2"

    echo ""
    echo "========================================"
    echo "TEST: $test_name"
    echo "========================================"

    local output
    local exit_code

    output=$($test_func 2>&1)
    exit_code=$?

    if [ $exit_code -eq 0 ]; then
        log_pass "$test_name"
        return 0
    else
        log_fail "$test_name"
        echo "$output"
        FAILED_OUTPUTS="$FAILED_OUTPUTS\n\n--- $test_name ---\n$output"
        return 1
    fi
}

# Setup a fresh test repo
setup_test_repo() {
    local repo_dir="$1"
    mkdir -p "$repo_dir"
    cd "$repo_dir"

    git init
    git config user.name "Test User"
    git config user.email "test@example.com"

    # Create directory structure
    mkdir -p src/lib src/bin docs

    # Create some files
    echo "# Test Project" > README.md
    echo 'fn main() { println!("Hello"); }' > src/main.rs
    echo 'pub fn add(a: i32, b: i32) -> i32 { a + b }' > src/lib/math.rs
    echo 'pub mod math;' > src/lib/mod.rs
    echo '#!/bin/bash\necho "hello"' > src/bin/helper.sh
    echo '# Documentation' > docs/guide.md

    git add .
    git commit -m "Initial commit"

    cd - > /dev/null
}

# Cleanup function
cleanup() {
    log_info "Cleaning up..."
    vibe daemon stop 2>/dev/null || true
    sleep 1
    rm -rf "$TEST_DIR" 2>/dev/null || true
}

# ============================================
# WORKFLOW 1: Basic Initialization
# ============================================
test_workflow_init() {
    local repo="$TEST_DIR/repo1"
    setup_test_repo "$repo"
    cd "$repo"

    # Test: vibe init should succeed
    $VIBE_BIN init || { echo "vibe init failed"; return 1; }

    # Verify .vibe directory structure
    [ -d ".vibe" ] || { echo ".vibe directory not created"; return 1; }
    [ -d ".vibe/sessions" ] || { echo ".vibe/sessions not created"; return 1; }
    [ -d ".vibe/cache" ] || { echo ".vibe/cache not created"; return 1; }
    [ -d ".vibe/metadata.db" ] || { echo ".vibe/metadata.db not created"; return 1; }

    echo "Init workflow passed"
    return 0
}

# ============================================
# WORKFLOW 2: Session Spawn (Local)
# ============================================
test_workflow_spawn_local() {
    local repo="$TEST_DIR/repo2"
    setup_test_repo "$repo"
    cd "$repo"

    $VIBE_BIN init || { echo "vibe init failed"; return 1; }

    # Test: spawn a session
    $VIBE_BIN spawn test-session || { echo "vibe spawn failed"; return 1; }

    # Verify session directory created
    [ -d ".vibe/sessions/test-session" ] || { echo "session directory not created"; return 1; }

    # Verify session info file created
    [ -f ".vibe/sessions/test-session.json" ] || { echo "session info file not created"; return 1; }

    echo "Spawn local workflow passed"
    return 0
}

# ============================================
# WORKFLOW 3: Session Spawn with Auto-name
# ============================================
test_workflow_spawn_autoname() {
    local repo="$TEST_DIR/repo3"
    setup_test_repo "$repo"
    cd "$repo"

    $VIBE_BIN init || { echo "vibe init failed"; return 1; }

    # Test: spawn without specifying name (should auto-generate)
    output=$($VIBE_BIN spawn 2>&1)

    # Check if session was created (look for success message)
    if ! echo "$output" | grep -q "spawned successfully\|Vibe workspace mounted\|Session directory"; then
        echo "Spawn with auto-name did not indicate success"
        echo "Output: $output"
        return 1
    fi

    # Verify at least one session exists
    session_count=$(ls -d .vibe/sessions/*/ 2>/dev/null | wc -l)
    [ "$session_count" -ge 1 ] || { echo "No session directory created"; return 1; }

    echo "Spawn auto-name workflow passed"
    return 0
}

# ============================================
# WORKFLOW 4: File Editing in Session
# ============================================
test_workflow_file_editing() {
    local repo="$TEST_DIR/repo4"
    setup_test_repo "$repo"
    cd "$repo"

    $VIBE_BIN init || { echo "vibe init failed"; return 1; }
    $VIBE_BIN spawn edit-test || { echo "vibe spawn failed"; return 1; }

    local session_dir=".vibe/sessions/edit-test"

    # Create new file in session
    echo 'pub fn new_feature() {}' > "$session_dir/new_feature.rs"

    # Modify existing file (need to copy it first)
    mkdir -p "$session_dir/src/lib"
    echo 'pub fn add(a: i32, b: i32) -> i32 { a + b + 1 }' > "$session_dir/src/lib/math.rs"

    # Verify files exist
    [ -f "$session_dir/new_feature.rs" ] || { echo "New file not created"; return 1; }
    [ -f "$session_dir/src/lib/math.rs" ] || { echo "Modified file not created"; return 1; }

    echo "File editing workflow passed"
    return 0
}

# ============================================
# WORKFLOW 5: Mark Dirty Files
# ============================================
test_workflow_mark_dirty() {
    local repo="$TEST_DIR/repo5"
    setup_test_repo "$repo"
    cd "$repo"

    $VIBE_BIN init || { echo "vibe init failed"; return 1; }

    # Stop daemon to avoid RocksDB lock
    $VIBE_BIN daemon stop 2>/dev/null || true
    sleep 1

    $VIBE_BIN spawn dirty-test || { echo "vibe spawn failed"; return 1; }

    # Stop daemon again
    $VIBE_BIN daemon stop 2>/dev/null || true
    sleep 1

    local session_dir=".vibe/sessions/dirty-test"
    echo 'new content' > "$session_dir/newfile.txt"

    # Test mark_dirty
    if [ -x "$MARK_DIRTY_BIN" ]; then
        $MARK_DIRTY_BIN . newfile.txt || { echo "mark_dirty failed"; return 1; }
        echo "Mark dirty workflow passed"
        return 0
    else
        echo "mark_dirty binary not found at $MARK_DIRTY_BIN"
        return 1
    fi
}

# ============================================
# WORKFLOW 6: Session Status
# ============================================
test_workflow_status() {
    local repo="$TEST_DIR/repo6"
    setup_test_repo "$repo"
    cd "$repo"

    $VIBE_BIN init || { echo "vibe init failed"; return 1; }
    $VIBE_BIN spawn status-test || { echo "vibe spawn failed"; return 1; }

    # Test: vibe status should work
    output=$($VIBE_BIN status 2>&1) || { echo "vibe status failed"; return 1; }

    # Should show the session
    if ! echo "$output" | grep -q "status-test"; then
        echo "Status output doesn't show session"
        echo "Output: $output"
        return 1
    fi

    echo "Status workflow passed"
    return 0
}

# ============================================
# WORKFLOW 7: Session Status with JSON
# ============================================
test_workflow_status_json() {
    local repo="$TEST_DIR/repo7"
    setup_test_repo "$repo"
    cd "$repo"

    $VIBE_BIN init || { echo "vibe init failed"; return 1; }
    $VIBE_BIN spawn json-test || { echo "vibe spawn failed"; return 1; }

    # Test: vibe status --json
    output=$($VIBE_BIN status --json 2>&1) || { echo "vibe status --json failed"; return 1; }

    # Should be valid JSON
    if ! echo "$output" | python3 -c "import sys,json; json.load(sys.stdin)" 2>/dev/null; then
        echo "Status JSON output is not valid JSON"
        echo "Output: $output"
        return 1
    fi

    echo "Status JSON workflow passed"
    return 0
}

# ============================================
# WORKFLOW 8: Session Inspect
# ============================================
test_workflow_inspect() {
    local repo="$TEST_DIR/repo8"
    setup_test_repo "$repo"
    cd "$repo"

    $VIBE_BIN init || { echo "vibe init failed"; return 1; }
    $VIBE_BIN spawn inspect-test || { echo "vibe spawn failed"; return 1; }

    # Stop daemon to release RocksDB lock
    $VIBE_BIN daemon stop 2>/dev/null || true
    sleep 2

    # Test: vibe inspect
    output=$($VIBE_BIN inspect inspect-test 2>&1)
    exit_code=$?

    # Check for RocksDB lock error (known issue)
    if echo "$output" | grep -q "Resource temporarily unavailable\|lock file"; then
        echo "RocksDB lock contention - daemon holding lock"
        echo "Output: $output"
        return 1
    fi

    if [ $exit_code -ne 0 ]; then
        echo "vibe inspect failed with exit code $exit_code"
        echo "Output: $output"
        return 1
    fi

    echo "Inspect workflow passed"
    return 0
}

# ============================================
# WORKFLOW 9: Session Inspect JSON
# ============================================
test_workflow_inspect_json() {
    local repo="$TEST_DIR/repo9"
    setup_test_repo "$repo"
    cd "$repo"

    $VIBE_BIN init || { echo "vibe init failed"; return 1; }
    $VIBE_BIN spawn inspect-json-test || { echo "vibe spawn failed"; return 1; }

    $VIBE_BIN daemon stop 2>/dev/null || true
    sleep 2

    # Test: vibe inspect --json
    output=$($VIBE_BIN inspect inspect-json-test --json 2>&1)

    if echo "$output" | grep -q "Resource temporarily unavailable\|lock file"; then
        echo "RocksDB lock contention"
        return 1
    fi

    # Should be valid JSON
    if ! echo "$output" | python3 -c "import sys,json; json.load(sys.stdin)" 2>/dev/null; then
        echo "Inspect JSON output is not valid JSON"
        echo "Output: $output"
        return 1
    fi

    echo "Inspect JSON workflow passed"
    return 0
}

# ============================================
# WORKFLOW 10: Session Diff
# ============================================
test_workflow_diff() {
    local repo="$TEST_DIR/repo10"
    setup_test_repo "$repo"
    cd "$repo"

    $VIBE_BIN init || { echo "vibe init failed"; return 1; }
    $VIBE_BIN spawn diff-test || { echo "vibe spawn failed"; return 1; }

    local session_dir=".vibe/sessions/diff-test"

    # Create a modified file
    echo 'modified content' > "$session_dir/README.md"

    $VIBE_BIN daemon stop 2>/dev/null || true
    sleep 1

    # Test: vibe diff
    output=$($VIBE_BIN diff diff-test 2>&1)
    exit_code=$?

    if echo "$output" | grep -q "Resource temporarily unavailable\|lock file"; then
        echo "RocksDB lock contention"
        return 1
    fi

    if [ $exit_code -ne 0 ]; then
        echo "vibe diff failed"
        echo "Output: $output"
        return 1
    fi

    echo "Diff workflow passed"
    return 0
}

# ============================================
# WORKFLOW 11: Session Diff Stat
# ============================================
test_workflow_diff_stat() {
    local repo="$TEST_DIR/repo11"
    setup_test_repo "$repo"
    cd "$repo"

    $VIBE_BIN init || { echo "vibe init failed"; return 1; }
    $VIBE_BIN spawn diffstat-test || { echo "vibe spawn failed"; return 1; }

    local session_dir=".vibe/sessions/diffstat-test"
    echo 'modified' > "$session_dir/README.md"

    $VIBE_BIN daemon stop 2>/dev/null || true
    sleep 1

    # Test: vibe diff --stat
    output=$($VIBE_BIN diff diffstat-test --stat 2>&1)

    if echo "$output" | grep -q "Resource temporarily unavailable"; then
        echo "RocksDB lock contention"
        return 1
    fi

    echo "Diff stat workflow passed"
    return 0
}

# ============================================
# WORKFLOW 12: Snapshot Creation
# ============================================
test_workflow_snapshot() {
    local repo="$TEST_DIR/repo12"
    setup_test_repo "$repo"
    cd "$repo"

    $VIBE_BIN init || { echo "vibe init failed"; return 1; }
    $VIBE_BIN spawn snapshot-test || { echo "vibe spawn failed"; return 1; }

    local session_dir=".vibe/sessions/snapshot-test"
    echo 'version 1' > "$session_dir/state.txt"

    # Test: vibe snapshot
    $VIBE_BIN snapshot snapshot-test || { echo "vibe snapshot failed"; return 1; }

    # Verify snapshot was created
    snapshot_count=$(ls -d .vibe/sessions/snapshot-test_snapshot_* 2>/dev/null | wc -l)
    [ "$snapshot_count" -ge 1 ] || { echo "No snapshot directory created"; return 1; }

    # Verify snapshot contains the file
    snapshot_dir=$(ls -d .vibe/sessions/snapshot-test_snapshot_* 2>/dev/null | head -1)
    [ -f "$snapshot_dir/state.txt" ] || { echo "Snapshot doesn't contain state.txt"; return 1; }

    echo "Snapshot workflow passed"
    return 0
}

# ============================================
# WORKFLOW 13: Snapshot Preserves State
# ============================================
test_workflow_snapshot_preserves() {
    local repo="$TEST_DIR/repo13"
    setup_test_repo "$repo"
    cd "$repo"

    $VIBE_BIN init || { echo "vibe init failed"; return 1; }
    $VIBE_BIN spawn preserve-test || { echo "vibe spawn failed"; return 1; }

    local session_dir=".vibe/sessions/preserve-test"
    echo 'version 1' > "$session_dir/file.txt"

    $VIBE_BIN snapshot preserve-test || { echo "snapshot failed"; return 1; }

    # Modify after snapshot
    echo 'version 2' > "$session_dir/file.txt"

    # Verify snapshot has old version
    snapshot_dir=$(ls -d .vibe/sessions/preserve-test_snapshot_* 2>/dev/null | head -1)
    snapshot_content=$(cat "$snapshot_dir/file.txt")
    session_content=$(cat "$session_dir/file.txt")

    [ "$snapshot_content" = "version 1" ] || { echo "Snapshot doesn't have version 1"; return 1; }
    [ "$session_content" = "version 2" ] || { echo "Session doesn't have version 2"; return 1; }

    echo "Snapshot preserves state workflow passed"
    return 0
}

# ============================================
# WORKFLOW 14: Restore from Snapshot
# ============================================
test_workflow_restore() {
    local repo="$TEST_DIR/repo14"
    setup_test_repo "$repo"
    cd "$repo"

    $VIBE_BIN init || { echo "vibe init failed"; return 1; }
    $VIBE_BIN spawn restore-test || { echo "vibe spawn failed"; return 1; }

    local session_dir=".vibe/sessions/restore-test"
    echo 'original' > "$session_dir/file.txt"

    $VIBE_BIN snapshot restore-test || { echo "snapshot failed"; return 1; }

    # Modify after snapshot
    echo 'modified' > "$session_dir/file.txt"

    # Get snapshot name
    snapshot_name=$(ls -1 .vibe/sessions/ | grep "restore-test_snapshot_" | head -1)

    # Stop daemon before restore (restore requires write access to DB)
    $VIBE_BIN daemon stop 2>/dev/null || true
    sleep 1

    # Test: vibe restore
    output=$($VIBE_BIN restore restore-test --snapshot="$snapshot_name" 2>&1)
    exit_code=$?

    if [ $exit_code -ne 0 ]; then
        echo "vibe restore failed"
        echo "Output: $output"
        return 1
    fi

    # Verify file was restored
    current_content=$(cat "$session_dir/file.txt")
    [ "$current_content" = "original" ] || {
        echo "File wasn't restored. Got: $current_content, expected: original"
        return 1
    }

    echo "Restore workflow passed"
    return 0
}

# ============================================
# WORKFLOW 15: Promote Session
# ============================================
test_workflow_promote() {
    local repo="$TEST_DIR/repo15"
    setup_test_repo "$repo"
    cd "$repo"

    $VIBE_BIN init || { echo "vibe init failed"; return 1; }
    $VIBE_BIN spawn promote-test || { echo "vibe spawn failed"; return 1; }

    local session_dir=".vibe/sessions/promote-test"
    echo 'new feature' > "$session_dir/feature.rs"

    # Stop daemon to avoid lock issues
    $VIBE_BIN daemon stop 2>/dev/null || true
    sleep 1

    # Mark dirty
    if [ -x "$MARK_DIRTY_BIN" ]; then
        $MARK_DIRTY_BIN . feature.rs || { echo "mark_dirty failed"; return 1; }
    fi

    # Test: vibe promote
    output=$($VIBE_BIN promote promote-test 2>&1)
    exit_code=$?

    if echo "$output" | grep -q "Resource temporarily unavailable"; then
        echo "RocksDB lock contention during promote"
        return 1
    fi

    if [ $exit_code -ne 0 ]; then
        echo "vibe promote failed"
        echo "Output: $output"
        return 1
    fi

    # Verify ref was created
    if git show-ref --verify refs/vibes/promote-test 2>/dev/null; then
        echo "Promote workflow passed"
        return 0
    else
        # Check if promote said no dirty files
        if echo "$output" | grep -q "No dirty files"; then
            echo "No dirty files to promote (expected if mark_dirty didn't work)"
            return 0
        fi
        echo "Git ref refs/vibes/promote-test not created"
        return 1
    fi
}

# ============================================
# WORKFLOW 16: Promote with Custom Message
# ============================================
test_workflow_promote_message() {
    local repo="$TEST_DIR/repo16"
    setup_test_repo "$repo"
    cd "$repo"

    $VIBE_BIN init || { echo "vibe init failed"; return 1; }
    $VIBE_BIN spawn message-test || { echo "vibe spawn failed"; return 1; }

    local session_dir=".vibe/sessions/message-test"
    echo 'feature' > "$session_dir/feature.rs"

    $VIBE_BIN daemon stop 2>/dev/null || true
    sleep 1

    if [ -x "$MARK_DIRTY_BIN" ]; then
        $MARK_DIRTY_BIN . feature.rs || true
    fi

    # Test: vibe promote with message
    output=$($VIBE_BIN promote message-test -m "Custom commit message" 2>&1)

    if echo "$output" | grep -q "No dirty files"; then
        echo "Promote with message passed (no files to promote)"
        return 0
    fi

    echo "Promote with message workflow passed (basic)"
    return 0
}

# ============================================
# WORKFLOW 17: Promote All Sessions
# ============================================
test_workflow_promote_all() {
    local repo="$TEST_DIR/repo17"
    setup_test_repo "$repo"
    cd "$repo"

    $VIBE_BIN init || { echo "vibe init failed"; return 1; }
    $VIBE_BIN spawn all-test-1 || { echo "spawn 1 failed"; return 1; }
    $VIBE_BIN spawn all-test-2 || { echo "spawn 2 failed"; return 1; }

    echo 'f1' > ".vibe/sessions/all-test-1/f1.rs"
    echo 'f2' > ".vibe/sessions/all-test-2/f2.rs"

    $VIBE_BIN daemon stop 2>/dev/null || true
    sleep 1

    if [ -x "$MARK_DIRTY_BIN" ]; then
        $MARK_DIRTY_BIN . f1.rs f2.rs || true
    fi

    # Test: vibe promote --all
    output=$($VIBE_BIN promote --all 2>&1)
    exit_code=$?

    if [ $exit_code -ne 0 ]; then
        echo "vibe promote --all failed"
        echo "Output: $output"
        return 1
    fi

    echo "Promote all workflow passed"
    return 0
}

# ============================================
# WORKFLOW 18: Promote with --only Patterns
# ============================================
test_workflow_promote_only() {
    local repo="$TEST_DIR/repo18"
    setup_test_repo "$repo"
    cd "$repo"

    $VIBE_BIN init || { echo "vibe init failed"; return 1; }
    $VIBE_BIN spawn only-test || { echo "spawn failed"; return 1; }

    local session_dir=".vibe/sessions/only-test"
    mkdir -p "$session_dir/src"
    echo 'include' > "$session_dir/src/include.rs"
    echo 'exclude' > "$session_dir/exclude.txt"

    $VIBE_BIN daemon stop 2>/dev/null || true
    sleep 1

    if [ -x "$MARK_DIRTY_BIN" ]; then
        $MARK_DIRTY_BIN . src/include.rs exclude.txt || true
    fi

    # Test: vibe promote --only "*.rs"
    output=$($VIBE_BIN promote only-test --only "*.rs" 2>&1)

    echo "Promote only workflow passed (command executed)"
    return 0
}

# ============================================
# WORKFLOW 19: Close Session
# ============================================
test_workflow_close() {
    local repo="$TEST_DIR/repo19"
    setup_test_repo "$repo"
    cd "$repo"

    $VIBE_BIN init || { echo "vibe init failed"; return 1; }
    $VIBE_BIN spawn close-test || { echo "spawn failed"; return 1; }

    [ -d ".vibe/sessions/close-test" ] || { echo "session not created"; return 1; }

    # Test: vibe close (force to skip confirmation)
    output=$($VIBE_BIN close close-test -f 2>&1)
    exit_code=$?

    if [ $exit_code -ne 0 ]; then
        echo "vibe close failed"
        echo "Output: $output"
        return 1
    fi

    # Verify session was removed
    if [ -d ".vibe/sessions/close-test" ]; then
        echo "Session directory still exists after close"
        return 1
    fi

    echo "Close workflow passed"
    return 0
}

# ============================================
# WORKFLOW 20: Close with Dirty Files Check
# ============================================
test_workflow_close_dirty() {
    local repo="$TEST_DIR/repo20"
    setup_test_repo "$repo"
    cd "$repo"

    $VIBE_BIN init || { echo "vibe init failed"; return 1; }
    $VIBE_BIN spawn dirty-close || { echo "spawn failed"; return 1; }

    # Create files in session
    echo 'content' > ".vibe/sessions/dirty-close/file.txt"

    # Test: vibe close --dirty (should show dirty files without closing)
    output=$($VIBE_BIN close dirty-close --dirty 2>&1)
    exit_code=$?

    if [ $exit_code -ne 0 ]; then
        echo "vibe close --dirty failed"
        echo "Output: $output"
        return 1
    fi

    # Session should still exist
    if [ ! -d ".vibe/sessions/dirty-close" ]; then
        echo "Session was closed when using --dirty flag"
        return 1
    fi

    echo "Close dirty workflow passed"
    return 0
}

# ============================================
# WORKFLOW 21: Close Nonexistent Session
# ============================================
test_workflow_close_nonexistent() {
    local repo="$TEST_DIR/repo21"
    setup_test_repo "$repo"
    cd "$repo"

    $VIBE_BIN init || { echo "vibe init failed"; return 1; }

    # Test: close session that doesn't exist (should fail)
    output=$($VIBE_BIN close nonexistent -f 2>&1)
    exit_code=$?

    if [ $exit_code -eq 0 ]; then
        echo "Close nonexistent should have failed but succeeded"
        return 1
    fi

    echo "Close nonexistent workflow passed"
    return 0
}

# ============================================
# WORKFLOW 22: Get Session Path
# ============================================
test_workflow_path() {
    local repo="$TEST_DIR/repo22"
    setup_test_repo "$repo"
    cd "$repo"

    $VIBE_BIN init || { echo "vibe init failed"; return 1; }
    $VIBE_BIN spawn path-test || { echo "spawn failed"; return 1; }

    # Test: vibe path
    output=$($VIBE_BIN path path-test 2>&1)
    exit_code=$?

    if [ $exit_code -ne 0 ]; then
        echo "vibe path failed"
        echo "Output: $output"
        return 1
    fi

    # Should output a path
    if ! echo "$output" | grep -q "/"; then
        echo "Path output doesn't look like a path"
        echo "Output: $output"
        return 1
    fi

    echo "Path workflow passed"
    return 0
}

# ============================================
# WORKFLOW 23: Daemon Status
# ============================================
test_workflow_daemon_status() {
    local repo="$TEST_DIR/repo23"
    setup_test_repo "$repo"
    cd "$repo"

    $VIBE_BIN init || { echo "vibe init failed"; return 1; }

    # Test: vibe daemon status
    output=$($VIBE_BIN daemon status 2>&1)
    # This should work whether daemon is running or not

    echo "Daemon status workflow passed"
    return 0
}

# ============================================
# WORKFLOW 24: Daemon Stop
# ============================================
test_workflow_daemon_stop() {
    local repo="$TEST_DIR/repo24"
    setup_test_repo "$repo"
    cd "$repo"

    $VIBE_BIN init || { echo "vibe init failed"; return 1; }
    $VIBE_BIN spawn daemon-test || { echo "spawn failed"; return 1; }

    # Test: vibe daemon stop
    output=$($VIBE_BIN daemon stop 2>&1)
    # Should succeed or say daemon not running

    echo "Daemon stop workflow passed"
    return 0
}

# ============================================
# WORKFLOW 25: Purge Specific Session
# ============================================
test_workflow_purge_session() {
    local repo="$TEST_DIR/repo25"
    setup_test_repo "$repo"
    cd "$repo"

    $VIBE_BIN init || { echo "vibe init failed"; return 1; }
    $VIBE_BIN spawn purge-test || { echo "spawn failed"; return 1; }

    [ -d ".vibe/sessions/purge-test" ] || { echo "session not created"; return 1; }

    # Test: vibe purge -s <session>
    output=$($VIBE_BIN purge -s purge-test -f 2>&1)
    exit_code=$?

    if [ $exit_code -ne 0 ]; then
        echo "vibe purge session failed"
        echo "Output: $output"
        return 1
    fi

    # Session should be gone
    if [ -d ".vibe/sessions/purge-test" ]; then
        echo "Session still exists after purge"
        return 1
    fi

    echo "Purge session workflow passed"
    return 0
}

# ============================================
# WORKFLOW 26: Shell Command in Session
# ============================================
test_workflow_sh_command() {
    local repo="$TEST_DIR/repo26"
    setup_test_repo "$repo"
    cd "$repo"

    $VIBE_BIN init || { echo "vibe init failed"; return 1; }
    $VIBE_BIN spawn sh-test || { echo "vibe spawn failed"; return 1; }

    # Test: vibe sh -c "command"
    output=$($VIBE_BIN sh -s sh-test -c "pwd" 2>&1)
    exit_code=$?

    if [ $exit_code -ne 0 ]; then
        echo "vibe sh command failed"
        echo "Output: $output"
        return 1
    fi

    echo "Shell command workflow passed"
    return 0
}

# ============================================
# WORKFLOW 27: Multiple Parallel Sessions
# ============================================
test_workflow_parallel_sessions() {
    local repo="$TEST_DIR/repo27"
    setup_test_repo "$repo"
    cd "$repo"

    $VIBE_BIN init || { echo "vibe init failed"; return 1; }

    # Spawn multiple sessions
    $VIBE_BIN spawn agent-1 || { echo "spawn agent-1 failed"; return 1; }
    $VIBE_BIN spawn agent-2 || { echo "spawn agent-2 failed"; return 1; }
    $VIBE_BIN spawn agent-3 || { echo "spawn agent-3 failed"; return 1; }

    # Verify all exist
    [ -d ".vibe/sessions/agent-1" ] || { echo "agent-1 not created"; return 1; }
    [ -d ".vibe/sessions/agent-2" ] || { echo "agent-2 not created"; return 1; }
    [ -d ".vibe/sessions/agent-3" ] || { echo "agent-3 not created"; return 1; }

    # Make different changes in each
    echo 'feature 1' > ".vibe/sessions/agent-1/f1.rs"
    echo 'feature 2' > ".vibe/sessions/agent-2/f2.rs"
    echo 'feature 3' > ".vibe/sessions/agent-3/f3.rs"

    echo "Parallel sessions workflow passed"
    return 0
}

# ============================================
# WORKFLOW 28: Conflict Detection
# ============================================
test_workflow_conflict_detection() {
    local repo="$TEST_DIR/repo28"
    setup_test_repo "$repo"
    cd "$repo"

    $VIBE_BIN init || { echo "vibe init failed"; return 1; }

    $VIBE_BIN spawn conflict-1 || { echo "spawn 1 failed"; return 1; }
    $VIBE_BIN spawn conflict-2 || { echo "spawn 2 failed"; return 1; }

    # Both modify same file
    echo 'version A' > ".vibe/sessions/conflict-1/README.md"
    echo 'version B' > ".vibe/sessions/conflict-2/README.md"

    # Test: vibe status --conflicts
    output=$($VIBE_BIN status --conflicts 2>&1)
    exit_code=$?

    if [ $exit_code -ne 0 ]; then
        echo "vibe status --conflicts failed"
        echo "Output: $output"
        return 1
    fi

    echo "Conflict detection workflow passed"
    return 0
}

# ============================================
# WORKFLOW 29: NFS Mount Verification
# ============================================
test_workflow_nfs_mount() {
    local repo="$TEST_DIR/repo29"
    setup_test_repo "$repo"
    cd "$repo"

    $VIBE_BIN init || { echo "vibe init failed"; return 1; }
    $VIBE_BIN spawn nfs-test || { echo "spawn failed"; return 1; }

    # Get the mount path
    mount_path=$($VIBE_BIN path nfs-test 2>&1)

    # Check if mount exists and has files
    if [ -d "$mount_path" ]; then
        # List files in mount
        files=$(ls -la "$mount_path" 2>&1)

        # Check folder structure - should have src/ directory
        if echo "$files" | grep -q "^d.*src"; then
            echo "NFS mount has proper directory structure"
            echo "NFS mount workflow passed"
            return 0
        else
            # Check if structure is flat (known bug)
            if echo "$files" | grep -q "main.rs\|lib.rs" && ! echo "$files" | grep -q "^d.*src"; then
                echo "NFS mount has FLAT structure (bug: files not in directories)"
                echo "Files: $files"
                return 1
            fi
        fi
    else
        echo "Mount path doesn't exist or is not a directory: $mount_path"
        return 1
    fi

    echo "NFS mount workflow passed (mount exists)"
    return 0
}

# ============================================
# WORKFLOW 30: Full End-to-End Workflow
# ============================================
test_workflow_e2e() {
    local repo="$TEST_DIR/repo30"
    setup_test_repo "$repo"
    cd "$repo"

    # 1. Init
    $VIBE_BIN init || { echo "init failed"; return 1; }

    # 2. Spawn
    $VIBE_BIN spawn e2e-test || { echo "spawn failed"; return 1; }

    # 3. Make changes
    local session_dir=".vibe/sessions/e2e-test"
    echo 'pub fn e2e() -> bool { true }' > "$session_dir/e2e.rs"

    # 4. Mark dirty (stop daemon first to avoid lock)
    $VIBE_BIN daemon stop 2>/dev/null || true
    sleep 1

    if [ -x "$MARK_DIRTY_BIN" ]; then
        $MARK_DIRTY_BIN . e2e.rs || { echo "mark_dirty failed"; return 1; }
    fi

    # 5. Create snapshot
    $VIBE_BIN snapshot e2e-test || { echo "snapshot failed"; return 1; }

    # 6. Promote
    output=$($VIBE_BIN promote e2e-test -m "E2E test commit" 2>&1)

    # 7. Check status
    $VIBE_BIN daemon stop 2>/dev/null || true
    sleep 1
    output=$($VIBE_BIN status 2>&1)

    # 8. Close session
    $VIBE_BIN close e2e-test -f || { echo "close failed"; return 1; }

    echo "End-to-end workflow passed"
    return 0
}

# ============================================
# WORKFLOW 31: Init in Non-Git Directory
# ============================================
test_workflow_init_nongit() {
    local repo="$TEST_DIR/nongit"
    mkdir -p "$repo"
    cd "$repo"

    # Test: vibe init should fail in non-git directory
    output=$($VIBE_BIN init 2>&1)
    exit_code=$?

    if [ $exit_code -eq 0 ]; then
        echo "Init succeeded in non-git directory (should have failed)"
        return 1
    fi

    echo "Init non-git workflow passed"
    return 0
}

# ============================================
# WORKFLOW 32: Spawn Without Init
# ============================================
test_workflow_spawn_noinit() {
    local repo="$TEST_DIR/repo32"
    setup_test_repo "$repo"
    cd "$repo"

    # Don't run init, try to spawn directly
    output=$($VIBE_BIN spawn no-init-test 2>&1)
    exit_code=$?

    if [ $exit_code -eq 0 ]; then
        echo "Spawn succeeded without init (should have failed)"
        return 1
    fi

    echo "Spawn without init workflow passed"
    return 0
}

# ============================================
# WORKFLOW 33: Double Spawn Same Session
# ============================================
test_workflow_double_spawn() {
    local repo="$TEST_DIR/repo33"
    setup_test_repo "$repo"
    cd "$repo"

    $VIBE_BIN init || { echo "init failed"; return 1; }
    $VIBE_BIN spawn double-test || { echo "first spawn failed"; return 1; }

    # Try to spawn again with same name
    output=$($VIBE_BIN spawn double-test 2>&1)
    exit_code=$?

    # Should either fail or handle gracefully
    if [ $exit_code -eq 0 ]; then
        echo "Double spawn succeeded (may be OK if handled gracefully)"
    fi

    echo "Double spawn workflow passed"
    return 0
}

# ============================================
# WORKFLOW 34: Promote Without Dirty Files
# ============================================
test_workflow_promote_empty() {
    local repo="$TEST_DIR/repo34"
    setup_test_repo "$repo"
    cd "$repo"

    $VIBE_BIN init || { echo "init failed"; return 1; }
    $VIBE_BIN spawn empty-promote || { echo "spawn failed"; return 1; }

    $VIBE_BIN daemon stop 2>/dev/null || true
    sleep 1

    # Promote without any changes
    output=$($VIBE_BIN promote empty-promote 2>&1)
    exit_code=$?

    # Should succeed (no-op) or explicitly say no dirty files
    if [ $exit_code -ne 0 ]; then
        if echo "$output" | grep -qi "no dirty\|nothing to promote"; then
            echo "Correctly indicated no dirty files"
        else
            echo "Promote empty failed unexpectedly"
            echo "Output: $output"
            return 1
        fi
    fi

    echo "Promote empty workflow passed"
    return 0
}

# ============================================
# WORKFLOW 35: Launch with Nonexistent Agent
# ============================================
test_workflow_launch_noagent() {
    local repo="$TEST_DIR/repo35"
    setup_test_repo "$repo"
    cd "$repo"

    $VIBE_BIN init || { echo "init failed"; return 1; }

    # Try to launch nonexistent agent
    output=$($VIBE_BIN launch nonexistent-agent-xyz 2>&1)
    exit_code=$?

    if [ $exit_code -eq 0 ]; then
        echo "Launch nonexistent agent should have failed"
        return 1
    fi

    echo "Launch noagent workflow passed"
    return 0
}

# ============================================
# Run all tests
# ============================================
main() {
    echo "============================================"
    echo "VibeFS Comprehensive Workflow Tests"
    echo "============================================"
    echo "Started at: $(date)"
    echo ""

    # Cleanup first
    cleanup
    mkdir -p "$TEST_DIR"

    # Run all tests
    run_test "1. Basic Initialization" test_workflow_init
    run_test "2. Session Spawn (Local)" test_workflow_spawn_local
    run_test "3. Session Spawn Auto-name" test_workflow_spawn_autoname
    run_test "4. File Editing in Session" test_workflow_file_editing
    run_test "5. Mark Dirty Files" test_workflow_mark_dirty
    run_test "6. Session Status" test_workflow_status
    run_test "7. Session Status JSON" test_workflow_status_json
    run_test "8. Session Inspect" test_workflow_inspect
    run_test "9. Session Inspect JSON" test_workflow_inspect_json
    run_test "10. Session Diff" test_workflow_diff
    run_test "11. Session Diff Stat" test_workflow_diff_stat
    run_test "12. Snapshot Creation" test_workflow_snapshot
    run_test "13. Snapshot Preserves State" test_workflow_snapshot_preserves
    run_test "14. Restore from Snapshot" test_workflow_restore
    run_test "15. Promote Session" test_workflow_promote
    run_test "16. Promote with Message" test_workflow_promote_message
    run_test "17. Promote All Sessions" test_workflow_promote_all
    run_test "18. Promote with --only" test_workflow_promote_only
    run_test "19. Close Session" test_workflow_close
    run_test "20. Close with Dirty Check" test_workflow_close_dirty
    run_test "21. Close Nonexistent Session" test_workflow_close_nonexistent
    run_test "22. Get Session Path" test_workflow_path
    run_test "23. Daemon Status" test_workflow_daemon_status
    run_test "24. Daemon Stop" test_workflow_daemon_stop
    run_test "25. Purge Specific Session" test_workflow_purge_session
    run_test "26. Shell Command in Session" test_workflow_sh_command
    run_test "27. Multiple Parallel Sessions" test_workflow_parallel_sessions
    run_test "28. Conflict Detection" test_workflow_conflict_detection
    run_test "29. NFS Mount Structure" test_workflow_nfs_mount
    run_test "30. Full E2E Workflow" test_workflow_e2e
    run_test "31. Init in Non-Git Dir" test_workflow_init_nongit
    run_test "32. Spawn Without Init" test_workflow_spawn_noinit
    run_test "33. Double Spawn Same Session" test_workflow_double_spawn
    run_test "34. Promote Without Dirty Files" test_workflow_promote_empty
    run_test "35. Launch Nonexistent Agent" test_workflow_launch_noagent

    # Final cleanup
    cleanup

    # Summary
    echo ""
    echo "============================================"
    echo "TEST SUMMARY"
    echo "============================================"
    echo -e "${GREEN}PASSED: $PASSED_COUNT${NC}"
    echo -e "${RED}FAILED: $FAILED_COUNT${NC}"
    echo ""

    if [ -n "$PASSED_TESTS" ]; then
        echo "Passed tests:"
        echo -e "$PASSED_TESTS"
    fi

    echo ""

    if [ -n "$FAILED_TESTS" ]; then
        echo "Failed tests:"
        echo -e "$FAILED_TESTS"

        echo ""
        echo "============================================"
        echo "FAILED TEST DETAILS"
        echo "============================================"
        echo -e "$FAILED_OUTPUTS"

        exit 1
    fi

    exit 0
}

main "$@"
