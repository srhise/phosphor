# word — a DOS-style distraction-free writer

**Date:** 2026-08-25
**Status:** Design approved, pending spec review
**Inspiration:** WordPerfect 6.0 for DOS

## 1. Overview

A native macOS desktop application for writing prose, presented as a
simulated VGA text-mode display. The nostalgia is architectural rather
than cosmetic: the app maintains a real 80x25 grid of character cells,
blits them from the IBM VGA ROM bitmap font into a 720x400 framebuffer,
and passes that framebuffer through a CRT shader. It is a DOS screen
because it is built like one, not because it is styled like one.

Documents are plain UTF-8 text files on disk. The app owns no format,
no library, and no lock-in.

### Goals

- Feel like sitting at a 1993 machine: blue screen, phosphor glow, block cursor.
- Remove every affordance that is not the words. One status line, nothing else.
- Modern editing behavior. Cmd-S saves, Cmd-Z undoes, the mouse works.
- Ship as a single self-contained binary in a real `.app`. No runtime.
- Start instantly and idle at near-zero CPU.

### Non-goals

- Rich text, styling, fonts, printing, or page layout.
- Multiple windows or multiple open documents.
- Any network capability whatsoever.
- Faithful reproduction of WordPerfect's command set. We take its
  *look*, not its keybindings. (See Decision D2.)

### Deliberately deferred

Find and replace, spell check, and a document browser are all plausible
and all excluded from v1. They are additive and can arrive later without
disturbing the architecture.

## 2. Decisions carried in from brainstorming

| # | Decision | Rationale |
|---|---|---|
| D1 | Native desktop app, not terminal, not browser | Full control of the CRT simulation; a real icon in the dock |
| D2 | DOS look, modern keybindings | Nostalgia should not cost muscle memory |
| D3 | Plain `.txt` files anywhere on disk | Writing outlives the app that made it |
| D4 | Rust + `winit` + `pixels` + WGSL, not Electron | Emulating a character grid *is* the design; a webview would be pretending |

## 3. Architecture

Four layers, each depending only on the one below it:

```
  keystrokes
      |
      v
  [ editor ]   text buffer, cursor, selection, wrap, undo   <- pure, tested
      |
      v
  [ vga ]      80x25 cell grid: glyph + fg + bg per cell     <- pure, tested
      |
      v
  [ framebuffer ]  720x400 RGBA pixels, blitted from font    <- pure, tested
      |
      v
  [ crt.wgsl ]  scanlines, bloom, curvature, vignette        <- GPU
      |
      v
   window
```

The top three layers are pure functions over data with no windowing
dependency, so the entire application logic is testable with `cargo test`
in a headless process. Only `main.rs` touches `winit`.

### Module layout

```
src/main.rs        window creation, event loop, input dispatch, redraw scheduling
src/editor.rs      text buffer, cursor, selection, undo/redo
src/wrap.rs        soft-wrap a document into visual lines
src/vga.rs         cell grid, palette, string blitting, framebuffer render
src/font.rs        embedded 8x16 and 8x8 glyph tables
src/cp437.rs       Unicode <-> code page 437 mapping
src/status.rs      page / line / position arithmetic and formatting
src/fileio.rs      open, save, atomic write, autosave, crash recovery
src/keymap.rs      key event -> Command enum
shaders/crt.wgsl   post-processing pass
```

`main.rs` is the only module allowed to perform I/O or talk to the OS.
Every other module is a pure library: it takes data and returns data.

## 4. The VGA layer

### Text modes

Both supported modes produce an identical 720x400 framebuffer, exactly as
real VGA hardware did:

| Mode | Grid | Cell | Font |
|---|---|---|---|
| Default | 80x25 | 9x16 | 8x16 |
| Dense (F5 toggles) | 80x50 | 9x8 | 8x8 |

The font is 8 pixels wide but each cell is 9 pixels. VGA hardware
replicated the 8th column into the 9th for glyphs `0xC0..=0xDF` (the box
drawing range) so lines would connect, and left it blank otherwise. We
reproduce this rule.

