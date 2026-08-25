//! A VGA text-mode display: an 80-column grid of character cells that
//! rasterizes to a 720x400 RGBA framebuffer.

use crate::cp437;
use crate::font::{self, Face};

pub const FB_WIDTH: usize = 720;
pub const FB_HEIGHT: usize = 400;

/// Stand-in glyph for characters CP437 cannot represent. Should be
/// unreachable for buffer text, which is filtered on input, but the
/// renderer must never panic on a surprise.
const REPLACEMENT: u8 = 0xFE; // solid centred block

/// The standard 16-colour EGA/VGA palette.
pub const PALETTE: [[u8; 3]; 16] = [
    [0x00, 0x00, 0x00], // 0  black
    [0x00, 0x00, 0xAA], // 1  blue
    [0x00, 0xAA, 0x00], // 2  green
    [0x00, 0xAA, 0xAA], // 3  cyan
    [0xAA, 0x00, 0x00], // 4  red
    [0xAA, 0x00, 0xAA], // 5  magenta
    [0xAA, 0x55, 0x00], // 6  brown
    [0xAA, 0xAA, 0xAA], // 7  light grey
    [0x55, 0x55, 0x55], // 8  dark grey
    [0x55, 0x55, 0xFF], // 9  bright blue
    [0x55, 0xFF, 0x55], // 10 bright green
    [0x55, 0xFF, 0xFF], // 11 bright cyan
    [0xFF, 0x55, 0x55], // 12 bright red
    [0xFF, 0x55, 0xFF], // 13 bright magenta
    [0xFF, 0xFF, 0x55], // 14 yellow
    [0xFF, 0xFF, 0xFF], // 15 white
];

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Mode {
    Text80x25,
    Text80x50,
}

impl Mode {
    pub fn face(self) -> Face {
        match self {
            Mode::Text80x25 => Face::W8x16,
            Mode::Text80x50 => Face::W8x8,
        }
    }

