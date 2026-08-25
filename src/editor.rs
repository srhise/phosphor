//! The document: text, cursor, selection, and undo history.

/// How long typing may pause before a new undo step begins.
const COALESCE_WINDOW_MS: u64 = 500;

/// One reversible edit: `removed` was replaced by `inserted` at `start`.
#[derive(Clone, Debug)]
struct UndoEntry {
    start: usize,
    removed: Vec<char>,
    inserted: Vec<char>,
    cursor_before: usize,
    anchor_before: Option<usize>,
    cursor_after: usize,
}

/// What kind of edit the current undo run is accumulating. A run only
/// coalesces with more of the same kind.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum RunKind {
    Insert,
    Delete,
}

pub struct Editor {
    text: Vec<char>,
    cursor: usize,
    /// Selection anchor. The caret is always at `cursor`; the selection is
    /// the range between the two, in whichever order.
    anchor: Option<usize>,
    dirty: bool,
    undo: Vec<UndoEntry>,
    redo: Vec<UndoEntry>,
    /// The open run, if any: its kind and when it was last extended.
    run: Option<(RunKind, u64)>,
    /// Undo depth at the last save, used to clear `dirty` on undo.
    saved_depth: usize,
}

impl Editor {
    pub fn new() -> Self {
        Self {
            text: Vec::new(),
            cursor: 0,
            anchor: None,
            dirty: false,
            undo: Vec::new(),
            redo: Vec::new(),
            run: None,
            saved_depth: 0,
        }
    }

    pub fn from_str(s: &str) -> Self {
        let mut e = Self::new();
        e.text = s.chars().collect();
        e
    }

    pub fn text(&self) -> &[char] {
        &self.text
    }

    #[allow(clippy::inherent_to_string)]
    pub fn to_string(&self) -> String {
        self.text.iter().collect()
    }

    pub fn len(&self) -> usize {
        self.text.len()
    }

    #[allow(dead_code)] // paired with `len` to satisfy clippy
    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn mark_saved(&mut self) {
        self.dirty = false;
        self.saved_depth = self.undo.len();
        self.run = None;
    }

    /// The selection as `(low, high)`, or `None` when nothing is selected.
    ///
    /// Both ends are clamped to the buffer: the anchor is a bare offset,
    /// and an edit can shrink the text out from under it. Clamping here
    /// makes an out-of-range selection unrepresentable no matter which
    /// path left the anchor behind.
    pub fn selection(&self) -> Option<(usize, usize)> {
        let len = self.text.len();
        let anchor = self.anchor?.min(len);
        let cursor = self.cursor.min(len);
        if anchor == cursor {
            return None;
        }
        Some((anchor.min(cursor), anchor.max(cursor)))
    }

    pub fn selected_text(&self) -> String {
        match self.selection() {
            Some((lo, hi)) => self.text[lo..hi].iter().collect(),
            None => String::new(),
        }
    }

    /// Move the caret. `extend` keeps (or starts) a selection; without it
    /// any selection collapses.
    pub fn set_cursor(&mut self, offset: usize, extend: bool) {
        let offset = offset.min(self.text.len());
        if offset != self.cursor {
            self.run = None;
        }
        if extend {
            if self.anchor.is_none() {
                self.anchor = Some(self.cursor);
            }
        } else {
            self.anchor = None;
        }
        self.cursor = offset;
    }

