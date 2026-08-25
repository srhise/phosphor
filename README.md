# word

A distraction-free writing app that emulates VGA text mode.

The nostalgia here is architectural, not cosmetic. The app maintains a
real 80×25 grid of character cells, blits them from the IBM VGA ROM font
into a 720×400 framebuffer, and passes that through a CRT shader. It is a
DOS screen because it is built like one.

Inspired by WordPerfect 6.0 for DOS.

```
   It was a bright cold day in April, and the clocks were striking
   thirteen. Winston Smith, his chin nuzzled into his breast in an
   effort to escape the vile wind, slipped quickly through the glass
   doors of Victory Mansions.█



C:\USERS\SRHISE\DOCUMENTS\CHAPTER-ONE.TXT *   Doc 1   Pg 1   Ln 2"   Pos 6.3"
```

## Build

Needs a Rust toolchain. Nothing else.

```sh
cargo run                 # run it
cargo test                # 174 tests, all headless
./tools/package.sh        # build target/Word.app
```

## Keys

DOS look, modern muscle memory.

| | |
|---|---|
| `Cmd-N` / `Cmd-O` / `Cmd-S` / `Cmd-Shift-S` | New, open, save, save as |
| `Cmd-Z` / `Cmd-Shift-Z` | Undo, redo — one word at a time |
| `Cmd-A` / `Cmd-C` / `Cmd-X` / `Cmd-V` | Select all, copy, cut, paste |
| `Opt-Arrow` | Move by word |
| `Cmd-Arrow` | Line start/end, document start/end |
| `Cmd-Q` | Quit |
| `F1` | Help |
| `F3` | CRT effects on/off |
| `F5` | 80×25 / 80×50 |
| `F6` | Word count |
| `F11` | Fullscreen |
| `Esc` | Close any box |

The mouse places the caret, drags to select, and scrolls. There is no
permanent F-key hint bar: it would be a second row of chrome you read
once and then never again. `F1` covers it.

## Files

Plain UTF-8 `.txt`, anywhere on disk. No proprietary format, no library
folder, nothing to export from. Saves are atomic — a failed write never
destroys the previous version.

Text wraps at 65 columns, centred in the 80-column screen. That is a 6.5″
line at 10 characters per inch: an 8.5″ page with 1″ margins, which is
exactly what the `Pos` readout in the status line is measuring.

### Two things it does to your files

Both are consequences of the character grid being real rather than themed,
and both are worth knowing before you trust it with a manuscript:

- **Tabs are expanded to spaces** (8-column stops) when a file is opened,
  and do not survive a round trip. Expanding them at the boundary is what
  lets every column calculation downstream be exact.
- **Only CP437 characters can be typed.** That is ASCII plus accented
  Latin, Greek, and box drawing — the 256 glyphs in the VGA ROM. No CJK,
  no Cyrillic, no emoji: there is no glyph to draw them with, so they are
  rejected at the input boundary rather than stored and shown as blanks.
  Curly quotes, en and em dashes, and ellipses are silently converted to
  their ASCII equivalents on the way in.

Line endings are the exception: CRLF files stay CRLF files.

## Backups

Every 30 seconds a modified document is copied to:

```
~/Library/Application Support/word/backup/
```

This never overwrites your own file — it mirrors WordPerfect's timed
backup. If a backup outlives its document, the next launch offers to
recover it. Settings live beside it in `config.toml`; delete it to reset.

## How it fits together

Four layers, each depending only on the one below. The top three are pure
functions over data with no windowing dependency, which is why the whole
application logic tests headless.

```
 keystrokes → editor → vga grid → framebuffer → crt.wgsl → window
              ‾‾‾‾‾‾‾‾‾‾‾‾‾‾‾‾‾‾‾‾‾‾‾‾‾‾‾‾‾‾‾‾
                    pure, fully tested
```

| | |
|---|---|
| `src/editor.rs` | Text buffer, cursor, selection, undo |
| `src/wrap.rs` | Soft wrap; offset ↔ (line, column) |
| `src/vga.rs` | Cell grid, palette, framebuffer rasterizer |
| `src/font.rs` | The embedded ROM fonts |
| `src/cp437.rs` | Encoding and the input filter |
| `src/status.rs` | Page/line/position arithmetic |
| `src/fileio.rs` | Load, atomic save, normalization |
| `src/backup.rs` | Timed backups and recovery |
| `src/app.rs` | State and the command interpreter |
| `src/main.rs` | Window, events, everything OS-facing |

Two ignored tests dump what the renderer actually produces, which is the
fastest way to see a change:

```sh
cargo test dump_app_preview -- --ignored   # target/app-preview.bmp
```

Design notes are in `docs/superpowers/`.

## Fonts

`assets/fonts/*.bin` are raw character-generator ROM dumps from IBM VGA
hardware. See `assets/fonts/PROVENANCE.md`.