### Font data

The IBM VGA ROM fonts are embedded directly in the binary as byte arrays:
4096 bytes for the 8x16 face (256 glyphs x 16 rows) and 2048 bytes for
the 8x8 face. Each row is one byte, one bit per pixel, MSB leftmost.
No font files, no loading, no fallback path.

### Palette

The standard 16-color EGA/VGA palette. The application uses four entries:

| Role | Index | RGB |
|---|---|---|
| Screen background | 1 (blue) | `#0000AA` |
| Body text | 7 (light gray) | `#AAAAAA` |
| Status line and emphasis | 15 (white) | `#FFFFFF` |
| Cursor block / selection | inverse video | glyph and cell colors swap |

### Character encoding

The grid stores code page 437 byte values, because that is what the font
indexes. `cp437.rs` holds the canonical 256-entry CP437-to-Unicode table
and derives the reverse map at startup.

On text input, characters macOS produces that CP437 lacks are mapped
where an obvious equivalent exists and dropped otherwise:

| macOS produces | stored as |
|---|---|
| left/right single quote (U+2018/2019) | apostrophe `'` |
| left/right double quote (U+201C/201D) | quote `"` |
| en dash (U+2013) | hyphen `-` |
| em dash (U+2014) | two hyphens `--` |
| ellipsis (U+2026) | three periods `...` |
| non-breaking space (U+00A0) | space |

Accented Latin characters that CP437 does contain pass through
unchanged. Anything unrepresentable (CJK, emoji, Cyrillic) is rejected at
the input boundary and never enters the buffer, so the buffer and the
screen can never disagree. This is a real limitation and is documented in
the README: this app writes English.

## 5. The editor layer

### Text representation

The document is a `Vec<char>`. Insertion and deletion are O(n) memmoves.
For a 200,000-character manuscript that is a sub-millisecond copy on any
machine this app will run on, and it keeps offset arithmetic trivial
throughout the codebase. A rope would be a defensive optimization against
a problem this app does not have.

The cursor is a `usize` character offset. A selection is
`Option<Selection { anchor: usize, head: usize }>`; the caret is always at
`head`.

### The viewport

The status line consumes the last row, leaving **24 text rows** in default
mode and **49** in dense mode. The viewport scrolls vertically only, and
only far enough to keep the cursor on screen: moving the caret one line
past the bottom edge scrolls by exactly one line. There is no horizontal
scrolling, because text always wraps within the visible width.

### Word wrap

Text wraps at **65 columns**, centered in the 80-column screen with a
7-column margin on each side. Sixty-five characters is a 6.5-inch line at
10 characters per inch, which is what an 8.5-inch page with one-inch
margins gives you — the same measure WordPerfect was showing. It is also,
conveniently, a comfortable reading measure.

Wrapping breaks at the last space at or before column 65. A word longer
than 65 characters is broken hard at the column boundary. Wrapping is
recomputed lazily and cached, invalidated from the first affected
paragraph forward on each edit.

### Undo

Command-based, with a `Vec<UndoEntry>` undo stack and a redo stack that is
cleared on any new edit. Each entry records the affected range, the text
removed, the text inserted, and the cursor position before and after.

Consecutive single-character insertions coalesce into one entry. The run
is broken by: a space or newline, any cursor movement, a deletion, a save,
or 500ms of idle time. This produces undo granularity at roughly the word
level, which is what a writer expects.

## 6. The status line

Row 24 (or row 49 in dense mode) is the only permanent interface
element. It renders in white on blue.

**Left:** the current file's path, uppercased and rendered in DOS style —
`/Users/srhise/Documents/ch1.txt` displays as
`C:\USERS\SRHISE\DOCUMENTS\CH1.TXT`. Paths too long for the space are
truncated from the left. An unsaved document reads `(UNTITLED)`. A
modified document is suffixed with `*`.

**Right:** `Doc 1   Pg 1   Ln 1"   Pos 1"`

The measurements are WordPerfect's, in inches, computed from the printer
metrics of the era:

