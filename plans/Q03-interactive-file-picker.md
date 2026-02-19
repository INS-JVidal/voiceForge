# Plan: Interactive File Picker with Live Autocomplete

## Context

The current file picker (`'o'` key) is a plain text input with no feedback — users must type a full path blind. Replace it with an interactive picker similar to Claude Code's file open dialog: live suggestions appear as the user types, Up/Down arrows navigate, Tab completes paths, and files are verified as valid audio before loading.

---

## Goals

1. **Live autocomplete** — Show up to 5 matching files/directories as user types
2. **Navigation** — Use Up/Down arrows to select from suggestions; Up from 0 → deselect
3. **Directory traversal** — Tab on a directory to navigate into it; Tab on a file to preview the path
4. **File integrity** — Verify file is readable and matches known audio format signatures before loading
5. **Robustness** — Handle edge cases: large directories (1000+ files), non-UTF8 names, inaccessible dirs, missing HOME
6. **Clean state** — Clear picker state when entering Save mode to prevent visual bleed

---

## Key Implementation Facts

- **Input state**: `app.file_picker_input` (String) + `app.input_cursor` (byte offset) — shared with `AppMode::Saving`
- **Current validation**: checks `exists()` + `is_file()` before emitting `Action::LoadFile`
- **Popup height**: currently hard-coded at 5 rows; must become dynamic (4–10 rows based on matches)
- **Inner area**: currently shows 3 rows (hint + blank + input); must split into hint + input + match list
- **Text editing**: `handle_text_input()` handles cursor keys; Up/Down/Tab fall through (not handled today)

---

## Design Decisions

### Path Matching Algorithm

```
1. Parse input at last '/' → (dir_part, prefix)
   - No '/'      → dir = ".",   prefix = input
   - Ends '/'    → dir = input, prefix = ""
   - Has '/'     → dir = input[..=last_slash], prefix = input[last_slash+1..]

2. Expand ~/  → $HOME/; fallback to "." if HOME unset

3. read_dir(dir).take(1000) — scan up to 1000 entries (large-dir guard)

4. Filter:
   - Hide dotfiles unless prefix starts with '.'
   - Match entries where name.starts_with(prefix)

5. Sort: directories first (followed symlinks), then files, alphabetical within each

6. Take first 5 matches; store full path (strip "./" prefix if dir was ".")
```

### File Integrity Verification

Check file magic bytes to recognize audio formats without relying on extension:
- **WAV**: `RIFF` at 0, `WAVE` at 8
- **FLAC**: `fLaC` at 0
- **OGG**: `OggS` at 0 (Vorbis, Opus, Flac)
- **MP3**: `ID3` at 0, OR sync word `0xFF 0xEX`
- **M4A/AAC**: `ftyp` at offset 4
- **AIFF**: `FORM` at 0, `AIFF` at 8

Read first 12 bytes; if no match, reject with error message (but allow raw input fallback).

### Keyboard Behavior

| Key | Action |
|-----|--------|
| Down | `None→0`, `i→min(i+1, len-1)` |
| Up | `0→None`, `i→i-1` |
| Tab | Dir: set input + `/`, clear selection, refresh matches; File: set input to path |
| Enter (selected dir) | Navigate into directory (same as Tab) |
| Enter (selected file) | Precheck audio format → load if valid, error status if not |
| Enter (raw input) | Validate path exists + is_file → precheck → load or error |
| Char/Backspace/Delete | Update input → recompute matches → reset selection to None |
| Left/Right/Home/End | Move cursor only (preserve selection) |
| Esc | Reset mode, clear input/cursor/matches/selection |

### Popup Rendering

**Dynamic height:**
```
n = matches.len()
popup_h = if n == 0 { 4 } else { 5 + n }  // max 10
```

**Inner area split (3 sections):**
1. Hint row (1): `" ↑↓ select   Tab complete   Esc cancel "`
2. Input row (1): `" > text█after"`
3. Match area (≤5 rows when n > 0):
   - Row 0: divider `"─────"` in dark gray
   - Rows 1..n: match items
     - Selected: `"▶ path"` in bold yellow
     - Unselected dir: `"  path/"` in cyan
     - Unselected file: `"  path"` in white
   - If n == 0 and input non-empty: show `"  no matches"` in dark gray

---

## Files to Modify

| File | Changes |
|------|---------|
| `src/app.rs` | Add 2 new AppState fields: `file_picker_matches`, `file_picker_selected` |
| `src/input/handler.rs` | Add `update_file_picker_matches()`, `precheck_audio_file()`, rewrite `handle_file_picker()`, patch `'o'` and `'s'` arms |
| `src/ui/file_picker.rs` | Dynamic height, 3-section layout, match list rendering |

---

## Verification Checklist

```bash
cargo build                              # Clean build
cargo clippy --all-targets -- -D warnings  # Zero warnings
cargo test --all-targets                  # All tests pass
cargo run <audio_file>
```

**Manual testing:**
- ✓ Press `o` → popup opens, CWD entries appear (up to 5, dirs in cyan)
- ✓ Type letters → matches update live, selection resets
- ✓ Press `↓`/`↑` → highlight moves, `↑` from 0 → deselect
- ✓ Press `Tab` on dir → input advances, list refreshes
- ✓ Press `Tab` on file → input set to path (preview)
- ✓ Press `Enter` on valid audio file → loads and closes
- ✓ Press `Enter` on non-audio file → status error, picker stays open
- ✓ Press `Enter` on unreadable file (perms) → status error, stays open
- ✓ Press `Esc` → closes cleanly
- ✓ Press `s` (save dialog) → no stale file picker suggestions shown
- ✓ Non-UTF8 filenames → handled via `to_string_lossy()`
- ✓ Huge directory (1000+ files) → no hang

---

## Success Criteria

✅ All 52 existing tests pass
✅ Zero clippy warnings
✅ File picker opens with CWD contents on first open
✅ Live matching on every keystroke (performance acceptable)
✅ Up/Down/Tab navigation works smoothly
✅ Audio format verification rejects non-audio files
✅ State cleanup prevents visual leaks between modes
✅ No crashes on edge cases (missing HOME, huge dirs, bad perms)