    /// Push an edit onto the undo stack, coalescing into the open run when
    /// the kind, adjacency, and timing all allow it.
    fn record(&mut self, entry: UndoEntry, kind: RunKind, now_ms: u64) {
        self.redo.clear();

        let coalesce = match (self.run, self.undo.last()) {
            (Some((run_kind, last_ms)), Some(last)) => {
                run_kind == kind
                    && now_ms.saturating_sub(last_ms) <= COALESCE_WINDOW_MS
                    && entry.removed.is_empty() == last.removed.is_empty()
                    && match kind {
                        // Consecutive typing: this insert begins exactly
                        // where the last one ended.
                        RunKind::Insert => entry.start == last.start + last.inserted.len(),
                        // Consecutive backspacing: this deletion ends
                        // exactly where the last one began.
                        RunKind::Delete => {
                            entry.start + entry.removed.len() == last.start
                                || entry.start == last.start
                        }
                    }
            }
            _ => false,
        };

        if coalesce {
            // Safe: `coalesce` is only true when `undo.last()` is Some.
            let last = self.undo.last_mut().expect("coalesce implies a last entry");
            match kind {
                RunKind::Insert => last.inserted.extend_from_slice(&entry.inserted),
                RunKind::Delete => {
                    if entry.start < last.start {
                        let mut merged = entry.removed.clone();
                        merged.extend_from_slice(&last.removed);
                        last.removed = merged;
                        last.start = entry.start;
                    } else {
                        last.removed.extend_from_slice(&entry.removed);
                    }
                }
            }
            last.cursor_after = entry.cursor_after;
        } else {
            self.undo.push(entry);
        }

        // Whitespace closes a run, so undo lands on word boundaries.
        let ends_run = self.undo.last().map(entry_ends_run).unwrap_or(false);
        self.run = if ends_run { None } else { Some((kind, now_ms)) };
        self.dirty = self.undo.len() != self.saved_depth;
    }

    pub fn insert(&mut self, s: &str, now_ms: u64) {
        if s.is_empty() {
            return;
        }
        let cursor_before = self.cursor;
        let anchor_before = self.anchor;
        let (start, removed) = match self.selection() {
            Some((lo, hi)) => (lo, self.text[lo..hi].to_vec()),
            None => (self.cursor, Vec::new()),
        };
        if !removed.is_empty() {
            self.text.drain(start..start + removed.len());
        }
        let inserted: Vec<char> = s.chars().collect();
        self.text.splice(start..start, inserted.iter().copied());
        self.cursor = start + inserted.len();
        self.anchor = None;

        // Replacing a selection is never part of a typing run.
        if !removed.is_empty() {
            self.run = None;
        }
        self.record(
            UndoEntry {
                start,
                removed,
                inserted,
                cursor_before,
                anchor_before,
                cursor_after: self.cursor,
            },
            RunKind::Insert,
            now_ms,
        );
    }

    pub fn backspace(&mut self, now_ms: u64) {
        if self.selection().is_some() {
            self.delete_range_as_edit(now_ms);
            return;
        }
        if self.cursor == 0 {
            return;
        }
        let cursor_before = self.cursor;
        let start = self.cursor - 1;
        let removed = vec![self.text[start]];
        self.text.remove(start);
        self.cursor = start;
        self.anchor = None; // an edit collapses any selection
        self.record(
            UndoEntry {
                start,
                removed,
                inserted: Vec::new(),
                cursor_before,
                anchor_before: None,
                cursor_after: self.cursor,
            },
            RunKind::Delete,
            now_ms,
        );
    }

    pub fn delete_forward(&mut self, now_ms: u64) {
        if self.selection().is_some() {
            self.delete_range_as_edit(now_ms);
            return;
        }
        if self.cursor >= self.text.len() {
            return;
        }
        let cursor_before = self.cursor;
        let start = self.cursor;
        let removed = vec![self.text[start]];
        self.text.remove(start);
        self.anchor = None; // an edit collapses any selection
        self.record(
            UndoEntry {
                start,
                removed,
                inserted: Vec::new(),
                cursor_before,
                anchor_before: None,
                cursor_after: self.cursor,
            },
            RunKind::Delete,
            now_ms,
        );
    }

    /// Delete the current selection as one undo step of its own.
    fn delete_range_as_edit(&mut self, now_ms: u64) {
        let Some((lo, hi)) = self.selection() else {
            return;
        };
        let cursor_before = self.cursor;
        let anchor_before = self.anchor;
        let removed = self.text[lo..hi].to_vec();
        self.text.drain(lo..hi);
        self.cursor = lo;
        self.anchor = None;
        self.run = None; // a block delete never joins a run
        self.record(
            UndoEntry {
                start: lo,
                removed,
                inserted: Vec::new(),
                cursor_before,
                anchor_before,
                cursor_after: lo,
            },
            RunKind::Delete,
            now_ms,
        );
        self.run = None;
    }