- 6 lines per inch vertically, 10 characters per inch horizontally
- One-inch margins, so both axes start at `1"`
- 54 text lines per page (a 9-inch text block at 6 lpi)

```
Pg  = 1 + (visual_line_index / 54)
Ln  = 1.0 + (visual_line_index % 54) / 6.0
Pos = 1.0 + column / 10.0
```

Values format with trailing zeros stripped: `1"`, `1.17"`, `2.5"`.

### Why no F-key hint bar

Earlier discussion assumed a permanent hint row. On reflection it
contradicts the primary goal: it is a second row of chrome that a writer
reads once and then never again, occupying screen space forever. Instead
**F1 opens a full-screen help overlay** in the WordPerfect idiom — a
double-line box listing every key — dismissed with Escape or any key.
The keys stay discoverable; the writing screen stays clean.

## 7. Input

### Key map

| Keys | Action |
|---|---|
| Arrows | Move by character / visual line |
| Opt-Left / Opt-Right | Move by word |
| Cmd-Left / Cmd-Right, Home / End | Line start / end |
| Cmd-Up / Cmd-Down | Document start / end |
| PgUp / PgDn | Move by one screen |
| Shift + any of the above | Extend selection |
| Cmd-A | Select all |
| Cmd-C / Cmd-X / Cmd-V | Clipboard (via `arboard`) |
| Cmd-Z / Cmd-Shift-Z | Undo / redo |
| Cmd-N / Cmd-O / Cmd-S / Cmd-Shift-S | New / open / save / save as |
| Cmd-Q | Quit, with an unsaved-changes prompt |
| Tab | Insert spaces to the next 8-column stop |
| F1 | Help overlay |
| F3 | CRT effects on / off |
| F5 | Toggle 80x25 / 80x50 |
| F6 | Word count (flashes in the status line for 3 seconds) |
| F11 | Fullscreen |
| Escape | Dismiss any overlay or prompt |

### Mouse

Click positions the cursor at the nearest character cell. Click and drag
selects. Scroll wheel scrolls the viewport. Nothing else.

### Cursor rendering

A filled block in the cell's foreground color, with the glyph beneath
drawn in the background color — the VGA hardware cursor's inverse-video
behavior. It blinks at 2 Hz. When a selection is active the block is
hidden and the selected cells render inverted instead.

## 8. Files

### Loading

`rfd` provides native open and save panels. Files are read as UTF-8;
invalid sequences are replaced lossily rather than refusing the file.

Two normalizations happen on load and are documented as lossy:

1. **Line endings.** CRLF and CR become LF. The original ending is
   remembered and restored on save, so a CRLF file stays a CRLF file.
2. **Tabs.** Expanded to spaces at 8-column stops. Tab characters do not
   survive a load-and-save round trip. This is an acceptable trade for a
   prose editor, and expanding at the boundary keeps every downstream
   column calculation exact.

### Saving

Writes are atomic: write to a temporary file in the destination directory,
`fsync`, then `rename` over the target. A failed save never destroys the
previous version.

### Autosave and crash recovery

Every 30 seconds, if the document is modified, a backup is written to:

```
~/Library/Application Support/word/backup/
```

This mirrors WordPerfect's timed backup rather than silently overwriting
the user's file. Untitled documents are backed up as `UNTITLED.BK1`;
named documents use their base name.

On launch, if a backup exists that is newer than its corresponding file,
the app presents a recovery prompt in period-appropriate language and
offers to open the backup instead. Backups are deleted on a clean save
and on a clean quit.

### Error handling

There are no crashes on user-facing errors. A failed open, a failed save,
a permissions problem, or a file that vanished all render the same way: a
centered double-line box, red on white, with the message and a single
`Press any key to continue` line. The document in memory is never
discarded because of an I/O failure.

Panics in pure modules are treated as bugs, not conditions to handle.
Every `unwrap` in the codebase must be provably unreachable or replaced.

## 9. CRT simulation

A single WGSL fragment shader runs as a post-process pass over the
720x400 framebuffer texture, composed of four effects:

