//! Application state: the document, where it is on screen, and the
//! interpretation of every command.

use std::path::PathBuf;

use crate::cp437;
use crate::editor::Editor;
use crate::fileio::TAB_STOP;
use crate::keymap::{Command, Motion};
use crate::status;
use crate::vga::{Mode, Screen};
use crate::wrap::{self, VisualLine};

/// Body text and status line colours (Global Constraints).
const FG: u8 = 7;
const BG: u8 = 1;
const STATUS_FG: u8 = 15;

pub struct App {
    editor: Editor,
    screen: Screen,
    path: Option<PathBuf>,
    crlf: bool,
    /// First visual line shown in the viewport.
    viewport_top: usize,
    /// Cached wrap of the whole document, rebuilt when the text changes.
    lines: Vec<VisualLine>,
    /// Column the cursor is trying to keep while moving vertically.
    goal_col: Option<usize>,
    pub should_quit: bool,
}

impl App {
    pub fn new() -> Self {
        let editor = Editor::new();
        let lines = wrap::wrap(editor.text(), wrap::TEXT_COLS);
        Self {
            editor,
            screen: Screen::new(Mode::Text80x25),
            path: None,
            crlf: false,
            viewport_top: 0,
            lines,
            goal_col: None,
            should_quit: false,
        }
    }

    pub fn editor(&self) -> &Editor {
        &self.editor
    }

    pub fn screen(&self) -> &Screen {
        &self.screen
    }

    pub fn viewport_top(&self) -> usize {
        self.viewport_top
    }

    pub fn path(&self) -> Option<&std::path::Path> {
        self.path.as_deref()
    }

    pub fn crlf(&self) -> bool {
        self.crlf
    }

    pub fn set_path(&mut self, path: PathBuf, crlf: bool) {
        self.path = Some(path);
        self.crlf = crlf;
    }

    pub fn mark_saved(&mut self) {
        self.editor.mark_saved();
    }

    /// Rows of the grid available for text: everything but the status line.
    pub fn text_rows(&self) -> usize {
        self.screen.rows() - 1
    }

    /// Replace the document, as when opening a file.
    pub fn load_text(&mut self, text: &str) {
        self.editor = Editor::from_str(text);
        self.viewport_top = 0;
        self.goal_col = None;
        self.reflow();
    }

    fn reflow(&mut self) {
        self.lines = wrap::wrap(self.editor.text(), wrap::TEXT_COLS);
    }

    fn cursor_position(&self) -> (usize, usize) {
        wrap::position_of(&self.lines, self.editor.cursor())
    }

    /// Scroll the minimum distance that puts the cursor back on screen.
    fn scroll_to_cursor(&mut self) {
        let (line, _) = self.cursor_position();
        let rows = self.text_rows();
        if line < self.viewport_top {
            self.viewport_top = line;
        } else if line >= self.viewport_top + rows {
            self.viewport_top = line + 1 - rows;
        }
    }

    /// Every text-changing command ends the same way.
    fn after_edit(&mut self) {
        self.goal_col = None;
        self.reflow();
        self.scroll_to_cursor();
    }

    fn move_cursor(&mut self, motion: Motion, extend: bool) {
        let (line, col) = self.cursor_position();
        let rows = self.text_rows();
        let cursor = self.editor.cursor();

        // Vertical motion remembers the column it started from, so a trip
        // through a short line does not lose your place.
        let vertical = matches!(
            motion,
            Motion::Up | Motion::Down | Motion::PageUp | Motion::PageDown
        );
        let goal = if vertical {
            let g = self.goal_col.unwrap_or(col);
            self.goal_col = Some(g);
            g
        } else {
            self.goal_col = None;
            col
        };

        let target = match motion {
            Motion::Left => cursor.saturating_sub(1),
            Motion::Right => (cursor + 1).min(self.editor.len()),
            Motion::WordLeft => self.editor.word_left(),
            Motion::WordRight => self.editor.word_right(),
            Motion::LineStart => wrap::offset_at(&self.lines, line, 0),
            Motion::LineEnd => wrap::offset_at(&self.lines, line, usize::MAX),
            Motion::DocStart => 0,
            Motion::DocEnd => self.editor.len(),
            Motion::Up => wrap::offset_at(&self.lines, line.saturating_sub(1), goal),
            Motion::Down => wrap::offset_at(&self.lines, line + 1, goal),
            Motion::PageUp => wrap::offset_at(&self.lines, line.saturating_sub(rows), goal),
            Motion::PageDown => wrap::offset_at(&self.lines, line + rows, goal),
        };

        self.editor.set_cursor(target, extend);
        // set_cursor cleared the run; restore the goal for vertical moves.
        if vertical {
            self.goal_col = Some(goal);
        }
        self.scroll_to_cursor();
    }

