# Q03 — Interactive File Picker with Live Autocomplete

**Date:** 2026-02-19
**Status:** ✅ Complete
**Plan:** `plans/Q03-interactive-file-picker.md`

---

## Summary

Replaced the simple text-input file picker with an interactive dialog that shows live suggestions as the user types. Features include real-time directory listing (up to 5 matches), Up/Down arrow navigation, Tab-based directory traversal, and audio format verification before loading. The picker is inspired by Claude Code's file open dialog.

---

## Implementation Details

### New AppState Fields (`src/app.rs`)

```rust
pub file_picker_matches: Vec<String>,    // up to 5 matching paths; dirs have trailing '/'
pub file_picker_selected: Option<usize>, // highlighted index into matches
```

Both initialized to empty/`None` in `AppState::new()`.

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
- Take first 5 matches
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
| `Esc` | Clear mode, input, cursor, matches, selection → return Normal mode |
| `Down` | Move selection: `None→0`, `i→min(i+1, len-1)`; no-op if empty |
| `Up` | Move selection: `0→None`, `i→i-1`; no-op if None or empty |
| `Tab` | If selected dir: set input to path + "/", clear selection, refresh matches; if file: set input to path |
| `Enter` (selected dir) | Same as Tab: navigate into directory |
| `Enter` (selected file) | `precheck_audio_file()` → emit `Action::LoadFile` if OK, else error status |
| `Enter` (no selection) | Validate raw input with `exists()` + `is_file()`, then `precheck`, emit or error |
| `Char/Backspace/Delete` | `handle_text_input()` → `update_file_picker_matches()` → reset selection |
| `Left/Right/Home/End` | `handle_text_input()` only; preserve selection |

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
app.file_picker_selected = None;
app.mode = AppMode::Saving;
```

### Updated Popup Render (`src/ui/file_picker.rs`)

**Dynamic height calculation:**
```rust
let n = app.file_picker_matches.len();
let popup_h: u16 = if n == 0 { 4 } else { (5 + n) as u16 };  // range: 4–10 rows
```

**Inner area layout (3 sections):**

1. **Hint area (1 row):**
   - Text: `" ↑↓ select   Tab complete   Esc cancel "`
   - Style: dark gray

2. **Input area (1 row):**
   - Reuses existing `render_input_line()` with horizontal scrolling
   - Renders as: `" > before█after"` with cursor block in cyan

3. **Match area (0–5 rows):**
   - If n > 0: divider line `"─────"` in dark gray, then match items
   - Each match item (one row):
     - Selected: `"▶ path"` in bold yellow
     - Unselected dir: `"  path/"` in cyan
     - Unselected file: `"  path"` in white
   - If n = 0 and input non-empty: show `"  no matches"` in dark gray
   - Paths longer than width - 2: truncated with `…` suffix

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

### Manual Testing Checklist
✓ Open picker with `o` → CWD contents appear immediately (up to 5)
✓ Type a letter → suggestions update live; selection resets
✓ Press `↓`/`↑` → highlight moves through list; `↑` from 0 deselects
✓ Press `Tab` on directory → input advances to dir path, list refreshes
✓ Press `Tab` on file → input set to file path (allows preview before Enter)
✓ Press `Enter` on audio file match → file loads, picker closes
✓ Press `Enter` on non-audio file match → status error shown, picker stays open
✓ Press `Enter` on unreadable file (permission denied) → status error, stays open
✓ Press `Enter` on directory match → navigates into directory
✓ Press `Enter` with raw input (no selection) → original validation flow
✓ Press `Esc` → picker closes, state cleaned
✓ Open save dialog (`s`) → no stale file picker suggestions
✓ Non-UTF8 filenames → handled gracefully via `to_string_lossy()`
✓ Large directory (1000+ files) → no hang (capped at 1000 scanned)

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
- **Memory:** matches vector never exceeds 5 items
- **UI redraw:** full popup redrawn each frame (standard ratatui pattern)

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

## Commit

`3e8af8b` — feat: add interactive file picker with live autocomplete

---

## Future Enhancements (Out of Scope)

- Keyboard shortcuts to jump to home, root, or recent directories
- Search mode (inverse of type-to-filter)
- File preview (image thumbnails, audio waveform) — would need rasterization
- Sort by name/date/size
- Favorite/bookmark directories
- Shell glob expansion (`*.wav`)
