# Q03 — Interactive File Picker with Live Autocomplete

**Date:** 2026-02-19
**Status:** ✅ Complete
**Plan:** `plans/Q03-interactive-file-picker.md`

---

## Summary

Replaced the simple text-input file picker with an interactive dialog that shows live suggestions as the user types. Features include real-time directory listing with scrollable 5-row window, Up/Down arrow navigation with auto-scroll tracking, Tab-based directory traversal, and audio format verification before loading. The picker is inspired by Claude Code's file open dialog.

**Enhancement (2026-02-19):** Extended to support directories with 10+ matches via a 5-row scrolling window. All matches are stored; scroll offset tracks selection automatically.

---

## Implementation Details

### New AppState Fields (`src/app.rs`)

```rust
pub file_picker_matches: Vec<String>,    // all matching paths; dirs have trailing '/'
pub file_picker_scroll: usize,           // scroll offset: first visible item in 5-row window
pub file_picker_selected: Option<usize>, // highlighted index into matches (absolute, not relative to scroll)
```

All initialized to empty/0/`None` in `AppState::new()`. The `file_picker_scroll` field enables scrolling through lists larger than the 5-row visible area.

### Helper Function: `update_file_picker_matches()` (`src/input/handler.rs`)

Called on every input change and when picker opens. Algorithm:

**Path parsing:**
- Split input at `rfind('/')` to extract `(dir_part, prefix)`
- If no `/`: dir = ".", prefix = whole input
- If ends with `/`: dir = input, prefix = ""
- If `~/` prefix: expand to `$HOME`; fallback to "." if HOME unset
- Fallback empty dir to "."

**Directory listing:**
- `fs::read_dir(dir).take(1000)` — scan up to 1000 entries (guard against huge directories)
- For each entry:
  - Skip if name starts with '.' and prefix doesn't
  - Skip if name doesn't start with prefix
  - Determine if directory via `entry.path().metadata()` (follows symlinks)
  - Store full path with trailing `/` for directories

**Sorting and filtering:**
- Directories first, then files; alphabetical within each group
- Store all filtered matches (no `.take(5)` cap)
- Reset `file_picker_scroll` to 0 (when list recomputed, list may have changed completely)
- Clamp `file_picker_selected` to valid range; set `None` if matches empty

### Helper Function: `precheck_audio_file()` (`src/input/handler.rs`)

Verifies file is readable and matches known audio format magic bytes. Returns `Result<(), String>` where error is a user-friendly message.

**Magic byte signatures checked:**
- WAV: `RIFF` (bytes 0-3) + `WAVE` (bytes 8-11)
- FLAC: `fLaC` (bytes 0-3)
- OGG: `OggS` (bytes 0-3)
- MP3: `ID3` (bytes 0-2) OR sync word `0xFF + 0xEX`
- M4A/AAC: `ftyp` (bytes 4-7)
- AIFF: `FORM` (bytes 0-3) + `AIFF` (bytes 8-11)

**Edge cases:**
- File can't be opened → "Cannot open file: {error}"
- File is empty → "File is empty"
- File exists but isn't audio → "Not a recognized audio format: {filename}"

### Updated `handle_file_picker()` (`src/input/handler.rs`)

Replaced the simple `_ => handle_text_input() → None` with explicit key routing:

| Key | Handler |
|-----|---------|
| `Esc` | Clear mode, input, cursor, matches, selection, **scroll** → return Normal mode |
| `Down` | Move selection: `None→0`, `i→min(i+1, len-1)`; adjust scroll to keep sel visible in [scroll..scroll+5] |
| `Up` | Move selection: `0→None`, `i→i-1`; adjust scroll to keep sel visible in [scroll..scroll+5] |
| `Tab` | **Auto-complete to first match if no selection**, else use selected match. If dir: set input + "/", clear selection, refresh; if file: set input to path |
| `Enter` (selected dir) | Same as Tab: navigate into directory |
| `Enter` (selected file) | `precheck_audio_file()` → emit `Action::LoadFile` if OK, else error status |
| `Enter` (no selection) | Validate raw input with `exists()` + `is_file()`, then `precheck`, emit or error |
| `Char/Backspace/Delete` | `handle_text_input()` → `update_file_picker_matches()` → reset selection |
| `Left/Right/Home/End` | `handle_text_input()` only; preserve selection |