    pub fn apply(&mut self, cmd: Command, now_ms: u64) {
        match cmd {
            Command::Insert(text) => {
                // Filter through CP437 so the buffer can only ever hold
                // characters the screen can draw.
                let filtered: String = text.chars().filter_map(cp437::accept).collect();
                if filtered.is_empty() {
                    return;
                }
                self.editor.insert(&filtered, now_ms);
                self.after_edit();
            }
            Command::Newline => {
                self.editor.insert("\n", now_ms);
                self.after_edit();
            }
            Command::Tab => {
                let (_, col) = self.cursor_position();
                let spaces = TAB_STOP - (col % TAB_STOP);
                self.editor.insert(&" ".repeat(spaces), now_ms);
                self.after_edit();
            }
            Command::Backspace => {
                self.editor.backspace(now_ms);
                self.after_edit();
            }
            Command::DeleteForward => {
                self.editor.delete_forward(now_ms);
                self.after_edit();
            }
            Command::Move { motion, extend } => self.move_cursor(motion, extend),
            Command::SelectAll => {
                self.editor.set_cursor(0, false);
                self.editor.set_cursor(self.editor.len(), true);
            }
            Command::Undo => {
                if self.editor.undo() {
                    self.after_edit();
                }
            }
            Command::Redo => {
                if self.editor.redo() {
                    self.after_edit();
                }
            }
            Command::Quit => self.should_quit = true,
            // Handled by main.rs or by later tasks.
            Command::Copy
            | Command::Cut
            | Command::Paste
            | Command::New
            | Command::Open
            | Command::Save
            | Command::SaveAs
            | Command::ToggleHelp
            | Command::ToggleEffects
            | Command::ToggleDenseMode
            | Command::ShowWordCount
            | Command::ToggleFullscreen
            | Command::Dismiss => {}
        }
    }

    /// Test hook: the wrap cache, so tests can compute expected offsets.
    #[cfg(test)]
    pub fn lines_for_test(&self) -> &[VisualLine] {
        &self.lines
    }

    /// The character offset under a grid cell.
    pub fn offset_at_cell(&self, col: usize, row: usize) -> usize {
        let line = self.viewport_top + row;
        // Clicking in the empty space below the text lands at the end of
        // the document, not at the start of the last line.
        if line >= self.lines.len() {
            return self.editor.len();
        }
        let col = col.saturating_sub(wrap::TEXT_LEFT);
        wrap::offset_at(&self.lines, line, col)
    }

    pub fn click(&mut self, col: usize, row: usize, extend: bool) {
        let offset = self.offset_at_cell(col, row);
        self.editor.set_cursor(offset, extend);
        self.goal_col = None;
        self.scroll_to_cursor();
    }

    /// Scroll without moving the caret, as a scroll wheel should.
    pub fn scroll(&mut self, lines: i32) {
        let max_top = self.lines.len().saturating_sub(self.text_rows());
        let next = self.viewport_top as i64 + lines as i64;
        self.viewport_top = next.clamp(0, max_top as i64) as usize;
    }

    /// Repaint the grid from application state. `blink_on` drives the
    /// cursor's 2Hz blink.
    pub fn paint(&mut self, blink_on: bool) {
        self.screen.clear(FG, BG);
        let rows = self.text_rows();
        let text = self.editor.text();

        for row in 0..rows {
            let Some(line) = self.lines.get(self.viewport_top + row) else {
                break;
            };
            let s: String = text[line.start..line.end].iter().collect();
            self.screen.put_str(wrap::TEXT_LEFT, row, &s, FG, BG);
        }

        self.paint_selection();
        if blink_on && self.editor.selection().is_none() {
            self.paint_cursor();
        }
        self.paint_status();
    }