    pub fn rows(self) -> usize {
        match self {
            Mode::Text80x25 => 25,
            Mode::Text80x50 => 50,
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Cell {
    pub glyph: u8,
    pub fg: u8,
    pub bg: u8,
}

pub struct Screen {
    mode: Mode,
    cells: Vec<Cell>,
}

impl Screen {
    pub fn new(mode: Mode) -> Self {
        let blank = Cell { glyph: 0x20, fg: 7, bg: 1 };
        Self { mode, cells: vec![blank; 80 * mode.rows()] }
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// Switching modes reallocates and blanks the grid; the caller
    /// repaints from application state afterwards.
    pub fn set_mode(&mut self, mode: Mode) {
        if mode != self.mode {
            *self = Screen::new(mode);
        }
    }

    pub fn cols(&self) -> usize {
        80
    }

    pub fn rows(&self) -> usize {
        self.mode.rows()
    }

    fn index(&self, col: usize, row: usize) -> Option<usize> {
        (col < self.cols() && row < self.rows()).then(|| row * self.cols() + col)
    }

    pub fn cell(&self, col: usize, row: usize) -> Cell {
        self.index(col, row)
            .map(|i| self.cells[i])
            .unwrap_or(Cell { glyph: 0x20, fg: 7, bg: 1 })
    }

    pub fn clear(&mut self, fg: u8, bg: u8) {
        for c in &mut self.cells {
            *c = Cell { glyph: 0x20, fg, bg };
        }
    }

    pub fn set(&mut self, col: usize, row: usize, glyph: u8, fg: u8, bg: u8) {
        if let Some(i) = self.index(col, row) {
            self.cells[i] = Cell { glyph, fg, bg };
        }
    }

    /// Writes `s` starting at `col`, clipping at the right edge rather
    /// than wrapping. Returns the number of cells written.
    pub fn put_str(&mut self, col: usize, row: usize, s: &str, fg: u8, bg: u8) -> usize {
        let mut written = 0;
        for ch in s.chars() {
            let x = col + written;
            if x >= self.cols() {
                break;
            }
            let glyph = cp437::encode(ch).unwrap_or(REPLACEMENT);
            self.set(x, row, glyph, fg, bg);
            written += 1;
        }
        written
    }

    /// Inverse video, which is how both the cursor and the selection are
    /// drawn -- exactly as the VGA hardware cursor behaved.
    pub fn invert(&mut self, col: usize, row: usize) {
        if let Some(i) = self.index(col, row) {
            let c = self.cells[i];
            self.cells[i] = Cell { glyph: c.glyph, fg: c.bg, bg: c.fg };
        }
    }

    /// Rasterize the grid into `out`, which must be `FB_WIDTH * FB_HEIGHT * 4`
    /// bytes of RGBA. Alpha is always opaque.
    pub fn render(&self, out: &mut [u8]) {
        debug_assert_eq!(out.len(), FB_WIDTH * FB_HEIGHT * 4);
        let face = self.mode.face();
        let cell_h = face.cell_height();

        for row in 0..self.rows() {
            for y in 0..cell_h {
                let fb_y = row * cell_h + y;
                for col in 0..self.cols() {
                    let cell = self.cells[row * self.cols() + col];
                    let bits = font::glyph_row(face, cell.glyph, y);
                    let fg = PALETTE[(cell.fg & 0x0F) as usize];
                    let bg = PALETTE[(cell.bg & 0x0F) as usize];
                    let fb_x = col * font::CELL_WIDTH;

                    for x in 0..font::CELL_WIDTH {
                        // bit 8 is the leftmost pixel.
                        let lit = (bits >> (font::CELL_WIDTH - 1 - x)) & 1 == 1;
                        let rgb = if lit { fg } else { bg };
                        let i = (fb_y * FB_WIDTH + fb_x + x) * 4;
                        out[i] = rgb[0];
                        out[i + 1] = rgb[1];
                        out[i + 2] = rgb[2];
                        out[i + 3] = 0xFF;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_dimensions_always_fill_the_framebuffer() {
        for mode in [Mode::Text80x25, Mode::Text80x50] {
            let s = Screen::new(mode);
            assert_eq!(s.cols() * font::CELL_WIDTH, FB_WIDTH);
            assert_eq!(s.rows() * mode.face().cell_height(), FB_HEIGHT);
        }
    }

    #[test]
    fn clear_fills_every_cell() {
        let mut s = Screen::new(Mode::Text80x25);
        s.clear(7, 1);
        assert_eq!(s.cell(0, 0), Cell { glyph: 0x20, fg: 7, bg: 1 });
        assert_eq!(s.cell(79, 24), Cell { glyph: 0x20, fg: 7, bg: 1 });
    }

    #[test]
    fn put_str_writes_cells_and_returns_the_count() {
        let mut s = Screen::new(Mode::Text80x25);
        s.clear(7, 1);
        let n = s.put_str(2, 3, "Hi", 15, 1);
        assert_eq!(n, 2);
        assert_eq!(s.cell(2, 3).glyph, b'H');
        assert_eq!(s.cell(3, 3).glyph, b'i');
        assert_eq!(s.cell(3, 3).fg, 15);
        assert_eq!(s.cell(4, 3).glyph, 0x20, "must not write past the string");
    }

    #[test]
    fn put_str_clips_at_the_right_edge_instead_of_wrapping() {
        let mut s = Screen::new(Mode::Text80x25);
        s.clear(7, 1);
        let n = s.put_str(78, 0, "ABCD", 7, 1);
        assert_eq!(n, 2, "only two cells were available");
        assert_eq!(s.cell(78, 0).glyph, b'A');
        assert_eq!(s.cell(79, 0).glyph, b'B');
        assert_eq!(s.cell(0, 1).glyph, 0x20, "must not spill onto the next row");
    }

    #[test]
    fn put_str_substitutes_unrepresentable_characters() {
        let mut s = Screen::new(Mode::Text80x25);
        s.clear(7, 1);
        s.put_str(0, 0, "a\u{3042}b", 7, 1);
        assert_eq!(s.cell(0, 0).glyph, b'a');
        assert_eq!(s.cell(1, 0).glyph, 0xFE, "solid block stands in for the unrenderable");
        assert_eq!(s.cell(2, 0).glyph, b'b');
    }

    #[test]
    fn writes_outside_the_grid_are_ignored() {
        let mut s = Screen::new(Mode::Text80x25);
        s.clear(7, 1);
        s.set(80, 0, b'X', 7, 1);
        s.set(0, 25, b'X', 7, 1);
        s.put_str(0, 99, "nope", 7, 1);
        assert_eq!(s.cell(79, 24).glyph, 0x20);
    }

    #[test]
    fn invert_swaps_foreground_and_background() {
        let mut s = Screen::new(Mode::Text80x25);
        s.clear(7, 1);
        s.invert(5, 5);
        assert_eq!(s.cell(5, 5), Cell { glyph: 0x20, fg: 1, bg: 7 });
    }

    #[test]
    fn render_produces_a_full_rgba_framebuffer() {
        let mut s = Screen::new(Mode::Text80x25);
        s.clear(7, 1);
        let mut fb = vec![0u8; FB_WIDTH * FB_HEIGHT * 4];
        s.render(&mut fb);
        let blue = PALETTE[1];
        assert_eq!(&fb[0..4], &[blue[0], blue[1], blue[2], 0xFF]);
        let last = fb.len() - 4;
        assert_eq!(&fb[last..], &[blue[0], blue[1], blue[2], 0xFF]);
    }

    #[test]
    fn render_draws_glyph_pixels_in_the_foreground_color() {
        let mut s = Screen::new(Mode::Text80x25);
        s.clear(7, 1);
        s.set(0, 0, b'A', 15, 1);
        let mut fb = vec![0u8; FB_WIDTH * FB_HEIGHT * 4];
        s.render(&mut fb);

        // Row 2 of 'A' is 0b00010000: exactly one lit pixel, at column 3.
        let px = |x: usize, y: usize| {
            let i = (y * FB_WIDTH + x) * 4;
            [fb[i], fb[i + 1], fb[i + 2]]
        };
        assert_eq!(px(3, 2), PALETTE[15], "the lit pixel");
        assert_eq!(px(2, 2), PALETTE[1], "its neighbour is background");
        assert_eq!(px(4, 2), PALETTE[1], "its neighbour is background");
    }

    #[test]
    fn render_places_cells_at_nine_pixel_intervals() {
        let mut s = Screen::new(Mode::Text80x25);
        s.clear(7, 1);
        s.set(1, 0, b'A', 15, 1);
        let mut fb = vec![0u8; FB_WIDTH * FB_HEIGHT * 4];
        s.render(&mut fb);
        let i = (2 * FB_WIDTH + (font::CELL_WIDTH + 3)) * 4;
        assert_eq!(&fb[i..i + 3], &PALETTE[15][..], "cell 1 starts at x=9");
    }
}
