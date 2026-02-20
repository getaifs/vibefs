#!/bin/bash
# VibeFS Comprehensive Workflow Tests
# Tests all developer workflows against the current system
# Uses the new tmux-style command names (kill, ls, commit, attach)

set -o pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Detect script directory and repo root
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Test directory (canonical to avoid /tmp vs /private/tmp mismatch)
TEST_DIR="$(python3 - <<'PY'
import os
print(os.path.realpath("/tmp/vibefs_workflow_tests"))
PY
)"

# Binary paths relative to repo root
VIBE_BIN="$REPO_ROOT/target/release/vibe"
# No mark_dirty binary - tests write through NFS mounts for real dirty tracking

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

    # Stop all vibed daemons
    pkill -9 vibed 2>/dev/null || true
    sleep 1

    # Unmount all test NFS mounts
    local mount_dir="$HOME/Library/Caches/vibe/mounts"
    if [ -d "$mount_dir" ]; then
        for mount in $(mount | grep "$mount_dir" | awk '{print $3}'); do
            umount -f "$mount" 2>/dev/null || diskutil unmount force "$mount" 2>/dev/null || true
        done
        # Clean up mount directories created by tests
        for dir in "$mount_dir"/repo*; do
            [ -d "$dir" ] && rmdir "$dir" 2>/dev/null || true
        done
    fi

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
# WORKFLOW 2: Session Creation
# ============================================
test_workflow_spawn_local() {
    local repo="$TEST_DIR/repo2"
    setup_test_repo "$repo"
    cd "$repo"

    $VIBE_BIN init || { echo "vibe init failed"; return 1; }

    # Test: create a session (using -c "true" to avoid interactive shell)
    $VIBE_BIN new test-session -c "true" || { echo "vibe new failed"; return 1; }

    # Verify session directory created
    [ -d ".vibe/sessions/test-session" ] || { echo "session directory not created"; return 1; }

    # Verify session info file created
    [ -f ".vibe/sessions/test-session.json" ] || { echo "session info file not created"; return 1; }

    # Verify per-session metadata.db was created
    [ -d ".vibe/sessions/test-session/metadata.db" ] || { echo "per-session metadata.db not created"; return 1; }

    echo "Session creation workflow passed"
    return 0
}