    fn paint_selection(&mut self) {
        let Some((lo, hi)) = self.editor.selection() else {
            return;
        };
        let rows = self.text_rows();
        for row in 0..rows {
            let Some(line) = self.lines.get(self.viewport_top + row).copied() else {
                break;
            };
            let from = lo.max(line.start);
            let to = hi.min(line.end);
            for offset in from..to {
                let col = wrap::TEXT_LEFT + (offset - line.start);
                self.screen.invert(col, row);
            }
        }
    }

    fn paint_cursor(&mut self) {
        let (line, col) = self.cursor_position();
        if line < self.viewport_top {
            return;
        }
        let row = line - self.viewport_top;
        if row < self.text_rows() {
            self.screen.invert(wrap::TEXT_LEFT + col, row);
        }
    }

    fn paint_status(&mut self) {
        let row = self.screen.rows() - 1;
        // The status line is its own colour across the full width.
        for col in 0..self.screen.cols() {
            self.screen.set(col, row, 0x20, STATUS_FG, BG);
        }

        let left = status::dos_path(self.path.as_deref(), self.editor.is_dirty());
        self.screen.put_str(0, row, &left, STATUS_FG, BG);

        let (line, col) = self.cursor_position();
        let right = status::right_field(&status::measure(line, col));
        let start = self.screen.cols().saturating_sub(right.chars().count());
        self.screen.put_str(start, row, &right, STATUS_FG, BG);
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T: u64 = 0;

    fn app_with(text: &str) -> App {
        let mut a = App::new();
        a.load_text(text);
        a
    }

    #[test]
    fn a_new_app_starts_empty_at_the_top() {
        let a = App::new();
        assert_eq!(a.viewport_top(), 0);
        assert_eq!(a.editor().cursor(), 0);
    }

    #[test]
    fn typing_inserts_and_marks_the_document_dirty() {
        let mut a = App::new();
        a.apply(Command::Insert("hi".to_string()), T);
        assert_eq!(a.editor().to_string(), "hi");
        assert!(a.editor().is_dirty());
    }

    #[test]
    fn unrepresentable_characters_are_dropped_at_the_boundary() {
        let mut a = App::new();
        a.apply(Command::Insert("a\u{3042}b".to_string()), T);
        assert_eq!(a.editor().to_string(), "ab", "no glyph, no character");
    }

    #[test]
    fn typographic_characters_are_substituted_on_the_way_in() {
        let mut a = App::new();
        a.apply(Command::Insert("\u{201C}x\u{201D}".to_string()), T);
        assert_eq!(a.editor().to_string(), "\"x\"");
    }

    #[test]
    fn tab_inserts_spaces_to_the_next_stop() {
        let mut a = App::new();
        a.apply(Command::Tab, T);
        assert_eq!(a.editor().to_string(), "        ");
        a.apply(Command::Insert("x".to_string()), T);
        a.apply(Command::Tab, T);
        assert_eq!(a.editor().to_string().len(), 16, "next stop, not another eight");
    }

    #[test]
    fn moving_down_a_line_keeps_the_column() {
        let mut a = app_with("abcdef\nghijkl");
        a.apply(Command::Move { motion: Motion::Right, extend: false }, T);
        a.apply(Command::Move { motion: Motion::Right, extend: false }, T);
        a.apply(Command::Move { motion: Motion::Down, extend: false }, T);
        assert_eq!(a.editor().cursor(), 9, "column 2 of the second line");
    }

    #[test]
    fn moving_down_onto_a_shorter_line_clamps_to_its_end() {
        let mut a = app_with("abcdef\nxy");
        a.apply(Command::Move { motion: Motion::LineEnd, extend: false }, T);
        a.apply(Command::Move { motion: Motion::Down, extend: false }, T);
        assert_eq!(a.editor().cursor(), 9, "end of the short line");
    }

    #[test]
    fn the_goal_column_survives_a_short_line() {
        let mut a = app_with("abcdef\nxy\nabcdef");
        a.apply(Command::Move { motion: Motion::LineEnd, extend: false }, T);
        a.apply(Command::Move { motion: Motion::Down, extend: false }, T);
        a.apply(Command::Move { motion: Motion::Down, extend: false }, T);
        assert_eq!(a.editor().cursor(), 16, "column 6 again on the third line");
    }

    #[test]
    fn line_start_and_end_work_on_the_visual_line() {
        let mut a = app_with("ab\ncd");
        a.apply(Command::Move { motion: Motion::DocEnd, extend: false }, T);
        a.apply(Command::Move { motion: Motion::LineStart, extend: false }, T);
        assert_eq!(a.editor().cursor(), 3);
        a.apply(Command::Move { motion: Motion::LineEnd, extend: false }, T);
        assert_eq!(a.editor().cursor(), 5);
    }

    #[test]
    fn shift_movement_builds_a_selection() {
        let mut a = app_with("abcdef");
        a.apply(Command::Move { motion: Motion::Right, extend: true }, T);
        a.apply(Command::Move { motion: Motion::Right, extend: true }, T);
        assert_eq!(a.editor().selection(), Some((0, 2)));
    }

    #[test]
    fn select_all_covers_the_document() {
        let mut a = app_with("abc");
        a.apply(Command::SelectAll, T);
        assert_eq!(a.editor().selection(), Some((0, 3)));
    }

    #[test]
    fn the_viewport_follows_the_cursor_down() {
        // 40 lines in a 24-row viewport.
        let text = (0..40).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n");
        let mut a = app_with(&text);
        assert_eq!(a.viewport_top(), 0);
        a.apply(Command::Move { motion: Motion::DocEnd, extend: false }, T);
        let rows = a.text_rows();
        assert_eq!(a.viewport_top(), 40 - rows, "the last line is the bottom one");
    }

    #[test]
    fn the_viewport_follows_the_cursor_back_up() {
        let text = (0..40).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n");
        let mut a = app_with(&text);
        a.apply(Command::Move { motion: Motion::DocEnd, extend: false }, T);
        a.apply(Command::Move { motion: Motion::DocStart, extend: false }, T);
        assert_eq!(a.viewport_top(), 0);
    }

    #[test]
    fn a_short_document_never_scrolls() {
        let mut a = app_with("one\ntwo");
        a.apply(Command::Move { motion: Motion::DocEnd, extend: false }, T);
        assert_eq!(a.viewport_top(), 0);
    }

    #[test]
    fn painting_puts_text_at_the_left_margin() {
        let mut a = app_with("hello");
        a.paint(true);
        assert_eq!(a.screen().cell(wrap::TEXT_LEFT, 0).glyph, b'h');
        assert_eq!(a.screen().cell(wrap::TEXT_LEFT - 1, 0).glyph, 0x20, "margin is blank");
    }

    #[test]
    fn painting_shows_the_cursor_as_inverse_video_when_blinking_on() {
        let mut a = app_with("hi");
        a.paint(true);
        let c = a.screen().cell(wrap::TEXT_LEFT, 0);
        assert_eq!(c.bg, 7, "background and foreground are swapped");
        a.paint(false);
        assert_eq!(a.screen().cell(wrap::TEXT_LEFT, 0).bg, 1, "and back again");
    }

    #[test]
    fn painting_writes_the_status_line_on_the_last_row() {
        let mut a = app_with("hi");
        a.paint(true);
        let row = a.screen().rows() - 1;
        let line: String = (0..80)
            .map(|c| cp437::decode(a.screen().cell(c, row).glyph))
            .collect();
        assert!(line.contains("(UNTITLED)"), "got: {line}");
        assert!(line.contains("Ln 1\""), "got: {line}");
        assert!(line.contains("Pos 1\""), "got: {line}");
    }

    #[test]
    fn the_status_line_reflects_the_cursor_position() {
        let mut a = app_with("abcde");
        a.apply(Command::Move { motion: Motion::LineEnd, extend: false }, T);
        a.paint(true);
        let row = a.screen().rows() - 1;
        let line: String = (0..80)
            .map(|c| cp437::decode(a.screen().cell(c, row).glyph))
            .collect();
        assert!(line.contains("Pos 1.5\""), "five characters in: {line}");
    }

    #[test]
    fn the_status_line_right_field_is_not_clipped() {
        let mut a = app_with("hi");
        a.paint(true);
        let row = a.screen().rows() - 1;
        let line: String = (0..80)
            .map(|c| cp437::decode(a.screen().cell(c, row).glyph))
            .collect();
        assert!(line.trim_end().ends_with("Pos 1\""), "right-aligned: {line:?}");
    }

    #[test]
    fn a_click_maps_a_cell_to_a_character_offset() {
        let a = app_with("hello\nworld");
        assert_eq!(a.offset_at_cell(wrap::TEXT_LEFT + 2, 0), 2);
        assert_eq!(a.offset_at_cell(wrap::TEXT_LEFT + 3, 1), 9);
    }

    #[test]
    fn a_click_left_of_the_margin_lands_at_the_line_start() {
        let a = app_with("hello");
        assert_eq!(a.offset_at_cell(0, 0), 0);
    }

    #[test]
    fn a_click_past_the_end_of_a_line_lands_at_its_end() {
        let a = app_with("hi\nthere");
        assert_eq!(a.offset_at_cell(79, 0), 2);
    }

    #[test]
    fn a_click_below_the_last_line_lands_at_the_document_end() {
        let a = app_with("hi");
        assert_eq!(a.offset_at_cell(wrap::TEXT_LEFT, 20), 2);
    }

    #[test]
    fn clicking_moves_the_caret_and_clears_the_selection() {
        let mut a = app_with("hello");
        a.apply(Command::SelectAll, T);
        a.click(wrap::TEXT_LEFT + 2, 0, false);
        assert_eq!(a.editor().cursor(), 2);
        assert_eq!(a.editor().selection(), None);
    }

    #[test]
    fn dragging_extends_the_selection() {
        let mut a = app_with("hello");
        a.click(wrap::TEXT_LEFT + 1, 0, false);
        a.click(wrap::TEXT_LEFT + 4, 0, true);
        assert_eq!(a.editor().selection(), Some((1, 4)));
    }

    #[test]
    fn the_offset_accounts_for_scrolling() {
        let text = (0..40).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n");
        let mut a = app_with(&text);
        a.apply(Command::Move { motion: Motion::DocEnd, extend: false }, T);
        let top = a.viewport_top();
        assert!(top > 0);
        let expected = wrap::offset_at(a.lines_for_test(), top, 0);
        assert_eq!(a.offset_at_cell(wrap::TEXT_LEFT, 0), expected);
    }

    #[test]
    fn scrolling_does_not_move_the_caret() {
        let text = (0..40).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n");
        let mut a = app_with(&text);
        a.scroll(5);
        assert_eq!(a.viewport_top(), 5);
        assert_eq!(a.editor().cursor(), 0, "the caret stays put");
    }

    #[test]
    fn scrolling_stops_at_the_document_edges() {
        let mut a = app_with("one\ntwo");
        a.scroll(-10);
        assert_eq!(a.viewport_top(), 0);
        a.scroll(100);
        assert_eq!(a.viewport_top(), 0, "a short document has nowhere to go");
    }

    /// Visual check of the whole paint path. Run deliberately with
    /// `cargo test dump_app_preview -- --ignored`.
    #[test]
    #[ignore]
    fn dump_app_preview() {
        use crate::vga::{FB_HEIGHT, FB_WIDTH};

        let mut a = App::new();
        a.set_path(std::path::PathBuf::from("/Users/srhise/Documents/chapter-one.txt"), false);
        for (i, word) in "It was a bright cold day in April, and the clocks were \
striking thirteen. Winston Smith, his chin nuzzled into his breast in an effort \
to escape the vile wind, slipped quickly through the glass doors of Victory \
Mansions, though not quickly enough to prevent a swirl of gritty dust from \
entering along with him."
            .split(' ')
            .enumerate()
        {
            if i > 0 {
                a.apply(Command::Insert(" ".to_string()), i as u64 * 10);
            }
            a.apply(Command::Insert(word.to_string()), i as u64 * 10);
        }
        a.apply(Command::Newline, 9_000);
        a.apply(Command::Newline, 9_000);
        a.apply(Command::Insert("The hallway smelt of boiled cabbage and old rag mats.".to_string()), 9_100);

        a.paint(true);
        let mut fb = vec![0u8; FB_WIDTH * FB_HEIGHT * 4];
        a.screen().render(&mut fb);
        crate::vga::preview::write_bmp("target/app-preview.bmp", &fb);
        println!("wrote target/app-preview.bmp");
    }
}