**Tab behavior details:**
```rust
// If no selection, auto-complete to first match (if any matches exist)
let match_idx = app.file_picker_selected
    .or_else(|| if app.file_picker_matches.is_empty() { None } else { Some(0) });
```

This means:
- User types `aud` → sees `[audio.wav, audio_2.wav, ...]` → presses Tab → auto-completes to `audio.wav`
- User types `m` → sees `[music/, main.wav, ...]` → presses Tab → navigates into `music/` directory
- Already highlighted a match → Tab uses that match (preserves selection)

### Updated `handle_normal()` Key Arms (`src/input/handler.rs`)

**`'o'` arm (open file picker):**
```rust
app.mode = AppMode::FilePicker;
app.file_picker_input.clear();
app.input_cursor = 0;
app.file_picker_selected = None;
update_file_picker_matches(app);  // ← populate CWD immediately
```

**`'s'` arm (open save dialog):**
```rust
// ... existing code ...
app.file_picker_matches.clear();   // ← prevent visual bleed
app.file_picker_scroll = 0;        // ← reset scroll offset
app.file_picker_selected = None;
app.mode = AppMode::Saving;
```

### Updated Popup Render (`src/ui/file_picker.rs`)

**Dynamic height calculation:**
```rust
let total = app.file_picker_matches.len();
let n_visible = total.min(5);  // 5-row window max
let popup_h: u16 = if n_visible == 0 { 4 } else { (5 + n_visible) as u16 };  // range: 4–10 rows (fixed height)
```

The popup height is now constant—capped at 5 visible items. If total matches exceeds 5, users scroll with Up/Down.

**Inner area layout (3 sections):**

1. **Hint area (1 row):**
   - Text: `" ↑↓ select   Tab complete   Esc cancel "`
   - Style: dark gray

2. **Input area (1 row):**
   - Reuses existing `render_input_line()` with horizontal scrolling
   - Renders as: `" > before█after"` with cursor block in cyan

3. **Match area (0–5 rows):**
   - If n_visible > 0: render windowed divider with scroll indicator, then match items
   - **Divider with scroll indicator:**
     - No items above/below: plain dashes `"─────"`
     - Items above (above > 0): `"─ ↑N ─────"` (where N = count above scroll)
     - Items below (below > 0): `"─ ↓N ─────"` (where N = count below visible window)
     - Both: `"─ ↑N ↓M ─"` (shows bidirectional scroll availability)
   - **Match items (windowed slice [scroll..scroll+5]):**
     - Each match item (one row):
       - Selected: `"▶ path"` in bold yellow
       - Unselected dir: `"  path/"` in cyan
       - Unselected file: `"  path"` in white
     - Paths longer than width - 2: truncated with `…` suffix
   - If n_visible = 0 and input non-empty: show `"  no matches"` in dark gray

---

## Code Statistics

- **Lines added to handler.rs:** ~200 (2 new functions + updated logic)
- **Lines modified in file_picker.rs:** ~60 (render redesign)
- **Lines added to app.rs:** 3 (field declarations + initialization)
- **Total new code:** ~260 lines

---

## Testing & Verification

### Build & Lint
```
✓ cargo build — Clean compilation
✓ cargo clippy --all-targets -- -D warnings — Zero warnings
✓ cargo test --all-targets — All 52 tests passing
```