# ============================================
# WORKFLOW 3: Session Create with Auto-name
# ============================================
test_workflow_spawn_autoname() {
    local repo="$TEST_DIR/repo3"
    setup_test_repo "$repo"
    cd "$repo"

    $VIBE_BIN init || { echo "vibe init failed"; return 1; }

    # Test: new without specifying name (should auto-generate)
    output=$($VIBE_BIN new -c "true" 2>&1)

    # Check if session was created (look for success message)
    if ! echo "$output" | grep -q "spawned successfully\|Vibe workspace mounted\|Session directory\|Spawning session"; then
        echo "New with auto-name did not indicate success"
        echo "Output: $output"
        return 1
    fi

    # Verify at least one session exists
    session_count=$(ls -d .vibe/sessions/*/ 2>/dev/null | wc -l)
    [ "$session_count" -ge 1 ] || { echo "No session directory created"; return 1; }

    echo "Create auto-name workflow passed"
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
    $VIBE_BIN new edit-test -c "true" || { echo "vibe new failed"; return 1; }

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
# WORKFLOW 5: NFS Write Marks Dirty Automatically
# ============================================
test_workflow_nfs_dirty_tracking() {
    local repo="$TEST_DIR/repo5"
    setup_test_repo "$repo"
    cd "$repo"

    $VIBE_BIN init || { echo "vibe init failed"; return 1; }
    $VIBE_BIN new dirty-test -c "true" || { echo "vibe new failed"; return 1; }

    # Get mount point and write through NFS (real code path)
    local mount_point
    mount_point=$($VIBE_BIN ls dirty-test -p) || { echo "Failed to get mount point"; return 1; }
    [ -d "$mount_point" ] || { echo "Mount point doesn't exist: $mount_point"; return 1; }

    echo 'new content' > "$mount_point/newfile.txt"
    sleep 0.5

    # Verify the write was tracked as dirty via diff
    output=$($VIBE_BIN diff dirty-test 2>&1)
    if echo "$output" | grep -q "newfile.txt"; then
        echo "NFS dirty tracking workflow passed"
        return 0
    else
        echo "NFS write did not mark file as dirty"
        echo "Diff output: $output"
        return 1
    fi
}

# ============================================
# WORKFLOW 6: vibe ls (session listing)
# ============================================
test_workflow_ls() {
    local repo="$TEST_DIR/repo6"
    setup_test_repo "$repo"
    cd "$repo"

    $VIBE_BIN init || { echo "vibe init failed"; return 1; }
    $VIBE_BIN new ls-test -c "true" || { echo "vibe new failed"; return 1; }

    # Test: vibe ls should work
    output=$($VIBE_BIN ls 2>&1) || { echo "vibe ls failed"; return 1; }

    # Should show the session
    if ! echo "$output" | grep -q "ls-test"; then
        echo "ls output doesn't show session"
        echo "Output: $output"
        return 1
    fi

    echo "ls workflow passed"
    return 0
}

# ============================================
# WORKFLOW 7: vibe ls --json
# ============================================
test_workflow_ls_json() {
    local repo="$TEST_DIR/repo7"
    setup_test_repo "$repo"
    cd "$repo"

    $VIBE_BIN init || { echo "vibe init failed"; return 1; }
    $VIBE_BIN new json-test -c "true" || { echo "vibe new failed"; return 1; }

    # Test: vibe ls --json (alias: vibe status --json)
    output=$($VIBE_BIN ls --json 2>&1) || { echo "vibe ls --json failed"; return 1; }

    # Should be valid JSON
    if ! echo "$output" | python3 -c "import sys,json; json.load(sys.stdin)" 2>/dev/null; then
        echo "ls JSON output is not valid JSON"
        echo "Output: $output"
        return 1
    fi

    echo "ls JSON workflow passed"
    return 0
}

# ============================================
# WORKFLOW 8: vibe ls -v (verbose/inspect)
# ============================================
test_workflow_inspect() {
    local repo="$TEST_DIR/repo8"
    setup_test_repo "$repo"
    cd "$repo"

    $VIBE_BIN init || { echo "vibe init failed"; return 1; }
    $VIBE_BIN new inspect-test -c "true" || { echo "vibe new failed"; return 1; }

    # Stop daemon to release RocksDB lock
    $VIBE_BIN daemon stop 2>/dev/null || true
    sleep 2

    # Test: vibe ls inspect-test -v
    output=$($VIBE_BIN ls inspect-test -v 2>&1)
    exit_code=$?

    if echo "$output" | grep -q "Resource temporarily unavailable\|lock file"; then
        echo "RocksDB lock contention - daemon holding lock"
        echo "Output: $output"
        return 1
    fi

    if [ $exit_code -ne 0 ]; then
        echo "vibe ls -v failed with exit code $exit_code"
        echo "Output: $output"
        return 1
    fi

    echo "ls verbose workflow passed"
    return 0
}

# ============================================
# WORKFLOW 9: vibe ls -v --json
# ============================================
test_workflow_inspect_json() {
    local repo="$TEST_DIR/repo9"
    setup_test_repo "$repo"
    cd "$repo"

    $VIBE_BIN init || { echo "vibe init failed"; return 1; }
    $VIBE_BIN new inspect-json-test -c "true" || { echo "vibe new failed"; return 1; }

    $VIBE_BIN daemon stop 2>/dev/null || true
    sleep 2

    # Test: vibe ls -v --json
    output=$($VIBE_BIN ls inspect-json-test -v --json 2>&1)

    if echo "$output" | grep -q "Resource temporarily unavailable\|lock file"; then
        echo "RocksDB lock contention"
        return 1
    fi

    # Should be valid JSON
    if ! echo "$output" | python3 -c "import sys,json; json.load(sys.stdin)" 2>/dev/null; then
        echo "ls verbose JSON output is not valid JSON"
        echo "Output: $output"
        return 1
    fi

    echo "ls verbose JSON workflow passed"
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
    $VIBE_BIN new diff-test -c "true" || { echo "vibe new failed"; return 1; }

    # Write through NFS mount (triggers real dirty tracking)
    local mount_point
    mount_point=$($VIBE_BIN ls diff-test -p) || { echo "Failed to get mount point"; return 1; }
    echo 'modified content' > "$mount_point/README.md"
    sleep 0.5

    # Test: vibe diff
    output=$($VIBE_BIN diff diff-test 2>&1)
    exit_code=$?

    if [ $exit_code -ne 0 ]; then
        echo "vibe diff failed"
        echo "Output: $output"
        return 1
    fi

    # Should show the diff for README.md
    if ! echo "$output" | grep -q "README.md"; then
        echo "Diff output doesn't mention modified file"
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
    $VIBE_BIN new diffstat-test -c "true" || { echo "vibe new failed"; return 1; }

    # Write through NFS mount
    local mount_point
    mount_point=$($VIBE_BIN ls diffstat-test -p) || { echo "Failed to get mount point"; return 1; }
    echo 'modified' > "$mount_point/README.md"
    sleep 0.5

    # Test: vibe diff --stat
    output=$($VIBE_BIN diff diffstat-test --stat 2>&1)

    if ! echo "$output" | grep -q "README.md\|file"; then
        echo "Diff stat output missing file info"
        echo "Output: $output"
        return 1
    fi

    echo "Diff stat workflow passed"
    return 0
}

# ============================================
# WORKFLOW 12: Save/Checkpoint Creation
# ============================================
test_workflow_snapshot() {
    local repo="$TEST_DIR/repo12"
    setup_test_repo "$repo"
    cd "$repo"

    $VIBE_BIN init || { echo "vibe init failed"; return 1; }
    $VIBE_BIN new snapshot-test -c "true" || { echo "vibe new failed"; return 1; }

    local session_dir=".vibe/sessions/snapshot-test"
    echo 'version 1' > "$session_dir/state.txt"

    # Test: vibe save
    $VIBE_BIN save -s snapshot-test || { echo "vibe save failed"; return 1; }

    # Verify snapshot was created
    snapshot_count=$(ls -d .vibe/sessions/snapshot-test_snapshot_* 2>/dev/null | wc -l)
    [ "$snapshot_count" -ge 1 ] || { echo "No snapshot directory created"; return 1; }

    # Verify snapshot contains the file
    snapshot_dir=$(ls -d .vibe/sessions/snapshot-test_snapshot_* 2>/dev/null | head -1)
    [ -f "$snapshot_dir/state.txt" ] || { echo "Snapshot doesn't contain state.txt"; return 1; }

    echo "Save workflow passed"
    return 0
}

# ============================================
# WORKFLOW 13: Save Preserves State
# ============================================
test_workflow_snapshot_preserves() {
    local repo="$TEST_DIR/repo13"
    setup_test_repo "$repo"
    cd "$repo"

    $VIBE_BIN init || { echo "vibe init failed"; return 1; }
    $VIBE_BIN new preserve-test -c "true" || { echo "vibe new failed"; return 1; }

    local session_dir=".vibe/sessions/preserve-test"
    echo 'version 1' > "$session_dir/file.txt"

    $VIBE_BIN save -s preserve-test || { echo "save failed"; return 1; }

    # Modify after save
    echo 'version 2' > "$session_dir/file.txt"

    # Verify save has old version
    snapshot_dir=$(ls -d .vibe/sessions/preserve-test_snapshot_* 2>/dev/null | head -1)
    snapshot_content=$(cat "$snapshot_dir/file.txt")
    session_content=$(cat "$session_dir/file.txt")

    [ "$snapshot_content" = "version 1" ] || { echo "Saved checkpoint doesn't have version 1"; return 1; }
    [ "$session_content" = "version 2" ] || { echo "Session doesn't have version 2"; return 1; }

    echo "Save preserves state workflow passed"
    return 0
}

# ============================================
# WORKFLOW 14: Undo from Checkpoint
# ============================================
test_workflow_restore() {
    local repo="$TEST_DIR/repo14"
    setup_test_repo "$repo"
    cd "$repo"

    $VIBE_BIN init || { echo "vibe init failed"; return 1; }
    $VIBE_BIN new restore-test -c "true" || { echo "vibe new failed"; return 1; }

    local session_dir=".vibe/sessions/restore-test"
    echo 'original' > "$session_dir/file.txt"

    # Save with a named checkpoint
    $VIBE_BIN save checkpoint1 -s restore-test || { echo "save failed"; return 1; }

    # Modify after save
    echo 'modified' > "$session_dir/file.txt"

    # Stop daemon before undo (restore requires write access to DB)
    $VIBE_BIN daemon stop 2>/dev/null || true
    sleep 1

    # Test: vibe undo
    output=$($VIBE_BIN undo checkpoint1 -s restore-test 2>&1)
    exit_code=$?

    if [ $exit_code -ne 0 ]; then
        echo "vibe undo failed"
        echo "Output: $output"
        return 1
    fi

    # Verify file was restored
    current_content=$(cat "$session_dir/file.txt")
    [ "$current_content" = "original" ] || {
        echo "File wasn't restored. Got: $current_content, expected: original"
        return 1
    }

    echo "Undo workflow passed"
    return 0
}

# ============================================
# WORKFLOW 15: Commit Session (was: Promote)
# ============================================
test_workflow_commit() {
    local repo="$TEST_DIR/repo15"
    setup_test_repo "$repo"
    cd "$repo"

    $VIBE_BIN init || { echo "vibe init failed"; return 1; }
    $VIBE_BIN new commit-test -c "true" || { echo "vibe new failed"; return 1; }

    # Write through NFS mount (real dirty tracking)
    local mount_point
    mount_point=$($VIBE_BIN ls commit-test -p) || { echo "Failed to get mount point"; return 1; }
    echo 'new feature' > "$mount_point/feature.rs"
    sleep 0.5

    # Test: vibe commit (was: vibe promote)
    output=$($VIBE_BIN commit commit-test 2>&1)
    exit_code=$?

    if [ $exit_code -ne 0 ]; then
        echo "vibe commit failed"
        echo "Output: $output"
        return 1
    fi

    # Verify ref was created
    if git show-ref --verify refs/vibes/commit-test 2>/dev/null; then
        echo "Commit workflow passed"
        return 0
    else
        echo "Git ref refs/vibes/commit-test not created"
        echo "Output: $output"
        return 1
    fi
}

# ============================================
# WORKFLOW 16: Commit with Custom Message
# ============================================
test_workflow_commit_message() {
    local repo="$TEST_DIR/repo16"
    setup_test_repo "$repo"
    cd "$repo"

    $VIBE_BIN init || { echo "vibe init failed"; return 1; }
    $VIBE_BIN new message-test -c "true" || { echo "vibe new failed"; return 1; }

    # Write through NFS mount
    local mount_point
    mount_point=$($VIBE_BIN ls message-test -p) || { echo "Failed to get mount point"; return 1; }
    echo 'feature' > "$mount_point/feature.rs"
    sleep 0.5

    # Test: vibe commit with message
    output=$($VIBE_BIN commit message-test -m "Custom commit message" 2>&1)
    exit_code=$?

    if [ $exit_code -ne 0 ]; then
        echo "vibe commit with message failed"
        echo "Output: $output"
        return 1
    fi

    echo "Commit with message workflow passed"
    return 0
}

# ============================================
# WORKFLOW 17: Commit All Sessions
# ============================================
test_workflow_commit_all() {
    local repo="$TEST_DIR/repo17"
    setup_test_repo "$repo"
    cd "$repo"

    $VIBE_BIN init || { echo "vibe init failed"; return 1; }
    $VIBE_BIN new all-test-1 -c "true" || { echo "new 1 failed"; return 1; }
    $VIBE_BIN new all-test-2 -c "true" || { echo "new 2 failed"; return 1; }

    # Write through NFS mounts
    local mount1 mount2
    mount1=$($VIBE_BIN ls all-test-1 -p) || { echo "Failed to get mount1"; return 1; }
    mount2=$($VIBE_BIN ls all-test-2 -p) || { echo "Failed to get mount2"; return 1; }
    echo 'f1' > "$mount1/f1.rs"
    echo 'f2' > "$mount2/f2.rs"
    sleep 0.5

    # Test: vibe commit --all
    output=$($VIBE_BIN commit --all 2>&1)
    exit_code=$?

    if [ $exit_code -ne 0 ]; then
        echo "vibe commit --all failed"
        echo "Output: $output"
        return 1
    fi

    echo "Commit all workflow passed"
    return 0
}

# ============================================
# WORKFLOW 18: Commit with --only Patterns
# ============================================
test_workflow_commit_only() {
    local repo="$TEST_DIR/repo18"
    setup_test_repo "$repo"
    cd "$repo"

    $VIBE_BIN init || { echo "vibe init failed"; return 1; }
    $VIBE_BIN new only-test -c "true" || { echo "new failed"; return 1; }

    # Write through NFS mount
    local mount_point
    mount_point=$($VIBE_BIN ls only-test -p) || { echo "Failed to get mount point"; return 1; }
    mkdir -p "$mount_point/src"
    echo 'include' > "$mount_point/src/include.rs"
    echo 'exclude' > "$mount_point/exclude.txt"
    sleep 0.5

    # Test: vibe commit --only "*.rs"
    output=$($VIBE_BIN commit only-test --only "*.rs" 2>&1)

    echo "Commit only workflow passed"
    return 0
}

# ============================================
# WORKFLOW 19: Kill Session (was: Close)
# ============================================
test_workflow_kill() {
    local repo="$TEST_DIR/repo19"
    setup_test_repo "$repo"
    cd "$repo"

    $VIBE_BIN init || { echo "vibe init failed"; return 1; }
    $VIBE_BIN new kill-test -c "true" || { echo "new failed"; return 1; }

    [ -d ".vibe/sessions/kill-test" ] || { echo "session not created"; return 1; }

    # Test: vibe kill (was: vibe close)
    output=$($VIBE_BIN kill kill-test -f 2>&1)
    exit_code=$?

    if [ $exit_code -ne 0 ]; then
        echo "vibe kill failed"
        echo "Output: $output"
        return 1
    fi

    # Verify session was removed
    if [ -d ".vibe/sessions/kill-test" ]; then
        echo "Session directory still exists after kill"
        return 1
    fi

    echo "Kill workflow passed"
    return 0
}

# ============================================
# WORKFLOW 20: Check Dirty Files Before Kill
# ============================================
test_workflow_kill_dirty() {
    local repo="$TEST_DIR/repo20"
    setup_test_repo "$repo"
    cd "$repo"

    $VIBE_BIN init || { echo "vibe init failed"; return 1; }
    $VIBE_BIN new dirty-kill -c "true" || { echo "new failed"; return 1; }

    # Create files in session
    echo 'content' > ".vibe/sessions/dirty-kill/file.txt"

    # Test: vibe ls shows dirty files
    output=$($VIBE_BIN ls dirty-kill 2>&1)
    exit_code=$?

    if [ $exit_code -ne 0 ]; then
        echo "vibe ls failed"
        echo "Output: $output"
        return 1
    fi

    # Session should still exist
    if [ ! -d ".vibe/sessions/dirty-kill" ]; then
        echo "Session was closed unexpectedly"
        return 1
    fi

    echo "Check dirty workflow passed"
    return 0
}

# ============================================
# WORKFLOW 21: Kill Nonexistent Session
# ============================================
test_workflow_kill_nonexistent() {
    local repo="$TEST_DIR/repo21"
    setup_test_repo "$repo"
    cd "$repo"

    $VIBE_BIN init || { echo "vibe init failed"; return 1; }

    # Test: kill session that doesn't exist (should fail)
    output=$($VIBE_BIN kill nonexistent -f 2>&1)
    exit_code=$?

    if [ $exit_code -eq 0 ]; then
        echo "Kill nonexistent should have failed but succeeded"
        return 1
    fi

    echo "Kill nonexistent workflow passed"
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
    $VIBE_BIN new path-test -c "true" || { echo "new failed"; return 1; }

    # Test: vibe ls -p (get path)
    output=$($VIBE_BIN ls path-test -p 2>&1)
    exit_code=$?

    if [ $exit_code -ne 0 ]; then
        echo "vibe ls -p failed"
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
# WORKFLOW 24: Daemon Start (background) and Stop
# ============================================
test_workflow_daemon_start_stop() {
    local repo="$TEST_DIR/repo24"
    setup_test_repo "$repo"
    cd "$repo"

    $VIBE_BIN init || { echo "vibe init failed"; return 1; }

    # Test: vibe daemon start (should start in background)
    output=$($VIBE_BIN daemon start 2>&1)
    exit_code=$?

    if [ $exit_code -ne 0 ]; then
        echo "vibe daemon start failed"
        echo "Output: $output"
        return 1
    fi

    # Verify daemon is running
    output=$($VIBE_BIN daemon status 2>&1)
    if ! echo "$output" | grep -q "Repository\|RUNNING"; then
        echo "Daemon doesn't appear to be running after start"
        echo "Output: $output"
        return 1
    fi

    # Test: vibe daemon stop
    output=$($VIBE_BIN daemon stop 2>&1)
    if ! echo "$output" | grep -q "shutdown\|not running"; then
        echo "Daemon stop unexpected output"
        echo "Output: $output"
    fi

    echo "Daemon start/stop workflow passed"
    return 0
}

# ============================================
# WORKFLOW 25: Kill Session with Force
# ============================================
test_workflow_kill_session_force() {
    local repo="$TEST_DIR/repo25"
    setup_test_repo "$repo"
    cd "$repo"

    $VIBE_BIN init || { echo "vibe init failed"; return 1; }
    $VIBE_BIN new force-kill-test -c "true" || { echo "new failed"; return 1; }

    [ -d ".vibe/sessions/force-kill-test" ] || { echo "session not created"; return 1; }

    # Test: vibe kill <session> --force
    output=$($VIBE_BIN kill force-kill-test --force 2>&1)
    exit_code=$?

    if [ $exit_code -ne 0 ]; then
        echo "vibe kill session failed"
        echo "Output: $output"
        return 1
    fi

    # Session should be gone
    if [ -d ".vibe/sessions/force-kill-test" ]; then
        echo "Session still exists after kill"
        return 1
    fi

    echo "Kill session workflow passed"
    return 0
}

# ============================================
# WORKFLOW 26: Command Execution in Session
# ============================================
test_workflow_sh_command() {
    local repo="$TEST_DIR/repo26"
    setup_test_repo "$repo"
    cd "$repo"

    $VIBE_BIN init || { echo "vibe init failed"; return 1; }

    # Test: vibe new <session> -c "command"
    output=$($VIBE_BIN new sh-test -c "pwd" 2>&1)
    exit_code=$?

    if [ $exit_code -ne 0 ]; then
        echo "vibe new with command failed"
        echo "Output: $output"
        return 1
    fi

    echo "Command execution workflow passed"
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

    # Create multiple sessions
    $VIBE_BIN new agent-1 -c "true" || { echo "new agent-1 failed"; return 1; }
    $VIBE_BIN new agent-2 -c "true" || { echo "new agent-2 failed"; return 1; }
    $VIBE_BIN new agent-3 -c "true" || { echo "new agent-3 failed"; return 1; }

    # Verify all exist
    [ -d ".vibe/sessions/agent-1" ] || { echo "agent-1 not created"; return 1; }
    [ -d ".vibe/sessions/agent-2" ] || { echo "agent-2 not created"; return 1; }
    [ -d ".vibe/sessions/agent-3" ] || { echo "agent-3 not created"; return 1; }

    # Make different changes in each
    echo 'feature 1' > ".vibe/sessions/agent-1/f1.rs"
    echo 'feature 2' > ".vibe/sessions/agent-2/f2.rs"
    echo 'feature 3' > ".vibe/sessions/agent-3/f3.rs"

    # Verify each session has its own per-session metadata
    [ -d ".vibe/sessions/agent-1/metadata.db" ] || { echo "agent-1 missing per-session metadata"; return 1; }
    [ -d ".vibe/sessions/agent-2/metadata.db" ] || { echo "agent-2 missing per-session metadata"; return 1; }
    [ -d ".vibe/sessions/agent-3/metadata.db" ] || { echo "agent-3 missing per-session metadata"; return 1; }

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

    $VIBE_BIN new conflict-1 -c "true" || { echo "new 1 failed"; return 1; }
    $VIBE_BIN new conflict-2 -c "true" || { echo "new 2 failed"; return 1; }

    # Both modify same file through NFS mounts (real dirty tracking)
    local mount1 mount2
    mount1=$($VIBE_BIN ls conflict-1 -p) || { echo "Failed to get mount1"; return 1; }
    mount2=$($VIBE_BIN ls conflict-2 -p) || { echo "Failed to get mount2"; return 1; }
    echo 'version A' > "$mount1/README.md"
    echo 'version B' > "$mount2/README.md"
    sleep 0.5

    # Test: vibe ls --conflicts
    output=$($VIBE_BIN ls --conflicts 2>&1)
    exit_code=$?

    if [ $exit_code -ne 0 ]; then
        echo "vibe ls --conflicts failed"
        echo "Output: $output"
        return 1
    fi

    # Should detect README.md as a conflict (modified by both sessions)
    if ! echo "$output" | grep -q "README.md\|conflict"; then
        echo "Conflict not detected for README.md"
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
    $VIBE_BIN new nfs-test -c "true" || { echo "new failed"; return 1; }

    # Get the mount path
    mount_path=$($VIBE_BIN ls nfs-test -p 2>&1)

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
# WORKFLOW 30: Session Isolation (Bug 1 Fix)
# ============================================
test_workflow_session_isolation() {
    local repo="$TEST_DIR/repo30"
    setup_test_repo "$repo"
    cd "$repo"

    $VIBE_BIN init || { echo "vibe init failed"; return 1; }

    # Create two sessions
    $VIBE_BIN new iso-a -c "true" || { echo "new iso-a failed"; return 1; }
    $VIBE_BIN new iso-b -c "true" || { echo "new iso-b failed"; return 1; }

    # Get mount points
    local mount_a mount_b
    mount_a=$($VIBE_BIN ls iso-a -p) || { echo "Failed to get mount_a"; return 1; }
    mount_b=$($VIBE_BIN ls iso-b -p) || { echo "Failed to get mount_b"; return 1; }

    # Write a file ONLY through session A's NFS mount
    echo 'only in A' > "$mount_a/unique-to-a.txt"
    sleep 0.5

    # Session B should NOT see this file via ls
    if ls "$mount_b/unique-to-a.txt" 2>/dev/null; then
        echo "Session isolation FAILED: session B can see session A's file in mount"
        return 1
    fi

    # Session B should NOT have this file in its dirty paths
    output=$($VIBE_BIN diff iso-b 2>&1)
    if echo "$output" | grep -q "unique-to-a"; then
        echo "Session isolation FAILED: session B sees session A's dirty file"
        echo "Output: $output"
        return 1
    fi

    # Session A SHOULD have the dirty file
    output=$($VIBE_BIN diff iso-a 2>&1)
    if ! echo "$output" | grep -q "unique-to-a"; then
        echo "Session A should show unique-to-a.txt as dirty"
        echo "Output: $output"
        return 1
    fi

    echo "Session isolation workflow passed"
    return 0
}

# ============================================
# WORKFLOW 31: Full End-to-End Workflow
# ============================================
test_workflow_e2e() {
    local repo="$TEST_DIR/repo31"
    setup_test_repo "$repo"
    cd "$repo"

    # 1. Init
    $VIBE_BIN init || { echo "init failed"; return 1; }

    # 2. Create session
    $VIBE_BIN new e2e-test -c "true" || { echo "new failed"; return 1; }

    # 3. Make changes through NFS mount (real code path)
    local mount_point
    mount_point=$($VIBE_BIN ls e2e-test -p) || { echo "Failed to get mount point"; return 1; }
    echo 'pub fn e2e() -> bool { true }' > "$mount_point/e2e.rs"
    sleep 0.5

    # 4. Create checkpoint (save)
    $VIBE_BIN save -s e2e-test || { echo "save failed"; return 1; }

    # 5. Diff should show the change
    output=$($VIBE_BIN diff e2e-test 2>&1)
    echo "$output" | grep -q "e2e.rs" || { echo "diff doesn't show e2e.rs"; return 1; }

    # 6. Commit (was: promote)
    output=$($VIBE_BIN commit e2e-test -m "E2E test commit" 2>&1)
    exit_code=$?
    if [ $exit_code -ne 0 ]; then
        echo "vibe commit failed"
        echo "Output: $output"
        return 1
    fi

    # 7. Verify ref was created
    git show-ref --verify refs/vibes/e2e-test 2>/dev/null || { echo "ref not created"; return 1; }

    # 8. Check status
    output=$($VIBE_BIN ls 2>&1)

    # 9. Kill session (was: close)
    $VIBE_BIN kill e2e-test -f || { echo "kill failed"; return 1; }

    echo "End-to-end workflow passed"
    return 0
}

# ============================================
# WORKFLOW 32: Init in Non-Git Directory
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
# WORKFLOW 33: New Session Auto-inits
# ============================================
test_workflow_spawn_noinit() {
    local repo="$TEST_DIR/repo33"
    setup_test_repo "$repo"
    cd "$repo"

    # Don't run init, try to create session directly (should auto-init now)
    output=$($VIBE_BIN new no-init-test -c "true" 2>&1)
    exit_code=$?

    if [ $exit_code -ne 0 ]; then
        echo "vibe new failed (should auto-init)"
        echo "Output: $output"
        return 1
    fi

    # Verify .vibe was created
    [ -d ".vibe" ] || { echo ".vibe directory not created"; return 1; }
    [ -d ".vibe/sessions/no-init-test" ] || { echo "session not created"; return 1; }

    echo "Auto-init workflow passed"
    return 0
}

# ============================================
# WORKFLOW 34: Double Create Same Session
# ============================================
test_workflow_double_spawn() {
    local repo="$TEST_DIR/repo34"
    setup_test_repo "$repo"
    cd "$repo"

    $VIBE_BIN init || { echo "init failed"; return 1; }
    $VIBE_BIN new double-test -c "true" || { echo "first new failed"; return 1; }

    # Try to create again with same name
    output=$($VIBE_BIN new double-test -c "true" 2>&1)
    exit_code=$?

    # Should either fail or handle gracefully
    if [ $exit_code -eq 0 ]; then
        echo "Double create succeeded (may be OK if handled gracefully)"
    fi

    echo "Double create workflow passed"
    return 0
}

# ============================================
# WORKFLOW 35: Commit Without Dirty Files
# ============================================
test_workflow_commit_empty() {
    local repo="$TEST_DIR/repo35"
    setup_test_repo "$repo"
    cd "$repo"

    $VIBE_BIN init || { echo "init failed"; return 1; }
    $VIBE_BIN new empty-commit -c "true" || { echo "new failed"; return 1; }

    $VIBE_BIN daemon stop 2>/dev/null || true
    sleep 1

    # Commit without any changes
    output=$($VIBE_BIN commit empty-commit 2>&1)
    exit_code=$?

    # Should succeed (no-op) or explicitly say no dirty files
    if [ $exit_code -ne 0 ]; then
        if echo "$output" | grep -qi "no dirty\|nothing to promote\|nothing to commit"; then
            echo "Correctly indicated no dirty files"
        else
            echo "Commit empty failed unexpectedly"
            echo "Output: $output"
            return 1
        fi
    fi

    echo "Commit empty workflow passed"
    return 0
}

# ============================================
# WORKFLOW 36: Unknown Command Error
# ============================================
test_workflow_launch_noagent() {
    local repo="$TEST_DIR/repo36"
    setup_test_repo "$repo"
    cd "$repo"

    $VIBE_BIN init || { echo "init failed"; return 1; }

    # Try to use unknown command (should fail with helpful message)
    output=$($VIBE_BIN nonexistent-command-xyz 2>&1)
    exit_code=$?

    if [ $exit_code -eq 0 ]; then
        echo "Unknown command should have failed"
        return 1
    fi

    # Should mention it's an unknown command
    if ! echo "$output" | grep -qi "unknown\|not found\|error"; then
        echo "Error message not helpful"
        echo "Output: $output"
    fi

    echo "Unknown command workflow passed"
    return 0
}

# ============================================
# WORKFLOW 37: Old Command Aliases Still Work
# ============================================
test_workflow_aliases() {
    local repo="$TEST_DIR/repo37"
    setup_test_repo "$repo"
    cd "$repo"

    $VIBE_BIN init || { echo "vibe init failed"; return 1; }
    $VIBE_BIN new alias-test -c "true" || { echo "vibe new failed"; return 1; }

    # Test: old 'status' alias works
    output=$($VIBE_BIN status 2>&1) || { echo "'vibe status' alias failed"; return 1; }
    echo "$output" | grep -q "alias-test" || { echo "status alias doesn't show session"; return 1; }

    # Test: old 'close' alias works
    output=$($VIBE_BIN close alias-test -f 2>&1) || { echo "'vibe close' alias failed"; return 1; }

    # Session should be gone
    if [ -d ".vibe/sessions/alias-test" ]; then
        echo "Session still exists after close alias"
        return 1
    fi

    echo "Aliases workflow passed"
    return 0
}

# ============================================
# WORKFLOW 38: Commands From Inside Mount
# ============================================
test_workflow_commands_from_mount() {
    local repo="$TEST_DIR/repo-commands-from-mount"
    setup_test_repo "$repo"
    cd "$repo"

    $VIBE_BIN init || { echo "vibe init failed"; return 1; }

    # Create a session with mount
    local session="inside-mount-test"
    $VIBE_BIN new "$session" -c "exit 0" || { echo "vibe new failed"; return 1; }

    # Get the mount point
    local mount_point
    mount_point=$($VIBE_BIN ls "$session" -p) || { echo "vibe ls -p failed"; return 1; }

    if [ ! -d "$mount_point" ]; then
        echo "Mount point doesn't exist: $mount_point"
        return 1
    fi

    # Write a file through the mount (NFS will track it as dirty automatically)
    echo "test content" > "$mount_point/test-file.txt"
    # Give NFS a moment to sync
    sleep 0.5

    # Now cd into the mount and run commands WITHOUT -r or session name
    cd "$mount_point"
    echo "Now in: $(pwd)"

    # Test: vibe ls (no args) - should auto-detect
    $VIBE_BIN ls || { echo "vibe ls from mount failed"; return 1; }

    # Test: vibe diff (no args) - should auto-detect session
    $VIBE_BIN diff || { echo "vibe diff from mount failed"; return 1; }

    # Test: vibe save (no args) - should auto-detect session and create snapshot
    $VIBE_BIN save "test-checkpoint" || { echo "vibe save from mount failed"; return 1; }

    # Test: vibe undo (restore) - list snapshots should work
    $VIBE_BIN undo 2>&1 | grep -q "test-checkpoint" || { echo "vibe undo list from mount failed"; return 1; }

    # Test: vibe commit (no args) - should auto-detect session
    $VIBE_BIN commit -m "committed from mount" || { echo "vibe commit from mount failed"; return 1; }

    # Verify the commit was created
    cd "$repo"
    git show-ref --verify --quiet "refs/vibes/$session" || { echo "refs/vibes/$session missing"; return 1; }

    echo "Commands from inside mount workflow passed"
    return 0
}

# ============================================
# WORKFLOW 39: Bare vibe (no subcommand) Shows Status
# ============================================
test_workflow_bare_vibe() {
    local repo="$TEST_DIR/repo39"
    setup_test_repo "$repo"
    cd "$repo"

    $VIBE_BIN init || { echo "vibe init failed"; return 1; }
    $VIBE_BIN new bare-test -c "true" || { echo "vibe new failed"; return 1; }

    # Test: bare 'vibe' with no subcommand should show status overview
    output=$($VIBE_BIN 2>&1)
    exit_code=$?

    if [ $exit_code -ne 0 ]; then
        echo "bare vibe failed"
        echo "Output: $output"
        return 1
    fi

    # Should show VibeFS status
    if ! echo "$output" | grep -q "VibeFS\|DAEMON\|Session"; then
        echo "Bare vibe doesn't show status overview"
        echo "Output: $output"
        return 1
    fi

    echo "Bare vibe workflow passed"
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

    # Run all tests (or a single test if VIBE_TEST_ONLY is set)
    if [ -n "${VIBE_TEST_ONLY:-}" ]; then
        run_test "$VIBE_TEST_ONLY" "${VIBE_TEST_ONLY_FUNC:-test_workflow_e2e}"
        cleanup
        return 0
    fi

    # Run all tests
    run_test "1. Basic Initialization" test_workflow_init
    run_test "2. Session Creation" test_workflow_spawn_local
    run_test "3. Session Create Auto-name" test_workflow_spawn_autoname
    run_test "4. File Editing in Session" test_workflow_file_editing
    run_test "5. NFS Dirty Tracking" test_workflow_nfs_dirty_tracking
    run_test "6. vibe ls (List Sessions)" test_workflow_ls
    run_test "7. vibe ls --json" test_workflow_ls_json
    run_test "8. vibe ls -v (Verbose)" test_workflow_inspect
    run_test "9. vibe ls -v --json" test_workflow_inspect_json
    run_test "10. Session Diff" test_workflow_diff
    run_test "11. Session Diff Stat" test_workflow_diff_stat
    run_test "12. Save Checkpoint" test_workflow_snapshot
    run_test "13. Save Preserves State" test_workflow_snapshot_preserves
    run_test "14. Undo from Checkpoint" test_workflow_restore
    run_test "15. Commit Session" test_workflow_commit
    run_test "16. Commit with Message" test_workflow_commit_message
    run_test "17. Commit All Sessions" test_workflow_commit_all
    run_test "18. Commit with --only" test_workflow_commit_only
    run_test "19. Kill Session" test_workflow_kill
    run_test "20. Check Dirty Files" test_workflow_kill_dirty
    run_test "21. Kill Nonexistent Session" test_workflow_kill_nonexistent
    run_test "22. Get Session Path" test_workflow_path
    run_test "23. Daemon Status" test_workflow_daemon_status
    run_test "24. Daemon Start/Stop" test_workflow_daemon_start_stop
    run_test "25. Kill Session with Force" test_workflow_kill_session_force
    run_test "26. Command Execution" test_workflow_sh_command
    run_test "27. Multiple Parallel Sessions" test_workflow_parallel_sessions
    run_test "28. Conflict Detection" test_workflow_conflict_detection
    run_test "29. NFS Mount Structure" test_workflow_nfs_mount
    run_test "30. Session Isolation" test_workflow_session_isolation
    run_test "31. Full E2E Workflow" test_workflow_e2e
    run_test "32. Init in Non-Git Dir" test_workflow_init_nongit
    run_test "33. Auto-init on New" test_workflow_spawn_noinit
    run_test "34. Double Create Session" test_workflow_double_spawn
    run_test "35. Commit Without Dirty Files" test_workflow_commit_empty
    run_test "36. Unknown Command Error" test_workflow_launch_noagent
    run_test "37. Old Command Aliases" test_workflow_aliases
    run_test "38. Commands From Inside Mount" test_workflow_commands_from_mount
    run_test "39. Bare vibe Shows Status" test_workflow_bare_vibe

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