    pub fn undo(&mut self) -> bool {
        let Some(entry) = self.undo.pop() else {
            return false;
        };
        // Reverse: take out what was inserted, put back what was removed.
        let end = entry.start + entry.inserted.len();
        self.text
            .splice(entry.start..end, entry.removed.iter().copied());
        self.cursor = entry.cursor_before;
        self.anchor = entry.anchor_before;
        self.redo.push(entry);
        self.run = None;
        self.dirty = self.undo.len() != self.saved_depth;
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(entry) = self.redo.pop() else {
            return false;
        };
        let end = entry.start + entry.removed.len();
        self.text
            .splice(entry.start..end, entry.inserted.iter().copied());
        self.cursor = entry.cursor_after;
        self.anchor = None;
        self.undo.push(entry);
        self.run = None;
        self.dirty = self.undo.len() != self.saved_depth;
        true
    }

    /// Offset of the start of the word at or before the caret. Skips any
    /// whitespace immediately to the left, then the word itself.
    pub fn word_left(&self) -> usize {
        let mut i = self.cursor;
        while i > 0 && is_word_break(self.text[i - 1]) {
            i -= 1;
        }
        while i > 0 && !is_word_break(self.text[i - 1]) {
            i -= 1;
        }
        i
    }

    /// Offset just past the word at or after the caret.
    pub fn word_right(&self) -> usize {
        let n = self.text.len();
        let mut i = self.cursor;
        while i < n && is_word_break(self.text[i]) {
            i += 1;
        }
        while i < n && !is_word_break(self.text[i]) {
            i += 1;
        }
        i
    }

    pub fn word_count(&self) -> usize {
        let mut count = 0;
        let mut in_word = false;
        for &c in &self.text {
            if is_word_break(c) {
                in_word = false;
            } else if !in_word {
                in_word = true;
                count += 1;
            }
        }
        count
    }
}

impl Default for Editor {
    fn default() -> Self {
        Self::new()
    }
}

/// Whitespace at the end of an edit closes the run, which is what puts
/// undo granularity at roughly one word.
fn entry_ends_run(entry: &UndoEntry) -> bool {
    entry
        .inserted
        .last()
        .map(|c| c.is_whitespace())
        .unwrap_or(false)
}