### Manual Testing Checklist (Original Features)
✓ Open picker with `o` → CWD contents appear immediately
✓ Type a letter → suggestions update live; selection resets, scroll resets to 0
✓ Press `↓`/`↑` → highlight moves through list; `↑` from 0 deselects
✓ Press `Tab` with no selection → auto-completes to first visible match (dir or file)
✓ Press `Tab` on highlighted directory → input advances to dir path, list refreshes
✓ Press `Tab` on highlighted file → input set to file path (allows preview before Enter)
✓ Press `Enter` on audio file match → file loads, picker closes
✓ Press `Enter` on non-audio file match → status error shown, picker stays open
✓ Press `Enter` on unreadable file (permission denied) → status error, stays open
✓ Press `Enter` on directory match → navigates into directory
✓ Press `Enter` with raw input (no selection) → original validation flow
✓ Press `Esc` → picker closes, state and scroll cleaned
✓ Open save dialog (`s`) → no stale file picker suggestions or scroll offset
✓ Non-UTF8 filenames → handled gracefully via `to_string_lossy()`
✓ Large directory (1000+ files) → no hang (capped at 1000 scanned)

### Manual Testing Checklist (Scrollable Enhancement — 2026-02-19)
✓ Directory with 10+ matches → stores all; only 5 visible at a time
✓ Press `↓` beyond row 4 → scroll advances automatically; selection stays visible
✓ Press `↑` above visible window → scroll adjusts; selection stays visible
✓ Divider shows `↓N` when more items below the window
✓ Divider shows `↑N` when items above the window
✓ Divider shows `↑N ↓M` when both above and below
✓ Divider shows plain dashes when all items fit in window
✓ Tab auto-completes to first _visible_ match (respects scroll)
✓ Enter on selection beyond row 5 works correctly (uses absolute index)

### Edge Cases Verified
- Empty directory → no matches, "no matches" message shown
- Directory with only dotfiles → hidden unless prefix starts with '.'
- Symlinks to directories → correctly identified as directories
- HOME environment variable missing → fallback to "."
- Input with `~/path` → correctly expanded
- File permission denied → clear error message before loading
- Binary non-audio file → rejected by magic byte check

---

## Behavioral Changes

### User-Facing
- **Before:** Type full path blind, press Enter, hope it works
- **After:** See matches as you type, navigate with arrow keys, auto-complete with Tab, files verified before loading

### State Management
- File picker state (`matches`, `selected`) is **file-picker-only** (not shared with save dialog)
- Clear on: Esc, Enter (file load), or mode switch to Saving
- Preserve on: cursor movement keys (Left/Right/Home/End)

### Error Handling
- Non-audio files: rejected with `"Not a recognized audio format"`
- Unreadable files: rejected with `"Cannot open file: {error}"`
- Empty files: rejected with `"File is empty"`
- All errors shown as status messages; picker stays open

---

## Performance Notes

- **Directory scan cap:** 1000 entries max (prevents hang on huge directories)
- **Update frequency:** every keystroke (acceptable for typical directories <1000 files)
- **Memory:** matches vector can grow to 1000 items (all filtered matches stored); scroll window is O(1) offset tracking
- **Visible items:** fixed at 5-row window, regardless of total list size
- **UI redraw:** full popup redrawn each frame (standard ratatui pattern); only visible slice rendered

---

## Integration with Existing Systems

### Audio Pipeline
- File loading via `Action::LoadFile(path)` unchanged
- Processing thread, playback, resynthesis all work with prevalidated files

### Saving/Export
- Save dialog (`'s'` key) uses same `file_picker_input` + `input_cursor` fields
- File picker matches cleared when entering Saving mode (no visual interference)

### Input Handler
- `handle_text_input()` unchanged; called for Char/Backspace/Delete, then `update_matches()` called separately
- Arrow key handling moved to file picker context (Up/Down now meaningful)

---

## Commits

**Initial implementation:**
- `3e8af8b` — feat: add interactive file picker with live autocomplete

**Enhancement (2026-02-19):**
- `1ace706` — feat: implement scrollable file picker with 5-row window

---

## Future Enhancements (Out of Scope)

- Keyboard shortcuts to jump to home, root, or recent directories
- Search mode (inverse of type-to-filter)
- File preview (image thumbnails, audio waveform) — would need rasterization
- Sort by name/date/size
- Favorite/bookmark directories
- Shell glob expansion (`*.wav`)