- **Scanlines** — a vertical darkening at the source-pixel row frequency
- **Bloom** — a small gaussian tap set on bright pixels, giving the
  phosphor halo that makes white text on blue look like it is emitting
  light rather than being drawn
- **Barrel distortion** — a slight positive curvature so the corners
  pull in
- **Vignette** — corner falloff

All four are governed by one uniform struct and toggle off together with
F3, because the effects are a delight for ten minutes and a fatigue for
an hour. The setting persists across launches.

### Window and scaling

The framebuffer is presented at a 4:3 aspect ratio, not at its native
pixel aspect. Real VGA text mode displayed 720x400 pixels on a 4:3
monitor, meaning pixels were noticeably taller than they were wide; the
letterforms look wrong without this. The default window is 1440x1080,
letterboxed in black when resized to other aspects.

### Redraw policy

The event loop is `ControlFlow::WaitUntil`. A redraw is requested on
input, on window events, and on the 500ms cursor-blink tick. There is no
continuous animation loop, so an idle window costs approximately nothing.
The `time` uniform for shader flicker advances only on frames that are
already being drawn.

## 10. Persistence of settings

A single TOML file at `~/Library/Application Support/word/config.toml`
stores: CRT effects on/off, text mode (25 or 50), window size, fullscreen
state, and the most recently opened file path. A missing or malformed
config file is replaced with defaults rather than reported as an error.

## 11. Testing

Development is test-first. The three pure layers carry real coverage:

**`editor.rs`** — insertion and deletion at boundaries; cursor movement
across line ends and document ends; word-motion boundary rules; selection
replacement; undo coalescing and every break condition; redo invalidation.

**`wrap.rs`** — breaking at spaces; hard-breaking over-long words;
trailing whitespace at a wrap point; empty lines; a document of one
character; a document of none; offset-to-visual-position round trips.

**`status.rs`** — page, line, and position at the origin, at a page
boundary, and past one; inch formatting including the trailing-zero rules.

**`vga.rs`** — cell writes and clipping at grid edges; the 9th-column
replication rule for box-drawing glyphs; byte-exact framebuffer output
for one known glyph in one known color pair.

**`cp437.rs`** — round-trip fidelity across all 256 code points; the
typographic substitution table; rejection of unrepresentable input.

No test constructs a window or a GPU context. `fileio.rs` is tested
against `tempfile` directories, including the atomic-write path and a
simulated interrupted save. The window, the shader, and the input plumbing
are verified by running the application.

## 12. Dependencies

Deliberately few:

| Crate | Purpose |
|---|---|
| `winit` | window and input |
| `pixels` | wgpu-backed framebuffer with a custom render pass |
| `rfd` | native open and save panels |
| `arboard` | system clipboard |
| `toml` + `serde` | config file |
| `dirs` | Application Support path |
| `tempfile` | dev-dependency, file I/O tests |

Fonts, the CP437 table, and the palette are embedded source, not
dependencies.

## 13. Packaging

`cargo-packager` produces `Word.app` with an ad-hoc signature, sufficient
for local use. The icon is a rendering of the app's own blue screen. The
binary name is `word`; renaming the product is a change to `Cargo.toml`
and the packager manifest only.

## 14. Milestones

Each milestone ends in something that runs.

1. **Screen.** Window, framebuffer, embedded font, palette. Renders a
   static string of blue-screen text at the correct aspect ratio.
2. **Editor core.** `editor.rs` and `wrap.rs` complete and under test,
   with no rendering attached.
3. **Typing.** Editor wired to the grid. Text appears, the cursor blinks
   and moves, the viewport scrolls, the mouse positions the caret.
4. **Status line.** `status.rs` under test and rendering; DOS path
   display; dirty marker.
5. **Files.** New, open, save, save as; dirty tracking; the quit prompt;
   atomic writes.
6. **Durability.** Timed backups and the launch-time recovery prompt.
7. **The glow.** `crt.wgsl`, the F3 toggle, dense mode, fullscreen,
   persisted config.
8. **Finish.** Help overlay, word count, error boxes, `.app` packaging.