fn is_word_break(c: char) -> bool {
    c.is_whitespace()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An arbitrary base time; offsets from it are what matter.
    const T0: u64 = 1_000_000;

    #[test]
    fn a_new_editor_is_empty_and_clean() {
        let e = Editor::new();
        assert_eq!(e.len(), 0);
        assert_eq!(e.cursor(), 0);
        assert_eq!(e.selection(), None);
        assert!(!e.is_dirty());
    }

    #[test]
    fn from_str_places_the_cursor_at_the_start() {
        let e = Editor::from_str("hello");
        assert_eq!(e.to_string(), "hello");
        assert_eq!(e.cursor(), 0);
        assert!(!e.is_dirty(), "loading a file is not an edit");
    }

    #[test]
    fn insert_writes_at_the_cursor_and_advances_it() {
        let mut e = Editor::new();
        e.insert("ab", T0);
        assert_eq!(e.to_string(), "ab");
        assert_eq!(e.cursor(), 2);
        assert!(e.is_dirty());
    }

    #[test]
    fn insert_in_the_middle_splits_the_text() {
        let mut e = Editor::from_str("ac");
        e.set_cursor(1, false);
        e.insert("b", T0);
        assert_eq!(e.to_string(), "abc");
        assert_eq!(e.cursor(), 2);
    }

    #[test]
    fn insert_counts_characters_not_bytes() {
        let mut e = Editor::new();
        e.insert("\u{00E9}\u{00E9}", T0);
        assert_eq!(e.len(), 2, "two accented characters, not four bytes");
        assert_eq!(e.cursor(), 2);
    }

    #[test]
    fn mark_saved_clears_the_dirty_flag() {
        let mut e = Editor::new();
        e.insert("x", T0);
        assert!(e.is_dirty());
        e.mark_saved();
        assert!(!e.is_dirty());
    }

    #[test]
    fn backspace_removes_the_character_before_the_cursor() {
        let mut e = Editor::from_str("abc");
        e.set_cursor(2, false);
        e.backspace(T0);
        assert_eq!(e.to_string(), "ac");
        assert_eq!(e.cursor(), 1);
    }

    #[test]
    fn backspace_at_the_start_does_nothing() {
        let mut e = Editor::from_str("abc");
        e.set_cursor(0, false);
        e.backspace(T0);
        assert_eq!(e.to_string(), "abc");
        assert_eq!(e.cursor(), 0);
        assert!(!e.is_dirty(), "a no-op must not dirty the document");
    }

    #[test]
    fn delete_forward_removes_the_character_after_the_cursor() {
        let mut e = Editor::from_str("abc");
        e.set_cursor(1, false);
        e.delete_forward(T0);
        assert_eq!(e.to_string(), "ac");
        assert_eq!(e.cursor(), 1);
    }

    #[test]
    fn delete_forward_at_the_end_does_nothing() {
        let mut e = Editor::from_str("ab");
        e.set_cursor(2, false);
        e.delete_forward(T0);
        assert_eq!(e.to_string(), "ab");
        assert!(!e.is_dirty());
    }

    #[test]
    fn set_cursor_clamps_to_the_document() {
        let mut e = Editor::from_str("abc");
        e.set_cursor(99, false);
        assert_eq!(e.cursor(), 3);
    }

    #[test]
    fn extending_creates_a_normalized_selection() {
        let mut e = Editor::from_str("abcdef");
        e.set_cursor(4, false);
        e.set_cursor(1, true);
        assert_eq!(
            e.selection(),
            Some((1, 4)),
            "low, high regardless of direction"
        );
        assert_eq!(e.cursor(), 1, "the caret follows the head");
    }

    #[test]
    fn moving_without_extending_drops_the_selection() {
        let mut e = Editor::from_str("abcdef");
        e.set_cursor(1, false);
        e.set_cursor(4, true);
        assert!(e.selection().is_some());
        e.set_cursor(2, false);
        assert_eq!(e.selection(), None);
    }

    #[test]
    fn a_collapsed_selection_is_no_selection() {
        let mut e = Editor::from_str("abc");
        e.set_cursor(2, false);
        e.set_cursor(2, true);
        assert_eq!(e.selection(), None);
    }

    #[test]
    fn selected_text_returns_the_range() {
        let mut e = Editor::from_str("abcdef");
        e.set_cursor(1, false);
        e.set_cursor(4, true);
        assert_eq!(e.selected_text(), "bcd");
    }

    #[test]
    fn insert_replaces_the_selection() {
        let mut e = Editor::from_str("abcdef");
        e.set_cursor(1, false);
        e.set_cursor(4, true);
        e.insert("X", T0);
        assert_eq!(e.to_string(), "aXef");
        assert_eq!(e.cursor(), 2);
        assert_eq!(e.selection(), None);
    }

    #[test]
    fn backspace_deletes_the_selection_rather_than_one_character() {
        let mut e = Editor::from_str("abcdef");
        e.set_cursor(1, false);
        e.set_cursor(4, true);
        e.backspace(T0);
        assert_eq!(e.to_string(), "aef");
        assert_eq!(e.cursor(), 1);
    }

    #[test]
    fn delete_forward_deletes_the_selection_too() {
        let mut e = Editor::from_str("abcdef");
        e.set_cursor(1, false);
        e.set_cursor(4, true);
        e.delete_forward(T0);
        assert_eq!(e.to_string(), "aef");
    }

    #[test]
    fn word_left_stops_at_the_start_of_the_current_word() {
        let mut e = Editor::from_str("the quick brown");
        e.set_cursor(15, false);
        assert_eq!(e.word_left(), 10, "start of 'brown'");
        e.set_cursor(10, false);
        assert_eq!(e.word_left(), 4, "start of 'quick'");
    }

    #[test]
    fn word_left_from_the_middle_of_a_word() {
        let mut e = Editor::from_str("the quick");
        e.set_cursor(7, false);
        assert_eq!(e.word_left(), 4);
    }

    #[test]
    fn word_right_stops_after_the_current_word() {
        let mut e = Editor::from_str("the quick brown");
        e.set_cursor(0, false);
        assert_eq!(e.word_right(), 3, "end of 'the'");
        e.set_cursor(3, false);
        assert_eq!(e.word_right(), 9, "end of 'quick'");
    }

    #[test]
    fn word_motion_saturates_at_the_document_edges() {
        let mut e = Editor::from_str("word");
        e.set_cursor(0, false);
        assert_eq!(e.word_left(), 0);
        e.set_cursor(4, false);
        assert_eq!(e.word_right(), 4);
    }

    #[test]
    fn word_motion_treats_newlines_as_boundaries() {
        let mut e = Editor::from_str("ab\ncd");
        e.set_cursor(5, false);
        assert_eq!(e.word_left(), 3, "start of 'cd', not across the newline");
    }

    #[test]
    fn word_count_counts_whitespace_separated_runs() {
        assert_eq!(Editor::from_str("").word_count(), 0);
        assert_eq!(Editor::from_str("   ").word_count(), 0);
        assert_eq!(Editor::from_str("one").word_count(), 1);
        assert_eq!(Editor::from_str("one two  three").word_count(), 3);
        assert_eq!(Editor::from_str("one\ntwo\n\nthree ").word_count(), 3);
    }

    // ---- undo ----

    #[test]
    fn undo_reverses_an_insert_and_restores_the_cursor() {
        let mut e = Editor::from_str("ac");
        e.set_cursor(1, false);
        e.insert("b", T0);
        assert_eq!(e.to_string(), "abc");
        assert!(e.undo());
        assert_eq!(e.to_string(), "ac");
        assert_eq!(e.cursor(), 1);
    }

    #[test]
    fn undo_on_an_untouched_document_reports_nothing_to_do() {
        let mut e = Editor::from_str("abc");
        assert!(!e.undo());
        assert_eq!(e.to_string(), "abc");
    }

    #[test]
    fn redo_reapplies_an_undone_edit() {
        let mut e = Editor::new();
        e.insert("hi", T0);
        e.undo();
        assert_eq!(e.to_string(), "");
        assert!(e.redo());
        assert_eq!(e.to_string(), "hi");
        assert_eq!(e.cursor(), 2);
    }

    #[test]
    fn a_new_edit_discards_the_redo_stack() {
        let mut e = Editor::new();
        e.insert("a", T0);
        e.undo();
        e.insert("b", T0);
        assert!(!e.redo(), "the old redo branch is gone");
        assert_eq!(e.to_string(), "b");
    }

    #[test]
    fn typing_a_word_coalesces_into_one_undo_step() {
        let mut e = Editor::new();
        for (i, c) in "hello".chars().enumerate() {
            e.insert(&c.to_string(), T0 + i as u64 * 50);
        }
        assert_eq!(e.to_string(), "hello");
        assert!(e.undo());
        assert_eq!(e.to_string(), "", "the whole word goes at once");
    }

    #[test]
    fn a_space_ends_the_undo_run() {
        let mut e = Editor::new();
        e.insert("ab", T0);
        e.insert(" ", T0 + 10);
        e.insert("cd", T0 + 20);
        e.undo();
        assert_eq!(e.to_string(), "ab ", "only the second word is undone");
        e.undo();
        assert_eq!(e.to_string(), "", "the space belongs to the first run");
    }

    #[test]
    fn a_newline_ends_the_undo_run() {
        let mut e = Editor::new();
        e.insert("ab", T0);
        e.insert("\n", T0 + 10);
        e.insert("cd", T0 + 20);
        e.undo();
        assert_eq!(e.to_string(), "ab\n");
    }

    #[test]
    fn an_idle_pause_ends_the_undo_run() {
        let mut e = Editor::new();
        e.insert("ab", T0);
        e.insert("cd", T0 + 501);
        e.undo();
        assert_eq!(e.to_string(), "ab", "500ms of silence starts a new step");
    }

    #[test]
    fn moving_the_cursor_ends_the_undo_run() {
        let mut e = Editor::from_str("xy");
        e.set_cursor(0, false);
        e.insert("a", T0);
        e.set_cursor(0, false);
        e.insert("b", T0 + 10);
        e.undo();
        assert_eq!(e.to_string(), "axy", "only the second insert is undone");
    }

    #[test]
    fn saving_ends_the_undo_run() {
        let mut e = Editor::new();
        e.insert("ab", T0);
        e.mark_saved();
        e.insert("cd", T0 + 10);
        e.undo();
        assert_eq!(e.to_string(), "ab");
    }

    #[test]
    fn backspaces_coalesce_with_each_other_but_not_with_inserts() {
        let mut e = Editor::from_str("abcdef");
        e.set_cursor(6, false);
        e.backspace(T0);
        e.backspace(T0 + 10);
        assert_eq!(e.to_string(), "abcd");
        e.undo();
        assert_eq!(e.to_string(), "abcdef", "both deletions undo together");
    }

    #[test]
    fn undo_restores_a_deleted_selection() {
        let mut e = Editor::from_str("abcdef");
        e.set_cursor(1, false);
        e.set_cursor(4, true);
        e.insert("X", T0);
        assert_eq!(e.to_string(), "aXef");
        e.undo();
        assert_eq!(e.to_string(), "abcdef");
        assert_eq!(e.selection(), Some((1, 4)), "the selection comes back");
    }

    #[test]
    fn repeated_undo_walks_all_the_way_back() {
        let mut e = Editor::new();
        e.insert("one", T0);
        e.insert(" ", T0 + 10);
        e.insert("two", T0 + 20);
        while e.undo() {}
        assert_eq!(e.to_string(), "");
        while e.redo() {}
        assert_eq!(e.to_string(), "one two");
    }

    #[test]
    fn undoing_back_to_the_saved_state_clears_dirty() {
        let mut e = Editor::from_str("ab");
        e.insert("c", T0);
        assert!(e.is_dirty());
        e.undo();
        assert!(!e.is_dirty(), "back where we started, so nothing to save");
    }

    // --- regression: a selection anchor left dangling past the buffer end ---

    #[test]
    fn backspace_does_not_leave_a_stale_selection_anchor() {
        let mut e = Editor::from_str("ab");
        e.set_cursor(2, false);
        e.set_cursor(2, true); // anchor at 2, collapsed
        e.backspace(T0); // buffer is now 1 long, anchor still says 2
        assert_eq!(e.selection(), None, "an edit collapses the selection");
        assert_eq!(e.selected_text(), "", "must not slice past the end");
    }

    #[test]
    fn delete_forward_does_not_leave_a_stale_selection_anchor() {
        let mut e = Editor::from_str("ab");
        e.set_cursor(2, false);
        e.set_cursor(2, true);
        e.set_cursor(0, false);
        e.set_cursor(0, true);
        e.delete_forward(T0);
        assert_eq!(e.selection(), None);
        assert_eq!(e.selected_text(), "");
    }

    #[test]
    fn inserting_after_a_shrink_does_not_panic() {
        let mut e = Editor::from_str("ab");
        e.set_cursor(2, false);
        e.set_cursor(2, true);
        e.backspace(T0);
        e.insert("z", T0 + 600); // used to slice a stale selection range
        assert_eq!(e.to_string(), "az");
    }

    #[test]
    fn a_selection_can_never_run_past_the_buffer() {
        // The invariant itself, independent of which path broke it.
        let mut e = Editor::from_str("abcdef");
        e.set_cursor(6, false);
        e.set_cursor(6, true);
        e.backspace(T0);
        e.backspace(T0 + 10);
        e.backspace(T0 + 20);
        if let Some((lo, hi)) = e.selection() {
            assert!(
                hi <= e.len(),
                "selection {lo}..{hi} exceeds len {}",
                e.len()
            );
        }
        assert_eq!(e.selected_text(), "");
    }
}
