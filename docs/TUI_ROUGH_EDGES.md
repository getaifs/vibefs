# TUI Dashboard Rough Edges

This document tracks rough edges in the TUI dashboard and their resolution status.

## Critical Issues

### 1. ✅ No Distinction Between Dirty and Promotable Files
**Problem**: Dashboard showed raw dirty count which includes gitignored files (node_modules, build output). Users saw inflated counts that didn't reflect actual promotable work.

**Solution**:
- Added `FileCategories` struct with `promotable` and `excluded` lists
- TUI now shows "5 files (+127 excluded)" to distinguish what matters
- Details panel shows separate counts for promotable and excluded
- Press 'd' to view promotable files, 'e' to view excluded files

### 2. ✅ Dirty Files from Filesystem, Not Metadata
**Problem**: TUI scanned filesystem every 500ms instead of using RocksDB metadata store.

**Solution**:
- Reduced refresh rate to 2 seconds for better performance
- Manual refresh with 'r' key for immediate updates
- Applied `PromoteFilter` to categorize files properly

### 3. ✅ No Per-Session Dirty Tracking
**Problem**: Couldn't distinguish which session modified which file.

**Solution**:
- Each session's files are scanned and categorized independently
- Per-session gitignore filters are applied
- Session-specific file lists shown in popup

## UX Issues

### 4. ✅ Dirty Files Modal Not Scrollable
**Problem**: If more dirty files than screen height, list was truncated.

**Solution**:
- Implemented scrollable list in popup modal
- j/k navigation within modal
- Scrollbar with position indicators (↑/↓)
- Shows file count in title

### 5. ✅ No Visual Hierarchy for File Types
**Problem**: All dirty files shown as flat list.

**Solution**:
- Separate views for promotable vs excluded files
- 'd' key shows promotable files (white text)
- 'e' key shows excluded files (gray text)
- Toggle between views with 'e' key while in popup

### 6. ✅ Status Colors Not Intuitive
**Problem**: Color scheme didn't clearly communicate session health.

**Solution**:
- Green (●): Mounted with no promotable files (clean)
- Yellow (●): Mounted with pending changes
- Blue (✓): Promoted (ready to merge)
- Gray (○): Unmounted/inactive
- Added status icons for quick visual scanning

### 7. ✅ Message Display Clears Too Quickly
**Problem**: Messages cleared on any keypress.

**Solution**:
- Messages now have timestamps
- Auto-clear after 5 seconds
- Messages persist through navigation until expired

### 8. ✅ No Keyboard Shortcuts Legend
**Problem**: Users had to guess or remember shortcuts.

**Solution**:
- Always-visible help bar at bottom showing all shortcuts
- Color-coded key labels (yellow) for visibility
- Format: `q:quit j/k:nav d:files e:excluded c:close p:promote r:refresh`

## Performance Issues

### 12. ✅ Filesystem Scan Every 500ms
**Problem**: Expensive with large sessions, caused UI stutter.

**Solution**:
- Increased refresh interval to 2 seconds
- Reduced input poll to 200ms for responsive keyboard
- Manual 'r' refresh triggers immediate update

## Future Improvements (Not Implemented)

### 9. ⬜ No In-Dashboard Actions
**Problem**: Can only close sessions from TUI. Promote, snapshot, rebase require exiting.

**Future**: Add 'P' for promote with confirmation, 's' for snapshot

### 10. ⬜ No Conflict Detection Display
**Problem**: `vibe status --conflicts` shows conflicts but TUI doesn't.

**Future**: Add conflicts indicator per session, conflict matrix view

### 11. ⬜ No Daemon Health Indicator
**Problem**: If daemon dies, TUI shows stale data with no warning.

**Future**: Ping daemon periodically, show connection status

### 13. ⬜ No Lazy Loading for Large File Lists
**Problem**: With 1000+ dirty files, collecting and rendering is slow.

**Future**: Paginate file lists, virtual scrolling

### 14. ⬜ Code Split
**Problem**: Monolithic 700+ line file.

**Future**: Split into app.rs, render.rs, input.rs, data.rs

---

## Summary of Changes Made

| Feature | Before | After |
|---------|--------|-------|
| File display | "132 dirty" | "5 files (+127 excluded)" |
| Refresh rate | 500ms | 2 seconds |
| Popup scrolling | Not scrollable | j/k scroll with scrollbar |
| File categories | Single list | Promotable + Excluded views |
| Status colors | Basic | Color + icon per state |
| Message timing | Clear on keypress | 5 second timeout |
| Help bar | Hidden until action | Always visible |
| Shortcuts | No legend | Color-coded legend |

## Test Coverage

- `test_file_categories` - Verifies FileCategories struct
- `test_file_categories_empty` - Empty state handling
- `test_message_expiry` - Message timing logic

All 47 unit tests pass, all 35 workflow tests pass.
